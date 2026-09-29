//! End-to-end: run the `gig` binary in a temporary GIG_HOME through a whole order.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Env {
    root: tempfile::TempDir,
}

impl Env {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let templates = root.path().join("templates");
        fs::create_dir_all(&templates).unwrap();
        fs::write(
            templates.join("NOTES.md.j2"),
            "# {{ slug }}\n\n{{ material_path }}\n",
        )
        .unwrap();
        fs::write(
            templates.join("JOB.md.j2"),
            "# {{ title }}\n\n- slug: {{ slug }}\n- price: {{ currency }} {{ price }}\n{% if draft_notes %}\n## Notes\n\n{{ draft_notes }}{% endif %}\n",
        )
        .unwrap();
        fs::write(
            templates.join("QUOTE.md.j2"),
            "# Quote\n\n{{ currency }} {{ price }} on {{ today }}\n\n> {{ client_words }}\n",
        )
        .unwrap();
        fs::write(templates.join("AGENTS.md.j2"), "# Agents\n").unwrap();
        fs::write(templates.join("AGENTS.tool.md.j2"), "# Agents for tools\n").unwrap();
        fs::write(templates.join("README.md.j2"), "# {{ title }}\n").unwrap();
        fs::write(templates.join("gitignore"), "delivery/\n.venv/\n").unwrap();
        fs::create_dir_all(root.path().join("dev")).unwrap();
        fs::create_dir_all(root.path().join("archive")).unwrap();
        let cfg = format!(
            "[general]\ndev_root = {:?}\narchive_root = {:?}\ntemplates_dir = {:?}\nwarranty_days = 15\n",
            root.path().join("dev"),
            root.path().join("archive"),
            templates
        );
        fs::create_dir_all(root.path().join("config")).unwrap();
        fs::write(root.path().join("config/config.toml"), cfg).unwrap();
        Self { root }
    }

    fn dev(&self) -> PathBuf {
        self.root.path().join("dev")
    }

    fn run_in(&self, cwd: &Path, args: &[&str]) -> (bool, Value) {
        let out = Command::new(env!("CARGO_BIN_EXE_gig"))
            .args(args)
            .env("GIG_HOME", self.root.path())
            .current_dir(cwd)
            .output()
            .unwrap();
        let text = String::from_utf8(out.stdout).unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        let v: Value = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("not JSON ({e}) for {args:?}: {text}\nstderr: {stderr}"));
        assert_eq!(v["ok"].as_bool().unwrap(), out.status.success(), "{text}");
        (out.status.success(), v)
    }

    fn run(&self, args: &[&str]) -> (bool, Value) {
        self.run_in(self.root.path(), args)
    }

    fn ok(&self, args: &[&str]) -> Value {
        let (ok, v) = self.run(args);
        assert!(ok, "{args:?}: {v}");
        v["data"].clone()
    }

    fn err(&self, args: &[&str]) -> String {
        let (ok, v) = self.run(args);
        assert!(!ok, "{args:?} should fail: {v}");
        v["error"]["code"].as_str().unwrap().to_string()
    }
}

