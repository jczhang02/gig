//! View models read from gig-core in one pass on every refresh (spec
//! section 4). Everything that depends on the date takes `today` so tests are
//! deterministic; `Snapshot::load` is the only place that reads the clock.

pub mod job_md;
pub mod money;

use gig_core::clock;
use gig_core::models::{
    Artifact, Draft, DraftStatus, Event, Order, OrderStatus, Package, RequirementChange, Scorecard,
};
use gig_core::repo::{self, artifacts, drafts, events, orders, packages, scorecards};
use gig_core::services::Ctx;
use gig_core::Result;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub use job_md::JobMd;
pub use money::Money;

/// Everything the views draw.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    /// `YYYY-MM-DD` the snapshot was computed for.
    pub today: String,
    /// All orders, archived and cancelled included, in database order
    /// (newest id first). Use `active_orders` / `history` for the views.
    pub orders: Vec<OrderRow>,
    /// All drafts, newest first. Use `open_drafts` for the Drafts view.
    pub drafts: Vec<Draft>,
    pub money: Money,
    /// `[general] default_currency`; empty means CNY.
    pub currency: String,
}

/// One order with everything its row and detail need.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderRow {
    pub order: Order,
    pub next_action: String,
    pub group: Group,
    /// Calendar days since the order entered its current status.
    pub days_in_status: Option<i64>,
    pub packages: Vec<Package>,
    pub artifacts: Vec<Artifact>,
    pub requirement_changes: Vec<RequirementChange>,
    /// Parsed from `orders.notes`, oldest first.
    pub notes: Vec<Note>,
    pub scorecard: Option<Scorecard>,
    pub events: Vec<Event>,
    pub job: JobMd,
}

/// Groups of the Orders list, in display order (spec 2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    /// Delivered, not paid.
    Unpaid,
    /// Paid. Includes orders whose warranty has ended and wait for archiving
    /// (next action "archive"): they sit with the warranty group, after
    /// unpaid and before work in progress.
    Warranty,
    InProgress,
    Queued,
    /// Archived and cancelled; only shown with `a` and in History.
    Closed,
}

impl Group {
    pub fn of(status: OrderStatus) -> Self {
        match status {
            OrderStatus::Delivered => Group::Unpaid,
            OrderStatus::Paid => Group::Warranty,
            OrderStatus::InProgress => Group::InProgress,
            OrderStatus::Queued => Group::Queued,
            OrderStatus::Archived | OrderStatus::Cancelled => Group::Closed,
        }
    }
}

/// One line of `orders.notes`, written by gig as `[<instant>] <text>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// The stamp between the brackets; empty for text gig did not stamp.
    pub at: String,
    pub text: String,
}

/// Parsed JOB.md files kept between refreshes, keyed by path and checked
/// by modification time, so a refresh re-reads only files that changed
/// (most orders, archived ones above all, never change).
#[derive(Debug, Default)]
pub struct JobCache {
    entries: HashMap<PathBuf, (Option<SystemTime>, JobMd)>,
}

impl JobCache {
    /// The parsed file, re-read only when its mtime differs from last time.
    pub fn get(&mut self, path: &Path) -> JobMd {
        let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        if let Some((seen, job)) = self.entries.get(path) {
            if mtime.is_some() && *seen == mtime {
                return job.clone();
            }
        }
        let job = JobMd::load(path);
        self.entries
            .insert(path.to_path_buf(), (mtime, job.clone()));
        job
    }

    /// Drop entries for paths no order uses any more.
    fn retain(&mut self, used: &HashSet<PathBuf>) {
        self.entries.retain(|p, _| used.contains(p));
    }
}

impl Snapshot {
    /// Read everything for the current day, with no JOB.md cache.
    pub fn load(ctx: &Ctx) -> Result<Self> {
        Self::load_cached(ctx, &clock::today(), &mut JobCache::default())
    }

    /// Read everything, computing day counts and money against `today`.
    pub fn load_at(ctx: &Ctx, today: &str) -> Result<Self> {
        Self::load_cached(ctx, today, &mut JobCache::default())
    }

