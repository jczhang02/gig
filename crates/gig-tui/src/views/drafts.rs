//! Drafts view (spec 2.2): open drafts with slug, title, material path and
//! age. `Enter` shows the tail of NOTES.md, `N` new draft, `P` promote.

use super::{banded, cell, cell_right, empty, window_start};
use crate::data::calendar_days;
use crate::text;
use crate::ui::RenderCx;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const AGE: usize = 5;

pub fn render(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let t = cx.theme;
    let drafts = cx.state.draft_list();
    let today = &cx.state.data.today;
    let slug = drafts
        .iter()
        .map(|d| text::width(&d.slug))
        .max()
        .unwrap_or(4)
        .clamp(4, 20);
    // " " slug _ age _ title _ material
    let rest = usize::from(area.width).saturating_sub(1 + slug + 1 + AGE + 1 + 1);
    let title_w = rest * 55 / 100;
    let material_w = rest.saturating_sub(title_w);

    let d = t.muted();
    let mut lines = vec![Line::from(vec![
        Span::raw(" "),
        cell("slug", slug, d),
        Span::raw(" "),
        cell_right("age", AGE, d),
        Span::raw(" "),
        cell("title", title_w, d),
        Span::raw(" "),
        cell("material", material_w, d),
    ])];
    if drafts.is_empty() {
        frame.render_widget(Paragraph::new(lines), area);
        let body = Rect {
            y: area.y + 1,
            height: area.height.saturating_sub(1),
            ..area
        };
        let msg: &[&str] = if cx.state.filter().text.is_empty() {
            &["no open drafts", "N records one"]
        } else {
            &["no draft matches the filter"]
        };
        empty(frame, body, t, msg);
        return;
    }
    let selected_id = cx.state.selected_draft().map(|d| d.id);
    let selected = drafts
        .iter()
        .position(|d| Some(d.id) == selected_id)
        .unwrap_or(0);
    let start = window_start(
        &vec![1; drafts.len()],
        selected,
        area.height.saturating_sub(1),
    );
    for (i, dr) in drafts.iter().enumerate().skip(start) {
        let age = calendar_days(&dr.created_at, today).map_or("-".to_string(), |n| format!("{n}d"));
        let line = Line::from(vec![
            Span::raw(" "),
            cell(&dr.slug, slug, t.text().add_modifier(Modifier::BOLD)),
            Span::raw(" "),
            cell_right(&age, AGE, t.muted()),
            Span::raw(" "),
            cell(dr.title.as_deref().unwrap_or("-"), title_w, t.text()),
            Span::raw(" "),
            // Paths lose their start: the end names the folder.
            cell(
                &text::truncate_left(dr.material_path.as_deref().unwrap_or("-"), material_w),
                material_w,
                t.muted(),
            ),
        ]);
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
