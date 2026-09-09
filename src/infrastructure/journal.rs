use reqwest::Method;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::infrastructure::{
    api::{ApiError, EmsysApiClient},
    query::QueryRequest,
};

const JOURNAL_SEARCH_PATH: &str = "/journals/search";
const JOURNAL_PATH: &str = "/journals";

pub type JournalSearchRequest = QueryRequest;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JournalSearchResponse {
    pub success: bool,
    pub message: String,
    #[serde(default, deserialize_with = "deserialize_vec")]
    pub data: Vec<Journal>,
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

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PostJournalRequest {
    pub transaction_type: String,
    pub income_statement_id: u32,
    pub income_statement: IncomeStatementPostReference,
    pub date: String,
    pub amount: f64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub ref_number: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub currency: String,
    pub rate: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub employee: Option<EmployeePostReference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<AccountPostReference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_account: Option<AccountPostReference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_account: Option<AccountPostReference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoice_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoice: Option<InvoicePostReference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_method: Option<PaymentMethod>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zelle_transaction_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zelle_transaction_name: Option<String>,
    #[serde(rename = "checkNumber", skip_serializing_if = "Option::is_none")]
    pub check_number: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IncomeStatementPostReference {
    pub id: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmployeePostReference {
    pub id: u16,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AccountPostReference {
    pub id: u32,
    pub name: String,
    #[serde(rename = "type")]
    pub account_type: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InvoicePostReference {
    pub number: String,
    pub cost: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub discount: f64,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PostJournalResponse {
    pub success: bool,
    pub message: String,
    pub data: Journal,
    #[serde(default)]
    pub error: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Journal {
    #[serde(default)]
    pub id: Value,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub ref_number: String,
    #[serde(default)]
    pub payment_method: Option<Value>,
    #[serde(default)]
    pub currency: String,
    #[serde(default, deserialize_with = "deserialize_f64")]
    pub rate: f64,
    #[serde(default)]
    pub transaction_type: String,
    #[serde(default)]
    pub invoice: Option<Value>,
    #[serde(default)]
    pub income_statement: Option<Value>,
    #[serde(default)]
    pub customer: Option<Value>,
    #[serde(default)]
    pub employee: Option<Value>,
    #[serde(default)]
    pub accounts: Vec<JournalAccount>,
    #[serde(default, deserialize_with = "deserialize_f64")]
    pub transaction_amount: f64,
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_f64")]
    pub transaction_balance: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PaymentMethod {
    #[serde(default)]
    pub id: u16,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JournalAccount {
    #[serde(default)]
    pub id: Value,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    #[serde(rename = "type")]
    pub account_type: String,
    #[serde(default, deserialize_with = "deserialize_f64")]
    pub debit: f64,
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_f64")]
    pub credit: f64,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IncomeStatementReference {
    #[serde(default)]
    pub id: u32,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NamedReference {
    #[serde(default)]
    pub id: serde_json::Value,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceReference {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub number: String,
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub payment: f64,
    #[serde(default)]
    pub balance: f64,
}

fn deserialize_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::Number(number) => Ok(number.as_f64().unwrap_or_default()),
        Value::String(text) => Ok(text.parse().unwrap_or_default()),
        Value::Null => Ok(0.0),
        _ => Ok(0.0),
    }
}

fn deserialize_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<Vec<T>>::deserialize(deserializer).map(|value| value.unwrap_or_default())
}

#[derive(Debug, Deserialize, Default)]
struct ApiErrorResponse {
    #[serde(default)]
    message: String,
    #[serde(default)]
    error: String,
}

#[derive(Debug, Error)]
pub enum JournalError {
    #[error(transparent)]
    Api(#[from] ApiError),

    #[error("journal request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("EMSYS API returned status {status}: {message}")]
    Response { status: u16, message: String },
}

impl EmsysApiClient {
    pub async fn search_journals(
        &self,
        request: &JournalSearchRequest,
    ) -> Result<JournalSearchResponse, JournalError> {
        let response = self
            .tenant_request(Method::POST, JOURNAL_SEARCH_PATH)
            .await?
            .json(request)
            .send()
            .await?;

        parse_response(response).await
    }

    pub async fn post_journal(
        &self,
        request: &PostJournalRequest,
    ) -> Result<PostJournalResponse, JournalError> {
        let response = self
            .tenant_request(Method::POST, JOURNAL_PATH)
            .await?
            .json(request)
            .send()
            .await?;

        parse_response(response).await
    }
}

fn is_zero(value: &f64) -> bool {
    value.abs() < f64::EPSILON
}

async fn parse_response<T>(response: reqwest::Response) -> Result<T, JournalError>
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
            .unwrap_or_else(|| "journal request failed".to_string());

        return Err(JournalError::Response {
            status: status.as_u16(),
            message,
        });
    }

    Ok(response.json().await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::query::{Pagination, QueryFilter, Sort};

    #[test]
    fn serializes_journal_search_for_income_statement_page() {
        let request = JournalSearchRequest {
            filters: vec![QueryFilter {
                field: Some("incomeStatement.id".into()),
                operator: "eq".into(),
                value: Some(serde_json::json!(42)),
                filters: Vec::new(),
            }],
            pagination: Some(Pagination {
                page: Some(2),
                offset: Some(10),
                limit: Some(10),
            }),
            sort: vec![Sort {
                field: "date".into(),
                direction: "desc".into(),
            }],
            ..Default::default()
        };

        let value = serde_json::to_value(request).expect("request should serialize");

        assert_eq!(value["filters"][0]["field"], "incomeStatement.id");
        assert_eq!(value["filters"][0]["operator"], "eq");
        assert_eq!(value["filters"][0]["value"], 42);
        assert_eq!(value["pagination"]["page"], 2);
        assert_eq!(value["pagination"]["limit"], 10);
        assert_eq!(value["sort"][0]["field"], "date");
    }

    #[test]
    fn deserializes_journal_search_response() {
        let response: JournalSearchResponse = serde_json::from_value(serde_json::json!({
            "success": true,
            "message": "Request successful",
            "data": [{
                "id": "66f000000000000000000001",
                "description": "Invoice payment",
                "date": "2026-09-08T00:00:00Z",
                "refNumber": "A-100",
                "paymentMethod": { "id": 1, "name": "CASH" },
                "transactionType": "PAYMENT",
                "incomeStatement": { "id": 42 },
                "accounts": [{
                    "id": 1,
                    "name": "Cash on Hand",
                    "type": "ASSET",
                    "debit": 120.0,
                    "credit": 0.0
                }],
                "transactionAmount": 120.0,
                "transactionBalance": 0.0
            }],
            "page": 1,
            "resultsPerPage": 10,
            "total": 1,
            "subtotal": 1
        }))
        .expect("response should deserialize");

        assert_eq!(response.data[0].description, "Invoice payment");
        assert_eq!(response.data[0].accounts[0].debit, 120.0);
        assert_eq!(
            response.data[0].income_statement.as_ref().unwrap()["id"],
            42
        );
    }

    #[test]
    fn deserializes_null_journal_data_as_empty_page() {
        let response: JournalSearchResponse = serde_json::from_value(serde_json::json!({
            "success": true,
            "message": "Request successful",
            "data": null,
            "page": 1,
            "resultsPerPage": 10,
            "total": 0,
            "subtotal": 0
        }))
        .expect("response should deserialize");

        assert!(response.data.is_empty());
        assert_eq!(response.total, 0);
    }

    #[test]
    fn serializes_post_journal_request_using_portal_contract() {
        let request = PostJournalRequest {
            transaction_type: "PAYMENT".into(),
            income_statement_id: 32658,
            income_statement: IncomeStatementPostReference { id: 32658 },
            date: "2026-09-09T00:00:00Z".into(),
            amount: 25.0,
            ref_number: String::new(),
            description: "Payment".into(),
            currency: "USD".into(),
            rate: 1.0,
            employee: Some(EmployeePostReference {
                id: 7,
                name: "Ada".into(),
            }),
            account: None,
            payment_account: Some(AccountPostReference {
                id: 20,
                name: "Checking".into(),
                account_type: "BANK".into(),
            }),
            source_account: None,
            invoice_id: Some("inv-1".into()),
            invoice: None,
            payment_method: Some(PaymentMethod {
                id: 4,
                name: "ZELLE".into(),
            }),
            zelle_transaction_date: Some("2026-09-09".into()),
            zelle_transaction_name: Some("Zelle Ref".into()),
            check_number: None,
        };

        let value = serde_json::to_value(request).expect("request should serialize");

        assert_eq!(value["transactionType"], "PAYMENT");
        assert_eq!(value["incomeStatementId"], 32658);
        assert_eq!(value["incomeStatement"]["id"], 32658);
        assert_eq!(value["paymentAccount"]["type"], "BANK");
        assert_eq!(value["invoiceId"], "inv-1");
        assert_eq!(value["paymentMethod"]["name"], "ZELLE");
        assert_eq!(value["zelleTransactionName"], "Zelle Ref");
        assert!(value.get("refNumber").is_none());
        assert!(value.get("checkNumber").is_none());
    }
}
