FROM rust:1.73-bullseye
COPY init.sql /rust/init.sql
COPY migration /rust/migration
COPY docker/migrator/Cargo.toml /rust
ENV INIT_FILE_PATH=/rust/init.sql
COPY digital_asset_types /rust/digital_asset_types
WORKDIR /rust/migration
RUN cargo build --release
WORKDIR /rust/target/release
CMD /rust/target/release/migration up -n 100
