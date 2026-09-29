# gig v2 spec

Status: working design, 2026-09-29. Source of requirements: `~/dev/partjobs-workflow/WORKFLOW.md` (layer 1). This file is layer 2: what gig must store and do so that the `partjob` skill can drive a job from draft to archive. Layer 3 (the skill and project templates) reads this file and nothing else about gig.

## 1. Decisions

- gig is an agent-facing store. JC never runs it by hand. Every command prints one JSON document on stdout and nothing else. There is no table output, no colour, no GUI. Exit code 0 on success, 1 on a domain error, 2 on usage error.
- Language stays Rust. The two security-critical modules are ported, not rewritten: package validation (`services/client_package.rs`: path rules, symlink refusal, zip-entry equality) and the uploader (`delivery/mod.rs`, `delivery/s3.rs`: S3 presign, multipart, short link). Everything else is new code.
- Same repository, new branch `v2`. Crates: `gig-core`, `gig-cli`. `gig-gui` is deleted. Old `docs/*.html`, `docs/superpowers`, templates embedded in the binary and the `import`, `lead`, `quote`, `plan`, `work`, `acceptance`, `serve`, `gui`, `stats`, `export`, `client`, `source`, `tag`, `cut`, `status` commands are removed.
- New database file. v2 opens `$XDG_DATA_HOME/gig/gig-v2.db`; it never opens or modifies `gig.db`. `gig migrate` reads the old file and writes the new one. Nothing in v2 touches the old file except read-only during migrate.
- Secrets never live in `config.toml`. Loading a config that contains `access_key`, `secret_key` or `token` is a hard error naming the field. Secrets come from environment variables, then from `$XDG_CONFIG_HOME/gig/secrets.toml` (mode 0600 enforced on Unix).
- Templates are files under `general.templates_dir` (default `~/.agents/skills/partjob/templates`). They are never compiled into the binary, so `/partjob rule` can edit them.
- Money is stored as integer minor units and printed both ways. Timestamps are RFC 3339 UTC text. Dates (delivery, payment) are `YYYY-MM-DD`.
- Irreversible or outward commands need `--yes`: `draft drop`, `package upload`, `package sent`, `artifact upload`, `archive`, `cancel`, `delete`. Without it the command prints what it would do (`"dry_run": true`) and exits 0. The conversational approval lives in the skill; `--yes` only stops an agent from firing one by accident.
- Cross-platform: no Unix-only calls outside the `#[cfg(unix)]` permission helpers. Path handling uses `std::path`. Every config value has an env override `GIG_<SECTION>_<KEY>` and most have a CLI flag.

## 2. Files on disk

```
$XDG_DATA_HOME/gig/gig-v2.db          database
$XDG_CONFIG_HOME/gig/config.toml      config, no secrets
$XDG_CONFIG_HOME/gig/secrets.toml     optional, 0600
$XDG_STATE_HOME/gig/backups/          gig backup output
<dev_root>/.drafts/<slug>/NOTES.md    pre-order notes (drafts_dir)
<dev_root>/<slug>/                    project (dev_path)
<dev_root>/<slug>/.gig/JOB.md
<dev_root>/<slug>/.gig/QUOTE.md
<dev_root>/<slug>/delivery/<package-id>/            client files only
<dev_root>/<slug>/delivery/<package-id>.manifest.toml
<dev_root>/<slug>/delivery/<package-id>.zip
<archive_root>/<slug>/                after archive
```

### config.toml

```toml
[general]
dev_root = "/home/jc/dev/partjobs"
archive_root = "/home/jc/Documents/archive/work"
drafts_dir = "/home/jc/dev/partjobs/.drafts"        # default: <dev_root>/.drafts
templates_dir = "/home/jc/.agents/skills/partjob/templates"
default_cut_ratio = 0.6
default_currency = "CNY"
warranty_days = 15

[delivery]
uploader = "s3:aliyun-bj"        # "" disables upload; package sent --channel phone still works
link_ttl_seconds = 604800

[delivery.s3.aliyun-bj]
bucket = "..."
region = "..."
endpoint = "https://..."
download_endpoint = ""           # optional
path_style = false
allow_insecure_http = false

[delivery.short_link]
enabled = true
endpoint = "https://go.example/api/links"
```

