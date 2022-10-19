FROM rust:1.63-bullseye
RUN cargo install sea-orm-cli
COPY init.sql /init.sql
COPY migration /migration
ENV INIT_FILE_PATH=/init.sql
WORKDIR /migration
RUN cargo build
WORKDIR /
CMD sea-orm-cli migrate up
