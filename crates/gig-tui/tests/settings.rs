//! Settings and theme picker behaviour against a temporary gig home
//! (TUI-SPEC 8.1): every accepted change is written to config.toml in place
//! through gig-core and applied to the running dashboard; refusals write
//! nothing; theme files are reloaded when they change.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use gig_core::config::{Config, Paths, Tui};
use gig_core::db;
use gig_core::services::Ctx;
use gig_tui::app::{App, Outcome};
use gig_tui::theme::{ColorMode, Theme};
use gig_tui::themes::{self, Catalog};
use std::path::Path;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

/// A gig home laid out as `GIG_HOME=<dir>` gives it.
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

/// One key through the app's routing, then the outcomes that need no
/// terminal. Returns what is left (quit, editor, actions).
fn press(app: &mut App, code: KeyCode) -> Option<Outcome> {
    let outcome = app
        .ui
        .handle_key(KeyEvent::new(code, KeyModifiers::NONE), 160);
    app.settle(outcome)
}

fn type_str(app: &mut App, s: &str) {
    for c in s.chars() {
        press(app, KeyCode::Char(c));
    }
}

/// Move the Settings cursor to `key`.
fn go_to(app: &mut App, key: &str) {
    press(app, KeyCode::Home);
    let at = gig_tui::settings::Settings::index(key).unwrap();
    for _ in 0..at {
        press(app, KeyCode::Down);
    }
    assert_eq!(app.ui.settings.as_ref().unwrap().entry().key, key);
}

const COMMENTED: &str = "\
# my gig config
[general]
warranty_days = 15 # two weeks

[tui]
# icons need a Nerd Font
icons = true
refresh_seconds = 2
";

#[test]
fn toggling_icons_writes_the_file_and_keeps_comments() {
    let dir = TempDir::new().unwrap();
    let paths = Paths::under_root(dir.path());
    paths.ensure_dirs().unwrap();
    std::fs::write(&paths.config_file, COMMENTED).unwrap();
    let mut app = app_in(dir.path(), &Tui::default());
    assert!(app.icons.enabled);

    assert_eq!(press(&mut app, KeyCode::Char(',')), None);
    assert!(app.ui.settings.is_some());
    go_to(&mut app, "tui.icons");
    press(&mut app, KeyCode::Char(' '));

    let text = std::fs::read_to_string(&paths.config_file).unwrap();
    assert_eq!(text, COMMENTED.replace("icons = true", "icons = false"));
    // Applied at once, and the row shows the new value.
    assert!(!app.icons.enabled);
    assert!(!app.running.icons);
    let s = app.ui.settings.as_ref().unwrap();
    assert_eq!(s.values[s.cursor], "false");
    assert!(app.ui.toast.as_ref().unwrap().text.contains("tui.icons"));

    // Back on: Enter works too.
    press(&mut app, KeyCode::Enter);
    let text = std::fs::read_to_string(&paths.config_file).unwrap();
    assert_eq!(text, COMMENTED);
    assert!(app.icons.enabled);

    // A general key updates the config the services use.
    go_to(&mut app, "general.warranty_days");
    press(&mut app, KeyCode::Char('+'));
    assert_eq!(app.ctx.config.general.warranty_days, 16);
    let text = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(text.contains("warranty_days = 16 # two weeks"), "{text}");

    press(&mut app, KeyCode::Esc);
    assert!(app.ui.settings.is_none());
}

