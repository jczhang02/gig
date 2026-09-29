//! Orders view (TUI-DESIGN.md section 8): the active orders under group
//! headings, one line each, the selected one on the selection band with a
//! second line.

use super::{
    banded, cell, cell_right, days_cell, empty, gap, highlighted, marker, price_cell, status_chip,
    status_style, window_start, DOT,
};
use crate::data::{day_part, Group, OrderRow};
use crate::text;
use crate::ui::{RenderCx, WidthClass, MIN_TITLE};
use gig_core::models::OrderStatus;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// Cells of the fixed columns (section 6.3).
pub const SLUG: usize = 22;
pub const STATUS: usize = 13;
pub const NEXT: usize = 15;
pub const DAYS: usize = 4;
pub const PRICE: usize = 6;
/// Between columns.
pub const GAP: usize = 2;

/// Column layout of one list width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Columns {
    /// Glyph plus its space: 2, or 0 with `--no-icons`.
    pub icon: usize,
    /// The `next` column (Wide only).
    pub next: bool,
    /// 0 when the title column is dropped.
    pub title: usize,
}

impl Columns {
    pub fn fit(width: usize, class: WidthClass, icons: bool) -> Self {
        let icon = if icons { 2 } else { 0 };
        let next = class == WidthClass::Wide;
        let mut c = Self {
            icon,
            next,
            title: 0,
        };
        let title = width.saturating_sub(c.fixed() + GAP);
        c.title = if title >= MIN_TITLE { title } else { 0 };
        c
    }

    /// Cells before the slug column: marker, space, glyph, space.
    pub fn indent(&self) -> usize {
        2 + self.icon
    }

    /// Cells up to the end of the price column.
    pub fn fixed(&self) -> usize {
        let next = if self.next { NEXT + GAP } else { 0 };
        self.indent() + SLUG + GAP + STATUS + GAP + next + DAYS + GAP + PRICE
    }
}

/// One screen row of the list.
#[derive(Debug, Clone, Copy)]
enum Item<'a> {
    Blank,
    Heading(Heading),
    Row(&'a OrderRow),
    /// The second line of the selected row.
    Second(&'a OrderRow),
}

/// A group heading: its word, and the rows it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Heading {
    group: Group,
    cancelled: bool,
}

impl Heading {
    fn of(r: &OrderRow) -> Self {
        Self {
            group: r.group,
            cancelled: r.order.status == OrderStatus::Cancelled,
        }
    }

    fn word(&self) -> &'static str {
        match (self.group, self.cancelled) {
            (Group::Unpaid, _) => "owed",
            (Group::Warranty, _) => "paid",
            (Group::InProgress, _) => "in progress",
            (Group::Queued, _) => "queued",
            (Group::Closed, false) => "archived",
            (Group::Closed, true) => "cancelled",
        }
    }
}

pub fn render(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let t = cx.theme;
    let rows = cx.state.order_list();
    let class = WidthClass::of(frame.area().width);
    let cols = Columns::fit(usize::from(area.width), class, cx.icons.enabled);
    let header = header(cx, &cols);
    if rows.is_empty() {
        frame.render_widget(Paragraph::new(header), area);
        let filter = &cx.state.filter().text;
        let msg: &[&str] = if !filter.is_empty() {
            &["no order matches the filter", "Esc clears it"]
        } else if cx.state.show_closed {
            &["no orders yet", "N new order"]
        } else {
            &["nothing needs you", "N new order"]
        };
        let body = Rect {
            y: area.y + 2,
            height: area.height.saturating_sub(2),
            ..area
        };
        empty(frame, body, t, msg);
        return;
    }
    let selected_id = cx.state.selected_order().map(|r| r.order.id);
    let mut items = Vec::new();
    let mut last: Option<Heading> = None;
    let mut target = 0;
    for r in &rows {
        let h = Heading::of(r);
        if last != Some(h) {
            items.push(Item::Blank);
            items.push(Item::Heading(h));
            last = Some(h);
        }
        items.push(Item::Row(r));
        target = if Some(r.order.id) == selected_id {
            if has_second_line(r, &cols) {
                items.push(Item::Second(r));
            }
            items.len() - 1
        } else {
            target
        };
    }
    let body_rows = area.height.saturating_sub(1);
    let heights = vec![1; items.len()];
    let mut start = window_start(&heights, target, body_rows);
    let mut lines = vec![header];
    // Sticky heading: a scrolled list names the group of its first row.
    if matches!(items.get(start), Some(Item::Row(_) | Item::Second(_))) {
        start = window_start(&heights, target, body_rows.saturating_sub(1));
        if let Some(Item::Row(r) | Item::Second(r)) = items.get(start) {
            lines.push(heading_line(cx, &cols, Heading::of(r), &rows));
        }
    }
    let width = area.width;
    for item in items.iter().skip(start) {
        if lines.len() >= usize::from(area.height) {
            break;
        }
        lines.push(match item {
            Item::Blank => Line::raw(""),
            Item::Heading(h) => heading_line(cx, &cols, *h, &rows),
            Item::Row(r) if Some(r.order.id) == selected_id => {
                banded(row_line(cx, &cols, r, true), width, t)
            }
            Item::Row(r) => row_line(cx, &cols, r, false),
            Item::Second(r) => banded(second_line(cx, &cols, r, width), width, t),
        });
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// Row 2: the column names, aligned with the columns, all `muted`.
fn header(cx: &RenderCx, c: &Columns) -> Line<'static> {
    let m = cx.theme.muted();
    let mut spans = vec![
        gap(c.indent()),
        cell("order", SLUG, m),
        gap(GAP),
        cell("status", STATUS, m),
        gap(GAP),
    ];
    if c.next {
        spans.push(cell("next", NEXT, m));
        spans.push(gap(GAP));
    }
    spans.extend([
        cell_right("days", DAYS, m),
        gap(GAP),
        cell_right(cx.state.data.currency(), PRICE, m),
    ]);
    if c.title > 0 {
        spans.push(gap(GAP));
        spans.push(Span::styled("title", m));
    }
    Line::from(spans)
}

/// `  owed  2` with the group's money total right-aligned in the price
/// column, all `muted`.
fn heading_line(cx: &RenderCx, c: &Columns, h: Heading, rows: &[&OrderRow]) -> Line<'static> {
    let m = cx.theme.muted();
    let members: Vec<&&OrderRow> = rows.iter().filter(|r| Heading::of(r) == h).collect();
    let total: i64 = members.iter().filter_map(|r| r.order.price_minor).sum();
    let lead = format!("  {}  {}", h.word(), members.len());
    let total = text::money(total);
    let pad = c
        .fixed()
        .saturating_sub(text::width(&lead) + text::width(&total));
    Line::from(vec![
        Span::styled(lead, m),
        gap(pad),
        Span::styled(total, m),
    ])
}

