use crate::delivery::{UploadOpts, Uploader};
use crate::models::DeliveryArtifact;
use crate::repo::{delivery_artifacts, orders};
use crate::{Error, Result};
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};
use time::OffsetDateTime;

#[derive(Debug, Clone, Copy)]
pub struct ArtifactSendInput<'a> {
    pub files: &'a [PathBuf],
    pub uploaded_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactSendResult {
    pub artifacts: Vec<DeliveryArtifact>,
}

pub fn send_order_artifacts(
    conn: &Connection,
    order_id: i64,
    input: ArtifactSendInput<'_>,
    uploader: &dyn Uploader,
) -> Result<ArtifactSendResult> {
    orders::find_by_id(conn, order_id)?;
    for file in input.files {
        require_upload_file(file)?;
    }

    let uploaded_at = input.uploaded_at.unix_timestamp();
    let object_key_timestamp = input.uploaded_at.unix_timestamp_nanos();
    let send_attempt = delivery_artifacts::list_for_order(conn, order_id)?.len();
    let mut uploaded = Vec::with_capacity(input.files.len());
    for (index, file) in input.files.iter().enumerate() {
        let upload_result = uploader.upload(
            file,
            &UploadOpts {
                link_ttl_days: None,
                object_key: Some(artifact_object_key(
                    order_id,
                    object_key_timestamp,
                    send_attempt,
                    index,
                    file,
                )?),
            },
        )?;
        uploaded.push((file, upload_result));
    }

    let tx = conn.unchecked_transaction()?;
    let mut artifacts = Vec::with_capacity(uploaded.len());
    for (file, upload_result) in uploaded {
        artifacts.push(delivery_artifacts::insert(
            &tx,
            order_id,
            Some(&path_string(file)),
            Some(uploader.name()),
            Some(&upload_result.url),
            upload_result.expires_at,
            uploaded_at,
        )?);
    }
    tx.commit()?;

    Ok(ArtifactSendResult { artifacts })
}

