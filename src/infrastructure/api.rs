use reqwest::Client;
use thiserror::Error;

use crate::infrastructure::{
    auth::AuthSession,
    config::AppConfig,
    session::{SessionError, SessionManager},
};

#[derive(Debug, Clone)]
pub struct EmsysApiClient {
    http: Client,
    base_url: String,
    session: SessionManager,
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("EMSYS API request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error(transparent)]
    Session(#[from] SessionError),

    #[error("EMSYS API authentication verification failed with status {0}")]
    Verification(u16),
}

impl EmsysApiClient {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            http: Client::new(),
            base_url: config.api_url.trim_end_matches('/').to_string(),
            session: SessionManager::new(&config.firebase),
        }
    }

    pub async fn verify_auth(&self) -> Result<AuthSession, ApiError> {
        let session = self.session.refresh().await?;
        let url = format!("{}/v1/users/me", self.base_url);
        let response = self
            .http
            .get(url)
            .bearer_auth(&session.id_token)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            return Err(ApiError::Verification(status.as_u16()));
        }

        Ok(session)
    }
}
