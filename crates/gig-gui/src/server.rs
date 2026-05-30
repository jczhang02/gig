use crate::api::{router, AppState};
use gig_core::config::{Config, Paths};
use gig_core::{Error, Result};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use std::net::{Ipv4Addr, SocketAddr};
use tower_http::trace::TraceLayer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuiOptions {
    /// Localhost port. Use 0 to let the OS assign a free port.
    pub port: u16,
    /// Print the URL without launching a browser.
    pub no_open: bool,
}

impl Default for GuiOptions {
    fn default() -> Self {
        Self {
            port: 0,
            no_open: false,
        }
    }
}

pub fn run(options: GuiOptions) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| Error::Invalid(format!("failed to start GUI runtime: {err}")))?;
    runtime.block_on(run_async(options))
}

async fn run_async(options: GuiOptions) -> Result<()> {
    let paths = Paths::from_env()?;
    paths.ensure_dirs()?;
    let config = Config::load_or_default(&paths.config_file)?;
    let token = random_token();

    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, options.port))
        .await
        .map_err(|err| Error::Invalid(format!("failed to bind gig gui to localhost: {err}")))?;
    let addr = listener
        .local_addr()
        .map_err(|err| Error::Invalid(format!("failed to read GUI address: {err}")))?;
    require_localhost(addr)?;

    let app = router(AppState {
        paths,
        config,
        token: token.clone(),
    })
    .layer(TraceLayer::new_for_http());

    let url = format!("http://127.0.0.1:{}/?token={token}", addr.port());
    println!("gig gui listening on http://127.0.0.1:{}", addr.port());
    println!("open: {url}");

    if !options.no_open {
        if let Err(err) = webbrowser::open(&url) {
            eprintln!("warning: failed to open browser: {err}");
        }
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|err| Error::Invalid(format!("gig gui server failed: {err}")))
}

fn require_localhost(addr: SocketAddr) -> Result<()> {
    if addr.ip().is_loopback() {
        Ok(())
    } else {
        Err(Error::Invalid(format!(
            "gig gui must bind to localhost, got {addr}"
        )))
    }
}

fn random_token() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv6Addr};

    #[test]
    fn token_has_enough_entropy_surface() {
        let token = random_token();
        assert_eq!(token.len(), 32);
        assert!(token.chars().all(|ch| ch.is_ascii_alphanumeric()));
    }

    #[test]
    fn localhost_guard_rejects_non_loopback() {
        let external = SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 8080);
        assert!(require_localhost(external).is_err());
    }
}
