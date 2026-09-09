use anyhow::Context;
use serde_json::Value;

use crate::infrastructure::{
    api::EmsysApiClient,
    income_statement::{IncomeStatement, IncomeStatementSummary},
    journal::{Journal, JournalSearchResponse},
    query::{Pagination, QueryFilter, QueryRequest, Sort},
};

pub const DEFAULT_STATEMENT_LIMIT: u64 = 20;
pub const DEFAULT_JOURNAL_LIMIT: u64 = 10;

#[derive(Debug, Clone, PartialEq)]
pub struct IncomeStatementDetail {
    pub statement: IncomeStatement,
    pub summary: IncomeStatementSummary,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IncomeStatementScreen {
    pub statements: Vec<IncomeStatement>,
    pub statement_page: u64,
    pub statement_results_per_page: u64,
    pub statement_total: u64,
    pub selected_index: usize,
    pub detail: IncomeStatementDetail,
    pub journals: JournalPage,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JournalPage {
    pub entries: Vec<Journal>,
    pub page: u64,
    pub results_per_page: u64,
    pub total: u64,
    pub subtotal: u64,
}

#[derive(Debug, Clone)]
pub struct IncomeStatementService {
    api: EmsysApiClient,
}

impl IncomeStatementService {
    pub fn new(api: EmsysApiClient) -> Self {
        Self { api }
    }

    pub async fn latest(&self) -> anyhow::Result<IncomeStatementDetail> {
        let response = self
            .api
            .search_income_statements(&QueryRequest {
                pagination: Some(Pagination {
                    page: Some(1),
                    offset: Some(0),
                    limit: Some(1),
                }),
                sort: vec![Sort {
                    field: "date".into(),
                    direction: "desc".into(),
                }],
                ..Default::default()
            })
            .await?;

        let statement_id = response
            .data
            .first()
            .context("no income statements were found")?
            .id;

        self.show(statement_id).await
    }

    pub async fn screen(
        &self,
        statement_page: u64,
        selected_index: usize,
        journal_page: u64,
    ) -> anyhow::Result<IncomeStatementScreen> {
        let statement_page = statement_page.max(1);
        let statement_offset = (statement_page - 1) * DEFAULT_STATEMENT_LIMIT;
        let response = self
            .api
            .search_income_statements(&QueryRequest {
                pagination: Some(Pagination {
                    page: Some(statement_page),
                    offset: Some(statement_offset),
                    limit: Some(DEFAULT_STATEMENT_LIMIT),
                }),
                sort: vec![Sort {
                    field: "date".into(),
                    direction: "desc".into(),
                }],
                ..Default::default()
            })
            .await?;

        let selected_index = selected_index.min(response.data.len().saturating_sub(1));
        let statement_id = response
            .data
            .get(selected_index)
            .context("no income statements were found")?
            .id;
        let detail = self.show(statement_id).await?;
        let journals = self.journal_page(statement_id, journal_page).await?;

        Ok(IncomeStatementScreen {
            statements: response.data,
            statement_page: response.page.max(1),
            statement_results_per_page: response.results_per_page,
            statement_total: response.total,
            selected_index,
            detail,
            journals,
        })
    }

    pub async fn show(&self, id: u32) -> anyhow::Result<IncomeStatementDetail> {
        let statement = self.api.income_statement(id).await?.data;
        let summary = self.api.income_statement_summary(id).await?.data;

        Ok(IncomeStatementDetail { statement, summary })
    }

    pub async fn journal_page(&self, statement_id: u32, page: u64) -> anyhow::Result<JournalPage> {
        let page = page.max(1);
        let offset = (page - 1) * DEFAULT_JOURNAL_LIMIT;
        let response = self
            .api
            .search_journals(&QueryRequest {
                filters: vec![QueryFilter {
                    field: Some("incomeStatement.id".into()),
                    operator: "eq".into(),
                    value: Some(Value::from(statement_id)),
                    filters: Vec::new(),
                }],
                pagination: Some(Pagination {
                    page: Some(page),
                    offset: Some(offset),
                    limit: Some(DEFAULT_JOURNAL_LIMIT),
                }),
                sort: vec![
                    Sort {
                        field: "date".into(),
                        direction: "desc".into(),
                    },
                    Sort {
                        field: "createdAt".into(),
                        direction: "desc".into(),
                    },
                ],
                ..Default::default()
            })
            .await?;

        Ok(journal_page_from_response(response))
    }
}

fn journal_page_from_response(response: JournalSearchResponse) -> JournalPage {
    JournalPage {
        entries: response.data,
        page: response.page.max(1),
        results_per_page: response.results_per_page,
        total: response.total,
        subtotal: response.subtotal,
    }
}
