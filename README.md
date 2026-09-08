# emsys-cli

Rust command-line and terminal user interface for EMSYS.

## Development

```bash
cargo run
```

Running without a subcommand starts the Ratatui interface.

```bash
cargo run -- version
```

Running with a subcommand executes the CLI action and exits.

## Quality checks

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```
