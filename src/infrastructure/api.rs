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
    session: SessionManager,
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("EMSYS API request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error(transparent)]
    Session(#[from] SessionError),

    #[error("authenticated Firebase user has no companyId claim")]
    MissingCompany,

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
        let session = self.session.refresh().await?;
        let company_id = session.company_id.as_deref().ok_or(ApiError::MissingCompany)?;

        Ok(self.tenant_authenticated_request(method, path, &session.id_token, company_id))
    }

    fn tenant_authenticated_request(
        &self,
        method: Method,
        path: &str,
        id_token: &str,
        company_id: &str,
    ) -> RequestBuilder {
        self.authenticated_request(method, path, id_token)
            .header(COMPANY_HEADER, company_id)
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

    fn client() -> EmsysApiClient {
        EmsysApiClient::new(&AppConfig {
            api_url: "https://api.embarqueros.com".into(),
            firebase: FirebaseConfig {
                web_api_key: "firebase-key".into(),
                project_id: "emsys-project".into(),
            },
        })
    }

    #[test]
    fn builds_v1_request_url() {
        let request = client()
            .request(Method::GET, "/users/me")
            .build()
            .expect("request should build");

        assert_eq!(
            request.url().as_str(),
            "https://api.embarqueros.com/v1/users/me"
        );
    }

    #[test]
    fn authenticated_request_adds_bearer_token_only() {
        let request = client()
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
    fn tenant_authenticated_request_adds_auth_and_company_headers() {
        let request = client()
            .tenant_authenticated_request(
                Method::GET,
                "/income-statements",
                "test-token",
                "64d5c0b0d1eab2aaf30b1818",
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
