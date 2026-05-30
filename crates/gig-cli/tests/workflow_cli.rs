use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use gig_core::db;
use gig_core::models::{DeliveryPackageStatus, OrderStatus, ProjectType, QuoteDraftStatus};
use gig_core::repo::quote_drafts::{self, NewQuoteDraft};
use gig_core::repo::{delivery_artifacts, delivery_packages, order_workflow, orders};
use serde_json::Value;

fn unique_temp_dir(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    path.push(format!("gig-{name}-{}-{nanos}", std::process::id()));
    path
}

fn gig_command(config_home: &Path, data_home: &Path, state_home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_gig"));
    command
        .env("XDG_CONFIG_HOME", config_home)
        .env("XDG_DATA_HOME", data_home)
        .env("XDG_STATE_HOME", state_home);
    command
}

fn isolated_homes(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    (
        unique_temp_dir(&format!("{name}-config")),
        unique_temp_dir(&format!("{name}-data")),
        unique_temp_dir(&format!("{name}-state")),
    )
}

fn seed_quote_draft(data_home: &Path, slug: &str, title: &str) {
    let conn = db::open(&data_home.join("gig/gig.db")).unwrap();
    quote_drafts::insert(
        &conn,
        &NewQuoteDraft {
            slug,
            title,
            client_label: Some("Client A"),
            source_org: Some("wechat"),
            project_type: ProjectType::Crawler,
            status: QuoteDraftStatus::QuoteDraft,
            summary: "Need a crawler for listings",
            quote_min: None,
            quote_recommended: None,
            quote_max: None,
            currency: "CNY",
            xdg_path: data_home
                .join(format!("gig/quotes/{slug}"))
                .to_str()
                .unwrap(),
            created_at: "2026-05-27T00:00:00Z",
            updated_at: "2026-05-27T00:00:00Z",
        },
    )
    .unwrap();
}

fn seed_order_with_workflow(data_home: &Path, status: OrderStatus, gig_dir: &Path) -> i64 {
    let conn = db::open(&data_home.join("gig/gig.db")).unwrap();
    let order = orders::insert(
        &conn,
        &orders::NewOrder {
            slug: Some("workflow-order"),
            external_id: None,
            title: "Build a crawler",
            client_id: None,
            source_org: Some("wechat"),
            source_id: None,
            project_type: Some(ProjectType::Crawler),
            status,
            quoted_price: Some(15_000),
            final_price: None,
            my_cut_ratio: 0.6,
            currency: "CNY",
            notes: Some("Need a crawler for listings"),
            created_at: 1_700_000_000,
            accepted_at: Some(1_700_000_000),
        },
    )
    .unwrap();
    order_workflow::insert(
        &conn,
        &order_workflow::NewOrderWorkflow {
            order_id: order.id,
            project_type: Some(ProjectType::Crawler),
            gig_dir: Some(gig_dir.to_str().unwrap()),
            index_path: Some(gig_dir.join("INDEX.html").to_str().unwrap()),
            job_path: Some(gig_dir.join("JOB.md").to_str().unwrap()),
            quote_path: Some(gig_dir.join("QUOTE.md").to_str().unwrap()),
            plan_md_path: Some(gig_dir.join("plan/PLAN.md").to_str().unwrap()),
            plan_html_path: Some(gig_dir.join("plan/PLAN.html").to_str().unwrap()),
            acceptance_path: Some(gig_dir.join("acceptance/ACCEPTANCE.md").to_str().unwrap()),
            created_at: "2026-05-27T02:00:00Z",
            updated_at: "2026-05-27T02:00:00Z",
        },
    )
    .unwrap();
    order.id
}

fn seed_legacy_order_without_workflow(data_home: &Path, status: OrderStatus, slug: &str) -> i64 {
    let conn = db::open(&data_home.join("gig/gig.db")).unwrap();
    orders::insert(
        &conn,
        &orders::NewOrder {
            slug: Some(slug),
            external_id: None,
            title: "Legacy order",
            client_id: None,
            source_org: Some("wechat"),
            source_id: None,
            project_type: None,
            status,
            quoted_price: Some(15_000),
            final_price: Some(15_000),
            my_cut_ratio: 0.6,
            currency: "CNY",
            notes: Some("Created before workflow metadata existed"),
            created_at: 1_700_000_000,
            accepted_at: Some(1_700_000_000),
        },
    )
    .unwrap()
    .id
}

