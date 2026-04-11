//! CLI argument structure.

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "gig",
    version,
    about = "Track your freelance gig orders",
    long_about = None,
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Create a new order.
    New(NewArgs),

    /// List orders.
    #[command(visible_alias = "list")]
    Ls(LsArgs),

    /// Show a single order in detail.
    Show(ShowArgs),
}

#[derive(clap::Args, Debug)]
pub struct NewArgs {
    /// One-line title for the order.
    #[arg(long)]
    pub title: String,

    /// Short slug (used as the folder name later by `gig init`).
    #[arg(long)]
    pub slug: Option<String>,

    /// Quoted price in minor units (e.g. cents). Optional at create time.
    #[arg(long)]
    pub quoted_price: Option<i64>,

    /// Final price in minor units.
    #[arg(long)]
    pub final_price: Option<i64>,

    /// Override default cut ratio (e.g. 0.6).
    #[arg(long)]
    pub cut_ratio: Option<f64>,

    /// Currency ISO code. Defaults to config value.
    #[arg(long)]
    pub currency: Option<String>,

    /// Source group / org identifier.
    #[arg(long)]
    pub source_org: Option<String>,

    /// Notes.
    #[arg(long)]
    pub notes: Option<String>,

    /// Create as a lead (not yet accepted).
    #[arg(long)]
    pub lead: bool,
}

#[derive(clap::Args, Debug)]
pub struct LsArgs {
    /// Filter by status (e.g. lead, accepted, in_progress).
    #[arg(long)]
    pub status: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct ShowArgs {
    /// Order id or slug.
    pub id: String,
}
