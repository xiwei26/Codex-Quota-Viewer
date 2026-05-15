use std::fs::{self, File};
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::account_models::{
    AccountId, AccountPayload, AccountValidationError, AddApiAccountInput, VaultAccountRecord,
};
use crate::errors::AppError;

const INDEX_VERSION: u32 = 1;
const INDEX_FILE: &str = "index.json";
const RECORDS_DIR: &str = "records";

#[derive(Debug, Clone)]
pub struct AccountVault {
    root: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VaultListResult {
    pub records: Vec<VaultAccountRecord>,
    pub issue: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VaultIndex {
    version: u32,
    preferred_account_id: Option<String>,
    account_ids: Vec<String>,
}

impl Default for VaultIndex {
    fn default() -> Self {
        Self {
            version: INDEX_VERSION,
            preferred_account_id: None,
            account_ids: Vec::new(),
        }
    }
}

impl AccountVault {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn list_accounts(&self) -> Result<VaultListResult, AppError> {
        let mut issue = None;
        let account_ids = match self.read_index() {
            Ok(index) => index.account_ids,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => {
                issue = Some(format!(
                    "Account vault index {} could not be read: {error}",
                    self.index_path().display()
                ));
                return self.list_record_files(issue);
            }
        };

        let mut records = Vec::new();
        for account_id in account_ids {
            match self.read_record(&account_id) {
                Ok(record) => records.push(record),
                Err(error) => {
                    let message = format!(
                        "Account record {} could not be read: {error}",
                        self.record_path(&account_id).display()
                    );
                    append_issue(&mut issue, message);
                }
            }
        }

        Ok(VaultListResult { records, issue })
    }

    pub fn add_api_account(
        &self,
        input: AddApiAccountInput,
    ) -> Result<VaultAccountRecord, AppError> {
        let validated = input
            .validate()
            .map_err(|error| AppError::AccountValidationFailed(validation_message(error)))?;
        let mut index = self.load_index_for_write()?;
        let now = Utc::now();
        let id = self.next_account_id(&index);
        let record =
            VaultAccountRecord::new_api(id.clone(), validated.display_name, validated.payload, now);

        self.write_record(&record)?;
        index.account_ids.push(id.as_str().to_string());
        self.write_index(&index)?;

        Ok(record)
    }

    pub fn import_current_chatgpt_account(
        &self,
        codex_home: &Path,
        display_name: Option<String>,
    ) -> Result<VaultAccountRecord, AppError> {
        let auth_json = fs::read(codex_home.join("auth.json"))
            .map_err(|_| AppError::SignInRequired)
            .and_then(|data| {
                serde_json::from_slice::<serde_json::Value>(&data)
                    .map_err(|_| AppError::SignInRequired)
            })?;
        let display_name = display_name
            .and_then(|value| non_empty_trimmed(&value))
            .or_else(|| auth_email(&auth_json))
            .unwrap_or_else(|| "Current ChatGPT".to_string());
        let mut index = self.load_index_for_write()?;
        let now = Utc::now();
        let id = self.next_account_id(&index);
        let record = VaultAccountRecord::new_chatgpt(id.clone(), display_name, auth_json, now);

        self.write_record(&record)?;
        index.account_ids.push(id.as_str().to_string());
        self.write_index(&index)?;

        Ok(record)
    }

    pub fn rename_account(
        &self,
        account_id: &str,
        display_name: &str,
    ) -> Result<VaultAccountRecord, AppError> {
        let display_name = non_empty_trimmed(display_name)
            .ok_or_else(|| AppError::AccountValidationFailed("Display name is required".into()))?;
        let index = self.load_index_for_write()?;
        if !index.account_ids.iter().any(|id| id == account_id) {
            return Err(AppError::AccountNotFound(account_id.to_string()));
        }

        let mut record = self.load_record(account_id)?;
        record.metadata.display_name = display_name;
        record.metadata.updated_at = Utc::now();
        self.write_record(&record)?;

        Ok(record)
    }

    pub fn forget_account(&self, account_id: &str) -> Result<(), AppError> {
        let mut index = self.load_index_for_write()?;
        let original_len = index.account_ids.len();
        index.account_ids.retain(|id| id != account_id);
        if index.account_ids.len() == original_len {
            return Err(AppError::AccountNotFound(account_id.to_string()));
        }
        if index.preferred_account_id.as_deref() == Some(account_id) {
            index.preferred_account_id = None;
        }

        let path = self.record_path(account_id);
        let tombstone = match move_to_tombstone(&path) {
            Ok(tombstone) => tombstone,
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(vault_error_with_context(
                    format!("move record {} to tombstone", path.display()),
                    error,
                ));
            }
        };

        if let Err(error) = self.write_index(&index) {
            if let Some(tombstone) = tombstone.as_ref() {
                let _ = fs::rename(tombstone, &path);
            }
            return Err(error);
        }

        if let Some(tombstone) = tombstone {
            fs::remove_file(&tombstone).map_err(|error| {
                vault_error_with_context(format!("delete tombstone {}", tombstone.display()), error)
            })?;
        }
        Ok(())
    }

    pub fn load_record(&self, account_id: &str) -> Result<VaultAccountRecord, AppError> {
        self.read_record(account_id).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                AppError::AccountNotFound(account_id.to_string())
            } else {
                vault_error_with_context(
                    format!("load record {}", self.record_path(account_id).display()),
                    error,
                )
            }
        })
    }

    fn index_path(&self) -> PathBuf {
        self.root.join(INDEX_FILE)
    }

    fn records_dir(&self) -> PathBuf {
        self.root.join(RECORDS_DIR)
    }

    fn record_path(&self, account_id: &str) -> PathBuf {
        self.records_dir().join(format!("{account_id}.json"))
    }

    fn read_index(&self) -> io::Result<VaultIndex> {
        let path = self.index_path();
        let data = fs::read(&path).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("read index {} failed: {error}", path.display()),
            )
        })?;
        serde_json::from_slice(&data).map_err(invalid_data)
    }

    fn load_index_for_write(&self) -> Result<VaultIndex, AppError> {
        match self.read_index() {
            Ok(index) => Ok(index),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(VaultIndex::default()),
            Err(error) => Err(vault_error_with_context(
                format!("read index {}", self.index_path().display()),
                error,
            )),
        }
    }

    fn write_index(&self, index: &VaultIndex) -> Result<(), AppError> {
        let path = self.index_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                vault_error_with_context(format!("create directory {}", parent.display()), error)
            })?;
        }
        let data = serde_json::to_vec_pretty(index)
            .map_err(|error| vault_error_with_context("serialize account index", error))?;
        replace_file_safely(&path, &data).map_err(|error| {
            vault_error_with_context(format!("write index {}", path.display()), error)
        })
    }

    fn read_record(&self, account_id: &str) -> io::Result<VaultAccountRecord> {
        let path = self.record_path(account_id);
        let data = fs::read(&path).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("read record {} failed: {error}", path.display()),
            )
        })?;
        serde_json::from_slice(&data).map_err(invalid_data)
    }

    fn write_record(&self, record: &VaultAccountRecord) -> Result<(), AppError> {
        let path = self.record_path(record.id.as_str());
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                vault_error_with_context(format!("create directory {}", parent.display()), error)
            })?;
        }
        let data = serde_json::to_vec_pretty(record).map_err(|error| {
            vault_error_with_context(format!("serialize record {}", record.id.as_str()), error)
        })?;
        replace_file_safely(&path, &data).map_err(|error| {
            vault_error_with_context(format!("write record {}", path.display()), error)
        })
    }

    fn next_account_id(&self, index: &VaultIndex) -> AccountId {
        let base = format!("acct-{}", Utc::now().timestamp_millis());
        let mut candidate = base.clone();
        let mut suffix = 2;

        while index.account_ids.iter().any(|id| id == &candidate)
            || self.record_path(&candidate).exists()
        {
            candidate = format!("{base}-{suffix}");
            suffix += 1;
        }

        AccountId::new(candidate)
    }

    fn list_record_files(&self, mut issue: Option<String>) -> Result<VaultListResult, AppError> {
        let records_dir = self.records_dir();
        let entries = match fs::read_dir(records_dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(VaultListResult {
                    records: Vec::new(),
                    issue,
                });
            }
            Err(error) => {
                return Err(vault_error_with_context(
                    format!("read records directory {}", self.records_dir().display()),
                    error,
                ));
            }
        };

        let mut paths = entries
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                vault_error_with_context(
                    format!("read records directory {}", self.records_dir().display()),
                    error,
                )
            })?
            .into_iter()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
            .collect::<Vec<_>>();
        paths.sort();

        let mut records = Vec::new();
        for path in paths {
            match fs::read(&path).and_then(|data| {
                serde_json::from_slice::<VaultAccountRecord>(&data).map_err(invalid_data)
            }) {
                Ok(record) => records.push(record),
                Err(error) => {
                    append_issue(
                        &mut issue,
                        format!(
                            "Account record {} could not be read: {error}",
                            path.display()
                        ),
                    );
                }
            }
        }

        Ok(VaultListResult { records, issue })
    }
}

