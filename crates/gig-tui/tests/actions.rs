//! Action flows driven by key events against gig-core in a temp home (spec
//! section 6): the same service calls the CLI makes, no terminal involved.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use gig_core::clock;
use gig_core::config::{Config, Paths};
use gig_core::db;
use gig_core::models::{DraftStatus, OrderStatus, ProjectType};
use gig_core::repo::orders as repo_orders;
use gig_core::services::{orders, Ctx};
use gig_tui::actions::{self, Action, Effect};
use gig_tui::app::{Outcome, UiState, View};
use gig_tui::data::Snapshot;
use gig_tui::popup::Popup;
use std::path::Path;
use tempfile::TempDir;

/// A gig home under `root`: database file, config with a temp dev root, and
/// minimal templates (the skill's real ones live outside this crate).
fn test_ctx(root: &Path) -> Ctx {
    let paths = Paths::under_root(root);
    paths.ensure_dirs().unwrap();
    let mut config = Config::default();
    config.general.dev_root = root.join("dev");
    config.general.archive_root = root.join("archive");
    config.general.drafts_dir = std::path::PathBuf::new();
    config.general.templates_dir = root.join("templates");
    std::fs::create_dir_all(&config.general.dev_root).unwrap();
    let t = &config.general.templates_dir;
    std::fs::create_dir_all(t).unwrap();
    for (name, text) in [
        ("NOTES.md.j2", "# {{ slug }}\n\nmaterial: {{ material_path }}\n"),
        (
            "JOB.md.j2",
            "# {{ title }}\n\n- slug: {{ slug }}\n{% if client_words %}\n## Client words\n\n{{ client_words }}\n{% endif %}{% if draft_notes %}\n## Draft notes\n\n{{ draft_notes }}\n{% endif %}",
        ),
        ("QUOTE.md.j2", "# Quote\n\n- price: {{ currency }} {{ price }}\n"),
        ("AGENTS.md.j2", "# Agents\n\nRead .gig/JOB.md first.\n"),
        ("README.md.j2", "# {{ title }}\n"),
        ("gitignore", ".venv/\ndelivery/\n"),
    ] {
        std::fs::write(t.join(name), text).unwrap();
    }
    let conn = db::open(&paths.db_file).unwrap();
    Ctx {
        paths,
        config,
        conn,
    }
}

struct Harness {
    _dir: TempDir,
    ctx: Ctx,
    ui: UiState,
    /// Effects other than calls, for assertions.
    effects: Vec<Effect>,
}

impl Harness {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let ctx = test_ctx(dir.path());
        let mut h = Self {
            _dir: dir,
            ctx,
            ui: UiState::default(),
            effects: Vec::new(),
        };
        h.refresh();
        h
    }

    fn refresh(&mut self) {
        self.ui.data = Snapshot::load(&self.ctx).unwrap();
    }

    /// One key through the app's routing; calls run as `App::apply` runs
    /// them (perform, then refresh).
    fn key(&mut self, code: KeyCode) {
        let key = KeyEvent::new(code, KeyModifiers::NONE);
        match self.ui.handle_key(key, 200) {
            Outcome::Act(Effect::Call(action)) => {
                actions::perform(&self.ctx, &mut self.ui, &action);
                self.refresh();
            }
            Outcome::Act(Effect::None) => {}
            Outcome::Act(e) => self.effects.push(e),
            Outcome::Refresh => self.refresh(),
            _ => {}
        }
    }

    fn chars(&mut self, s: &str) {
        for c in s.chars() {
            self.key(KeyCode::Char(c));
        }
    }

    fn backspaces(&mut self, n: usize) {
        for _ in 0..n {
            self.key(KeyCode::Backspace);
        }
    }

    fn order(&self, slug: &str) -> gig_core::models::Order {
        repo_orders::resolve(&self.ctx.conn, slug).unwrap()
    }

    fn register(&mut self, slug: &str, price: Option<i64>) -> i64 {
        let created = orders::new(
            &self.ctx,
            &orders::NewOrderInput {
                slug,
                title: "Title",
                price_minor: price,
                currency: None,
                cut_ratio: None,
                project_type: ProjectType::Tool,
                material_path: None,
                platform: None,
                external_id: None,
                client_words: None,
                from_draft: false,
                adopt: false,
                adopt_status: None,
                no_scaffold: true,
            },
        )
        .unwrap();
        self.refresh();
        created.order.id
    }

    fn deliver(&mut self, id: i64) {
        repo_orders::set_text(&self.ctx.conn, id, "started_at", Some(&clock::now())).unwrap();
        repo_orders::set_text(&self.ctx.conn, id, "delivered_at", Some(&clock::now())).unwrap();
        repo_orders::set_status(&self.ctx.conn, id, OrderStatus::Delivered).unwrap();
        self.refresh();
    }

    /// A write succeeded: its result popup names it (no write is silent),
    /// and Enter closes it.
    fn ack(&mut self, title: &str) {
        match &self.ui.popup {
            Some(Popup::Message {
                title: t, error, ..
            }) => {
                assert!(!error, "refused: {}", self.popup_text());
                assert_eq!(t, title);
            }
            other => panic!("result popup {title:?} expected, got {other:?}"),
        }
        self.key(KeyCode::Enter);
        assert_eq!(self.ui.popup, None);
    }

    fn popup_text(&self) -> String {
        match &self.ui.popup {
            Some(Popup::Message { title, lines, .. }) => format!("{title}\n{}", lines.join("\n")),
            other => panic!("expected a message popup, got {other:?}"),
        }
    }
}