#[test]
fn quote_new_json_invalid_project_type_emits_machine_readable_error_to_stderr() {
    let (config_home, data_home, state_home) = isolated_homes("quote-json-error");

    let output = gig_command(&config_home, &data_home, &state_home)
        .args([
            "quote",
            "new",
            "--slug",
            "bad-type",
            "--title",
            "Bad type",
            "--project-type",
            "unknown_kind",
            "--summary",
            "Bad project type",
            "--json",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let json: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(json["status"], "error");
    assert_eq!(json["error"]["code"], "invalid_project_type");
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("unknown project type"));
}

#[test]
fn plan_ready_json_missing_workflow_file_emits_machine_readable_error_to_stderr() {
    let (config_home, data_home, state_home) = isolated_homes("plan-json-error");
    let project_dir = unique_temp_dir("plan-json-error-project");
    let gig_dir = project_dir.join(".gig");
    let order_id = seed_order_with_workflow(&data_home, OrderStatus::Accepted, &gig_dir);

    let output = gig_command(&config_home, &data_home, &state_home)
        .args(["plan", "ready", &order_id.to_string(), "--json"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let json: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(json["status"], "error");
    assert_eq!(json["error"]["code"], "missing_workflow_file");
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("PLAN.md"));
}

#[test]
fn package_send_json_missing_uploader_emits_machine_readable_error_to_stderr() {
    let (config_home, data_home, state_home) = isolated_homes("package-json-error");
    let project_dir = unique_temp_dir("package-json-error-project");
    let gig_dir = project_dir.join(".gig");
    let delivery_dir = prepare_client_delivery(&gig_dir, false);
    let order_id = seed_order_with_workflow(&data_home, OrderStatus::ReadyToDeliver, &gig_dir);

    let output = gig_command(&config_home, &data_home, &state_home)
        .args([
            "package",
            "send",
            &order_id.to_string(),
            "--delivery-date",
            "2026-05-27",
            "--delivery-dir",
            delivery_dir.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let json: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(json["status"], "error");
    assert_eq!(json["error"]["code"], "missing_required_field");
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("no uploader configured"));
}

#[test]
fn package_check_json_validates_existing_package_without_upload_artifact() {
    let (config_home, data_home, state_home) = isolated_homes("package-check-json");
    let project_dir = unique_temp_dir("package-check-json-project");
    let gig_dir = project_dir.join(".gig");
    let delivery_dir = prepare_client_delivery(&gig_dir, true);
    let order_id = seed_order_with_workflow(&data_home, OrderStatus::ReadyToDeliver, &gig_dir);

    let output = gig_command(&config_home, &data_home, &state_home)
        .args([
            "package",
            "check",
            &order_id.to_string(),
            "--delivery-date",
            "2026-05-27",
            "--delivery-dir",
            delivery_dir.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "package check failed
stdout:
{}
stderr:
{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "validated");
    assert_eq!(json["next_action"], "send_package");
    assert_eq!(json["package"]["order_id"], order_id);
    assert_eq!(json["package"]["status"], "validated");
    assert_eq!(
        json["paths"]["package_path"],
        delivery_dir
            .join("export/client-package.zip")
            .to_string_lossy()
            .as_ref()
    );
    assert!(!json.as_object().unwrap().contains_key("artifact"));

    let conn = db::open(&data_home.join("gig/gig.db")).unwrap();
    let packages = delivery_packages::list_for_order(&conn, order_id).unwrap();
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].status, DeliveryPackageStatus::Validated);
    assert!(delivery_artifacts::list_for_order(&conn, order_id)
        .unwrap()
        .is_empty());
}

#[test]
fn artifact_send_json_missing_uploader_emits_machine_readable_error_to_stderr() {
    let (config_home, data_home, state_home) = isolated_homes("artifact-json-error");
    let project_dir = unique_temp_dir("artifact-json-error-project");
    let gig_dir = project_dir.join(".gig");
    let order_id = seed_order_with_workflow(&data_home, OrderStatus::Accepted, &gig_dir);
    let artifact_path = project_dir.join("note.txt");
    fs::create_dir_all(&project_dir).unwrap();
    fs::write(&artifact_path, "standalone artifact").unwrap();

    let output = gig_command(&config_home, &data_home, &state_home)
        .args([
            "artifact",
            "send",
            &order_id.to_string(),
            artifact_path.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let json: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(json["status"], "error");
    assert_eq!(json["error"]["code"], "missing_required_field");
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("no uploader configured"));
}

#[test]
fn ls_json_includes_quote_and_order_decision_items() {
    let (config_home, data_home, state_home) = isolated_homes("ls-json-workflow");
    let project_dir = unique_temp_dir("ls-json-workflow-project");
    let gig_dir = project_dir.join(".gig");
    seed_quote_draft(&data_home, "draft-1", "Build a crawler");
    let order_id = seed_order_with_workflow(&data_home, OrderStatus::PlanReady, &gig_dir);

    let output = gig_command(&config_home, &data_home, &state_home)
        .args(["ls", "--json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "ls --json failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["quote_items"][0]["slug"], "draft-1");
    assert_eq!(json["quote_items"][0]["next_action"], "price_quote");
    assert_eq!(json["order_items"][0]["id"], order_id);
    assert_eq!(json["order_items"][0]["status"], "plan_ready");
    assert_eq!(json["order_items"][0]["next_action"], "approve_plan");
}

#[test]
fn ls_json_marks_ready_to_deliver_without_validated_package_as_needing_package_check() {
    let (config_home, data_home, state_home) = isolated_homes("ls-json-package-check");
    let project_dir = unique_temp_dir("ls-json-package-check-project");
    let gig_dir = project_dir.join(".gig");
    let order_id = seed_order_with_workflow(&data_home, OrderStatus::ReadyToDeliver, &gig_dir);

    let output = gig_command(&config_home, &data_home, &state_home)
        .args(["ls", "--json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "ls --json failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["order_items"][0]["id"], order_id);
    assert_eq!(json["order_items"][0]["status"], "ready_to_deliver");
    assert_eq!(json["order_items"][0]["next_action"], "check_package");
}

#[test]
fn ls_json_marks_validated_ready_to_deliver_package_as_sendable() {
    let (config_home, data_home, state_home) = isolated_homes("ls-json-package-sendable");
    let project_dir = unique_temp_dir("ls-json-package-sendable-project");
    let gig_dir = project_dir.join(".gig");
    let order_id = seed_order_with_workflow(&data_home, OrderStatus::ReadyToDeliver, &gig_dir);
    {
        let conn = db::open(&data_home.join("gig/gig.db")).unwrap();
        delivery_packages::insert(
            &conn,
            &delivery_packages::NewDeliveryPackage {
                order_id,
                delivery_date: "2026-05-27",
                delivery_dir: gig_dir.join("delivery/2026-05-27").to_str().unwrap(),
                client_dir: gig_dir.join("delivery/2026-05-27/client").to_str().unwrap(),
                manifest_path: gig_dir
                    .join("delivery/2026-05-27/manifest.toml")
                    .to_str()
                    .unwrap(),
                package_path: Some(
                    gig_dir
                        .join("delivery/2026-05-27/export/client-package.zip")
                        .to_str()
                        .unwrap(),
                ),
                status: DeliveryPackageStatus::Validated,
                created_at: "2026-05-27T06:00:00Z",
                updated_at: "2026-05-27T06:00:00Z",
            },
        )
        .unwrap();
    }

    let output = gig_command(&config_home, &data_home, &state_home)
        .args(["ls", "--json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "ls --json failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["order_items"][0]["id"], order_id);
    assert_eq!(json["order_items"][0]["status"], "ready_to_deliver");
    assert_eq!(json["order_items"][0]["next_action"], "send_package");
}

#[test]
fn show_json_includes_workflow_paths_and_next_action() {
    let (config_home, data_home, state_home) = isolated_homes("show-json-workflow");
    let project_dir = unique_temp_dir("show-json-workflow-project");
    let gig_dir = project_dir.join(".gig");
    let order_id = seed_order_with_workflow(&data_home, OrderStatus::Accepted, &gig_dir);

    let output = gig_command(&config_home, &data_home, &state_home)
        .args(["show", &order_id.to_string(), "--json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "show --json failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["next_action"], "prepare_plan");
    assert_eq!(json["order"]["id"], order_id);
    assert_eq!(
        json["workflow"]["paths"]["plan_md_path"],
        gig_dir.join("plan/PLAN.md").to_string_lossy().into_owned()
    );
}

#[test]
fn doctor_reports_missing_workflow_files_and_package_artifacts() {
    let (config_home, data_home, state_home) = isolated_homes("doctor-workflow");
    let project_dir = unique_temp_dir("doctor-workflow-project");
    let gig_dir = project_dir.join(".gig");
    let order_id = seed_order_with_workflow(&data_home, OrderStatus::PlanReady, &gig_dir);
    let package_path = gig_dir.join("delivery/2026-05-27/export/client-package.zip");
    {
        let conn = db::open(&data_home.join("gig/gig.db")).unwrap();
        delivery_packages::insert(
            &conn,
            &delivery_packages::NewDeliveryPackage {
                order_id,
                delivery_date: "2026-05-27",
                delivery_dir: gig_dir.join("delivery/2026-05-27").to_str().unwrap(),
                client_dir: gig_dir.join("delivery/2026-05-27/client").to_str().unwrap(),
                manifest_path: gig_dir
                    .join("delivery/2026-05-27/manifest.toml")
                    .to_str()
                    .unwrap(),
                package_path: Some(package_path.to_str().unwrap()),
                status: DeliveryPackageStatus::Validated,
                created_at: "2026-05-27T06:00:00Z",
                updated_at: "2026-05-27T06:00:00Z",
            },
        )
        .unwrap();
    }

    let output = gig_command(&config_home, &data_home, &state_home)
        .args(["doctor"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("missing workflow file"));
    assert!(stdout.contains("PLAN.md"));
    assert!(stdout.contains("missing package artifact"));
    assert!(stdout.contains("client-package.zip"));
}

#[test]
fn legacy_orders_without_workflow_metadata_still_list_show_export_and_archive() {
    let (config_home, data_home, state_home) = isolated_homes("legacy-workflow");
    let accepted_id =
        seed_legacy_order_without_workflow(&data_home, OrderStatus::Accepted, "legacy-accepted");
    let delivered_id =
        seed_legacy_order_without_workflow(&data_home, OrderStatus::Delivered, "legacy-delivered");

    let ls_output = gig_command(&config_home, &data_home, &state_home)
        .args(["ls", "--json"])
        .output()
        .unwrap();
    assert!(
        ls_output.status.success(),
        "ls --json failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&ls_output.stdout),
        String::from_utf8_lossy(&ls_output.stderr)
    );
    let ls_json: Value = serde_json::from_slice(&ls_output.stdout).unwrap();
    let accepted_item = ls_json["order_items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == accepted_id)
        .unwrap();
    assert_eq!(accepted_item["status"], "accepted");
    assert_eq!(
        accepted_item["next_action"],
        "legacy_workflow_metadata_missing"
    );

    let show_output = gig_command(&config_home, &data_home, &state_home)
        .args(["show", &accepted_id.to_string(), "--json"])
        .output()
        .unwrap();
    assert!(
        show_output.status.success(),
        "show --json failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&show_output.stdout),
        String::from_utf8_lossy(&show_output.stderr)
    );
    let show_json: Value = serde_json::from_slice(&show_output.stdout).unwrap();
    assert_eq!(show_json["status"], "ok");
    assert_eq!(show_json["next_action"], "legacy_workflow_metadata_missing");
    assert!(show_json["workflow"]["paths"]["plan_md_path"].is_null());

    let export_output = gig_command(&config_home, &data_home, &state_home)
        .args(["export", "json"])
        .output()
        .unwrap();
    assert!(
        export_output.status.success(),
        "export json failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&export_output.stdout),
        String::from_utf8_lossy(&export_output.stderr)
    );
    let exported: Value = serde_json::from_slice(&export_output.stdout).unwrap();
    assert!(exported
        .as_array()
        .unwrap()
        .iter()
        .any(|order| order["id"] == accepted_id));

    let archive_output = gig_command(&config_home, &data_home, &state_home)
        .args(["archive", &delivered_id.to_string(), "--yes"])
        .output()
        .unwrap();
    assert!(
        archive_output.status.success(),
        "archive failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&archive_output.stdout),
        String::from_utf8_lossy(&archive_output.stderr)
    );
    let conn = db::open(&data_home.join("gig/gig.db")).unwrap();
    let archived = orders::find_by_id(&conn, delivered_id).unwrap();
    assert_eq!(archived.status, OrderStatus::Archived);
}

#[test]
fn quote_new_json_records_draft_without_creating_quote_files() {
    let config_home = unique_temp_dir("quote-new-config");
    let data_home = unique_temp_dir("quote-new-data");
    let state_home = unique_temp_dir("quote-new-state");

    let output = gig_command(&config_home, &data_home, &state_home)
        .args([
            "quote",
            "new",
            "--slug",
            "draft-1",
            "--title",
            "Build a crawler",
            "--project-type",
            "crawler",
            "--summary",
            "Need a crawler for listings",
            "--client-label",
            "Client A",
            "--source-org",
            "wechat",
            "--json",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "quote new failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["status"], "quote_draft");
    assert_eq!(json["next_action"], "price_quote");
    assert_eq!(json["quote_draft"]["slug"], "draft-1");
    assert_eq!(json["quote_draft"]["project_type"], "crawler");
    let expected_xdg_path = data_home
        .join("gig/quotes/draft-1")
        .to_string_lossy()
        .into_owned();
    assert_eq!(json["paths"]["xdg_path"], expected_xdg_path);

    let quote_dir = data_home.join("gig/quotes/draft-1");
    assert!(
        !quote_dir.exists(),
        "quote new must not generate quote files"
    );

    let conn = db::open(&data_home.join("gig/gig.db")).unwrap();
    let draft = quote_drafts::find_by_slug(&conn, "draft-1")
        .unwrap()
        .unwrap();
    assert_eq!(draft.status, QuoteDraftStatus::QuoteDraft);
    assert_eq!(draft.project_type, ProjectType::Crawler);
    assert_eq!(draft.title, "Build a crawler");
    assert_eq!(draft.client_label.as_deref(), Some("Client A"));
    assert_eq!(draft.source_org.as_deref(), Some("wechat"));
    assert_eq!(draft.xdg_path, quote_dir.to_string_lossy());
}

#[test]
fn quote_new_json_accepts_document_pdf_project_type() {
    let config_home = unique_temp_dir("quote-new-document-pdf-config");
    let data_home = unique_temp_dir("quote-new-document-pdf-data");
    let state_home = unique_temp_dir("quote-new-document-pdf-state");

    let output = gig_command(&config_home, &data_home, &state_home)
        .args([
            "quote",
            "new",
            "--slug",
            "document-draft",
            "--title",
            "Polish a PDF report",
            "--project-type",
            "document_pdf",
            "--summary",
            "Need a PDF document polished",
            "--json",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "quote new failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["quote_draft"]["project_type"], "document_pdf");

    let conn = db::open(&data_home.join("gig/gig.db")).unwrap();
    let draft = quote_drafts::find_by_slug(&conn, "document-draft")
        .unwrap()
        .unwrap();
    assert_eq!(
        draft.project_type,
        "document_pdf".parse::<ProjectType>().unwrap()
    );
}

#[test]
fn quote_show_json_reads_draft_by_slug() {
    let (config_home, data_home, state_home) = isolated_homes("quote-show");
    seed_quote_draft(&data_home, "draft-1", "Build a crawler");

    let output = gig_command(&config_home, &data_home, &state_home)
        .args(["quote", "show", "draft-1", "--json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "quote show failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["status"], "quote_draft");
    assert_eq!(json["next_action"], "price_quote");
    assert_eq!(json["quote_draft"]["slug"], "draft-1");
    assert_eq!(json["quote_draft"]["title"], "Build a crawler");
    assert_eq!(json["quote_draft"]["project_type"], "crawler");
}

#[test]
fn quote_list_json_lists_drafts_newest_first() {
    let (config_home, data_home, state_home) = isolated_homes("quote-list");
    seed_quote_draft(&data_home, "draft-1", "First draft");
    seed_quote_draft(&data_home, "draft-2", "Second draft");

    let output = gig_command(&config_home, &data_home, &state_home)
        .args(["quote", "list", "--json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "quote list failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["status"], "ok");
    let drafts = json["quote_drafts"].as_array().unwrap();
    assert_eq!(drafts.len(), 2);
    assert_eq!(drafts[0]["slug"], "draft-2");
    assert_eq!(drafts[1]["slug"], "draft-1");
}

#[test]
fn quote_drop_json_marks_draft_dropped_without_creating_files() {
    let (config_home, data_home, state_home) = isolated_homes("quote-drop");
    seed_quote_draft(&data_home, "draft-1", "Build a crawler");

    let output = gig_command(&config_home, &data_home, &state_home)
        .args([
            "quote",
            "drop",
            "draft-1",
            "--reason",
            "not a fit",
            "--json",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "quote drop failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["status"], "dropped");
    assert_eq!(json["next_action"], "none");
    assert_eq!(json["quote_draft"]["slug"], "draft-1");
    assert_eq!(json["quote_draft"]["drop_reason"], "not a fit");

    let quote_dir = data_home.join("gig/quotes/draft-1");
    assert!(
        !quote_dir.exists(),
        "quote drop must not generate quote files"
    );

    let conn = db::open(&data_home.join("gig/gig.db")).unwrap();
    let draft = quote_drafts::find_by_slug(&conn, "draft-1")
        .unwrap()
        .unwrap();
    assert_eq!(draft.status, QuoteDraftStatus::Dropped);
    assert_eq!(draft.drop_reason.as_deref(), Some("not a fit"));
}

#[test]
fn quote_price_json_records_prices_without_accepting() {
    let (config_home, data_home, state_home) = isolated_homes("quote-price");
    seed_quote_draft(&data_home, "draft-1", "Build a crawler");

    let output = gig_command(&config_home, &data_home, &state_home)
        .args([
            "quote",
            "price",
            "draft-1",
            "--min",
            "100",
            "--recommended",
            "150",
            "--max",
            "200",
            "--json",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "quote price failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["status"], "quoted");
    assert_eq!(json["next_action"], "send_or_accept");
    assert_eq!(json["quote_draft"]["quote_min"], 10_000);
    assert_eq!(json["quote_draft"]["quote_recommended"], 15_000);
    assert_eq!(json["quote_draft"]["quote_max"], 20_000);

    let conn = db::open(&data_home.join("gig/gig.db")).unwrap();
    let draft = quote_drafts::find_by_slug(&conn, "draft-1")
        .unwrap()
        .unwrap();
    assert_eq!(draft.status, QuoteDraftStatus::Quoted);
    assert!(draft.promoted_order_id.is_none());
    assert!(draft.accepted_at.is_none());
}

#[test]
fn quote_mark_sent_json_records_sent_timestamp_without_accepting() {
    let (config_home, data_home, state_home) = isolated_homes("quote-mark-sent");
    seed_quote_draft(&data_home, "draft-1", "Build a crawler");

    let price = gig_command(&config_home, &data_home, &state_home)
        .args([
            "quote",
            "price",
            "draft-1",
            "--min",
            "100",
            "--recommended",
            "150",
            "--max",
            "200",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(price.status.success());

    let output = gig_command(&config_home, &data_home, &state_home)
        .args(["quote", "mark-sent", "draft-1", "--json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "quote mark-sent failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["status"], "quoted");
    assert_eq!(json["next_action"], "send_or_accept");
    assert_eq!(json["quote_draft"]["slug"], "draft-1");

    let conn = db::open(&data_home.join("gig/gig.db")).unwrap();
    let draft = quote_drafts::find_by_slug(&conn, "draft-1")
        .unwrap()
        .unwrap();
    assert_eq!(draft.status, QuoteDraftStatus::Quoted);
    assert!(draft.sent_at.is_some());
    assert!(draft.accepted_at.is_none());
}

#[test]
fn quote_accept_json_rejects_missing_workflow_index_before_promotion() {
    let (config_home, data_home, state_home) = isolated_homes("quote-accept-missing-index");
    let project_dir = unique_temp_dir("quote-accept-missing-index-project");
    let gig_dir = project_dir.join(".gig");
    std::fs::create_dir_all(&gig_dir).unwrap();
    std::fs::write(gig_dir.join("JOB.md"), "workflow-created job").unwrap();
    std::fs::write(gig_dir.join("QUOTE.md"), "workflow-created quote").unwrap();
    seed_quote_draft(&data_home, "draft-1", "Build a crawler");

    let price = gig_command(&config_home, &data_home, &state_home)
        .args([
            "quote",
            "price",
            "draft-1",
            "--min",
            "100",
            "--recommended",
            "150",
            "--max",
            "200",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(price.status.success());

    let output = gig_command(&config_home, &data_home, &state_home)
        .args([
            "quote",
            "accept",
            "draft-1",
            "--project-dir",
            project_dir.to_str().unwrap(),
            "--my-cut-ratio",
            "0.6",
            "--json",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    let json: Value = serde_json::from_str(&stderr).unwrap();
    assert_eq!(json["error"]["code"], "missing_workflow_file");
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("INDEX.html"));

    let conn = db::open(&data_home.join("gig/gig.db")).unwrap();
    let draft = quote_drafts::find_by_slug(&conn, "draft-1")
        .unwrap()
        .unwrap();
    assert_eq!(draft.status, QuoteDraftStatus::Quoted);
    assert_eq!(draft.promoted_order_id, None);
}

#[test]
fn quote_accept_json_promotes_order_and_records_workflow_paths_without_generating_files() {
    let (config_home, data_home, state_home) = isolated_homes("quote-accept");
    let project_dir = unique_temp_dir("quote-accept-project");
    let gig_dir = project_dir.join(".gig");
    std::fs::create_dir_all(&gig_dir).unwrap();
    std::fs::write(gig_dir.join("INDEX.html"), "workflow-created index").unwrap();
    std::fs::write(gig_dir.join("JOB.md"), "workflow-created job").unwrap();
    std::fs::write(gig_dir.join("QUOTE.md"), "workflow-created quote").unwrap();
    seed_quote_draft(&data_home, "draft-1", "Build a crawler");

    let price = gig_command(&config_home, &data_home, &state_home)
        .args([
            "quote",
            "price",
            "draft-1",
            "--min",
            "100",
            "--recommended",
            "150",
            "--max",
            "200",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(price.status.success());

    let output = gig_command(&config_home, &data_home, &state_home)
        .args([
            "quote",
            "accept",
            "draft-1",
            "--project-dir",
            project_dir.to_str().unwrap(),
            "--my-cut-ratio",
            "0.6",
            "--json",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "quote accept failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["status"], "accepted");
    assert_eq!(json["next_action"], "prepare_plan");
    assert_eq!(json["quote_draft"]["slug"], "draft-1");
    assert_eq!(json["order"]["status"], "accepted");
    assert_eq!(json["order"]["project_type"], "crawler");
    assert_eq!(
        json["paths"]["gig_dir"],
        gig_dir.to_string_lossy().into_owned()
    );
    assert_eq!(
        json["paths"]["job_path"],
        gig_dir.join("JOB.md").to_string_lossy().into_owned()
    );
    assert_eq!(
        json["paths"]["quote_path"],
        gig_dir.join("QUOTE.md").to_string_lossy().into_owned()
    );

    assert!(!gig_dir.join("plan/PLAN.md").exists());
    assert!(!gig_dir.join("plan/PLAN.html").exists());
    assert!(!gig_dir.join("acceptance/ACCEPTANCE.md").exists());

    let conn = db::open(&data_home.join("gig/gig.db")).unwrap();
    let draft = quote_drafts::find_by_slug(&conn, "draft-1")
        .unwrap()
        .unwrap();
    assert_eq!(draft.status, QuoteDraftStatus::Accepted);
    let order_id = draft.promoted_order_id.unwrap();
    let order = orders::find_by_id(&conn, order_id).unwrap();
    assert_eq!(order.status, OrderStatus::Accepted);
    assert_eq!(order.project_type, Some(ProjectType::Crawler));
    let workflow = order_workflow::find_by_order_id(&conn, order_id)
        .unwrap()
        .unwrap();
    assert_eq!(workflow.gig_dir.as_deref(), Some(gig_dir.to_str().unwrap()));
}

#[test]
fn plan_ready_json_records_ready_timestamp_without_creating_acceptance_file() {
    let (config_home, data_home, state_home) = isolated_homes("plan-ready");
    let project_dir = unique_temp_dir("plan-ready-project");
    let gig_dir = project_dir.join(".gig");
    let plan_dir = gig_dir.join("plan");
    std::fs::create_dir_all(&plan_dir).unwrap();
    std::fs::write(plan_dir.join("PLAN.md"), "workflow-created plan").unwrap();
    std::fs::write(plan_dir.join("PLAN.html"), "workflow-created plan html").unwrap();
    let order_id = seed_order_with_workflow(&data_home, OrderStatus::Accepted, &gig_dir);

    let output = gig_command(&config_home, &data_home, &state_home)
        .args(["plan", "ready", &order_id.to_string(), "--json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "plan ready failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["status"], "plan_ready");
    assert_eq!(json["next_action"], "approve_plan");
    assert_eq!(json["order"]["id"], order_id);
    assert_eq!(
        json["paths"]["plan_md_path"],
        plan_dir.join("PLAN.md").to_string_lossy().into_owned()
    );
    assert!(!gig_dir.join("acceptance/ACCEPTANCE.md").exists());

    let conn = db::open(&data_home.join("gig/gig.db")).unwrap();
    let order = orders::find_by_id(&conn, order_id).unwrap();
    assert_eq!(order.status, OrderStatus::PlanReady);
    let workflow = order_workflow::find_by_order_id(&conn, order_id)
        .unwrap()
        .unwrap();
    assert!(workflow.plan_ready_at.is_some());
}

#[test]
fn plan_approve_json_records_approval_without_creating_acceptance_file() {
    let (config_home, data_home, state_home) = isolated_homes("plan-approve");
    let project_dir = unique_temp_dir("plan-approve-project");
    let gig_dir = project_dir.join(".gig");
    let plan_dir = gig_dir.join("plan");
    std::fs::create_dir_all(&plan_dir).unwrap();
    std::fs::write(plan_dir.join("PLAN.md"), "workflow-created plan").unwrap();
    std::fs::write(plan_dir.join("PLAN.html"), "workflow-created plan html").unwrap();
    let order_id = seed_order_with_workflow(&data_home, OrderStatus::Accepted, &gig_dir);
    let ready = gig_command(&config_home, &data_home, &state_home)
        .args(["plan", "ready", &order_id.to_string(), "--json"])
        .output()
        .unwrap();
    assert!(ready.status.success());

    let output = gig_command(&config_home, &data_home, &state_home)
        .args(["plan", "approve", &order_id.to_string(), "--json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "plan approve failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["status"], "plan_approved");
    assert_eq!(json["next_action"], "start_work");
    assert!(!gig_dir.join("acceptance/ACCEPTANCE.md").exists());

    let conn = db::open(&data_home.join("gig/gig.db")).unwrap();
    let order = orders::find_by_id(&conn, order_id).unwrap();
    assert_eq!(order.status, OrderStatus::PlanApproved);
    let workflow = order_workflow::find_by_order_id(&conn, order_id)
        .unwrap()
        .unwrap();
    assert!(workflow.plan_approved_at.is_some());
}

#[test]
fn work_start_json_records_started_timestamp_without_creating_acceptance_file() {
    let (config_home, data_home, state_home) = isolated_homes("work-start");
    let project_dir = unique_temp_dir("work-start-project");
    let gig_dir = project_dir.join(".gig");
    let plan_dir = gig_dir.join("plan");
    std::fs::create_dir_all(&plan_dir).unwrap();
    std::fs::write(plan_dir.join("PLAN.md"), "workflow-created plan").unwrap();
    std::fs::write(plan_dir.join("PLAN.html"), "workflow-created plan html").unwrap();
    let order_id = seed_order_with_workflow(&data_home, OrderStatus::Accepted, &gig_dir);
    let ready = gig_command(&config_home, &data_home, &state_home)
        .args(["plan", "ready", &order_id.to_string(), "--json"])
        .output()
        .unwrap();
    assert!(ready.status.success());
    let approved = gig_command(&config_home, &data_home, &state_home)
        .args(["plan", "approve", &order_id.to_string(), "--json"])
        .output()
        .unwrap();
    assert!(approved.status.success());

    let output = gig_command(&config_home, &data_home, &state_home)
        .args(["work", "start", &order_id.to_string(), "--json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "work start failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["status"], "in_progress");
    assert_eq!(json["next_action"], "complete_acceptance");
    assert!(!gig_dir.join("acceptance/ACCEPTANCE.md").exists());

    let conn = db::open(&data_home.join("gig/gig.db")).unwrap();
    let order = orders::find_by_id(&conn, order_id).unwrap();
    assert_eq!(order.status, OrderStatus::InProgress);
    let workflow = order_workflow::find_by_order_id(&conn, order_id)
        .unwrap()
        .unwrap();
    assert!(workflow.work_started_at.is_some());
}

#[test]
fn work_start_json_rejects_order_before_plan_approval() {
    let (config_home, data_home, state_home) = isolated_homes("work-start-before-approval");
    let project_dir = unique_temp_dir("work-start-before-approval-project");
    let gig_dir = project_dir.join(".gig");
    std::fs::create_dir_all(&gig_dir).unwrap();
    let order_id = seed_order_with_workflow(&data_home, OrderStatus::Accepted, &gig_dir);

    let output = gig_command(&config_home, &data_home, &state_home)
        .args(["work", "start", &order_id.to_string(), "--json"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "work start unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8(output.stderr).unwrap();
    let json: Value = serde_json::from_str(&stderr).unwrap();
    assert_eq!(json["status"], "error");
    assert_eq!(json["error"]["code"], "invalid_transition");

    let conn = db::open(&data_home.join("gig/gig.db")).unwrap();
    let order = orders::find_by_id(&conn, order_id).unwrap();
    assert_eq!(order.status, OrderStatus::Accepted);
    let workflow = order_workflow::find_by_order_id(&conn, order_id)
        .unwrap()
        .unwrap();
    assert_eq!(workflow.work_started_at, None);
}

#[test]
fn plan_reject_json_records_reason_and_returns_order_to_accepted_without_rewriting_plan() {
    let (config_home, data_home, state_home) = isolated_homes("plan-reject");
    let project_dir = unique_temp_dir("plan-reject-project");
    let gig_dir = project_dir.join(".gig");
    let plan_dir = gig_dir.join("plan");
    std::fs::create_dir_all(&plan_dir).unwrap();
    std::fs::write(plan_dir.join("PLAN.md"), "original plan").unwrap();
    std::fs::write(plan_dir.join("PLAN.html"), "original plan html").unwrap();
    let order_id = seed_order_with_workflow(&data_home, OrderStatus::Accepted, &gig_dir);
    let ready = gig_command(&config_home, &data_home, &state_home)
        .args(["plan", "ready", &order_id.to_string(), "--json"])
        .output()
        .unwrap();
    assert!(ready.status.success());

    let output = gig_command(&config_home, &data_home, &state_home)
        .args([
            "plan",
            "reject",
            &order_id.to_string(),
            "--reason",
            "scope missing",
            "--json",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "plan reject failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["status"], "accepted");
    assert_eq!(json["next_action"], "revise_plan");
    assert_eq!(json["workflow"]["plan_rejection_reason"], "scope missing");
    assert_eq!(
        std::fs::read_to_string(plan_dir.join("PLAN.md")).unwrap(),
        "original plan"
    );

    let conn = db::open(&data_home.join("gig/gig.db")).unwrap();
    let order = orders::find_by_id(&conn, order_id).unwrap();
    assert_eq!(order.status, OrderStatus::Accepted);
    let workflow = order_workflow::find_by_order_id(&conn, order_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        workflow.plan_rejection_reason.as_deref(),
        Some("scope missing")
    );
}

#[test]
fn acceptance_check_json_validates_existing_acceptance_file_without_transition() {
    let (config_home, data_home, state_home) = isolated_homes("acceptance-check");
    let project_dir = unique_temp_dir("acceptance-check-project");
    let gig_dir = project_dir.join(".gig");
    let acceptance_dir = gig_dir.join("acceptance");
    std::fs::create_dir_all(&acceptance_dir).unwrap();
    std::fs::write(
        acceptance_dir.join("ACCEPTANCE.md"),
        "| 验收项 | 方法 | 证据 | 结论 |\n| --- | --- | --- | --- |\n| ok | run | log | pass |",
    )
    .unwrap();
    let order_id = seed_order_with_workflow(&data_home, OrderStatus::InProgress, &gig_dir);

    let output = gig_command(&config_home, &data_home, &state_home)
        .args(["acceptance", "check", &order_id.to_string(), "--json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "acceptance check failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["status"], "in_progress");
    assert_eq!(json["next_action"], "complete_acceptance");
    assert_eq!(
        json["paths"]["acceptance_path"],
        acceptance_dir
            .join("ACCEPTANCE.md")
            .to_string_lossy()
            .into_owned()
    );

    let conn = db::open(&data_home.join("gig/gig.db")).unwrap();
    let order = orders::find_by_id(&conn, order_id).unwrap();
    assert_eq!(order.status, OrderStatus::InProgress);
}

#[test]
fn acceptance_check_json_rejects_incomplete_acceptance_item_without_transition() {
    let (config_home, data_home, state_home) = isolated_homes("acceptance-check-incomplete");
    let project_dir = unique_temp_dir("acceptance-check-incomplete-project");
    let gig_dir = project_dir.join(".gig");
    let acceptance_dir = gig_dir.join("acceptance");
    std::fs::create_dir_all(&acceptance_dir).unwrap();
    std::fs::write(
        acceptance_dir.join("ACCEPTANCE.md"),
        "| 验收项 | 方法 | 证据 | 结论 |\n| --- | --- | --- | --- |\n| crawler runs | run command |  | pass |",
    )
    .unwrap();
    let order_id = seed_order_with_workflow(&data_home, OrderStatus::InProgress, &gig_dir);

    let output = gig_command(&config_home, &data_home, &state_home)
        .args(["acceptance", "check", &order_id.to_string(), "--json"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    let json: Value = serde_json::from_str(&stderr).unwrap();
    assert_eq!(json["error"]["code"], "acceptance_incomplete");
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("acceptance item incomplete"));

    let conn = db::open(&data_home.join("gig/gig.db")).unwrap();
    let order = orders::find_by_id(&conn, order_id).unwrap();
    assert_eq!(order.status, OrderStatus::InProgress);
}

#[test]
fn acceptance_check_json_rejects_blocked_acceptance_conclusion_without_transition() {
    let (config_home, data_home, state_home) = isolated_homes("acceptance-blocked-conclusion");
    let project_dir = unique_temp_dir("acceptance-blocked-conclusion-project");
    let gig_dir = project_dir.join(".gig");
    let acceptance_dir = gig_dir.join("acceptance");
    std::fs::create_dir_all(&acceptance_dir).unwrap();
    std::fs::write(
        acceptance_dir.join("ACCEPTANCE.md"),
        "| 验收项 | 方法 | 证据 | 结论 |
| --- | --- | --- | --- |
| crawler runs | run command | log | blocked |",
    )
    .unwrap();
    let order_id = seed_order_with_workflow(&data_home, OrderStatus::InProgress, &gig_dir);

    let output = gig_command(&config_home, &data_home, &state_home)
        .args(["acceptance", "check", &order_id.to_string(), "--json"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    let json: Value = serde_json::from_str(&stderr).unwrap();
    assert_eq!(json["error"]["code"], "acceptance_incomplete");
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("conclusion must be pass-like"));

    let conn = db::open(&data_home.join("gig/gig.db")).unwrap();
    let order = orders::find_by_id(&conn, order_id).unwrap();
    assert_eq!(order.status, OrderStatus::InProgress);
}

#[test]
fn acceptance_complete_json_sets_ready_to_deliver_without_creating_delivery_files() {
    let (config_home, data_home, state_home) = isolated_homes("acceptance-complete");
    let project_dir = unique_temp_dir("acceptance-complete-project");
    let gig_dir = project_dir.join(".gig");
    let acceptance_dir = gig_dir.join("acceptance");
    std::fs::create_dir_all(&acceptance_dir).unwrap();
    std::fs::write(
        acceptance_dir.join("ACCEPTANCE.md"),
        "| 验收项 | 方法 | 证据 | 结论 |\n| --- | --- | --- | --- |\n| ok | run | log | pass |",
    )
    .unwrap();
    let order_id = seed_order_with_workflow(&data_home, OrderStatus::InProgress, &gig_dir);

    let output = gig_command(&config_home, &data_home, &state_home)
        .args(["acceptance", "complete", &order_id.to_string(), "--json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "acceptance complete failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["status"], "ready_to_deliver");
    assert_eq!(json["next_action"], "check_package");
    assert!(!gig_dir.join("delivery").exists());

    let conn = db::open(&data_home.join("gig/gig.db")).unwrap();
    let order = orders::find_by_id(&conn, order_id).unwrap();
    assert_eq!(order.status, OrderStatus::ReadyToDeliver);
    let workflow = order_workflow::find_by_order_id(&conn, order_id)
        .unwrap()
        .unwrap();
    assert!(workflow.acceptance_completed_at.is_some());
}

fn write_client_manifest<const N: usize>(delivery_dir: &Path, files: [&str; N]) {
    let items = files
        .iter()
        .map(|file| format!("\"{file}\""))
        .collect::<Vec<_>>()
        .join(", ");
    std::fs::write(
        delivery_dir.join("manifest.toml"),
        format!("version = 1\ndelivery_date = \"2026-05-27\"\nclient_files = [{items}]\n"),
    )
    .unwrap();
}

fn write_zip<const N: usize>(path: &Path, entries: [(&str, &str); N]) {
    let file = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, contents) in entries {
        zip.start_file(name, options).unwrap();
        zip.write_all(contents.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

fn prepare_client_delivery(gig_dir: &Path, with_zip: bool) -> PathBuf {
    let delivery_dir = gig_dir.join("delivery/2026-05-27");
    let client_dir = delivery_dir.join("client");
    let internal_dir = delivery_dir.join("internal");
    std::fs::create_dir_all(&client_dir).unwrap();
    std::fs::create_dir_all(&internal_dir).unwrap();
    std::fs::write(delivery_dir.join("DELIVERY.md"), "delivery source").unwrap();
    std::fs::write(
        internal_dir.join("DELIVERY_INTERNAL.html"),
        "internal delivery html",
    )
    .unwrap();
    std::fs::write(client_dir.join("DELIVERY_CLIENT.html"), "client html").unwrap();
    std::fs::write(client_dir.join("DELIVERY_CLIENT.pdf"), "client pdf").unwrap();
    write_client_manifest(
        &delivery_dir,
        ["DELIVERY_CLIENT.html", "DELIVERY_CLIENT.pdf"],
    );
    if with_zip {
        let export_dir = delivery_dir.join("export");
        std::fs::create_dir_all(&export_dir).unwrap();
        write_zip(
            &export_dir.join("client-package.zip"),
            [
                ("DELIVERY_CLIENT.html", "client html"),
                ("DELIVERY_CLIENT.pdf", "client pdf"),
            ],
        );
    }
    delivery_dir
}
