# CHANGELOG

One line per change made through `/partjob rule`: date (source), one sentence.

- 2026-10-12 (chat): payment follows the platform's escrow: start records the escrow in QUOTE.md only, `paid` records the client's confirmation (warranty from then, or `--warranty-start`), `paid --received` records the platform's payout; status lists orders awaiting confirmation or payout.
- 2026-10-12 (chat): setup or service orders are `custom`; pack and preview get "Setup jobs" items (install manual, clean-environment install check, screenshots only in previews).
- 2026-10-12 (retro): later requests in a session re-read the command file; the Skill tool cannot load partjob; `~/dev/partjobs/AGENTS.md` and every AGENTS template point sessions started without `/partjob` to the skill.
- 2026-10-12 (retro): `.gitattributes` template marks `.gig`, AGENTS.md and other project records `export-ignore`; pack relies on it and gig's package check scans nested zips.
- 2026-10-12 (retro): start asks once about client payment vs JC's share (`--cut-ratio`), prepayment, and whose words they are; QUOTE.md holds only facts JC stated; a new "Adopting a project set up by hand" checklist.
- 2026-10-12 (retro): new `AGENTS.research_writing.md.j2` (terms from the manuscript and literature only, field reporting conventions, grill first, Zotero first, private LaTeX build); writing previews are watermarked results pages.
- 2026-10-12 (retro): project-record rules (failures visible, aggregation stated) do not govern client text; manuscripts follow the journal's punctuation; notes for JC alone may be Markdown, JC-only material goes in `reports/internal/`.
- 2026-10-12 (retro): send gets "Sent by JC" (`--channel other`), a second rehearsal with the chosen `--uploader`, and the `delivery.bdpan.bin` fix; status lists legacy orders and full packages never recorded as sent.
- 2026-10-12 (retro): pack lists top-level entries with sizes before building, checks Windows reproduction on the same VM (vm.md "Reproduction check") and output metadata, edits a patch report in place, runs the humanizer skill itself, and reports paths and the `.scratch/` size.
- 2026-10-12 (retro): replies use JC's language, absolute paths, and plain numbered lines for forwardable text; a pending question goes into "Now" once; a client unknown that blocks twice becomes an `ask` item.
- 2026-10-12 (retro): gig defects go into TODO-gig.md at once and are committed; rule deletes a rule when JC asks instead of adding its opposite (rule.md limit deleted).
- 2026-10-12 (retro): other agents run outside `~/dev/partjobs` with their own HOME; metered calls name the provider; long jobs use one background waiter; archive checks `git ls-files` before deleting; decide lists data of a superseded route; log counts repeat questions for the scorecard.
- 2026-10-12 (retro): CodeBuild runbook gets a TLS EOF row and a one-time credit question; gig.md documents `--cut-ratio`, `--currency`, output shapes, the full banned-name list and the gig source path.
- 2026-10-12 (retro): build defaults to `--via vm`; CodeBuild when the VM cannot be used; GitHub Actions only when JC decides it (for example macOS), and naming that channel does not approve the push.
- 2026-10-12 (retro): an "ok" to a plan item does not approve an action whose object did not exist yet; show the object and ask again (thermal-video-curve send).
- 2026-10-12 (retro): a yes to a numbered list approves only the items and content shown; unshown values such as QUOTE.md defaults stay "(to fill)" (bllc-reproduction adopt).
- 2026-10-12 (thermal-video-curve): send no longer suggests handoff or a fresh session after a compacted session; the sentence is deleted at JC's explicit request (removes part of the 2026-10-08 send rule).
- 2026-10-08 (chat): build gets `--via vm`, a free Windows channel on the local libvirt VM `win11` run by `scripts/vm-build.sh` (runbook in references/vm.md; a VM it started is shut down afterwards); `codebuild.ps1` becomes the shared `build-windows.ps1` (UTF-8 console, uv installed once, onedir zips with tar.exe); status checks the VM for vm orders.
- 2026-10-08 (bdpan-uploader): send gets `--via pan` (Baidu Netdisk, the default) beside oss and phone, asks `网盘, OSS, or phone?`, passes `--uploader` explicitly and reports the Pan Share link with its code and `pwd`.
- 2026-10-08 (thermal-video-curve): new `dogfood` subcommand before `preview`; JC tries each version and findings become decisions.
- 2026-10-08 (thermal-video-curve): preview shows a candidate in `.scratch/preview/` first and registers only the agreed version in gig; it reports `delivery/` entries gig does not know.
- 2026-10-08 (thermal-video-curve): decide searches derived documents and the gig title for a superseded rule and updates or lists them.
- 2026-10-08 (thermal-video-curve): send runs gsconnect-send with bash and states that phone-side receipt is unknown; suggests handoff after a compacted session.
- 2026-10-08 (thermal-video-curve): status checks the AWS session for CodeBuild orders; build copies build files only after the channel decision.
- 2026-10-08 (thermal-video-curve): start copies the applicable rules of a second project type by hand (gig change in TODO-gig.md).
- 2026-10-08 (thermal-video-curve): JOB.md gets a rewritten "Now" section (template, log, status, handoff); log reports the `.scratch/` size and asks once about clearing it.
- 2026-10-04 (chat): start runs setup-matt-pocock-skills only for projects the agent judges large (stated to JC, who may overrule); small projects track tasks in JOB.md.
- 2026-10-04 (chat): sepia removed at JC's request; report prose passes through humanizer (humanizer-zh for Chinese) in SKILL, pack, workflow, AGENTS templates and the contract test.
- 2026-10-04 (chat): in Claude Code the matt skills are user-only: start asks JC to run `/mattpocock-skills:setup-matt-pocock-skills` and names the grill commands with the plugin prefix; handoff writes the document directly by the handoff skill's rules.
- 2026-10-04 (chat): Claude Code sessions move to the project directory with `/cd` (start step 2 and a generic rule); drafts excluded since their directory is removed.
- 2026-09-29 (chat): new `build` subcommand, opt-in per order via a confirmed decision: GitHub Actions (windows, linux, macos) or AWS CodeBuild (windows, no git hosting); runbook in references/codebuild.md, build files in templates/build/ (the English test now walks template subdirectories); pack, workflow section 5 and 6, AGENTS.tool and gitignore narrowed to match.
- 2026-09-29 (chat): skill, templates and workflow document rewritten in English at JC's request.
- 2026-09-29 (chat): routing accepts plain-language arguments and names the chosen subcommand; send asks which channel.
- 2026-09-29 (rewrite): partjob v2 initial version. Workflow in references/workflow.md, gig v2 in references/gig.md.
