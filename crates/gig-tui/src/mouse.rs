//! Mouse support (TUI-SPEC 8.2): a hit-test layer filled by every frame.
//!
//! While drawing, each view records what can be clicked (`Target`) and
//! which pane the wheel scrolls (`Pane`) with the screen rectangle it took.
//! Recording never changes what is drawn. A mouse event is then resolved
//! against the last frame: the region recorded last wins, so a popup drawn
//! over a list takes the click. Popups, the help and the theme picker open
//! a modal layer (`Hits::modal`): only what is recorded after it counts,
//! and a click outside its rectangle closes it as Esc would.

use crate::app::View;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;
use std::cell::RefCell;
use std::time::{Duration, Instant};

/// Two clicks on the same target within this time are a double-click.
pub const DOUBLE_CLICK: Duration = Duration::from_millis(400);

/// Rows moved or scrolled per wheel notch in a detail or popup body.
pub const WHEEL_ROWS: u16 = 3;

/// Something a click acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A banner tab.
    Tab(View),
    /// A key hint (footer, popup buttons): the click presses the key, so it
    /// goes through the same confirmations.
    Key(KeyEvent),
    /// A row of the Orders or History list.
    Order(i64),
    /// A row of the Drafts list.
    Draft(i64),
    /// A row of the Money outstanding table.
    Owed(i64),
    /// A month slot of the Money chart, by its index (0 is the oldest).
    Month(usize),
    /// A row of the month drill-down under the chart.
    Paid(i64),
    /// A short link; the full URL is copied.
    Link(String),
    /// A Settings row, by schema index.
    Setting(usize),
    /// A theme picker row.
    Theme(usize),
    /// A form field, by index.
    Field(usize),
    /// An item of a pick list.
    Item(usize),
}

/// What the wheel scrolls under the pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    /// The list of the current view (moves its selection).
    List,
    /// The order detail, pane or full screen.
    Detail,
    /// The Settings overlay (moves its cursor).
    Settings,
    /// The help popup.
    Help,
    /// The theme picker (moves its cursor, previewing).
    Picker,
    /// An action popup: its body scrolls, a pick list moves.
    Popup,
}

/// Regions recorded by one frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Map {
    pub targets: Vec<(Rect, Target)>,
    pub panes: Vec<(Rect, Pane)>,
    /// The topmost popup, when one is open.
    pub modal: Option<Rect>,
}

/// The hit-test store in `UiState`. Drawing borrows the state immutably,
/// so recording goes through a `RefCell`, as the scroll limits do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hits(RefCell<Map>);

impl Hits {
    /// Forget the last frame (every frame starts here).
    pub fn clear(&self) {
        *self.0.borrow_mut() = Map::default();
    }

    /// Record a clickable region. Empty rectangles are ignored.
    pub fn add(&self, rect: Rect, target: Target) {
        if !rect.is_empty() {
            self.0.borrow_mut().targets.push((rect, target));
        }
    }

    /// Record a wheel pane.
    pub fn pane(&self, rect: Rect, pane: Pane) {
        if !rect.is_empty() {
            self.0.borrow_mut().panes.push((rect, pane));
        }
    }

    /// A popup was drawn at `rect`: everything below it stops counting.
    pub fn modal(&self, rect: Rect) {
        *self.0.borrow_mut() = Map {
            modal: Some(rect),
            ..Map::default()
        };
    }

    /// A copy of what the last frame recorded.
    pub fn map(&self) -> Map {
        self.0.borrow().clone()
    }

    /// Key hints drawn as `line` at `(x, y)`: one region per pair.
    pub fn hints(&self, x: u16, y: u16, line: &Line, right: u16) {
        for (dx, w, key) in hint_spans(line) {
            let Some(key) = key_of(&key) else { continue };
            let x0 = x.saturating_add(dx);
            if x0 >= right {
                break;
            }
            let w = w.min(right - x0);
            self.add(Rect::new(x0, y, w, 1), Target::Key(key));
        }
    }
}

impl Map {
    /// The click target under `(x, y)`: the last one recorded there.
    pub fn target_at(&self, x: u16, y: u16) -> Option<&Target> {
        let p = Position::new(x, y);
        self.targets
            .iter()
            .rev()
            .find(|(r, _)| r.contains(p))
            .map(|(_, t)| t)
    }

    /// The wheel pane under `(x, y)`: the last one recorded there.
    pub fn pane_at(&self, x: u16, y: u16) -> Option<Pane> {
        let p = Position::new(x, y);
        self.panes
            .iter()
            .rev()
            .find(|(r, _)| r.contains(p))
            .map(|(_, t)| *t)
    }

