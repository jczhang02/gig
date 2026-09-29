//! Orders view. Filled by a later ticket; see spec section 2.

use crate::ui::RenderCx;
use ratatui::layout::Rect;
use ratatui::Frame;

pub fn render(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    super::placeholder(frame, area, cx, "Orders");
}
