use crate::cli::{ArtifactArgs, ArtifactCommand, ArtifactSendArgs};
use crate::commands::resolve_order;
use gig_core::config::{Config, Paths};
use gig_core::delivery::s3::S3Uploader;
use gig_core::delivery::Uploader;
use gig_core::models::DeliveryArtifact;
use gig_core::services::artifact_send::{send_order_artifacts, ArtifactSendInput};
use gig_core::{Error, Result};
use rusqlite::Connection;
use serde_json::json;
use time::OffsetDateTime;

pub fn run(conn: &Connection, args: ArtifactArgs) -> Result<()> {
    match args.command {
        ArtifactCommand::Send(args) => send(conn, args),
    }
}

fn send(conn: &Connection, args: ArtifactSendArgs) -> Result<()> {
    let paths = Paths::from_env()?;
    let config = Config::load_or_default(&paths.config_file)?;
    let uploader_name = args
        .uploader
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(config.delivery.default_uploader.as_str());

    if uploader_name.is_empty() {
        return Err(Error::Config(
            "no uploader configured; set [delivery] default_uploader in config.toml \
             or pass --uploader"
                .into(),
        ));
    }

    let order = resolve_order(Some(args.id_or_slug), conn)?;
    let uploader = build_uploader(uploader_name, &config)?;
    let uploaded_at = OffsetDateTime::now_utc();
    let result = send_order_artifacts(
        conn,
        order.id,
        ArtifactSendInput {
            files: &args.files,
            uploaded_at,
        },
        uploader.as_ref(),
    )?;

    if args.json {
        print_artifacts_json(&result.artifacts);
    } else {
        print_artifacts_human(&result.artifacts);
    }
    Ok(())
}

fn print_artifacts_json(artifacts: &[DeliveryArtifact]) {
    let output = json!({
        "status": "ok",
        "artifacts": artifacts.iter().map(|artifact| json!({
            "id": artifact.id,
            "order_id": artifact.order_id,
            "local_path": artifact.local_path,
            "uploader_name": artifact.uploader_name,
            "remote_url": artifact.remote_url,
            "expires_at": artifact.expires_at,
            "uploaded_at": artifact.uploaded_at,
        })).collect::<Vec<_>>()
    });
    println!("{output}");
}

fn print_artifacts_human(artifacts: &[DeliveryArtifact]) {
    for artifact in artifacts {
        println!("artifact #{} for order #{}", artifact.id, artifact.order_id);
        if let Some(local_path) = &artifact.local_path {
            println!("local: {local_path}");
        }
        if let Some(uploader_name) = &artifact.uploader_name {
            println!("uploader: {uploader_name}");
        }
        if let Some(remote_url) = &artifact.remote_url {
            println!("link: {remote_url}");
        }
    }
}

fn build_uploader(name: &str, config: &Config) -> Result<Box<dyn Uploader>> {
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
    Ok(Box::new(S3Uploader::new(name.to_string(), s3_cfg)?))
}
