//! Order and draft actions (spec 2.1 and 2.2): which key opens which form,
//! what a submitted form turns into, and the gig-core call behind it.
//!
//! Everything here is terminal-free so tests can drive it with key events and
//! a `Ctx`: key handlers return an `Effect`, and `App` carries out the effects
//! that need the terminal ($EDITOR, clipboard). Every service call passes the
//! order's slug as the key; `None` would resolve the TUI's own cwd.

use crate::app::{UiState, View};
use crate::data::{self, OrderRow};
use crate::popup::{Field, Form, Popup, PopupKey};
use crossterm::event::{KeyCode, KeyEvent};
use gig_core::models::{Draft, ProjectType};
use gig_core::money::{format_minor, parse_amount};
use gig_core::services::{archive, drafts, orders, Ctx};
use gig_core::{clock, Error, Result};
use std::path::PathBuf;

/// Which form a popup is, and the order it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormKind {
    Paid { slug: String },
    Price { slug: String },
    Change { slug: String },
    Scorecard { slug: String },
    Cancel { slug: String },
    NewOrder,
    NewDraft,
}

/// One gig-core call with the raw form input; parsing happens in `run` so
/// parse refusals (`invalid_input`) surface like any other refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Start {
        slug: String,
    },
    Paid {
        slug: String,
        date: String,
        amount: String,
    },
    Price {
        slug: String,
        amount: String,
        reason: String,
    },
    Change {
        slug: String,
        description: String,
        delta: String,
    },
    Note {
        slug: String,
        text: String,
    },
    Scorecard {
        slug: String,
        decisions: String,
        repeat_questions: String,
        cleanups: String,
        report_reworks: String,
        score: String,
        note: String,
    },
    /// Only built from a confirmed popup; runs `cancel(yes=true)`.
    Cancel {
        slug: String,
        reason: String,
    },
    ArchivePreview {
        slug: String,
    },
    NewOrder(NewOrder),
    NewDraft {
        slug: String,
        title: String,
        material: String,
        project_type: String,
    },
}

/// Fields of the New order form (spec 2.1 `N`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NewOrder {
    pub slug: String,
    pub title: String,
    pub price: String,
    pub project_type: String,
    pub material: String,
    pub platform: String,
    pub client_words: String,
    pub from_draft: bool,
}

/// What the app loop must do after a key press.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    None,
    Refresh,
    /// Run a gig-core call (`perform`), then refresh.
    Call(Action),
    /// Open $EDITOR on field `index` of the open form.
    EditField(usize),
    /// Open $EDITOR on an empty note for this order, then `orders::note`.
    EditNote {
        slug: String,
    },
    /// Open $EDITOR on a file in place (JOB.md), then refresh.
    EditFile(PathBuf),
    /// Put a link on the clipboard, or show it when there is none.
    Copy(String),
}

/// Choices of the project type select, as gig-core spells them.
fn project_types() -> Vec<&'static str> {
    ProjectType::ALL.iter().map(|t| t.as_str()).collect()
}

/// The draft type select also allows "none".
const NO_TYPE: &str = "none";

