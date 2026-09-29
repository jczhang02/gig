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
    assert_eq!(h.ui.popup, None, "no refusal");
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
        text.contains("invalid_state: invalid state: paid needs delivered, order is queued"),
        "{text}"
    );
    assert_eq!(h.order("tk-queued").status, OrderStatus::Queued);
    // Any key closes the message; then the app is usable again.
    h.key(KeyCode::Esc);
    assert_eq!(h.ui.popup, None);
    h.key(KeyCode::Char('s'));
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

    h.key(KeyCode::Char('$'));
    h.backspaces(10);
    h.chars("150");
    h.key(KeyCode::Tab);
    h.chars("scope grew");
    h.key(KeyCode::Enter);
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
    assert_eq!(h.order("tk-misc").price_minor, Some(17_000));
    assert_eq!(h.ui.data.order(id).unwrap().requirement_changes.len(), 1);

    h.key(KeyCode::Char('k'));
    h.chars("2");
    for _ in 0..4 {
        h.key(KeyCode::Tab);
    }
    h.key(KeyCode::Right); // 3 -> 4
    h.key(KeyCode::Enter);
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
    assert!(h.order("tk-misc").notes.contains("called the client"));

    h.key(KeyCode::Char('e'));
    match h.effects.pop() {
        Some(Effect::EditFile(p)) => assert!(p.ends_with(".gig/JOB.md")),
        other => panic!("{other:?}"),
    }
    h.key(KeyCode::Char('y'));
    assert!(h.popup_text().contains("no uploaded link"));
}