fn validation_message(error: AccountValidationError) -> String {
    match error {
        AccountValidationError::MissingDisplayName => "Display name is required",
        AccountValidationError::MissingApiKey => "API key is required",
        AccountValidationError::MissingBaseUrl => "Base URL is required",
        AccountValidationError::InvalidBaseUrl => "Base URL is invalid",
    }
    .to_string()
}

fn auth_email(auth_json: &serde_json::Value) -> Option<String> {
    auth_json
        .get("account")
        .and_then(|account| account.get("email"))
        .and_then(|email| email.as_str())
        .and_then(non_empty_trimmed)
}

fn non_empty_trimmed(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn append_issue(issue: &mut Option<String>, message: String) {
    match issue {
        Some(existing) => {
            existing.push_str("; ");
            existing.push_str(&message);
        }
        None => *issue = Some(message),
    }
}

fn invalid_data(error: serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

fn vault_error_with_context(context: impl Into<String>, error: impl std::error::Error) -> AppError {
    AppError::AccountVaultFailed(format!("{} failed: {}", context.into(), error))
}

fn replace_file_safely(path: &Path, data: &[u8]) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} has no parent directory", path.display()),
        )
    })?;
    fs::create_dir_all(parent)?;

    let temp_path = unique_sidecar_path(path, "tmp");
    let backup_path = unique_sidecar_path(path, "bak");

    {
        let mut file = File::create(&temp_path)?;
        file.write_all(data)?;
        file.sync_all()?;
    }

    let had_existing = match fs::rename(path, &backup_path) {
        Ok(()) => true,
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => {
            let _ = fs::remove_file(&temp_path);
            return Err(error);
        }
    };

    if let Err(error) = fs::rename(&temp_path, path) {
        if had_existing {
            let _ = fs::rename(&backup_path, path);
        }
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }

    if had_existing {
        let _ = fs::remove_file(&backup_path);
    }

    Ok(())
}

