# gig Workflow Hardening Design

**Status:** Design accepted by continuation directive; implementation not started  
**Date:** 2026-05-29  
**Context:** Follow-up to the approved `partjob-workflow` protocol and the `gig` workflow support layer. This document closes the root-design question after auditing the current workflow implementation.

## 0. Problem

`gig` now has the right high-level boundary: it is a workflow support layer, not the workflow brain. It records state, paths, and delivery traces while `partjob-workflow`, files, agents, and JC drive the work.

The remaining root problem is that `gig` advertises a gated workflow but some gates are incomplete:

- `PlanApproved` exposes `next_action: start_work`, but the CLI has no semantic command for that action.
- `workflow::start_work` exists, but it only updates `order_workflow.updated_at`; the schema has no `work_started_at` timestamp.
- `gig package check` exists in core as `check_client_package`, but the CLI only exposes `package send`.
- Acceptance validation checks only for global headings, not that each acceptance item has method, evidence, and conclusion.
- Delivery package validation checks safe manifest/zip boundaries, but not the required delivery document set.
- `.gig/INDEX.html` is recorded and doctor-checked, but quote acceptance does not require it and there is no staleness signal.
- README still teaches `gig status <slug> in_progress`, which bypasses the workflow-specific action language.

The fix should harden the workflow action model, not turn `gig` into a generator or an agent runner.

## 1. Design Options Considered

### Option A: Patch only `start_work`

Add one command that calls `workflow::start_work` and update README.

**Pros:** Smallest diff; fixes the most visible mismatch.

**Cons:** Leaves the same class of bug elsewhere. `ReadyToDeliver -> send_package` still skips a public validate step; acceptance can still pass with incomplete items; delivery can still send without the required docs; the workflow remains only partially semantic.

### Option B: Hardening pass for all advertised workflow gates

Make every first-class `next_action` reachable through a first-class command and strengthen the corresponding validators.

**Pros:** Fixes the root model. `gig` remains a management tool, but its gates become reliable enough for the skill and humans to use without remembering hidden caveats.

**Cons:** Larger multi-file change and needs careful tests.

### Option C: Rebuild the workflow engine around explicit action objects

Introduce a new action registry/table and migrate all lifecycle transitions through it.

**Pros:** The most uniform long-term model.

**Cons:** Too much architecture for the current problem. It risks turning this into a framework and delaying the concrete fixes.

## 2. Decision

Choose **Option B: workflow hardening pass**.

This is the root implementation because it fixes the mismatch between advertised workflow actions and enforceable CLI/service gates, while preserving the core boundary:

```text
partjob-workflow skill = drives process and asks JC at decision points
gig = records state, indexes paths, validates externally-created files, gates transitions
workflow files = external interface between JC, agents, and gig
agents = generate/execute/fill evidence
JC = commercial and delivery decisions
```

`gig` must not generate `PLAN.md`, `PLAN.html`, `ACCEPTANCE.md`, delivery docs, client package contents, or `.gig/INDEX.html`. It may require, validate, timestamp, and report on those files.

## 3. Target Workflow Contract

### Quote Accepted

`gig quote accept <draft> --project-dir <path>` requires the external workflow/project setup to have created:

- `<project>/.gig/INDEX.html`
- `<project>/.gig/JOB.md`
- `<project>/.gig/QUOTE.md`

It records workflow paths for plan, acceptance, and delivery, but does not create those files.

### Plan Ready / Approved

`gig plan ready <slug>` requires externally-created `.gig/plan/PLAN.md` and `.gig/plan/PLAN.html`.

`gig plan approve <slug>` records plan approval atomically with the order status transition.

### Work Started

`gig work start <slug> [--json]` is the semantic execution gate.

It requires `PlanApproved`, transitions the order to `InProgress`, records `order_workflow.work_started_at`, and emits `next_action: complete_acceptance`.

`gig status <slug> in_progress` remains a generic/manual lifecycle tool, but README and workflow-facing output should not teach it as the normal plan-approved execution gate.

### Acceptance Complete

`gig acceptance check <slug>` and `gig acceptance complete <slug>` require `.gig/acceptance/ACCEPTANCE.md` to contain acceptance items where every item has:

- item / 验收项
- method / 方法
- evidence / 证据
- conclusion / 结论

The conclusion must be a pass-like value, not empty or blocked. This validation is intentionally structural. It does not judge whether the evidence is good; JC and the workflow do that.

### Package Checked / Sent

`gig package check <slug> --delivery-date <date> --delivery-dir <dir> [--json]` validates an externally-created delivery directory and records a `Validated` package without upload.

`gig package send ...` keeps the existing upload/send path, but uses the same validation.

Required delivery docs are:

- `<delivery_dir>/DELIVERY.md`
- `<delivery_dir>/internal/DELIVERY_INTERNAL.html`
- `<delivery_dir>/client/DELIVERY_CLIENT.html`
- `<delivery_dir>/client/DELIVERY_CLIENT.pdf`

The client package remains manifest/allowlist based. Internal files stay outside the zip and are still forbidden in `manifest.toml` `client_files`.

### Dashboard / Next Action

Dashboard next actions should match public gates:

- `Accepted -> prepare_plan`
- `PlanReady -> approve_plan`
- `PlanApproved -> start_work`
- `InProgress -> complete_acceptance`
- `ReadyToDeliver -> check_package` until a matching validated package exists
- `ReadyToDeliver -> send_package` when the current delivery package is validated and sendable

