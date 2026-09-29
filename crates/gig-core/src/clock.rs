//! Timestamps and dates. Storage format: RFC 3339 UTC for instants, YYYY-MM-DD for dates.

use crate::{Error, Result};
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::{Date, Duration, OffsetDateTime};

const DATE: &[time::format_description::FormatItem<'static>] =
    format_description!("[year]-[month]-[day]");

pub fn now() -> String {
    OffsetDateTime::now_utc()
        .replace_nanosecond(0)
        .expect("zero nanoseconds is valid")
        .format(&Rfc3339)
        .expect("rfc3339 formatting cannot fail")
}

pub fn today() -> String {
    OffsetDateTime::now_utc()
        .date()
        .format(DATE)
        .expect("date formatting cannot fail")
}

/// A compact UTC stamp for object keys: 20260929T101500Z.
pub fn now_compact() -> String {
    let n = OffsetDateTime::now_utc();
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        n.year(),
        u8::from(n.month()),
        n.day(),
        n.hour(),
        n.minute(),
        n.second()
    )
}

pub fn parse_date(value: &str) -> Result<Date> {
    Date::parse(value.trim(), DATE)
        .map_err(|e| Error::InvalidInput(format!("invalid date {value:?}, want YYYY-MM-DD ({e})")))
}

pub fn parse_rfc3339(value: &str) -> Result<OffsetDateTime> {
    OffsetDateTime::parse(value.trim(), &Rfc3339)
        .map_err(|e| Error::InvalidInput(format!("invalid timestamp {value:?} ({e})")))
}

pub fn date_plus_days(date: &str, days: i64) -> Result<String> {
    let d = parse_date(date)? + Duration::days(days);
    Ok(d.format(DATE).expect("date formatting cannot fail"))
}

/// The date part of an RFC 3339 instant.
pub fn date_of(instant: &str) -> Result<String> {
    Ok(parse_rfc3339(instant)?
        .date()
        .format(DATE)
        .expect("date formatting cannot fail"))
}

/// Whole days from `earlier` to `later` (both RFC 3339). Negative when reversed.
pub fn days_between(earlier: &str, later: &str) -> Result<i64> {
    let a = parse_rfc3339(earlier)?;
    let b = parse_rfc3339(later)?;
    Ok((b - a).whole_days())
}

/// Whole days from a date to today.
pub fn days_since_date(date: &str) -> Result<i64> {
    let d = parse_date(date)?;
    Ok((OffsetDateTime::now_utc().date() - d).whole_days())
}

/// v1 stored instants as epoch seconds (INTEGER), numeric strings, or RFC 3339 text.
pub fn legacy_to_rfc3339(value: &LegacyStamp) -> Result<String> {
    let dt = match value {
        LegacyStamp::Int(secs) => OffsetDateTime::from_unix_timestamp(*secs)
            .map_err(|e| Error::InvalidInput(format!("invalid epoch {secs} ({e})")))?,
        LegacyStamp::Text(text) => {
            let t = text.trim();
            if let Ok(secs) = t.parse::<i64>() {
                OffsetDateTime::from_unix_timestamp(secs)
                    .map_err(|e| Error::InvalidInput(format!("invalid epoch {t} ({e})")))?
            } else {
                parse_rfc3339(t)?
            }
        }
    };
    Ok(dt
        .replace_nanosecond(0)
        .expect("zero nanoseconds is valid")
        .format(&Rfc3339)
        .expect("rfc3339 formatting cannot fail"))
}

#[derive(Debug, Clone)]
pub enum LegacyStamp {
    Int(i64),
    Text(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_is_rfc3339_without_fraction() {
        let n = now();
        assert!(n.ends_with('Z'));
        assert!(!n.contains('.'));
        parse_rfc3339(&n).unwrap();
    }

    #[test]
    fn date_arithmetic() {
        assert_eq!(date_plus_days("2026-09-28", 15).unwrap(), "2026-10-13");
        assert_eq!(date_plus_days("2026-12-31", 1).unwrap(), "2027-01-01");
        assert!(parse_date("2026/09/28").is_err());
    }

    #[test]
    fn legacy_stamps_convert() {
        assert_eq!(
            legacy_to_rfc3339(&LegacyStamp::Int(1775606400)).unwrap(),
            "2026-04-08T00:00:00Z"
        );
        assert_eq!(
            legacy_to_rfc3339(&LegacyStamp::Text("1775606400".into())).unwrap(),
            "2026-04-08T00:00:00Z"
        );
        assert_eq!(
            legacy_to_rfc3339(&LegacyStamp::Text("2026-08-21T20:53:37.765944868Z".into())).unwrap(),
            "2026-08-21T20:53:37Z"
        );
        assert!(legacy_to_rfc3339(&LegacyStamp::Text("yesterday".into())).is_err());
    }

    #[test]
    fn compact_stamp_shape() {
        let c = now_compact();
        assert_eq!(c.len(), 16);
        assert!(c.ends_with('Z'));
    }
}
