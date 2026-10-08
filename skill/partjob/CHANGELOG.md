# CHANGELOG

One line per change made through `/partjob rule`: date (source), one sentence.

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