Region and endpoint are JC's configuration; the tool ships no region default.

### secrets

Environment first, file second:

| secret | env | secrets.toml |
|---|---|---|
| S3 access key for uploader `<name>` | `GIG_S3_<NAME>_ACCESS_KEY` | `[s3.<name>] access_key` |
| S3 secret key | `GIG_S3_<NAME>_SECRET_KEY` | `[s3.<name>] secret_key` |
| short link token | `GIG_SHORT_LINK_TOKEN` | `[short_link] token` |

`<NAME>` is the uploader name upper-cased with `-` replaced by `_`.

Internally `Config` never holds secrets. The uploader is built from a `ResolvedS3 { config: S3UploaderConfig, access_key, secret_key }` assembled at the call site by `secrets::resolve(&config, name)`; the ported `S3Uploader::new` takes that resolved struct.

### manifest

```toml
version = 1
package_id = "tk-dtf-compact-v1.1.0"
kind = "full"                      # or "preview"
files = ["manual.pdf", "program/tk-dtf-compact.exe", "source.zip", "results/1-19093371527.png"]
client_named = ["results/"]        # optional: prefixes exempt from the ASCII naming convention
```

`files` are paths relative to `delivery/<package-id>/`, forward slashes, unique, non-empty.

## 3. Schema (migrations/V001__v2.sql)

```sql
CREATE TABLE drafts (
  id INTEGER PRIMARY KEY,
  slug TEXT NOT NULL UNIQUE,
  title TEXT,
  material_path TEXT,
  project_type TEXT,
  notes_dir TEXT NOT NULL,
  status TEXT NOT NULL,            -- open | promoted | dropped
  drop_reason TEXT,
  notes_snapshot TEXT,             -- NOTES.md content captured at drop/promote
  promoted_order_id INTEGER REFERENCES orders(id),
  created_at TEXT NOT NULL,
  closed_at TEXT
);

CREATE TABLE orders (
  id INTEGER PRIMARY KEY,
  slug TEXT NOT NULL UNIQUE,
  title TEXT NOT NULL,
  material_path TEXT,
  platform TEXT,
  external_id TEXT,
  project_type TEXT NOT NULL,      -- tool | cv_ml | data_processing | research_writing | custom
  status TEXT NOT NULL,            -- queued | in_progress | delivered | paid | archived | cancelled
  currency TEXT NOT NULL,
  price_minor INTEGER,             -- NULL only for legacy rows without a price
  cut_ratio REAL NOT NULL,
  dev_path TEXT,
  archive_path TEXT,
  client_words TEXT,               -- verbatim acceptance message from JC/client
  notes TEXT NOT NULL DEFAULT '',  -- append-only, "[rfc3339] text\n"
  created_at TEXT NOT NULL,
  started_at TEXT,
  delivered_at TEXT,
  paid_at TEXT,
  warranty_until TEXT,             -- date
  archived_at TEXT,
  cancelled_at TEXT,
  cancel_reason TEXT,
  legacy_id INTEGER                -- v1 orders.id, NULL for v2-native
);

CREATE TABLE price_history (
  id INTEGER PRIMARY KEY,
  order_id INTEGER NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
  old_minor INTEGER, new_minor INTEGER, reason TEXT, created_at TEXT NOT NULL
);

CREATE TABLE requirement_changes (
  id INTEGER PRIMARY KEY,
  order_id INTEGER NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
  description TEXT NOT NULL, price_delta_minor INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL
);

CREATE TABLE packages (
  id INTEGER PRIMARY KEY,
  order_id INTEGER NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
  package_id TEXT NOT NULL,
  kind TEXT NOT NULL,              -- full | preview
  dir TEXT NOT NULL, manifest_path TEXT NOT NULL, zip_path TEXT NOT NULL,
  zip_sha256 TEXT, file_count INTEGER,
  status TEXT NOT NULL,            -- checked | sent | legacy
  checked_at TEXT, sent_at TEXT,
  channel TEXT,                    -- oss | phone | other
  uploader TEXT, remote_url TEXT, short_url TEXT, expires_at TEXT,
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
  UNIQUE(order_id, package_id)
);

CREATE TABLE artifacts (
  id INTEGER PRIMARY KEY,
  order_id INTEGER NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
  local_path TEXT, uploader TEXT, remote_url TEXT, short_url TEXT, expires_at TEXT,
  uploaded_at TEXT NOT NULL
);

CREATE TABLE events (
  id INTEGER PRIMARY KEY,
  order_id INTEGER NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,              -- check_rejected | ...
  detail TEXT,
  at TEXT NOT NULL
);

CREATE TABLE scorecards (
  order_id INTEGER PRIMARY KEY REFERENCES orders(id) ON DELETE CASCADE,
  decisions INTEGER, repeat_questions INTEGER, days_to_preview INTEGER,
  cleanups INTEGER, check_rejections INTEGER, report_reworks INTEGER,
  score INTEGER,                   -- 1..5
  note TEXT, created_at TEXT NOT NULL
);
```

