//! Order detail (spec 2.1): right pane at 110+ columns, full screen below
//! (and from History). Shows the order the actions apply to.
//!
//! The lines are wrapped here, not by the Paragraph, so continuation rows
//! keep the item indent and the row count is known: a detail taller than
//! its area scrolls with PgUp/PgDn/Home/End (`UiState::detail_scroll`) and
//! its last row says how much is below.

use super::{days, empty, price, status_chip, status_style};
use crate::data::money::take_home;
use crate::data::{Group, OrderRow};
use crate::text;
use crate::ui::RenderCx;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// Notes shown (the newest).
pub const NOTES: usize = 5;
/// JOB.md status entries shown (the newest).
pub const STATUS: usize = 3;

/// Item indent under a section heading.
const ITEM: usize = 2;
/// Indent of the text of a `- ` entry.
const ENTRY: usize = 4;

pub fn render(frame: &mut Frame, area: Rect, cx: &RenderCx, _full: bool) {
    let Some(row) = cx.state.selected_order() else {
        empty(frame, area, cx.theme, &["no order selected"]);
        return;
    };
    let rows = wrapped(&lines(cx, row), usize::from(area.width));
    let height = usize::from(area.height);
    let scroll = &cx.state.detail_scroll;
    if rows.len() <= height {
        scroll.max.set(0);
        frame.render_widget(Paragraph::new(rows), area);
        return;
    }
    // The last row is the scroll marker.
    let body = height.saturating_sub(1);
    let max = rows.len() - body;
    scroll.max.set(u16::try_from(max).unwrap_or(u16::MAX));
    let offset = usize::from(scroll.offset).min(max);
    let below = rows.len() - offset - body;
    let mut shown: Vec<Line> = rows.into_iter().skip(offset).take(body).collect();
    let marker = if below > 0 {
        format!("\u{2193} {below} more lines  PgDn")
    } else {
        "\u{2191} PgUp back to the top".to_string()
    };
    shown.push(Line::from(Span::styled(
        text::truncate(&marker, usize::from(area.width)),
        cx.theme.muted(),
    )));
    frame.render_widget(Paragraph::new(shown), area);
}

/// One logical line of the detail and the indent of its continuation rows.
pub struct Item {
    pub spans: Vec<Span<'static>>,
    pub hang: usize,
}

impl Item {
    fn new(spans: Vec<Span<'static>>, hang: usize) -> Self {
        Self { spans, hang }
    }
}

/// `items` as screen rows of at most `width` cells; continuation rows start
/// at the item's hanging indent.
pub fn wrapped(items: &[Item], width: usize) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    for item in items {
        // Concatenate the spans (control characters as spaces) and remember
        // where each one starts.
        let mut flat = String::new();
        let mut bounds = Vec::new();
        for s in &item.spans {
            let start = flat.len();
            flat.push_str(&text::flatten(&s.content));
            bounds.push((start..flat.len(), s.style));
        }
        let hang = item.hang.min(width.saturating_sub(1));
        let ranges = text::wrap_ranges(&flat, width, width - hang.min(width));
        for (i, r) in ranges.into_iter().enumerate() {
            let mut spans = Vec::new();
            if i > 0 && hang > 0 {
                spans.push(Span::raw(" ".repeat(hang)));
            }
            for (b, style) in &bounds {
                let lo = b.start.max(r.start);
                let hi = b.end.min(r.end);
                if lo < hi {
                    spans.push(Span::styled(flat[lo..hi].to_string(), *style));
                }
            }
            out.push(Line::from(spans));
        }
    }
    out
}

