# rule <one sentence>

Change this skill itself. Use it when something went wrong during work, or JC says "from now on, do it this way". Without arguments, distil this session's lesson and list candidates for JC to choose from.

## Steps

1. Decide whether the rule is generic or project-specific:
   - Generic (applies to every future order): goes into the skill, in `SKILL.md`, a `commands/<subcommand>.md`, `references/workflow.md`, or a file under `templates/`.
   - Project-specific (this order only): goes into the "Project-specific" section of the project's `AGENTS.md`, not into the skill.
   State the judgement and the reason; JC may overrule.
2. Generic rule: find the place, show the exact diff (before / after). When it contradicts an existing rule, list the conflicting items and let JC choose to override or narrow.
3. After JC approves the diff:
   - Write the file.
   - Append one line to `CHANGELOG.md`: `- YYYY-MM-DD (<source slug or "chat">): <one sentence>`.
   - Run `python3 -m unittest discover -s tests` in the skill directory. On failure roll back and report.
   - Remind JC the skill lives in the `~/Documents/dev-tools/gig` repository and needs a commit (the agent may commit locally; it never pushes).
4. Anything that needs a gig code change: do not touch the code; append an entry to `TODO-gig.md` (date, need, source) and tell JC.
5. The change takes effect at the next invocation. Follow the new rule in the current session as well.

## Limits

- Never delete an existing rule; override or narrow it, with a CHANGELOG line.
- Never edit paragraphs tagged [JC] in `references/workflow.md` unless JC explicitly says that paragraph is what changes.
- Template edits keep the variable names gig renders (`slug`, `title`, `price`, `currency`, `today`, `material_path`, `client_words`, `draft_notes`, `project_type`, `platform`, `cut_ratio`, `warranty_days`).

## Reply

The judgement (generic / project), the diff, the test result, the CHANGELOG line.