#[test]
fn full_lifecycle_with_phone_delivery() {
    let env = Env::new();

    // draft
    let d = env.ok(&[
        "draft",
        "new",
        "pdf-tool",
        "--title",
        "PDF tool",
        "--material",
        "/mnt/1",
    ]);
    let notes = PathBuf::from(d["notes_path"].as_str().unwrap());
    assert!(notes.is_file());
    fs::write(&notes, "budget 800\n").unwrap();
    assert_eq!(env.ok(&["draft", "ls"]).as_array().unwrap().len(), 1);

    // new from draft
    let created = env.ok(&[
        "new",
        "pdf-tool",
        "--title",
        "PDF tool",
        "--price",
        "800",
        "--type",
        "tool",
        "--from-draft",
        "--client-words",
        "报价 800, 成交",
    ]);
    assert_eq!(created["order"]["status"], "queued");
    assert_eq!(created["order"]["price"], "800.00");
    assert_eq!(created["order"]["material_path"], "/mnt/1");
    let dev = env.dev().join("pdf-tool");
    assert!(fs::read_to_string(dev.join(".gig/JOB.md"))
        .unwrap()
        .contains("budget 800"));
    assert!(fs::read_to_string(dev.join(".gig/QUOTE.md"))
        .unwrap()
        .contains("报价 800"));
    assert_eq!(
        fs::read_to_string(dev.join("AGENTS.md")).unwrap(),
        "# Agents for tools\n"
    );
    assert!(!notes.exists());
    assert!(env.ok(&["draft", "ls"]).as_array().unwrap().is_empty());
    assert_eq!(
        env.err(&["new", "pdf-tool", "--title", "again"]),
        "invalid_input"
    );

    // resolve from cwd
    let (ok, shown) = env.run_in(&dev, &["show"]);
    assert!(ok);
    assert_eq!(shown["data"]["order"]["slug"], "pdf-tool");
    assert_eq!(shown["data"]["next_action"], "start");
    assert_eq!(
        env.ok(&["cd", "pdf-tool"])["path"].as_str().unwrap(),
        dev.to_string_lossy()
    );

    // start, note, change
    assert_eq!(env.ok(&["start", "pdf-tool"])["status"], "in_progress");
    env.ok(&["note", "--order", "pdf-tool", "client wants csv"]);
    let changed = env.ok(&[
        "change",
        "pdf-tool",
        "--desc",
        "csv export",
        "--price-delta",
        "200",
    ]);
    assert_eq!(changed["order"]["price"], "1000.00");

    // package
    let pkg = dev.join("delivery/pdf-tool-v1.0.0");
    fs::create_dir_all(pkg.join("program")).unwrap();
    fs::write(pkg.join("manual.pdf"), "pdf").unwrap();
    fs::write(pkg.join("program/tool.exe"), "exe").unwrap();
    assert_eq!(
        env.err(&[
            "package",
            "upload",
            "pdf-tool-v1.0.0",
            "--order",
            "pdf-tool",
            "--yes"
        ]),
        "needs_check"
    );
    let built = env.ok(&[
        "package",
        "build",
        "pdf-tool-v1.0.0",
        "--order",
        "pdf-tool",
        "--write-manifest",
    ]);
    assert_eq!(built["package"]["status"], "checked");
    assert_eq!(built["files"].as_array().unwrap().len(), 2);
    assert!(dev.join("delivery/pdf-tool-v1.0.0.zip").is_file());
    assert!(dev.join("delivery/pdf-tool-v1.0.0.manifest.toml").is_file());

    // planted secret is rejected and counted
    fs::write(pkg.join("id_rsa"), "k").unwrap();
    assert_eq!(
        env.err(&["package", "check", "pdf-tool-v1.0.0", "--order", "pdf-tool"]),
        "unsafe_package"
    );
    fs::remove_file(pkg.join("id_rsa")).unwrap();
    env.ok(&["package", "check", "pdf-tool-v1.0.0", "--order", "pdf-tool"]);

    // sent by phone: dry run first
    let dry = env.ok(&[
        "package",
        "sent",
        "pdf-tool-v1.0.0",
        "--order",
        "pdf-tool",
        "--channel",
        "phone",
    ]);
    assert_eq!(dry["dry_run"], true);
    assert_eq!(dry["order_status"], "in_progress");
    let sent = env.ok(&[
        "package",
        "sent",
        "pdf-tool-v1.0.0",
        "--order",
        "pdf-tool",
        "--channel",
        "phone",
        "--yes",
    ]);
    assert_eq!(sent["package"]["status"], "sent");
    assert_eq!(sent["order_status"], "delivered");

    // ls shows unpaid
    let ls = env.ok(&["ls"]);
    let row = &ls["orders"][0];
    assert_eq!(row["unpaid"], true);
    assert_eq!(row["next_action"], "collect payment");

    // paid, scorecard, archive
    let paid = env.ok(&["paid", "pdf-tool", "--date", "2026-01-01"]);
    assert_eq!(paid["warranty_until"], "2026-01-16");
    let sc = env.ok(&[
        "scorecard",
        "pdf-tool",
        "--decisions",
        "2",
        "--repeat-questions",
        "0",
        "--cleanups",
        "0",
        "--report-reworks",
        "0",
        "--score",
        "5",
    ]);
    assert_eq!(sc["check_rejections"], 1);
    let preview = env.ok(&["archive", "pdf-tool"]);
    assert_eq!(preview["dry_run"], true);
    assert!(
        preview["blockers"].as_array().unwrap().is_empty(),
        "{preview}"
    );
    let archived = env.ok(&["archive", "pdf-tool", "--yes"]);
    assert_eq!(archived["order"]["status"], "archived");
    assert!(env
        .root
        .path()
        .join("archive/pdf-tool/.gig/JOB.md")
        .is_file());
    assert!(!dev.exists());
    assert!(env.ok(&["ls"])["orders"].as_array().unwrap().is_empty());
    assert_eq!(
        env.ok(&["ls", "--all"])["orders"].as_array().unwrap().len(),
        1
    );
}