#[test]
fn paid_form_defaults_mark_paid_and_set_warranty() {
    let mut h = Harness::new();
    let id = h.register("tk-paid", Some(80_000));
    h.deliver(id);
    h.ui.selected = Some(id);
    h.key(KeyCode::Char('p'));
    match &h.ui.popup {
        Some(Popup::Form(f)) => {
            assert_eq!(f.get("date"), clock::today());
            assert_eq!(f.get("amount"), "800.00");
        }
        other => panic!("paid form expected, got {other:?}"),
    }
    h.key(KeyCode::Enter);
    h.ack("paid tk-paid");
    let o = h.order("tk-paid");
    assert_eq!(o.status, OrderStatus::Paid);
    assert_eq!(o.paid_at.as_deref(), Some(clock::today().as_str()));
    let until = clock::date_plus_days(&clock::today(), h.ctx.config.general.warranty_days).unwrap();
    assert_eq!(o.warranty_until.as_deref(), Some(until.as_str()));
    assert_eq!(o.price_minor, Some(80_000));
    // The snapshot refreshed after the call.
    assert_eq!(h.ui.data.order(id).unwrap().order.status, OrderStatus::Paid);
}

#[test]
fn paid_form_with_edited_date_and_amount() {
    let mut h = Harness::new();
    let id = h.register("tk-amount", Some(80_000));
    h.deliver(id);
    h.key(KeyCode::Char('p'));
    h.backspaces(10);
    h.chars("2026-09-01");
    h.key(KeyCode::Tab);
    h.backspaces(6);
    h.chars("750.5");
    h.key(KeyCode::Enter);
    let o = h.order("tk-amount");
    assert_eq!(o.status, OrderStatus::Paid);
    assert_eq!(o.paid_at.as_deref(), Some("2026-09-01"));
    assert_eq!(o.price_minor, Some(75_050));
}

#[test]
fn refusals_are_shown_verbatim_and_change_nothing() {
    let mut h = Harness::new();
    h.register("tk-queued", Some(1000));
    h.key(KeyCode::Char('p'));
    h.key(KeyCode::Enter);
    let text = h.popup_text();
    assert!(
        text.contains("invalid_state\ninvalid state: paid needs delivered, order is queued"),
        "{text}"
    );
    assert_eq!(h.order("tk-queued").status, OrderStatus::Queued);
    // Closing the refusal brings the refused form back; Esc closes it.
    h.key(KeyCode::Esc);
    assert!(matches!(h.ui.popup, Some(Popup::Form(_))));
    h.key(KeyCode::Esc);
    assert_eq!(h.ui.popup, None);
    h.key(KeyCode::Char('s'));
    assert!(h
        .popup_text()
        .contains("started tk-queued: now in_progress"));
    assert_eq!(h.order("tk-queued").status, OrderStatus::InProgress);
}

#[test]
fn cancel_needs_a_typed_y() {
    let mut h = Harness::new();
    h.register("tk-cancel", None);

    // Esc on the reason form.
    h.key(KeyCode::Char('x'));
    h.chars("client left");
    h.key(KeyCode::Esc);
    assert_eq!(h.ui.popup, None);
    assert_eq!(h.order("tk-cancel").status, OrderStatus::Queued);

    // Reason submitted, then anything but y.
    for refuse in [KeyCode::Char('n'), KeyCode::Enter, KeyCode::Esc] {
        h.key(KeyCode::Char('x'));
        h.chars("client left");
        h.key(KeyCode::Enter);
        assert!(matches!(h.ui.popup, Some(Popup::Confirm { .. })));
        h.key(refuse);
        assert_eq!(h.ui.popup, None);
        let o = h.order("tk-cancel");
        assert_eq!(o.status, OrderStatus::Queued, "{refuse:?}");
        assert_eq!(o.cancel_reason, None);
    }

    // And with y.
    h.key(KeyCode::Char('x'));
    h.chars("client left");
    h.key(KeyCode::Enter);
    h.key(KeyCode::Char('y'));
    let o = h.order("tk-cancel");
    assert_eq!(o.status, OrderStatus::Cancelled);
    assert_eq!(o.cancel_reason.as_deref(), Some("client left"));
}

#[test]
fn a_taken_slug_is_refused_in_the_form() {
    let mut h = Harness::new();
    h.register("tk-taken", Some(1000));
    h.key(KeyCode::Char('N'));
    h.chars("tk-taken");
    h.key(KeyCode::Enter);
    // No call: the form stays, the field says why, the message row too.
    let Some(Popup::Form(form)) = &h.ui.popup else {
        panic!("form kept, got {:?}", h.ui.popup)
    };
    assert_eq!(
        form.field("slug").unwrap().error.as_deref(),
        Some("slug already exists")
    );
    let toast = h.ui.toast.clone().expect("message row");
    assert!(toast.text.contains("slug already exists"), "{toast:?}");
    // Typing clears the error; a bad slug is caught too.
    h.key(KeyCode::Backspace);
    let Some(Popup::Form(form)) = &h.ui.popup else {
        panic!()
    };
    assert_eq!(form.field("slug").unwrap().error, None);
    h.chars("X Y");
    h.key(KeyCode::Enter);
    let Some(Popup::Form(form)) = &h.ui.popup else {
        panic!()
    };
    assert!(form.field("slug").unwrap().error.is_some());
}

