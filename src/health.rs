use axum::{Router, http::StatusCode, routing::get};
use tokio_util::sync::CancellationToken;

pub async fn serve(port: u16, shutdown: CancellationToken) -> crate::error::Result<()> {
    let app = Router::new()
        .route("/healthz", get(ok))
        .route("/readyz", get(ok));

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "health server listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async move { shutdown.cancelled().await })
        .await?;
    Ok(())
}

async fn ok() -> StatusCode {
    StatusCode::OK
}
