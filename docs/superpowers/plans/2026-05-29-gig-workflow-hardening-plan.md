# gig Workflow Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Use superpowers:test-driven-development for every behavior change.

**Goal:** Harden `gig` so its advertised workflow actions are real semantic gates with matching CLI commands, timestamps, validators, dashboard next actions, and docs, while preserving the boundary that `partjob-workflow` and agents generate workflow files and `gig` only records, indexes, validates, uploads, and traces.

**Design Spec:** `docs/superpowers/specs/2026-05-29-gig-workflow-hardening-design.md`

**Architecture:** Keep the existing Rust split. `gig-core` owns schema, models, repositories, services, validators, dashboard view models, and package safety. `gig-cli` remains a thin `clap` shell that resolves config, opens the database, calls `gig_core::services::*`, and formats human/JSON output. Do not add AI calls, file generation, or project scaffolding.

**Expected Public Surface:**

- `gig work start <id_or_slug> [--json]`
- `gig package check <id_or_slug> --delivery-date <YYYY-MM-DD> --delivery-dir <path> [--json]`
- Existing `gig package send ...` remains, using the same stricter validation.
- README happy path uses `gig work start`, not `gig status ... in_progress`.

---

## Files to Modify

### Core Schema / Model / Repository

- `crates/gig-core/src/db/migrations/V004__workflow_hardening.sql`  
  Add `work_started_at TEXT` to `order_workflow`.
- `crates/gig-core/src/models/order_workflow.rs`  
  Add `work_started_at: Option<OffsetDateTime>`.
- `crates/gig-core/src/repo/order_workflow.rs`  
  Include `work_started_at` in selected columns and row mapping; update `record_work_started` to set `work_started_at` and `updated_at`.

### Core Services

- `crates/gig-core/src/services/workflow.rs`  
  Add item-level acceptance validation and make multi-write workflow transitions atomic where practical.
- `crates/gig-core/src/services/quote.rs`  
  Require `.gig/INDEX.html` during quote acceptance.
- `crates/gig-core/src/services/client_package.rs`  
  Add required delivery-doc completeness checks used by both check and send.
- `crates/gig-core/src/services/dashboard.rs`  
  Return `check_package` before package validation and `send_package` after a validated package exists.

### CLI

- `crates/gig-cli/src/cli.rs`  
  Add `WorkCommand`; add `PackageCommand::Check`.
- `crates/gig-cli/src/commands/mod.rs`  
  Export `work` command module.
- `crates/gig-cli/src/commands/dispatch.rs`  
  Dispatch `Commands::Work` and package check.
- `crates/gig-cli/src/commands/work.rs`  
  New command implementation for `gig work start`.
- `crates/gig-cli/src/commands/package.rs`  
  Add `check` dispatcher and output formatting.

### Tests / Docs

- `crates/gig-core/src/services/quote.rs` tests  
  Add missing-index acceptance failure and update existing acceptance fixture.
- `crates/gig-core/src/services/workflow.rs` tests  
  Add `work_started_at`; add incomplete acceptance-item failures.
- `crates/gig-core/src/services/client_package.rs` tests  
  Add missing required delivery docs failures; update happy fixtures.
- `crates/gig-core/src/services/dashboard.rs` or existing dashboard tests  
  Add next-action behavior around validated packages.
- `crates/gig-cli/tests/workflow_cli.rs`  
  Add CLI coverage for `work start` and `package check`; update fixtures for `.gig/INDEX.html` and required delivery docs.
- `README.md`  
  Update happy path and command reference.

---

## Implementation Tasks

### Task 1: Add `work_started_at` schema/model support

- [ ] Write or update a core test that proves `workflow::start_work` records a distinct `work_started_at` timestamp after plan approval.
- [ ] Run the focused test and confirm it fails because the field does not exist or is not populated.
- [ ] Add `V004__workflow_hardening.sql` with `ALTER TABLE order_workflow ADD COLUMN work_started_at TEXT;`.
- [ ] Update `OrderWorkflow` model and `order_workflow` repo select/map helpers.
- [ ] Update `record_work_started` to set `work_started_at = ?` and `updated_at = ?`.
- [ ] Run the focused core workflow test.

### Task 2: Add semantic `gig work start`

- [ ] Write a CLI test for `gig work start <slug> --json` succeeding after `gig plan approve` and returning status `in_progress` plus `next_action: complete_acceptance`.
- [ ] Write a CLI test for `gig work start <slug> --json` failing before plan approval.
- [ ] Run the new CLI tests and confirm they fail because the command is missing.
- [ ] Add `WorkCommand` and `WorkStartArgs` in `crates/gig-cli/src/cli.rs`.
- [ ] Create `crates/gig-cli/src/commands/work.rs` using existing command patterns: resolve order, call `workflow::start_work`, format human/JSON output.
- [ ] Wire `commands/mod.rs` and `commands/dispatch.rs`.
- [ ] Run the focused CLI tests.