/// `warranty until 2026-10-05` shortened to `until 10-05` for the column.
fn next_short(next: &str) -> String {
    match next.strip_prefix("warranty until ") {
        Some(d) => format!("until {}", d.get(5..).unwrap_or(d)),
        None => next.to_string(),
    }
}

fn row_line(cx: &RenderCx, c: &Columns, r: &OrderRow, selected: bool) -> Line<'static> {
    let t = cx.theme;
    let o = &r.order;
    let closed = r.group == Group::Closed;
    let ink = if closed {
        t.text().fg(t.archived)
    } else {
        t.text()
    };
    let icon_style = if closed { ink } else { t.muted() };
    let mut spans: Vec<Span<'static>> = marker(selected, t).into();
    if c.icon > 0 {
        spans.push(Span::styled(
            cx.icons.project_type(o.project_type),
            icon_style,
        ));
        spans.push(Span::raw(" "));
    }
    let slug_style = if selected {
        ink.add_modifier(Modifier::BOLD)
    } else {
        ink
    };
    spans.extend(highlighted(
        &o.slug,
        SLUG,
        &cx.state.filter().text,
        slug_style,
    ));
    spans.push(gap(GAP));
    let chip = if closed { ink } else { status_style(cx, r) };
    spans.push(cell(&status_chip(cx, o.status), STATUS, chip));
    spans.push(gap(GAP));
    if c.next {
        // Closed orders have no next step: an empty dim `·`, not `none`.
        if closed {
            spans.push(cell(DOT, NEXT, t.dim()));
        } else {
            spans.push(cell(&next_short(&r.next_action), NEXT, ink));
        }
        spans.push(gap(GAP));
    }
    spans.push(days_cell(r, DAYS, cx));
    spans.push(gap(GAP));
    spans.push(price_cell(o.price_minor, PRICE, ink, t));
    if c.title > 0 {
        spans.push(gap(GAP));
        spans.push(cell(&o.title, c.title, ink));
    }
    Line::from(spans)
}

/// The newest sent package: its sent date and link.
fn last_sent(r: &OrderRow) -> Option<(&str, Option<&str>)> {
    r.packages
        .iter()
        .filter_map(|p| {
            let at = p.sent_at.as_deref()?;
            let link = p.short_url.as_deref().or(p.remote_url.as_deref());
            Some((at, link))
        })
        .max_by(|a, b| a.0.cmp(b.0))
}

/// `2026-09-20: text` split into `MM-DD` and the text.
pub(crate) fn split_dated(entry: &str) -> (Option<&str>, &str) {
    let day = day_part(entry);
    match day {
        Some(d) => {
            let rest = entry[10..].trim_start_matches([':', ' ']).trim_start();
            (d.get(5..10), rest)
        }
        None => (None, entry),
    }
}