/// The detail as logical lines.
pub fn lines(cx: &RenderCx, r: &OrderRow) -> Vec<Item> {
    let t = cx.theme;
    let o = &r.order;
    let mut out = Vec::new();

    // Header. The title wraps: it is shown in full nowhere else.
    out.push(Item::new(
        vec![Span::styled(
            o.title.clone(),
            t.text().add_modifier(Modifier::BOLD),
        )],
        0,
    ));
    let type_label = cx.icons.label(
        cx.icons.project_type(o.project_type),
        o.project_type.as_str(),
    );
    let mut id_line = vec![
        Span::styled(o.slug.clone(), t.title()),
        Span::styled("  ", t.muted()),
        Span::styled(type_label, t.muted()),
    ];
    if let Some(p) = &o.platform {
        id_line.push(Span::styled(format!("  {p}"), t.muted()));
    }
    out.push(Item::new(id_line, 0));
    out.push(Item::new(
        vec![
            Span::styled(status_chip(cx, o.status), status_style(cx, r)),
            Span::styled(format!("  {} in status", days(r.days_in_status)), t.muted()),
        ],
        0,
    ));
    let cut = format!("cut {:.0}%", o.cut_ratio * 100.0);
    let money = match o.price_minor {
        Some(p) => vec![
            Span::styled("price ", t.muted()),
            Span::styled(format!("{} {}", price(Some(p)), o.currency), t.text()),
            Span::styled(format!("  {cut}  take-home "), t.muted()),
            Span::styled(
                format!("{} {}", price(Some(take_home(p, o.cut_ratio))), o.currency),
                t.text(),
            ),
        ],
        None => vec![
            Span::styled("no price", t.text()),
            Span::styled(format!("  {cut}"), t.muted()),
        ],
    };
    out.push(Item::new(money, 0));
    if let Some(w) = &o.warranty_until {
        out.push(kv(cx, "warranty until", w.clone(), status_style(cx, r)));
    }
    if r.group == Group::Closed {
        if let Some(reason) = &o.cancel_reason {
            out.push(kv(cx, "cancelled", reason.clone(), t.muted()));
        }
    }

    section(cx, &mut out, "Next action", false);
    out.push(item(r.next_action.clone(), t.text()));

    section(cx, &mut out, "Packages", r.packages.is_empty());
    let col = |f: &dyn Fn(&gig_core::models::Package) -> String| {
        r.packages
            .iter()
            .map(|p| text::width(&f(p)))
            .max()
            .unwrap_or(0)
    };
    let id_w = col(&|p| p.package_id.clone());
    let kind_w = col(&|p| p.kind.to_string());
    let status_w = col(&|p| p.status.to_string());
    let channel_w = col(&|p| p.channel.map_or(String::new(), |c| c.as_str().to_string()));
    for p in &r.packages {
        let sent = p
            .sent_at
            .as_deref()
            .and_then(crate::data::day_part)
            .unwrap_or("");
        let channel = p.channel.map_or(String::new(), |c| c.as_str().to_string());
        let link = p
            .short_url
            .as_deref()
            .or(p.remote_url.as_deref())
            .unwrap_or("");
        let fixed = format!(
            " {}  {}  {}  {:10}",
            text::fit(&p.kind.to_string(), kind_w),
            text::fit(&p.status.to_string(), status_w),
            text::fit(&channel, channel_w),
            sent,
        );
        let mut spans = vec![
            Span::raw(" ".repeat(ITEM)),
            Span::styled(text::fit(&p.package_id, id_w), t.text()),
            Span::styled(fixed.trim_end().to_string(), t.muted()),
        ];
        if !link.is_empty() {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                link.to_string(),
                t.title().remove_modifier(Modifier::BOLD),
            ));
        }
        out.push(Item::new(spans, ITEM + id_w + 1));
    }

    let why = if o.dev_path.is_none() {
        Some("(no project directory)")
    } else if !r.job.found {
        Some("(no JOB.md)")
    } else {
        None
    };
    let no_status = r.job.status.is_empty();
    match (no_status, why) {
        (true, Some(why)) => {
            section_note(cx, &mut out, "Latest status", why);
        }
        _ => {
            section(cx, &mut out, "Latest status", no_status);
            for s in r.job.status.iter().take(STATUS) {
                push_entry(cx, &mut out, s, t.text());
            }
        }
    }

    section(
        cx,
        &mut out,
        "Client questions",
        r.job.open_questions.is_empty(),
    );
    for q in &r.job.open_questions {
        push_entry(cx, &mut out, q, t.text().fg(t.warranty));
    }

    section(
        cx,
        &mut out,
        "Requirement changes",
        r.requirement_changes.is_empty(),
    );
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
        // A description written in $EDITOR may span several lines: the
        // first follows the date, the others are indented under it.
        let mut desc = c.description.lines();
        let prefix = format!("{day} ");
        out.push(Item::new(
            vec![
                Span::raw(" ".repeat(ITEM)),
                Span::styled(prefix.clone(), t.muted()),
                Span::styled(delta, t.key()),
                Span::styled(desc.next().unwrap_or("").to_string(), t.text()),
            ],
            ITEM + text::width(&prefix),
        ));
        for more in desc {
            out.push(Item::new(
                vec![
                    Span::raw(" ".repeat(ITEM + text::width(&prefix))),
                    Span::styled(more.to_string(), t.text()),
                ],
                ITEM + text::width(&prefix),
            ));
        }
    }

    section(cx, &mut out, "Notes", r.notes.is_empty());
    let skip = r.notes.len().saturating_sub(NOTES);
    for n in r.notes.iter().skip(skip) {
        let day = crate::data::day_part(&n.at).unwrap_or("");
        let first = n.text.lines().next().unwrap_or("");
        let more = if n.text.lines().count() > 1 {
            " ..."
        } else {
            ""
        };
        let prefix = format!("{day} ");
        out.push(Item::new(
            vec![
                Span::raw(" ".repeat(ITEM)),
                Span::styled(prefix.clone(), t.muted()),
                Span::styled(format!("{first}{more}"), t.text()),
            ],
            ITEM + text::width(&prefix),
        ));
    }

    match &r.scorecard {
        None => section_note(cx, &mut out, "Scorecard", "(none; k records one)"),
        Some(s) => {
            section(cx, &mut out, "Scorecard", false);
            let n = |v: Option<i64>| v.map_or("-".to_string(), |v| v.to_string());
            out.push(item(
                format!(
                    "score {}  decisions {}  repeat qs {}  days to preview {}  cleanups {}  \
                     check rejections {}  report reworks {}",
                    n(s.score),
                    n(s.decisions),
                    n(s.repeat_questions),
                    n(s.days_to_preview),
                    n(s.cleanups),
                    n(s.check_rejections),
                    n(s.report_reworks)
                ),
                t.text(),
            ));
            if let Some(note) = &s.note {
                out.push(item(note.clone(), t.muted()));
            }
        }
    }
    out
}

