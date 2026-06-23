#![allow(clippy::all)]

pub mod generated {
    pub mod certificate {
        include!(concat!(env!("OUT_DIR"), "/certificate.rs"));
    }
    pub mod cluster_issuer {
        include!(concat!(env!("OUT_DIR"), "/cluster_issuer.rs"));
    }
    pub mod ingress_group {
        include!(concat!(env!("OUT_DIR"), "/ingress_group.rs"));
    }
    pub mod ingress_route {
        include!(concat!(env!("OUT_DIR"), "/ingress_route.rs"));
    }
    pub mod middleware {
        include!(concat!(env!("OUT_DIR"), "/middleware.rs"));
    }
    pub mod tls_store {
        include!(concat!(env!("OUT_DIR"), "/tls_store.rs"));
    }
}

pub use generated::certificate::*;
pub use generated::cluster_issuer::*;
pub use generated::ingress_group::*;
pub use generated::ingress_route::*;
pub use generated::middleware::*;
pub use generated::tls_store::*;
