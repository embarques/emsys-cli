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

```bash
cargo run -- income search --field status --value open
cargo run -- income show 32661
cargo run -- income close 32661
cargo run -- income open 32661
cargo run -- income add-transaction \
  --statement-id 32661 \
  --transaction-type expense \
  --amount 42.50 \
  --employee-id 7 \
  --account-id 30 \
  --source-account-id 10 \
  --description "Fuel"
```

## Quality checks

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```
