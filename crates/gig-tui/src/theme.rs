//! Truecolor palettes (dark default, light variant) and the fixed status
//! colours of spec section 3.

use gig_core::models::OrderStatus;
use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub light: bool,
    pub bg: Color,
    pub fg: Color,
    pub dim: Color,
    pub accent: Color,
    /// Subtle band behind the selected row; never reverse video.
    pub selection_bg: Color,
    pub border: Color,
    pub key: Color,
    pub error: Color,
    pub ok: Color,
    /// Delivered and not yet paid.
    pub unpaid: Color,
    /// Paid, still inside the warranty period.
    pub warranty: Color,
    pub queued: Color,
    /// Archived and cancelled.
    pub inactive: Color,
}

const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

impl Theme {
    pub const DARK: Theme = Theme {
        light: false,
        bg: rgb(0x16181d),
        fg: rgb(0xd5d8de),
        dim: rgb(0x6b7280),
        accent: rgb(0x8ab4f8),
        selection_bg: rgb(0x262a33),
        border: rgb(0x3a3f4b),
        key: rgb(0xe0b86a),
        error: rgb(0xf07178),
        ok: rgb(0x8fce8f),
        unpaid: rgb(0xef5350),
        warranty: rgb(0xf0a830),
        queued: rgb(0x5c9ded),
        inactive: rgb(0x6b7280),
    };

    pub const LIGHT: Theme = Theme {
        light: true,
        bg: rgb(0xfafaf7),
        fg: rgb(0x2b2f36),
        dim: rgb(0x8a8f98),
        accent: rgb(0x2f6fd0),
        selection_bg: rgb(0xe8eaee),
        border: rgb(0xc8ccd3),
        key: rgb(0x9a6a10),
        error: rgb(0xc62828),
        ok: rgb(0x2e7d32),
        unpaid: rgb(0xd32f2f),
        warranty: rgb(0xc77800),
        queued: rgb(0x1e6fd9),
        inactive: rgb(0x9aa0a8),
    };

    pub fn new(light: bool) -> Self {
        if light {
            Self::LIGHT
        } else {
            Self::DARK
        }
    }

    /// Fixed status colours: unpaid red, warranty amber, in progress default
    /// foreground, queued blue, archived and cancelled dim grey.
    pub fn status_color(&self, status: OrderStatus) -> Color {
        match status {
            OrderStatus::Delivered => self.unpaid,
            OrderStatus::Paid => self.warranty,
            OrderStatus::InProgress => self.fg,
            OrderStatus::Queued => self.queued,
            OrderStatus::Archived | OrderStatus::Cancelled => self.inactive,
        }
    }

    pub fn base(&self) -> Style {
        Style::new().fg(self.fg).bg(self.bg)
    }

    pub fn text(&self) -> Style {
        Style::new().fg(self.fg)
    }

    pub fn dim(&self) -> Style {
        Style::new().fg(self.dim)
    }

    pub fn title(&self) -> Style {
        Style::new().fg(self.accent).add_modifier(Modifier::BOLD)
    }

    pub fn key(&self) -> Style {
        Style::new().fg(self.key).add_modifier(Modifier::BOLD)
    }

    pub fn error(&self) -> Style {
        Style::new().fg(self.error)
    }

    pub fn border(&self) -> Style {
        Style::new().fg(self.border)
    }

    pub fn selected(&self) -> Style {
        Style::new().bg(self.selection_bg)
    }

    pub fn status(&self, status: OrderStatus) -> Style {
        Style::new().fg(self.status_color(status))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_colours_are_fixed() {
        for theme in [Theme::DARK, Theme::LIGHT] {
            assert_eq!(theme.status_color(OrderStatus::InProgress), theme.fg);
            assert_eq!(
                theme.status_color(OrderStatus::Archived),
                theme.status_color(OrderStatus::Cancelled)
            );
            assert_ne!(
                theme.status_color(OrderStatus::Delivered),
                theme.status_color(OrderStatus::Paid)
            );
        }
        assert!(Theme::new(true).light);
        assert!(!Theme::new(false).light);
    }
}
