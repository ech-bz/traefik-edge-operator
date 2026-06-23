use std::collections::BTreeMap;

pub(super) const GROUP_LABEL: &str = "ech.bz/ingress-group";
pub(super) const FINALIZER: &str = "ech.bz/ingress-cleanup";
pub(super) const PUBLIC_IP_LABEL: &str = "ech.bz/public-ip";

const REFLECTOR_ALLOWED: &str = "reflector.v1.k8s.emberstack.com/reflection-allowed";
const REFLECTOR_ALLOWED_NS: &str = "reflector.v1.k8s.emberstack.com/reflection-allowed-namespaces";
const REFLECTOR_AUTO: &str = "reflector.v1.k8s.emberstack.com/reflection-auto-enabled";
const REFLECTOR_AUTO_NS: &str = "reflector.v1.k8s.emberstack.com/reflection-auto-namespaces";

const HASH_LEN: usize = 12;

pub(super) fn group_labels(group: &str) -> BTreeMap<String, String> {
    BTreeMap::from([(GROUP_LABEL.into(), group.into())])
}

pub(super) fn group_label_selector(group: &str) -> String {
    format!("{GROUP_LABEL}={group}")
}

pub(super) fn reflector_annotations(group: &str) -> BTreeMap<String, String> {
    let ns_regex = format!("{}.*", edge_namespace_prefix(group));
    BTreeMap::from([
        (REFLECTOR_ALLOWED.into(), "true".into()),
        (REFLECTOR_ALLOWED_NS.into(), ns_regex.clone()),
        (REFLECTOR_AUTO.into(), "true".into()),
        (REFLECTOR_AUTO_NS.into(), ns_regex),
    ])
}

fn hash(input: &str) -> String {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(input.as_bytes());
    let full = hex::encode(hasher.finalize());
    full[..HASH_LEN].to_string()
}

pub(super) fn edge_namespace_prefix(group: &str) -> String {
    format!("edge-{group}-")
}

pub(super) fn edge_namespace(group: &str, node_name: &str) -> String {
    format!("{}{}", edge_namespace_prefix(group), hash(node_name))
}

pub(super) fn traefik_crb_prefix() -> String {
    "traefik-edge-".to_string()
}

pub(super) fn traefik_ingress_class(group: &str) -> String {
    format!("traefik-{group}")
}

pub(super) fn traefik_crb_name(node_name: &str) -> String {
    format!("{}{}", traefik_crb_prefix(), hash(node_name))
}

pub(super) fn domain_cert_name(group: &str, domain: &str) -> String {
    format!("domain-cert-{group}-{}", hash(domain))
}

pub(super) fn domain_cert_secret_name(group: &str, domain: &str) -> String {
    format!("domain-secret-{group}-{}", hash(domain))
}

pub(super) fn ip_cert_name(group: &str, node_ip: &str) -> String {
    format!("ip-cert-{group}-{}", hash(node_ip))
}

pub(super) fn ip_cert_secret_name(group: &str, node_ip: &str) -> String {
    format!("ip-secret-{group}-{}", hash(node_ip))
}

pub(super) fn strip_prefix_middleware_name(
    group: &str,
    scope: &str,
    path_prefix: &str,
    service_namespace: &str,
    service_name: &str,
    service_port: u16,
) -> String {
    let key = format!("{scope}|{path_prefix}|{service_namespace}|{service_name}|{service_port}");
    format!("strip-{group}-{}", hash(&key))
}

pub(super) fn route_resource_name(
    group: &str,
    scope: &str,
    path_prefix: &str,
    service_namespace: &str,
    service_name: &str,
    service_port: u16,
) -> String {
    let key = format!("{scope}|{path_prefix}|{service_namespace}|{service_name}|{service_port}");
    format!("route-{group}-{}", hash(&key))
}
