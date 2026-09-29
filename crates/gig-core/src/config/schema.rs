//! The keys the dashboard's Settings view may edit (TUI-SPEC section 8.1),
//! with their types, ranges and help. The view renders whatever `ENTRIES`
//! holds, so adding a key is one line here. `gig config set` validates
//! through the same `validate` when the key is listed.
//!
//! Paths, the uploader, endpoints and secrets are deliberately absent.

use crate::{Error, Result};

/// Where a select row takes its options from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SelectSource {
    /// Theme names: built-ins and user files. The caller (gig-tui) resolves
    /// the list; gig-core only checks the name's shape.
    Theme,
    /// A fixed list of allowed strings.
    Static(&'static [&'static str]),
}

/// Value type and constraints of one entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Toggle,
    /// Inclusive range.
    Integer {
        min: i64,
        max: i64,
    },
    /// Inclusive range, finite values only.
    Number {
        min: f64,
        max: f64,
    },
    Select(SelectSource),
    /// A single line of text.
    Text,
}

/// One editable key.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Entry {
    /// Dot path in config.toml, e.g. `tui.refresh_seconds`.
    pub key: &'static str,
    /// Group heading in the Settings view.
    pub section: &'static str,
    pub label: &'static str,
    pub kind: Kind,
    /// One line shown under the row.
    pub help: &'static str,
}

pub const SECTION_DASHBOARD: &str = "Dashboard";
pub const SECTION_GENERAL: &str = "General";

/// Longest accepted text value, in characters.
pub const TEXT_MAX_CHARS: usize = 32;

/// Every editable key, in display order.
pub static ENTRIES: &[Entry] = &[
    Entry {
        key: "tui.theme",
        section: SECTION_DASHBOARD,
        label: "Theme",
        kind: Kind::Select(SelectSource::Theme),
        help: "Colour theme: a built-in or a file in the themes directory",
    },
    Entry {
        key: "tui.icons",
        section: SECTION_DASHBOARD,
        label: "Icons",
        kind: Kind::Toggle,
        help: "Nerd Font glyphs next to the text labels",
    },
    Entry {
        key: "tui.refresh_seconds",
        section: SECTION_DASHBOARD,
        label: "Refresh",
        kind: Kind::Integer { min: 0, max: 60 },
        help: "Auto-refresh period in seconds; 0 turns the timer off",
    },
    Entry {
        key: "tui.mouse",
        section: SECTION_DASHBOARD,
        label: "Mouse",
        kind: Kind::Toggle,
        help: "Clicks and the wheel; M toggles it for native text selection",
    },
    Entry {
        key: "general.warranty_days",
        section: SECTION_GENERAL,
        label: "Warranty days",
        kind: Kind::Integer { min: 0, max: 365 },
        help: "Days of free fixes after delivery",
    },
    Entry {
        key: "general.default_currency",
        section: SECTION_GENERAL,
        label: "Currency",
        kind: Kind::Text,
        help: "Currency for new orders, e.g. CNY",
    },
    Entry {
        key: "general.default_cut_ratio",
        section: SECTION_GENERAL,
        label: "Cut ratio",
        kind: Kind::Number { min: 0.0, max: 1.0 },
        help: "Your share of the price for new orders, 0 to 1",
    },
];

/// The entry for `key`, if the Settings view may edit it.
pub fn entry(key: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|e| e.key == key)
}

/// Parse and check `raw` for `key`. The error is what the Settings view shows
/// under the row and what `gig config set` reports (code `invalid_input`).
pub fn validate(key: &str, raw: &str) -> Result<toml::Value> {
    let e = entry(key)
        .ok_or_else(|| Error::InvalidInput(format!("{key} is not editable in settings")))?;
    validate_entry(e, raw)
}

