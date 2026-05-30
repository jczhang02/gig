# gig

`gig` is a local-first freelance order management tool. This context names the customer-delivery workflow concepts that must stay consistent across CLI, GUI, and agent-written plans.

## Language

**Order**:
A client work record tracked from lead or negotiation through delivery, payment, and archive.
_Avoid_: Job, deal, task

**Quote Draft**:
A pre-order pricing proposal that may need clarification, be quoted, be accepted into an **Order**, or be dropped.
_Avoid_: Estimate, bid, draft order

**Workflow Gate**:
A required checkpoint that must be satisfied before an **Order** may advance to the next delivery state.
_Avoid_: Step, status, checklist item

**Client Package**:
The canonical deliverable bundle prepared for a client from an approved package manifest.
_Avoid_: Release zip, delivery folder, upload bundle

**Delivery Artifact**:
An additional client-facing file shared for an **Order** outside the canonical **Client Package**.
_Avoid_: Attachment, asset, upload

**Short Link**:
A stable client-facing redirect URL that points to an expiring delivery URL.
_Avoid_: CDN URL, storage URL, proxy link

## Relationships

- A **Quote Draft** may be accepted into at most one **Order**.
- An **Order** advances through **Workflow Gates** before final delivery.
- A **Client Package** belongs to exactly one **Order**.
- A **Delivery Artifact** belongs to exactly one **Order**.
- A **Short Link** points to one expiring delivery URL for a **Client Package** or **Delivery Artifact**.

## Example dialogue

> **Dev:** "Can the GUI send this **Order** now?"
> **Domain expert:** "Only if the required **Workflow Gates** are complete and the **Client Package** has been validated. Extra files can be sent as **Delivery Artifacts**, but they do not replace the package gate."

## Flagged ambiguities

- "package" should mean **Client Package** when discussing the canonical final bundle; use **Delivery Artifact** for extra one-off files.
- "short link" should mean a redirecting **Short Link**, not a storage endpoint, CDN hostname, or proxy download service.
