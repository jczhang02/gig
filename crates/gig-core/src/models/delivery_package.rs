use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryPackageStatus {
    Prepared,
    Validated,
    Sent,
    Cancelled,
}

impl DeliveryPackageStatus {
    pub const ALL: &'static [DeliveryPackageStatus; 4] = &[
        DeliveryPackageStatus::Prepared,
        DeliveryPackageStatus::Validated,
        DeliveryPackageStatus::Sent,
        DeliveryPackageStatus::Cancelled,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            DeliveryPackageStatus::Prepared => "prepared",
            DeliveryPackageStatus::Validated => "validated",
            DeliveryPackageStatus::Sent => "sent",
            DeliveryPackageStatus::Cancelled => "cancelled",
        }
    }
}

impl fmt::Display for DeliveryPackageStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DeliveryPackageStatus {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "prepared" => DeliveryPackageStatus::Prepared,
            "validated" => DeliveryPackageStatus::Validated,
            "sent" => DeliveryPackageStatus::Sent,
            "cancelled" => DeliveryPackageStatus::Cancelled,
            other => {
                return Err(Error::Invalid(format!(
                    "unknown delivery package status: {other}"
                )))
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeliveryPackage {
    pub id: i64,
    pub order_id: i64,
    pub delivery_date: String,
    pub delivery_dir: String,
    pub client_dir: String,
    pub manifest_path: String,
    pub package_path: Option<String>,
    pub status: DeliveryPackageStatus,
    pub created_at: String,
    pub updated_at: String,
}
