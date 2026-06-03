use crate::{config::OperatorConfig, error::OperatorError, reconciler};
use kube::Client;
use kube_lease_manager::LeaseManagerBuilder;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

pub async fn run(
    client: Client,
    config: OperatorConfig,
    shutdown: CancellationToken,
) -> Result<(), OperatorError> {
    let manager = LeaseManagerBuilder::new(client.clone(), config.lease_name.clone())
        .with_namespace(config.lease_namespace.clone())
        .with_identity(config.holder_id.clone())
        .with_duration(config.lease_duration_seconds)
        .with_grace(config.lease_grace_seconds)
        .build()
        .await?;

    let (mut state, lease_task) = manager.watch().await;
    let mut current: Option<(CancellationToken, JoinHandle<Result<(), OperatorError>>)> = None;

    loop {
        let reconciler_done = async {
            match current.as_mut() {
                Some((_, handle)) => (&mut *handle).await,
                None => std::future::pending().await,
            }
        };

        tokio::select! {
            _ = shutdown.cancelled() => break,
            res = reconciler_done => {
                warn!(result = ?res, "reconciler task exited unexpectedly, releasing lease");
                shutdown.cancel();
                break;
            }
            changed = state.changed() => {
                if changed.is_err() {
                    warn!("lease manager watch channel closed");
                    shutdown.cancel();
                    break;
                }
                let is_leader = *state.borrow_and_update();
                if is_leader && current.is_none() {
                    info!(lease = %config.lease_name, "leader lease acquired");
                    let token = shutdown.child_token();
                    let task_token = token.clone();
                    let task_client = client.clone();
                    let task_config = config.clone();
                    let handle = tokio::spawn(async move {
                        reconciler::run(task_client, task_config, task_token).await
                    });
                    current = Some((token, handle));
                } else if !is_leader && current.is_some() {
                    info!("leader lease lost");
                    if let Some((token, handle)) = current.take() {
                        token.cancel();
                        if let Err(err) = handle.await {
                            warn!(error = %err, "reconciler join failed");
                        }
                    }
                }
            }
        }
    }

    if let Some((token, handle)) = current.take() {
        token.cancel();
        if let Err(err) = handle.await {
            warn!(error = %err, "reconciler join failed during shutdown");
        }
    }
    drop(state);
    let _ = lease_task.await;
    info!("leader runner exiting");
    Ok(())
}