#[test]
fn an_out_of_range_refresh_is_refused_and_not_written() {
    let dir = TempDir::new().unwrap();
    let paths = Paths::under_root(dir.path());
    paths.ensure_dirs().unwrap();
    std::fs::write(&paths.config_file, COMMENTED).unwrap();
    let mut app = app_in(dir.path(), &Tui::default());
    let before = app.refresh_every;
    assert_eq!(before, Some(Duration::from_secs(2)));

    press(&mut app, KeyCode::Char(','));
    go_to(&mut app, "tui.refresh_seconds");
    type_str(&mut app, "61");
    press(&mut app, KeyCode::Enter);

    assert_eq!(
        std::fs::read_to_string(&paths.config_file).unwrap(),
        COMMENTED
    );
    assert_eq!(app.refresh_every, before);
    let s = app.ui.settings.as_ref().unwrap();
    assert_eq!(
        s.errors[s.cursor].as_deref(),
        Some("invalid input: tui.refresh_seconds must be a whole number from 0 to 60")
    );
    assert_eq!(s.edit.as_deref(), Some("61"), "the typed value stays");
    assert_eq!(s.values[s.cursor], "2");

    // Fixing it clears the error, writes, and restarts the timer.
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Backspace);
    type_str(&mut app, "0");
    press(&mut app, KeyCode::Enter);
    let s = app.ui.settings.as_ref().unwrap();
    assert_eq!(s.errors[s.cursor], None);
    assert_eq!(s.edit, None);
    assert_eq!(app.refresh_every, None);
    assert_eq!(app.next_tick, None);
    assert!(std::fs::read_to_string(&paths.config_file)
        .unwrap()
        .contains("refresh_seconds = 0\n"));
    press(&mut app, KeyCode::Char('+'));
    assert_eq!(app.refresh_every, Some(Duration::from_secs(1)));
    assert!(app.next_tick.is_some(), "the timer starts again");

    // `-` at 0 is refused by gig-core, not clamped silently.
    type_str(&mut app, "0");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('-'));
    let s = app.ui.settings.as_ref().unwrap();
    assert!(s.errors[s.cursor].is_some());
    assert!(std::fs::read_to_string(&paths.config_file)
        .unwrap()
        .contains("refresh_seconds = 0\n"));
}

#[test]
fn theme_selection_writes_tui_theme_and_esc_restores() {
    let dir = TempDir::new().unwrap();
    let paths = Paths::under_root(dir.path());
    let mut app = app_in(dir.path(), &Tui::default());
    assert_eq!(app.theme.name, "gig-dark");

    // Esc: the preview goes, nothing is written.
    press(&mut app, KeyCode::Char('T'));
    press(&mut app, KeyCode::Down);
    let p = app.ui.picker.as_ref().unwrap();
    assert_eq!(p.preview().unwrap().name, "gig-light");
    press(&mut app, KeyCode::Esc);
    assert!(app.ui.picker.is_none());
    assert_eq!(app.theme.name, "gig-dark");
    assert!(!paths.config_file.exists());

    // Enter from the Settings row keeps the highlighted theme.
    press(&mut app, KeyCode::Char(','));
    go_to(&mut app, "tui.theme");
    press(&mut app, KeyCode::Enter);
    assert!(app.ui.picker.is_some());
    press(&mut app, KeyCode::End);
    assert_eq!(
        app.ui.picker.as_ref().unwrap().preview().unwrap().name,
        "dracula"
    );
    press(&mut app, KeyCode::Up);
    press(&mut app, KeyCode::Enter);
    assert!(app.ui.picker.is_none());
    assert_eq!(app.theme, Theme::NORD);
    let text = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(text.contains("[tui]\ntheme = \"nord\"\n"), "{text}");
    let s = app.ui.settings.as_ref().unwrap();
    assert_eq!(s.values[0], "nord");
    assert_eq!(s.session[0], None);
}

/// The section 15 example, with an accent to change.
fn theme_file(accent: &str) -> String {
    let t = Theme::CATPPUCCIN_MOCHA;
    let mut text = themes::to_toml(&t, "test");
    text = text.replace(
        "accent   = \"#cba6f7\"",
        &format!("accent   = \"{accent}\""),
    );
    text
}

