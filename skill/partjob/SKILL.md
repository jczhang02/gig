---
name: partjob
description: JC's freelance order workflow. From the pre-order draft directory through delivery, payment, warranty and archive, with the agent recording state in gig. Invoked by hand, with subcommands.
argument-hint: "<subcommand> [args]. No args = status. Subcommands: status draft drop start decide ask log dogfood preview build pack send revise paid archive handoff rule"
disable-model-invocation: true
---

# partjob

This is the workflow for JC's freelance orders. The full rules live in `references/workflow.md`; this file only routes and states the shortest constraints. gig is the data layer, used only by agents; every command prints one JSON document. See `references/gig.md`.

## Routing

When the first word of the arguments is a subcommand, read the matching `commands/<subcommand>.md` before acting; never run from memory. No arguments means `status`.

When the arguments are not a subcommand name (JC speaks plainly: `/partjob the client wants no watermark`, `/partjob money arrived`): pick the closest subcommand by intent, say which one in the first sentence of the reply ("handling as revise"), then follow that file. If two subcommands fit, or the intent is unclear, ask one question before acting; do not guess. Free text after a subcommand works the same way (`/partjob send by phone` = `send --via phone`).

Later in the same session, a request that maps to a subcommand ("make a preview", "the money arrived") re-reads that `commands/<subcommand>.md` before acting; the files change between sessions. Read the files directly: the Skill tool cannot load partjob, it is user-only.

| Subcommand | Phase | One line |
|---|---|---|
| `status` | any | Where things stand, the next step, what waits on JC, unpaid and in-warranty orders |
| `draft <slug> [material path]` | pre-order | Create the draft directory and NOTES.md, survey the materials, list questions and effort |
| `drop <slug> [reason]` | pre-order | Give up: notes go into gig, directory removed |
| `start <slug>` | kickoff | Register the order, scaffold the project, set up agent rules, go to grill |
| `decide <text>` | kickoff | Write one of JC's decisions into JOB.md "Confirmed decisions" |
| `ask` | kickoff | Turn open questions for the client into a forwardable message in JOB.md |
| `log` | kickoff | End of a phase: append a status entry |
| `dogfood [version]` | delivery | JC tries the work before any preview; findings become decisions |
| `preview [version]` | delivery | Show JC a preview candidate, then build and check the package |
| `build [targets] [--via actions\|codebuild\|vm]` | delivery | Executables, only for orders whose decisions call for them; the local Windows VM by default, CodeBuild when the VM cannot be used, GitHub Actions only when JC decides it (for example macOS) |
| `pack [version]` | delivery | Build the full package, run the reproduction check, validate |
| `send <package-id> [--via pan\|oss\|phone]` | delivery | Upload to Baidu Netdisk (default) or object storage, or send to the phone, after JC approves |
| `revise` | delivery / warranty | Client feedback: rework or scope change |
| `paid [date] [amount]` | warranty | Record payment, compute the warranty end |
| `archive` | warranty | Scorecard, report the dirty state, archive after approval |
| `handoff` | any | Hand over to the next session |
| `rule <one sentence>` | any | Change this skill itself |

## Actions only JC can approve

Silence, "continue", or an "ok" with no specific object is not approval. Approval covers one action, once.

- An "ok" to a plan item approves the plan, not an action whose object did not exist yet. Once the package, the push or the file list exists, show it and ask again.
- One yes to a numbered list approves each listed item with the content the list showed. Values the list did not show (for example QUOTE.md fields filled from defaults) are not approved: write "(to fill)" and ask.

1. Sending anything out: uploading a package, sending to the phone, sharing a link.
2. Pushing to a remote repository.
3. Deleting files, cleaning up a project, archiving.
4. Using paid remote resources.
5. Scope changes beyond the confirmed decisions in `.gig/JOB.md`.
6. Changing commercial facts in `.gig/QUOTE.md`.

JC approves; the agent acts. In gig these commands take `--yes`; without JC's explicit yes in the current turn, do not pass `--yes`.

## Rules for every subcommand

