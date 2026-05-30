# gig GUI Phase A Checkpoint

Date: 2026-05-30

## Scope

Phase A is a read-only `gig gui` checkpoint. It proves the localhost GUI shell can safely read existing `gig-core` data without changing existing CLI command handlers, CLI output, JSON error semantics, or workflow gates.

Out of scope for this checkpoint:

- `gig-core::actions` catalog and execution model.
- GUI mutations.
- Delivery/package send actions.
- React/Vite frontend buildout.
- Fixing unrelated pre-existing clippy warnings in `gig-core` tests/workflow code.

## Delivered

- `crates/gig-gui` Axum library invoked only through thin `gig gui` CLI entrypoint.
- Localhost-only bind with per-process bearer token.
- Read-only endpoints:
  - `GET /api/health`
  - `GET /api/dashboard`
  - `GET /api/orders`
  - `GET /api/orders/:id`
  - `GET /api/config` with secret redaction.
- Embedded minimal cockpit page.
- Focused API/security tests for public health, dashboard token rejection, dashboard token success, and config secret redaction.
- Root `CONTEXT.md` glossary for the delivery workflow terms used by CLI, GUI, and planning docs.

## Verification

Passing:

```bash
cargo fmt --check
cargo test -q
```

Observed test result after Phase A hardening:

```text
229 passed, 5 ignored
```

Manual smoke:

```bash
cargo run -q -p gig-cli -- gui --no-open --port 0
curl http://127.0.0.1:<port>/api/health
curl http://127.0.0.1:<port>/api/dashboard
curl -H "Authorization: Bearer <token>" http://127.0.0.1:<port>/api/dashboard
```

Observed result:

```text
health={"status":"ok","app":"gig-gui"}
unauth_dashboard=401
auth_dashboard=200
```

Follow-up cleanup after the initial checkpoint restored the clippy gate:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

The cleanup removed unused `client_dir` variables in existing `client_package.rs` tests, replaced the Rust 1.82-only `Option::is_none_or` call in `workflow.rs` with Rust 1.75-compatible code, and adjusted Phase A GUI code to satisfy clippy's strict warnings.

## Next phase

Phase B should add the `gig-core::actions` skeleton: stable action ids, action metadata catalog, confirmation/preflight types, and catalog coverage tests. Keep existing CLI handlers and output behavior unchanged while the catalog lands.
