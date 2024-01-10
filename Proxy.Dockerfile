FROM rust:1.73-bullseye AS builder
COPY ./metaplex-rpc-proxy /rust/metaplex-rpc-proxy
COPY docker/proxy/Cargo.toml /rust
WORKDIR /rust/metaplex-rpc-proxy
RUN cargo install wasm-pack
RUN wasm-pack build --release

FROM envoyproxy/envoy:v1.24.0
COPY --from=builder /rust/target/wasm32-unknown-unknown/release/metaplex_rpc_proxy.wasm /etc/rpc_proxy.wasm
RUN apt-get update && apt-get install -y ca-certificates
ENTRYPOINT /usr/local/bin/envoy -c /etc/envoy.yaml -l trace --service-cluster proxy
