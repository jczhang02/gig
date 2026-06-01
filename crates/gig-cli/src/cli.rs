//! CLI argument structure.

use clap::{Parser, Subcommand, ValueEnum};

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

impl Cli {
    pub fn wants_json_errors(&self) -> bool {
        match &self.command {
            Command::Ls(args) => args.json,
            Command::Show(args) => args.json,
            Command::Quote(args) => quote_wants_json(args),
            Command::Plan(args) => plan_wants_json(args),
            Command::Work(args) => work_wants_json(args),
            Command::Acceptance(args) => acceptance_wants_json(args),
            Command::Package(args) => package_wants_json(args),
            Command::Artifact(args) => artifact_wants_json(args),
            _ => false,
        }
    }
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
    Doctor(DoctorArgs),

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

    /// Delete an order from the database.
    Delete(DeleteArgs),

    /// Snapshot the database to the backups directory.
    Backup,

    /// Print shell completion script.
    Completion(CompletionArgs),

    /// Source subcommands (add / ls).
    Source(SourceArgs),

    /// Quote draft workflow subcommands.
    Quote(QuoteArgs),

    /// Workflow plan gate subcommands.
    Plan(PlanArgs),

    /// Workflow work-start gate subcommands.
    Work(WorkArgs),

    /// Workflow acceptance gate subcommands.
    Acceptance(AcceptanceArgs),

    /// Client package validation subcommands.
    Package(PackageArgs),

    /// Standalone order artifact upload subcommands.
    Artifact(ArtifactArgs),

    /// Serve workflow artifacts over a localhost URL.
    Serve(ServeArgs),

    /// Start the local read-only GUI companion.
    Gui(GuiArgs),
}

#[derive(clap::Args, Debug)]
pub struct ServeArgs {
    /// Order id or slug.
    pub id_or_slug: String,

    /// Localhost port to bind. Use 0 for a random free port.
    #[arg(long, default_value_t = 0)]
    pub port: u16,

    /// Open the printed URL in the default browser.
    #[arg(long)]
    pub open: bool,
}

#[derive(clap::Args, Debug)]
pub struct GuiArgs {
    /// Do not open a browser; only print the local URL.
    #[arg(long)]
    pub no_open: bool,

    /// Localhost port to bind. Use 0 for a random free port.
    #[arg(long, default_value_t = 0)]
    pub port: u16,
}

fn artifact_wants_json(args: &ArtifactArgs) -> bool {
    match &args.command {
        ArtifactCommand::Send(args) => args.json,
    }
}

fn package_wants_json(args: &PackageArgs) -> bool {
    match &args.command {
        PackageCommand::Check(args) => args.json,
        PackageCommand::Send(args) => args.json,
    }
}

fn work_wants_json(args: &WorkArgs) -> bool {
    match &args.command {
        WorkCommand::Start(args) => args.json,
    }
}

fn acceptance_wants_json(args: &AcceptanceArgs) -> bool {
    match &args.command {
        AcceptanceCommand::Check(args) => args.json,
        AcceptanceCommand::Complete(args) => args.json,
    }
}

fn plan_wants_json(args: &PlanArgs) -> bool {
    match &args.command {
        PlanCommand::Ready(args) => args.json,
        PlanCommand::Approve(args) => args.json,
        PlanCommand::Reject(args) => args.json,
    }
}

fn quote_wants_json(args: &QuoteArgs) -> bool {
    match &args.command {
        QuoteCommand::New(args) => args.json,
        QuoteCommand::Show(args) => args.json,
        QuoteCommand::List(args) => args.json,
        QuoteCommand::Price(args) => args.json,
        QuoteCommand::MarkSent(args) => args.json,
        QuoteCommand::Accept(args) => args.json,
        QuoteCommand::Drop(args) => args.json,
    }
}

#[derive(clap::Args, Debug)]
pub struct CompletionArgs {
    /// Target shell type.
    pub shell: CompletionShell,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum CompletionShell {
    Zsh,
}

// ─── Existing ─────────────────────────────────────────────────────────────────

#[derive(clap::Args, Debug)]
pub struct NewArgs {
    /// One-line title for the order.
    #[arg(long)]
    pub title: String,

    /// Short slug used to identify the order.
    #[arg(long)]
    pub slug: Option<String>,

    /// Quoted price in yuan (e.g. 1200 or 1200.50). Optional at create time.
    #[arg(long)]
    pub quoted_price: Option<String>,

