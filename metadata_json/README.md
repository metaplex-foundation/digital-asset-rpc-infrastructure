# DAS Metadata JSON Indexer CLI

## Overview
The DAS Metadata JSON Indexer CLI is a tool for indexing metadata JSON associated with tokens. It supports operations such as ingesting new metadata and backfilling missing metadata, as well as providing metrics and performance tuning options.

## Features
- **Ingest**: Process and index new metadata JSON files with various configuration options.
- **Backfill**: Fill in missing metadata for previously indexed tokens with configurable parameters.
- **Metrics**: Collect and send metrics to a specified host and port.

## Installation
Ensure you have Rust installed on your machine. If not, install it from [the official Rust website](https://www.rust-lang.org/).


```
cargo run --bin das-metadata-json -- --help 
```

## Usage

### Ingest Command

To continuously process metadata JSON, the METADATA_JSON Redis stream is monitored. Upon reading an ID from the stream, the ingest loop lookups the corresponding asset_data using the ID within the DAS DB, fetches the metadata JSON, and then updates the asset_data record with the retrieved metadata.
```
das-metadata-json ingest [OPTIONS] --messenger-redis-url <MESSENGER_REDIS_URL> --database-url <DATABASE_URL>
```

#### Options
- `--messenger-redis-url <MESSENGER_REDIS_URL>`: The Redis URL for the messenger service.
- `--messenger-redis-batch-size <MESSENGER_REDIS_BATCH_SIZE>`: Batch size for Redis operations (default: 100).
- `--metrics-host <METRICS_HOST>`: Host for sending metrics (default: 127.0.0.1).
- `--metrics-port <METRICS_PORT>`: Port for sending metrics (default: 8125).
- `--metrics-prefix <METRICS_PREFIX>`: Prefix for metrics (default: das.backfiller).
- `--database-url <DATABASE_URL>`: The database URL.
- `--database-max-connections <DATABASE_MAX_CONNECTIONS>`: Maximum database connections (default: 125).
- `--database-min-connections <DATABASE_MIN_CONNECTIONS>`: Minimum database connections (default: 5).
- `--timeout <TIMEOUT>`: Timeout for operations in milliseconds (default: 1000).
- `--queue-size <QUEUE_SIZE>`: Size of the job queue (default: 1000).
- `--worker-count <WORKER_COUNT>`: Number of worker threads (default: 100).
- `-h, --help`: Print help information.

### Backfill Command

To backfill any `asset_data` marked for indexing with `reindex=true`:

```
das-metadata-json backfill [OPTIONS] --database-url <DATABASE_URL>
```

#### Options
- `--database-url <DATABASE_URL>`: The database URL.
- `--database-max-connections <DATABASE_MAX_CONNECTIONS>`: Maximum database connections (default: 125).
- `--database-min-connections <DATABASE_MIN_CONNECTIONS>`: Minimum database connections (default: 5).
- `--metrics-host <METRICS_HOST>`: Host for sending metrics (default: 127.0.0.1).
- `--metrics-port <METRICS_PORT>`: Port for sending metrics (default: 8125).
- `--metrics-prefix <METRICS_PREFIX>`: Prefix for metrics (default: das.backfiller).
- `--queue-size <QUEUE_SIZE>`: Size of the job queue (default: 1000).
- `--worker-count <WORKER_COUNT>`: Number of worker threads (default: 100).
- `--timeout <TIMEOUT>`: Timeout for operations in milliseconds (default: 1000).
- `--batch-size <BATCH_SIZE>`: Number of records to process in a single batch (default: 1000).
- `-h, --help`: Print help information.

## Lib

The `das-metadata-json` crate provides a `sender` module which can be integrated in a third-party service (eg `nft_ingester`) to push asset data IDs for indexing. To configure follow the steps below:

### Configuration

1. **Set up the `SenderArgs`:** Ensure that the `nft_ingester` is configured with the necessary `SenderArgs`. These arguments include the Redis URL, batch size, and the number of queue connections. For example:

```rust
let sender_args = SenderArgs {
messenger_redis_url: "redis://localhost:6379".to_string(),
messenger_redis_batch_size: "100".to_string(),
messenger_queue_connections: 5,
};
```

2. **Initialize the `SenderPool`:** Use the `try_from_config` async function to create a `SenderPool` instance from the `SenderArgs`. This will set up the necessary channels and messengers for communication.

```rust
let sender_pool = SenderPool::try_from_config(sender_args).await?;
```

3. **Push Asset Data IDs for Indexing:** With the `SenderPool` instance, you can now push asset data IDs to be indexed using the `push` method. The IDs should be serialized into a byte array before being sent. The `asset_data` record should be written to the database before pushing its ID. 

```rust
let message = asset_data.id;

sender_pool.push(&message).await?;
```

Within the `nft_ingester`, the `sender_pool` is orchestrated by the `TaskManager`. When configured appropriately, upon receiving a `DownloadMetadata` task, the `task_manager` will forego the usual process of creating a task record. Instead, it will directly push the asset ID to the `METADATA_JSON` Redis stream. This action queues the ID for processing by the `das-metadata-json` indexer, streamlining the workflow for indexing metadata JSON.

## Configuration
The CLI can be configured using command-line options or environment variables. For options that have an associated environment variable, you can set the variable instead of passing the option on the command line.

## Logging
Logging is managed by `env_logger`. Set the `RUST_LOG` environment variable to control the logging level, e.g., `RUST_LOG=info`.

## Error Handling
The CLI provides error messages for any issues encountered during execution.