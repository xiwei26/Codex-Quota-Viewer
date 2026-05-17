use serde::Serialize;

use crate::account_activation::safely_activate_account_record_with_rollout;
use crate::account_models::{AccountKind, AccountRowState, AddApiAccountInput};
use crate::account_vault::AccountVault;
use crate::app_state::SharedAppState;
use crate::errors::AppError;
use crate::localization::{app_error_message, localize, LocalizedText};
use crate::provider_mode::{
    enter_provider_mode as enter_provider_mode_files,
    exit_provider_mode as exit_provider_mode_files, load_provider_mode_state, ProviderModeState,
};
use crate::restore_points::RestorePointManager;
use crate::rollout_sync::{
    provider_counts as rollout_provider_counts, target_provider_for_config, ProviderCount,
};
use crate::session_manager::OfficialRepairSummary;
use crate::settings::ResolvedAppLanguage;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountRow {
    pub id: String,
    pub display_name: String,
    pub kind: AccountKind,
    pub state: AccountRowState,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountLabels {
    pub accounts: String,
    pub sign_in_with_chatgpt: String,
    pub add_api_account: String,
    pub open_vault_folder: String,
    pub rollback_last_change: String,
    pub repair_now: String,
    pub activate: String,
    pub rename: String,
    pub forget: String,
    pub switch_to_provider: String,
    pub switch_back_from_provider: String,
    pub provider_mode_active: String,
    pub current: String,
    pub no_saved_accounts: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsPresentation {
    pub labels: AccountLabels,
    pub rows: Vec<AccountRow>,
    pub provider_mode: Option<ProviderModeState>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalProviderSyncPresentation {
    pub title: String,
    pub status: String,
    pub expected_provider: Option<String>,
    pub rollout_providers: Vec<ProviderCount>,
    pub thread_providers: Vec<ProviderCount>,
    pub thread_issue: Option<String>,
}

pub fn build_accounts_presentation(
    vault: &AccountVault,
    language: ResolvedAppLanguage,
    message: Option<String>,
) -> Result<AccountsPresentation, AppError> {
    let listed = vault.list_accounts()?;
    let rows = listed
        .records
        .into_iter()
        .map(|record| AccountRow {
            id: record.id.as_str().to_string(),
            display_name: record.metadata.display_name,
            kind: record.metadata.kind,
            state: AccountRowState::Available,
            status: localize(
                language,
                LocalizedText::new("Available", "\u{53ef}\u{7528}"),
            ),
        })
        .collect();

    Ok(AccountsPresentation {
        labels: AccountLabels {
            accounts: localize(language, LocalizedText::new("Accounts", "\u{8d26}\u{53f7}")),
            sign_in_with_chatgpt: localize(
                language,
                LocalizedText::new(
                    "Sign in with ChatGPT",
                    "\u{4f7f}\u{7528} ChatGPT \u{767b}\u{5f55}",
                ),
            ),
            add_api_account: localize(
                language,
                LocalizedText::new("Add API Account", "\u{6dfb}\u{52a0} API \u{8d26}\u{53f7}"),
            ),
            open_vault_folder: localize(
                language,
                LocalizedText::new(
                    "Open Vault Folder",
                    "\u{6253}\u{5f00}\u{8d26}\u{53f7}\u{4ed3}\u{6587}\u{4ef6}\u{5939}",
                ),
            ),
            rollback_last_change: localize(
                language,
                LocalizedText::new(
                    "Rollback Last Change",
                    "\u{56de}\u{6eda}\u{6700}\u{8fd1}\u{66f4}\u{6539}",
                ),
            ),
            repair_now: localize(
                language,
                LocalizedText::new("Repair Now", "\u{7acb}\u{5373}\u{4fee}\u{590d}"),
            ),
            activate: localize(language, LocalizedText::new("Activate", "\u{6fc0}\u{6d3b}")),
            rename: localize(
                language,
                LocalizedText::new("Rename", "\u{91cd}\u{547d}\u{540d}"),
            ),
            forget: localize(language, LocalizedText::new("Forget", "\u{79fb}\u{9664}")),
            switch_to_provider: localize(
                language,
                LocalizedText::new("Use as Provider", "\u{7528}\u{4f5c} Provider"),
            ),
            switch_back_from_provider: localize(
                language,
                LocalizedText::new(
                    "Switch Back from Provider",
                    "\u{9000}\u{51fa} Provider \u{6a21}\u{5f0f}",
                ),
            ),
            provider_mode_active: localize(
                language,
                LocalizedText::new(
                    "Third-party Provider mode is active",
                    "\u{7b2c}\u{4e09}\u{65b9} Provider \u{6a21}\u{5f0f}\u{5df2}\u{5f00}\u{542f}",
                ),
            ),
            current: localize(language, LocalizedText::new("Current", "\u{5f53}\u{524d}")),
            no_saved_accounts: localize(
                language,
                LocalizedText::new(
                    "No saved accounts",
                    "\u{6682}\u{65e0}\u{5df2}\u{4fdd}\u{5b58}\u{8d26}\u{53f7}",
                ),
            ),
        },
        rows,
        provider_mode: None,
        message: message.or(listed.issue),
    })
}

pub fn build_accounts_presentation_with_provider_mode(
    vault: &AccountVault,
    provider_mode_dir: &std::path::Path,
    language: ResolvedAppLanguage,
    message: Option<String>,
) -> Result<AccountsPresentation, AppError> {
    let mut presentation = build_accounts_presentation(vault, language, message)?;
    presentation.provider_mode = load_provider_mode_state(provider_mode_dir)?;
    Ok(presentation)
}

#[tauri::command]
pub async fn get_accounts(
    state: tauri::State<'_, SharedAppState>,
) -> Result<AccountsPresentation, String> {
    let app_state = state.inner().clone();
    let language = super::current_resolved_language(&app_state).await;
    let vault = AccountVault::new(app_state.accounts_dir.clone());
    build_accounts_presentation_with_provider_mode(
        &vault,
        &app_state.provider_mode_dir,
        language,
        None,
    )
    .map_err(|error| app_error_message(language, &error))
}

#[tauri::command]
pub async fn import_current_chatgpt_account(
    state: tauri::State<'_, SharedAppState>,
    display_name: Option<String>,
) -> Result<AccountsPresentation, String> {
    let app_state = state.inner().clone();
    let language = super::current_resolved_language(&app_state).await;
    let vault = AccountVault::new(app_state.accounts_dir.clone());
    vault
        .import_current_chatgpt_account(&app_state.codex_home, display_name)
        .map_err(|error| app_error_message(language, &error))?;
    build_accounts_presentation_with_provider_mode(
        &vault,
        &app_state.provider_mode_dir,
        language,
        Some(localize(
            language,
            LocalizedText::new("Account saved", "\u{8d26}\u{53f7}\u{5df2}\u{4fdd}\u{5b58}"),
        )),
    )
    .map_err(|error| app_error_message(language, &error))
}

#[tauri::command]
pub async fn add_api_account(
    state: tauri::State<'_, SharedAppState>,
    input: AddApiAccountInput,
) -> Result<AccountsPresentation, String> {
    let app_state = state.inner().clone();
    let language = super::current_resolved_language(&app_state).await;
    let vault = AccountVault::new(app_state.accounts_dir.clone());
    vault
        .add_api_account(input)
        .map_err(|error| app_error_message(language, &error))?;
    build_accounts_presentation_with_provider_mode(
        &vault,
        &app_state.provider_mode_dir,
        language,
        Some(localize(
            language,
            LocalizedText::new("Account saved", "\u{8d26}\u{53f7}\u{5df2}\u{4fdd}\u{5b58}"),
        )),
    )
    .map_err(|error| app_error_message(language, &error))
}

#[tauri::command]
pub async fn activate_account(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedAppState>,
    account_id: String,
) -> Result<AccountsPresentation, String> {
    let app_state = state.inner().clone();
    let language = super::current_resolved_language(&app_state).await;
    let vault = AccountVault::new(app_state.accounts_dir.clone());
    ensure_provider_mode_inactive(&app_state.provider_mode_dir)
        .map_err(|error| app_error_message(language, &error))?;
    let record = vault
        .load_record(&account_id)
        .map_err(|error| app_error_message(language, &error))?;
    let (_restore_point, rollout_updates) = safely_activate_account_record_with_rollout(
        &record,
        &app_state.codex_home,
        &app_state.switch_backups_dir,
    )
    .map_err(|error| app_error_message(language, &error))?;
    let repair_summary = repair_local_threads(&app_state)
        .await
        .map_err(|error| app_error_message(language, &error))?;
    super::spawn_refresh(app, app_state.clone());
    build_accounts_presentation_with_provider_mode(
        &vault,
        &app_state.provider_mode_dir,
        language,
        Some(switch_success_message(
            language,
            "Account activated",
            &repair_summary,
            rollout_updates,
        )),
    )
    .map_err(|error| app_error_message(language, &error))
}

#[tauri::command]
pub async fn rollback_last_change(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedAppState>,
) -> Result<AccountsPresentation, String> {
    let app_state = state.inner().clone();
    let language = super::current_resolved_language(&app_state).await;
    let vault = AccountVault::new(app_state.accounts_dir.clone());
    RestorePointManager::new(app_state.switch_backups_dir.clone())
        .restore_latest()
        .map_err(|error| app_error_message(language, &error))?;
    super::spawn_refresh(app, app_state.clone());
    build_accounts_presentation_with_provider_mode(
        &vault,
        &app_state.provider_mode_dir,
        language,
        Some(localize(
            language,
            LocalizedText::new(
                "Rollback complete",
                "\u{56de}\u{6eda}\u{5df2}\u{5b8c}\u{6210}",
            ),
        )),
    )
    .map_err(|error| app_error_message(language, &error))
}

#[tauri::command]
pub async fn repair_now(
    state: tauri::State<'_, SharedAppState>,
) -> Result<AccountsPresentation, String> {
    let app_state = state.inner().clone();
    let language = super::current_resolved_language(&app_state).await;
    let vault = AccountVault::new(app_state.accounts_dir.clone());
    let summary = repair_local_threads(&app_state)
        .await
        .map_err(|error| app_error_message(language, &error))?;
    build_accounts_presentation_with_provider_mode(
        &vault,
        &app_state.provider_mode_dir,
        language,
        Some(
            localize(
                language,
                LocalizedText::new(
                    "Repair complete",
                    "\u{4fee}\u{590d}\u{5df2}\u{5b8c}\u{6210}",
                ),
            ) + &format!(
                ": +{} ~{} index {}",
                summary.created_threads,
                summary.updated_threads,
                summary.updated_session_index_entries
            ),
        ),
    )
    .map_err(|error| app_error_message(language, &error))
}

#[tauri::command]
pub async fn inspect_local_provider_sync(
    state: tauri::State<'_, SharedAppState>,
) -> Result<LocalProviderSyncPresentation, String> {
    let app_state = state.inner().clone();
    let language = super::current_resolved_language(&app_state).await;
    let rollout_providers = rollout_provider_counts(&app_state.codex_home)
        .map_err(|error| app_error_message(language, &error))?;
    let expected_provider = expected_provider_from_current_config(&app_state.codex_home)
        .or_else(|| most_common_provider(&rollout_providers));

    let thread_result = {
        let mut manager = app_state.session_manager.lock().await;
        manager.provider_counts().await
    };
    let (thread_providers, thread_issue) = match thread_result {
        Ok(counts) => (
            counts
                .into_iter()
                .map(|count| ProviderCount {
                    provider_id: count.provider_id,
                    count: count.count,
                })
                .collect(),
            None,
        ),
        Err(error) => (Vec::new(), Some(app_error_message(language, &error))),
    };

    let status = provider_sync_status(
        language,
        expected_provider.as_deref(),
        &rollout_providers,
        &thread_providers,
        thread_issue.as_deref(),
    );

    Ok(LocalProviderSyncPresentation {
        title: localize(
            language,
            LocalizedText::new(
                "Local Provider Sync",
                "\u{672c}\u{5730} Provider \u{540c}\u{6b65}",
            ),
        ),
        status,
        expected_provider,
        rollout_providers,
        thread_providers,
        thread_issue,
    })
}

#[tauri::command]
pub async fn enter_provider_mode(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedAppState>,
    account_id: String,
) -> Result<AccountsPresentation, String> {
    let app_state = state.inner().clone();
    let language = super::current_resolved_language(&app_state).await;
    let vault = AccountVault::new(app_state.accounts_dir.clone());
    let record = vault
        .load_record(&account_id)
        .map_err(|error| app_error_message(language, &error))?;
    let (_provider_state, rollout_updates) = enter_provider_mode_files(
        &record,
        &app_state.codex_home,
        &app_state.provider_mode_dir,
        &app_state.switch_backups_dir,
    )
    .map_err(|error| app_error_message(language, &error))?;
    let repair_summary = repair_local_threads(&app_state)
        .await
        .map_err(|error| app_error_message(language, &error))?;
    super::spawn_refresh(app, app_state.clone());
    build_accounts_presentation_with_provider_mode(
        &vault,
        &app_state.provider_mode_dir,
        language,
        Some(switch_success_message(
            language,
            "Third-party Provider enabled",
            &repair_summary,
            rollout_updates,
        )),
    )
    .map_err(|error| app_error_message(language, &error))
}

fn ensure_provider_mode_inactive(provider_mode_dir: &std::path::Path) -> Result<(), AppError> {
    if load_provider_mode_state(provider_mode_dir)?.is_some() {
        return Err(AppError::ProviderModeFailed(
            "switch back from third-party Provider mode before activating another account".into(),
        ));
    }
    Ok(())
}

#[tauri::command]
pub async fn exit_provider_mode(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedAppState>,
) -> Result<AccountsPresentation, String> {
    let app_state = state.inner().clone();
    let language = super::current_resolved_language(&app_state).await;
    let vault = AccountVault::new(app_state.accounts_dir.clone());
    exit_provider_mode_files(
        &app_state.codex_home,
        &app_state.provider_mode_dir,
        &app_state.switch_backups_dir,
    )
    .map_err(|error| app_error_message(language, &error))?;
    let repair_summary = repair_local_threads(&app_state)
        .await
        .map_err(|error| app_error_message(language, &error))?;
    super::spawn_refresh(app, app_state.clone());
    build_accounts_presentation_with_provider_mode(
        &vault,
        &app_state.provider_mode_dir,
        language,
        Some(switch_success_message(
            language,
            "Restored normal ChatGPT mode",
            &repair_summary,
            0,
        )),
    )
    .map_err(|error| app_error_message(language, &error))
}

#[tauri::command]
pub async fn rename_account(
    state: tauri::State<'_, SharedAppState>,
    account_id: String,
    display_name: String,
) -> Result<AccountsPresentation, String> {
    let app_state = state.inner().clone();
    let language = super::current_resolved_language(&app_state).await;
    let vault = AccountVault::new(app_state.accounts_dir.clone());
    vault
        .rename_account(&account_id, &display_name)
        .map_err(|error| app_error_message(language, &error))?;
    build_accounts_presentation_with_provider_mode(
        &vault,
        &app_state.provider_mode_dir,
        language,
        Some(localize(
            language,
            LocalizedText::new(
                "Account renamed",
                "\u{8d26}\u{53f7}\u{5df2}\u{91cd}\u{547d}\u{540d}",
            ),
        )),
    )
    .map_err(|error| app_error_message(language, &error))
}

#[tauri::command]
pub async fn forget_account(
    state: tauri::State<'_, SharedAppState>,
    account_id: String,
) -> Result<AccountsPresentation, String> {
    let app_state = state.inner().clone();
    let language = super::current_resolved_language(&app_state).await;
    let vault = AccountVault::new(app_state.accounts_dir.clone());
    vault
        .forget_account(&account_id)
        .map_err(|error| app_error_message(language, &error))?;
    build_accounts_presentation_with_provider_mode(
        &vault,
        &app_state.provider_mode_dir,
        language,
        Some(localize(
            language,
            LocalizedText::new(
                "Account forgotten",
                "\u{8d26}\u{53f7}\u{5df2}\u{79fb}\u{9664}",
            ),
        )),
    )
    .map_err(|error| app_error_message(language, &error))
}

#[tauri::command]
pub async fn open_vault_folder(state: tauri::State<'_, SharedAppState>) -> Result<(), String> {
    let app_state = state.inner().clone();
    std::fs::create_dir_all(&app_state.accounts_dir).map_err(|error| error.to_string())?;
    open::that(&app_state.accounts_dir).map_err(|error| error.to_string())
}

pub fn spawn_activate_account_from_tray(
    app: tauri::AppHandle,
    state: SharedAppState,
    account_id: String,
) {
    tauri::async_runtime::spawn(async move {
        let language = super::current_resolved_language(&state).await;
        let vault = AccountVault::new(state.accounts_dir.clone());
        let result = async {
            ensure_provider_mode_inactive(&state.provider_mode_dir)
                .map_err(|error| app_error_message(language, &error))?;
            let record = vault
                .load_record(&account_id)
                .map_err(|error| app_error_message(language, &error))?;
            let (_restore_point, _rollout_updates) = safely_activate_account_record_with_rollout(
                &record,
                &state.codex_home,
                &state.switch_backups_dir,
            )
            .map_err(|error| app_error_message(language, &error))?;
            repair_local_threads(&state)
                .await
                .map_err(|error| app_error_message(language, &error))?;
            Ok::<(), String>(())
        }
        .await;

        if let Err(error) = result {
            let mut snapshot = state.tray_snapshot.lock().await;
            snapshot.last_error = Some(crate::errors::AppError::AccountActivationFailed(error));
            drop(snapshot);
            let _ = super::update_tray_from_state(&app, &state).await;
        } else {
            super::spawn_refresh(app.clone(), state.clone());
        }
    });
}

async fn repair_local_threads(state: &SharedAppState) -> Result<OfficialRepairSummary, AppError> {
    let mut manager = state.session_manager.lock().await;
    manager.rescan_and_repair().await
}

fn switch_success_message(
    language: ResolvedAppLanguage,
    english_prefix: &'static str,
    repair_summary: &OfficialRepairSummary,
    rollout_updates: usize,
) -> String {
    let prefix = localize(
        language,
        LocalizedText::new(english_prefix, "\u{64cd}\u{4f5c}\u{5df2}\u{5b8c}\u{6210}"),
    );
    format!(
        "{prefix}. Rollout: {rollout_updates}. Repair: +{} ~{} index {}",
        repair_summary.created_threads,
        repair_summary.updated_threads,
        repair_summary.updated_session_index_entries
    )
}

fn expected_provider_from_current_config(codex_home: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(codex_home.join("config.toml"))
        .ok()
        .and_then(|config| target_provider_for_config(&config))
}

fn most_common_provider(counts: &[ProviderCount]) -> Option<String> {
    counts.first().map(|count| count.provider_id.clone())
}

fn provider_sync_status(
    language: ResolvedAppLanguage,
    expected_provider: Option<&str>,
    rollout_providers: &[ProviderCount],
    thread_providers: &[ProviderCount],
    thread_issue: Option<&str>,
) -> String {
    let Some(expected) = expected_provider else {
        return localize(
            language,
            LocalizedText::new(
                "No local provider metadata found.",
                "\u{672a}\u{627e}\u{5230}\u{672c}\u{5730} Provider \u{5143}\u{6570}\u{636e}\u{3002}",
            ),
        );
    };
    let rollout_mismatch = rollout_providers
        .iter()
        .any(|count| count.provider_id != expected);
    let thread_mismatch = thread_providers
        .iter()
        .any(|count| count.provider_id != expected);

    if rollout_mismatch || thread_mismatch {
        return localize(
            language,
            LocalizedText::new(
                "Repair recommended: provider metadata does not all match the current config.",
                "\u{5efa}\u{8bae}\u{4fee}\u{590d}\u{ff1a}Provider \u{5143}\u{6570}\u{636e}\u{4e0e}\u{5f53}\u{524d}\u{914d}\u{7f6e}\u{4e0d}\u{5b8c}\u{5168}\u{4e00}\u{81f4}\u{3002}",
            ),
        );
    }

    if thread_issue.is_some() {
        return localize(
            language,
            LocalizedText::new(
                "Rollout metadata matches. Official thread counts are unavailable.",
                "Rollout \u{5143}\u{6570}\u{636e}\u{4e00}\u{81f4}\u{3002}\u{6682}\u{65f6}\u{65e0}\u{6cd5}\u{8bfb}\u{53d6}\u{5b98}\u{65b9} thread \u{8ba1}\u{6570}\u{3002}",
            ),
        );
    }

    localize(
        language,
        LocalizedText::new(
            "Provider metadata is aligned.",
            "Provider \u{5143}\u{6570}\u{636e}\u{5df2}\u{5bf9}\u{9f50}\u{3002}",
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account_models::AddApiAccountInput;
    use crate::account_vault::AccountVault;

    #[test]
    fn builds_empty_accounts_presentation() {
        let temp = tempfile::tempdir().unwrap();
        let vault = AccountVault::new(temp.path().join("Accounts"));

        let presentation =
            build_accounts_presentation(&vault, ResolvedAppLanguage::English, None).unwrap();

        assert_eq!(presentation.rows.len(), 0);
        assert_eq!(presentation.labels.accounts, "Accounts");
        assert_eq!(presentation.labels.no_saved_accounts, "No saved accounts");
    }

    #[test]
    fn maps_api_record_to_available_row() {
        let temp = tempfile::tempdir().unwrap();
        let vault = AccountVault::new(temp.path().join("Accounts"));
        vault
            .add_api_account(AddApiAccountInput {
                display_name: "Work API".to_string(),
                api_key: "sk-test".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                model: None,
                provider_name: None,
            })
            .unwrap();

        let presentation = build_accounts_presentation(
            &vault,
            ResolvedAppLanguage::English,
            Some("Saved".to_string()),
        )
        .unwrap();

        assert_eq!(presentation.rows[0].display_name, "Work API");
        assert_eq!(presentation.rows[0].kind, AccountKind::Api);
        assert_eq!(presentation.rows[0].state, AccountRowState::Available);
        assert_eq!(presentation.message.as_deref(), Some("Saved"));
    }
}
