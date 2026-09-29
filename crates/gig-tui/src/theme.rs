//! Theme slots, the eight built-in palettes and the contrast rules of
//! docs/v2/TUI-DESIGN.md sections 2, 14 and 16. User theme files live in
//! `themes.rs`.

use gig_core::models::OrderStatus;
use ratatui::style::{Color, Modifier, Style};
use std::borrow::Cow;

/// A delivered order unpaid this many days is overdue (TUI-DESIGN.md section 2).
pub const OVERDUE_DAYS: i64 = 14;

/// Fifteen colour slots plus a name. In-progress status is `text` by rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub name: Cow<'static, str>,
    /// Page background.
    pub bg: Color,
    /// Popup fill.
    pub surface: Color,
    /// Selection band behind the selected row; never reverse video.
    pub sel: Color,
    /// Popup frames, chart baseline, progress track. Decorative only.
    pub border: Color,
    /// Primary content and the in-progress status.
    pub text: Color,
    /// Labels, headers, meta lines.
    pub muted: Color,
    /// Separators and placeholders; never the only carrier of a fact.
    pub dim: Color,
    /// The one accent: selection marker, active tab, focus, current.
    pub accent: Color,
    /// Key letters in hints and help.
    pub key: Color,
    /// Delivered and not yet paid; also refusals and errors.
    pub unpaid: Color,
    /// Paid, still inside the warranty period.
    pub warranty: Color,
    pub queued: Color,
    /// Archived and cancelled.
    pub archived: Color,
    /// Chart bars of past months.
    pub bar: Color,
    /// Chart bar of the current month.
    pub bar_now: Color,
}

const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Slot names in file order; also the exact key set of a theme file.
pub const SLOTS: [&str; 15] = [
    "bg", "surface", "sel", "border", "text", "muted", "dim", "accent", "key", "unpaid",
    "warranty", "queued", "archived", "bar", "bar_now",
];

/// Palettes of TUI-DESIGN.md section 16.2.
impl Theme {
    pub const GIG_DARK: Theme = Theme {
        name: Cow::Borrowed("gig-dark"),
        bg: rgb(0x14161a),
        surface: rgb(0x0f1115),
        sel: rgb(0x252a32),
        border: rgb(0x2c313a),
        text: rgb(0xdfe2e7),
        muted: rgb(0x9ba2ad),
        dim: rgb(0x6e7581),
        accent: rgb(0x7fc8b6),
        key: rgb(0xdfe2e7),
        unpaid: rgb(0xf26b63),
        warranty: rgb(0xe6a23c),
        queued: rgb(0x6ea8f2),
        archived: rgb(0x8d939d),
        bar: rgb(0x5f6874),
        bar_now: rgb(0x7fc8b6),
    };

    pub const GIG_LIGHT: Theme = Theme {
        name: Cow::Borrowed("gig-light"),
        bg: rgb(0xf7f6f2),
        surface: rgb(0xffffff),
        sel: rgb(0xebe8df),
        border: rgb(0xd8d4c9),
        text: rgb(0x1f2328),
        muted: rgb(0x565c66),
        dim: rgb(0x7d838d),
        accent: rgb(0x1d7563),
        key: rgb(0x1f2328),
        unpaid: rgb(0xbf2f28),
        warranty: rgb(0x945600),
        queued: rgb(0x1f5fc0),
        archived: rgb(0x626770),
        bar: rgb(0x898f99),
        bar_now: rgb(0x1d7563),
    };

    pub const CATPPUCCIN_MOCHA: Theme = Theme {
        name: Cow::Borrowed("catppuccin-mocha"),
        bg: rgb(0x1e1e2e),
        surface: rgb(0x181825),
        sel: rgb(0x313244),
        border: rgb(0x45475a),
        text: rgb(0xcdd6f4),
        muted: rgb(0xa6adc8),
        dim: rgb(0x7f849c),
        accent: rgb(0xcba6f7),
        key: rgb(0xcdd6f4),
        unpaid: rgb(0xf38ba8),
        warranty: rgb(0xfab387),
        queued: rgb(0x89b4fa),
        archived: rgb(0x969cb4),
        bar: rgb(0x6c7086),
        bar_now: rgb(0xcba6f7),
    };

