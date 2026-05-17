use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::AppError;

const MANIFEST_FILE: &str = "manifest.json";
const FILES_DIR: &str = "files";
const MAX_RESTORE_POINTS: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePointFileRecord {
    pub original_path: PathBuf,
    pub backup_relative_path: Option<PathBuf>,
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePointManifest {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub reason: String,
    pub summary: String,
    pub files: Vec<RestorePointFileRecord>,
}

#[derive(Debug, Clone)]
pub struct RestorePointManager {
    root: PathBuf,
}

impl RestorePointManager {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn create_restore_point(
        &self,
        reason: &str,
        summary: &str,
        files: &[PathBuf],
    ) -> Result<RestorePointManifest, AppError> {
        fs::create_dir_all(&self.root).map_err(|error| {
            restore_error(format!(
                "create restore point root {}: {error}",
                self.root.display()
            ))
        })?;

        let created_at = Utc::now();
        let id = format!("{}-{}", created_at.format("%Y%m%d-%H%M%S-%3f"), short_id());
        let restore_dir = self.root.join(&id);
        let files_dir = restore_dir.join(FILES_DIR);
        fs::create_dir_all(&files_dir).map_err(|error| {
            restore_error(format!(
                "create restore point directory {}: {error}",
                files_dir.display()
            ))
        })?;

        let mut records = Vec::new();
        for (index, path) in deduplicated_paths(files).into_iter().enumerate() {
            if path.exists() {
                let backup_name = format!("{index:03}-{}", sanitized_name(&path));
                let relative = PathBuf::from(FILES_DIR).join(backup_name);
                let backup_path = restore_dir.join(&relative);
                fs::copy(&path, &backup_path).map_err(|error| {
                    restore_error(format!(
                        "backup {} to {}: {error}",
                        path.display(),
                        backup_path.display()
                    ))
                })?;
                records.push(RestorePointFileRecord {
                    original_path: path,
                    backup_relative_path: Some(relative),
                    exists: true,
                });
            } else {
                records.push(RestorePointFileRecord {
                    original_path: path,
                    backup_relative_path: None,
                    exists: false,
                });
            }
        }

        let manifest = RestorePointManifest {
            id,
            created_at,
            reason: reason.to_string(),
            summary: summary.to_string(),
            files: records,
        };
        self.write_manifest(&restore_dir, &manifest)?;
        self.prune_if_needed(&manifest.id)?;
        Ok(manifest)
    }

    pub fn latest_restore_point(&self) -> Result<Option<RestorePointManifest>, AppError> {
        let Some(dir) = self.latest_restore_point_dir()? else {
            return Ok(None);
        };
        self.read_manifest(&dir).map(Some)
    }

    pub fn restore_latest(&self) -> Result<RestorePointManifest, AppError> {
        let dir = self
            .latest_restore_point_dir()?
            .ok_or(AppError::RestorePointUnavailable)?;
        let manifest = self.read_manifest(&dir)?;
        self.restore_manifest(&dir, &manifest)?;
        Ok(manifest)
    }

    pub fn restore_manifest(
        &self,
        restore_dir: &Path,
        manifest: &RestorePointManifest,
    ) -> Result<(), AppError> {
        for file in &manifest.files {
            if file.exists {
                let relative = file.backup_relative_path.as_ref().ok_or_else(|| {
                    restore_error(format!(
                        "restore point {} is missing backup path for {}",
                        manifest.id,
                        file.original_path.display()
                    ))
                })?;
                let backup = restore_dir.join(relative);
                if let Some(parent) = file.original_path.parent() {
                    fs::create_dir_all(parent).map_err(|error| {
                        restore_error(format!("create directory {}: {error}", parent.display()))
                    })?;
                }
                replace_file(
                    &file.original_path,
                    &fs::read(&backup).map_err(|error| {
                        restore_error(format!("read backup {}: {error}", backup.display()))
                    })?,
                )?;
            } else {
                match fs::remove_file(&file.original_path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        return Err(restore_error(format!(
                            "remove {}: {error}",
                            file.original_path.display()
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    fn latest_restore_point_dir(&self) -> Result<Option<PathBuf>, AppError> {
        if !self.root.exists() {
            return Ok(None);
        }
        let mut dirs = restore_point_dirs(&self.root)?;
        dirs.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
        Ok(dirs.into_iter().next())
    }

    fn read_manifest(&self, restore_dir: &Path) -> Result<RestorePointManifest, AppError> {
        let path = restore_dir.join(MANIFEST_FILE);
        let data = fs::read(&path)
            .map_err(|error| restore_error(format!("read manifest {}: {error}", path.display())))?;
        serde_json::from_slice(&data)
            .map_err(|error| restore_error(format!("decode manifest {}: {error}", path.display())))
    }

    fn write_manifest(
        &self,
        restore_dir: &Path,
        manifest: &RestorePointManifest,
    ) -> Result<(), AppError> {
        let data = serde_json::to_vec_pretty(manifest)
            .map_err(|error| restore_error(format!("serialize manifest: {error}")))?;
        replace_file(&restore_dir.join(MANIFEST_FILE), &data)
    }

    fn prune_if_needed(&self, protected_id: &str) -> Result<(), AppError> {
        let mut dirs = restore_point_dirs(&self.root)?;
        dirs.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
        for dir in dirs.into_iter().skip(MAX_RESTORE_POINTS) {
            if dir.file_name().and_then(|name| name.to_str()) == Some(protected_id) {
                continue;
            }
            let _ = fs::remove_dir_all(dir);
        }
        Ok(())
    }
}

pub fn replace_file(path: &Path, data: &[u8]) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| restore_error(format!("{} has no parent directory", path.display())))?;
    fs::create_dir_all(parent).map_err(|error| {
        restore_error(format!("create directory {}: {error}", parent.display()))
    })?;
    let temp = unique_sidecar_path(path, "tmp");
    fs::write(&temp, data)
        .map_err(|error| restore_error(format!("write temp file {}: {error}", temp.display())))?;
    fs::rename(&temp, path).map_err(|error| {
        let _ = fs::remove_file(&temp);
        restore_error(format!("replace file {}: {error}", path.display()))
    })
}

fn restore_point_dirs(root: &Path) -> Result<Vec<PathBuf>, AppError> {
    let entries = fs::read_dir(root)
        .map_err(|error| restore_error(format!("read restore root {}: {error}", root.display())))?;
    entries
        .map(|entry| {
            let entry =
                entry.map_err(|error| restore_error(format!("read restore entry: {error}")))?;
            let file_type = entry.file_type().map_err(|error| {
                restore_error(format!(
                    "read restore entry type {}: {error}",
                    entry.path().display()
                ))
            })?;
            Ok((entry.path(), file_type.is_dir()))
        })
        .filter_map(|result: Result<(PathBuf, bool), AppError>| match result {
            Ok((path, true)) => Some(Ok(path)),
            Ok((_path, false)) => None,
            Err(error) => Some(Err(error)),
        })
        .collect()
}

fn deduplicated_paths(files: &[PathBuf]) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for file in files {
        let key = file.to_string_lossy().to_ascii_lowercase();
        if seen.insert(key) {
            result.push(file.clone());
        }
    }
    result
}

fn sanitized_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file")
        .chars()
        .map(|character| match character {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => character,
        })
        .collect()
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

fn short_id() -> String {
    let nanos = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    format!("{:08x}", (nanos as u64) & 0xffff_ffff)
}

fn restore_error(message: impl Into<String>) -> AppError {
    AppError::RestorePointFailed(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_restore_point_for_existing_and_missing_files() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("auth.json");
        let missing = temp.path().join("config.toml");
        fs::write(&target, b"before").unwrap();
        let manager = RestorePointManager::new(temp.path().join("backups"));

        let manifest = manager
            .create_restore_point("safe-switch", "Switch", &[target.clone(), missing.clone()])
            .unwrap();

        assert_eq!(manifest.files.len(), 2);
        assert!(manifest.files[0].exists);
        assert!(!manifest.files[1].exists);
        assert!(manager.latest_restore_point().unwrap().is_some());
    }

    #[test]
    fn restores_latest_restore_point() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("auth.json");
        let missing = temp.path().join("config.toml");
        fs::write(&target, b"before").unwrap();
        fs::write(&missing, b"temporary").unwrap();
        let manager = RestorePointManager::new(temp.path().join("backups"));
        manager
            .create_restore_point("safe-switch", "Switch", &[target.clone(), missing.clone()])
            .unwrap();
        fs::write(&target, b"after").unwrap();
        fs::remove_file(&missing).unwrap();

        manager.restore_latest().unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "before");
        assert_eq!(fs::read_to_string(&missing).unwrap(), "temporary");
    }

    #[test]
    fn restores_missing_file_by_removing_created_file() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("config.toml");
        let manager = RestorePointManager::new(temp.path().join("backups"));
        manager
            .create_restore_point("safe-switch", "Switch", std::slice::from_ref(&missing))
            .unwrap();
        fs::write(&missing, b"created").unwrap();

        manager.restore_latest().unwrap();

        assert!(!missing.exists());
    }
}
