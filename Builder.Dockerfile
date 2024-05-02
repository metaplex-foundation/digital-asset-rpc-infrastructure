FROM rust:1.75-bullseye AS builder
RUN apt-get update -y && \
    apt-get install -y build-essential make git
    
RUN mkdir /rust
RUN mkdir /rust/bins
COPY Cargo.toml /rust
COPY core /rust/core
COPY das_api /rust/das_api
COPY digital_asset_types /rust/digital_asset_types
COPY integration_tests /rust/integration_tests
COPY metaplex-rpc-proxy /rust/metaplex-rpc-proxy
COPY migration /rust/migration
COPY nft_ingester /rust/nft_ingester
COPY ops /rust/ops
COPY program_transformers /rust/program_transformers
COPY tools /rust/tools
COPY blockbuster rust/blockbuster
WORKDIR /rust
RUN --mount=type=cache,target=/rust/target,id=das-rust \
  cargo build --release --bins && cp `find /rust/target/release -maxdepth 1 -type f | sed 's/^\.\///' | grep -v "\." ` /rust/bins
    
FROM rust:1.75-slim-bullseye as final
COPY --from=builder /rust/bins /das/
CMD echo "Built the DAS API bins!"
