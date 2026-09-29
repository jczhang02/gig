//! Nerd Font glyphs for project types, and statuses. With icons off
//! every glyph is empty and the text label next to it stays.

use gig_core::models::{OrderStatus, ProjectType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Icons {
    pub enabled: bool,
}

impl Icons {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    fn pick(&self, glyph: &'static str) -> &'static str {
        if self.enabled {
            glyph
        } else {
            ""
        }
    }

    pub fn project_type(&self, t: ProjectType) -> &'static str {
        self.pick(match t {
            ProjectType::Tool => "\u{f0ad}",            // wrench
            ProjectType::CvMl => "\u{f06e}",            // eye
            ProjectType::DataProcessing => "\u{f1c0}",  // database
            ProjectType::ResearchWriting => "\u{f02d}", // book
            ProjectType::Custom => "\u{f1b2}",          // cube
        })
    }

    pub fn status(&self, s: OrderStatus) -> &'static str {
        self.pick(match s {
            OrderStatus::Queued => "\u{f017}",     // clock
            OrderStatus::InProgress => "\u{f013}", // cog
            OrderStatus::Delivered => "\u{f06a}",  // exclamation circle
            OrderStatus::Paid => "\u{f132}",       // shield
            OrderStatus::Archived => "\u{f187}",   // archive
            OrderStatus::Cancelled => "\u{f05e}",  // ban
        })
    }

    /// "glyph text" with icons on, "text" with icons off.
    pub fn label(&self, glyph: &str, text: &str) -> String {
        if glyph.is_empty() {
            text.to_string()
        } else {
            format!("{glyph} {text}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_icons_keeps_text() {
        let on = Icons::new(true);
        let off = Icons::new(false);
        for t in ProjectType::ALL {
            assert!(!on.project_type(*t).is_empty());
            assert_eq!(off.project_type(*t), "");
        }
        for s in OrderStatus::ALL {
            assert!(!on.status(*s).is_empty());
            assert_eq!(off.label(off.status(*s), s.as_str()), s.as_str());
        }
        assert_eq!(
            on.label(on.status(OrderStatus::Paid), "paid"),
            "\u{f132} paid".to_string()
        );
    }
}
