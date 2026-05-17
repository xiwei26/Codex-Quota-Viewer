use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::errors::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexDesktopSession {
    exe_path: Option<PathBuf>,
}

impl CodexDesktopSession {
    pub fn was_running(&self) -> bool {
        self.exe_path.is_some()
    }
}

#[cfg(test)]
pub fn close_if_running() -> Result<CodexDesktopSession, AppError> {
    Ok(CodexDesktopSession { exe_path: None })
}

#[cfg(not(test))]
pub fn close_if_running() -> Result<CodexDesktopSession, AppError> {
    let processes = running_desktop_processes()?;
    let exe_path = processes.first().map(|process| process.exe_path.clone());
    for process in processes {
        let output = Command::new("taskkill")
            .args(["/PID", &process.pid.to_string(), "/T"])
            .output()
            .map_err(|error| desktop_error(format!("start taskkill: {error}")))?;
        if !output.status.success() {
            return Err(desktop_error(format!(
                "taskkill failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
    }
    wait_until_closed(Duration::from_secs(8))?;
    Ok(CodexDesktopSession { exe_path })
}

#[cfg(test)]
pub fn reopen_if_needed(_session: &CodexDesktopSession) -> Result<(), AppError> {
    Ok(())
}

#[cfg(not(test))]
pub fn reopen_if_needed(session: &CodexDesktopSession) -> Result<(), AppError> {
    let Some(path) = &session.exe_path else {
        return Ok(());
    };
    Command::new(path)
        .spawn()
        .map_err(|error| desktop_error(format!("reopen {}: {error}", path.display())))?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DesktopProcess {
    pid: u32,
    exe_path: PathBuf,
}

fn running_desktop_processes() -> Result<Vec<DesktopProcess>, AppError> {
    let script = r#"
Get-Process -Name Codex -ErrorAction SilentlyContinue |
  Where-Object { $_.Path -and ($_.Path -notmatch '\\bin\\codex\.exe$') } |
  ForEach-Object { "$($_.Id)|$($_.Path)" }
"#;
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .output()
        .map_err(|error| desktop_error(format!("query Codex process: {error}")))?;
    if !output.status.success() {
        return Err(desktop_error(format!(
            "query Codex process failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    parse_process_lines(&String::from_utf8_lossy(&output.stdout))
}

fn parse_process_lines(text: &str) -> Result<Vec<DesktopProcess>, AppError> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (pid, path) = line
                .split_once('|')
                .ok_or_else(|| desktop_error(format!("invalid process line: {line}")))?;
            let pid = pid
                .trim()
                .parse::<u32>()
                .map_err(|error| desktop_error(format!("invalid process id {pid}: {error}")))?;
            Ok(DesktopProcess {
                pid,
                exe_path: PathBuf::from(path.trim()),
            })
        })
        .collect()
}

fn wait_until_closed(timeout: Duration) -> Result<(), AppError> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if running_desktop_processes()?.is_empty() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(desktop_error("timed out while waiting for Codex to close"))
}

fn desktop_error(message: impl Into<String>) -> AppError {
    AppError::CodexDesktopControlFailed(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_process_lines() {
        let processes =
            parse_process_lines("123|C:\\Users\\Ada\\AppData\\Local\\OpenAI\\Codex\\Codex.exe\n")
                .unwrap();

        assert_eq!(processes.len(), 1);
        assert_eq!(processes[0].pid, 123);
        assert!(processes[0].exe_path.ends_with("Codex.exe"));
    }

    #[test]
    fn rejects_invalid_process_lines() {
        assert!(parse_process_lines("not-valid\n").is_err());
    }
}
