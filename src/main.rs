mod config;
mod crds;
mod error;
mod health;
mod leader;
mod reconciler;
mod resources;

use error::OperatorError;
use tokio::signal::unix::{SignalKind, signal};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), OperatorError> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .compact()
        .init();

    let config = config::OperatorConfig::from_env()?;
    let client = kube::Client::try_default().await?;

    let shutdown = CancellationToken::new();
    spawn_signal_handler(shutdown.clone());

    let health_port = config.health_port;
    let health_shutdown = shutdown.clone();
    tokio::spawn(async move {
        if let Err(err) = health::serve(health_port, health_shutdown).await {
            tracing::error!(error = %err, "health server failed");
        }
    });

    leader::run(client, config, shutdown).await
}

fn spawn_signal_handler(token: CancellationToken) {
    tokio::spawn(async move {
        let mut term = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(err) => {
                tracing::error!(error = %err, "failed to install SIGTERM handler");
                return;
            }
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => tracing::info!("received SIGINT"),
            _ = term.recv() => tracing::info!("received SIGTERM"),
        }
        token.cancel();
    });
}
