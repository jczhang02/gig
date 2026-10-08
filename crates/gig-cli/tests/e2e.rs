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
fn config_set_edits_the_file_in_place() {
    let env = Env::new();
    let cfg = env.root.path().join("config/config.toml");
    let before = format!("# hand-written\n{}", fs::read_to_string(&cfg).unwrap())
        .replace("warranty_days = 15", "warranty_days = 15 # days");
    fs::write(&cfg, &before).unwrap();
    let v = env.ok(&["config", "set", "general.warranty_days", "30"]);
    assert_eq!(
        v,
        serde_json::json!({ "key": "general.warranty_days", "value": 30 })
    );
    assert_eq!(
        fs::read_to_string(&cfg).unwrap(),
        before.replace("= 15 #", "= 30 #")
    );
    let v = env.ok(&["config", "set", "tui.mouse", "false"]);
    assert_eq!(v, serde_json::json!({ "key": "tui.mouse", "value": false }));
    assert!(fs::read_to_string(&cfg)
        .unwrap()
        .ends_with("\n[tui]\nmouse = false\n"));
    // Schema ranges apply to the CLI too; a refusal writes nothing.
    let after = fs::read_to_string(&cfg).unwrap();
    assert_eq!(
        env.err(&["config", "set", "tui.refresh_seconds", "90"]),
        "invalid_input"
    );
    assert_eq!(fs::read_to_string(&cfg).unwrap(), after);
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
fn split_secrets_works_on_a_v1_config() {
    let env = Env::new();
    let cfg = env.root.path().join("config/config.toml");
    let v1 = format!(
        "[general]\ndev_root = {:?}\n[delivery]\ndefault_uploader = \"s3:bj\"\n[delivery.s3.bj]\nbucket = \"b\"\nregion = \"r\"\nendpoint = \"https://x\"\naccess_key = \"AKDUMMY\"\nsecret_key = \"SKDUMMY\"\n[delivery.short_link]\nenabled = true\nendpoint = \"https://go.example/api\"\ntoken = \"TOKDUMMY\"\n",
        env.dev()
    );
    fs::write(&cfg, v1).unwrap();
    assert_eq!(env.err(&["config", "get", "general.dev_root"]), "secrets");
    assert_eq!(env.err(&["migrate", "--dry-run"]), "secrets");
    let dry = env.ok(&["config", "split-secrets"]);
    assert_eq!(dry["dry_run"], true);
    assert_eq!(dry["moved"].as_array().unwrap().len(), 3);
    let done = env.ok(&["config", "split-secrets", "--yes"]);
    assert_eq!(done["dry_run"], false);
    let text = fs::read_to_string(&cfg).unwrap();
    assert!(!text.contains("AKDUMMY") && !text.contains("TOKDUMMY"));
    assert!(env.root.path().join("config/config.toml.v1").is_file());
    let secrets = env.root.path().join("config/secrets.toml");
    assert!(fs::read_to_string(&secrets).unwrap().contains("SKDUMMY"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&secrets).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    assert_eq!(
        env.ok(&["config", "get", "delivery.uploader"])["value"],
        "s3:bj"
    );
    let (ok, v) = env.run(&["doctor"]);
    assert!(ok, "{v}");
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

#[test]
fn list_themes_needs_no_database_and_shows_user_files() {
    let root = tempfile::tempdir().unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_gig"))
            .arg("--list-themes")
            .env("GIG_HOME", root.path())
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    };
    let out = run();
    assert!(out.status.success());
    let names = String::from_utf8(out.stdout).unwrap();
    assert_eq!(
        names.lines().collect::<Vec<_>>(),
        [
            "gig-dark",
            "gig-light",
            "catppuccin-mocha",
            "catppuccin-latte",
            "tokyonight",
            "gruvbox-dark",
            "nord",
            "dracula"
        ]
    );
    assert!(!root.path().join("data").exists(), "no database created");

    let themes = root.path().join("config/themes");
    fs::create_dir_all(&themes).unwrap();
    fs::write(themes.join("broken.toml"), "bg = \"#000000\"\n").unwrap();
    fs::write(themes.join("Bad_Name.toml"), "").unwrap();
    let out = run();
    assert!(out.status.success());
    assert_eq!(String::from_utf8(out.stdout).unwrap().lines().count(), 8);
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.starts_with("gig: theme "), "{err}");
    assert!(
        err.contains("broken.toml: missing key \"surface\""),
        "{err}"
    );
    assert!(
        err.contains("Bad_Name.toml: file name must be lowercase letters, digits and -"),
        "{err}"
    );

    let out = Command::new(env!("CARGO_BIN_EXE_gig"))
        .args(["tui", "--list-themes"])
        .env("GIG_HOME", root.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8(out.stdout)
        .unwrap()
        .starts_with("gig-dark\n"));
}

#[test]
fn dashboard_errors_name_the_command_as_typed() {
    let root = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_gig"))
            .args(args)
            .env("GIG_HOME", root.path())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .output()
            .unwrap();
        (out.status.code(), String::from_utf8(out.stderr).unwrap())
    };
    // No terminal: `gig tui` fails to enter raw mode.
    let (code, err) = run(&["tui"]);
    assert_eq!(code, Some(1));
    assert!(err.starts_with("gig tui: "), "{err}");
}

