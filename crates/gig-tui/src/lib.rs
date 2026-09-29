//! gig-tui: the terminal dashboard behind `gig tui`. See docs/v2/TUI-SPEC.md.
//!
//! Reads and writes go through gig-core, the same service functions the CLI
//! calls. This crate owns the terminal, the event loop, and the drawing.

pub mod actions;
pub mod app;
pub mod data;
pub mod editor;
pub mod help;
pub mod icons;
pub mod popup;
pub mod terminal;
pub mod text;
pub mod theme;
pub mod themes;
pub mod ui;
pub mod upload;
pub mod views;

use gig_core::config::{Paths, Tui};
use gig_core::services::Ctx;
use gig_core::Result;
use std::io::Write;
use theme::Theme;
use themes::Catalog;

/// Command-line flags of the dashboard (bare `gig` or `gig tui`). `None`
/// means "not given", so config and environment values survive.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Opts {
    /// `--theme <name>`
    pub theme: Option<String>,
    /// `--light`, the alias of `--theme gig-light`.
    pub light: Option<bool>,
    /// `--no-icons` maps to `Some(false)`.
    pub icons: Option<bool>,
    /// `--refresh <seconds>`; 0 disables the timer.
    pub refresh_seconds: Option<u64>,
    /// `--mouse` / `--no-mouse`.
    pub mouse: Option<bool>,
}

/// Effective settings: config file, then flags, then `GIG_TUI_*` (spec
/// section 5). `theme` holds the chosen name, `None` for the default.
pub fn resolve_settings(config: &Tui, opts: &Opts) -> Result<Tui> {
    resolve_with_env(config, opts, |k| {
        std::env::var(k).ok().filter(|v| !v.is_empty())
    })
}

/// The theme a layer selects: its `theme`, else `gig-light` when its `light`
/// is true (`light = false` selects nothing). TUI-DESIGN.md section 14.2.
fn layer_theme(theme: Option<&str>, light: bool) -> Option<String> {
    theme
        .map(str::to_string)
        .or_else(|| light.then(|| Theme::LIGHT_NAME.to_string()))
}

fn resolve_with_env(
    config: &Tui,
    opts: &Opts,
    env: impl Fn(&str) -> Option<String>,
) -> Result<Tui> {
    let from_config = layer_theme(config.theme.as_deref(), config.light);
    let from_flags = layer_theme(opts.theme.as_deref(), opts.light == Some(true));
    let mut tui = apply_flags(config, opts);
    // The env layer starts with no theme of its own so its alias applies alone.
    tui.theme = None;
    tui.light = false;
    tui.apply_overrides(env)?;
    let from_env = layer_theme(tui.theme.as_deref(), tui.light);
    tui.theme = from_env.or(from_flags).or(from_config);
    tui.light = tui.theme.as_deref() == Some(Theme::LIGHT_NAME);
    Ok(tui)
}

fn apply_flags(config: &Tui, opts: &Opts) -> Tui {
    let mut tui = config.clone();
    if let Some(v) = &opts.theme {
        tui.theme = Some(v.clone());
    }
    if let Some(v) = opts.light {
        tui.light = v;
    }
    if let Some(v) = opts.icons {
        tui.icons = v;
    }
    if let Some(v) = opts.refresh_seconds {
        tui.refresh_seconds = v;
    }
    if let Some(v) = opts.mouse {
        tui.mouse = v;
    }
    tui
}

/// `--list-themes`: every available name on `out`, one per line; broken
/// theme files on `err` as `gig: theme <file>: <reason>`. Needs no database
/// and no terminal.
pub fn list_themes(out: &mut impl Write, err: &mut impl Write) -> Result<()> {
    let catalog = Catalog::load(&Paths::from_env()?.themes_dir());
    for name in catalog.names() {
        writeln!(out, "{name}")?;
    }
    for b in &catalog.broken {
        writeln!(err, "gig: theme {}: {}", b.path.display(), b.detail)?;
    }
    Ok(())
}

