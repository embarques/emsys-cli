use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QueryRequest {
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