## 4. State machine

```
queued -> in_progress -> delivered -> paid -> archived
  |           |  ^          |
  v           v  +----------+   (revision / warranty work: gig start again)
cancelled   cancelled
```

- `start`: queued -> in_progress, or delivered -> in_progress (records a note "revision").
- `package upload` / `package sent` with kind=full: in_progress -> delivered. kind=preview: no change.
- `paid`: delivered -> paid; sets `warranty_until = paid_date + warranty_days`.
- Warranty work: while status is `paid`, `package upload` / `package sent` are allowed and change nothing but the package row (a note "warranty revision: <package-id>" is appended). No state is added for it.
- `archive`: paid -> archived; refuses before `warranty_until` unless `--before-warranty-end`; refuses without a scorecard unless `--no-scorecard`.
- `cancel`: queued | in_progress -> cancelled. `archive` is also allowed from cancelled (moves or purges the directory).
- Anything else is `invalid_state`.

`next_action` is computed, never stored: queued -> `start`; in_progress -> `preview or pack`; delivered -> `collect payment`; paid before warranty end -> `warranty until <date>`; paid after -> `archive`; archived/cancelled -> none.

## 5. Commands

Output envelope for every command:

```json
{"ok": true, "command": "show", "data": {...}, "warnings": []}
{"ok": false, "command": "show", "error": {"code": "not_found", "message": "..."}}
```

Error codes: `not_found`, `invalid_state`, `invalid_input`, `unsafe_package`, `needs_check`, `needs_yes`, `config`, `secrets`, `io`, `upload`, `legacy_db`.

`<slug>` may be omitted where noted; then the order is resolved from the working directory (longest `dev_path` / `archive_path` prefix, component-wise).

### Drafts (before an order)

- `gig draft new <slug> [--title T] [--material PATH] [--type TYPE]`
  Creates `drafts_dir/<slug>/NOTES.md` from `templates/NOTES.md.j2`, inserts draft row. Data: draft + `notes_path`.
- `gig draft ls` Data: open drafts.
- `gig draft drop <slug> --reason TEXT [--yes]`
  Snapshots NOTES.md into `notes_snapshot`, removes the draft directory, status dropped. Without `--yes`: lists what would be deleted.

### Orders

