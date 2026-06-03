FROM lukemathwalker/cargo-chef:latest-rust-1.88-bookworm AS chef
WORKDIR /src

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --bin traefik-edge-operator

FROM gcr.io/distroless/cc-debian12
COPY --from=build /src/target/release/traefik-edge-operator /usr/local/bin/traefik-edge-operator
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/traefik-edge-operator"]
