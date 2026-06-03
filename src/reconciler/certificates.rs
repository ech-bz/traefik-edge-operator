use super::naming::{
    domain_cert_name, domain_cert_secret_name, group_label_selector, group_labels, ip_cert_name,
    ip_cert_secret_name, reflector_annotations,
};
use crate::{
    crds::{Certificate, CertificateIssuerRef, CertificateSecretTemplate, CertificateSpec},
    error::Result,
    resources,
};
use kube::{Api, Client};
use std::collections::BTreeSet;

pub(super) async fn ensure_domain_certs(
    client: &Client,
    group: &str,
    group_ns: &str,
    domains: &[String],
) -> Result<()> {
    let api: Api<Certificate> = Api::namespaced(client.clone(), group_ns);
    for domain in domains {
        let name = domain_cert_name(group, domain);
        let cert = build_certificate(
            &name,
            group_ns,
            group,
            domain_cert_secret_name(group, domain),
            Some(vec![domain.clone()]),
            None,
            true,
        );
        resources::apply(&api, &name, &cert).await?;
    }
    Ok(())
}

pub(super) async fn prune_domain_certs(
    client: &Client,
    group: &str,
    group_ns: &str,
    domains: &[String],
) -> Result<()> {
    let desired: BTreeSet<String> = domains.iter().map(|d| domain_cert_name(group, d)).collect();
    let api: Api<Certificate> = Api::namespaced(client.clone(), group_ns);
    resources::prune(&api, &group_label_selector(group), &desired).await
}

pub(super) async fn ensure_ip_cert(
    client: &Client,
    group: &str,
    edge_ns: &str,
    node_ip: &str,
) -> Result<()> {
    let name = ip_cert_name(group, node_ip);
    let cert = build_certificate(
        &name,
        edge_ns,
        group,
        ip_cert_secret_name(group, node_ip),
        None,
        Some(vec![node_ip.to_string()]),
        false,
    );
    let api: Api<Certificate> = Api::namespaced(client.clone(), edge_ns);
    resources::apply(&api, &name, &cert).await
}

pub(super) async fn prune_ip_certs(
    client: &Client,
    group: &str,
    edge_ns: &str,
    node_ip: &str,
) -> Result<()> {
    let api: Api<Certificate> = Api::namespaced(client.clone(), edge_ns);
    resources::prune(
        &api,
        &group_label_selector(group),
        &BTreeSet::from([ip_cert_name(group, node_ip)]),
    )
    .await
}

fn build_certificate(
    name: &str,
    namespace: &str,
    group: &str,
    secret_name: String,
    dns_names: Option<Vec<String>>,
    ip_addresses: Option<Vec<String>>,
    reflect: bool,
) -> Certificate {
    let secret_template = if reflect {
        Some(CertificateSecretTemplate {
            annotations: Some(reflector_annotations(group)),
            labels: None,
        })
    } else {
        None
    };
    let mut cert = Certificate::new(
        name,
        CertificateSpec {
            secret_name,
            dns_names,
            ip_addresses,
            secret_template,
            issuer_ref: CertificateIssuerRef {
                name: group.to_string(),
                kind: Some("ClusterIssuer".into()),
                group: Some("cert-manager.io".into()),
            },
            ..Default::default()
        },
    );
    cert.metadata.namespace = Some(namespace.to_string());
    cert.metadata.labels = Some(group_labels(group));
    cert
}
