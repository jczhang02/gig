# gig cheat sheet (v2)

The full spec is `docs/v2/SPEC.md` in the gig repository. This lists what an agent needs day to day.

## Conventions

- Every command prints exactly one JSON document on stdout: `{"ok": true, "command": "...", "data": {...}, "warnings": [...]}` or `{"ok": false, "command": "...", "error": {"code": "...", "message": "..."}}`. Exit 0 on success, 1 on a domain error, 2 on a usage error. Only `gig completion` and `gig tui` (JC's dashboard, not for agents) print raw text.
- Naming the order: every command accepts `--order <slug>`; inside a project directory it can be omitted.
- Irreversible or outward commands need `--yes`; without it they only rehearse (`"dry_run": true`): `draft drop`, `package upload`, `package sent`, `artifact upload`, `archive`, `cancel`, `delete`.
- Money is a decimal in major units, at most two fractional digits (`800`, `800.50`). Dates are `YYYY-MM-DD`.
- Error codes: `not_found`, `invalid_state`, `invalid_input`, `unsafe_package`, `needs_check`, `needs_yes`, `config`, `secrets`, `upload`, `legacy_db`, `io`, `db`.
- States: `queued -> in_progress -> delivered -> paid -> archived`, plus `cancelled`. A preview changes no state. Packages may still be sent during the warranty (`paid`).

## Commands

```
gig draft new <slug> [--title T] [--material PATH] [--type tool|cv_ml|data_processing|research_writing|custom]
gig draft ls [--all]
gig draft drop <slug> --reason TEXT [--yes]

gig new <slug> --title T [--price 800] [--type ...] [--material PATH] [--platform P] [--external-id X]
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
gig package upload <package-id> [--order <slug>] [--yes]
gig package sent <package-id> [--order <slug>] --channel phone|other [--note TEXT] [--yes]
gig package ls [--order <slug>]

gig artifact upload <FILE> [--order <slug>] [--yes]
gig artifact ls [--order <slug>]

gig delete <slug> --yes                      removes the row only, never files; for registration mistakes

gig doctor [--fix]
gig config get KEY
gig config set KEY VALUE
gig config path
gig config split-secrets [--yes]
gig migrate --from OLD.db [--to NEW.db] [--dry-run] [--fix-path OLD=NEW]
gig backup
gig completion zsh                           raw text
gig tui [--light] [--no-icons] [--refresh N]  JC's dashboard; agents never run it
gig version
```

## Package layout

```
<project>/delivery/<package-id>/              client files only
<project>/delivery/<package-id>.manifest.toml  the allowlist, outside the zip
<project>/delivery/<package-id>.zip            entries equal the manifest exactly
```

`package-id` defaults to `<slug>-vX.Y.Z`; previews use `<slug>-vX.Y.Z-preview`. File name rule: ASCII letters, digits, `. _ - /`; no hidden files, `..`, symlinks, key or credential files, or `.gig/ .git/ .scratch/ internal/ prompts/`. Files named by the client (batch outputs) may be exempted from the naming convention with `client_named = ["results/"]` in the manifest; `check` reports every exemption as a warning.

`build --write-manifest` derives the manifest from the directory and writes the zip; pass `--client-named results/` at the same time for client-named subdirectories, and it goes into the manifest. After any change to the package contents run `build` or `check` again, or `upload` / `sent` fail with `needs_check`.
