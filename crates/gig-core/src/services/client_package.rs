use crate::models::{DeliveryPackage, DeliveryPackageStatus, OrderStatus};
use crate::repo::{delivery_packages, orders};
use crate::services::orders as order_service;
use crate::{Error, Result};
use rusqlite::Connection;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::path::{Component, Path, PathBuf};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(Debug, Clone, Copy)]
pub struct PackageCheckInput<'a> {
    pub delivery_date: &'a str,
    pub delivery_dir: &'a Path,
    pub checked_at: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct PackageRecordInput<'a> {
    pub delivery_date: &'a str,
    pub delivery_dir: &'a Path,
    pub client_dir: &'a Path,
    pub manifest_path: &'a Path,
    pub package_path: Option<&'a Path>,
    pub status: DeliveryPackageStatus,
    pub recorded_at: &'a str,
}

#[derive(Debug, Deserialize)]
struct ClientManifest {
    version: i64,
    delivery_date: String,
    client_files: Vec<String>,
}

pub fn record_client_package(
    conn: &Connection,
    order_id: i64,
    input: PackageRecordInput<'_>,
) -> Result<DeliveryPackage> {
    let order = orders::find_by_id(conn, order_id)?;
    if order.status != OrderStatus::ReadyToDeliver {
        return Err(Error::Invalid(format!(
            "invalid transition: package record requires ready_to_deliver, got {}",
            order.status
        )));
    }
    if !matches!(
        input.status,
        DeliveryPackageStatus::Prepared | DeliveryPackageStatus::Validated
    ) {
        return Err(Error::Invalid(
            "package record status must be prepared or validated".to_string(),
        ));
    }
    require_existing_dir(input.delivery_dir, "delivery dir")?;
    require_existing_dir(input.client_dir, "client dir")?;
    require_existing_file(input.manifest_path, "package manifest")?;
    if input.status == DeliveryPackageStatus::Validated {
        let package_path = input.package_path.ok_or_else(|| {
            Error::Invalid(
                "missing workflow-created package artifact: client-package.zip".to_string(),
            )
        })?;
        validate_package_contents(
            input.delivery_date,
            input.client_dir,
            input.manifest_path,
            package_path,
        )?;
    } else if let Some(package_path) = input.package_path {
        require_existing_file(package_path, "package artifact")?;
    }

    let package_path = input.package_path.map(path_string);
    delivery_packages::insert(
        conn,
        &delivery_packages::NewDeliveryPackage {
            order_id,
            delivery_date: input.delivery_date,
            delivery_dir: &path_string(input.delivery_dir),
            client_dir: &path_string(input.client_dir),
            manifest_path: &path_string(input.manifest_path),
            package_path: package_path.as_deref(),
            status: input.status,
            created_at: input.recorded_at,
            updated_at: input.recorded_at,
        },
    )
}

pub fn mark_client_package_sent(
    conn: &Connection,
    package_id: i64,
    sent_at: &str,
) -> Result<DeliveryPackage> {
    let package = delivery_packages::find_by_id(conn, package_id)?;
    if package.status != DeliveryPackageStatus::Validated {
        return Err(Error::Invalid(format!(
            "package must be validated before marking sent, got {}",
            package.status
        )));
    }
    let delivered_at = parse_rfc3339_unix(sent_at)?;
    let tx = conn.unchecked_transaction()?;
    let sent =
        delivery_packages::update_status(&tx, package_id, DeliveryPackageStatus::Sent, sent_at)?;
    order_service::transition(&tx, package.order_id, OrderStatus::Delivered, delivered_at)?;
    tx.commit()?;
    Ok(sent)
}

pub fn check_client_package(
    conn: &Connection,
    order_id: i64,
    input: PackageCheckInput<'_>,
) -> Result<DeliveryPackage> {
    let order = orders::find_by_id(conn, order_id)?;
    if order.status != OrderStatus::ReadyToDeliver {
        return Err(Error::Invalid(format!(
            "invalid transition: package check requires ready_to_deliver, got {}",
            order.status
        )));
    }

    let delivery_dir = input.delivery_dir;
    let client_dir = delivery_dir.join("client");
    let manifest_path = delivery_dir.join("manifest.toml");
    let package_path = delivery_dir.join("export").join("client-package.zip");
    validate_package_contents(
        input.delivery_date,
        &client_dir,
        &manifest_path,
        &package_path,
    )?;
    let package_path = path_string(&package_path);

    delivery_packages::insert(
        conn,
        &delivery_packages::NewDeliveryPackage {
            order_id,
            delivery_date: input.delivery_date,
            delivery_dir: &path_string(delivery_dir),
            client_dir: &path_string(&client_dir),
            manifest_path: &path_string(&manifest_path),
            package_path: Some(&package_path),
            status: DeliveryPackageStatus::Validated,
            created_at: input.checked_at,
            updated_at: input.checked_at,
        },
    )
}