    /// True when a popup is open and `(x, y)` is outside it.
    pub fn outside_modal(&self, x: u16, y: u16) -> bool {
        self.modal.is_some_and(|m| !m.contains(Position::new(x, y)))
    }
}

/// The key a hint names: `Enter`, `Esc`, `Tab`, `Space`, or one character.
/// Compound hints (`1-4`, `+ -`, `↑↓`) are not buttons.
pub fn key_of(hint: &str) -> Option<KeyEvent> {
    let code = match hint {
        "Enter" => KeyCode::Enter,
        "Esc" => KeyCode::Esc,
        "Tab" => KeyCode::Tab,
        "Space" => KeyCode::Char(' '),
        _ => {
            let mut chars = hint.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            KeyCode::Char(c)
        }
    };
    let mods = match code {
        KeyCode::Char(c) if c.is_ascii_uppercase() => KeyModifiers::SHIFT,
        _ => KeyModifiers::NONE,
    };
    Some(KeyEvent::new(code, mods))
}

/// Pairs of a hint line (`key` span, then its ` label` span): the offset
/// from the line start, the width of key and label, and the key text.
/// Other spans (gaps, the group dot) only advance the offset.
fn hint_spans(line: &Line) -> Vec<(u16, u16, String)> {
    let mut out = Vec::new();
    let mut x = 0u16;
    let spans = &line.spans;
    let mut i = 0;
    while i < spans.len() {
        let s = &spans[i];
        let w = s.width() as u16;
        let label = spans.get(i + 1).filter(|n| n.content.starts_with(' '));
        let is_key = !s.content.trim().is_empty()
            && !s.content.starts_with(' ')
            && s.content != "\u{b7}"
            && label.is_some_and(|l| !l.content.trim().is_empty());
        if let (true, Some(l)) = (is_key, label) {
            let lw = l.width() as u16;
            out.push((x, w + lw, s.content.to_string()));
            x = x.saturating_add(w + lw);
            i += 2;
        } else {
            x = x.saturating_add(w);
            i += 1;
        }
    }
    out
}

/// Offset and width of the first span of `line` drawn in `style` (links
/// are the only spans in the link style).
pub fn span_x(line: &Line, style: ratatui::style::Style) -> Option<(u16, u16)> {
    let mut x = 0u16;
    for s in &line.spans {
        let w = s.width() as u16;
        if s.style == style && w > 0 {
            return Some((x, w));
        }
        x = x.saturating_add(w);
    }
    None
}

/// The last click, to spot a double-click.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Click {
    pub at: Instant,
    pub target: Target,
}

