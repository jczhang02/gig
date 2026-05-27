//! Repository layer: pure SQL ↔ model mapping.
//! No business logic, no cross-table composition. Services compose repos.

pub mod clients;
pub mod delivery_artifacts;
pub mod delivery_packages;
pub mod order_workflow;
pub mod orders;
pub mod price_history;
pub mod quote_drafts;
pub mod requirement_changes;
pub mod sources;
pub mod tags;