    /// `load_at` reusing JOB.md parses from earlier refreshes.
    pub fn load_cached(ctx: &Ctx, today: &str, cache: &mut JobCache) -> Result<Self> {
        let conn = &ctx.conn;
        let mut rows = Vec::new();
        let mut used = HashSet::new();
        for order in orders::list(conn, true)? {
            let id = order.id;
            let job = match job_md_path(&order) {
                Some(p) => {
                    let job = cache.get(&p);
                    used.insert(p);
                    job
                }
                None => JobMd::default(),
            };
            rows.push(OrderRow {
                next_action: order.next_action(today),
                group: Group::of(order.status),
                days_in_status: days_in_status(&order, today),
                packages: packages::list_for_order(conn, id)?,
                artifacts: artifacts::list_for_order(conn, id)?,
                requirement_changes: repo::list_requirement_changes(conn, id)?,
                notes: parse_notes(&order.notes),
                scorecard: scorecards::find(conn, id)?,
                events: events::list_for_order(conn, id)?,
                job,
                order,
            });
        }
        cache.retain(&used);
        let raw: Vec<Order> = rows.iter().map(|r| r.order.clone()).collect();
        Ok(Self {
            today: today.to_string(),
            money: Money::compute(&raw, today),
            orders: rows,
            drafts: drafts::list(conn, true)?,
            currency: ctx.config.general.default_currency.clone(),
        })
    }

    /// The Orders list: active orders (plus closed ones when `show_closed`,
    /// the `a` toggle), sorted as spec 2.1.
    pub fn active_orders(&self, show_closed: bool) -> Vec<&OrderRow> {
        let mut v: Vec<&OrderRow> = self
            .orders
            .iter()
            .filter(|r| show_closed || r.group != Group::Closed)
            .collect();
        sort_orders(&mut v);
        v
    }

    /// The History view: every order, newest first.
    pub fn history(&self) -> Vec<&OrderRow> {
        let mut v: Vec<&OrderRow> = self.orders.iter().collect();
        v.sort_by(|a, b| {
            b.order
                .created_at
                .cmp(&a.order.created_at)
                .then(b.order.id.cmp(&a.order.id))
        });
        v
    }

    pub fn open_drafts(&self) -> impl Iterator<Item = &Draft> {
        self.drafts.iter().filter(|d| d.status == DraftStatus::Open)
    }

    /// The default currency code for labels.
    pub fn currency(&self) -> &str {
        if self.currency.is_empty() {
            "CNY"
        } else {
            &self.currency
        }
    }

    pub fn order(&self, id: i64) -> Option<&OrderRow> {
        self.orders.iter().find(|r| r.order.id == id)
    }
}

/// Spec 2.1: unpaid, in warranty, in progress, queued (then closed); within a
/// group by days in status descending. Unknown days sort last in their group
/// and ties fall back to the newer id, so rows do not jump between refreshes.
pub fn sort_orders(rows: &mut [&OrderRow]) {
    rows.sort_by(|a, b| compare_rows(a, b));
}

fn compare_rows(a: &OrderRow, b: &OrderRow) -> Ordering {
    // Closed orders: archived before cancelled, each under its heading.
    let cancelled = |r: &OrderRow| r.order.status == OrderStatus::Cancelled;
    a.group
        .cmp(&b.group)
        .then_with(|| cancelled(a).cmp(&cancelled(b)))
        .then_with(|| {
            b.days_in_status
                .unwrap_or(i64::MIN)
                .cmp(&a.days_in_status.unwrap_or(i64::MIN))
        })
        .then(b.order.id.cmp(&a.order.id))
}

/// `<root>/.gig/JOB.md`, where root is the archive path for archived orders
/// (as `gig show` does) and the dev path otherwise.
pub fn job_md_path(order: &Order) -> Option<PathBuf> {
    let root = order
        .archive_path
        .as_deref()
        .filter(|_| order.status == OrderStatus::Archived)
        .or(order.dev_path.as_deref())?;
    Some(Path::new(root).join(".gig").join("JOB.md"))
}

