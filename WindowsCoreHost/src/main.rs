#![allow(dead_code, unused_imports)]

use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[path = "../../WindowsTray/src-tauri/src/account_activation.rs"]
mod account_activation;
#[path = "../../WindowsTray/src-tauri/src/account_models.rs"]
mod account_models;
#[path = "../../WindowsTray/src-tauri/src/account_vault.rs"]
mod account_vault;
#[path = "../../WindowsTray/src-tauri/src/codex_desktop.rs"]
mod codex_desktop;
#[path = "../../WindowsTray/src-tauri/src/codex_home.rs"]
mod codex_home;
#[path = "../../WindowsTray/src-tauri/src/errors.rs"]
mod errors;
#[path = "../../WindowsTray/src-tauri/src/localization.rs"]
mod localization;
#[path = "../../WindowsTray/src-tauri/src/provider_mode.rs"]
mod provider_mode;
#[path = "../../WindowsTray/src-tauri/src/quota.rs"]
mod quota;
#[path = "../../WindowsTray/src-tauri/src/restore_points.rs"]
mod restore_points;
#[path = "../../WindowsTray/src-tauri/src/rollout_sync.rs"]
mod rollout_sync;
#[path = "../../WindowsTray/src-tauri/src/session_manager.rs"]
mod session_manager;
#[path = "../../WindowsTray/src-tauri/src/settings.rs"]
mod settings;

