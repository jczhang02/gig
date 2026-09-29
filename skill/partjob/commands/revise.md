# revise

The client responded to the preview or the package. Classify first, then act.

## Steps

1. Record the client's feedback verbatim in JOB.md "Status" (dated; say whether it is preview feedback or post-delivery feedback).
2. Compare each point against "Confirmed decisions" and QUOTE.md:
   - A promised item missing or wrong: rework. No price change.
   - Something outside the decisions and the quote: scope change. JC decides whether to do it free, charge, or decline.
   - Unclear: list the difference and let JC judge.
3. Rework:
   - Order is `delivered`: `gig start --order <slug>` returns it to in_progress.
   - Order is `paid` (warranty): no state change; just fix it.
   - New requirements from the feedback become decisions via `/partjob decide` (state what they supersede).
   - When done, a new version through `/partjob pack` or `/partjob preview`, then `/partjob send`.
4. Scope change (after JC decides): `gig change --desc "<change>" [--price-delta <difference>]`. If the price changed, the QUOTE.md price line is edited by JC, or by the agent after JC states the new price, with the date noted.

## Reply

The classification per point (rework / scope change / for JC to judge), the plan, and what JC has to decide.