/// What the second line says besides the title (section 8.3).
fn second_items(cx: &RenderCx, r: &OrderRow) -> Vec<Span<'static>> {
    let t = cx.theme;
    if let Some(entry) = r.job.latest_status() {
        let (day, rest) = split_dated(entry);
        let mut out = Vec::new();
        if let Some(d) = day {
            out.push(Span::styled(format!("{d}  "), t.muted()));
        }
        out.push(Span::styled(text::flatten(rest), t.muted()));
        return out;
    }
    if r.group == Group::Closed {
        return Vec::new();
    }
    let mut out = vec![Span::styled(r.next_action.clone(), t.muted())];
    if let Some((at, link)) = last_sent(r) {
        let day = day_part(at).and_then(|d| d.get(5..)).unwrap_or("");
        out.push(Span::raw("  "));
        out.push(Span::styled(DOT, t.dim()));
        out.push(Span::styled(format!("  sent {day}"), t.muted()));
        if let Some(link) = link {
            out.push(Span::raw("  "));
            out.push(Span::styled(text::strip_scheme(link).to_string(), t.link()));
        }
    }
    out
}

fn has_second_line(r: &OrderRow, c: &Columns) -> bool {
    c.title == 0 || r.job.latest_status().is_some() || r.group != Group::Closed
}

/// The selected row's second line, indented to the slug column and cut
/// at the list width.
fn second_line(cx: &RenderCx, c: &Columns, r: &OrderRow, width: u16) -> Line<'static> {
    let t = cx.theme;
    let mut spans: Vec<Span<'static>> = marker(true, t).into();
    spans.push(gap(c.icon));
    let mut body = Vec::new();
    let items = second_items(cx, r);
    if c.title == 0 {
        body.push(Span::styled(text::flatten(&r.order.title), t.text()));
        if !items.is_empty() {
            body.push(Span::raw("  "));
            body.push(Span::styled(DOT, t.dim()));
            body.push(Span::raw("  "));
        }
    }
    body.extend(items);
    let room = usize::from(width).saturating_sub(c.indent() + 1);
    spans.extend(clip(body, room));
    Line::from(spans)
}

/// Spans cut to `room` cells in total, the last one with an ellipsis.
pub(crate) fn clip(spans: Vec<Span<'static>>, room: usize) -> Vec<Span<'static>> {
    let mut out = Vec::new();
    let mut used = 0;
    for s in spans {
        let w = text::width(&s.content);
        if used + w <= room {
            used += w;
            out.push(s);
        } else {
            let left = room - used;
            if left > 0 {
                out.push(Span::styled(text::truncate(&s.content, left), s.style));
            }
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::UiState;
    use crate::data::tests::sample_snapshot;
    use crate::icons::Icons;
    use crate::theme::Theme;

    #[test]
    fn column_budget_matches_the_design() {
        let icons = |w, class| Columns::fit(w, class, true);
        // 6.3: 80 columns (list 78) leave 21 for the title.
        assert_eq!(icons(78, WidthClass::Narrow).title, 21);
        assert_eq!(Columns::fit(78, WidthClass::Narrow, false).title, 23);
        assert_eq!(icons(76, WidthClass::Medium).title, 19);
        assert_eq!(icons(55, WidthClass::Medium).title, 0);
        let wide = icons(114, WidthClass::Wide);
        assert!(wide.next);
        assert_eq!(wide.title, 40);
        assert_eq!(icons(78, WidthClass::Narrow).fixed(), 55);
        assert_eq!(wide.fixed(), 72);
    }

    #[test]
    fn rows_never_exceed_the_width() {
        let mut data = sample_snapshot("2026-09-29");
        data.orders[0].order.title = "图像去噪与超分辨率批处理工具开发".repeat(4);
        data.orders[3].order.slug = "a-rather-long-slug-for-a-row".into();
        let state = UiState {
            data,
            ..UiState::default()
        };
        for icons in [true, false] {
            let icons = Icons::new(icons);
            let cx = RenderCx {
                state: &state,
                theme: &Theme::DARK,
                icons: &icons,
            };
            let rows = state.order_list();
            for width in 55..=250usize {
                for class in [WidthClass::Narrow, WidthClass::Medium, WidthClass::Wide] {
                    let cols = Columns::fit(width, class, icons.enabled);
                    if cols.fixed() > width {
                        continue;
                    }
                    for r in &rows {
                        let w = row_line(&cx, &cols, r, true).width();
                        assert!(w <= width, "{w} > {width}");
                        let s = second_line(&cx, &cols, r, width as u16).width();
                        assert!(s <= width, "second line {s} > {width}");
                    }
                    assert!(header(&cx, &cols).width() <= width);
                }
            }
        }
    }

    #[test]
    fn dated_entries_split() {
        assert_eq!(
            split_dated("2026-09-20: 预览已发送"),
            (Some("09-20"), "预览已发送")
        );
        assert_eq!(split_dated("no date"), (None, "no date"));
        assert_eq!(next_short("warranty until 2026-10-05"), "until 10-05");
        assert_eq!(next_short("collect payment"), "collect payment");
    }
}
