FROM rust:1.75-bullseye AS builder

RUN mkdir /rust
COPY ./Cargo.toml /rust
COPY ./core /rust/core
COPY ./backfill /rust/backfill
COPY ./das_api /rust/das_api
COPY ./digital_asset_types /rust/digital_asset_types
COPY ./integration_tests /rust/integration_tests
COPY ./metaplex-rpc-proxy /rust/metaplex-rpc-proxy
COPY ./migration /rust/migration
COPY ./nft_ingester /rust/nft_ingester
COPY ./ops /rust/ops
COPY ./program_transformers /rust/program_transformers
COPY ./tools /rust/tools

WORKDIR /rust/metaplex-rpc-proxy
RUN cargo install wasm-pack
RUN wasm-pack build --release

FROM envoyproxy/envoy:v1.24.0
COPY --from=builder /rust/target/wasm32-unknown-unknown/release/metaplex_rpc_proxy.wasm /etc/rpc_proxy.wasm
RUN apt-get update && apt-get install -y ca-certificates
ENTRYPOINT /usr/local/bin/envoy -c /etc/envoy.yaml -l trace --service-cluster proxy
