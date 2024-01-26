FROM rust:1.73-bullseye
COPY init.sql /init.sql
ENV INIT_FILE_PATH=/init.sql

COPY Cargo.toml /
COPY ./das_api /das_api
COPY ./digital_asset_types /digital_asset_types
COPY ./metaplex-rpc-proxy /metaplex-rpc-proxy
COPY ./migration /migration
COPY ./nft_ingester /nft_ingester
COPY ./tools /tools

WORKDIR /migration
RUN cargo build --release
WORKDIR /target/release
CMD /target/release/migration up -n 100