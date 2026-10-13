# 0002 baidupcs uploader replaces bdpan

Status: accepted, 2026-10-13. Supersedes 0001.

## Context

The `bdpan` uploader (ADR 0001) drove the official Baidu Netdisk CLI. In
practice it often failed, and it printed no progress, so a 32.7 GB package gave
no sign of how far it had got. JC wants it gone.

`baidupcs-go` (Gentoo `net-misc/baidupcs-go`, v4.0.2-dev) reaches the same
Baidu account. A live probe on 2026-10-13 showed what gig has to cope with:
uploads need both BDUSS and STOKEN in its login; `upload` does not create
parent directories; exit codes are not reliable (`mkdir` on an existing
directory, `meta` on a missing path and an upload skipped because the name
exists all exit 0); upload progress comes as `\r`-separated
`↑ <sent>/<total> <rate> in <t>` lines about every 3 s; and `share set
--period 0` means a permanent share.

## Decision

- An uploader named `baidupcs` runs `baidupcs-go` as a subprocess: `who`,
  `mkdir <dir>`, `upload <local> <dir>`, `meta <file>`, `share set --period
  <days> -f <file>`. The remote directory is `<remote_root>/<dir part of the
  object key>` under a full-netdisk `remote_root` (default `/gig`).
- Every step is judged by its text output, with the checks the PRD lists:
  `who` must name an account (`当前帐号 uid:`, uid not 0); an existing directory
  is fine; the upload needs exit 0 and the `上传文件成功, 保存到网盘路径:` line;
  `meta`'s `文件大小` must equal the local size; the share reply must carry a
  `https://pan.baidu.com/` link with `pwd=<code>`. Output naming `STOKEN`, or
  a `who` without an account, is `secrets` with the hint
  `! baidupcs-go login -bduss=<BDUSS> -stoken=<STOKEN>`.
- The upload's progress lines are parsed and reported, so the CLI (stderr) and
  the TUI (progress bar) show real bytes. The upload has no timeout; the other
  calls time out after 60 s.
- The share period is `ceil(link_ttl_seconds / 86400)` days, at least 1, no
  cap, and always passed, so a share is never permanent.
- gig never reads or stores Baidu credentials; baidupcs-go keeps its own login.
  The result is still a **Pan Share**, never wrapped in a **Short Link**, and
  the channel stays `pan`.
- The old uploader is removed with its config table, doctor check and TUI
  choice. Database rows that say `uploader = "bdpan"` stay readable (the
  column is text); nothing migrates them.

## Consequences

- gig depends on baidupcs-go's Chinese text output. An update that rewords a
  line breaks parsing; the fake baidupcs-go in the tests replays the probed
  output, and the opt-in live test (`GIG_LIVE_BAIDUPCS=1`) is where a change
  shows up.
- JC must log baidupcs-go in with both BDUSS and STOKEN. `gig doctor` can only
  see whether an account is logged in; a missing STOKEN shows up at the first
  upload, as `secrets` with the login hint.
- Paths are full netdisk paths (`/gig/...`), not under an app root. Remote
  files accumulate under `<remote_root>/`; every upload gets a new timestamped
  directory and gig never deletes remote files.
- Baidu's per-file size limit depends on the account's membership. gig does no
  size pre-check and passes baidupcs-go's error through.
- The old uploader's config table is now an unknown field and makes the
  config invalid, so the cutover edits the config and installs the new gig
  together.
