pub mod acceptance;
pub mod archive;
pub mod artifact;
pub mod backup;
pub mod cd;
pub mod change;
pub mod client;
pub mod completion;
pub mod config;
pub mod cut;
pub mod delete;
pub mod doctor;
pub mod export;
pub mod import;
pub mod lead;
pub mod ls;
pub mod new;
pub mod note;
pub mod package;
pub mod paid;
pub mod plan;
pub mod price;
pub mod quote;
pub mod show;
pub mod source;
pub mod stats;
pub mod status;
pub mod tag;
pub mod template;
pub mod work;

use gig_core::config::Config;
use gig_core::context::resolve_context;
use gig_core::delivery::s3::S3Uploader;
use gig_core::delivery::{HttpShortLinker, ShorteningUploader, Uploader};
use gig_core::models::Order;
use gig_core::repo::orders::find_by_id_or_slug;
use gig_core::{Error, Result};
use rusqlite::Connection;

/// Resolve an order from an optional CLI id/slug, falling back to context.
pub fn resolve_order(id: Option<String>, conn: &Connection) -> Result<Order> {
    if let Some(ref s) = id {
        return find_by_id_or_slug(conn, s);
    }

    let order_id = resolve_context(conn)?.ok_or_else(|| {
        Error::Invalid(
            "not inside a gig project directory; pass <id> or cd into one.
             hint: `gig cd <id>` prints the dev_path."
                .into(),
        )
    })?;
    gig_core::repo::orders::find_by_id(conn, order_id)
}

pub fn build_delivery_uploader(name: &str, config: &Config) -> Result<Box<dyn Uploader>> {
    let s3_name = name.strip_prefix("s3:").ok_or_else(|| {
        Error::Config(format!(
            "unsupported uploader '{name}'; use 's3:<name>' format"
        ))
    })?;
    let s3_cfg = config.delivery.s3.get(s3_name).ok_or_else(|| {
        Error::Config(format!(
            "no [delivery.s3.{s3_name}] section found in config.toml"
        ))
    })?;
    let uploader: Box<dyn Uploader> = Box::new(S3Uploader::new(name.to_string(), s3_cfg)?);

    if !config.delivery.short_link.enabled {
        return Ok(uploader);
    }

    let short_linker = HttpShortLinker::new(
        config.delivery.short_link.endpoint.clone(),
        config.delivery.short_link.token.clone(),
    )?;
    Ok(Box::new(ShorteningUploader::new(
        uploader,
        Box::new(short_linker),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gig_core::config::{Config, S3UploaderConfig, ShortLinkConfig};

    fn s3_config() -> S3UploaderConfig {
        S3UploaderConfig {
            bucket: "gig-delivery".into(),
            region: "cn-hongkong".into(),
            endpoint: "https://s3.oss-cn-hongkong.aliyuncs.com".into(),
            access_key: "AKIAIOSFODNN7EXAMPLE".into(),
            secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into(),
            link_ttl_seconds: 604_800,
            path_style: false,
        }
    }

    #[test]
    fn build_delivery_uploader_validates_enabled_short_link_config() {
        let mut config = Config::default();
        config.delivery.s3.insert("main".into(), s3_config());
        config.delivery.short_link = ShortLinkConfig {
            enabled: true,
            endpoint: String::new(),
            token: "secret-token".into(),
        };

        let err = match build_delivery_uploader("s3:main", &config) {
            Ok(_) => panic!("short link endpoint should be required when enabled"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("short link endpoint is empty"));
    }
}
