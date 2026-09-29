//! gig-core: the store behind the `gig` CLI.
//!
//! gig is driven by agents, never by hand. This crate owns the SQLite schema,
//! the package safety rules, the uploader, and every state transition. It
//! knows nothing about terminals or JSON envelopes; the CLI crate does that.

pub mod clock;
pub mod config;
pub mod context;
pub mod db;
pub mod delivery;
pub mod error;
pub mod models;
pub mod money;
pub mod package;
pub mod repo;
pub mod secrets;
pub mod services;
pub mod templates;

pub use error::{Error, Result};
