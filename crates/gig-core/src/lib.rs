//! gig-core: pure business logic for the gig CLI.
//!
//! This crate owns the SQLite schema, models, repositories, and services.
//! It must not depend on any CLI or UI layer.

pub mod actions;
pub mod config;
pub mod context;
pub mod db;
pub mod delivery;
pub mod error;
pub mod models;
pub mod repo;
pub mod services;
pub mod templates;

pub use error::{Error, Result};
