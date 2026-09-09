use std::io::{self, Write};

use clap::{Args, Parser, Subcommand, ValueEnum};
use serde_json::Value;

use crate::{
    application::income_statement::{
        IncomeStatementService, JournalTransactionType, JournalTransactionValues,
    },
    context::AppContext,
    infrastructure::{
        api::EmsysApiClient,
        auth::FirebaseAuthClient,
        income_statement::{IncomeStatementSearchRequest, SummaryTotalLine},
        query::{Pagination, Sort},
        session::SessionManager,
    },
};

#[derive(Debug, Parser)]
#[command(
    name = "emsys-cli",
    version,
    about = "EMSYS command-line and terminal interface"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Show the current CLI version information.
    Version,

    /// Firebase authentication commands.
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },

    /// Income statement commands.
    Income {
        #[command(subcommand)]
        command: Box<IncomeCommand>,
    },
}

#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Sign in with Firebase and save the session.
    Login,

    /// Verify the saved authentication session.
    Status,

    /// Remove the saved authentication session.
    Logout,
}

#[derive(Debug, Subcommand)]
pub enum IncomeCommand {
    /// Search income statements for the authenticated company.
    Search(IncomeSearchArgs),

    /// Show one income statement with its calculated summary totals.
    Show {
        /// Income statement numeric ID.
        id: u32,
    },

    /// Reopen a closed income statement.
    Open {
        /// Income statement numeric ID.
        id: u32,
    },

    /// Close an open income statement.
    Close {
        /// Income statement numeric ID.
        id: u32,
    },

    /// Add a journal transaction to an income statement.
    AddTransaction(IncomeAddTransactionArgs),
}

#[derive(Debug, Args)]
pub struct IncomeSearchArgs {
    /// Field to filter by, for example status, date, or branch.id.
    #[arg(long)]
    field: Option<String>,

    /// Filter operator such as eq, neq, gt, gte, lt, or lte.
    #[arg(long, default_value = "eq")]
    operator: String,

    /// Filter value. Used only when --field is provided.
    #[arg(long, requires = "field")]
    value: Option<String>,

    /// 1-based result page.
    #[arg(long, default_value_t = 1)]
    page: u64,

    /// Number of results to return.
    #[arg(long, default_value_t = 40)]
    limit: u64,

    /// Sort expression in field:direction form, for example date:desc.
    #[arg(long, default_value = "date:desc")]
    sort: String,
}

#[derive(Debug, Args)]
pub struct IncomeAddTransactionArgs {
    /// Income statement numeric ID.
    #[arg(long)]
    statement_id: u32,

    /// Transaction type, matching the daily income portal add form.
    #[arg(long, value_enum)]
    transaction_type: CliTransactionType,

    /// Transaction amount.
    #[arg(long, default_value_t = 0.0)]
    amount: f64,

    /// Employee numeric ID.
    #[arg(long)]
    employee_id: u16,

    /// Account ID for expense, sales, or transfer destination.
    #[arg(long)]
    account_id: Option<u32>,

    /// Source account ID for expense or transfer.
    #[arg(long)]
    source_account_id: Option<u32>,

    /// Bank account ID for deposit or Zelle payment methods.
    #[arg(long)]
    payment_account_id: Option<u32>,

    /// Existing invoice ID for payment, discount, or surcharge.
    #[arg(long)]
    invoice_id: Option<String>,

    /// New invoice number for initial payment.
    #[arg(long, default_value = "")]
    invoice_number: String,

    /// New invoice cost for initial payment.
    #[arg(long, default_value_t = 0.0)]
    invoice_cost: f64,

    /// New invoice discount for initial payment.
    #[arg(long, default_value_t = 0.0)]
    invoice_discount: f64,

    /// Payment method for payment-related transaction types.
    #[arg(long, value_enum)]
    payment_method: Option<CliPaymentMethod>,

    /// Zelle transaction date, required when payment method is zelle.
    #[arg(long, default_value = "")]
    zelle_transaction_date: String,

    /// Zelle transaction name, required when payment method is zelle.
    #[arg(long, default_value = "")]
    zelle_transaction_name: String,

    /// Check number, required when payment method is check.
    #[arg(long, default_value = "")]
    check_number: String,

    /// Reference number.
    #[arg(long, default_value = "")]
    ref_number: String,

