use reqwest::Method;
use serde::{Deserialize, Deserializer};
use thiserror::Error;

use crate::infrastructure::{
    api::{ApiError, EmsysApiClient},
    query::QueryRequest,
};

const EMPLOYEES_SEARCH_PATH: &str = "/employees/search";

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Employee {
    #[serde(default)]
    pub id: u16,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub active: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmployeeSearchResponse {
    #[serde(default, deserialize_with = "deserialize_vec")]
    pub data: Vec<Employee>,
}

#[derive(Debug, Deserialize, Default)]
struct ApiErrorResponse {
    #[serde(default)]
    message: String,
    #[serde(default)]
    error: String,
}

#[derive(Debug, Error)]
pub enum EmployeeError {
    #[error(transparent)]
    Api(#[from] ApiError),

    #[error("employee request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("EMSYS API returned status {status}: {message}")]
    Response { status: u16, message: String },
}

impl EmsysApiClient {
    pub async fn search_employees(
        &self,
        request: &QueryRequest,
    ) -> Result<EmployeeSearchResponse, EmployeeError> {
        let response = self
            .tenant_request(Method::POST, EMPLOYEES_SEARCH_PATH)
            .await?
            .json(request)
            .send()
            .await?;

        parse_response(response).await
    }
}

async fn parse_response<T>(response: reqwest::Response) -> Result<T, EmployeeError>
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
            .unwrap_or_else(|| "employee request failed".to_string());

        return Err(EmployeeError::Response {
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
