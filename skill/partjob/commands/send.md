# send <package-id> [--via pan|oss|phone]

Send a checked package out. JC approves; the agent acts. Without JC's explicit "send" in the current turn, only rehearse.

## Preconditions

- The package was built or checked and not changed since. A change fails with `needs_check`; run `gig package check <package-id>` first.
- Channel: `pan` by default (upload to Baidu Netdisk with the `bdpan` uploader and get a Pan Share: a share link with the extraction code in it; JC sends the link to the client; clients are used to it). `oss` uploads to object storage with an `s3:<name>` uploader and gets a short link; the name is the key under `gig config get delivery.s3`. `phone` sends the zip to JC's phone via gsconnect; JC forwards it.

## Steps

1. Rehearse: `gig package upload <package-id>` (no `--yes`). It returns `dry_run: true`, `size`, `uploader` (only the configured default, not the channel JC picks; left out when none is set), `warnings`. A `config` error here means `delivery.uploader` names an uploader gig does not know; stop and say so (`gig doctor` names it). Tell JC the package id, size, kind (preview / full), the state change it causes (a full package moves the order to delivered) and the intended channel, then ask in one sentence: `网盘, OSS, or phone?`, plus "Send?". Default to pan when JC named no channel, but always offer the other two so one word switches it.
2. After JC explicitly agrees:
   - pan: `gig package upload <package-id> --uploader bdpan --yes`. It returns `url` (the share link with `?pwd=` in it), `pwd` (the extraction code on its own), `expires_at` (Baidu ends the share then) and no `short_url`; a Pan Share is never shortened. A `secrets` error means bdpan is not logged in: ask JC to run `! bdpan login`, then retry; the agent cannot log in for JC. An `upload` error before the upload (bdpan's `whoami` failed or timed out) carries bdpan's message: report it; a logged-out bdpan is one possible cause. A failed upload after the file reached the netdisk names the remote path; a retry uploads again to a new directory.
   - oss: `gig package upload <package-id> --uploader s3:<name> --yes`. It returns `short_url` (or `url` when short links are off) and `expires_at`.
   - Always pass `--uploader` for pan and oss, so the channel does not depend on the configured default. A `secrets` or `config` error otherwise means the environment is not set up; stop and say so, never work around it (`gig doctor` names what is missing).
   - phone: first run the `gsconnect-send` skill on `delivery/<package-id>.zip`, with bash (its script uses bash arrays; the Claude Code shell may be zsh); once delivered, `gig package sent <package-id> --channel phone --yes --note "gsconnect"`.
3. JOB.md "Status": time sent, channel, link (for pan the full `url`, which carries the code) or "sent to phone", expiry. Replace "not sent out yet".
4. After a full package the order is `delivered` and the next step is payment; a package sent during the warranty (`paid`) changes no state and only records a warranty revision.

## Reply

The full link (easy to copy) and its expiry date; for pan also the extraction code `pwd` on its own line, in case the client types it by hand. Or "sent to the phone": GSConnect accepted the transfer, whether the phone received it is not known, JC checks. The order's current state. If this session has already been compacted, suggest `/partjob handoff` and a fresh session that starts with `/partjob status`. Do not dress anything beyond the link up as a message for the client; client communication is JC's.
