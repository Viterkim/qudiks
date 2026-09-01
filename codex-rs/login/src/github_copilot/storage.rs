//! Credential storage. Own file rather than `auth.json` so upstream auth
//! storage stays untouched. Holds a long-lived token, so 0600.

use super::error::GithubCopilotError;
use super::error::Result;
use super::token::CopilotToken;
use serde::Deserialize;
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

const CREDENTIALS_FILE: &str = "github_copilot_auth.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GithubCopilotCredentials {
    pub github_token: String,
    pub domain: String,
    pub client_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copilot_token: Option<CopilotToken>,
}

pub fn credentials_path(codex_home: &Path) -> PathBuf {
    codex_home.join(CREDENTIALS_FILE)
}

fn storage_error(path: &Path, source: std::io::Error) -> GithubCopilotError {
    GithubCopilotError::Storage {
        path: path.to_path_buf(),
        source,
    }
}

pub fn load(codex_home: &Path) -> Result<Option<GithubCopilotCredentials>> {
    let path = credentials_path(codex_home);
    let contents = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(storage_error(&path, err)),
    };
    serde_json::from_str(&contents)
        .map(Some)
        .map_err(|err| storage_error(&path, err.into()))
}

pub fn save(codex_home: &Path, credentials: &GithubCopilotCredentials) -> Result<()> {
    let path = credentials_path(codex_home);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| storage_error(&path, err))?;
    }

    let json = serde_json::to_string_pretty(credentials)
        .map_err(|err| storage_error(&path, err.into()))?;
    let mut options = OpenOptions::new();
    options.truncate(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let mut file = options
        .open(&path)
        .map_err(|err| storage_error(&path, err))?;
    file.write_all(json.as_bytes())
        .map_err(|err| storage_error(&path, err))?;
    file.flush().map_err(|err| storage_error(&path, err))
}

pub fn delete(codex_home: &Path) -> Result<bool> {
    let path = credentials_path(codex_home);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(false),
        Err(err) => Err(storage_error(&path, err)),
    }
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
