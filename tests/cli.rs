use clap::Parser;
use emsys_cli::cli::{Cli, Command};

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
