use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use crate::errors::AppError;

#[derive(Debug, Clone)]
pub struct SessionManagerPaths {
    pub node_exe: PathBuf,
    pub server_entry: PathBuf,
    pub app_dir: PathBuf,
    pub codex_home: PathBuf,
    pub manager_home: PathBuf,
}

pub struct SessionManager {
    paths: SessionManagerPaths,
    owned_child: Option<tokio::process::Child>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialRepairSummary {
    pub created_threads: u32,
    pub updated_threads: u32,
    pub updated_session_index_entries: u32,
    pub removed_broken_threads: u32,
    pub hidden_snapshot_only_sessions: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionManagerProviderCount {
    pub provider_id: String,
    pub count: usize,
}

#[derive(Debug, serde::Deserialize)]
struct RepairEnvelope {
    stats: OfficialRepairSummary,
}

#[derive(Debug, serde::Deserialize)]
struct ProviderCountsEnvelope {
    thread_providers: Vec<SessionManagerProviderCount>,
}

impl SessionManager {
    pub fn new(paths: SessionManagerPaths) -> Self {
        Self {
            paths,
            owned_child: None,
        }
    }

    pub async fn is_healthy(&self) -> bool {
        reqwest::get("http://127.0.0.1:4318/api/health")
            .await
            .map(|response| response.status().is_success())
            .unwrap_or(false)
    }

    pub async fn ensure_running(&mut self) -> Result<bool, AppError> {
        if self.is_healthy().await {
            return Ok(false);
        }

        self.start_owned_process()?;
        self.wait_until_healthy(Duration::from_secs(10)).await?;
        Ok(true)
    }

    fn start_owned_process(&mut self) -> Result<(), AppError> {
        if !self.paths.node_exe.exists() {
            return Err(AppError::NodeRuntimeMissing);
        }
        if !self.paths.server_entry.exists() {
            return Err(AppError::SessionManagerFilesIncomplete);
        }

        let mut command = tokio::process::Command::new(&self.paths.node_exe);
        command.arg(&self.paths.server_entry);
        command.current_dir(&self.paths.app_dir);
        command.env("PORT", "4318");
        command.env("CODEX_HOME", &self.paths.codex_home);
        command.env("CODEX_MANAGER_HOME", &self.paths.manager_home);
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        configure_hidden_process_window(&mut command);

        let child = command
            .spawn()
            .map_err(|error| classify_startup_diagnostics(&error.to_string()))?;

        self.owned_child = Some(child);
        Ok(())
    }

    async fn wait_until_healthy(&self, timeout_duration: Duration) -> Result<(), AppError> {
        let start = Instant::now();
        while start.elapsed() < timeout_duration {
            if self.is_healthy().await {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        Err(AppError::SessionManagerStartFailed(
            "Timed out while waiting for the session manager to start.".to_string(),
        ))
    }

    pub async fn open_in_browser(&mut self) -> Result<bool, AppError> {
        let started = self.ensure_running().await?;
        open::that("http://127.0.0.1:4318")
            .map_err(|error| AppError::SessionManagerStartFailed(error.to_string()))?;
        Ok(started)
    }

    pub async fn rescan_and_repair(&mut self) -> Result<OfficialRepairSummary, AppError> {
        self.ensure_running().await?;
        let _: serde_json::Value =
            post_json_empty("http://127.0.0.1:4318/api/sessions/rescan").await?;
        let envelope: RepairEnvelope =
            post_json_empty("http://127.0.0.1:4318/api/codex/repair").await?;
        Ok(envelope.stats)
    }

    pub async fn provider_counts(&mut self) -> Result<Vec<SessionManagerProviderCount>, AppError> {
        self.ensure_running().await?;
        let envelope: ProviderCountsEnvelope =
            get_json("http://127.0.0.1:4318/api/codex/provider-counts").await?;
        Ok(envelope.thread_providers)
    }

    pub async fn stop_owned_process(&mut self) {
        if let Some(child) = self.owned_child.as_mut() {
            let _ = child.kill().await;
        }
        self.owned_child = None;
    }
}

async fn post_json_empty<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, AppError> {
    let response = reqwest::Client::new()
        .post(url)
        .json(&serde_json::json!({}))
        .send()
        .await
        .map_err(|error| AppError::RepairFailed(error.to_string()))?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|error| AppError::RepairFailed(error.to_string()))?;
    if !status.is_success() {
        return Err(AppError::RepairFailed(
            String::from_utf8_lossy(&body).trim().to_string(),
        ));
    }
    serde_json::from_slice(&body).map_err(|error| AppError::RepairFailed(error.to_string()))
}

async fn get_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, AppError> {
    let response = reqwest::get(url)
        .await
        .map_err(|error| AppError::RepairFailed(error.to_string()))?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|error| AppError::RepairFailed(error.to_string()))?;
    if !status.is_success() {
        return Err(AppError::RepairFailed(
            String::from_utf8_lossy(&body).trim().to_string(),
        ));
    }
    serde_json::from_slice(&body).map_err(|error| AppError::RepairFailed(error.to_string()))
}

pub fn classify_startup_diagnostics(text: &str) -> AppError {
    let lowered = text.to_ascii_lowercase();
    if lowered.contains("eaddrinuse") || lowered.contains("address already in use") {
        return AppError::SessionManagerPortInUse;
    }
    if lowered.contains("cannot find module") || lowered.contains("module not found") {
        return AppError::SessionManagerFilesIncomplete;
    }
    AppError::SessionManagerStartFailed(tail_diagnostics(text, 1200))
}

fn tail_diagnostics(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let start = chars.len().saturating_sub(max_chars);
    chars[start..].iter().collect()
}

#[cfg(windows)]
fn configure_hidden_process_window(command: &mut tokio::process::Command) {
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn configure_hidden_process_window(_command: &mut tokio::process::Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_port_conflict() {
        let error = classify_startup_diagnostics(
            "Error: listen EADDRINUSE: address already in use 127.0.0.1:4318",
        );
        assert_eq!(error, AppError::SessionManagerPortInUse);
    }

    #[test]
    fn classifies_missing_module_as_incomplete_bundle() {
        let error =
            classify_startup_diagnostics("Error: Cannot find module './dist/server/index.js'");
        assert_eq!(error, AppError::SessionManagerFilesIncomplete);
    }

    #[test]
    fn preserves_tail_of_unknown_startup_diagnostics() {
        let diagnostics = format!("{}tail", "x".repeat(1300));

        let error = classify_startup_diagnostics(&diagnostics);

        assert_eq!(
            error,
            AppError::SessionManagerStartFailed(format!("{}tail", "x".repeat(1196)))
        );
    }
}