/// The file `e` opens: `<dev_path>/.gig/JOB.md` (spec 2.1), only when it
/// exists, so the editor never creates a file the TUI should not write.
pub fn job_md_edit_path(order: &Order) -> std::result::Result<PathBuf, String> {
    let dev = order
        .dev_path
        .as_deref()
        .ok_or_else(|| format!("{} has no project directory", order.slug))?;
    let path = Path::new(dev).join(".gig").join("JOB.md");
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!("{} does not exist", path.display()))
    }
}

/// Calendar days since the timestamp of the current status. Unlike `gig ls`
/// this also works for `paid_at`, which is a plain date.
pub fn days_in_status(o: &Order, today: &str) -> Option<i64> {
    let since = match o.status {
        OrderStatus::Queued => Some(&o.created_at),
        OrderStatus::InProgress => o.started_at.as_ref().or(Some(&o.created_at)),
        OrderStatus::Delivered => o.delivered_at.as_ref(),
        OrderStatus::Paid => o.paid_at.as_ref(),
        OrderStatus::Archived => o.archived_at.as_ref(),
        OrderStatus::Cancelled => o.cancelled_at.as_ref(),
    }?;
    calendar_days(since, today)
}

/// The `YYYY-MM-DD` part of a date or an RFC 3339 instant.
pub(crate) fn day_part(stamp: &str) -> Option<&str> {
    let d = stamp.trim().get(..10)?;
    clock::parse_date(d).ok().map(|_| d)
}

/// Whole calendar days from `stamp` (date or instant) to `today`.
pub(crate) fn calendar_days(stamp: &str, today: &str) -> Option<i64> {
    let from = clock::parse_date(day_part(stamp)?).ok()?;
    let to = clock::parse_date(today).ok()?;
    Some((to - from).whole_days())
}

