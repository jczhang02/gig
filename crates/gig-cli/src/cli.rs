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

    /// Pack a project into an archive (zip or tar.zst).
    Pack(PackArgs),

    /// Pack, upload, record artifact, and transition order to delivered.
    Deliver(DeliverArgs),

    /// Show income statistics.
    Stats(StatsArgs),

    /// Export orders to CSV or JSON.
    Export(ExportArgs),

    /// Client subcommands (ls / show).
    Client(ClientArgs),

    /// Template subcommands (ls / show / edit).
    Template(TemplateArgs),

    /// Config subcommands (get / set / edit).
    Config(ConfigArgs),

    /// Import existing project directories into the database.
    Import(ImportArgs),

    /// Snapshot the database to the backups directory.
    Backup,

    /// Source subcommands (add / ls).
    Source(SourceArgs),
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

    /// Source name (looks up existing source entity, auto-fills cut ratio).
    #[arg(long)]
    pub source: Option<String>,

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

#[derive(clap::Args, Debug)]
pub struct PackArgs {
    /// Order id or slug (omit to use context from current directory).
    pub id: Option<String>,

    /// Archive format: "zip" (default) or "tar.zst".
    #[arg(long, default_value = "zip")]
    pub format: String,

    /// Print the file list without creating the archive.
    #[arg(long)]
    pub dry_run: bool,

    /// Override the output path (default: $TMPDIR/gig-<slug>.<ext>).
    #[arg(long)]
    pub output: Option<std::path::PathBuf>,
}

#[derive(clap::Args, Debug)]
pub struct DeliverArgs {
    /// Order id or slug (omit to use context from current directory).
    pub id: Option<String>,

    /// Uploader name (overrides config default_uploader).
    #[arg(long)]
    pub uploader: Option<String>,

    /// Skip pack step and re-upload the most recent local archive.
    #[arg(long)]
    pub resend: bool,

    /// Archive format used when packing: "zip" or "tar.zst".
    #[arg(long, default_value = "zip")]
    pub format: String,
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

// ─── Stats ────────────────────────────────────────────────────────────────────

#[derive(clap::Args, Debug)]
pub struct StatsArgs {
    /// Date range as YYYY-MM..YYYY-MM (defaults to current month).
    #[arg(long)]
    pub range: Option<String>,

    /// Group breakdown by "tag" or "client".
    #[arg(long, value_name = "tag|client")]
    pub by: Option<String>,
}

// ─── Export ───────────────────────────────────────────────────────────────────

#[derive(clap::Args, Debug)]
pub struct ExportArgs {
    /// Format: csv or json.
    pub format: String,

    /// Output file path (defaults to stdout).
    #[arg(long)]
    pub output: Option<std::path::PathBuf>,
}

// ─── Client subgroup ──────────────────────────────────────────────────────────

#[derive(clap::Args, Debug)]
pub struct ClientArgs {
    #[command(subcommand)]
    pub command: ClientCommand,
}

#[derive(Subcommand, Debug)]
pub enum ClientCommand {
    /// List all clients.
    Ls,

    /// Show a client's details and their orders.
    Show(ClientIdArgs),
}

#[derive(clap::Args, Debug)]
pub struct ClientIdArgs {
    /// Client id.
    pub id: i64,
}

// ─── Template subgroup ────────────────────────────────────────────────────────

#[derive(clap::Args, Debug)]
pub struct TemplateArgs {
    #[command(subcommand)]
    pub command: TemplateCommand,
}

#[derive(Subcommand, Debug)]
pub enum TemplateCommand {
    /// List available templates.
    Ls,

    /// Print template content to stdout.
    Show(TemplateNameArgs),

    /// Open template in $EDITOR (copies embedded default if not in user dir yet).
    Edit(TemplateNameArgs),
}

#[derive(clap::Args, Debug)]
pub struct TemplateNameArgs {
    /// Template name (without extension), e.g. "project-readme".
    pub name: String,
}

// ─── Import ───────────────────────────────────────────────────────────────────

#[derive(clap::Args, Debug)]
pub struct ImportArgs {
    /// Directories to import (defaults to current directory if omitted).
    pub paths: Vec<std::path::PathBuf>,

    /// Prompt to confirm/override each inferred field.
    #[arg(long, short = 'i')]
    pub interactive: bool,

    /// Force a specific status for all imported projects (e.g. archived, in_progress).
    #[arg(long)]
    pub status: Option<String>,

    /// Move the directory to dev_root or archive_root after import (by status).
    #[arg(long)]
    pub relocate: bool,

    /// Print what would happen without writing to the database.
    #[arg(long)]
    pub dry_run: bool,
}

// ─── Source subgroup ──────────────────────────────────────────────────────────

#[derive(clap::Args, Debug)]
pub struct SourceArgs {
    #[command(subcommand)]
    pub command: SourceCommand,
}

#[derive(Subcommand, Debug)]
pub enum SourceCommand {
    /// Add a new source.
    Add(SourceAddArgs),

    /// List all sources.
    Ls,
}

#[derive(clap::Args, Debug)]
pub struct SourceAddArgs {
    /// Source name (e.g. "平台A").
    #[arg(long)]
    pub name: String,

    /// Commission cut ratio (0.0–1.0, e.g. 0.6 means 60% goes to you).
    #[arg(long)]
    pub cut_ratio: f64,

    /// Optional notes.
    #[arg(long)]
    pub notes: Option<String>,
}

// ─── Config subgroup ──────────────────────────────────────────────────────────

#[derive(clap::Args, Debug)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommand {
    /// Print a config value (dot-notation key).
    Get(ConfigGetArgs),

    /// Set a config value and save.
    Set(ConfigSetArgs),

    /// Open config.toml in $EDITOR.
    Edit,
}

#[derive(clap::Args, Debug)]
pub struct ConfigGetArgs {
    /// Key in dot-notation, e.g. "general.dev_root".
    pub key: String,
}

#[derive(clap::Args, Debug)]
pub struct ConfigSetArgs {
    /// Key in dot-notation.
    pub key: String,
    /// Value to set.
    pub value: String,
}