/// A blank line and the section heading; an empty section says so on the
/// heading line instead of taking a row of its own.
fn section(cx: &RenderCx, out: &mut Vec<Item>, title: &str, is_empty: bool) {
    if is_empty {
        section_note(cx, out, title, "(none)");
    } else {
        out.push(Item::new(Vec::new(), 0));
        out.push(Item::new(
            vec![Span::styled(title.to_string(), cx.theme.title())],
            0,
        ));
    }
}

fn section_note(cx: &RenderCx, out: &mut Vec<Item>, title: &str, note: &str) {
    out.push(Item::new(Vec::new(), 0));
    out.push(Item::new(
        vec![
            Span::styled(title.to_string(), cx.theme.title()),
            Span::styled(format!("  {note}"), cx.theme.muted()),
        ],
        ITEM,
    ));
}

fn item(text: String, style: Style) -> Item {
    Item::new(
        vec![Span::raw(" ".repeat(ITEM)), Span::styled(text, style)],
        ITEM,
    )
}

fn kv(cx: &RenderCx, k: &str, v: String, style: Style) -> Item {
    Item::new(
        vec![
            Span::styled(format!("{k} "), cx.theme.muted()),
            Span::styled(v, style),
        ],
        0,
    )
}

/// A JOB.md entry: first line as a `- ` item, continuation lines (and
/// wrapped rows) indented under its text.
fn push_entry(cx: &RenderCx, out: &mut Vec<Item>, entry: &str, style: Style) {
    for (i, l) in entry.lines().enumerate() {
        let lead = if i == 0 { "- " } else { "  " };
        out.push(Item::new(
            vec![
                Span::raw(" ".repeat(ITEM)),
                Span::styled(lead, cx.theme.muted()),
                Span::styled(l.to_string(), style),
            ],
            ENTRY,
        ));
    }
}
