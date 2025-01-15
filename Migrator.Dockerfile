FROM das-api/builder AS files

FROM rust:1.79-bullseye
COPY init.sql /init.sql
ENV INIT_FILE_PATH=/init.sql
COPY --from=files /das/migration /bins/migration
CMD /bins/migration up -n 100