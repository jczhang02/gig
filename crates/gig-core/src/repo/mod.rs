//! Repository layer: pure SQL ↔ model mapping.
//! No business logic, no cross-table composition. Services compose repos.

pub mod clients;
pub mod delivery_artifacts;
pub mod orders;
pub mod price_history;
pub mod requirement_changes;
pub mod tags;
