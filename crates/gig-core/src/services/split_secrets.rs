//! Move v1 secrets out of config.toml into secrets.toml and rewrite config in v2 layout.

use crate::config::{Config, Paths};
use crate::services::require_yes;
use crate::{secrets, Error, Result};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct SplitReport {
    pub moved: Vec<String>,
    pub dropped: Vec<String>,
    pub config_backup: Option<String>,
    pub dry_run: bool,
}

pub fn split(paths: &Paths, yes: bool) -> Result<SplitReport> {
    let text = std::fs::read_to_string(&paths.config_file)
        .map_err(|e| Error::PathUnavailable(paths.config_file.clone(), e))?;
    let old: toml::Value = toml::from_str(&text)?;
    let (config, secrets_text, moved, dropped) = convert(&old)?;
    if !yes {
        return Ok(SplitReport {
            moved,
            dropped,
            config_backup: None,
            dry_run: true,
        });
    }
    require_yes(yes, "rewrite config")?;
    let backup = paths.config_file.with_extension("toml.v1");
    std::fs::rename(&paths.config_file, &backup)
        .map_err(|e| Error::PathUnavailable(backup.clone(), e))?;
    private(&backup)?;
    if !secrets_text.is_empty() {
        secrets::write_file(&paths.secrets_file, &secrets_text)?;
    }
    config.save(&paths.config_file)?;
    Ok(SplitReport {
        moved,
        dropped,
        config_backup: Some(backup.to_string_lossy().into_owned()),
        dry_run: false,
    })
}

