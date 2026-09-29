# draft <slug> [material path]

Pre-order. The client has sent a short request; nothing is agreed yet. The goal is to give JC a clear question list and an effort estimate to negotiate with. No formal project, no code.

## Preconditions

- slug: lowercase letters, digits, `-`, `_`, `.`. Given by JC or derived as a short English name from the request.
- The material path is usually `/mnt/virtiofs/<id>/`. Leave it empty if unknown.

## Steps

1. `gig draft new <slug> --material <path> --title "<one-line title>"`. A draft or order with the same slug fails with `invalid_input`; pick another slug or run `status` first.
2. Open the returned `notes_path` (`~/dev/partjobs/.drafts/<slug>/NOTES.md`). Paste the client's words verbatim under "Client words", noting the source file and encoding.
3. Survey the materials, read-only: file types, counts, sizes, encodings, anomalies. Write them under "Feasibility". Copy a few samples under `.drafts/<slug>/` if needed, never the whole batch.
4. "Questions": written so JC can forward them to the client as is, one question per line, no internal reasoning.
5. "Effort estimate": by phase, for JC's reference. Never quote the client; pricing is JC's.
6. "Budget and pricing": left for JC. When JC mentions a budget or price, record it.

## Never

- Create `~/dev/partjobs/<slug>/`, `git init`, write code, or run training.
- Contact the client. The question list goes to JC.

## Reply

The question list verbatim (so JC can forward it), the effort estimate, risks found in the survey. State that the next step is JC's negotiation; on acceptance `/partjob start <slug>`, on withdrawal `/partjob drop <slug>`.
