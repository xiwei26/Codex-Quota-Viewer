use std::fs;
use std::path::{Path, PathBuf};

use crate::account_models::{AccountPayload, VaultAccountRecord};
use crate::codex_desktop;
use crate::errors::AppError;
use crate::restore_points::{RestorePointManager, RestorePointManifest};

const AUTH_FILE: &str = "auth.json";
const CONFIG_FILE: &str = "config.toml";

pub fn safely_activate_account_record(
    record: &VaultAccountRecord,
    codex_home: &Path,
    switch_backups_dir: &Path,
) -> Result<RestorePointManifest, AppError> {
    let manager = RestorePointManager::new(switch_backups_dir.to_path_buf());
    let files = protected_activation_files(codex_home);
    let restore_point = manager.create_restore_point(
        "safe-switch",
        &format!("Switch to {}", record.metadata.display_name),
        &files,
    )?;
    let desktop_session = codex_desktop::close_if_running()?;

    match activate_account_record(record, codex_home) {
        Ok(()) => {
            codex_desktop::reopen_if_needed(&desktop_session)?;
            Ok(restore_point)
        }
        Err(error) => {
            manager
                .restore_manifest(&switch_backups_dir.join(&restore_point.id), &restore_point)?;
            let _ = codex_desktop::reopen_if_needed(&desktop_session);
            Err(error)
        }
    }
}

pub fn activate_account_record(
    record: &VaultAccountRecord,
    codex_home: &Path,
) -> Result<(), AppError> {
    fs::create_dir_all(codex_home)
        .map_err(|error| AppError::AccountActivationFailed(error.to_string()))?;

    match &record.payload {
        AccountPayload::ChatGpt { auth_json } => {
            write_json_file(&codex_home.join(AUTH_FILE), auth_json)?;
            write_text_file(
                &codex_home.join(CONFIG_FILE),
                "model_provider = \"openai\"\n",
            )
        }
        AccountPayload::Api(payload) => {
            let auth_json = serde_json::json!({
                "OPENAI_API_KEY": payload.api_key,
                "type": "api"
            });
            write_json_file(&codex_home.join(AUTH_FILE), &auth_json)?;

            let model = payload.model.as_deref().unwrap_or("gpt-5.4");
            let provider = payload.provider_name.as_deref().unwrap_or("openai");
            let config = format!(
                "model = \"{}\"\nmodel_provider = \"{}\"\n\n[model_providers.{}]\nname = \"{}\"\nbase_url = \"{}\"\nenv_key = \"OPENAI_API_KEY\"\n",
                escape_toml(model),
                escape_toml(provider),
                escape_toml(provider),
                escape_toml(provider),
                escape_toml(&payload.base_url)
            );
            write_text_file(&codex_home.join(CONFIG_FILE), &config)
        }
    }
}

fn protected_activation_files(codex_home: &Path) -> Vec<PathBuf> {
    vec![codex_home.join(AUTH_FILE), codex_home.join(CONFIG_FILE)]
}

fn write_json_file(path: &Path, value: &serde_json::Value) -> Result<(), AppError> {
    let data = serde_json::to_vec_pretty(value)
        .map_err(|error| AppError::AccountActivationFailed(error.to_string()))?;
    write_bytes_file(path, &data)
}

fn write_text_file(path: &Path, text: &str) -> Result<(), AppError> {
    write_bytes_file(path, text.as_bytes())
}

fn write_bytes_file(path: &Path, data: &[u8]) -> Result<(), AppError> {
    let temp = path.with_extension("tmp");
    fs::write(&temp, data).map_err(|error| AppError::AccountActivationFailed(error.to_string()))?;
    fs::rename(&temp, path).map_err(|error| AppError::AccountActivationFailed(error.to_string()))
}

fn escape_toml(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account_models::{AccountId, AddApiAccountInput};

    #[test]
    fn activates_chatgpt_by_writing_auth_json() {
        let temp = tempfile::tempdir().unwrap();
        let codex_home = temp.path().join(".codex");
        let record = VaultAccountRecord::new_chatgpt(
            AccountId::new("acct-chat"),
            "Chat".to_string(),
            serde_json::json!({"account":{"email":"ada@example.com"}}),
            chrono::Utc::now(),
        );

        activate_account_record(&record, &codex_home).unwrap();

        let text = fs::read_to_string(codex_home.join("auth.json")).unwrap();
        let config = fs::read_to_string(codex_home.join("config.toml")).unwrap();
        assert!(text.contains("ada@example.com"));
        assert!(config.contains("model_provider = \"openai\""));
    }

    #[test]
    fn activates_api_by_writing_auth_and_config() {
        let temp = tempfile::tempdir().unwrap();
        let codex_home = temp.path().join(".codex");
        let payload = AddApiAccountInput {
            display_name: "API".to_string(),
            api_key: "sk-test".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            model: Some("gpt-5.4".to_string()),
            provider_name: Some("OpenAI".to_string()),
        }
        .validate()
        .unwrap();
        let record = VaultAccountRecord::new_api(
            AccountId::new("acct-api"),
            "API".to_string(),
            payload.payload,
            chrono::Utc::now(),
        );

        activate_account_record(&record, &codex_home).unwrap();

        let auth = fs::read_to_string(codex_home.join("auth.json")).unwrap();
        let config = fs::read_to_string(codex_home.join("config.toml")).unwrap();
        assert!(auth.contains("sk-test"));
        assert!(config.contains("https://api.openai.com/v1"));
        assert!(config.contains("gpt-5.4"));
    }

    #[test]
    fn safe_activation_creates_restore_point_and_can_restore() {
        let temp = tempfile::tempdir().unwrap();
        let codex_home = temp.path().join(".codex");
        let backups = temp.path().join("backups");
        fs::create_dir_all(&codex_home).unwrap();
        fs::write(codex_home.join("auth.json"), b"{\"before\":true}").unwrap();
        fs::write(codex_home.join("config.toml"), b"model = \"before\"\n").unwrap();
        let record = VaultAccountRecord::new_chatgpt(
            AccountId::new("acct-chat"),
            "Chat".to_string(),
            serde_json::json!({"account":{"email":"ada@example.com"}}),
            chrono::Utc::now(),
        );

        let manifest = safely_activate_account_record(&record, &codex_home, &backups).unwrap();
        RestorePointManager::new(backups).restore_latest().unwrap();

        assert_eq!(manifest.reason, "safe-switch");
        assert_eq!(
            fs::read_to_string(codex_home.join("config.toml")).unwrap(),
            "model = \"before\"\n"
        );
    }
}
