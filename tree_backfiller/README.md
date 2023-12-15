# Tree Backfiller

The Tree Backfiller crawls all trees on-chain and backfills any transactions related to a tree that have not already been observed.

## Commands

Command line arguments can also be set through environment variables.

### Run

The `run` command initiates the crawling and backfilling process. It requires the Solana RPC URL, the database URL, and the messenger Redis URL.

```
Usage: das-tree-backfiller run [OPTIONS] --solana-rpc-url <SOLANA_RPC_URL> --database-url <DATABASE_URL> --messenger-redis-url <MESSENGER_REDIS_URL>

Options:
      --solana-rpc-url <SOLANA_RPC_URL>
          Solana RPC URL [env: SOLANA_RPC_URL=]
      --tree-crawler-count <TREE_CRAWLER_COUNT>
          Number of tree crawler workers [env: TREE_CRAWLER_COUNT=] [default: 100]
      --signature-channel-size <SIGNATURE_CHANNEL_SIZE>
          The size of the signature channel. This is the number of signatures that can be queued up. [env: SIGNATURE_CHANNEL_SIZE=] [default: 10000]
      --queue-channel-size <QUEUE_CHANNEL_SIZE>
          [env: QUEUE_CHANNEL_SIZE=] [default: 1000]
      --database-url <DATABASE_URL>
          [env: DATABASE_URL=postgres://solana:solana@localhost:5432/solana]
      --database-max-connections <DATABASE_MAX_CONNECTIONS>
          [env: DATABASE_MAX_CONNECTIONS=] [default: 125]
      --database-min-connections <DATABASE_MIN_CONNECTIONS>
          [env: DATABASE_MIN_CONNECTIONS=] [default: 5]
      --messenger-redis-url <MESSENGER_REDIS_URL>
          [env: MESSENGER_REDIS_URL=redis://localhost:6379]
      --messenger-redis-batch-size <MESSENGER_REDIS_BATCH_SIZE>
          [env: MESSENGER_REDIS_BATCH_SIZE=] [default: 100]
      --messenger-stream-max-buffer-size <MESSENGER_STREAM_MAX_BUFFER_SIZE>
          [env: MESSENGER_STREAM_MAX_BUFFER_SIZE=] [default: 10000000]
      --metrics-host <METRICS_HOST>
          [env: METRICS_HOST=] [default: 127.0.0.1]
      --metrics-port <METRICS_PORT>
          [env: METRICS_PORT=] [default: 8125]
  -h, --help
          Print help
```

### Metrics

The Tree Backfiller provides several metrics for monitoring performance and status:

Metric | Description
--- | ---
transaction.failed | Count of failed transaction
transaction.queued | Time for a transaction to be queued
tree.crawled | Time to crawl a tree
tree.completed | Count of completed tree crawl
tree.failed | Count of failed tree crawls
job.completed | Time to complete the job