- `gig new <slug> --title T --price AMOUNT [--currency CNY] [--cut-ratio 0.6] [--type TYPE] [--material PATH] [--platform P] [--external-id X] [--client-words TEXT|@FILE] [--from-draft] [--no-scaffold]`
  Inserts order (queued). If `<dev_root>/<slug>/` already exists the command refuses unless `--adopt` is given: then it registers the order with `dev_path` pointing at the existing directory, touches no file, and warns for each of `.gig/JOB.md`, `.gig/QUOTE.md` that is missing. `--adopt --status S` sets the initial status for a job already under way (used at handover for tk-dtf-compact and bllc-reproduction). Otherwise, unless `--no-scaffold`: creates `<dev_root>/<slug>/` from templates (`JOB.md.j2`, `QUOTE.md.j2`, `AGENTS.md.j2` or `AGENTS.<type>.md.j2`, `README.md.j2`, `gitignore`), runs `git init` if git is available, writes nothing else. With `--from-draft`: the draft's NOTES.md is rendered into JOB.md's intake section, the draft becomes promoted, its directory is removed. `AMOUNT` is a decimal in major units with at most 2 fractional digits ("800", "800.5", "800.00"); anything else is `invalid_input`. Data: order + `created_files`.
- `gig ls [--all]` Data: orders (active only unless `--all`) with `next_action`, `days_in_status`, `unpaid` flag, `warranty_until`.
- `gig show [<slug>]` Data: order, packages, artifacts, requirement_changes, price_history, scorecard, `paths` (dev_path, job_md, quote_md, delivery_dir), `next_action`.
- `gig start [<slug>]` See state machine.
- `gig change [<slug>] --desc TEXT [--price-delta AMOUNT]` Appends requirement change; when delta != 0 also updates price and price_history.
- `gig price [<slug>] --amount AMOUNT --reason TEXT`
- `gig note [<slug>] TEXT` Appends a timestamped line to `notes`.
- `gig paid [<slug>] [--date YYYY-MM-DD] [--amount AMOUNT]` Defaults to today. `--amount` records a final price different from the current one (price_history).
- `gig scorecard [<slug>] --decisions N --repeat-questions N --cleanups N --report-reworks N --score 1..5 [--note TEXT]` Upserts. `days_to_preview` (started_at to the first preview `sent_at`) and `check_rejections` (counted by `package check` failures for this order, stored in a small `events` table: order_id, kind, at) are computed, not supplied.
- `gig archive [<slug>] [--yes] [--before-warranty-end] [--no-scorecard] [--purge]`
  Preview (no `--yes`) reports: git dirty files, untracked files, files > 50 MB, packages never sent, missing scorecard. With `--yes`: moves `dev_path` to `<archive_root>/<slug>`, sets archive_path, status archived. `--purge` deletes the directory instead of moving it; requires `--yes`.
- `gig cancel [<slug>] --reason TEXT [--yes]`
- `gig cd [<slug>]` Data: `{ "path": ... }` (for shell aliases).
- `gig delete <slug> --yes` Removes the row (not files). For mistakes only.

### Packages

Layout is fixed: `<dev_path>/delivery/<package-id>/`, `<dev_path>/delivery/<package-id>.manifest.toml`, `<dev_path>/delivery/<package-id>.zip`.

- `gig package build [<slug>] <package-id> [--kind full|preview] [--write-manifest]`
  With `--write-manifest`: walks `delivery/<package-id>/`, refuses on the first unsafe entry, writes the manifest listing every regular file. Then writes the zip from the manifest (deterministic order, stored mtimes zeroed), then runs the same validation as `check`. Data: as `check`.
- `gig package check [<slug>] <package-id>`
  Validates and records a package row (status checked, zip sha256). Data: `{ package, files: [...], zip_sha256, warnings }`. Warnings include: `.gitignore` does not ignore `delivery/`.
- `gig package upload [<slug>] <package-id> [--yes]`
  Re-validates; refuses if not previously checked or if the zip sha256 changed since the check (`needs_check`). Uploads via the configured uploader to object key `<slug>/<package-id>/<sent_at compact UTC>/<package-id>.zip` so the presigned URL's basename is the package id; the port keeps the Content-Disposition attachment header. Shortens the link when enabled, records channel oss, status sent. kind=full moves the order to delivered. Data: `{ package, url, short_url, expires_at, size }`.
