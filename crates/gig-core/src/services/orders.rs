//! Orders: registration, the state machine, money, notes, scorecards.

use crate::context;
use crate::models::{
    Artifact, Event, Order, OrderStatus, Package, PriceHistory, ProjectType, RequirementChange,
    Scorecard,
};
use crate::money::format_minor;
use crate::repo::{
    self, artifacts, drafts as draft_repo, events, orders as repo_orders, packages, scorecards,
};
use crate::services::{drafts, require_yes, scaffold, validate_slug, Ctx};
use crate::{clock, Error, Result};
use serde::Serialize;
use std::path::PathBuf;

pub struct NewOrderInput<'a> {
    pub slug: &'a str,
    pub title: &'a str,
    pub price_minor: Option<i64>,
    pub currency: Option<&'a str>,
    pub cut_ratio: Option<f64>,
    pub project_type: ProjectType,
    pub material_path: Option<&'a str>,
    pub platform: Option<&'a str>,
    pub external_id: Option<&'a str>,
    pub client_words: Option<&'a str>,
    pub from_draft: bool,
    pub adopt: bool,
    pub adopt_status: Option<OrderStatus>,
    pub no_scaffold: bool,
}

#[derive(Debug, Serialize)]
pub struct OrderCreated {
    pub order: Order,
    pub created_files: Vec<PathBuf>,
    pub warnings: Vec<String>,
    pub next_action: String,
}

pub fn new(ctx: &Ctx, input: &NewOrderInput<'_>) -> Result<OrderCreated> {
    validate_slug(input.slug)?;
    if let Some(r) = input.cut_ratio {
        if !(0.0..=1.0).contains(&r) {
            return Err(Error::InvalidInput(
                "cut ratio must be between 0 and 1".into(),
            ));
        }
    }
    if input.price_minor.is_some_and(|p| p < 0) {
        return Err(Error::InvalidInput("price must not be negative".into()));
    }
    let dev_path = ctx.config.general.dev_root.join(input.slug);
    let exists = dev_path.exists();
    if exists && !input.adopt {
        return Err(Error::InvalidInput(format!(
            "{} already exists; pass --adopt to register it as is",
            dev_path.display()
        )));
    }
    if input.adopt && !exists {
        return Err(Error::NotFound(format!(
            "--adopt needs an existing directory at {}",
            dev_path.display()
        )));
    }
    if input.adopt_status.is_some() && !input.adopt {
        return Err(Error::InvalidInput(
            "--status is only valid with --adopt".into(),
        ));
    }
    let draft = if input.from_draft {
        Some(drafts::open_draft(ctx, input.slug)?)
    } else {
        None
    };
    let draft_notes = draft.as_ref().and_then(drafts::read_notes);

    let now = clock::now();
    let status = input.adopt_status.unwrap_or(OrderStatus::Queued);
    let material = input
        .material_path
        .map(str::to_string)
        .or_else(|| draft.as_ref().and_then(|d| d.material_path.clone()));
    let started = matches!(
        status,
        OrderStatus::InProgress | OrderStatus::Delivered | OrderStatus::Paid
    );
    let order = repo_orders::insert(
        &ctx.conn,
        &repo_orders::NewOrder {
            id: None,
            slug: input.slug,
            title: input.title,
            material_path: material.as_deref(),
            platform: input.platform,
            external_id: input.external_id,
            project_type: input.project_type,
            status,
            currency: input
                .currency
                .unwrap_or(&ctx.config.general.default_currency),
            price_minor: input.price_minor,
            cut_ratio: input
                .cut_ratio
                .unwrap_or(ctx.config.general.default_cut_ratio),
            dev_path: Some(&dev_path.to_string_lossy()),
            archive_path: None,
            client_words: input.client_words,
            notes: "",
            created_at: &now,
            started_at: started.then_some(now.as_str()),
            delivered_at: None,
            paid_at: None,
            warranty_until: None,
            archived_at: None,
            cancelled_at: None,
            cancel_reason: None,
            legacy_id: None,
        },
    )?;

    let mut created_files = Vec::new();
    let mut warnings = Vec::new();
    if input.adopt {
        warnings.extend(scaffold::adoption_warnings(&dev_path));
        repo_orders::append_note(&ctx.conn, order.id, &now, "adopted existing directory")?;
    } else if !input.no_scaffold {
        match scaffold::create(&ctx.config, &order, draft_notes.as_deref()) {
            Ok(s) => {
                created_files = s.created;
                warnings.extend(s.warnings);
            }
            Err(e) => {
                repo_orders::delete(&ctx.conn, order.id)?;
                return Err(e);
            }
        }
    }
    if let Some(d) = &draft {
        drafts::promote(ctx, d, order.id)?;
    }
    let order = repo_orders::find_by_id(&ctx.conn, order.id)?;
    let next_action = order.next_action(&clock::today());
    Ok(OrderCreated {
        order,
        created_files,
        warnings,
        next_action,
    })
}

