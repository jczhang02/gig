# gig tui spec

Status: agreed design, 2026-09-29 (grilled with JC). Implementation follows this file.

## 1. What it is

A terminal dashboard for JC, the one human interface to gig. Agents keep using the JSON CLI; the TUI is for what JC does by hand: see who owes money and what comes next, record payments and notes, score orders, register orders and drafts, send packages the agent has built. It never touches project files beyond what `gig new` already does.

- Crate `gig-tui` (Rust, ratatui + crossterm), calling `gig-core` directly. Same state machine, same rules as the CLI.
- Entry: `gig` with no subcommand launches the dashboard; `gig tui` stays as an alias. Bare `gig` starts the dashboard only when stdin and stdout are terminals; otherwise (agents, pipes) it behaves exactly as before: clap's missing-subcommand error, exit 2. It is the second non-JSON command after `gig completion`. Flags (on bare `gig` and on `gig tui`): `--theme <name>`, `--light` (alias of `--theme gig-light`; giving both is a usage error), `--no-icons`, `--refresh <seconds>` (default 2, 0 disables), `--list-themes` (prints the theme names, one per line, and exits).
- Runs anywhere; inside a project directory it preselects that order.

## 2. Views

Keys in the whole app: `?` help, `q` quit, `r` refresh, `1..4` jump to a view, `Tab` next view, `/` filter the current list, `T` next theme (not persisted), `Esc` close a popup or clear the filter.

### 2.1 Orders (default)

Left: the active orders, one line each: icon for project type, slug, status chip, next action (at 160 columns and wider; below that it repeats the status and is shown for the selected row only), days in status, price, title, grouped under headings with counts and totals. The selected row expands to a second line with the latest JOB.md status entry, or, without one, the next action and the last sent link (TUI-DESIGN.md section 8). `a` toggles archived and cancelled orders in. Sorting: unpaid first, then in warranty, then in progress, then queued; within a group by days descending.

Right (when at least 110 columns; exact column budgets in TUI-DESIGN.md section 6): the selected order's detail. Below that width the list is full width and `Enter` opens the detail full screen.

Detail sections: header (title, slug, type, platform, price and cut, warranty end), Next action, Packages (id, kind, status, channel, sent date, short link), Latest status (last 3 entries of JOB.md "Status"), Client questions (unanswered ones from JOB.md), Requirement changes, Notes (last 5), Scorecard.

Actions from Orders or Detail (each shows an error verbatim and refreshes when gig-core refuses, e.g. `invalid_state`):

| Key | Action | gig-core call | Confirmation |
|---|---|---|---|
| `s` | start | `orders::start` | none |
| `p` | paid | form: date (default today), amount (default recorded price) -> `orders::paid` | none |
| `$` | price | form: amount, reason -> `orders::price` | none |
| `c` | change | form: description ($EDITOR), price delta -> `orders::change` | none |
| `n` | note | $EDITOR -> `orders::note` | none |
| `k` | scorecard | form: 4 counts + score 1..5 + note -> `orders::scorecard` | none |
| `x` | cancel | form: reason -> `orders::cancel(yes=true)` | yes, typed `y` in a popup |
| `A` | archive preview | `archive::archive(yes=false)`: shows blockers, dirty files, large files, unsent packages. No execution; the popup says to have the agent archive. | n/a |
| `u` | upload package | pick one checked package -> confirm popup (id, kind, size, resulting state) -> `packages::upload(yes=true)` with a progress bar; on success the short link is shown and copied to the clipboard | yes |
| `m` | mark sent | pick a checked package, channel phone/other, note -> `packages::sent(yes=true)` | yes |
| `U` | upload artifact | path field ($EDITOR-less, single line) -> `artifacts::upload(yes=true)` | yes |
| `e` | edit JOB.md | `$EDITOR <dev_path>/.gig/JOB.md`, TUI suspended | n/a |
| `y` | copy link | latest short link (or url) of the selected order to the clipboard | n/a |
| `N` | new order | form: slug, title, price, type, material path, platform, client words ($EDITOR), from draft (toggle) -> `orders::new` with scaffold | none; the result popup lists created files and says the next step is grill |

Uploads are synchronous: the UI blocks with a progress bar (S3 multipart progress from gig-core; single PUT shows an indeterminate spinner). One upload at a time.

### 2.2 Drafts