/// Keys of the Orders view and the detail (spec 2.1 table, minus the
/// uploads), and of the Drafts view (2.2). `None` means "not an action key
/// here", so the caller can pass the key on.
pub fn view_key(ui: &mut UiState, key: KeyEvent) -> Option<Effect> {
    let KeyCode::Char(c) = key.code else {
        return None;
    };
    if ui.view == View::Drafts && !ui.detail_open {
        return drafts_key(ui, c);
    }
    let order_view = ui.view == View::Orders || ui.detail_open;
    if !order_view {
        return None;
    }
    if c == 'N' {
        ui.popup = Some(Popup::Form(new_order_form(None)));
        return Some(Effect::None);
    }
    let row = ui.selected_order()?;
    let slug = row.order.slug.clone();
    let effect = match c {
        's' => Effect::Call(Action::Start { slug }),
        'p' => open(ui, paid_form(row, &clock::today())),
        '$' => open(ui, price_form(row)),
        'c' => open(ui, change_form(row)),
        'n' => Effect::EditNote { slug },
        'k' => open(ui, scorecard_form(row)),
        'x' => open(ui, cancel_form(row)),
        'A' => Effect::Call(Action::ArchivePreview { slug }),
        'e' => match data::job_md_path(&row.order) {
            Some(p) => Effect::EditFile(p),
            None => {
                ui.popup = Some(Popup::Message {
                    title: "edit JOB.md".into(),
                    lines: vec![format!("{slug} has no project directory")],
                    error: true,
                });
                Effect::None
            }
        },
        'y' => match latest_link(row) {
            Some(link) => Effect::Copy(link),
            None => {
                ui.popup = Some(Popup::message(
                    "copy link",
                    vec![format!("{slug} has no uploaded link yet")],
                ));
                Effect::None
            }
        },
        _ => return None,
    };
    Some(effect)
}

fn drafts_key(ui: &mut UiState, c: char) -> Option<Effect> {
    match c {
        'N' => {
            ui.popup = Some(Popup::Form(new_draft_form()));
            Some(Effect::None)
        }
        'P' => {
            let draft = ui.selected_draft()?.clone();
            ui.popup = Some(Popup::Form(new_order_form(Some(&draft))));
            Some(Effect::None)
        }
        _ => None,
    }
}

fn open(ui: &mut UiState, form: Form) -> Effect {
    ui.popup = Some(Popup::Form(form));
    Effect::None
}

/// A key while a popup is open. Esc closes any popup.
pub fn popup_key(ui: &mut UiState, key: KeyEvent) -> Effect {
    let Some(popup) = ui.popup.as_mut() else {
        return Effect::None;
    };
    match popup.handle_key(key) {
        PopupKey::None => Effect::None,
        PopupKey::Close => {
            ui.popup = None;
            Effect::None
        }
        PopupKey::EditField(i) => Effect::EditField(i),
        PopupKey::Confirmed => match ui.popup.take() {
            Some(Popup::Confirm { then, .. }) => Effect::Call(then),
            _ => Effect::None,
        },
        PopupKey::Submit => match ui.popup.take() {
            Some(Popup::Form(form)) => submit(ui, form),
            other => {
                ui.popup = other;
                Effect::None
            }
        },
    }
}

/// A submitted form: a call, or (for cancel) the typed-`y` confirmation.
fn submit(ui: &mut UiState, form: Form) -> Effect {
    let f = |label: &str| form.get(label).to_string();
    let action = match &form.kind {
        FormKind::Paid { slug } => Action::Paid {
            slug: slug.clone(),
            date: f("date"),
            amount: f("amount"),
        },
        FormKind::Price { slug } => Action::Price {
            slug: slug.clone(),
            amount: f("amount"),
            reason: f("reason"),
        },
        FormKind::Change { slug } => Action::Change {
            slug: slug.clone(),
            description: f("description"),
            delta: f("price delta"),
        },
        FormKind::Scorecard { slug } => Action::Scorecard {
            slug: slug.clone(),
            decisions: f("decisions"),
            repeat_questions: f("repeat qs"),
            cleanups: f("cleanups"),
            report_reworks: f("report reworks"),
            score: f("score"),
            note: f("note"),
        },
        FormKind::Cancel { slug } => {
            ui.popup = Some(Popup::Confirm {
                title: format!("cancel {slug}"),
                lines: vec![
                    format!("Cancel order {slug}?"),
                    format!("reason: {}", form.get("reason")),
                    "The project directory stays; archive it later.".into(),
                ],
                then: Action::Cancel {
                    slug: slug.clone(),
                    reason: f("reason"),
                },
            });
            return Effect::None;
        }
        FormKind::NewOrder => Action::NewOrder(NewOrder {
            slug: f("slug"),
            title: f("title"),
            price: f("price"),
            project_type: f("type"),
            material: f("material"),
            platform: f("platform"),
            client_words: f("client words"),
            from_draft: form.field("from draft").is_some_and(Field::is_on),
        }),
        FormKind::NewDraft => Action::NewDraft {
            slug: f("slug"),
            title: f("title"),
            material: f("material"),
            project_type: f("type"),
        },
    };
    Effect::Call(action)
}

