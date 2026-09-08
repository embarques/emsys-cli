use reqwest::{Client, Method, RequestBuilder};
use thiserror::Error;

use crate::infrastructure::{
    auth::AuthSession,
    config::AppConfig,
    session::{SessionError, SessionManager},
};

const COMPANY_HEADER: &str = "x-company-id";

#[derive(Debug, Clone)]
pub struct EmsysApiClient {
    http: Client,
    base_url: String,
    company_id: Option<String>,
    session: SessionManager,
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("EMSYS API request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error(transparent)]
    Session(#[from] SessionError),

    #[error("no active company configured; set EMSYS_COMPANY_ID")]
    MissingCompany,

    #[error("EMSYS API authentication verification failed with status {0}")]
    Verification(u16),
}

impl EmsysApiClient {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            http: Client::new(),
            base_url: config.api_url.trim_end_matches('/').to_string(),
            company_id: config.company_id.clone(),
            session: SessionManager::new(&config.firebase),
        }
    }

    pub async fn verify_auth(&self) -> Result<AuthSession, ApiError> {
        let session = self.session.refresh().await?;
        let response = self
            .authenticated_request(Method::GET, "/users/me", &session.id_token)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            return Err(ApiError::Verification(status.as_u16()));
        }

        Ok(session)
    }

    pub async fn tenant_request(
        &self,
        method: Method,
        path: &str,
    ) -> Result<RequestBuilder, ApiError> {
        let company_id = self.company_id.as_deref().ok_or(ApiError::MissingCompany)?;
        let session = self.session.refresh().await?;

        Ok(self
            .authenticated_request(method, path, &session.id_token)
            .header(COMPANY_HEADER, company_id))
    }

    fn authenticated_request(&self, method: Method, path: &str, id_token: &str) -> RequestBuilder {
        self.request(method, path).bearer_auth(id_token)
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        let path = path.trim_start_matches('/');
        self.http
            .request(method, format!("{}/v1/{path}", self.base_url))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::config::FirebaseConfig;

    fn client(company_id: Option<&str>) -> EmsysApiClient {
        EmsysApiClient::new(&AppConfig {
            api_url: "https://api.embarqueros.com".into(),
            company_id: company_id.map(str::to_string),
            firebase: FirebaseConfig {
                web_api_key: "firebase-key".into(),
                project_id: "emsys-project".into(),
            },
        })
    }

    #[test]
    fn builds_v1_request_url() {
        let request = client(None)
            .request(Method::GET, "/users/me")
            .build()
            .expect("request should build");

        assert_eq!(
            request.url().as_str(),
            "https://api.embarqueros.com/v1/users/me"
        );
    }

    #[test]
    fn authenticated_request_adds_bearer_token() {
        let request = client(None)
            .authenticated_request(Method::GET, "/users/me", "test-token")
            .build()
            .expect("request should build");

        assert_eq!(
            request.headers().get("authorization").unwrap(),
            "Bearer test-token"
        );
        assert!(request.headers().get(COMPANY_HEADER).is_none());
    }

    #[test]
    fn tenant_request_adds_company_header() {
        let client = client(Some("64d5c0b0d1eab2aaf30b1818"));
        let request = client
            .authenticated_request(Method::GET, "/income-statements", "test-token")
            .header(
                COMPANY_HEADER,
                client.company_id.as_deref().expect("company should exist"),
            )
            .build()
            .expect("request should build");

        assert_eq!(
            request.headers().get("authorization").unwrap(),
            "Bearer test-token"
        );
        assert_eq!(
            request.headers().get(COMPANY_HEADER).unwrap(),
            "64d5c0b0d1eab2aaf30b1818"
        );
    }
}
