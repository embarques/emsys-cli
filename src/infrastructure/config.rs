use std::env;

use thiserror::Error;

const EMSYS_API_URL: &str = "EMSYS_API_URL";
const FIREBASE_WEB_API_KEY: &str = "FIREBASE_WEB_API_KEY";
const FIREBASE_PROJECT_ID: &str = "FIREBASE_PROJECT_ID";

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub api_url: String,
    pub firebase: FirebaseConfig,
}

#[derive(Debug, Clone)]
pub struct FirebaseConfig {
    pub web_api_key: String,
    pub project_id: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("missing required environment variable: {0}")]
    Missing(&'static str),

    #[error("invalid configuration value: {0}")]
    Invalid(&'static str),
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        dotenvy::dotenv().ok();

        Self::from_values(
            required_env(EMSYS_API_URL)?,
            required_env(FIREBASE_WEB_API_KEY)?,
            required_env(FIREBASE_PROJECT_ID)?,
        )
    }

    fn from_values(
        api_url: String,
        firebase_web_api_key: String,
        firebase_project_id: String,
    ) -> Result<Self, ConfigError> {
        let config = Self {
            api_url,
            firebase: FirebaseConfig {
                web_api_key: firebase_web_api_key,
                project_id: firebase_project_id,
            },
        };

        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if self.api_url.trim().is_empty()
            || !(self.api_url.starts_with("http://") || self.api_url.starts_with("https://"))
        {
            return Err(ConfigError::Invalid(EMSYS_API_URL));
        }

        if self.firebase.web_api_key.trim().is_empty() {
            return Err(ConfigError::Invalid(FIREBASE_WEB_API_KEY));
        }

        if self.firebase.project_id.trim().is_empty() {
            return Err(ConfigError::Invalid(FIREBASE_PROJECT_ID));
        }

        Ok(())
    }
}

fn required_env(name: &'static str) -> Result<String, ConfigError> {
    env::var(name).map_err(|_| ConfigError::Missing(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_configuration() {
        let config = AppConfig::from_values(
            "http://localhost:8080".into(),
            "firebase-key".into(),
            "emsys-project".into(),
        )
        .expect("valid config should pass");

        assert_eq!(config.api_url, "http://localhost:8080");
        assert_eq!(config.firebase.project_id, "emsys-project");
    }

    #[test]
    fn rejects_invalid_api_url() {
        let error = AppConfig::from_values(
            "localhost:8080".into(),
            "firebase-key".into(),
            "emsys-project".into(),
        )
        .expect_err("invalid URL should fail");

        assert_eq!(error, ConfigError::Invalid(EMSYS_API_URL));
    }

    #[test]
    fn rejects_empty_firebase_key() {
        let error = AppConfig::from_values(
            "https://api.embarqueros.com".into(),
            "   ".into(),
            "emsys-project".into(),
        )
        .expect_err("empty Firebase key should fail");

        assert_eq!(error, ConfigError::Invalid(FIREBASE_WEB_API_KEY));
    }

    #[test]
    fn rejects_empty_project_id() {
        let error = AppConfig::from_values(
            "https://api.embarqueros.com".into(),
            "firebase-key".into(),
            "".into(),
        )
        .expect_err("empty project ID should fail");

        assert_eq!(error, ConfigError::Invalid(FIREBASE_PROJECT_ID));
    }
}
