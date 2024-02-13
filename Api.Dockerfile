# Stage: Build Application
FROM rust:1.74-bullseye AS chef
RUN cargo install cargo-chef

## Setup Work Directory
FROM chef AS builder
COPY das_api /rust/das_api/
COPY docker/api/Cargo.toml /rust
COPY digital_asset_types /rust/digital_asset_types
WORKDIR /rust/das_api
RUN cargo chef prepare --recipe-path recipe.json

## Build Dependencies
RUN apt-get update -y && \
    apt-get install -y build-essential make git
WORKDIR /
RUN mkdir -p /rust/das_api
WORKDIR /rust/das_api
COPY das_api .

## Build application
RUN cargo build --release


# Stage: Run Application Release
FROM rust:1.73-slim-bullseye
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
