use std::path::Path;

use thiserror::Error;

use crate::infrastructure::{
    auth::{AuthError, AuthSession, FirebaseAuthClient},
    config::FirebaseConfig,
    session_store::{SessionFileStore, SessionStoreError},
};

#[derive(Debug, Clone)]
pub struct SessionManager {
    auth: FirebaseAuthClient,
    session_store: SessionFileStore,
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("not logged in; run `emsys-cli auth login`")]
    NotLoggedIn,

    #[error(transparent)]
    Auth(#[from] AuthError),

    #[error(transparent)]
    SessionStore(#[from] SessionStoreError),
}

impl SessionManager {
    pub fn new(config: &FirebaseConfig) -> Self {
        Self {
            auth: FirebaseAuthClient::new(config),
            session_store: SessionFileStore::new(),
        }
    }

    pub fn save(&self, session: &AuthSession) -> Result<(), SessionError> {
        self.session_store
            .save_refresh_token(&session.refresh_token)?;
        Ok(())
    }

    pub fn clear(&self) -> Result<bool, SessionError> {
        Ok(self.session_store.clear_refresh_token()?)
    }

    pub fn session_file_path(&self) -> &Path {
        self.session_store.path()
    }

    pub async fn refresh(&self) -> Result<AuthSession, SessionError> {
        let refresh_token = self
            .session_store
            .load_refresh_token()?
            .ok_or(SessionError::NotLoggedIn)?;

        let session = self.auth.refresh(&refresh_token).await?;

        if session.refresh_token != refresh_token {
            self.session_store
                .save_refresh_token(&session.refresh_token)?;
        }

        Ok(session)
    }
}
