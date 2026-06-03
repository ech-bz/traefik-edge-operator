mod certificates;
mod naming;
mod nodes;
mod routes;
mod traefik;

use crate::{
    config::OperatorConfig,
    crds::{ClusterIssuer, IngressGroup},
    error::{OperatorError, Result},
};
use futures::stream::StreamExt;
use k8s_openapi::api::core::v1::Node;
use kube::{
    Client, ResourceExt,
    api::Api,
    runtime::{
        controller::{Action, Controller},
        finalizer::{Error as FinalizerError, Event as FinalizerEvent, finalizer},
        reflector::ObjectRef,
        watcher,
    },
};
use std::{collections::BTreeSet, sync::Arc};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use naming::{FINALIZER, GROUP_LABEL, edge_namespace};

pub(crate) struct Ctx {
    pub client: Client,
    pub config: OperatorConfig,
}

pub async fn run(
    client: Client,
    config: OperatorConfig,
    shutdown: CancellationToken,
) -> Result<()> {
    let ingress_api = Api::<IngressGroup>::all(client.clone());
    let node_api = Api::<Node>::all(client.clone());
    let context = Arc::new(Ctx { client, config });

    let ctrl = Controller::new(ingress_api, watcher::Config::default());
    let store = ctrl.store();

    ctrl.watches(
        node_api,
        watcher::Config::default().labels(GROUP_LABEL),
        move |node: Node| {
            let group_name = node
                .metadata
                .labels
                .as_ref()
                .and_then(|l| l.get(GROUP_LABEL))
                .cloned()
                .unwrap_or_default();

            if group_name.is_empty() {
                return vec![];
            }

            store
                .state()
                .into_iter()
                .filter(|ig| ig.name_any() == group_name)
                .map(|ig| ObjectRef::from_obj(&*ig))
                .collect::<Vec<_>>()
        },
    )
    .graceful_shutdown_on(shutdown.clone().cancelled_owned())
    .run(reconcile, error_policy, context)
    .for_each(|res| async move {
        match res {
            Ok((objref, action)) => {
                tracing::debug!(name = %objref.name, namespace = ?objref.namespace, ?action, "ingress reconciled");
            }
            Err(err) => {
                warn!(error = %err, "ingress controller stream error");
            }
        }
    })
    .await;

    if shutdown.is_cancelled() {
        Ok(())
    } else {
        Err(OperatorError::ControllerFatal(
            "controller stream terminated unexpectedly".into(),
        ))
    }
}

async fn reconcile(group: Arc<IngressGroup>, ctx: Arc<Ctx>) -> Result<Action> {
    let namespace = group
        .namespace()
        .ok_or_else(|| OperatorError::MissingNamespace {
            kind: "IngressGroup",
            name: group.name_any(),
        })?;
    let api: Api<IngressGroup> = Api::namespaced(ctx.client.clone(), &namespace);

    finalizer(&api, FINALIZER, group, |event| async move {
        match event {
            FinalizerEvent::Apply(g) => reconcile_apply(&ctx, &namespace, g).await,
            FinalizerEvent::Cleanup(g) => reconcile_cleanup(&ctx, &namespace, g).await,
        }
    })
    .await
    .map_err(|e| match e {
        FinalizerError::ApplyFailed(e) | FinalizerError::CleanupFailed(e) => e,
        FinalizerError::AddFinalizer(e) | FinalizerError::RemoveFinalizer(e) => e.into(),
        other => OperatorError::Finalizer(other.to_string()),
    })
}

async fn reconcile_apply(ctx: &Ctx, group_ns: &str, group: Arc<IngressGroup>) -> Result<Action> {
    let group_name = group.name_any();
    let client = &ctx.client;
    let domains = group.spec.domains.as_deref().unwrap_or(&[]);
    let routes = group.spec.routes.as_deref().unwrap_or(&[]);

    let issuers: Api<ClusterIssuer> = Api::all(client.clone());
    if issuers.get_opt(&group_name).await?.is_none() {
        info!(group = %group_name, "skipping ingress reconciliation until ClusterIssuer exists");
        return Ok(Action::requeue(ctx.config.wait_backoff()));
    }

    certificates::ensure_domain_certs(client, &group_name, group_ns, domains).await?;

    let nodes = nodes::nodes_for_group(client, &group_name).await?;
    let mut desired_edge_namespaces = BTreeSet::new();

    for node in &nodes {
        let Some((node_name, node_ip)) = nodes::edge_node_identity(node) else {
            continue;
        };
        let edge_ns = edge_namespace(&group_name, &node_name);
        desired_edge_namespaces.insert(edge_ns.clone());

        nodes::ensure_edge_namespace(client, &group_name, &edge_ns).await?;
        certificates::ensure_ip_cert(client, &group_name, &edge_ns, &node_ip).await?;
        routes::ensure_tls_store(client, &group_name, &edge_ns, domains, &node_ip).await?;
        let desired_routes =
            routes::ensure_ingress_routes(client, &group_name, &edge_ns, group_ns, routes).await?;
        traefik::ensure_traefik_stack(
            client,
            &ctx.config.traefik_image,
            &group_name,
            &edge_ns,
            &node_name,
            &nodes::route_watch_namespaces(&edge_ns, group_ns, routes),
        )
        .await?;

        routes::prune_routes(client, &group_name, &edge_ns, &desired_routes).await?;
        certificates::prune_ip_certs(client, &group_name, &edge_ns, &node_ip).await?;
    }

    certificates::prune_domain_certs(client, &group_name, group_ns, domains).await?;
    nodes::prune_edge_namespaces(client, &group_name, &desired_edge_namespaces).await?;

    Ok(Action::requeue(ctx.config.reconcile_interval()))
}

async fn reconcile_cleanup(ctx: &Ctx, group_ns: &str, group: Arc<IngressGroup>) -> Result<Action> {
    let group_name = group.name_any();
    info!(group = %group_name, "cleaning up deleted IngressGroup");
    nodes::prune_edge_namespaces(&ctx.client, &group_name, &BTreeSet::new()).await?;
    certificates::prune_domain_certs(&ctx.client, &group_name, group_ns, &[]).await?;
    Ok(Action::await_change())
}

fn error_policy(_obj: Arc<IngressGroup>, err: &OperatorError, ctx: Arc<Ctx>) -> Action {
    warn!(error = %err, "ingress reconcile failed");
    Action::requeue(ctx.config.error_backoff())
}