use account_activation::safely_activate_account_record_with_rollout;
use account_models::{
    normalize_api_base_url, AccountKind, AccountPayload, AddApiAccountInput, VaultAccountRecord,
};
use account_vault::AccountVault;
use errors::AppError;
use quota::QuotaSnapshot;
use restore_points::RestorePointManager;
use session_manager::{SessionManager, SessionManagerPaths};
use settings::{load_settings, save_settings, AppSettings};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RpcRequest {
    id: Value,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RpcResponse {
    id: Value,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct RpcError {
    code: &'static str,
    message: String,
    diagnostics: Option<String>,
}

#[derive(Debug)]
struct OwnedRefreshError {
    owner_account_id: Option<String>,
    error: RpcError,
}

#[derive(Debug, Default)]
struct QuotaCache {
    snapshot: Option<QuotaSnapshot>,
    owner_account_id: Option<String>,
    refresh_error: Option<OwnedRefreshError>,
}

impl QuotaCache {
    fn synchronize_owner(&mut self, owner_account_id: &Option<String>) {
        if self
            .refresh_error
            .as_ref()
            .is_some_and(|cached| &cached.owner_account_id != owner_account_id)
        {
            self.refresh_error = None;
        }
    }

    fn should_refresh(&self, force: bool, owner_account_id: &Option<String>) -> bool {
        force
            || self.snapshot.is_none()
            || owner_account_id.is_none()
            || &self.owner_account_id != owner_account_id
    }

    fn record_success(&mut self, owner_account_id: Option<String>, snapshot: QuotaSnapshot) {
        self.snapshot = Some(snapshot);
        self.owner_account_id = owner_account_id;
        self.refresh_error = None;
    }

    fn record_failure(&mut self, owner_account_id: Option<String>, error: RpcError) {
        self.refresh_error = Some(OwnedRefreshError {
            owner_account_id,
            error,
        });
    }

    fn snapshot_for(&self, owner_account_id: &Option<String>) -> Option<QuotaSnapshot> {
        (owner_account_id.is_some() && &self.owner_account_id == owner_account_id)
            .then(|| self.snapshot.clone())
            .flatten()
    }

    fn error_for(&self, owner_account_id: &Option<String>) -> Option<RpcError> {
        self.refresh_error
            .as_ref()
            .filter(|cached| &cached.owner_account_id == owner_account_id)
            .map(|cached| cached.error.clone())
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

impl From<AppError> for RpcError {
    fn from(error: AppError) -> Self {
        Self {
            code: error_code(&error),
            message: error.user_message().to_string(),
            diagnostics: error.diagnostics().map(str::to_string),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountView {
    id: String,
    display_name: String,
    kind: AccountKind,
    active: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DashboardState {
    schema_version: u32,
    quota: Option<QuotaSnapshot>,
    accounts: Vec<AccountView>,
    active_account_id: Option<String>,
    last_error: Option<RpcError>,
    settings: AppSettings,
    settings_issue: Option<String>,
    codex_home: String,
    updated_at: chrono::DateTime<Utc>,
}

struct Host {
    codex_home: PathBuf,
    accounts_dir: PathBuf,
    provider_mode_dir: PathBuf,
    switch_backups_dir: PathBuf,
    settings_path: PathBuf,
    session_manager: SessionManager,
    quota_cache: QuotaCache,
    shutdown_requested: bool,
}

impl Host {
    fn new(resource_root: PathBuf) -> Result<Self, AppError> {
        let codex_home = resolve_core_host_codex_home()?;
        let app_data_dir = resolve_app_data_dir()?;
        let accounts_dir = app_data_dir.join("Accounts");
        let provider_mode_dir = app_data_dir.join("ProviderMode");
        let switch_backups_dir = app_data_dir.join("SwitchBackups");
        let settings_path = app_data_dir.join("settings.json");
        let session_root = resolve_resource_path(&resource_root, "SessionManager")
            .unwrap_or_else(|| resource_root.join("SessionManager"));
        let node_exe = resolve_resource_path(&resource_root, "NodeRuntime/node.exe")
            .or_else(|| find_executable_on_path("node.exe"))
            .unwrap_or_else(|| resource_root.join("NodeRuntime").join("node.exe"));
        let server_entry = session_root.join("dist").join("server").join("index.js");

        Ok(Self {
            codex_home: codex_home.clone(),
            accounts_dir,
            provider_mode_dir,
            switch_backups_dir,
            settings_path,
            session_manager: SessionManager::new(SessionManagerPaths {
                node_exe,
                server_entry,
                app_dir: session_root,
                codex_home,
                manager_home: app_data_dir.join("SessionManager"),
            }),
            quota_cache: QuotaCache::default(),
            shutdown_requested: false,
        })
    }

    async fn handle(&mut self, request: &RpcRequest) -> Result<Value, AppError> {
        match request.method.as_str() {
            "ping" => Ok(json!({ "version": env!("CARGO_PKG_VERSION") })),
            "getDashboard" => {
                let refresh = request
                    .params
                    .get("refresh")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                self.dashboard(refresh).await
            }
            "getSettings" => {
                let loaded = load_settings(&self.settings_path);
                Ok(json!({ "settings": loaded.settings, "issue": loaded.issue }))
            }
            "saveSettings" => {
                let settings: AppSettings = value_param(&request.params, "settings")?;
                save_settings(&self.settings_path, &settings)?;
                Ok(json!({ "settings": settings }))
            }
            "importCurrentChatGpt" => {
                let display_name = optional_string_param(&request.params, "displayName");
                self.vault()
                    .import_current_chatgpt_account(&self.codex_home, display_name)?;
                self.dashboard(false).await
            }
            "addApiAccount" => {
                let input: AddApiAccountInput = value_param(&request.params, "input")?;
                self.vault().add_api_account(input)?;
                self.dashboard(false).await
            }
            "renameAccount" => {
                let account_id = string_param(&request.params, "accountId")?;
                let display_name = string_param(&request.params, "displayName")?;
                self.vault().rename_account(&account_id, &display_name)?;
                self.dashboard(false).await
            }
            "forgetAccount" => {
                let account_id = string_param(&request.params, "accountId")?;
                self.vault().forget_account(&account_id)?;
                self.dashboard(false).await
            }
            "activateAccount" => {
                let account_id = string_param(&request.params, "accountId")?;
                if provider_mode::load_provider_mode_state(&self.provider_mode_dir)?.is_some() {
                    return Err(AppError::ProviderModeFailed(
                        "Switch back from Provider mode before activating an account".into(),
                    ));
                }
                let record = self.vault().load_record(&account_id)?;
                let (_, rollout_updates) = safely_activate_account_record_with_rollout(
                    &record,
                    &self.codex_home,
                    &self.switch_backups_dir,
                )?;
                let repair_warning = self
                    .session_manager
                    .rescan_and_repair()
                    .await
                    .err()
                    .map(|error| error.to_string());
                self.quota_cache.clear();
                let mut dashboard = self.dashboard(true).await?;
                if let Value::Object(ref mut object) = dashboard {
                    object.insert("rolloutUpdates".into(), json!(rollout_updates));
                    object.insert("repairWarning".into(), json!(repair_warning));
                }
                Ok(dashboard)
            }
            "rollback" => {
                RestorePointManager::new(self.switch_backups_dir.clone()).restore_latest()?;
                self.quota_cache.clear();
                self.dashboard(true).await
            }
            "openCodexFolder" => {
                fs::create_dir_all(&self.codex_home).map_err(|_| AppError::CodexFolderNotFound)?;
                open::that(&self.codex_home)
                    .map_err(|error| AppError::AccountVaultFailed(error.to_string()))?;
                Ok(json!({ "opened": true }))
            }
            "openVaultFolder" => {
                fs::create_dir_all(&self.accounts_dir)
                    .map_err(|error| AppError::AccountVaultFailed(error.to_string()))?;
                open::that(&self.accounts_dir)
                    .map_err(|error| AppError::AccountVaultFailed(error.to_string()))?;
                Ok(json!({ "opened": true }))
            }
            "openSessionManager" => {
                let started = self.session_manager.open_in_browser().await?;
                Ok(json!({ "opened": true, "started": started }))
            }
            "repair" => {
                let summary = self.session_manager.rescan_and_repair().await?;
                Ok(serde_json::to_value(summary)
                    .map_err(|error| AppError::RepairFailed(error.to_string()))?)
            }
            "shutdown" => {
                self.shutdown_requested = true;
                Ok(json!({ "stopping": true }))
            }
            _ => Err(AppError::AccountValidationFailed(format!(
                "Unknown CoreHost method: {}",
                request.method
            ))),
        }
    }

    async fn dashboard(&mut self, refresh: bool) -> Result<Value, AppError> {
        let requested_at = Utc::now();
        let listed = self.vault().list_accounts()?;
        let (active_account_id, active_owner_account_id) =
            detect_active_account(&self.codex_home, &listed.records);
        self.quota_cache
            .synchronize_owner(&active_owner_account_id);
        if self
            .quota_cache
            .should_refresh(refresh, &active_owner_account_id)
        {
            let refresh_owner_account_id = active_owner_account_id.clone();
            match quota::fetch_current_quota(&self.codex_home, Duration::from_secs(12)).await {
                Ok(quota) => self
                    .quota_cache
                    .record_success(refresh_owner_account_id, quota),
                Err(error) => self
                    .quota_cache
                    .record_failure(refresh_owner_account_id, RpcError::from(error)),
            }
        }

        let quota = self.quota_cache.snapshot_for(&active_owner_account_id);
        let last_error = self.quota_cache.error_for(&active_owner_account_id);
        let accounts = listed
            .records
            .into_iter()
            .map(|record| AccountView {
                active: active_account_id.as_deref() == Some(record.id.as_str()),
                id: record.id.as_str().to_string(),
                display_name: record.metadata.display_name,
                kind: record.metadata.kind,
            })
            .collect();
        let loaded = load_settings(&self.settings_path);
        let state = DashboardState {
            schema_version: 1,
            quota,
            accounts,
            active_account_id,
            last_error,
            settings: loaded.settings,
            settings_issue: listed.issue.or(loaded.issue),
            codex_home: self.codex_home.to_string_lossy().to_string(),
            updated_at: requested_at,
        };
        serde_json::to_value(state).map_err(|error| AppError::QuotaRefreshFailed(error.to_string()))
    }

    fn vault(&self) -> AccountVault {
        AccountVault::new(self.accounts_dir.clone())
    }
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("CoreHost failed: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let resource_root = resource_root_from_args();
    let mut host = Host::new(resource_root)?;
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) if !line.trim().is_empty() => line,
            Ok(_) => continue,
            Err(error) => return Err(Box::new(error)),
        };
        let response = match serde_json::from_str::<RpcRequest>(&line) {
            Ok(request) => match host.handle(&request).await {
                Ok(result) => RpcResponse {
                    id: request.id,
                    ok: true,
                    result: Some(result),
                    error: None,
                },
                Err(error) => RpcResponse {
                    id: request.id,
                    ok: false,
                    result: None,
                    error: Some(error.into()),
                },
            },
            Err(error) => RpcResponse {
                id: Value::Null,
                ok: false,
                result: None,
                error: Some(RpcError {
                    code: "invalidRequest",
                    message: "Invalid JSON Lines request".into(),
                    diagnostics: Some(error.to_string()),
                }),
            },
        };
        serde_json::to_writer(&mut stdout, &response)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
        if host.shutdown_requested {
            break;
        }
    }

    host.session_manager.stop_owned_process().await;
    Ok(())
}

fn resolve_app_data_dir() -> Result<PathBuf, AppError> {
    if let Some(override_path) = std::env::var_os("CODEX_QUOTA_VIEWER_APP_DATA") {
        return Ok(PathBuf::from(override_path));
    }
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("com.halfmelon.codexquotaviewer.windows"))
        .ok_or_else(|| AppError::SettingsLoadFailed("APPDATA is not set".into()))
}

fn resolve_core_host_codex_home() -> Result<PathBuf, AppError> {
    if let Some(override_path) = std::env::var_os("CODEX_QUOTA_VIEWER_CODEX_HOME") {
        return Ok(PathBuf::from(override_path));
    }
    codex_home::resolve_codex_home()
}

fn resource_root_from_args() -> PathBuf {
    let args: Vec<String> = std::env::args().collect();
    args.windows(2)
        .find(|pair| pair[0] == "--resource-root")
        .map(|pair| PathBuf::from(&pair[1]))
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(Path::to_path_buf))
        })
        .unwrap_or_else(|| PathBuf::from("."))
}

fn resolve_resource_path(root: &Path, relative: &str) -> Option<PathBuf> {
    let direct = root.join(relative);
    if direct.exists() {
        return Some(direct);
    }
    let mut cursor = Some(root);
    while let Some(path) = cursor {
        let candidate = path.join("Vendor").join("CodexMM");
        let resolved = if relative == "SessionManager" {
            candidate
        } else {
            path.join(relative)
        };
        if resolved.exists() {
            return Some(resolved);
        }
        cursor = path.parent();
    }
    None
}

fn find_executable_on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|path| path.join(name))
            .find(|path| path.is_file())
    })
}

