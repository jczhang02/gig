# 0001 bdpan uploader drives the official CLI

Status: accepted, 2026-10-08

## Context

Clients are used to Baidu Netdisk (百度网盘) links and get a file from a Baidu
share more easily than from object storage. Until now gig delivered a **Client
Package** or **Delivery Artifact** only through S3 (Aliyun OSS) plus a **Short
Link**. JC lives in the US and has no other use for Baidu Netdisk, so gig must
not make him manage Baidu credentials or keep a Baidu SDK up to date.

Baidu offers two ways in: the Open Platform HTTP API (OAuth app, access and
refresh tokens, chunked upload protocol) and the official `bdpan` CLI, which
owns its own login and has a `--json` mode. bdpan 3.8.7 was verified live:
upload creates missing parent directories, `share --period` accepts 0, 1, 7 or
30 days and returns a link plus an extraction code, and a failed upload can
still exit 0 with `code != 0` in its JSON.

## Decision

- Add an uploader named `bdpan` that runs `<bin> --json --no-check-update
  <subcommand>` as a subprocess: `whoami`, then `upload`, then `share`. No Open
  Platform API in Rust.
- gig never reads or stores Baidu credentials. bdpan keeps its own login; when
  it is logged out, gig fails with `secrets` and tells JC to run `! bdpan login`.
- The result is a **Pan Share**: `url` is `<link>?pwd=<code>`, and `pwd` is also
  returned on its own. The share period is the smallest of 1, 7, 30 days that
  covers `link_ttl_seconds`, never permanent.
- No **Short Link** over a Pan Share. The link already carries the code, Baidu
  controls its expiry, and a redirect would only add a second thing that can
  expire. `delivery.short_link` applies to `s3:*` uploaders only.
- S3 stays as a fallback. `delivery.uploader` picks the default, and every
  upload can pick another with `--uploader`. The channel follows the uploader:
  `bdpan` records `pan`, `s3:*` records `oss`.

## Consequences

- gig depends on the shape of bdpan's JSON replies. A bdpan update can break
  parsing; the fake bdpan in the tests and the opt-in live test
  (`GIG_LIVE_BDPAN=1`) are where that shows up. The exit code alone is not
  trusted: `code`, `errno` and `error` are checked too.
- The bdpan login token expires about every 30 days. `gig doctor` reports a
  logged-out or expired login as a problem and warns 7 days ahead.
- `share` is a paid API on Baidu's side. It works on JC's account today; if it
  stops, uploads succeed but the command fails with `upload` and names the
  remote path, and S3 remains usable with `--uploader s3:<name>`.
- Remote files accumulate under `<remote_root>/` (default `gig`, inside
  `/apps/bdpan/`). Every upload gets a new timestamped directory and gig never
  deletes remote files; cleanup is JC's, by hand.
- One Baidu account only. Several accounts would need `bdpan:<name>` names,
  which this shape leaves room for.
