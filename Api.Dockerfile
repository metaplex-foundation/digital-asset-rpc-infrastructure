FROM rust:1.75-bullseye AS chef
RUN cargo install cargo-chef
FROM chef AS planner

RUN mkdir /rust
COPY Cargo.toml /rust
COPY core /rust/core
COPY das_api /rust/das_api
COPY digital_asset_types /rust/digital_asset_types
COPY integration_tests /rust/integration_tests
COPY metaplex-rpc-proxy /rust/metaplex-rpc-proxy
COPY migration /rust/migration
COPY nft_ingester /rust/nft_ingester
COPY ops /rust/ops
COPY tools /rust/tools

WORKDIR /rust/das_api
RUN cargo chef prepare --recipe-path /rust/das_api/recipe.json

FROM chef AS builder
RUN apt-get update -y && \
    apt-get install -y build-essential make git

RUN mkdir /rust
COPY Cargo.toml /rust
COPY core /rust/core
COPY das_api /rust/das_api
COPY digital_asset_types /rust/digital_asset_types
COPY integration_tests /rust/integration_tests
COPY metaplex-rpc-proxy /rust/metaplex-rpc-proxy
COPY migration /rust/migration
COPY nft_ingester /rust/nft_ingester
COPY ops /rust/ops
COPY tools /rust/tools

WORKDIR /rust/das_api
COPY --from=planner /rust/das_api/recipe.json recipe.json

# Build dependencies - this is the caching Docker layer!
# TODO: Fix this.  For now we are building without the cached dependencies as there's apparently
# some problem with digital-asset-types feature activation.

# RUN cargo chef cook --release --recipe-path recipe.json --target-dir /rust/target --all-features

# Build application
RUN cargo build --release

FROM rust:1.75-slim-bullseye
ARG APP=/usr/src/app
RUN apt update \
    && apt install -y curl ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/*
ENV TZ=Etc/UTC \
    APP_USER=appuser
RUN groupadd $APP_USER \
    && useradd -g $APP_USER $APP_USER \
    && mkdir -p ${APP}
COPY --from=builder /rust/target/release/das_api ${APP}
RUN chown -R $APP_USER:$APP_USER ${APP}
USER $APP_USER
WORKDIR ${APP}
CMD /usr/src/app/das_api
