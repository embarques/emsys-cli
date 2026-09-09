use reqwest::Method;
use serde::{Deserialize, Deserializer};
use thiserror::Error;

use crate::infrastructure::{
    api::{ApiError, EmsysApiClient},
    query::QueryRequest,
};

const CHART_ACCOUNTS_SEARCH_PATH: &str = "/chart-accounts/search";

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChartAccount {
    #[serde(default)]
    pub id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(rename = "type", default)]
    pub account_type: String,
    #[serde(default)]
    pub system_account: bool,
}

impl ChartAccount {
    pub fn label(&self) -> String {
        if self.display_name.trim().is_empty() {
            self.name.clone()
        } else {
            self.display_name.clone()
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartAccountSearchResponse {
    #[serde(default, deserialize_with = "deserialize_vec")]
    pub data: Vec<ChartAccount>,
}

#[derive(Debug, Deserialize, Default)]
struct ApiErrorResponse {
    #[serde(default)]
    message: String,
    #[serde(default)]
    error: String,
}

#[derive(Debug, Error)]
pub enum ChartAccountError {
    #[error(transparent)]
    Api(#[from] ApiError),

    #[error("chart account request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("EMSYS API returned status {status}: {message}")]
    Response { status: u16, message: String },
}

impl EmsysApiClient {
    pub async fn search_chart_accounts(
        &self,
        request: &QueryRequest,
    ) -> Result<ChartAccountSearchResponse, ChartAccountError> {
        let response = self
            .tenant_request(Method::POST, CHART_ACCOUNTS_SEARCH_PATH)
            .await?
            .json(request)
            .send()
            .await?;

        parse_response(response).await
    }
}

async fn parse_response<T>(response: reqwest::Response) -> Result<T, ChartAccountError>
where
    T: for<'de> Deserialize<'de>,
{
    let status = response.status();
    if !status.is_success() {
        let body = response.json::<ApiErrorResponse>().await.ok();
        let message = body
            .map(|body| {
                if body.error.is_empty() {
                    body.message
                } else {
                    body.error
                }
            })
            .filter(|message| !message.is_empty())
            .unwrap_or_else(|| "chart account request failed".to_string());

        return Err(ChartAccountError::Response {
            status: status.as_u16(),
            message,
        });
    }

    Ok(response.json().await?)
}

fn deserialize_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<Vec<T>>::deserialize(deserializer).map(|value| value.unwrap_or_default())
}
