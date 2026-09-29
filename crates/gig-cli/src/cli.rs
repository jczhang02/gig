//! Command-line surface. Every command prints JSON; see docs/v2/SPEC.md section 5.

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(
    name = "gig",
    version,
    about = "Agent-facing store for freelance orders. Output is always JSON."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    pub fn command_name(&self) -> String {
        match &self.command {
            Command::Draft(d) => format!("draft {}", d.name()),
            Command::New(_) => "new".into(),
            Command::Ls(_) => "ls".into(),
            Command::Show(_) => "show".into(),
            Command::Start(_) => "start".into(),
            Command::Change(_) => "change".into(),
            Command::Price(_) => "price".into(),
            Command::Note(_) => "note".into(),
            Command::Paid(_) => "paid".into(),
            Command::Scorecard(_) => "scorecard".into(),
            Command::Archive(_) => "archive".into(),
            Command::Cancel(_) => "cancel".into(),
            Command::Cd(_) => "cd".into(),
            Command::Delete(_) => "delete".into(),
            Command::Package(p) => format!("package {}", p.name()),
            Command::Artifact(a) => format!("artifact {}", a.name()),
            Command::Migrate(_) => "migrate".into(),
            Command::Doctor(_) => "doctor".into(),
            Command::Config(c) => format!("config {}", c.name()),
            Command::Backup => "backup".into(),
            Command::Completion(_) => "completion".into(),
            Command::Version => "version".into(),
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Pre-order drafts: notes in a hidden directory
    #[command(subcommand)]
    Draft(DraftCmd),
    /// Register an order and scaffold its project directory
    New(NewArgs),
    /// List orders with their next action
    Ls(LsArgs),
    /// Everything about one order
    Show(KeyArg),
    /// queued -> in_progress (or delivered -> in_progress for a revision)
    Start(KeyArg),
    /// Record a requirement change
    Change(ChangeArgs),
    /// Set the price with a reason
    Price(PriceArgs),
    /// Append a timestamped note
    Note(NoteArgs),
    /// delivered -> paid; starts the warranty period
    Paid(PaidArgs),
    /// Record the per-order scorecard
    Scorecard(ScorecardArgs),
    /// Move the project to the archive root (dry run without --yes)
    Archive(ArchiveArgs),
    /// queued | in_progress -> cancelled
    Cancel(CancelArgs),
    /// Print the order's directory
    Cd(KeyArg),
    /// Delete an order row (files untouched)
    Delete(DeleteArgs),
    /// Client packages: build, check, upload, sent, ls
    #[command(subcommand)]
    Package(PackageCmd),
    /// Single files shared outside a package
    #[command(subcommand)]
    Artifact(ArtifactCmd),
    /// Convert a v1 database into the v2 database
    Migrate(MigrateArgs),
    /// Consistency checks
    Doctor(DoctorArgs),
    /// Read or change config.toml
    #[command(subcommand)]
    Config(ConfigCmd),
    /// Copy the database into the backups directory
    Backup,
    /// Print a shell completion script
    Completion(CompletionArgs),
    /// Print the version
    Version,
}

#[derive(Args, Debug, Clone)]
pub struct KeyArg {
    /// Order slug or #id; resolved from the working directory when omitted
    pub key: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum DraftCmd {
    /// Create a draft and its NOTES.md
    New(DraftNewArgs),
    /// List open drafts
    Ls(DraftLsArgs),
    /// Snapshot the notes into the database and delete the directory
    Drop(DraftDropArgs),
}

impl DraftCmd {
    fn name(&self) -> &'static str {
        match self {
            DraftCmd::New(_) => "new",
            DraftCmd::Ls(_) => "ls",
            DraftCmd::Drop(_) => "drop",
        }
    }
}

#[derive(Args, Debug)]
pub struct DraftNewArgs {
    pub slug: String,
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long)]
    pub material: Option<String>,
    #[arg(long = "type", value_enum)]
    pub project_type: Option<ProjectTypeArg>,
}

#[derive(Args, Debug)]
pub struct DraftLsArgs {
    #[arg(long)]
    pub all: bool,
}

#[derive(Args, Debug)]
pub struct DraftDropArgs {
    pub slug: String,
    #[arg(long)]
    pub reason: String,
    #[arg(long)]
    pub yes: bool,
}

#[derive(ValueEnum, Debug, Clone, Copy)]
pub enum ProjectTypeArg {
    Tool,
    CvMl,
    DataProcessing,
    ResearchWriting,
    Custom,
}