    pub const CATPPUCCIN_LATTE: Theme = Theme {
        name: Cow::Borrowed("catppuccin-latte"),
        bg: rgb(0xeff1f5),
        surface: rgb(0xf9fafb),
        sel: rgb(0xdce0e8),
        border: rgb(0xbcc0cc),
        text: rgb(0x4c4f69),
        muted: rgb(0x5c5f77),
        dim: rgb(0x797c91),
        accent: rgb(0x7f2aef),
        key: rgb(0x4c4f69),
        unpaid: rgb(0xc30f36),
        warranty: rgb(0x9e4d00),
        queued: rgb(0x0a54e6),
        archived: rgb(0x5e6170),
        bar: rgb(0x86899d),
        bar_now: rgb(0x7f2aef),
    };

    pub const TOKYONIGHT: Theme = Theme {
        name: Cow::Borrowed("tokyonight"),
        bg: rgb(0x1a1b26),
        surface: rgb(0x16161e),
        sel: rgb(0x232538),
        border: rgb(0x3b4261),
        text: rgb(0xc0caf5),
        muted: rgb(0xa9b1d6),
        dim: rgb(0x737aa2),
        accent: rgb(0xbb9af7),
        key: rgb(0xc0caf5),
        unpaid: rgb(0xf7768e),
        warranty: rgb(0xe0af68),
        queued: rgb(0x7aa2f7),
        archived: rgb(0x848cb1),
        bar: rgb(0x5c6592),
        bar_now: rgb(0xbb9af7),
    };

    pub const GRUVBOX_DARK: Theme = Theme {
        name: Cow::Borrowed("gruvbox-dark"),
        bg: rgb(0x282828),
        surface: rgb(0x1d2021),
        sel: rgb(0x3c3836),
        border: rgb(0x504945),
        text: rgb(0xebdbb2),
        muted: rgb(0xbdae93),
        dim: rgb(0x928374),
        accent: rgb(0x8ec07c),
        key: rgb(0xebdbb2),
        unpaid: rgb(0xfe7c6b),
        warranty: rgb(0xfabd2f),
        queued: rgb(0x89a99c),
        archived: rgb(0xb0a290),
        bar: rgb(0x7c6f64),
        bar_now: rgb(0x8ec07c),
    };

    pub const NORD: Theme = Theme {
        name: Cow::Borrowed("nord"),
        bg: rgb(0x2e3440),
        surface: rgb(0x292e39),
        sel: rgb(0x3a4150),
        border: rgb(0x4c566a),
        text: rgb(0xeceff4),
        muted: rgb(0xd8dee9),
        dim: rgb(0x8a95ab),
        accent: rgb(0xc2a3bb),
        key: rgb(0xeceff4),
        unpaid: rgb(0xed969c),
        warranty: rgb(0xebcb8b),
        queued: rgb(0x93b2cd),
        archived: rgb(0xa3adbf),
        bar: rgb(0x717e98),
        bar_now: rgb(0xc2a3bb),
    };

    pub const DRACULA: Theme = Theme {
        name: Cow::Borrowed("dracula"),
        bg: rgb(0x282a36),
        surface: rgb(0x21222c),
        sel: rgb(0x33364a),
        border: rgb(0x44475a),
        text: rgb(0xf8f8f2),
        muted: rgb(0xc3c6d9),
        dim: rgb(0x7181ae),
        accent: rgb(0xbd93f9),
        key: rgb(0xf8f8f2),
        unpaid: rgb(0xff7878),
        warranty: rgb(0xffb86c),
        queued: rgb(0x8be9fd),
        archived: rgb(0x9aa0bd),
        bar: rgb(0x6272a4),
        bar_now: rgb(0xbd93f9),
    };

    /// Built-ins in cycling and listing order; the first is the default.
    pub const BUILTIN: [Theme; 8] = [
        Self::GIG_DARK,
        Self::GIG_LIGHT,
        Self::CATPPUCCIN_MOCHA,
        Self::CATPPUCCIN_LATTE,
        Self::TOKYONIGHT,
        Self::GRUVBOX_DARK,
        Self::NORD,
        Self::DRACULA,
    ];

    /// Short aliases kept for the render tests.
    pub const DARK: Theme = Self::GIG_DARK;
    pub const LIGHT: Theme = Self::GIG_LIGHT;

    pub const DEFAULT_NAME: &'static str = "gig-dark";
    pub const LIGHT_NAME: &'static str = "gig-light";

