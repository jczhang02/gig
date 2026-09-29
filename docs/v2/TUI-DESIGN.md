# gig dashboard design system

Status: agreed design, 2026-09-29. Implementation follows this file. It refines [TUI-SPEC.md](TUI-SPEC.md) section 3 (Look) and adds the theme system of section 5; where the two disagree on appearance, this file wins, and TUI-SPEC.md still governs behaviour.

Origin: three proposals (editorial, instrument, yazi-native) were judged by three reviewers. Editorial won all three verdicts; this document is editorial with the grafts the verdicts agreed on, conflicts resolved into single rules. Every rule below is meant to be applied without taste: if a case is not covered, pick the nearest rule and add the case here.

Contents: 1 principles, 2 colour roles, 3 typography, 4 spacing, 5 glyphs, 6 frame and width classes, 7 banner, message row and footer, 8 Orders list, 9 detail pane, 10 Drafts and History, 11 Money, 12 popups and forms, 13 status chips, 14 theme system, 15 theme file format, 16 palettes and contrast, 17 mockups, 18 tests, 19 change list, 20 deferred.

## 1. Principles

1. Hierarchy comes from weight (bold), ink (text, muted, dim) and position. Never from boxes, fills or extra hues.
2. One accent per theme. It marks where you are and what is current: the selection marker, the active tab underline, the focused form field, the filter text, the current-month bar, progress and spinner, the border of a "done" popup. Nothing else is accent-coloured.
3. Status hues (unpaid red, warranty amber, queued blue, archived grey; in progress is `text`) are semantic, not decoration. They appear only on status glyphs and words, on the owed number, and on overdue days.
4. Colour never carries meaning alone. Every status has its word, every alarm has a number.
5. No boxes in views. Rounded boxes are for popups only. The chart is not boxed (a deliberate change to TUI-SPEC.md section 3, recorded there).
6. Nothing char-wraps. Text either fits, truncates with `…` by display width, or wraps prose at word or CJK-glyph boundaries.
7. Columns are dropped before they are squeezed below their useful minimum.

## 2. Colour roles

Fifteen slots per theme (section 14 lists them with their hex values). Usage, exhaustive:

| slot | used for | never used for |
|---|---|---|
| `bg` | page background | |
| `surface` | popup fill | view areas |
| `sel` | selection band (both lines of the selected row, full list width); focused form field row | anything that is not "selected" or "focused" |
| `border` | popup frames, chart baseline under months with money, progress track | text of any kind |
| `text` | primary content, values, slugs, titles, key letters, in-progress status | |
| `muted` | labels, column headers, group headings, meta lines, dates, counts, currency codes, days, empty-section line, overflow indicators, help preconditions | the only copy of a value the user came for |
| `dim` | separators `·`, empty-cell dots `·`, zero-month baseline `┈`, form placeholders | anything that carries information on its own |
| `accent` | the list in principle 2 | text that is not a marker, focus or "current" |
| `key` | key letters in hints and help (equal to `text` in every built-in; a user theme may diverge) | |
| `unpaid` | delivered status glyph and word; owed value > 0 in the banner and tiles; overdue days; refusal and destructive popup borders; validation errors | decoration |
| `warranty` | paid status while `warranty_until > today`; client-question `?`; external-action popup borders (`u`, `m`, `U`); package status `uploaded` | |
| `queued` | queued status; package status `checked` | |
| `archived` | archived and cancelled rows, whole row | |
| `bar` | chart bars of past months | text |
| `bar_now` | chart bar of the current month (equal to `accent` in every built-in) | text |

The dim rule: `dim` is held to only 3:1, so it must never be the sole carrier of a fact. If removing a dim element would lose information, the element is `muted` instead.

`error` and `ok` from the current theme are folded: refusals and errors use `unpaid`, success uses `accent`.

Overdue: one constant `OVERDUE_DAYS = 14` in `theme.rs`. A delivered order at `days_in_status >= OVERDUE_DAYS` shows its days in `unpaid` bold (list, detail status line, outstanding table). Nothing else keys off a different threshold.

## 3. Typography

A terminal has bold, colour, italic, underline and reverse. Each has one job.

- **Bold**, exactly these: the app name `gig`; the active tab word; the selected row's slug; the detail title; section headings in the detail pane; stat-tile values; popup titles; key letters in hints, help and popup footers; the chart's current-month label and its value label. Alarms: the owed value when > 0 and overdue days. Never bold a whole row.
- **Italic**: form placeholders only (ASCII hint text such as `slug, lowercase-with-dashes`). Never on CJK text (many CJK fonts synthesise an ugly slant) and never on JOB.md content.
- **Underline**: short links, the active tab word (in `accent` via `underline_color`; terminals and tmux setups that ignore underline colour fall back to a text-colour underline, which is still correct), filter matches in slugs. Nothing else, so underline always means "link" or "where you are".
- **Reverse video**: never. The form cursor is the real terminal cursor (`Frame::set_cursor_position`), not a drawn cell.
- **Strikethrough**: never (unreliable across terminals; cancelled rows use the `archived` colour and the word `cancelled`).
- **SGR faint** (`\e[2m`): never. Faintness is the `muted` and `dim` slots.
- Case: sentence case. View names capitalised in the banner only (`Orders`). Group headings, labels and status words lower case (`owed`, `take-home`, `in progress`). Detail section headings are sentence case (`Requirement changes`). No colons after labels.
- Numbers: money grouped with `,` by thousands (`29,550`), right-aligned in columns, no decimals for CNY. The chart and nothing else uses compact form (`12.9k`, section 11.2). Status words use display labels, never enum spellings (`in progress`, not `in_progress`).

## 4. Spacing

All units are cells.

| rule | value |
|---|---|
| page margin left and right | 1 |
| column gap in tables | 2 |
| gutter between list and detail pane | 2, whitespace only, no rule |
| section gap (detail sections, list groups, Money blocks) | exactly 1 blank row, never 2 |
| detail section body indent | 2 |
| hanging indent after a date | date width + 2 (`MM-DD` gives 7 in the body, 9 from the pane edge) |
| popup padding | 1 row top and bottom, 2 cells left and right, 1 blank row before the popup footer |
| label to number | at least 2 cells, or a right-aligned column |

Rhythm: rows inside a group have no gap; groups and sections are one row apart. Nothing is centred except popups, the min-size message and the empty-chart line.

## 5. Glyphs

