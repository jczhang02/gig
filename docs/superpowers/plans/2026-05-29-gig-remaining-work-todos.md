# gig Remaining Work TODOs

> Scope: post workflow-hardening backlog only. `gig` core workflow gates are implemented; these are the next product directions.

## DONE 1: Improve short-link download speed

**Status:** Completed on 2026-05-30. `gig` now supports a separate S3 `download_endpoint` for presigned GET links, the local config can point downloads at Alibaba Cloud OSS transfer acceleration, and `scripts/benchmark-short-link-download.py` documents the repeatable measurement path. See `docs/superpowers/plans/2026-05-29-gig-short-link-download-speed-notes.md`.

**Problem:** The current `gig` short-link delivery path works, but client downloads are too slow.

**Goal:** Make short-link downloads fast enough for real client delivery without weakening package validation, upload tracing, or the `package check -> package send` workflow gate.

**Candidate work:**

- Measure current short-link download latency and throughput with representative client packages.
- Identify whether the bottleneck is redirect handling, object storage region, proxy/worker streaming, signed URL generation, or cache behavior.
- Add a performance target before implementation, for example first-byte latency and sustained throughput for a typical package size.
- Preserve the current safety model: package contents still come from an explicit manifest and successful package validation.
- Add regression tests or a repeatable benchmark script so future delivery changes do not silently slow downloads again.

**Done when:** a realistic client package downloads through the short-link path within the chosen performance target, with documented measurement evidence.

## TODO 2: Build the `gig gui` companion

**Status:** Phase A, Phase B, and Phase C pre-GUI backend checkpoints completed on 2026-05-30, then hardened by parallel review before frontend work. Phase A delivered the isolated read-only `crates/gig-gui` localhost companion and root `CONTEXT.md` glossary. Phase B added the `gig-core::actions` metadata skeleton, catalog coverage tests, and protected `GET /api/actions`. Phase C added request-scoped `ActionContext`, executable read actions, preflight/execute API skeletons, read-only DB access, side-effect-aware preflight, and routed existing GUI read endpoints through core actions. Existing CLI command handlers, outputs, and JSON semantics stay untouched. TODO 2 remains open for formal frontend GUI development next. See `docs/superpowers/plans/2026-05-30-gig-gui-phase-a-checkpoint.md`, `docs/superpowers/plans/2026-05-30-gig-gui-phase-b-actions.md`, and `docs/superpowers/plans/2026-05-30-gig-gui-phase-c-pre-gui-backend.md`.

**Problem:** `gig` is now workflow-complete at the CLI layer, but daily operation still requires command-line sequencing and reading JSON/human output manually.

**Goal:** Build a local GUI companion for operating `gig` without replacing the CLI or reimplementing workflow rules.

**Existing planning doc:** `docs/superpowers/plans/2026-05-28-gig-local-gui-companion-plan.md`

**Core constraints:**

- The GUI must call typed `gig-core` actions or shared service surfaces, not shell-composed `gig` commands.
- The GUI must be invoked as `gig gui`, not as a standalone binary; only a thin CLI entrypoint may depend on `gig-gui`.
- Existing CLI command handlers, outputs, JSON error semantics, and workflow gates must remain isolated from GUI work.
- It must preserve the same workflow gates: quote draft, quote acceptance, plan ready/approve, `work start`, acceptance check/complete, package check/send, paid/archive.
- It must not generate workflow artifacts on behalf of `gig`; external workflow/agents still create `.gig` files and delivery packages.
- It should make the current `next_action` and required missing files obvious to JC.
- It should stay local-first; no hosted SaaS or public network exposure.

**Done when:** JC can run a local `gig gui`, inspect orders/quotes/workflow state, see the next required gate, and trigger supported actions through the same core semantics as the CLI.
