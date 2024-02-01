# Integration Package

This Cargo package helps us run multi-package tests in our workspace. This setup is inspired by the tokio integration test setup.

## Setup

First setup a local Postgres database and export the postgres database URL as follows:
```export DATABASE_TEST_URL=postgres://postgres@localhost/<database_name>```

Also gain access to mainnet RPCs and devnet RPCs and export the URLs as follows. Currently,
we only use these URLs for reading data and storing it locally. 

```
export DEVNET_RPC_URL=...
export MAINNET_RPC_URL=...
```

Afterwards, you can simply run the following command to run tests:
```cargo test```


## How do tests work? 

Most tests currently are configured to run as "scenario" tests. They pull test input data from mainnet/devnet
and store it locally to avoid tests breaking if mainnet/devnet data ever changes. Afterwards, they feed
the tests to the `handle_account_update` and `handle_transaction` functions of the ingester and populate
the indexed data in the database. Finally, they create an instance of the `DasApi` struct, run queries against
this struct, store the results of these queries as snapshots through the `insta` testing library and assert that
future runs of the same test produce the same snapshot. 

Note that tests do not actually run the ingester binaries and the API binaries and only test the primary internal functions
of each, e.g.  `handle_account_update` and `handle_transaction` for the ingester and functions like `search_assets` 
and `get_asset` for the binary. By following this approach, we are able to test the vast majority of the code
in a way that's easy to setup and fast to run -- avoiding to have to compile and run multiple binaries.
