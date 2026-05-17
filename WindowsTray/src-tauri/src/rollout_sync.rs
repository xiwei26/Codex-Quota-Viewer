use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::errors::AppError;
use crate::restore_points::replace_file;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RolloutProviderSyncResult {
    pub updated_files: Vec<PathBuf>,
}

pub fn rollout_roots(codex_home: &Path) -> [PathBuf; 2] {
    [
        codex_home.join("sessions"),
        codex_home.join("archived_sessions"),
    ]
}

pub fn planned_rollout_updates(
    codex_home: &Path,
    target_provider: &str,
) -> Result<Vec<PathBuf>, AppError> {
    let mut updates = Vec::new();
    for file in rollout_files(&rollout_roots(codex_home))? {
        if updated_content_if_needed(&file, target_provider)?.is_some() {
            updates.push(file);
        }
    }
    updates.sort();
    Ok(updates)
}

pub fn sync_rollout_providers(
    codex_home: &Path,
    target_provider: &str,
) -> Result<RolloutProviderSyncResult, AppError> {
    let mut updated_files = Vec::new();
    for file in rollout_files(&rollout_roots(codex_home))? {
        let Some(updated_content) = updated_content_if_needed(&file, target_provider)? else {
            continue;
        };
        replace_file(&file, updated_content.as_bytes())?;
        updated_files.push(file);
    }
    updated_files.sort();
    Ok(RolloutProviderSyncResult { updated_files })
}

pub fn target_provider_for_config(config: &str) -> Option<String> {
    config.lines().find_map(|line| {
        let trimmed = line.trim();
        let value = trimmed.strip_prefix("model_provider")?.trim_start();
        let value = value.strip_prefix('=')?.trim_start();
        parse_toml_string(value)
    })
}

fn rollout_files(roots: &[PathBuf]) -> Result<Vec<PathBuf>, AppError> {
    let mut files = Vec::new();
    for root in roots {
        collect_rollout_files(root, &mut files)?;
    }
    files.sort();
    Ok(files)
}

fn collect_rollout_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), AppError> {
    if !path.exists() {
        return Ok(());
    }
    let metadata = fs::metadata(path)
        .map_err(|error| rollout_error(format!("read metadata {}: {error}", path.display())))?;
    if metadata.is_file() {
        if path.extension().and_then(|value| value.to_str()) == Some("jsonl") {
            files.push(path.to_path_buf());
        }
        return Ok(());
    }
    if !metadata.is_dir() {
        return Ok(());
    }

    let entries = fs::read_dir(path)
        .map_err(|error| rollout_error(format!("read directory {}: {error}", path.display())))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| rollout_error(format!("read directory entry: {error}")))?;
        collect_rollout_files(&entry.path(), files)?;
    }
    Ok(())
}

fn updated_content_if_needed(
    file: &Path,
    target_provider: &str,
) -> Result<Option<String>, AppError> {
    let content = fs::read_to_string(file)
        .map_err(|error| rollout_error(format!("read rollout file {}: {error}", file.display())))?;
    let Some((first_line, rest)) = split_first_line(&content) else {
        return Ok(None);
    };
    if first_line.trim().is_empty() {
        return Ok(None);
    }

    let mut object: Value = match serde_json::from_str(first_line) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    if object.get("type").and_then(Value::as_str) != Some("session_meta") {
        return Ok(None);
    }
    let Some(payload) = object.get_mut("payload").and_then(Value::as_object_mut) else {
        return Ok(None);
    };
    if payload
        .get("model_provider")
        .and_then(Value::as_str)
        .unwrap_or_default()
        == target_provider
    {
        return Ok(None);
    }

    payload.insert(
        "model_provider".to_string(),
        Value::String(target_provider.to_string()),
    );
    let first_line = serde_json::to_string(&object)
        .map_err(|error| rollout_error(format!("serialize rollout metadata: {error}")))?;
    Ok(Some(format!("{first_line}{rest}")))
}

fn split_first_line(content: &str) -> Option<(&str, &str)> {
    if content.is_empty() {
        return None;
    }
    match content.find('\n') {
        Some(index) => {
            let mut first = &content[..index];
            if let Some(stripped) = first.strip_suffix('\r') {
                first = stripped;
            }
            Some((first, &content[index..]))
        }
        None => Some((content.strip_suffix('\r').unwrap_or(content), "")),
    }
}

fn parse_toml_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let quoted = trimmed.strip_prefix('"')?;
    let end = quoted.find('"')?;
    let parsed = quoted[..end].trim();
    if parsed.is_empty() {
        None
    } else {
        Some(parsed.to_string())
    }
}

fn rollout_error(message: impl Into<String>) -> AppError {
    AppError::AccountActivationFailed(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_and_syncs_session_meta_provider() {
        let temp = tempfile::tempdir().unwrap();
        let session_dir = temp.path().join("sessions").join("2026").join("05");
        fs::create_dir_all(&session_dir).unwrap();
        let file = session_dir.join("thread.jsonl");
        fs::write(
            &file,
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"abc\",\"model_provider\":\"old\"}}\n{\"type\":\"response\"}\n",
        )
        .unwrap();

        assert_eq!(
            planned_rollout_updates(temp.path(), "openai").unwrap(),
            vec![file.clone()]
        );
        let result = sync_rollout_providers(temp.path(), "openai").unwrap();

        assert_eq!(result.updated_files, vec![file.clone()]);
        let updated = fs::read_to_string(file).unwrap();
        assert!(updated
            .lines()
            .next()
            .unwrap()
            .contains("\"model_provider\":\"openai\""));
        assert!(updated.contains("{\"type\":\"response\"}"));
    }

    #[test]
    fn leaves_invalid_or_non_meta_files_untouched() {
        let temp = tempfile::tempdir().unwrap();
        let session_dir = temp.path().join("sessions");
        fs::create_dir_all(&session_dir).unwrap();
        let invalid = session_dir.join("invalid.jsonl");
        let response = session_dir.join("response.jsonl");
        fs::write(&invalid, "{not json}\n").unwrap();
        fs::write(&response, "{\"type\":\"response\"}\n").unwrap();

        let result = sync_rollout_providers(temp.path(), "openai").unwrap();

        assert!(result.updated_files.is_empty());
        assert_eq!(fs::read_to_string(invalid).unwrap(), "{not json}\n");
        assert_eq!(
            fs::read_to_string(response).unwrap(),
            "{\"type\":\"response\"}\n"
        );
    }

    #[test]
    fn parses_model_provider_from_config() {
        assert_eq!(
            target_provider_for_config("model = \"gpt\"\nmodel_provider = \"OpenAI\"\n").as_deref(),
            Some("OpenAI")
        );
        assert_eq!(target_provider_for_config("model_provider = \"\"\n"), None);
    }
}
