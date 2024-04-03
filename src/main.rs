use anyhow::Context;
use axum::routing::get;
use axum::Router;
use tracing::info;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use crate::config::meta::MetaOverrideCache;
use crate::config::{get_config_routes, FederationConfigCache};
use crate::federation::{get_federations_routes, FederationObserver};

/// Fedimint config fetching service implementation
mod config;
/// `anyhow`-based error handling for axum
mod error;
mod federation;

#[derive(Debug, Clone)]
struct AppState {
    federation_config_cache: FederationConfigCache,
    meta_override_cache: MetaOverrideCache,
    federation_observer: FederationObserver,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(
            EnvFilter::builder()
                .with_default_directive("info".parse().unwrap())
                .from_env()
                .unwrap(),
        )
        .init();

    let bind_address = dotenv::var("FO_BIND").unwrap_or_else(|_| "127.0.0.1:3000".to_owned());
    info!("Starting API server on {bind_address}");

    let app = Router::new()
        .route("/health", get(|| async { "Server is up and running!" }))
        .nest("/config", get_config_routes())
        .nest("/federations", get_federations_routes())
        .with_state(AppState {
            federation_config_cache: Default::default(),
            meta_override_cache: Default::default(),
            federation_observer: FederationObserver::new(
                &dotenv::var("FO_DATABASE")
                    .unwrap_or_else(|_| "sqlite://fedimint_observer.db".to_owned()),
                &dotenv::var("FO_ADMIN_AUTH").context("No FO_ADMIN_AUTH provided")?,
            )
            .await?,
        });

    let listener = tokio::net::TcpListener::bind(bind_address)
        .await
        .context("Binding to port")?;

    axum::serve(listener, app)
        .await
        .context("Starting axum server")?;

    Ok(())
}