#[derive(Debug, Serialize)]
pub struct Listed {
    #[serde(flatten)]
    pub order: Order,
    pub next_action: String,
    pub days_in_status: Option<i64>,
    pub unpaid: bool,
}

pub fn list(ctx: &Ctx, all: bool) -> Result<Vec<Listed>> {
    let today = clock::today();
    let now = clock::now();
    repo_orders::list(&ctx.conn, all)?
        .into_iter()
        .map(|o| {
            let since = match o.status {
                OrderStatus::Queued => Some(&o.created_at),
                OrderStatus::InProgress => o.started_at.as_ref().or(Some(&o.created_at)),
                OrderStatus::Delivered => o.delivered_at.as_ref(),
                OrderStatus::Paid => o.paid_at.as_ref(),
                OrderStatus::Archived => o.archived_at.as_ref(),
                OrderStatus::Cancelled => o.cancelled_at.as_ref(),
            };
            let days_in_status = since.and_then(|s| clock::days_between(s, &now).ok());
            Ok(Listed {
                next_action: o.next_action(&today),
                days_in_status,
                unpaid: matches!(o.status, OrderStatus::Delivered),
                order: o,
            })
        })
        .collect()
}

#[derive(Debug, Serialize)]
pub struct Shown {
    pub order: Order,
    pub next_action: String,
    pub paths: ShownPaths,
    pub packages: Vec<Package>,
    pub artifacts: Vec<Artifact>,
    pub requirement_changes: Vec<RequirementChange>,
    pub price_history: Vec<PriceHistory>,
    pub events: Vec<Event>,
    pub scorecard: Option<Scorecard>,
}

#[derive(Debug, Serialize)]
pub struct ShownPaths {
    pub dev_path: Option<String>,
    pub archive_path: Option<String>,
    pub job_md: Option<String>,
    pub quote_md: Option<String>,
    pub delivery_dir: Option<String>,
}

pub fn show(ctx: &Ctx, key: Option<&str>) -> Result<Shown> {
    let order = context::resolve_key_or_cwd(&ctx.conn, key)?;
    let root = order
        .archive_path
        .clone()
        .filter(|_| order.status == OrderStatus::Archived)
        .or_else(|| order.dev_path.clone())
        .map(PathBuf::from);
    let sub = |s: &str| {
        root.as_ref()
            .map(|r| r.join(s).to_string_lossy().into_owned())
    };
    Ok(Shown {
        next_action: order.next_action(&clock::today()),
        paths: ShownPaths {
            dev_path: order.dev_path.clone(),
            archive_path: order.archive_path.clone(),
            job_md: sub(".gig/JOB.md"),
            quote_md: sub(".gig/QUOTE.md"),
            delivery_dir: sub("delivery"),
        },
        packages: packages::list_for_order(&ctx.conn, order.id)?,
        artifacts: artifacts::list_for_order(&ctx.conn, order.id)?,
        requirement_changes: repo::list_requirement_changes(&ctx.conn, order.id)?,
        price_history: repo::list_price_history(&ctx.conn, order.id)?,
        events: events::list_for_order(&ctx.conn, order.id)?,
        scorecard: scorecards::find(&ctx.conn, order.id)?,
        order,
    })
}

