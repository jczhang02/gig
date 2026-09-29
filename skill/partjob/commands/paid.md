# paid [date] [amount]

JC says the money arrived.

## Steps

1. The order must be `delivered`. Otherwise report the current state; do not force it.
2. `gig paid --order <slug> [--date YYYY-MM-DD] [--amount 800]`. The date defaults to today; give the amount only when what was received differs from QUOTE.md (it goes into price_history).
3. The returned `warranty_until` is the end of the warranty. Change "Payment" in QUOTE.md to "paid YYYY-MM-DD, warranty until <warranty_until>" (a commercial fact, editable only because JC stated the payment).
4. Append a JOB.md "Status" entry.

## Reply

Payment date, amount, warranty end. During the warranty JC answers client questions; changes go through `/partjob revise`; after the warranty `/partjob archive`.
