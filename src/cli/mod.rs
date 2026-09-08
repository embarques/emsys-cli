use std::io::{self, Write};

use clap::{Parser, Subcommand};

use crate::{
    context::AppContext,
    infrastructure::{
        api::EmsysApiClient,
        auth::FirebaseAuthClient,
        credentials::CredentialStore,
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
        AuthCommand::Logout => logout(),
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

    let api = EmsysApiClient::new(&context.config.api_url);
    api.verify_auth(&session.id_token).await?;

    CredentialStore::new().save_refresh_token(&session.refresh_token)?;

    println!("Authentication successful");
    println!("User: {}", session.email.as_deref().unwrap_or(&email));
    println!("Firebase UID: {}", session.user_id);
    println!("EMSYS API verification: OK");
    println!("Session saved securely");

    Ok(())
}

async fn auth_status(context: &AppContext) -> anyhow::Result<()> {
    let credentials = CredentialStore::new();
    let refresh_token = credentials
        .load_refresh_token()?
        .ok_or_else(|| anyhow::anyhow!("not logged in; run `emsys-cli auth login`"))?;

    let auth = FirebaseAuthClient::new(&context.config.firebase);
    let session = auth.refresh(&refresh_token).await?;

    if session.refresh_token != refresh_token {
        credentials.save_refresh_token(&session.refresh_token)?;
    }

    let api = EmsysApiClient::new(&context.config.api_url);
    api.verify_auth(&session.id_token).await?;

    println!("Logged in");
    println!("Firebase UID: {}", session.user_id);
    println!("EMSYS API verification: OK");

    Ok(())
}

fn logout() -> anyhow::Result<()> {
    if CredentialStore::new().clear_refresh_token()? {
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
