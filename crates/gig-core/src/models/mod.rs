pub mod client;
pub mod delivery_artifact;
pub mod delivery_package;
pub mod order;
pub mod order_workflow;
pub mod price_history;
pub mod project_type;
pub mod quote_draft;
pub mod requirement_change;
pub mod source;
pub mod tag;

pub use client::Client;
pub use delivery_artifact::DeliveryArtifact;
pub use delivery_package::{DeliveryPackage, DeliveryPackageStatus};
pub use order::{Order, OrderStatus};
pub use order_workflow::OrderWorkflow;
pub use price_history::PriceHistoryEntry;
pub use project_type::ProjectType;
pub use quote_draft::{QuoteDraft, QuoteDraftStatus};
pub use requirement_change::RequirementChange;
pub use source::Source;
pub use tag::Tag;

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn project_type_roundtrips_stable_strings() {
        assert_eq!(
            ProjectType::from_str("automation_script").unwrap(),
            ProjectType::AutomationScript
        );
        assert_eq!(ProjectType::AutomationScript.as_str(), "automation_script");
        assert_eq!(
            ProjectType::from_str("data_processing").unwrap(),
            ProjectType::DataProcessing
        );
        assert_eq!(
            ProjectType::from_str("crawler").unwrap(),
            ProjectType::Crawler
        );
        assert_eq!(ProjectType::from_str("cv_ml").unwrap(), ProjectType::CvMl);
        assert_eq!(
            ProjectType::from_str("frontend_web").unwrap(),
            ProjectType::FrontendWeb
        );
        assert_eq!(
            ProjectType::from_str("research_writing").unwrap(),
            ProjectType::ResearchWriting
        );
        assert_eq!(
            ProjectType::from_str("custom").unwrap(),
            ProjectType::Custom
        );
    }

    #[test]
    fn quote_draft_status_roundtrips_stable_strings() {
        assert_eq!(
            QuoteDraftStatus::from_str("quote_draft").unwrap(),
            QuoteDraftStatus::QuoteDraft
        );
        assert_eq!(QuoteDraftStatus::QuoteDraft.as_str(), "quote_draft");
        assert_eq!(
            QuoteDraftStatus::from_str("needs_clarification").unwrap(),
            QuoteDraftStatus::NeedsClarification
        );
        assert_eq!(
            QuoteDraftStatus::from_str("quoted").unwrap(),
            QuoteDraftStatus::Quoted
        );
        assert_eq!(
            QuoteDraftStatus::from_str("accepted").unwrap(),
            QuoteDraftStatus::Accepted
        );
        assert_eq!(
            QuoteDraftStatus::from_str("dropped").unwrap(),
            QuoteDraftStatus::Dropped
        );
    }

    #[test]
    fn delivery_package_status_roundtrips_stable_strings() {
        assert_eq!(
            DeliveryPackageStatus::from_str("prepared").unwrap(),
            DeliveryPackageStatus::Prepared
        );
        assert_eq!(DeliveryPackageStatus::Prepared.as_str(), "prepared");
        assert_eq!(
            DeliveryPackageStatus::from_str("validated").unwrap(),
            DeliveryPackageStatus::Validated
        );
        assert_eq!(
            DeliveryPackageStatus::from_str("sent").unwrap(),
            DeliveryPackageStatus::Sent
        );
        assert_eq!(
            DeliveryPackageStatus::from_str("cancelled").unwrap(),
            DeliveryPackageStatus::Cancelled
        );
    }
}