pub fn start(ctx: &Ctx, key: Option<&str>) -> Result<Order> {
    let o = context::resolve_key_or_cwd(&ctx.conn, key)?;
    let now = clock::now();
    match o.status {
        OrderStatus::Queued => {
            repo_orders::set_text(&ctx.conn, o.id, "started_at", Some(&now))?;
        }
        OrderStatus::Delivered => {
            repo_orders::append_note(&ctx.conn, o.id, &now, "revision started")?;
        }
        other => {
            return Err(Error::InvalidState(format!(
                "cannot start from {other}; start needs queued or delivered"
            )))
        }
    }
    repo_orders::set_status(&ctx.conn, o.id, OrderStatus::InProgress)?;
    repo_orders::find_by_id(&ctx.conn, o.id)
}

pub fn change(
    ctx: &Ctx,
    key: Option<&str>,
    description: &str,
    price_delta_minor: i64,
) -> Result<Shown> {
    let o = context::resolve_key_or_cwd(&ctx.conn, key)?;
    if !o.status.is_active() {
        return Err(Error::InvalidState(format!("order is {}", o.status)));
    }
    if description.trim().is_empty() {
        return Err(Error::InvalidInput("description is empty".into()));
    }
    let now = clock::now();
    repo::insert_requirement_change(&ctx.conn, o.id, description.trim(), price_delta_minor, &now)?;
    if price_delta_minor != 0 {
        let new = o.price_minor.unwrap_or(0) + price_delta_minor;
        repo::insert_price_history(
            &ctx.conn,
            o.id,
            o.price_minor,
            Some(new),
            Some(&format!("requirement change: {}", description.trim())),
            &now,
        )?;
        repo_orders::set_price(&ctx.conn, o.id, Some(new))?;
    }
    show(ctx, Some(&o.slug))
}

pub fn price(ctx: &Ctx, key: Option<&str>, new_minor: i64, reason: &str) -> Result<Order> {
    let o = context::resolve_key_or_cwd(&ctx.conn, key)?;
    if new_minor < 0 {
        return Err(Error::InvalidInput("price must not be negative".into()));
    }
    let now = clock::now();
    repo::insert_price_history(
        &ctx.conn,
        o.id,
        o.price_minor,
        Some(new_minor),
        Some(reason),
        &now,
    )?;
    repo_orders::set_price(&ctx.conn, o.id, Some(new_minor))?;
    repo_orders::find_by_id(&ctx.conn, o.id)
}

pub fn note(ctx: &Ctx, key: Option<&str>, text: &str) -> Result<Order> {
    let o = context::resolve_key_or_cwd(&ctx.conn, key)?;
    if text.trim().is_empty() {
        return Err(Error::InvalidInput("note is empty".into()));
    }
    repo_orders::append_note(&ctx.conn, o.id, &clock::now(), text)?;
    repo_orders::find_by_id(&ctx.conn, o.id)
}

pub fn paid(
    ctx: &Ctx,
    key: Option<&str>,
    date: Option<&str>,
    amount_minor: Option<i64>,
) -> Result<Order> {
    let o = context::resolve_key_or_cwd(&ctx.conn, key)?;
    if o.status != OrderStatus::Delivered {
        return Err(Error::InvalidState(format!(
            "paid needs delivered, order is {}",
            o.status
        )));
    }
    let date = match date {
        Some(d) => {
            clock::parse_date(d)?;
            d.to_string()
        }
        None => clock::today(),
    };
    let now = clock::now();
    if let Some(a) = amount_minor {
        if Some(a) != o.price_minor {
            repo::insert_price_history(
                &ctx.conn,
                o.id,
                o.price_minor,
                Some(a),
                Some("final amount paid"),
                &now,
            )?;
            repo_orders::set_price(&ctx.conn, o.id, Some(a))?;
        }
    }
    let until = clock::date_plus_days(&date, ctx.config.general.warranty_days)?;
    repo_orders::set_text(&ctx.conn, o.id, "paid_at", Some(&date))?;
    repo_orders::set_text(&ctx.conn, o.id, "warranty_until", Some(&until))?;
    repo_orders::set_status(&ctx.conn, o.id, OrderStatus::Paid)?;
    repo_orders::find_by_id(&ctx.conn, o.id)
}