impl ProjectTypeArg {
    pub fn to_model(self) -> gig_core::models::ProjectType {
        use gig_core::models::ProjectType as P;
        match self {
            ProjectTypeArg::Tool => P::Tool,
            ProjectTypeArg::CvMl => P::CvMl,
            ProjectTypeArg::DataProcessing => P::DataProcessing,
            ProjectTypeArg::ResearchWriting => P::ResearchWriting,
            ProjectTypeArg::Custom => P::Custom,
        }
    }
}

#[derive(ValueEnum, Debug, Clone, Copy)]
pub enum StatusArg {
    Queued,
    InProgress,
    Delivered,
    Paid,
}

impl StatusArg {
    pub fn to_model(self) -> gig_core::models::OrderStatus {
        use gig_core::models::OrderStatus as S;
        match self {
            StatusArg::Queued => S::Queued,
            StatusArg::InProgress => S::InProgress,
            StatusArg::Delivered => S::Delivered,
            StatusArg::Paid => S::Paid,
        }
    }
}

#[derive(Args, Debug)]
pub struct NewArgs {
    pub slug: String,
    #[arg(long)]
    pub title: String,
    /// Major units, up to two decimals (800 or 800.50)
    #[arg(long)]
    pub price: Option<String>,
    #[arg(long)]
    pub currency: Option<String>,
    #[arg(long)]
    pub cut_ratio: Option<f64>,
    #[arg(long = "type", value_enum, default_value = "custom")]
    pub project_type: ProjectTypeArg,
    /// Original materials (usually /mnt/virtiofs/<id>/); read-only
    #[arg(long)]
    pub material: Option<String>,
    #[arg(long)]
    pub platform: Option<String>,
    #[arg(long)]
    pub external_id: Option<String>,
    /// Verbatim acceptance message; @FILE reads a file, - reads stdin
    #[arg(long)]
    pub client_words: Option<String>,
    /// Promote the draft with the same slug (its notes go into JOB.md)
    #[arg(long)]
    pub from_draft: bool,
    /// Register an existing directory as is
    #[arg(long)]
    pub adopt: bool,
    /// Initial status for an adopted order
    #[arg(long, value_enum, requires = "adopt")]
    pub status: Option<StatusArg>,
    /// Register only; create no files
    #[arg(long)]
    pub no_scaffold: bool,
}

#[derive(Args, Debug)]
pub struct LsArgs {
    /// Include archived and cancelled orders
    #[arg(short, long)]
    pub all: bool,
}

#[derive(Args, Debug)]
pub struct ChangeArgs {
    pub key: Option<String>,
    #[arg(long)]
    pub desc: String,
    /// Major units; may be negative
    #[arg(long, default_value = "0")]
    pub price_delta: String,
}

#[derive(Args, Debug)]
pub struct PriceArgs {
    pub key: Option<String>,
    #[arg(long)]
    pub amount: String,
    #[arg(long)]
    pub reason: String,
}

#[derive(Args, Debug)]
pub struct NoteArgs {
    pub key: Option<String>,
    pub text: String,
}

#[derive(Args, Debug)]
pub struct PaidArgs {
    pub key: Option<String>,
    /// YYYY-MM-DD, default today
    #[arg(long)]
    pub date: Option<String>,
    /// Final amount when it differs from the recorded price
    #[arg(long)]
    pub amount: Option<String>,
}

#[derive(Args, Debug)]
pub struct ScorecardArgs {
    pub key: Option<String>,
    #[arg(long)]
    pub decisions: i64,
    #[arg(long)]
    pub repeat_questions: i64,
    #[arg(long)]
    pub cleanups: i64,
    #[arg(long)]
    pub report_reworks: i64,
    /// 1..5
    #[arg(long)]
    pub score: i64,
    #[arg(long)]
    pub note: Option<String>,
}

#[derive(Args, Debug)]
pub struct ArchiveArgs {
    pub key: Option<String>,
    #[arg(long)]
    pub yes: bool,
    #[arg(long)]
    pub before_warranty_end: bool,
    #[arg(long)]
    pub no_scorecard: bool,
    /// Delete the directory instead of moving it
    #[arg(long)]
    pub purge: bool,
}

#[derive(Args, Debug)]
pub struct CancelArgs {
    pub key: Option<String>,
    #[arg(long)]
    pub reason: String,
    #[arg(long)]
    pub yes: bool,
}