fn require_upload_file(path: &Path) -> Result<()> {
    let file_type = fs::symlink_metadata(path)
        .map_err(|err| {
            Error::Invalid(format!(
                "missing upload artifact: {} ({err})",
                path.display()
            ))
        })?
        .file_type();
    if file_type.is_symlink() || !file_type.is_file() {
        return Err(Error::Invalid(format!(
            "missing upload artifact: {}",
            path.display()
        )));
    }
    Ok(())
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn artifact_object_key(
    order_id: i64,
    uploaded_at: i128,
    send_attempt: usize,
    index: usize,
    file: &Path,
) -> Result<String> {
    let basename = file
        .file_name()
        .ok_or_else(|| {
            Error::Invalid(format!(
                "upload artifact has no filename: {}",
                file.display()
            ))
        })?
        .to_string_lossy();
    Ok(format!(
        "orders/{order_id}/artifacts/{uploaded_at}/{send_attempt}-{index}-{basename}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::delivery::{ShortLinker, ShorteningUploader, UploadOpts, UploadResult, Uploader};
    use crate::models::{OrderStatus, ProjectType};
    use crate::repo::delivery_artifacts;
    use crate::repo::orders::{self, NewOrder};
    use crate::Result;
    use std::fs;
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
    struct FailingSecondUploader {
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
                url: format!(
                    "https://example.test/{}",
                    local.file_name().unwrap().to_string_lossy()
                ),
                expires_at: Some(1_780_000_000),
                provider: self.name().to_string(),
                file_size: fs::metadata(local).unwrap().len(),
            })
        }
    }

    impl FailingSecondUploader {
        fn uploaded_paths(&self) -> Vec<PathBuf> {
            self.upload_calls
                .lock()
                .unwrap()
                .iter()
                .map(|call| call.local.clone())
                .collect()
        }
    }

    impl Uploader for FailingSecondUploader {
        fn name(&self) -> &str {
            "test:failing"
        }

        fn upload(&self, local: &Path, opts: &UploadOpts) -> Result<UploadResult> {
            assert_eq!(opts.link_ttl_days, None);
            let mut upload_calls = self.upload_calls.lock().unwrap();
            upload_calls.push(UploadCall {
                local: local.to_path_buf(),
                object_key: opts.object_key.clone(),
            });
            if upload_calls.len() == 2 {
                return Err(Error::Invalid("upload failed".to_string()));
            }
            Ok(UploadResult {
                url: format!(
                    "https://example.test/{}",
                    local.file_name().unwrap().to_string_lossy()
                ),
                expires_at: Some(1_780_000_000),
                provider: self.name().to_string(),
                file_size: fs::metadata(local).unwrap().len(),
            })
        }
    }

    impl ShortLinker for StaticShortLinker {
        fn shorten(&self, _long_url: &str, _ttl_seconds: Option<u32>) -> Result<String> {
            Ok("https://go.jczhang.cc/art12345".to_string())
        }
    }

    fn seed_order(conn: &rusqlite::Connection) -> i64 {
        orders::insert(
            conn,
            &NewOrder {
                slug: Some("artifact-order"),
                external_id: None,
                title: "Artifact order",
                client_id: None,
                source_org: None,
                source_id: None,
                project_type: Some(ProjectType::Crawler),
                status: OrderStatus::Accepted,
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

    fn write_temp_file(name: &str, contents: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "gig-artifact-{name}-{}-{}",
            std::process::id(),
            time::OffsetDateTime::now_utc().unix_timestamp_nanos()
        ));
        fs::write(&path, contents).unwrap();
        path
    }

    fn uploaded_at() -> time::OffsetDateTime {
        time::OffsetDateTime::from_unix_timestamp(1_700_000_123).unwrap()
    }

    #[test]
    fn send_order_artifacts_uploads_each_file_records_artifacts_and_preserves_order_status() {
        let conn = open_in_memory().unwrap();
        let order_id = seed_order(&conn);
        let first = write_temp_file("one.txt", "one");
        let second = write_temp_file("two.txt", "two");
        let files = vec![first.clone(), second.clone()];
        let uploader = RecordingUploader::default();

        let result = send_order_artifacts(
            &conn,
            order_id,
            ArtifactSendInput {
                files: &files,
                uploaded_at: uploaded_at(),
            },
            &uploader,
        )
        .unwrap();

        assert_eq!(uploader.uploaded_paths(), files);
        let first_key = format!(
            "orders/{order_id}/artifacts/1700000123000000000/0-0-{}",
            first.file_name().unwrap().to_string_lossy()
        );
        let second_key = format!(
            "orders/{order_id}/artifacts/1700000123000000000/0-1-{}",
            second.file_name().unwrap().to_string_lossy()
        );
        assert_ne!(first_key, second_key);
        assert_eq!(
            uploader.uploaded_object_keys(),
            vec![Some(first_key), Some(second_key)]
        );
        assert_eq!(result.artifacts.len(), 2);
        assert_eq!(result.artifacts[0].order_id, order_id);
        assert_eq!(
            result.artifacts[0].local_path.as_deref(),
            Some(first.to_str().unwrap())
        );
        assert_eq!(
            result.artifacts[0].uploader_name.as_deref(),
            Some("test:uploader")
        );
        assert!(result.artifacts[0]
            .remote_url
            .as_deref()
            .unwrap()
            .contains("gig-artifact-one.txt"));
        assert_eq!(result.artifacts[0].expires_at, Some(1_780_000_000));
        assert_eq!(result.artifacts[0].uploaded_at, 1_700_000_123);
        assert_eq!(
            delivery_artifacts::list_for_order(&conn, order_id)
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            orders::find_by_id(&conn, order_id).unwrap().status,
            OrderStatus::Accepted
        );
    }

    #[test]
    fn send_order_artifacts_records_shortened_remote_urls() {
        let conn = open_in_memory().unwrap();
        let order_id = seed_order(&conn);
        let artifact = write_temp_file("short.txt", "short");
        let files = vec![artifact];
        let uploader: Box<dyn Uploader> = Box::new(ShorteningUploader::new(
            Box::new(RecordingUploader::default()),
            Box::new(StaticShortLinker),
        ));

        let result = send_order_artifacts(
            &conn,
            order_id,
            ArtifactSendInput {
                files: &files,
                uploaded_at: uploaded_at(),
            },
            uploader.as_ref(),
        )
        .unwrap();

        assert_eq!(
            result.artifacts[0].remote_url.as_deref(),
            Some("https://go.jczhang.cc/art12345")
        );
        assert_eq!(
            delivery_artifacts::list_for_order(&conn, order_id).unwrap()[0]
                .remote_url
                .as_deref(),
            Some("https://go.jczhang.cc/art12345")
        );
    }

    #[test]
    fn send_order_artifacts_same_second_repeat_uses_distinct_object_keys() {
        let conn = open_in_memory().unwrap();
        let order_id = seed_order(&conn);
        let artifact = write_temp_file("repeat.txt", "repeat");
        let files = vec![artifact];
        let uploader = RecordingUploader::default();

        send_order_artifacts(
            &conn,
            order_id,
            ArtifactSendInput {
                files: &files,
                uploaded_at: uploaded_at(),
            },
            &uploader,
        )
        .unwrap();
        send_order_artifacts(
            &conn,
            order_id,
            ArtifactSendInput {
                files: &files,
                uploaded_at: uploaded_at(),
            },
            &uploader,
        )
        .unwrap();

        let object_keys = uploader.uploaded_object_keys();
        assert_eq!(object_keys.len(), 2);
        assert_ne!(object_keys[0], object_keys[1]);
    }

    #[test]
    fn send_order_artifacts_rejects_missing_file_before_uploading() {
        let conn = open_in_memory().unwrap();
        let order_id = seed_order(&conn);
        let missing = std::env::temp_dir().join(format!(
            "gig-artifact-missing-{}-{}",
            std::process::id(),
            time::OffsetDateTime::now_utc().unix_timestamp_nanos()
        ));
        let files = vec![missing.clone()];
        let uploader = RecordingUploader::default();

        let err = send_order_artifacts(
            &conn,
            order_id,
            ArtifactSendInput {
                files: &files,
                uploaded_at: uploaded_at(),
            },
            &uploader,
        )
        .unwrap_err();

        assert!(err.to_string().contains("missing upload artifact"));
        assert!(uploader.uploaded_paths().is_empty());
        assert!(delivery_artifacts::list_for_order(&conn, order_id)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn send_order_artifacts_upload_failure_records_no_partial_artifacts() {
        let conn = open_in_memory().unwrap();
        let order_id = seed_order(&conn);
        let first = write_temp_file("one.txt", "one");
        let second = write_temp_file("two.txt", "two");
        let files = vec![first.clone(), second.clone()];
        let uploader = FailingSecondUploader::default();

        let err = send_order_artifacts(
            &conn,
            order_id,
            ArtifactSendInput {
                files: &files,
                uploaded_at: uploaded_at(),
            },
            &uploader,
        )
        .unwrap_err();

        assert!(err.to_string().contains("upload failed"));
        assert_eq!(uploader.uploaded_paths(), files);
        assert!(delivery_artifacts::list_for_order(&conn, order_id)
            .unwrap()
            .is_empty());
    }
}