    /// A built-in by name.
    pub fn builtin(name: &str) -> Option<Theme> {
        Self::BUILTIN.into_iter().find(|t| t.name == name)
    }

    /// The colour of one slot by its file key.
    pub fn slot(&self, key: &str) -> Option<Color> {
        Some(match key {
            "bg" => self.bg,
            "surface" => self.surface,
            "sel" => self.sel,
            "border" => self.border,
            "text" => self.text,
            "muted" => self.muted,
            "dim" => self.dim,
            "accent" => self.accent,
            "key" => self.key,
            "unpaid" => self.unpaid,
            "warranty" => self.warranty,
            "queued" => self.queued,
            "archived" => self.archived,
            "bar" => self.bar,
            "bar_now" => self.bar_now,
            _ => return None,
        })
    }

    /// Mutable access to one slot by its file key.
    pub(crate) fn slot_mut(&mut self, key: &str) -> Option<&mut Color> {
        Some(match key {
            "bg" => &mut self.bg,
            "surface" => &mut self.surface,
            "sel" => &mut self.sel,
            "border" => &mut self.border,
            "text" => &mut self.text,
            "muted" => &mut self.muted,
            "dim" => &mut self.dim,
            "accent" => &mut self.accent,
            "key" => &mut self.key,
            "unpaid" => &mut self.unpaid,
            "warranty" => &mut self.warranty,
            "queued" => &mut self.queued,
            "archived" => &mut self.archived,
            "bar" => &mut self.bar,
            "bar_now" => &mut self.bar_now,
            _ => return None,
        })
    }

    /// Fixed status colours: unpaid red, warranty amber, in progress `text`,
    /// queued blue, archived and cancelled grey.
    pub fn status_color(&self, status: OrderStatus) -> Color {
        match status {
            OrderStatus::Delivered => self.unpaid,
            OrderStatus::Paid => self.warranty,
            OrderStatus::InProgress => self.text,
            OrderStatus::Queued => self.queued,
            OrderStatus::Archived | OrderStatus::Cancelled => self.archived,
        }
    }

    pub fn base(&self) -> Style {
        Style::new().fg(self.text).bg(self.bg)
    }

    pub fn text(&self) -> Style {
        Style::new().fg(self.text)
    }

    /// Label ink.
    pub fn muted(&self) -> Style {
        Style::new().fg(self.muted)
    }

    pub fn title(&self) -> Style {
        Style::new().fg(self.accent).add_modifier(Modifier::BOLD)
    }

    pub fn key(&self) -> Style {
        Style::new().fg(self.key).add_modifier(Modifier::BOLD)
    }

    /// Refusals and errors use `unpaid` (section 2).
    pub fn error(&self) -> Style {
        Style::new().fg(self.unpaid)
    }

    pub fn border(&self) -> Style {
        Style::new().fg(self.border)
    }

    pub fn selected(&self) -> Style {
        Style::new().bg(self.sel)
    }

    pub fn status(&self, status: OrderStatus) -> Style {
        Style::new().fg(self.status_color(status))
    }

    /// The first rule of TUI-DESIGN.md section 16.1 this theme breaks, as a
    /// sentence (`muted 3.9:1 on sel, needs 4.5`); `None` when all pass.
    pub fn contrast_failure(&self) -> Option<String> {
        contrast::failures(self).into_iter().next()
    }
}

/// WCAG 2 relative luminance and contrast, and the thresholds of
/// TUI-DESIGN.md section 16.1.
pub mod contrast {
    use super::Theme;
    use ratatui::style::Color;

    fn channel(c: u8) -> f64 {
        let x = f64::from(c) / 255.0;
        if x <= 0.04045 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    }

