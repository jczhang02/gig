//! Order detail: right pane at 110+ columns, full screen below. Filled by a
//! later ticket; see spec section 2.1.

use crate::ui::RenderCx;
use ratatui::layout::Rect;
use ratatui::Frame;

pub fn render(frame: &mut Frame, area: Rect, cx: &RenderCx) {
    super::placeholder(frame, area, cx, "Detail");
}
