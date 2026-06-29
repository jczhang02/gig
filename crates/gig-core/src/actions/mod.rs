//! Shared action metadata for CLI and GUI adapters.
//!
//! Phase B intentionally defines the catalog and contracts only. Existing CLI
//! command handlers still own behavior until commands migrate one at a time.

mod catalog;
mod context;
mod preflight;
mod read;
mod types;

pub use catalog::{all, find_by_cli_path, find_by_id};
pub use context::{ActionContext, ActionContextSeed};
pub use preflight::preflight_action;
pub use read::{
    execute_read_action, get_dashboard, get_order_detail, get_redacted_config,
    get_redacted_config_for_paths, is_read_action, list_orders, DashboardOrderView,
    DashboardResponse, DashboardSummaryView, OrderResponse, OrderView, OrderWithWorkflowView,
    OrdersResponse, QuoteDashboardView, QuoteDraftView, ReadActionInput, ReadActionOutput,
    RedactedConfigResponse, RedactedDelivery, RedactedGeneral, RedactedPack, RedactedPaths,
    RedactedS3, RedactedShortLink, WorkflowView,
};
pub use types::{
    ActionField, ActionFieldKind, ActionId, ActionKind, ActionMeta, ActionOutputKind,
    ActionPreflight, ConfirmationPolicy, SideEffect,
};