/// Text back from $EDITOR for field `index` of the open form.
pub fn set_edited_field(ui: &mut UiState, index: usize, text: String) {
    if let Some(Popup::Form(form)) = ui.popup.as_mut() {
        form.set_text(index, text);
    }
}

/// Current text of field `index` of the open form, for $EDITOR.
pub fn field_text(ui: &UiState, index: usize) -> String {
    match &ui.popup {
        Some(Popup::Form(form)) => form
            .fields
            .get(index)
            .map_or(String::new(), |f| f.value().to_string()),
        _ => String::new(),
    }
}

/// Run `action` and put the outcome in a popup: the result, or the refusal
/// verbatim. The caller refreshes the snapshot afterwards either way.
pub fn perform(ctx: &Ctx, ui: &mut UiState, action: &Action) {
    match run(ctx, action) {
        Ok(Some(p)) => ui.popup = Some(p),
        Ok(None) => {}
        Err(e) => ui.popup = Some(Popup::error(&e)),
    }
}

/// The gig-core call behind `action`, with the same arguments the CLI passes.
/// Returns a popup when the result has something to show.
pub fn run(ctx: &Ctx, action: &Action) -> Result<Option<Popup>> {
    match action {
        Action::Start { slug } => {
            orders::start(ctx, Some(slug))?;
            Ok(None)
        }
        Action::Paid { slug, date, amount } => {
            let amount = opt_amount(amount)?;
            orders::paid(ctx, Some(slug), non_empty(date), amount)?;
            Ok(None)
        }
        Action::Price {
            slug,
            amount,
            reason,
        } => {
            orders::price(ctx, Some(slug), parse_amount(amount)?, reason.trim())?;
            Ok(None)
        }
        Action::Change {
            slug,
            description,
            delta,
        } => {
            let delta = opt_amount(delta)?.unwrap_or(0);
            orders::change(ctx, Some(slug), description, delta)?;
            Ok(None)
        }
        Action::Note { slug, text } => {
            orders::note(ctx, Some(slug), text)?;
            Ok(None)
        }
        Action::Scorecard {
            slug,
            decisions,
            repeat_questions,
            cleanups,
            report_reworks,
            score,
            note,
        } => {
            orders::scorecard(
                ctx,
                Some(slug),
                &orders::ScorecardInput {
                    decisions: count("decisions", decisions)?,
                    repeat_questions: count("repeat_questions", repeat_questions)?,
                    cleanups: count("cleanups", cleanups)?,
                    report_reworks: count("report_reworks", report_reworks)?,
                    score: count("score", score)?,
                    note: non_empty(note).map(str::to_string),
                },
            )?;
            Ok(None)
        }
        Action::Cancel { slug, reason } => {
            orders::cancel(ctx, Some(slug), reason, true)?;
            Ok(None)
        }
        Action::ArchivePreview { slug } => {
            let report = archive::archive(
                ctx,
                Some(slug),
                &archive::ArchiveOptions {
                    yes: false,
                    before_warranty_end: false,
                    no_scorecard: false,
                    purge: false,
                },
            )?;
            Ok(Some(archive_popup(&report)))
        }
        Action::NewOrder(n) => {
            let price_minor = opt_amount(&n.price)?;
            let project_type = ProjectType::parse(&n.project_type)?;
            let created = orders::new(
                ctx,
                &orders::NewOrderInput {
                    slug: n.slug.trim(),
                    title: n.title.trim(),
                    price_minor,
                    currency: None,
                    cut_ratio: None,
                    project_type,
                    material_path: non_empty(&n.material),
                    platform: non_empty(&n.platform),
                    external_id: None,
                    client_words: non_empty(&n.client_words),
                    from_draft: n.from_draft,
                    adopt: false,
                    adopt_status: None,
                    no_scaffold: false,
                },
            )?;
            Ok(Some(new_order_popup(&created)))
        }
        Action::NewDraft {
            slug,
            title,
            material,
            project_type,
        } => {
            let project_type = match project_type.trim() {
                "" | NO_TYPE => None,
                t => Some(ProjectType::parse(t)?),
            };
            let created = drafts::new(
                ctx,
                slug.trim(),
                non_empty(title),
                non_empty(material),
                project_type,
            )?;
            Ok(Some(Popup::message(
                format!("draft {}", created.draft.slug),
                vec![
                    format!("created draft {}", created.draft.slug),
                    format!("notes: {}", created.notes_path.display()),
                ],
            )))
        }
    }
}