fn require_existing_dir(path: &Path, label: &str) -> Result<()> {
    let file_type = fs::symlink_metadata(path)
        .map_err(|err| {
            Error::Invalid(format!(
                "missing workflow-created {label}: {} ({err})",
                path.display()
            ))
        })?
        .file_type();
    if file_type.is_symlink() {
        return Err(Error::Invalid(format!(
            "workflow-created {label} must not be a symlink: {}",
            path.display()
        )));
    }
    if !file_type.is_dir() {
        Err(Error::Invalid(format!(
            "missing workflow-created {label}: {}",
            path.display()
        )))
    } else {
        Ok(())
    }
}

fn require_existing_file(path: &Path, label: &str) -> Result<()> {
    if path.is_file() {
        Ok(())
    } else {
        Err(Error::Invalid(format!(
            "missing workflow-created {label}: {}",
            path.display()
        )))
    }
}

fn read_manifest(path: &Path) -> Result<ClientManifest> {
    let contents = fs::read_to_string(path).map_err(|err| {
        Error::Invalid(format!(
            "missing workflow-created package manifest: {} ({err})",
            path.display()
        ))
    })?;
    toml::from_str(&contents).map_err(Error::TomlDe)
}

fn validate_package_contents(
    delivery_date: &str,
    client_dir: &Path,
    manifest_path: &Path,
    package_path: &Path,
) -> Result<()> {
    require_existing_dir(client_dir, "client dir")?;
    let canonical_client_dir = canonicalize_workflow_path(client_dir, "client dir")?;
    let manifest = read_manifest(manifest_path)?;
    validate_manifest(delivery_date, &manifest)?;

    for file in &manifest.client_files {
        validate_client_file_path(file)?;
        require_client_file_inside_dir(
            &canonical_client_dir,
            &client_dir.join(file),
            "package artifact",
        )?;
    }

    validate_zip_package(package_path, &manifest.client_files)
}

fn canonicalize_workflow_path(path: &Path, label: &str) -> Result<PathBuf> {
    path.canonicalize().map_err(|err| {
        Error::Invalid(format!(
            "missing workflow-created {label}: {} ({err})",
            path.display()
        ))
    })
}

fn validate_manifest(delivery_date: &str, manifest: &ClientManifest) -> Result<()> {
    if manifest.version != 1 {
        return Err(Error::Invalid(format!(
            "unsupported client manifest version: {}",
            manifest.version
        )));
    }
    if manifest.delivery_date != delivery_date {
        return Err(Error::Invalid(format!(
            "manifest delivery_date mismatch: expected {}, got {}",
            delivery_date, manifest.delivery_date
        )));
    }
    Ok(())
}

fn require_regular_file(path: &Path, label: &str) -> Result<()> {
    let file_type = fs::symlink_metadata(path)
        .map_err(|err| {
            Error::Invalid(format!(
                "missing workflow-created {label}: {} ({err})",
                path.display()
            ))
        })?
        .file_type();
    if file_type.is_symlink() {
        return Err(Error::Invalid(format!(
            "workflow-created {label} must not be a symlink: {}",
            path.display()
        )));
    }
    if !file_type.is_file() {
        return Err(Error::Invalid(format!(
            "missing workflow-created {label}: {}",
            path.display()
        )));
    }
    Ok(())
}

fn require_client_file_inside_dir(client_dir: &Path, path: &Path, label: &str) -> Result<()> {
    require_regular_file(path, label)?;
    let canonical_path = canonicalize_workflow_path(path, label)?;
    if !canonical_path.starts_with(client_dir) {
        return Err(Error::Invalid(format!(
            "workflow-created {label} must stay inside client dir: {}",
            path.display()
        )));
    }
    Ok(())
}

