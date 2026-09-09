use anyhow::{Context, bail};
use serde_json::Value;

use crate::infrastructure::{
    api::EmsysApiClient,
    chart_account::ChartAccount,
    employee::Employee,
    income_statement::{IncomeStatement, IncomeStatementSummary},
    invoice::Invoice,
    journal::{
        AccountPostReference, EmployeePostReference, IncomeStatementPostReference,
        InvoicePostReference, Journal, JournalSearchResponse, PaymentMethod, PostJournalRequest,
    },
    query::{Pagination, QueryFilter, QueryRequest, Sort},
};

pub const DEFAULT_STATEMENT_LIMIT: u64 = 20;
pub const DEFAULT_JOURNAL_LIMIT: u64 = 10;
pub const LOOKUP_LIMIT: u64 = 500;

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
    pub lookups: TransactionLookups,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JournalPage {
    pub entries: Vec<Journal>,
    pub page: u64,
    pub results_per_page: u64,
    pub total: u64,
    pub subtotal: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TransactionLookups {
    pub employees: Vec<Employee>,
    pub accounts: Vec<ChartAccount>,
    pub invoices: Vec<Invoice>,
    pub payment_methods: Vec<PaymentMethod>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalTransactionType {
    InitialPayment,
    Payment,
    Expense,
    Sales,
    Discount,
    Surcharge,
    Transfer,
}

impl JournalTransactionType {
    pub fn all() -> &'static [Self] {
        &[
            Self::InitialPayment,
            Self::Payment,
            Self::Expense,
            Self::Sales,
            Self::Discount,
            Self::Surcharge,
            Self::Transfer,
        ]
    }

    pub fn as_api(self) -> &'static str {
        match self {
            Self::InitialPayment => "INITIAL-PAYMENT",
            Self::Payment => "PAYMENT",
            Self::Expense => "EXPENSE",
            Self::Sales => "SALES",
            Self::Discount => "DISCOUNT",
            Self::Surcharge => "SURCHARGE",
            Self::Transfer => "TRANSFER",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::InitialPayment => "Initial payment",
            Self::Payment => "Payment",
            Self::Expense => "Expense",
            Self::Sales => "Sales",
            Self::Discount => "Discount",
            Self::Surcharge => "Surcharge",
            Self::Transfer => "Transfer",
        }
    }

    pub fn needs_existing_invoice(self) -> bool {
        matches!(self, Self::Payment | Self::Discount | Self::Surcharge)
    }

    pub fn needs_account(self) -> bool {
        matches!(self, Self::Expense | Self::Sales | Self::Transfer)
    }

    pub fn needs_payment_method(self) -> bool {
        matches!(
            self,
            Self::InitialPayment | Self::Payment | Self::Discount | Self::Surcharge | Self::Sales
        )
    }

    pub fn needs_source_account(self) -> bool {
        matches!(self, Self::Expense | Self::Transfer)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct JournalTransactionValues {
    pub transaction_type: JournalTransactionType,
    pub amount: f64,
    pub ref_number: String,
    pub description: String,
    pub employee_id: Option<u16>,
    pub account_id: Option<u32>,
    pub payment_account_id: Option<u32>,
    pub source_account_id: Option<u32>,
    pub invoice_id: Option<String>,
    pub invoice_number: String,
    pub invoice_cost: f64,
    pub invoice_discount: f64,
    pub payment_method_id: Option<u16>,
    pub zelle_transaction_date: String,
    pub zelle_transaction_name: String,
    pub check_number: String,
}

impl Default for JournalTransactionValues {
    fn default() -> Self {
        Self {
            transaction_type: JournalTransactionType::InitialPayment,
            amount: 0.0,
            ref_number: String::new(),
            description: String::new(),
            employee_id: None,
            account_id: None,
            payment_account_id: None,
            source_account_id: None,
            invoice_id: None,
            invoice_number: String::new(),
            invoice_cost: 0.0,
            invoice_discount: 0.0,
            payment_method_id: Some(1),
            zelle_transaction_date: String::new(),
            zelle_transaction_name: String::new(),
            check_number: String::new(),
        }
    }
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
        let lookups = self.transaction_lookups().await?;

        Ok(IncomeStatementScreen {
            statements: response.data,
            statement_page: response.page.max(1),
            statement_results_per_page: response.results_per_page,
            statement_total: response.total,
            selected_index,
            detail,
            journals,
            lookups,
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

    pub async fn transaction_lookups(&self) -> anyhow::Result<TransactionLookups> {
        let (employees, accounts, invoices) = tokio::try_join!(
            self.search_active_employees(),
            self.search_chart_accounts(),
            self.search_open_invoices()
        )?;

        Ok(TransactionLookups {
            employees,
            accounts,
            invoices,
            payment_methods: payment_methods(),
        })
    }

    pub async fn post_transaction(
        &self,
        statement: &IncomeStatement,
        lookups: &TransactionLookups,
        values: &JournalTransactionValues,
    ) -> anyhow::Result<Journal> {
        let request = build_post_request(statement, lookups, values)?;
        let response = self.api.post_journal(&request).await?;

        Ok(response.data)
    }

    pub async fn set_statement_open(&self, id: u32, open: bool) -> anyhow::Result<IncomeStatement> {
        Ok(self.api.set_income_statement_open(id, open).await?.data)
    }

    async fn search_active_employees(&self) -> anyhow::Result<Vec<Employee>> {
        let response = self
            .api
            .search_employees(&QueryRequest {
                filters: vec![QueryFilter {
                    field: Some("active".into()),
                    operator: "eq".into(),
                    value: Some(Value::Bool(true)),
                    filters: Vec::new(),
                }],
                pagination: Some(lookup_pagination()),
                sort: vec![Sort {
                    field: "name".into(),
                    direction: "asc".into(),
                }],
                ..Default::default()
            })
            .await?;

        Ok(response.data)
    }

    async fn search_chart_accounts(&self) -> anyhow::Result<Vec<ChartAccount>> {
        let response = self
            .api
            .search_chart_accounts(&QueryRequest {
                pagination: Some(lookup_pagination()),
                sort: vec![Sort {
                    field: "displayName".into(),
                    direction: "asc".into(),
                }],
                ..Default::default()
            })
            .await?;

        Ok(response.data)
    }

    async fn search_open_invoices(&self) -> anyhow::Result<Vec<Invoice>> {
        let response = self
            .api
            .search_invoices(&QueryRequest {
                filters: vec![
                    QueryFilter {
                        field: Some("isVoid".into()),
                        operator: "eq".into(),
                        value: Some(Value::Bool(false)),
                        filters: Vec::new(),
                    },
                    QueryFilter {
                        field: Some("isArchive".into()),
                        operator: "eq".into(),
                        value: Some(Value::Bool(false)),
                        filters: Vec::new(),
                    },
                ],
                pagination: Some(lookup_pagination()),
                sort: vec![Sort {
                    field: "date".into(),
                    direction: "desc".into(),
                }],
                ..Default::default()
            })
            .await?;

        Ok(response.data)
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

fn build_post_request(
    statement: &IncomeStatement,
    lookups: &TransactionLookups,
    values: &JournalTransactionValues,
) -> anyhow::Result<PostJournalRequest> {
    validate_transaction(statement, lookups, values)?;

    let transaction_type = values.transaction_type;
    let payment_method = if payment_required(values) {
        payment_method_by_id(lookups, values.payment_method_id)
    } else {
        None
    };
    let mut ref_number = values.ref_number.trim().to_string();
    let check_number = values.check_number.trim().to_string();
    if payment_method
        .as_ref()
        .is_some_and(|method| method.name == "CHECK")
        && ref_number.is_empty()
    {
        ref_number = check_number.clone();
    }

    Ok(PostJournalRequest {
        transaction_type: transaction_type.as_api().into(),
        income_statement_id: statement.id,
        income_statement: IncomeStatementPostReference { id: statement.id },
        date: statement.date.clone(),
        amount: values.amount,
        ref_number,
        description: values.description.trim().to_string(),
        currency: statement.currency.clone(),
        rate: statement.rate,
        employee: employee_by_id(lookups, values.employee_id).map(|employee| {
            EmployeePostReference {
                id: employee.id,
                name: employee.name.clone(),
            }
        }),
        account: account_by_id(lookups, values.account_id).map(account_reference),
        payment_account: account_by_id(lookups, values.payment_account_id).map(account_reference),
        source_account: account_by_id(lookups, values.source_account_id).map(account_reference),
        invoice_id: if transaction_type.needs_existing_invoice() {
            values.invoice_id.clone()
        } else {
            None
        },
        invoice: if matches!(transaction_type, JournalTransactionType::InitialPayment) {
            Some(InvoicePostReference {
                number: values.invoice_number.trim().to_string(),
                cost: values.invoice_cost,
                discount: values.invoice_discount,
            })
        } else {
            None
        },
        payment_method,
        zelle_transaction_date: non_empty(values.zelle_transaction_date.trim()),
        zelle_transaction_name: non_empty(values.zelle_transaction_name.trim()),
        check_number: non_empty(check_number.trim()),
    })
}

fn validate_transaction(
    statement: &IncomeStatement,
    lookups: &TransactionLookups,
    values: &JournalTransactionValues,
) -> anyhow::Result<()> {
    if !statement.status.eq_ignore_ascii_case("open") {
        bail!(
            "income statement #{} is {}; open it before adding journal transactions",
            statement.id,
            status_text(&statement.status)
        );
    }

    let transaction_type = values.transaction_type;
    if matches!(transaction_type, JournalTransactionType::InitialPayment) {
        ensure_non_negative(values.amount, "amount")?;
    } else {
        ensure_positive(values.amount, "amount")?;
    }

    if employee_by_id(lookups, values.employee_id).is_none() {
        bail!("select an employee");
    }

    if transaction_type.needs_existing_invoice() {
        let invoice = selected_invoice(lookups, values)?;
        if matches!(transaction_type, JournalTransactionType::Payment)
            && invoice.balance > 0.0
            && values.amount > invoice.balance
        {
            bail!(
                "payment amount cannot exceed invoice balance {}",
                format_money(invoice.balance)
            );
        }
    }

    if matches!(transaction_type, JournalTransactionType::InitialPayment) {
        if values.invoice_number.trim().is_empty() {
            bail!("enter an invoice number");
        }
        ensure_positive(values.invoice_cost, "invoice cost")?;
        ensure_non_negative(values.invoice_discount, "invoice discount")?;

        let net_invoice = (values.invoice_cost - values.invoice_discount).max(0.0);
        if values.amount > net_invoice {
            bail!(
                "initial payment cannot exceed invoice cost minus discount ({})",
                format_money(net_invoice)
            );
        }
    }

    if transaction_type.needs_account() {
        let account = account_by_id(lookups, values.account_id).context("select an account")?;
        validate_account_type(transaction_type, account)?;
    }

    if transaction_type.needs_source_account() {
        let source =
            account_by_id(lookups, values.source_account_id).context("select a source account")?;
        if source.account_type != "ASSET" {
            bail!("source account must be an asset account");
        }
    }

    if payment_required(values) {
        let payment_method = payment_method_by_id(lookups, values.payment_method_id)
            .context("select a payment method")?;
        validate_payment_details(lookups, values, &payment_method)?;
    }

    Ok(())
}

fn validate_account_type(
    transaction_type: JournalTransactionType,
    account: &ChartAccount,
) -> anyhow::Result<()> {
    match transaction_type {
        JournalTransactionType::Expense if account.account_type != "EXPENSE" => {
            bail!("expense transactions require an expense account")
        }
        JournalTransactionType::Sales
            if account.account_type != "REVENUE" || account.system_account =>
        {
            bail!("sales transactions require a non-system revenue account")
        }
        JournalTransactionType::Transfer if account.account_type != "ASSET" => {
            bail!("transfer destination must be an asset account")
        }
        _ => Ok(()),
    }
}

fn validate_payment_details(
    lookups: &TransactionLookups,
    values: &JournalTransactionValues,
    payment_method: &PaymentMethod,
) -> anyhow::Result<()> {
    match payment_method.name.as_str() {
        "DEPOSIT" | "ZELLE" => {
            let account = account_by_id(lookups, values.payment_account_id)
                .context("select a bank account")?;
            if account.account_type != "BANK" {
                bail!("payment account must be a bank account");
            }
        }
        _ => {}
    }

    if payment_method.name == "ZELLE" {
        if values.zelle_transaction_date.trim().is_empty() {
            bail!("enter the Zelle transaction date");
        }
        if values.zelle_transaction_name.trim().is_empty() {
            bail!("enter the Zelle transaction name");
        }
    }

    if payment_method.name == "CHECK" && values.check_number.trim().is_empty() {
        bail!("enter the check number");
    }

    Ok(())
}

fn payment_required(values: &JournalTransactionValues) -> bool {
    values.transaction_type.needs_payment_method()
        && (!matches!(
            values.transaction_type,
            JournalTransactionType::InitialPayment
        ) || values.amount > 0.0)
}

fn selected_invoice<'a>(
    lookups: &'a TransactionLookups,
    values: &JournalTransactionValues,
) -> anyhow::Result<&'a Invoice> {
    let invoice_id = values
        .invoice_id
        .as_ref()
        .filter(|id| !id.trim().is_empty())
        .context("select an invoice")?;

    lookups
        .invoices
        .iter()
        .find(|invoice| invoice.id_string() == *invoice_id)
        .context("selected invoice is no longer available")
}

fn account_reference(account: &ChartAccount) -> AccountPostReference {
    AccountPostReference {
        id: account.id,
        name: account.label(),
        account_type: account.account_type.clone(),
    }
}

fn account_by_id(lookups: &TransactionLookups, id: Option<u32>) -> Option<&ChartAccount> {
    lookups
        .accounts
        .iter()
        .find(|account| Some(account.id) == id)
}

fn employee_by_id(lookups: &TransactionLookups, id: Option<u16>) -> Option<&Employee> {
    lookups
        .employees
        .iter()
        .find(|employee| Some(employee.id) == id)
}

fn payment_method_by_id(lookups: &TransactionLookups, id: Option<u16>) -> Option<PaymentMethod> {
    lookups
        .payment_methods
        .iter()
        .find(|method| Some(method.id) == id)
        .cloned()
}

fn payment_methods() -> Vec<PaymentMethod> {
    ["CASH", "DEPOSIT", "CHECK", "ZELLE", "CREDIT-CARD"]
        .into_iter()
        .enumerate()
        .map(|(index, name)| PaymentMethod {
            id: index as u16 + 1,
            name: name.into(),
        })
        .collect()
}

fn lookup_pagination() -> Pagination {
    Pagination {
        page: Some(1),
        offset: Some(0),
        limit: Some(LOOKUP_LIMIT),
    }
}

fn ensure_positive(value: f64, label: &str) -> anyhow::Result<()> {
    if value <= 0.0 {
        bail!("{label} must be greater than zero");
    }

    Ok(())
}

fn ensure_non_negative(value: f64, label: &str) -> anyhow::Result<()> {
    if value < 0.0 {
        bail!("{label} cannot be negative");
    }

    Ok(())
}

fn non_empty(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn status_text(status: &str) -> &str {
    if status.trim().is_empty() {
        "not open"
    } else {
        status
    }
}

fn format_money(value: f64) -> String {
    format!("${value:.2}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::income_statement::IncomeStatement;

    #[test]
    fn builds_portal_style_expense_request() {
        let lookups = sample_lookups();
        let values = JournalTransactionValues {
            transaction_type: JournalTransactionType::Expense,
            amount: 42.5,
            employee_id: Some(7),
            account_id: Some(30),
            source_account_id: Some(10),
            description: "Fuel".into(),
            ..Default::default()
        };

        let request = build_post_request(&open_statement(), &lookups, &values)
            .expect("request should be valid");

        assert_eq!(request.transaction_type, "EXPENSE");
        assert_eq!(request.income_statement_id, 32658);
        assert_eq!(request.employee.unwrap().name, "Ada");
        assert_eq!(request.account.unwrap().account_type, "EXPENSE");
        assert_eq!(request.source_account.unwrap().account_type, "ASSET");
        assert!(request.payment_method.is_none());
    }

    #[test]
    fn requires_bank_account_for_zelle_payment() {
        let lookups = sample_lookups();
        let values = JournalTransactionValues {
            transaction_type: JournalTransactionType::Payment,
            amount: 10.0,
            employee_id: Some(7),
            invoice_id: Some("inv-1".into()),
            payment_method_id: Some(4),
            zelle_transaction_date: "2026-09-09".into(),
            zelle_transaction_name: "Zelle Ref".into(),
            payment_account_id: None,
            ..Default::default()
        };

        let error = build_post_request(&open_statement(), &lookups, &values)
            .expect_err("zelle requires bank account");

        assert!(error.to_string().contains("select a bank account"));
    }

    #[test]
    fn rejects_closed_statement_transaction() {
        let mut statement = open_statement();
        statement.status = "closed".into();
        let values = JournalTransactionValues {
            transaction_type: JournalTransactionType::Expense,
            amount: 42.5,
            employee_id: Some(7),
            account_id: Some(30),
            source_account_id: Some(10),
            ..Default::default()
        };

        let error = build_post_request(&statement, &sample_lookups(), &values)
            .expect_err("closed statement should be rejected");

        assert!(error.to_string().contains("open it before adding"));
    }

    fn open_statement() -> IncomeStatement {
        IncomeStatement {
            id: 32658,
            date: "2026-09-09T00:00:00Z".into(),
            branch: None,
            container: None,
            delivery: None,
            rate: 1.0,
            currency: "USD".into(),
            status: "open".into(),
            summary_total: None,
            created_at: None,
            updated_at: None,
        }
    }

    fn sample_lookups() -> TransactionLookups {
        TransactionLookups {
            employees: vec![Employee {
                id: 7,
                name: "Ada".into(),
                active: true,
            }],
            accounts: vec![
                ChartAccount {
                    id: 10,
                    name: "Cash on Hand".into(),
                    display_name: "Cash on Hand".into(),
                    account_type: "ASSET".into(),
                    system_account: true,
                },
                ChartAccount {
                    id: 20,
                    name: "Checking".into(),
                    display_name: "Checking".into(),
                    account_type: "BANK".into(),
                    system_account: false,
                },
                ChartAccount {
                    id: 30,
                    name: "Fuel Expense".into(),
                    display_name: "Fuel Expense".into(),
                    account_type: "EXPENSE".into(),
                    system_account: false,
                },
            ],
            invoices: vec![Invoice {
                id: serde_json::json!("inv-1"),
                number: "A-100".into(),
                cost: 100.0,
                payment: 20.0,
                balance: 80.0,
                discount: 0.0,
            }],
            payment_methods: payment_methods(),
        }
    }
}