/// `validate` for an entry already in hand.
pub fn validate_entry(e: &Entry, raw: &str) -> Result<toml::Value> {
    let key = e.key;
    let bad = |msg: String| Error::InvalidInput(format!("{key} {msg}"));
    let raw = raw.trim();
    match e.kind {
        Kind::Toggle => match raw.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Ok(toml::Value::Boolean(true)),
            "false" | "0" | "no" | "off" => Ok(toml::Value::Boolean(false)),
            _ => Err(bad("must be true or false".into())),
        },
        Kind::Integer { min, max } => {
            let range = || bad(format!("must be a whole number from {min} to {max}"));
            let n: i64 = raw.parse().map_err(|_| range())?;
            if (min..=max).contains(&n) {
                Ok(toml::Value::Integer(n))
            } else {
                Err(range())
            }
        }
        Kind::Number { min, max } => {
            let range = || bad(format!("must be a number from {min} to {max}"));
            let n: f64 = raw.parse().map_err(|_| range())?;
            if n.is_finite() && n >= min && n <= max {
                Ok(toml::Value::Float(n))
            } else {
                Err(range())
            }
        }
        Kind::Select(SelectSource::Theme) => {
            let ok = !raw.is_empty()
                && raw
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            if ok {
                Ok(toml::Value::String(raw.into()))
            } else {
                Err(bad(
                    "must be a theme name: lowercase letters, digits and -".into()
                ))
            }
        }
        Kind::Select(SelectSource::Static(options)) => {
            if options.contains(&raw) {
                Ok(toml::Value::String(raw.into()))
            } else {
                Err(bad(format!("must be one of {}", options.join(", "))))
            }
        }
        Kind::Text => {
            if raw.is_empty() {
                Err(bad("must not be empty".into()))
            } else if raw.chars().count() > TEXT_MAX_CHARS {
                Err(bad(format!("must be at most {TEXT_MAX_CHARS} characters")))
            } else if raw.chars().any(char::is_control) {
                Err(bad("must be a single line".into()))
            } else {
                Ok(toml::Value::String(raw.into()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(key: &str, raw: &str) -> String {
        let e = validate(key, raw).unwrap_err();
        assert_eq!(e.code(), "invalid_input");
        e.to_string()
    }

    #[test]
    fn lists_exactly_the_spec_keys() {
        let keys: Vec<&str> = ENTRIES.iter().map(|e| e.key).collect();
        assert_eq!(
            keys,
            [
                "tui.theme",
                "tui.icons",
                "tui.refresh_seconds",
                "tui.mouse",
                "general.warranty_days",
                "general.default_currency",
                "general.default_cut_ratio",
            ]
        );
        // Every key exists in the config model.
        let cfg = crate::config::Config {
            tui: crate::config::Tui {
                theme: Some("nord".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        for e in ENTRIES {
            cfg.get(e.key).unwrap();
            assert!(!e.help.is_empty() && !e.label.is_empty());
        }
    }

    #[test]
    fn integer_bounds_are_inclusive() {
        let k = "tui.refresh_seconds";
        assert_eq!(validate(k, "0").unwrap(), toml::Value::Integer(0));
        assert_eq!(validate(k, "60").unwrap(), toml::Value::Integer(60));
        assert_eq!(validate(k, " 7 ").unwrap(), toml::Value::Integer(7));
        assert_eq!(
            err(k, "61"),
            "invalid input: tui.refresh_seconds must be a whole number from 0 to 60"
        );
        err(k, "-1");
        err(k, "1.5");
        err(k, "soon");
        let k = "general.warranty_days";
        assert_eq!(validate(k, "365").unwrap(), toml::Value::Integer(365));
        err(k, "366");
    }

    #[test]
    fn number_bounds_are_inclusive() {
        let k = "general.default_cut_ratio";
        assert_eq!(validate(k, "0").unwrap(), toml::Value::Float(0.0));
        assert_eq!(validate(k, "1").unwrap(), toml::Value::Float(1.0));
        assert_eq!(validate(k, "0.65").unwrap(), toml::Value::Float(0.65));
        assert_eq!(
            err(k, "1.01"),
            "invalid input: general.default_cut_ratio must be a number from 0 to 1"
        );
        err(k, "-0.1");
        err(k, "NaN");
        err(k, "inf");
        err(k, "half");
    }

    #[test]
    fn toggles_text_and_themes() {
        assert_eq!(
            validate("tui.mouse", "off").unwrap(),
            toml::Value::Boolean(false)
        );
        assert_eq!(
            validate("tui.icons", "TRUE").unwrap(),
            toml::Value::Boolean(true)
        );
        err("tui.icons", "maybe");

        let k = "general.default_currency";
        assert_eq!(
            validate(k, "USD").unwrap(),
            toml::Value::String("USD".into())
        );
        err(k, "");
        err(k, "  ");
        err(k, "a\tb");
        err(k, &"x".repeat(TEXT_MAX_CHARS + 1));

        assert_eq!(
            validate("tui.theme", "gruvbox-dark").unwrap(),
            toml::Value::String("gruvbox-dark".into())
        );
        err("tui.theme", "Nord");
        err("tui.theme", "../x");
        err("tui.theme", "");
        err("delivery.uploader", "s3:x");
    }

    #[test]
    fn static_selects_check_membership() {
        let e = Entry {
            key: "x.y",
            section: "",
            label: "",
            kind: Kind::Select(SelectSource::Static(&["a", "b"])),
            help: "",
        };
        assert_eq!(
            validate_entry(&e, "b").unwrap(),
            toml::Value::String("b".into())
        );
        assert_eq!(
            validate_entry(&e, "c").unwrap_err().to_string(),
            "invalid input: x.y must be one of a, b"
        );
    }
}
