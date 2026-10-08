//! Row types and enums. Everything here serializes straight into the JSON envelope.

use crate::money::format_minor;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! text_enum {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $text),+
                }
            }

            pub fn parse(text: &str) -> Result<Self> {
                match text.trim() {
                    $($text => Ok($name::$variant),)+
                    other => Err(Error::InvalidInput(format!(
                        "unknown {}: {other:?}; expected one of {}",
                        stringify!($name),
                        [$($text),+].join(", ")
                    ))),
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

text_enum!(ProjectType {
    Tool => "tool",
    CvMl => "cv_ml",
    DataProcessing => "data_processing",
    ResearchWriting => "research_writing",
    Custom => "custom",
});

text_enum!(OrderStatus {
    Queued => "queued",
    InProgress => "in_progress",
    Delivered => "delivered",
    Paid => "paid",
    Archived => "archived",
    Cancelled => "cancelled",
});

impl OrderStatus {
    pub fn is_active(self) -> bool {
        !matches!(self, OrderStatus::Archived | OrderStatus::Cancelled)
    }
}

text_enum!(DraftStatus {
    Open => "open",
    Promoted => "promoted",
    Dropped => "dropped",
});

text_enum!(PackageKind {
    Full => "full",
    Preview => "preview",
});

text_enum!(PackageStatus {
    Checked => "checked",
    Sent => "sent",
    Legacy => "legacy",
});

text_enum!(Channel {
    Oss => "oss",
    Pan => "pan",
    Phone => "phone",
    Other => "other",
});

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Order {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub material_path: Option<String>,
    pub platform: Option<String>,
    pub external_id: Option<String>,
    pub project_type: ProjectType,
    pub status: OrderStatus,
    pub currency: String,
    pub price_minor: Option<i64>,
    /// Decimal string of `price_minor`, for readers.
    pub price: Option<String>,
    pub cut_ratio: f64,
    pub dev_path: Option<String>,
    pub archive_path: Option<String>,
    pub client_words: Option<String>,
    pub notes: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub delivered_at: Option<String>,
    pub paid_at: Option<String>,
    pub warranty_until: Option<String>,
    pub archived_at: Option<String>,
    pub cancelled_at: Option<String>,
    pub cancel_reason: Option<String>,
    pub legacy_id: Option<i64>,
}

impl Order {
    pub fn with_price(mut self) -> Self {
        self.price = self.price_minor.map(format_minor);
        self
    }

    /// What the skill should do next. Computed, never stored.
    pub fn next_action(&self, today: &str) -> String {
        match self.status {
            OrderStatus::Queued => "start".into(),
            OrderStatus::InProgress => "preview or pack".into(),
            OrderStatus::Delivered => "collect payment".into(),
            OrderStatus::Paid => match &self.warranty_until {
                Some(until) if until.as_str() > today => format!("warranty until {until}"),
                _ => "archive".into(),
            },
            OrderStatus::Archived | OrderStatus::Cancelled => "none".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Draft {
    pub id: i64,
    pub slug: String,
    pub title: Option<String>,
    pub material_path: Option<String>,
    pub project_type: Option<ProjectType>,
    pub notes_dir: String,
    pub status: DraftStatus,
    pub drop_reason: Option<String>,
    pub notes_snapshot: Option<String>,
    pub promoted_order_id: Option<i64>,
    pub created_at: String,
    pub closed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Package {
    pub id: i64,
    pub order_id: i64,
    pub package_id: String,
    pub kind: PackageKind,
    pub dir: String,
    pub manifest_path: String,
    pub zip_path: String,
    pub zip_sha256: Option<String>,
    pub file_count: Option<i64>,
    pub status: PackageStatus,
    pub checked_at: Option<String>,
    pub sent_at: Option<String>,
    pub channel: Option<Channel>,
    pub uploader: Option<String>,
    pub remote_url: Option<String>,
    pub short_url: Option<String>,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Artifact {
    pub id: i64,
    pub order_id: i64,
    pub local_path: Option<String>,
    pub uploader: Option<String>,
    pub remote_url: Option<String>,
    pub short_url: Option<String>,
    pub expires_at: Option<String>,
    pub uploaded_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PriceHistory {
    pub id: i64,
    pub order_id: i64,
    pub old_minor: Option<i64>,
    pub new_minor: Option<i64>,
    pub reason: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RequirementChange {
    pub id: i64,
    pub order_id: i64,
    pub description: String,
    pub price_delta_minor: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Event {
    pub id: i64,
    pub order_id: i64,
    pub kind: String,
    pub detail: Option<String>,
    pub at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scorecard {
    pub order_id: i64,
    pub decisions: Option<i64>,
    pub repeat_questions: Option<i64>,
    pub days_to_preview: Option<i64>,
    pub cleanups: Option<i64>,
    pub check_rejections: Option<i64>,
    pub report_reworks: Option<i64>,
    pub score: Option<i64>,
    pub note: Option<String>,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_pan_parses() {
        assert_eq!(Channel::parse("pan").unwrap(), Channel::Pan);
    }

    #[test]
    fn enums_round_trip() {
        for s in OrderStatus::ALL {
            assert_eq!(OrderStatus::parse(s.as_str()).unwrap(), *s);
        }
        assert!(OrderStatus::parse("accepted").is_err());
        assert_eq!(
            serde_json::to_string(&ProjectType::CvMl).unwrap(),
            "\"cv_ml\""
        );
    }

    #[test]
    fn next_action_follows_warranty() {
        let mut o = sample();
        o.status = OrderStatus::Paid;
        o.warranty_until = Some("2026-10-13".into());
        assert_eq!(o.next_action("2026-10-01"), "warranty until 2026-10-13");
        assert_eq!(o.next_action("2026-10-13"), "archive");
        o.status = OrderStatus::Queued;
        assert_eq!(o.next_action("2026-10-01"), "start");
    }

    pub(crate) fn sample() -> Order {
        Order {
            id: 1,
            slug: "x".into(),
            title: "X".into(),
            material_path: None,
            platform: None,
            external_id: None,
            project_type: ProjectType::Tool,
            status: OrderStatus::Queued,
            currency: "CNY".into(),
            price_minor: Some(80000),
            price: None,
            cut_ratio: 0.6,
            dev_path: None,
            archive_path: None,
            client_words: None,
            notes: String::new(),
            created_at: "2026-09-28T00:00:00Z".into(),
            started_at: None,
            delivered_at: None,
            paid_at: None,
            warranty_until: None,
            archived_at: None,
            cancelled_at: None,
            cancel_reason: None,
            legacy_id: None,
        }
        .with_price()
    }
}
