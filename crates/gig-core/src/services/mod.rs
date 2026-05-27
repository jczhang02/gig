//! Service layer. The only layer the CLI (and eventually the desktop) calls.
//! Composes repositories, enforces invariants, and manages transactions.

pub mod client_package;
pub mod dashboard;
pub mod import;
pub mod init;
pub mod lifecycle;
pub mod orders;
pub mod pack;
pub mod quote;
pub mod stats;
pub mod workflow;
