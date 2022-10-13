FROM rust:1.63-bullseye AS chef
RUN cargo install cargo-chef
FROM chef AS planner
COPY das_api /rust/das_api/
WORKDIR /rust/das_api
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
RUN apt-get update -y && \
    apt-get install -y build-essential make git
COPY digital_asset_types /rust/digital_asset_types
RUN mkdir -p /rust/das_api
WORKDIR /rust/das_api
COPY --from=planner /rust/das_api/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
COPY das_api/Cargo.toml .
RUN cargo chef cook --release --recipe-path recipe.json
COPY das_api .
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
COPY --from=builder /rust/das_api/target/release/das_api ${APP}
RUN chown -R $APP_USER:$APP_USER ${APP}
USER $APP_USER
WORKDIR ${APP}
CMD /usr/src/app/das_api
