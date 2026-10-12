# gig cheat sheet (v2)

The full spec is `docs/v2/SPEC.md` in the gig repository (`~/Documents/dev-tools/gig`, source under `crates/`). This lists what an agent needs day to day.

## Conventions

- Every command prints exactly one JSON document on stdout: `{"ok": true, "command": "...", "data": {...}, "warnings": [...]}` or `{"ok": false, "command": "...", "error": {"code": "...", "message": "..."}}` (no `warnings` key on an error). Exit 0 on success, 1 on a domain error, 2 on a usage error. Only `gig completion`, `gig --list-themes` and the dashboard (`gig tui`, or bare `gig` in a terminal; JC's, not for agents) print raw text. Bare `gig` without a terminal is still a usage error (exit 2).
- Naming the order: every command accepts `--order <slug>`; inside a project directory it can be omitted.
- Irreversible or outward commands need `--yes`; without it they only rehearse (`"dry_run": true`): `draft drop`, `package upload`, `package sent`, `package drop`, `artifact upload`, `archive`, `cancel`, `delete`.
- Money is a decimal in major units, at most two fractional digits (`800`, `800.50`). Dates are `YYYY-MM-DD`. `--cut-ratio` is JC's share of the price after the platform's cut (0.6 means 480 of 800).
- Error codes: `not_found`, `invalid_state`, `invalid_input`, `unsafe_package`, `needs_check`, `needs_yes`, `config`, `secrets`, `upload`, `legacy_db`, `io`, `db`.
- Uploads: `--uploader` overrides `delivery.uploader` for one upload. `bdpan` uploads to Baidu Netdisk and records channel `pan`; its `url` is the share link with the extraction code in it (`?pwd=`), `pwd` repeats the code, there is no `short_url`, and a `secrets` error means JC must run `! bdpan login` (an `upload` error from `whoami` carries bdpan's own message). A dry run already refuses with `config` an unknown uploader name or a bdpan binary that does not resolve (the fix is `delivery.bdpan.bin`). `s3:<name>` uploads to object storage, records channel `oss` and shortens the link when short links are on.
- States: `queued -> in_progress -> delivered -> paid -> archived`, plus `cancelled`. A preview changes no state. Packages may still be sent during the warranty (`paid`).

## Commands

```
gig draft new <slug> [--title T] [--material PATH] [--type tool|cv_ml|data_processing|research_writing|custom]
gig draft ls [--all]
gig draft drop <slug> --reason TEXT [--yes]

gig new <slug> --title T [--price 800] [--currency CNY] [--cut-ratio 0.6] [--type ...] [--material PATH] [--platform P] [--external-id X]
        [--client-words TEXT|@FILE|-] [--from-draft] [--adopt [--status queued|in_progress|delivered|paid]] [--no-scaffold]
gig ls [--all]
gig show [--order <slug>]
gig start [--order <slug>]
gig change [--order <slug>] --desc TEXT [--price-delta 200]
gig price [--order <slug>] --amount 1000 --reason TEXT
gig note [--order <slug>] TEXT
gig paid [--order <slug>] [--date YYYY-MM-DD] [--amount 800]
gig scorecard [--order <slug>] --decisions N --repeat-questions N --cleanups N --report-reworks N --score 1..5 [--note TEXT]
gig archive [--order <slug>] [--yes] [--before-warranty-end] [--no-scorecard] [--purge]
gig cancel [--order <slug>] --reason TEXT [--yes]
gig cd [--order <slug>]

gig package build <package-id> [--order <slug>] [--kind full|preview] [--write-manifest [--client-named results/]...]
gig package check <package-id> [--order <slug>]
gig package upload <package-id> [--order <slug>] [--uploader bdpan|s3:<name>] [--yes]
gig package sent <package-id> [--order <slug>] --channel phone|other [--note TEXT] [--yes]
gig package drop <package-id> [--order <slug>] [--yes]  removes an unsent package: row, delivery/<id>/, manifest, zip; a sent one is invalid_state
gig package ls [--order <slug>]              data is an array of packages (gig show has them under data.packages)

gig artifact upload <FILE> [--order <slug>] [--uploader bdpan|s3:<name>] [--yes]
gig artifact ls [--order <slug>]

gig delete <slug> --yes                      removes the row only, never files; for registration mistakes

gig doctor [--fix]                           warnings name legacy orders whose directory has no .gig/JOB.md
gig config get KEY
gig config set KEY VALUE                     edits config.toml in place; Settings keys are range-checked (tui.refresh_seconds 0..60, general.warranty_days 0..365, general.default_cut_ratio 0..1, general.default_currency a 3-letter code) and a refusal is invalid_input
gig config path
gig config split-secrets [--yes]
gig migrate --from OLD.db [--to NEW.db] [--dry-run] [--fix-path OLD=NEW]
gig backup
gig completion zsh                           raw text
gig tui [--theme NAME] [--light] [--no-icons] [--refresh N] [--mouse|--no-mouse]  JC's dashboard (bare gig in a terminal); agents never run it
gig version
```

## Package layout

```
<project>/delivery/<package-id>/              client files only
<project>/delivery/<package-id>.manifest.toml  the allowlist, outside the zip
<project>/delivery/<package-id>.zip            entries equal the manifest exactly
```

`package-id` defaults to `<slug>-vX.Y.Z`; previews use `<slug>-vX.Y.Z-preview`. File name rule: ASCII letters, digits, `. _ - /`; no hidden files, `..`, symlinks, key or credential files, or the components `.gig .git .scratch internal prompts node_modules __pycache__ venv .venv`; rename a deliverable directory that hits one (`prompts/` becomes `agent-notes/`). Components may start with `_` (`__init__.py`). Zips inside the package (`source.zip`) are opened and their entries meet the same hard rules (errors name `source.zip!/<path>`). Files named by the client (batch outputs) may be exempted from the naming convention with `client_named = ["results/"]` in the manifest; `check` reports every exemption as a warning.

`build --write-manifest` derives the manifest from the directory and writes the zip; pass `--client-named results/` at the same time for client-named subdirectories, and it goes into the manifest. After any change to the package contents run `build` or `check` again, or `upload` / `sent` fail with `needs_check`.
