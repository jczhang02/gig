use super::ProjectType;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuoteDraftStatus {
    QuoteDraft,
    NeedsClarification,
    Quoted,
    Accepted,
    Dropped,
}

impl QuoteDraftStatus {
    pub const ALL: &'static [QuoteDraftStatus; 5] = &[
        QuoteDraftStatus::QuoteDraft,
        QuoteDraftStatus::NeedsClarification,
        QuoteDraftStatus::Quoted,
        QuoteDraftStatus::Accepted,
        QuoteDraftStatus::Dropped,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            QuoteDraftStatus::QuoteDraft => "quote_draft",
            QuoteDraftStatus::NeedsClarification => "needs_clarification",
            QuoteDraftStatus::Quoted => "quoted",
            QuoteDraftStatus::Accepted => "accepted",
            QuoteDraftStatus::Dropped => "dropped",
        }
    }
}

impl fmt::Display for QuoteDraftStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for QuoteDraftStatus {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "quote_draft" => QuoteDraftStatus::QuoteDraft,
            "needs_clarification" => QuoteDraftStatus::NeedsClarification,
            "quoted" => QuoteDraftStatus::Quoted,
            "accepted" => QuoteDraftStatus::Accepted,
            "dropped" => QuoteDraftStatus::Dropped,
            other => {
                return Err(Error::Invalid(format!(
                    "unknown quote draft status: {other}"
                )))
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuoteDraft {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub client_label: Option<String>,
    pub source_org: Option<String>,
    pub project_type: ProjectType,
    pub status: QuoteDraftStatus,
    pub summary: String,
    pub quote_min: Option<i64>,
    pub quote_recommended: Option<i64>,
    pub quote_max: Option<i64>,
    pub currency: String,
    pub xdg_path: String,
    pub drop_reason: Option<String>,
    pub promoted_order_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub quoted_at: Option<String>,
    pub sent_at: Option<String>,
    pub accepted_at: Option<String>,
    pub dropped_at: Option<String>,
}
