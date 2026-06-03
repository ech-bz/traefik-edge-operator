# traefik-edge-operator

A Kubernetes operator that turns selected nodes into a TLS-terminating edge tier.

## What it does

You declare an `IngressGroup` — a list of domains, a list of routes, and a label that picks the nodes that will serve them. The operator does the rest:

- runs a per-node Traefik DaemonSet on `hostNetwork`, pinned to each selected node;
- requests and renews a TLS certificate for every domain via cert-manager;
- requests a per-node certificate that includes the node's public IP as a SAN, so clients can hit the node directly by IP and still get a valid cert;
- distributes the resulting secrets into each node's dedicated namespace using [emberstack/reflector](https://github.com/emberstack/kubernetes-reflector);
- generates the corresponding `IngressRoute` and `TLSStore` objects for Traefik;
- cleans everything up when a node leaves the group or when the `IngressGroup` is deleted.

The typical use case is exposing services from small clusters, edge nodes or bare-metal hosts where there is no cloud load balancer in front and clients must reach the nodes directly.

## Install

The operator and its CRDs ship as two separate Helm charts.

```sh
helm install traefik-edge-operator-crds \
  oci://ghcr.io/ech-bz/charts/traefik-edge-operator-crds

helm install traefik-edge-operator \
  oci://ghcr.io/ech-bz/charts/traefik-edge-operator \
  -n traefik-edge-system --create-namespace
```

### Prerequisites

These must already be installed in the cluster:

- [cert-manager](https://cert-manager.io/) with at least one `ClusterIssuer` whose name matches your `IngressGroup` name;
- the Traefik CRDs and RBACs;
- [emberstack/reflector](https://github.com/emberstack/kubernetes-reflector).

The operator does not install them.

### Labeling nodes

A node joins a group when it carries:

- `ech.bz/ingress-group=<group-name>` — picks the group;
- `ech.bz/public-ip=<routable-ip>` — the IP that gets baked into the per-node certificate.

### Defining an `IngressGroup`

```yaml
apiVersion: ech.bz/v1alpha1
kind: IngressGroup
metadata:
  name: public
  namespace: ingress
spec:
  domains:
    - app.example.com
  routes:
    - pathPrefix: /
      serviceName: app
      serviceNamespace: apps
      servicePort: 8080
```
