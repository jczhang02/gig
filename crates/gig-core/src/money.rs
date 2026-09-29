//! Money is stored as integer minor units (cents). Input and output use decimal strings.

use crate::{Error, Result};

/// Parse "800", "800.5", "800.00" into minor units. At most two fractional digits.
pub fn parse_amount(text: &str) -> Result<i64> {
    let t = text.trim();
    let bad = || Error::InvalidInput(format!("invalid amount {text:?}, want e.g. 800 or 800.50"));
    if t.is_empty() {
        return Err(bad());
    }
    let (neg, body) = match t.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, t),
    };
    let (whole, frac) = match body.split_once('.') {
        Some((_, "")) => return Err(bad()),
        Some((w, f)) => (w, f),
        None => (body, ""),
    };
    if whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    if frac.len() > 2 || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    let whole: i64 = whole.parse().map_err(|_| bad())?;
    let frac: i64 = match frac.len() {
        0 => 0,
        1 => frac.parse::<i64>().map_err(|_| bad())? * 10,
        _ => frac.parse().map_err(|_| bad())?,
    };
    let minor = whole
        .checked_mul(100)
        .and_then(|w| w.checked_add(frac))
        .ok_or_else(bad)?;
    Ok(if neg { -minor } else { minor })
}

/// Format minor units as "800.00".
pub fn format_minor(minor: i64) -> String {
    let sign = if minor < 0 { "-" } else { "" };
    let abs = minor.unsigned_abs();
    format!("{sign}{}.{:02}", abs / 100, abs % 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_major_units() {
        assert_eq!(parse_amount("800").unwrap(), 80000);
        assert_eq!(parse_amount("800.5").unwrap(), 80050);
        assert_eq!(parse_amount("800.05").unwrap(), 80005);
        assert_eq!(parse_amount(" 0 ").unwrap(), 0);
        assert_eq!(parse_amount("-200").unwrap(), -20000);
    }

    #[test]
    fn rejects_bad_amounts() {
        for bad in ["", "abc", "800.123", "1,000", ".5", "800.", "8e2"] {
            assert!(parse_amount(bad).is_err(), "{bad} should fail");
        }
    }

    #[test]
    fn formats() {
        assert_eq!(format_minor(80000), "800.00");
        assert_eq!(format_minor(5), "0.05");
        assert_eq!(format_minor(-20050), "-200.50");
    }
}
