use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::account_models::{AccountPayload, VaultAccountRecord};
use crate::errors::AppError;

const STATE_FILE: &str = "chatgpt-provider-mode.json";
const AUTH_FILE: &str = "auth.json";
const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModeState {
    pub provider_account_id: String,
    pub provider_display_name: String,
    pub activated_at: DateTime<Utc>,
}

pub fn load_provider_mode_state(state_dir: &Path) -> Result<Option<ProviderModeState>, AppError> {
    let path = state_path(state_dir);
    match fs::read(&path) {
        Ok(data) => serde_json::from_slice(&data)
            .map(Some)
            .map_err(|error| provider_error(format!("read state {}: {error}", path.display()))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(provider_error(format!(
            "read state {}: {error}",
            path.display()
        ))),
    }
}

pub fn enter_provider_mode(
    record: &VaultAccountRecord,
    codex_home: &Path,
    state_dir: &Path,
) -> Result<ProviderModeState, AppError> {
    let payload = match &record.payload {
        AccountPayload::Api(payload) => payload,
        AccountPayload::ChatGpt { .. } => {
            return Err(provider_error(
                "choose a saved API account as the third-party Provider",
            ));
        }
    };
    if payload.api_key.trim().is_empty() {
        return Err(provider_error("saved API account is missing an API key"));
    }
    if payload.base_url.trim().is_empty() {
        return Err(provider_error("saved API account is missing a Base URL"));
    }

    let auth_path = codex_home.join(AUTH_FILE);
    let auth_data = fs::read(&auth_path).map_err(|_| AppError::SignInRequired)?;
    let auth_json: serde_json::Value =
        serde_json::from_slice(&auth_data).map_err(|_| AppError::SignInRequired)?;
    if !looks_like_chatgpt_auth(&auth_json) {
        return Err(provider_error(
            "current Codex account must be signed in with ChatGPT",
        ));
    }

    fs::create_dir_all(codex_home).map_err(|error| {
        provider_error(format!(
            "create Codex home {}: {error}",
            codex_home.display()
        ))
    })?;
    fs::create_dir_all(state_dir).map_err(|error| {
        provider_error(format!(
            "create Provider mode state directory {}: {error}",
            state_dir.display()
        ))
    })?;

    write_backup_file(state_dir, AUTH_FILE, Some(&auth_data))?;
    let config_path = codex_home.join(CONFIG_FILE);
    let config_data = fs::read(&config_path).ok();
    write_backup_file(state_dir, CONFIG_FILE, config_data.as_deref())?;

    let state = ProviderModeState {
        provider_account_id: record.id.as_str().to_string(),
        provider_display_name: record.metadata.display_name.clone(),
        activated_at: Utc::now(),
    };

    let provider_auth_data = provider_mode_auth_data(auth_json)?;
    let provider_config = synthesized_provider_config(
        &payload.base_url,
        &payload.api_key,
        payload.model.as_deref(),
    );

    replace_file(&auth_path, &provider_auth_data)?;
    replace_file(&config_path, provider_config.as_bytes())?;
    let state_data = serde_json::to_vec_pretty(&state)
        .map_err(|error| provider_error(format!("serialize Provider mode state: {error}")))?;
    replace_file(&state_path(state_dir), &state_data)?;

    Ok(state)
}

pub fn exit_provider_mode(codex_home: &Path, state_dir: &Path) -> Result<(), AppError> {
    if load_provider_mode_state(state_dir)?.is_none() {
        return Err(AppError::ProviderModeNotActive);
    }

    let auth_backup = backup_path(state_dir, AUTH_FILE);
    let auth_data = fs::read(&auth_backup).map_err(|error| {
        provider_error(format!(
            "read auth backup {}: {error}",
            auth_backup.display()
        ))
    })?;
    fs::create_dir_all(codex_home).map_err(|error| {
        provider_error(format!(
            "create Codex home {}: {error}",
            codex_home.display()
        ))
    })?;
    replace_file(&codex_home.join(AUTH_FILE), &auth_data)?;

    let config_backup = backup_path(state_dir, CONFIG_FILE);
    let config_marker = backup_missing_marker_path(state_dir, CONFIG_FILE);
    let config_path = codex_home.join(CONFIG_FILE);
    if config_marker.exists() {
        match fs::remove_file(&config_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(provider_error(format!(
                    "remove restored-missing config {}: {error}",
                    config_path.display()
                )));
            }
        }
    } else {
        let config_data = fs::read(&config_backup).map_err(|error| {
            provider_error(format!(
                "read config backup {}: {error}",
                config_backup.display()
            ))
        })?;
        replace_file(&config_path, &config_data)?;
    }

    fs::remove_file(state_path(state_dir)).map_err(|error| {
        provider_error(format!(
            "remove state {}: {error}",
            state_path(state_dir).display()
        ))
    })?;
    Ok(())
}

