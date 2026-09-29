//! One module per view. Each exposes `render(frame, area, cx)`.
//!
//! Lists are drawn as plain lines (whitespace and colour, no table borders,
//! spec section 3) with columns sized in display cells, so Chinese titles
//! stay aligned.

pub mod detail;
pub mod drafts;
pub mod history;
pub mod money;
pub mod orders;

use crate::app::View;
use crate::text;
use crate::theme::Theme;
use crate::ui::RenderCx;
use gig_core::models::OrderStatus;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// Draw the body of `view` into `area`.
pub fn render(frame: &mut Frame, area: Rect, view: View, cx: &RenderCx) {
    match view {
        View::Orders => orders::render(frame, area, cx),
        View::Drafts => drafts::render(frame, area, cx),
        View::Money => money::render(frame, area, cx),
        View::History => history::render(frame, area, cx),
    }
}

/// `3 of 5`: rows the current view's filter keeps, of all its rows.
pub fn filter_count(state: &crate::app::UiState) -> String {
    let (shown, all) = match state.view {
        View::Orders => (
            state.order_list().len(),
            state.data.active_orders(state.show_closed).len(),
        ),
        View::History => (state.order_list().len(), state.data.orders.len()),
        View::Drafts => (state.draft_list().len(), state.data.open_drafts().count()),
        View::Money => (0, 0),
    };
    format!("{shown} of {all}")
}

/// First item to draw so that item `selected` (of rows `heights`) is fully
/// inside `rows` lines. The selection sits as low as needed, never lower.
pub(crate) fn window_start(heights: &[u16], selected: usize, rows: u16) -> usize {
    if heights.is_empty() {
        return 0;
    }
    let selected = selected.min(heights.len() - 1);
    let mut start = 0;
    loop {
        let used: u16 = heights[start..=selected].iter().sum();
        if used <= rows || start >= selected {
            return start;
        }
        start += 1;
    }
}

/// Drawn in the marker column of the selected row (both lines).
pub(crate) const SELECTED_MARK: &str = "\u{258e}";

/// The empty-cell dot, and the separator between meta items.
pub(crate) const DOT: &str = "\u{b7}";

/// A cell of exactly `cells` display cells.
pub(crate) fn cell(s: &str, cells: usize, style: Style) -> Span<'static> {
    Span::styled(text::fit(s, cells), style)
}

/// A right-aligned cell of exactly `cells` display cells.
pub(crate) fn cell_right(s: &str, cells: usize, style: Style) -> Span<'static> {
    let t = text::truncate(s, cells);
    let pad = cells.saturating_sub(text::width(&t));
    Span::styled(format!("{}{t}", " ".repeat(pad)), style)
}

/// `"12d"`, or `"-"` when unknown.
pub(crate) fn days(d: Option<i64>) -> String {
    d.map_or_else(|| "-".to_string(), |d| format!("{d}d"))
}

/// Price in major units, or `"-"` when there is none.
pub(crate) fn price(minor: Option<i64>) -> String {
    minor.map_or_else(|| "-".to_string(), crate::data::money::major)
}

/// Blank cells.
pub(crate) fn gap(cells: usize) -> Span<'static> {
    Span::raw(" ".repeat(cells))
}

/// The marker column and the space after it.
pub(crate) fn marker(selected: bool, theme: &Theme) -> [Span<'static>; 2] {
    if selected {
        [Span::styled(SELECTED_MARK, theme.accent()), Span::raw(" ")]
    } else {
        [Span::raw(" "), Span::raw(" ")]
    }
}

/// An empty numeric cell: a `dim` dot at the right edge.
pub(crate) fn empty_right(cells: usize, theme: &Theme) -> Span<'static> {
    cell_right(DOT, cells, theme.dim())
}

/// The status word as people read it: `in progress`, not `in_progress`.
pub fn status_word(status: OrderStatus) -> &'static str {
    match status {
        OrderStatus::InProgress => "in progress",
        s => s.as_str(),
    }
}

/// Status chip: glyph (when icons are on) and the status word.
pub(crate) fn status_chip(cx: &RenderCx, status: OrderStatus) -> String {
    cx.icons.label(cx.icons.status(status), status_word(status))
}

