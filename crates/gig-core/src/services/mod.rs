//! Service layer. The only layer the CLI (and eventually the desktop) calls.
//! Composes repositories, enforces invariants, and manages transactions.

pub mod dashboard;
pub mod init;
pub mod lifecycle;
pub mod orders;
pub mod pack;
pub mod stats;
