# send <package-id> [--via pan|oss|phone]

Send a checked package out. JC approves; the agent acts. Without JC's explicit "send" in the current turn, only rehearse.

## Preconditions

- The package was built or checked and not changed since. A change fails with `needs_check`; run `gig package check <package-id>` first.
- Channel: `pan` by default (upload to Baidu Netdisk with the `baidupcs` uploader and get a Pan Share: a share link with the extraction code in it; JC sends the link to the client; clients are used to it). `oss` uploads to object storage with an `s3:<name>` uploader and gets a short link; the name is the key under `gig config get delivery.s3`. `phone` sends the zip to JC's phone via gsconnect; JC forwards it.
- JC already sent it (by hand, from the VM, through the platform): skip the steps and follow "Sent by JC" below.

## Steps

1. Rehearse: `gig package upload <package-id>` (no `--yes`). It returns `dry_run: true`, `size`, `uploader` (only the configured default, not the channel JC picks; left out when none is set), `warnings`. A `config` error here means `delivery.uploader` names an uploader gig does not know, or the baidupcs-go binary does not resolve; stop and say so (`gig doctor` names it). Tell JC the package id, size, kind (preview / full), the state change it causes (a full package moves the order to delivered) and the intended channel, then ask in one sentence: `网盘, OSS, or phone?`, plus "Send?". Default to pan when JC named no channel, but always offer the other two so one word switches it.
   Once JC names pan or oss, rehearse again with that `--uploader` (`baidupcs` or `s3:<name>`): that dry run checks the chosen uploader, and for baidupcs its binary.
2. After JC explicitly agrees:
   - pan: `gig package upload <package-id> --uploader baidupcs --yes`. It returns `url` (the share link with `?pwd=` in it), `pwd` (the extraction code on its own), `expires_at` (Baidu ends the share then) and no `short_url`; a Pan Share is never shortened. A `secrets` error means baidupcs-go is not logged in, or its login lacks the STOKEN: ask JC to run `! baidupcs-go login -bduss=<BDUSS> -stoken=<STOKEN>`, then retry; the agent cannot log in for JC and never asks for the values. An `upload` error carries baidupcs-go's own words and the remote path: report it. Baidu's per-file size limit depends on JC's membership and gig does not check it first, so a file that is too large fails here. A retry uploads again to a new directory.
   - oss: `gig package upload <package-id> --uploader s3:<name> --yes`. It returns `short_url` (or `url` when short links are off) and `expires_at`.
   - Always pass `--uploader` for pan and oss, so the channel does not depend on the configured default. A `secrets` or `config` error otherwise means the environment is not set up; stop and say so, never work around it (`gig doctor` names what is missing). `baidupcs-go binary "baidupcs-go" not found` means it is not on the shell's PATH; the fix is JC's: install it, or `gig config set delivery.baidupcs.bin <absolute path>`.
   - Progress (pan and oss): while it uploads, gig prints lines `upload <sent> / <total> (<pct>%) <rate>/s ETA <eta>` on stderr, one each time another 10% is done or a minute has passed; stdout stays the one JSON document. A package over 1 GB: run the upload as a long job in the background (one Monitor waiter, see SKILL.md) with stdout and stderr in separate files (`> .scratch/upload.json 2> .scratch/upload.log`), report each new progress line to JC (percent, rate, ETA), and read the JSON when it ends. A smaller package runs in the foreground.
   - phone: first run the `gsconnect-send` skill on `delivery/<package-id>.zip`, with bash (its script uses bash arrays; the Claude Code shell may be zsh); once delivered, `gig package sent <package-id> --channel phone --yes --note "gsconnect"`.
3. JOB.md "Status": time sent, channel, link (for pan the full `url`, which carries the code) or "sent to phone", expiry. Replace "not sent out yet".
4. After a full package the order is `delivered` and the next step is the client's confirmation on the platform (`paid`); a package sent during the warranty (`paid`) changes no state and only records a warranty revision.

## Sent by JC

1. Confirm with JC which package the client got: the package id and the first 12 characters of its sha256 (`gig package ls`).
2. Rehearse `gig package sent <package-id> --channel other --note "<how it was sent>"` and show the result.
3. After JC agrees, run it with `--yes`. No link and no expiry; JOB.md "Status" records how it was sent. Step 4 above applies.

## Reply

The full link (easy to copy) and its expiry date; for pan also the extraction code `pwd` on its own line, in case the client types it by hand. Or "sent to the phone": GSConnect accepted the transfer, whether the phone received it is not known, JC checks. The order's current state. After a full package, the `.scratch/` size with one question whether to clear it. Do not dress anything beyond the link up as a message for the client; client communication is JC's.
