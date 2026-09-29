//! Money view (TUI-DESIGN.md section 11): three stat tiles, received per
//! month for the last 12 months as unboxed bars, and the outstanding
//! table.

use super::orders::{GAP, PRICE, SLUG};
use super::{banded, cell, cell_right, empty_right, gap, marker, price_cell, DOT};
use crate::data::money::{Amount, Month};
use crate::data::{day_part, Group};
use crate::mouse::Target;
use crate::text;
use crate::theme::OVERDUE_DAYS;
use crate::ui::{RenderCx, WidthClass, MIN_TITLE};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Days column of the outstanding table.
const SINCE: usize = 5;

/// `(year, month index 0..12)` of a `YYYY-MM` label.
fn year_month(label: &str) -> Option<(&str, usize)> {
    let y = label.get(..4)?;
    let m = label.get(5..7)?.parse::<usize>().ok()?;
    (1..=12).contains(&m).then_some((y, m - 1))
}

/// Rows of bar in the chart at terminal height `height`.
pub fn plot_height(height: u16) -> u16 {
    match height {
        34.. => 10,
        24..=33 => 6,
        _ => 4,
    }
}

/// Slot and bar widths for a chart `width` cells wide at terminal width
/// `term_width` (section 11.2).
pub fn geometry(width: u16, term_width: u16) -> (u16, u16) {
    let s = width / 12;
    let b = match term_width {
        100.. => 5.min(s.saturating_sub(2)),
        80..=99 => 3.min(s.saturating_sub(2)),
        _ => 2.min(s.saturating_sub(1)),
    };
    (s, b.max(1))
}

pub fn render(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let term = frame.area();
    let class = WidthClass::of(term.width);
    // Tiles, gap, heading, gap: 6 rows. The chart needs `h + 4` rows; in a
    // short body its plot shrinks, and below one plot row the chart and its
    // heading are left out rather than drawn empty.
    let spare = area.height.saturating_sub(6 + 4);
    let h = plot_height(term.height).min(spare);
    // Top to bottom; a block that does not fit is left out.
    let mut y = area.y;
    let mut take = |rows: u16| -> Option<Rect> {
        let r = Rect::new(area.x, y, area.width, rows);
        y = y.saturating_add(rows);
        (y <= area.bottom()).then_some(r)
    };
    let tiles_r = take(3);
    let (heading_r, chart_r) = if h > 0 {
        take(1);
        let heading = take(1);
        take(1);
        (heading, take(h + 4))
    } else {
        (None, None)
    };
    take(1);
    // A clicked month lists its payments between the chart and the
    // outstanding table (section 11.4): as many rows as fit while the table
    // keeps its heading, header and one row.
    let drill = cx
        .state
        .money_month
        .filter(|_| chart_r.is_some())
        .and_then(|i| cx.state.data.money.by_month.get(i));
    let drill_r = drill.and_then(|m| {
        let want = 2 + m.order_ids.len().max(1) as u16;
        let room = take(0).map_or(0, |r| area.bottom().saturating_sub(r.y));
        let keep = 1 + 3;
        let rows = want.min(room.saturating_sub(keep).max(3)).min(room);
        let r = take(rows);
        take(1);
        r
    });
    let table_r = take(0).map(|r| Rect {
        height: area.bottom().saturating_sub(r.y),
        ..r
    });
    if let Some(r) = tiles_r {
        tiles(frame, r, cx, class);
    }
    if let Some(r) = heading_r {
        chart_heading(frame, r, cx);
    }
    if let Some(r) = chart_r {
        chart(frame, r, cx, h, term.width);
    }
    if let (Some(r), Some(m)) = (drill_r, drill) {
        month_orders(frame, r, cx, class, m);
    }
    if let Some(r) = table_r {
        outstanding(frame, r, cx, class);
    }
}

