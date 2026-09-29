//! Order detail (TUI-DESIGN.md section 9): the right pane at Medium and
//! Wide, full screen at Narrow and from History and Money. Shows the order
//! the actions apply to.
//!
//! Lines are wrapped here, not by the Paragraph, so continuation rows keep
//! their hanging indent and the row count is known: a detail taller than
//! its area scrolls with PgUp/PgDn/Home/End (`UiState::detail_scroll`).

use super::orders::split_dated;
use super::{empty, overdue, status_chip, status_style, DOT};
use crate::data::money::take_home;
use crate::data::{day_part, Group, OrderRow};
use crate::text;
use crate::ui::RenderCx;
use gig_core::models::{OrderStatus, PackageStatus};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// Notes shown (the newest).
pub const NOTES: usize = 5;
/// JOB.md status entries shown (the newest).
pub const STATUS: usize = 3;
/// Widest prose.
pub const MEASURE: usize = 76;

/// Section body indent.
const BODY: usize = 2;
/// Mini statement: label column, number column.
const LABEL: usize = 11;
const NUMBER: usize = 7;

/// One screen row and whether it is a section heading.
struct Row {
    line: Line<'static>,
    heading: bool,
}

/// Rows of the detail at `width` cells; `full` shows every line of every
/// entry, the pane caps entries at 2 lines.
struct Builder<'a> {
    cx: &'a RenderCx<'a>,
    width: usize,
    full: bool,
    rows: Vec<Row>,
}

impl<'a> Builder<'a> {
    fn push(&mut self, line: Line<'static>) {
        self.rows.push(Row {
            line,
            heading: false,
        });
    }

    fn blank(&mut self) {
        self.push(Line::raw(""));
    }

    /// `spans` wrapped at the width, continuation rows at `hang`, at most
    /// `cap` rows (the last ending in an ellipsis when cut).
    fn wrapped(&mut self, spans: Vec<Span<'static>>, hang: usize, cap: Option<usize>) {
        let mut rows = wrap_spans(&spans, self.width, hang);
        if let Some(cap) = cap {
            if rows.len() > cap {
                rows.truncate(cap);
                if let Some(last) = rows.last_mut() {
                    ellipsize(last, self.width);
                }
            }
        }
        for r in rows {
            self.push(r);
        }
    }

    /// A blank row and the heading, with its count unless `None`.
    fn heading(&mut self, title: &str, count: Option<usize>) {
        let t = self.cx.theme;
        self.blank();
        let mut spans = vec![Span::styled(title.to_string(), t.title())];
        if let Some(n) = count {
            spans.push(Span::styled(format!("  {n}"), t.muted()));
        }
        self.rows.push(Row {
            line: Line::from(spans),
            heading: true,
        });
    }

    /// A dated entry: `MM-DD`, 2 cells, the text with a hanging indent
    /// under its start; `lead` goes before the date (`? `).
    fn dated(&mut self, lead: Vec<Span<'static>>, day: Option<&str>, body: Vec<Span<'static>>) {
        let t = self.cx.theme;
        let mut spans = vec![Span::raw(" ".repeat(BODY))];
        let mut hang = BODY;
        for s in lead {
            hang += text::width(&s.content);
            spans.push(s);
        }
        if let Some(d) = day {
            let shown = self.date(d);
            hang += text::width(&shown) + 2;
            spans.push(Span::styled(format!("{shown}  "), t.muted()));
        }
        spans.extend(body);
        let cap = (!self.full).then_some(2);
        self.wrapped(spans, hang, cap);
    }

    /// `MM-DD` in the current year, else `YYYY-MM-DD`.
    fn date(&self, day: &str) -> String {
        let today = &self.cx.state.data.today;
        if day.get(..4) == today.get(..4) {
            day.get(5..10).unwrap_or(day).to_string()
        } else {
            day.to_string()
        }
    }
}

