# partjobs workflow v2 (layer 1: knowledge and rules)

Written 2026-09-29 from JC's own description of the process; English edition. This document is the source of the whole system: the skill's text is generated from it, gig v2 stores only what it requires, and the project templates create only the files it names.

Tags:
- [Practice] Already done this way in the existing projects (tk-dtf-compact, bllc-reproduction, sers-colitis-analysis, patent-value-identification) and in the v1 gig database.
- [JC] JC's own description, adopted as is.
- [Decided] Settled on 2026-09-29.

## 0. Purpose

[JC] This workflow covers every part of taking freelance orders: from the first request to the end of the warranty. JC does not operate gig; agents read and write it. Client communication and pricing are JC's, done on the platform; the workflow records outcomes and supplies material.

[Decided] Priorities: (1) a project keeps its memory across sessions and agents; (2) deliverables are safe: no client materials or internal files leak; (3) the money comes in, the warranty runs out, the project is archived; (4) agents never take irreversible or outward actions on their own.

## 1. Process

Four phases. The boundaries between them are recording points, not gates.

### 1.1 Before the order

[JC]
- An order comes in with a short first request from the client. Create a temporary, hidden working directory.
- Talk with the client until the requirements are concrete.
- Quote and negotiate. JC usually prices from the client's budget and prefers to learn the budget first.
- The client agrees and places the order: the job officially starts and the temporary directory becomes the formal one at `~/dev/partjobs/<slug>/`. The client declines: delete the temporary directory.

[Decided]
- The temporary directory is `~/dev/partjobs/.drafts/<slug>/` (hidden, not visible at the top of partjobs). It holds only `NOTES.md` (the client's words, material locations, questions, budget, pricing notes) and a few samples copied from the originals. No git, no `.gig/`.
- gig records a `draft` for this phase: slug, material path, creation date. Promotion turns it into an order; dropping snapshots `NOTES.md` into gig, removes the directory and keeps a dropped record with the reason, so similar requests can be looked up later.
- What the agent may do here: inspect the materials, assess feasibility, prepare a question list for JC to relay to the client, estimate the effort for JC's reference. No code, no formal project.

### 1.2 Kickoff

[JC]
- In the formal directory, set up agent rules with `setup-matt-pocock-skills`, for large projects only (the agent judges, JC may overrule).
- Settle the concrete route and goals with `grill-me` or `grill-with-docs`.
- The agent and JC do the work.
- Questions for the client may come up midway.

[Practice] What the existing projects share:
- A separate git repository, branch main. Originals stay at their path, read-only (usually `/mnt/virtiofs/<id>/`); samples are copied into `data/` or `references/`.
- Client samples, images carrying order numbers or personal information, the delivery directory and build outputs are not committed; every `.gitignore` line carries its reason.
- `.gig/JOB.md` and `.gig/QUOTE.md` are the only workflow files (section 2). The project's `AGENTS.md` holds project-specific agent rules.
- Grill conclusions go into JOB.md "Confirmed decisions", dated and marked "confirmed by grilling".

[Decided] The day-zero skeleton when the draft is promoted:

```
<slug>/
  .git/  .gitignore  .python-version
  .gig/JOB.md  .gig/QUOTE.md
  AGENTS.md            # generated, then extended with the project-specific rules of section 4
  CONTEXT.md  docs/adr/   # produced by grill-with-docs; absent with grill-me
  README.md
  data/  references/   # client samples inside are gitignored
  src/  tests/         # by project type
```

No longer generated: INDEX.html, PLAN.md/PLAN.html, ACCEPTANCE.md, MEASURE.md, progress-log.md, any delivery metadata. Plans and acceptance records go into JOB.md.

Questions for the client midway: the agent writes them as a forwardable message under "Client questions" in JOB.md; when JC brings the answer it becomes a new decision.

### 1.3 Delivery

[JC]
- Once JC is satisfied, a preview of good output goes to the client first, as proof the work is done.
- If the client is satisfied they accept, and the full delivery follows.
- If not, work continues.

[Practice] Past deliveries:
- tk-dtf-compact (tool): programs (Windows exe and Linux binary), a manual PDF (Kami, with screenshots), source zip (git archive without .gig), this batch's processed results. A separate comparison directory with 19 before/after images was effectively the preview.
- bllc-reproduction (reproduction): programs and configurations, four Kami PDFs (delivery report, unreproduced items and blockers, program differences, code walkthrough), results.
- sers (data analysis): restyled figures and analysis results.

[Decided] Preview and full package (see section 5):

```
delivery/                          # the whole directory is gitignored
  <package-id>/                    # client files only. package-id defaults to <slug>-vX.Y.Z
    <files...>
  <package-id>.manifest.toml       # outside the package, not in the zip. version=1, package_id, kind, files=[...] relative to <package-id>/
  <package-id>.zip                 # entries equal the manifest
  <slug>-vX.Y.Z-preview/           # previews follow the same rules with kind=preview
```

- Sending channels [JC]: usually `gig upload` (object storage, short link); sometimes gsconnect to the phone, with JC forwarding to the client. JC approves; the agent may act: after approval the agent runs the upload or gsconnect and reports the short link or delivery.
- `gig package upload` re-checks, uploads, shortens the link and records it. Preview and full packages both go this way, distinguished by kind. The phone channel also re-checks and records channel=phone without a remote URL.
- Uploading or sending to the phone needs JC's explicit approval, once per send. After approval the agent executes; JC need not run anything.
- When the client is not satisfied the job returns to 1.2, new decisions go into JOB.md, and a new version is delivered.
- Before a preview, JC tries the version (`dogfood`): the program run on JC's machine, or the figures and report draft read. Findings become decisions. (2026-10-08, thermal-video-curve: the light theme and the curve drawing both came from JC's own use, not from tests.)
- A preview is first a candidate under `.scratch/preview/<package-id>/`; only the version JC agrees to send is built into `delivery/` and registered in gig. (Same order: four previews built, one sent.)