/// Split `orders.notes` into entries. Lines not starting with a `[stamp]`
/// continue the previous note (a note text may contain newlines).
pub fn parse_notes(notes: &str) -> Vec<Note> {
    let mut out: Vec<Note> = Vec::new();
    for line in notes.lines() {
        let stamped = line
            .strip_prefix('[')
            .and_then(|rest| rest.split_once("] "))
            .filter(|(at, _)| day_part(at).is_some());
        match (stamped, out.last_mut()) {
            (Some((at, text)), _) => out.push(Note {
                at: at.to_string(),
                text: text.to_string(),
            }),
            (None, Some(last)) => {
                last.text.push('\n');
                last.text.push_str(line);
            }
            (None, None) if !line.trim().is_empty() => out.push(Note {
                at: String::new(),
                text: line.to_string(),
            }),
            (None, None) => {}
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use gig_core::models::ProjectType;

    pub(crate) fn order(id: i64, status: OrderStatus, price_minor: i64) -> Order {
        Order {
            id,
            slug: format!("o{id}"),
            title: format!("Order {id}"),
            material_path: None,
            platform: None,
            external_id: None,
            project_type: ProjectType::Tool,
            status,
            currency: "CNY".into(),
            price_minor: Some(price_minor),
            price: None,
            cut_ratio: 0.6,
            dev_path: None,
            archive_path: None,
            client_words: None,
            notes: String::new(),
            created_at: "2026-09-01T00:00:00Z".into(),
            started_at: None,
            delivered_at: None,
            paid_at: None,
            warranty_until: None,
            archived_at: None,
            cancelled_at: None,
            cancel_reason: None,
            legacy_id: None,
        }
        .with_price()
    }

    fn row(o: Order, today: &str) -> OrderRow {
        OrderRow {
            next_action: o.next_action(today),
            group: Group::of(o.status),
            days_in_status: days_in_status(&o, today),
            packages: vec![],
            artifacts: vec![],
            requirement_changes: vec![],
            notes: vec![],
            scorecard: None,
            events: vec![],
            job: JobMd::default(),
            order: o,
        }
    }

    /// The sort-test snapshot, for view and key tests elsewhere in the crate.
    pub(crate) fn sample_snapshot(today: &str) -> Snapshot {
        snapshot(today)
    }

    fn snapshot(today: &str) -> Snapshot {
        let mut v = Vec::new();
        let mut q_old = order(1, OrderStatus::Queued, 1);
        q_old.created_at = "2026-09-01T00:00:00Z".into();
        v.push(q_old);
        let mut q_new = order(2, OrderStatus::Queued, 1);
        q_new.created_at = "2026-09-20T00:00:00Z".into();
        v.push(q_new);
        let mut w = order(3, OrderStatus::Paid, 1);
        w.paid_at = Some("2026-09-25".into());
        w.warranty_until = Some("2026-10-10".into());
        v.push(w);
        let mut due = order(4, OrderStatus::Paid, 1);
        due.paid_at = Some("2026-08-01".into());
        due.warranty_until = Some("2026-08-16".into());
        v.push(due);
        let mut p = order(5, OrderStatus::InProgress, 1);
        p.started_at = Some("2026-09-10T09:00:00Z".into());
        v.push(p);
        let mut u1 = order(6, OrderStatus::Delivered, 1);
        u1.delivered_at = Some("2026-09-28T23:00:00Z".into());
        v.push(u1);
        let mut u2 = order(7, OrderStatus::Delivered, 1);
        u2.delivered_at = Some("2026-08-01T00:00:00Z".into());
        v.push(u2);
        let mut a = order(8, OrderStatus::Archived, 1);
        a.archived_at = Some("2026-09-01T00:00:00Z".into());
        a.created_at = "2026-07-01T00:00:00Z".into();
        v.push(a);
        let mut c = order(9, OrderStatus::Cancelled, 1);
        c.cancelled_at = Some("2026-09-02T00:00:00Z".into());
        c.created_at = "2026-09-26T00:00:00Z".into();
        v.push(c);
        // Unknown days in the unpaid group sort last within it.
        v.push(order(10, OrderStatus::Delivered, 1));
        Snapshot {
            today: today.into(),
            money: Money::compute(&v, today),
            orders: v.into_iter().rev().map(|o| row(o, today)).collect(),
            drafts: vec![],
            currency: "CNY".into(),
        }
    }

    fn ids(rows: &[&OrderRow]) -> Vec<i64> {
        rows.iter().map(|r| r.order.id).collect()
    }

    #[test]
    fn orders_sort_by_group_then_days() {
        let s = snapshot("2026-09-29");
        let list = s.active_orders(false);
        // unpaid (59 days, 1 day, unknown), paid (archive due 59, warranty 4),
        // in progress, queued (28 days, 9 days).
        assert_eq!(ids(&list), vec![7, 6, 10, 4, 3, 5, 1, 2]);
        let groups: Vec<Group> = list.iter().map(|r| r.group).collect();
        assert!(groups.windows(2).all(|w| w[0] <= w[1]));
        assert_eq!(list[0].days_in_status, Some(59));
        assert_eq!(list[1].days_in_status, Some(1));
        assert_eq!(list[3].next_action, "archive");
        assert_eq!(list[4].next_action, "warranty until 2026-10-10");
        assert_eq!(list[4].days_in_status, Some(4));
    }

    #[test]
    fn closed_orders_only_with_toggle() {
        let s = snapshot("2026-09-29");
        let all = s.active_orders(true);
        assert_eq!(all.len(), 10);
        // Archived 28 days, cancelled 27 days: closed group last.
        assert_eq!(ids(&all[8..]), vec![8, 9]);
    }

    #[test]
    fn history_is_newest_first() {
        let s = snapshot("2026-09-29");
        let h = s.history();
        assert_eq!(h.len(), 10);
        assert_eq!(h[0].order.id, 9);
        assert_eq!(h[1].order.id, 2);
        assert_eq!(h.last().unwrap().order.id, 8);
    }

    #[test]
    fn days_accept_dates_and_instants() {
        assert_eq!(calendar_days("2026-09-28", "2026-09-29"), Some(1));
        assert_eq!(calendar_days("2026-09-28T23:59:59Z", "2026-09-29"), Some(1));
        assert_eq!(calendar_days("2025-12-31", "2026-01-01"), Some(1));
        assert_eq!(calendar_days("bogus", "2026-09-29"), None);
        assert_eq!(day_part("2026-09-02"), Some("2026-09-02"));
        assert_eq!(day_part("2026-9-2"), None);
    }

    #[test]
    fn notes_split_on_stamps() {
        let text = "[2026-09-01T10:00:00Z] first\n[2026-09-02T10:00:00Z] second\nwith a second line\n[2026-09-03T00:00:00Z] [bracket] text\n";
        let n = parse_notes(text);
        assert_eq!(n.len(), 3);
        assert_eq!(n[0].at, "2026-09-01T10:00:00Z");
        assert_eq!(n[1].text, "second\nwith a second line");
        assert_eq!(n[2].text, "[bracket] text");
        let legacy = parse_notes("free text from v1\n");
        assert_eq!(legacy[0].at, "");
        assert_eq!(legacy[0].text, "free text from v1");
        assert!(parse_notes("").is_empty());
    }

    #[test]
    fn job_md_path_follows_status() {
        let mut o = order(1, OrderStatus::InProgress, 1);
        assert_eq!(job_md_path(&o), None);
        o.dev_path = Some("/dev/x".into());
        o.archive_path = Some("/archive/x".into());
        assert_eq!(job_md_path(&o).unwrap(), Path::new("/dev/x/.gig/JOB.md"));
        o.status = OrderStatus::Archived;
        assert_eq!(
            job_md_path(&o).unwrap(),
            Path::new("/archive/x/.gig/JOB.md")
        );
    }

    #[test]
    fn load_reads_repos_and_job_md() {
        use gig_core::config::{Config, Paths};
        use gig_core::repo::orders::NewOrder;

        let dir = tempfile::tempdir().unwrap();
        let dev = dir.path().join("dev").join("tk");
        std::fs::create_dir_all(dev.join(".gig")).unwrap();
        std::fs::write(
            dev.join(".gig").join("JOB.md"),
            "## 待客户确认\n\n- 2026-09-20: 输出 PNG 吗?\n\n## 状态\n\n- 2026-09-21: 预览完成.\n",
        )
        .unwrap();
        let ctx = Ctx {
            paths: Paths::under_root(dir.path()),
            config: Config::default(),
            conn: gig_core::db::open_in_memory().unwrap(),
        };
        let dev_s = dev.to_string_lossy().into_owned();
        let order = orders::insert(
            &ctx.conn,
            &NewOrder {
                id: None,
                slug: "tk",
                title: "图像去噪",
                material_path: None,
                platform: None,
                external_id: None,
                project_type: gig_core::models::ProjectType::Tool,
                status: OrderStatus::Delivered,
                currency: "CNY",
                price_minor: Some(80000),
                cut_ratio: 0.6,
                dev_path: Some(&dev_s),
                archive_path: None,
                client_words: None,
                notes: "",
                created_at: "2026-09-01T00:00:00Z",
                started_at: None,
                delivered_at: Some("2026-09-22T00:00:00Z"),
                paid_at: None,
                warranty_until: None,
                archived_at: None,
                cancelled_at: None,
                cancel_reason: None,
                legacy_id: None,
            },
        )
        .unwrap();
        orders::append_note(&ctx.conn, order.id, "2026-09-23T00:00:00Z", "client ok").unwrap();
        repo::insert_requirement_change(
            &ctx.conn,
            order.id,
            "add TIFF",
            5000,
            "2026-09-24T00:00:00Z",
        )
        .unwrap();

        let s = Snapshot::load_at(&ctx, "2026-09-29").unwrap();
        assert_eq!(s.today, "2026-09-29");
        let r = s.order(order.id).unwrap();
        assert_eq!(r.group, Group::Unpaid);
        assert_eq!(r.days_in_status, Some(7));
        assert_eq!(r.next_action, "collect payment");
        assert!(r.job.found);
        assert_eq!(r.job.latest_status(), Some("2026-09-21: 预览完成."));
        assert_eq!(r.job.open_questions, vec!["2026-09-20: 输出 PNG 吗?"]);
        assert_eq!(r.notes.len(), 1);
        assert_eq!(r.requirement_changes.len(), 1);
        assert_eq!(s.money.outstanding.gross, 80000);
        assert_eq!(s.money.outstanding.take_home, 48000);
        assert!(s.drafts.is_empty());
    }
}
