//! The `?` popup: global keys on the left, the current view's keys on the right.

use crate::app::View;
use crate::ui::RenderCx;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};
use ratatui::Frame;

pub const GLOBAL_KEYS: &[(&str, &str)] = &[
    ("?", "help"),
    ("q", "quit"),
    ("r", "refresh"),
    ("1..4", "jump to a view"),
    ("Tab", "next view"),
    ("/", "filter the list"),
    ("Esc", "close / clear filter"),
];

const ORDERS_KEYS: &[(&str, &str)] = &[
    ("Up/Dn", "select"),
    ("Enter", "detail (narrow)"),
    ("PgDn", "scroll detail"),
    ("a", "toggle archived"),
    ("s", "start"),
    ("p", "paid"),
    ("$", "price"),
    ("c", "change"),
    ("n", "note"),
    ("k", "scorecard"),
    ("x", "cancel"),
    ("A", "archive preview"),
    ("u", "upload package"),
    ("m", "mark sent"),
    ("U", "upload artifact"),
    ("e", "edit JOB.md"),
    ("y", "copy link"),
    ("N", "new order"),
];

const DRAFTS_KEYS: &[(&str, &str)] = &[
    ("Up/Dn", "select"),
    ("Enter", "notes tail"),
    ("N", "new draft"),
    ("P", "promote to order"),
];

const MONEY_KEYS: &[(&str, &str)] = &[];

const HISTORY_KEYS: &[(&str, &str)] = &[
    ("Up/Dn", "select"),
    ("Enter", "detail (order keys work there)"),
];

pub fn view_keys(view: View) -> &'static [(&'static str, &'static str)] {
    match view {
        View::Orders => ORDERS_KEYS,
        View::Drafts => DRAFTS_KEYS,
        View::Money => MONEY_KEYS,
        View::History => HISTORY_KEYS,
    }
}

/// Keys of what is on screen: the order keys whenever an order detail is
/// open (from History too, where every order action works), else the
/// view's own keys.
pub fn keys_for(state: &crate::app::UiState) -> &'static [(&'static str, &'static str)] {
    if state.detail_open {
        ORDERS_KEYS
    } else {
        view_keys(state.view)
    }
}

fn key_lines<'a>(cx: &RenderCx, heading: &'a str, keys: &[(&'a str, &'a str)]) -> Vec<Line<'a>> {
    let mut lines = vec![Line::from(Span::styled(heading, cx.theme.title()))];
    for (k, what) in keys {
        lines.push(Line::from(vec![
            Span::styled(format!("{k:>6}  "), cx.theme.key()),
            Span::styled(*what, cx.theme.text()),
        ]));
    }
    lines
}

/// Rounded box centred in `area`, clamped to it.
pub fn render(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let view = cx.state.view;
    let keys = keys_for(cx.state);
    let heading = if cx.state.detail_open {
        "order detail"
    } else {
        view.title()
    };
    let rows = GLOBAL_KEYS.len().max(keys.len()) as u16 + 1;
    let popup = area.centered(
        Constraint::Length(72.min(area.width)),
        Constraint::Length((rows + 2).min(area.height)),
    );
    frame.render_widget(Clear, popup);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(cx.theme.border())
        .title(Span::styled(" keys ", cx.theme.title()))
        .style(cx.theme.base());
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    // The global column is as wide as its longest line, so nothing in it
    // is cut; the view keys take the rest.
    let global = key_lines(cx, "global", GLOBAL_KEYS);
    let left_w = global.iter().map(Line::width).max().unwrap_or(0) as u16 + 2;
    let [left, right] =
        Layout::horizontal([Constraint::Length(left_w), Constraint::Min(0)]).areas(inner);
    frame.render_widget(Paragraph::new(global), left);
    if !keys.is_empty() {
        frame.render_widget(Paragraph::new(key_lines(cx, heading, keys)), right);
    }
}