### 1.4 Warranty and payment

[JC]
- After receiving the delivery the client may ask questions while using it. JC answers.
- Client satisfied: JC is paid and the order is complete, followed by a 15-day warranty.
- Client not satisfied: JC keeps revising until they are.

[Decided]
- Payment: `gig paid <slug>` records the date and updates the payment line in QUOTE.md. The warranty ends on the payment date plus 15 days; gig computes it and `gig ls` lists orders still in warranty.
- Changes during the warranty: recorded as decisions in JOB.md; the order stays `paid` and a new package is delivered. No new order is opened during the warranty.
- After the warranty: `gig archive <slug>`. It first reports uncommitted changes, large files and unsent packages; after JC's approval the project moves to archive_root. Whether large files (checkpoints, data) are removed follows the bllc precedent of 2026-09-27: an explicit deletion list, explicit approval, git history kept.
- States:

```
draft -> queued -> in_progress -> delivered -> paid -> archived
  |         |           ^             |
  +-dropped +-cancelled +-------------+ (rework or warranty work)
```

  `cancelled` is reachable from `queued` or `in_progress` (after delivery there is no cancellation, only rework). `draft` before the order; `queued` registered but not started; `in_progress` work under way; `delivered` full package sent; `paid` paid and in warranty (warranty_until = paid_at + 15 days); `archived` archived. Sending a preview changes no state; it is only a package record with kind=preview.

## 2. JOB.md and QUOTE.md

[Practice] The JOB.md files of tk and bllc are the model:

```
# <title>
- slug, material id, path of the originals, location of the project copy
- the client's request (verbatim, with source file and encoding)
- price and payment facts: QUOTE.md

## Now                    overwritten at each phase end: at most five lines (phase and version, last send, what waits on JC, next step)
## Material facts         objective observations, no judgements
## Confirmed decisions    numbered; each is an executable constraint, dated, with how it was confirmed; overturned ones stay, a new item supersedes them
## Client questions       written by the agent, forwarded by JC, turned into decisions when answered
## Status                 dated log: what was done, verification figures, delivery directory, CI run / commit, what remains
```

QUOTE.md: project, material id, acceptance date, currency and total, commercial status, payment status, the client's words quoted, what was not agreed (delivery date, payment milestones, warranty).

[Decided]
- JOB.md is the project's memory. Agents read it before working and append a status entry at the end of every phase. "Now" is the only section that is rewritten, so a cold session finds the current state without reading the whole log (2026-10-08, thermal-video-curve: a 1.5-day order filled 199 lines and its session was compacted). "Confirmed decisions" is only ever extended by JC; agents do not edit it.
- QUOTE.md is the commercial snapshot; only JC changes it. A payment change runs `gig paid` at the same time.
- Actions that need JC's explicit approval, where silence or "continue" never counts: sending anything out; pushing to a remote; deleting, cleaning up, archiving; paid remote resources; scope changes beyond the confirmed decisions; changes to QUOTE.md.

## 3. General requirements

[JC]
- All reports use Kami or LaTeX and keep no intermediates. They must not read as machine-written; they pass through humanizer (humanizer-zh for Chinese).
- Working directories and file names are all English.
- Directories stay as clean as possible; no stray files.

