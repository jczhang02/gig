use crate::server::{random_token, require_localhost, shutdown_signal};
use axum::Router;
use gig_core::{Error, Result};
use std::io::{self, Write};
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticServeOptions {
    /// Directory exposed below the random URL prefix.
    pub root_dir: PathBuf,
    /// Initial artifact URL to print for the caller.
    pub index_path: PathBuf,
    /// Localhost port. Use 0 to let the OS assign a free port.
    pub port: u16,
    /// Open the printed artifact URL in the default browser.
    pub open: bool,
}

pub fn serve_static(options: StaticServeOptions) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| Error::Invalid(format!("failed to start static server runtime: {err}")))?;
    runtime.block_on(serve_static_async(options))
}

async fn serve_static_async(options: StaticServeOptions) -> Result<()> {
    let root_dir = canonicalize_existing_dir(&options.root_dir)?;
    let index_path = canonicalize_existing_file(&options.index_path)?;
    let relative_index = index_path.strip_prefix(&root_dir).map_err(|_| {
        Error::Invalid(format!(
            "artifact path {} is not inside served directory {}",
            index_path.display(),
            root_dir.display()
        ))
    })?;

    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, options.port))
        .await
        .map_err(|err| Error::Invalid(format!("failed to bind gig serve to localhost: {err}")))?;
    let addr = listener
        .local_addr()
        .map_err(|err| Error::Invalid(format!("failed to read serve address: {err}")))?;
    require_localhost(addr)?;

    let token = random_token();
    let route_prefix = format!("/{token}");
    let app = Router::new()
        .nest_service(
            &route_prefix,
            ServeDir::new(&root_dir).append_index_html_on_directories(false),
        )
        .layer(TraceLayer::new_for_http());

    let relative_url_path = path_for_url(relative_index);
    let url = format!(
        "http://127.0.0.1:{}/{}/{}",
        addr.port(),
        token,
        relative_url_path
    );

    println!("gig serve listening on http://127.0.0.1:{}", addr.port());
    println!("URL : {url}");
    println!("Path: {}", index_path.display());
    io::stdout().flush().ok();

    if options.open {
        if let Err(err) = webbrowser::open(&url) {
            eprintln!("warning: failed to open browser: {err}");
        }
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|err| Error::Invalid(format!("gig serve server failed: {err}")))
}

fn canonicalize_existing_dir(path: &Path) -> Result<PathBuf> {
    let canonical = path
        .canonicalize()
        .map_err(|err| Error::PathUnavailable(path.to_path_buf(), err))?;
    if canonical.is_dir() {
        Ok(canonical)
    } else {
        Err(Error::Invalid(format!(
            "serve root is not a directory: {}",
            canonical.display()
        )))
    }
}

fn canonicalize_existing_file(path: &Path) -> Result<PathBuf> {
    let canonical = path
        .canonicalize()
        .map_err(|err| Error::PathUnavailable(path.to_path_buf(), err))?;
    if canonical.is_file() {
        Ok(canonical)
    } else {
        Err(Error::Invalid(format!(
            "artifact path is not a file: {}",
            canonical.display()
        )))
    }
}

fn path_for_url(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