/// Colour of a row's status chip (TUI-DESIGN.md section 13): the fixed
/// status colour, except that a paid order whose warranty has ended is
/// `muted`, since amber means "in warranty".
pub(crate) fn status_style(cx: &RenderCx, row: &crate::data::OrderRow) -> Style {
    if row.order.status == OrderStatus::Paid && row.next_action == "archive" {
        cx.theme.muted()
    } else {
        cx.theme.status(row.order.status)
    }
}

/// Days in status: `muted`, bold `unpaid` when a delivered order is
/// overdue; a `dim` dot when unknown.
pub(crate) fn days_cell(row: &crate::data::OrderRow, cells: usize, cx: &RenderCx) -> Span<'static> {
    let t = cx.theme;
    match row.days_in_status {
        None => empty_right(cells, t),
        Some(d) => {
            let style = if row.group == crate::data::Group::Closed {
                t.text().fg(t.archived)
            } else if overdue(row) {
                t.title().fg(t.unpaid)
            } else {
                t.muted()
            };
            cell_right(&format!("{d}d"), cells, style)
        }
    }
}

/// A delivered order waiting `OVERDUE_DAYS` or more.
pub(crate) fn overdue(row: &crate::data::OrderRow) -> bool {
    row.order.status == OrderStatus::Delivered
        && row
            .days_in_status
            .is_some_and(|d| d >= crate::theme::OVERDUE_DAYS)
}

/// A price cell: grouped, right-aligned; a `dim` dot when missing.
pub(crate) fn price_cell(
    minor: Option<i64>,
    cells: usize,
    style: Style,
    theme: &Theme,
) -> Span<'static> {
    match minor {
        Some(p) => cell_right(&text::money(p), cells, style),
        None => empty_right(cells, theme),
    }
}

/// `s` fitted to `cells` with every case-insensitive match of `needle`
/// underlined (filter matches in slugs).
pub(crate) fn highlighted(s: &str, cells: usize, needle: &str, style: Style) -> Vec<Span<'static>> {
    let shown = text::fit(s, cells);
    let needle = needle.trim().to_lowercase();
    if needle.is_empty() || !shown.is_ascii() {
        return vec![Span::styled(shown, style)];
    }
    let lower = shown.to_lowercase();
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = lower[at..].find(&needle) {
        let start = at + i;
        let end = start + needle.len();
        if start > at {
            out.push(Span::styled(shown[at..start].to_string(), style));
        }
        out.push(Span::styled(
            shown[start..end].to_string(),
            style.add_modifier(ratatui::style::Modifier::UNDERLINED),
        ));
        at = end;
    }
    if at < shown.len() {
        out.push(Span::styled(shown[at..].to_string(), style));
    }
    out
}

/// A message at the list origin, the second line with its key in bold.
pub(crate) fn empty(frame: &mut Frame, area: Rect, theme: &Theme, lines: &[&str]) {
    let lines: Vec<Line> = lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            // Later lines start with a key ("N new order"): key bold.
            match l.split_once(' ') {
                Some((k, rest)) if i > 0 => Line::from(vec![
                    Span::raw("  "),
                    Span::styled(k.to_string(), theme.key()),
                    Span::styled(format!(" {rest}"), theme.muted()),
                ]),
                _ => Line::from(Span::styled(format!("  {l}"), theme.muted())),
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// `line` on the selection band, padded to `width`.
pub(crate) fn banded(mut line: Line<'static>, width: u16, theme: &Theme) -> Line<'static> {
    let pad = usize::from(width).saturating_sub(line.width());
    if pad > 0 {
        line.push_span(Span::raw(" ".repeat(pad)));
    }
    line.style(theme.selected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_keeps_selection_visible() {
        let h = [1, 1, 1, 1, 2, 1];
        assert_eq!(window_start(&h, 0, 3), 0);
        assert_eq!(window_start(&h, 2, 3), 0);
        assert_eq!(window_start(&h, 3, 3), 1);
        // The two-line selected row needs both lines.
        assert_eq!(window_start(&h, 4, 3), 3);
        assert_eq!(window_start(&h, 5, 10), 0);
        assert_eq!(window_start(&[], 0, 3), 0);
    }

    #[test]
    fn right_cells() {
        let s = cell_right("800", 6, Style::new());
        assert_eq!(s.content, "   800");
        let s = cell_right("1234567", 4, Style::new());
        assert_eq!(text::width(&s.content), 4);
    }
}
