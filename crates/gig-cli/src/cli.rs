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

    /// Initialise a project folder for an accepted order.
    Init(InitArgs),

    /// Update the final price of an order.
    Price(PriceArgs),

    /// Record a requirement change.
    Change(ChangeArgs),

    /// Append a note to an order.
    Note(NoteArgs),

    /// Add tags to an order.
    Tag(TagArgs),

    /// Update the cut ratio for an order.
    Cut(CutArgs),

    /// Force-set the status of an order.
    Status(StatusArgs),

    /// Mark an order as paid.
    Paid(PaidArgs),

    /// Archive an order (move project folder to archive root).
    Archive(ArchiveArgs),

    /// Lead subcommands (promote / drop / ls).
    Lead(LeadArgs),

    /// Print the dev_path of an order (for shell `cd`).
    Cd(CdArgs),

    /// Run consistency checks on paths and data.
    Doctor,
}

// ─── Existing ─────────────────────────────────────────────────────────────────

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
    /// Order id or slug (omit to use context from current directory).
    pub id: Option<String>,
}

// ─── New commands ─────────────────────────────────────────────────────────────

#[derive(clap::Args, Debug)]
pub struct InitArgs {
    /// Order id or slug.
    pub id: String,

    /// Override the slug / folder name used for the project directory.
    #[arg(long)]
    pub slug: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct PriceArgs {
    /// Order id or slug (omit to use context from current directory).
    pub id: Option<String>,

    /// New final price in minor currency units (cents).
    #[arg(long)]
    pub amount: i64,

    /// Optional reason for the price change.
    #[arg(long)]
    pub reason: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct ChangeArgs {
    /// Order id or slug (omit to use context from current directory).
    pub id: Option<String>,

    /// Description of the requirement change.
    #[arg(long)]
    pub message: String,

    /// Price delta in minor units (positive = increase, negative = decrease).
    #[arg(long, default_value = "0")]
    pub delta: i64,
}

#[derive(clap::Args, Debug)]
pub struct NoteArgs {
    /// Order id or slug (omit to use context from current directory).
    pub id: Option<String>,

    /// Text to append to the order's notes.
    #[arg(long)]
    pub text: String,
}

#[derive(clap::Args, Debug)]
pub struct TagArgs {
    /// Order id or slug (omit to use context from current directory).
    pub id: Option<String>,

    /// Tags to add (space-separated).
    pub tags: Vec<String>,
}

#[derive(clap::Args, Debug)]
pub struct CutArgs {
    /// Order id or slug (omit to use context from current directory).
    pub id: Option<String>,

    /// New cut ratio (0.0 – 1.0).
    #[arg(long)]
    pub ratio: f64,
}

#[derive(clap::Args, Debug)]
pub struct StatusArgs {
    /// Order id or slug (omit to use context from current directory).
    pub id: Option<String>,

    /// Target status (lead, negotiating, accepted, in_progress, delivered, paid, archived, cancelled).
    #[arg(long)]
    pub status: String,

    /// Skip confirmation prompt.
    #[arg(long)]
    pub yes: bool,
}

#[derive(clap::Args, Debug)]
pub struct PaidArgs {
    /// Order id or slug (omit to use context from current directory).
    pub id: Option<String>,

    /// Override the paid date (unix timestamp). Defaults to now.
    #[arg(long)]
    pub on: Option<i64>,
}

#[derive(clap::Args, Debug)]
pub struct ArchiveArgs {
    /// Order id or slug (omit to use context from current directory).
    pub id: Option<String>,

    /// Skip confirmation prompt.
    #[arg(long)]
    pub yes: bool,
}

#[derive(clap::Args, Debug)]
pub struct CdArgs {
    /// Order id or slug (omit to use context from current directory).
    pub id: Option<String>,
}

// ─── Lead subgroup ────────────────────────────────────────────────────────────

#[derive(clap::Args, Debug)]
pub struct LeadArgs {
    #[command(subcommand)]
    pub command: LeadCommand,
}

#[derive(Subcommand, Debug)]
pub enum LeadCommand {
    /// Promote a lead to negotiating.
    Promote(LeadIdArgs),

    /// Drop a lead (cancel it).
    Drop(LeadIdArgs),

    /// List all leads.
    Ls,
}

#[derive(clap::Args, Debug)]
pub struct LeadIdArgs {
    /// Order id or slug.
    pub id: String,
}