/// `label` for the three tiles: context line of each.
fn tiles(frame: &mut Frame, area: Rect, cx: &RenderCx, class: WidthClass) {
    let t = cx.theme;
    let m = &cx.state.data.money;
    let today = &cx.state.data.today;
    let currency = cx.state.data.currency();
    let month = today
        .get(5..7)
        .and_then(|m| m.parse::<usize>().ok())
        .and_then(|m| MONTH_NAMES.get(m.wrapping_sub(1)))
        .copied()
        .unwrap_or("this month");
    let year = today.get(..4).unwrap_or("this year");
    let plural =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let paid_months = m
        .by_month
        .iter()
        .filter(|mo| mo.label.get(..4) == Some(year) && mo.amount.gross > 0)
        .count();
    let tiles: [(String, Amount, bool, Option<String>); 3] = [
        (
            "outstanding".into(),
            m.outstanding,
            true,
            Some(plural(m.owed.len(), "order", "orders")),
        ),
        (format!("received in {month}"), m.month, false, None),
        (
            format!("received in {year}"),
            m.year,
            false,
            (paid_months > 0).then(|| plural(paid_months, "month", "months")),
        ),
    ];
    let value_style = |amount: &Amount, alarm: bool| {
        if alarm && amount.gross > 0 {
            t.title().fg(t.unpaid)
        } else {
            t.title()
        }
    };
    let context = |a: &Amount, extra: &Option<String>| {
        let mut s = format!("take-home {}", text::money(a.take_home));
        if let Some(e) = extra {
            s.push_str(&format!(" {DOT} {e}"));
        }
        s
    };
    if class == WidthClass::Narrow {
        let label_w = tiles
            .iter()
            .map(|(l, ..)| text::width(l))
            .max()
            .unwrap_or(0);
        let value_w = tiles
            .iter()
            .map(|(_, a, ..)| text::width(&text::money(a.gross)) + 1 + currency.len())
            .max()
            .unwrap_or(0);
        let lines: Vec<Line> = tiles
            .iter()
            .map(|(label, a, alarm, extra)| {
                let value = text::money(a.gross);
                let pad = value_w.saturating_sub(text::width(&value) + 1 + currency.len());
                // The context line ends in `…` when it does not fit.
                let room = usize::from(area.width).saturating_sub(label_w + 2 + value_w + 2);
                Line::from(vec![
                    Span::styled(text::fit(label, label_w), t.muted()),
                    Span::raw("  "),
                    Span::styled(value, value_style(a, *alarm)),
                    Span::styled(format!(" {currency}"), t.muted()),
                    gap(pad + 2),
                    Span::styled(text::truncate(&context(a, extra), room), t.muted()),
                ])
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), area);
        return;
    }
    let w = area.width / 3;
    for (i, (label, a, alarm, extra)) in tiles.iter().enumerate() {
        let x = area.x + w * i as u16;
        let width = if i == 2 { area.right() - x } else { w };
        let fit = |s: &str| text::truncate(s, usize::from(width.saturating_sub(1)));
        let lines = vec![
            Line::from(Span::styled(fit(label), t.muted())),
            Line::from(vec![
                Span::styled(text::money(a.gross), value_style(a, *alarm)),
                Span::styled(format!(" {currency}"), t.muted()),
            ]),
            Line::from(Span::styled(fit(&context(a, extra)), t.muted())),
        ];
        frame.render_widget(
            Paragraph::new(lines),
            Rect::new(x, area.y, width.saturating_sub(1), area.height),
        );
    }
}

fn chart_heading(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let t = cx.theme;
    let months = &cx.state.data.money.by_month;
    let name = |mo: &Month| {
        year_month(&mo.label).map_or(mo.label.clone(), |(y, m)| format!("{} {y}", MONTHS[m]))
    };
    let span = match (months.first(), months.last()) {
        (Some(a), Some(b)) => format!("{} - {}", name(a), name(b)),
        _ => String::new(),
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("Received per month", t.title()))),
        area,
    );
    frame.render_widget(
        Paragraph::new(
            Line::from(vec![
                Span::styled(span, t.muted()),
                Span::raw("  "),
                Span::styled(DOT, t.dim()),
                Span::raw("  "),
                Span::styled(cx.state.data.currency().to_string(), t.muted()),
            ])
            .right_aligned(),
        ),
        area,
    );
}

/// Eighths of a cell: ` ▁▂▃▄▅▆▇█`.
const EIGHTHS: [&str; 9] = [
    " ", "\u{2581}", "\u{2582}", "\u{2583}", "\u{2584}", "\u{2585}", "\u{2586}", "\u{2587}",
    "\u{2588}",
];

