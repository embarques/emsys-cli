use reqwest::Method;
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use thiserror::Error;

use crate::infrastructure::{
    api::{ApiError, EmsysApiClient},
    query::QueryRequest,
};

const INVOICES_SEARCH_PATH: &str = "/invoices/search";

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Invoice {
    #[serde(default)]
    pub id: Value,
    #[serde(default)]
    pub number: String,
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub payment: f64,
    #[serde(default)]
    pub balance: f64,
    #[serde(default)]
    pub discount: f64,
}

impl Invoice {
    pub fn id_string(&self) -> String {
        match &self.id {
            Value::String(value) => value.clone(),
            Value::Number(value) => value.to_string(),
            _ => String::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceSearchResponse {
    #[serde(default, deserialize_with = "deserialize_vec")]
    pub data: Vec<Invoice>,
}

#[derive(Debug, Deserialize, Default)]
struct ApiErrorResponse {
    #[serde(default)]
    message: String,
    #[serde(default)]
    error: String,
}

#[derive(Debug, Error)]
pub enum InvoiceError {
    #[error(transparent)]
    Api(#[from] ApiError),

    #[error("invoice request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("EMSYS API returned status {status}: {message}")]
    Response { status: u16, message: String },
}

impl EmsysApiClient {
    pub async fn search_invoices(
        &self,
        request: &QueryRequest,
    ) -> Result<InvoiceSearchResponse, InvoiceError> {
        let response = self
            .tenant_request(Method::POST, INVOICES_SEARCH_PATH)
            .await?
            .json(request)
            .send()
            .await?;

        parse_response(response).await
    }
}

async fn parse_response<T>(response: reqwest::Response) -> Result<T, InvoiceError>
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
            .unwrap_or_else(|| "invoice request failed".to_string());

        return Err(InvoiceError::Response {
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