#[derive(Args, Debug)]
pub struct DeleteArgs {
    pub slug: String,
    #[arg(long)]
    pub yes: bool,
}

#[derive(Subcommand, Debug)]
pub enum PackageCmd {
    /// Write the zip from the manifest (and optionally the manifest from the directory), then check
    Build(PackageBuildArgs),
    /// Validate layout, manifest, directory and zip; record the check
    Check(PackageIdArgs),
    /// Upload the checked zip and record the link
    Upload(PackageUploadArgs),
    /// Record a package sent by hand (phone, other)
    Sent(PackageSentArgs),
    /// Packages of an order
    Ls(KeyArg),
}

impl PackageCmd {
    fn name(&self) -> &'static str {
        match self {
            PackageCmd::Build(_) => "build",
            PackageCmd::Check(_) => "check",
            PackageCmd::Upload(_) => "upload",
            PackageCmd::Sent(_) => "sent",
            PackageCmd::Ls(_) => "ls",
        }
    }
}

#[derive(ValueEnum, Debug, Clone, Copy)]
pub enum KindArg {
    Full,
    Preview,
}

#[derive(ValueEnum, Debug, Clone, Copy)]
pub enum ChannelArg {
    Phone,
    Other,
}

#[derive(Args, Debug)]
pub struct PackageBuildArgs {
    /// Order slug or #id, or omit to resolve from the working directory
    #[arg(long)]
    pub order: Option<String>,
    pub package_id: String,
    #[arg(long, value_enum, default_value = "full")]
    pub kind: KindArg,
    /// Derive the manifest from the package directory
    #[arg(long)]
    pub write_manifest: bool,
}

#[derive(Args, Debug)]
pub struct PackageIdArgs {
    #[arg(long)]
    pub order: Option<String>,
    pub package_id: String,
}

#[derive(Args, Debug)]
pub struct PackageUploadArgs {
    #[arg(long)]
    pub order: Option<String>,
    pub package_id: String,
    #[arg(long)]
    pub yes: bool,
}

#[derive(Args, Debug)]
pub struct PackageSentArgs {
    #[arg(long)]
    pub order: Option<String>,
    pub package_id: String,
    #[arg(long, value_enum)]
    pub channel: ChannelArg,
    #[arg(long)]
    pub note: Option<String>,
    #[arg(long)]
    pub yes: bool,
}

#[derive(Subcommand, Debug)]
pub enum ArtifactCmd {
    /// Upload one file
    Upload(ArtifactUploadArgs),
    /// Artifacts of an order
    Ls(KeyArg),
}

impl ArtifactCmd {
    fn name(&self) -> &'static str {
        match self {
            ArtifactCmd::Upload(_) => "upload",
            ArtifactCmd::Ls(_) => "ls",
        }
    }
}

#[derive(Args, Debug)]
pub struct ArtifactUploadArgs {
    #[arg(long)]
    pub order: Option<String>,
    pub file: String,
    #[arg(long)]
    pub yes: bool,
}

#[derive(Args, Debug)]
pub struct MigrateArgs {
    /// v1 database (default: gig.db next to the v2 database)
    #[arg(long)]
    pub from: Option<String>,
    /// v2 database to create (default: the configured v2 path)
    #[arg(long)]
    pub to: Option<String>,
    #[arg(long)]
    pub dry_run: bool,
    /// OLD=NEW path prefix rewrite for dev_path / archive_path; repeatable
    #[arg(long = "fix-path")]
    pub fix_paths: Vec<String>,
}

#[derive(Args, Debug)]
pub struct DoctorArgs {
    #[arg(long)]
    pub fix: bool,
}

#[derive(Subcommand, Debug)]
pub enum ConfigCmd {
    /// Print one value (dot notation)
    Get { key: String },
    /// Set one value
    Set { key: String, value: String },
    /// Print resolved paths
    Path,
    /// Move v1 secrets out of config.toml into secrets.toml
    SplitSecrets {
        #[arg(long)]
        yes: bool,
    },
}

impl ConfigCmd {
    fn name(&self) -> &'static str {
        match self {
            ConfigCmd::Get { .. } => "get",
            ConfigCmd::Set { .. } => "set",
            ConfigCmd::Path => "path",
            ConfigCmd::SplitSecrets { .. } => "split-secrets",
        }
    }
}

#[derive(Args, Debug)]
pub struct CompletionArgs {
    #[arg(value_enum)]
    pub shell: clap_complete::Shell,
}