## 4. Component Design

### Database Migration

Create `crates/gig-core/src/db/migrations/V004__workflow_hardening.sql`.

Responsibilities:

- Add nullable `work_started_at TEXT` to `order_workflow`.
- Avoid backfilling legacy rows. Missing timestamps are expected for historical workflows.

### Workflow Model / Repo

Modify:

- `crates/gig-core/src/models/order_workflow.rs`
- `crates/gig-core/src/repo/order_workflow.rs`

Responsibilities:

- Add `work_started_at: Option<OffsetDateTime>`.
- Include the column in selects and row mapping.
- Make `record_work_started` set both `work_started_at` and `updated_at`.

### Workflow Service

Modify `crates/gig-core/src/services/workflow.rs`.

Responsibilities:

- Keep `mark_plan_ready`, `approve_plan`, `start_work`, `complete_acceptance`, and `reject_plan` as content-free validators/state gates.
- Make status transition plus workflow timestamp updates atomic where the operation writes both order state and workflow metadata.
- Replace global heading-only acceptance validation with item-level structural validation.
- Preserve the current no-file-generation invariant.

### Quote Service

Modify `crates/gig-core/src/services/quote.rs`.

Responsibilities:

- Require externally-created `.gig/INDEX.html` at quote acceptance along with `.gig/JOB.md` and `.gig/QUOTE.md`.
- Continue recording expected future plan and acceptance paths without creating them.

### Client Package Service

Modify `crates/gig-core/src/services/client_package.rs`.

Responsibilities:

- Keep existing manifest/zip safety checks.
- Add delivery-doc completeness checks before `check_client_package` and `send_client_package` can record or upload.
- Keep internal docs forbidden from `client_files`.

### Dashboard Service

Modify `crates/gig-core/src/services/dashboard.rs`.

Responsibilities:

- Change `ReadyToDeliver` next action from unconditional `send_package` to `check_package` unless a validated sendable package exists.
- Keep legacy metadata behavior for orders missing `order_workflow`.

### CLI Commands

Modify:

- `crates/gig-cli/src/cli.rs`
- `crates/gig-cli/src/commands/mod.rs`
- `crates/gig-cli/src/commands/dispatch.rs`
- `crates/gig-cli/src/commands/plan.rs`
- `crates/gig-cli/src/commands/package.rs`

Create:

- `crates/gig-cli/src/commands/work.rs`

Responsibilities:

- Add `gig work start <slug> [--json]`.
- Add `gig package check <slug> --delivery-date <date> --delivery-dir <dir> [--json]`.
- Keep CLI thin: resolve config, open DB, find order, call core service, format output.

### Documentation

Modify `README.md`.

Responsibilities:

- Replace `gig status <slug> in_progress` in the happy path with `gig work start <slug>`.
- Document package check before package send.
- Document required delivery docs and the `.gig/INDEX.html` requirement.
- Make clear that `gig` validates externally-created files and does not generate them.

## 5. Validation Strategy

Use vertical TDD slices. Each behavior change gets a failing test first, then minimal implementation.

Core tests should cover:

- Quote accept fails without `.gig/INDEX.html`.
- `start_work` records `work_started_at` and updates status atomically.
- Acceptance check fails when any item lacks method, evidence, or conclusion.
- Package check fails if required delivery docs are missing.
- Dashboard returns `check_package` before a validated package and `send_package` after validation.

CLI tests should cover:

- `gig work start --json` succeeds only after plan approval.
- `gig work start` fails before approval with machine-readable error in JSON mode.
- `gig package check --json` records a `Validated` package.
- Existing `package send` still works with the stricter delivery doc set.
- README happy path command sequence matches real CLI commands.

## 6. Non-Goals

- No AI/model calls from `gig`.
- No generation of plan, acceptance, delivery, quote, package, or index content.
- No TUI/GUI work.
- No replacement of the generic `gig status` command.
- No automatic sending to clients without explicit command execution.
- No project-root scaffolding like `input/`, `work/`, or `output/`.

## 7. Risks

### Delivery directory shape

The design assumes internal delivery docs live under `<delivery_dir>/internal/DELIVERY_INTERNAL.html`, while client docs live under `<delivery_dir>/client/`. This matches the existing safety model that rejects `internal` from client manifests. If current external workflow writes `DELIVERY_INTERNAL.html` somewhere else, update the skill/doc contract or choose one canonical path before implementation.

### Acceptance parser complexity

Markdown can be irregular. Keep v1 parser deliberately small: support the table format already used in tests and a simple heading-section fallback only if needed. Do not build a general Markdown parser unless tests prove the need.

### Atomic transition refactor

Existing services may assume `&Connection` and repository helpers that update independently. Make the smallest transaction-friendly change possible and avoid broad repository rewrites.

## 8. Success Criteria

- Every workflow `next_action` in the happy path maps to a public semantic CLI command.
- Workflow timestamps record plan ready, plan approved, work started, and acceptance complete.
- `gig` blocks incomplete acceptance and incomplete delivery docs before delivery.
- `package check` is usable from CLI and creates `Validated` package state.
- README no longer teaches generic status as the normal execution gate.
- Tests pass for `gig-core` and `gig-cli` workflow coverage.