#[test]
fn leaving_the_slug_field_checks_it() {
    let mut h = Harness::new();
    h.register("tk-taken", Some(1000));
    h.key(KeyCode::Char('N'));
    let slug_error = |h: &Harness| match &h.ui.popup {
        Some(Popup::Form(form)) => form.field("slug").unwrap().error.clone(),
        other => panic!("form, got {other:?}"),
    };
    // Tabbing past an empty slug says nothing yet.
    h.key(KeyCode::Tab);
    assert_eq!(slug_error(&h), None);
    h.key(KeyCode::BackTab);
    h.chars("Bad Slug");
    assert_eq!(slug_error(&h), None, "not while typing");
    h.key(KeyCode::Tab);
    assert_eq!(
        slug_error(&h).as_deref(),
        Some("lowercase letters, digits, - _ . only")
    );
    h.key(KeyCode::BackTab);
    h.backspaces(8);
    h.chars("tk-taken");
    h.key(KeyCode::Down);
    assert_eq!(slug_error(&h).as_deref(), Some("slug already exists"));
    h.key(KeyCode::Up);
    h.chars("-2");
    h.key(KeyCode::Tab);
    assert_eq!(slug_error(&h), None);
}

#[test]
fn new_order_scaffolds_like_the_cli() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('N'));
    h.chars("tk-new");
    h.key(KeyCode::Tab);
    h.chars("图像去噪工具");
    h.key(KeyCode::Tab);
    h.chars("1200");
    h.key(KeyCode::Tab);
    h.key(KeyCode::Right); // custom -> tool (wraps)
    h.key(KeyCode::Tab);
    h.chars("/mnt/virtiofs/42");
    h.key(KeyCode::Tab);
    h.chars("xianyu");
    h.key(KeyCode::Tab);
    // Client words: Enter asks the app for $EDITOR; feed the text back.
    h.key(KeyCode::Enter);
    let Some(Effect::EditField(i)) = h.effects.pop() else {
        panic!("editor effect expected")
    };
    actions::set_edited_field(&mut h.ui, i, "please\ndenoise".into());
    h.key(KeyCode::Tab);
    h.key(KeyCode::Enter);

    let text = h.popup_text();
    let o = h.order("tk-new");
    assert_eq!(o.status, OrderStatus::Queued);
    assert_eq!(o.title, "图像去噪工具");
    assert_eq!(o.price_minor, Some(120_000));
    assert_eq!(o.project_type, ProjectType::Tool);
    assert_eq!(o.material_path.as_deref(), Some("/mnt/virtiofs/42"));
    assert_eq!(o.platform.as_deref(), Some("xianyu"));
    assert_eq!(o.client_words.as_deref(), Some("please\ndenoise"));
    let dev = h.ctx.config.general.dev_root.join("tk-new");
    for f in [
        ".gig/JOB.md",
        ".gig/QUOTE.md",
        "AGENTS.md",
        "README.md",
        ".gitignore",
    ] {
        assert!(dev.join(f).is_file(), "{f}");
        assert!(text.contains(&dev.join(f).display().to_string()), "{text}");
    }
    let job = std::fs::read_to_string(dev.join(".gig/JOB.md")).unwrap();
    assert!(job.contains("please\ndenoise"), "{job}");
    assert!(text.contains("Next step: grill"), "{text}");
}

#[test]
fn drafts_new_and_promote() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('2'));
    assert_eq!(h.ui.view, View::Drafts);
    h.key(KeyCode::Char('N'));
    h.chars("tk-draft");
    h.key(KeyCode::Tab);
    h.chars("Draft title");
    h.key(KeyCode::Tab);
    h.chars("/mnt/m");
    h.key(KeyCode::Tab);
    h.key(KeyCode::Right); // none -> tool
    h.key(KeyCode::Enter);
    assert!(h.popup_text().contains("created draft tk-draft"));
    h.key(KeyCode::Enter);

    // Enter shows the tail of NOTES.md, read only.
    let d = h.ui.selected_draft().unwrap().clone();
    let notes = Path::new(&d.notes_dir).join("NOTES.md");
    let body: String = (1..=50).map(|i| format!("line {i}\n")).collect();
    std::fs::write(&notes, &body).unwrap();
    // Medium and Wide: a pane with the last 30 lines; Enter again closes it.
    h.key(KeyCode::Enter);
    let pane = h.ui.notes_pane.clone().expect("notes pane at 200 columns");
    assert_eq!(pane.lines.len(), 30);
    assert_eq!(pane.lines.last().map(String::as_str), Some("line 50"));
    assert_eq!(h.ui.popup, None);
    h.key(KeyCode::Enter);
    assert_eq!(h.ui.notes_pane, None);
    // Narrow: the popup.
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(h.ui.handle_key(enter, 80), Outcome::None);
    let text = h.popup_text();
    assert!(text.starts_with("notes tk-draft"), "{text}");
    assert!(text.contains("(20 earlier lines)") && text.contains("line 50"));
    assert!(!text.contains("line 20\n"));
    h.key(KeyCode::Esc);
    assert_eq!(std::fs::read_to_string(&notes).unwrap(), body);

    h.key(KeyCode::Char('P'));
    match &h.ui.popup {
        Some(Popup::Form(f)) => {
            assert_eq!(f.get("slug"), "tk-draft");
            assert!(f.field("slug").unwrap().locked);
            assert!(f.field("from draft").unwrap().is_on());
            assert_eq!(f.get("title"), "Draft title");
            assert_eq!(f.get("type"), "tool");
            assert_eq!(f.get("material"), "/mnt/m");
        }
        other => panic!("new order form expected, got {other:?}"),
    }
    // The locked slug cannot be typed into: focus starts on the title.
    h.chars("!");
    h.key(KeyCode::Enter);
    assert!(h.popup_text().contains("Next step: grill"));
    let o = h.order("tk-draft");
    assert_eq!(o.title, "Draft title!");
    assert_eq!(o.material_path.as_deref(), Some("/mnt/m"));
    let d =
        h.ui.data
            .drafts
            .iter()
            .find(|d| d.slug == "tk-draft")
            .unwrap();
    assert_eq!(d.status, DraftStatus::Promoted);
    assert_eq!(d.promoted_order_id, Some(o.id));
}

