FROM rust:1.63-bullseye AS chef
RUN cargo install cargo-chef
FROM chef AS planner
COPY load_generation /rust/load_generation/
WORKDIR /rust/load_generation
RUN cargo chef prepare --recipe-path recipe.json
FROM chef AS builder
RUN apt-get update -y && \
    apt-get install -y build-essential make git
COPY load_generation /rust/load_generation
RUN mkdir -p /rust/load_generation
WORKDIR /rust/load_generation
COPY --from=planner /rust/load_generation/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
COPY load_generation/Cargo.toml .
RUN cargo chef cook --release --recipe-path recipe.json
COPY load_generation .
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
COPY --from=builder /rust/load_generation/target/release/load_generation ${APP}
RUN chown -R $APP_USER:$APP_USER ${APP}
USER $APP_USER
WORKDIR ${APP}
CMD /usr/src/app/load_generation
