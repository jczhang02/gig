use crate::cli::{PackageArgs, PackageCheckArgs, PackageCommand, PackageSendArgs};
use crate::commands::{build_delivery_uploader, resolve_order};
use gig_core::config::{Config, Paths};
use gig_core::models::{DeliveryArtifact, DeliveryPackage, DeliveryPackageStatus};
use gig_core::services::client_package::{
    check_client_package, send_client_package, PackageCheckInput, PackageSendInput,
};
use gig_core::{Error, Result};
use rusqlite::Connection;
use serde_json::json;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub fn run(conn: &Connection, args: PackageArgs) -> Result<()> {
    match args.command {
        PackageCommand::Check(args) => check(conn, args),
        PackageCommand::Send(args) => send(conn, args),
    }
}

fn check(conn: &Connection, args: PackageCheckArgs) -> Result<()> {
    let order = resolve_order(Some(args.id_or_slug), conn)?;
    let checked_at = now_rfc3339();
    let package = check_client_package(
        conn,
        order.id,
        PackageCheckInput {
            delivery_date: &args.delivery_date,
            delivery_dir: &args.delivery_dir,
            package_id: args.package_id.as_deref(),
            checked_at: &checked_at,
        },
    )?;

    if args.json {
        print_checked_package_json(&package);
    } else {
        print_package_human(&package);
    }
    Ok(())
}

fn send(conn: &Connection, args: PackageSendArgs) -> Result<()> {
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
    let uploader = build_delivery_uploader(uploader_name, &config)?;
    let sent_at = now_rfc3339();
    let result = send_client_package(
        conn,
        order.id,
        PackageSendInput {
            delivery_date: &args.delivery_date,
            delivery_dir: &args.delivery_dir,
            package_id: args.package_id.as_deref(),
            sent_at: &sent_at,
        },
        uploader.as_ref(),
    )?;

    if args.json {
        print_package_json(&result.package, &result.artifact);
    } else {
        print_package_human(&result.package);
        if let Some(url) = &result.artifact.remote_url {
            println!("link: {url}");
        }
    }
    Ok(())
}

fn print_checked_package_json(package: &DeliveryPackage) {
    println!("{}", package_check_json(package));
}

fn print_package_json(package: &DeliveryPackage, artifact: &DeliveryArtifact) {
    println!("{}", package_json(package, artifact));
}

fn package_check_json(package: &DeliveryPackage) -> serde_json::Value {
    json!({
        "status": package.status.as_str(),
        "next_action": next_action(package.status),
        "package": {
            "id": package.id,
            "order_id": package.order_id,
            "delivery_date": package.delivery_date,
            "status": package.status.as_str(),
        },
        "paths": {
            "delivery_dir": package.delivery_dir,
            "client_dir": package.client_dir,
            "manifest_path": package.manifest_path,
            "package_path": package.package_path,
        }
    })
}

fn package_json(package: &DeliveryPackage, artifact: &DeliveryArtifact) -> serde_json::Value {
    let mut output = package_check_json(package);
    output.as_object_mut().unwrap().insert(
        "artifact".to_string(),
        json!({
            "id": artifact.id,
            "order_id": artifact.order_id,
            "local_path": artifact.local_path,
            "uploader_name": artifact.uploader_name,
            "remote_url": artifact.remote_url,
            "expires_at": artifact.expires_at,
            "uploaded_at": artifact.uploaded_at,
        }),
    );
    output
}

fn print_package_human(package: &DeliveryPackage) {
    println!("package #{} for order #{}", package.id, package.order_id);
    println!("status: {}", package.status.as_str());
    println!("next_action: {}", next_action(package.status));
}

fn next_action(status: DeliveryPackageStatus) -> &'static str {
    match status {
        DeliveryPackageStatus::Prepared | DeliveryPackageStatus::Validated => "send_package",
        DeliveryPackageStatus::Sent | DeliveryPackageStatus::Cancelled => "none",
    }
}

fn now_rfc3339() -> String {
    let now = OffsetDateTime::now_utc();
    now.format(&Rfc3339)
        .unwrap_or_else(|_| now.unix_timestamp().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gig_core::models::DeliveryArtifact;

    #[test]
    fn package_json_includes_upload_artifact_link() {
        let package = DeliveryPackage {
            id: 1,
            order_id: 2,
            delivery_date: "2026-05-27".to_string(),
            delivery_dir: "/tmp/delivery".to_string(),
            client_dir: "/tmp/delivery/client".to_string(),
            manifest_path: "/tmp/delivery/manifest.toml".to_string(),
            package_path: Some("/tmp/delivery/export/client-package.zip".to_string()),
            status: DeliveryPackageStatus::Sent,
            created_at: "2026-05-27T07:00:00Z".to_string(),
            updated_at: "2026-05-27T07:00:00Z".to_string(),
        };
        let artifact = DeliveryArtifact {
            id: 3,
            order_id: 2,
            local_path: Some("/tmp/delivery/export/client-package.zip".to_string()),
            uploader_name: Some("s3:default".to_string()),
            remote_url: Some("https://example.test/client-package.zip".to_string()),
            expires_at: Some(1_780_000_000),
            uploaded_at: 1_779_865_200,
        };

        let output = package_json(&package, &artifact);

        assert_eq!(output["status"], "sent");
        assert_eq!(
            output["artifact"]["remote_url"],
            artifact.remote_url.unwrap()
        );
        assert_eq!(output["artifact"]["uploader_name"], "s3:default");
    }
}
