//! Paths and config.toml. This module is the only one that reads `$XDG_*`, `$HOME`
//! and `$GIG_*` environment variables. It never sees a secret: a config file that
//! contains one is refused (see `secrets.rs` for where secrets live).

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const DB_FILE_NAME: &str = "gig-v2.db";
pub const LEGACY_DB_FILE_NAME: &str = "gig.db";

/// Resolved filesystem locations. Always absolute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
    pub db_file: PathBuf,
    pub legacy_db_file: PathBuf,
    pub config_file: PathBuf,
    pub secrets_file: PathBuf,
    pub backups_dir: PathBuf,
}

impl Paths {
    /// `GIG_HOME` puts everything under one directory (tests, portable setups).
    /// Otherwise the XDG base directories apply.
    pub fn from_env() -> Result<Self> {
        if let Some(root) = std::env::var_os("GIG_HOME").filter(|v| !v.is_empty()) {
            let root = PathBuf::from(root);
            if !root.is_absolute() {
                return Err(Error::Config("GIG_HOME must be an absolute path".into()));
            }
            return Ok(Self::under_root(&root));
        }
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .ok_or_else(|| Error::Config("HOME is not set to an absolute path".into()))?;
        let data_dir = xdg_dir("XDG_DATA_HOME", &home, ".local/share").join("gig");
        let config_dir = xdg_dir("XDG_CONFIG_HOME", &home, ".config").join("gig");
        let state_dir = xdg_dir("XDG_STATE_HOME", &home, ".local/state").join("gig");
        Ok(Self::build(data_dir, config_dir, state_dir))
    }

    pub fn under_root(root: &Path) -> Self {
        Self::build(root.join("data"), root.join("config"), root.join("state"))
    }

    fn build(data_dir: PathBuf, config_dir: PathBuf, state_dir: PathBuf) -> Self {
        Self {
            db_file: data_dir.join(DB_FILE_NAME),
            legacy_db_file: data_dir.join(LEGACY_DB_FILE_NAME),
            config_file: config_dir.join("config.toml"),
            secrets_file: config_dir.join("secrets.toml"),
            backups_dir: state_dir.join("backups"),
            data_dir,
            config_dir,
            state_dir,
        }
    }

    /// User theme files for the dashboard, `<config_dir>/themes/<name>.toml`.
    /// Optional; gig never creates it.
    pub fn themes_dir(&self) -> PathBuf {
        self.config_dir.join("themes")
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        for dir in [
            &self.data_dir,
            &self.config_dir,
            &self.state_dir,
            &self.backups_dir,
        ] {
            std::fs::create_dir_all(dir).map_err(|e| Error::PathUnavailable(dir.clone(), e))?;
            secure_dir(dir)?;
        }
        Ok(())
    }
}

fn xdg_dir(var: &str, home: &Path, fallback: &str) -> PathBuf {
    match std::env::var_os(var) {
        Some(v) if !v.is_empty() => {
            let p = PathBuf::from(v);
            if p.is_absolute() {
                return p;
            }
            home.join(fallback)
        }
        _ => home.join(fallback),
    }
}

#[cfg(unix)]
pub(crate) fn secure_dir(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| Error::PathUnavailable(path.to_path_buf(), e))
}

#[cfg(not(unix))]
pub(crate) fn secure_dir(_path: &Path) -> Result<()> {
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub general: General,
    pub delivery: Delivery,
    pub tui: Tui,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct General {
    pub dev_root: PathBuf,
    pub archive_root: PathBuf,
    /// Empty means `<dev_root>/.drafts`.
    pub drafts_dir: PathBuf,
    pub templates_dir: PathBuf,
    pub default_cut_ratio: f64,
    pub default_currency: String,
    pub warranty_days: i64,
}

impl Default for General {
    fn default() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        Self {
            dev_root: home.join("dev/partjobs"),
            archive_root: home.join("Documents/archive/work"),
            drafts_dir: PathBuf::new(),
            templates_dir: home.join(".agents/skills/partjob/templates"),
            default_cut_ratio: 0.6,
            default_currency: "CNY".into(),
            warranty_days: 15,
        }
    }
}

