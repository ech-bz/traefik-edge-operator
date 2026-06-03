use super::naming::{GROUP_LABEL, group_labels, traefik_crb_name, traefik_ingress_class};
use crate::{error::Result, resources};
use k8s_openapi::api::{
    apps::v1::{DaemonSet, DaemonSetSpec},
    core::v1::{
        Capabilities, Container, ContainerPort, PodSpec, PodTemplateSpec, SecurityContext,
        ServiceAccount, Toleration,
    },
    rbac::v1::{ClusterRoleBinding, RoleRef, Subject},
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use kube::{
    Client,
    api::{Api, ObjectMeta},
};
use std::collections::BTreeMap;

const TRAEFIK_SA: &str = "traefik-edge";
const TRAEFIK_DS: &str = "traefik-edge";
const TRAEFIK_APP_LABEL: &str = "traefik-edge";
const TRAEFIK_CLUSTER_ROLE: &str = "traefik-ingress-controller";
const INGRESS_ONLY_TAINT: &str = "ech.bz/ingress-only";

pub(super) async fn ensure_traefik_stack(
    client: &Client,
    traefik_image: &str,
    group: &str,
    edge_ns: &str,
    node_name: &str,
    watch_namespaces: &str,
) -> Result<()> {
    ensure_traefik_rbac(client, group, edge_ns, node_name).await?;
    ensure_traefik_daemonset(
        client,
        traefik_image,
        group,
        edge_ns,
        node_name,
        watch_namespaces,
    )
    .await
}

async fn ensure_traefik_rbac(
    client: &Client,
    group: &str,
    edge_ns: &str,
    node_name: &str,
) -> Result<()> {
    let labels = group_labels(group);

    let sa_api: Api<ServiceAccount> = Api::namespaced(client.clone(), edge_ns);
    let sa = ServiceAccount {
        metadata: ObjectMeta {
            name: Some(TRAEFIK_SA.into()),
            namespace: Some(edge_ns.to_string()),
            labels: Some(labels.clone()),
            ..Default::default()
        },
        ..Default::default()
    };
    resources::apply(&sa_api, TRAEFIK_SA, &sa).await?;

    let crb_api: Api<ClusterRoleBinding> = Api::all(client.clone());
    let crb_name = traefik_crb_name(node_name);
    let crb = ClusterRoleBinding {
        metadata: ObjectMeta {
            name: Some(crb_name.clone()),
            labels: Some(labels),
            ..Default::default()
        },
        role_ref: RoleRef {
            api_group: "rbac.authorization.k8s.io".into(),
            kind: "ClusterRole".into(),
            name: TRAEFIK_CLUSTER_ROLE.into(),
        },
        subjects: Some(vec![Subject {
            kind: "ServiceAccount".into(),
            name: TRAEFIK_SA.into(),
            namespace: Some(edge_ns.to_string()),
            ..Default::default()
        }]),
    };
    resources::apply(&crb_api, &crb_name, &crb).await?;
    Ok(())
}

async fn ensure_traefik_daemonset(
    client: &Client,
    traefik_image: &str,
    group: &str,
    edge_ns: &str,
    node_name: &str,
    watch_namespaces: &str,
) -> Result<()> {
    let mut ds_labels = group_labels(group);
    ds_labels.insert("app".into(), TRAEFIK_APP_LABEL.into());

    let ds = DaemonSet {
        metadata: ObjectMeta {
            name: Some(TRAEFIK_DS.into()),
            namespace: Some(edge_ns.to_string()),
            labels: Some(ds_labels),
            ..Default::default()
        },
        spec: Some(DaemonSetSpec {
            selector: LabelSelector {
                match_labels: Some(BTreeMap::from([("app".into(), TRAEFIK_APP_LABEL.into())])),
                ..Default::default()
            },
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: Some(BTreeMap::from([
                        ("app".into(), TRAEFIK_APP_LABEL.into()),
                        (GROUP_LABEL.into(), group.to_string()),
                    ])),
                    ..Default::default()
                }),
                spec: Some(PodSpec {
                    host_network: Some(true),
                    dns_policy: Some("ClusterFirstWithHostNet".into()),
                    node_name: Some(node_name.to_string()),
                    tolerations: Some(vec![Toleration {
                        key: Some(INGRESS_ONLY_TAINT.into()),
                        operator: Some("Equal".into()),
                        value: Some("true".into()),
                        effect: Some("NoSchedule".into()),
                        ..Default::default()
                    }]),
                    service_account_name: Some(TRAEFIK_SA.into()),
                    containers: vec![Container {
                        name: "traefik".into(),
                        image: Some(traefik_image.to_string()),
                        args: Some(vec![
                            "--entryPoints.web.address=:80".into(),
                            "--entryPoints.websecure.address=:443".into(),
                            "--providers.kubernetesingress".into(),
                            format!(
                                "--providers.kubernetesingress.ingressclass={}",
                                traefik_ingress_class(group)
                            ),
                            format!("--providers.kubernetesingress.namespaces={watch_namespaces}"),
                            "--providers.kubernetescrd".into(),
                            "--providers.kubernetescrd.allowcrossnamespace=true".into(),
                            format!("--providers.kubernetescrd.namespaces={watch_namespaces}"),
                        ]),
                        ports: Some(vec![
                            ContainerPort {
                                name: Some("web".into()),
                                container_port: 80,
                                ..Default::default()
                            },
                            ContainerPort {
                                name: Some("websecure".into()),
                                container_port: 443,
                                ..Default::default()
                            },
                        ]),
                        security_context: Some(SecurityContext {
                            capabilities: Some(Capabilities {
                                add: Some(vec!["NET_BIND_SERVICE".into()]),
                                drop: Some(vec!["ALL".into()]),
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
            },
            ..Default::default()
        }),
        ..Default::default()
    };

    let ds_api: Api<DaemonSet> = Api::namespaced(client.clone(), edge_ns);
    resources::apply(&ds_api, TRAEFIK_DS, &ds).await
}
