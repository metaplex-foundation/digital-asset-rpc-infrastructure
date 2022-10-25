FROM rust:1.63-bullseye AS chef
RUN cargo install cargo-chef
FROM chef AS planner
COPY nft_ingester /rust/nft_ingester/
WORKDIR /rust/nft_ingester
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
RUN apt-get update -y && \
    apt-get install -y build-essential make git
COPY digital_asset_types /rust/digital_asset_types
WORKDIR /
#RUN git clone https://github.com/metaplex-foundation/blockbuster
#RUN cd blockbuster; git fetch origin && git checkout origin/alex/temp; cd ..
RUN mkdir -p /rust/nft_ingester
WORKDIR /rust/nft_ingester
COPY --from=planner /rust/nft_ingester/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
COPY nft_ingester/Cargo.toml .
RUN cargo chef cook --release --recipe-path recipe.json
COPY nft_ingester .
# Build application
RUN cargo build --release

FROM rust:1.63-slim-bullseye
ARG APP=/usr/src/app
RUN apt update \
    && apt install -y curl ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/*
ENV TZ=Etc/UTC \
    APP_USER=appuser
RUN groupadd $APP_USER \
    && useradd -g $APP_USER $APP_USER \
    && mkdir -p ${APP}
COPY --from=builder /rust/nft_ingester/target/release/nft_ingester ${APP}
RUN chown -R $APP_USER:$APP_USER ${APP}
USER $APP_USER
WORKDIR ${APP}
CMD /usr/src/app/nft_ingester
