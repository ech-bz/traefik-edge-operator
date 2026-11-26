use super::naming::{
    domain_cert_secret_name, group_label_selector, group_labels, headers_middleware_name,
    ip_cert_secret_name, route_resource_name, strip_prefix_middleware_name,
};
use crate::{
    crds::{
        IngressGroupRoutes, IngressRoute, IngressRouteRoutes, IngressRouteRoutesKind,
        IngressRouteRoutesMiddlewares, IngressRouteRoutesServices,
        IngressRouteSpec as IngressRouteCrdSpec, IngressRouteTls, Middleware, MiddlewareHeaders,
        MiddlewareSpec, MiddlewareStripPrefix, TlsStore, TlsStoreCertificates,
        TlsStoreDefaultCertificate, TlsStoreSpec,
    },
    error::{OperatorError, Result},
    resources,
};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::{Api, Client};
use std::collections::{BTreeMap, BTreeSet};

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

pub(super) struct DesiredResources {
    pub routes: BTreeSet<String>,
    pub middlewares: BTreeSet<String>,
}

pub(super) async fn ensure_ingress_routes(
    client: &Client,
    group: &str,
    edge_ns: &str,
    group_ns: &str,
    routes: &[IngressGroupRoutes],
) -> Result<DesiredResources> {
    let mut desired_routes = BTreeSet::new();
    let mut desired_middlewares = BTreeSet::new();
    let api: Api<IngressRoute> = Api::namespaced(client.clone(), edge_ns);
    let mw_api: Api<Middleware> = Api::namespaced(client.clone(), edge_ns);
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

        let headers_key = headers_key(route.headers.as_ref())?;
        validate_headers(route, group)?;

        let strip = route.strip_prefix.unwrap_or(false);
        let name = route_resource_name(
            group,
            edge_ns,
            &route.path_prefix,
            &service_namespace,
            &route.service_name,
            service_port,
            strip,
            &headers_key,
        );
        desired_routes.insert(name.clone());

        let mut middlewares: Vec<IngressRouteRoutesMiddlewares> = Vec::new();
        if strip {
            let mw_name = strip_prefix_middleware_name(
                group,
                edge_ns,
                &route.path_prefix,
                &service_namespace,
                &route.service_name,
                service_port,
            );
            desired_middlewares.insert(mw_name.clone());
            ensure_strip_prefix_middleware(&mw_api, group, edge_ns, &mw_name, &route.path_prefix)
                .await?;
            middlewares.push(IngressRouteRoutesMiddlewares {
                name: mw_name,
                namespace: None,
            });
        }
        if let Some(headers) = &route.headers {
            let mw_name = headers_middleware_name(
                group,
                edge_ns,
                &route.path_prefix,
                &service_namespace,
                &route.service_name,
                service_port,
                &headers_key,
            );
            desired_middlewares.insert(mw_name.clone());
            ensure_headers_middleware(&mw_api, group, edge_ns, &mw_name, headers).await?;
            middlewares.push(IngressRouteRoutesMiddlewares {
                name: mw_name,
                namespace: None,
            });
        }
        let middlewares = if middlewares.is_empty() {
            None
        } else {
            Some(middlewares)
        };

        let ingress_route = build_ingress_route(RouteBuild {
            name: &name,
            namespace: edge_ns,
            group,
            match_rule: format!("PathPrefix(`{}`)", route.path_prefix),
            service_name: route.service_name.clone(),
            service_namespace,
            service_port,
            service_scheme: route.service_scheme.clone(),
            middlewares,
        });
        resources::apply(&api, &name, &ingress_route).await?;
    }
    Ok(DesiredResources {
        routes: desired_routes,
        middlewares: desired_middlewares,
    })
}

async fn ensure_strip_prefix_middleware(
    api: &Api<Middleware>,
    group: &str,
    edge_ns: &str,
    name: &str,
    path_prefix: &str,
) -> Result<()> {
    let mut mw = Middleware::new(
        name,
        MiddlewareSpec {
            strip_prefix: Some(MiddlewareStripPrefix {
                prefixes: Some(vec![path_prefix.to_string()]),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    mw.metadata.namespace = Some(edge_ns.to_string());
    mw.metadata.labels = Some(group_labels(group));
    resources::apply(api, name, &mw).await
}

async fn ensure_headers_middleware(
    api: &Api<Middleware>,
    group: &str,
    edge_ns: &str,
    name: &str,
    headers: &BTreeMap<String, String>,
) -> Result<()> {
    let mut mw = Middleware::new(
        name,
        MiddlewareSpec {
            headers: Some(MiddlewareHeaders {
                custom_request_headers: Some(headers.clone()),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    mw.metadata.namespace = Some(edge_ns.to_string());
    mw.metadata.labels = Some(group_labels(group));
    resources::apply(api, name, &mw).await
}

fn headers_key(headers: Option<&BTreeMap<String, String>>) -> Result<String> {
    match headers {
        Some(headers) => serde_json::to_string(headers)
            .map_err(|e| OperatorError::Config(format!("serialize route headers: {e}"))),
        None => Ok(String::new()),
    }
}

fn validate_headers(route: &IngressGroupRoutes, group: &str) -> Result<()> {
    if route.headers.as_ref().is_some_and(|m| m.is_empty()) {
        return Err(OperatorError::Config(format!(
            "headers of route {} in group {} must define at least one header",
            route.path_prefix, group
        )));
    }
    Ok(())
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

pub(super) async fn prune_middlewares(
    client: &Client,
    group: &str,
    edge_ns: &str,
    desired: &BTreeSet<String>,
) -> Result<()> {
    let api: Api<Middleware> = Api::namespaced(client.clone(), edge_ns);
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
    service_scheme: Option<String>,
    middlewares: Option<Vec<IngressRouteRoutesMiddlewares>>,
}

fn build_ingress_route(r: RouteBuild<'_>) -> IngressRoute {
    let mut route = IngressRoute::new(
        r.name,
        IngressRouteCrdSpec {
            entry_points: Some(vec!["websecure".into()]),
            routes: vec![IngressRouteRoutes {
                kind: Some(IngressRouteRoutesKind::Rule),
                r#match: r.match_rule,
                middlewares: r.middlewares,
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
                    scheme: r.service_scheme,
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