fn looks_like_chatgpt_auth(auth_json: &serde_json::Value) -> bool {
    let auth_mode = auth_json
        .get("auth_mode")
        .or_else(|| auth_json.get("type"))
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if auth_mode == "apikey" || auth_mode == "api" {
        return false;
    }
    auth_json
        .get("OPENAI_API_KEY")
        .and_then(|value| value.as_str())
        .is_none()
}

fn provider_mode_auth_data(mut auth_json: serde_json::Value) -> Result<Vec<u8>, AppError> {
    let object = auth_json
        .as_object_mut()
        .ok_or_else(|| provider_error("auth.json is not a JSON object"))?;
    object.insert(
        "auth_mode".to_string(),
        serde_json::Value::String("chatgpt".into()),
    );
    object.insert("OPENAI_API_KEY".to_string(), serde_json::Value::Null);
    serde_json::to_vec_pretty(&auth_json)
        .map_err(|error| provider_error(format!("serialize Provider mode auth: {error}")))
}

fn synthesized_provider_config(base_url: &str, api_key: &str, model: Option<&str>) -> String {
    let normalized_base_url = normalized_provider_base_url(base_url);
    let mut lines = vec!["model_provider = \"OpenAI\"".to_string()];
    if let Some(model) = model.map(str::trim).filter(|value| !value.is_empty()) {
        lines.push(format!("model = \"{}\"", escape_toml(model)));
    }
    lines.extend([
        String::new(),
        "[model_providers.OpenAI]".to_string(),
        "name = \"OpenAI\"".to_string(),
        format!("base_url = \"{}\"", escape_toml(&normalized_base_url)),
        "wire_api = \"responses\"".to_string(),
        format!(
            "experimental_bearer_token = \"{}\"",
            escape_toml(api_key.trim())
        ),
        "requires_openai_auth = true".to_string(),
    ]);
    lines.join("\n") + "\n"
}

fn normalized_provider_base_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.to_ascii_lowercase().ends_with("/v1") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/v1")
    }
}

fn write_backup_file(state_dir: &Path, name: &str, data: Option<&[u8]>) -> Result<(), AppError> {
    let data_path = backup_path(state_dir, name);
    let marker_path = backup_missing_marker_path(state_dir, name);
    match data {
        Some(data) => {
            replace_file(&data_path, data)?;
            let _ = fs::remove_file(marker_path);
        }
        None => {
            if data_path.exists() {
                fs::remove_file(&data_path).map_err(|error| {
                    provider_error(format!(
                        "remove stale backup {}: {error}",
                        data_path.display()
                    ))
                })?;
            }
            replace_file(&marker_path, b"missing")?;
        }
    }
    Ok(())
}

fn replace_file(path: &Path, data: &[u8]) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| provider_error(format!("{} has no parent directory", path.display())))?;
    fs::create_dir_all(parent).map_err(|error| {
        provider_error(format!("create directory {}: {error}", parent.display()))
    })?;
    let temp = unique_sidecar_path(path, "tmp");
    fs::write(&temp, data)
        .map_err(|error| provider_error(format!("write temp file {}: {error}", temp.display())))?;
    fs::rename(&temp, path).map_err(|error| {
        let _ = fs::remove_file(&temp);
        provider_error(format!("replace file {}: {error}", path.display()))
    })
}

fn state_path(state_dir: &Path) -> PathBuf {
    state_dir.join(STATE_FILE)
}

fn backup_path(state_dir: &Path, name: &str) -> PathBuf {
    state_dir.join(format!("{name}.bak"))
}

