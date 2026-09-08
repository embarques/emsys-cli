use thiserror::Error;

use crate::infrastructure::{
    auth::{AuthError, AuthSession, FirebaseAuthClient},
    config::FirebaseConfig,
    credentials::{CredentialError, CredentialStore},
};

#[derive(Debug, Clone)]
pub struct SessionManager {
    auth: FirebaseAuthClient,
    credentials: CredentialStore,
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("not logged in; run `emsys-cli auth login`")]
    NotLoggedIn,

    #[error(transparent)]
    Auth(#[from] AuthError),

    #[error(transparent)]
    Credentials(#[from] CredentialError),
}

impl SessionManager {
    pub fn new(config: &FirebaseConfig) -> Self {
        Self {
            auth: FirebaseAuthClient::new(config),
            credentials: CredentialStore::new(),
        }
    }

    pub fn save(&self, session: &AuthSession) -> Result<(), SessionError> {
        self.credentials
            .save_refresh_token(&session.refresh_token)?;
        Ok(())
    }

    pub fn clear(&self) -> Result<bool, SessionError> {
        Ok(self.credentials.clear_refresh_token()?)
    }

    pub async fn refresh(&self) -> Result<AuthSession, SessionError> {
        let refresh_token = self
            .credentials
            .load_refresh_token()?
            .ok_or(SessionError::NotLoggedIn)?;

        let session = self.auth.refresh(&refresh_token).await?;

        if session.refresh_token != refresh_token {
            self.credentials
                .save_refresh_token(&session.refresh_token)?;
        }

        Ok(session)
    }
}