fn validate_zip_package(package_path: &Path, client_files: &[String]) -> Result<()> {
    require_regular_file(package_path, "package artifact")?;
    let allowed_entries = client_files
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let file = File::open(package_path)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|err| {
        Error::Invalid(format!(
            "invalid package zip: {} ({err})",
            package_path.display()
        ))
    })?;

    let mut entries = HashSet::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|err| {
            Error::Invalid(format!(
                "invalid package zip entry in {} ({err})",
                package_path.display()
            ))
        })?;
        let name = entry.name().trim_end_matches('/');
        validate_client_file_path(name)?;
        if let Some(mode) = entry.unix_mode() {
            if mode & 0o170000 == 0o120000 {
                return Err(Error::Invalid(format!(
                    "package zip entry must not be a symlink: {}",
                    entry.name()
                )));
            }
        }
        if !entry.is_dir() {
            if !allowed_entries.contains(name) {
                return Err(Error::Invalid(format!(
                    "unmanifested package zip entry: {name}"
                )));
            }
            entries.insert(name.to_string());
        } else if !is_manifest_parent_dir(name, client_files) {
            return Err(Error::Invalid(format!(
                "unmanifested package zip entry: {name}"
            )));
        }
    }

    for file in client_files {
        if !entries.contains(file) {
            return Err(Error::Invalid(format!("missing package zip entry: {file}")));
        }
    }
    Ok(())
}

fn is_manifest_parent_dir(name: &str, client_files: &[String]) -> bool {
    let prefix = format!("{name}/");
    client_files.iter().any(|file| file.starts_with(&prefix))
}

fn parse_rfc3339_unix(timestamp: &str) -> Result<i64> {
    OffsetDateTime::parse(timestamp, &Rfc3339)
        .map(|timestamp| timestamp.unix_timestamp())
        .map_err(|err| Error::Invalid(format!("invalid sent_at timestamp: {timestamp} ({err})")))
}

fn validate_client_file_path(path: &str) -> Result<()> {
    if path.is_empty() || path.contains('\\') {
        return unsafe_package_path(path);
    }
    for component in path.split('/') {
        if component.is_empty()
            || component == "."
            || component == ".."
            || is_banned_client_component(component)
        {
            return unsafe_package_path(path);
        }
    }

    let relative = Path::new(path);
    if relative.is_absolute() {
        return unsafe_package_path(path);
    }

    for component in relative.components() {
        match component {
            Component::Normal(value) => {
                let value = value.to_string_lossy();
                if is_banned_client_component(&value) {
                    return unsafe_package_path(path);
                }
            }
            Component::CurDir => return unsafe_package_path(path),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return unsafe_package_path(path);
            }
        }
    }

    Ok(())
}

fn is_banned_client_component(component: &str) -> bool {
    component.starts_with('.')
        || matches!(
            component,
            "prompts" | "internal" | "ACCEPTANCE.md" | "DELIVERY_INTERNAL.html"
        )
}