    /// Final price in yuan (e.g. 1200 or 1200.50).
    #[arg(long)]
    pub final_price: Option<String>,

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

    /// Show all orders including archived and cancelled.
    #[arg(long, short)]
    pub all: bool,

    /// Emit stable machine-readable JSON for the default decision board.
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct ShowArgs {
    /// Order id or slug (omit to use context from current directory).
    pub id: Option<String>,

    /// Emit stable machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct PackageArgs {
    #[command(subcommand)]
    pub command: PackageCommand,
}

#[derive(Subcommand, Debug)]
pub enum PackageCommand {
    /// Validate workflow-created package without uploading.
    Check(PackageCheckArgs),

    /// Validate and upload workflow-created package, then mark sent.
    Send(PackageSendArgs),
}

#[derive(clap::Args, Debug)]
pub struct PackageCheckArgs {
    /// Order id or slug.
    pub id_or_slug: String,

    /// Delivery date matching manifest.toml.
    #[arg(long)]
    pub delivery_date: String,

    /// Existing .gig delivery directory.
    #[arg(long)]
    pub delivery_dir: std::path::PathBuf,

    /// Emit stable machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct PackageSendArgs {
    /// Order id or slug.
    pub id_or_slug: String,

    /// Delivery date matching manifest.toml.
    #[arg(long)]
    pub delivery_date: String,

    /// Existing .gig delivery directory.
    #[arg(long)]
    pub delivery_dir: std::path::PathBuf,

    /// Uploader name (overrides config default_uploader).
    #[arg(long)]
    pub uploader: Option<String>,

    /// Emit stable machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct ArtifactArgs {
    #[command(subcommand)]
    pub command: ArtifactCommand,
}

#[derive(Subcommand, Debug)]
pub enum ArtifactCommand {
    /// Upload standalone files and record artifacts without changing order status.
    Send(ArtifactSendArgs),
}

#[derive(clap::Args, Debug)]
pub struct ArtifactSendArgs {
    /// Order id or slug.
    pub id_or_slug: String,

    /// Files to upload and attach to the order.
    #[arg(required = true, num_args = 1..)]
    pub files: Vec<std::path::PathBuf>,

    /// Uploader name (overrides config default_uploader).
    #[arg(long)]
    pub uploader: Option<String>,

    /// Emit stable machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct AcceptanceArgs {
    #[command(subcommand)]
    pub command: AcceptanceCommand,
}

#[derive(Subcommand, Debug)]
pub enum AcceptanceCommand {
    /// Check workflow-created acceptance evidence.
    Check(AcceptanceCheckArgs),

    /// Complete acceptance and mark the order ready to deliver.
    Complete(AcceptanceCompleteArgs),
}

#[derive(clap::Args, Debug)]
pub struct AcceptanceCheckArgs {
    /// Order id or slug.
    pub id_or_slug: String,

    /// Emit stable machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct AcceptanceCompleteArgs {
    /// Order id or slug.
    pub id_or_slug: String,

    /// Emit stable machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct PlanArgs {
    #[command(subcommand)]
    pub command: PlanCommand,
}

#[derive(Subcommand, Debug)]
pub enum PlanCommand {
    /// Mark workflow-created plan files ready for review.
    Ready(PlanReadyArgs),

    /// Approve a ready plan.
    Approve(PlanApproveArgs),

    /// Reject a ready plan and record the reason.
    Reject(PlanRejectArgs),
}

#[derive(clap::Args, Debug)]
pub struct PlanApproveArgs {
    /// Order id or slug.
    pub id_or_slug: String,

    /// Emit stable machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct PlanRejectArgs {
    /// Order id or slug.
    pub id_or_slug: String,

    /// Non-empty reason for rejection.
    #[arg(long)]
    pub reason: String,

    /// Emit stable machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct PlanReadyArgs {
    /// Order id or slug.
    pub id_or_slug: String,

    /// Emit stable machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct WorkArgs {
    #[command(subcommand)]
    pub command: WorkCommand,
}

#[derive(Subcommand, Debug)]
pub enum WorkCommand {
    /// Start execution after the plan is approved.
    Start(WorkStartArgs),
}

#[derive(clap::Args, Debug)]
pub struct WorkStartArgs {
    /// Order id or slug.
    pub id_or_slug: String,

    /// Emit stable machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct QuoteArgs {
    #[command(subcommand)]
    pub command: QuoteCommand,
}

#[derive(Subcommand, Debug)]
pub enum QuoteCommand {
    /// Record a pre-acceptance quote draft.
    New(QuoteNewArgs),