/// The chart: a value-label row, `h` plot rows, the baseline, the month
/// row and the year row (`h + 4` rows).
fn chart(frame: &mut Frame, area: Rect, cx: &RenderCx, h: u16, term_width: u16) {
    let t = cx.theme;
    let months = &cx.state.data.money.by_month;
    let (s, b) = geometry(area.width, term_width);
    if months.is_empty() || s == 0 {
        return;
    }
    let off = (s - b) / 2;
    let whole = |g: i64| (g.max(0) + 50) / 100;
    let max = months
        .iter()
        .map(|m| whole(m.amount.gross))
        .max()
        .unwrap_or(0);
    let last = months.len() - 1;
    let buf = frame.buffer_mut();
    let base_y = area.y + 1 + h;
    let put = |buf: &mut ratatui::buffer::Buffer, x: u16, y: u16, sym: &str, style: Style| {
        if x < area.right() && y < area.bottom() {
            buf[(x, y)].set_symbol(sym).set_style(style);
        }
    };
    let set_str = |buf: &mut ratatui::buffer::Buffer, x: u16, y: u16, s: &str, style: Style| {
        if y < area.bottom() && x < area.right() {
            buf.set_stringn(x, y, s, usize::from(area.right() - x), style);
        }
    };
    // Baseline across the 12 slots.
    for i in 0..(12 * s) {
        put(
            buf,
            area.x + i,
            base_y,
            "\u{2500}",
            Style::new().fg(t.border),
        );
    }
    let picked = cx.state.money_month;
    for (i, mo) in months.iter().enumerate() {
        let now = i == last;
        let x0 = area.x + i as u16 * s;
        let bx = x0 + off;
        // Mouse: the whole slot (label, bar, month and year) picks the
        // month for the drill-down.
        cx.state
            .hits
            .add(Rect::new(x0, area.y, s, area.height), Target::Month(i));
        let value = whole(mo.amount.gross);
        if value == 0 {
            let style = Style::new().fg(if now { t.bar_now } else { t.dim });
            for dx in 0..b {
                put(buf, bx + dx, base_y, "\u{2508}", style);
            }
        } else {
            let color = if now { t.bar_now } else { t.bar };
            let style = Style::new().fg(color);
            let mut e = ((value as f64 / max as f64) * f64::from(h) * 8.0).round() as u16;
            e = e.max(1);
            // NO_COLOR: past bars are textured whole rows.
            let texture = t.no_color() && !now;
            if texture {
                e = e.div_ceil(8) * 8;
            }
            let full = e / 8;
            let cap = e % 8;
            for row in 0..full {
                let y = base_y - 1 - row;
                for dx in 0..b {
                    put(
                        buf,
                        bx + dx,
                        y,
                        if texture { "\u{2592}" } else { EIGHTHS[8] },
                        style,
                    );
                }
            }
            let mut top = base_y - full;
            if cap > 0 {
                top -= 1;
                for dx in 0..b {
                    put(buf, bx + dx, top, EIGHTHS[usize::from(cap)], style);
                }
            }
            // Value label on the row above the bar's top cell.
            let label = text::compact(value);
            let lw = text::width(&label) as u16;
            if lw < s && top > area.y {
                let lx = centred(bx, b, lw).clamp(x0, x0 + s - lw);
                let style = if now { t.title() } else { t.text() };
                set_str(buf, lx, top - 1, &label, style);
            }
        }
        // Month and year rows.
        if let Some((year, m)) = year_month(&mo.label) {
            let name = MONTHS[m];
            let lx = centred(bx, b, 3).clamp(x0, x0 + s.saturating_sub(3));
            let style = if picked == Some(i) {
                // The picked month reads like the active tab: bold, with an
                // accent underline (section 11.4).
                let style = t.title().add_modifier(Modifier::UNDERLINED);
                if t.no_color() {
                    style
                } else {
                    style.underline_color(t.accent)
                }
            } else if now {
                t.title()
            } else {
                t.muted()
            };
            set_str(buf, lx, base_y + 1, name, style);
            if i == 0 || m == 0 {
                set_str(buf, lx, base_y + 2, year, t.muted());
            }
        }
    }
    if max == 0 {
        for i in 0..(12 * s) {
            put(buf, area.x + i, base_y, "\u{2508}", Style::new().fg(t.dim));
        }
        // The current month keeps its `bar_now` footprint (section 11.2).
        let bx = area.x + last as u16 * s + off;
        for dx in 0..b {
            put(buf, bx + dx, base_y, "\u{2508}", Style::new().fg(t.bar_now));
        }
        let msg = "no payments in the last 12 months";
        let mx = area.x + (12 * s).saturating_sub(msg.len() as u16) / 2;
        set_str(buf, mx, area.y + 1 + h / 2, msg, t.muted());
    }
}

