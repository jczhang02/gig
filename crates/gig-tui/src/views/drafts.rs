//! Drafts view (TUI-DESIGN.md section 10.1): open drafts with slug, age,
//! title and material path. `Enter` shows the tail of NOTES.md (a pane at
//! Medium and Wide, a popup at Narrow), `N` new draft, `P` promote.

use super::orders::{GAP, SLUG};
use super::{banded, cell, cell_right, empty, gap, highlighted, marker, window_start};
use crate::data::calendar_days;
use crate::text;
use crate::ui::RenderCx;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const AGE: usize = 4;
/// Narrowest title column.
const MIN_TITLE: usize = 16;
/// Widest material column.
const MAX_MATERIAL: usize = 40;
/// Narrowest material column worth drawing.
const MIN_MATERIAL: usize = 12;

/// Title and material widths for a list `width` cells wide. The title keeps
/// its natural width (at least 16) before material gets any room; material
/// is 0 when dropped (it goes first, section 10.1).
pub fn columns(width: usize, longest_title: usize, longest_material: usize) -> (usize, usize) {
    let rest = width.saturating_sub(2 + SLUG + GAP + AGE + GAP);
    let want = longest_material.min(MAX_MATERIAL);
    let title = longest_title.max(MIN_TITLE).min(rest);
    let room = rest.saturating_sub(title + GAP);
    if want == 0 || room < MIN_MATERIAL.min(want) {
        return (rest, 0);
    }
    let material = want.min(room);
    (rest - GAP - material, material)
}

pub fn render(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let t = cx.theme;
    let drafts = cx.state.draft_list();
    let today = &cx.state.data.today;
    let longest = drafts
        .iter()
        .filter_map(|d| d.material_path.as_deref())
        .map(text::width)
        .max()
        .unwrap_or(0);
    let longest_title = drafts
        .iter()
        .filter_map(|d| d.title.as_deref())
        .map(text::width)
        .max()
        .unwrap_or(0);
    let (title_w, material_w) = columns(usize::from(area.width), longest_title, longest);

    let m = t.muted();
    let mut header = vec![
        gap(2),
        cell("draft", SLUG, m),
        gap(GAP),
        cell_right("age", AGE, m),
        gap(GAP),
        cell("title", title_w, m),
    ];
    if material_w > 0 {
        header.push(gap(GAP));
        header.push(Span::styled("material", m));
    }
    let mut lines = vec![Line::from(header), Line::raw("")];
    if drafts.is_empty() {
        frame.render_widget(Paragraph::new(lines), area);
        let body = Rect {
            y: area.y + 2,
            height: area.height.saturating_sub(2),
            ..area
        };
        let msg: &[&str] = if cx.state.filter().text.is_empty() {
            &["no open drafts", "N new draft"]
        } else {
            &["no draft matches the filter", "Esc clears it"]
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
        area.height.saturating_sub(2),
    );
    for (i, dr) in drafts.iter().enumerate().skip(start) {
        if lines.len() >= usize::from(area.height) {
            break;
        }
        cx.state.hits.add(
            Rect::new(area.x, area.y + lines.len() as u16, area.width, 1),
            crate::mouse::Target::Draft(dr.id),
        );
        let is_sel = i == selected;
        let mut spans: Vec<Span<'static>> = marker(is_sel, t).into();
        let slug_style = if is_sel {
            t.text().add_modifier(Modifier::BOLD)
        } else {
            t.text()
        };
        spans.extend(highlighted(
            &dr.slug,
            SLUG,
            &cx.state.filter().text,
            slug_style,
        ));
        spans.push(gap(GAP));
        spans.push(match calendar_days(&dr.created_at, today) {
            Some(n) => cell_right(&format!("{n}d"), AGE, m),
            None => cell_right(super::DOT, AGE, t.dim()),
        });
        spans.push(gap(GAP));
        spans.push(match dr.title.as_deref() {
            Some(title) => cell(title, title_w, t.text()),
            None => cell(super::DOT, title_w, t.dim()),
        });
        if material_w > 0 {
            if let Some(path) = dr.material_path.as_deref() {
                spans.push(gap(GAP));
                // Paths lose their start: the end names the file.
                spans.push(Span::styled(text::truncate_left(path, material_w), m));
            }
        }
        let line = Line::from(spans);
        lines.push(if is_sel {
            banded(line, area.width, t)
        } else {
            line
        });
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// The NOTES.md tail pane (Medium and Wide `Enter`).
pub fn render_notes(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    let t = cx.theme;
    let Some(pane) = &cx.state.notes_pane else {
        return;
    };
    let mut lines = vec![Line::from(Span::styled("Notes", t.title())), Line::raw("")];
    let width = usize::from(area.width.min(76));
    let mut body: Vec<Line> = pane
        .lines
        .iter()
        .flat_map(|l| text::wrap(l, width))
        .map(|l| Line::from(Span::styled(l, t.text())))
        .collect();
    // The newest lines matter most: keep the end in view.
    let room = usize::from(area.height).saturating_sub(lines.len());
    if body.len() > room {
        body.drain(..body.len() - room);
    }
    lines.extend(body);
    frame.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
mod tests {
    use super::columns;

    #[test]
    fn material_gives_way_to_the_title() {
        // 60 cells for title and material: a 30-cell title and a 26-cell
        // path both fit; the title takes what is left.
        let rest = 2 + 22 + 2 + 4 + 2;
        assert_eq!(columns(rest + 60, 30, 26), (32, 26));
        // Less room: the title keeps its 30 cells, material shrinks.
        assert_eq!(columns(rest + 50, 30, 26), (30, 18));
        // Too little left for material: dropped, the title takes it all.
        assert_eq!(columns(rest + 40, 30, 26), (40, 0));
        // Short titles keep the 16-cell minimum and material the rest.
        assert_eq!(columns(rest + 40, 5, 26), (16, 22));
        assert_eq!(columns(rest + 40, 5, 0), (40, 0));
    }
}