- Read `.gig/JOB.md` before working. Price and payment facts come only from `.gig/QUOTE.md`; quotes in old reports are void.
- Original materials are read-only. New results go to new directories, never overwriting old ones; failures and unusable states stay visible.
- Never tune numbers toward a paper or the client's expectations. Reported figures come from saved results, with the data split, configuration and aggregation stated.
- The two rules above govern project records (JOB.md, internal reports). Text written for the client (a manuscript, a manual) follows its field's conventions and JC's decisions on what to show.
- At the end of every phase append to JOB.md "Status": commands, verification figures, commit / CI run, remaining work, whether anything was sent out.
- Deliverables must reproduce: rerun the delivered program and compare byte for byte with the source run; record it in JOB.md.
- Commit each verified unit of work. Before a long experiment commit the code, configuration and evaluation protocol first. Check the staged diff and keep JC's concurrent edits; an uncommitted change may also come from another agent session, so check `git log` before calling it JC's. Never push.
- Temporary files and recovery copies go in `.scratch/` inside the project, never in a parent directory. Clear it at the end of a phase.
- Use only the materials at hand. Questions for the client go into "Client questions" (`ask`); never assume, never ask the client for more samples unless JC decides to. An unknown about the client that blocks a decision a second time becomes an `ask` item at once.
- Working directory and file names are English throughout, inside client packages too. Chinese prose uses ASCII punctuation; a client manuscript follows its target journal's punctuation. Explain concepts with concrete names, no undefined abbreviations.
- Reports for the client use Kami or LaTeX; only the final PDF goes into `reports/` or the package; sources live in `.scratch/reports/`, prose passes through humanizer (humanizer-zh for Chinese). Notes for JC alone may stay Markdown under `docs/research/`; material for JC alone that lives in the project goes in `reports/internal/` (export-ignored).
- A gig defect found during any subcommand goes into `TODO-gig.md` at once (date, need, source order), committed locally in the gig repository; say so in the reply.
- Runs of other agents (Codex, Open Design and the like) go to a workspace outside `~/dev/partjobs` with their own HOME and config home, so they read no project or personal rules; copy results into the project after each run. Before any metered call, name the provider explicitly and check that IDs are not empty: a fallback to another paid provider is unapproved spending.
- Keep the tree clean: any top-level directory beyond the template is explained in README or AGENTS.md.
- Sub-agents do only the assigned work; they cannot approve, send, change scope, delete or archive. Handoff items are in `commands/handoff.md`.
- Claude Code only: work on an order runs with the session's working directory at its project directory. When the session runs elsewhere (for example `~/dev/partjobs`), ask JC to run `/cd ~/dev/partjobs/<slug>` before writing project files; a shell `cd` outside the session directory is reset. Long jobs run in the background with one waiter (Monitor); a job started with `&` in a foreground call sends no notification. Not for `draft`: `.drafts/<slug>/` is removed by `start` and `drop`.

## How to reply

Say what actually happened (commands and real results), what comes next, and what waits on JC. No fixed template, no "done" manufactured from chat momentum. Undecided things are stated as undecided.

Reply in JC's language (Chinese by default), whatever language these files are in. Give absolute paths for files JC is meant to open. A question still waiting on JC goes into JOB.md "Now" once, not into every reply. Text JC forwards to the client is plain numbered lines: no tables, bold or code blocks.

## Layout

- `commands/`: one file per subcommand: preconditions, gig commands, files written, approvals needed, what to reply.
- `references/workflow.md`: the complete workflow and rules, the source of this skill.
- `references/gig.md`: gig command cheat sheet.
- `references/codebuild.md`: the AWS CodeBuild runbook used by `build --via codebuild`. `references/vm.md`: the local Windows VM runbook used by `build --via vm`.
- `scripts/vm-build.sh`: the host-side runner for `build --via vm`; it stays in the skill and is not copied into projects.
- `templates/`: files gig renders when scaffolding a project (`JOB.md.j2`, `QUOTE.md.j2`, `AGENTS*.md.j2`, `README.md.j2`, `gitignore`, `gitattributes`, `NOTES.md.j2`). `templates/build/` holds the build files `build` copies into a project (GitHub Actions workflow, CodeBuild buildspec, and the Windows build script that CodeBuild and the VM share); gig does not render them.
- `CHANGELOG.md`: every change made through `rule`.
- `TODO-gig.md`: things that need gig code changes; `rule` does not edit code.
