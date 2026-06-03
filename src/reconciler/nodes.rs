use super::naming::{
    PUBLIC_IP_LABEL, edge_namespace_prefix, group_label_selector, group_labels, traefik_crb_prefix,
};
use crate::{crds::IngressGroupRoutes, error::Result, resources};
use k8s_openapi::api::{
    core::v1::{Namespace, Node},
    rbac::v1::ClusterRoleBinding,
};
use kube::{
    Client, ResourceExt,
    api::{Api, ListParams, ObjectMeta},
};
use std::collections::BTreeSet;

pub(super) async fn nodes_for_group(client: &Client, group: &str) -> Result<Vec<Node>> {
    let api: Api<Node> = Api::all(client.clone());
    let lp = ListParams::default().labels(&group_label_selector(group));
    Ok(api.list(&lp).await?.items)
}

pub(super) fn edge_node_identity(node: &Node) -> Option<(String, String)> {
    let node_name = node.name_any();
    let node_ip = node
        .metadata
        .labels
        .as_ref()
        .and_then(|l| l.get(PUBLIC_IP_LABEL))
        .cloned()?;
    if node_ip.is_empty() {
        return None;
    }
    Some((node_name, node_ip))
}

pub(super) fn route_watch_namespaces(
    edge_ns: &str,
    group_ns: &str,
    routes: &[IngressGroupRoutes],
) -> String {
    let mut namespaces = BTreeSet::new();
    namespaces.insert(edge_ns.to_string());
    namespaces.insert(group_ns.to_string());
    for route in routes {
        let ns = route.service_namespace.as_deref().unwrap_or(group_ns);
        namespaces.insert(ns.to_string());
    }
    namespaces.into_iter().collect::<Vec<_>>().join(",")
}

pub(super) async fn ensure_edge_namespace(
    client: &Client,
    group: &str,
    edge_ns: &str,
) -> Result<()> {
    let api: Api<Namespace> = Api::all(client.clone());
    let ns = Namespace {
        metadata: ObjectMeta {
            name: Some(edge_ns.to_string()),
            labels: Some(group_labels(group)),
            ..Default::default()
        },
        ..Default::default()
    };
    resources::apply(&api, edge_ns, &ns).await
}

pub(super) async fn prune_edge_namespaces(
    client: &Client,
    group: &str,
    desired: &BTreeSet<String>,
) -> Result<()> {
    let selector = group_label_selector(group);
    let prefix = edge_namespace_prefix(group);
    let crb_prefix = traefik_crb_prefix();
    let desired_crbs: BTreeSet<String> = desired
        .iter()
        .map(|ns| format!("{}{}", crb_prefix, &ns[prefix.len()..]))
        .collect();

    let ns_api: Api<Namespace> = Api::all(client.clone());
    let crb_api: Api<ClusterRoleBinding> = Api::all(client.clone());
    resources::prune(&ns_api, &selector, desired).await?;
    resources::prune(&crb_api, &selector, &desired_crbs).await?;
    Ok(())
}