#[cfg(unix)]
fn private(p: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn private(_p: &Path) -> Result<()> {
    Ok(())
}

fn get_str(v: &toml::Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

/// v1 -> (v2 Config, secrets.toml text, moved field names, dropped field names).
pub fn convert(old: &toml::Value) -> Result<(Config, String, Vec<String>, Vec<String>)> {
    let mut cfg = Config::default();
    let mut moved = Vec::new();
    let mut dropped = Vec::new();
    let mut secret_lines = String::new();

    if let Some(g) = old.get("general") {
        if let Some(v) = get_str(g, "dev_root") {
            cfg.general.dev_root = v.into();
        }
        if let Some(v) = get_str(g, "archive_root") {
            cfg.general.archive_root = v.into();
        }
        if let Some(v) = g.get("default_cut_ratio").and_then(|x| x.as_float()) {
            cfg.general.default_cut_ratio = v;
        }
        if let Some(v) = get_str(g, "default_currency") {
            cfg.general.default_currency = v;
        }
        for k in ["drafts_dir", "templates_dir"] {
            if let Some(v) = get_str(g, k) {
                if k == "drafts_dir" {
                    cfg.general.drafts_dir = v.into();
                } else {
                    cfg.general.templates_dir = v.into();
                }
            }
        }
        if let Some(v) = g.get("warranty_days").and_then(|x| x.as_integer()) {
            cfg.general.warranty_days = v;
        }
    }
    if old.get("pack").is_some() {
        dropped.push("pack".into());
    }
    if let Some(d) = old.get("delivery") {
        if let Some(v) = get_str(d, "default_uploader") {
            cfg.delivery.uploader = v;
        } else if let Some(v) = get_str(d, "uploader") {
            cfg.delivery.uploader = v;
        }
        if let Some(v) = d.get("link_ttl_seconds").and_then(|x| x.as_integer()) {
            cfg.delivery.link_ttl_seconds = v as u32;
        }
        if let Some(sl) = d.get("short_link") {
            cfg.delivery.short_link.enabled =
                sl.get("enabled").and_then(|x| x.as_bool()).unwrap_or(false);
            if let Some(v) = get_str(sl, "endpoint") {
                cfg.delivery.short_link.endpoint = v;
            }
            if let Some(t) = get_str(sl, "token") {
                secret_lines.push_str(&format!("[short_link]\ntoken = {}\n\n", toml_str(&t)));
                moved.push("delivery.short_link.token".into());
            }
        }
        if let Some(s3) = d.get("s3").and_then(|x| x.as_table()) {
            for (name, v) in s3 {
                let mut entry = crate::config::S3 {
                    bucket: get_str(v, "bucket").unwrap_or_default(),
                    region: get_str(v, "region").unwrap_or_default(),
                    endpoint: get_str(v, "endpoint").unwrap_or_default(),
                    download_endpoint: get_str(v, "download_endpoint"),
                    path_style: v
                        .get("path_style")
                        .and_then(|x| x.as_bool())
                        .unwrap_or(false),
                    allow_insecure_http: v
                        .get("allow_insecure_http")
                        .and_then(|x| x.as_bool())
                        .unwrap_or(false),
                };
                if entry.download_endpoint.as_deref() == Some("") {
                    entry.download_endpoint = None;
                }
                if let Some(ttl) = v.get("link_ttl_seconds").and_then(|x| x.as_integer()) {
                    cfg.delivery.link_ttl_seconds = ttl as u32;
                }
                let ak = get_str(v, "access_key");
                let sk = get_str(v, "secret_key");
                if ak.is_some() || sk.is_some() {
                    secret_lines.push_str(&format!(
                        "[s3.{name}]\naccess_key = {}\nsecret_key = {}\n\n",
                        toml_str(ak.as_deref().unwrap_or("")),
                        toml_str(sk.as_deref().unwrap_or(""))
                    ));
                    moved.push(format!("delivery.s3.{name}.access_key"));
                    moved.push(format!("delivery.s3.{name}.secret_key"));
                }
                cfg.delivery.s3.insert(name.clone(), entry);
            }
        }
    }
    Ok((cfg, secret_lines, moved, dropped))
}

fn toml_str(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const V1: &str = r#"
[general]
dev_root = "/home/x/dev/partjobs"
archive_root = "/home/x/archive"
default_cut_ratio = 0.6
default_currency = "CNY"

[pack]
default_format = "zip"
extra_ignore = []

[delivery]
default_uploader = "s3:bj"

[delivery.s3.bj]
bucket = "b"
region = "r"
endpoint = "https://s3.example"
access_key = "AKTEST"
secret_key = "SKTEST"
link_ttl_seconds = 604800
path_style = false

[delivery.short_link]
enabled = true
endpoint = "https://go.example/api/links"
token = "TOKTEST"
"#;

    #[test]
    fn converts_and_moves_secrets() {
        let old: toml::Value = toml::from_str(V1).unwrap();
        let (cfg, secrets_text, moved, dropped) = convert(&old).unwrap();
        assert_eq!(cfg.delivery.uploader, "s3:bj");
        assert_eq!(cfg.delivery.s3["bj"].bucket, "b");
        assert_eq!(cfg.delivery.link_ttl_seconds, 604800);
        assert!(cfg.delivery.short_link.enabled);
        assert_eq!(moved.len(), 3);
        assert_eq!(dropped, vec!["pack"]);
        assert!(secrets_text.contains("AKTEST") && secrets_text.contains("TOKTEST"));
        let rendered = toml::to_string(&cfg).unwrap();
        assert!(!rendered.contains("AKTEST"));
        Config::parse(&rendered).unwrap();
        let parsed: secrets::SecretsFile = toml::from_str(&secrets_text).unwrap();
        assert_eq!(parsed.s3["bj"].secret_key, "SKTEST");
        assert_eq!(parsed.short_link.token, "TOKTEST");
    }

    #[test]
    fn split_writes_files() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(dir.path());
        paths.ensure_dirs().unwrap();
        std::fs::write(&paths.config_file, V1).unwrap();
        let r = split(&paths, false).unwrap();
        assert!(r.dry_run);
        assert!(std::fs::read_to_string(&paths.config_file)
            .unwrap()
            .contains("AKTEST"));
        let r = split(&paths, true).unwrap();
        assert!(!r.dry_run);
        assert!(paths.config_file.with_extension("toml.v1").is_file());
        assert!(!std::fs::read_to_string(&paths.config_file)
            .unwrap()
            .contains("AKTEST"));
        Config::load(&paths.config_file).unwrap();
        assert_eq!(
            secrets::load_file(&paths.secrets_file).unwrap().s3["bj"].access_key,
            "AKTEST"
        );
    }
}
