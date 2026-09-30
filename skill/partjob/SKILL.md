---
name: partjob
description: JC's freelance order workflow. From the pre-order draft directory through delivery, payment, warranty and archive, with the agent recording state in gig. Invoked by hand, with subcommands.
argument-hint: "<subcommand> [args]. No args = status. Subcommands: status draft drop start decide ask log preview build pack send revise paid archive handoff rule"
disable-model-invocation: true
---

# partjob

This is the workflow for JC's freelance orders. The full rules live in `references/workflow.md`; this file only routes and states the shortest constraints. gig is the data layer, used only by agents; every command prints one JSON document. See `references/gig.md`.

## Routing

When the first word of the arguments is a subcommand, read the matching `commands/<subcommand>.md` before acting; never run from memory. No arguments means `status`.

When the arguments are not a subcommand name (JC speaks plainly: `/partjob the client wants no watermark`, `/partjob money arrived`): pick the closest subcommand by intent, say which one in the first sentence of the reply ("handling as revise"), then follow that file. If two subcommands fit, or the intent is unclear, ask one question before acting; do not guess. Free text after a subcommand works the same way (`/partjob send by phone` = `send --via phone`).

| Subcommand | Phase | One line |
|---|---|---|
| `status` | any | Where things stand, the next step, what waits on JC, unpaid and in-warranty orders |
| `draft <slug> [material path]` | pre-order | Create the draft directory and NOTES.md, survey the materials, list questions and effort |
| `drop <slug> [reason]` | pre-order | Give up: notes go into gig, directory removed |
| `start <slug>` | kickoff | Register the order, scaffold the project, set up agent rules, go to grill |
| `decide <text>` | kickoff | Write one of JC's decisions into JOB.md "Confirmed decisions" |
| `ask` | kickoff | Turn open questions for the client into a forwardable message in JOB.md |
| `log` | kickoff | End of a phase: append a status entry |
| `preview [version]` | delivery | Build and check a preview package |
| `build [targets] [--via actions\|codebuild]` | delivery | Executables, only for orders whose decisions call for them; GitHub Actions by default, CodeBuild when the source must not go to a git host |
| `pack [version]` | delivery | Build the full package, run the reproduction check, validate |
| `send <package-id> [--via oss\|phone]` | delivery | Upload or send to the phone after JC approves |
| `revise` | delivery / warranty | Client feedback: rework or scope change |
| `paid [date] [amount]` | warranty | Record payment, compute the warranty end |
| `archive` | warranty | Scorecard, report the dirty state, archive after approval |
| `handoff` | any | Hand over to the next session |
| `rule <one sentence>` | any | Change this skill itself |

## Actions only JC can approve

Silence, "continue", or an "ok" with no specific object is not approval. Approval covers one action, once.

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
- At the end of every phase append to JOB.md "Status": commands, verification figures, commit / CI run, remaining work, whether anything was sent out.
- Deliverables must reproduce: rerun the delivered program and compare byte for byte with the source run; record it in JOB.md.
- Commit each verified unit of work. Before a long experiment commit the code, configuration and evaluation protocol first. Check the staged diff and keep JC's concurrent edits. Never push.
- Temporary files and recovery copies go in `.scratch/` inside the project, never in a parent directory. Clear it at the end of a phase.
- Use only the materials at hand. Questions for the client go into "Client questions" (`ask`); never assume, never ask the client for more samples unless JC decides to.
- Working directory and file names are English throughout, inside client packages too. Chinese prose uses ASCII punctuation. Explain concepts with concrete names, no undefined abbreviations.
- Reports use Kami or LaTeX; only the final PDF goes into `reports/` or the package; sources live in `.scratch/reports/`, prose passes through sepia.
- Keep the tree clean: any top-level directory beyond the template is explained in README or AGENTS.md.
- Sub-agents do only the assigned work; they cannot approve, send, change scope, delete or archive. Handoff items are in `commands/handoff.md`.

## How to reply

Say what actually happened (commands and real results), what comes next, and what waits on JC. No fixed template, no "done" manufactured from chat momentum. Undecided things are stated as undecided.

## Layout

- `commands/`: one file per subcommand: preconditions, gig commands, files written, approvals needed, what to reply.
- `references/workflow.md`: the complete workflow and rules, the source of this skill.
- `references/gig.md`: gig command cheat sheet.
- `references/codebuild.md`: the AWS CodeBuild runbook used by `build --via codebuild`.
- `templates/`: files gig renders when scaffolding a project (`JOB.md.j2`, `QUOTE.md.j2`, `AGENTS*.md.j2`, `README.md.j2`, `gitignore`, `NOTES.md.j2`). `templates/build/` holds the build files `build` copies into a project (GitHub Actions workflow, CodeBuild buildspec and script); gig does not render them.
- `CHANGELOG.md`: every change made through `rule`.
- `TODO-gig.md`: things that need gig code changes; `rule` does not edit code.