Nerd Font glyphs from the Font Awesome BMP range (stable in NF v2 and v3, and already proven to render in JC's tmux). Each is 1 cell. With `--no-icons` a glyph and its trailing space disappear, so columns close up (the Orders list gets 2 cells narrower, the title column 2 cells wider); text labels always stay.

| thing | glyph | codepoint | colour |
|---|---|---|---|
| tool | wrench | U+F0AD | `muted` |
| cv_ml | eye | U+F06E | `muted` |
| data_processing | database | U+F1C0 | `muted` |
| research_writing | book | U+F02D | `muted` |
| custom | cube | U+F1B2 | `muted` |
| delivered | exclamation-circle | U+F06A | status |
| paid | shield | U+F132 | status |
| in progress | cog | U+F013 | status |
| queued | clock-o | U+F017 | status |
| archived | archive | U+F187 | status |
| cancelled | ban | U+F05E | status |

No glyphs on tabs (the numbers identify them), no link glyph, no view glyphs (`Icons::view` is removed).

Structural characters, always drawn, 1 cell each (East-Asian-ambiguous; ratatui measures them as 1, which matches every common terminal in its default setting): `▎` marker, `·` separator and empty dot, `…` truncation, `─` baseline and popup edge, `┈` zero baseline, `╭╮╰╯│` popup frame, `▁▂▃▄▅▆▇█` bars, `▒` NO_COLOR bars, `▏▎▍▌▋▊▉█` progress fill, `‹ ›` select arrows, `↑ ↓` scroll, braille `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` spinner, ASCII `!` and `?`.

Banned (fonts draw them as emoji or double width, which breaks alignment next to CJK): `● ○ ◆ ◇ ★ ☆ ✓ ✕ ✗ ■ □ ◈ ▪`.

## 6. Frame and width classes

### 6.1 Frame

| row | content |
|---|---|
| 0 | banner (section 7.1) |
| 1 | message row: blank when idle (section 7.2) |
| 2 .. H-2 | view body |
| H-1 | footer: contextual key hints (section 7.3) |

Minimum size 60x16. Below it the whole screen is `bg` with one centred `muted` line `gig needs 60x16, this is 52x14` and nothing else; keys still work (`q` quits).

### 6.2 Width classes

One enum `WidthClass { Narrow, Medium, Wide }` in `ui.rs`, computed from the terminal width `W` and used by every view and by the render tests. The 110-column threshold of TUI-SPEC.md stays.

| class | W | Orders | Drafts | History | Money |
|---|---|---|---|---|---|
| Narrow | < 110 | list full width; `Enter` opens the detail full screen | list full width; `Enter` opens the NOTES.md tail popup | full width, no created and no warranty column | tiles as three one-line rows, chart below |
| Medium | 110 .. 159 | list + detail pane | list + NOTES.md tail pane | full width | three tiles side by side |
| Wide | >= 160 | list with `next` column + detail pane | list + NOTES.md tail pane | full width | three tiles side by side |

### 6.3 Orders column budget

Columns, left to right, in cells:

| column | cells | align | notes |
|---|---|---|---|
| marker | 1 | | `▎` in `accent` on the selected row, else blank |
| space | 1 | | |
| type glyph | 1 | | removed with `--no-icons` (with its space) |
| space | 1 | | |
| slug | 22 | left | truncated with `…` (fits every slug but the 24-cell ones) |
| gap | 2 | | |
| status | 13 | left | glyph, space, word (longest word `in progress` = 11) |
| gap | 2 | | Wide only: `next` 15 + gap 2 here |
| days | 4 | right | `34d` |
| gap | 2 | | |
| price | 6 | right | `12,850` fits; missing price is a `dim` `·` |
| gap | 2 | | |
| title | rest | left | shown only when it gets >= 10 cells |

Fixed part before the title: 55 cells (57 with the gap), 72 at Wide (with `next`). With `--no-icons`, subtract 2.

`next` is dropped at Narrow and Medium because the next action is a function of the status (`Order::next_action`), so it repeats the chip; the selected row's next action is still on its second line, in the pane and in the footer. At Wide the column shows the full text, except `warranty until YYYY-MM-DD`, which is shortened to `until MM-DD`.

Arithmetic (`L` = list width, `P` = pane width, margins 1 + 1, gutter 2, so `L + P = W - 4`):

- Narrow: `L = W - 2`, no pane. Title = `L - 57`: 21 cells at 80, 41 at 100. Title column dropped below `W = 69`; the second line of the selected row then starts with the full title (section 8.3).
- Medium: `P = clamp(W - 4 - 57 - 20, 40, 64)`, `L = W - 4 - P`. At 110: P 40, title 9, so the title column is dropped and the list is 55 cells, the pane 51. At 120: P 40, L 76, title 19. At 139: P 58, title 20. At 159: P 64, title 34.
- Wide: title = `clamp(W - 4 - 72 - 2 - 60, 24, 40)`, `L = 72 + 2 + title`, `P = W - 4 - L`. At 160: title 24, P 58 (the title minimum wins over the pane's 60; P >= 58 always holds from 160 up). At 200: title 40, P 82.
- Full-screen detail (Narrow `Enter`, or any class from History): left margin 2, prose measure `min(W - 4, 76)`.
- The pane never draws prose wider than 76 cells even at Wide; extra pane width stays as right margin.

## 7. Banner, message row, footer

### 7.1 Banner (row 0)

```
 gig   1 Orders   2 Drafts   3 Money   4 History               owed 1,600  ·  sep 0  ·  2026 29,550 CNY 
```

- `gig`: bold `text`. Then 3 cells, then the tabs, 3 cells apart (both gaps are 2 cells below W 100).
- A tab is the number in `muted` and the word in `muted`; the active tab's word is bold `text` with an `accent` underline. No background pill, no reverse, no glyph.
- Right cluster, right-aligned to the margin, items separated by `  ·  ` with a `dim` dot:
  - `owed 1,600`: label `muted`; value bold `unpaid` when > 0, plain `text` when 0 (nothing owed is not an alarm).
  - `sep 0`: the current month's received total, labelled by the lower-case 3-letter English month name. Value `text`; 0 is a real value and shows `0`.
  - `2026 29,550`: the current year's received total, labelled by the year.
  - `CNY` in `muted`, once, at the end (the default currency).
- Degradation when the tabs and the cluster do not fit with at least 3 cells between them: drop `CNY`, then the year item, then the month item; `owed` goes last. If `owed` alone still does not fit, tabs become `1 2 3 4` with only the active word shown.

### 7.2 Message row (row 1)

Row 1 is blank spacing when idle and doubles as the message row, so no row of chrome is added.

- Left: the filter while typing or active, `/ sers▏` in `accent` followed by `3 of 5` in `muted`.
- Right, right-aligned to the margin: one toast at a time.
  - `copied go.jczhang.cc/a30bd870` (`text`), `theme nord  ·  set [tui] theme to keep` (`text`, after `T`), warnings such as `unknown theme "nrod", using gig-dark` (`warranty`). Toasts clear after 3 s or on the next key.
  - Errors that are not shown in a refusal popup (for example the clipboard is unavailable, or `$EDITOR` failed) in `unpaid`. Errors stay until the next key.

### 7.3 Footer (row H-1)

Contextual hints in two groups joined by `   ·   ` (`dim` dot). Keys are bold `key`, labels `muted`, pairs 2 cells apart.

Group 1, the actions that fit the selected order's state (a hint, not a guard; gig-core still decides):

| selected order | group 1 |
|---|---|
| queued | `s start  $ price  n note  x cancel` |
| in progress | `u upload  m mark sent  c change  n note` |
| delivered | `p paid  y copy link  n note  k score` |
| paid, in warranty | `y copy link  n note  k score` |
| paid, warranty over | `A archive  y copy link  n note` |
| archived or cancelled (History) | `y copy link  e JOB.md` |
| no order selected | (empty) |

At Narrow, `Enter detail` is prepended to group 1. In Drafts group 1 is `Enter notes  N new draft  P promote`; in Money `Enter open order  y copy link`.

Group 2 per view:

| view | group 2 |
|---|---|
| Orders | `/ filter  a archived  N new  ? keys  q quit` |
| Drafts, History | `/ filter  ? keys  q quit` |
| Money | `1-4 views  T theme  ? keys  q quit` |

Degradation, whole pairs only, never truncating inside a pair, in this order until the footer fits the width minus the margins: (1) drop the optional pairs `N new`, `a archived`, `T theme`, `1-4 views`; (2) drop pairs from the right end of group 1 until 2 remain; (3) drop `/ filter`, then `q quit`; (4) drop the rest of group 1 from the right. `? keys` is never dropped. Worked example at 80 (Narrow, delivered selected): step 1 removes `N new` and `a archived`, step 2 removes `k score`, giving the footer of mockup 17.2.

## 8. Orders list

### 8.1 Header and groups

- Row 2: column header in `muted`, words only, aligned with the columns: `order`, `status`, (`next`), `days`, the currency code over the price column (`CNY`), `title`.
- Rows are grouped by the spec sort. Each non-empty group opens with a blank row, then a heading row: indent 2, the group word, 2 cells, the count; all `muted`. The group's money total is right-aligned in the price column, `muted` (`owed 2 .......... 1,600`).
- Group words: `owed` (delivered), `paid`, `in progress`, `queued`, and with `a` on, `archived`, `cancelled`. Headings are not selectable; arrow keys skip them.
- Sticky heading: when the list is scrolled so the first visible row is an order, that row position shows the heading of the order's group instead, and the list starts one row lower. Every visible order therefore has its group named on screen.
- Empty list: centred-left at the list origin, `nothing needs you` in `muted`, next row `N new order` (key bold).

### 8.2 Row

| part | style |
|---|---|
| marker | `▎` `accent` on both lines of the selected row; blank otherwise |
| type glyph | `muted` |
| slug | `text`; bold on the selected row only; filter matches underlined |
| status | glyph + display word, both in the status colour (section 13) |
| next (Wide) | `text` |
| days | `muted`, right-aligned, `d` suffix; bold `unpaid` when delivered and >= `OVERDUE_DAYS` |
| price | `text`, grouped, right-aligned; missing price is a `dim` `·` |
| title | `text`, truncated with `…` by display width |

The selected row (both lines) sits on the `sel` band across the full list width. Archived and cancelled rows are drawn entirely in `archived` (slug, glyphs, numbers, title), no strikethrough.

### 8.3 Second line of the selected row

Indented to the slug column (col 4, col 2 with `--no-icons`), truncated at the list width, on the `sel` band, first match wins:

1. The latest JOB.md status entry: `MM-DD` in `muted`, 2 cells, the entry in `muted` (not italic).
2. Else the full next action in `muted`, then, when a package was sent, `  ·  sent MM-DD` (`muted`) and 2 cells and the short link (underlined `text`).
3. Else nothing: the selected row stays one line.

When the title column is absent (Narrow below W 69, and Medium at W 110), the second line always exists and starts with the full title in `text`, then `  ·  ` and the item above if any.

The string `(no status entry in JOB.md)` and any other placeholder never appear in the list.

## 9. Detail pane

Drawn on `bg` at the right of the list (Medium and Wide) or full screen (Narrow `Enter`, and from History). Width `P`; prose measure `min(P, 76)`.

### 9.1 Header block

```
小鼠结肠炎 SERS 光谱分析与图表改版            title: bold text, wraps, max 2 lines (full screen: all)
sers-colitis-analysis · <eye> cv_ml · xianyu    meta: muted, dots dim
<!> delivered · 34 days in status               glyph+word status colour; rest muted
                                                (blank)
price          800 CNY                          mini statement: label muted in 11 cells,
cut             60%                             number text right-aligned in 7 cells (ends at col 18),
take-home      480 CNY                          unit muted ( CNY or %)
```

- Status line extras: delivered at >= `OVERDUE_DAYS` shows `34 days in status` in bold `unpaid`. Paid shows `· warranty until 2026-10-05`, in `warranty` while running and `muted` after. Cancelled shows the reason when recorded.
- Price missing: `price` row shows a `dim` `·` in the number column; `take-home` is omitted.

### 9.2 Sections

Order: Next, Packages, Latest status, Client questions, Requirement changes, Notes, Scorecard. Each non-empty section is: one blank row, the heading in bold `text`, 2 cells, the count in `muted` (omitted for Next and Scorecard), then the body indented 2.

- Next: the full next action in `text`; the key that performs it right-aligned on the same row (`p paid`, key bold, label `muted`). Mapping: delivered `p paid`, queued `s start`, in progress `u upload`, paid with warranty over `A archive`, otherwise no key.
- Packages, per package, newest first:
  - line 1: the package id in `text`, middle-truncated to the body width keeping its last 10 cells (the date): `sers-colitis-analysis-deliv…2026-08-26`;
  - line 2: `kind · status · channel · MM-DD` in `muted`, the status word coloured (`checked` in `queued`, `uploaded` in `warranty`, `sent` in `text`), missing fields skipped;
  - the short link, scheme stripped, underlined `text`: after 2 cells on line 2 when it fits, else on its own line 3. Never char-wrapped.
- Latest status (last 3), Requirement changes, Notes (last 5): dated entries. Date `MM-DD` in `muted` (`YYYY-MM-DD` when not the current year), 2 cells, the text in `text` wrapped by display width with a hanging indent under the text start. In the pane each entry is capped at 2 lines, the second ending in `…` when the entry was cut; full screen shows every line.
- Client questions: each prefixed `? ` with the `?` in `warranty`.
- Scorecard: `score 4/5` (score bold), then `  ·  2 decisions  ·  1 repeat question  ·  0 cleanups  ·  0 report reworks` in `muted`, wrapped at `  ·  ` boundaries.
- Empty sections get no heading. They are listed together, after the last non-empty section, one blank row above, in `muted`: `no status entries · no client questions · no scorecard (k records one)`, wrapped at ` · ` boundaries. Next is never empty (for archived orders it reads `none`).

### 9.3 Scrolling

`PgUp`/`PgDn` scroll the pane (or the full-screen detail). When content continues below, the last visible row shows `↓ 12 more  PgDn` in `muted`, right-aligned; when scrolled, the first visible row shows `↑ 3 above  PgUp`. Scroll positions snap so a section heading is never the last visible row.

## 10. Drafts and History

### 10.1 Drafts

Columns: marker 1, space 1, slug 22, gap 2, age 4 (right, `muted`, `3d`), gap 2, title (rest, min 16, `text`), gap 2, material (`muted`, left-truncated so the file name survives: `…/Downloads/sers-v2.zip`, at most 40 cells, dropped first when space runs out). No type glyph (drafts often have none).

`Enter`: at Medium and Wide the NOTES.md tail opens as a right pane with the Orders split (`P` as in 6.3), drawn like a detail pane with the heading `Notes` and the tail as prose; `Enter` again or `Esc` closes it. At Narrow it stays the popup it is today.

Empty: `no open drafts` in `muted`, next row `N new draft` (key bold).

### 10.2 History

Flat list, newest first, no groups. Columns: marker 1, space 1, type glyph 1, space 1, slug 22, gap 2, status 13, gap 2, created 10 (`YYYY-MM-DD`, `muted`), gap 2, score 3 (right, `4/5`), gap 2, warranty end 10 (`YYYY-MM-DD`; `warranty` while running, `muted` after), gap 2, price 6, gap 2, title (rest). At Narrow the created and warranty columns are dropped. Empty cells are a `dim` `·` (right-aligned in numeric columns), never `-` (reads as minus in a money table) and never `(none)`. Archived and cancelled rows in `archived`.

## 11. Money

### 11.1 Stat tiles

Three tiles: `outstanding`, `received in September` (the month's full English name), `received in 2026`.

- Medium and Wide: three equal columns across the body (`(W - 2) / 3`, the remainder to the last), 3 rows each: label (`muted`), value (bold; `unpaid` for outstanding when > 0, else `text`; unit ` CNY` in `muted`), context (`muted`): `take-home 960 · 2 orders`, `take-home 0`, `take-home 17,730 · 4 months`.
- Narrow: three one-line rows, `label  value  take-home N` with the labels padded to one column.

### 11.2 Chart: received per month

- Heading row: `Received per month` in bold `text`; right-aligned `Oct 2025 - Sep 2026  ·  CNY` in `muted`. One blank row under it.
- Form: single-series vertical bars, the last 12 months, oldest on the left. No box, no y-axis, no gridlines, no legend.
- Geometry: chart width `C = W - 2`; slot `s = floor(C / 12)`; bar width `b = min(5, s - 2)` at W >= 100, `min(3, s - 2)` at 80..99, `min(2, s - 1)` below; bar offset in the slot `floor((s - b) / 2)`. Unused cells (`C - 12 s`) stay on the right.
- Plot height `h` (rows of bar): 10 at H >= 34, 6 at 24..33, 4 below. One value-label row sits above the plot. The chart never fills leftover space.
- Heights in eighths: `e = round(value / max * h * 8)`; `floor(e / 8)` full rows of `█`, then one cap cell `▁..▇` for `e mod 8` when non-zero. A non-zero month gets at least `▁`.
- Colour: past months `bar`, the current month `bar_now`. Labels never take the bar colour.
- Value labels: on the row directly above the bar's top cell, centred on the bar, in `text` (current month bold). Compact form: below 1,000 the exact number; from 1,000 one decimal and `k` (`3.1k`, `10.0k`, `12.9k`); from 1,000,000 one decimal and `M`. Round half up. A zero month has no label. A label wider than `s - 1` is omitted.
- Baseline row under the plot, across `12 s` cells: `─` in `border`, except under the footprint of a zero month, which is `┈` in `dim` (`┈` in `bar_now` for the current month). "No income" is a drawn shape, not a gap.
- Month row: 3-letter English month centred under each bar, `muted`; the current month bold `text`. Year row: the year under the first month and under each January, `muted`.
- Empty chart (all twelve zero): the baseline is all `┈`, the plot rows are blank except one centred `muted` line `no payments in the last 12 months`.
- NO_COLOR (section 14.6): past bars are drawn with `▒` rounded to whole rows, the current month with `█` and eighth caps.

Vertical budget at H = 36: banner 1, message 1, tiles 3, blank 1, heading 1, blank 1, label 1, plot 10, baseline 1, months 1, years 1, blank 1, outstanding heading 1, table header 1, rows, footer 1 = 26 + rows. At H = 24 with h = 6 the same stack fits exactly two outstanding rows; more rows scroll with a `↓ n more` line.

### 11.3 Outstanding table

Heading `Outstanding  2` (bold word, `muted` count) with the total right-aligned in the price column (`muted`). Header row (`muted`): `order`, `CNY`, `since`, `title`. Rows: marker, type glyph, slug 22, gap 2, price 6, gap 2, days since delivery 5 (right; bold `unpaid` at >= `OVERDUE_DAYS`), gap 2, title (rest; dropped at Narrow). Sorted by days descending. Up/Down select, `Enter` opens the order, `y` copies its link.

## 12. Popups and forms

### 12.1 Frame and placement

- Rounded box `╭─ title ─╮`, frame in `border`, title bold `text` with 1 space on each side, fill `surface`. Padding per section 4.
- Width: help `min(72, W - 4)`; forms `min(64, W - 4)`; confirm, refusal and result `min(56, W - 4)`. Height fits the content up to `H - 2`, then the content scrolls inside.
- Placement: centred horizontally; top row `max(1, floor((H - height) / 3))`, so the popup sits on the eye line, not dead centre.
- Tone, frame and title colour only: forms and help `border`; external actions (`u` upload, `m` mark sent, `U` upload artifact) `warranty`; destructive (`x` cancel) and refusals `unpaid`; results ("done") `accent`.

### 12.2 Scrim and clearing

Before drawing any popup, in this order:

1. Scrim: one pass over `frame.buffer_mut()` for the whole screen: every cell's fg becomes `dim`, `BOLD` and `UNDERLINED` are removed, bg is kept (the selection band stays visible). The popup is then the only live content.
2. Clear the popup rect (`Clear`), then fill it with `surface`.
3. Wide-glyph straddle fix, for every row of the rect: if the cell just left of the rect holds a 2-cell symbol, set it to a space; if the cell just right of the rect is a continuation cell (empty symbol), set it to a space. This removes the stray half glyph seen where a CJK character was cut by the popup edge.

Under NO_COLOR the scrim only removes bold and underline.

### 12.3 Form anatomy

- Rows: the 2-cell left padding (its first cell is the marker column), the label right-aligned in 14 cells (`muted`), 2 cells, the value (`text`).
- Focused field: `▎` in `accent` in the marker column, the whole inner row on `sel`, the label turns `text`; the terminal cursor sits in the value.
- Empty unfocused field: a placeholder in italic `dim` (`slug, lowercase-with-dashes`).
- Select: `‹ cv_ml ›`, arrows `muted`. Toggle: `[x] yes` / `[ ] no`, box `muted`.
- `$EDITOR` fields: empty shows `Enter opens $EDITOR` (italic `dim`); filled shows the first line in `text` and `(+3 lines)` in `muted`.
- Validation: under the field, starting at the value column, `! slug already exists` in `unpaid`. Submit is refused while any field has an error; the message row says which.
- Footer inside the box after one blank row: `Tab next  Space choose  Enter submit  Esc cancel`, keys bold `key`, labels `muted`.

### 12.4 Other popups

- Confirm (`x`, `u`, `m`, `U`): the facts as a label/value block (id middle-truncated, kind, size, resulting state shown as its status chip), one blank row, the question in bold `text`, one blank row, `y yes   Esc no`.
- Refusal (gig-core error): title ` refused `, first line the code in bold (`invalid_state`), then the message verbatim in `text`, wrapped at the popup width.
- Result: title ` done `, created files in `muted`, the short link underlined `text`, `copied to clipboard` in `muted` (or `clipboard unavailable, link shown above`).
- Progress: one row, fill `█` plus a partial `▏..▉` in `accent` on a `─` track in `border`, then the percentage in `text` and `12.1 / 18.9 MB` in `muted`. Single PUT: the braille spinner in `accent` at 80 ms per frame plus `uploading` in `muted`. These are the only motion in the app.
- Help: two columns, `global` and the current view's keys (order keys whenever an order detail is open). Column headings bold. Keys right-aligned in a 4-cell column, bold `key`; label `text`; conditional keys add their precondition in `muted`: `when queued or delivered` (`s`), `when delivered` (`p`), `when not archived` (`c`), `when queued or in progress` (`x`), `when a package is checked` (`u`, `m`). Footer: `theme gig-dark   ·   T cycles, [tui] theme keeps it`.

## 13. Status chips

Chips are glyph + word in the status colour on the row's own background. No filled pills.

| state | word | colour slot | glyph |
|---|---|---|---|
| delivered | `delivered` | `unpaid` | U+F06A |
| paid, warranty running | `paid` | `warranty` | U+F132 |
| paid, warranty over or unset | `paid` | `muted` | U+F132 |
| in progress | `in progress` | `text` | U+F013 |
| queued | `queued` | `queued` | U+F017 |
| archived | `archived` | `archived` | U+F187 |
| cancelled | `cancelled` | `archived` | U+F05E |

`Theme::status_color` keeps the fixed mapping (in progress equals `text`, archived equals cancelled, unpaid differs from warranty); the "warranty over" case is decided by the caller from `warranty_until` and today.

## 14. Theme system

### 14.1 Slots and built-ins

`Theme { name: Cow<'static, str>, bg, surface, sel, border, text, muted, dim, accent, key, unpaid, warranty, queued, archived, bar, bar_now }`, all `Color::Rgb`. In-progress status is `text` by rule, not a slot.

Built-in themes, `Theme::BUILTIN: [Theme; 8]`, in this order: `gig-dark` (default), `gig-light`, `catppuccin-mocha`, `catppuccin-latte`, `tokyonight`, `gruvbox-dark`, `nord`, `dracula`. Values in section 16.

Hue logic: the accent is each theme's signature hue that is not red, amber or blue (gig-dark sea-glass teal, gig-light deep teal, catppuccin mauve, tokyonight magenta, gruvbox aqua, nord aurora purple, dracula purple), so the accent never impersonates a status. Queued is the theme's blue (gruvbox's blue-grey, dracula's cyan). Dark themes sink `surface` below `bg`; light themes raise it to near white, so the `sel` band stays visible inside popups.

### 14.2 Selecting a theme

- Config: `[tui] theme = "nord"` (string, optional).
- Flag: `--theme <name>` on bare `gig` (and on the `gig tui` alias).
- Env: `GIG_TUI_THEME=nord`.
- Precedence stays as today for every `[tui]` key: config file, then flags, then `GIG_TUI_*` (env wins).
- `light` is an alias inside each layer: `light = true` in config means `theme = "gig-light"` when that same layer sets no `theme`; `--light` means `--theme gig-light` (clap makes the two flags conflict); `GIG_TUI_LIGHT=1` means `GIG_TUI_THEME=gig-light` when `GIG_TUI_THEME` is unset. `light = false` selects nothing.
- Resolution: the name from the highest layer that sets one; none means `gig-dark`. A user theme file with that name wins over a built-in of the same name.
- Unknown name, or a user file that fails to load: the TUI opens with `gig-dark` and shows the reason as a `warranty` toast (`unknown theme "nrod", using gig-dark`). Startup is never blocked.
- `gig --list-themes` prints every available name, one per line, plain text (built-ins in the order above, then user themes alphabetically; a user file shadowing a built-in is listed once, in the built-in position). Broken user files go to stderr as `gig: theme <file>: <reason>`. Exit 0. It needs no terminal and no database.

### 14.3 Cycling

`T` (free today) cycles to the next theme: built-ins in order, then user themes alphabetically, wrapping. It applies at once, is not persisted, and shows the toast `theme nord  ·  set [tui] theme to keep`. The help popup footer names the current theme.

### 14.4 Where the theme applies

`bg` is painted on every cell of every frame (the terminal's own background is never shown), `surface` in popups. Text colours come only from the slots; no `Color::Reset` except under NO_COLOR.

### 14.5 256-colour terminals

When `COLORTERM` is not `truecolor` or `24bit` (plain tmux without `Tc`, many SSH sessions), each slot is quantised once at load to the nearest xterm-256 index (the 6x6x6 cube 16..231 and the grey ramp 232..255; nearest by squared distance in sRGB) and drawn as `Color::Indexed`. Users force truecolor with `COLORTERM=truecolor`.

### 14.6 NO_COLOR

When `NO_COLOR` is set and non-empty: every slot becomes `Color::Reset`, the `bg` paint is skipped, the selected row keeps the `▎` marker and the bold slug (no band), the active tab keeps its underline, status glyphs and words stay, the chart uses the texture rule of 11.2, and the popup scrim only strips bold and underline.

## 15. Theme file format

- Location: `<config_dir>/themes/<name>.toml`, that is `$XDG_CONFIG_HOME/gig/themes/<name>.toml` (default `~/.config/gig/themes/`), or `$GIG_HOME/config/themes/<name>.toml` when `GIG_HOME` is set. The directory is optional and is not created by gig.
- Name: the file stem. Allowed: `[a-z0-9][a-z0-9-]*`. Other files in the directory are ignored.
- Content: exactly the 15 slot keys of the built-in themes, each a string `"#rrggbb"` (hex digits in either case). No other keys, no sections, no inheritance. A missing key, an unknown key or a malformed value makes the file invalid; the error names the key (`theme mocha-soft: missing key "bar"`, `theme mocha-soft: "muted" is not #rrggbb`).
- Contrast on load: the thresholds of section 16.1 are checked; a failing file still loads, and the first failure is shown as a toast (`theme mocha-soft: muted 3.9:1 on sel, needs 4.5`).
- Loaded when the TUI starts and for `--list-themes`; `T` cycles through the themes loaded at start (no hot reload).
- Parsed with `toml` into a `BTreeMap<String, String>` (`deny_unknown_fields` semantics implemented by comparing keys), then into `Theme`.

Complete example, `~/.config/gig/themes/mocha-soft.toml` (the catppuccin-mocha values with a softer accent):

```toml
bg       = "#1e1e2e"
surface  = "#181825"
sel      = "#313244"
border   = "#45475a"
text     = "#cdd6f4"
muted    = "#a6adc8"
dim      = "#7f849c"
accent   = "#b4befe"
key      = "#cdd6f4"
unpaid   = "#f38ba8"
warranty = "#fab387"
queued   = "#89b4fa"
archived = "#969cb4"
bar      = "#6c7086"
bar_now  = "#b4befe"
```

Select it with `gig --theme mocha-soft`, `GIG_TUI_THEME=mocha-soft`, or `[tui] theme = "mocha-soft"`.

## 16. Palettes and contrast

### 16.1 Thresholds (the Rust test enforces all of them for every built-in)

WCAG 2 relative luminance and contrast ratio.

- `text`, `muted`, `accent`, `key`, `unpaid`, `warranty`, `queued`, `archived`: >= 4.5:1 on `bg`, on `sel` and on `surface`.
- `dim`: >= 3.0:1 on `bg`, `sel` and `surface` (it never carries information alone, section 2).
- `bar`, `bar_now`: >= 3.0:1 on `bg` (graphical objects).
- `sel` vs `bg`: 1.10 to 1.40 (a visible band, not a block); `sel` vs `surface`: >= 1.15 (the focused field stays visible in popups).
- `muted` vs `dim`: >= 1.4 (the two ink steps stay distinguishable; this is why `dim` is not raised to 4.5).
- `unpaid`, `warranty`, `queued` pairwise different; `bar_now` different from `bar`.
- `border` is decorative and unchecked; nothing readable is drawn in it.

### 16.2 Palettes

| slot | gig-dark | gig-light | catppuccin-mocha | catppuccin-latte | tokyonight | gruvbox-dark | nord | dracula |
|---|---|---|---|---|---|---|---|---|
| bg | `#14161a` | `#f7f6f2` | `#1e1e2e` | `#eff1f5` | `#1a1b26` | `#282828` | `#2e3440` | `#282a36` |
| surface | `#0f1115` | `#ffffff` | `#181825` | `#f9fafb` | `#16161e` | `#1d2021` | `#292e39` | `#21222c` |
| sel | `#252a32` | `#ebe8df` | `#313244` | `#dce0e8` | `#232538` | `#3c3836` | `#3a4150` | `#33364a` |
| border | `#2c313a` | `#d8d4c9` | `#45475a` | `#bcc0cc` | `#3b4261` | `#504945` | `#4c566a` | `#44475a` |
| text | `#dfe2e7` | `#1f2328` | `#cdd6f4` | `#4c4f69` | `#c0caf5` | `#ebdbb2` | `#eceff4` | `#f8f8f2` |
| muted | `#9ba2ad` | `#565c66` | `#a6adc8` | `#5c5f77` | `#a9b1d6` | `#bdae93` | `#d8dee9` | `#c3c6d9` |
| dim | `#6e7581` | `#7d838d` | `#7f849c` | `#797c91` | `#737aa2` | `#928374` | `#8a95ab` | `#7181ae` |
| accent | `#7fc8b6` | `#1d7563` | `#cba6f7` | `#7f2aef` | `#bb9af7` | `#8ec07c` | `#c2a3bb` | `#bd93f9` |
| key | `#dfe2e7` | `#1f2328` | `#cdd6f4` | `#4c4f69` | `#c0caf5` | `#ebdbb2` | `#eceff4` | `#f8f8f2` |
| unpaid | `#f26b63` | `#bf2f28` | `#f38ba8` | `#c30f36` | `#f7768e` | `#fe7c6b` | `#ed969c` | `#ff7878` |
| warranty | `#e6a23c` | `#945600` | `#fab387` | `#9e4d00` | `#e0af68` | `#fabd2f` | `#ebcb8b` | `#ffb86c` |
| queued | `#6ea8f2` | `#1f5fc0` | `#89b4fa` | `#0a54e6` | `#7aa2f7` | `#89a99c` | `#93b2cd` | `#8be9fd` |
| archived | `#8d939d` | `#626770` | `#969cb4` | `#5e6170` | `#848cb1` | `#b0a290` | `#a3adbf` | `#9aa0bd` |
| bar | `#5f6874` | `#898f99` | `#6c7086` | `#86899d` | `#5c6592` | `#7c6f64` | `#717e98` | `#6272a4` |
| bar_now | `#7fc8b6` | `#1d7563` | `#cba6f7` | `#7f2aef` | `#bb9af7` | `#8ec07c` | `#c2a3bb` | `#bd93f9` |

### 16.3 Contrast, computed

Output of the script in 16.5 (ratios to 2 decimals). "(from ...)" marks a value snapped from the editorial proposal to meet 16.1.

#### gig-dark
| slot | hex | on bg | on sel | on surface | need | ok |
|---|---|---|---|---|---|---|
| text | `#dfe2e7` | 13.95 | 11.11 | 14.55 | 4.5 | yes |
| muted | `#9ba2ad` | 7.04 | 5.61 | 7.35 | 4.5 | yes |
| accent | `#7fc8b6` | 9.35 | 7.45 | 9.75 | 4.5 | yes |
| key | `#dfe2e7` | 13.95 | 11.11 | 14.55 | 4.5 | yes |
| unpaid | `#f26b63` | 6.09 | 4.85 | 6.35 | 4.5 | yes |
| warranty | `#e6a23c` | 8.28 | 6.59 | 8.64 | 4.5 | yes |
| queued | `#6ea8f2` | 7.37 | 5.87 | 7.69 | 4.5 | yes |
| archived | `#8d939d` | 5.86 | 4.67 | 6.11 | 4.5 | yes |
| dim | `#6e7581` | 3.90 | 3.11 | 4.07 | 3.0 | yes |
| bar | `#5f6874` | 3.21 | - | - | 3.0 on bg | yes |
| bar_now | `#7fc8b6` | 9.35 | - | - | 3.0 on bg | yes |
| border | `#2c313a` | 1.39 | - | - | decorative | - |
| sel | `#252a32` | 1.26 | - | 1.31 | band: 1.10..1.40 vs bg, >= 1.15 vs surface | yes |
| surface | `#0f1115` | 1.04 | - | - | popup fill | - |
| muted/dim | | 1.80 | | | >= 1.4 (steps stay distinct) | yes |

#### gig-light
| slot | hex | on bg | on sel | on surface | need | ok |
|---|---|---|---|---|---|---|
| text | `#1f2328` | 14.61 | 12.89 | 15.80 | 4.5 | yes |
| muted | `#565c66` | 6.23 | 5.50 | 6.73 | 4.5 | yes |
| accent | `#1d7563` | 5.14 | 4.54 | 5.56 | 4.5 | yes |
| key | `#1f2328` | 14.61 | 12.89 | 15.80 | 4.5 | yes |
| unpaid | `#bf2f28` | 5.32 | 4.69 | 5.75 | 4.5 | yes |
| warranty | `#945600` | 5.41 | 4.77 | 5.85 | 4.5 | yes |
| queued | `#1f5fc0` | 5.62 | 4.96 | 6.08 | 4.5 | yes |
| archived | `#626770` (from `#646a73`) | 5.26 | 4.64 | 5.69 | 4.5 | yes |
| dim | `#7d838d` | 3.53 | 3.11 | 3.82 | 3.0 | yes |
| bar | `#898f99` | 3.01 | - | - | 3.0 on bg | yes |
| bar_now | `#1d7563` | 5.14 | - | - | 3.0 on bg | yes |
| border | `#d8d4c9` | 1.37 | - | - | decorative | - |
| sel | `#ebe8df` | 1.13 | - | 1.23 | band: 1.10..1.40 vs bg, >= 1.15 vs surface | yes |
| surface | `#ffffff` | 1.08 | - | - | popup fill | - |
| muted/dim | | 1.76 | | | >= 1.4 (steps stay distinct) | yes |

#### catppuccin-mocha
| slot | hex | on bg | on sel | on surface | need | ok |
|---|---|---|---|---|---|---|
| text | `#cdd6f4` | 11.34 | 8.69 | 12.14 | 4.5 | yes |
| muted | `#a6adc8` | 7.37 | 5.65 | 7.89 | 4.5 | yes |
| accent | `#cba6f7` | 8.07 | 6.19 | 8.64 | 4.5 | yes |
| key | `#cdd6f4` | 11.34 | 8.69 | 12.14 | 4.5 | yes |
| unpaid | `#f38ba8` | 7.08 | 5.43 | 7.58 | 4.5 | yes |
| warranty | `#fab387` | 9.27 | 7.10 | 9.92 | 4.5 | yes |
| queued | `#89b4fa` | 7.79 | 5.97 | 8.34 | 4.5 | yes |
| archived | `#969cb4` (from `#9399b2`) | 6.02 | 4.62 | 6.45 | 4.5 | yes |
| dim | `#7f849c` | 4.44 | 3.40 | 4.75 | 3.0 | yes |
| bar | `#6c7086` | 3.36 | - | - | 3.0 on bg | yes |
| bar_now | `#cba6f7` | 8.07 | - | - | 3.0 on bg | yes |
| border | `#45475a` | 1.80 | - | - | decorative | - |
| sel | `#313244` | 1.30 | - | 1.40 | band: 1.10..1.40 vs bg, >= 1.15 vs surface | yes |
| surface | `#181825` | 1.07 | - | - | popup fill | - |
| muted/dim | | 1.66 | | | >= 1.4 (steps stay distinct) | yes |

#### catppuccin-latte
| slot | hex | on bg | on sel | on surface | need | ok |
|---|---|---|---|---|---|---|
| text | `#4c4f69` | 7.06 | 6.04 | 7.64 | 4.5 | yes |
| muted | `#5c5f77` | 5.53 | 4.73 | 5.98 | 4.5 | yes |
| accent | `#7f2aef` | 5.34 | 4.56 | 5.77 | 4.5 | yes |
| key | `#4c4f69` | 7.06 | 6.04 | 7.64 | 4.5 | yes |
| unpaid | `#c30f36` | 5.40 | 4.61 | 5.84 | 4.5 | yes |
| warranty | `#9e4d00` | 5.28 | 4.51 | 5.72 | 4.5 | yes |
| queued | `#0a54e6` | 5.42 | 4.64 | 5.87 | 4.5 | yes |
| archived | `#5e6170` (from `#66697c`) | 5.43 | 4.64 | 5.88 | 4.5 | yes |
| dim | `#797c91` | 3.64 | 3.11 | 3.94 | 3.0 | yes |
| bar | `#86899d` | 3.06 | - | - | 3.0 on bg | yes |
| bar_now | `#7f2aef` | 5.34 | - | - | 3.0 on bg | yes |
| border | `#bcc0cc` | 1.61 | - | - | decorative | - |
| sel | `#dce0e8` | 1.17 | - | 1.27 | band: 1.10..1.40 vs bg, >= 1.15 vs surface | yes |
| surface | `#f9fafb` | 1.08 | - | - | popup fill | - |
| muted/dim | | 1.52 | | | >= 1.4 (steps stay distinct) | yes |

#### tokyonight
| slot | hex | on bg | on sel | on surface | need | ok |
|---|---|---|---|---|---|---|
| text | `#c0caf5` | 10.59 | 9.34 | 11.14 | 4.5 | yes |
| muted | `#a9b1d6` | 8.10 | 7.14 | 8.52 | 4.5 | yes |
| accent | `#bb9af7` | 7.39 | 6.52 | 7.77 | 4.5 | yes |
| key | `#c0caf5` | 10.59 | 9.34 | 11.14 | 4.5 | yes |
| unpaid | `#f7768e` | 6.46 | 5.70 | 6.80 | 4.5 | yes |
| warranty | `#e0af68` | 8.55 | 7.54 | 8.99 | 4.5 | yes |
| queued | `#7aa2f7` | 6.79 | 5.99 | 7.14 | 4.5 | yes |
| archived | `#848cb1` (from `#8189af`) | 5.18 | 4.57 | 5.45 | 4.5 | yes |
| dim | `#737aa2` | 4.10 | 3.61 | 4.31 | 3.0 | yes |
| bar | `#5c6592` | 3.04 | - | - | 3.0 on bg | yes |
| bar_now | `#bb9af7` | 7.39 | - | - | 3.0 on bg | yes |
| border | `#3b4261` | 1.74 | - | - | decorative | - |
| sel | `#232538` | 1.13 | - | 1.19 | band: 1.10..1.40 vs bg, >= 1.15 vs surface | yes |
| surface | `#16161e` | 1.05 | - | - | popup fill | - |
| muted/dim | | 1.98 | | | >= 1.4 (steps stay distinct) | yes |

#### gruvbox-dark
| slot | hex | on bg | on sel | on surface | need | ok |
|---|---|---|---|---|---|---|
| text | `#ebdbb2` | 10.75 | 8.45 | 11.95 | 4.5 | yes |
| muted | `#bdae93` | 6.77 | 5.32 | 7.53 | 4.5 | yes |
| accent | `#8ec07c` | 7.01 | 5.51 | 7.79 | 4.5 | yes |
| key | `#ebdbb2` | 10.75 | 8.45 | 11.95 | 4.5 | yes |
| unpaid | `#fe7c6b` | 5.83 | 4.59 | 6.49 | 4.5 | yes |
| warranty | `#fabd2f` | 8.69 | 6.84 | 9.67 | 4.5 | yes |
| queued | `#89a99c` | 5.77 | 4.54 | 6.42 | 4.5 | yes |
| archived | `#b0a290` (from `#a89984`) | 5.91 | 4.65 | 6.57 | 4.5 | yes |
| dim | `#928374` | 4.02 | 3.16 | 4.47 | 3.0 | yes |
| bar | `#7c6f64` | 3.03 | - | - | 3.0 on bg | yes |
| bar_now | `#8ec07c` | 7.01 | - | - | 3.0 on bg | yes |
| border | `#504945` | 1.67 | - | - | decorative | - |
| sel | `#3c3836` | 1.27 | - | 1.41 | band: 1.10..1.40 vs bg, >= 1.15 vs surface | yes |
| surface | `#1d2021` | 1.11 | - | - | popup fill | - |
| muted/dim | | 1.68 | | | >= 1.4 (steps stay distinct) | yes |

#### nord
| slot | hex | on bg | on sel | on surface | need | ok |
|---|---|---|---|---|---|---|
| text | `#eceff4` | 10.84 | 8.88 | 11.80 | 4.5 | yes |
| muted | `#d8dee9` | 9.25 | 7.58 | 10.07 | 4.5 | yes |
| accent | `#c2a3bb` | 5.50 | 4.51 | 5.99 | 4.5 | yes |
| key | `#eceff4` | 10.84 | 8.88 | 11.80 | 4.5 | yes |
| unpaid | `#ed969c` | 5.62 | 4.60 | 6.12 | 4.5 | yes |
| warranty | `#ebcb8b` | 8.00 | 6.55 | 8.71 | 4.5 | yes |
| queued | `#93b2cd` | 5.64 | 4.63 | 6.15 | 4.5 | yes |
| archived | `#a3adbf` | 5.52 | 4.53 | 6.01 | 4.5 | yes |
| dim | `#8a95ab` | 4.14 | 3.40 | 4.51 | 3.0 | yes |
| bar | `#717e98` | 3.06 | - | - | 3.0 on bg | yes |
| bar_now | `#c2a3bb` | 5.50 | - | - | 3.0 on bg | yes |
| border | `#4c566a` | 1.69 | - | - | decorative | - |
| sel | `#3a4150` | 1.22 | - | 1.33 | band: 1.10..1.40 vs bg, >= 1.15 vs surface | yes |
| surface | `#292e39` | 1.09 | - | - | popup fill | - |
| muted/dim | | 2.23 | | | >= 1.4 (steps stay distinct) | yes |

#### dracula
| slot | hex | on bg | on sel | on surface | need | ok |
|---|---|---|---|---|---|---|
| text | `#f8f8f2` | 13.36 | 11.15 | 14.81 | 4.5 | yes |
| muted | `#c3c6d9` | 8.41 | 7.02 | 9.32 | 4.5 | yes |
| accent | `#bd93f9` | 5.90 | 4.93 | 6.55 | 4.5 | yes |
| key | `#f8f8f2` | 13.36 | 11.15 | 14.81 | 4.5 | yes |
| unpaid | `#ff7878` | 5.57 | 4.65 | 6.17 | 4.5 | yes |
| warranty | `#ffb86c` | 8.36 | 6.97 | 9.27 | 4.5 | yes |
| queued | `#8be9fd` | 10.29 | 8.58 | 11.41 | 4.5 | yes |
| archived | `#9aa0bd` | 5.52 | 4.60 | 6.12 | 4.5 | yes |
| dim | `#7181ae` | 3.70 | 3.09 | 4.10 | 3.0 | yes |
| bar | `#6272a4` | 3.03 | - | - | 3.0 on bg | yes |
| bar_now | `#bd93f9` | 5.90 | - | - | 3.0 on bg | yes |
| border | `#44475a` | 1.56 | - | - | decorative | - |
| sel | `#33364a` | 1.20 | - | 1.33 | band: 1.10..1.40 vs bg, >= 1.15 vs surface | yes |
| surface | `#21222c` | 1.11 | - | - | popup fill | - |
| muted/dim | | 2.27 | | | >= 1.4 (steps stay distinct) | yes |

### 16.4 Changes from the editorial proposal

- `surface` moved so the band stays visible in popups (editorial never checked `surface`; gig-light `sel` on its `surface` was 1.03): gig-dark `#1c1f25` -> `#0f1115`, gig-light `#efede7` -> `#ffffff`, catppuccin-latte `#e6e9ef` -> `#f9fafb`, catppuccin-mocha `#181825` (unchanged), tokyonight `#16161e` (unchanged), gruvbox-dark `#32302f` -> `#1d2021`, nord `#3b4252` -> `#292e39`, dracula `#21222c` (unchanged).
- `archived` held to 4.5 on `sel` as well (editorial allowed 4.0), which snapped five values: gig-light `#646a73` -> `#626770`, catppuccin-mocha `#9399b2` -> `#969cb4`, catppuccin-latte `#66697c` -> `#5e6170`, tokyonight `#8189af` -> `#848cb1`, gruvbox-dark `#a89984` -> `#b0a290`.
- Everything else is editorial's palette, including its snaps from the upstream theme colours (catppuccin-latte accent, red, peach and blue darkened; gruvbox, nord and dracula reds lightened; four upstream selection colours softened: latte surface0 -> crust, tokyonight `#283457` -> `#232538`, nord nord2 -> `#3a4150`, dracula current-line -> `#33364a`).

### 16.5 Contrast script

Snapping steps HLS lightness by 0.01 (up on dark themes, down on light ones) until every threshold passes. Port it as a unit test over `Theme::BUILTIN` (the thresholds of 16.1) and reuse the same function for the load-time check of user files.

```python
import colorsys
def lum(h):
    c = [int(h[i:i+2], 16) / 255 for i in (1, 3, 5)]
    c = [x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in c]
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
def cr(a, b):
    la, lb = sorted((lum(a), lum(b)), reverse=True)
    return (la + 0.05) / (lb + 0.05)
TEXT = ["text", "muted", "accent", "key", "unpaid", "warranty", "queued", "archived"]
NEED = {**{r: 4.5 for r in TEXT}, "dim": 3.0}
def worst(t, r):
    return min(cr(t[r], t[b]) for b in ("bg", "sel", "surface"))
def step(h, d):
    r, g, b = [int(h[i:i+2], 16) / 255 for i in (1, 3, 5)]
    H, L, S = colorsys.rgb_to_hls(r, g, b)
    r, g, b = colorsys.hls_to_rgb(H, min(1, max(0, L + d)), S)
    return "#%02x%02x%02x" % tuple(round(x * 255) for x in (r, g, b))
def snap(t):
    d = 0.01 if lum(t["bg"]) < 0.2 else -0.01
    for r, need in NEED.items():
        while worst(t, r) < need:
            t[r] = step(t[r], d)
    for r in ("bar", "bar_now"):
        while cr(t[r], t["bg"]) < 3.0:
            t[r] = step(t[r], d)
    t["key"] = t["text"]
    return t
```

## 17. Mockups

Every line below was generated by a script that asserts each line is exactly W display cells (East Asian Wide and Fullwidth = 2) and each screen exactly H lines. Nerd Font glyphs are the real codepoints of section 5 (they show as blank or boxes without a Nerd Font). Data is from the live database on 2026-09-29; the requirement-change and note texts are shortened JOB.md content, with full-width punctuation replaced by ASCII. Colours are not drawn; the annotations say where they go.

### 17.1 Orders, 120x36 (Medium: list 76, gutter 2, pane 40)

Annotations: `gig` bold; `1 Orders` word bold with accent underline; `owed 1,600` bold unpaid. Row 1 is the idle message row. Header, group headings, totals, days, dates and the empty-section line muted. Both `▎` rows are on the `sel` band, marker accent, slug bold. `delivered` glyph and word unpaid; `paid` muted (point-cloud-registration's warranty ended 2026-04-30); `in progress` text. Pane title and section headings bold; link underlined. Footer keys bold, labels muted.

```
 gig   1 Orders   2 Drafts   3 Money   4 History                               owed 1,600  ·  sep 0  ·  2026 29,550 CNY 
                                                                                                                        
     order                   status         days     CNY  title                小鼠结肠炎 SERS 光谱分析与图表改版       
                                                                               sers-colitis-analysis ·  cv_ml · xianyu 
   owed  2                                         1,600                        delivered · 34 days in status          
 ▎  sers-colitis-analysis    delivered     34d     800  小鼠结肠炎 SERS 光…                                           
 ▎   collect payment  ·  sent 08-26  go.jczhang.cc/a30bd870                    price          800 CNY                   
    tk-dtf-compact           delivered      0d     800  TK DTF 生产图纵向…   cut             60%                      
                                                                               take-home      480 CNY                   
   paid  1                                         2,800                                                                
    point-cloud-registrat…   paid         167d   2,800  point-cloud-regist…  Next                                     
                                                                                 collect payment                 p paid 
   in progress  2                                  2,000                                                                
    bllc-reproduction        in progress    0d   2,000  BLLC 论文模型复现…   Packages  1                              
    high-value-patent-rev…   in progress    0d       ·  高价值专利识别论文…    sers-colitis-analysis-deliv…2026-08-26 
                                                                                 full · sent · oss · 08-26              
                                                                                 go.jczhang.cc/a30bd870                 
                                                                                                                        
                                                                               Requirement changes  4                   
                                                                                 08-21  JC要求把模型优化的验证方案改为  
                                                                                        客户现有代码重跑方案            
                                                                                 08-21  JC将Phase 4达标标准改为仅看AUC, 
                                                                                         四个任务分别AUC>=0.90即可      
                                                                                 08-22  按代码与历史产物重新确认的用户  
                                                                                        原始方案执行                    
                                                                                 08-22  最终DSS模型必须保留OPLS-DA, 继  
                                                                                        续在现有代码上优化              
                                                                                                                        
                                                                               Notes  1                                 
                                                                                 08-21  quote draft, min/rec/max 300 /  
                                                                                        800 / 800                       
                                                                                                                        
                                                                               no status entries · no client questions  
                                                                               no scorecard (k records one)             
                                                                                                                        
 p paid  y copy link  n note  k score   ·   / filter  a archived  N new  ? keys  q quit                                 
```

### 17.2 Orders, 80x24 (Narrow: list 78, title 21)

```
 gig  1 Orders  2 Drafts  3 Money  4 History               owed 1,600  ·  sep 0 
                                                                                
     order                   status         days     CNY  title                 
                                                                                
   owed  2                                         1,600                        
 ▎  sers-colitis-analysis    delivered     34d     800  小鼠结肠炎 SERS 光谱… 
 ▎   collect payment  ·  sent 08-26  go.jczhang.cc/a30bd870                     
    tk-dtf-compact           delivered      0d     800  TK DTF 生产图纵向压…  
                                                                                
   paid  1                                         2,800                        
    point-cloud-registrat…   paid         167d   2,800  point-cloud-registra… 
                                                                                
   in progress  2                                  2,000                        
    bllc-reproduction        in progress    0d   2,000  BLLC 论文模型复现与…  
    high-value-patent-rev…   in progress    0d       ·  高价值专利识别论文重… 
                                                                                
                                                                                
                                                                                
                                                                                
                                                                                
                                                                                
                                                                                
                                                                                
 Enter detail  p paid  y copy link  n note   ·   / filter  ? keys  q quit       
```

### 17.3 Money, 120x36

Annotations: tile values bold (`1,600 CNY` unpaid); chart Apr/May/Jun/Jul bars in `bar`, Sep is the current month (zero: its `┈` is in `bar_now`, its month label bold); `─` border, `┈` dim; value labels text; month and year rows muted.

```
 gig   1 Orders   2 Drafts   3 Money   4 History                               owed 1,600  ·  sep 0  ·  2026 29,550 CNY 
                                                                                                                        
 outstanding                            received in September                  received in 2026                         
 1,600 CNY                              0 CNY                                  29,550 CNY                               
 take-home 960 · 2 orders               take-home 0                            take-home 17,730 · 4 months              
                                                                                                                        
 Received per month                                                                         Oct 2025 - Sep 2026  ·  CNY 
                                                                                                                        
                                                                           12.9k                                        
                                                                           █████                                        
                                                         10.0k             █████                                        
                                                         ▆▆▆▆▆             █████                                        
                                                         █████             █████                                        
                                                         █████             █████                                        
                                                         █████             █████                                        
                                                         █████    3.1k     █████    3.6k                                
                                                         █████    ▃▃▃▃▃    █████    ▆▆▆▆▆                               
                                                         █████    █████    █████    █████                               
                                                         █████    █████    █████    █████                               
 ──┈┈┈┈┈────┈┈┈┈┈────┈┈┈┈┈────┈┈┈┈┈────┈┈┈┈┈────┈┈┈┈┈────────────────────────────────────────┈┈┈┈┈────┈┈┈┈┈──           
    Oct      Nov      Dec      Jan      Feb      Mar      Apr      May      Jun      Jul      Aug      Sep              
    2025                       2026                                                                                     
                                                                                                                        
 Outstanding  2               1,600                                                                                     
    order                      CNY  since  title                                                                        
 ▎  sers-colitis-analysis      800    34d  小鼠结肠炎 SERS 光谱分析与图表改版                                          
    tk-dtf-compact             800     0d  TK DTF 生产图纵向压缩工具                                                   
                                                                                                                        
                                                                                                                        
                                                                                                                        
                                                                                                                        
                                                                                                                        
                                                                                                                        
                                                                                                                        
                                                                                                                        
 Enter open order  y copy link   ·   1-4 views  T theme  ? keys  q quit                                                 
```

### 17.4 Help popup (72 wide; first rows shown, the real popup lists every key of `help.rs`)

```
╭─ keys ───────────────────────────────────────────────────────────────╮
│                                                                      │
│  global                  orders                                      │
│     ?  keys                 s  start      when queued or delivered   │
│     q  quit                 p  paid       when delivered             │
│     r  refresh              $  price                                 │
│   1-4  views                c  change     when not archived          │
│   Tab  next view            n  note                                  │
│     /  filter               k  scorecard                             │
│   Esc  close, clear         x  cancel     when queued or in progress │
│     T  next theme           u  upload     when a package is checked  │
│  Home  first row            m  mark sent  when a package is checked  │
│                                                                      │
│  theme gig-dark   ·   T cycles, [tui] theme keeps it                 │
╰──────────────────────────────────────────────────────────────────────╯
```

### 17.5 New order form (64 wide; `slug` focused and invalid)

The `█` stands for the terminal cursor.

```
╭─ new order ──────────────────────────────────────────────────╮
│                                                              │
│▎           slug  sers-colitis-v2█                            │
│                  ! slug already exists                       │
│           title  小鼠结肠炎 SERS 二期                        │
│           price  1200                                        │
│            type  ‹ cv_ml ›                                   │
│        material  ~/Downloads/sers-v2.zip                     │
│        platform  xianyu                                      │
│    client words  3 lines, Enter opens $EDITOR                │
│      from draft  [ ] no                                      │
│                                                              │
│  Tab next  Space choose  Enter submit  Esc cancel            │
╰──────────────────────────────────────────────────────────────╯
```

## 18. Tests

Keep and update the existing `TestBackend` tests (literal strings change: `in_progress` -> `in progress`, `owed 1600` -> `owed 1,600`, `Latest status  (none)` -> the empty-section line; filter and error text move from the hint line to row 1). Add:

- Render at 80x24, 120x36 and 200x50 for every view; no cell outside the frame, no panic, for every built-in theme and with `--no-icons`.
- 120x36 Orders: every unselected order row shows a title prefix of at least 18 cells; the pane contains no line consisting of a single character; the package link line equals `go.jczhang.cc/...` whole.
- 80x24 Orders: title column 21 cells; `(no status entry` appears nowhere.
- `--no-icons`: the slug column starts exactly 2 cells further left.
- Money at 80x24 and 120x36: all 12 month labels present; a zero month's baseline footprint is `┈`; the chart occupies exactly `h + 4` rows.
- Popup: after drawing a popup over a CJK row, no continuation cell is left right of the rect; every non-popup cell has fg `dim`.
- Min size: at 59x16 the only non-blank text is the `gig needs 60x16` line.
- `theme.rs`: the thresholds of 16.1 over `Theme::BUILTIN`; `status_colours_are_fixed` over every built-in.
- Theme files: a valid file loads; missing, unknown and malformed keys each give the named error; a user file shadows a built-in; unknown names fall back to gig-dark with a warning.
- Settings: precedence config < flag < env for `theme`, and the `light` alias per layer.
- `text.rs`: `group(29550) == "29,550"`, `compact(12850) == "12.9k"`, `compact(800) == "800"`, `compact(10000) == "10.0k"`, middle truncation keeps the last 10 cells.
- CLI: `gig --list-themes` prints 8 names without a database; bare `gig` with stdout not a terminal behaves exactly as today (clap's missing-subcommand error, exit 2); every JSON command's output is unchanged.

## 19. Change list

Rule references are to sections of this file.

| file | change | rules |
|---|---|---|
| `crates/gig-cli/src/cli.rs` | `Cli.command: Option<Command>`; `#[command(flatten)] tui: TuiArgs` on `Cli` with `args_conflicts_with_subcommands = true`; `TuiArgs` gains `--theme <NAME>` (conflicts with `--light`) and `--list-themes`; `gig tui` keeps `TuiArgs` and its help says "same as bare gig"; `command_name()` handles `None` | 14.2 |
| `crates/gig-cli/src/main.rs` | `None` + `--list-themes`: print names, exit 0. `None` with stdin and stdout terminals (`std::io::IsTerminal`): run the TUI. `None` otherwise: emit clap's `MissingSubcommand` error exactly as today (exit 2), so agents see no change. `Some(Tui)`: as today | 14.2, 18 |
| `crates/gig-cli/src/run.rs` | match arms for the `Option` | |
| `crates/gig-core/src/config.rs` | `Tui.theme: Option<String>`; `GIG_TUI_THEME` in `apply_overrides`; `Paths::themes_dir() = config_dir/themes`; tests for parse and env | 14.2, 15 |
| `crates/gig-tui/Cargo.toml` | add `toml` (workspace) | 15 |
| `crates/gig-tui/src/theme.rs` | new `Theme` slots and `Cow` name; `BUILTIN` with the 8 palettes of 16.2; `by_name`; `OVERDUE_DAYS`; contrast functions and the 16.1 test; quantise-to-256 and NO_COLOR variants; style helpers renamed (`fg` -> `text`, `inactive` -> `archived`, `selection_bg` -> `sel`, `error`/`ok` removed) | 2, 13, 14, 16 |
| `crates/gig-tui/src/themes.rs` (new) | user theme files: scan, parse, validate, shadow built-ins; `available()` for `--list-themes` and `T` | 15 |
| `crates/gig-tui/src/lib.rs` | `Opts.theme`; `light` alias per layer; theme resolution with fallback warning; `pub fn list_themes()` | 14.2 |
| `crates/gig-tui/src/terminal.rs` | colour-mode detection (`COLORTERM`, `NO_COLOR`) | 14.5, 14.6 |
| `crates/gig-tui/src/app.rs` | `T` cycles themes; toast state with 3 s expiry; filter and errors move to the message row; list selection skips group headings | 7.2, 8.1, 14.3 |
| `crates/gig-tui/src/ui.rs` | `WidthClass`; frame rows; min-size guard; banner and degradation; message row; footer groups and degradation; pane split per 6.3 replacing `Percentage(55)`; paint `bg` | 6, 7 |
| `crates/gig-tui/src/help.rs` | contextual footer keys by order state (7.3); help preconditions; `T` in global keys; theme footer | 7.3, 12.4 |
| `crates/gig-tui/src/popup.rs` | widths, 1/3 placement, padding, tones, scrim, straddle fix, form anatomy (marker, band, cursor, placeholders, `!` errors), confirm/refusal/result/progress styling | 12 |
| `crates/gig-tui/src/text.rs` | `group(i64)`, `compact(i64)`, `truncate_middle(s, max, keep)` | 3, 9.2, 11.2 |
| `crates/gig-tui/src/icons.rs` | remove `view`; status glyph table unchanged | 5 |
| `crates/gig-tui/src/views/mod.rs` | shared column builder, group headings with right-aligned totals, sticky heading, empty-cell dot | 8.1, 10.2 |
| `crates/gig-tui/src/views/orders.rs` | columns of 6.3, groups, row styles, second-line rules | 6.3, 8 |
| `crates/gig-tui/src/views/detail.rs` | header block and mini statement, section rules, empty-section line, stacked packages, dated entries capped at 2 lines, scroll indicators, prose measure | 9 |
| `crates/gig-tui/src/views/drafts.rs` | columns, NOTES.md tail pane at Medium and Wide, empty state | 10.1 |
| `crates/gig-tui/src/views/history.rs` | columns, empty-cell dots, archived rows | 10.2 |
| `crates/gig-tui/src/views/money.rs` | tiles, chart geometry, labels, baseline, month and year rows, empty state, outstanding table | 11 |
| `crates/gig-tui/src/data/money.rs` | month totals keyed for the 12-month window including zero months; per-group totals for headings | 8.1, 11 |
| `docs/v2/TUI-SPEC.md` | entry, flags, section 3 pointer, section 5 keys, section 7 scope (done with this file) | |
| `README.md`, `skill/partjob/references/gig.md` | say bare `gig` opens the dashboard in a terminal, `gig tui` stays; mention `--theme` and the themes directory | |

## 20. Deferred

- `theme = "auto"` from the terminal's reported background (OSC 11). Under tmux it needs `allow-passthrough on` and still fails in some setups; not in this round.
- Hot reload of theme files.
- Month-grouped History.