/// True when a click on `target` at `now` completes a double-click on the
/// same target.
pub fn is_double(last: Option<&Click>, target: &Target, now: Instant) -> bool {
    last.is_some_and(|c| {
        c.target == *target
            && now
                .checked_duration_since(c.at)
                .is_some_and(|d| d <= DOUBLE_CLICK)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::text::Span;

    #[test]
    fn last_recorded_region_wins() {
        let hits = Hits::default();
        hits.add(Rect::new(0, 5, 80, 1), Target::Order(1));
        hits.add(Rect::new(10, 5, 20, 1), Target::Link("https://x".into()));
        hits.pane(Rect::new(0, 2, 80, 20), Pane::List);
        let m = hits.map();
        assert_eq!(m.target_at(0, 5), Some(&Target::Order(1)));
        assert_eq!(m.target_at(10, 5), Some(&Target::Link("https://x".into())));
        assert_eq!(m.target_at(29, 5), Some(&Target::Link("https://x".into())));
        assert_eq!(m.target_at(30, 5), Some(&Target::Order(1)));
        assert_eq!(m.target_at(80, 5), None);
        assert_eq!(m.target_at(0, 6), None);
        assert_eq!(m.pane_at(40, 21), Some(Pane::List));
        assert_eq!(m.pane_at(40, 22), None);
        assert!(!m.outside_modal(0, 0));
        // Empty regions never match.
        hits.add(Rect::new(0, 0, 0, 1), Target::Order(9));
        assert_eq!(hits.map().target_at(0, 0), None);
    }

    #[test]
    fn a_modal_hides_what_is_below_it() {
        let hits = Hits::default();
        hits.add(Rect::new(0, 5, 80, 1), Target::Order(1));
        hits.pane(Rect::new(0, 2, 80, 20), Pane::List);
        hits.modal(Rect::new(10, 3, 40, 10));
        hits.add(Rect::new(12, 5, 20, 1), Target::Field(0));
        let m = hits.map();
        assert_eq!(m.target_at(0, 5), None, "the list is under the popup");
        assert_eq!(m.target_at(12, 5), Some(&Target::Field(0)));
        assert_eq!(m.pane_at(0, 5), None);
        assert!(m.outside_modal(0, 5));
        assert!(m.outside_modal(50, 5));
        assert!(!m.outside_modal(10, 3));
        assert!(!m.outside_modal(49, 12));
        hits.clear();
        assert_eq!(hits.map(), Map::default());
    }

    #[test]
    fn hint_pairs_become_key_buttons() {
        // ` p paid  y copy link   ·   ? keys` drawn from x = 0.
        let line = Line::from(vec![
            Span::raw(" "),
            Span::raw("p"),
            Span::raw(" paid"),
            Span::raw("  "),
            Span::raw("y"),
            Span::raw(" copy link"),
            Span::raw("   "),
            Span::raw("\u{b7}"),
            Span::raw("   "),
            Span::raw("?"),
            Span::raw(" keys"),
            Span::raw("  "),
            Span::raw("1-4"),
            Span::raw(" views"),
            Span::raw("  "),
            Span::raw("Esc"),
            Span::raw(" close"),
        ]);
        let hits = Hits::default();
        hits.hints(0, 23, &line, 80);
        let m = hits.map();
        let key = |x| m.target_at(x, 23).cloned();
        let press = |c| {
            Some(Target::Key(KeyEvent::new(
                KeyCode::Char(c),
                KeyModifiers::NONE,
            )))
        };
        assert_eq!(key(0), None);
        assert_eq!(key(1), press('p'));
        assert_eq!(key(6), press('p'));
        assert_eq!(key(7), None, "the gap between pairs");
        assert_eq!(key(9), press('y'));
        assert_eq!(key(20), None);
        assert_eq!(key(26), None, "the group dot");
        assert_eq!(key(27), press('?'));
        // `1-4` is not a button; `Esc` is.
        let one_to_four = 27 + 6 + 2;
        assert_eq!(key(one_to_four), None);
        let esc = one_to_four + 9 + 2;
        assert_eq!(
            key(esc),
            Some(Target::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
        );
        // Clipped at the right edge.
        let hits = Hits::default();
        hits.hints(0, 0, &line, 4);
        assert_eq!(hits.map().targets.len(), 1);
        assert_eq!(hits.map().targets[0].0.width, 3);
    }

    #[test]
    fn keys_of_hints() {
        assert_eq!(key_of("Enter").unwrap().code, KeyCode::Enter);
        assert_eq!(key_of("Space").unwrap().code, KeyCode::Char(' '));
        assert_eq!(key_of("N").unwrap().modifiers, KeyModifiers::SHIFT);
        assert_eq!(key_of("$").unwrap().code, KeyCode::Char('$'));
        for compound in ["1-4", "+ -", "\u{2191}\u{2193}", ""] {
            assert_eq!(key_of(compound), None, "{compound}");
        }
    }

    #[test]
    fn double_click_timing() {
        let t0 = Instant::now();
        let order = Target::Order(3);
        let last = Click {
            at: t0,
            target: order.clone(),
        };
        assert!(!is_double(None, &order, t0));
        assert!(is_double(
            Some(&last),
            &order,
            t0 + Duration::from_millis(250)
        ));
        assert!(is_double(Some(&last), &order, t0 + DOUBLE_CLICK));
        assert!(!is_double(
            Some(&last),
            &order,
            t0 + DOUBLE_CLICK + Duration::from_millis(1)
        ));
        assert!(!is_double(
            Some(&last),
            &Target::Order(4),
            t0 + Duration::from_millis(100)
        ));
        // A clock that went backwards is not a double-click.
        let later = Click {
            at: t0 + Duration::from_secs(1),
            target: order.clone(),
        };
        assert!(!is_double(Some(&later), &order, t0));
    }

    #[test]
    fn link_span_offset() {
        let link = ratatui::style::Style::new().add_modifier(ratatui::style::Modifier::UNDERLINED);
        let line = Line::from(vec![
            Span::raw("  "),
            Span::raw("full \u{b7} sent"),
            Span::raw("  "),
            Span::styled("go.jczhang.cc/a30bd870", link),
        ]);
        assert_eq!(span_x(&line, link), Some((2 + 11 + 2, 22)));
        assert_eq!(span_x(&Line::raw("no link"), link), None);
    }
}

/// Mouse events through the app state: each helper draws a real frame on a
/// `TestBackend` first, so the regions are the ones the renderer recorded.
#[cfg(test)]
mod events {
    use crate::actions::Effect;
    use crate::app::{Outcome, UiState, View};
    use crate::data::tests::sample_snapshot;
    use crate::icons::Icons;
    use crate::popup::Popup;
    use crate::theme::Theme;
    use crate::ui::{self, RenderCx};
    use crossterm::event::MouseEventKind;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent};
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::Terminal;
    use std::time::{Duration, Instant};

    fn draw(state: &UiState, w: u16, h: u16) -> Buffer {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        let icons = Icons::new(true);
        term.draw(|f| {
            ui::draw(
                f,
                &RenderCx {
                    state,
                    theme: &Theme::DARK,
                    icons: &icons,
                },
            )
        })
        .unwrap();
        term.backend().buffer().clone()
    }

    /// Cell of the first match of an ASCII `needle`, searching rows from
    /// `from_y` down.
    fn find_from(buf: &Buffer, needle: &str, from_y: u16) -> Option<(u16, u16)> {
        let n: Vec<char> = needle.chars().collect();
        for y in from_y..buf.area.height {
            for x in 0..buf.area.width.saturating_sub(n.len() as u16 - 1) {
                let hit = n.iter().enumerate().all(|(k, c)| {
                    let mut s = [0u8; 4];
                    buf[(x + k as u16, y)].symbol() == c.encode_utf8(&mut s)
                });
                if hit {
                    return Some((x, y));
                }
            }
        }
        None
    }

    fn find(buf: &Buffer, needle: &str) -> (u16, u16) {
        find_from(buf, needle, 0).unwrap_or_else(|| panic!("{needle:?} not on screen"))
    }

    fn event(kind: MouseEventKind, x: u16, y: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        }
    }

    const LEFT: MouseEventKind = MouseEventKind::Down(MouseButton::Left);

    /// Draw at `w`x`h`, then click at `(x, y)` at time `now`.
    fn click_xy(s: &mut UiState, w: u16, h: u16, x: u16, y: u16, now: Instant) -> Outcome {
        draw(s, w, h);
        s.handle_mouse(event(LEFT, x, y), now, w)
    }

    /// Draw, then click on the first cell of `needle` at or below `from_y`.
    fn click_on(
        s: &mut UiState,
        w: u16,
        h: u16,
        needle: &str,
        from_y: u16,
        now: Instant,
    ) -> Outcome {
        let buf = draw(s, w, h);
        let (x, y) =
            find_from(&buf, needle, from_y).unwrap_or_else(|| panic!("{needle:?} not on screen"));
        s.handle_mouse(event(LEFT, x, y), now, w)
    }

    fn wheel_at(s: &mut UiState, w: u16, h: u16, x: u16, y: u16, down: bool) -> Outcome {
        draw(s, w, h);
        let kind = if down {
            MouseEventKind::ScrollDown
        } else {
            MouseEventKind::ScrollUp
        };
        s.handle_mouse(event(kind, x, y), Instant::now(), w)
    }

    fn state() -> UiState {
        UiState {
            data: sample_snapshot("2026-09-29"),
            ..UiState::default()
        }
    }

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn row_click_selects_and_double_click_opens_the_detail() {
        let t0 = Instant::now();
        let mut s = state();
        assert_eq!(s.selected_order().unwrap().order.id, 7);
        click_on(&mut s, 120, 36, " o6  ", 2, t0);
        assert_eq!(s.selected, Some(6));
        assert!(!s.detail_open);
        // Slower than a double-click: still one click each.
        click_on(&mut s, 120, 36, " o6  ", 2, t0 + MS(1000));
        assert!(!s.detail_open);
        // Quick second click on the same row: the detail opens.
        click_on(&mut s, 120, 36, " o6  ", 2, t0 + MS(1200));
        assert!(s.detail_open);
        assert_eq!(s.selected_order().unwrap().order.id, 6);
        // Two quick clicks on different rows are two selections.
        let mut s = state();
        click_on(&mut s, 80, 24, " o6  ", 2, t0);
        click_on(&mut s, 80, 24, " o10  ", 2, t0 + MS(100));
        assert_eq!(s.selected, Some(10));
        assert!(!s.detail_open);
        // The second line of the selected row belongs to it too.
        let buf = draw(&s, 80, 24);
        let (_, y) = find(&buf, " o10  ");
        click_xy(&mut s, 80, 24, 40, y + 1, t0 + MS(900));
        assert_eq!(s.selected, Some(10));
        // Header and group headings select nothing.
        click_on(&mut s, 80, 24, "owed", 2, t0 + MS(2000));
        assert_eq!(s.selected, Some(10));
    }

    #[test]
    fn history_double_click_opens_the_detail() {
        let t0 = Instant::now();
        let mut s = state();
        s.view = View::History;
        click_on(&mut s, 200, 50, " o8  ", 2, t0);
        assert_eq!(s.selected, Some(8));
        assert!(!s.detail_open);
        click_on(&mut s, 200, 50, " o8  ", 2, t0 + MS(300));
        assert!(s.detail_open);
        assert_eq!(s.selected_order().unwrap().order.id, 8);
    }

    #[test]
    fn banner_tabs_switch_views() {
        let t0 = Instant::now();
        let mut s = state();
        for (word, view) in [
            ("Money", View::Money),
            ("History", View::History),
            ("Drafts", View::Drafts),
            ("Orders", View::Orders),
        ] {
            click_on(&mut s, 120, 36, word, 0, t0);
            assert_eq!(s.view, view, "{word}");
        }
        // The tab number works as well, and a tab closes Settings.
        s.settings = Some(settings());
        let buf = draw(&s, 120, 36);
        let (x, _) = find(&buf, "3 Money");
        click_xy(&mut s, 120, 36, x, 0, t0);
        assert_eq!(s.view, View::Money);
        assert!(s.settings.is_none());
        // Compact tabs (only the active word) still map to their views.
        let buf = draw(&s, 60, 16);
        let (x, _) = find(&buf, "4");
        click_xy(&mut s, 60, 16, x, 0, t0);
        assert_eq!(s.view, View::History);
        // `gig` and the money cluster are not tabs.
        click_on(&mut s, 120, 36, "gig", 0, t0);
        click_on(&mut s, 120, 36, "owed", 0, t0);
        assert_eq!(s.view, View::History);
    }

    #[test]
    fn footer_hints_press_their_keys_with_the_same_confirmations() {
        let t0 = Instant::now();
        let mut s = state();
        s.selected = Some(1); // queued: s start  $ price  n note  x cancel
                              // `x cancel` opens the cancel form, exactly as the key does.
        let out = click_on(&mut s, 200, 50, "x cancel", 49, t0);
        assert_eq!(out, Outcome::Act(Effect::None));
        assert!(matches!(&s.popup, Some(Popup::Form(f)) if f.title.contains("cancel")));
        // A click outside the popup discards the form, as Esc does.
        click_xy(&mut s, 200, 50, 0, 40, t0);
        assert_eq!(s.popup, None);
        // `? keys` opens help; a click outside closes it.
        click_on(&mut s, 200, 50, "? keys", 49, t0);
        assert!(s.help_open);
        click_xy(&mut s, 200, 50, 0, 40, t0);
        assert!(!s.help_open);
        // `a archived` toggles closed orders; `q quit` quits.
        click_on(&mut s, 200, 50, "a archived", 49, t0);
        assert!(s.show_closed);
        assert_eq!(click_on(&mut s, 200, 50, "q quit", 49, t0), Outcome::Quit);
        // Gaps and the group dot are not buttons.
        let buf = draw(&s, 200, 50);
        let (x, y) = find_from(&buf, "   \u{b7}   ", 49).unwrap();
        assert_eq!(click_xy(&mut s, 200, 50, x + 3, y, t0), Outcome::None);
    }

    #[test]
    fn popup_buttons_act_as_their_keys() {
        let t0 = Instant::now();
        let mut s = state();
        let confirm = || {
            Popup::confirm(
                "upload",
                vec!["kind: full".into(), "Upload it?".into()],
                Effect::Refresh,
            )
        };
        s.popup = Some(confirm());
        // `y yes` confirms and carries the effect.
        assert_eq!(
            click_on(&mut s, 120, 36, "y yes", 2, t0),
            Outcome::Act(Effect::Refresh)
        );
        assert_eq!(s.popup, None);
        // `Esc no` cancels.
        s.popup = Some(confirm());
        click_on(&mut s, 120, 36, "Esc no", 2, t0);
        assert_eq!(s.popup, None);
        // Clicks inside the popup but on nothing keep it open.
        s.popup = Some(confirm());
        click_on(&mut s, 120, 36, "Upload it?", 2, t0);
        assert!(s.popup.is_some());
        // Outside: cancelled, the effect never runs.
        assert_eq!(
            click_xy(&mut s, 120, 36, 1, 30, t0),
            Outcome::Act(Effect::None)
        );
        assert_eq!(s.popup, None);
    }

    #[test]
    fn form_fields_focus_and_work_on_click() {
        let t0 = Instant::now();
        let mut s = state();
        s.handle_key(KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT), 120);
        let focus = |s: &UiState| match &s.popup {
            Some(Popup::Form(f)) => (f.focus, f.fields[f.focus].label.clone()),
            _ => panic!("no form"),
        };
        assert_eq!(focus(&s).1, "slug");
        // Labels inside the popup (the list header also says `title`).
        let top = find(&draw(&s, 120, 36), "new order").1;
        click_on(&mut s, 120, 36, "title", top, t0);
        assert_eq!(focus(&s).1, "title");
        // A toggle flips on the second click (the first focuses it).
        let on = |s: &UiState| match &s.popup {
            Some(Popup::Form(f)) => f.field("from draft").unwrap().is_on(),
            _ => panic!("no form"),
        };
        let before = on(&s);
        click_on(&mut s, 120, 36, "from draft", top, t0);
        assert_eq!(focus(&s).1, "from draft");
        assert_eq!(on(&s), before);
        click_on(&mut s, 120, 36, "from draft", top, t0 + MS(2000));
        assert_eq!(on(&s), !before);
        // An $EDITOR field opens the editor on the second click.
        click_on(&mut s, 120, 36, "client words", top, t0);
        let i = focus(&s).0;
        let out = click_on(&mut s, 120, 36, "client words", top, t0 + MS(2000));
        assert_eq!(out, Outcome::Act(Effect::EditField(i)));
        // `Esc cancel` in the popup footer discards the form.
        click_on(&mut s, 120, 36, "Esc cancel", 2, t0);
        assert_eq!(s.popup, None);
    }

    #[test]
    fn wheel_scrolls_the_pane_under_the_pointer() {
        let mut s = state();
        // Over the list: the selection moves.
        let buf = draw(&s, 120, 36);
        let (x, y) = find(&buf, " o6  ");
        wheel_at(&mut s, 120, 36, x, y, true);
        assert_eq!(s.selected, Some(6));
        wheel_at(&mut s, 120, 36, x, y, false);
        assert_eq!(s.selected, Some(7));
        // Over the detail pane: it scrolls when it is taller than the pane.
        s.selected = Some(3);
        let notes: String = (1..=5)
            .map(|d| format!("[2026-09-0{d}T10:00:00Z] note {d}\n"))
            .collect();
        for r in &mut s.data.orders {
            r.notes = crate::data::parse_notes(&notes);
        }
        draw(&s, 120, 16);
        let max = s.detail_scroll.max.get();
        assert!(max > 0, "the detail must overflow at 120x16");
        let pane = ui::shell(ratatui::layout::Rect::new(0, 0, 120, 16), &s)
            .pane
            .unwrap();
        wheel_at(&mut s, 120, 16, pane.x + 2, pane.y + 2, true);
        assert_eq!(s.detail_scroll.offset, super::WHEEL_ROWS.min(max));
        assert_eq!(s.selected, Some(3), "the list stays put");
        wheel_at(&mut s, 120, 16, pane.x + 2, pane.y + 2, false);
        assert_eq!(s.detail_scroll.offset, 0);
        // Over the banner or footer: nothing.
        wheel_at(&mut s, 120, 16, 5, 0, true);
        wheel_at(&mut s, 120, 16, 5, 15, true);
        assert_eq!(s.selected, Some(3));
        // Help: its body scrolls; the list under it does not move.
        s.help_open = true;
        wheel_at(&mut s, 60, 16, 30, 8, true);
        assert_eq!(s.help_scroll.offset, super::WHEEL_ROWS);
        wheel_at(&mut s, 60, 16, 0, 8, true);
        assert_eq!(s.help_scroll.offset, super::WHEEL_ROWS);
        assert_eq!(s.selected, Some(3));
    }

    /// Order 8 (archived) paid in August next to order 4 (paid, still in
    /// Orders), and order 3 paid in September.
    fn money() -> UiState {
        let mut s = state();
        s.view = View::Money;
        for r in &mut s.data.orders {
            if r.order.id == 8 {
                r.order.paid_at = Some("2026-08-15".into());
            }
        }
        let orders: Vec<_> = s.data.orders.iter().map(|r| r.order.clone()).collect();
        s.data.money = crate::data::money::Money::compute(&orders, "2026-09-29");
        s
    }

    #[test]
    fn a_bar_click_lists_the_month_and_its_rows_jump() {
        let t0 = Instant::now();
        let mut s = money();
        let before = draw(&s, 120, 36);
        // The month row: `Aug` is slot 10 of 12 (Oct 2025 .. Sep 2026).
        click_on(&mut s, 120, 36, "Aug", 2, t0);
        assert_eq!(s.money_month, Some(10));
        let buf = draw(&s, 120, 36);
        let (_, head) = find(&buf, "Received in August 2026  2");
        // Newest payment first: o8 (08-15), then o4 (08-01).
        let (_, y8) = find_from(&buf, " o8  ", head).unwrap();
        let (_, y4) = find_from(&buf, " o4  ", head).unwrap();
        assert!(head < y8 && y8 < y4);
        assert!(find_from(&buf, "08-15", head).is_some());
        // The picked month label is underlined in the accent.
        let (x, y) = find(&buf, "Aug");
        assert!(buf[(x, y)]
            .modifier
            .contains(ratatui::style::Modifier::UNDERLINED));
        assert_eq!(buf[(x, y)].underline_color, Theme::DARK.accent);
        // The outstanding table is still there, below.
        assert!(find_from(&buf, "Outstanding", y4).is_some());
        // Esc (or the footer hint) closes the month.
        assert!(find_from(&buf, "Esc close month", 35).is_some());
        s.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), 120);
        assert_eq!(s.money_month, None);
        assert_eq!(draw(&s, 120, 36), before, "closing restores the view");
        // The same bar twice: open, then closed.
        let (_, months_y) = find(&before, "Aug");
        let (x, y) = find_from(&before, "Sep", months_y).unwrap();
        click_xy(&mut s, 120, 36, x, y - 3, t0);
        assert_eq!(s.money_month, Some(11));
        click_xy(&mut s, 120, 36, x, y - 3, t0 + MS(2000));
        assert_eq!(s.money_month, None);
        // A drill-down row jumps: archived orders to History ...
        click_on(&mut s, 120, 36, "Aug", 2, t0);
        let head = find(&draw(&s, 120, 36), "Received in").1;
        click_on(&mut s, 120, 36, " o8  ", head, t0);
        assert_eq!(s.view, View::History);
        assert_eq!(s.selected_order().unwrap().order.id, 8);
        // ... open ones to Orders.
        s.view = View::Money;
        click_on(&mut s, 120, 36, " o4  ", head, t0);
        assert_eq!(s.view, View::Orders);
        assert_eq!(s.selected_order().unwrap().order.id, 4);
        // A month without payments says so.
        s.view = View::Money;
        click_on(&mut s, 120, 36, "Jan", 2, t0);
        let buf = draw(&s, 120, 36);
        assert!(find_from(&buf, "no payments in January 2026", 2).is_some());
        // Small terminals: the drill-down and the table share the rest.
        for (w, h) in [(80, 24), (60, 16)] {
            let mut s = money();
            s.money_month = Some(10);
            draw(&s, w, h);
        }
    }

    #[test]
    fn an_outstanding_row_jumps_to_the_order_in_orders() {
        let t0 = Instant::now();
        let mut s = money();
        s.filters[0].text = "o3".into(); // would hide o6 in Orders
        let head = find(&draw(&s, 120, 36), "Outstanding").1;
        click_on(&mut s, 120, 36, " o6  ", head, t0);
        assert_eq!(s.view, View::Orders);
        assert_eq!(s.selected_order().unwrap().order.id, 6);
        assert_eq!(s.filters[0].text, "", "the filter that hid it is cleared");
    }

    #[test]
    fn a_link_click_copies_the_full_url() {
        let t0 = Instant::now();
        let mut s = state();
        for r in &mut s.data.orders {
            if r.order.id == 6 {
                r.packages = vec![gig_core::models::Package {
                    id: 1,
                    order_id: 6,
                    package_id: "o6-delivery-2026-09-28".into(),
                    kind: gig_core::models::PackageKind::Full,
                    dir: String::new(),
                    manifest_path: String::new(),
                    zip_path: String::new(),
                    zip_sha256: None,
                    file_count: Some(3),
                    status: gig_core::models::PackageStatus::Sent,
                    checked_at: None,
                    sent_at: Some("2026-09-28T10:00:00Z".into()),
                    channel: Some(gig_core::models::Channel::Oss),
                    uploader: None,
                    remote_url: None,
                    short_url: Some("https://go.jczhang.cc/a30bd870".into()),
                    expires_at: None,
                    created_at: "2026-09-28T10:00:00Z".into(),
                    updated_at: "2026-09-28T10:00:00Z".into(),
                }];
            }
        }
        s.selected = Some(6);
        let copy = Outcome::Act(Effect::Copy("https://go.jczhang.cc/a30bd870".into()));
        // In the list's second line and in the detail pane.
        let buf = draw(&s, 120, 36);
        let (x1, y1) = find(&buf, "go.jczhang.cc");
        assert!(x1 < ui::split(120).0, "the first one is in the list");
        assert_eq!(click_xy(&mut s, 120, 36, x1 + 5, y1, t0), copy);
        let (x2, y2) = find_from(&buf, "go.jczhang.cc", y1 + 1)
            .or_else(|| {
                // Same row: search right of the list.
                (0..36).find_map(|y| {
                    let (x, _) = find_from(&buf, "go.jczhang.cc", y)?;
                    (x > x1).then_some((x, y))
                })
            })
            .unwrap();
        assert_eq!(click_xy(&mut s, 120, 36, x2, y2, t0), copy);
        // Next to the link, the row just selects.
        assert_eq!(click_xy(&mut s, 120, 36, x1 - 3, y1, t0), Outcome::None);
    }

    fn settings() -> crate::settings::Settings {
        let cfg = gig_core::config::Config::default();
        crate::settings::Settings::new(Ok(cfg.clone()), &cfg, std::path::Path::new("/tmp/c.toml"))
    }

    #[test]
    fn settings_rows_select_and_double_click_works_them() {
        let t0 = Instant::now();
        let mut s = state();
        s.settings = Some(settings());
        let icons = crate::settings::Settings::index("tui.icons").unwrap();
        click_on(&mut s, 120, 36, "tui.icons", 2, t0);
        assert_eq!(s.settings.as_ref().unwrap().cursor, icons);
        let out = click_on(&mut s, 120, 36, "tui.icons", 2, t0 + MS(200));
        assert_eq!(
            out,
            Outcome::WriteSetting {
                key: "tui.icons",
                raw: "false".into()
            }
        );
        // The help line of a row is that row too.
        click_on(&mut s, 120, 36, "Auto-refresh", 2, t0 + MS(5000));
        let refresh = crate::settings::Settings::index("tui.refresh_seconds").unwrap();
        assert_eq!(s.settings.as_ref().unwrap().cursor, refresh);
        // Double-click on a number row starts typing; a click elsewhere
        // cancels the typing.
        click_on(&mut s, 120, 36, "Auto-refresh", 2, t0 + MS(5100));
        assert!(s.settings.as_ref().unwrap().edit.is_some());
        click_on(&mut s, 120, 36, "tui.icons", 2, t0 + MS(9000));
        let st = s.settings.as_ref().unwrap();
        assert!(st.edit.is_none());
        assert_eq!(st.cursor, icons);
        // The wheel moves the cursor.
        wheel_at(&mut s, 120, 36, 20, 10, true);
        assert_eq!(s.settings.as_ref().unwrap().cursor, icons + 1);
        // The theme row opens the picker on a double-click.
        click_on(&mut s, 120, 36, "tui.theme", 2, t0);
        assert_eq!(
            click_on(&mut s, 120, 36, "tui.theme", 2, t0 + MS(100)),
            Outcome::OpenPicker
        );
    }

    #[test]
    fn picker_rows_preview_keep_and_outside_restores() {
        let t0 = Instant::now();
        let dir = tempfile::tempdir().unwrap();
        let catalog = crate::themes::Catalog::load(dir.path());
        let mut s = state();
        s.picker = Some(crate::picker::Picker::new(
            &catalog,
            "gig-dark",
            crate::theme::ColorMode::TrueColor,
        ));
        click_on(&mut s, 120, 36, "nord", 2, t0);
        assert_eq!(s.picker.as_ref().unwrap().highlighted(), Some("nord"));
        let out = click_on(&mut s, 120, 36, "nord", 2, t0 + MS(100));
        assert_eq!(
            out,
            Outcome::WriteSetting {
                key: "tui.theme",
                raw: "nord".into()
            }
        );
        assert!(s.picker.is_none());
        // Outside: closed, nothing written.
        s.picker = Some(crate::picker::Picker::new(
            &catalog,
            "gig-dark",
            crate::theme::ColorMode::TrueColor,
        ));
        click_on(&mut s, 120, 36, "dracula", 2, t0);
        assert_eq!(click_xy(&mut s, 120, 36, 0, 35, t0), Outcome::None);
        assert!(s.picker.is_none());
    }

    #[test]
    fn m_toggles_the_mouse_except_while_typing() {
        let mut s = state();
        let m = KeyEvent::new(KeyCode::Char('M'), KeyModifiers::SHIFT);
        assert_eq!(s.handle_key(m, 120), Outcome::ToggleMouse);
        s.filters[0].editing = true;
        assert_eq!(s.handle_key(m, 120), Outcome::None);
        assert_eq!(s.filters[0].text, "M");
        s.filters[0].editing = false;
        s.settings = Some(settings());
        assert_eq!(s.handle_key(m, 120), Outcome::ToggleMouse);
        s.popup = Some(Popup::message("done", vec![]));
        assert_ne!(s.handle_key(m, 120), Outcome::ToggleMouse);
    }

    #[test]
    fn other_buttons_and_motion_do_nothing() {
        let mut s = state();
        draw(&s, 120, 36);
        let before = s.clone();
        for kind in [
            MouseEventKind::Moved,
            MouseEventKind::Up(MouseButton::Left),
            MouseEventKind::Down(MouseButton::Right),
            MouseEventKind::Drag(MouseButton::Left),
        ] {
            assert_eq!(
                s.handle_mouse(event(kind, 10, 8), Instant::now(), 120),
                Outcome::None
            );
        }
        assert_eq!(s, before);
        // A click on empty space forgets the last click.
        click_on(&mut s, 120, 36, " o6  ", 2, Instant::now());
        assert!(s.last_click.is_some());
        click_xy(&mut s, 120, 36, 119, 1, Instant::now());
        assert!(s.last_click.is_none());
    }
}
