use clap::{Parser, Subcommand};

use crate::context::AppContext;

#[derive(Debug, Parser)]
#[command(name = "emsys-cli", version, about = "EMSYS command-line and terminal interface")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Show the current CLI version information.
    Version,
}

pub async fn run(_context: &AppContext, command: Command) -> anyhow::Result<()> {
    match command {
        Command::Version => {
            println!("emsys-cli {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}