fn backup_missing_marker_path(state_dir: &Path, name: &str) -> PathBuf {
    state_dir.join(format!("{name}.missing"))
}

fn unique_sidecar_path(path: &Path, extension_suffix: &str) -> PathBuf {
    let mut candidate = path.with_extension(extension_suffix);
    let mut suffix = 2;
    while candidate.exists() {
        candidate = path.with_extension(format!("{extension_suffix}-{suffix}"));
        suffix += 1;
    }
    candidate
}

fn escape_toml(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn provider_error(message: impl Into<String>) -> AppError {
    AppError::ProviderModeFailed(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account_models::{AccountId, AddApiAccountInput, VaultAccountRecord};

    fn api_record() -> VaultAccountRecord {
        let payload = AddApiAccountInput {
            display_name: "Provider".to_string(),
            api_key: "sk-test".to_string(),
            base_url: "https://proxy.example.com".to_string(),
            model: Some("gpt-5.4".to_string()),
            provider_name: Some("OpenAI".to_string()),
        }
        .validate()
        .unwrap();
        VaultAccountRecord::new_api(
            AccountId::new("acct-api"),
            payload.display_name,
            payload.payload,
            Utc::now(),
        )
    }

    #[test]
    fn enters_provider_mode_and_writes_chatgpt_auth_plus_provider_config() {
        let temp = tempfile::tempdir().unwrap();
        let codex_home = temp.path().join(".codex");
        let state_dir = temp.path().join("provider-mode");
        fs::create_dir_all(&codex_home).unwrap();
        fs::write(
            codex_home.join(AUTH_FILE),
            br#"{"account":{"email":"ada@example.com"}}"#,
        )
        .unwrap();
        fs::write(codex_home.join(CONFIG_FILE), b"model = \"gpt-5\"\n").unwrap();

        let state = enter_provider_mode(&api_record(), &codex_home, &state_dir).unwrap();

        let auth = fs::read_to_string(codex_home.join(AUTH_FILE)).unwrap();
        let config = fs::read_to_string(codex_home.join(CONFIG_FILE)).unwrap();
        assert_eq!(state.provider_account_id, "acct-api");
        assert!(auth.contains("\"auth_mode\": \"chatgpt\""));
        assert!(auth.contains("\"OPENAI_API_KEY\": null"));
        assert!(config.contains("model_provider = \"OpenAI\""));
        assert!(config.contains("base_url = \"https://proxy.example.com/v1\""));
        assert!(config.contains("experimental_bearer_token = \"sk-test\""));
        assert!(state_path(&state_dir).exists());
    }

    #[test]
    fn exits_provider_mode_and_restores_previous_files() {
        let temp = tempfile::tempdir().unwrap();
        let codex_home = temp.path().join(".codex");
        let state_dir = temp.path().join("provider-mode");
        fs::create_dir_all(&codex_home).unwrap();
        fs::write(
            codex_home.join(AUTH_FILE),
            br#"{"account":{"email":"ada@example.com"}}"#,
        )
        .unwrap();
        fs::write(codex_home.join(CONFIG_FILE), b"model = \"before\"\n").unwrap();

        enter_provider_mode(&api_record(), &codex_home, &state_dir).unwrap();
        exit_provider_mode(&codex_home, &state_dir).unwrap();

        assert_eq!(
            fs::read_to_string(codex_home.join(CONFIG_FILE)).unwrap(),
            "model = \"before\"\n"
        );
        assert!(fs::read_to_string(codex_home.join(AUTH_FILE))
            .unwrap()
            .contains("ada@example.com"));
        assert!(!state_path(&state_dir).exists());
    }

    #[test]
    fn rejects_api_current_account() {
        let temp = tempfile::tempdir().unwrap();
        let codex_home = temp.path().join(".codex");
        let state_dir = temp.path().join("provider-mode");
        fs::create_dir_all(&codex_home).unwrap();
        fs::write(codex_home.join(AUTH_FILE), br#"{"auth_mode":"apikey"}"#).unwrap();

        let result = enter_provider_mode(&api_record(), &codex_home, &state_dir);

        assert!(matches!(result, Err(AppError::ProviderModeFailed(_))));
    }
}
