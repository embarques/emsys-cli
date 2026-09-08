use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
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
    pub company_id: Option<String>,
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Firebase authentication request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Firebase authentication failed ({status}): {message}")]
    Firebase { status: u16, message: String },

    #[error("Firebase returned an invalid expiresIn value")]
    InvalidExpiry,

    #[error("Firebase returned an invalid ID token")]
    InvalidIdToken,

    #[error("Firebase companyId claim is not a valid EMSYS company ObjectID")]
    InvalidCompanyId,
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
        let company_id = company_id_from_token(&body.id_token)?;

        Ok(AuthSession {
            id_token: body.id_token,
            refresh_token: body.refresh_token,
            expires_in: parse_expiry(&body.expires_in)?,
            user_id: body.local_id,
            email: Some(body.email),
            company_id,
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
        let company_id = company_id_from_token(&body.id_token)?;

        Ok(AuthSession {
            id_token: body.id_token,
            refresh_token: body.refresh_token,
            expires_in: parse_expiry(&body.expires_in)?,
            user_id: body.user_id,
            email: None,
            company_id,
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

#[derive(Debug, Deserialize)]
struct IdTokenClaims {
    #[serde(rename = "companyId")]
    company_id: Option<String>,
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

fn company_id_from_token(id_token: &str) -> Result<Option<String>, AuthError> {
    let payload = id_token.split('.').nth(1).ok_or(AuthError::InvalidIdToken)?;
    let decoded = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| AuthError::InvalidIdToken)?;
    let claims: IdTokenClaims =
        serde_json::from_slice(&decoded).map_err(|_| AuthError::InvalidIdToken)?;

    let company_id = claims
        .company_id
        .map(|company_id| company_id.trim().to_string())
        .filter(|company_id| !company_id.is_empty());

    if let Some(company_id) = &company_id {
        if !is_mongodb_object_id(company_id) {
            return Err(AuthError::InvalidCompanyId);
        }
    }

    Ok(company_id)
}

fn is_mongodb_object_id(value: &str) -> bool {
    value.len() == 24 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token_with_payload(payload: &str) -> String {
        let payload = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        format!("header.{payload}.signature")
    }

    #[test]
    fn parses_expiry_seconds() {
        assert_eq!(parse_expiry("3600").expect("valid expiry"), 3600);
    }

    #[test]
    fn rejects_invalid_expiry() {
        assert!(matches!(parse_expiry("invalid"), Err(AuthError::InvalidExpiry)));
    }

    #[test]
    fn extracts_company_id_from_token_claim() {
        let token = token_with_payload(r#"{"companyId":"64d5c0b0d1eab2aaf30b1818"}"#);

        assert_eq!(
            company_id_from_token(&token).expect("valid token"),
            Some("64d5c0b0d1eab2aaf30b1818".into())
        );
    }

    #[test]
    fn allows_token_without_company_claim() {
        let token = token_with_payload(r#"{"sub":"firebase-user"}"#);

        assert_eq!(company_id_from_token(&token).expect("valid token"), None);
    }

    #[test]
    fn rejects_invalid_company_id_claim() {
        let token = token_with_payload(r#"{"companyId":"not-an-object-id"}"#);

        assert!(matches!(
            company_id_from_token(&token),
            Err(AuthError::InvalidCompanyId)
        ));
    }

    #[test]
    fn rejects_invalid_token_payload() {
        assert!(matches!(
            company_id_from_token("not-a-jwt"),
            Err(AuthError::InvalidIdToken)
        ));
    }
}