fn unsafe_package_path<T>(path: &str) -> Result<T> {
    Err(Error::Invalid(format!("unsafe package path: {path}")))
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::models::{DeliveryPackageStatus, OrderStatus, ProjectType};
    use crate::repo::orders::NewOrder;
    use crate::repo::{delivery_packages, orders};
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    fn ready_order(conn: &rusqlite::Connection) -> i64 {
        orders::insert(
            conn,
            &NewOrder {
                slug: Some("package-order"),
                external_id: None,
                title: "Package order",
                client_id: None,
                source_org: None,
                source_id: None,
                project_type: Some(ProjectType::Crawler),
                status: OrderStatus::ReadyToDeliver,
                quoted_price: Some(15_000),
                final_price: None,
                my_cut_ratio: 0.6,
                currency: "CNY",
                notes: None,
                created_at: 1_700_000_000,
                accepted_at: Some(1_700_000_000),
            },
        )
        .unwrap()
        .id
    }

    fn write_manifest<const N: usize>(delivery_dir: &Path, client_files: [&str; N]) {
        let entries = client_files
            .iter()
            .map(|path| format!("\"{path}\""))
            .collect::<Vec<_>>()
            .join(", ");
        fs::write(
            delivery_dir.join("manifest.toml"),
            format!("version = 1\ndelivery_date = \"2026-05-27\"\nclient_files = [{entries}]\n"),
        )
        .unwrap();
    }

    fn delivery_layout(root: &Path) -> PathBuf {
        root.join("project")
            .join(".gig")
            .join("delivery")
            .join("2026-05-27")
    }

    fn write_zip<const N: usize>(path: &Path, entries: [(&str, &str); N]) {
        let file = fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, contents) in entries {
            zip.start_file(name, options).unwrap();
            zip.write_all(contents.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn check_client_package_validates_manifest_and_records_existing_zip() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let client_dir = delivery_dir.join("client");
        let export_dir = delivery_dir.join("export");
        fs::create_dir_all(&client_dir).unwrap();
        fs::create_dir_all(&export_dir).unwrap();
        fs::write(client_dir.join("DELIVERY_CLIENT.html"), "client html").unwrap();
        fs::write(client_dir.join("DELIVERY_CLIENT.pdf"), "client pdf").unwrap();
        write_zip(
            &export_dir.join("client-package.zip"),
            [
                ("DELIVERY_CLIENT.html", "client html"),
                ("DELIVERY_CLIENT.pdf", "client pdf"),
            ],
        );
        write_manifest(
            &delivery_dir,
            ["DELIVERY_CLIENT.html", "DELIVERY_CLIENT.pdf"],
        );

        let package = check_client_package(
            &conn,
            order_id,
            PackageCheckInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                checked_at: "2026-05-27T06:00:00Z",
            },
        )
        .unwrap();

        assert_eq!(package.order_id, order_id);
        assert_eq!(package.status, DeliveryPackageStatus::Validated);
        assert_eq!(package.delivery_date, "2026-05-27");
        assert_eq!(package.client_dir, client_dir.to_string_lossy());
        assert_eq!(
            package.manifest_path,
            delivery_dir.join("manifest.toml").to_string_lossy()
        );
        assert_eq!(
            package.package_path,
            Some(
                export_dir
                    .join("client-package.zip")
                    .to_string_lossy()
                    .into_owned()
            )
        );

        let listed = delivery_packages::list_for_order(&conn, order_id).unwrap();
        assert_eq!(listed, vec![package]);
    }

    #[test]
    fn check_client_package_rejects_unsafe_manifest_paths_without_recording() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let client_dir = delivery_dir.join("client");
        fs::create_dir_all(&client_dir).unwrap();
        fs::write(client_dir.join("DELIVERY_CLIENT.html"), "client html").unwrap();
        write_manifest(
            &delivery_dir,
            [
                "DELIVERY_CLIENT.html",
                "../secret.txt",
                "internal/notes.md",
                "prompts/build.md",
                "ACCEPTANCE.md",
                "DELIVERY_INTERNAL.html",
                ".hidden",
            ],
        );

        let err = check_client_package(
            &conn,
            order_id,
            PackageCheckInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                checked_at: "2026-05-27T06:00:00Z",
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("unsafe package path"));
        assert!(delivery_packages::list_for_order(&conn, order_id)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn check_client_package_rejects_symlinked_client_files_without_recording() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let client_dir = delivery_dir.join("client");
        let export_dir = delivery_dir.join("export");
        fs::create_dir_all(&client_dir).unwrap();
        fs::create_dir_all(&export_dir).unwrap();
        let secret = root.path().join("secret.pdf");
        fs::write(&secret, "not a package file").unwrap();
        std::os::unix::fs::symlink(&secret, client_dir.join("DELIVERY_CLIENT.pdf")).unwrap();
        write_manifest(&delivery_dir, ["DELIVERY_CLIENT.pdf"]);
        write_zip(
            &export_dir.join("client-package.zip"),
            [("DELIVERY_CLIENT.pdf", "client pdf")],
        );

        let err = check_client_package(
            &conn,
            order_id,
            PackageCheckInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                checked_at: "2026-05-27T06:00:00Z",
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("symlink"));
        assert!(delivery_packages::list_for_order(&conn, order_id)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn check_client_package_rejects_symlinked_client_dir_without_recording() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let client_dir = delivery_dir.join("client");
        let export_dir = delivery_dir.join("export");
        let outside_client_dir = root.path().join("outside-client");
        fs::create_dir_all(&outside_client_dir).unwrap();
        fs::create_dir_all(&export_dir).unwrap();
        fs::write(
            outside_client_dir.join("DELIVERY_CLIENT.pdf"),
            "not a package file",
        )
        .unwrap();
        std::os::unix::fs::symlink(&outside_client_dir, &client_dir).unwrap();
        write_manifest(&delivery_dir, ["DELIVERY_CLIENT.pdf"]);
        write_zip(
            &export_dir.join("client-package.zip"),
            [("DELIVERY_CLIENT.pdf", "client pdf")],
        );

        let err = check_client_package(
            &conn,
            order_id,
            PackageCheckInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                checked_at: "2026-05-27T06:00:00Z",
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("symlink"));
        assert!(delivery_packages::list_for_order(&conn, order_id)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn check_client_package_rejects_unsafe_zip_entries_without_recording() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let client_dir = delivery_dir.join("client");
        let export_dir = delivery_dir.join("export");
        fs::create_dir_all(&client_dir).unwrap();
        fs::create_dir_all(&export_dir).unwrap();
        fs::write(client_dir.join("DELIVERY_CLIENT.html"), "client html").unwrap();
        write_manifest(&delivery_dir, ["DELIVERY_CLIENT.html"]);
        write_zip(
            &export_dir.join("client-package.zip"),
            [
                ("DELIVERY_CLIENT.html", "client html"),
                ("../secret.txt", "secret"),
            ],
        );

        let err = check_client_package(
            &conn,
            order_id,
            PackageCheckInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                checked_at: "2026-05-27T06:00:00Z",
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("unsafe package path"));
        assert!(delivery_packages::list_for_order(&conn, order_id)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn check_client_package_rejects_unmanifested_zip_entries_without_recording() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let client_dir = delivery_dir.join("client");
        let export_dir = delivery_dir.join("export");
        fs::create_dir_all(&client_dir).unwrap();
        fs::create_dir_all(&export_dir).unwrap();
        fs::write(client_dir.join("DELIVERY_CLIENT.html"), "client html").unwrap();
        write_manifest(&delivery_dir, ["DELIVERY_CLIENT.html"]);
        write_zip(
            &export_dir.join("client-package.zip"),
            [
                ("DELIVERY_CLIENT.html", "client html"),
                ("secret.txt", "secret"),
            ],
        );

        let err = check_client_package(
            &conn,
            order_id,
            PackageCheckInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                checked_at: "2026-05-27T06:00:00Z",
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("unmanifested package zip entry"));
        assert!(delivery_packages::list_for_order(&conn, order_id)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn check_client_package_rejects_backslash_zip_entries_without_recording() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let client_dir = delivery_dir.join("client");
        let export_dir = delivery_dir.join("export");
        fs::create_dir_all(&client_dir).unwrap();
        fs::create_dir_all(&export_dir).unwrap();
        fs::write(client_dir.join("DELIVERY_CLIENT.html"), "client html").unwrap();
        write_manifest(&delivery_dir, ["DELIVERY_CLIENT.html"]);
        write_zip(
            &export_dir.join("client-package.zip"),
            [
                ("DELIVERY_CLIENT.html", "client html"),
                (r"internal\notes.md", "secret"),
            ],
        );

        let err = check_client_package(
            &conn,
            order_id,
            PackageCheckInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                checked_at: "2026-05-27T06:00:00Z",
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("unsafe package path"));
        assert!(delivery_packages::list_for_order(&conn, order_id)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn check_client_package_requires_existing_zip_without_creating_archive() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let client_dir = delivery_dir.join("client");
        let package_path = delivery_dir.join("export").join("client-package.zip");
        fs::create_dir_all(&client_dir).unwrap();
        fs::write(client_dir.join("DELIVERY_CLIENT.html"), "client html").unwrap();
        write_manifest(&delivery_dir, ["DELIVERY_CLIENT.html"]);

        let err = check_client_package(
            &conn,
            order_id,
            PackageCheckInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                checked_at: "2026-05-27T06:00:00Z",
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("client-package.zip"));
        assert!(!package_path.exists());
        assert!(delivery_packages::list_for_order(&conn, order_id)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn record_client_package_records_existing_metadata_without_validating_contents() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let client_dir = delivery_dir.join("client");
        let manifest_path = delivery_dir.join("manifest.toml");
        fs::create_dir_all(&client_dir).unwrap();
        fs::write(&manifest_path, "version = 1\n").unwrap();

        let package = record_client_package(
            &conn,
            order_id,
            PackageRecordInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                client_dir: &client_dir,
                manifest_path: &manifest_path,
                package_path: None,
                status: DeliveryPackageStatus::Prepared,
                recorded_at: "2026-05-27T06:30:00Z",
            },
        )
        .unwrap();

        assert_eq!(package.status, DeliveryPackageStatus::Prepared);
        assert_eq!(package.package_path, None);
        assert_eq!(package.delivery_dir, delivery_dir.to_string_lossy());
        assert_eq!(
            delivery_packages::list_for_order(&conn, order_id).unwrap(),
            vec![package]
        );
    }

    #[test]
    fn record_client_package_validated_rejects_invalid_manifest_without_recording() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let client_dir = delivery_dir.join("client");
        let export_dir = delivery_dir.join("export");
        let manifest_path = delivery_dir.join("manifest.toml");
        let package_path = export_dir.join("client-package.zip");
        fs::create_dir_all(&client_dir).unwrap();
        fs::create_dir_all(&export_dir).unwrap();
        fs::write(client_dir.join("DELIVERY_CLIENT.html"), "client html").unwrap();
        write_manifest(&delivery_dir, ["DELIVERY_CLIENT.html", "../secret.txt"]);
        write_zip(&package_path, [("DELIVERY_CLIENT.html", "client html")]);

        let err = record_client_package(
            &conn,
            order_id,
            PackageRecordInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                client_dir: &client_dir,
                manifest_path: &manifest_path,
                package_path: Some(&package_path),
                status: DeliveryPackageStatus::Validated,
                recorded_at: "2026-05-27T06:30:00Z",
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("unsafe package path"));
        assert!(delivery_packages::list_for_order(&conn, order_id)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn record_client_package_rejects_sent_status() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let client_dir = delivery_dir.join("client");
        let manifest_path = delivery_dir.join("manifest.toml");
        fs::create_dir_all(&client_dir).unwrap();
        fs::write(&manifest_path, "version = 1\n").unwrap();

        let err = record_client_package(
            &conn,
            order_id,
            PackageRecordInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                client_dir: &client_dir,
                manifest_path: &manifest_path,
                package_path: None,
                status: DeliveryPackageStatus::Sent,
                recorded_at: "2026-05-27T06:30:00Z",
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("prepared or validated"));
        assert!(delivery_packages::list_for_order(&conn, order_id)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn mark_client_package_sent_requires_validated_package() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let client_dir = delivery_dir.join("client");
        let manifest_path = delivery_dir.join("manifest.toml");
        fs::create_dir_all(&client_dir).unwrap();
        fs::write(&manifest_path, "version = 1\n").unwrap();
        let package = record_client_package(
            &conn,
            order_id,
            PackageRecordInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                client_dir: &client_dir,
                manifest_path: &manifest_path,
                package_path: None,
                status: DeliveryPackageStatus::Prepared,
                recorded_at: "2026-05-27T06:30:00Z",
            },
        )
        .unwrap();

        let err = mark_client_package_sent(&conn, package.id, "2026-05-27T07:00:00Z").unwrap_err();

        assert!(err.to_string().contains("validated"));
        assert_eq!(
            delivery_packages::find_by_id(&conn, package.id)
                .unwrap()
                .status,
            DeliveryPackageStatus::Prepared
        );
    }

    #[test]
    fn mark_client_package_sent_records_external_send_confirmation() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let client_dir = delivery_dir.join("client");
        let export_dir = delivery_dir.join("export");
        fs::create_dir_all(&client_dir).unwrap();
        fs::create_dir_all(&export_dir).unwrap();
        fs::write(client_dir.join("DELIVERY_CLIENT.html"), "client html").unwrap();
        write_manifest(&delivery_dir, ["DELIVERY_CLIENT.html"]);
        write_zip(
            &export_dir.join("client-package.zip"),
            [("DELIVERY_CLIENT.html", "client html")],
        );
        let package = check_client_package(
            &conn,
            order_id,
            PackageCheckInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                checked_at: "2026-05-27T06:00:00Z",
            },
        )
        .unwrap();

        let sent = mark_client_package_sent(&conn, package.id, "2026-05-27T07:00:00Z").unwrap();

        assert_eq!(sent.status, DeliveryPackageStatus::Sent);
        assert_eq!(sent.updated_at, "2026-05-27T07:00:00Z");
        let order = orders::find_by_id(&conn, order_id).unwrap();
        assert_eq!(order.status, OrderStatus::Delivered);
        assert_eq!(order.delivered_at, Some(1_779_865_200));
    }
}
