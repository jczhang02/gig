//! Dispatch: one clap command -> one service call -> one JSON value.

use crate::cli::*;
use clap::CommandFactory;
use gig_core::models::Channel;
use gig_core::money::parse_amount;
use gig_core::services::{
    archive, artifacts, doctor, drafts, migrate, orders, packages, split_secrets, Ctx,
};
use gig_core::{clock, delivery, Error, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::io::Read;
use std::path::PathBuf;

pub struct Output {
    pub data: Value,
    pub warnings: Vec<String>,
}

fn out<T: Serialize>(data: T) -> Result<Output> {
    Ok(Output {
        data: serde_json::to_value(data).map_err(|e| Error::InvalidInput(e.to_string()))?,
        warnings: vec![],
    })
}

fn out_with<T: Serialize>(data: T, warnings: Vec<String>) -> Result<Output> {
    Ok(Output {
        data: serde_json::to_value(data).map_err(|e| Error::InvalidInput(e.to_string()))?,
        warnings,
    })
}

fn text_arg(value: Option<String>) -> Result<Option<String>> {
    match value {
        None => Ok(None),
        Some(v) if v == "-" => {
            let mut s = String::new();
            std::io::stdin().read_to_string(&mut s)?;
            Ok(Some(s.trim_end().to_string()))
        }
        Some(v) => match v.strip_prefix('@') {
            Some(path) => Ok(Some(std::fs::read_to_string(path)?.trim_end().to_string())),
            None => Ok(Some(v)),
        },
    }
}

pub fn run(cli: Cli) -> Result<Output> {
    match cli.command {
        Command::Version => out(json!({ "version": env!("CARGO_PKG_VERSION") })),
        Command::Completion(a) => {
            let mut cmd = Cli::command();
            let mut buf = Vec::new();
            clap_complete::generate(a.shell, &mut cmd, "gig", &mut buf);
            out(json!({ "script": String::from_utf8_lossy(&buf) }))
        }
        Command::Config(c) => run_config(c),
        Command::Migrate(a) => run_migrate(a),
        other => {
            let ctx = Ctx::open()?;
            run_with_db(&ctx, other)
        }
    }
}

fn run_config(c: ConfigCmd) -> Result<Output> {
    let (paths, mut config) = Ctx::without_db()?;
    match c {
        ConfigCmd::Get { key } => out(json!({ "key": key, "value": config.get(&key)? })),
        ConfigCmd::Set { key, value } => {
            config.set(&key, &value)?;
            config.save(&paths.config_file)?;
            out(json!({ "key": key, "value": config.get(&key)? }))
        }
        ConfigCmd::Path => out(json!({
            "db_file": paths.db_file,
            "legacy_db_file": paths.legacy_db_file,
            "config_file": paths.config_file,
            "secrets_file": paths.secrets_file,
            "backups_dir": paths.backups_dir,
            "dev_root": config.general.dev_root,
            "archive_root": config.general.archive_root,
            "drafts_dir": config.general.drafts_dir(),
            "templates_dir": config.general.templates_dir,
        })),
        ConfigCmd::SplitSecrets { yes } => out(split_secrets::split(&paths, yes)?),
    }
}

fn run_migrate(a: MigrateArgs) -> Result<Output> {
    let paths = gig_core::config::Paths::from_env()?;
    paths.ensure_dirs()?;
    let config = gig_core::config::Config::load(&paths.config_file)?;
    let mut fix_paths = Vec::new();
    for f in a.fix_paths {
        let (old, new) = f
            .split_once('=')
            .ok_or_else(|| Error::InvalidInput(format!("--fix-path wants OLD=NEW, got {f:?}")))?;
        fix_paths.push((
            old.trim_end_matches('/').to_string(),
            new.trim_end_matches('/').to_string(),
        ));
    }
    let opts = migrate::Options {
        from: a
            .from
            .map(PathBuf::from)
            .unwrap_or_else(|| paths.legacy_db_file.clone()),
        to: a
            .to
            .map(PathBuf::from)
            .unwrap_or_else(|| paths.db_file.clone()),
        dry_run: a.dry_run,
        fix_paths,
        warranty_days: config.general.warranty_days,
    };
    let report = migrate::run(&opts)?;
    let warnings = report.warnings.clone();
    out_with(report, warnings)
}

fn run_with_db(ctx: &Ctx, cmd: Command) -> Result<Output> {
    match cmd {
        Command::Draft(d) => match d {
            DraftCmd::New(a) => out(drafts::new(
                ctx,
                &a.slug,
                a.title.as_deref(),
                a.material.as_deref(),
                a.project_type.map(ProjectTypeArg::to_model),
            )?),
            DraftCmd::Ls(a) => out(drafts::list(ctx, a.all)?),
            DraftCmd::Drop(a) => out(drafts::drop(ctx, &a.slug, &a.reason, a.yes)?),
        },
        Command::New(a) => {
            let price_minor = a.price.as_deref().map(parse_amount).transpose()?;
            let client_words = text_arg(a.client_words)?;
            let created = orders::new(
                ctx,
                &orders::NewOrderInput {
                    slug: &a.slug,
                    title: &a.title,
                    price_minor,
                    currency: a.currency.as_deref(),
                    cut_ratio: a.cut_ratio,
                    project_type: a.project_type.to_model(),
                    material_path: a.material.as_deref(),
                    platform: a.platform.as_deref(),
                    external_id: a.external_id.as_deref(),
                    client_words: client_words.as_deref(),
                    from_draft: a.from_draft,
                    adopt: a.adopt,
                    adopt_status: a.status.map(StatusArg::to_model),
                    no_scaffold: a.no_scaffold,
                },
            )?;
            let warnings = created.warnings.clone();
            out_with(created, warnings)
        }
        Command::Ls(a) => {
            out(json!({ "orders": orders::list(ctx, a.all)?, "today": clock::today() }))
        }
        Command::Show(a) => out(orders::show(ctx, a.key.as_deref())?),
        Command::Start(a) => out(orders::start(ctx, a.key.as_deref())?),
        Command::Change(a) => out(orders::change(
            ctx,
            a.key.as_deref(),
            &a.desc,
            parse_amount(&a.price_delta)?,
        )?),
        Command::Price(a) => out(orders::price(
            ctx,
            a.key.as_deref(),
            parse_amount(&a.amount)?,
            &a.reason,
        )?),
        Command::Note(a) => out(orders::note(ctx, a.key.as_deref(), &a.text)?),
        Command::Paid(a) => {
            let amount = a.amount.as_deref().map(parse_amount).transpose()?;
            out(orders::paid(
                ctx,
                a.key.as_deref(),
                a.date.as_deref(),
                amount,
            )?)
        }
        Command::Scorecard(a) => out(orders::scorecard(
            ctx,
            a.key.as_deref(),
            &orders::ScorecardInput {
                decisions: a.decisions,
                repeat_questions: a.repeat_questions,
                cleanups: a.cleanups,
                report_reworks: a.report_reworks,
                score: a.score,
                note: a.note,
            },
        )?),
        Command::Archive(a) => out(archive::archive(
            ctx,
            a.key.as_deref(),
            &archive::ArchiveOptions {
                yes: a.yes,
                before_warranty_end: a.before_warranty_end,
                no_scorecard: a.no_scorecard,
                purge: a.purge,
            },
        )?),
        Command::Cancel(a) => out(orders::cancel(ctx, a.key.as_deref(), &a.reason, a.yes)?),
        Command::Cd(a) => out(json!({ "path": orders::cd(ctx, a.key.as_deref())? })),
        Command::Delete(a) => out(orders::delete(ctx, &a.slug, a.yes)?),
        Command::Package(p) => match p {
            PackageCmd::Build(a) => {
                let kind = match a.kind {
                    KindArg::Full => gig_core::models::PackageKind::Full,
                    KindArg::Preview => gig_core::models::PackageKind::Preview,
                };
                let r = packages::build_package(
                    ctx,
                    a.order.as_deref(),
                    &a.package_id,
                    kind,
                    a.write_manifest,
                )?;
                let w = r.warnings.clone();
                out_with(r, w)
            }
            PackageCmd::Check(a) => {
                let r = packages::check(ctx, a.order.as_deref(), &a.package_id)?;
                let w = r.warnings.clone();
                out_with(r, w)
            }
            PackageCmd::Upload(a) => {
                if !a.yes {
                    // Dry run needs no uploader.
                    let r = packages::upload(
                        ctx,
                        a.order.as_deref(),
                        &a.package_id,
                        false,
                        &NoUploader,
                    )?;
                    let w = r.warnings.clone();
                    return out_with(r, w);
                }
                let uploader = delivery::configured_uploader(&ctx.config, &ctx.paths)?;
                let r = packages::upload(
                    ctx,
                    a.order.as_deref(),
                    &a.package_id,
                    true,
                    uploader.as_ref(),
                )?;
                let w = r.warnings.clone();
                out_with(r, w)
            }
            PackageCmd::Sent(a) => {
                let channel = match a.channel {
                    ChannelArg::Phone => Channel::Phone,
                    ChannelArg::Other => Channel::Other,
                };
                let r = packages::sent(
                    ctx,
                    a.order.as_deref(),
                    &a.package_id,
                    channel,
                    a.note.as_deref(),
                    a.yes,
                )?;
                let w = r.warnings.clone();
                out_with(r, w)
            }
            PackageCmd::Ls(a) => out(packages::list(ctx, a.key.as_deref())?),
        },
        Command::Artifact(ar) => match ar {
            ArtifactCmd::Upload(a) => {
                let file = PathBuf::from(&a.file);
                if !a.yes {
                    return out(artifacts::upload(
                        ctx,
                        a.order.as_deref(),
                        &file,
                        false,
                        &NoUploader,
                    )?);
                }
                let uploader = delivery::configured_uploader(&ctx.config, &ctx.paths)?;
                out(artifacts::upload(
                    ctx,
                    a.order.as_deref(),
                    &file,
                    true,
                    uploader.as_ref(),
                )?)
            }
            ArtifactCmd::Ls(a) => out(artifacts::list(ctx, a.key.as_deref())?),
        },
        Command::Doctor(a) => {
            let r = doctor::run(ctx, a.fix)?;
            let w: Vec<String> = r
                .problems
                .iter()
                .map(|p| format!("{}: {}", p.scope, p.message))
                .collect();
            out_with(r, w)
        }
        Command::Backup => {
            std::fs::create_dir_all(&ctx.paths.backups_dir)?;
            let name = format!("gig-v2-{}.db", clock::now_compact());
            let dest = ctx.paths.backups_dir.join(name);
            ctx.conn
                .execute("VACUUM INTO ?1", [dest.to_string_lossy().as_ref()])
                .map_err(Error::Db)?;
            out(json!({ "backup": dest }))
        }
        Command::Version | Command::Completion(_) | Command::Config(_) | Command::Migrate(_) => {
            unreachable!()
        }
    }
}

/// Placeholder for dry runs; never uploads.
struct NoUploader;

impl delivery::Uploader for NoUploader {
    fn name(&self) -> &str {
        "none"
    }
    fn upload(
        &self,
        _local: &std::path::Path,
        _opts: &delivery::UploadOpts,
    ) -> Result<delivery::UploadResult> {
        Err(Error::Upload("no uploader in a dry run".into()))
    }
}
