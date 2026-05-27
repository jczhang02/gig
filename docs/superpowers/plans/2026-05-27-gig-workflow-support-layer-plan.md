# gig Workflow Support Layer Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Use superpowers:test-driven-development for every behavior change.

**Goal:** Refactor `gig` from a mostly order-centric Rust CLI into a dual-use support layer for the approved `partjob-workflow`: human-friendly in daily use, stable enough for the workflow skill to call automatically. `partjob-workflow` remains the workflow driver; `gig` records state, indexes paths, exposes expected workflow locations, validates files that the workflow skill or agent created, gates transitions, and records client-package metadata. `gig` does not generate workflow files.

**Architecture:** Keep the current Rust two-crate split. `gig-core` owns schema, models, repositories, services, path contracts, validation rules, and safety checks. `gig-cli` stays a thin `clap` shell that calls `gig_core::services::*` and formats output. Command names, human CLI output, machine output, status labels, and view-model fields all stay English by default. Phase 1 does not include TUI or GUI work. Workflow documents and HTML pages are produced by `partjob-workflow` or agents, not by `gig`.

**Tech Stack:** Rust 1.75+, `rusqlite`, `refinery`, `serde`, `toml`, `time`, `thiserror`, `directories`, `clap`, `comfy-table`, `owo-colors`, `tempfile`, `minijinja`, `ignore`, `arboard`, existing upload dependencies. Add `serde_json` only if implementing `--json` output in this plan.

**Specs:**

- `docs/superpowers/specs/2026-05-26-partjob-workflow-protocol-design.md`
- `docs/superpowers/specs/2026-05-26-gig-quote-to-delivery-refactor-design.md`
- `~/.agents/skills/partjob-workflow/SKILL.md`
- `~/.agents/skills/partjob-workflow/resources/file-contracts.md`

**Status:** Design freeze checklist added; implementation not started. This plan is the next artifact before Rust edits.

---

## Design Decision

`gig` becomes a management substrate, not the whole workflow.

```text
partjob-workflow skill = 主流程入口
gig = 管理工具 / 状态、索引、路径、校验、安全记录
workflow files = skill / agent / user interface
agent = execution worker
JC = commercial and delivery decision-maker
```

The critical design choice is **stable English tool language**:

- Keep `gig` in Rust. Do not rewrite it to TypeScript for this refactor.
- Command names, DB enum values, JSON fields, skill-facing output, and human CLI output use stable English: `quote_draft`, `quoted`, `plan_ready`, `plan_approved`, `ready_to_deliver`.
- Do not add Chinese command names or Chinese default status labels in v1. The `partjob-workflow` skill can speak Chinese to JC; `gig` itself stays an English CLI tool.
- Do not implement TUI or GUI in phase 1. Future TUI/GUI should be a thin UI shell over `gig-core` services or stable `--json`/view-model output, not a reason to move core logic into TypeScript.

Quote drafts must be separate from formal orders. Do not reuse `orders.status = lead` for pre-deal opportunities. The user explicitly does not want a formal project/order before the client accepts.

---

## Design Freeze Checklist

This section is the implementation gate. If implementation discovers a contradiction, update this section first; do not silently resolve it in Rust code.

### Frozen Ownership Boundary

- `partjob-workflow` remains the workflow driver and user-facing orchestration surface.
- `partjob-workflow` or agents create Markdown, HTML, prompt files, delivery files, and client archives.
- `gig` records state, expected paths, timestamps, package metadata, and validation results.
- `gig` must not generate, copy, render, scaffold, refresh, zip, upload, or send workflow artifacts.
- `gig-cli` is a thin adapter. Workflow invariants live in `gig-core` services.

### Frozen State Machines

Quote draft states:

```text
quote_draft -> needs_clarification -> quote_draft
quote_draft -> quoted -> accepted
needs_clarification -> quoted -> accepted
quote_draft | needs_clarification | quoted -> dropped
```

Rules:

- `accepted` is terminal for a quote draft and must set `promoted_order_id`.
- `dropped` is terminal for a quote draft and requires non-empty `drop_reason`.
- New workflow code must not create `orders.status = lead` for pre-acceptance work.

Formal order workflow states:

