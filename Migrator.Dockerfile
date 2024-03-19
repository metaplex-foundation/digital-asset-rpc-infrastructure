FROM rust:1.75-bullseye
COPY init.sql /init.sql
ENV INIT_FILE_PATH=/init.sql

COPY Cargo.toml /
COPY ./core /core
COPY ./das_api /das_api
COPY ./digital_asset_types /digital_asset_types
COPY ./integration_tests /integration_tests
COPY ./metaplex-rpc-proxy /metaplex-rpc-proxy
COPY ./migration /migration
COPY ./nft_ingester /nft_ingester
COPY ./ops /ops
COPY ./tools /tools

WORKDIR /migration
RUN cargo build --release
WORKDIR /target/release
CMD /target/release/migration up -n 100