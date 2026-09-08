use std::io::{self, Write};

use clap::{Parser, Subcommand};

use crate::{
    context::AppContext,
    infrastructure::{
        api::EmsysApiClient,
        auth::FirebaseAuthClient,
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
}

#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Sign in with Firebase and save the session securely.
    Login,

    /// Verify the saved authentication session.
    Status,

    /// Remove the saved authentication session.
    Logout,
}

pub async fn run(context: &AppContext, command: Command) -> anyhow::Result<()> {
    match command {
        Command::Version => {
            println!("emsys-cli {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Command::Auth { command } => run_auth(context, command).await,
    }
}

async fn run_auth(context: &AppContext, command: AuthCommand) -> anyhow::Result<()> {
    match command {
        AuthCommand::Login => login(context).await,
        AuthCommand::Status => auth_status(context).await,
        AuthCommand::Logout => logout(context),
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
    println!("Session saved securely");

    Ok(())
}

async fn auth_status(context: &AppContext) -> anyhow::Result<()> {
    let api = EmsysApiClient::new(&context.config);
    let session = api.verify_auth().await?;

    println!("Logged in");
    println!("Firebase UID: {}", session.user_id);
    print_company(&session.company_id);
    println!("EMSYS API verification: OK");

    Ok(())
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
