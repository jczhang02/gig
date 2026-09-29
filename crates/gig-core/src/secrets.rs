//! Secrets: environment first, then `secrets.toml` (0600). Never config.toml.

use crate::config::{Config, Paths, S3};
use crate::{Error, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct SecretsFile {
    pub s3: BTreeMap<String, S3Secrets>,
    pub short_link: ShortLinkSecrets,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct S3Secrets {
    pub access_key: String,
    pub secret_key: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ShortLinkSecrets {
    pub token: String,
}

/// Everything the S3 uploader needs, assembled at the call site.
#[derive(Debug, Clone)]
pub struct ResolvedS3 {
    pub name: String,
    pub config: S3,
    pub access_key: String,
    pub secret_key: String,
    pub link_ttl_seconds: u32,
}

pub fn load_file(path: &Path) -> Result<SecretsFile> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            check_mode(path)?;
            Ok(toml::from_str(&text)?)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(SecretsFile::default()),
        Err(e) => Err(Error::PathUnavailable(path.to_path_buf(), e)),
    }
}

#[cfg(unix)]
fn check_mode(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(path)?.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(Error::Secrets(format!(
            "{} is mode {mode:o}; it must not be readable by group or others (chmod 600)",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(not(unix))]
fn check_mode(_path: &Path) -> Result<()> {
    Ok(())
}

pub fn env_name(uploader: &str, suffix: &str) -> String {
    let upper: String = uploader
        .chars()
        .map(|c| {
            if c == '-' {
                '_'
            } else {
                c.to_ascii_uppercase()
            }
        })
        .collect();
    format!("GIG_S3_{upper}_{suffix}")
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// Resolve the configured S3 uploader with its credentials.
pub fn resolve_s3(config: &Config, paths: &Paths) -> Result<ResolvedS3> {
    let name = config.s3_uploader_name().ok_or_else(|| {
        Error::Config("delivery.uploader is not set to an s3:<name> target".into())
    })?;
    let s3 =
        config.delivery.s3.get(name).ok_or_else(|| {
            Error::Config(format!("no [delivery.s3.{name}] section in config.toml"))
        })?;
    let file = load_file(&paths.secrets_file)?;
    let from_file = file.s3.get(name);
    let access_key = env(&env_name(name, "ACCESS_KEY"))
        .or_else(|| {
            from_file
                .map(|s| s.access_key.clone())
                .filter(|v| !v.is_empty())
        })
        .ok_or_else(|| {
            Error::Secrets(format!(
                "no access key for uploader {name}: set {} or [s3.{name}] access_key in {}",
                env_name(name, "ACCESS_KEY"),
                paths.secrets_file.display()
            ))
        })?;
    let secret_key = env(&env_name(name, "SECRET_KEY"))
        .or_else(|| {
            from_file
                .map(|s| s.secret_key.clone())
                .filter(|v| !v.is_empty())
        })
        .ok_or_else(|| {
            Error::Secrets(format!(
                "no secret key for uploader {name}: set {} or [s3.{name}] secret_key in {}",
                env_name(name, "SECRET_KEY"),
                paths.secrets_file.display()
            ))
        })?;
    Ok(ResolvedS3 {
        name: format!("s3:{name}"),
        config: s3.clone(),
        access_key,
        secret_key,
        link_ttl_seconds: config.delivery.link_ttl_seconds,
    })
}

/// The short-link bearer token, when short links are enabled.
pub fn resolve_short_link_token(config: &Config, paths: &Paths) -> Result<Option<String>> {
    if !config.delivery.short_link.enabled {
        return Ok(None);
    }
    if let Some(t) = env("GIG_SHORT_LINK_TOKEN") {
        return Ok(Some(t));
    }
    let file = load_file(&paths.secrets_file)?;
    if !file.short_link.token.trim().is_empty() {
        return Ok(Some(file.short_link.token));
    }
    Err(Error::Secrets(format!(
        "short links are enabled but no token: set GIG_SHORT_LINK_TOKEN or [short_link] token in {}",
        paths.secrets_file.display()
    )))
}

/// Which secrets are available, without revealing them (for `gig doctor`).
pub fn availability(config: &Config, paths: &Paths) -> Vec<String> {
    let mut problems = Vec::new();
    if config.s3_uploader_name().is_some() {
        if let Err(e) = resolve_s3(config, paths) {
            problems.push(e.to_string());
        }
    }
    if let Err(e) = resolve_short_link_token(config, paths) {
        problems.push(e.to_string());
    }
    problems
}

/// Write secrets.toml with mode 0600.
pub fn write_file(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::PathUnavailable(parent.to_path_buf(), e))?;
    }
    write_private(path, text.as_bytes())
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| Error::PathUnavailable(path.to_path_buf(), e))?;
    f.write_all(bytes)?;
    std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes).map_err(|e| Error::PathUnavailable(path.to_path_buf(), e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_names() {
        assert_eq!(
            env_name("aliyun-bj", "ACCESS_KEY"),
            "GIG_S3_ALIYUN_BJ_ACCESS_KEY"
        );
    }

    #[cfg(unix)]
    #[test]
    fn secrets_file_must_be_private() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("secrets.toml");
        std::fs::write(&p, "[short_link]\ntoken = \"t\"\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(load_file(&p).unwrap_err().code(), "secrets");
        write_file(&p, "[short_link]\ntoken = \"t\"\n").unwrap();
        assert_eq!(load_file(&p).unwrap().short_link.token, "t");
    }

    #[test]
    fn resolves_from_file_when_env_missing() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(dir.path());
        let mut cfg = Config::default();
        cfg.delivery.uploader = "s3:hk".into();
        cfg.delivery.s3.insert(
            "hk".into(),
            S3 {
                bucket: "b".into(),
                region: "r".into(),
                endpoint: "https://x".into(),
                ..Default::default()
            },
        );
        assert_eq!(resolve_s3(&cfg, &paths).unwrap_err().code(), "secrets");
        write_file(
            &paths.secrets_file,
            "[s3.hk]\naccess_key = \"AK\"\nsecret_key = \"SK\"\n",
        )
        .unwrap();
        let r = resolve_s3(&cfg, &paths).unwrap();
        assert_eq!(r.access_key, "AK");
        assert_eq!(r.name, "s3:hk");
    }
}
