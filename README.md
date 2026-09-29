# gig

Agent-facing store for freelance orders. `gig` records drafts, orders, client
packages, deliveries, payments and archives in a local SQLite file, and prints
one JSON document per command. It is driven by the `partjob` skill
(`skill/partjob/`), not by hand.

- Spec: `docs/v2/SPEC.md`
- Workflow the skill implements: `skill/partjob/references/workflow.md`
- Command cheat sheet for agents: `skill/partjob/references/gig.md`

## Install

Builds with whatever Rust the system provides (tested on 1.98); no toolchain pin, no rustup needed.

```bash
git clone https://github.com/jczhang02/gig.git
cd gig
cargo install --locked --path crates/gig-cli
gig version
```

Skill:

```bash
ln -s "$PWD/skill/partjob" ~/.agents/skills/partjob
ln -s ~/.agents/skills/partjob ~/.claude/skills/partjob
```

Zsh completion (the only command that prints something other than JSON):

```bash
gig completion zsh > ~/.local/share/zsh/site-functions/_gig
```

## Configure

`~/.config/gig/config.toml` (no secrets in it, ever):

```toml
[general]
dev_root = "/home/me/dev/partjobs"
archive_root = "/home/me/Documents/archive/work"
templates_dir = "/home/me/.agents/skills/partjob/templates"
default_cut_ratio = 0.6
default_currency = "CNY"
warranty_days = 15

[delivery]
uploader = "s3:main"
link_ttl_seconds = 604800

[delivery.s3.main]
bucket = "..."
region = "..."
endpoint = "https://..."

[delivery.short_link]
enabled = true
endpoint = "https://go.example/api/links"
```

Secrets go in the environment (`GIG_S3_MAIN_ACCESS_KEY`, `GIG_S3_MAIN_SECRET_KEY`,
`GIG_SHORT_LINK_TOKEN`) or in `~/.config/gig/secrets.toml` with mode 0600:

```toml
[s3.main]
access_key = "..."
secret_key = "..."

[short_link]
token = "..."
```

Every config value can be overridden with `GIG_GENERAL_<KEY>` / `GIG_DELIVERY_<KEY>`;
`GIG_HOME` moves all state under one directory.

## Use

```bash
gig draft new pdf-tool --material /mnt/materials/000123
gig new pdf-tool --title "PDF tool" --price 800 --type tool --from-draft --client-words "accepted, 800"
gig start --order pdf-tool
# ... work; put client files under <project>/delivery/pdf-tool-v1.0.0/
gig package build pdf-tool-v1.0.0 --write-manifest
gig package upload pdf-tool-v1.0.0 --yes        # or: gig package sent ... --channel phone --yes
gig paid --order pdf-tool
gig scorecard --order pdf-tool --decisions 3 --repeat-questions 0 --cleanups 0 --report-reworks 0 --score 5
gig archive --order pdf-tool --yes
```

Commands that send, delete, move or cancel need `--yes`; without it they print
what they would do and change nothing.

## Dashboard

`gig` with no subcommand, run in a terminal, opens the dashboard, the one human interface (`gig tui` is the same; without a terminal bare `gig` still fails with the usual usage error, so agents see no change): orders with their next action, drafts, money (outstanding, this month, this year, a 12-month chart), history; record payments, notes, scorecards, changes; register orders and drafts; send packages the agent has built. Flags `--theme <name>` (`gig-dark`, `gig-light`, `catppuccin-mocha`, `catppuccin-latte`, `tokyonight`, `gruvbox-dark`, `nord`, `dracula`, or a file of your own in `~/.config/gig/themes/<name>.toml`), `--light`, `--no-icons`, `--refresh <seconds>`, `--list-themes`; `T` opens a theme picker that previews on the whole screen and keeps the choice in config.toml; `,` opens Settings (theme, icons, refresh, mouse, warranty days, currency, cut ratio), written to config.toml in place with comments kept; `[tui]` in config.toml holds the same. Design: `docs/v2/TUI-SPEC.md` and `docs/v2/TUI-DESIGN.md`.

## Package safety

A client package is `delivery/<id>/` (client files only), `delivery/<id>.manifest.toml`
(the allowlist, outside the zip) and `delivery/<id>.zip` whose entries equal the
manifest exactly. `gig package check` refuses hidden files, `..`, absolute paths,
symlinks, unlisted files, key or credential files, and internal directories
(`.gig`, `.git`, `.scratch`, `internal`, `prompts`). Uploads re-check and refuse
a zip that changed since its last check.

## Migrating from v1

```bash
gig config split-secrets --yes         # moves keys from config.toml into secrets.toml
gig migrate --from ~/.local/share/gig/gig.db --dry-run
gig migrate --from ~/.local/share/gig/gig.db [--fix-path OLD=NEW]
gig doctor
```

v2 writes `gig-v2.db` and never touches the v1 file.

## Develop

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
cd skill/partjob && python3 -m unittest discover -s tests
```

## License

MIT
