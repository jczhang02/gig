# send <package-id> [--via oss|phone]

Send a checked package out. JC approves; the agent acts. Without JC's explicit "send" in the current turn, only rehearse.

## Preconditions

- The package was built or checked and not changed since. A change fails with `needs_check`; run `gig package check <package-id>` first.
- Channel: `oss` by default (upload to object storage, get a short link, JC sends the link to the client). `phone` sends the zip to JC's phone via gsconnect; JC forwards it.

## Steps

1. Rehearse: `gig package upload <package-id>` (no `--yes`). It returns `dry_run: true`, `size`, `warnings`. Tell JC the package id, size, kind (preview / full), the state change it causes (a full package moves the order to delivered) and the intended channel, then ask in one sentence: "OSS with a short link, or to the phone? Send?". Default to OSS when JC named no channel, but always offer the other so one word switches it.
2. After JC explicitly agrees:
   - oss: `gig package upload <package-id> --yes`. It returns `short_url` (or `url` when short links are off) and `expires_at`. A `secrets` or `config` error means the environment is not set up; stop and say so, never work around it.
   - phone: first run the `gsconnect-send` skill on `delivery/<package-id>.zip`, with bash (its script uses bash arrays; the Claude Code shell may be zsh); once delivered, `gig package sent <package-id> --channel phone --yes --note "gsconnect"`.
3. JOB.md "Status": time sent, channel, link or "sent to phone", expiry. Replace "not sent out yet".
4. After a full package the order is `delivered` and the next step is payment; a package sent during the warranty (`paid`) changes no state and only records a warranty revision.

## Reply

The full link (easy to copy) and its expiry date, or "sent to the phone": GSConnect accepted the transfer, whether the phone received it is not known, JC checks. The order's current state. If this session has already been compacted, suggest `/partjob handoff` and a fresh session that starts with `/partjob status`. Do not dress anything beyond the link up as a message for the client; client communication is JC's.
