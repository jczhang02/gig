//! gig-tui: the terminal dashboard behind `gig tui`. See docs/v2/TUI-SPEC.md.
//!
//! Reads and writes go through gig-core, the same service functions the CLI
//! calls. This crate owns the terminal, the event loop, and the drawing.

pub mod actions;
pub mod app;
pub mod data;
pub mod help;
pub mod icons;
pub mod popup;
pub mod terminal;
pub mod theme;
pub mod ui;
pub mod upload;
pub mod views;

use gig_core::config::Tui;
use gig_core::services::Ctx;
use gig_core::Result;

/// Command-line flags of `gig tui`. `None` means "not given", so config and
/// environment values survive.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Opts {
    /// `--light`
    pub light: Option<bool>,
    /// `--no-icons` maps to `Some(false)`.
    pub icons: Option<bool>,
    /// `--refresh <seconds>`; 0 disables the timer.
    pub refresh_seconds: Option<u64>,
}

/// Effective settings: config file, then flags, then `GIG_TUI_*` (spec section 5).
pub fn resolve_settings(config: &Tui, opts: Opts) -> Result<Tui> {
    let mut tui = apply_flags(config, opts);
    tui.apply_env_overrides()?;
    Ok(tui)
}

fn apply_flags(config: &Tui, opts: Opts) -> Tui {
    let mut tui = config.clone();
    if let Some(v) = opts.light {
        tui.light = v;
    }
    if let Some(v) = opts.icons {
        tui.icons = v;
    }
    if let Some(v) = opts.refresh_seconds {
        tui.refresh_seconds = v;
    }
    tui
}

/// Entry point of `gig tui`: open the database once, take over the terminal,
/// run the event loop, and restore the terminal on every exit path.
pub fn run(opts: Opts) -> Result<()> {
    let ctx = Ctx::open()?;
    let settings = resolve_settings(&ctx.config.tui, opts)?;
    let mut app = app::App::new(ctx, &settings);
    app.refresh();
    let mut term = terminal::enter()?;
    let _guard = terminal::Guard;
    app.run_loop(&mut term)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_override_config_only_when_given() {
        let cfg = Tui {
            light: false,
            icons: true,
            refresh_seconds: 5,
        };
        assert_eq!(apply_flags(&cfg, Opts::default()), cfg);
        let got = apply_flags(
            &cfg,
            Opts {
                light: Some(true),
                icons: Some(false),
                refresh_seconds: Some(0),
            },
        );
        assert_eq!(
            got,
            Tui {
                light: true,
                icons: false,
                refresh_seconds: 0
            }
        );
    }
}