#[test]
fn yes_gates_and_error_envelope() {
    let env = Env::new();
    env.ok(&["draft", "new", "x"]);
    let dry = env.ok(&["draft", "drop", "x", "--reason", "no"]);
    assert_eq!(dry["dry_run"], true);
    assert_eq!(env.ok(&["draft", "ls"]).as_array().unwrap().len(), 1);
    env.ok(&["draft", "drop", "x", "--reason", "no", "--yes"]);
    assert_eq!(env.err(&["show", "nope"]), "not_found");
    assert_eq!(
        env.err(&["new", "Bad Slug", "--title", "t"]),
        "invalid_input"
    );
    assert_eq!(
        env.err(&["new", "p", "--title", "t", "--price", "8.123"]),
        "invalid_input"
    );
    env.ok(&["new", "p", "--title", "t"]);
    assert_eq!(env.err(&["paid", "p"]), "invalid_state");
    let c = env.ok(&["cancel", "p", "--reason", "gone"]);
    assert_eq!(c["dry_run"], true);
    assert_eq!(
        env.ok(&["cancel", "p", "--reason", "gone", "--yes"])["order"]["status"],
        "cancelled"
    );
    let (ok, v) = env.run(&["show", "p"]);
    assert!(ok);
    assert_eq!(v["command"], "show");
    assert!(v["warnings"].as_array().unwrap().is_empty());
}

#[test]
fn adopt_registers_without_touching_files() {
    let env = Env::new();
    let dev = env.dev().join("existing");
    fs::create_dir_all(dev.join(".gig")).unwrap();
    fs::write(dev.join(".gig/JOB.md"), "keep").unwrap();
    assert_eq!(
        env.err(&["new", "existing", "--title", "E"]),
        "invalid_input"
    );
    let (ok, v) = env.run(&[
        "new",
        "existing",
        "--title",
        "E",
        "--price",
        "800",
        "--adopt",
        "--status",
        "in_progress",
    ]);
    assert!(ok, "{v}");
    assert_eq!(v["data"]["order"]["status"], "in_progress");
    assert!(v["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w.as_str().unwrap().contains("QUOTE.md")));
    assert_eq!(fs::read_to_string(dev.join(".gig/JOB.md")).unwrap(), "keep");
    assert!(!dev.join("README.md").exists());
}

#[test]
fn config_and_doctor() {
    let env = Env::new();
    let p = env.ok(&["config", "path"]);
    assert!(p["db_file"].as_str().unwrap().ends_with("gig-v2.db"));
    env.ok(&["config", "set", "general.warranty_days", "20"]);
    assert_eq!(
        env.ok(&["config", "get", "general.warranty_days"])["value"],
        20
    );
    assert_eq!(
        env.err(&["config", "set", "delivery.s3.x.access_key", "k"]),
        "secrets"
    );
    let (ok, v) = env.run(&["doctor"]);
    assert!(ok);
    assert!(v["data"]["problems"].as_array().unwrap().is_empty(), "{v}");
    env.ok(&["new", "d", "--title", "D"]);
    fs::remove_dir_all(env.dev().join("d")).unwrap();
    let (_, v) = env.run(&["doctor"]);
    assert!(v["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w.as_str().unwrap().contains("missing")));
}

#[test]
fn migrate_dry_run_reports_without_writing() {
    let env = Env::new();
    let old = env.root.path().join("data/gig.db");
    fs::create_dir_all(old.parent().unwrap()).unwrap();
    let conn = rusqlite_open(&old);
    conn.execute_batch(include_str!("../../gig-core/tests/fixtures/v1/schema.sql"))
        .unwrap();
    conn.execute_batch(
        "INSERT INTO orders (id, slug, title, status, quoted_price, final_price, my_cut_ratio, currency, created_at, paid_at) VALUES (7, 'legacy', 'Legacy', 'paid', 50000, 50000, 0.6, 'CNY', 1775606400, 1776256502);",
    )
    .unwrap();
    drop(conn);
    let r = env.ok(&["migrate", "--dry-run"]);
    assert_eq!(r["orders"][0]["slug"], "legacy");
    assert_eq!(r["orders"][0]["warranty_until"], "2026-04-30");
    assert!(!env.root.path().join("data/gig-v2.db").exists());
    let r = env.ok(&["migrate"]);
    assert_eq!(r["counts"]["v2.orders"], 1);
    assert_eq!(env.ok(&["show", "legacy"])["order"]["legacy_id"], 7);
    assert_eq!(env.err(&["migrate"]), "invalid_input");
}

fn rusqlite_open(p: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open(p).unwrap()
}
