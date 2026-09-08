use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::infrastructure::api::{ApiError, EmsysApiClient};

const INCOME_STATEMENT_SEARCH_PATH: &str = "/income-statements/search";

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IncomeStatementSearchRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<QueryFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagination: Option<Pagination>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sort: Vec<Sort>,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QueryFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub operator: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<QueryFilter>,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Pagination {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Sort {
    pub field: String,
    pub direction: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IncomeStatementSearchResponse {
    pub success: bool,
    pub message: String,
    pub data: Vec<IncomeStatement>,
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub page: u64,
    #[serde(default)]
    pub results_per_page: u64,
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub subtotal: u64,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IncomeStatementResponse {
    pub success: bool,
    pub message: String,
    pub data: IncomeStatement,
    #[serde(default)]
    pub error: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IncomeStatementSummaryResponse {
    pub success: bool,
    pub message: String,
    pub data: IncomeStatementSummary,
    #[serde(default)]
    pub error: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IncomeStatementSummary {
    #[serde(default)]
    pub currency: String,
    #[serde(default)]
    pub rate: f64,
    #[serde(default)]
    pub totals: Vec<SummaryTotalLine>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SummaryTotalLine {
    pub header: String,
    #[serde(default)]
    pub value: f64,
    #[serde(default)]
    pub order: i64,
    #[serde(default)]
    pub details: Vec<SummaryTotalLine>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IncomeStatement {
    pub id: u32,
    pub date: String,
    #[serde(default)]
    pub branch: Option<Branch>,
    #[serde(default)]
    pub container: Option<Container>,
    #[serde(default)]
    pub delivery: Option<Delivery>,
    #[serde(default)]
    pub rate: f64,
    #[serde(default)]
    pub currency: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub summary_total: Option<IncomeStatementTotal>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    pub id: u16,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub code: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Container {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub container_number: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Delivery {
    pub id: u32,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IncomeStatementTotal {
    #[serde(default)]
    pub invoices: f64,
    #[serde(default)]
    pub receipts: f64,
    #[serde(default)]
    pub invoice_payments: f64,
    #[serde(default)]
    pub receipt_payments: f64,
    #[serde(default)]
    pub other_incomes: f64,
    #[serde(default)]
    pub cash: f64,
    #[serde(default)]
    pub deposits: f64,
    #[serde(default)]
    pub check: f64,
    #[serde(default)]
    pub zelle: f64,
    #[serde(default)]
    pub credit_cards: f64,
    #[serde(default)]
    pub expenses: f64,
    #[serde(default)]
    pub account_receivables: f64,
    #[serde(default)]
    pub discounts: f64,
    #[serde(default)]
    pub accounts_transfer: f64,
    #[serde(default)]
    pub loans: f64,
    #[serde(default)]
    pub total_income: f64,
    #[serde(default)]
    pub total_general: f64,
    #[serde(default)]
    pub total_cash: f64,
    #[serde(default)]
    pub net_income: f64,
}

#[derive(Debug, Deserialize, Default)]
struct ApiErrorResponse {
    #[serde(default)]
    message: String,
    #[serde(default)]
    error: String,
}

#[derive(Debug, Error)]
pub enum IncomeStatementError {
    #[error(transparent)]
    Api(#[from] ApiError),

    #[error("income statement request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("EMSYS API returned status {status}: {message}")]
    Response { status: u16, message: String },
}

impl EmsysApiClient {
    pub async fn search_income_statements(
        &self,
        request: &IncomeStatementSearchRequest,
    ) -> Result<IncomeStatementSearchResponse, IncomeStatementError> {
        let response = self
            .tenant_request(Method::POST, INCOME_STATEMENT_SEARCH_PATH)
            .await?
            .json(request)
            .send()
            .await?;

        parse_response(response).await
    }

    pub async fn income_statement(
        &self,
        id: u32,
    ) -> Result<IncomeStatementResponse, IncomeStatementError> {
        let path = format!("/income-statements/{id}");
        let response = self.tenant_request(Method::GET, &path).await?.send().await?;

        parse_response(response).await
    }

    pub async fn income_statement_summary(
        &self,
        id: u32,
    ) -> Result<IncomeStatementSummaryResponse, IncomeStatementError> {
        let path = format!("/income-statements/{id}/summary-total");
        let response = self.tenant_request(Method::GET, &path).await?.send().await?;

        parse_response(response).await
    }
}

async fn parse_response<T>(response: reqwest::Response) -> Result<T, IncomeStatementError>
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
            .unwrap_or_else(|| "income statement request failed".to_string());

        return Err(IncomeStatementError::Response {
            status: status.as_u16(),
            message,
        });
    }

    Ok(response.json().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_search_request_using_api_contract() {
        let request = IncomeStatementSearchRequest {
            filters: vec![QueryFilter {
                field: Some("status".into()),
                operator: "eq".into(),
                value: Some(Value::String("open".into())),
                filters: Vec::new(),
            }],
            pagination: Some(Pagination {
                page: Some(1),
                offset: Some(0),
                limit: Some(40),
            }),
            sort: vec![Sort {
                field: "date".into(),
                direction: "desc".into(),
            }],
            ..Default::default()
        };

        let value = serde_json::to_value(request).expect("request should serialize");

        assert_eq!(value["filters"][0]["field"], "status");
        assert_eq!(value["filters"][0]["operator"], "eq");
        assert_eq!(value["filters"][0]["value"], "open");
        assert_eq!(value["pagination"]["limit"], 40);
        assert_eq!(value["sort"][0]["field"], "date");
        assert_eq!(value["sort"][0]["direction"], "desc");
    }

    #[test]
    fn deserializes_income_statement_response() {
        let response: IncomeStatementSearchResponse = serde_json::from_value(serde_json::json!({
            "success": true,
            "message": "Request successful",
            "data": [{
                "id": 42,
                "date": "2026-09-08T00:00:00Z",
                "branch": { "id": 1, "name": "Main", "code": "NYC" },
                "rate": 1.0,
                "currency": "USD",
                "status": "open",
                "summaryTotal": {
                    "cash": 4200.0,
                    "expenses": 965.0,
                    "netIncome": 11335.0
                }
            }],
            "error": "",
            "duration": 0.004,
            "page": 1,
            "resultsPerPage": 40,
            "total": 1,
            "subtotal": 1
        }))
        .expect("response should deserialize");

        assert_eq!(response.data.len(), 1);
        assert_eq!(response.data[0].id, 42);
        assert_eq!(response.data[0].status, "open");
        assert_eq!(
            response.data[0]
                .summary_total
                .as_ref()
                .expect("summary total")
                .net_income,
            11335.0
        );
    }

    #[test]
    fn deserializes_summary_total_response() {
        let response: IncomeStatementSummaryResponse = serde_json::from_value(serde_json::json!({
            "success": true,
            "message": "Request successful",
            "data": {
                "currency": "USD",
                "rate": 1.0,
                "totals": [{
                    "header": "Total Ingresos",
                    "value": 12800.0,
                    "order": 2,
                    "details": [{
                        "header": "Efectivo",
                        "value": 4200.0,
                        "order": 1
                    }]
                }]
            }
        }))
        .expect("summary should deserialize");

        assert_eq!(response.data.currency, "USD");
        assert_eq!(response.data.totals[0].header, "Total Ingresos");
        assert_eq!(response.data.totals[0].details[0].header, "Efectivo");
    }
}