fn move_to_tombstone(path: &Path) -> io::Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }

    let tombstone = unique_sidecar_path(path, "deleted");
    fs::rename(path, &tombstone)?;
    Ok(Some(tombstone))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account_models::AccountKind;
    use serde_json::json;

    fn input(display_name: &str) -> AddApiAccountInput {
        AddApiAccountInput {
            display_name: display_name.to_string(),
            api_key: " sk-test ".to_string(),
            base_url: " https://api.openai.com/v1/ ".to_string(),
            model: Some(" gpt-5 ".to_string()),
            provider_name: Some(" OpenAI ".to_string()),
        }
    }

    #[test]
    fn empty_vault_lists_no_accounts() {
        let temp = tempfile::tempdir().unwrap();
        let vault = AccountVault::new(temp.path().join("accounts"));

        let result = vault.list_accounts().unwrap();

        assert!(result.records.is_empty());
        assert_eq!(result.issue, None);
    }

    #[test]
    fn saves_and_lists_api_account() {
        let temp = tempfile::tempdir().unwrap();
        let vault = AccountVault::new(temp.path().join("accounts"));

        let saved = vault.add_api_account(input(" OpenAI ")).unwrap();
        let result = vault.list_accounts().unwrap();

        assert_eq!(result.records, vec![saved.clone()]);
        assert_eq!(result.issue, None);
        assert_eq!(saved.metadata.display_name, "OpenAI");
        assert_eq!(saved.metadata.kind, AccountKind::Api);
    }

    #[test]
    fn imports_current_chatgpt_auth_json() {
        let temp = tempfile::tempdir().unwrap();
        let codex_home = temp.path().join("codex");
        fs::create_dir_all(&codex_home).unwrap();
        fs::write(
            codex_home.join("auth.json"),
            serde_json::to_vec_pretty(&json!({
                "account": {
                    "email": "person@example.com"
                },
                "tokens": {
                    "accessToken": "secret"
                }
            }))
            .unwrap(),
        )
        .unwrap();
        let vault = AccountVault::new(temp.path().join("accounts"));

        let saved = vault
            .import_current_chatgpt_account(&codex_home, None)
            .unwrap();

        assert_eq!(saved.metadata.display_name, "person@example.com");
        assert_eq!(saved.metadata.kind, AccountKind::ChatGpt);
        match saved.payload {
            AccountPayload::ChatGpt { auth_json } => {
                assert_eq!(auth_json["tokens"]["accessToken"], "secret");
            }
            AccountPayload::Api(_) => panic!("expected ChatGPT payload"),
        }
    }

    #[test]
    fn rename_updates_display_name() {
        let temp = tempfile::tempdir().unwrap();
        let vault = AccountVault::new(temp.path().join("accounts"));
        let saved = vault.add_api_account(input("OpenAI")).unwrap();

        let renamed = vault
            .rename_account(saved.id.as_str(), " Team API ")
            .unwrap();

        assert_eq!(renamed.metadata.display_name, "Team API");
        assert_eq!(renamed.metadata.created_at, saved.metadata.created_at);
        assert!(renamed.metadata.updated_at >= saved.metadata.updated_at);
        assert_eq!(renamed.payload, saved.payload);
    }

    #[test]
    fn forget_removes_record_and_index_entry() {
        let temp = tempfile::tempdir().unwrap();
        let vault = AccountVault::new(temp.path().join("accounts"));
        let saved = vault.add_api_account(input("OpenAI")).unwrap();

        vault.forget_account(saved.id.as_str()).unwrap();

        let result = vault.list_accounts().unwrap();
        assert!(result.records.is_empty());
        assert!(!vault.record_path(saved.id.as_str()).exists());
        let index = vault.read_index().unwrap();
        assert!(index.account_ids.is_empty());
    }

    #[test]
    fn corrupted_index_lists_readable_record_files_with_issue() {
        let temp = tempfile::tempdir().unwrap();
        let vault = AccountVault::new(temp.path().join("accounts"));
        let saved = vault.add_api_account(input("OpenAI")).unwrap();
        fs::write(vault.index_path(), b"{not json").unwrap();

        let result = vault.list_accounts().unwrap();

        assert_eq!(result.records, vec![saved]);
        assert!(result.issue.unwrap().contains("index"));
    }

    #[test]
    fn corrupted_record_is_skipped_with_issue() {
        let temp = tempfile::tempdir().unwrap();
        let vault = AccountVault::new(temp.path().join("accounts"));
        let saved = vault.add_api_account(input("OpenAI")).unwrap();
        fs::write(vault.record_path(saved.id.as_str()), b"{not json").unwrap();

        let result = vault.list_accounts().unwrap();

        assert!(result.records.is_empty());
        assert!(result.issue.unwrap().contains(saved.id.as_str()));
    }
}
