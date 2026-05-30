# gig Local GUI Companion Implementation Plan

> **2026-05-30 scope update:** The GUI stays invoked as `gig gui`, but implementation must be isolated in `crates/gig-gui`. `crates/gig-cli` may only add a thin subcommand/dispatch entrypoint. Existing CLI command handlers, output rendering, JSON error semantics, and workflow gates must not be migrated as part of the first GUI slice. Phase A is read-only and may use shared `gig-core` services/repos directly before a full typed-action migration.

> **2026-05-30 Phase A checkpoint:** Read-only `gig gui` is now implemented and hardened with API/security tests. See `docs/superpowers/plans/2026-05-30-gig-gui-phase-a-checkpoint.md`.

> **2026-05-30 Phase B checkpoint:** The `gig-core::actions` metadata skeleton and protected `GET /api/actions` endpoint are implemented. See `docs/superpowers/plans/2026-05-30-gig-gui-phase-b-actions.md`.

> **2026-05-30 Phase C pre-GUI backend checkpoint:** Executable read actions, request-scoped `ActionContext`, preflight/execute API skeletons, and core-routed GUI read endpoints are implemented. See `docs/superpowers/plans/2026-05-30-gig-gui-phase-c-pre-gui-backend.md`. The next phase may begin formal frontend GUI development; keep mutation execution behind preflight/confirmation and audit-log work.

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:test-driven-development` for behavior changes, `superpowers:verification-before-completion` before claiming completion, and `coding-standards` for Rust/TypeScript edits. Frontend implementation must use `design-taste-frontend`, `vercel-react-best-practices`, and `vercel-composition-patterns`.

## Goal

Build `gig gui` as a localhost Web GUI companion for personal operation of `gig`. The GUI must cover the full CLI capability set, but it must not execute shell-composed `gig` commands. The fundamental implementation is to move command behavior into typed `gig-core` actions, then make the existing CLI and the new GUI call the same action layer.

## Non-Goals

- Do not build a hosted SaaS or multi-user app.
- Do not expose the GUI on LAN or public interfaces.
- Do not implement a terminal TUI.
- Do not use a CLI runner or `sh -c` as the GUI execution path.
- Do not change existing CLI command names, flags, human output, JSON output, or error semantics except where explicitly captured by updated tests.
- Do not reimplement package validation, upload, short-link, workflow, or lifecycle rules in GUI code.

## Current Evidence

- `crates/gig-core/src/lib.rs` says core owns SQLite schema, models, repositories, and services, with no CLI/UI dependency.
- `crates/gig-core/src/services/mod.rs` says services are the only layer the CLI and eventual desktop should call.
- `crates/gig-cli/src/main.rs` owns clap parse, JSON error printing, process exit, and brittle string-based JSON error-code classification.
- `crates/gig-cli/src/dispatch.rs` owns the common `Paths::from_env`, directory creation, database opening, and the full command match, but some handlers still call `Paths::from_env` / `Config::load_or_default` directly and must be swept explicitly.
- `crates/gig-cli/src/commands/mod.rs` still owns reusable order resolution and delivery uploader construction.
- `crates/gig-cli/src/cli.rs` defines the full command surface and JSON-error eligibility.
- `crates/gig-cli/tests/workflow_cli.rs` is the main CLI behavior preservation surface for workflow JSON and package behavior; older human-output commands need extra smoke coverage before broad dispatch migration.
- Existing workspace has only `crates/gig-core` and `crates/gig-cli`; no web package exists yet.
- Official/current docs support Axum router state/middleware, `tokio::net::TcpListener::bind`, `tower-http` `ServeDir`, Vite production builds, and shadcn/ui Vite React setup.

## Target Architecture

```text
crates/gig-core
  actions/                 # single action model for CLI and GUI
    mod.rs
    catalog.rs             # ActionMeta registry
    context.rs             # ActionContext construction inputs and per-call DB lifecycle
    types.rs               # ActionId, ActionKind, FieldSchema, outputs, errors
    read.rs                # list/show/dashboard/client/source/stats/export
    orders.rs              # order lifecycle actions
    workflow.rs            # quote/plan/acceptance actions
    delivery.rs            # package/artifact/uploaders actions
    maintenance.rs         # doctor/backup/import/delete/config/templates