List of open drafts (slug, title, material path, age). `Enter` shows the tail of NOTES.md. `N` new draft (slug, title, material path, type). `P` promote: opens the New order form with `from_draft` set and the slug fixed. No drop from the TUI (deletion stays on the CLI).

### 2.3 Money

- Header numbers: outstanding (delivered, unpaid) total; received this month; received this year; and for each the take-home (price x cut_ratio).
- Bar chart: received per month, last 12 months, in major units.
- Outstanding list sorted by days since delivery, with slug, price, days.

### 2.4 History

All orders including archived and cancelled, newest first, with scorecard score and warranty end; `Enter` opens the detail. This is the same list as Orders with `a` on, kept as its own view so `1` always means "what needs me now".

## 3. Look

The complete design system (colour roles, typography, spacing, glyphs, width classes, every view's anatomy, popups, chart, themes with hex values and contrast, mockups, and the file-by-file change list) is [TUI-DESIGN.md](TUI-DESIGN.md). It is binding for appearance. The summary below is kept for orientation; where it and TUI-DESIGN.md differ, TUI-DESIGN.md wins.

- Reference: yazi's layout discipline (whitespace and colour instead of borders), btop's rounded boxes for popups. The bar chart is not boxed (TUI-DESIGN.md section 11.2).
- Colours: truecolor themes, `gig-dark` by default; eight built-ins and user theme files (section 5). Status colours are fixed roles in every theme: unpaid red, warranty amber, in progress default foreground, queued blue, archived and cancelled grey. Selection is a subtle background band plus an accent marker, not reverse video.
- Icons: Nerd Font glyphs for project type and status, with text next to them; `--no-icons` replaces glyphs with nothing and keeps the text.
- Density: one line per order, the selected one expands to two; details in the right pane.
- Motion: only the upload progress bar and a spinner; no transitions.
- Width: adaptive; under 110 columns the detail pane is hidden and `Enter` opens it full screen (tmux popup friendly). Below 60x16 the dashboard shows only a size notice.
- Text: titles may be Chinese; layout uses display width (unicode-width) so columns stay aligned; titles take the remaining width and are truncated with an ellipsis.

## 4. Data and concurrency

- Reads go through gig-core repos; the view refreshes every `--refresh` seconds and on every keypress that returns from an action.
- Writes call the same service functions the CLI uses, so all state checks apply. A refusal (`invalid_state`, `needs_check`, `secrets`, ...) is shown in a popup verbatim and the list refreshes; nothing is retried automatically.
- The TUI never runs `git` itself, never deletes files, never moves directories. `gig new` from the TUI scaffolds exactly as the CLI does. One exception, decided 2026-09-29 while implementing: the `A` archive preview calls `archive::archive(yes=false)`, which runs a read-only `git status` in the project to list dirty files (2.1 asks for them).
- Clipboard: `arboard`; when unavailable the link is shown in the popup only.

## 5. Config

`[tui]` section in config.toml, all optional: `theme = "gig-dark"`, `light = false` (alias: `theme = "gig-light"` when `theme` is unset), `icons = true`, `refresh_seconds = 2`. Flags override config; `GIG_TUI_*` env overrides both (`GIG_TUI_THEME`, `GIG_TUI_LIGHT`, `GIG_TUI_ICONS`, `GIG_TUI_REFRESH_SECONDS`).

Themes: built-in `gig-dark`, `gig-light`, `catppuccin-mocha`, `catppuccin-latte`, `tokyonight`, `gruvbox-dark`, `nord`, `dracula`. User themes are TOML files in the themes directory, `$XDG_CONFIG_HOME/gig/themes/<name>.toml` (`$GIG_HOME/config/themes/` under `GIG_HOME`), with the same keys as the built-ins, selectable by name; a user file shadows a built-in of the same name. An unknown or invalid theme falls back to `gig-dark` with a warning and never blocks startup. `T` cycles themes for the session. Format and rules: TUI-DESIGN.md sections 14 and 15.

## 6. Tests

- Unit tests for the view models (sorting, grouping, money aggregation by month, width-aware truncation of Chinese titles).
- A headless render test with ratatui's `TestBackend` for the Orders view at 80x24 and 200x50, checking that key elements are present and nothing overflows.
- Action tests through gig-core in a temp GIG_HOME: paid form -> state paid; cancel without confirmation does nothing.

## 7. Out of scope

Package building (files and manifest are the agent's job), archive execution, deleting anything, browser opening, launching agents, theme detection from the terminal background (OSC 11), hot reload of theme files.
