//! Order and draft actions (spec 2.1 and 2.2): which key opens which form,
//! what a submitted form turns into, and the gig-core call behind it.
//!
//! Everything here is terminal-free so tests can drive it with key events and
//! a `Ctx`: key handlers return an `Effect`, and `App` carries out the effects
//! that need the terminal ($EDITOR, clipboard). Every service call passes the
//! order's slug as the key; `None` would resolve the TUI's own cwd.

use crate::app::{UiState, View};
use crate::data::{self, OrderRow};
use crate::popup::{Field, Form, Pick, PickFor, Popup, PopupKey};
use crate::upload::{self, UploadJob, UploadKind};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use gig_core::delivery::DryRunUploader;
use gig_core::models::{Channel, Draft, ProjectType};
use gig_core::money::{format_minor, parse_amount};
use gig_core::services::{archive, artifacts, drafts, orders, packages, Ctx};
use gig_core::{clock, Error, Result};
use std::path::PathBuf;

/// Which form a popup is, and the order it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormKind {
    Paid {
        slug: String,
    },
    Price {
        slug: String,
    },
    Change {
        slug: String,
    },
    Scorecard {
        slug: String,
    },
    Cancel {
        slug: String,
    },
    /// `m` after the package pick: channel and note.
    MarkSent {
        slug: String,
        package_id: String,
    },
    /// `U`: the file path.
    Artifact {
        slug: String,
    },
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
    /// `u` after the pick: dry run, then the confirmation popup.
    UploadPreview {
        slug: String,
        package_id: String,
    },
    /// `m` after the form: dry run, then the confirmation popup.
    SentPreview(MarkSent),
    /// Only built from a confirmed popup; runs `packages::sent(yes=true)`.
    MarkSent(MarkSent),
    /// `U` after the path form: dry run, then the confirmation popup.
    ArtifactPreview {
        slug: String,
        path: String,
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

/// Input of `m` (mark sent).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkSent {
    pub slug: String,
    pub package_id: String,
    /// `phone` or `other`; oss goes through `u`.
    pub channel: String,
    pub note: String,
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
    /// Build the job's uploader and run the upload with a progress popup,
    /// then show and copy the link (`App::upload`).
    Upload(UploadJob),
}

/// Choices of the project type select, as gig-core spells them.
fn project_types() -> Vec<&'static str> {
    ProjectType::ALL.iter().map(|t| t.as_str()).collect()
}

/// The draft type select also allows "none".
const NO_TYPE: &str = "none";

/// Keys of the Orders view and the detail (spec 2.1 table), and of the
/// Drafts view (2.2). `None` means "not an action key
/// here", so the caller can pass the key on.
pub fn view_key(ui: &mut UiState, key: KeyEvent) -> Option<Effect> {
    // Ctrl and Alt chords are never action keys (raw mode passes Ctrl+S
    // through); Shift stays allowed for `$`, `A`, `U`, `N`, `P`.
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return None;
    }
    let KeyCode::Char(c) = key.code else {
        return None;
    };
    if ui.view == View::Drafts && !ui.detail_open {
        return drafts_key(ui, c);
    }
    // Money: `y` copies the link of the selected outstanding order.
    if ui.view == View::Money && !ui.detail_open {
        if c != 'y' {
            return None;
        }
        let id = ui.selected_owed()?;
        let row = ui.data.order(id)?;
        return Some(copy_link(ui, row.order.slug.clone(), latest_link(row)));
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
        'u' => open_pick(ui, PickFor::Upload { slug }),
        'm' => open_pick(ui, PickFor::MarkSent { slug }),
        'U' => open(ui, artifact_form(&slug)),
        'e' => match data::job_md_edit_path(&row.order) {
            Ok(p) => Effect::EditFile(p),
            Err(why) => {
                ui.popup = Some(Popup::error_text("edit JOB.md", why));
                Effect::None
            }
        },
        'y' => {
            let link = latest_link(row);
            copy_link(ui, slug, link)
        }
        _ => return None,
    };
    Some(effect)
}

