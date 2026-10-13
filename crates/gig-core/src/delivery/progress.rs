//! Progress for uploads the services run: the service functions build their
//! own `UploadOpts`, so a progress callback rides in on a decorating
//! `Uploader` instead of a new argument. The TUI and the CLI both use it.

use super::{Progress, UploadOpts, UploadResult, Uploader};
use crate::Result;
use std::path::Path;
use std::sync::Arc;

/// Puts a progress callback into the options the service builds.
pub struct WithProgress<'a> {
    inner: &'a dyn Uploader,
    progress: Progress,
}

impl<'a> WithProgress<'a> {
    pub fn new(inner: &'a dyn Uploader, progress: Progress) -> Self {
        Self { inner, progress }
    }
}

impl Uploader for WithProgress<'_> {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn upload(&self, local: &Path, opts: &UploadOpts) -> Result<UploadResult> {
        let mut opts = opts.clone();
        opts.progress = Some(Arc::clone(&self.progress));
        self.inner.upload(local, &opts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::delivery::{Progress, UploadOpts, UploadResult, Uploader};
    use crate::Result;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    /// Reports `(1, 2)` and `(2, 2)` through whatever callback it gets and
    /// records the object key it was asked for.
    #[derive(Default)]
    struct Reporting {
        key: Mutex<Option<String>>,
    }

    impl Uploader for Reporting {
        fn name(&self) -> &str {
            "reporting"
        }

        fn upload(&self, _local: &Path, opts: &UploadOpts) -> Result<UploadResult> {
            *self.key.lock().unwrap() = opts.object_key.clone();
            opts.report(1, 2);
            opts.report(2, 2);
            Ok(UploadResult {
                url: "https://example.test/x".into(),
                short_url: None,
                expires_at: None,
                provider: "reporting".into(),
                file_size: 2,
                pwd: None,
            })
        }
    }

    #[test]
    fn passes_the_callback_to_the_inner_uploader() {
        let inner = Reporting::default();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let progress: Progress = Arc::new(move |a, b| sink.lock().unwrap().push((a, b)));
        let wrapped = WithProgress::new(&inner, progress);
        assert_eq!(wrapped.name(), "reporting");
        let opts = UploadOpts {
            object_key: Some("gig/a/a-v1.zip".into()),
            progress: None,
        };
        wrapped.upload(Path::new("/tmp/a-v1.zip"), &opts).unwrap();
        assert_eq!(*seen.lock().unwrap(), vec![(1, 2), (2, 2)]);
        assert_eq!(inner.key.lock().unwrap().as_deref(), Some("gig/a/a-v1.zip"));
    }
}
