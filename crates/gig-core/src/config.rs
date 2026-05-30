//! XDG paths and config.toml loading for gig.
//!
//! The only module allowed to touch `$XDG_*` env vars or read config files.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Resolved filesystem paths for gig. Always absolute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub data_dir: PathBuf,      // $XDG_DATA_HOME/gig
    pub config_dir: PathBuf,    // $XDG_CONFIG_HOME/gig
    pub state_dir: PathBuf,     // $XDG_STATE_HOME/gig
    pub db_file: PathBuf,       // data_dir/gig.db
    pub config_file: PathBuf,   // config_dir/config.toml
    pub templates_dir: PathBuf, // data_dir/templates
    pub backups_dir: PathBuf,   // state_dir/backups
}

impl Paths {
    /// Resolve paths from the process environment.
    ///
    /// Uses the XDG Base Directory spec. `$HOME` must be set.
    pub fn from_env() -> Result<Self> {
        let home =
            std::env::var_os("HOME").ok_or_else(|| Error::Config("HOME is not set".into()))?;
        let home = PathBuf::from(home);

        let data_dir = xdg_dir("XDG_DATA_HOME", &home, ".local/share").join("gig");
        let config_dir = xdg_dir("XDG_CONFIG_HOME", &home, ".config").join("gig");
        let state_dir = xdg_dir("XDG_STATE_HOME", &home, ".local/state").join("gig");

        Ok(Self {
            db_file: data_dir.join("gig.db"),
            templates_dir: data_dir.join("templates"),
            config_file: config_dir.join("config.toml"),
            backups_dir: state_dir.join("backups"),
            data_dir,
            config_dir,
            state_dir,
        })
    }

    /// Resolve paths under a given root. For tests only.
    #[doc(hidden)]
    pub fn under_root(root: &Path) -> Self {
        let data_dir = root.join("data/gig");
        let config_dir = root.join("config/gig");
        let state_dir = root.join("state/gig");
        Self {
            db_file: data_dir.join("gig.db"),
            templates_dir: data_dir.join("templates"),
            config_file: config_dir.join("config.toml"),
            backups_dir: state_dir.join("backups"),
            data_dir,
            config_dir,
            state_dir,
        }
    }

    /// Ensure all required directories exist.
    pub fn ensure_dirs(&self) -> Result<()> {
        for dir in [
            &self.data_dir,
            &self.config_dir,
            &self.state_dir,
            &self.templates_dir,
            &self.backups_dir,
        ] {
            std::fs::create_dir_all(dir).map_err(|e| Error::PathUnavailable(dir.clone(), e))?;
        }
        Ok(())
    }
}

fn xdg_dir(env_var: &str, home: &Path, fallback: &str) -> PathBuf {
    match std::env::var_os(env_var) {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => home.join(fallback),
    }
}

/// User-facing config loaded from `config_file`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub pack: PackConfig,
    pub delivery: DeliveryConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct PackConfig {
    /// Default archive format: "zip" or "tar.zst".
    pub default_format: String,
    /// Extra ignore patterns applied to all projects (gitignore syntax).
    pub extra_ignore: Vec<String>,
}

