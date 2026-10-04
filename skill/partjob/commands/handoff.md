# handoff

Hand over to the next session or a sub-agent. Use the `handoff` skill, but the handoff document must contain the items below; a missing one means the next agent asks JC again.
In Claude Code `handoff` is user-only: write the document directly by its rules (save to the OS temp directory, not the project; add a "suggested skills" section; reference existing artifacts by path; redact secrets).

## Required items

- Order slug, current gig state (`order.status` and `next_action` from `gig show`).
- Absolute project path, files to read first (`.gig/JOB.md`, `.gig/QUOTE.md`, `AGENTS.md`, relevant CONTEXT.md / ADRs).
- Where writing is allowed (which directories) and what must not be touched (original materials, QUOTE.md, checked packages under `delivery/`).
- Completion criteria: cite the numbered "Confirmed decisions" in JOB.md; do not write a second set.
- The latest verification command and its last result.
- Open items: unanswered "Client questions", actions waiting on JC's approval.
- Sub-agent limits: no approving, sending, scope changes, deleting, archiving; no edits to QUOTE.md or confirmed decisions.

## Reply

The handoff document path, and one sentence that the next session should start with `/partjob status`.
