## DAS Backfill

The DAS Backfill library facilitates the initial setup and data backfilling for DAS, focusing on the bubblegum program. This program's indexing heavily relies on transaction data. While the library supports parallel backfilling across different trees, it ensures that transactions within each tree are processed sequentially. This approach guarantees accurate representation of every modification in the merkle tree within DAS.

## Usage

```rust
use das_backfill::{
  BubblegumBackfillArgs,
  BubblegumBackfillContext,
  start_bubblegum_backfill
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let database_pool = sqlx::PgPool::connect("your_database_url").await?;
    let solana_rpc = Rpc::new("your_solana_rpc_url");

    let context = BubblegumBackfillContext::new(database_pool, solana_rpc);
    let args = BubblegumBackfillArgs::parse(); // Parses args from CLI

    start_bubblegum_backfill(context, args).await
}
```
