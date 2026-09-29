//! Key hints: the contextual footer (TUI-DESIGN.md section 7.3) and the
//! `?` popup (section 12.4).

use crate::app::{UiState, View};
use crate::data::OrderRow;
use crate::popup::{self, Tone};
use crate::text;
use crate::ui::{RenderCx, WidthClass};
use gig_core::models::OrderStatus;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// A key and what it does.
pub type Pair = (&'static str, &'static str);

/// A key, what it does, and when it applies (empty: always).
type Entry = (&'static str, &'static str, &'static str);

/// Keys of the whole app, navigation included.
const GLOBAL_KEYS: &[Entry] = &[
    ("?", "keys", ""),
    ("q", "quit", ""),
    ("r", "refresh", ""),
    ("1-4", "views", ""),
    ("Tab", "next view", ""),
    ("/", "filter", ""),
    ("Esc", "close, clear", ""),
    ("T", "theme picker", ""),
    (",", "settings", ""),
    ("M", "mouse on/off", ""),
    ("\u{2191}\u{2193}", "select", ""),
    ("Enter", "open", ""),
    ("PgDn", "scroll detail", ""),
];

/// Keys that act on the selected order (Orders, or any order detail).
const ORDER_KEYS: &[Entry] = &[
    ("s", "start", "when queued or delivered"),
    ("p", "paid", "when delivered"),
    ("$", "price", ""),
    ("c", "change", "when not archived"),
    ("n", "note", ""),
    ("k", "scorecard", ""),
    ("x", "cancel", "when queued or in progress"),
    ("A", "archive preview", ""),
    ("u", "upload", "when a package is checked"),
    ("m", "mark sent", "when a package is checked"),
    ("U", "upload artifact", ""),
    ("e", "edit JOB.md", ""),
    ("y", "copy link", ""),
    ("N", "new order", ""),
    ("a", "show archived", ""),
];

const DRAFTS_KEYS: &[Entry] = &[
    ("Enter", "NOTES.md tail", ""),
    ("N", "new draft", ""),
    ("P", "promote to an order", ""),
];

const MONEY_KEYS: &[Entry] = &[
    ("Enter", "open order", ""),
    ("y", "copy link", ""),
    ("Esc", "close the month", "after a bar click"),
];

const HISTORY_KEYS: &[Entry] = &[("Enter", "detail", "order keys work there")];

const SETTINGS_KEYS: &[Entry] = &[
    ("\u{2191}\u{2193}", "select", ""),
    ("Space", "toggle", "yes or no rows"),
    ("+ -", "step", "number rows"),
    ("0-9", "type a value", "number rows"),
    ("Enter", "edit or choose", ""),
    ("T", "theme picker", ""),
    ("Esc", "cancel typing, close", ""),
];

/// Heading and keys of the right column of the help popup.
fn view_keys(state: &UiState) -> (&'static str, &'static [Entry]) {
    if state.settings.is_some() {
        return ("settings", SETTINGS_KEYS);
    }
    if state.detail_open {
        return ("order", ORDER_KEYS);
    }
    match state.view {
        View::Orders => ("orders", ORDER_KEYS),
        View::Drafts => ("drafts", DRAFTS_KEYS),
        View::Money => ("money", MONEY_KEYS),
        View::History => ("history", HISTORY_KEYS),
    }
}

/// The actions that fit an order's state (a hint, not a guard).
pub fn order_actions(row: &OrderRow) -> Vec<Pair> {
    match row.order.status {
        OrderStatus::Queued => vec![
            ("s", "start"),
            ("$", "price"),
            ("n", "note"),
            ("x", "cancel"),
        ],
        OrderStatus::InProgress => vec![
            ("u", "upload"),
            ("m", "mark sent"),
            ("c", "change"),
            ("n", "note"),
        ],
        OrderStatus::Delivered => vec![
            ("p", "paid"),
            ("y", "copy link"),
            ("n", "note"),
            ("k", "score"),
        ],
        OrderStatus::Paid if row.next_action == "archive" => {
            vec![("A", "archive"), ("y", "copy link"), ("n", "note")]
        }
        OrderStatus::Paid => vec![("y", "copy link"), ("n", "note"), ("k", "score")],
        OrderStatus::Archived | OrderStatus::Cancelled => {
            vec![("y", "copy link"), ("e", "JOB.md")]
        }
    }
}

/// The two footer groups before degradation.
pub fn footer_groups(state: &UiState, class: WidthClass) -> (Vec<Pair>, Vec<Pair>) {
    let actions = || {
        state
            .selected_order()
            .map(order_actions)
            .unwrap_or_default()
    };
    if let Some(s) = &state.settings {
        let g2 = if s.edit.is_some() {
            vec![("?", "keys")]
        } else {
            vec![
                ("Esc", "close"),
                ("T", "theme"),
                ("?", "keys"),
                ("q", "quit"),
            ]
        };
        return (crate::settings::footer_keys(s), g2);
    }
    if state.detail_open {
        let mut g1 = vec![("Esc", "back")];
        g1.extend(actions());
        return (g1, vec![("?", "keys"), ("q", "quit")]);
    }
    match state.view {
        View::Orders => {
            let mut g1 = Vec::new();
            if class == WidthClass::Narrow && state.selected_order().is_some() {
                g1.push(("Enter", "detail"));
            }
            g1.extend(actions());
            let archived = if state.show_closed {
                ("a", "hide archived")
            } else {
                ("a", "archived")
            };
            (
                g1,
                vec![
                    ("/", "filter"),
                    archived,
                    ("N", "new"),
                    (",", "settings"),
                    ("?", "keys"),
                    ("q", "quit"),
                ],
            )
        }
        View::Drafts => {
            let enter = if state.notes_pane.is_some() {
                ("Enter", "close notes")
            } else {
                ("Enter", "notes")
            };
            let g1 = if state.selected_draft().is_some() {
                vec![enter, ("N", "new draft"), ("P", "promote")]
            } else {
                vec![("N", "new draft")]
            };
            (
                g1,
                vec![
                    ("/", "filter"),
                    (",", "settings"),
                    ("?", "keys"),
                    ("q", "quit"),
                ],
            )
        }
        View::Money => {
            let mut g1 = if state.selected_owed().is_some() {
                vec![("Enter", "open order"), ("y", "copy link")]
            } else {
                Vec::new()
            };
            // A bar was clicked: its month is listed under the chart.
            if state.money_month.is_some() {
                g1.push(("Esc", "close month"));
            }
            (
                g1,
                vec![
                    ("1-4", "views"),
                    ("T", "theme"),
                    (",", "settings"),
                    ("?", "keys"),
                    ("q", "quit"),
                ],
            )
        }
        View::History => {
            let g1 = if state.selected_order().is_some() {
                vec![("Enter", "detail")]
            } else {
                Vec::new()
            };
            (
                g1,
                vec![
                    ("/", "filter"),
                    (",", "settings"),
                    ("?", "keys"),
                    ("q", "quit"),
                ],
            )
        }
    }
}

/// Keys whose pair goes first when the footer is short.
const OPTIONAL: [&str; 5] = ["N", "a", "T", "1-4", ","];

/// Cells a footer takes: pairs 2 apart, groups joined by `   ·   `.
pub fn footer_width(g1: &[Pair], g2: &[Pair]) -> usize {
    let group = |g: &[Pair]| {
        g.iter()
            .map(|(k, l)| text::width(k) + 1 + text::width(l))
            .sum::<usize>()
            + 2 * g.len().saturating_sub(1)
    };
    let join = if g1.is_empty() || g2.is_empty() { 0 } else { 7 };
    group(g1) + join + group(g2)
}

/// Whole pairs dropped until the footer fits `room` cells, in the order
/// of section 7.3. `? keys` is never dropped.
pub fn fit_footer(mut g1: Vec<Pair>, mut g2: Vec<Pair>, room: usize) -> (Vec<Pair>, Vec<Pair>) {
    let fits = |a: &[Pair], b: &[Pair]| footer_width(a, b) <= room;
    if fits(&g1, &g2) {
        return (g1, g2);
    }
    g2.retain(|(k, _)| !OPTIONAL.contains(k));
    while g1.len() > 2 && !fits(&g1, &g2) {
        g1.pop();
    }
    for key in ["/", "q"] {
        if !fits(&g1, &g2) {
            g2.retain(|(k, _)| *k != key);
        }
    }
    while !g1.is_empty() && !fits(&g1, &g2) {
        g1.pop();
    }
    (g1, g2)
}

/// The footer line: keys bold `key`, labels `muted`, groups joined by a
/// `dim` dot.
pub fn footer_line(cx: &RenderCx, g1: &[Pair], g2: &[Pair]) -> Line<'static> {
    let t = cx.theme;
    let mut spans = Vec::new();
    let push_group = |spans: &mut Vec<Span<'static>>, g: &[Pair]| {
        for (i, (k, l)) in g.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw("  "));
            }
            spans.push(Span::styled(*k, t.key()));
            spans.push(Span::styled(format!(" {l}"), t.muted()));
        }
    };
    push_group(&mut spans, g1);
    if !g1.is_empty() && !g2.is_empty() {
        spans.push(Span::raw("   "));
        spans.push(Span::styled("\u{b7}", t.dim()));
        spans.push(Span::raw("   "));
    }
    push_group(&mut spans, g2);
    Line::from(spans)
}

