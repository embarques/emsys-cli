use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::infrastructure::config::FirebaseConfig;

const SIGN_IN_URL: &str =
    "https://identitytoolkit.googleapis.com/v1/accounts:signInWithPassword";
const REFRESH_URL: &str = "https://securetoken.googleapis.com/v1/token";

#[derive(Debug, Clone)]
pub struct FirebaseAuthClient {
    http: Client,
    api_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSession {
    pub id_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
    pub user_id: String,
    pub email: Option<String>,
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Firebase authentication request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Firebase authentication failed ({status}): {message}")]
    Firebase { status: u16, message: String },

    #[error("Firebase returned an invalid expiresIn value")]
    InvalidExpiry,
}

impl FirebaseAuthClient {
    pub fn new(config: &FirebaseConfig) -> Self {
        Self {
            http: Client::new(),
            api_key: config.web_api_key.clone(),
        }
    }

    pub async fn sign_in(&self, email: &str, password: &str) -> Result<AuthSession, AuthError> {
        let response = self
            .http
            .post(SIGN_IN_URL)
            .query(&[("key", self.api_key.as_str())])
            .json(&SignInRequest {
                email,
                password,
                return_secure_token: true,
            })
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            return Err(firebase_error(response, status.as_u16()).await);
        }

        let body: SignInResponse = response.json().await?;
        Ok(AuthSession {
            id_token: body.id_token,
            refresh_token: body.refresh_token,
            expires_in: parse_expiry(&body.expires_in)?,
            user_id: body.local_id,
            email: Some(body.email),
        })
    }

    pub async fn refresh(&self, refresh_token: &str) -> Result<AuthSession, AuthError> {
        let response = self
            .http
            .post(REFRESH_URL)
            .query(&[("key", self.api_key.as_str())])
            .form(&RefreshRequest {
                grant_type: "refresh_token",
                refresh_token,
            })
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            return Err(firebase_error(response, status.as_u16()).await);
        }

        let body: RefreshResponse = response.json().await?;
        Ok(AuthSession {
            id_token: body.id_token,
            refresh_token: body.refresh_token,
            expires_in: parse_expiry(&body.expires_in)?,
            user_id: body.user_id,
            email: None,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SignInRequest<'a> {
    email: &'a str,
    password: &'a str,
    return_secure_token: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SignInResponse {
    id_token: String,
    email: String,
    refresh_token: String,
    expires_in: String,
    local_id: String,
}

#[derive(Debug, Serialize)]
struct RefreshRequest<'a> {
    grant_type: &'static str,
    refresh_token: &'a str,
}

#[derive(Debug, Deserialize)]
struct RefreshResponse {
    #[serde(rename = "id_token")]
    id_token: String,
    #[serde(rename = "refresh_token")]
    refresh_token: String,
    #[serde(rename = "expires_in")]
    expires_in: String,
    #[serde(rename = "user_id")]
    user_id: String,
}

#[derive(Debug, Deserialize)]
struct FirebaseErrorEnvelope {
    error: FirebaseErrorBody,
}

#[derive(Debug, Deserialize)]
struct FirebaseErrorBody {
    message: String,
}

async fn firebase_error(response: reqwest::Response, status: u16) -> AuthError {
    let message = response
        .json::<FirebaseErrorEnvelope>()
        .await
        .map(|body| body.error.message)
        .unwrap_or_else(|_| "unknown Firebase authentication error".to_string());

    AuthError::Firebase { status, message }
}

fn parse_expiry(value: &str) -> Result<u64, AuthError> {
    value.parse().map_err(|_| AuthError::InvalidExpiry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_expiry_seconds() {
        assert_eq!(parse_expiry("3600").expect("valid expiry"), 3600);
    }

    #[test]
    fn rejects_invalid_expiry() {
        assert!(matches!(parse_expiry("invalid"), Err(AuthError::InvalidExpiry)));
    }
}