```text
accepted -> plan_ready -> plan_approved -> in_progress -> ready_to_deliver -> delivered -> paid -> archived
plan_ready -> accepted  # plan rejected; reason required; revised plan can be marked ready again
```

Additional order rules:

- `cancelled` remains an allowed manual terminal state from any non-terminal state except `archived`.
- `revision` remains supported for legacy/manual flows and dashboard display, but is not part of the first automated `partjob-workflow` happy path.
- Existing `lead`, `negotiating`, `in_progress`, `delivered`, `revision`, `paid`, `archived`, and `cancelled` rows must keep parsing.
- Existing `gig status` remains a manual override command. `partjob-workflow` should prefer gated workflow commands for new states.

Delivery package states:

```text
prepared -> validated -> sent
prepared | validated -> cancelled
```

Rules:

- `sent` records that a package was sent outside `gig`; `gig` must not send it.
- Package validation may record an existing workflow-created zip, but must not create one.

### Frozen Transition Gates

| Command | From | To | Required evidence | Must not do |
| --- | --- | --- | --- | --- |
| `gig quote new` | none | `quote_draft` | title, summary, project type, expected XDG quote path | create project dir or workflow files |
| `gig quote price` | `quote_draft`, `needs_clarification` | `quoted` | quote range and recommendation | mark accepted |
| `gig quote mark-sent` | `quoted` | `quoted` | sent timestamp | send anything externally |
| `gig quote accept` | `quoted` | quote `accepted`, order `accepted` | workflow-created XDG `JOB.md` and `QUOTE.md`, accepted project root path | create `.gig/` files or project scaffolding |
| `gig quote drop` | `quote_draft`, `needs_clarification`, `quoted` | `dropped` | non-empty reason | delete quote files |
| `gig plan ready` | `accepted` | `plan_ready` | workflow-created `.gig/plan/PLAN.md` and `.gig/plan/PLAN.html` | render plan or index HTML |
| `gig plan approve` | `plan_ready` | `plan_approved` | explicit approval | infer approval from file presence |
| `gig plan reject` | `plan_ready` | `accepted` | non-empty rejection reason | delete or rewrite plan files |
| `gig status <id> in_progress` | `plan_approved` | `in_progress` | manual user confirmation or existing command semantics | bypass plan approval in workflow path |
| `gig acceptance check` | `in_progress` | no state change | `.gig/acceptance/ACCEPTANCE.md` and required evidence headings | infer quality from arbitrary text |
| `gig acceptance complete` | `in_progress` | `ready_to_deliver` | passing acceptance check | refresh `.gig/INDEX.html` |
| `gig package check` | `ready_to_deliver` | package `validated` | workflow-created manifest and client dir | create archives |
| `gig package record` | `ready_to_deliver` | package `prepared` or `validated` | existing package metadata | send or upload package |
| `gig package mark-sent` | package `validated` | package `sent` | explicit external-send confirmation | send anything externally |

### Frozen `--json` Contract

Successful workflow command output must use stable English keys and enum values. The minimum common fields are:

```json
{
  "status": "plan_ready",
  "next_action": "approve_plan",
  "paths": {
    "plan_html_path": "/home/jc/dev/partjobs/foo/.gig/plan/PLAN.html"
  }
}
```

Rules:

- Do not localize JSON keys, enum values, status values, next actions, or error codes.
- Prefer additive JSON changes. Do not rename or repurpose fields after release.
- Human output is not an API; `partjob-workflow` should consume `--json`.
- New workflow command failures with `--json` should emit machine-readable errors to stderr and exit non-zero.

Frozen error codes for new workflow commands:

```text
invalid_project_type
missing_required_field
missing_workflow_file
stale_workflow_index
invalid_transition
acceptance_incomplete
unsafe_package_path
missing_package_artifact
legacy_workflow_metadata_missing
```

### Frozen Client Manifest Contract

`gig package check` validates a workflow-created manifest under `.gig/delivery/<YYYY-MM-DD>/`. Phase 1 should use TOML because `gig` already depends on TOML.

```toml
version = 1
delivery_date = "YYYY-MM-DD"
client_files = ["DELIVERY_CLIENT.html", "DELIVERY_CLIENT.pdf"]
```

Rules:

- `client_files` are paths relative to `.gig/delivery/<YYYY-MM-DD>/client/`.
- Absolute paths, `..`, hidden paths, `.gig/`, `prompts/`, `internal/`, `ACCEPTANCE.md`, and `DELIVERY_INTERNAL.html` are invalid.
- The canonical existing archive path is `.gig/delivery/<YYYY-MM-DD>/export/client-package.zip`.
- `gig` may validate and record that archive if it already exists; it must not create it.

### Frozen Legacy Compatibility

- Existing orders without `order_workflow` rows must still load, list, show, export, and archive.
- Missing workflow metadata should appear as `legacy_workflow_metadata_missing` in machine-readable diagnostics, not as a panic.
- `gig init` must not be extended as a workflow scaffolder. Keep legacy/manual file creation separate from the `partjob-workflow` path and document any remaining behavior in help text.
- Existing `gig pack` and `gig deliver` remain legacy/manual commands. New client-package safety work must use `gig package check|record|mark-sent` instead of root project packing.
- Backfill of `project_type` for old orders can be a later helper; do not block this plan on full migration cleanup.

### First Implementation Slice

The first vertical slice remains:

```text
V002 schema + models + quote draft repo/service + gig quote new/show/list/drop
```

This slice must prove the XDG quote-draft design without creating project directories or workflow files.

---

## Out Of Scope

- No direct model invocation from `gig`.
- No WeCom/企业微信 automation.
- No sending messages or files to clients.
- No TypeScript rewrite in this phase.
- No TUI or GUI in this phase.
- No Chinese command aliases or Chinese default CLI labels in v1.
- No default project-root `input/`, `work/`, `output/` scaffolding.
- No root-level workflow files in formal projects.
- No workflow file generation from `gig`: no rendering, copying, scaffolding, or refreshing `JOB.md`, `QUOTE.md`, `PLAN.md`, `PLAN.html`, `ACCEPTANCE.md`, `DELIVERY.md`, `DELIVERY_*.html`, or `.gig/INDEX.html`.
- No client-package archive generation from `gig`; `gig` validates and records packages created by the workflow.
- No raw project packing for client delivery.
- No weakening of existing legacy commands unless this plan explicitly replaces behavior.

---

## Current Patterns To Preserve

- `gig-core` must remain independent from CLI/UI.
- `gig-cli` should parse args, call services, and format results.
- Repositories perform SQL mapping only; business rules live in services.
- New schema changes go through refinery migrations.
- Tests can live inline under `#[cfg(test)]`, matching existing core modules.
- Existing orders and historical price data must remain readable.
- Legacy states must keep parsing so older rows do not break.

---

## File Structure

### Create

```text
crates/gig-core/src/db/migrations/V002__workflow_support.sql
crates/gig-core/src/models/project_type.rs
crates/gig-core/src/models/quote_draft.rs
crates/gig-core/src/models/order_workflow.rs
crates/gig-core/src/models/delivery_package.rs
crates/gig-core/src/repo/quote_drafts.rs
crates/gig-core/src/repo/order_workflow.rs
crates/gig-core/src/repo/delivery_packages.rs
crates/gig-core/src/services/quote.rs
crates/gig-core/src/services/workflow_paths.rs
crates/gig-core/src/services/workflow.rs
crates/gig-core/src/services/client_package.rs
crates/gig-cli/src/commands/quote.rs
crates/gig-cli/src/commands/plan.rs
crates/gig-cli/src/commands/acceptance.rs
crates/gig-cli/src/commands/package.rs
```

### Modify

```text
crates/gig-core/Cargo.toml
crates/gig-core/src/config.rs
crates/gig-core/src/db/mod.rs
crates/gig-core/src/models/mod.rs
crates/gig-core/src/models/order.rs
crates/gig-core/src/repo/mod.rs
crates/gig-core/src/services/mod.rs
crates/gig-core/src/services/orders.rs
crates/gig-core/src/services/init.rs
crates/gig-core/src/services/dashboard.rs
crates/gig-cli/Cargo.toml
crates/gig-cli/src/cli.rs
crates/gig-cli/src/dispatch.rs
crates/gig-cli/src/commands/mod.rs
crates/gig-cli/src/commands/init.rs
crates/gig-cli/src/commands/ls.rs
crates/gig-cli/src/commands/show.rs
crates/gig-cli/src/commands/deliver.rs
crates/gig-cli/src/ui.rs
```

