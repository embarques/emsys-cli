use std::io::{self, Write};

use clap::{Args, Parser, Subcommand};
use serde_json::Value;

use crate::{
    application::income_statement::IncomeStatementService,
    context::AppContext,
    infrastructure::{
        api::EmsysApiClient,
        auth::FirebaseAuthClient,
        income_statement::{IncomeStatementSearchRequest, Pagination, Sort, SummaryTotalLine},
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
        command: IncomeCommand,
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

pub async fn run(context: &AppContext, command: Command) -> anyhow::Result<()> {
    match command {
        Command::Version => {
            println!("emsys-cli {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Command::Auth { command } => run_auth(context, command).await,
        Command::Income { command } => run_income(context, command).await,
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