fn non_empty(s: &str) -> Option<&str> {
    let t = s.trim();
    (!t.is_empty()).then_some(t)
}

fn opt_amount(s: &str) -> Result<Option<i64>> {
    non_empty(s).map(parse_amount).transpose()
}

fn count(name: &str, s: &str) -> Result<i64> {
    s.trim()
        .parse()
        .map_err(|_| Error::InvalidInput(format!("{name} must be a whole number, got {s:?}")))
}

/// Result of `N`: what was created, then the next step (spec 2.1).
fn new_order_popup(c: &orders::OrderCreated) -> Popup {
    let mut lines = vec![format!("registered {} ({})", c.order.slug, c.order.status)];
    if let Some(dev) = &c.order.dev_path {
        lines.push(format!("directory: {dev}"));
    }
    if !c.created_files.is_empty() {
        lines.push("created files:".into());
        lines.extend(c.created_files.iter().map(|p| format!("  {}", p.display())));
    }
    for w in &c.warnings {
        lines.push(format!("warning: {w}"));
    }
    lines.push(String::new());
    lines.push(
        "Next step: grill. Have the agent grill the requirements in the project directory.".into(),
    );
    Popup::message(format!("new order {}", c.order.slug), lines)
}

/// `A`: the dry-run report; the TUI never archives (spec 2.1, 7).
fn archive_popup(r: &archive::ArchiveReport) -> Popup {
    let mut lines = Vec::new();
    match (&r.source, &r.destination) {
        (Some(s), Some(d)) => lines.push(format!("{s} -> {d}")),
        (Some(s), None) => lines.push(format!("{s} (purge)")),
        _ => {}
    }
    section(&mut lines, "blockers", &r.blockers);
    section(&mut lines, "dirty files", &r.git_dirty);
    let large: Vec<String> = r
        .large_files
        .iter()
        .map(|f| format!("{} ({:.1} MB)", f.path, f.bytes as f64 / 1_048_576.0))
        .collect();
    section(&mut lines, "large files", &large);
    section(&mut lines, "unsent packages", &r.unsent_packages);
    if r.missing_scorecard {
        lines.push("scorecard: missing".into());
    }
    lines.push(String::new());
    lines.push("Preview only. Have the agent archive this order.".into());
    Popup::message(format!("archive preview {}", r.order.slug), lines)
}

fn section(lines: &mut Vec<String>, name: &str, items: &[String]) {
    if items.is_empty() {
        lines.push(format!("{name}: none"));
    } else {
        lines.push(format!("{name}:"));
        lines.extend(items.iter().map(|i| format!("  - {i}")));
    }
}

/// Newest link among the order's packages and artifacts: the short link when
/// there is one, else the full url.
pub fn latest_link(row: &OrderRow) -> Option<String> {
    let packages = row.packages.iter().filter_map(|p| {
        let link = p.short_url.clone().or_else(|| p.remote_url.clone())?;
        let at = p.sent_at.clone().unwrap_or_else(|| p.updated_at.clone());
        Some((at, link))
    });
    let artifacts = row.artifacts.iter().filter_map(|a| {
        let link = a.short_url.clone().or_else(|| a.remote_url.clone())?;
        Some((a.uploaded_at.clone(), link))
    });
    packages
        .chain(artifacts)
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, link)| link)
}

