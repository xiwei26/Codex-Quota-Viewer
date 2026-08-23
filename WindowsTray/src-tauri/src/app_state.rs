use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Duration;

use tauri::async_runtime::Mutex;

use crate::errors::AppError;
use crate::quota::QuotaSnapshot;
use crate::scheduler::RefreshScheduler;
use crate::session_manager::SessionManager;
use crate::settings::AppSettings;

#[derive(Debug, Clone)]
pub struct TraySnapshot {
    pub quota: Option<QuotaSnapshot>,
    pub quota_owner_account_id: Option<String>,
    pub is_refreshing: bool,
    pub last_error: Option<AppError>,
}

impl TraySnapshot {
    pub fn loading() -> Self {
        Self {
            quota: None,
            quota_owner_account_id: None,
            is_refreshing: true,
            last_error: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct RefreshGate {
    in_progress: bool,
    pending: bool,
    requested_revision: u64,
    active_revision: u64,
    completed_revision: u64,
}

impl RefreshGate {
    pub fn request(&mut self) -> bool {
        self.requested_revision = self.requested_revision.saturating_add(1);
        if self.in_progress {
            self.pending = true;
            false
        } else {
            self.in_progress = true;
            self.active_revision = self.requested_revision;
            true
        }
    }

    pub fn complete_cycle(&mut self) -> bool {
        self.completed_revision = self.active_revision;
        if self.pending {
            self.pending = false;
            self.active_revision = self.requested_revision;
            true
        } else {
            self.in_progress = false;
            false
        }
    }

    pub fn revisions(&self) -> (u64, u64) {
        (self.requested_revision, self.completed_revision)
    }
}

pub struct AppState {
    pub codex_home: PathBuf,
    pub settings_path: PathBuf,
    pub accounts_dir: PathBuf,
    pub provider_mode_dir: PathBuf,
    pub switch_backups_dir: PathBuf,
    pub settings: Mutex<AppSettings>,
    pub settings_load_issue: Mutex<Option<String>>,
    pub tray_snapshot: Mutex<TraySnapshot>,
    pub session_manager: Mutex<SessionManager>,
    pub refresh_scheduler: Mutex<RefreshScheduler>,
    pub refresh_gate: std::sync::Mutex<RefreshGate>,
    pub dashboard_revision: AtomicU64,
    pub quota_timeout: Duration,
}

pub type SharedAppState = Arc<AppState>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalesces_pending_requests_without_losing_the_latest_revision() {
        let mut gate = RefreshGate::default();

        assert!(gate.request());
        assert!(!gate.request());
        assert!(!gate.request());
        assert_eq!(gate.revisions(), (3, 0));

        assert!(gate.complete_cycle());
        assert_eq!(gate.revisions(), (3, 1));
        assert!(!gate.complete_cycle());
        assert_eq!(gate.revisions(), (3, 3));
    }
}
