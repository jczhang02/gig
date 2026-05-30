//! Shared action metadata for CLI and GUI adapters.
//!
//! Phase B intentionally defines the catalog and contracts only. Existing CLI
//! command handlers still own behavior until commands migrate one at a time.

mod catalog;
mod types;

pub use catalog::{all, find_by_cli_path, find_by_id};
pub use types::{
    ActionField, ActionFieldKind, ActionId, ActionKind, ActionMeta, ActionOutputKind,
    ActionPreflight, ConfirmationPolicy, SideEffect,
};
