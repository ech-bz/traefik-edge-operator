use super::naming::{
    domain_cert_secret_name, group_label_selector, group_labels, ip_cert_secret_name,
    route_resource_name,
};
use crate::{
    crds::{
        IngressGroupRoutes, IngressRoute, IngressRouteRoutes, IngressRouteRoutesKind,
        IngressRouteRoutesServices, IngressRouteSpec as IngressRouteCrdSpec, IngressRouteTls,
        TlsStore, TlsStoreCertificates, TlsStoreDefaultCertificate, TlsStoreSpec,
    },
    error::{OperatorError, Result},
    resources,
};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::{Api, Client};
use std::collections::BTreeSet;

const TLSSTORE_DEFAULT: &str = "default";

pub(super) async fn ensure_tls_store(
    client: &Client,
    group: &str,
    edge_ns: &str,
    domains: &[String],
    node_ip: &str,
) -> Result<()> {
    let domain_certificates: Vec<TlsStoreCertificates> = domains
        .iter()
        .map(|d| TlsStoreCertificates {
            secret_name: domain_cert_secret_name(group, d),
        })
        .collect();

    let mut tlsstore = TlsStore::new(
        TLSSTORE_DEFAULT,
        TlsStoreSpec {
            certificates: if domain_certificates.is_empty() {
                None
            } else {
                Some(domain_certificates)
            },
            default_certificate: Some(TlsStoreDefaultCertificate {
                secret_name: ip_cert_secret_name(group, node_ip),
            }),
            default_generated_cert: None,
        },
    );
    tlsstore.metadata.namespace = Some(edge_ns.to_string());
    tlsstore.metadata.labels = Some(group_labels(group));
    let api: Api<TlsStore> = Api::namespaced(client.clone(), edge_ns);
    resources::apply(&api, TLSSTORE_DEFAULT, &tlsstore).await
}

pub(super) async fn ensure_ingress_routes(
    client: &Client,
    group: &str,
    edge_ns: &str,
    group_ns: &str,
    routes: &[IngressGroupRoutes],
) -> Result<BTreeSet<String>> {
    let mut desired = BTreeSet::new();
    let api: Api<IngressRoute> = Api::namespaced(client.clone(), edge_ns);
    for route in routes {
        let service_port: u16 = route.service_port.try_into().map_err(|_| {
            OperatorError::Config(format!(
                "invalid servicePort {} for route {} in group {}",
                route.service_port, route.path_prefix, group
            ))
        })?;
        let service_namespace = route
            .service_namespace
            .as_deref()
            .unwrap_or(group_ns)
            .to_string();

        let name = route_resource_name(
            group,
            edge_ns,
            &route.path_prefix,
            &service_namespace,
            &route.service_name,
            service_port,
        );
        desired.insert(name.clone());

        let ingress_route = build_ingress_route(RouteBuild {
            name: &name,
            namespace: edge_ns,
            group,
            match_rule: format!("PathPrefix(`{}`)", route.path_prefix),
            service_name: route.service_name.clone(),
            service_namespace,
            service_port,
        });
        resources::apply(&api, &name, &ingress_route).await?;
    }
    Ok(desired)
}

pub(super) async fn prune_routes(
    client: &Client,
    group: &str,
    edge_ns: &str,
    desired: &BTreeSet<String>,
) -> Result<()> {
    let api: Api<IngressRoute> = Api::namespaced(client.clone(), edge_ns);
    resources::prune(&api, &group_label_selector(group), desired).await
}

struct RouteBuild<'a> {
    name: &'a str,
    namespace: &'a str,
    group: &'a str,
    match_rule: String,
    service_name: String,
    service_namespace: String,
    service_port: u16,
}

fn build_ingress_route(r: RouteBuild<'_>) -> IngressRoute {
    let mut route = IngressRoute::new(
        r.name,
        IngressRouteCrdSpec {
            entry_points: Some(vec!["websecure".into()]),
            routes: vec![IngressRouteRoutes {
                kind: Some(IngressRouteRoutesKind::Rule),
                r#match: r.match_rule,
                middlewares: None,
                observability: None,
                priority: None,
                services: Some(vec![IngressRouteRoutesServices {
                    name: r.service_name,
                    namespace: Some(r.service_namespace),
                    port: Some(IntOrString::Int(r.service_port as i32)),
                    kind: None,
                    health_check: None,
                    native_lb: None,
                    node_port_lb: None,
                    pass_host_header: None,
                    response_forwarding: None,
                    scheme: None,
                    servers_transport: None,
                    sticky: None,
                    strategy: None,
                    weight: None,
                }]),
                syntax: None,
            }],
            tls: Some(IngressRouteTls {
                cert_resolver: None,
                domains: None,
                options: None,
                secret_name: None,
                store: None,
            }),
        },
    );
    route.metadata.namespace = Some(r.namespace.to_string());
    route.metadata.labels = Some(group_labels(r.group));
    route
}
