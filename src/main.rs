use clap::Parser;
use emsys_cli::{bootstrap, cli, tui};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    bootstrap::init_tracing();

    let args = cli::Cli::parse();
    let context = bootstrap::build_context();

    match args.command {
        Some(command) => cli::run(&context, command).await,
        None => tui::run(context),
    }
}
