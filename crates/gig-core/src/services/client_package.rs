use crate::delivery::{UploadOpts, Uploader};
use crate::models::{DeliveryArtifact, DeliveryPackage, DeliveryPackageStatus, OrderStatus};
use crate::repo::{delivery_artifacts, delivery_packages, orders};
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
    pub package_id: Option<&'a str>,
    pub checked_at: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct PackageSendInput<'a> {
    pub delivery_date: &'a str,
    pub delivery_dir: &'a Path,
    pub package_id: Option<&'a str>,
    pub sent_at: &'a str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PackageSendResult {
    pub package: DeliveryPackage,
    pub artifact: DeliveryArtifact,
}

#[derive(Debug, Deserialize)]
struct ClientManifest {
    version: i64,
    delivery_date: String,
    client_files: Vec<String>,
}

pub fn send_client_package(
    conn: &Connection,
    order_id: i64,
    input: PackageSendInput<'_>,
    uploader: &dyn Uploader,
) -> Result<PackageSendResult> {
    let order = orders::find_by_id(conn, order_id)?;
    ensure_package_send_status(order.status)?;
    let paths = validate_package_input(input.delivery_date, input.delivery_dir, input.package_id)?;
    let package_path = path_string(&paths.package_path);
    require_validated_package_for_send(conn, order_id, input.delivery_date, &package_path)?;
    let sent_at = parse_rfc3339(input.sent_at)?;
    let uploaded_at = sent_at.unix_timestamp();
    let delivery_attempt = next_delivery_attempt(conn, order_id, input.delivery_date)?;
    let upload_result = uploader.upload(
        &paths.package_path,
        &UploadOpts {
            link_ttl_days: None,
            object_key: Some(client_package_object_key(
                order_id,
                input.delivery_date,
                sent_at,
                delivery_attempt,
                &paths.package_path,
            )?),
        },
    )?;

    let tx = conn.unchecked_transaction()?;
    let package = delivery_packages::insert(
        &tx,
        &delivery_packages::NewDeliveryPackage {
            order_id,
            delivery_date: input.delivery_date,
            delivery_dir: &path_string(input.delivery_dir),
            client_dir: &path_string(&paths.client_dir),
            manifest_path: &path_string(&paths.manifest_path),
            package_path: Some(&package_path),
            status: DeliveryPackageStatus::Sent,
            created_at: input.sent_at,
            updated_at: input.sent_at,
        },
    )?;
    let artifact = delivery_artifacts::insert(
        &tx,
        order_id,
        Some(&package_path),
        Some(uploader.name()),
        Some(&upload_result.url),
        upload_result.expires_at,
        uploaded_at,
    )?;
    order_service::transition(&tx, order_id, OrderStatus::Delivered, uploaded_at)?;
    tx.commit()?;

    Ok(PackageSendResult { package, artifact })
}

fn require_validated_package_for_send(
    conn: &Connection,
    order_id: i64,
    delivery_date: &str,
    package_path: &str,
) -> Result<()> {
    let has_validated_package = delivery_packages::list_for_order(conn, order_id)?
        .into_iter()
        .any(|package| {
            package.status == DeliveryPackageStatus::Validated
                && package.delivery_date == delivery_date
                && package.package_path.as_deref() == Some(package_path)
        });

    if has_validated_package {
        return Ok(());
    }

    Err(Error::Invalid(format!(
        "client package must be validated before send: run gig package check for {delivery_date}"
    )))
}

pub fn check_client_package(
    conn: &Connection,
    order_id: i64,
    input: PackageCheckInput<'_>,
) -> Result<DeliveryPackage> {
    let order = orders::find_by_id(conn, order_id)?;
    ensure_package_send_status(order.status)?;

    let paths = validate_package_input(input.delivery_date, input.delivery_dir, input.package_id)?;
    let package_path = path_string(&paths.package_path);

    delivery_packages::insert(
        conn,
        &delivery_packages::NewDeliveryPackage {
            order_id,
            delivery_date: input.delivery_date,
            delivery_dir: &path_string(input.delivery_dir),
            client_dir: &path_string(&paths.client_dir),
            manifest_path: &path_string(&paths.manifest_path),
            package_path: Some(&package_path),
            status: DeliveryPackageStatus::Validated,
            created_at: input.checked_at,
            updated_at: input.checked_at,
        },
    )
}