/// Left edge of a `w`-cell label centred on the bar at `bx`, `b` wide.
fn centred(bx: u16, b: u16, w: u16) -> u16 {
    ((2 * bx + b).saturating_sub(w)) / 2
}

/// The outstanding table (section 11.3).
fn outstanding(frame: &mut Frame, area: Rect, cx: &RenderCx, class: WidthClass) {
    let t = cx.theme;
    let owed = &cx.state.data.money.owed;
    let icon = if cx.icons.enabled { 2 } else { 0 };
    let price_end = 2 + icon + SLUG + GAP + PRICE;
    let total: i64 = owed.iter().filter_map(|o| o.price_minor).sum();
    let lead = format!("Outstanding  {}", owed.len());
    let total_s = text::money(total);
    let mut lines = vec![Line::from(vec![
        Span::styled("Outstanding", t.title()),
        Span::styled(format!("  {}", owed.len()), t.muted()),
        gap(price_end.saturating_sub(text::width(&lead) + text::width(&total_s))),
        Span::styled(total_s, t.muted()),
    ])];
    if owed.is_empty() {
        lines.push(Line::from(Span::styled("nothing outstanding", t.muted())));
        frame.render_widget(Paragraph::new(lines), area);
        return;
    }
    let fixed = price_end + GAP + SINCE;
    let title_w = usize::from(area.width).saturating_sub(fixed + GAP);
    let title_w = if class == WidthClass::Narrow || title_w < MIN_TITLE {
        0
    } else {
        title_w
    };
    let m = t.muted();
    let mut head = vec![
        gap(2 + icon),
        cell("order", SLUG, m),
        gap(GAP),
        cell_right(cx.state.data.currency(), PRICE, m),
        gap(GAP),
        cell_right("since", SINCE, m),
    ];
    if title_w > 0 {
        head.push(gap(GAP));
        head.push(Span::styled("title", m));
    }
    lines.push(Line::from(head));
    let room = usize::from(area.height).saturating_sub(lines.len());
    let selected = cx.state.selected_owed();
    let at = owed
        .iter()
        .position(|o| Some(o.order_id) == selected)
        .unwrap_or(0);
    // When the table does not fit, the last row says how many are left.
    let fits = owed.len() <= room;
    let shown = if fits {
        owed.len()
    } else {
        room.saturating_sub(1)
    };
    let start = if at >= shown { at + 1 - shown } else { 0 };
    for (i, o) in owed.iter().enumerate().skip(start).take(shown) {
        // Mouse: a click jumps to the order in Orders.
        cx.state.hits.add(
            Rect::new(area.x, area.y + lines.len() as u16, area.width, 1),
            Target::Owed(o.order_id),
        );
        let row = cx.state.data.order(o.order_id);
        let is_sel = i == at;
        let mut spans: Vec<Span<'static>> = marker(is_sel, t).into();
        if icon > 0 {
            let glyph = row.map_or("", |r| cx.icons.project_type(r.order.project_type));
            spans.push(Span::styled(glyph.to_string(), t.muted()));
            spans.push(Span::raw(" "));
        }
        let slug_style = if is_sel {
            t.text().add_modifier(Modifier::BOLD)
        } else {
            t.text()
        };
        spans.push(cell(&o.slug, SLUG, slug_style));
        spans.push(gap(GAP));
        spans.push(price_cell(o.price_minor, PRICE, t.text(), t));
        spans.push(gap(GAP));
        spans.push(match o.days {
            Some(d) => {
                let style = if d >= OVERDUE_DAYS {
                    t.title().fg(t.unpaid)
                } else {
                    t.muted()
                };
                cell_right(&format!("{d}d"), SINCE, style)
            }
            None => empty_right(SINCE, t),
        });
        if title_w > 0 {
            let title = row.map_or("", |r| r.order.title.as_str());
            spans.push(gap(GAP));
            spans.push(cell(title, title_w, t.text()));
        }
        let line = Line::from(spans);
        lines.push(if is_sel {
            banded(line, area.width, t)
        } else {
            line
        });
    }
    let hidden = owed.len() - shown;
    if hidden > 0 {
        lines.push(Line::from(Span::styled(
            format!("  \u{2193} {hidden} more"),
            m,
        )));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// Days column of the drill-down: the payment day, `MM-DD`.
const PAID: usize = 5;

/// The orders paid in the clicked month (section 11.4): a heading with the
/// count and the total in the price column, a header row, then one row per
/// order with the day it was paid. No selection: a click on a row jumps to
/// the order.
fn month_orders(frame: &mut Frame, area: Rect, cx: &RenderCx, class: WidthClass, m: &Month) {
    let t = cx.theme;
    let icon = if cx.icons.enabled { 2 } else { 0 };
    let price_end = 2 + icon + SLUG + GAP + PRICE;
    let name =
        year_month(&m.label).map_or(m.label.clone(), |(y, k)| format!("{} {y}", MONTH_NAMES[k]));
    let n = m.order_ids.len();
    let lead_word = format!("Received in {name}");
    let lead = format!("{lead_word}  {n}");
    let total_s = text::money(m.amount.gross);
    let mut lines = vec![Line::from(vec![
        Span::styled(lead_word, t.title()),
        Span::styled(format!("  {n}"), t.muted()),
        gap(price_end
            .saturating_sub(text::width(&lead) + text::width(&total_s))
            .max(2)),
        Span::styled(total_s, t.muted()),
    ])];
    if n == 0 {
        lines.push(Line::from(Span::styled(
            format!("no payments in {name}"),
            t.muted(),
        )));
        frame.render_widget(Paragraph::new(lines), area);
        return;
    }
    let fixed = price_end + GAP + PAID;
    let title_w = usize::from(area.width).saturating_sub(fixed + GAP);
    let title_w = if class == WidthClass::Narrow || title_w < MIN_TITLE {
        0
    } else {
        title_w
    };
    let muted = t.muted();
    let mut head = vec![
        gap(2 + icon),
        cell("order", SLUG, muted),
        gap(GAP),
        cell_right(cx.state.data.currency(), PRICE, muted),
        gap(GAP),
        cell_right("paid", PAID, muted),
    ];
    if title_w > 0 {
        head.push(gap(GAP));
        head.push(Span::styled("title", muted));
    }
    lines.push(Line::from(head));
    let room = usize::from(area.height).saturating_sub(lines.len());
    let shown = if n <= room { n } else { room.saturating_sub(1) };
    for id in m.order_ids.iter().take(shown) {
        let Some(row) = cx.state.data.order(*id) else {
            continue;
        };
        cx.state.hits.add(
            Rect::new(area.x, area.y + lines.len() as u16, area.width, 1),
            Target::Paid(*id),
        );
        let o = &row.order;
        let closed = row.group == Group::Closed;
        let ink = if closed {
            t.text().fg(t.archived)
        } else {
            t.text()
        };
        let mut spans = vec![gap(2)];
        if icon > 0 {
            let style = if closed { ink } else { muted };
            spans.push(Span::styled(
                cx.icons.project_type(o.project_type).to_string(),
                style,
            ));
            spans.push(Span::raw(" "));
        }
        spans.push(cell(&o.slug, SLUG, ink));
        spans.push(gap(GAP));
        spans.push(price_cell(o.price_minor, PRICE, ink, t));
        spans.push(gap(GAP));
        let day = o
            .paid_at
            .as_deref()
            .and_then(day_part)
            .and_then(|d| d.get(5..));
        spans.push(match day {
            Some(d) => cell_right(d, PAID, if closed { ink } else { muted }),
            None => empty_right(PAID, t),
        });
        if title_w > 0 {
            spans.push(gap(GAP));
            spans.push(cell(&o.title, title_w, ink));
        }
        lines.push(Line::from(spans));
    }
    let hidden = n - shown;
    if hidden > 0 {
        lines.push(Line::from(Span::styled(
            format!("  \u{2193} {hidden} more"),
            muted,
        )));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_follows_the_design() {
        // 120 columns: chart 118, slot 9, bar 5.
        assert_eq!(geometry(118, 120), (9, 5));
        // 80 columns: chart 78, slot 6, bar 3.
        assert_eq!(geometry(78, 80), (6, 3));
        // 200 columns: slot 16, bar 5.
        assert_eq!(geometry(198, 200), (16, 5));
        assert_eq!(geometry(58, 60), (4, 2));
        assert_eq!(plot_height(36), 10);
        assert_eq!(plot_height(24), 6);
        assert_eq!(plot_height(20), 4);
    }
}
