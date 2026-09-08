# AGENTS.md

## EMSYS CLI

### Purpose

EMSYS CLI is a Rust command-line and terminal user interface for working with the EMSYS platform.

The project uses Rust 2024, Clap, Ratatui, Crossterm, Tokio, Reqwest, Firebase REST authentication, and the EMSYS REST API.

The architecture and coding conventions in this file are mandatory.

When making changes, preserve the existing structure and coding patterns.

---

# High-Level Rules

* Follow the existing architecture.
* Do not redesign the application unless explicitly requested.
* Do not move folders unless explicitly requested.
* Do not introduce unnecessary abstractions, traits, frameworks, or dependency-injection libraries.
* Keep the project as a single crate unless explicitly requested otherwise.
* Keep code idiomatic Rust.
* Prefer simple concrete types over generic abstractions when one implementation is sufficient.
* Keep functions small and focused.
* Handle errors explicitly.
* Use meaningful names.
* Do not duplicate authentication, API, or tenant logic in CLI/TUI presentation code.
* Prefer well-established production-used crates.

---

# Repository Structure

Preserve the current layout.

```text
emsys-cli/
├── src/
│   ├── application/       # application-level orchestration/services
│   ├── cli/               # Clap commands and CLI presentation
│   ├── domain/            # domain types and domain rules
│   ├── infrastructure/    # EMSYS HTTP, Firebase, config, credentials
│   ├── tui/               # Ratatui presentation and terminal event handling
│   ├── bootstrap.rs       # application startup wiring
│   ├── context.rs         # shared application context
│   ├── error.rs           # shared application errors where appropriate
│   ├── lib.rs
│   └── main.rs
├── tests/
├── Cargo.toml
└── Cargo.lock
```

Do not create alternative top-level architectures such as `services/`, `clients/`, `models/`, or `utils/` unless they clearly fit the existing structure and are explicitly justified.

---

# Architecture

| Layer | Location | Responsibility |
| --- | --- | --- |
| Entrypoint | `src/main.rs` | parse CLI, bootstrap context, dispatch CLI or TUI |
| Bootstrap | `src/bootstrap.rs` | initialize tracing/config/context |
| Shared context | `src/context.rs` | application-wide initialized dependencies/config |
| CLI | `src/cli/` | Clap parsing, prompts, command output |
| TUI | `src/tui/` | Ratatui rendering, terminal events, user interaction |
| Application | `src/application/` | feature orchestration shared by CLI and TUI |
| Domain | `src/domain/` | domain data and rules independent of transport/UI |
| Infrastructure | `src/infrastructure/` | HTTP, Firebase, secure credentials, environment config |

Dependency direction should remain approximately:

```text
CLI ---------┐
             ├── Application ---- Domain
TUI ---------┘         |
                       └── Infrastructure
```

Presentation code must not own infrastructure behavior.

---

# CLI Rules

CLI commands should remain thin.

CLI code may:

* Parse arguments.
* Prompt the user.
* Call application or infrastructure services when the command is infrastructure-specific, such as authentication.
* Format command output.

CLI code must not:

* Call `reqwest` directly.
* Build Firebase REST requests directly.
* Read or write keyring credentials directly when a session abstraction exists.
* Manually construct authorization headers for normal EMSYS commands.
* Duplicate domain/business rules.

Feature commands such as `income show` should call shared application services that are reusable by the TUI.

---

# TUI Rules

The TUI is a presentation layer.

Rules:

* Keep rendering functions free of network calls.
* Never perform HTTP requests from Ratatui render functions.
* Keep terminal setup/restore centralized.
* Preserve the existing synchronous event shell unless a deliberate async event architecture is introduced for a concrete feature.
* Do not reintroduce an incompatible async `tui::run()` flow without updating the entrypoint and event architecture together.
* CLI and TUI should share application services rather than implementing feature logic twice.

---

# Application Rules

Application services coordinate use cases.

They may:

* Call infrastructure clients.
* Combine authentication, tenant context, and feature API operations.
* Transform API results into domain types.
* Provide operations shared by CLI and TUI.

They should not:

* Render terminal UI.
* Parse Clap arguments.
* Read stdin directly.
* Contain Reqwest-specific request construction when that belongs in infrastructure.

Do not introduce traits merely to imitate enterprise architecture. Add interfaces only when there is an actual testing, substitution, or design need.

---

# Infrastructure Rules

Infrastructure contains external integration details.

Current responsibilities include:

* `config` — environment configuration.
* `auth` — Firebase REST sign-in and token refresh.
* `credentials` — OS secure credential storage.
* `session` — saved-session refresh orchestration.
* `api` — EMSYS REST API communication.

Rules:

* Keep Firebase-specific logic inside auth/session infrastructure.
* Keep keyring-specific logic inside credential infrastructure.
* Keep HTTP request construction inside API/auth infrastructure.
* Reuse long-lived `reqwest::Client` instances where practical.
* Normalize `EMSYS_API_URL` once rather than scattering URL manipulation.
* Never print or log tokens or passwords.

---

# Authentication

Authentication uses Firebase REST directly.

Do not use the EMSYS API development token endpoint as the primary CLI authentication mechanism.

Expected flow:

```text
emsys-cli auth login
        ↓
Firebase sign-in
        ↓
ID token + refresh token
        ↓
verify with GET /v1/users/me
        ↓
save refresh token securely
```

Normal authenticated request flow:

```text
saved refresh token
        ↓
Firebase refresh
        ↓
fresh ID token in memory
        ↓
Authorization: Bearer <token>
        ↓
EMSYS API
```

Security rules:

* Passwords are never stored.
* ID tokens are memory-only.
* Refresh tokens are stored using the OS credential store.
* Never commit Firebase secrets.
* Never log ID tokens, refresh tokens, passwords, or credential-store contents.
* Firebase service-account JSON/private keys must never be embedded in this CLI.

---

# Tenant / Company Context

The EMSYS API is multi-tenant.

Most protected feature endpoints require:

```text
Authorization: Bearer <firebase-id-token>
X-Company-ID: <company-id>
```

Rules:

* Tenant selection belongs in shared application/infrastructure context, not individual feature commands.
* Commands must not manually add `X-Company-ID` in multiple places.
* The backend remains authoritative for tenant membership and authorization.
* Never assume a company header alone grants access.

`GET /v1/users/me` is used to verify authentication and does not require tenant context.

---

# EMSYS API Rules

The CLI communicates with the Go EMSYS API over HTTPS/JSON.

Configured base URL:

```text
EMSYS_API_URL
```

Rules:

* Always use the configured API URL.
* Do not add localhost/public-environment switching logic unless explicitly requested.
* Do not connect directly to MongoDB.
* Do not move business rules from the Go API into the Rust CLI.
* The API owns database access, authorization, audit logging, and server-side business rules.

Base API path is `/v1`.

---

# Configuration

Current required variables:

```text
EMSYS_API_URL
FIREBASE_WEB_API_KEY
FIREBASE_PROJECT_ID
```

Rules:

* `.env` is for local development only.
* Existing process environment must take precedence over `.env` values.
* Never commit `.env`.
* Maintain `.env.example` without secrets.
* Do not hardcode production URLs or Firebase credentials in source code.

---

# Error Handling

Use `thiserror` for typed library/infrastructure errors and `anyhow` where appropriate at application/command boundaries.

Rules:

* Do not ignore actionable errors.
* Error messages should explain the failed operation without exposing secrets.
* Avoid `unwrap()` and `expect()` in production paths unless failure is provably impossible and documented.
* Tests may use `expect()` when it improves readability.

---

# Logging

Use `tracing`.

Rules:

* Do not log secrets.
* Avoid noisy logs for normal CLI output.
* CLI user-facing output should remain intentional and concise.
* Infrastructure diagnostics should use tracing rather than ad-hoc debug printing.

---

# Dependencies

Prefer stable, widely adopted, production-used crates.

Current core crates include:

* `clap`
* `ratatui`
* `crossterm`
* `tokio`
* `reqwest`
* `serde`
* `anyhow`
* `thiserror`
* `tracing`
* `tracing-subscriber`
* `dotenvy`
* `rpassword`
* `keyring`

Before adding a crate:

1. Confirm there is a concrete need.
2. Prefer the existing standard-library or current dependency solution when adequate.
3. Avoid introducing duplicate libraries for the same purpose.
4. Keep feature flags minimal.

---

# Testing

Prefer unit tests for parsing, validation, transformations, and application logic.

Tests must not:

* Depend on production data.
* Write to production systems.
* Require a real user credential unless explicitly running a manual E2E check.
* Print secrets.

Authentication E2E checks against the configured EMSYS/Firebase environment are manual unless a safe isolated test environment is explicitly provided.

---

# Commands Before Completing Work

Run:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

If formatting changes are needed, run:

```bash
cargo fmt
```

For authentication changes, manually verify when appropriate:

```bash
cargo run -- auth login
cargo run -- auth status
cargo run -- auth logout
```

Do not claim these checks passed unless they were actually executed or confirmed by the user/CI.

---

# Git Workflow

* Never make feature changes directly on `main`.
* Never make feature changes directly on `dev`.
* Use a dedicated feature/fix branch.
* Feature branches should normally originate from `dev`.
* Preserve clean commit history.
* Avoid unnecessary merge commits on feature branches.
* When local and remote feature-branch commits diverge, inspect history before resolving; do not blindly use `ours`/`theirs` during conflicts.
* After completing a feature, create a pull request targeting `dev` so the user can validate before merge.

---

# Pull Request Expectations

Before completing a feature:

1. Preserve the architecture.
2. Keep CLI and TUI behavior consistent.
3. Add or update tests where useful.
4. Run formatting, Clippy, and tests.
5. Update documentation when behavior or configuration changes.
6. Explain significant design decisions.
7. Do not claim validation that has not actually occurred.

---

# Prohibited

Do not:

* Connect the Rust CLI directly to MongoDB.
* Store passwords.
* Store ID tokens persistently.
* Print or log tokens.
* Commit `.env` files or Firebase secrets.
* Put network calls inside TUI render functions.
* Duplicate API/auth/tenant logic in individual commands.
* Introduce heavy frameworks for dependency injection or application architecture.
* Convert the repository into a workspace without explicit instruction.
* Rename the executable from `emsys-cli`.
* Reintroduce an incompatible async TUI entry flow accidentally.
* Modify `main` or `dev` directly for feature work.
