//! One module per view. Each exposes `render(frame, area, cx)`.

pub mod detail;
pub mod drafts;
pub mod history;
pub mod money;
pub mod orders;

use crate::app::View;
use crate::ui::RenderCx;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// Placeholder body used until a view ticket fills its module.
pub(crate) fn placeholder(frame: &mut Frame, area: Rect, cx: &RenderCx, title: &str) {
    let lines = vec![
        Line::from(Span::styled(title.to_string(), cx.theme.title())),
        Line::from(Span::styled("(not implemented yet)", cx.theme.dim())),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

/// Draw the body of `view` into `area`.
pub fn render(frame: &mut Frame, area: Rect, view: View, cx: &RenderCx) {
    match view {
        View::Orders => orders::render(frame, area, cx),
        View::Drafts => drafts::render(frame, area, cx),
        View::Money => money::render(frame, area, cx),
        View::History => history::render(frame, area, cx),
    }
}
