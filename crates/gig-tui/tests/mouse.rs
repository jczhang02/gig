//! Mouse support against a temporary gig home (TUI-SPEC 8.2): events go
//! through the app after a frame is drawn on a `TestBackend`, and the
//! outcomes that need no terminal are carried out.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use gig_core::config::{Config, Paths, Tui};
use gig_core::db;
use gig_core::services::Ctx;
use gig_tui::app::{App, Outcome, View};
use gig_tui::theme::ColorMode;
use gig_tui::themes::Catalog;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use std::path::Path;
use std::time::{Duration, Instant};
use tempfile::TempDir;

const W: u16 = 120;
const H: u16 = 36;

fn app_in(dir: &Path, settings: &Tui) -> App {
    let paths = Paths::under_root(dir);
    paths.ensure_dirs().unwrap();
    let conn = db::open(&paths.db_file).unwrap();
    let config = Config::load(&paths.config_file).unwrap();
    let catalog = Catalog::load(&paths.themes_dir());
    let (theme, _) = catalog.pick(settings.theme.as_deref());
    let ctx = Ctx {
        paths,
        config,
        conn,
    };
    let mut app = App::new(ctx, settings, theme, catalog, ColorMode::TrueColor);
    app.refresh();
    app
}

/// Draw a frame, returning the screen as text rows.
fn draw(app: &App) -> Vec<String> {
    let mut term = Terminal::new(TestBackend::new(W, H)).unwrap();
    app.draw_on(&mut term).unwrap();
    let buf = term.backend().buffer().clone();
    (0..H)
        .map(|y| (0..W).map(|x| buf[(x, y)].symbol().to_string()).collect())
        .collect()
}

/// Cell of `needle` on the screen (ASCII, one symbol per cell).
fn find(screen: &[String], needle: &str) -> (u16, u16) {
    screen
        .iter()
        .enumerate()
        .find_map(|(y, row)| row.find(needle).map(|x| (x as u16, y as u16)))
        .unwrap_or_else(|| panic!("{needle:?} not on screen:\n{}", screen.join("\n")))
}

/// Draw, click `needle`, then settle what needs no terminal.
fn click(app: &mut App, needle: &str, now: Instant) -> Option<Outcome> {
    let (x, y) = find(&draw(app), needle);
    let ev = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    };
    let outcome = app.ui.handle_mouse(ev, now, W);
    app.settle(outcome)
}

fn press(app: &mut App, code: KeyCode, mods: KeyModifiers) -> Option<Outcome> {
    let outcome = app.ui.handle_key(KeyEvent::new(code, mods), W);
    app.settle(outcome)
}

#[test]
fn m_and_the_settings_row_turn_the_mouse_on_and_off() {
    let dir = TempDir::new().unwrap();
    let mut app = app_in(dir.path(), &Tui::default());
    assert!(app.running.mouse, "on by default");

    // `M`: this session only, with a toast; config.toml is not written.
    assert_eq!(
        press(&mut app, KeyCode::Char('M'), KeyModifiers::SHIFT),
        None
    );
    assert!(!app.running.mouse);
    assert!(!gig_tui::terminal::mouse());
    let toast = app.ui.toast.clone().unwrap().text;
    assert!(toast.starts_with("mouse off"), "{toast}");
    assert!(!app.ctx.paths.config_file.exists());
    press(&mut app, KeyCode::Char('M'), KeyModifiers::SHIFT);
    assert!(app.running.mouse);
    assert!(gig_tui::terminal::mouse());
    assert_eq!(app.ui.toast.clone().unwrap().text, "mouse on");

    // Settings: a double-click on the `tui.mouse` row flips it, writes the
    // file and applies it to the running dashboard.
    press(&mut app, KeyCode::Char(','), KeyModifiers::NONE);
    let t0 = Instant::now();
    assert_eq!(click(&mut app, "tui.mouse", t0), None);
    assert_eq!(
        click(&mut app, "tui.mouse", t0 + Duration::from_millis(150)),
        None
    );
    assert!(!app.running.mouse);
    assert!(!gig_tui::terminal::mouse());
    let file = std::fs::read_to_string(&app.ctx.paths.config_file).unwrap();
    assert!(file.contains("mouse = false"), "{file}");
    assert_eq!(
        app.ui.toast.clone().unwrap().text,
        "saved tui.mouse = false"
    );
}

#[test]
fn clicks_switch_views_and_the_footer_opens_forms() {
    let dir = TempDir::new().unwrap();
    let mut app = app_in(dir.path(), &Tui::default());
    let t0 = Instant::now();
    click(&mut app, "Money", t0);
    assert_eq!(app.ui.view, View::Money);
    click(&mut app, "History", t0);
    assert_eq!(app.ui.view, View::History);
    click(&mut app, "Orders", t0);
    // The empty list's footer: `N new` opens the new order form, as the key.
    let screen = draw(&app);
    let footer = &screen[usize::from(H) - 1];
    assert!(footer.contains("N new"), "{footer}");
    let x = footer.find("N new").unwrap() as u16;
    let at = |x: u16, y: u16| MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    };
    let out = app.ui.handle_mouse(at(x, H - 1), t0, W);
    assert!(matches!(app.settle(out), Some(Outcome::Act(_))));
    assert!(app.ui.popup.is_some());
    // A click outside discards it.
    draw(&app);
    let out = app.ui.handle_mouse(at(0, H - 2), t0, W);
    assert!(matches!(app.settle(out), Some(Outcome::Act(_))));
    assert!(app.ui.popup.is_none());
}
