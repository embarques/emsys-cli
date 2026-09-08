use reqwest::Client;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct EmsysApiClient {
    http: Client,
    base_url: String,
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("EMSYS API request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("EMSYS API authentication verification failed with status {0}")]
    Verification(u16),
}

impl EmsysApiClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            http: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    pub async fn verify_auth(&self, id_token: &str) -> Result<(), ApiError> {
        let url = format!("{}/v1/users/me", self.base_url);
        let response = self.http.get(url).bearer_auth(id_token).send().await?;

        let status = response.status();
        if !status.is_success() {
            return Err(ApiError::Verification(status.as_u16()));
        }

        Ok(())
    }
}