fn string_param(params: &Value, name: &str) -> Result<String, AppError> {
    params
        .get(name)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| AppError::AccountValidationFailed(format!("Missing {name}")))
}

fn optional_string_param(params: &Value, name: &str) -> Option<String> {
    params
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn value_param<T: serde::de::DeserializeOwned>(params: &Value, name: &str) -> Result<T, AppError> {
    serde_json::from_value(
        params
            .get(name)
            .cloned()
            .ok_or_else(|| AppError::AccountValidationFailed(format!("Missing {name}")))?,
    )
    .map_err(|error| AppError::AccountValidationFailed(error.to_string()))
}

fn detect_active_account(
    codex_home: &Path,
    records: &[VaultAccountRecord],
) -> (Option<String>, Option<String>) {
    let auth = fs::read(codex_home.join("auth.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    let config = fs::read_to_string(codex_home.join("config.toml")).unwrap_or_default();
    let active_account_id = records
        .iter()
        .find(|record| record_matches_current(record, auth.as_ref(), &config))
        .map(|record| record.id.as_str().to_string());
    let owner_account_id =
        current_account_owner_id(active_account_id.as_deref(), auth.as_ref(), config.as_str());
    (active_account_id, owner_account_id)
}

fn current_account_owner_id(
    active_account_id: Option<&str>,
    auth: Option<&Value>,
    config: &str,
) -> Option<String> {
    if let Some(account_id) = active_account_id {
        return Some(account_id.to_string());
    }
    let auth = auth?;
    if let Some(account_id) = auth_account_id(auth) {
        return Some(format!("current:id:{account_id}"));
    }
    if let Some(email) = auth_email(auth).filter(|value| !value.trim().is_empty()) {
        return Some(format!("current:email:{}", email.trim().to_lowercase()));
    }

    // This value never leaves the CoreHost. It only distinguishes unsaved API or
    // otherwise anonymous auth payloads while the process is alive.
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    auth.to_string().hash(&mut hasher);
    config.hash(&mut hasher);
    Some(format!("current:fingerprint:{:016x}", hasher.finish()))
}

fn record_matches_current(record: &VaultAccountRecord, auth: Option<&Value>, config: &str) -> bool {
    let Some(auth) = auth else { return false };
    match &record.payload {
        AccountPayload::ChatGpt { auth_json } => {
            auth == auth_json
                || auth_account_id(auth)
                    .zip(auth_account_id(auth_json))
                    .map(|(left, right)| left == right)
                    .unwrap_or(false)
                || auth_email(auth)
                    .zip(auth_email(auth_json))
                    .map(|(left, right)| left.eq_ignore_ascii_case(&right))
                    .unwrap_or(false)
        }
        AccountPayload::Api(payload) => {
            auth.get("OPENAI_API_KEY").and_then(Value::as_str) == Some(payload.api_key.as_str())
                && configured_provider_base_url(config)
                    .zip(normalize_api_base_url(&payload.base_url, true))
                    .map(|(configured, saved)| configured == saved)
                    .unwrap_or(false)
        }
    }
}

fn configured_provider_base_url(config: &str) -> Option<String> {
    let document = toml::from_str::<toml::Value>(config).ok()?;
    let provider = document.get("model_provider")?.as_str()?;
    let base_url = document
        .get("model_providers")?
        .get(provider)?
        .get("base_url")?
        .as_str()?;
    normalize_api_base_url(base_url, true)
}

fn auth_email(auth: &Value) -> Option<String> {
    auth.pointer("/account/email")
        .or_else(|| auth.pointer("/tokens/id_token/email"))
        .or_else(|| auth.get("email"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn auth_account_id(auth: &Value) -> Option<String> {
    auth.pointer("/account/id")
        .or_else(|| auth.pointer("/tokens/account_id"))
        .or_else(|| auth.get("account_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn error_code(error: &AppError) -> &'static str {
    match error {
        AppError::CodexFolderNotFound => "codexFolderNotFound",
        AppError::SignInRequired => "signInRequired",
        AppError::QuotaTimeout => "quotaTimeout",
        AppError::QuotaRefreshFailed(_) => "quotaRefreshFailed",
        AppError::SessionManagerPortInUse => "sessionManagerPortInUse",
        AppError::SessionManagerFilesIncomplete => "sessionManagerFilesIncomplete",
        AppError::NodeRuntimeMissing => "nodeRuntimeMissing",
        AppError::SessionManagerStartFailed(_) => "sessionManagerStartFailed",
        AppError::SettingsLoadFailed(_) => "settingsLoadFailed",
        AppError::SettingsSaveFailed(_) => "settingsSaveFailed",
        AppError::LaunchAtLoginFailed(_) => "launchAtLoginFailed",
        AppError::AccountVaultFailed(_) => "accountVaultFailed",
        AppError::AccountValidationFailed(_) => "accountValidationFailed",
        AppError::AccountNotFound(_) => "accountNotFound",
        AppError::AccountActivationFailed(_) => "accountActivationFailed",
        AppError::ProviderModeFailed(_) => "providerModeFailed",
        AppError::ProviderModeNotActive => "providerModeNotActive",
        AppError::RestorePointFailed(_) => "restorePointFailed",
        AppError::RestorePointUnavailable => "restorePointUnavailable",
        AppError::RepairFailed(_) => "repairFailed",
        AppError::CodexDesktopControlFailed(_) => "codexDesktopControlFailed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use account_models::{AccountId, ApiAccountPayload};
    use quota::{AccountSummary, QuotaWindow};

    #[test]
    fn chatgpt_record_matches_current_email() {
        let record = VaultAccountRecord::new_chatgpt(
            AccountId::new("chat"),
            "Personal",
            json!({ "account": { "email": "ada@example.com" }, "token": "old" }),
            Utc::now(),
        );
        assert!(record_matches_current(
            &record,
            Some(&json!({ "account": { "email": "ADA@example.com" }, "token": "new" })),
            ""
        ));
    }

    #[test]
    fn chatgpt_record_matches_stable_token_account_id_after_refresh() {
        let record = VaultAccountRecord::new_chatgpt(
            AccountId::new("chat"),
            "Personal",
            json!({ "tokens": { "account_id": "acct-stable", "access_token": "old" } }),
            Utc::now(),
        );
        assert!(record_matches_current(
            &record,
            Some(&json!({ "tokens": { "account_id": "acct-stable", "access_token": "new" } })),
            ""
        ));
    }

    #[test]
    fn api_record_requires_key_and_base_url() {
        let record = VaultAccountRecord::new_api(
            AccountId::new("api"),
            "Workspace",
            ApiAccountPayload {
                api_key: "sk-test".into(),
                base_url: "https://example.test/v1".into(),
                model: None,
                provider_name: Some("workspace".into()),
            },
            Utc::now(),
        );
        assert!(record_matches_current(
            &record,
            Some(&json!({ "OPENAI_API_KEY": "sk-test" })),
            "model_provider = \"workspace\"\n[model_providers.workspace]\nbase_url = \"https://example.test/v1/\""
        ));
        assert!(!record_matches_current(
            &record,
            Some(&json!({ "OPENAI_API_KEY": "wrong" })),
            "model_provider = \"workspace\"\n[model_providers.workspace]\nbase_url = \"https://example.test/v1\""
        ));
        assert!(!record_matches_current(
            &record,
            Some(&json!({ "OPENAI_API_KEY": "sk-test" })),
            "model_provider = \"other\"\n[model_providers.other]\nbase_url = \"https://other.test/v1\"\n[model_providers.workspace]\nbase_url = \"https://example.test/v1\""
        ));
    }

    #[test]
    fn api_record_normalizes_equivalent_provider_urls() {
        let record = VaultAccountRecord::new_api(
            AccountId::new("api"),
            "Workspace",
            ApiAccountPayload {
                api_key: "sk-test".into(),
                base_url: "https://EXAMPLE.test".into(),
                model: None,
                provider_name: Some("workspace".into()),
            },
            Utc::now(),
        );
        assert!(record_matches_current(
            &record,
            Some(&json!({ "OPENAI_API_KEY": "sk-test" })),
            "model_provider = \"workspace\"\n[model_providers.workspace]\nbase_url = \"https://example.test/v1/\""
        ));
    }

    #[test]
    fn quota_owner_prefers_stable_vault_account_id() {
        let auth = json!({ "account": { "email": "ada@example.com" } });
        assert_eq!(
            current_account_owner_id(Some("acct-chat"), Some(&auth), ""),
            Some("acct-chat".into())
        );
    }

    #[test]
    fn quota_owner_distinguishes_unsaved_accounts() {
        let first = json!({ "account": { "email": "ada@example.com" } });
        let second = json!({ "account": { "email": "grace@example.com" } });
        assert_ne!(
            current_account_owner_id(None, Some(&first), ""),
            current_account_owner_id(None, Some(&second), "")
        );
    }

    #[test]
    fn quota_owner_distinguishes_unsaved_api_configuration() {
        let auth = json!({ "OPENAI_API_KEY": "sk-private" });
        assert_ne!(
            current_account_owner_id(None, Some(&auth), "base_url = 'https://one.example/v1'"),
            current_account_owner_id(None, Some(&auth), "base_url = 'https://two.example/v1'")
        );
    }

    #[test]
    fn quota_cache_persists_same_owner_failure_and_hides_it_after_switch() {
        let owner = Some("acct-a".to_string());
        let mut cache = QuotaCache::default();
        cache.record_success(owner.clone(), quota_snapshot(71.0));
        let error = RpcError {
            code: "quotaTimeout",
            message: "Quota refresh timed out".into(),
            diagnostics: None,
        };
        cache.record_failure(owner.clone(), error.clone());

        cache.synchronize_owner(&owner);
        assert!(!cache.should_refresh(false, &owner));
        assert_eq!(cache.error_for(&owner), Some(error));
        assert_eq!(
            cache.snapshot_for(&owner).unwrap().windows[0].remaining_percent,
            71.0
        );

        let other_owner = Some("acct-b".to_string());
        cache.synchronize_owner(&other_owner);
        assert!(cache.error_for(&other_owner).is_none());
        assert!(cache.snapshot_for(&other_owner).is_none());
        assert!(cache.should_refresh(false, &other_owner));
    }

    #[test]
    fn quota_cache_success_clears_same_owner_failure() {
        let owner = Some("acct-a".to_string());
        let mut cache = QuotaCache::default();
        cache.record_failure(
            owner.clone(),
            RpcError {
                code: "quotaTimeout",
                message: "Quota refresh timed out".into(),
                diagnostics: None,
            },
        );

        cache.record_success(owner.clone(), quota_snapshot(82.0));

        assert!(cache.error_for(&owner).is_none());
        assert_eq!(
            cache.snapshot_for(&owner).unwrap().windows[0].remaining_percent,
            82.0
        );
    }

    #[test]
    fn protocol_serializes_one_line_response() {
        let response = RpcResponse {
            id: json!(7),
            ok: true,
            result: Some(json!({ "pong": true })),
            error: None,
        };
        let line = serde_json::to_string(&response).unwrap();
        assert!(!line.contains('\n'));
        assert!(line.contains("\"ok\":true"));
    }

    fn quota_snapshot(remaining_percent: f64) -> QuotaSnapshot {
        QuotaSnapshot {
            account: AccountSummary {
                id: Some("remote-account".into()),
                email: Some("ada@example.com".into()),
                account_type: "chatgpt".into(),
            },
            windows: vec![QuotaWindow {
                label: "1w".into(),
                remaining_percent,
                window_duration_mins: Some(10_080),
                resets_at: None,
            }],
            fetched_at: Utc::now(),
        }
    }
}
