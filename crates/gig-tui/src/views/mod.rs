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

/// Status chip: glyph (when icons are on) and the gig status word, in the
/// fixed status colour.
pub(crate) fn status_chip(cx: &RenderCx, status: OrderStatus) -> String {
    cx.icons.label(cx.icons.status(status), status.as_str())
}

/// Colour of a row's status chip: the fixed status colour, except that a
/// paid order whose warranty has ended (next action "archive") is no longer
/// amber (spec 3: amber means in warranty).
pub(crate) fn status_style(cx: &RenderCx, row: &crate::data::OrderRow) -> Style {
    if row.order.status == OrderStatus::Paid && row.next_action == "archive" {
        cx.theme.text()
    } else {
        cx.theme.status(row.order.status)
    }
}

/// Cells the status chip column takes: the longest chip.
pub(crate) fn chip_width(cx: &RenderCx) -> usize {
    OrderStatus::ALL
        .iter()
        .map(|s| text::width(&status_chip(cx, *s)))
        .max()
        .unwrap_or(0)
}

/// `"12d"`, or `"-"` when unknown.
pub(crate) fn days(d: Option<i64>) -> String {
    d.map_or_else(|| "-".to_string(), |d| format!("{d}d"))
}

/// Price in major units, or `"-"` when there is none.
pub(crate) fn price(minor: Option<i64>) -> String {
    minor.map_or_else(|| "-".to_string(), crate::data::money::major)
}

/// A one-line message in the middle-left of an empty view.
pub(crate) fn empty(frame: &mut Frame, area: Rect, theme: &Theme, lines: &[&str]) {
    let lines: Vec<Line> = lines
        .iter()
        .map(|l| Line::from(Span::styled(format!(" {l}"), theme.dim())))
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// `line` padded with the selection band to the full `width`.
pub(crate) fn banded(mut line: Line<'static>, width: u16, theme: &Theme) -> Line<'static> {
    let used = line.width();
    let pad = usize::from(width).saturating_sub(used);
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
