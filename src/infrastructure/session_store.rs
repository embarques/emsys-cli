use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

const SESSION_FILE_ENV: &str = "EMSYS_SESSION_FILE";
const DEFAULT_SESSION_FILE: &str = ".emsys/session.json";

#[derive(Debug, Clone)]
pub struct SessionFileStore {
    path: PathBuf,
}

#[derive(Debug, Error)]
pub enum SessionStoreError {
    #[error("session file error: {0}")]
    Io(#[from] io::Error),

    #[error("session file contains invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Deserialize, Serialize)]
struct SessionFile {
    refresh_token: String,
}

impl SessionFileStore {
    pub fn new() -> Self {
        Self {
            path: session_file_path(),
        }
    }

    pub fn save_refresh_token(&self, refresh_token: &str) -> Result<(), SessionStoreError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = SessionFile {
            refresh_token: refresh_token.to_string(),
        };
        let content = serde_json::to_string_pretty(&file)?;
        fs::write(&self.path, content)?;
        set_private_permissions(&self.path)?;

        Ok(())
    }

    pub fn load_refresh_token(&self) -> Result<Option<String>, SessionStoreError> {
        let content = match fs::read_to_string(&self.path) {
            Ok(content) => content,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };

        let file: SessionFile = serde_json::from_str(&content)?;
        Ok(Some(file.refresh_token))
    }

    pub fn clear_refresh_token(&self) -> Result<bool, SessionStoreError> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Default for SessionFileStore {
    fn default() -> Self {
        Self::new()
    }
}

fn session_file_path() -> PathBuf {
    env::var_os(SESSION_FILE_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_SESSION_FILE))
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    impl SessionFileStore {
        fn with_path(path: PathBuf) -> Self {
            Self { path }
        }
    }

    #[test]
    fn missing_session_file_returns_none() {
        let store = SessionFileStore::with_path(temp_path("missing-session.json"));

        assert_eq!(store.load_refresh_token().expect("load should pass"), None);
    }

    #[test]
    fn saves_loads_and_clears_refresh_token() {
        let path = temp_path("session.json");
        let store = SessionFileStore::with_path(path);

        store
            .save_refresh_token("refresh-token")
            .expect("save should pass");

        assert_eq!(
            store.load_refresh_token().expect("load should pass"),
            Some("refresh-token".into())
        );
        assert!(store.clear_refresh_token().expect("clear should pass"));
        assert_eq!(store.load_refresh_token().expect("load should pass"), None);
    }

    fn temp_path(name: &str) -> PathBuf {
        let mut path = env::temp_dir();
        path.push(format!(
            "emsys-cli-session-store-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);
        path
    }
}