### Task 3: Require `.gig/INDEX.html` on quote acceptance

- [ ] Add a core quote-service test that `accept_quote_draft` fails when `.gig/INDEX.html` is missing even if `JOB.md` and `QUOTE.md` exist.
- [ ] Update existing quote acceptance fixtures to create `.gig/INDEX.html` where acceptance should succeed.
- [ ] Run focused quote tests and confirm the new test fails first.
- [ ] Update `accept_quote_draft` to require `INDEX.html` along with `JOB.md` and `QUOTE.md`.
- [ ] Update CLI workflow fixtures that accept quotes to create `.gig/INDEX.html`.
- [ ] Run focused core and CLI workflow tests.

### Task 4: Strengthen acceptance validation

- [ ] Add tests where `ACCEPTANCE.md` has global headings but one row/item lacks method, evidence, or conclusion.
- [ ] Add one test for a blocked/empty conclusion that must not pass completion.
- [ ] Run focused workflow tests and confirm failures.
- [ ] Replace or extend `require_acceptance_headings` with a small structural validator.
- [ ] Support the existing Markdown table fixture format first.
- [ ] Keep validation structural only; do not judge evidence quality.
- [ ] Run focused workflow and CLI acceptance tests.

### Task 5: Add required delivery-doc completeness checks

- [ ] Add core package tests proving package check/send fail if any required doc is missing: `DELIVERY.md`, `internal/DELIVERY_INTERNAL.html`, `client/DELIVERY_CLIENT.html`, `client/DELIVERY_CLIENT.pdf`.
- [ ] Update existing package happy-path fixtures to create all required docs.
- [ ] Run focused package tests and confirm failures.
- [ ] Add a helper in `client_package.rs` that validates required delivery docs before manifest/zip recording or upload.
- [ ] Ensure internal docs remain forbidden in `manifest.toml` `client_files`.
- [ ] Run focused package tests.

### Task 6: Expose `gig package check`

- [ ] Write a CLI test for `gig package check <slug> --delivery-date ... --delivery-dir ... --json` recording a `Validated` package.
- [ ] Run the test and confirm the command is missing.
- [ ] Add `PackageCommand::Check` and args matching `send` minus uploader concerns.
- [ ] Implement `commands/package.rs::check` by calling `check_client_package`.
- [ ] Output `status: validated` and `next_action: send_package` in JSON/human modes.
- [ ] Run focused CLI package tests.

### Task 7: Align dashboard next actions

- [ ] Add or update dashboard tests so `ReadyToDeliver` returns `check_package` when no matching validated package exists.
- [ ] Add or update dashboard tests so `ReadyToDeliver` returns `send_package` when a matching validated package exists.
- [ ] Run focused dashboard tests and confirm the first new case fails.
- [ ] Update `dashboard.rs` next-action logic to inspect package validation state.
- [ ] Run focused dashboard/list/show tests.

### Task 8: Atomic workflow writes

- [ ] Identify workflow service methods that update both order status and `order_workflow`: `mark_plan_ready`, `approve_plan`, `reject_plan`, `start_work`, `complete_acceptance`.
- [ ] Add a narrow internal helper or transaction pattern consistent with existing `rusqlite` usage.
- [ ] Convert multi-write operations to a transaction without changing public behavior.
- [ ] Avoid broad repository rewrites.
- [ ] Run focused workflow tests after each converted operation.

### Task 9: Update README and command examples

- [ ] Replace `gig status <slug> in_progress` in the documented happy path with `gig work start <slug>`.
- [ ] Add `gig package check` before `gig package send` in the delivery sequence.
- [ ] Document required delivery docs and the `.gig/INDEX.html` acceptance requirement.
- [ ] Keep wording explicit that external workflow/agents create files and `gig` validates them.
- [ ] Run README-adjacent grep to ensure no stale happy-path `status ... in_progress` remains.

### Task 10: Full verification

- [ ] Run `cargo fmt --all`.
- [ ] Run focused package/core/CLI tests touched by the changes.
- [ ] Run broader `cargo test` if runtime is acceptable.
- [ ] Run `cargo run -p gig-cli -- --help` and `cargo run -p gig-cli -- package --help` to verify command surfaces.
- [ ] Run LSP diagnostics on changed Rust files.
- [ ] Check `rtk git status --short` and confirm only intended files changed, preserving pre-existing dirty docs/untracked files.

---

## Implementation Notes

- Use vertical TDD. Do not write all tests first.
- Keep `gig-cli` thin. All validation logic belongs in `gig-core` services.
- Do not add compatibility shims for unreleased draft behavior unless an existing persisted database requires them.
- Do not remove generic `gig status`; just stop presenting it as the workflow happy path for starting work.
- Do not commit unless explicitly requested.