#[test]
fn archive_preview_reports_and_moves_nothing() {
    let mut h = Harness::new();
    h.register("tk-arch", None);
    h.key(KeyCode::Char('A'));
    let text = h.popup_text();
    assert!(text.contains("blockers:"), "{text}");
    assert!(text.contains("archive needs paid or cancelled"), "{text}");
    assert!(text.contains("Have the agent archive"), "{text}");
    assert_eq!(h.order("tk-arch").status, OrderStatus::Queued);
}

#[test]
fn other_forms_reach_gig_core() {
    let mut h = Harness::new();
    let id = h.register("tk-misc", Some(10_000));
    h.key(KeyCode::Char('s'));
    h.ack("start tk-misc");

    h.key(KeyCode::Char('$'));
    h.backspaces(10);
    h.chars("150");
    h.key(KeyCode::Tab);
    h.chars("scope grew");
    h.key(KeyCode::Enter);
    h.ack("price tk-misc");
    assert_eq!(h.order("tk-misc").price_minor, Some(15_000));

    h.key(KeyCode::Char('c'));
    h.key(KeyCode::Enter);
    let Some(Effect::EditField(i)) = h.effects.pop() else {
        panic!("editor effect expected")
    };
    actions::set_edited_field(&mut h.ui, i, "add a CLI".into());
    h.key(KeyCode::Tab);
    h.backspaces(1);
    h.chars("20");
    h.key(KeyCode::Enter);
    h.ack("change tk-misc");
    assert_eq!(h.order("tk-misc").price_minor, Some(17_000));
    assert_eq!(h.ui.data.order(id).unwrap().requirement_changes.len(), 1);

    h.key(KeyCode::Char('k'));
    h.chars("2");
    for _ in 0..4 {
        h.key(KeyCode::Tab);
    }
    h.key(KeyCode::Right); // 3 -> 4
    h.key(KeyCode::Enter);
    h.ack("scorecard tk-misc");
    let sc = h.ui.data.order(id).unwrap().scorecard.clone().unwrap();
    assert_eq!(sc.decisions, Some(2));
    assert_eq!(sc.score, Some(4));

    h.key(KeyCode::Char('n'));
    assert_eq!(
        h.effects.pop(),
        Some(Effect::EditNote {
            slug: "tk-misc".into()
        })
    );
    actions::perform(
        &h.ctx,
        &mut h.ui,
        &Action::Note {
            slug: "tk-misc".into(),
            text: "called the client".into(),
        },
    );
    h.refresh();
    h.ack("note tk-misc");
    assert!(h.order("tk-misc").notes.contains("called the client"));

    // `e` refuses a JOB.md that does not exist, so the editor never
    // creates one ...
    h.key(KeyCode::Char('e'));
    assert!(h.effects.is_empty());
    assert!(h.popup_text().contains("JOB.md does not exist"));
    h.key(KeyCode::Esc);
    // ... and opens `<dev_path>/.gig/JOB.md` when it does.
    let dev = h.order("tk-misc").dev_path.unwrap();
    let job = Path::new(&dev).join(".gig").join("JOB.md");
    std::fs::create_dir_all(job.parent().unwrap()).unwrap();
    std::fs::write(&job, "# x\n").unwrap();
    h.key(KeyCode::Char('e'));
    assert_eq!(h.effects.pop(), Some(Effect::EditFile(job)));
    h.key(KeyCode::Char('y'));
    let toast = h.ui.toast.clone().expect("a toast");
    assert!(toast.text.contains("no uploaded link"), "{toast:?}");
}

// ---- uploads (spec 2.1 `u`, `m`, `U`) ----

