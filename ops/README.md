### DAS Ops

DAS Ops is a collection of operational tools and scripts for managing and maintaining the Digital Asset RPC infrastructure.

> **Note:** Run these commands from the root of the project

### Setup

```bash
sudo docker compose up db
```

### Running the cli

```bash
cargo run --bin das-ops -- --help
```

#### Required Args

- `--solana-rpc-url` - RPC URL of the Solana cluster
- `--database-url` - URL of the Postgres database (if using Docker: `postgres://solana:solana@localhost:5432/solana`)

### Commands

- `account` : Account related operations

  #### Subcommands

  - `program <PROGRAM>` command is used to backfill the index against on-chain accounts owned by a program

  - `single <ACCOUNT>` command is used to backfill the index against a single account

  - `nft <MINT>` command is used to backfill the index against an NFT mint, token metadata, and token account

- `bubblegum` : Bubblegum program related operations

  #### Subcommands

  - `backfill` command is used to cross-reference the index against on-chain accounts. It crawls through trees and backfills any missed tree transactions.
  - `replay <TREE>` command is used to replay the Bubblegum program transactions for a given tree address and parse all the cNFT instructions
