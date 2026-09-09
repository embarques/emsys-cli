EMSYS CLI - Cursor Rules

1. Overview
- Language: Rust 2024.
- Binary: `emsys-cli`.
- CLI: Clap.
- TUI: Ratatui + Crossterm.
- Async runtime: Tokio.
- HTTP: Reqwest.
- Authentication: Firebase REST.
- Session storage: JSON file through `SessionManager`.
- Backend: EMSYS Go REST API.
- Multi-tenant: authenticated requests eventually include `X-Company-ID`.

2. Project Structure
Keep the existing project structure. Do not redesign it or move folders unless explicitly requested.

```text
emsys-cli/
├── src/
│   ├── application/       # shared use-case orchestration
│   ├── cli/               # command-line presentation
│   ├── domain/            # domain data/rules
│   ├── infrastructure/    # API, Firebase, config, session
│   ├── tui/               # Ratatui UI + terminal events
│   ├── bootstrap.rs
│   ├── context.rs
│   ├── error.rs
│   ├── lib.rs
│   └── main.rs
├── tests/
├── Cargo.toml
└── Cargo.lock
```

Do not introduce parallel top-level folders such as `services/`, `clients/`, `models/`, or `utils/` just to reorganize code.

3. Architecture Rules

| Layer | Location | Responsibility |
| --- | --- | --- |
| Entrypoint | `src/main.rs` | bootstrap and dispatch |
| Bootstrap | `src/bootstrap.rs` | tracing/config/context |
| Context | `src/context.rs` | shared initialized application state |
| CLI | `src/cli/` | Clap args, prompts, terminal output |
| TUI | `src/tui/` | Ratatui rendering/events |
| Application | `src/application/` | shared use cases for CLI and TUI |
| Domain | `src/domain/` | domain types/rules |
| Infrastructure | `src/infrastructure/` | external systems and persistence |

Preferred dependency direction:

```text
CLI ---------┐
             ├── Application ---- Domain
TUI ---------┘         |
                       └── Infrastructure
```

Do not put infrastructure behavior in presentation code.

4. Coding Rules
- Keep Rust idiomatic and simple.
- Prefer concrete structs over unnecessary traits.
- Avoid overengineering and speculative abstractions.
- Keep functions focused.
- Always handle errors.
- Use meaningful names.
- Avoid `unwrap()`/`expect()` in production paths unless failure is provably impossible.
- Prefer established, production-used crates.

5. CLI Rules
CLI commands should stay thin.

Allowed:
- parse arguments
- prompt for input
- call shared services
- format results

Do not:
- call `reqwest` directly from feature commands
- construct Bearer headers in commands
- read session files directly when `SessionManager` exists
- duplicate authentication or tenant logic
- reimplement feature logic separately from the TUI

6. TUI Rules
- TUI is presentation only.
- Never make HTTP requests from render functions.
- Keep rendering deterministic from current app state.
- Centralize terminal enter/restore behavior.
- Preserve the current synchronous TUI shell unless intentionally redesigning the whole event flow.
- Do not accidentally reintroduce an incompatible async `tui::run()` implementation.
- Reuse the same application services as CLI commands.

7. Infrastructure Rules
Current infrastructure responsibilities:
- `config`: environment loading and validation
- `auth`: Firebase REST sign-in/refresh
- `session_store`: file-backed refresh-token storage
- `session`: session refresh orchestration
- `api`: EMSYS HTTP requests

Keep external-system details inside infrastructure.

8. Authentication
Use Firebase REST directly.

Login flow:
```text
email/password
    ↓
Firebase sign-in
    ↓
ID token + refresh token
    ↓
GET /v1/users/me
    ↓
save refresh token in the configured session file
```

Normal request flow:
```text
stored refresh token
    ↓
SessionManager
    ↓
Firebase refresh
    ↓
fresh ID token in memory
    ↓
EmsysApiClient
    ↓
Authorization: Bearer <token>
```

Security:
- never store passwords
- never persist ID tokens
- never log tokens
- never commit Firebase secrets
- never embed Firebase service-account private keys in this CLI

9. EMSYS API
Base URL comes from:
```text
EMSYS_API_URL
```

Rules:
- always use configured API URL
- no hardcoded localhost/public switching
- no direct MongoDB access from Rust
- business rules remain in the Go API
- backend owns tenant authorization, audit logging, and persistence

Base path is `/v1`.

10. Tenant Context
Most feature routes require:
```text
Authorization: Bearer <firebase-id-token>
X-Company-ID: <company-id>
```

Rules:
- company selection must be shared, not implemented independently by commands
- do not scatter `X-Company-ID` request construction
- backend membership checks remain authoritative
- `/v1/users/me` only needs Bearer authentication

11. Configuration
Current environment variables:
```text
EMSYS_API_URL
FIREBASE_WEB_API_KEY
FIREBASE_PROJECT_ID
```

Rules:
- process environment takes precedence over `.env`
- `.env` is local-only and gitignored
- `.env.example` contains names/placeholders only
- never hardcode secrets

12. Error Handling
- Use `thiserror` for typed infrastructure/library errors.
- Use `anyhow` at command/application boundaries where appropriate.
- Error messages should be actionable but must not expose secrets.

13. Logging
- Use `tracing` for diagnostics.
- User-facing CLI output should remain concise.
- Never log passwords, ID tokens, refresh tokens, or credential-store contents.

14. Dependency Rules
Before adding a crate:
1. Confirm there is a concrete requirement.
2. Check whether std or an existing dependency already solves it.
3. Prefer mature production-used crates.
4. Keep enabled features minimal.
5. Avoid two libraries for the same responsibility.

15. Agent Working Rules
- Inspect the current branch and relevant files before large edits.
- Preserve existing architecture.
- Do not invent API contracts; inspect the EMSYS API implementation when needed.
- Keep feature logic reusable by both CLI and TUI.
- Refactor duplicated auth/session behavior into shared services rather than copying it.
- Do not claim a Cargo validation command passed unless it was actually run by the agent, CI, or confirmed by the user.
- If local/remote Git history diverges, inspect commits before recommending conflict resolution.
- Never blindly recommend `ours`/`theirs` for conflict resolution.

16. Commands After Changes
Run:
```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

If formatting is needed:
```bash
cargo fmt
```

Authentication changes may additionally be verified manually with:
```bash
cargo run -- auth login
cargo run -- auth status
cargo run -- auth logout
```

17. Git Workflow
- Never make feature changes directly on `main`.
- Never make feature changes directly on `dev`.
- Use dedicated feature/fix branches from `dev`.
- Keep feature history clean.
- Avoid unnecessary merge commits.
- Open a pull request to `dev` after feature completion for user verification.

18. Prohibited
- No direct MongoDB connection from Rust
- No credentials or `.env` committed
- No password storage
- No persistent ID-token storage
- No secret logging
- No HTTP calls from TUI render code
- No duplicated auth/tenant logic in individual feature commands
- No unnecessary DI framework
- No workspace conversion unless explicitly requested
- Do not rename the executable; it remains `emsys-cli`
- Do not accidentally restore the old incompatible async TUI flow
