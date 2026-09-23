//! infra-plot server: persists diagrams on disk and serves the web client.
//!
//! All rendering happens in the browser; this process only stores, validates and
//! converts documents between JSON and TOML.

mod api;
mod store;

use std::{net::SocketAddr, path::PathBuf};

use anyhow::Context;
use axum::Router;
use clap::Parser;
use tower_http::{
    compression::CompressionLayer,
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Address to listen on.
    #[arg(long, env = "INFRAPLOT_BIND", default_value = "127.0.0.1:31080")]
    bind: SocketAddr,
    /// Directory where diagrams are stored.
    #[arg(long, env = "INFRAPLOT_DATA_DIR", default_value = "./data")]
    data_dir: PathBuf,
    /// Built web client (`web/dist`). When absent only the API is served.
    #[arg(long, env = "INFRAPLOT_STATIC_DIR")]
    static_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let store = store::Store::open(&args.data_dir)
        .await
        .with_context(|| format!("opening data dir {}", args.data_dir.display()))?;

    let mut app = Router::new().nest("/api", api::router(store));
    match &args.static_dir {
        Some(dir) if dir.join("index.html").is_file() => {
            tracing::info!(dir = %dir.display(), "serving web client");
            let spa = ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html")));
            app = app.fallback_service(spa);
        }
        Some(dir) => {
            tracing::warn!(dir = %dir.display(), "static dir has no index.html, serving API only");
        }
        None => tracing::info!("no static dir configured, serving API only"),
    }
    let app = app
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(args.bind)
        .await
        .with_context(|| format!("binding {}", args.bind))?;
    tracing::info!("listening on http://{}", listener.local_addr()?);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = term => {},
    }
}
