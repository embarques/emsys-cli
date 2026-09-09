use clap::Parser;
use emsys_cli::cli::{Cli, Command, IncomeCommand};

#[test]
fn parses_version_command() {
    let cli = Cli::try_parse_from(["emsys-cli", "version"]).expect("version command should parse");
    assert!(matches!(cli.command, Some(Command::Version)));
}

#[test]
fn no_subcommand_selects_tui_mode() {
    let cli = Cli::try_parse_from(["emsys-cli"]).expect("empty command should parse");
    assert!(cli.command.is_none());
}

#[test]
fn parses_income_open_command() {
    let cli = Cli::try_parse_from(["emsys-cli", "income", "open", "32661"])
        .expect("income open command should parse");

    let Some(Command::Income { command }) = cli.command else {
        panic!("expected income command");
    };
    assert!(matches!(*command, IncomeCommand::Open { id: 32661 }));
}

#[test]
fn parses_income_close_command() {
    let cli = Cli::try_parse_from(["emsys-cli", "income", "close", "32661"])
        .expect("income close command should parse");

    let Some(Command::Income { command }) = cli.command else {
        panic!("expected income command");
    };
    assert!(matches!(*command, IncomeCommand::Close { id: 32661 }));
}

#[test]
fn parses_income_add_transaction_command() {
    let cli = Cli::try_parse_from([
        "emsys-cli",
        "income",
        "add-transaction",
        "--statement-id",
        "32661",
        "--transaction-type",
        "payment",
        "--amount",
        "25.50",
        "--employee-id",
        "7",
        "--invoice-id",
        "invoice-1",
        "--payment-method",
        "credit-card",
        "--description",
        "Card payment",
    ])
    .expect("income add-transaction command should parse");

    let Some(Command::Income { command }) = cli.command else {
        panic!("expected income command");
    };
    assert!(matches!(*command, IncomeCommand::AddTransaction(_)));
}