fn minor_text(m: Option<i64>) -> String {
    m.map(format_minor).unwrap_or_default()
}

pub fn paid_form(row: &OrderRow, today: &str) -> Form {
    let o = &row.order;
    Form::new(
        format!("paid {}", o.slug),
        FormKind::Paid {
            slug: o.slug.clone(),
        },
        vec![
            Field::text("date", today),
            Field::text("amount", minor_text(o.price_minor)),
        ],
    )
}

pub fn price_form(row: &OrderRow) -> Form {
    let o = &row.order;
    Form::new(
        format!("price {}", o.slug),
        FormKind::Price {
            slug: o.slug.clone(),
        },
        vec![
            Field::text("amount", minor_text(o.price_minor)),
            Field::text("reason", ""),
        ],
    )
}

pub fn change_form(row: &OrderRow) -> Form {
    let o = &row.order;
    Form::new(
        format!("requirement change {}", o.slug),
        FormKind::Change {
            slug: o.slug.clone(),
        },
        vec![
            Field::editor("description", ""),
            Field::text("price delta", "0"),
        ],
    )
}

pub fn scorecard_form(row: &OrderRow) -> Form {
    let o = &row.order;
    let s = row.scorecard.as_ref();
    let n = |v: Option<i64>| v.unwrap_or(0).to_string();
    let score = s.and_then(|s| s.score).unwrap_or(3).to_string();
    Form::new(
        format!("scorecard {}", o.slug),
        FormKind::Scorecard {
            slug: o.slug.clone(),
        },
        vec![
            Field::text("decisions", n(s.and_then(|s| s.decisions))),
            Field::text("repeat qs", n(s.and_then(|s| s.repeat_questions))),
            Field::text("cleanups", n(s.and_then(|s| s.cleanups))),
            Field::text("report reworks", n(s.and_then(|s| s.report_reworks))),
            Field::select("score", &["1", "2", "3", "4", "5"], &score),
            Field::text("note", s.and_then(|s| s.note.clone()).unwrap_or_default()),
        ],
    )
}

pub fn cancel_form(row: &OrderRow) -> Form {
    let o = &row.order;
    Form::new(
        format!("cancel {}", o.slug),
        FormKind::Cancel {
            slug: o.slug.clone(),
        },
        vec![Field::text("reason", "")],
    )
}

/// `N`, or `P` on a draft: `from_draft` on and the slug fixed.
pub fn new_order_form(draft: Option<&Draft>) -> Form {
    let types = project_types();
    let ty = draft
        .and_then(|d| d.project_type)
        .map_or("custom", |t| t.as_str());
    let slug = Field::text("slug", draft.map_or("", |d| d.slug.as_str()));
    let from_draft = Field::toggle("from draft", draft.is_some());
    let (slug, from_draft) = if draft.is_some() {
        (slug.locked(), from_draft.locked())
    } else {
        (slug, from_draft)
    };
    let title = match draft {
        Some(d) => format!("promote draft {}", d.slug),
        None => "new order".to_string(),
    };
    Form::new(
        title,
        FormKind::NewOrder,
        vec![
            slug,
            Field::text(
                "title",
                draft.and_then(|d| d.title.clone()).unwrap_or_default(),
            ),
            Field::text("price", ""),
            Field::select("type", &types, ty),
            Field::text(
                "material",
                draft
                    .and_then(|d| d.material_path.clone())
                    .unwrap_or_default(),
            ),
            Field::text("platform", ""),
            Field::editor("client words", ""),
            from_draft,
        ],
    )
}

pub fn new_draft_form() -> Form {
    let mut types = vec![NO_TYPE];
    types.extend(project_types());
    Form::new(
        "new draft",
        FormKind::NewDraft,
        vec![
            Field::text("slug", ""),
            Field::text("title", ""),
            Field::text("material", ""),
            Field::select("type", &types, NO_TYPE),
        ],
    )
}