/// `spans` as rows of at most `width` cells; continuation rows start at
/// `hang` cells.
fn wrap_spans(spans: &[Span<'static>], width: usize, hang: usize) -> Vec<Line<'static>> {
    let mut flat = String::new();
    let mut bounds = Vec::new();
    for s in spans {
        let start = flat.len();
        flat.push_str(&text::flatten(&s.content));
        bounds.push((start..flat.len(), s.style));
    }
    let hang = hang.min(width.saturating_sub(1));
    let mut out = Vec::new();
    for (i, r) in text::wrap_ranges(&flat, width, width - hang)
        .into_iter()
        .enumerate()
    {
        let mut line = Vec::new();
        if i > 0 && hang > 0 {
            line.push(Span::raw(" ".repeat(hang)));
        }
        for (b, style) in &bounds {
            let lo = b.start.max(r.start);
            let hi = b.end.min(r.end);
            if lo < hi {
                line.push(Span::styled(flat[lo..hi].to_string(), *style));
            }
        }
        out.push(Line::from(line));
    }
    out
}

/// Cut the end of `line` so an ellipsis fits within `width`.
fn ellipsize(line: &mut Line<'static>, width: usize) {
    let mut room = width.saturating_sub(1);
    let mut spans = Vec::new();
    let mut style = Style::new();
    for s in line.spans.drain(..) {
        let w = text::width(&s.content);
        style = s.style;
        if w <= room {
            room -= w;
            spans.push(s);
        } else {
            let mut cut = String::new();
            let mut used = 0;
            for g in unicode_segmentation::UnicodeSegmentation::graphemes(s.content.as_ref(), true)
            {
                let gw = text::width(g);
                if used + gw > room {
                    break;
                }
                used += gw;
                cut.push_str(g);
            }
            spans.push(Span::styled(cut, s.style));
            break;
        }
    }
    spans.push(Span::styled(text::ELLIPSIS.to_string(), style));
    line.spans = spans;
}

/// `items` packed into rows of at most `width` cells, joined by `sep`;
/// rows break only between items.
fn pack(items: &[String], sep: &str, width: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in items {
        match out.last_mut() {
            Some(row) if text::width(row) + text::width(sep) + text::width(item) <= width => {
                row.push_str(sep);
                row.push_str(item);
            }
            _ => out.push(text::truncate(item, width)),
        }
    }
    out
}

pub fn render(frame: &mut Frame, area: Rect, cx: &RenderCx, full: bool) {
    let Some(row) = cx.state.selected_order() else {
        // Beside an empty list the pane stays blank: the list's own empty
        // state (section 8.1) already says why.
        if full {
            empty(frame, area, cx.theme, &["no order selected"]);
        }
        return;
    };
    let width = usize::from(area.width).min(MEASURE);
    let rows = rows(cx, row, width, full);
    let height = usize::from(area.height);
    let scroll = &cx.state.detail_scroll;
    if rows.len() <= height {
        scroll.max.set(0);
        let lines: Vec<Line> = rows.into_iter().map(|r| r.line).collect();
        frame.render_widget(Paragraph::new(lines), area);
        return;
    }
    // At the end only the top marker is shown.
    let max = rows.len() + 1 - height;
    scroll.max.set(u16::try_from(max).unwrap_or(u16::MAX));
    let offset = usize::from(scroll.offset).min(max);
    let top = usize::from(offset > 0);
    let mut shown = height - top;
    if offset + shown < rows.len() {
        shown -= 1;
        // A heading is never the last row: it goes with its body.
        if shown > 1 && rows[offset + shown - 1].heading {
            shown -= 1;
        }
    }
    let below = rows.len() - offset - shown;
    let m = cx.theme.muted();
    let marker = |s: String| Line::from(Span::styled(s, m)).right_aligned();
    let mut lines = Vec::new();
    if top > 0 {
        lines.push(marker(format!("\u{2191} {offset} above  PgUp")));
    }
    lines.extend(rows.into_iter().skip(offset).take(shown).map(|r| r.line));
    if below > 0 {
        while lines.len() < height - 1 {
            lines.push(Line::raw(""));
        }
        lines.push(marker(format!("\u{2193} {below} more  PgDn")));
    }
    let area = Rect {
        width: width as u16,
        ..area
    };
    frame.render_widget(Paragraph::new(lines), area);
}

/// The key that performs the next action, when there is one.
fn next_key(r: &OrderRow) -> Option<(&'static str, &'static str)> {
    match r.order.status {
        OrderStatus::Delivered => Some(("p", "paid")),
        OrderStatus::Queued => Some(("s", "start")),
        OrderStatus::InProgress => Some(("u", "upload")),
        OrderStatus::Paid if r.next_action == "archive" => Some(("A", "archive")),
        _ => None,
    }
}

/// The detail as screen rows.
fn rows(cx: &RenderCx, r: &OrderRow, width: usize, full: bool) -> Vec<Row> {
    let t = cx.theme;
    let o = &r.order;
    let mut b = Builder {
        cx,
        width,
        full,
        rows: Vec::new(),
    };

    // Header: title (2 rows in the pane), meta, status line.
    b.wrapped(
        vec![Span::styled(o.title.clone(), t.title())],
        0,
        (!full).then_some(2),
    );
    let dot = || Span::styled(format!(" {DOT} "), t.dim());
    let mut meta = vec![Span::styled(o.slug.clone(), t.muted()), dot()];
    meta.push(Span::styled(
        cx.icons.label(
            cx.icons.project_type(o.project_type),
            o.project_type.as_str(),
        ),
        t.muted(),
    ));
    if let Some(p) = &o.platform {
        meta.push(dot());
        meta.push(Span::styled(p.clone(), t.muted()));
    }
    b.wrapped(meta, 0, None);
    let mut status = vec![Span::styled(status_chip(cx, o.status), status_style(cx, r))];
    if let Some(d) = r.days_in_status {
        status.push(dot());
        let unit = if d == 1 { "day" } else { "days" };
        let style = if overdue(r) {
            t.title().fg(t.unpaid)
        } else {
            t.muted()
        };
        status.push(Span::styled(format!("{d} {unit} in status"), style));
    }
    if o.status == OrderStatus::Paid {
        if let Some(w) = &o.warranty_until {
            let running = w.as_str() > cx.state.data.today.as_str();
            status.push(dot());
            let style = if running {
                t.text().fg(t.warranty)
            } else {
                t.muted()
            };
            status.push(Span::styled(format!("warranty until {w}"), style));
        }
    }
    if o.status == OrderStatus::Cancelled {
        if let Some(reason) = &o.cancel_reason {
            status.push(dot());
            status.push(Span::styled(reason.clone(), t.muted()));
        }
    }
    b.wrapped(status, 0, None);

    // Mini statement.
    b.blank();
    let statement = |label: &str, number: Span<'static>, unit: &str| {
        Line::from(vec![
            Span::styled(text::fit(label, LABEL), t.muted()),
            number,
            Span::styled(unit.to_string(), t.muted()),
        ])
    };
    let currency = format!(" {}", o.currency);
    let num = |s: String| super::cell_right(&s, NUMBER, t.text());
    match o.price_minor {
        Some(p) => {
            b.push(statement("price", num(text::money(p)), &currency));
            b.push(statement(
                "cut",
                num(format!("{:.0}", o.cut_ratio * 100.0)),
                "%",
            ));
            b.push(statement(
                "take-home",
                num(text::money(take_home(p, o.cut_ratio))),
                &currency,
            ));
        }
        None => {
            b.push(statement("price", super::empty_right(NUMBER, t), ""));
            b.push(statement(
                "cut",
                num(format!("{:.0}", o.cut_ratio * 100.0)),
                "%",
            ));
        }
    }

    // Next: the action and the key that performs it.
    b.heading("Next", None);
    let next = if r.group == Group::Closed {
        "none".to_string()
    } else {
        r.next_action.clone()
    };
    let key = next_key(r).filter(|_| r.group != Group::Closed);
    let key_w = key.map_or(0, |(k, l)| text::width(k) + 1 + text::width(l));
    let room = width.saturating_sub(BODY + key_w + if key_w > 0 { 2 } else { 0 });
    let next = text::truncate(&next, room);
    let mut spans = vec![
        Span::raw(" ".repeat(BODY)),
        Span::styled(next.clone(), t.text()),
    ];
    if let Some((k, l)) = key {
        let pad = width.saturating_sub(BODY + text::width(&next) + key_w);
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::styled(k, t.key()));
        spans.push(Span::styled(format!(" {l}"), t.muted()));
    }
    b.push(Line::from(spans));

    let mut missing: Vec<String> = Vec::new();

    // Packages, newest first.
    if r.packages.is_empty() {
        missing.push("no packages".into());
    } else {
        b.heading("Packages", Some(r.packages.len()));
        let mut packages: Vec<_> = r.packages.iter().collect();
        packages.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        let body_w = width.saturating_sub(BODY);
        for p in packages {
            b.push(Line::from(vec![
                Span::raw(" ".repeat(BODY)),
                Span::styled(text::truncate_middle(&p.package_id, body_w, 10), t.text()),
            ]));
            let status_style = match p.status {
                PackageStatus::Checked => t.text().fg(t.queued),
                PackageStatus::Sent => t.text(),
                PackageStatus::Legacy => t.muted(),
            };
            let mut parts = vec![
                Span::styled(p.kind.to_string(), t.muted()),
                Span::styled(format!(" {DOT} "), t.dim()),
                Span::styled(p.status.to_string(), status_style),
            ];
            if let Some(c) = p.channel {
                parts.push(Span::styled(format!(" {DOT} "), t.dim()));
                parts.push(Span::styled(c.as_str().to_string(), t.muted()));
            }
            if let Some(d) = p.sent_at.as_deref().and_then(day_part) {
                parts.push(Span::styled(format!(" {DOT} "), t.dim()));
                parts.push(Span::styled(b.date(d), t.muted()));
            }
            let mut line = vec![Span::raw(" ".repeat(BODY))];
            line.extend(parts);
            let used: usize = line.iter().map(|s| text::width(&s.content)).sum();
            let link = p
                .short_url
                .as_deref()
                .or(p.remote_url.as_deref())
                .map(|l| text::strip_scheme(l).to_string());
            match link {
                Some(l) if used + 2 + text::width(&l) <= width => {
                    line.push(Span::raw("  "));
                    line.push(Span::styled(l, t.link()));
                    b.push(Line::from(line));
                }
                Some(l) => {
                    b.push(Line::from(line));
                    b.push(Line::from(vec![
                        Span::raw(" ".repeat(BODY)),
                        Span::styled(text::truncate(&l, body_w), t.link()),
                    ]));
                }
                None => b.push(Line::from(line)),
            }
        }
    }

    // Latest status: the newest JOB.md entries.
    if r.job.status.is_empty() {
        missing.push(if o.dev_path.is_none() {
            "no project directory".into()
        } else if !r.job.found {
            "no JOB.md".into()
        } else {
            "no status entries".into()
        });
    } else {
        b.heading("Latest status", Some(r.job.status.len().min(STATUS)));
        for s in r.job.status.iter().take(STATUS) {
            let (day, rest) = split_dated(s);
            b.dated(
                Vec::new(),
                day.and(day_part(s)),
                vec![Span::styled(rest.to_string(), t.text())],
            );
        }
    }

    if r.job.open_questions.is_empty() {
        missing.push("no client questions".into());
    } else {
        b.heading("Client questions", Some(r.job.open_questions.len()));
        for q in &r.job.open_questions {
            let (day, rest) = split_dated(q);
            b.dated(
                vec![Span::styled("? ", t.text().fg(t.warranty))],
                day.and(day_part(q)),
                vec![Span::styled(rest.to_string(), t.text())],
            );
        }
    }

    if r.requirement_changes.is_empty() {
        missing.push("no requirement changes".into());
    } else {
        b.heading("Requirement changes", Some(r.requirement_changes.len()));
        for c in &r.requirement_changes {
            let mut body = Vec::new();
            if c.price_delta_minor != 0 {
                let sign = if c.price_delta_minor > 0 { "+" } else { "" };
                body.push(Span::styled(
                    format!("{sign}{}  ", text::money(c.price_delta_minor)),
                    t.key(),
                ));
            }
            body.push(Span::styled(c.description.clone(), t.text()));
            b.dated(Vec::new(), day_part(&c.created_at), body);
        }
    }

    if r.notes.is_empty() {
        missing.push("no notes".into());
    } else {
        b.heading("Notes", Some(r.notes.len()));
        let skip = r.notes.len().saturating_sub(NOTES);
        for n in r.notes.iter().skip(skip) {
            b.dated(
                Vec::new(),
                day_part(&n.at),
                vec![Span::styled(n.text.clone(), t.text())],
            );
        }
    }

    match &r.scorecard {
        None => missing.push("no scorecard (k records one)".into()),
        Some(s) => {
            b.heading("Scorecard", None);
            let n = |v: Option<i64>, one: &str, many: &str| {
                v.map(|v| format!("{v} {}", if v == 1 { one } else { many }))
            };
            let items: Vec<String> = [
                n(s.decisions, "decision", "decisions"),
                n(s.repeat_questions, "repeat question", "repeat questions"),
                n(s.days_to_preview, "day to preview", "days to preview"),
                n(s.cleanups, "cleanup", "cleanups"),
                n(s.check_rejections, "check rejection", "check rejections"),
                n(s.report_reworks, "report rework", "report reworks"),
            ]
            .into_iter()
            .flatten()
            .collect();
            let score = s
                .score
                .map_or(format!("score {DOT}"), |v| format!("score {v}/5"));
            let sep = format!("  {DOT}  ");
            let mut all = vec![score.clone()];
            all.extend(items);
            let body_w = width.saturating_sub(BODY);
            for (i, row) in pack(&all, &sep, body_w).into_iter().enumerate() {
                let mut spans = vec![Span::raw(" ".repeat(BODY))];
                if i == 0 {
                    let rest = row[score.len()..].to_string();
                    spans.push(Span::styled(score.clone(), t.title()));
                    spans.push(Span::styled(rest, t.muted()));
                } else {
                    spans.push(Span::styled(row, t.muted()));
                }
                b.push(Line::from(spans));
            }
            if let Some(note) = &s.note {
                b.wrapped(
                    vec![
                        Span::raw(" ".repeat(BODY)),
                        Span::styled(note.clone(), t.muted()),
                    ],
                    BODY,
                    None,
                );
            }
        }
    }

    // Empty sections, together, last.
    if !missing.is_empty() {
        b.blank();
        for row in pack(&missing, &format!(" {DOT} "), width) {
            b.push(Line::from(Span::styled(row, t.muted())));
        }
    }
    b.rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packing_breaks_between_items() {
        let items: Vec<String> = [
            "no status entries",
            "no client questions",
            "no scorecard (k records one)",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            pack(&items, " \u{b7} ", 40),
            vec![
                "no status entries \u{b7} no client questions",
                "no scorecard (k records one)"
            ]
        );
    }

    #[test]
    fn capped_rows_end_in_an_ellipsis() {
        let mut line = Line::from(vec![Span::raw("abcdef"), Span::raw("ghij")]);
        ellipsize(&mut line, 8);
        assert_eq!(line.to_string(), "abcdefg\u{2026}");
    }
}