impl General {
    pub fn drafts_dir(&self) -> PathBuf {
        if self.drafts_dir.as_os_str().is_empty() {
            self.dev_root.join(".drafts")
        } else {
            self.drafts_dir.clone()
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Delivery {
    /// "s3:<name>" or "" for no uploader.
    pub uploader: String,
    pub link_ttl_seconds: u32,
    pub short_link: ShortLink,
    pub s3: BTreeMap<String, S3>,
}

impl Default for Delivery {
    fn default() -> Self {
        Self {
            uploader: String::new(),
            link_ttl_seconds: 604_800,
            short_link: ShortLink::default(),
            s3: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct ShortLink {
    pub enabled: bool,
    pub endpoint: String,
}

/// One S3-compatible target. Credentials are not here; see `secrets::resolve_s3`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct S3 {
    pub bucket: String,
    pub region: String,
    pub endpoint: String,
    pub download_endpoint: Option<String>,
    pub path_style: bool,
    pub allow_insecure_http: bool,
}

/// `[tui]`: settings for `gig tui`. Flags override these; `GIG_TUI_*` overrides both.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Tui {
    /// Theme name: a built-in or a file in `Paths::themes_dir()`. Unset means
    /// `gig-dark`, or `gig-light` when `light` is true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Alias of `theme = "gig-light"` when `theme` is unset.
    pub light: bool,
    /// Nerd Font glyphs next to the text labels.
    pub icons: bool,
    /// Auto-refresh period; 0 disables the timer.
    pub refresh_seconds: u64,
}

impl Default for Tui {
    fn default() -> Self {
        Self {
            theme: None,
            light: false,
            icons: true,
            refresh_seconds: 2,
        }
    }
}

impl Tui {
    /// Apply `GIG_TUI_THEME`, `GIG_TUI_LIGHT`, `GIG_TUI_ICONS` and
    /// `GIG_TUI_REFRESH_SECONDS`.
    /// Called by `gig tui` only, after its flags, for the env > flags > file
    /// precedence; `Config::load` leaves these variables alone.
    pub fn apply_env_overrides(&mut self) -> Result<()> {
        self.apply_overrides(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
    }

    pub fn apply_overrides(&mut self, env: impl Fn(&str) -> Option<String>) -> Result<()> {
        if let Some(v) = env("GIG_TUI_THEME") {
            self.theme = Some(v);
        }
        if let Some(v) = env("GIG_TUI_LIGHT") {
            self.light = parse_bool("GIG_TUI_LIGHT", &v)?;
        }
        if let Some(v) = env("GIG_TUI_ICONS") {
            self.icons = parse_bool("GIG_TUI_ICONS", &v)?;
        }
        if let Some(v) = env("GIG_TUI_REFRESH_SECONDS") {
            self.refresh_seconds = v.parse().map_err(|_| {
                Error::Config("GIG_TUI_REFRESH_SECONDS must be a non-negative integer".into())
            })?;
        }
        Ok(())
    }
}

fn parse_bool(var: &str, raw: &str) -> Result<bool> {
    match raw.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(Error::Config(format!(
            "{var} must be true or false (also 1/0, yes/no, on/off)"
        ))),
    }
}

const SECRET_KEYS: [&str; 3] = ["access_key", "secret_key", "token"];

impl Config {
    /// Load the file (defaults when absent), refuse secrets, apply `GIG_*` overrides.
    pub fn load(path: &Path) -> Result<Self> {
        let mut cfg = match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => return Err(Error::PathUnavailable(path.to_path_buf(), e)),
        };
        cfg.apply_env_overrides()?;
        Ok(cfg)
    }

    pub fn parse(text: &str) -> Result<Self> {
        let value: toml::Value = toml::from_str(text)?;
        if let Some(key) = find_secret_key(&value, "") {
            return Err(Error::Secrets(format!(
                "config.toml contains {key}; move it to secrets.toml or the environment (gig config split-secrets)"
            )));
        }
        Ok(value.try_into()?)
    }

    fn apply_env_overrides(&mut self) -> Result<()> {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        if let Some(v) = env("GIG_GENERAL_DEV_ROOT") {
            self.general.dev_root = PathBuf::from(v);
        }
        if let Some(v) = env("GIG_GENERAL_ARCHIVE_ROOT") {
            self.general.archive_root = PathBuf::from(v);
        }
        if let Some(v) = env("GIG_GENERAL_DRAFTS_DIR") {
            self.general.drafts_dir = PathBuf::from(v);
        }
        if let Some(v) = env("GIG_GENERAL_TEMPLATES_DIR") {
            self.general.templates_dir = PathBuf::from(v);
        }
        if let Some(v) = env("GIG_GENERAL_DEFAULT_CURRENCY") {
            self.general.default_currency = v;
        }
        if let Some(v) = env("GIG_GENERAL_DEFAULT_CUT_RATIO") {
            self.general.default_cut_ratio = v.parse().map_err(|_| {
                Error::Config("GIG_GENERAL_DEFAULT_CUT_RATIO must be a number".into())
            })?;
        }
        if let Some(v) = env("GIG_GENERAL_WARRANTY_DAYS") {
            self.general.warranty_days = v.parse().map_err(|_| {
                Error::Config("GIG_GENERAL_WARRANTY_DAYS must be an integer".into())
            })?;
        }
        if let Some(v) = env("GIG_DELIVERY_UPLOADER") {
            self.delivery.uploader = v;
        }
        if let Some(v) = env("GIG_DELIVERY_LINK_TTL_SECONDS") {
            self.delivery.link_ttl_seconds = v.parse().map_err(|_| {
                Error::Config("GIG_DELIVERY_LINK_TTL_SECONDS must be an integer".into())
            })?;
        }
        // `GIG_TUI_*` is applied by `gig tui` only (gig_tui::resolve_settings),
        // so a malformed TUI variable cannot break the JSON commands.
        Ok(())
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::PathUnavailable(parent.to_path_buf(), e))?;
            secure_dir(parent)?;
        }
        let text = toml::to_string_pretty(self)?;
        std::fs::write(path, text).map_err(|e| Error::PathUnavailable(path.to_path_buf(), e))
    }

    /// Dot-notation read used by `gig config get`.
    pub fn get(&self, key: &str) -> Result<toml::Value> {
        let root = toml::Value::try_from(self)?;
        let mut cur = &root;
        for part in key.split('.') {
            cur = cur
                .get(part)
                .ok_or_else(|| Error::NotFound(format!("config key {key}")))?;
        }
        Ok(cur.clone())
    }

    /// Dot-notation write used by `gig config set`. Values are parsed as TOML when
    /// possible, else stored as strings.
    pub fn set(&mut self, key: &str, raw: &str) -> Result<()> {
        if key.split('.').any(|p| SECRET_KEYS.contains(&p)) {
            return Err(Error::Secrets(format!(
                "{key} belongs in secrets.toml, not config"
            )));
        }
        let mut root = toml::Value::try_from(&*self)?;
        let parts: Vec<&str> = key.split('.').collect();
        let (last, dirs) = parts
            .split_last()
            .ok_or_else(|| Error::InvalidInput("empty config key".into()))?;
        let mut cur = &mut root;
        for part in dirs {
            let table = cur
                .as_table_mut()
                .ok_or_else(|| Error::InvalidInput(format!("{key} is not a table path")))?;
            cur = table
                .entry((*part).to_string())
                .or_insert_with(|| toml::Value::Table(Default::default()));
        }
        let table = cur
            .as_table_mut()
            .ok_or_else(|| Error::InvalidInput(format!("{key} is not a table path")))?;
        let existing = table.get(*last).cloned();
        let value = coerce(raw, existing.as_ref());
        table.insert((*last).to_string(), value);
        *self = root.try_into().map_err(|e: toml::de::Error| {
            Error::InvalidInput(format!("invalid value for {key}: {e}"))
        })?;
        Ok(())
    }

    /// The uploader name after "s3:", if any.
    pub fn s3_uploader_name(&self) -> Option<&str> {
        self.delivery
            .uploader
            .strip_prefix("s3:")
            .filter(|n| !n.is_empty())
    }
}

fn coerce(raw: &str, existing: Option<&toml::Value>) -> toml::Value {
    match existing {
        Some(toml::Value::String(_)) | None => {
            if let Ok(v) = raw.parse::<toml::Value>() {
                if !matches!(v, toml::Value::Table(_)) && existing.is_none() {
                    return v;
                }
            }
            toml::Value::String(raw.to_string())
        }
        Some(toml::Value::Integer(_)) => raw
            .parse::<i64>()
            .map(toml::Value::Integer)
            .unwrap_or_else(|_| toml::Value::String(raw.to_string())),
        Some(toml::Value::Float(_)) => raw
            .parse::<f64>()
            .map(toml::Value::Float)
            .unwrap_or_else(|_| toml::Value::String(raw.to_string())),
        Some(toml::Value::Boolean(_)) => match raw {
            "true" => toml::Value::Boolean(true),
            "false" => toml::Value::Boolean(false),
            _ => toml::Value::String(raw.to_string()),
        },
        Some(_) => raw
            .parse::<toml::Value>()
            .unwrap_or_else(|_| toml::Value::String(raw.to_string())),
    }
}

fn find_secret_key(value: &toml::Value, prefix: &str) -> Option<String> {
    let table = value.as_table()?;
    for (k, v) in table {
        let path = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{prefix}.{k}")
        };
        if SECRET_KEYS.contains(&k.as_str()) {
            return Some(path);
        }
        if let Some(found) = find_secret_key(v, &path) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_secrets_in_config() {
        let text = r#"
[delivery.s3.bj]
bucket = "b"
access_key = "AK"
"#;
        let err = Config::parse(text).unwrap_err();
        assert_eq!(err.code(), "secrets");
        assert!(err.to_string().contains("delivery.s3.bj.access_key"));

        let text = "[delivery.short_link]\ntoken = \"t\"\n";
        assert_eq!(Config::parse(text).unwrap_err().code(), "secrets");
    }

    #[test]
    fn parses_v2_layout() {
        let text = r#"
[general]
dev_root = "/tmp/p"
warranty_days = 10
[delivery]
uploader = "s3:bj"
[delivery.s3.bj]
bucket = "b"
region = "r"
endpoint = "https://x"
"#;
        let cfg = Config::parse(text).unwrap();
        assert_eq!(cfg.general.warranty_days, 10);
        assert_eq!(cfg.s3_uploader_name(), Some("bj"));
        assert_eq!(cfg.general.drafts_dir(), PathBuf::from("/tmp/p/.drafts"));
    }

    #[test]
    fn unknown_fields_are_errors() {
        assert!(Config::parse("[general]\ndefault_uploader = \"x\"\n").is_err());
    }

    #[test]
    fn get_and_set_by_dots() {
        let mut cfg = Config::default();
        cfg.set("general.warranty_days", "20").unwrap();
        assert_eq!(cfg.general.warranty_days, 20);
        cfg.set("delivery.uploader", "s3:hk").unwrap();
        assert_eq!(
            cfg.get("delivery.uploader").unwrap().as_str(),
            Some("s3:hk")
        );
        cfg.set("delivery.s3.hk.bucket", "gig").unwrap();
        assert_eq!(cfg.delivery.s3["hk"].bucket, "gig");
        assert_eq!(
            cfg.set("delivery.s3.hk.access_key", "x")
                .unwrap_err()
                .code(),
            "secrets"
        );
        assert!(cfg.set("general.warranty_days", "soon").is_err());
    }

    #[test]
    fn tui_section_defaults_and_parses() {
        let cfg = Config::parse("").unwrap();
        assert_eq!(cfg.tui, Tui::default());
        assert!(!cfg.tui.light);
        assert!(cfg.tui.icons);
        assert_eq!(cfg.tui.refresh_seconds, 2);

        let cfg =
            Config::parse("[tui]\nlight = true\nicons = false\nrefresh_seconds = 0\n").unwrap();
        assert!(cfg.tui.light);
        assert!(!cfg.tui.icons);
        assert_eq!(cfg.tui.refresh_seconds, 0);

        // Partial sections keep the other defaults.
        let cfg = Config::parse("[tui]\nrefresh_seconds = 5\n").unwrap();
        assert!(cfg.tui.icons);
        assert_eq!(cfg.tui.refresh_seconds, 5);
    }

    #[test]
    fn tui_section_refuses_unknown_fields() {
        assert!(Config::parse("[tui]\ncolour = \"dark\"\n").is_err());
        let cfg = Config::parse("[tui]\ntheme = \"nord\"\n").unwrap();
        assert_eq!(cfg.tui.theme.as_deref(), Some("nord"));
        assert!(Config::parse("[tui]\nrefresh_seconds = -1\n").is_err());
    }

    #[test]
    fn tui_env_overrides() {
        let vars: BTreeMap<&str, &str> = [
            ("GIG_TUI_THEME", "nord"),
            ("GIG_TUI_LIGHT", "1"),
            ("GIG_TUI_ICONS", "false"),
            ("GIG_TUI_REFRESH_SECONDS", "10"),
        ]
        .into();
        let mut tui = Tui::default();
        tui.apply_overrides(|k| vars.get(k).map(|v| v.to_string()))
            .unwrap();
        assert_eq!(
            tui,
            Tui {
                theme: Some("nord".into()),
                light: true,
                icons: false,
                refresh_seconds: 10
            }
        );

        // Unset variables leave the values alone.
        let mut tui = Tui::default();
        tui.apply_overrides(|_| None).unwrap();
        assert_eq!(tui, Tui::default());

        let mut tui = Tui::default();
        let err = tui
            .apply_overrides(|k| (k == "GIG_TUI_ICONS").then(|| "maybe".to_string()))
            .unwrap_err();
        assert_eq!(err.code(), "config");
        let err = tui
            .apply_overrides(|k| (k == "GIG_TUI_REFRESH_SECONDS").then(|| "-2".to_string()))
            .unwrap_err();
        assert_eq!(err.code(), "config");
    }

    #[test]
    fn tui_keys_work_with_config_set() {
        let mut cfg = Config::default();
        cfg.set("tui.refresh_seconds", "7").unwrap();
        cfg.set("tui.light", "true").unwrap();
        assert_eq!(cfg.tui.refresh_seconds, 7);
        assert!(cfg.tui.light);
        // Unset theme stays out of `config get tui`, so its JSON is unchanged.
        assert!(cfg.get("tui.theme").is_err());
        cfg.set("tui.theme", "dracula").unwrap();
        assert_eq!(cfg.tui.theme.as_deref(), Some("dracula"));
    }

    #[test]
    fn gig_home_puts_everything_under_one_root() {
        let p = Paths::under_root(Path::new("/tmp/gigtest"));
        assert_eq!(p.db_file, PathBuf::from("/tmp/gigtest/data/gig-v2.db"));
        assert_eq!(
            p.secrets_file,
            PathBuf::from("/tmp/gigtest/config/secrets.toml")
        );
    }
}