/// `y`: copy `link`, or say on the message row that there is none.
fn copy_link(ui: &mut UiState, slug: String, link: Option<String>) -> Effect {
    match link {
        Some(link) => Effect::Copy(link),
        None => {
            ui.toast = Some(crate::app::Toast::warn(format!(
                "{slug} has no uploaded link yet"
            )));
            Effect::None
        }
    }
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

/// Lines of NOTES.md shown by `Enter` in Drafts.
pub const NOTES_TAIL: usize = 30;

/// `Enter` in Drafts: the tail of the selected draft's NOTES.md (read only).
/// With `pane` (Medium and Wide) the tail opens as the right pane
/// instead of a popup.
pub fn draft_notes(ui: &mut UiState, pane: bool) {
    let Some(d) = ui.selected_draft() else {
        return;
    };
    if pane {
        let lines = match drafts::read_notes(d) {
            Some(text) => {
                let all: Vec<&str> = text.lines().collect();
                let skip = all.len().saturating_sub(NOTES_TAIL);
                all[skip..].iter().map(|l| l.to_string()).collect()
            }
            None => vec![format!("no NOTES.md in {}", d.notes_dir)],
        };
        ui.notes_pane = Some(crate::app::NotesPane {
            draft_id: d.id,
            lines,
        });
        return;
    }
    let title = format!("notes {}", d.slug);
    ui.popup = Some(match drafts::read_notes(d) {
        Some(text) => {
            let all: Vec<&str> = text.lines().collect();
            let skip = all.len().saturating_sub(NOTES_TAIL);
            let mut lines = Vec::new();
            if skip > 0 {
                lines.push(format!("({skip} earlier lines)"));
            }
            lines.extend(all[skip..].iter().map(|l| l.to_string()));
            if lines.is_empty() {
                lines.push("(NOTES.md is empty)".into());
            }
            // Opened at the end: the newest notes matter most.
            Popup::message_at_end(title, lines)
        }
        None => Popup::error_text(title, format!("no NOTES.md in {}", d.notes_dir)),
    });
}

fn open(ui: &mut UiState, form: Form) -> Effect {
    ui.popup = Some(Popup::Form(form));
    Effect::None
}

/// `u` and `m`: the checked packages of the selected order, or a message
/// when there are none.
fn open_pick(ui: &mut UiState, purpose: PickFor) -> Effect {
    let (slug, title) = match &purpose {
        PickFor::Upload { slug } => (slug.clone(), format!("upload package {slug}")),
        PickFor::MarkSent { slug } => (slug.clone(), format!("mark sent {slug}")),
    };
    let items: Vec<(String, String)> = ui
        .selected_order()
        .map(|row| {
            upload::checked_packages(row)
                .into_iter()
                .map(|p| {
                    let files = p
                        .file_count
                        .map_or(String::new(), |n| format!("  {n} files"));
                    (
                        p.package_id.clone(),
                        format!("{}  {}{files}", p.package_id, p.kind),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    ui.popup = Some(if items.is_empty() {
        Popup::message(
            title,
            vec![
                format!("{slug} has no checked package to send."),
                "Have the agent build and check one first.".into(),
            ],
        )
    } else {
        Popup::Pick(Pick {
            title,
            purpose,
            items,
            selected: 0,
        })
    });
    Effect::None
}

/// Enter in a pick list.
fn picked(ui: &mut UiState, pick: Pick) -> Effect {
    let Some(package_id) = pick.value().map(str::to_string) else {
        return Effect::None;
    };
    match pick.purpose {
        PickFor::Upload { slug } => Effect::Call(Action::UploadPreview { slug, package_id }),
        PickFor::MarkSent { slug } => open(ui, mark_sent_form(&slug, &package_id)),
    }
}

/// A key while a popup is open. Esc closes any popup.
pub fn popup_key(ui: &mut UiState, key: KeyEvent) -> Effect {
    let Some(popup) = ui.popup.as_mut() else {
        return Effect::None;
    };
    let left = match &*popup {
        Popup::Form(form) => Some(form.focus),
        _ => None,
    };
    let out = popup.handle_key(key);
    // Leaving the slug field checks it, so the error shows before submit
    // (section 12.3). An empty slug is only reported on submit.
    if let (Some(from), Some(Popup::Form(form))) = (left, ui.popup.as_ref()) {
        let is_slug = form.fields.get(from).is_some_and(|f| f.label == "slug");
        if form.focus != from && is_slug && !form.fields[from].value().trim().is_empty() {
            let mut form = form.clone();
            validate(ui, &mut form);
            ui.popup = Some(Popup::Form(form));
        }
    }
    match out {
        PopupKey::None => Effect::None,
        PopupKey::Close => {
            // Closing a refusal brings back the form that was refused, with
            // everything typed; closing anything else forgets it.
            let refused = matches!(ui.popup, Some(Popup::Message { error: true, .. }));
            ui.popup = None;
            let form = ui.last_form.take();
            if refused {
                ui.popup = form.map(Popup::Form);
            }
            Effect::None
        }
        PopupKey::EditField(i) => Effect::EditField(i),
        PopupKey::Confirmed => match ui.popup.take() {
            Some(Popup::Confirm { then, .. }) => then,
            _ => Effect::None,
        },
        PopupKey::Submit => match ui.popup.take() {
            Some(Popup::Form(form)) => {
                let kept = form.clone();
                let effect = submit(ui, form);
                // Kept until the call succeeds (see `perform`).
                ui.last_form = Some(kept);
                effect
            }
            Some(Popup::Pick(pick)) => picked(ui, pick),
            other => {
                ui.popup = other;
                Effect::None
            }
        },
    }
}

/// A click on form field `i` (a locked field keeps the focus where it
/// is). Leaving a filled slug field checks it, as Tab does.
pub fn focus_field(ui: &mut UiState, i: usize) {
    let Some(Popup::Form(form)) = ui.popup.as_mut() else {
        return;
    };
    if form.fields.get(i).is_none_or(|f| f.locked) || form.focus == i {
        return;
    }
    let from = form.focus;
    form.focus = i;
    let is_slug = form.fields.get(from).is_some_and(|f| f.label == "slug");
    if is_slug && !form.fields[from].value().trim().is_empty() {
        let mut form = form.clone();
        validate(ui, &mut form);
        ui.popup = Some(Popup::Form(form));
    }
}

/// Checks that need no database round trip: the slug of a new order or
/// draft is valid and free. Each problem is set on its field; true when
/// there is none.
fn validate(ui: &UiState, form: &mut Form) -> bool {
    let taken: Vec<&str> = match form.kind {
        FormKind::NewOrder => ui
            .data
            .orders
            .iter()
            .map(|r| r.order.slug.as_str())
            .collect(),
        FormKind::NewDraft => ui.data.drafts.iter().map(|d| d.slug.as_str()).collect(),
        _ => return true,
    };
    let Some(field) = form.field_mut("slug") else {
        return true;
    };
    if field.locked {
        return true;
    }
    let slug = field.value().trim().to_string();
    field.error = if slug.is_empty() {
        Some("slug is required".into())
    } else if gig_core::services::validate_slug(&slug).is_err() {
        Some("lowercase letters, digits, - _ . only".into())
    } else if taken.contains(&slug.as_str()) {
        Some("slug already exists".into())
    } else {
        None
    };
    field.error.is_none()
}

/// A submitted form: a call, or (for cancel) the typed-`y` confirmation.
fn submit(ui: &mut UiState, mut form: Form) -> Effect {
    if !validate(ui, &mut form) {
        let which = form
            .fields
            .iter()
            .find_map(|f| f.error.as_ref().map(|e| format!("{}: {e}", f.label)))
            .unwrap_or_default();
        ui.toast = Some(crate::app::Toast::error(which));
        ui.popup = Some(Popup::Form(form));
        return Effect::None;
    }
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
            ui.popup = Some(Popup::confirm_danger(
                format!("cancel {slug}"),
                vec![
                    format!("Cancel order {slug}?"),
                    format!("reason: {}", form.get("reason")),
                    "The project directory stays; archive it later.".into(),
                ],
                Effect::Call(Action::Cancel {
                    slug: slug.clone(),
                    reason: f("reason"),
                }),
            ));
            return Effect::None;
        }
        FormKind::MarkSent { slug, package_id } => Action::SentPreview(MarkSent {
            slug: slug.clone(),
            package_id: package_id.clone(),
            channel: f("channel"),
            note: f("note"),
        }),
        FormKind::Artifact { slug } => Action::ArtifactPreview {
            slug: slug.clone(),
            path: resolve_artifact_path(ui, slug, form.get("path")),
        },
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

/// The path typed in `U`: `~` expanded, and a relative path taken from the
/// order's project directory (not from where gig tui was started). The
/// confirmation shows the absolute path either way.
fn resolve_artifact_path(ui: &UiState, slug: &str, typed: &str) -> String {
    let typed = typed.trim();
    if typed.is_empty() {
        return String::new();
    }
    let path = upload::expand_home(typed);
    if path.is_absolute() {
        return path.display().to_string();
    }
    let dev = ui
        .data
        .orders
        .iter()
        .find(|r| r.order.slug == slug)
        .and_then(|r| r.order.dev_path.as_deref());
    match dev {
        Some(dev) => std::path::Path::new(dev).join(path).display().to_string(),
        None => path.display().to_string(),
    }
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
        Ok(Some(p)) => {
            ui.popup = Some(p);
            ui.last_form = None;
        }
        Ok(None) => ui.last_form = None,
        // `last_form` stays: closing the refusal reopens the form.
        Err(e) => ui.popup = Some(Popup::error(&e)),
    }
}

/// The gig-core call behind `action`, with the same arguments the CLI passes.
/// Returns a popup when the result has something to show.
pub fn run(ctx: &Ctx, action: &Action) -> Result<Option<Popup>> {
    match action {
        Action::Start { slug } => {
            let o = orders::start(ctx, Some(slug))?;
            Ok(Some(done(
                format!("start {slug}"),
                vec![format!("started {slug}: now {}", o.status)],
            )))
        }
        Action::Paid { slug, date, amount } => {
            let amount = opt_amount(amount)?;
            let o = orders::paid(ctx, Some(slug), non_empty(date), amount)?;
            let mut lines = vec![format!("{slug} is {}", o.status)];
            if let Some(p) = &o.paid_at {
                lines.push(format!("paid: {p}"));
            }
            if let Some(w) = &o.warranty_until {
                lines.push(format!("warranty until {w}"));
            }
            Ok(Some(done(format!("paid {slug}"), lines)))
        }
        Action::Price {
            slug,
            amount,
            reason,
        } => {
            let o = orders::price(ctx, Some(slug), parse_amount(amount)?, reason.trim())?;
            Ok(Some(done(
                format!("price {slug}"),
                vec![format!("{slug} price is now {}", minor_text(o.price_minor))],
            )))
        }
        Action::Change {
            slug,
            description,
            delta,
        } => {
            let delta = opt_amount(delta)?.unwrap_or(0);
            let shown = orders::change(ctx, Some(slug), description, delta)?;
            Ok(Some(done(
                format!("change {slug}"),
                vec![
                    format!("recorded a requirement change for {slug}"),
                    format!("price: {}", minor_text(shown.order.price_minor)),
                ],
            )))
        }
        Action::Note { slug, text } => {
            orders::note(ctx, Some(slug), text)?;
            Ok(Some(done(
                format!("note {slug}"),
                vec![format!("added a note to {slug}")],
            )))
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
            Ok(Some(done(
                format!("scorecard {slug}"),
                vec![format!("saved the scorecard of {slug}")],
            )))
        }
        Action::Cancel { slug, reason } => {
            let r = orders::cancel(ctx, Some(slug), reason, true)?;
            Ok(Some(done(
                format!("cancel {slug}"),
                vec![format!("{slug} is {}", r.order.status)],
            )))
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
        Action::UploadPreview { slug, package_id } => {
            let dry = packages::upload(
                ctx,
                Some(slug),
                package_id,
                false,
                &DryRunUploader::default(),
            )?;
            let kind = dry.package.kind;
            let mut lines = vec![
                format!("Upload package {package_id} of {slug}?"),
                format!("kind: {kind}"),
                format!(
                    "size: {} ({} bytes)",
                    upload::human_size(dry.size),
                    dry.size
                ),
                format!(
                    "order: {} -> {}",
                    dry.order_status,
                    upload::state_after(dry.order_status, kind)
                ),
            ];
            lines.extend(dry.warnings.iter().map(|w| format!("warning: {w}")));
            lines.push("The link is copied to the clipboard when done.".into());
            Ok(Some(Popup::confirm(
                format!("upload {package_id}"),
                lines,
                Effect::Upload(UploadJob::new(
                    UploadKind::Package {
                        slug: slug.clone(),
                        package_id: package_id.clone(),
                    },
                    &ctx.config.delivery,
                )),
            )))
        }
        Action::SentPreview(m) => {
            let channel = Channel::parse(&m.channel)?;
            let note = non_empty(&m.note);
            let dry = packages::sent(ctx, Some(&m.slug), &m.package_id, channel, note, false)?;
            let kind = dry.package.kind;
            let mut lines = vec![
                format!(
                    "Record {} of {} as sent via {channel}?",
                    m.package_id, m.slug
                ),
                format!("kind: {kind}"),
                format!(
                    "size: {} ({} bytes)",
                    upload::human_size(dry.size),
                    dry.size
                ),
                format!(
                    "order: {} -> {}",
                    dry.order_status,
                    upload::state_after(dry.order_status, kind)
                ),
                format!("note: {}", note.unwrap_or("(none)")),
            ];
            lines.extend(dry.warnings.iter().map(|w| format!("warning: {w}")));
            Ok(Some(Popup::confirm(
                format!("mark sent {}", m.package_id),
                lines,
                Effect::Call(Action::MarkSent(m.clone())),
            )))
        }
        Action::MarkSent(m) => {
            let channel = Channel::parse(&m.channel)?;
            let r = packages::sent(
                ctx,
                Some(&m.slug),
                &m.package_id,
                channel,
                non_empty(&m.note),
                true,
            )?;
            Ok(Some(Popup::message(
                format!("sent {}", m.package_id),
                vec![
                    format!("recorded {} as sent via {channel}", m.package_id),
                    format!("order: {}", r.order_status),
                ],
            )))
        }
        Action::ArtifactPreview { slug, path } => {
            if path.trim().is_empty() {
                return Err(Error::InvalidInput("path is empty".into()));
            }
            let file = upload::expand_home(path);
            let dry = artifacts::upload(ctx, Some(slug), &file, false, &DryRunUploader::default())?;
            Ok(Some(Popup::confirm(
                format!("upload artifact {slug}"),
                vec![
                    format!("Upload {} for {slug}?", dry.local_path),
                    format!(
                        "size: {} ({} bytes)",
                        upload::human_size(dry.size),
                        dry.size
                    ),
                    "The order status does not change.".into(),
                    "The link is copied to the clipboard when done.".into(),
                ],
                Effect::Upload(UploadJob::new(
                    UploadKind::Artifact {
                        slug: slug.clone(),
                        path: dry.local_path.into(),
                    },
                    &ctx.config.delivery,
                )),
            )))
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

/// Result popup of a write, so no write goes unacknowledged.
fn done(title: String, lines: Vec<String>) -> Popup {
    Popup::done(title, lines)
}

impl Action {
    /// Text of the busy popup drawn before a call that can take a while on
    /// the UI thread (hashing a package, walking a project, scaffolding);
    /// `None` for quick database writes.
    pub fn busy_text(&self) -> Option<&'static str> {
        match self {
            Action::UploadPreview { .. } | Action::SentPreview(_) | Action::MarkSent(_) => {
                Some("checking the package...")
            }
            Action::ArtifactPreview { .. } => Some("reading the file..."),
            Action::ArchivePreview { .. } => {
                Some("checking the project (dirty files, large files)...")
            }
            Action::NewOrder(_) => Some("creating the project directory..."),
            Action::NewDraft { .. } => Some("creating the draft..."),
            _ => None,
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

/// `m` after the pick. Channel oss is `u`'s job, so it is not offered.
pub fn mark_sent_form(slug: &str, package_id: &str) -> Form {
    Form::new(
        format!("mark sent {package_id}"),
        FormKind::MarkSent {
            slug: slug.to_string(),
            package_id: package_id.to_string(),
        },
        vec![
            Field::select("channel", &["phone", "other"], "phone"),
            Field::text("note", ""),
        ],
    )
}

/// `U`: one typed path (no $EDITOR); `~/` is expanded.
pub fn artifact_form(slug: &str) -> Form {
    Form::new(
        format!("upload artifact {slug}"),
        FormKind::Artifact {
            slug: slug.to_string(),
        },
        vec![Field::text("path", "")],
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