Only add `serde_json` to manifests if the JSON tasks below are implemented in this same pass.

---

## Data Model

### Project Types

Add `ProjectType` as a stable enum in `crates/gig-core/src/models/project_type.rs`:

```text
automation_script
data_processing
crawler
cv_ml
frontend_web
research_writing
custom
```

Add nullable `project_type TEXT` to `orders` for existing rows, and a required `project_type TEXT` on quote drafts. Do not block existing old orders that do not yet have a type.

### Quote Drafts

Create `quote_drafts` for pre-deal opportunities:

```sql
CREATE TABLE quote_drafts (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  slug TEXT NOT NULL UNIQUE,
  title TEXT NOT NULL,
  client_label TEXT,
  source_org TEXT,
  project_type TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'quote_draft',
  summary TEXT NOT NULL,
  quote_min INTEGER,
  quote_recommended INTEGER,
  quote_max INTEGER,
  currency TEXT NOT NULL DEFAULT 'CNY',
  xdg_path TEXT NOT NULL,
  drop_reason TEXT,
  promoted_order_id INTEGER REFERENCES orders(id),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  quoted_at TEXT,
  accepted_at TEXT,
  dropped_at TEXT
);
```

Allowed quote statuses:

```text
quote_draft
needs_clarification
quoted
accepted
dropped
```

Pre-acceptance files live only under XDG:

```text
$XDG_DATA_HOME/gig/quotes/<quote-id>/
├── JOB.md
├── QUOTE.md
└── prompts/
```

### Order Workflow Metadata

Create `order_workflow` rather than bloating `orders`:

```sql
CREATE TABLE order_workflow (
  order_id INTEGER PRIMARY KEY REFERENCES orders(id) ON DELETE CASCADE,
  project_type TEXT,
  gig_dir TEXT,
  index_path TEXT,
  job_path TEXT,
  quote_path TEXT,
  plan_md_path TEXT,
  plan_html_path TEXT,
  plan_ready_at TEXT,
  plan_approved_at TEXT,
  plan_rejected_at TEXT,
  plan_rejection_reason TEXT,
  acceptance_path TEXT,
  acceptance_completed_at TEXT,
  latest_delivery_dir TEXT,
  latest_client_package_path TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

Extend `OrderStatus` while preserving legacy parse support:

```text
lead
negotiating
accepted
plan_ready
plan_approved
in_progress
ready_to_deliver
delivered
revision
paid
archived
cancelled
```

`Lead` and `Negotiating` remain for old/manual flows, but the new workflow should use `quote_drafts` before acceptance.

### Delivery Packages

Create `delivery_packages` to record safe client exports:

```sql
CREATE TABLE delivery_packages (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  order_id INTEGER NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
  delivery_date TEXT NOT NULL,
  delivery_dir TEXT NOT NULL,
  client_dir TEXT NOT NULL,
  manifest_path TEXT NOT NULL,
  package_path TEXT,
  status TEXT NOT NULL DEFAULT 'prepared',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

Allowed package statuses:

```text
prepared
validated
sent
cancelled
```

---

## Workflow File Contract

### Formal Project Layout

After quote acceptance:

```text
<project>/
├── real work files, organized by JC
└── .gig/
    ├── INDEX.html
    ├── JOB.md
    ├── QUOTE.md
    ├── plan/
    │   ├── PLAN.md
    │   └── PLAN.html
    ├── acceptance/
    │   └── ACCEPTANCE.md
    ├── delivery/
    │   └── <YYYY-MM-DD>/
    │       ├── DELIVERY.md
    │       ├── client/
    │       │   ├── DELIVERY_CLIENT.html
    │       │   └── DELIVERY_CLIENT.pdf
    │       ├── internal/
    │       │   └── DELIVERY_INTERNAL.html
    │       └── export/
    │           └── client-package.zip
    └── prompts/
```

Rules:

- Project root contains only real work files.
- Client originals count as real work files.
- Canonical status page path: `<project>/.gig/INDEX.html`.
- Do not create root `JOB.md`, `PLAN.md`, `ACCEPTANCE.md`, or `DELIVERY.md`.
- Do not default-create `input/`, `work/`, or `output`.
- Do not default-create `input/`, `work/`, or `output/`.
- `.gig/INDEX.html` is a human-facing navigation/status page, not source of truth.
- `partjob-workflow` or agents generate and update `.gig/INDEX.html`; `gig` only records the expected path, exposes state via `--json`, and validates whether the file exists or appears stale.
- All workflow file content is owned by `partjob-workflow` / agents. `gig` must not render, copy, scaffold, or refresh these files.

---

## CLI Contract

### Human Output

Default command output should remain English and decision-oriented. Example labels:

```text
needs_clarification
quote_ready
waiting_client
plan_missing
plan_ready
plan_approved
in_progress
acceptance_missing
ready_to_deliver
delivered_unpaid
archived
```

Use English for table titles, status labels, next actions, and blockers. Keep text compact and action-oriented. Chinese belongs in the surrounding `partjob-workflow` agent conversation, not in `gig`'s default CLI output.

### Machine Output

Add `--json` to new workflow commands first. Add `--json` to `ls` and `show` if this plan includes dashboard integration.

Machine output must use stable internal fields:

```json
{
  "status": "plan_ready",
  "next_action": "approve_plan",
  "plan_html_path": "/home/jc/dev/partjobs/foo/.gig/plan/PLAN.html"
}
```

Do not localize JSON keys or enum values.

### New Command Groups

```text
gig quote new
gig quote show
gig quote list
gig quote price
gig quote mark-sent
gig quote accept
gig quote drop

gig plan ready
gig plan approve
gig plan reject

gig acceptance check
gig acceptance complete

gig package check
gig package record
gig package mark-sent
```

`gig init` should move toward registration/validation rather than scaffolding. Quote acceptance should prefer `gig quote accept` because it knows the quote draft, formal order, expected XDG paths, and expected `.gig/` layout, but it still must not create workflow files itself.

---

## Service Design

### `services::quote`

Responsibilities:

- Record/update/list quote drafts.
- Store the expected XDG quote directory path supplied by the workflow.
- Validate whether workflow-created quote files exist when a transition requires them.
- Record quote range and recommendation after the skill produces it.
- Accept a quote draft and create a formal order.
- Drop a quote draft with a required reason.

Must not:

- Create `/home/jc/dev/partjobs/<slug>` before acceptance.
- Create XDG directories or quote files.
- Generate `JOB.md`, `QUOTE.md`, `PLAN.md`, or `PLAN.html`.
- Treat `Lead` orders as quote drafts.

### `services::workflow_paths`

Responsibilities:

- Build canonical XDG and `.gig/` paths.
- Record expected workflow file locations in `order_workflow`.
- Validate that workflow-created directories and files exist when state transitions require them.
- Expose path/status data for `.gig/INDEX.html` generation through `--json` or service view models.
- Mark `.gig/INDEX.html` stale/fresh in metadata if needed.

Must not:

- Create root-level workflow files.
- Create `.gig/` directories or workflow files.
- Copy, expand, render, or refresh any Markdown/HTML workflow file.
- Create default real-work directories.

### `services::workflow`

Responsibilities:

- Validate order workflow transitions.
- Record plan ready, plan approved, acceptance complete, ready to deliver, delivered.
- Store paths, timestamps, and plan rejection reason in `order_workflow`.
- Refuse execution-facing transitions when required files or approvals are missing.

Must not:

- Decide whether an agent's work is actually good.
- Read arbitrary evidence and infer acceptance by itself. It can check declared file/path completeness.

### `services::client_package`

Responsibilities:

- Read workflow-created delivery manifests.
- Validate that listed client-visible files live under `.gig/delivery/<date>/client/`.
- Validate an existing `.gig/delivery/<date>/export/client-package.zip` if the workflow created one.
- Record delivery package metadata and sent/cancelled status.
- Reject internal paths and forbidden filenames.

Blocklist must include:

```text
.gig/
prompts/
internal/
ACCEPTANCE.md
DELIVERY_INTERNAL.html
raw evidence
internal workflow files
```

This service must not call `pack_order(project_dir)` for client delivery. It must not create archives, write manifests, or copy package files in phase 1; packaging is owned by `partjob-workflow` / agents, while `gig` validates and records the result.

Do not use existing `pack_order(project_dir)` for client delivery.

---

## Dashboard Design

`gig ls` should become a decision dashboard by default.

Decision items should include:

- Quote drafts missing client facts: `needs_clarification`
- Quote drafts ready for price review: `quote_ready`
- Quoted opportunities waiting for client: `waiting_client`
- Accepted orders with no plan: `plan_missing`
- `plan_ready` orders: `plan_review`
- `plan_approved` orders not started: `ready_to_start`
- `in_progress` orders with missing acceptance evidence: `acceptance_missing`
- `ready_to_deliver` orders: `delivery_review`
- `delivered` unpaid orders: `payment_due`
- Revision feedback: `revision_scope_review`

`gig ls --all` should still show a full table. `gig ls --json` should emit all dashboard items with internal status and next action fields.

---

## Implementation Tasks

### Phase 1: Schema And Models

- [ ] Write a failing migration test proving V002 creates `quote_drafts`, `order_workflow`, `delivery_packages`, and `orders.project_type`.
- [ ] Add `V002__workflow_support.sql` with the new tables and column.
- [ ] Run the migration test and confirm it fails for the intended reason before implementation, then passes after adding V002.
- [ ] Add `ProjectType` model with parse/serialize tests for the 7 built-ins plus `custom`.
- [ ] Add `QuoteDraftStatus` and `QuoteDraft` model tests.
- [ ] Add `OrderWorkflow` model tests for path fields and timestamps.
- [ ] Add `DeliveryPackage` model tests.
- [ ] Update `models/mod.rs` exports.
- [ ] Extend `OrderStatus` with `PlanReady`, `PlanApproved`, and `ReadyToDeliver` while keeping all legacy statuses parseable.
- [ ] Add tests proving old status strings still parse.

### Phase 2: Repositories

- [ ] Write failing repo tests for inserting and finding a quote draft by id and slug.
- [ ] Implement `repo/quote_drafts.rs`.
- [ ] Write failing repo tests for updating quote draft status, quote range, drop reason, and promoted order id.
- [ ] Implement quote draft update methods.
- [ ] Write failing repo tests for creating and updating an `order_workflow` row.
- [ ] Implement `repo/order_workflow.rs`.
- [ ] Write failing repo tests for creating and updating a delivery package record.
- [ ] Implement `repo/delivery_packages.rs`.
- [ ] Update `repo/mod.rs` exports.

### Phase 3: XDG Quote Draft Record Service

- [ ] Add `Paths::quote_drafts_dir()` in `config.rs` with a unit test using `Paths::under_root()`.
- [ ] Write a failing `services::quote` test: recording a draft stores `$XDG_DATA_HOME/gig/quotes/<quote-id>` as the expected path and does not create any directory or file.
- [ ] Implement `services::quote::record_quote_draft`.
- [ ] Write a failing test that missing project type is rejected.
- [ ] Add validation for project type and required summary/title.
- [ ] Write a failing test for `quote drop` requiring a non-empty reason.
- [ ] Implement drop behavior and timestamp update.
- [ ] Write a failing test for recording quote range and recommendation without marking the quote as accepted.
- [ ] Implement quote price recording.
- [ ] Add validation that workflow-created quote-stage `JOB.md` and `QUOTE.md` exist before transitions that require them.

### Phase 4: Formal Project `.gig/` Registration

- [ ] Write a failing test: accepting a quote creates an order and records expected `<project>/.gig/` paths without creating project directories or workflow files.
- [ ] Implement `services::quote::accept_quote_draft` up to order creation and path registration.
- [ ] Write a failing test: accepted project does not default-create `input/`, `work/`, or `output/`.
- [ ] Implement `.gig/` path registration and existence validation in `services::workflow_paths`.
- [ ] Write a failing test that quote `JOB.md` and `QUOTE.md` must already exist in `.gig/` before `gig` records the project as ready for planning.
- [ ] Implement quote-to-project file validation without copying files.
- [ ] Write a failing test that `.gig/INDEX.html` is expected but not generated by `gig`.
- [ ] Implement index existence/staleness checks only.
- [ ] Refactor `services::init` so workflow paths do not write root `README.md` or other workflow files.
- [ ] Preserve legacy/manual `gig init` behavior only where explicitly needed; document any changed behavior in the command help.

### Phase 5: Plan Approval Workflow

- [ ] Write a failing transition test: `Accepted -> PlanReady -> PlanApproved -> InProgress` is allowed.
- [ ] Write a failing transition test: `Accepted -> InProgress` is rejected in the workflow path.
- [ ] Implement new `OrderStatus` transitions in `services/orders.rs`.
- [ ] Write a failing service test: marking plan ready requires `.gig/plan/PLAN.md` and `.gig/plan/PLAN.html` paths.
- [ ] Implement `services::workflow::mark_plan_ready`.
- [ ] Write a failing service test: approving a plan records `plan_approved_at` and marks `.gig/INDEX.html` stale or exposes changed state for the workflow to refresh.
- [ ] Implement `services::workflow::approve_plan` without rendering `.gig/INDEX.html`.
- [ ] Write a failing service test: rejecting a plan from `plan_ready` records `plan_rejected_at` and `plan_rejection_reason`, returns the order to `accepted`, and does not delete plan files.
- [ ] Implement `services::workflow::reject_plan`.
- [ ] Add `gig plan ready` CLI command.
- [ ] Add `gig plan approve` CLI command.
- [ ] Add `gig plan reject` CLI command.
- [ ] Add English human output and `--json` machine output for plan commands.

### Phase 6: Acceptance Gate

- [ ] Write a failing service test: acceptance cannot be completed without `.gig/acceptance/ACCEPTANCE.md`.
- [ ] Implement `services::workflow::check_acceptance` to check file existence and declared completeness markers.
- [ ] Write a failing test for required acceptance table headers: `验收项`, `方法`, `证据`, `结论` or English equivalents from the template.
- [ ] Implement conservative acceptance parsing. If parsing is uncertain, fail closed with a clear message.
- [ ] Implement `services::workflow::complete_acceptance` to set `acceptance_completed_at`, transition to `ReadyToDeliver`, and expose changed state for the workflow to refresh `.gig/INDEX.html`.
- [ ] Add `gig acceptance check` CLI command.
- [ ] Add `gig acceptance complete` CLI command.
- [ ] Add English human output and `--json` machine output for acceptance commands.

### Phase 7: Safe Client Package Validation

- [ ] Write a failing test proving raw `pack_order(project_dir)` would include hidden files unless ignored; document why it must not be used for client packages.
- [ ] Write a failing `client_package` test: a package manifest may only reference files under `.gig/delivery/<date>/client/`.
- [ ] Implement allowlist-rooted package validation from the client directory.
- [ ] Write a failing test that paths containing `.gig/`, `prompts/`, `internal/`, `ACCEPTANCE.md`, or `DELIVERY_INTERNAL.html` are rejected even if listed.
- [ ] Implement blocklist validation.
- [ ] Write a failing test that an existing `.gig/delivery/<date>/export/client-package.zip` can be validated and recorded.
- [ ] Implement zip validation without using `pack_order(project_dir)` and without creating archives.
- [ ] Record `delivery_packages` row after validation.
- [ ] Add `gig package check` CLI command to validate delivery layout and package safety.
- [ ] Add `gig package record` CLI command.
- [ ] Add `gig package mark-sent` CLI command, but do not send anything externally.
- [ ] Add English human output and `--json` machine output for package commands.

### Phase 8: Decision Dashboard

- [ ] Write failing dashboard tests for quote draft items: missing facts, ready to quote, quoted waiting client, dropped excluded by default.
- [ ] Extend `services::dashboard` to include quote drafts.
- [ ] Write failing dashboard tests for new order states: `plan_ready`, `plan_approved`, `in_progress` without acceptance, `ready_to_deliver`, delivered unpaid.
- [ ] Extend dashboard decision mapping.
- [ ] Add English workflow display labels in `gig-cli/src/ui.rs`.
- [ ] Keep internal status strings for JSON output.
- [ ] Update `gig ls` default title from “Today's focus” to an English workflow decision-board title.
- [ ] Add `gig ls --json` with stable internal fields.
- [ ] Preserve `gig ls --all` behavior, but use English workflow labels in human mode.

### Phase 9: CLI Wiring And Help Text

- [ ] Add `QuoteCommand`, `PlanCommand`, `AcceptanceCommand`, and `PackageCommand` to `cli.rs`.
- [ ] Add modules to `commands/mod.rs`.
- [ ] Add dispatch arms in `dispatch.rs`.
- [ ] Update top-level `about` text in English to describe `gig` as a freelance workflow support tool.
- [ ] Ensure every new command has examples in help text.
- [ ] Ensure command names remain English.
- [ ] Add `--json` to new commands where skill automation needs stable output.
- [ ] Do not localize JSON keys or enum values.

### Phase 10: Show, Doctor, And Backward Compatibility

- [ ] Extend `gig show` to display workflow paths and next decision in human mode.
- [ ] Add `gig show --json` if `serde_json` is added.
- [ ] Extend `gig doctor` to report missing `.gig/INDEX.html`, missing plan paths, missing acceptance file, and unsafe package artifacts.
- [ ] Add compatibility tests using old orders with legacy statuses and no `order_workflow` row.
- [ ] Ensure old commands do not panic on missing workflow metadata.
- [ ] Add a migration/backfill helper or documented command for adding `project_type` to old orders later. Do not require it in this plan.

### Phase 11: Documentation And Verification

- [ ] Update README command examples for the new support-layer workflow.
- [ ] Add a short doc section explaining XDG quote drafts vs project-local `.gig/`.
- [ ] Add a warning that client packages are allowlist-based and must not be created by packing the project root.
- [ ] Run `cargo fmt`.
- [ ] Run `cargo clippy --workspace --all-targets`.
- [ ] Run `cargo test --workspace`.
- [ ] Manually smoke-test the happy path in a temporary XDG root: quote draft record -> quote accept -> workflow-created `.gig/` project -> plan ready -> plan approve -> acceptance complete -> package validation.
- [ ] Manually inspect the created project root and confirm it contains no root workflow files and no default `input/work/output` directories.
- [ ] Manually inspect the workflow-created zip and confirm `gig package check` rejects leakage of `.gig/`, `internal/`, `prompts/`, `ACCEPTANCE.md`, or `DELIVERY_INTERNAL.html`.

---

## Suggested First Commit Boundary

Do not implement the whole plan in one commit. The clean first vertical slice is:

```text
V002 schema + models + quote draft repo/service + gig quote new/show/list/drop
```

This slice is useful on its own, does not touch formal project initialization yet, and proves the pre-acceptance XDG quote-draft design.

Suggested first commit message:

```text
feat(gig): add quote draft support layer
```

Only commit when explicitly requested.

---

## Risks

- Existing `Lead` semantics may overlap with `quote_drafts`. Keep them separate and document the boundary.
- `pack_order` is tempting to reuse for client packages, but it includes hidden files and can leak `.gig/`; use validation-only package services instead.
- Human output is English, but it is still not an API. Require `--json` for automation.
- Acceptance parsing should be conservative. If `ACCEPTANCE.md` is ambiguous, block and ask for completion rather than guessing.
- Changing `gig init` can break existing habits. Make workflow registration validation-only and avoid adding new file-writing behavior; if legacy `gig init` file creation remains temporarily, mark it as legacy/manual and keep it out of the `partjob-workflow` path.
- TUI/GUI can be considered after the support-layer model stabilizes. Do not let future UI concerns pull workflow logic out of Rust core in phase 1.

---

## Definition Of Done

- Quote draft records point to XDG locations and can be listed, priced, accepted, or dropped without creating project directories or workflow files before acceptance.
- Accepted quote creates a formal order and records project-local `.gig/` paths while keeping the project root clean.
- `.gig/INDEX.html` is generated/refreshed by `partjob-workflow` or agents; `gig` only validates existence/staleness and exposes state.
- Plans cannot be executed before explicit plan approval, and plan rejection records a reason while returning the order to `accepted` for revision.
- Acceptance cannot complete without `.gig/acceptance/ACCEPTANCE.md` evidence structure.
- Client package validation is allowlist/manifest-based and cannot leak internal files.
- `gig ls` works as an English decision dashboard for humans and provides stable machine output for the workflow skill.
- Existing legacy orders still load and display.
- `cargo fmt`, `cargo clippy --workspace --all-targets`, and `cargo test --workspace` pass.