[Practice] Current state that conflicts with this, to be changed for new orders:
- bllc's `reports/` keeps content.json, html, md, -visual directories and build scripts, against "no intermediates". Only PDFs belong there; generators and sources go to `.scratch/` or stay uncommitted.
- tk's delivered file names were Chinese. English inside the client package as well (decided, see 5.3).
- sers's `.gig/` holds 194 MB of checkpoints and logs. `.gig/` holds two markdown files and nothing else.

[Decided] "Clean directory" as checkable rules:
- The allowed top-level entries are fixed by the template; anything beyond them is explained in README or AGENTS.md.
- `.scratch/` is the only scratch area, gitignored, emptied or archived at the end of a phase.
- Report pipeline: sources (Kami content / LaTeX) are generated under `.scratch/reports/<name>/`, polished with humanizer (humanizer-zh for Chinese), and only the final PDF goes into `reports/` or the package.

## 4. Tech stack and agent rules

[Practice] Choices that recur across the four projects:
- Python: uv, pyproject.toml, ruff, pytest, `.python-version`.
- Configuration: Hydra with YAML (research) or ini plus command line (tools for non-technical clients). Every configuration item can be overridden on the command line (JC's hard requirement).
- Cross-platform: develop and verify on Linux; clients mostly use Windows. No Windows-only technology. Deliverable programs are PyInstaller onefile builds from GitHub Actions (windows-latest and ubuntu-latest) in a private repository, with the run id and commit recorded.
- Documents: client documents in Chinese, Kami PDFs with screenshots; a separate developer README.
- Projects without a local Python use Node/shell locally and run Python remotely.
- Training and heavy computation on the remote GPU server, environments and caches on the data disk (shanhe-computing-server skill).
- Task tracking: JOB.md status for small projects; beads or the issue tracker configured by setup-matt-pocock-skills for large ones.

[Decided] This is the skill's default tech stack, given per project type, overridable by JOB.md decisions. JC adds preferences with `/partjob rule`.

[Practice] Agent rules distilled from the AGENTS.md files of bllc and patent and from tk's JOB.md:

Generic (in the skill):
- Read `.gig/JOB.md` first; trust only `.gig/QUOTE.md` for price and payment.
- Originals are read-only. New results go to new directories; failures and unusable states stay visible.
- Never tune numbers toward a paper or the client's expectations. Report figures come from saved results, with split, configuration and aggregation stated.
- At the end of each phase record commands, verification results and remaining work in JOB.md "Status".
- Deliverables reproduce: rerun the delivered program and compare byte for byte with the source run; record it in JOB.md.
- Commit verified units of work promptly; before a long experiment commit the code, configuration and evaluation protocol. Check the staged diff; keep the user's concurrent edits.
- Temporary files and recovery copies stay in `.scratch/`, never in the parent directory.
- Use only existing materials; questions for the client go into "Client questions"; do not assume.
- ASCII punctuation in Chinese prose. Concrete names for concepts, no undefined abbreviations.
- Sub-agent handoff: current state, absolute project path, files to read, writable scope, completion criteria, the latest verification command. Sub-agents cannot approve, send, change scope, delete or archive.

Project-specific (in the project's AGENTS.md, drafted per type and then completed):
- Read-only paths and snapshot directories; whether training is allowed and where; local toolchain; whether continuous work is authorised; git commit and push authorisation.

## 5. Preview and full package

### 5.1 Preview

[Decided] A preview convinces the client the work is done while being useless on its own:
- Included: visual evidence of results (before/after comparisons, screenshots, metrics tables, the report's summary pages), a few processed results from the client's own samples (tk's 19 comparison images are the example), a short video or GIF of the program running.
- Excluded: source code, executables, the full batch of results, the full report, copyable data files.
- Form: a `<slug>-vX.Y.Z-preview/` package with the same manifest check and OSS short link; the client views it in a browser. Watermarking is JC's call.
- Defaults by type: tool = comparison images and a demo GIF; reproduction / analysis = key figures and summary pages; writing = table of contents and one section.

### 5.2 Full delivery

[Decided] Defaults by type, confirmed per order in JOB.md:
- Any job: delivery report PDF (installation, usage, results, known limitations, warranty terms), source code (git archive without .gig / .scratch / client samples), README.
- Tool jobs add: the executables the order's decisions call for (built with `/partjob build`; Windows and Linux by default, macOS when decided, none when the order needs no program), this batch's results, sample configuration.
- Reproduction / analysis jobs add: configurations, result data, figures, the differences or blockers report.
- Writing jobs add: PDF and sources (tex / docx).
- Never: copies of the client's originals, intermediates, internal audit files, data beyond test data.

### 5.3 Other settled points (2026-09-29)
- States as in 1.4.
- English file names inside client packages as well (the content stays in the client's language). Reason: consistent with the working-directory rule, and it avoids the encoding problems of Chinese names in zips on Windows (tk's original zip had GBK entries).
- Temporary directory `~/dev/partjobs/.drafts/<slug>/`.
- `delivery/` at the project root, gitignored; package-id defaults to `<slug>-vX.Y.Z`.
- The project's `AGENTS.md` is drafted from a template per type and then completed with project-specific rules.
- Unpromoted v1 quote drafts migrate as dropped drafts.
- `gig new` both registers the order and scaffolds the directory (called by the `start` subcommand).
- The skill is named `partjob`; subcommands in section 6.
- Tech stack: section 4 as written; JC adds to it with `/partjob rule`.

## 6. Skill subcommands

[JC] This is not only a process but a skill with real subcommands, one of which changes the workflow skill itself during work.

[Decided] Invocation `/partjob <subcommand> [args]` (`/skill:partjob ...` in Pi). The router table at the top of SKILL.md dispatches; each subcommand has a `commands/<name>.md` read only when invoked. No arguments means `status`. Plain-language arguments are routed to the closest subcommand.

| Subcommand | Phase | Does |
|---|---|---|
| `status` | any | Reads gig and JOB.md; reports where things stand, the next step, what waits on JC; lists unpaid and in-warranty orders. For the start of a session |
| `draft <slug>` | pre-order | Creates `.drafts/<slug>/NOTES.md` and the gig draft; the agent surveys the materials, lists questions and effort |
| `drop <slug> [reason]` | pre-order | Notes into gig, directory removed, dropped record |
| `start <slug>` | kickoff | Promotes the draft, scaffolds, drafts JOB.md / QUOTE.md, runs setup-matt-pocock-skills for large projects, points to grill |
| `decide <text>` | kickoff | Appends one of JC's decisions, dated, to "Confirmed decisions" |
| `ask` | kickoff | Turns open questions into a forwardable message under "Client questions" |
| `log` | kickoff | Appends a status entry at the end of a phase (commands, figures, commit, remaining), rewrites "Now", reports the `.scratch/` size |
| `dogfood [version]` | delivery | JC tries the version before any preview; a run command or local build, a checklist from the decisions; findings become decisions |
| `preview` | delivery | A candidate in `.scratch/preview/` for JC first; then builds `<slug>-vX.Y.Z-preview/` from the type's default list, writes the manifest, runs `gig package check` |
| `build [targets] [--via actions\|codebuild]` | delivery | Executables for orders whose decisions call for them: GitHub Actions (windows, linux, macos) by default, AWS CodeBuild (windows) when the source must not go to a git host; records commit and run or build id |
| `pack [id]` | delivery | Builds the full package with manifest and check; the reproduction check happens here |
| `send <id> [--via oss\|phone]` | delivery | After JC's approval, `gig package upload` or gsconnect; reports the short link or delivery |
| `revise` | delivery / warranty | Compares client feedback with the decisions: rework or scope change; rework adds decisions, scope changes run `gig change` |
| `paid [date]` | warranty | `gig paid`, updates QUOTE.md, computes the warranty end |
| `archive` | warranty | After the warranty: reports the dirty state and deletion list, then `gig archive` after approval |
| `handoff` | any | The handoff skill with the partjob items (state, path, writable scope, verification command) |
| `rule` | any | Changes the skill itself, see below |

The `rule` mechanism:
- `/partjob rule <one sentence>`, for example `/partjob rule reports in packages are PDF only`. Without arguments the agent distils this session's lesson.
- The agent first classifies the rule as generic (skill) or project-specific (the project's AGENTS.md), with its reason; JC may overrule.
- Generic rule: locate its place (a SKILL.md section or `references/*.md`), show the exact diff, write it after JC approves, add a `CHANGELOG.md` line (date, source project, one sentence), run the skill's contract tests.
- Conflicts: when the new rule contradicts an existing one, list the conflict and let JC choose to override or narrow.
- The skill lives in the gig repository; after writing, remind JC to commit. Changes take effect at the next invocation.
- Only the skill's text and references are edited, never gig code; gig changes are recorded in `TODO-gig.md`.

## Appendix A. What gig v2 remembers

- draft: slug, material path, creation date, outcome (promoted / dropped), drop reason, NOTES snapshot.
- order: slug, title, material_path, platform, external_id (optional), project_type, status, currency, price, cut_ratio, dev_path, archive_path, notes, client words, state timestamps, paid_at, warranty_until (= paid_at + 15 days).
- requirement_change: order, description, price delta, date.
- package: order, package_id, kind (preview / full), check time and result, send time, channel (oss / phone), remote URL, short link, expiry.
- artifact (single files sent outside a package): as in the v1 delivery_artifacts table.
- No longer needed: quote draft pricing fields, order_workflow, clients, sources, tags, templates.

Configuration: dev_root, archive_root, drafts_dir, default cut ratio, default currency, warranty days, OSS connection. Secrets stay out of config.toml, in the environment or a keyring-like secrets file.

## Appendix B. Migration inventory

The v1 database `~/.local/share/gig/gig.db`:

| Table | Rows | Handling |
|---|---|---|
| orders | 22 | All migrated, every column with a destination: source_org -> platform, notes -> notes, external_id -> external_id, quoted_price/final_price -> price (final wins, quoted goes to price_history), client_id/source_id dropped (their tables are empty). Timestamps are epoch-second strings, amounts are cents; both converted. #27's dev_path pointed at a renamed directory and was rewritten to patent-value-identification. |
| price_history | 13 | Migrated |
| requirement_changes | 5 | Migrated |
| delivery_packages | 38 | Migrated as kind=full; rows naming the same zip merged; missing local files flagged (sers's were gone) |
| delivery_artifacts | 40 | Migrated |
| quote_drafts | 8 | Promoted ones become a note on their order; unpromoted ones become dropped drafts (all 8 were promoted) |
| order_workflow | 9 | Dropped (its gig_dir supplied dev_path for orders that had none) |
| clients, sources, tags, order_tags | 0/0/5/4 | Dropped (tags became a note line) |

The OSS access key, secret and short-link token from `~/.config/gig/config.toml` moved to `secrets.toml`; the old file was deleted.

Migration touched only the database and configuration. v2's project rules apply to new orders; the existing four project directories were not retrofitted.

## Appendix C. Acceptance criteria (agreed with JC 2026-09-29)

### Before handover (done by the agent, results shown to JC)

1. Replay: from tk-dtf-compact's original request and materials, run `/partjob draft` through `/partjob pack`. Compared with the real JOB.md history: no question JC had already answered is asked; only template files are generated; the package content matches what was actually delivered.
2. Cold start: open a fresh session in bllc-reproduction and tk-dtf-compact, run only `/partjob status`; the agent states where things stand and what comes next, and JC can confirm it without adding context. Both projects were registered first with `gig new --adopt` from their QUOTE.md facts, touching no project file.
3. Pressure cases as automated tests: a package with a planted symlink, a `.gig/` file, a key file, a non-ASCII file name, a `..` path is rejected by the check; "continue" without approval sends nothing; an agent trying to change QUOTE.md stops.
4. Migration reconciliation: `gig migrate --dry-run` matches the v1 database row by row: 22 orders, correct amounts and dates.

All four had to pass before handover.

Rehearsal record (2026-09-29, all in a scratch GIG_HOME; the real database and project files were untouched):
1. Replay: tk-dtf-compact went from `gig draft new` to `gig archive --yes`. The only inputs were the four facts already in QUOTE.md (price, words, type, material path); nothing was asked twice. The generated JOB.md skeleton has three sections more than the real tk one ("Client request", "Pre-order notes", "Client questions"); the rest matches. The real delivery content (158 MB, with batch results carrying Chinese names) passed the check at once with `--client-named results/`, all 21 exempted files listed as warnings.
2. Cold start: new sessions in tk-dtf-compact, bllc-reproduction and the `~/dev/partjobs` root each ran `/partjob status`; all three reported the state, the next step, the unpaid order (sers, 33 days) and the order due for archiving correctly, and pointed out that bllc's JOB.md is not in template form.
3. Pressure cases: in gig's unit and end-to-end tests (symlink, hidden file, `..`, backslash, key-like names, non-ASCII names, unlisted files, zip differing from the manifest, package changed after its check). All rejected.
4. Reconciliation: 22 orders matched on id, slug, mapped status, price, platform and currency; the result is in `~/dev/partjobs-workflow/migration/reconcile.txt`.

### After handover (the per-order scorecard, stored in gig at archive time)

| Metric | Good direction |
|---|---|
| Decisions JC had to make | few, none repeated |
| Questions the agent asked that JC had already answered | 0 |
| Days from kickoff to the first preview | compared with history |
| Times JC cleaned up files by hand | 0 |
| Package check rejections and why | only the ones that should be rejected |
| Report reworks | toward 0 |
| JC's score, 1 to 5 | rising |

`/partjob archive` prompts for it; `/partjob rule` changes go into CHANGELOG, which shows whether the process is getting better or merely more complicated.