#[test]
fn bare_gig_without_a_terminal_still_requires_a_command() {
    let root = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_gig"))
        .env("GIG_HOME", root.path())
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("Usage: gig <COMMAND>"), "{err}");
    assert!(!err.contains("--theme"), "{err}");
}

/// An in-progress order `up` with a checked package `up-v1`, the default
/// uploader `s3:hk` (no keys), short links enabled (no token), and
/// `delivery.bdpan.bin` pointing at a fake bdpan.
#[cfg(unix)]
fn order_with_fake_bdpan(
    env: &Env,
) -> (
    tempfile::TempDir,
    gig_core::delivery::bdpan::fake::FakeBdpan,
) {
    let bin_dir = tempfile::tempdir().unwrap();
    let bdpan = gig_core::delivery::bdpan::fake::FakeBdpan::install(bin_dir.path()).unwrap();
    let cfg = env.root.path().join("config/config.toml");
    let mut text = fs::read_to_string(&cfg).unwrap();
    text.push_str(&format!(
        "[delivery]\nuploader = \"s3:hk\"\n[delivery.s3.hk]\nbucket = \"b\"\nregion = \"r\"\nendpoint = \"https://s3.example.test\"\n[delivery.short_link]\nenabled = true\nendpoint = \"https://go.example.test/api\"\n[delivery.bdpan]\nbin = {:?}\n",
        bdpan.bin()
    ));
    fs::write(&cfg, text).unwrap();
    env.ok(&["new", "up", "--title", "Up"]);
    env.ok(&["start", "up"]);
    let pkg = env.dev().join("up/delivery/up-v1");
    fs::create_dir_all(&pkg).unwrap();
    fs::write(pkg.join("manual.pdf"), "pdf").unwrap();
    env.ok(&[
        "package",
        "build",
        "up-v1",
        "--order",
        "up",
        "--write-manifest",
    ]);
    (bin_dir, bdpan)
}

