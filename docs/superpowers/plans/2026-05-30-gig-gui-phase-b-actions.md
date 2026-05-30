# gig GUI Phase B Actions Skeleton

Date: 2026-05-30

## Scope

Phase B adds the shared action metadata skeleton that later CLI and GUI adapters will execute through. It does not migrate existing CLI command handlers and does not add GUI mutations.

Out of scope for this checkpoint:

- Executing actions through `gig-core::actions`.
- Moving CLI rendering or JSON error classification.
- Mutation buttons in the GUI.
- React/Vite frontend buildout.

## Delivered

- `crates/gig-core/src/actions/` module tree.
- Stable string-backed `ActionId`.
- `ActionKind` classes: read, mutate, external I/O, dangerous.
- `ActionMeta` catalog entries for the current CLI command/subcommand surface.
- `ActionField`, `ConfirmationPolicy`, `ActionPreflight`, `SideEffect`, and `ActionOutputKind` contracts.
- Catalog lookup helpers: `actions::all`, `actions::find_by_id`, `actions::find_by_cli_path`.
- Catalog tests for unique ids/CLI paths, required Phase B contracts, and coverage against the current `crates/gig-cli/src/cli.rs` command/subcommand paths.
- Protected `GET /api/actions` endpoint in `gig-gui` returning the core catalog.
- Embedded read-only GUI now fetches `/api/actions` and shows the number of action metadata entries available.

## Invariants kept

- Existing CLI dispatch remains unchanged except the previously added thin `gig gui` entrypoint.
- Existing CLI command handlers still own behavior; no command was migrated in Phase B.
- GUI routes still do not shell out to `gig`, `sh`, or `Command::new(current_exe)`.
- Axum state still stores cloneable path/config/token data, not a shared `rusqlite::Connection`.

## Verification

Passing:

```bash
cargo fmt --check
cargo test -q
cargo clippy --workspace --all-targets -- -D warnings
```

Focused checks added in this phase:

```bash
cargo test -q -p gig-core actions
cargo test -q -p gig-gui
```

Manual smoke should still pass:

```bash
gig gui --no-open --port 0
curl http://127.0.0.1:<port>/api/health
curl http://127.0.0.1:<port>/api/actions
curl -H "Authorization: Bearer <token>" http://127.0.0.1:<port>/api/actions
curl -H "Authorization: Bearer <token>" http://127.0.0.1:<port>/api/dashboard
```

Observed result:

```text
health={"status":"ok","app":"gig-gui"}
unauth_actions=401
actions_count=49
auth_dashboard=200
```

## Next phase

Phase C should introduce executable read actions through the shared action layer, starting with `dashboard.get`, `orders.list`, and `orders.get_detail`. Keep the CLI output unchanged and route the GUI read endpoints through the same core execution path only after tests pin the existing JSON/human behavior.