#[derive(Debug)]
struct ValidatedPackagePaths {
    client_dir: PathBuf,
    manifest_path: PathBuf,
    package_path: PathBuf,
}

fn ensure_package_send_status(status: OrderStatus) -> Result<()> {
    if matches!(status, OrderStatus::ReadyToDeliver | OrderStatus::Revision) {
        Ok(())
    } else {
        Err(Error::Invalid(format!(
            "invalid transition: package send requires ready_to_deliver or revision, got {status}"
        )))
    }
}

fn validate_package_input(
    delivery_date: &str,
    delivery_dir: &Path,
    package_id: Option<&str>,
) -> Result<ValidatedPackagePaths> {
    let client_dir = delivery_dir.join("client");
    let manifest_path = delivery_dir.join("manifest.toml");
    let package_path = delivery_dir
        .join("export")
        .join(client_package_filename(package_id)?);
    validate_package_contents(
        delivery_date,
        delivery_dir,
        &client_dir,
        &manifest_path,
        &package_path,
    )?;
    Ok(ValidatedPackagePaths {
        client_dir,
        manifest_path,
        package_path,
    })
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
    delivery_dir: &Path,
    client_dir: &Path,
    manifest_path: &Path,
    package_path: &Path,
) -> Result<()> {
    require_existing_dir(delivery_dir, "delivery dir")?;
    let canonical_delivery_dir = canonicalize_workflow_path(delivery_dir, "delivery dir")?;
    require_existing_dir(client_dir, "client dir")?;
    let canonical_client_dir = canonicalize_workflow_path(client_dir, "client dir")?;
    require_path_inside_dir(
        &canonical_delivery_dir,
        &canonical_client_dir,
        client_dir,
        "client dir",
    )?;
    require_workflow_file_inside_dir(&canonical_delivery_dir, manifest_path, "package manifest")?;
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

    require_workflow_file_inside_dir(&canonical_delivery_dir, package_path, "package artifact")?;
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
    if manifest.client_files.is_empty() {
        return Err(Error::Invalid(
            "client manifest must list at least one file".to_string(),
        ));
    }
    let mut seen = HashSet::new();
    for file in &manifest.client_files {
        if !seen.insert(file) {
            return Err(Error::Invalid(format!(
                "duplicate client manifest entry: {file}"
            )));
        }
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

fn require_workflow_file_inside_dir(workflow_dir: &Path, path: &Path, label: &str) -> Result<()> {
    require_regular_file(path, label)?;
    let canonical_path = canonicalize_workflow_path(path, label)?;
    require_path_inside_dir(workflow_dir, &canonical_path, path, label)
}

fn require_path_inside_dir(
    expected_dir: &Path,
    canonical_path: &Path,
    original_path: &Path,
    label: &str,
) -> Result<()> {
    if !canonical_path.starts_with(expected_dir) {
        return Err(Error::Invalid(format!(
            "workflow-created {label} must stay inside delivery dir: {}",
            original_path.display()
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
            if !entries.insert(name.to_string()) {
                return Err(Error::Invalid(format!(
                    "duplicate package zip entry: {name}"
                )));
            }
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

fn parse_rfc3339(timestamp: &str) -> Result<OffsetDateTime> {
    OffsetDateTime::parse(timestamp, &Rfc3339)
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

fn next_delivery_attempt(conn: &Connection, order_id: i64, delivery_date: &str) -> Result<usize> {
    let attempts = delivery_packages::list_for_order(conn, order_id)?
        .iter()
        .filter(|package| {
            package.delivery_date == delivery_date && package.status == DeliveryPackageStatus::Sent
        })
        .count();
    Ok(attempts)
}

fn client_package_filename(package_id: Option<&str>) -> Result<String> {
    let Some(id) = package_id else {
        return Ok("client-package.zip".to_string());
    };

    let valid = !id.is_empty()
        && id.len() <= 64
        && id.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        && !id.contains("..")
        && !id.to_ascii_lowercase().ends_with(".zip");
    if !valid {
        return Err(Error::Invalid(format!(
            "invalid client package id: {id}; use 1-64 ASCII letters, numbers, '.', '_' or '-', start with a letter or number, and omit the .zip suffix"
        )));
    }

    Ok(format!("{id}.zip"))
}

fn client_package_object_key(
    order_id: i64,
    delivery_date: &str,
    uploaded_at: OffsetDateTime,
    delivery_attempt: usize,
    package_path: &Path,
) -> Result<String> {
    let filename = package_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            Error::Invalid(format!(
                "invalid client package path: {}",
                package_path.display()
            ))
        })?;
    let uploaded_at = uploaded_at.unix_timestamp_nanos();
    if filename == "client-package.zip" {
        Ok(format!(
            "deliveries/{order_id}/{delivery_date}/{uploaded_at}-{delivery_attempt}-{filename}"
        ))
    } else {
        Ok(format!(
            "deliveries/{order_id}/{delivery_date}/{uploaded_at}-{delivery_attempt}/{filename}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::delivery::{ShortLinker, ShorteningUploader, UploadOpts, UploadResult, Uploader};
    use crate::models::{DeliveryPackageStatus, OrderStatus, ProjectType};
    use crate::repo::orders::NewOrder;
    use crate::repo::{delivery_artifacts, delivery_packages, orders};
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct UploadCall {
        local: PathBuf,
        object_key: Option<String>,
    }

    #[derive(Debug, Default)]
    struct RecordingUploader {
        upload_calls: Mutex<Vec<UploadCall>>,
    }

    #[derive(Debug, Default)]
    struct FailingUploader {
        upload_calls: Mutex<Vec<UploadCall>>,
    }

    #[derive(Debug)]
    struct StaticShortLinker;

    impl RecordingUploader {
        fn uploaded_paths(&self) -> Vec<PathBuf> {
            self.upload_calls
                .lock()
                .unwrap()
                .iter()
                .map(|call| call.local.clone())
                .collect()
        }

        fn uploaded_object_keys(&self) -> Vec<Option<String>> {
            self.upload_calls
                .lock()
                .unwrap()
                .iter()
                .map(|call| call.object_key.clone())
                .collect()
        }
    }

    impl Uploader for RecordingUploader {
        fn name(&self) -> &str {
            "test:uploader"
        }

        fn upload(&self, local: &Path, opts: &UploadOpts) -> Result<UploadResult> {
            assert_eq!(opts.link_ttl_days, None);
            self.upload_calls.lock().unwrap().push(UploadCall {
                local: local.to_path_buf(),
                object_key: opts.object_key.clone(),
            });
            Ok(UploadResult {
                url: "https://example.test/client-package.zip".to_string(),
                expires_at: Some(1_780_000_000),
                provider: self.name().to_string(),
                file_size: fs::metadata(local).unwrap().len(),
            })
        }
    }

    impl FailingUploader {
        fn uploaded_paths(&self) -> Vec<PathBuf> {
            self.upload_calls
                .lock()
                .unwrap()
                .iter()
                .map(|call| call.local.clone())
                .collect()
        }
    }

    impl Uploader for FailingUploader {
        fn name(&self) -> &str {
            "test:failing"
        }

        fn upload(&self, local: &Path, opts: &UploadOpts) -> Result<UploadResult> {
            assert_eq!(opts.link_ttl_days, None);
            self.upload_calls.lock().unwrap().push(UploadCall {
                local: local.to_path_buf(),
                object_key: opts.object_key.clone(),
            });
            Err(Error::Invalid("upload failed".to_string()))
        }
    }

    impl ShortLinker for StaticShortLinker {
        fn shorten(&self, _long_url: &str, _ttl_seconds: Option<u32>) -> Result<String> {
            Ok("https://go.jczhang.cc/pkg12345".to_string())
        }
    }

    fn ready_order(conn: &rusqlite::Connection) -> i64 {
        order_with_status(conn, OrderStatus::ReadyToDeliver)
    }

    fn order_with_status(conn: &rusqlite::Connection, status: OrderStatus) -> i64 {
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
                status,
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

    fn write_required_delivery_docs(delivery_dir: &Path) {
        let client_dir = delivery_dir.join("client");
        let internal_dir = delivery_dir.join("internal");
        fs::create_dir_all(&client_dir).unwrap();
        fs::create_dir_all(&internal_dir).unwrap();
        fs::write(delivery_dir.join("DELIVERY.md"), "delivery source").unwrap();
        fs::write(
            internal_dir.join("DELIVERY_INTERNAL.html"),
            "internal delivery html",
        )
        .unwrap();
        fs::write(client_dir.join("DELIVERY_CLIENT.html"), "client html").unwrap();
        fs::write(client_dir.join("DELIVERY_CLIENT.pdf"), "client pdf").unwrap();
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

    fn validate_delivery_package(
        conn: &rusqlite::Connection,
        order_id: i64,
        delivery_dir: &Path,
    ) -> DeliveryPackage {
        check_client_package(
            conn,
            order_id,
            PackageCheckInput {
                delivery_date: "2026-05-27",
                delivery_dir,
                package_id: None,
                checked_at: "2026-05-27T06:00:00Z",
            },
        )
        .unwrap()
    }

    #[test]
    fn check_client_package_validates_manifest_and_records_existing_zip() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let client_dir = delivery_dir.join("client");
        let export_dir = delivery_dir.join("export");
        fs::create_dir_all(&export_dir).unwrap();
        write_required_delivery_docs(&delivery_dir);
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
                package_id: None,
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
    fn check_client_package_does_not_require_fixed_delivery_documents() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let client_dir = delivery_dir.join("client");
        let export_dir = delivery_dir.join("export");
        fs::create_dir_all(&client_dir).unwrap();
        fs::create_dir_all(&export_dir).unwrap();
        fs::write(client_dir.join("report.pdf"), "client report").unwrap();
        write_manifest(&delivery_dir, ["report.pdf"]);
        write_zip(
            &export_dir.join("client-package.zip"),
            [("report.pdf", "client report")],
        );

        let package = check_client_package(
            &conn,
            order_id,
            PackageCheckInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                package_id: None,
                checked_at: "2026-05-27T06:00:00Z",
            },
        )
        .unwrap();

        assert_eq!(package.status, DeliveryPackageStatus::Validated);
    }

    #[test]
    fn dotted_package_id_is_checked_and_sent_with_matching_remote_name() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let client_dir = delivery_dir.join("client");
        let export_dir = delivery_dir.join("export");
        let package_path = export_dir.join("release.v1.zip");
        fs::create_dir_all(&client_dir).unwrap();
        fs::create_dir_all(&export_dir).unwrap();
        fs::write(client_dir.join("report.pdf"), "client report").unwrap();
        write_manifest(&delivery_dir, ["report.pdf"]);
        write_zip(&package_path, [("report.pdf", "client report")]);

        let checked = check_client_package(
            &conn,
            order_id,
            PackageCheckInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                package_id: Some("release.v1"),
                checked_at: "2026-05-27T06:00:00Z",
            },
        )
        .unwrap();
        let uploader = RecordingUploader::default();
        let sent = send_client_package(
            &conn,
            order_id,
            PackageSendInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                package_id: Some("release.v1"),
                sent_at: "2026-05-27T07:00:00Z",
            },
            &uploader,
        )
        .unwrap();

        assert_eq!(
            checked.package_path.as_deref(),
            Some(path_string(&package_path).as_str())
        );
        assert_eq!(
            sent.package.package_path.as_deref(),
            Some(path_string(&package_path).as_str())
        );
        assert_eq!(uploader.uploaded_paths(), vec![package_path]);
        assert!(uploader.uploaded_object_keys()[0]
            .as_deref()
            .unwrap()
            .ends_with("/release.v1.zip"));
    }

    #[test]
    fn send_requires_the_same_custom_package_id_that_was_checked() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let client_dir = delivery_dir.join("client");
        let export_dir = delivery_dir.join("export");
        fs::create_dir_all(&client_dir).unwrap();
        fs::create_dir_all(&export_dir).unwrap();
        fs::write(client_dir.join("report.pdf"), "client report").unwrap();
        write_manifest(&delivery_dir, ["report.pdf"]);
        write_zip(
            &export_dir.join("approved.zip"),
            [("report.pdf", "client report")],
        );
        write_zip(
            &export_dir.join("other.zip"),
            [("report.pdf", "client report")],
        );
        check_client_package(
            &conn,
            order_id,
            PackageCheckInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                package_id: Some("approved"),
                checked_at: "2026-05-27T06:00:00Z",
            },
        )
        .unwrap();
        let uploader = RecordingUploader::default();

        let err = send_client_package(
            &conn,
            order_id,
            PackageSendInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                package_id: Some("other"),
                sent_at: "2026-05-27T07:00:00Z",
            },
            &uploader,
        )
        .unwrap_err();

        assert!(err.to_string().contains("must be validated"));
        assert!(uploader.uploaded_paths().is_empty());
    }

    #[test]
    fn client_package_id_validation_rejects_unsafe_stems() {
        for id in [
            "",
            ".hidden",
            "../secret",
            "a..b",
            "folder/name",
            r"folder\name",
            "archive.zip",
            "archive.ZIP",
            "white space",
            "é",
            &"a".repeat(65),
        ] {
            assert!(
                client_package_filename(Some(id)).is_err(),
                "accepted {id:?}"
            );
        }
        for id in ["a", "A-1", "order_42", "2026.05.27"] {
            assert_eq!(
                client_package_filename(Some(id)).unwrap(),
                format!("{id}.zip")
            );
        }
        assert_eq!(client_package_filename(None).unwrap(), "client-package.zip");
    }

    #[test]
    fn send_client_package_requires_validated_package_before_upload() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let export_dir = delivery_dir.join("export");
        fs::create_dir_all(&export_dir).unwrap();
        write_required_delivery_docs(&delivery_dir);
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
        let uploader = RecordingUploader::default();

        let err = send_client_package(
            &conn,
            order_id,
            PackageSendInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                package_id: None,
                sent_at: "2026-05-27T07:00:00Z",
            },
            &uploader,
        )
        .unwrap_err();

        assert!(err.to_string().contains("gig package check"));
        assert!(uploader.uploaded_paths().is_empty());
        assert!(delivery_packages::list_for_order(&conn, order_id)
            .unwrap()
            .is_empty());
        assert!(delivery_artifacts::list_for_order(&conn, order_id)
            .unwrap()
            .is_empty());
        assert_eq!(
            orders::find_by_id(&conn, order_id).unwrap().status,
            OrderStatus::ReadyToDeliver
        );
    }

    #[test]
    fn send_client_package_uploads_zip_records_artifact_and_delivers_order() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let export_dir = delivery_dir.join("export");
        let package_path = export_dir.join("client-package.zip");
        fs::create_dir_all(&export_dir).unwrap();
        write_required_delivery_docs(&delivery_dir);
        write_zip(
            &package_path,
            [
                ("DELIVERY_CLIENT.html", "client html"),
                ("DELIVERY_CLIENT.pdf", "client pdf"),
            ],
        );
        write_manifest(
            &delivery_dir,
            ["DELIVERY_CLIENT.html", "DELIVERY_CLIENT.pdf"],
        );
        let validated = validate_delivery_package(&conn, order_id, &delivery_dir);
        let uploader = RecordingUploader::default();

        let sent = send_client_package(
            &conn,
            order_id,
            PackageSendInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                package_id: None,
                sent_at: "2026-05-27T07:00:00Z",
            },
            &uploader,
        )
        .unwrap();

        assert_eq!(uploader.uploaded_paths(), vec![package_path.clone()]);
        assert_eq!(
            uploader.uploaded_object_keys(),
            vec![Some(format!(
                "deliveries/{order_id}/2026-05-27/1779865200000000000-0-client-package.zip"
            ))]
        );
        assert_eq!(sent.package.order_id, order_id);
        assert_eq!(sent.package.status, DeliveryPackageStatus::Sent);
        assert_eq!(
            sent.package.package_path,
            Some(package_path.to_string_lossy().into_owned())
        );
        assert_eq!(sent.artifact.order_id, order_id);
        assert_eq!(
            sent.artifact.local_path,
            Some(package_path.to_string_lossy().into_owned())
        );
        assert_eq!(
            sent.artifact.uploader_name,
            Some("test:uploader".to_string())
        );
        assert_eq!(
            sent.artifact.remote_url,
            Some("https://example.test/client-package.zip".to_string())
        );
        assert_eq!(sent.artifact.expires_at, Some(1_780_000_000));
        assert_eq!(sent.artifact.uploaded_at, 1_779_865_200);
        assert_eq!(
            orders::find_by_id(&conn, order_id).unwrap().status,
            OrderStatus::Delivered
        );
        assert_eq!(
            delivery_packages::list_for_order(&conn, order_id).unwrap(),
            vec![sent.package, validated]
        );
        assert_eq!(
            delivery_artifacts::list_for_order(&conn, order_id).unwrap(),
            vec![sent.artifact]
        );
    }

    #[test]
    fn send_client_package_records_shortened_remote_url() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let export_dir = delivery_dir.join("export");
        let package_path = export_dir.join("client-package.zip");
        fs::create_dir_all(&export_dir).unwrap();
        write_required_delivery_docs(&delivery_dir);
        write_zip(
            &package_path,
            [
                ("DELIVERY_CLIENT.html", "client html"),
                ("DELIVERY_CLIENT.pdf", "client pdf"),
            ],
        );
        write_manifest(
            &delivery_dir,
            ["DELIVERY_CLIENT.html", "DELIVERY_CLIENT.pdf"],
        );
        validate_delivery_package(&conn, order_id, &delivery_dir);
        let uploader: Box<dyn Uploader> = Box::new(ShorteningUploader::new(
            Box::new(RecordingUploader::default()),
            Box::new(StaticShortLinker),
        ));

        let sent = send_client_package(
            &conn,
            order_id,
            PackageSendInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                package_id: None,
                sent_at: "2026-05-27T07:00:00Z",
            },
            uploader.as_ref(),
        )
        .unwrap();

        assert_eq!(
            sent.artifact.remote_url.as_deref(),
            Some("https://go.jczhang.cc/pkg12345")
        );
        assert_eq!(
            delivery_artifacts::list_for_order(&conn, order_id).unwrap()[0]
                .remote_url
                .as_deref(),
            Some("https://go.jczhang.cc/pkg12345")
        );
    }

    #[test]
    fn send_client_package_allows_revision_resend_and_delivers_order() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let export_dir = delivery_dir.join("export");
        let package_path = export_dir.join("client-package.zip");
        fs::create_dir_all(&export_dir).unwrap();
        write_required_delivery_docs(&delivery_dir);
        write_zip(
            &package_path,
            [
                ("DELIVERY_CLIENT.html", "client html"),
                ("DELIVERY_CLIENT.pdf", "client pdf"),
            ],
        );
        write_manifest(
            &delivery_dir,
            ["DELIVERY_CLIENT.html", "DELIVERY_CLIENT.pdf"],
        );
        validate_delivery_package(&conn, order_id, &delivery_dir);
        let uploader = RecordingUploader::default();

        let first_sent = send_client_package(
            &conn,
            order_id,
            PackageSendInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                package_id: None,
                sent_at: "2026-05-27T07:00:00Z",
            },
            &uploader,
        )
        .unwrap();

        order_service::transition(&conn, order_id, OrderStatus::Revision, 1_779_900_000).unwrap();

        let resent = send_client_package(
            &conn,
            order_id,
            PackageSendInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                package_id: None,
                sent_at: "2026-05-28T07:00:00Z",
            },
            &uploader,
        )
        .unwrap();

        let first_key =
            format!("deliveries/{order_id}/2026-05-27/1779865200000000000-0-client-package.zip");
        let resend_key =
            format!("deliveries/{order_id}/2026-05-27/1779951600000000000-1-client-package.zip");
        assert_ne!(first_key, resend_key);
        assert_eq!(
            uploader.uploaded_paths(),
            vec![package_path.clone(), package_path]
        );
        assert_eq!(
            uploader.uploaded_object_keys(),
            vec![Some(first_key), Some(resend_key)]
        );
        assert_eq!(first_sent.package.status, DeliveryPackageStatus::Sent);
        assert_eq!(resent.package.status, DeliveryPackageStatus::Sent);
        assert_eq!(
            orders::find_by_id(&conn, order_id).unwrap().status,
            OrderStatus::Delivered
        );
    }

    #[test]
    fn send_client_package_same_second_resend_uses_distinct_object_keys() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let export_dir = delivery_dir.join("export");
        let package_path = export_dir.join("client-package.zip");
        fs::create_dir_all(&export_dir).unwrap();
        write_required_delivery_docs(&delivery_dir);
        write_zip(
            &package_path,
            [
                ("DELIVERY_CLIENT.html", "client html"),
                ("DELIVERY_CLIENT.pdf", "client pdf"),
            ],
        );
        write_manifest(
            &delivery_dir,
            ["DELIVERY_CLIENT.html", "DELIVERY_CLIENT.pdf"],
        );
        validate_delivery_package(&conn, order_id, &delivery_dir);
        let uploader = RecordingUploader::default();

        send_client_package(
            &conn,
            order_id,
            PackageSendInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                package_id: None,
                sent_at: "2026-05-27T07:00:00.000000001Z",
            },
            &uploader,
        )
        .unwrap();

        order_service::transition(&conn, order_id, OrderStatus::Revision, 1_779_900_000).unwrap();

        send_client_package(
            &conn,
            order_id,
            PackageSendInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                package_id: None,
                sent_at: "2026-05-27T07:00:00.000000002Z",
            },
            &uploader,
        )
        .unwrap();

        let object_keys = uploader.uploaded_object_keys();
        assert_eq!(object_keys.len(), 2);
        assert_ne!(object_keys[0], object_keys[1]);
    }

    #[test]
    fn send_client_package_upload_failure_leaves_no_package_record() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let export_dir = delivery_dir.join("export");
        let package_path = export_dir.join("client-package.zip");
        fs::create_dir_all(&export_dir).unwrap();
        write_required_delivery_docs(&delivery_dir);
        write_zip(
            &package_path,
            [
                ("DELIVERY_CLIENT.html", "client html"),
                ("DELIVERY_CLIENT.pdf", "client pdf"),
            ],
        );
        write_manifest(
            &delivery_dir,
            ["DELIVERY_CLIENT.html", "DELIVERY_CLIENT.pdf"],
        );
        let validated = validate_delivery_package(&conn, order_id, &delivery_dir);
        let uploader = FailingUploader::default();

        let err = send_client_package(
            &conn,
            order_id,
            PackageSendInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                package_id: None,
                sent_at: "2026-05-27T07:00:00Z",
            },
            &uploader,
        )
        .unwrap_err();

        assert!(err.to_string().contains("upload failed"));
        assert_eq!(uploader.uploaded_paths(), vec![package_path]);
        assert_eq!(
            delivery_packages::list_for_order(&conn, order_id).unwrap(),
            vec![validated]
        );
        assert!(delivery_artifacts::list_for_order(&conn, order_id)
            .unwrap()
            .is_empty());
        assert_eq!(
            orders::find_by_id(&conn, order_id).unwrap().status,
            OrderStatus::ReadyToDeliver
        );
    }

    #[test]
    fn check_client_package_rejects_empty_or_duplicate_manifests() {
        for client_files in [vec![], vec!["report.pdf", "report.pdf"]] {
            let conn = open_in_memory().unwrap();
            let order_id = ready_order(&conn);
            let root = tempfile::tempdir().unwrap();
            let delivery_dir = delivery_layout(root.path());
            let client_dir = delivery_dir.join("client");
            fs::create_dir_all(&client_dir).unwrap();
            fs::write(client_dir.join("report.pdf"), "client report").unwrap();
            let entries = client_files
                .iter()
                .map(|path| format!("\"{path}\""))
                .collect::<Vec<_>>()
                .join(", ");
            fs::write(
                delivery_dir.join("manifest.toml"),
                format!(
                    "version = 1\ndelivery_date = \"2026-05-27\"\nclient_files = [{entries}]\n"
                ),
            )
            .unwrap();

            let err = check_client_package(
                &conn,
                order_id,
                PackageCheckInput {
                    delivery_date: "2026-05-27",
                    delivery_dir: &delivery_dir,
                    package_id: None,
                    checked_at: "2026-05-27T06:00:00Z",
                },
            )
            .unwrap_err();

            assert!(
                err.to_string().contains("at least one") || err.to_string().contains("duplicate")
            );
            assert!(delivery_packages::list_for_order(&conn, order_id)
                .unwrap()
                .is_empty());
        }
    }

    #[test]
    fn check_client_package_rejects_unsafe_manifest_paths_without_recording() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        write_required_delivery_docs(&delivery_dir);
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
                package_id: None,
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
        fs::create_dir_all(delivery_dir.join("internal")).unwrap();
        fs::write(delivery_dir.join("DELIVERY.md"), "delivery source").unwrap();
        fs::write(
            delivery_dir.join("internal").join("DELIVERY_INTERNAL.html"),
            "internal delivery html",
        )
        .unwrap();
        fs::write(client_dir.join("DELIVERY_CLIENT.html"), "client html").unwrap();
        let secret = root.path().join("secret.pdf");
        fs::write(&secret, "not a package file").unwrap();
        std::os::unix::fs::symlink(&secret, client_dir.join("DELIVERY_CLIENT.pdf")).unwrap();
        write_manifest(
            &delivery_dir,
            ["DELIVERY_CLIENT.html", "DELIVERY_CLIENT.pdf"],
        );
        write_zip(
            &export_dir.join("client-package.zip"),
            [
                ("DELIVERY_CLIENT.html", "client html"),
                ("DELIVERY_CLIENT.pdf", "client pdf"),
            ],
        );

        let err = check_client_package(
            &conn,
            order_id,
            PackageCheckInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                package_id: None,
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
                package_id: None,
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
    fn check_client_package_rejects_symlinked_export_dir_without_recording() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let export_dir = delivery_dir.join("export");
        let outside_export_dir = root.path().join("outside-export");
        fs::create_dir_all(&outside_export_dir).unwrap();
        write_required_delivery_docs(&delivery_dir);
        std::os::unix::fs::symlink(&outside_export_dir, &export_dir).unwrap();
        write_manifest(
            &delivery_dir,
            ["DELIVERY_CLIENT.html", "DELIVERY_CLIENT.pdf"],
        );
        write_zip(
            &outside_export_dir.join("client-package.zip"),
            [
                ("DELIVERY_CLIENT.html", "client html"),
                ("DELIVERY_CLIENT.pdf", "client pdf"),
            ],
        );

        let err = check_client_package(
            &conn,
            order_id,
            PackageCheckInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                package_id: None,
                checked_at: "2026-05-27T06:00:00Z",
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("delivery dir"));
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
        let export_dir = delivery_dir.join("export");
        fs::create_dir_all(&export_dir).unwrap();
        write_required_delivery_docs(&delivery_dir);
        write_manifest(
            &delivery_dir,
            ["DELIVERY_CLIENT.html", "DELIVERY_CLIENT.pdf"],
        );
        write_zip(
            &export_dir.join("client-package.zip"),
            [
                ("DELIVERY_CLIENT.html", "client html"),
                ("DELIVERY_CLIENT.pdf", "client pdf"),
                ("../secret.txt", "secret"),
            ],
        );

        let err = check_client_package(
            &conn,
            order_id,
            PackageCheckInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                package_id: None,
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
        let export_dir = delivery_dir.join("export");
        fs::create_dir_all(&export_dir).unwrap();
        write_required_delivery_docs(&delivery_dir);
        write_manifest(
            &delivery_dir,
            ["DELIVERY_CLIENT.html", "DELIVERY_CLIENT.pdf"],
        );
        write_zip(
            &export_dir.join("client-package.zip"),
            [
                ("DELIVERY_CLIENT.html", "client html"),
                ("DELIVERY_CLIENT.pdf", "client pdf"),
                ("secret.txt", "secret"),
            ],
        );

        let err = check_client_package(
            &conn,
            order_id,
            PackageCheckInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                package_id: None,
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
    fn check_client_package_rejects_duplicate_zip_entries_without_recording() {
        let conn = open_in_memory().unwrap();
        let order_id = ready_order(&conn);
        let root = tempfile::tempdir().unwrap();
        let delivery_dir = delivery_layout(root.path());
        let export_dir = delivery_dir.join("export");
        fs::create_dir_all(&export_dir).unwrap();
        write_required_delivery_docs(&delivery_dir);
        write_manifest(&delivery_dir, ["DELIVERY_CLIENT.html"]);
        write_zip(
            &export_dir.join("client-package.zip"),
            [
                ("DELIVERY_CLIENT.html", "first"),
                ("DELIVERY_CLIENT.html", "second"),
            ],
        );

        let err = check_client_package(
            &conn,
            order_id,
            PackageCheckInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                package_id: None,
                checked_at: "2026-05-27T06:00:00Z",
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("duplicate package zip entry"));
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
        let export_dir = delivery_dir.join("export");
        fs::create_dir_all(&export_dir).unwrap();
        write_required_delivery_docs(&delivery_dir);
        write_manifest(
            &delivery_dir,
            ["DELIVERY_CLIENT.html", "DELIVERY_CLIENT.pdf"],
        );
        write_zip(
            &export_dir.join("client-package.zip"),
            [
                ("DELIVERY_CLIENT.html", "client html"),
                ("DELIVERY_CLIENT.pdf", "client pdf"),
                (r"internal\notes.md", "secret"),
            ],
        );

        let err = check_client_package(
            &conn,
            order_id,
            PackageCheckInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                package_id: None,
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
        let package_path = delivery_dir.join("export").join("client-package.zip");
        write_required_delivery_docs(&delivery_dir);
        write_manifest(
            &delivery_dir,
            ["DELIVERY_CLIENT.html", "DELIVERY_CLIENT.pdf"],
        );

        let err = check_client_package(
            &conn,
            order_id,
            PackageCheckInput {
                delivery_date: "2026-05-27",
                delivery_dir: &delivery_dir,
                package_id: None,
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
}