#[derive(Debug, Serialize)]
pub struct CancelResult {
    pub order: Order,
    pub dry_run: bool,
}

pub fn cancel(ctx: &Ctx, key: Option<&str>, reason: &str, yes: bool) -> Result<CancelResult> {
    let o = context::resolve_key_or_cwd(&ctx.conn, key)?;
    if !matches!(o.status, OrderStatus::Queued | OrderStatus::InProgress) {
        return Err(Error::InvalidState(format!(
            "cancel needs queued or in_progress, order is {}",
            o.status
        )));
    }
    if reason.trim().is_empty() {
        return Err(Error::InvalidInput("reason is empty".into()));
    }
    if !yes {
        return Ok(CancelResult {
            order: o,
            dry_run: true,
        });
    }
    require_yes(yes, "cancel order")?;
    let now = clock::now();
    repo_orders::set_text(&ctx.conn, o.id, "cancelled_at", Some(&now))?;
    repo_orders::set_text(&ctx.conn, o.id, "cancel_reason", Some(reason.trim()))?;
    repo_orders::set_status(&ctx.conn, o.id, OrderStatus::Cancelled)?;
    Ok(CancelResult {
        order: repo_orders::find_by_id(&ctx.conn, o.id)?,
        dry_run: false,
    })
}

pub struct ScorecardInput {
    pub decisions: i64,
    pub repeat_questions: i64,
    pub cleanups: i64,
    pub report_reworks: i64,
    pub score: i64,
    pub note: Option<String>,
}

pub fn scorecard(ctx: &Ctx, key: Option<&str>, input: &ScorecardInput) -> Result<Scorecard> {
    let o = context::resolve_key_or_cwd(&ctx.conn, key)?;
    if !(1..=5).contains(&input.score) {
        return Err(Error::InvalidInput("score must be 1..5".into()));
    }
    for (name, v) in [
        ("decisions", input.decisions),
        ("repeat_questions", input.repeat_questions),
        ("cleanups", input.cleanups),
        ("report_reworks", input.report_reworks),
    ] {
        if v < 0 {
            return Err(Error::InvalidInput(format!("{name} must not be negative")));
        }
    }
    let days_to_preview = match (
        &o.started_at,
        events::first_at(&ctx.conn, o.id, events::PREVIEW_SENT)?,
    ) {
        (Some(start), Some(preview)) => Some(clock::days_between(start, &preview)?),
        _ => None,
    };
    scorecards::upsert(
        &ctx.conn,
        &Scorecard {
            order_id: o.id,
            decisions: Some(input.decisions),
            repeat_questions: Some(input.repeat_questions),
            days_to_preview,
            cleanups: Some(input.cleanups),
            check_rejections: Some(events::count(&ctx.conn, o.id, events::CHECK_REJECTED)?),
            report_reworks: Some(input.report_reworks),
            score: Some(input.score),
            note: input.note.clone(),
            created_at: clock::now(),
        },
    )
}

pub fn cd(ctx: &Ctx, key: Option<&str>) -> Result<String> {
    let o = context::resolve_key_or_cwd(&ctx.conn, key)?;
    let p = if o.status == OrderStatus::Archived {
        o.archive_path.or(o.dev_path)
    } else {
        o.dev_path.or(o.archive_path)
    };
    p.ok_or_else(|| Error::NotFound("order has no directory".into()))
}

pub fn delete(ctx: &Ctx, slug: &str, yes: bool) -> Result<Order> {
    let o = repo_orders::resolve(&ctx.conn, slug)?;
    require_yes(yes, "delete order row")?;
    if let Some(d) = draft_repo::list(&ctx.conn, true)?
        .into_iter()
        .find(|d| d.promoted_order_id == Some(o.id))
    {
        draft_repo::close(
            &ctx.conn,
            d.id,
            d.status,
            d.drop_reason.as_deref(),
            d.notes_snapshot.as_deref(),
            None,
            &clock::now(),
        )?;
    }
    repo_orders::delete(&ctx.conn, o.id)?;
    Ok(o)
}

