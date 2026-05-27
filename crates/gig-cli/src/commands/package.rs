use crate::cli::{
    PackageArgs, PackageCheckArgs, PackageCommand, PackageMarkSentArgs, PackageRecordArgs,
};
use crate::commands::resolve_order;
use gig_core::models::{DeliveryPackage, DeliveryPackageStatus};
use gig_core::services::client_package::{
    check_client_package, mark_client_package_sent, record_client_package, PackageCheckInput,
    PackageRecordInput,
};
use gig_core::{Error, Result};
use rusqlite::Connection;
use serde_json::json;
use std::str::FromStr;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub fn run(conn: &Connection, args: PackageArgs) -> Result<()> {
    match args.command {
        PackageCommand::Check(args) => check(conn, args),
        PackageCommand::Record(args) => record(conn, args),
        PackageCommand::MarkSent(args) => mark_sent(conn, args),
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
            checked_at: &checked_at,
        },
    )?;
    if args.json {
        print_package_json(&package);
    } else {
        print_package_human(&package);
    }
    Ok(())
}

fn record(conn: &Connection, args: PackageRecordArgs) -> Result<()> {
    let order = resolve_order(Some(args.id_or_slug), conn)?;
    let status = DeliveryPackageStatus::from_str(&args.status)
        .map_err(|err| Error::Invalid(err.to_string()))?;
    let recorded_at = now_rfc3339();
    let package = record_client_package(
        conn,
        order.id,
        PackageRecordInput {
            delivery_date: &args.delivery_date,
            delivery_dir: &args.delivery_dir,
            client_dir: &args.client_dir,
            manifest_path: &args.manifest_path,
            package_path: args.package_path.as_deref(),
            status,
            recorded_at: &recorded_at,
        },
    )?;
    if args.json {
        print_package_json(&package);
    } else {
        print_package_human(&package);
    }
    Ok(())
}

fn mark_sent(conn: &Connection, args: PackageMarkSentArgs) -> Result<()> {
    let sent_at = now_rfc3339();
    let package = mark_client_package_sent(conn, args.package_id, &sent_at)?;
    if args.json {
        print_package_json(&package);
    } else {
        print_package_human(&package);
    }
    Ok(())
}

fn print_package_json(package: &DeliveryPackage) {
    let output = json!({
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
    });
    println!("{output}");
}

fn print_package_human(package: &DeliveryPackage) {
    println!("package #{} for order #{}", package.id, package.order_id);
    println!("status: {}", package.status.as_str());
    println!("next_action: {}", next_action(package.status));
}

fn next_action(status: DeliveryPackageStatus) -> &'static str {
    match status {
        DeliveryPackageStatus::Prepared => "check_package",
        DeliveryPackageStatus::Validated => "mark_package_sent",
        DeliveryPackageStatus::Sent | DeliveryPackageStatus::Cancelled => "none",
    }
}

fn now_rfc3339() -> String {
    let now = OffsetDateTime::now_utc();
    now.format(&Rfc3339)
        .unwrap_or_else(|_| now.unix_timestamp().to_string())
}
