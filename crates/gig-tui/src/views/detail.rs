//! Order detail (spec 2.1): right pane at 110+ columns, full screen below
//! (and from History). Shows the order the actions apply to.

use super::{days, empty, price, status_chip};
use crate::data::money::take_home;
use crate::data::{Group, OrderRow};
use crate::text;
use crate::ui::RenderCx;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

/// Notes shown (the newest).
pub const NOTES: usize = 5;

pub fn render(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    match cx.state.selected_order() {
        Some(row) => {
            let lines = lines(cx, row, area.width);
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
        }
        None => empty(frame, area, cx.theme, &["no order selected"]),
    }
}

/// The detail as lines; long values wrap in the paragraph.
pub fn lines(cx: &RenderCx, r: &OrderRow, width: u16) -> Vec<Line<'static>> {
    let t = cx.theme;
    let o = &r.order;
    let mut out = Vec::new();

    // Header.
    out.push(Line::from(Span::styled(
        text::truncate(&o.title, usize::from(width)),
        t.text().add_modifier(Modifier::BOLD),
    )));
    let type_label = cx.icons.label(
        cx.icons.project_type(o.project_type),
        o.project_type.as_str(),
    );
    let mut id_line = vec![
        Span::styled(o.slug.clone(), t.title()),
        Span::styled("  ", t.dim()),
        Span::styled(type_label, t.dim()),
    ];
    if let Some(p) = &o.platform {
        id_line.push(Span::styled(format!("  {p}"), t.dim()));
    }
    out.push(Line::from(id_line));
    out.push(Line::from(vec![
        Span::styled(status_chip(cx, o.status), t.status(o.status)),
        Span::styled(format!("  {} in status", days(r.days_in_status)), t.dim()),
    ]));
    let mut money = vec![
        Span::styled("price ", t.dim()),
        Span::styled(format!("{} {}", price(o.price_minor), o.currency), t.text()),
        Span::styled(format!("  cut {:.0}%", o.cut_ratio * 100.0), t.dim()),
    ];
    if let Some(p) = o.price_minor {
        money.push(Span::styled(
            format!(" = {}", price(Some(take_home(p, o.cut_ratio)))),
            t.dim(),
        ));
    }
    out.push(Line::from(money));
    if let Some(w) = &o.warranty_until {
        out.push(kv(cx, "warranty until", w.clone(), t.status(o.status)));
    }
    if r.group == Group::Closed {
        if let Some(reason) = &o.cancel_reason {
            out.push(kv(cx, "cancelled", reason.clone(), t.dim()));
        }
    }

    section(cx, &mut out, "Next action");
    out.push(item(cx, r.next_action.clone(), t.text()));

    section(cx, &mut out, "Packages");
    if r.packages.is_empty() {
        out.push(none(cx));
    }
    for p in &r.packages {
        let sent = p
            .sent_at
            .as_deref()
            .and_then(crate::data::day_part)
            .unwrap_or("-");
        let channel = p.channel.map_or("-", |c| c.as_str());
        let link = p
            .short_url
            .as_deref()
            .or(p.remote_url.as_deref())
            .unwrap_or("");
        out.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(p.package_id.clone(), t.text()),
            Span::styled(
                format!("  {}  {}  {channel}  {sent}  ", p.kind, p.status),
                t.dim(),
            ),
            Span::styled(link.to_string(), t.title().remove_modifier(Modifier::BOLD)),
        ]));
    }

    section(cx, &mut out, "Latest status");
    if r.job.status.is_empty() {
        let why = if r.job.found { "(none)" } else { "(no JOB.md)" };
        out.push(item(cx, why.into(), t.dim()));
    }
    for s in &r.job.status {
        push_entry(cx, &mut out, s, t.text());
    }

    section(cx, &mut out, "Client questions");
    if r.job.open_questions.is_empty() {
        out.push(none(cx));
    }
    for q in &r.job.open_questions {
        push_entry(cx, &mut out, q, t.text().fg(t.warranty));
    }

    section(cx, &mut out, "Requirement changes");
    if r.requirement_changes.is_empty() {
        out.push(none(cx));
    }
    for c in &r.requirement_changes {
        let delta = if c.price_delta_minor == 0 {
            String::new()
        } else {
            format!(
                "{}{} ",
                if c.price_delta_minor > 0 { "+" } else { "" },
                price(Some(c.price_delta_minor))
            )
        };
        let day = crate::data::day_part(&c.created_at).unwrap_or("");
        out.push(Line::from(vec![
            Span::styled(format!("  {day} "), t.dim()),
            Span::styled(delta, t.key()),
            Span::styled(c.description.clone(), t.text()),
        ]));
    }

    section(cx, &mut out, "Notes");
    if r.notes.is_empty() {
        out.push(none(cx));
    }
    let skip = r.notes.len().saturating_sub(NOTES);
    for n in r.notes.iter().skip(skip) {
        let day = crate::data::day_part(&n.at).unwrap_or("");
        let first = n.text.lines().next().unwrap_or("");
        let more = if n.text.lines().count() > 1 {
            " ..."
        } else {
            ""
        };
        out.push(Line::from(vec![
            Span::styled(format!("  {day} "), t.dim()),
            Span::styled(format!("{first}{more}"), t.text()),
        ]));
    }

    section(cx, &mut out, "Scorecard");
    match &r.scorecard {
        None => out.push(item(cx, "(none; k records one)".into(), t.dim())),
        Some(s) => {
            let n = |v: Option<i64>| v.map_or("-".to_string(), |v| v.to_string());
            out.push(item(
                cx,
                format!(
                    "score {}  decisions {}  repeat qs {}  cleanups {}  report reworks {}",
                    n(s.score),
                    n(s.decisions),
                    n(s.repeat_questions),
                    n(s.cleanups),
                    n(s.report_reworks)
                ),
                t.text(),
            ));
            if let Some(note) = &s.note {
                out.push(item(cx, note.clone(), t.dim()));
            }
        }
    }
    out
}

fn section(cx: &RenderCx, out: &mut Vec<Line<'static>>, title: &str) {
    out.push(Line::raw(""));
    out.push(Line::from(Span::styled(
        title.to_string(),
        cx.theme.title(),
    )));
}

fn item(_cx: &RenderCx, text: String, style: Style) -> Line<'static> {
    Line::from(vec![Span::raw("  "), Span::styled(text, style)])
}

fn none(cx: &RenderCx) -> Line<'static> {
    item(cx, "(none)".into(), cx.theme.dim())
}

fn kv(cx: &RenderCx, k: &str, v: String, style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{k} "), cx.theme.dim()),
        Span::styled(v, style),
    ])
}

/// A JOB.md entry: first line as an item, continuation lines indented.
fn push_entry(cx: &RenderCx, out: &mut Vec<Line<'static>>, entry: &str, style: Style) {
    for (i, l) in entry.lines().enumerate() {
        let lead = if i == 0 { "- " } else { "  " };
        out.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(lead, cx.theme.dim()),
            Span::styled(l.to_string(), style),
        ]));
    }
}