    /// Show a quote draft.
    Show(QuoteShowArgs),

    /// List quote drafts.
    List(QuoteListArgs),

    /// Price a quote draft.
    Price(QuotePriceArgs),

    /// Mark a priced quote as sent outside gig.
    MarkSent(QuoteMarkSentArgs),

    /// Accept a priced quote and register workflow paths.
    Accept(QuoteAcceptArgs),

    /// Drop a quote draft.
    Drop(QuoteDropArgs),
}

#[derive(clap::Args, Debug)]
pub struct QuoteShowArgs {
    /// Quote draft id or slug.
    pub id_or_slug: String,

    /// Emit stable machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct QuoteListArgs {
    /// Emit stable machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct QuotePriceArgs {
    /// Quote draft id or slug.
    pub id_or_slug: String,

    /// Minimum quote in yuan.
    #[arg(long)]
    pub min: String,

    /// Recommended quote in yuan.
    #[arg(long)]
    pub recommended: String,

    /// Maximum quote in yuan.
    #[arg(long)]
    pub max: String,

    /// Emit stable machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct QuoteMarkSentArgs {
    /// Quote draft id or slug.
    pub id_or_slug: String,

    /// Emit stable machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct QuoteAcceptArgs {
    /// Quote draft id or slug.
    pub id_or_slug: String,

    /// Existing formal project directory containing .gig/JOB.md and .gig/QUOTE.md.
    #[arg(long)]
    pub project_dir: std::path::PathBuf,

    /// Developer cut ratio for the promoted order.
    #[arg(long, default_value_t = 0.6)]
    pub my_cut_ratio: f64,

    /// Emit stable machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct QuoteDropArgs {
    /// Quote draft id or slug.
    pub id_or_slug: String,

    /// Non-empty reason for dropping the draft.
    #[arg(long)]
    pub reason: String,

    /// Emit stable machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct QuoteNewArgs {
    /// Stable quote draft slug.
    #[arg(long)]
    pub slug: String,

    /// One-line quote title.
    #[arg(long)]
    pub title: String,

    /// Partjob project type, e.g. crawler or frontend_web.
    #[arg(long)]
    pub project_type: String,

    /// Short problem or scope summary.
    #[arg(long)]
    pub summary: String,

    /// Human-readable client label.
    #[arg(long)]
    pub client_label: Option<String>,

    /// Source group / org identifier.
    #[arg(long)]
    pub source_org: Option<String>,

    /// Emit stable machine-readable JSON.
    #[arg(long)]
    pub json: bool,
}

// ─── New commands ─────────────────────────────────────────────────────────────

#[derive(clap::Args, Debug)]
pub struct PriceArgs {
    /// Order id or slug (omit to use context from current directory).
    pub id: Option<String>,

    /// New final price in yuan (e.g. 1200 or 1200.50).
    #[arg(long)]
    pub amount: String,

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

    /// Price delta in yuan (positive = increase, negative = decrease). E.g. 300 means ¥300.
    #[arg(long, default_value = "0", allow_hyphen_values = true)]
    pub delta: String,
}

#[derive(clap::Args, Debug)]
pub struct NoteArgs {
    /// Order id or slug, OR the note text when used alone (context resolves id).
    pub id_or_text: String,

    /// Note text (when the first positional is the order id).
    pub text_if_id: Option<String>,
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
    /// Order id or slug, OR target status when used alone (context resolves id).
    pub id_or_status: String,

    /// Target status (when the first positional is the order id).
    pub status_if_id: Option<String>,

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

    /// Delete local files instead of moving them to archive_root.
    #[arg(long)]
    pub purge: bool,
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

    /// Re-import even if the path is already in the database (deletes the old entry first).
    #[arg(long)]
    pub force: bool,
}

// ─── Doctor ─────────────────────────────────────────────────────────────────

#[derive(clap::Args, Debug)]
pub struct DoctorArgs {
    /// Attempt to fix broken paths by searching dev_root and archive_root.
    #[arg(long)]
    pub fix: bool,
}

// ─── Delete ─────────────────────────────────────────────────────────────────

#[derive(clap::Args, Debug)]
pub struct DeleteArgs {
    /// Order id or slug.
    pub id: String,

    /// Skip confirmation prompt.
    #[arg(long)]
    pub yes: bool,
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
    /// Source name (e.g. "PlatformA").
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
