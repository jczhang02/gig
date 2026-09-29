//! History view (spec 2.4): every order, archived and cancelled included,
//! newest first, with scorecard score and warranty end. `Enter` opens the
//! detail full screen.

use super::{banded, cell, cell_right, chip_width, empty, price, status_chip, window_start};
use crate::data::{day_part, Group, OrderRow};
use crate::ui::RenderCx;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const DATE: usize = 10;
const SCORE: usize = 5;

pub fn render(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let t = cx.theme;
    let rows = cx.state.order_list();
    let icon = if cx.icons.enabled { 2 } else { 0 };
    let slug = rows
        .iter()
        .map(|r| crate::text::width(&r.order.slug))
        .max()
        .unwrap_or(4)
        .clamp(4, 20);
    let chip = chip_width(cx);
    let price_w = 8;
    // " " icon slug _ chip _ created _ score _ warranty _ price _ title
    let fixed = 1 + icon + slug + 1 + chip + 1 + DATE + 1 + SCORE + 1 + DATE + 1 + price_w + 1;
    let title_w = usize::from(area.width).saturating_sub(fixed);

    let d = t.dim();
    let mut header = vec![
        Span::raw(" ".repeat(1 + icon)),
        cell("slug", slug, d),
        Span::raw(" "),
        cell("status", chip, d),
        Span::raw(" "),
        cell("created", DATE, d),
        Span::raw(" "),
        cell_right("score", SCORE, d),
        Span::raw(" "),
        cell("warranty", DATE, d),
        Span::raw(" "),
        cell_right("price", price_w, d),
    ];
    if title_w > 0 {
        header.push(Span::raw(" "));
        header.push(cell("title", title_w, d));
    }
    let mut lines = vec![Line::from(header)];
    if rows.is_empty() {
        frame.render_widget(Paragraph::new(lines), area);
        let body = Rect {
            y: area.y + 1,
            height: area.height.saturating_sub(1),
            ..area
        };
        let msg = if cx.state.filter().text.is_empty() {
            "no orders yet"
        } else {
            "no order matches the filter"
        };
        empty(frame, body, t, &[msg]);
        return;
    }
    let selected_id = cx.state.selected_order().map(|r| r.order.id);
    let selected = rows
        .iter()
        .position(|r| Some(r.order.id) == selected_id)
        .unwrap_or(0);
    let heights = vec![1; rows.len()];
    let start = window_start(&heights, selected, area.height.saturating_sub(1));
    for (i, r) in rows.iter().enumerate().skip(start) {
        let line = row_line(cx, r, icon, slug, chip, price_w, title_w);
        lines.push(if i == selected {
            banded(line, area.width, t)
        } else {
            line
        });
        if lines.len() >= usize::from(area.height) {
            break;
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn row_line(
    cx: &RenderCx,
    r: &OrderRow,
    icon: usize,
    slug: usize,
    chip: usize,
    price_w: usize,
    title_w: usize,
) -> Line<'static> {
    let t = cx.theme;
    let o = &r.order;
    let text = if r.group == Group::Closed {
        t.dim()
    } else {
        t.text()
    };
    let score = r
        .scorecard
        .as_ref()
        .and_then(|s| s.score)
        .map_or("-".to_string(), |s| format!("{s}/5"));
    let mut spans = vec![Span::raw(" ")];
    if icon > 0 {
        spans.push(cell(cx.icons.project_type(o.project_type), icon, t.dim()));
    }
    spans.extend([
        cell(&o.slug, slug, text.add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        cell(&status_chip(cx, o.status), chip, t.status(o.status)),
        Span::raw(" "),
        cell(day_part(&o.created_at).unwrap_or("-"), DATE, t.dim()),
        Span::raw(" "),
        cell_right(&score, SCORE, text),
        Span::raw(" "),
        cell(o.warranty_until.as_deref().unwrap_or("-"), DATE, t.dim()),
        Span::raw(" "),
        cell_right(&price(o.price_minor), price_w, text),
    ]);
    if title_w > 0 {
        spans.push(Span::raw(" "));
        spans.push(cell(&o.title, title_w, text));
    }
    Line::from(spans)
}