/// One column of the help popup: the heading, then keys right-aligned in
/// a 4-cell column, 2 cells, the label, and the precondition in `muted`
/// (section 12.4). A longer key (`Enter`) reaches left of the column; the
/// lines then start with that many cells, returned as the overflow, and the
/// caller draws the column that much further left.
fn key_column(cx: &RenderCx, heading: &str, keys: &[Entry]) -> (Vec<Line<'static>>, u16) {
    let t = cx.theme;
    let key_w = keys
        .iter()
        .map(|(k, ..)| text::width(k))
        .max()
        .unwrap_or(0)
        .max(4);
    // Only labels followed by a precondition set the label column.
    let label_w = keys
        .iter()
        .filter(|(.., when)| !when.is_empty())
        .map(|(_, l, _)| text::width(l))
        .max()
        .unwrap_or(0);
    let overflow = key_w - 4;
    let mut out = vec![Line::from(vec![
        Span::raw(" ".repeat(overflow)),
        Span::styled(heading.to_string(), t.title()),
    ])];
    for (k, label, when) in keys {
        let mut spans = vec![
            Span::styled(format!("{k:>key_w$}"), t.key()),
            Span::raw("  "),
        ];
        if when.is_empty() {
            spans.push(Span::styled(label.to_string(), t.text()));
        } else {
            spans.push(Span::styled(text::fit(label, label_w), t.text()));
            spans.push(Span::raw("  "));
            spans.push(Span::styled(when.to_string(), t.muted()));
        }
        out.push(Line::from(spans));
    }
    (out, overflow as u16)
}

