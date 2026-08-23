use std::fs;
use std::sync::atomic::Ordering;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::account_models::{AccountKind, AccountPayload, VaultAccountRecord};
use crate::account_vault::AccountVault;
use crate::app_state::SharedAppState;
use crate::localization::{app_error_message, localize, LocalizedText};
use crate::provider_mode::{load_provider_mode_state, ProviderModeState};
use crate::quota::{QuotaSnapshot, QuotaWindow};
use crate::settings::ResolvedAppLanguage;

pub const DASHBOARD_STATE_EVENT: &str = "dashboard-state-changed";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DashboardStatus {
    Loading,
    Ready,
    Empty,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardQuotaWindow {
    pub label: String,
    pub display_label: String,
    pub remaining_percent: f64,
    pub reset_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardAccount {
    pub id: String,
    pub display_name: String,
    pub kind: AccountKind,
    pub is_active: bool,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardCurrentAccount {
    pub display_name: String,
    pub detail: String,
    pub kind: AccountKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardLabels {
    pub title: String,
    pub active_account: String,
    pub refreshing: String,
    pub refresh: String,
    pub settings: String,
    pub quota: String,
    pub remaining: String,
    pub resets_in: String,
    pub resets_at: String,
    pub reset_unavailable: String,
    pub accounts: String,
    pub current: String,
    pub available: String,
    pub switch_account: String,
    pub no_saved_accounts: String,
    pub session_manager: String,
    pub repair_now: String,
    pub open_codex_folder: String,
    pub updated: String,
    pub just_now: String,
    pub updated_just_now: String,
    pub never_updated: String,
    pub quota_loading: String,
    pub quota_unavailable: String,
    pub no_quota_windows: String,
    pub try_again: String,
    pub switch_confirm: String,
    pub switching: String,
    pub repairing: String,
    pub repair_complete: String,
    pub action_failed: String,
    pub provider_active: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardState {
    pub schema_version: u32,
    pub state_revision: u64,
    pub status: DashboardStatus,
    pub language: ResolvedAppLanguage,
    pub labels: DashboardLabels,
    pub current_account: Option<DashboardCurrentAccount>,
    pub quota_windows: Vec<DashboardQuotaWindow>,
    pub accounts: Vec<DashboardAccount>,
    pub is_refreshing: bool,
    pub refresh_requested_revision: u64,
    pub refresh_completed_revision: u64,
    pub fetched_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub notice: Option<String>,
}

pub async fn build_dashboard_state(state: &SharedAppState) -> DashboardState {
    let state_revision = state
        .dashboard_revision
        .fetch_add(1, Ordering::SeqCst)
        .saturating_add(1);
    let (snapshot, refresh_requested_revision, refresh_completed_revision) = {
        let snapshot = state.tray_snapshot.lock().await;
        let gate = state
            .refresh_gate
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (requested, completed) = gate.revisions();
        (snapshot.clone(), requested, completed)
    };
    let language = super::current_resolved_language(state).await;
    let labels = dashboard_labels(language);
    let vault = AccountVault::new(state.accounts_dir.clone());

    let (records, mut notice) = match vault.list_accounts() {
        Ok(listed) => (listed.records, listed.issue),
        Err(error) => (Vec::new(), Some(app_error_message(language, &error))),
    };
    let provider_mode = match load_provider_mode_state(&state.provider_mode_dir) {
        Ok(provider_mode) => provider_mode,
        Err(error) => {
            append_notice(&mut notice, app_error_message(language, &error));
            None
        }
    };
    let current_auth = fs::read(state.codex_home.join("auth.json"))
        .ok()
        .and_then(|data| serde_json::from_slice::<serde_json::Value>(&data).ok());
    let current_config = fs::read_to_string(state.codex_home.join("config.toml")).ok();
    let active_account_id = active_account_id(
        &records,
        current_auth.as_ref(),
        current_config.as_deref(),
        provider_mode.as_ref(),
    );
    let current_quota = snapshot.quota.as_ref().filter(|_| {
        should_display_quota(
            active_account_id.as_deref(),
            snapshot.quota_owner_account_id.as_deref(),
        )
    });
    let accounts = dashboard_accounts(&records, active_account_id.as_deref(), &labels);
    let current_account = current_account(
        &records,
        active_account_id.as_deref(),
        current_quota,
        provider_mode.as_ref(),
        current_auth.as_ref(),
        &labels,
    );
    let quota_windows: Vec<DashboardQuotaWindow> = current_quota
        .map(|quota| {
            quota
                .windows
                .iter()
                .map(|window| dashboard_quota_window(window, language))
                .collect()
        })
        .unwrap_or_default();
    let error = snapshot
        .last_error
        .as_ref()
        .map(|error| app_error_message(language, error));
    let refresh_pending =
        snapshot.is_refreshing || refresh_requested_revision > refresh_completed_revision;
    let status = dashboard_status(
        current_quota.is_some(),
        quota_windows.is_empty(),
        refresh_pending,
        snapshot.last_error.is_some(),
    );

    DashboardState {
        schema_version: 1,
        state_revision,
        status,
        language,
        labels,
        current_account,
        quota_windows,
        accounts,
        is_refreshing: refresh_pending,
        refresh_requested_revision,
        refresh_completed_revision,
        fetched_at: current_quota.map(|quota| quota.fetched_at),
        error,
        notice,
    }
}

pub async fn emit_dashboard_state(app: &AppHandle, state: &SharedAppState) -> tauri::Result<()> {
    let dashboard = build_dashboard_state(state).await;
    app.emit_to("widget", DASHBOARD_STATE_EVENT, dashboard)
}

pub(crate) fn resolve_active_account_id(state: &SharedAppState) -> Option<String> {
    let records = AccountVault::new(state.accounts_dir.clone())
        .list_accounts()
        .ok()?
        .records;
    let provider_mode = load_provider_mode_state(&state.provider_mode_dir)
        .ok()
        .flatten();
    let current_auth = fs::read(state.codex_home.join("auth.json"))
        .ok()
        .and_then(|data| serde_json::from_slice::<serde_json::Value>(&data).ok());
    let current_config = fs::read_to_string(state.codex_home.join("config.toml")).ok();
    active_account_id(
        &records,
        current_auth.as_ref(),
        current_config.as_deref(),
        provider_mode.as_ref(),
    )
}

fn should_display_quota(
    active_account_id: Option<&str>,
    quota_owner_account_id: Option<&str>,
) -> bool {
    match (active_account_id, quota_owner_account_id) {
        (Some(active), Some(owner)) => active == owner,
        (Some(_), None) => false,
        (None, _) => true,
    }
}

fn dashboard_status(
    has_current_quota: bool,
    quota_windows_empty: bool,
    refresh_pending: bool,
    has_error: bool,
) -> DashboardStatus {
    if has_current_quota {
        if quota_windows_empty {
            DashboardStatus::Empty
        } else {
            DashboardStatus::Ready
        }
    } else if refresh_pending {
        DashboardStatus::Loading
    } else if has_error {
        DashboardStatus::Error
    } else {
        DashboardStatus::Loading
    }
}

fn dashboard_quota_window(
    window: &QuotaWindow,
    language: ResolvedAppLanguage,
) -> DashboardQuotaWindow {
    DashboardQuotaWindow {
        label: window.label.clone(),
        display_label: window_display_label(&window.label, language),
        remaining_percent: window.remaining_percent.clamp(0.0, 100.0),
        reset_at: window.reset_at,
    }
}

fn window_display_label(label: &str, language: ResolvedAppLanguage) -> String {
    match (label, language) {
        ("5h", ResolvedAppLanguage::English) => "5-hour limit".to_string(),
        ("5h", ResolvedAppLanguage::Chinese) => "5 小时额度".to_string(),
        ("1w", ResolvedAppLanguage::English) => "Weekly limit".to_string(),
        ("1w", ResolvedAppLanguage::Chinese) => "每周额度".to_string(),
        (_, ResolvedAppLanguage::English) => format!("{label} limit"),
        (_, ResolvedAppLanguage::Chinese) => format!("{label} 额度"),
    }
}

fn dashboard_accounts(
    records: &[VaultAccountRecord],
    active_account_id: Option<&str>,
    labels: &DashboardLabels,
) -> Vec<DashboardAccount> {
    records
        .iter()
        .map(|record| {
            let is_active = active_account_id == Some(record.id.as_str());
            DashboardAccount {
                id: record.id.as_str().to_string(),
                display_name: record.metadata.display_name.clone(),
                kind: record.metadata.kind,
                is_active,
                status: if is_active {
                    labels.current.clone()
                } else {
                    labels.available.clone()
                },
            }
        })
        .collect()
}

fn current_account(
    records: &[VaultAccountRecord],
    active_account_id: Option<&str>,
    quota: Option<&QuotaSnapshot>,
    provider_mode: Option<&ProviderModeState>,
    current_auth: Option<&serde_json::Value>,
    labels: &DashboardLabels,
) -> Option<DashboardCurrentAccount> {
    if let Some(record) = active_account_id.and_then(|active_id| {
        records
            .iter()
            .find(|record| record.id.as_str() == active_id)
    }) {
        let detail = if provider_mode.is_some() {
            labels.provider_active.clone()
        } else {
            quota
                .and_then(|quota| quota.account.email.clone())
                .unwrap_or_else(|| account_kind_label(record.metadata.kind).to_string())
        };
        return Some(DashboardCurrentAccount {
            display_name: record.metadata.display_name.clone(),
            detail,
            kind: record.metadata.kind,
        });
    }

    quota
        .map(|quota| DashboardCurrentAccount {
            display_name: quota
                .account
                .email
                .clone()
                .or_else(|| quota.account.id.clone())
                .unwrap_or_else(|| labels.active_account.clone()),
            detail: quota.account.account_type.clone(),
            kind: auth_account_kind(current_auth),
        })
        .or_else(|| {
            is_api_auth(current_auth?).then(|| DashboardCurrentAccount {
                display_name: "API".to_string(),
                detail: "API".to_string(),
                kind: AccountKind::Api,
            })
        })
}

fn active_account_id(
    records: &[VaultAccountRecord],
    current_auth: Option<&serde_json::Value>,
    current_config: Option<&str>,
    provider_mode: Option<&ProviderModeState>,
) -> Option<String> {
    if let Some(provider_mode) = provider_mode {
        return records
            .iter()
            .find(|record| record.id.as_str() == provider_mode.provider_account_id)
            .map(|record| record.id.as_str().to_string());
    }

    let current_auth = current_auth?;
    records
        .iter()
        .find(|record| match &record.payload {
            AccountPayload::ChatGpt { auth_json } => {
                auth_json == current_auth || auth_identity_matches(auth_json, current_auth)
            }
            AccountPayload::Api(payload) => {
                current_auth
                    .get("OPENAI_API_KEY")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|api_key| api_key == payload.api_key.as_str())
                    && current_config
                        .is_some_and(|config| config_matches_base_url(config, &payload.base_url))
            }
        })
        .map(|record| record.id.as_str().to_string())
}

fn config_matches_base_url(config: &str, expected_base_url: &str) -> bool {
    config.lines().any(|line| {
        let Some((key, value)) = line.split_once('=') else {
            return false;
        };
        if key.trim() != "base_url" {
            return false;
        }
        serde_json::from_str::<String>(value.trim())
            .ok()
            .and_then(|base_url| normalize_base_url(&base_url))
            .zip(normalize_base_url(expected_base_url))
            .is_some_and(|(actual, expected)| actual == expected)
    })
}

fn normalize_base_url(base_url: &str) -> Option<String> {
    let mut parsed = url::Url::parse(base_url.trim()).ok()?;
    parsed.set_fragment(None);
    let path = parsed.path().trim_end_matches('/').to_string();
    parsed.set_path(if path.is_empty() { "/" } else { &path });
    Some(parsed.as_str().trim_end_matches('/').to_string())
}

fn auth_account_kind(current_auth: Option<&serde_json::Value>) -> AccountKind {
    if current_auth.is_some_and(|auth| is_api_auth(auth)) {
        AccountKind::Api
    } else {
        AccountKind::ChatGpt
    }
}

fn is_api_auth(auth: &serde_json::Value) -> bool {
    auth.get("OPENAI_API_KEY")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|api_key| !api_key.trim().is_empty())
        || auth
            .get("type")
            .or_else(|| auth.get("auth_mode"))
            .and_then(serde_json::Value::as_str)
            .is_some_and(|kind| matches!(kind.to_ascii_lowercase().as_str(), "api" | "apikey"))
}

fn auth_identity_matches(left: &serde_json::Value, right: &serde_json::Value) -> bool {
    const PATHS: &[&[&str]] = &[
        &["account", "id"],
        &["account", "email"],
        &["tokens", "account_id"],
        &["tokens", "email"],
    ];

    PATHS.iter().any(|path| {
        let left = json_string_at_path(left, path);
        let right = json_string_at_path(right, path);
        left.is_some() && left == right
    })
}

fn json_string_at_path<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a str> {
    path.iter()
        .try_fold(value, |node, key| node.get(*key))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

fn account_kind_label(kind: AccountKind) -> &'static str {
    match kind {
        AccountKind::ChatGpt => "ChatGPT",
        AccountKind::Api => "API",
    }
}

fn append_notice(notice: &mut Option<String>, next: String) {
    match notice {
        Some(existing) => {
            existing.push_str("; ");
            existing.push_str(&next);
        }
        None => *notice = Some(next),
    }
}

fn dashboard_labels(language: ResolvedAppLanguage) -> DashboardLabels {
    let text = |english, chinese| localize(language, LocalizedText::new(english, chinese));
    DashboardLabels {
        title: "Codex Quota Viewer".to_string(),
        active_account: text("Active account", "当前账号"),
        refreshing: text("Refreshing quota", "正在刷新额度"),
        refresh: text("Refresh", "刷新"),
        settings: text("Settings", "设置"),
        quota: text("Quota", "额度"),
        remaining: text("left", "剩余"),
        resets_in: text("Resets in", "将在以下时间后重置"),
        resets_at: text("Resets", "重置于"),
        reset_unavailable: text("Reset time unavailable", "暂无重置时间"),
        accounts: text("Accounts", "账号"),
        current: text("Current", "当前"),
        available: text("Available", "可用"),
        switch_account: text("Switch", "切换"),
        no_saved_accounts: text(
            "No saved accounts. Add one in Settings.",
            "暂无已保存账号，请在设置中添加。",
        ),
        session_manager: text("Session Manager", "会话管理器"),
        repair_now: text("Repair now", "立即修复"),
        open_codex_folder: text("Open Codex folder", "打开 Codex 文件夹"),
        updated: text("Updated", "更新于"),
        just_now: text("just now", "刚刚"),
        updated_just_now: text("Updated just now", "刚刚更新"),
        never_updated: text("Not updated yet", "尚未更新"),
        quota_loading: text("Reading your latest quota…", "正在读取最新额度…"),
        quota_unavailable: text("Quota is unavailable", "暂时无法读取额度"),
        no_quota_windows: text(
            "No quota windows were returned for this account.",
            "此账号暂未返回额度窗口。",
        ),
        try_again: text("Try again", "重试"),
        switch_confirm: text(
            "Safely switch to this account? A restore point will be created first.",
            "安全切换到此账号？程序会先创建还原点。",
        ),
        switching: text("Switching account…", "正在切换账号…"),
        repairing: text("Repairing local sessions…", "正在修复本地会话…"),
        repair_complete: text("Repair complete", "修复完成"),
        action_failed: text("Action failed", "操作失败"),
        provider_active: text("Third-party Provider active", "第三方 Provider 已启用"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account_models::{AccountId, ApiAccountPayload};
    use chrono::TimeZone;
    use serde_json::json;

    fn chatgpt_record(id: &str, auth_json: serde_json::Value) -> VaultAccountRecord {
        VaultAccountRecord::new_chatgpt(
            AccountId::new(id),
            "Personal",
            auth_json,
            Utc.with_ymd_and_hms(2026, 8, 23, 0, 0, 0).unwrap(),
        )
    }

    #[test]
    fn matches_active_chatgpt_account_after_token_rotation() {
        let records = vec![chatgpt_record(
            "personal",
            json!({"account": {"email": "ada@example.com"}, "token": "old"}),
        )];
        let current = json!({"account": {"email": "ada@example.com"}, "token": "new"});

        assert_eq!(
            active_account_id(&records, Some(&current), None, None).as_deref(),
            Some("personal")
        );
    }

    #[test]
    fn provider_mode_selects_its_api_account() {
        let now = Utc.with_ymd_and_hms(2026, 8, 23, 0, 0, 0).unwrap();
        let records = vec![VaultAccountRecord::new_api(
            AccountId::new("workspace"),
            "Workspace",
            ApiAccountPayload {
                api_key: "sk-test".into(),
                base_url: "https://api.openai.com/v1".into(),
                model: None,
                provider_name: None,
            },
            now,
        )];
        let provider = ProviderModeState {
            restore_point_id: None,
            provider_account_id: "workspace".into(),
            provider_display_name: "Workspace".into(),
            activated_at: now,
        };

        assert_eq!(
            active_account_id(&records, None, None, Some(&provider)).as_deref(),
            Some("workspace")
        );
    }

    #[test]
    fn matches_directly_activated_api_account_without_exposing_its_key() {
        let now = Utc.with_ymd_and_hms(2026, 8, 23, 0, 0, 0).unwrap();
        let records = vec![VaultAccountRecord::new_api(
            AccountId::new("workspace"),
            "Workspace",
            ApiAccountPayload {
                api_key: "sk-private".into(),
                base_url: "https://api.example.com/v1".into(),
                model: None,
                provider_name: None,
            },
            now,
        )];
        let current = json!({"OPENAI_API_KEY": "sk-private", "type": "api"});
        let config = "model_provider = \"openai\"\n[model_providers.openai]\nbase_url = \"https://api.example.com/v1/\"\n";

        assert_eq!(
            active_account_id(&records, Some(&current), Some(config), None).as_deref(),
            Some("workspace")
        );
        assert_eq!(auth_account_kind(Some(&current)), AccountKind::Api);

        let other_config =
            "[model_providers.openai]\nbase_url = \"https://other.example.com/v1\"\n";
        assert_eq!(
            active_account_id(&records, Some(&current), Some(other_config), None),
            None
        );
    }

    #[test]
    fn localizes_common_quota_window_labels() {
        assert_eq!(
            window_display_label("5h", ResolvedAppLanguage::English),
            "5-hour limit"
        );
        assert_eq!(
            window_display_label("1w", ResolvedAppLanguage::Chinese),
            "每周额度"
        );
    }

    #[test]
    fn hides_quota_owned_by_a_different_saved_account() {
        assert!(should_display_quota(Some("personal"), Some("personal")));
        assert!(!should_display_quota(Some("work"), Some("personal")));
        assert!(!should_display_quota(Some("work"), None));
        assert!(should_display_quota(None, Some("personal")));
    }

    #[test]
    fn reports_loading_while_a_new_account_snapshot_is_pending() {
        assert_eq!(
            dashboard_status(false, true, true, false),
            DashboardStatus::Loading
        );
        assert_eq!(
            dashboard_status(false, true, false, true),
            DashboardStatus::Error
        );
    }
}