#[test]
fn hot_reload_picks_up_an_edited_theme_file() {
    let dir = TempDir::new().unwrap();
    let paths = Paths::under_root(dir.path());
    let themes_dir = paths.themes_dir();
    std::fs::create_dir_all(&themes_dir).unwrap();
    let file = themes_dir.join("mine.toml");
    std::fs::write(&file, theme_file("#b4befe")).unwrap();
    let settings = Tui {
        theme: Some("mine".into()),
        ..Tui::default()
    };
    let mut app = app_in(dir.path(), &settings);
    assert_eq!(app.theme.name, "mine");
    assert_eq!(
        app.theme.accent,
        ratatui::style::Color::Rgb(0xb4, 0xbe, 0xfe)
    );

    // Nothing changed: nothing reloads.
    app.tick();
    assert_eq!(app.ui.toast, None);

    std::fs::write(&file, theme_file("#f5c2e7")).unwrap();
    // Two writes within one timestamp tick look the same; move it on.
    let later = SystemTime::now() + Duration::from_secs(5);
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(later)
        .unwrap();
    app.tick();
    assert_eq!(
        app.theme.accent,
        ratatui::style::Color::Rgb(0xf5, 0xc2, 0xe7)
    );
    assert_eq!(app.ui.toast.as_ref().unwrap().text, "theme mine reloaded");

    // A broken edit keeps the colours loaded before and says why.
    std::fs::write(&file, "bg = \"#000000\"\n").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(later + Duration::from_secs(5))
        .unwrap();
    app.tick();
    assert_eq!(
        app.theme.accent,
        ratatui::style::Color::Rgb(0xf5, 0xc2, 0xe7)
    );
    let toast = app.ui.toast.as_ref().unwrap().text.clone();
    assert!(
        toast.starts_with("theme mine: missing key") && toast.ends_with("loaded before"),
        "{toast}"
    );

    // A new file shows up in the picker without a restart.
    std::fs::write(themes_dir.join("other.toml"), theme_file("#89dceb")).unwrap();
    app.tick();
    press(&mut app, KeyCode::Char('T'));
    let names: Vec<&str> = app
        .ui
        .picker
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .map(|r| r.name.as_str())
        .collect();
    assert!(names.contains(&"other"), "{names:?}");
    assert_eq!(
        names.last(),
        Some(&"mine"),
        "the broken file is listed last"
    );
}

#[test]
fn c_copies_a_builtin_in_the_documented_format() {
    let dir = TempDir::new().unwrap();
    let paths = Paths::under_root(dir.path());
    let mut app = app_in(dir.path(), &Tui::default());
    press(&mut app, KeyCode::Char('T'));
    for _ in 0..6 {
        press(&mut app, KeyCode::Down);
    }
    assert_eq!(
        press(&mut app, KeyCode::Char('c')),
        Some(Outcome::CopyTheme("nord".into()))
    );
    let path = app.copy_theme("nord").unwrap();
    assert_eq!(path, paths.themes_dir().join("nord-copy.toml"));
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("# gig theme, copied from the built-in nord."));
    assert!(text.contains("\nbg       = \"#2e3440\"\n"), "{text}");
    assert!(text.contains("\nbar_now  = \"#c2a3bb\"\n"), "{text}");
    let loaded = themes::load_file(&path).unwrap();
    assert_eq!(
        Theme {
            name: "nord".into(),
            ..loaded
        },
        Theme::NORD
    );

    // An existing copy is never overwritten.
    std::fs::write(&path, theme_file("#123456")).unwrap();
    app.copy_theme("nord").unwrap();
    assert!(std::fs::read_to_string(&path).unwrap().contains("#123456"));

    // After the editor: the picker lists the copy and previews it.
    app.after_theme_edit(&path);
    let p = app.ui.picker.as_ref().unwrap();
    assert_eq!(p.highlighted(), Some("nord-copy"));
    assert_eq!(
        p.preview().unwrap().accent,
        ratatui::style::Color::Rgb(0x12, 0x34, 0x56)
    );
    assert_eq!(app.theme.name, "gig-dark", "nothing kept until Enter");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.theme.name, "nord-copy");
}