/// The `?` popup: `global` on the left, the current view's keys on the
/// right, the theme on the footer line.
pub fn render(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let t = cx.theme;
    let (heading, keys) = view_keys(cx.state);
    let (left, left_over) = key_column(cx, "global", GLOBAL_KEYS);
    let (right, right_over) = key_column(cx, heading, keys);
    // Width of the left column from its 4-cell key column, plus the gap.
    let left_w = left.iter().map(Line::width).max().unwrap_or(0) - usize::from(left_over) + 3;
    let footer = Line::from(vec![
        Span::styled(format!("theme {}", t.name), t.muted()),
        Span::raw("   "),
        Span::styled("\u{b7}", t.dim()),
        Span::raw("   "),
        Span::styled("T", t.key()),
        Span::styled(" picks and keeps a theme  ", t.muted()),
        Span::styled(",", t.key()),
        Span::styled(" settings", t.muted()),
    ]);
    let rows = left.len().max(right.len());
    let width = 72.min(area.width.saturating_sub(4));
    let hits = &cx.state.hits;
    let inner = popup::open(
        frame,
        area,
        width,
        rows as u16 + 2,
        "keys",
        Tone::Plain,
        t,
        hits,
    );
    if let Some(m) = hits.map().modal {
        hits.pane(m, crate::mouse::Pane::Help);
    }
    let body = Rect {
        height: inner.height.saturating_sub(2),
        ..inner
    };
    // Overflowing keys use the popup padding (left) or the gap (right).
    let left_over = left_over.min(body.x - area.x);
    let right_x = (left_w as u16).min(body.width);
    // Both columns scroll together (Up/Down/PgUp/PgDn/Home/End); at the
    // end only the top marker shows, as in the detail pane.
    let h = usize::from(body.height);
    let total = left.len().max(right.len());
    let max = if total <= h || h < 3 {
        0
    } else {
        total + 1 - h
    };
    let scroll = &cx.state.help_scroll;
    scroll.max.set(u16::try_from(max).unwrap_or(u16::MAX));
    let offset = usize::from(scroll.offset).min(max);
    let window = |lines: Vec<Line<'static>>, over: u16| -> Vec<Line<'static>> {
        if max == 0 {
            return lines.into_iter().take(h).collect();
        }
        let pad = " ".repeat(usize::from(over));
        let marker = |s: String| Line::from(Span::styled(format!("{pad}{s}"), t.muted()));
        let top = usize::from(offset > 0);
        let mut shown = h - top;
        if offset + shown < total {
            shown -= 1;
        }
        let len = lines.len();
        let mut out = Vec::new();
        if top > 0 {
            // A shorter column counts only its own rows; the row stays so
            // both columns line up.
            let above = offset.min(len);
            out.push(if above > 0 {
                marker(format!("\u{2191} {above} above"))
            } else {
                Line::raw("")
            });
        }
        out.extend(lines.into_iter().skip(offset).take(shown));
        if offset + shown < total {
            while out.len() < h - 1 {
                out.push(Line::raw(""));
            }
            let below = len.saturating_sub(offset + shown);
            if below > 0 {
                out.push(marker(format!("\u{2193} {below} more")));
            }
        }
        out
    };
    frame.render_widget(
        Paragraph::new(window(left, left_over)),
        Rect {
            x: body.x - left_over,
            width: right_x + left_over,
            ..body
        },
    );
    let right_over = right_over.min(right_x);
    frame.render_widget(
        Paragraph::new(window(right, right_over)),
        Rect {
            x: body.x + right_x - right_over,
            width: body.width - right_x + right_over,
            ..body
        },
    );
    if inner.height >= 1 {
        frame.render_widget(
            Paragraph::new(footer),
            Rect {
                y: inner.bottom() - 1,
                height: 1,
                ..inner
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DELIVERED: [Pair; 5] = [
        ("Enter", "detail"),
        ("p", "paid"),
        ("y", "copy link"),
        ("n", "note"),
        ("k", "score"),
    ];
    const ORDERS: [Pair; 5] = [
        ("/", "filter"),
        ("a", "archived"),
        ("N", "new"),
        ("?", "keys"),
        ("q", "quit"),
    ];

    #[test]
    fn footer_degrades_by_whole_pairs() {
        // Mockup 17.2: 80 columns, delivered selected.
        let (g1, g2) = fit_footer(DELIVERED.to_vec(), ORDERS.to_vec(), 78);
        assert_eq!(g1, DELIVERED[..4].to_vec());
        assert_eq!(g2, vec![("/", "filter"), ("?", "keys"), ("q", "quit")]);
        // Everything fits at 200.
        let (g1, g2) = fit_footer(DELIVERED.to_vec(), ORDERS.to_vec(), 198);
        assert_eq!((g1.len(), g2.len()), (5, 5));
        // `? keys` survives anything.
        let (g1, g2) = fit_footer(DELIVERED.to_vec(), ORDERS.to_vec(), 4);
        assert!(g1.is_empty());
        assert_eq!(g2, vec![("?", "keys")]);
        for room in 0..200 {
            let (a, b) = fit_footer(DELIVERED.to_vec(), ORDERS.to_vec(), room);
            assert!(footer_width(&a, &b) <= room || (a.is_empty() && b.len() == 1));
        }
    }
}