- `gig package sent [<slug>] <package-id> --channel phone|other [--note TEXT] [--yes]`
  For packages JC sends by hand (gsconnect to phone, then forwarded). Same re-validation and state change as upload, no remote URL.
- `gig package ls [<slug>]`

Validation rules (ported and extended):

1. `delivery/`, `delivery/<id>/` exist, are directories, are not symlinks; their canonical paths are inside `dev_path`.
2. Manifest: version 1, `package_id` equals the argument, kind valid, `files` non-empty, unique, each passes the path rule.
3. Path rule for every manifest entry and every zip entry. Hard part (never relaxed): non-empty; forward slashes only; no `\`; no `.`/`..`/empty component; no absolute path; no control characters; no component starting with `.`; no banned component (`internal`, `prompts`, `node_modules`, `__pycache__`, `venv`, `.venv`, `.gig`, `.git`, `.scratch`); no banned basename pattern (`*.pem`, `*.key`, `*.p12`, `*.pfx`, `id_rsa*`, `id_ed25519*`, `.env*`, `secrets*`, `credentials*`, `config.toml` under any dir named `gig`).
4. Every manifest file exists, is a regular file, not a symlink, canonical path inside `delivery/<id>/`.
5. Every regular file under `delivery/<id>/` is in the manifest (no unlisted file).
6. The zip is a regular file inside `delivery/`; its non-directory entries equal the manifest set exactly; no symlink entries; directory entries only where a manifest path implies them.
7. Package id: 1..64 chars, `[A-Za-z0-9][A-Za-z0-9._-]*`, not ending in `.zip`, no `..`.

8. Naming convention (relaxable): every component matches `[A-Za-z0-9][A-Za-z0-9._-]*` (ASCII, no spaces). Files whose names come from the client (batch outputs named after their inputs) may be exempted by listing their directory prefixes in the manifest field `client_named = ["results/"]`; exempted files still pass the hard rule, and `check` reports each exemption as a warning so JC sees it.

Any failure is `unsafe_package` with the offending path in the message. Nothing is recorded on failure.

### Artifacts

- `gig artifact upload [<slug>] <FILE> [--yes]` Single file outside a package. The file must pass the path rule on its basename and must be a regular file. Records an artifact row.
- `gig artifact ls [<slug>]`

### Maintenance

- `gig migrate --from OLD.db [--to NEW.db] [--dry-run] [--fix-path OLD=NEW]...`
  See section 6. Refuses to overwrite an existing `--to`.
- `gig config split-secrets [--yes]`
  Reads a v1 `config.toml` that still contains `access_key`, `secret_key`, `short_link.token`, writes them to `secrets.toml` (0600) and rewrites `config.toml` without them (v2 layout, `delivery.uploader` from `default_uploader`, `link_ttl_seconds` hoisted). Prints only the field names moved, never the values. Without `--yes` it reports what it would move. This is the step that runs at handover before the first v2 command.
- `gig doctor [--fix]` Checks: dev_path/archive_path exist; package zip files exist; `.gig/JOB.md` and `.gig/QUOTE.md` present for active orders; `delivery/` gitignored; config has no secrets; secrets available for the configured uploader; templates_dir has every required template. `--fix` only repairs paths that can be found by slug under dev_root/archive_root.
- `gig config get KEY | set KEY VALUE | path` `path` prints all resolved paths.
- `gig backup` Copies the database to backups dir with a timestamp.
- `gig completion SHELL`
- `gig version`

## 6. Migration from v1

`gig migrate --from ~/.local/share/gig/gig.db` (default `--to` is the v2 path).

| v1 | v2 |
|---|---|
| orders.id | orders.id (kept) and legacy_id |
| slug NULL | error; must be fixed with `--slug ID=SLUG` before migrating (none in JC's data) |
| title | title |
| source_org | platform |
| external_id | external_id |
| project_type NULL | `custom` |
| status accepted, plan_ready, plan_approved | queued |
| status in_progress, ready_to_deliver, revision | in_progress |
| status delivered / paid / archived / cancelled | same |
| status lead, negotiating | cancelled with reason "legacy lead" |
| quoted_price, final_price | price_minor = final if set else quoted; if both set and differ, a price_history row. A value of 0 (orders #24, #25) migrates as 0 with a dry-run warning `zero price`, not as NULL |
| my_cut_ratio, currency | cut_ratio, currency |
| dev_path, archive_path | same, after `--fix-path` rewrites |
| notes | notes |
| created_at, accepted_at, delivered_at, paid_at, archived_at | RFC 3339; input may be integer epoch seconds or a numeric string or RFC 3339 text |
| paid_at | warranty_until = paid date + warranty_days |
| price_history | price_history |
| requirement_changes | requirement_changes |
| delivery_packages | v1 wrote one row per check and one per send, so rows sharing `(order_id, package_path)` merge into one v2 row: `checked_at` from the latest validated row, `sent_at` from the latest sent row, status sent if any row was sent, else checked if any was validated, else legacy. package_id = zip basename without `.zip`, or `legacy-<delivery_date>-<n>` when package_path is NULL. kind full. Paths are kept as recorded; v1 stored them relative to `dev_path`, so `doctor` resolves a relative path against `dev_path` then `archive_path`. remote_url from a delivery_artifact with the same local_path when one exists |
| delivery_artifacts | artifacts |
| quote_drafts promoted | one note line on the order: "quote draft <slug>: <summary>; min/rec/max" |
| quote_drafts not promoted | drafts with status dropped, notes_snapshot = summary plus the xdg files' text when present; missing xdg files produce a warning and summary-only snapshot |
| tags, order_tags | one note line "tags: a, b" |
| order_workflow, clients, sources | dropped |

`--dry-run` prints the full mapped dataset and the list of warnings (unparseable timestamps, missing files, path rewrites applied) without writing. The acceptance step compares this output with `gig ls --all` from the v1 binary: same ids, slugs, statuses (after mapping), prices.

## 7. Tests

- `gig-core`: unit tests for the path rule (table-driven, includes symlink, hidden, `..`, backslash, non-ASCII, banned names), manifest parsing, zip equality, state transitions, money parsing, timestamp parsing, secrets loading, config-with-secrets refusal.
- `gig-core/tests/migrate.rs`: builds a v1 database from the v1 migration SQL (kept under `tests/fixtures/v1/*.sql`) with synthetic rows covering every mapping row above, runs migrate, asserts the v2 rows. No real data is committed.
- `gig-cli/tests`: run the binary end to end in a temp XDG root: draft -> new -> start -> package build/check -> package sent --channel phone -> paid -> scorecard -> archive; every `--yes` command without `--yes` is a dry run; JSON envelope shape for success and each error code; unsafe package fixtures are rejected.
- The uploader keeps its existing S3 integration test (skipped without credentials).

## 8. Process

- Work on branch `v2`; commit locally in verified units; never push (public repo, push needs JC's approval).
- The v1 binary on PATH comes from the mise pin in dotfiles (`~/.config/mise/conf.d/30-rust.toml`). Do not `cargo install` over it until acceptance passes; run v2 from `target/`. Installing v2 is a dotfiles change JC sees.
- Before v2 replaces v1, dump the v1 database with sqlite into a saved reconciliation file; the acceptance check compares `gig migrate --dry-run` with that dump, not with v1 `gig ls --json` (which printed nothing parseable in this session).
- Old `gig.db`, old `config.toml`, the old skill directory and dotfiles stay untouched until handover.

## 9. Out of scope for v2.0

Pricing suggestions, client records, GUI, HTML indexes, plan/acceptance gates, CSV export, statistics. `gig ls --all` JSON is the export.