    /// Journal description.
    #[arg(long, default_value = "")]
    description: String,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliTransactionType {
    InitialPayment,
    Payment,
    Expense,
    Sales,
    Discount,
    Surcharge,
    Transfer,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliPaymentMethod {
    Cash,
    Deposit,
    Check,
    Zelle,
    CreditCard,
}

pub async fn run(context: &AppContext, command: Command) -> anyhow::Result<()> {
    match command {
        Command::Version => {
            println!("emsys-cli {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Command::Auth { command } => run_auth(context, command).await,
        Command::Income { command } => run_income(context, *command).await,
    }
}

async fn run_auth(context: &AppContext, command: AuthCommand) -> anyhow::Result<()> {
    match command {
        AuthCommand::Login => login(context).await,
        AuthCommand::Status => auth_status(context).await,
        AuthCommand::Logout => logout(context),
    }
}

async fn run_income(context: &AppContext, command: IncomeCommand) -> anyhow::Result<()> {
    match command {
        IncomeCommand::Search(args) => search_income_statements(context, args).await,
        IncomeCommand::Show { id } => show_income_statement(context, id).await,
        IncomeCommand::Open { id } => set_income_statement_status(context, id, true).await,
        IncomeCommand::Close { id } => set_income_statement_status(context, id, false).await,
        IncomeCommand::AddTransaction(args) => add_income_transaction(context, args).await,
    }
}

async fn login(context: &AppContext) -> anyhow::Result<()> {
    let email = prompt("Email: ")?;
    if email.is_empty() {
        anyhow::bail!("email is required");
    }

    let password = rpassword::prompt_password("Password: ")?;
    if password.is_empty() {
        anyhow::bail!("password is required");
    }

    let auth = FirebaseAuthClient::new(&context.config.firebase);
    let session = auth.sign_in(&email, &password).await?;
    let sessions = SessionManager::new(&context.config.firebase);
    sessions.save(&session)?;

    let api = EmsysApiClient::new(&context.config);
    if let Err(error) = api.verify_auth().await {
        let _ = sessions.clear();
        return Err(error.into());
    }

    println!("Authentication successful");
    println!("User: {}", session.email.as_deref().unwrap_or(&email));
    println!("Firebase UID: {}", session.user_id);
    print_company(&session.company_id);
    println!("EMSYS API verification: OK");
    println!("Session file: {}", sessions.session_file_path().display());

    Ok(())
}

async fn auth_status(context: &AppContext) -> anyhow::Result<()> {
    let api = EmsysApiClient::new(&context.config);
    let session = api.verify_auth().await?;

    println!("Logged in");
    println!("Firebase UID: {}", session.user_id);
    print_company(&session.company_id);
    println!("EMSYS API verification: OK");
    println!(
        "Session file: {}",
        SessionManager::new(&context.config.firebase)
            .session_file_path()
            .display()
    );

    Ok(())
}

async fn search_income_statements(
    context: &AppContext,
    args: IncomeSearchArgs,
) -> anyhow::Result<()> {
    let sort = parse_sort(&args.sort)?;
    let request = IncomeStatementSearchRequest {
        field: args.field,
        operator: args.value.as_ref().map(|_| args.operator),
        value: args.value.map(Value::String),
        pagination: Some(Pagination {
            page: Some(args.page),
            offset: Some(0),
            limit: Some(args.limit),
        }),
        sort: vec![sort],
        ..Default::default()
    };

    let api = EmsysApiClient::new(&context.config);
    let response = api.search_income_statements(&request).await?;

    println!("Income statements: {}", response.total);

    for statement in response.data {
        let branch = statement
            .branch
            .as_ref()
            .map(|branch| branch.name.as_str())
            .unwrap_or("-");
        let net_income = statement
            .summary_total
            .as_ref()
            .map(|total| total.net_income)
            .unwrap_or_default();

        println!(
            "#{} | {} | {} | {} | {} | Net: {:.2}",
            statement.id, statement.date, statement.status, branch, statement.currency, net_income
        );
    }

    Ok(())
}

async fn show_income_statement(context: &AppContext, id: u32) -> anyhow::Result<()> {
    let api = EmsysApiClient::new(&context.config);
    let detail = IncomeStatementService::new(api).show(id).await?;
    let statement = detail.statement;
    let summary = detail.summary;

    let branch = statement
        .branch
        .as_ref()
        .map(|branch| branch.name.as_str())
        .unwrap_or("-");

    println!("Income Statement #{}", statement.id);
    println!("Date: {}", statement.date);
    println!("Status: {}", statement.status);
    println!("Branch: {branch}");
    println!("Currency: {}", summary.currency);
    println!("Rate: {:.4}", summary.rate);
    println!();
    println!("Summary Totals");

    for line in &summary.totals {
        print_summary_total(line, 0);
    }

    Ok(())
}

async fn set_income_statement_status(
    context: &AppContext,
    id: u32,
    open: bool,
) -> anyhow::Result<()> {
    let api = EmsysApiClient::new(&context.config);
    let statement = IncomeStatementService::new(api)
        .set_statement_open(id, open)
        .await?;
    let action = if open { "opened" } else { "closed" };

    println!("Income statement #{} {action}", statement.id);
    println!("Status: {}", statement.status);

    Ok(())
}

async fn add_income_transaction(
    context: &AppContext,
    args: IncomeAddTransactionArgs,
) -> anyhow::Result<()> {
    let api = EmsysApiClient::new(&context.config);
    let service = IncomeStatementService::new(api);
    let statement = service.show(args.statement_id).await?.statement;
    let lookups = service.transaction_lookups().await?;
    let values = transaction_values(args);
    let journal = service
        .post_transaction(&statement, &lookups, &values)
        .await?;

    println!("Journal transaction created");
    println!("Income statement: #{}", statement.id);
    println!("Type: {}", journal.transaction_type);
    println!("Amount: {:.2}", journal.transaction_amount);
    println!("Reference: {}", empty_dash(&journal.ref_number));
    println!("Description: {}", empty_dash(&journal.description));

    Ok(())
}

fn transaction_values(args: IncomeAddTransactionArgs) -> JournalTransactionValues {
    JournalTransactionValues {
        transaction_type: transaction_type(args.transaction_type),
        amount: args.amount,
        ref_number: args.ref_number,
        description: args.description,
        employee_id: Some(args.employee_id),
        account_id: args.account_id,
        payment_account_id: args.payment_account_id,
        source_account_id: args.source_account_id,
        invoice_id: args.invoice_id,
        invoice_number: args.invoice_number,
        invoice_cost: args.invoice_cost,
        invoice_discount: args.invoice_discount,
        payment_method_id: args.payment_method.map(payment_method_id),
        zelle_transaction_date: args.zelle_transaction_date,
        zelle_transaction_name: args.zelle_transaction_name,
        check_number: args.check_number,
    }
}

fn transaction_type(value: CliTransactionType) -> JournalTransactionType {
    match value {
        CliTransactionType::InitialPayment => JournalTransactionType::InitialPayment,
        CliTransactionType::Payment => JournalTransactionType::Payment,
        CliTransactionType::Expense => JournalTransactionType::Expense,
        CliTransactionType::Sales => JournalTransactionType::Sales,
        CliTransactionType::Discount => JournalTransactionType::Discount,
        CliTransactionType::Surcharge => JournalTransactionType::Surcharge,
        CliTransactionType::Transfer => JournalTransactionType::Transfer,
    }
}

fn payment_method_id(value: CliPaymentMethod) -> u16 {
    match value {
        CliPaymentMethod::Cash => 1,
        CliPaymentMethod::Deposit => 2,
        CliPaymentMethod::Check => 3,
        CliPaymentMethod::Zelle => 4,
        CliPaymentMethod::CreditCard => 5,
    }
}

fn print_summary_total(line: &SummaryTotalLine, depth: usize) {
    let indent = "  ".repeat(depth);
    println!("{indent}{}: {:.2}", line.header, line.value);

    for detail in &line.details {
        print_summary_total(detail, depth + 1);
    }
}

fn parse_sort(value: &str) -> anyhow::Result<Sort> {
    let (field, direction) = value
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("sort must use field:direction, for example date:desc"))?;

    if field.trim().is_empty() {
        anyhow::bail!("sort field cannot be empty");
    }

    let direction = direction.trim().to_ascii_lowercase();
    if direction != "asc" && direction != "desc" {
        anyhow::bail!("sort direction must be asc or desc");
    }

    Ok(Sort {
        field: field.trim().to_string(),
        direction,
    })
}

fn print_company(company_id: &Option<String>) {
    match company_id {
        Some(company_id) => println!("Company ID: {company_id}"),
        None => println!("Company ID: not available from Firebase profile"),
    }
}

fn empty_dash(value: &str) -> &str {
    if value.trim().is_empty() { "-" } else { value }
}

fn logout(context: &AppContext) -> anyhow::Result<()> {
    let sessions = SessionManager::new(&context.config.firebase);

    if sessions.clear()? {
        println!("Logged out");
    } else {
        println!("Already logged out");
    }

    Ok(())
}

fn prompt(label: &str) -> io::Result<String> {
    print!("{label}");
    io::stdout().flush()?;

    let mut value = String::new();
    io::stdin().read_line(&mut value)?;
    Ok(value.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_income_statement_sort() {
        let sort = parse_sort("date:desc").expect("valid sort");

        assert_eq!(sort.field, "date");
        assert_eq!(sort.direction, "desc");
    }

    #[test]
    fn rejects_invalid_income_statement_sort_direction() {
        assert!(parse_sort("date:newest").is_err());
    }
}