pub fn describe_price(o: &Order) -> String {
    match o.price_minor {
        Some(p) => format!("{} {}", o.currency, format_minor(p)),
        None => "no price".into(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn make(ctx: &Ctx, slug: &str) -> Order {
        new(
            ctx,
            &NewOrderInput {
                slug,
                title: "Test",
                price_minor: Some(80000),
                currency: None,
                cut_ratio: None,
                project_type: ProjectType::Tool,
                material_path: Some("/mnt/x"),
                platform: None,
                external_id: None,
                client_words: Some("go"),
                from_draft: false,
                adopt: false,
                adopt_status: None,
                no_scaffold: false,
            },
        )
        .unwrap()
        .order
    }

    #[test]
    fn new_scaffolds_and_registers() {
        let root = tempfile::tempdir().unwrap();
        let ctx = Ctx::for_test(root.path());
        let created = new(
            &ctx,
            &NewOrderInput {
                slug: "tk",
                title: "TK",
                price_minor: Some(80000),
                currency: None,
                cut_ratio: None,
                project_type: ProjectType::Tool,
                material_path: Some("/mnt/092902"),
                platform: Some("platform-a"),
                external_id: None,
                client_words: Some("报价 800"),
                from_draft: false,
                adopt: false,
                adopt_status: None,
                no_scaffold: false,
            },
        )
        .unwrap();
        let dev = root.path().join("dev/tk");
        assert!(dev.join(".gig/JOB.md").is_file());
        assert!(dev.join(".gig/QUOTE.md").is_file());
        assert!(dev.join(".gitignore").is_file());
        assert!(std::fs::read_to_string(dev.join(".gig/QUOTE.md"))
            .unwrap()
            .contains("CNY 800.00"));
        assert_eq!(created.order.status, OrderStatus::Queued);
        assert_eq!(created.next_action, "start");
        assert_eq!(created.order.price.as_deref(), Some("800.00"));
        // second time: refuse without adopt
        assert!(make_result(&ctx, "tk").is_err());
        assert!(dev.join(".git").exists() || created.warnings.iter().any(|w| w.contains("git")));
    }

    fn make_result(ctx: &Ctx, slug: &str) -> Result<OrderCreated> {
        new(
            ctx,
            &NewOrderInput {
                slug,
                title: "T",
                price_minor: None,
                currency: None,
                cut_ratio: None,
                project_type: ProjectType::Custom,
                material_path: None,
                platform: None,
                external_id: None,
                client_words: None,
                from_draft: false,
                adopt: false,
                adopt_status: None,
                no_scaffold: false,
            },
        )
    }

    #[test]
    fn adopt_existing_directory() {
        let root = tempfile::tempdir().unwrap();
        let ctx = Ctx::for_test(root.path());
        let dev = root.path().join("dev/old");
        std::fs::create_dir_all(dev.join(".gig")).unwrap();
        std::fs::write(dev.join(".gig/JOB.md"), "x").unwrap();
        let created = new(
            &ctx,
            &NewOrderInput {
                slug: "old",
                title: "Old",
                price_minor: Some(80000),
                currency: None,
                cut_ratio: None,
                project_type: ProjectType::Tool,
                material_path: None,
                platform: None,
                external_id: None,
                client_words: None,
                from_draft: false,
                adopt: true,
                adopt_status: Some(OrderStatus::InProgress),
                no_scaffold: false,
            },
        )
        .unwrap();
        assert_eq!(created.order.status, OrderStatus::InProgress);
        assert!(created.order.started_at.is_some());
        assert!(created.created_files.is_empty());
        assert!(created.warnings.iter().any(|w| w.contains("QUOTE.md")));
        assert_eq!(
            std::fs::read_to_string(dev.join(".gig/JOB.md")).unwrap(),
            "x"
        );
    }

    #[test]
    fn from_draft_promotes_and_embeds_notes() {
        let root = tempfile::tempdir().unwrap();
        let ctx = Ctx::for_test(root.path());
        let d = drafts::new(&ctx, "job1", Some("J"), Some("/mnt/1"), None).unwrap();
        std::fs::write(&d.notes_path, "客户预算 800\n").unwrap();
        let created = new(
            &ctx,
            &NewOrderInput {
                slug: "job1",
                title: "Job 1",
                price_minor: Some(80000),
                currency: None,
                cut_ratio: None,
                project_type: ProjectType::Tool,
                material_path: None,
                platform: None,
                external_id: None,
                client_words: None,
                from_draft: true,
                adopt: false,
                adopt_status: None,
                no_scaffold: false,
            },
        )
        .unwrap();
        assert_eq!(created.order.material_path.as_deref(), Some("/mnt/1"));
        let job = std::fs::read_to_string(root.path().join("dev/job1/.gig/JOB.md")).unwrap();
        assert!(job.contains("客户预算 800"));
        assert!(!d.notes_path.exists());
        let drafts = draft_repo::list(&ctx.conn, true).unwrap();
        assert_eq!(drafts[0].promoted_order_id, Some(created.order.id));
    }

    #[test]
    fn state_machine() {
        let root = tempfile::tempdir().unwrap();
        let ctx = Ctx::for_test(root.path());
        let o = make(&ctx, "sm");
        assert_eq!(
            paid(&ctx, Some("sm"), None, None).unwrap_err().code(),
            "invalid_state"
        );
        let o2 = start(&ctx, Some(&o.slug)).unwrap();
        assert_eq!(o2.status, OrderStatus::InProgress);
        assert!(o2.started_at.is_some());
        assert_eq!(start(&ctx, Some("sm")).unwrap_err().code(), "invalid_state");
        // deliver by hand for the test
        repo_orders::set_status(&ctx.conn, o.id, OrderStatus::Delivered).unwrap();
        let o3 = paid(&ctx, Some("sm"), Some("2026-09-28"), Some(75000)).unwrap();
        assert_eq!(o3.status, OrderStatus::Paid);
        assert_eq!(o3.warranty_until.as_deref(), Some("2026-10-13"));
        assert_eq!(o3.price_minor, Some(75000));
        assert_eq!(repo::list_price_history(&ctx.conn, o.id).unwrap().len(), 1);
        assert_eq!(
            cancel(&ctx, Some("sm"), "x", true).unwrap_err().code(),
            "invalid_state"
        );
    }

    #[test]
    fn change_and_note_and_scorecard() {
        let root = tempfile::tempdir().unwrap();
        let ctx = Ctx::for_test(root.path());
        let o = make(&ctx, "ch");
        let shown = change(&ctx, Some("ch"), "add export", 20000).unwrap();
        assert_eq!(shown.order.price_minor, Some(100000));
        assert_eq!(shown.requirement_changes.len(), 1);
        note(&ctx, Some("ch"), "client asked for csv").unwrap();
        let shown = show(&ctx, Some("ch")).unwrap();
        assert!(shown.order.notes.contains("client asked for csv"));
        assert!(shown.paths.job_md.unwrap().ends_with(".gig/JOB.md"));
        let sc = scorecard(
            &ctx,
            Some("ch"),
            &ScorecardInput {
                decisions: 3,
                repeat_questions: 0,
                cleanups: 0,
                report_reworks: 1,
                score: 4,
                note: None,
            },
        )
        .unwrap();
        assert_eq!(sc.check_rejections, Some(0));
        assert_eq!(sc.order_id, o.id);
        let c = cancel(&ctx, Some("ch"), "client vanished", false).unwrap();
        assert!(c.dry_run);
        let c = cancel(&ctx, Some("ch"), "client vanished", true).unwrap();
        assert_eq!(c.order.status, OrderStatus::Cancelled);
        assert!(list(&ctx, false).unwrap().is_empty());
        assert_eq!(list(&ctx, true).unwrap().len(), 1);
    }
}
