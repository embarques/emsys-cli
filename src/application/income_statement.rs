use anyhow::Context;

use crate::infrastructure::{
    api::EmsysApiClient,
    income_statement::{
        IncomeStatement, IncomeStatementSearchRequest, IncomeStatementSummary, Pagination, Sort,
    },
};

#[derive(Debug, Clone, PartialEq)]
pub struct IncomeStatementDetail {
    pub statement: IncomeStatement,
    pub summary: IncomeStatementSummary,
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
            .search_income_statements(&IncomeStatementSearchRequest {
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

    pub async fn show(&self, id: u32) -> anyhow::Result<IncomeStatementDetail> {
        let statement = self.api.income_statement(id).await?.data;
        let summary = self.api.income_statement_summary(id).await?.data;

        Ok(IncomeStatementDetail { statement, summary })
    }
}
