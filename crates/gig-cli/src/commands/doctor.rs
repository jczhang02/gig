use crate::cli::DoctorArgs;
use gig_core::config::{Config, Paths};
use gig_core::context::{doctor_fix_paths, doctor_path_checks};
use gig_core::models::DeliveryPackageStatus;
use gig_core::repo::{delivery_packages, order_workflow};
use gig_core::Result;
use owo_colors::OwoColorize;
use rusqlite::Connection;
use std::fs;
use std::path::{Component, Path};

pub fn run(conn: &Connection, args: DoctorArgs) -> Result<()> {
    let mut diagnostics = doctor_path_checks(conn)?;
    append_workflow_file_diagnostics(conn, &mut diagnostics)?;
    append_package_artifact_diagnostics(conn, &mut diagnostics)?;

    if diagnostics.is_empty() {
        println!("{}", "all checks passed".green());
    } else {
        for d in &diagnostics {
            println!("  {}", d.yellow());
        }
        println!("\n{} issue(s) found", diagnostics.len());
    }

    if args.fix {
        let paths = Paths::from_env()?;
        let config = Config::load_or_default(&paths.config_file)?;
        let fixes = doctor_fix_paths(conn, &config.general.dev_root, &config.general.archive_root)?;
        if fixes.is_empty() {
            println!(
                "{}",
                "no fixable paths found (check dev_root and archive_root in config)".dimmed()
            );
        } else {
            println!();
            for f in &fixes {
                println!("  {}", f.green());
            }
            println!("\n{} path(s) fixed", fixes.len());
        }
    } else if !diagnostics.is_empty() {
        println!(
            "{}",
            "hint: run `gig doctor --fix` to attempt automatic repair".dimmed()
        );
    }

    Ok(())
}

fn append_workflow_file_diagnostics(
    conn: &Connection,
    diagnostics: &mut Vec<String>,
) -> Result<()> {
    for workflow in order_workflow::list(conn)? {
        for (label, path) in [
            ("index_path", workflow.index_path.as_deref()),
            ("job_path", workflow.job_path.as_deref()),
            ("quote_path", workflow.quote_path.as_deref()),
            ("plan_md_path", workflow.plan_md_path.as_deref()),
            ("plan_html_path", workflow.plan_html_path.as_deref()),
            ("acceptance_path", workflow.acceptance_path.as_deref()),
        ] {
            if let Some(path) = path {
                if !Path::new(path).exists() {
                    diagnostics.push(format!(
                        "missing workflow file: order #{} {label}={path}",
                        workflow.order_id
                    ));
                }
            }
        }
    }
    Ok(())
}

fn append_package_artifact_diagnostics(
    conn: &Connection,
    diagnostics: &mut Vec<String>,
) -> Result<()> {
    for package in delivery_packages::list_all(conn)? {
        let requires_artifact = matches!(
            package.status,
            DeliveryPackageStatus::Validated | DeliveryPackageStatus::Sent
        );

        if let Some(path) = package.package_path.as_deref() {
            if has_unsafe_path_components(Path::new(path)) {
                diagnostics.push(format!(
                    "unsafe package path: order #{} package #{} package_path={path}",
                    package.order_id, package.id
                ));
            } else if requires_artifact && fs::metadata(path).is_err() {
                diagnostics.push(format!(
                    "missing package artifact: order #{} package #{} package_path={path}",
                    package.order_id, package.id
                ));
            }
        } else if requires_artifact {
            diagnostics.push(format!(
                "missing package artifact: order #{} package #{} package_path is not recorded",
                package.order_id, package.id
            ));
        }
    }
    Ok(())
}

fn has_unsafe_path_components(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir))
}