use gig_core::delivery::{uploader_by_name, UploadOpts, UploadResult, Uploader};
use gig_core::models::{Channel, PackageKind, PackageStatus};
use gig_core::services::packages;
use gig_tui::upload::{self, UploadJob, UploadKind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Reports three parts through the progress callback, then waits until the
/// drawing side has seen the last report, so the test is not timing bound.
struct FakeUploader {
    seen_done: Arc<AtomicBool>,
    keys: Mutex<Vec<String>>,
}

impl FakeUploader {
    fn new() -> Self {
        Self {
            seen_done: Arc::new(AtomicBool::new(false)),
            keys: Mutex::new(Vec::new()),
        }
    }
}

impl Uploader for FakeUploader {
    fn name(&self) -> &str {
        "s3:fake"
    }

    fn upload(&self, local: &Path, opts: &UploadOpts) -> gig_core::Result<UploadResult> {
        assert!(
            opts.progress.is_some(),
            "the TUI passes a progress callback"
        );
        self.keys
            .lock()
            .unwrap()
            .push(opts.object_key.clone().unwrap());
        for part in 1..=3 {
            opts.report(part * 10, 30);
            std::thread::sleep(Duration::from_millis(5));
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.seen_done.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok(UploadResult {
            url: format!(
                "https://s3.test/{}",
                local.file_name().unwrap().to_string_lossy()
            ),
            short_url: Some("https://go.test/abc".into()),
            expires_at: Some(1_900_000_000),
            provider: "s3:fake".into(),
            file_size: std::fs::metadata(local).unwrap().len(),
            pwd: None,
        })
    }
}

/// Run a confirmed upload the way `App::upload` does, minus the terminal:
/// worker thread, a tick per poll. Returns the result and the ticks seen.
fn run_upload(
    h: &mut Harness,
    job: &UploadJob,
) -> (gig_core::Result<upload::Uploaded>, Vec<(u64, u64)>) {
    let fake = FakeUploader::new();
    let seen_done = fake.seen_done.clone();
    let mut ticks = Vec::new();
    let result = upload::run_on_worker(&mut h.ctx, job, &fake, |sent, total| {
        ticks.push((sent, total));
        if total > 0 && sent == total {
            seen_done.store(true, Ordering::SeqCst);
        }
        std::thread::sleep(Duration::from_millis(1));
    });
    h.refresh();
    (result, ticks)
}

/// An order in progress with a checked package `<slug>-v1` of `kind`.
fn with_checked_package(h: &mut Harness, slug: &str, kind: PackageKind) -> (i64, String) {
    let id = h.register(slug, Some(50_000));
    orders::start(&h.ctx, Some(slug)).unwrap();
    let dev = PathBuf::from(h.order(slug).dev_path.unwrap());
    let pkg = format!("{slug}-v1");
    let dir = dev.join("delivery").join(&pkg);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("manual.pdf"), "pdf bytes").unwrap();
    packages::build_package(&h.ctx, Some(slug), &pkg, kind, true, &[]).unwrap();
    h.refresh();
    h.ui.selected = Some(id);
    (id, pkg)
}

/// A confirmation as drawn: title, then the body.
fn confirm_text(h: &Harness) -> String {
    match &h.ui.popup {
        Some(Popup::Confirm {
            title, lines, then, ..
        }) => format!(
            "{title}\n{}",
            gig_tui::popup::confirm_body(lines, then).join("\n")
        ),
        other => panic!("expected a confirmation, got {other:?}"),
    }
}

/// S3 tables `a` and `b` with `s3:a` as the default, and `[delivery.bdpan]`
/// pointing at `bin`.
fn with_uploaders(h: &mut Harness, bin: &Path) {
    let d = &mut h.ctx.config.delivery;
    d.uploader = "s3:a".into();
    for name in ["a", "b"] {
        d.s3.insert(
            name.into(),
            gig_core::config::S3 {
                bucket: "bucket".into(),
                region: "r".into(),
                endpoint: "https://s3.example.test".into(),
                ..Default::default()
            },
        );
    }
    d.bdpan.bin = bin.to_string_lossy().into_owned();
}

/// The uploader line of the open confirmation.
fn uploader_shown(h: &Harness) -> String {
    let text = confirm_text(h);
    text.lines()
        .find_map(|l| l.strip_prefix("uploader: "))
        .unwrap_or_else(|| panic!("no uploader line in {text}"))
        .to_string()
}

#[test]
fn tab_switches_the_uploader_of_an_upload_confirm() {
    let mut h = Harness::new();
    let bin = h._dir.path().join("no-bdpan-here");
    with_uploaders(&mut h, &bin);
    let (_, pkg) = with_checked_package(&mut h, "tk-tab", PackageKind::Full);

    h.key(KeyCode::Char('u'));
    h.key(KeyCode::Enter);
    // The default from the config first; Tab visits bdpan, then each S3
    // table, and wraps.
    assert_eq!(uploader_shown(&h), "s3:a");
    let mut seen = Vec::new();
    for _ in 0..4 {
        h.key(KeyCode::Tab);
        seen.push(uploader_shown(&h));
    }
    assert_eq!(seen, ["s3:b", "bdpan", "s3:a", "s3:b"]);
    h.key(KeyCode::Tab);
    h.key(KeyCode::Char('y'));
    match h.effects.pop() {
        Some(Effect::Upload(job)) => {
            assert_eq!(job.uploader, "bdpan");
            assert_eq!(
                job.kind,
                UploadKind::Package {
                    slug: "tk-tab".into(),
                    package_id: pkg,
                }
            );
        }
        other => panic!("upload effect expected, got {other:?}"),
    }

    // The artifact confirm switches the same way.
    let file = h._dir.path().join("report.pdf");
    std::fs::write(&file, "report").unwrap();
    h.key(KeyCode::Char('U'));
    h.chars(&file.display().to_string());
    h.key(KeyCode::Enter);
    assert_eq!(uploader_shown(&h), "s3:a");
    h.key(KeyCode::Tab);
    h.key(KeyCode::Char('y'));
    match h.effects.pop() {
        Some(Effect::Upload(job)) => assert_eq!(job.uploader, "s3:b"),
        other => panic!("upload effect expected, got {other:?}"),
    }
}

/// The chosen uploader is the one `App::upload` builds and runs: here a fake
/// bdpan binary, so the package goes out as a Pan Share.
#[cfg(unix)]
#[test]
fn the_job_runs_with_the_chosen_uploader_and_shows_the_pwd() {
    use gig_core::delivery::bdpan::fake::{self, FakeBdpan};
    let mut h = Harness::new();
    let fake = FakeBdpan::install(h._dir.path()).unwrap();
    with_uploaders(&mut h, &fake.bin());
    let (id, pkg) = with_checked_package(&mut h, "tk-pan", PackageKind::Full);

    h.key(KeyCode::Char('u'));
    h.key(KeyCode::Enter);
    h.key(KeyCode::Tab);
    h.key(KeyCode::Tab);
    assert_eq!(uploader_shown(&h), "bdpan");
    h.key(KeyCode::Char('y'));
    let job = match h.effects.pop() {
        Some(Effect::Upload(job)) => job,
        other => panic!("upload effect expected, got {other:?}"),
    };

    let uploader = uploader_by_name(&h.ctx.config, &h.ctx.paths, &job.uploader).unwrap();
    assert_eq!(uploader.name(), "bdpan");
    let mut ticks = Vec::new();
    let done = upload::run_on_worker(&mut h.ctx, &job, uploader.as_ref(), |sent, total| {
        ticks.push((sent, total));
        std::thread::sleep(Duration::from_millis(1));
    })
    .unwrap();
    h.refresh();

    // bdpan reports once, at the end: until then the popup spins.
    assert!(
        ticks
            .iter()
            .all(|&(sent, total)| total == 0 || sent == total),
        "{ticks:?}"
    );
    assert!(
        fake.calls().iter().any(|c| c.contains(" upload ")),
        "{:?}",
        fake.calls()
    );
    let link = format!("{}?pwd={}", fake::LINK, fake::PWD);
    assert_eq!(done.link(), Some(link.as_str()));
    assert_eq!(done.pwd.as_deref(), Some(fake::PWD));
    // The extraction code sits right under the link.
    let shown = done.link_lines();
    let at = shown
        .iter()
        .position(|l| l == gig_tui::text::strip_scheme(&link))
        .unwrap_or_else(|| panic!("no link in {shown:?}"));
    assert_eq!(shown[at + 1], format!("pwd: {}", fake::PWD));

    let row = h.ui.data.order(id).unwrap();
    let p = row.packages.iter().find(|p| p.package_id == pkg).unwrap();
    assert_eq!(p.channel, Some(Channel::Pan));
    assert_eq!(p.uploader.as_deref(), Some("bdpan"));
    assert_eq!(p.short_url, None);
}

#[test]
fn an_unset_default_shows_none_and_tab_reaches_the_configured_uploaders() {
    let mut h = Harness::new();
    let bin = h._dir.path().join("no-bdpan-here");
    with_uploaders(&mut h, &bin);
    h.ctx.config.delivery.uploader = String::new();
    with_checked_package(&mut h, "tk-none", PackageKind::Full);
    h.key(KeyCode::Char('u'));
    h.key(KeyCode::Enter);
    assert_eq!(uploader_shown(&h), "none");
    // Tab never goes back to "none".
    let mut seen = Vec::new();
    for _ in 0..4 {
        h.key(KeyCode::Tab);
        seen.push(uploader_shown(&h));
    }
    assert_eq!(seen, ["bdpan", "s3:a", "s3:b", "bdpan"]);
    assert!(h.effects.is_empty());
}

#[test]
fn upload_package_shows_progress_and_delivers() {
    let mut h = Harness::new();
    let (id, pkg) = with_checked_package(&mut h, "tk-up", PackageKind::Full);

    h.key(KeyCode::Char('u'));
    match &h.ui.popup {
        Some(Popup::Pick(p)) => {
            assert_eq!(p.items.len(), 1);
            assert_eq!(p.value(), Some(pkg.as_str()));
            assert!(p.items[0].1.contains("full"), "{:?}", p.items);
        }
        other => panic!("package pick expected, got {other:?}"),
    }
    h.key(KeyCode::Enter);
    let text = confirm_text(&h);
    assert!(
        text.contains(&format!("Upload package {pkg} of tk-up?")),
        "{text}"
    );
    assert!(text.contains("kind: full"), "{text}");
    assert!(text.contains("size: "), "{text}");
    assert!(text.contains("order: in_progress -> delivered"), "{text}");

    // Anything but y cancels and uploads nothing.
    h.key(KeyCode::Char('n'));
    assert_eq!(h.ui.popup, None);
    assert!(h.effects.is_empty());

    h.key(KeyCode::Char('u'));
    h.key(KeyCode::Enter);
    h.key(KeyCode::Char('y'));
    let job = match h.effects.pop() {
        Some(Effect::Upload(job)) => job,
        other => panic!("upload effect expected, got {other:?}"),
    };
    assert_eq!(
        job.kind,
        UploadKind::Package {
            slug: "tk-up".into(),
            package_id: pkg.clone(),
        }
    );
    assert_eq!(job.uploader, "");
    // Nothing changed before the upload ran.
    assert_eq!(h.order("tk-up").status, OrderStatus::InProgress);

    let (result, ticks) = run_upload(&mut h, &job);
    let done = result.unwrap();
    assert!(ticks.contains(&(30, 30)), "{ticks:?}");
    assert!(
        ticks.windows(2).all(|w| w[0].0 <= w[1].0),
        "progress only grows: {ticks:?}"
    );
    assert_eq!(done.link(), Some("https://go.test/abc"));
    assert_eq!(done.order_status, Some(OrderStatus::Delivered));
    assert!(done.lines().join("\n").contains("order: delivered"));

    // The state transition went through gig-core.
    assert_eq!(h.order("tk-up").status, OrderStatus::Delivered);
    let row = h.ui.data.order(id).unwrap();
    let p = row.packages.iter().find(|p| p.package_id == pkg).unwrap();
    assert_eq!(p.status, PackageStatus::Sent);
    assert_eq!(p.channel, Some(Channel::Oss));
    assert_eq!(p.short_url.as_deref(), Some("https://go.test/abc"));
    assert_eq!(p.uploader.as_deref(), Some("s3:fake"));
    assert_eq!(
        actions::latest_link(row).as_deref(),
        Some("https://go.test/abc")
    );

    // Sent packages are no longer offered.
    h.key(KeyCode::Char('u'));
    assert!(h.popup_text().contains("no checked package"));
}

#[test]
fn upload_refusals_are_shown_before_confirming() {
    let mut h = Harness::new();
    with_checked_package(&mut h, "tk-stale", PackageKind::Full);
    // The zip is rebuilt after the check: gig-core wants a new check.
    let dev = PathBuf::from(h.order("tk-stale").dev_path.unwrap());
    std::fs::write(dev.join("delivery/tk-stale-v1/manual.pdf"), "changed").unwrap();
    gig_core::package::build::build(&dev, "tk-stale-v1", PackageKind::Full, false, &[]).unwrap();
    h.key(KeyCode::Char('u'));
    h.key(KeyCode::Enter);
    let text = h.popup_text();
    assert!(text.starts_with("refused\n"), "{text}");
    assert!(
        text.contains("needs_check\n") && text.contains("changed since it was checked"),
        "{text}"
    );
    assert_eq!(h.order("tk-stale").status, OrderStatus::InProgress);
}

#[test]
fn no_uploader_configured_is_an_error_not_a_panic() {
    let h = Harness::new();
    // A fresh home: the confirmation keeps the unset default.
    let job = UploadJob::new(
        UploadKind::Package {
            slug: "tk".into(),
            package_id: "tk-v1".into(),
        },
        &h.ctx.config.delivery,
    );
    let e = match uploader_by_name(&h.ctx.config, &h.ctx.paths, &job.uploader) {
        Ok(_) => panic!("a fresh home has no uploader"),
        Err(e) => e,
    };
    // `App::upload` shows exactly this popup.
    let Popup::Message { lines, error, .. } = Popup::error(&e) else {
        panic!()
    };
    assert!(error);
    // The code on its own line (drawn bold), then the message verbatim.
    assert_eq!(lines, vec![e.code().to_string(), e.to_string()]);
}

#[test]
fn mark_sent_by_phone_with_a_note() {
    let mut h = Harness::new();
    let (_, pkg) = with_checked_package(&mut h, "tk-phone", PackageKind::Full);
    h.key(KeyCode::Char('m'));
    assert!(matches!(h.ui.popup, Some(Popup::Pick(_))));
    h.key(KeyCode::Enter);
    match &h.ui.popup {
        Some(Popup::Form(f)) => assert_eq!(f.get("channel"), "phone"),
        other => panic!("mark sent form expected, got {other:?}"),
    }
    h.key(KeyCode::Tab);
    h.chars("via gsconnect");
    h.key(KeyCode::Enter);
    let text = confirm_text(&h);
    assert!(text.contains("sent via phone"), "{text}");
    assert!(text.contains("order: in_progress -> delivered"), "{text}");
    assert!(text.contains("note: via gsconnect"), "{text}");
    assert_eq!(h.order("tk-phone").status, OrderStatus::InProgress);

    h.key(KeyCode::Char('y'));
    let text = h.popup_text();
    assert!(text.contains("order: delivered"), "{text}");
    let o = h.order("tk-phone");
    assert_eq!(o.status, OrderStatus::Delivered);
    assert!(
        o.notes
            .contains(&format!("sent {pkg} via phone: via gsconnect")),
        "{}",
        o.notes
    );
    let p = &h.ui.data.order(o.id).unwrap().packages[0];
    assert_eq!(p.channel, Some(Channel::Phone));
}

#[test]
fn mark_sent_preview_keeps_the_order_state() {
    let mut h = Harness::new();
    with_checked_package(&mut h, "tk-prev", PackageKind::Preview);
    h.key(KeyCode::Char('m'));
    h.key(KeyCode::Enter);
    h.key(KeyCode::Right); // phone -> other
    h.key(KeyCode::Enter);
    let text = confirm_text(&h);
    assert!(text.contains("sent via other"), "{text}");
    assert!(text.contains("order: in_progress -> in_progress"), "{text}");
    h.key(KeyCode::Char('y'));
    assert_eq!(h.order("tk-prev").status, OrderStatus::InProgress);
    let o = h.order("tk-prev");
    let p = &h.ui.data.order(o.id).unwrap().packages[0];
    assert_eq!(p.channel, Some(Channel::Other));
    assert_eq!(p.status, PackageStatus::Sent);
}

#[test]
fn upload_artifact_from_a_typed_path() {
    let mut h = Harness::new();
    let id = h.register("tk-art", None);
    orders::start(&h.ctx, Some("tk-art")).unwrap();
    h.refresh();
    h.ui.selected = Some(id);
    let file = h._dir.path().join("report.pdf");
    std::fs::write(&file, "report").unwrap();

    h.key(KeyCode::Char('U'));
    assert!(matches!(h.ui.popup, Some(Popup::Form(_))));
    h.chars(&file.display().to_string());
    h.key(KeyCode::Enter);
    let text = confirm_text(&h);
    assert!(text.contains("report.pdf"), "{text}");
    assert!(text.contains("size: 6 B"), "{text}");
    h.key(KeyCode::Char('y'));
    let job = match h.effects.pop() {
        Some(Effect::Upload(job)) => job,
        other => panic!("upload effect expected, got {other:?}"),
    };
    assert_eq!(job.title(), "uploading report.pdf");
    let (result, ticks) = run_upload(&mut h, &job);
    let done = result.unwrap();
    assert!(!ticks.is_empty());
    assert_eq!(done.order_status, None);
    assert_eq!(done.link(), Some("https://go.test/abc"));
    let row = h.ui.data.order(id).unwrap();
    assert_eq!(row.artifacts.len(), 1);
    assert_eq!(
        row.artifacts[0].short_url.as_deref(),
        Some("https://go.test/abc")
    );
    assert_eq!(h.order("tk-art").status, OrderStatus::InProgress);

    // A missing file is refused before the confirmation.
    h.key(KeyCode::Char('U'));
    h.chars("/nonexistent/file.pdf");
    h.key(KeyCode::Enter);
    assert!(h.popup_text().contains("refused"));
    // Closing the refusal brings the form back; Esc drops it.
    h.key(KeyCode::Esc);
    h.key(KeyCode::Esc);
    assert_eq!(h.ui.popup, None);

    // A relative path is taken from the order's project directory.
    let dev = h.order("tk-art").dev_path.expect("scaffolded");
    std::fs::create_dir_all(&dev).unwrap();
    std::fs::write(Path::new(&dev).join("summary.txt"), "hello").unwrap();
    h.key(KeyCode::Char('U'));
    h.chars("summary.txt");
    h.key(KeyCode::Enter);
    let text = confirm_text(&h);
    let want = std::fs::canonicalize(Path::new(&dev).join("summary.txt")).unwrap();
    assert!(text.contains(&want.display().to_string()), "{text}");
    assert!(text.contains("size: 5 B"), "{text}");
}

#[test]
fn a_refused_form_keeps_what_was_typed() {
    let mut h = Harness::new();
    let id = h.register("tk-typo", Some(1000));
    h.deliver(id);
    h.key(KeyCode::Char('p'));
    h.backspaces(10);
    h.chars("2026-09-02");
    h.key(KeyCode::Tab);
    h.backspaces(5);
    h.chars("12x");
    h.key(KeyCode::Enter);
    let text = h.popup_text();
    assert!(text.contains("12x"), "{text}");
    assert_eq!(h.order("tk-typo").status, OrderStatus::Delivered);
    h.key(KeyCode::Enter);
    match &h.ui.popup {
        Some(Popup::Form(f)) => {
            assert_eq!(f.get("date"), "2026-09-02");
            assert_eq!(f.get("amount"), "12x");
        }
        other => panic!("form expected, got {other:?}"),
    }
    // Fixed, it goes through and is not reopened.
    h.key(KeyCode::Backspace);
    h.key(KeyCode::Enter);
    h.ack("paid tk-typo");
    assert_eq!(h.order("tk-typo").price_minor, Some(1200));
}

#[test]
fn pastes_type_text_and_never_run_keys() {
    let mut h = Harness::new();
    let id = h.register("tk-paste", Some(1000));
    h.deliver(id);
    // In the list a paste does nothing: no `s`, `x` or `a` runs.
    h.ui.handle_paste("start x archive\ny");
    assert_eq!(h.ui.popup, None);
    assert!(!h.ui.show_closed);
    assert_eq!(h.order("tk-paste").status, OrderStatus::Delivered);
    // Into the cancel form: the newline does not submit, `y` is text.
    h.key(KeyCode::Char('x'));
    h.ui.handle_paste("client left\ny\n");
    match &h.ui.popup {
        Some(Popup::Form(f)) => assert_eq!(f.get("reason"), "client left y"),
        other => panic!("form expected, got {other:?}"),
    }
    assert_eq!(h.order("tk-paste").status, OrderStatus::Delivered);
    h.key(KeyCode::Esc);
    // Into the filter.
    h.key(KeyCode::Char('/'));
    h.ui.handle_paste("tk-pa\n");
    assert_eq!(h.ui.filter().text, "tk-pa");
    // Chords type nothing into the filter; Ctrl+U clears it.
    h.ui.handle_key(
        KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL),
        200,
    );
    h.ui.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT), 200);
    assert_eq!(h.ui.filter().text, "tk-pa");
    h.ui.handle_key(
        KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
        200,
    );
    assert_eq!(h.ui.filter().text, "");
}
