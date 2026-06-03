use std::{env, fs, path::PathBuf};

use anyhow::{Context, Result};
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kopium::{Derive, SchemaMode, TypeGenerator};

const CRDS: &[(&str, &str)] = &[
    ("external-crds/cm-Certificate.yaml", "certificate.rs"),
    ("external-crds/cm-ClusterIssuer.yaml", "cluster_issuer.rs"),
    ("external-crds/tr-IngressRoute.yaml", "ingress_route.rs"),
    ("external-crds/tr-TLSStore.yaml", "tls_store.rs"),
    (
        "charts/traefik-edge-operator-crds/templates/ingressgroup.yaml",
        "ingress_group.rs",
    ),
];

fn main() -> Result<()> {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").context("OUT_DIR not set")?);

    println!("cargo:rerun-if-changed=build.rs");

    let generator = TypeGenerator::builder()
        .schema_mode(SchemaMode::Derived)
        .emit_docs(true)
        .smart_derive_elision(true)
        .derive(Derive::all("JsonSchema"))
        .derive(Derive::all("Default"))
        .build();

    for (input, output) in CRDS {
        println!("cargo:rerun-if-changed={input}");
        let yaml = fs::read_to_string(input).with_context(|| format!("read {input}"))?;
        let crd: CustomResourceDefinition = serde_saphyr::from_str(&yaml)
            .map_err(|e| anyhow::anyhow!("{e}"))
            .with_context(|| format!("parse CRD from {input}"))?;
        let code = generator
            .generate_rust_types_for(&crd, None::<&str>)
            .with_context(|| format!("kopium codegen for {input}"))?;
        fs::write(out_dir.join(output), code).with_context(|| format!("write {output}"))?;
    }

    Ok(())
}