#[cfg(unix)]
#[test]
fn package_upload_picks_the_uploader() {
    use gig_core::delivery::bdpan::fake;
    let env = Env::new();
    let (_bin_dir, bdpan) = order_with_fake_bdpan(&env);
    let up = ["package", "upload", "up-v1", "--order", "up"];
    let with = |extra: &[&'static str]| [&up[..], extra].concat();

    // Dry runs name the uploader and run nothing.
    assert_eq!(env.ok(&up)["uploader"], "s3:hk");
    let dry = env.ok(&with(&["--uploader", "bdpan"]));
    assert_eq!(dry["uploader"], "bdpan");
    assert_eq!(dry["dry_run"], true);
    assert!(bdpan.calls().is_empty());

    // Unknown names are config errors; the S3 default still needs its keys.
    assert_eq!(env.err(&with(&["--uploader", "nope", "--yes"])), "config");
    assert_eq!(env.err(&with(&["--yes"])), "secrets");

    // bdpan: channel pan, no short link even with short links enabled.
    let sent = env.ok(&with(&["--uploader", "bdpan", "--yes"]));
    assert_eq!(sent["package"]["status"], "sent");
    assert_eq!(sent["package"]["channel"], "pan");
    assert_eq!(sent["package"]["uploader"], "bdpan");
    assert_eq!(sent["package"]["short_url"], Value::Null);
    assert_eq!(sent["short_url"], Value::Null);
    assert_eq!(sent["uploader"], "bdpan");
    assert_eq!(sent["pwd"], fake::PWD);
    assert_eq!(
        sent["url"].as_str().unwrap(),
        format!("{}?pwd={}", fake::LINK, fake::PWD)
    );
    assert_eq!(sent["order_status"], "delivered");
    assert!(bdpan
        .calls()
        .iter()
        .any(|c| c.contains("upload") && c.contains("gig/up/up-v1/")));
}

#[cfg(unix)]
#[test]
fn artifact_upload_picks_the_uploader() {
    use gig_core::delivery::bdpan::fake;
    let env = Env::new();
    let (_bin_dir, bdpan) = order_with_fake_bdpan(&env);
    let file = env.root.path().join("demo.mp4");
    fs::write(&file, "video").unwrap();
    let file = file.to_string_lossy().into_owned();
    let up = ["artifact", "upload", "--order", "up", file.as_str()];
    let with = |extra: &[&'static str]| [&up[..], extra].concat();

    assert_eq!(env.ok(&up)["uploader"], "s3:hk");
    assert_eq!(env.ok(&with(&["--uploader", "bdpan"]))["uploader"], "bdpan");
    assert!(bdpan.calls().is_empty());
    assert_eq!(env.err(&with(&["--uploader", "nope", "--yes"])), "config");
    assert_eq!(env.err(&with(&["--yes"])), "secrets");

    let d = env.ok(&with(&["--uploader", "bdpan", "--yes"]));
    assert_eq!(d["artifact"]["uploader"], "bdpan");
    assert_eq!(d["artifact"]["short_url"], Value::Null);
    assert_eq!(d["short_url"], Value::Null);
    assert_eq!(d["uploader"], "bdpan");
    assert_eq!(d["pwd"], fake::PWD);
    assert!(bdpan
        .calls()
        .iter()
        .any(|c| c.contains("upload") && c.contains("gig/up/artifacts/")));
}

/// Dry runs check the uploader name as a real run does, and build or run
/// nothing: an unknown name, or an S3 name without its table, is `config`.
#[cfg(unix)]
#[test]
fn a_dry_run_refuses_an_unknown_uploader() {
    let env = Env::new();
    let (_bin_dir, bdpan) = order_with_fake_bdpan(&env);
    let file = env.root.path().join("demo.mp4");
    fs::write(&file, "video").unwrap();
    let file = file.to_string_lossy().into_owned();
    let package = ["package", "upload", "up-v1", "--order", "up"];
    let artifact = ["artifact", "upload", "--order", "up", file.as_str()];
    for up in [&package[..], &artifact[..]] {
        for bad in ["nope", "s3:missing", "bdpan:work"] {
            let (ok, v) = env.run(&[up, &["--uploader", bad]].concat());
            assert!(!ok, "{bad}: {v}");
            assert_eq!(v["error"]["code"], "config", "{bad}: {v}");
            assert!(
                v["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains(bad.trim_start_matches("s3:")),
                "{v}"
            );
        }
    }
    // The configured default is checked the same way.
    let cfg = env.root.path().join("config/config.toml");
    let text = fs::read_to_string(&cfg)
        .unwrap()
        .replace("uploader = \"s3:hk\"", "uploader = \"nope\"");
    fs::write(&cfg, text).unwrap();
    assert_eq!(env.err(&package), "config");
    assert_eq!(env.err(&artifact), "config");
    assert!(bdpan.calls().is_empty(), "{:?}", bdpan.calls());
}

#[test]
fn config_set_accepts_the_bdpan_uploader() {
    let env = Env::new();
    let v = env.ok(&["config", "set", "delivery.uploader", "bdpan"]);
    assert_eq!(v["value"], "bdpan");
    assert_eq!(
        env.ok(&["config", "get", "delivery.uploader"])["value"],
        "bdpan"
    );
}

#[cfg(unix)]
mod doctor_bdpan {
    use super::Env;
    use gig_core::delivery::bdpan::fake::{self, FakeBdpan, Reply};
    use serde_json::Value;
    use std::fs;
    use std::path::Path;

    /// Make `bin` the configured bdpan and the default uploader.
    fn use_bdpan(env: &Env, bin: &Path) {
        let cfg = env.root.path().join("config/config.toml");
        let mut text = fs::read_to_string(&cfg).unwrap();
        text.push_str(&format!(
            "[delivery]\nuploader = \"bdpan\"\n[delivery.bdpan]\nbin = {bin:?}\n"
        ));
        fs::write(&cfg, text).unwrap();
    }

    /// `scope: message` for each entry of `data.<key>` in a doctor report.
    fn entries(report: &Value, key: &str) -> Vec<String> {
        report[key]
            .as_array()
            .unwrap_or_else(|| panic!("no {key} in {report}"))
            .iter()
            .map(|p| {
                format!(
                    "{}: {}",
                    p["scope"].as_str().unwrap(),
                    p["message"].as_str().unwrap()
                )
            })
            .collect()
    }

    #[test]
    fn a_logged_in_bdpan_is_healthy_and_doctor_only_asks_whoami() {
        let env = Env::new();
        let fake = FakeBdpan::install(env.root.path()).unwrap();
        use_bdpan(&env, &fake.bin());

        let r = env.ok(&["doctor"]);

        assert!(entries(&r, "problems").is_empty(), "{r}");
        assert!(entries(&r, "warnings").is_empty(), "{r}");
        assert_eq!(fake.calls(), vec!["--json --no-check-update whoami"]);
    }

    #[test]
    fn a_missing_bdpan_binary_is_a_problem() {
        let env = Env::new();
        use_bdpan(&env, &env.root.path().join("no-such-bdpan"));

        let r = env.ok(&["doctor"]);

        let problems = entries(&r, "problems");
        assert_eq!(problems.len(), 1, "{r}");
        assert!(problems[0].contains("not found"), "{r}");
        assert!(problems[0].contains("delivery.bdpan.bin"), "{r}");
    }

    #[test]
    fn a_bdpan_binary_that_is_not_executable_is_a_problem() {
        use std::os::unix::fs::PermissionsExt;
        let env = Env::new();
        let fake = FakeBdpan::install(env.root.path()).unwrap();
        fs::set_permissions(fake.bin(), fs::Permissions::from_mode(0o644)).unwrap();
        use_bdpan(&env, &fake.bin());

        let r = env.ok(&["doctor"]);

        let problems = entries(&r, "problems");
        assert_eq!(problems.len(), 1, "{r}");
        assert!(problems[0].contains("not executable"), "{r}");
        assert!(problems[0].contains("delivery.bdpan.bin"), "{r}");
    }

    #[test]
    fn a_logged_out_or_expired_bdpan_is_a_problem_that_says_bdpan_login() {
        for reply in [
            Reply::whoami(false, false, "2099-01-01T00:00:00Z"),
            Reply::whoami(true, false, "2099-01-01T00:00:00Z"),
        ] {
            let env = Env::new();
            let fake = FakeBdpan::install(env.root.path()).unwrap();
            fake.reply("whoami", reply).unwrap();
            use_bdpan(&env, &fake.bin());

            let r = env.ok(&["doctor"]);

            let problems = entries(&r, "problems");
            assert_eq!(problems.len(), 1, "{r}");
            assert!(problems[0].contains("! bdpan login"), "{r}");
        }
    }

    #[test]
    fn a_whoami_that_fails_is_a_problem_in_bdpan_s_words() {
        let env = Env::new();
        let fake = FakeBdpan::install(env.root.path()).unwrap();
        fake.reply("whoami", Reply::fail(1, "Error: 网络连接失败\nUsage:\n"))
            .unwrap();
        use_bdpan(&env, &fake.bin());

        let r = env.ok(&["doctor"]);

        let problems = entries(&r, "problems");
        assert_eq!(problems.len(), 1, "{r}");
        assert!(
            problems[0].starts_with("bdpan: bdpan whoami failed: 网络连接失败"),
            "{r}"
        );
        assert!(!problems[0].contains("upload failed"), "{r}");
    }

    #[test]
    fn a_token_that_expires_within_7_days_is_a_warning_with_the_date() {
        let env = Env::new();
        let fake = FakeBdpan::install(env.root.path()).unwrap();
        let expires_at = fake::days_from_now(3);
        fake.reply("whoami", Reply::whoami(true, true, &expires_at))
            .unwrap();
        use_bdpan(&env, &fake.bin());

        let (ok, v) = env.run(&["doctor"]);

        assert!(ok, "{v}");
        assert!(entries(&v["data"], "problems").is_empty(), "{v}");
        let warnings = entries(&v["data"], "warnings");
        assert_eq!(warnings.len(), 1, "{v}");
        assert!(warnings[0].contains(&expires_at[..10]), "{v}");
        assert!(warnings[0].contains("! bdpan login"), "{v}");
        assert!(v["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w.as_str().unwrap().contains(&expires_at[..10])));
    }

    #[test]
    fn a_token_valid_for_8_days_or_more_is_fine() {
        let env = Env::new();
        let fake = FakeBdpan::install(env.root.path()).unwrap();
        fake.reply("whoami", Reply::whoami(true, true, &fake::days_from_now(8)))
            .unwrap();
        use_bdpan(&env, &fake.bin());

        let r = env.ok(&["doctor"]);

        assert!(entries(&r, "problems").is_empty(), "{r}");
        assert!(entries(&r, "warnings").is_empty(), "{r}");
    }
}
