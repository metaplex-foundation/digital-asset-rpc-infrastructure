## Dev setup

> **Note:** Run these commands from the root of the project

### Run redis, postgres and prometheus docker containers

```bash
docker compose up db redis prometheus
```

### Seed the database and run migrations

```bash
INIT_FILE_PATH=./init.sql sea migrate up --database-url=postgres://solana:solana@localhost:5432/solana
```

### Configs

Example config files are available at
- [./config-grpc2redis.example.yml](./config-grpc2redis.example.yml)
- [./config-ingester.example.yml](./config-ingester.example.yml)
- [./config-monitor.example.yml](./config-monitor.example.yml) 

Copy these files and modify them as needed to setup the project. 


### Run grpc2redis service

This service will listen to geyser gRPC account and transaction updates from triton's Dragon's Mouth gRPC. It makes multiple subscriptions to the gRPC stream and filter the data based on the config. The data (vec of bytes) is pushed to a pipeline and then flushed to redis at regular intervals.

> **Note:** Log level can be set to `info`, `debug`, `warn`, `error`

```bash
RUST_LOG=info cargo run --bin das-grpc-ingest  -- --config grpc-ingest/config-grpc2redis.yml grpc2redis
```

### Config for Ingester [./config-ingester.yml](./config-ingester.yml)

### Run the Ingester service

This service performs many concurrent tasks

- Fetch account updates from redis and process them using the `program_transformer` crate
- Fetch transaction updates from redis and processe them
- Fetch snapshots from redis and process them
- download token metedata json and store them in postgres db

```bash
 RUST_LOG=debug,sqlx=warn cargo run --bin das-grpc-ingest  -- --config grpc-ingest/config-ingester.yml ingester
```

### Metrics

Both grpc2redis and ingester services expose prometheus metrics and can be accessed at `http://localhost:9090/metrics`
