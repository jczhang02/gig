# gig GUI Phase C Pre-GUI Backend Checkpoint

Date: 2026-05-30

## Scope

Phase C completes the backend/action work needed before starting formal GUI frontend development. It stays read-only from the GUI perspective and does not add React/Vite/shadcn product UI, mutation buttons, or CLI behavior migration.

## Delivered

- `ActionContext` and `ActionContextSeed` in `gig-core::actions` for request-scoped action execution dependencies.
- Executable read action dispatcher:
  - `dashboard.get`
  - `orders.list`
  - `orders.get_detail`
  - `config.redacted.get`
- Core view DTOs for dashboard, orders, order detail, and redacted config responses.
- Core redaction of config secrets, including short-link token, S3 access key, and S3 secret key.
- Core `preflight_action` skeleton for all catalog actions.
- Protected API routes:
  - `POST /api/actions/:id/preview`
  - `POST /api/actions/:id/execute`
- Generic execute route intentionally accepts only read actions for now. Mutate, external-I/O, and dangerous actions return `unsupported_action` instead of executing.
- Existing GUI read endpoints now route through `gig-core::actions` instead of directly calling repos/services:
  - `/api/dashboard` → `dashboard.get`
  - `/api/orders` → `orders.list`
  - `/api/orders/:id` → `orders.get_detail`
  - `/api/config` → `config.redacted.get`
- Follow-up hardening after parallel review:
  - read-only GUI action requests use `db::open_readonly`, do not create/migrate the database, and set SQLite `query_only=ON`;
  - GUI action/domain errors map to 400/404 instead of collapsing into 500;
  - `delivery.package.check` and file-writing export metadata are no longer presented as pure reads;
  - preflight warnings now include database, file, external network, upload, and editor side effects;
  - `orders.get_detail` read action accepts id, slug, or current Order context;
  - Short Link/S3 endpoints require HTTPS except loopback local development;
  - config directories/files are permission-hardened on Unix;
  - browser token handoff uses a fragment, removes it from the address bar after storage, and `--no-open` prints the token separately from the redacted URL.

## Invariants kept

- No React/Vite/shadcn frontend work started.
- No GUI mutation UI added.
- No mutating, external-I/O, or dangerous action execution exposed through the GUI API.
- Existing CLI command handlers, human output, JSON output, and JSON error semantics were not migrated or changed.
- GUI server state still stores cloneable `Paths`, redacted-capable `Config`, `cwd`, and token only; no shared `rusqlite::Connection` is stored in Axum state.
- Every executed action request opens a fresh read-only SQLite connection inside `spawn_blocking`, builds one `ActionContext`, executes, and drops the connection before returning.
- Missing or stale databases surface as errors; read-only GUI requests do not initialize or migrate local state.
- GUI routes still do not shell out to `gig`, `sh`, or `Command::new(current_exe)`.

## Verification

Passing:

```bash
cargo fmt --check
cargo test -q
cargo clippy --workspace --all-targets -- -D warnings
```

Focused tests added/updated:

```bash
cargo test -q -p gig-core actions
cargo test -q -p gig-gui
```

Manual smoke:

```text
open=http://127.0.0.1:<port>/
health={"status":"ok","app":"gig-gui"}
unauth_actions=401
actions_count=51
auth_config=200
auth_dashboard=200
auth_preview=200
auth_execute_dashboard=200
wrong_input=400
mutating_execute=400
```

## Ready to start formal GUI frontend development

The next phase can start the real GUI interface work: crate-local React/Vite/TypeScript/shadcn frontend, product navigation, cockpit layout, action runner forms, and browser smoke tests. Mutation execution should still wait until per-action preflight/confirmation and audit logging are designed and tested.