    /// Relative luminance of an RGB colour; other colours count as black.
    pub fn luminance(c: Color) -> f64 {
        match c {
            Color::Rgb(r, g, b) => 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b),
            _ => 0.0,
        }
    }

    /// Contrast ratio, 1.0 to 21.0.
    pub fn ratio(a: Color, b: Color) -> f64 {
        let (la, lb) = (luminance(a), luminance(b));
        let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// Slots read as text: 4.5:1 on `bg`, `sel` and `surface`.
    pub const TEXT_SLOTS: [&str; 8] = [
        "text", "muted", "accent", "key", "unpaid", "warranty", "queued", "archived",
    ];

    /// Every broken rule, in the order of section 16.1.
    pub fn failures(t: &Theme) -> Vec<String> {
        let mut out = Vec::new();
        let slot = |k: &str| t.slot(k).expect("known slot");
        let grounds = ["bg", "sel", "surface"];
        let mut need = |fg: &str, bg: &str, min: f64| {
            let r = ratio(slot(fg), slot(bg));
            if r < min {
                out.push(format!("{fg} {r:.2}:1 on {bg}, needs {min}"));
            }
        };
        for fg in TEXT_SLOTS {
            for bg in grounds {
                need(fg, bg, 4.5);
            }
        }
        for bg in grounds {
            need("dim", bg, 3.0);
        }
        need("bar", "bg", 3.0);
        need("bar_now", "bg", 3.0);
        need("sel", "surface", 1.15);
        need("muted", "dim", 1.4);
        let band = ratio(t.sel, t.bg);
        if !(1.10..=1.40).contains(&band) {
            out.push(format!("sel {band:.2}:1 on bg, needs 1.1 to 1.4"));
        }
        for (a, b) in [
            ("unpaid", "warranty"),
            ("unpaid", "queued"),
            ("warranty", "queued"),
            ("bar_now", "bar"),
        ] {
            if slot(a) == slot(b) {
                out.push(format!("{a} equals {b}"));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::contrast::{failures, ratio};
    use super::*;

    #[test]
    fn status_colours_are_fixed() {
        for theme in Theme::BUILTIN {
            assert_eq!(theme.status_color(OrderStatus::InProgress), theme.text);
            assert_eq!(
                theme.status_color(OrderStatus::Archived),
                theme.status_color(OrderStatus::Cancelled)
            );
            assert_ne!(
                theme.status_color(OrderStatus::Delivered),
                theme.status_color(OrderStatus::Paid)
            );
        }
    }

    #[test]
    fn builtins_in_order_with_unique_names() {
        let names: Vec<_> = Theme::BUILTIN.iter().map(|t| t.name.to_string()).collect();
        assert_eq!(
            names,
            [
                "gig-dark",
                "gig-light",
                "catppuccin-mocha",
                "catppuccin-latte",
                "tokyonight",
                "gruvbox-dark",
                "nord",
                "dracula"
            ]
        );
        assert_eq!(Theme::builtin("nord").unwrap(), Theme::NORD);
        assert!(Theme::builtin("nrod").is_none());
        assert_eq!(Theme::BUILTIN[0].name, Theme::DEFAULT_NAME);
        assert_eq!(Theme::builtin(Theme::LIGHT_NAME).unwrap(), Theme::LIGHT);
    }

    #[test]
    fn contrast_matches_the_design_doc() {
        // Spot checks against TUI-DESIGN.md section 16.3.
        let t = Theme::GIG_DARK;
        assert_eq!(format!("{:.2}", ratio(t.text, t.bg)), "13.95");
        assert_eq!(format!("{:.2}", ratio(t.dim, t.sel)), "3.11");
        let n = Theme::NORD;
        assert_eq!(format!("{:.2}", ratio(n.accent, n.sel)), "4.51");
        assert_eq!(
            format!("{:.2}", ratio(rgb(0xffffff), rgb(0x000000))),
            "21.00"
        );
    }

    /// Text, muted and every status colour reach 4.5:1 on `bg` and at least
    /// 3:1 on the selection band, and every other rule of section 16.1 holds,
    /// for every built-in.
    #[test]
    fn builtin_themes_meet_contrast() {
        for t in Theme::BUILTIN {
            for slot in ["text", "muted", "unpaid", "warranty", "queued", "archived"] {
                let c = t.slot(slot).unwrap();
                assert!(ratio(c, t.bg) >= 4.5, "{} {slot} on bg", t.name);
                assert!(ratio(c, t.sel) >= 3.0, "{} {slot} on sel", t.name);
            }
            assert_eq!(failures(&t), Vec::<String>::new(), "{}", t.name);
            assert!(t.contrast_failure().is_none());
        }
    }

    #[test]
    fn a_failing_theme_names_the_rule() {
        let mut t = Theme::GIG_DARK;
        t.muted = t.bg;
        assert_eq!(
            t.contrast_failure().unwrap(),
            "muted 1.00:1 on bg, needs 4.5"
        );
    }
}
