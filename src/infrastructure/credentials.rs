use keyring::{Entry, Error as KeyringError};
use thiserror::Error;

const SERVICE: &str = "emsys-cli";
const REFRESH_TOKEN_ACCOUNT: &str = "firebase-refresh-token";

#[derive(Debug, Clone, Copy, Default)]
pub struct CredentialStore;

#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("secure credential store error: {0}")]
    Store(#[from] KeyringError),
}

impl CredentialStore {
    pub fn new() -> Self {
        Self
    }

    pub fn save_refresh_token(&self, refresh_token: &str) -> Result<(), CredentialError> {
        entry()?.set_password(refresh_token)?;
        Ok(())
    }

    pub fn load_refresh_token(&self) -> Result<Option<String>, CredentialError> {
        match entry()?.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn clear_refresh_token(&self) -> Result<bool, CredentialError> {
        match entry()?.delete_credential() {
            Ok(()) => Ok(true),
            Err(KeyringError::NoEntry) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
}

fn entry() -> Result<Entry, KeyringError> {
    Entry::new(SERVICE, REFRESH_TOKEN_ACCOUNT)
}