/// Entry point of the dashboard: open the database once, take over the
/// terminal, run the event loop, and restore the terminal on every exit path.
pub fn run(opts: Opts) -> Result<()> {
    let ctx = Ctx::open()?;
    let settings = resolve_settings(&ctx.config.tui, &opts)?;
    let catalog = Catalog::load(&ctx.paths.themes_dir());
    let (theme, warning) = catalog.pick(settings.theme.as_deref());
    let mode = theme::ColorMode::detect(|k| std::env::var(k).ok());
    let theme = theme.for_mode(mode);
    let themes = catalog.themes.iter().map(|t| t.for_mode(mode)).collect();
    let here = std::env::current_dir()
        .ok()
        .and_then(|d| gig_core::context::canonical(d).ok())
        .and_then(|d| gig_core::context::resolve_for(&ctx.conn, &d).ok().flatten());
    let mut app = app::App::new(ctx, &settings, theme, themes);
    app.refresh();
    if let Some(order) = here {
        app.ui.preselect(order.id);
    }
    if let Some(w) = warning {
        app.ui.toast = Some(app::Toast::warn(w));
    }
    let mut term = terminal::enter()?;
    let _guard = terminal::Guard;
    app.run_loop(&mut term)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn tui(theme: Option<&str>, light: bool) -> Tui {
        Tui {
            theme: theme.map(str::to_string),
            light,
            ..Tui::default()
        }
    }

    fn opts(theme: Option<&str>, light: bool) -> Opts {
        Opts {
            theme: theme.map(str::to_string),
            light: light.then_some(true),
            ..Opts::default()
        }
    }

    fn resolve(config: &Tui, opts: &Opts, env: &[(&str, &str)]) -> Tui {
        let env: BTreeMap<&str, &str> = env.iter().copied().collect();
        resolve_with_env(config, opts, |k| env.get(k).map(|v| v.to_string())).unwrap()
    }

    #[test]
    fn flags_override_config_only_when_given() {
        let cfg = Tui {
            theme: None,
            light: false,
            icons: true,
            refresh_seconds: 5,
            mouse: true,
        };
        assert_eq!(resolve(&cfg, &Opts::default(), &[]), cfg);
        let got = resolve(
            &cfg,
            &Opts {
                theme: None,
                light: Some(true),
                icons: Some(false),
                refresh_seconds: Some(0),
                mouse: Some(false),
            },
            &[],
        );
        assert_eq!(
            got,
            Tui {
                theme: Some("gig-light".into()),
                light: true,
                icons: false,
                refresh_seconds: 0,
                mouse: false,
            }
        );
    }

    #[test]
    fn theme_precedence_config_flag_env() {
        let name = |t: Tui| t.theme;
        let none = Opts::default();
        assert_eq!(name(resolve(&Tui::default(), &none, &[])), None);
        assert_eq!(
            name(resolve(&tui(Some("nord"), false), &none, &[])).as_deref(),
            Some("nord")
        );
        assert_eq!(
            name(resolve(
                &tui(Some("nord"), false),
                &opts(Some("dracula"), false),
                &[]
            ))
            .as_deref(),
            Some("dracula")
        );
        assert_eq!(
            name(resolve(
                &tui(Some("nord"), false),
                &opts(Some("dracula"), false),
                &[("GIG_TUI_THEME", "tokyonight")]
            ))
            .as_deref(),
            Some("tokyonight")
        );
    }

    #[test]
    fn light_is_an_alias_inside_each_layer() {
        let name = |t: Tui| t.theme;
        let none = Opts::default();
        // Config: light only when that layer sets no theme.
        assert_eq!(
            name(resolve(&tui(None, true), &none, &[])).as_deref(),
            Some("gig-light")
        );
        assert_eq!(
            name(resolve(&tui(Some("nord"), true), &none, &[])).as_deref(),
            Some("nord")
        );
        // A higher layer's light beats a lower layer's theme.
        assert_eq!(
            name(resolve(&tui(Some("nord"), false), &opts(None, true), &[])).as_deref(),
            Some("gig-light")
        );
        assert_eq!(
            name(resolve(
                &tui(Some("nord"), false),
                &opts(Some("dracula"), false),
                &[("GIG_TUI_LIGHT", "1")]
            ))
            .as_deref(),
            Some("gig-light")
        );
        // Env theme wins over env light; light = false selects nothing.
        assert_eq!(
            name(resolve(
                &tui(None, true),
                &none,
                &[("GIG_TUI_LIGHT", "1"), ("GIG_TUI_THEME", "nord")]
            ))
            .as_deref(),
            Some("nord")
        );
        assert_eq!(
            name(resolve(
                &tui(Some("nord"), false),
                &none,
                &[("GIG_TUI_LIGHT", "0")]
            ))
            .as_deref(),
            Some("nord")
        );
    }
}
