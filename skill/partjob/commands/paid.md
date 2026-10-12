# paid [date] [amount]

JC says the client confirmed on the platform (confirmed receipt or approved settlement), or that the platform paid the money out.

The client pays into the platform's escrow when ordering (QUOTE.md "Payment": in escrow). Two later dates matter: the client's confirmation, which `gig paid` records and the warranty counts from, and the platform's payout to JC, which comes on the platform's own schedule (weekly, monthly or ad hoc) and is recorded with `--received`.

## Steps

1. Ask which of the two happened when JC's words do not say ("the money arrived" is usually the payout).
2. Client confirmed: the order must be `delivered`; otherwise report the current state, do not force it. `gig paid --order <slug> [--date YYYY-MM-DD] [--amount 800] [--warranty-start YYYY-MM-DD] [--received YYYY-MM-DD]`. The date defaults to today; give the amount only when the settled price differs from QUOTE.md (it goes into price_history). `--warranty-start` when the warranty counts from another day, for example a remote session that completes the delivery (also allowed later on a `paid` order). `--received` when the payout came the same day.
3. Platform paid out: `gig paid --order <slug> --received YYYY-MM-DD` on a `paid` or `archived` order.
4. The returned `warranty_until` is the end of the warranty, `received_at` the payout date. Change "Payment" in QUOTE.md to "client confirmed YYYY-MM-DD, warranty until <warranty_until>", then add "paid out YYYY-MM-DD" when the payout arrives (a commercial fact, editable only because JC stated it).
5. Append a JOB.md "Status" entry.

## Reply

The confirmation date, the warranty end, and whether the payout is still open (`awaiting_payout`). During the warranty JC answers client questions; changes go through `/partjob revise`; after the warranty `/partjob archive`, which an open payout does not block.
