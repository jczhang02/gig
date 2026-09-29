//! Orders view (spec 2.1): the active orders, one line each, the selected
//! one expanded to two lines with the latest JOB.md status entry.

use super::{banded, cell, cell_right, chip_width, days, empty, price, status_chip, window_start};
use crate::data::{Group, OrderRow};
use crate::text;
use crate::ui::RenderCx;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// Column widths of one row, in display cells. `title` is 0 when the
/// terminal is too narrow for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Columns {
    pub icon: usize,
    pub slug: usize,
    pub chip: usize,
    pub next: usize,
    pub days: usize,
    pub price: usize,
    pub title: usize,
}

/// Cells between columns.
const GAP: usize = 1;
/// Narrowest title worth showing.
const MIN_TITLE: usize = 6;

impl Columns {
    pub fn fit(width: usize, cx: &RenderCx, rows: &[&OrderRow]) -> Self {
        let icon = if cx.icons.enabled { 2 } else { 0 };
        let longest = rows
            .iter()
            .map(|r| text::width(&r.order.slug))
            .max()
            .unwrap_or(4);
        let slug = longest.clamp(4, 20);
        let chip = chip_width(cx);
        let days = 4;
        let price = rows
            .iter()
            .map(|r| text::width(&price(r.order.price_minor)))
            .max()
            .unwrap_or(1)
            .clamp(5, 10);
        // Leading space, then gaps before chip, next, days, price, title.
        let fixed = 1 + icon + slug + chip + days + price + 5 * GAP;
        let rest = width.saturating_sub(fixed);
        let mut next = (rest * 45 / 100).clamp(8.min(rest), 24);
        let mut title = rest.saturating_sub(next);
        if title < MIN_TITLE {
            next = rest;
            title = 0;
        }
        Self {
            icon,
            slug,
            chip,
            next,
            days,
            price,
            title,
        }
    }

    /// Cells before the slug column (the second line starts there).
    pub fn indent(&self) -> usize {
        1 + self.icon
    }
}

pub fn render(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let t = cx.theme;
    let rows = cx.state.order_list();
    let cols = Columns::fit(usize::from(area.width), cx, &rows);
    let mut lines = vec![header(cx, &cols)];
    if rows.is_empty() {
        let filter = &cx.state.filter().text;
        let msg: &[&str] = if !filter.is_empty() {
            &["no order matches the filter", "Esc clears it"]
        } else if cx.state.show_closed {
            &["no orders yet", "N registers one"]
        } else {
            &["no active orders", "N registers one, a shows archived ones"]
        };
        frame.render_widget(Paragraph::new(lines), area);
        let body = Rect {
            y: area.y + 1,
            height: area.height.saturating_sub(1),
            ..area
        };
        empty(frame, body, t, msg);
        return;
    }
    let selected_id = cx.state.selected_order().map(|r| r.order.id);
    let selected = rows
        .iter()
        .position(|r| Some(r.order.id) == selected_id)
        .unwrap_or(0);
    let heights: Vec<u16> = (0..rows.len())
        .map(|i| if i == selected { 2 } else { 1 })
        .collect();
    let start = window_start(&heights, selected, area.height.saturating_sub(1));
    for (i, r) in rows.iter().enumerate().skip(start) {
        let line = row_line(cx, &cols, r);
        if i == selected {
            lines.push(banded(line, area.width, t));
            lines.push(banded(status_line(cx, &cols, r, area.width), area.width, t));
        } else {
            lines.push(line);
        }
        if lines.len() >= usize::from(area.height) {
            break;
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn header(cx: &RenderCx, c: &Columns) -> Line<'static> {
    let d = cx.theme.dim();
    let mut spans = vec![
        Span::raw(" ".repeat(c.indent())),
        cell("slug", c.slug, d),
        Span::raw(" "),
        cell("status", c.chip, d),
        Span::raw(" "),
        cell("next", c.next, d),
        Span::raw(" "),
        cell_right("days", c.days, d),
        Span::raw(" "),
        cell_right("price", c.price, d),
    ];
    if c.title > 0 {
        let title = if cx.state.show_closed {
            "title (+ archived, cancelled)"
        } else {
            "title"
        };
        spans.push(Span::raw(" "));
        spans.push(cell(title, c.title, d));
    }
    Line::from(spans)
}

fn row_line(cx: &RenderCx, c: &Columns, r: &OrderRow) -> Line<'static> {
    let t = cx.theme;
    let o = &r.order;
    let closed = r.group == Group::Closed;
    let text = if closed { t.dim() } else { t.text() };
    let mut spans = vec![Span::raw(" ")];
    if c.icon > 0 {
        spans.push(cell(cx.icons.project_type(o.project_type), c.icon, t.dim()));
    }
    spans.extend([
        cell(&o.slug, c.slug, text.add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        cell(&status_chip(cx, o.status), c.chip, t.status(o.status)),
        Span::raw(" "),
        cell(&r.next_action, c.next, text),
        Span::raw(" "),
        cell_right(&days(r.days_in_status), c.days, t.dim()),
        Span::raw(" "),
        cell_right(&price(o.price_minor), c.price, text),
    ]);
    if c.title > 0 {
        spans.push(Span::raw(" "));
        spans.push(cell(&o.title, c.title, text));
    }
    Line::from(spans)
}

/// Second line of the selected row: the newest JOB.md status entry.
fn status_line(cx: &RenderCx, c: &Columns, r: &OrderRow, width: u16) -> Line<'static> {
    let t = cx.theme;
    let indent = c.indent();
    let room = usize::from(width).saturating_sub(indent + 1);
    let (text_, style) = match r.job.latest_status() {
        Some(s) => (s.to_string(), t.dim().add_modifier(Modifier::ITALIC)),
        None if !r.job.found => ("(no JOB.md)".to_string(), t.dim()),
        None => ("(no status entry in JOB.md)".to_string(), t.dim()),
    };
    Line::from(vec![
        Span::raw(" ".repeat(indent)),
        Span::styled(text::truncate(&text_, room), style),
    ])
}
