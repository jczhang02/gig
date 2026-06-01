//! Local read-only GUI companion for gig.
//!
//! This crate deliberately depends on `gig-core`, not `gig-cli`. The CLI owns
//! only the thin `gig gui` entrypoint; GUI request handlers read through core
//! repositories and services directly.

pub mod api;
pub mod auth;
pub mod server;
pub mod static_server;

pub use server::{run, GuiOptions};
pub use static_server::{serve_static, StaticServeOptions};