crates/gig-cli
  cli.rs                   # clap only
  dispatch.rs              # maps clap args -> core actions -> existing renderers
  commands/*               # migrate toward thin adapters/renderers
  ui.rs                    # terminal formatting only

crates/gig-gui
  src/lib.rs               # server entry callable from gig-cli
  src/server.rs            # Axum app, localhost bind, graceful shutdown
  src/auth.rs              # ephemeral token extractor/middleware
  src/api.rs               # action catalog and execute routes
  src/static_assets.rs     # dev/prod web asset serving
  web/                     # React/Vite/TS/shadcn frontend
```

`gig-core` is the only business layer. `gig-cli` and `gig-gui` are adapters. GUI reads action metadata from core, renders forms/confirmations, and posts typed action requests to the local API. The Axum server state must store only cloneable paths/config/token/catalog data, not a shared `rusqlite::Connection`; handlers open a fresh connection per request inside `spawn_blocking` or through a tiny blocking executor wrapper so no SQLite connection crosses async await points.

## Action Model

### Core Types

- `ActionId`: stable string-backed ids such as `orders.create`, `dashboard.get`, `delivery.package.send`.
- `ActionKind`: `Read`, `Mutate`, `ExternalIo`, `Dangerous`.
- `ActionMeta`: id, CLI path, label, description, field schema, output kind, confirmation policy, side effects, JSON support, workflow-critical marker.
- `ActionContext`: explicit dependencies for one synchronous action execution: `Connection`, `Paths`, `Config`, `cwd`, clock, uploader factory, and confirmation/dry-run policy. It is built per CLI invocation or per GUI API request; it is not stored in Axum state.
- `ActionRequest`: action id plus typed payload. Use strongly typed per-action Rust structs internally; serde envelope is only for API transport.
- `ActionOutput`: view models preserving current CLI JSON keys where those keys already exist.
- `ActionError`: maps existing `gig_core::Error` to stable GUI/API error codes without changing CLI JSON error output. The current `workflow_error_code` / `classify_invalid_error` logic from `gig-cli/src/main.rs` must move into one shared core classifier verbatim first, with golden tests for each current code bucket before any error message refactor.

### Catalog Coverage

Workflow-critical actions:

- `app.bootstrap`
- `dashboard.get`
- `orders.list`
- `orders.get_detail`
- `orders.create`
- `orders.price.update`
- `orders.requirement_change.add`
- `orders.notes.append`
- `orders.tags.add`
- `orders.cut_ratio.update`
- `orders.status.set`
- `orders.payment.mark_paid`
- `orders.archive`
- `leads.list`
- `leads.promote`
- `leads.drop`
- `quotes.create`
- `quotes.get`
- `quotes.list`
- `quotes.price`
- `quotes.mark_sent`
- `quotes.accept`
- `quotes.drop`
- `workflow.plan.ready`
- `workflow.plan.approve`
- `workflow.plan.reject`
- `workflow.acceptance.check`
- `workflow.acceptance.complete`
- `delivery.package.send`
- `delivery.artifacts.send`
- `delivery.uploaders.list`
- `stats.get`
- `sources.list`
- `sources.create`
- `clients.list`
- `clients.get_detail`

Long-tail full-CLI coverage actions:

- `orders.dev_path.get`
- `orders.export`
- `orders.delete`
- `system.doctor.run`
- `system.backup.create`
- `projects.import`
- `templates.list`
- `templates.get`
- `templates.save_user_copy`
- `config.get`
- `config.set`
- `config.file.get_path`
- `cli.completion.generate`

## Frontend Product Shape

The GUI is a personal operator cockpit, not a generic SaaS dashboard.

Screens:

1. Dashboard: focus orders, quote drafts, monthly summary, next actions, risk alerts.
2. Order Detail: order fields, workflow state, notes, changes, delivery links, next action panel.
3. Action Runner: command palette exposing the full action catalog, including long-tail CLI coverage.
4. Delivery Panel: package send, artifact send, uploader selection, short-link result, confirmation for external I/O.
5. Doctor/Health: path and workflow diagnostics, optional confirmed fix actions.
6. Settings: read/edit supported config keys without exposing secrets; redact sensitive values.
7. Onboarding/Empty State: first-use checklist, config status, uploader status, workflow guide link, example commands.
8. Audit Log: local GUI action history with redacted secrets.

Stack:

- Rust server: Axum + Tokio + tower-http.
- Frontend: React + Vite + TypeScript + shadcn/ui + Tailwind.
- Data fetching: TanStack Query.
- Forms: react-hook-form + zod.
- Notifications: sonner.
- Icons: lucide-react.
- Tables: shadcn Table first; TanStack Table only when sorting/filtering needs exceed basic table.

## Security Model

- `gig gui` binds only to `127.0.0.1`.
- Use port `0` by default so the OS assigns a free localhost port; optionally allow `--port` later only for localhost.
- Generate a per-process random token on startup.
- Open browser with `http://127.0.0.1:<port>/?token=<token>`.
- Frontend stores token in memory/session storage and sends `Authorization: Bearer <token>`.
- API rejects missing/wrong token with `401`.
- Do not implement accounts, passwords, cookies, LAN binding, CORS permissive defaults, or remote access in v0.
- Redact `access_key`, `secret_key`, short-link token, and any env-derived secret from API responses and audit logs.

## Acceptance Criteria

1. `gig gui` starts a localhost server on `127.0.0.1`, prints/opens a URL containing a one-time token, and does not bind to `0.0.0.0`.
2. Requests to protected API routes without the token return `401`; requests with the token succeed.
3. Protected API handlers do not store or share a `rusqlite::Connection` in Axum state; tests prove request handlers open/use DB safely through the action executor.
4. The GUI can read action metadata from `gig-core` and display workflow-critical plus long-tail actions.
5. Existing CLI commands continue to pass current tests and retain current output/JSON behavior, including JSON error-code buckets.
6. At least one read action, one local mutation action, one external-I/O action, and one dangerous action execute through `gig-core` action APIs and are used by both CLI adapter tests and GUI API tests.
7. No GUI route shells out to `gig`, `sh`, or `Command::new(current_exe)` for action execution.
8. `cargo test --workspace`, `cargo clippy --workspace --all-targets`, frontend build, and browser smoke test all pass.
9. Manual QA runs `gig gui`, opens the page, verifies dashboard load, verifies token enforcement, and executes a safe test action against isolated temp XDG homes.

## Implementation Plan

### Phase 0: Preserve Current State

1. Record current status and existing dirty worktree. Do not include unrelated docs changes unless user explicitly wants them committed.
2. Run baseline verification: `rtk cargo fmt --check`, `rtk cargo clippy --workspace --all-targets`, `rtk cargo test --workspace`.
3. Add or update golden CLI tests before refactoring where output behavior is not currently pinned: JSON error classification, `show --json`, `ls --json`, workflow JSON, package/artifact JSON.
4. Pin JSON error-code buckets before moving classification: `invalid_transition`, `unsafe_package_path`, `missing_package_artifact`, `acceptance_incomplete`, `legacy_workflow_metadata_missing`, `missing_workflow_file`, `invalid_project_type`, `missing_required_field`.
5. Add preservation smoke for human-output commands likely to be touched by action migration: `new`, `price`, `change`, `note`, `tag`, `cut`, `paid`, `archive --yes`, `client`, `source`, `delete --yes`.

QA: Baseline commands pass or pre-existing failures are documented before production changes.

### Phase 0.5: Extract Shared Error and Confirmation Contracts

1. Move the current CLI JSON error classifier from `gig-cli/src/main.rs` into `gig-core` without changing the string matching or returned codes.
2. Make `gig-cli/src/main.rs` call the shared classifier.
3. Add `ConfirmationPolicy` and `ActionPreflight` types before migrating commands.
4. Inventory every stdin/confirmation command: `status`, `archive`, `delete`, `import --interactive`, `import --force`, `config edit`, `template edit`.
5. Define that core actions never block on stdin or launch editors. They return preflight/confirmation metadata; CLI adapters may still prompt or open `$EDITOR` to preserve UX.

QA: Existing JSON errors are byte-compatible, and confirmation metadata tests cover at least `status`, `archive`, and `delete`.

### Phase 1: Add Core Action Skeleton Without Behavior Migration

1. Add `crates/gig-core/src/actions/` module tree.
2. Add `ActionId`, `ActionKind`, `ActionMeta`, `ActionField`, `ConfirmationPolicy`, `SideEffect`, and catalog listing all commands.
3. Add unit tests that assert catalog includes every `cli.rs` command/subcommand path.
4. Expose `pub mod actions;` from `gig-core/src/lib.rs`.
5. Keep CLI dispatch unchanged.

QA: Catalog tests pass; CLI tests still pass; no command behavior changed.

### Phase 2: Add Action Context and Shared Helpers

1. Add `ActionContext` and a core context builder that accepts explicit `Paths`, `Config`, `Connection`, `cwd`, and clock for one synchronous execution.
2. Move optional id/context order resolution from `crates/gig-cli/src/commands/mod.rs` into core action helper.
3. Move delivery uploader construction from `crates/gig-cli/src/commands/mod.rs` into core delivery/action helper.
4. Move money/status/project-type parsing into core-friendly helpers while preserving CLI input strings.
5. Sweep all handlers for direct `Paths::from_env`, `Config::load_or_default`, current-directory, and clock calls; replace or wrap them so the action layer owns dependencies.
6. Add tests for context resolution, uploader construction, and parser compatibility.

QA: Existing CLI behavior remains unchanged, new core helper tests cover CLI-equivalent inputs, and `rtk grep -n "Paths::from_env|Config::load_or_default|now_utc|stdin|read_line" crates/gig-cli/src` has only documented adapter exceptions.

### Phase 2.5: Early End-to-End Vertical Slice

1. Implement `dashboard.get` through the action skeleton before migrating the whole CLI.
2. Add a minimal `crates/gig-gui` server exposing `GET /api/health`, `GET /api/actions`, and `GET /api/dashboard` with localhost token protection.
3. Add a minimal React/Vite/shadcn shell that loads dashboard data and displays an operator-cockpit empty state.
4. Keep this slice read-only. Do not add mutation UI yet.

QA: `gig gui --no-open` can serve the minimal dashboard from temp XDG homes, token enforcement works, and no CLI behavior has changed. This reduces late integration risk before the full action migration.

### Phase 3: Migrate Simple Order Actions

1. Implement action wrappers for `orders.price.update`, `orders.requirement_change.add`, `orders.notes.append`, `orders.tags.add`, `orders.cut_ratio.update`, `orders.payment.mark_paid`, `orders.status.set`, `leads.promote`, `leads.drop`, `orders.delete`.
2. Convert CLI handlers one at a time to call actions, then render existing output.
3. Replace stdin confirmation in migrated actions with core preflight/confirmation metadata; CLI adapters keep the existing prompt and `--yes` behavior.
4. Add action tests for success, invalid transition/confirmation, and context fallback.

QA: CLI tests pass after each migrated command group; action tests prove same core path.

### Phase 4: Migrate Read/View Actions

1. Implement `dashboard.get`, `orders.list`, `orders.get_detail`, `clients.list`, `clients.get_detail`, `sources.list`, `stats.get`.
2. Move CLI aggregation logic for show/client/list into core view models.
3. Keep terminal rendering in CLI only.
4. Add JSON shape tests for view outputs.

QA: `gig ls`, `gig ls --json`, `gig show --json`, client/source/stats commands preserve behavior.

### Phase 5: Migrate Workflow and Delivery Actions

1. Implement action wrappers for all `quotes.*`, `workflow.plan.*`, `workflow.acceptance.*`.
2. Implement `delivery.package.send`, `delivery.artifacts.send`, and `delivery.uploaders.list` through core uploader factory.
3. Preserve `next_action` values and frozen JSON fields.
4. Add tests for package validation, artifact upload records, short-link wrapping, and same-second object key uniqueness through action APIs.

QA: Current workflow CLI tests pass; new action tests cover the same behavior without CLI command execution.

### Phase 6: Migrate Maintenance and Long-Tail Actions

1. Implement structured `system.doctor.run`, moving CLI-only package/workflow diagnostics into core.
2. Implement `system.backup.create`.
3. Implement `orders.archive` including dirty git warning as structured preflight plus confirmed execution; CLI keeps existing `--yes` and warning behavior.
4. Implement `orders.export` returning content or writing output file through explicit output path.
5. Implement `projects.import` as preview/execute flow; replace stdin prompts with structured fields for GUI, while CLI keeps interactive adapter.
6. Implement `templates.*` and `config.*` without launching editors from core. CLI can still launch editor as adapter behavior.
7. Implement `cli.completion.generate` as CLI-only or catalog-visible read action.

QA: Full CLI command surface is represented in catalog; dangerous actions require confirmation metadata.

### Phase 7: Add `crates/gig-gui` Local Server

1. Add workspace member `crates/gig-gui`.
2. Add dependencies: `axum`, `tokio`, `tower-http`, `serde`, `serde_json`, `rand`, and an HTTP test client if needed.
3. Implement `run_gui_server(options)` callable from `gig-cli`.
4. Implement Axum state with only cloneable request-independent data: `Paths`, redacted `Config`/config path, database path, action catalog, token, static-dir/dev-proxy settings. Do not store `rusqlite::Connection` in server state.
5. Implement a blocking action executor: each API request validates token, clones state, opens a fresh SQLite connection from `db_path` inside `tokio::task::spawn_blocking`, builds one `ActionContext`, executes one action, then drops the connection before returning JSON.
6. Routes:
   - `GET /api/health`
   - `GET /api/actions`
   - `POST /api/actions/:id/preview`
   - `POST /api/actions/:id/execute`
   - `GET /api/dashboard`
   - `GET /api/audit-log`
7. Add token middleware/extractor.
8. Add static asset serving with `tower_http::services::ServeDir`; in dev, allow Vite dev URL/proxy only if explicitly configured.

QA: Server tests prove localhost binding, token rejection, action catalog response, isolated temp XDG DB use, and that handlers do not require a shared `Connection` in Axum state.

### Phase 8: Add `gig gui` CLI Adapter

1. Add `Gui(GuiArgs)` to `crates/gig-cli/src/cli.rs`.
2. Add flags: `--no-open`, optional `--port`, optional `--dev-web-url` for development only.
3. Dispatch completion stays pre-DB; `gui` builds server context through core/bootstrap and starts `gig-gui`.
4. Print the local URL and token-bearing open URL. Do not print secrets from config.

QA: `gig gui --no-open` starts server, prints localhost URL, and responds to health check.

### Phase 9: Add React/Vite/shadcn Frontend

1. Create `crates/gig-gui/web` with Vite React TypeScript, matching the user-approved crate-local GUI organization. If shadcn tooling expects project-root aliases, configure aliases explicitly in `components.json`, `vite.config.ts`, and `tsconfig.json` instead of moving the frontend to the workspace root.
2. Configure Tailwind and shadcn/ui with aliases.
3. Add core components: `AppShell`, `DashboardPage`, `OrderDetailPanel`, `ActionRunner`, `ConfirmActionDialog`, `DoctorPanel`, `SettingsPanel`, `AuditLogPanel`, `EmptyState`.
4. Add API client that attaches bearer token and uses TanStack Query.
5. Add zod schemas matching generated/catalog field metadata.
6. Implement personal operator cockpit layout: left order/action queue, center detail, right action/risk/command panel.
7. Keep shadcn as component substrate, not visual identity. Avoid generic SaaS card wall.

QA: `npm run build` or chosen package-manager build succeeds; static dist is served by `gig gui`.

### Phase 10: Action Execution UX

1. Render catalog actions in Command Palette / Action Runner.
2. High-frequency workflow actions get dedicated panels; long-tail commands use generated forms.
3. Show equivalent CLI command for every action.
4. Require confirmation for `Mutate`, `ExternalIo`, and `Dangerous` actions according to metadata.
5. Show execution result, errors, and audit-log entry after each action.
6. Redact secret fields in UI and logs.
7. Never allow dashboard auto-refresh or page load to execute a mutating, external-I/O, or dangerous action.

QA: Manual browser QA executes one read action and one safe mutation action against isolated temp data.

### Phase 11: Audit Log

1. Add local GUI audit log storage under XDG state, not the main order schema unless needed.
2. Record timestamp, action id, order id/slug if present, equivalent CLI, risk kind, result status, error summary, and redacted payload.
3. Expose `GET /api/audit-log` and display in GUI.

QA: Tests prove secret redaction and log persistence for success/failure.

### Phase 12: Final Verification and Documentation

1. Add README section for `gig gui`.
2. Update workflow HTML only if necessary; do not rewrite it again unless user asks.
3. Run Rust formatting, clippy, workspace tests.
4. Run frontend lint/build if configured.
5. Run browser smoke test against `gig gui`.
6. Run manual CLI smoke for representative commands: `gig ls`, `gig show --json`, `gig package send --json` test path if safe/mocked, `gig gui --no-open` health check.

QA: All acceptance criteria pass with actual command output captured.

## Review Checklist

- Does every CLI command map to an action id or explicit CLI-only exception?
- Does every GUI execution path use core actions, not shell command execution?
- Does every Axum handler open/use DB through the request-scoped blocking action executor rather than shared connection state?
- Did the JSON error classifier move once into core with golden tests for all current buckets?
- Did every direct `Paths::from_env`, config load, current time, stdin prompt, editor launch, and direct process call in CLI get classified as adapter-only or moved behind core actions?
- Are dangerous actions confirmable and auditable?
- Are existing CLI outputs preserved by tests?
- Are secrets redacted in API, UI, logs, and docs?
- Does the GUI stay localhost-only with token protection?
- Are package/artifact/S3/short-link rules still implemented only in core?
- Are generated web assets reproducible and documented?

## Initial Work Split

- Core lane: Phase 1 through Phase 6 action model and migration.
- CLI lane: adapter migration and output preservation tests.
- GUI server lane: Phase 2.5, Phase 7, and Phase 8.
- Frontend lane: Phase 2.5, Phase 9, and Phase 10.
- QA lane: golden tests, action tests, API tests, browser smoke, final verification.

## Known Risks

- This is a large architecture migration; keep phases incremental even if the Ralph loop goal is end-to-end completion.
- Late integration is a major risk; keep the Phase 2.5 read-only vertical slice mandatory rather than waiting for all actions to migrate.
- SQLite is synchronous; never hold a `rusqlite::Connection` across async awaits or in Axum state.
- Current JSON error codes depend on string matching; moving classifier without golden tests can silently break consumers.
- `config edit` and `template edit` currently launch an editor; GUI needs structured alternatives while CLI keeps editor behavior.
- `status`, `archive`, `delete`, and `import --interactive/--force` currently use prompts; GUI needs preview/confirm instead of stdin.
- Archive and package/artifact send have real filesystem/network side effects; tests must isolate or mock them.
- Existing uncommitted `docs/gig-workflow-usage-kami.html` must not be accidentally included in GUI commits unless requested.
