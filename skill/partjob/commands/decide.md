# decide <text>

Write one of JC's decisions into "Confirmed decisions" in `.gig/JOB.md`. Only JC adds decisions; the agent never edits old ones.

## Steps

1. A decision is an executable constraint, one sentence with its object and rule. Example: "merge distance 2.5 cm; flag 1.5 to 4 cm for review". Restate a vague one and let JC confirm first.
2. Append to the end of "Confirmed decisions" with the next number, the date and how it was confirmed: `N. (2026-09-29, confirmed in chat) ...` or `(2026-09-29, confirmed by grilling)`.
3. When it contradicts an earlier decision, keep the old one. The new entry says "supersedes item M", and a line in "Status" records why (for example "JC found the old labels wrong").
   Then search the project's derived text for the superseded rule: README.md, PRODUCT.md, AGENTS.md, CONTEXT.md, `docs/`, the notes of the current preview candidate, user-facing strings in the code, and the order title in `gig show`. Update each, or list what still carries the old rule. Packages already built under `delivery/` stay as they are.
4. When the decision changes the deliverable scope or the price: stop and ask JC whether it is rework or a scope change (see `revise.md`); a scope change runs `gig change --desc "..." [--price-delta ...]`.
5. When it touches commercial facts in QUOTE.md (price, payment terms, warranty): that file is JC's. Change it only when JC states the new content explicitly, and record it with `gig price` or `gig change` at the same time.

## Reply

The number and text of the new decision, whether it supersedes an old item (and which documents were updated or still carry the old rule), whether it triggered a scope change.
