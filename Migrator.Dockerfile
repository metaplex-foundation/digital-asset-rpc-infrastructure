FROM rust:1.73-bullseye
COPY init.sql /init.sql
COPY migration /migration
ENV INIT_FILE_PATH=/init.sql
COPY digital_asset_types /digital_asset_types
WORKDIR /migration
RUN cargo build --release
WORKDIR /migration/target/release
CMD /migration/target/release/migration up -n 100
