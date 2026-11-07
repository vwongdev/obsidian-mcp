mod auth;
mod config;
mod server;
#[expect(dead_code, reason = "used by the note tools")]
mod vault;

use std::sync::Arc;

use anyhow::Result;
use axum::{Router, middleware, routing::get};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use tokio::{net::TcpListener, signal};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

use crate::{
    auth::{Auth, protected_resource_metadata, require_token},
    config::Config,
    server::ObsidianServer,
};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = Config::from_env()?;
    let allowed_hosts = config.allowed_hosts();
    let auth = Arc::new(Auth::discover(config.auth).await?);
    let shutdown = CancellationToken::new();
    let app = router(allowed_hosts, auth, shutdown.child_token());

    let listener = TcpListener::bind(config.listen_addr).await?;
    tracing::info!("listening on {}", config.listen_addr);
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            shutdown.cancel();
        })
        .await?;
    Ok(())
}

fn router(allowed_hosts: Vec<String>, auth: Arc<Auth>, shutdown: CancellationToken) -> Router {
    let mcp = StreamableHttpService::new(
        || Ok(ObsidianServer),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default()
            .with_cancellation_token(shutdown)
            .with_allowed_hosts(allowed_hosts),
    );
    Router::new()
        .route_service("/", mcp)
        .route_layer(middleware::from_fn_with_state(auth.clone(), require_token))
        .route("/healthz", get(|| async { "ok" }))
        .route(
            "/.well-known/oauth-protected-resource",
            get(protected_resource_metadata),
        )
        .with_state(auth)
}

async fn shutdown_signal() {
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    tokio::select! {
        _ = signal::ctrl_c() => {}
        _ = terminate => {}
    }
}