impl Default for PackConfig {
    fn default() -> Self {
        Self {
            default_format: "zip".into(),
            extra_ignore: vec![],
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
#[serde(default)]
pub struct DeliveryConfig {
    /// Name of the default uploader (e.g. "s3:aliyun-hk").
    pub default_uploader: String,
    /// Optional short-link service used to wrap generated upload links.
    pub short_link: ShortLinkConfig,
    /// Named S3-compatible uploader configurations.
    ///
    /// Keys are short names used after the `s3:` prefix in `default_uploader`.
    /// Example config.toml:
    /// ```toml
    /// [delivery.s3.aliyun-hk]
    /// bucket = "gig-delivery"
    /// region = "cn-hongkong"
    /// endpoint = "https://s3.oss-cn-hongkong.aliyuncs.com"
    /// access_key = "LTAI5t..."
    /// secret_key = "..."
    /// link_ttl_seconds = 604800
    /// ```
    #[serde(default)]
    pub s3: HashMap<String, S3UploaderConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
#[serde(default)]
pub struct ShortLinkConfig {
    /// Whether upload links should be shortened after backend upload.
    pub enabled: bool,
    /// API endpoint that accepts `{ url, ttl_seconds }` and returns `{ short_url }`.
    pub endpoint: String,
    /// Bearer token sent to the short-link API.
    pub token: String,
}

/// Configuration for a single S3-compatible uploader.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct S3UploaderConfig {
    /// Bucket name (e.g. `"gig-delivery"`).
    pub bucket: String,
    /// Region identifier (e.g. `"cn-hongkong"` or `"us-east-1"`).
    pub region: String,
    /// Base endpoint URL without trailing slash
    /// (e.g. `"https://s3.oss-cn-hongkong.aliyuncs.com"`).
    pub endpoint: String,
    /// Optional endpoint used only for generated download links.
    ///
    /// This lets uploads keep using the regional API endpoint while client links use
    /// an accelerated or CDN-compatible endpoint that supports the same bucket and
    /// signing scheme.
    #[serde(default)]
    pub download_endpoint: Option<String>,
    /// AWS / OSS access key ID.
    pub access_key: String,
    /// AWS / OSS secret access key.
    pub secret_key: String,
    /// How long presigned GET links remain valid (seconds). Default: 604800 (7 days).
    #[serde(default = "default_link_ttl")]
    pub link_ttl_seconds: u32,
    /// Use path-style URLs. Required for MinIO; set false for OSS/AWS.
    #[serde(default)]
    pub path_style: bool,
}

fn default_link_ttl() -> u32 {
    604_800
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct General {
    /// Default root for user-managed project folders.
    pub dev_root: PathBuf,
    /// Root directory where `gig archive` moves finished projects.
    pub archive_root: PathBuf,
    /// Default take-home ratio applied to new orders.
    pub default_cut_ratio: f64,
    /// Default currency code (ISO 4217).
    pub default_currency: String,
}

impl Default for General {
    fn default() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        Self {
            dev_root: home.join("dev"),
            archive_root: home.join("Documents/gig-archive"),
            default_cut_ratio: 0.60,
            default_currency: "CNY".into(),
        }
    }
}

impl Config {
    /// Load config from the given path, returning defaults if the file does not exist.
    pub fn load_or_default(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(toml::from_str(&text)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(Error::PathUnavailable(path.to_path_buf(), e)),
        }
    }

    /// Serialize to disk. Creates parent directory if missing.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::PathUnavailable(parent.to_path_buf(), e))?;
        }
        let text = toml::to_string_pretty(self)?;
        std::fs::write(path, text).map_err(|e| Error::PathUnavailable(path.to_path_buf(), e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn paths_under_root_uses_child_dirs() {
        let tmp = TempDir::new().unwrap();
        let p = Paths::under_root(tmp.path());
        assert!(p.db_file.starts_with(tmp.path()));
        assert!(p.db_file.ends_with("gig.db"));
        assert!(p.config_file.ends_with("config.toml"));
    }

    #[test]
    fn ensure_dirs_creates_all_required() {
        let tmp = TempDir::new().unwrap();
        let p = Paths::under_root(tmp.path());
        p.ensure_dirs().unwrap();
        assert!(p.data_dir.is_dir());
        assert!(p.config_dir.is_dir());
        assert!(p.state_dir.is_dir());
        assert!(p.templates_dir.is_dir());
        assert!(p.backups_dir.is_dir());
    }

    #[test]
    fn config_load_or_default_returns_default_when_missing() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("nope.toml");
        let cfg = Config::load_or_default(&missing).unwrap();
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn config_save_then_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("cfg/config.toml");
        let cfg = Config::default();
        cfg.save(&path).unwrap();
        let loaded = Config::load_or_default(&path).unwrap();
        assert_eq!(cfg, loaded);
    }

    #[test]
    fn config_loads_optional_short_link_delivery_settings() {
        let cfg: Config = toml::from_str(
            r#"
            [delivery]
            default_uploader = "s3:main"

            [delivery.short_link]
            enabled = true
            endpoint = "https://go.jczhang.cc/api/links"
            token = "secret-token"
            "#,
        )
        .unwrap();

        assert!(cfg.delivery.short_link.enabled);
        assert_eq!(
            cfg.delivery.short_link.endpoint,
            "https://go.jczhang.cc/api/links"
        );
        assert_eq!(cfg.delivery.short_link.token, "secret-token");
        assert!(!Config::default().delivery.short_link.enabled);
    }

    #[test]
    fn config_loads_optional_s3_download_endpoint() {
        let cfg: Config = toml::from_str(
            r#"
            [delivery]
            default_uploader = "s3:main"

            [delivery.s3.main]
            bucket = "gig-delivery"
            region = "cn-hongkong"
            endpoint = "https://s3.oss-cn-hongkong.aliyuncs.com"
            download_endpoint = "https://oss-accelerate.aliyuncs.com"
            access_key = "access"
            secret_key = "secret"
            "#,
        )
        .unwrap();

        let s3 = cfg.delivery.s3.get("main").unwrap();
        assert_eq!(
            s3.download_endpoint.as_deref(),
            Some("https://oss-accelerate.aliyuncs.com")
        );
    }

    #[test]
    fn xdg_dir_uses_env_override_when_set() {
        std::env::set_var("GIG_TEST_XDG", "/tmp/override");
        let home = PathBuf::from("/home/fake");
        let d = xdg_dir("GIG_TEST_XDG", &home, ".config");
        assert_eq!(d, PathBuf::from("/tmp/override"));
        std::env::remove_var("GIG_TEST_XDG");
    }

    #[test]
    fn xdg_dir_falls_back_to_home_when_unset() {
        std::env::remove_var("GIG_TEST_XDG_UNSET");
        let home = PathBuf::from("/home/fake");
        let d = xdg_dir("GIG_TEST_XDG_UNSET", &home, ".config");
        assert_eq!(d, PathBuf::from("/home/fake/.config"));
    }
}
