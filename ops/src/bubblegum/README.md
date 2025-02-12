# Bubblegum
The bubblegum CLI assists in detecting and indexing missing updates for compression trees that are managed by the MPL bubblegum program.
## Commands
Command line arguments can also be set through environment variables.

### Backfill

The `backfill` command initiates the crawling and backfilling process. It requires the Solana RPC URL, the database URL.

**warning**: The command expects full archive access to transactions. Before proceeding ensure your RPC is able to serve complete transaction history for Solana.  