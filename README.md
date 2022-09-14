### This repo was originally built inside https://github.com/jarry-xiao/candyland as a join effort between Solana X Metaplex
This repo is in transition, and we are factoring out components from CandyLand here.

## Digital Asset RPC API Infrastructure
This repo houses the API Ingester and Database Types components of the Metaplex Digital Asset RPC API. Together these 
components are responsible for the aggregation of Solana Validator Data into an extremely fast and well typed api. This 
api provides a nice interface on top of the Metaplex programs. It abstracts the byte layout on chain, allows for 
super-fast querying and searching, as well as serves the merkle proofs needed to operate over compressed nfts. 

### Components
1. Ingester -> A background processing system that gets messages from a [Messenger](https://github.com/metaplex-foundation/digital-asset-validator-plugin), and uses [BlockBuster](https://github.com/metaplex-foundation/blockbuster) Parsers to store the canonical representation of Metaplex types in a storage system. This system also holds the re-articulated Merkle tree that supports the compressed NFTs system.
2. Api -> A JSON Rpc api that serves Metaplex objects. This api allows filtering, pagination and searching over Metaplex data. This data includes serving the merkle proofs for the compressed NFTs system. It is intended to be run right alongside the Solana RPC and works in much the same way. Just like the solana RPC takes data from the validator and serves it in a new format, so this api takes data off the validator and serves it.

### Infrastructure and Deployment Examples
Along with the above rust binaries, this repo also maintains examples and best practice settings for running the entire infrastructure. 
The example infrastructure is as follows. 

* A Solana No-Vote Validator - This validator is configured to only have secure access to the validator ledger and account data under consensus.
* A Geyser Plugin (Plerkle) - The above validator is further configured to load this geyser plugin that sends Plerkle Serialized Messages over a messaging system.
* A Redis Cluster (Stream Optimized) - The example messaging system is a light weight redis deployment that supports the streaming configuration.
* A Kubernetes Cluster - The orchestration system for the API and Ingester processes. Probably overkill for a small installation, but it's a rock solid platform for critical software.

This repo houses Helm Charts, Docker files and Terraform files to assist in the deployment of the example infrastructure.

### Developing

Because this is a multi component system the easiest way to develop or locally test this system is with docker but developing locally without docker is possible.

#### Developing Locally
 *Prerequisites* 
 * A Postgres Server running with the database setup according to ./init.sql
 * A Redis instance that has streams enabled or a version that supports streams
 * A local solana validator with the Plerkle plugin running.
 * Environment Variables set to allow your validator, ingester and api to access those prerequisites.

See [Plugin Configuration](https://github.com/metaplex-foundation/digital-asset-validator-plugin#building-locally) for how to locally configure the test validator plugin to work.

For the API you need the following environment variables:
```bash
APP_DATABASE_URL=postgres://solana:solana@db/solana  #change to your db host
APP_SERVER_PORT=9090
```

```bash
cargo run -p das_api
```

For the Ingester you need the following environment variables:
```bash
INGESTER_DATABASE_CONFIG: '{listener_channel="backfill_item_added", url="postgres://solana:solana@db/solana"}' # your database host
INGESTER_MESSENGER_CONFIG: '{redis_connection_str="redis://redis"}' #your redis
INGESTER_RPC_CONFIG: '{url="http://validator:8899", commitment="finalized"}' # your solana validator or same network rpc, if local you must use your solana instance running localy
```

```bash
cargo run -p nft_ingester
```

When making changes you will need to stop the cargo process and re-run. Someday we will have auto rebuild for local cargo stuff but for now you are on your own.

### Developing With Docker
Developing with Docker is much easier, but has some nuances to it. This test docker compose system relies on a programs folder being accessible, this folder needs to have the shared object files for the following programs
* Token Metadata
* Bubblegum
* Gummyroll
* Token 2022
* Latest version of the Associated token program

You need to run the following script (which takes a long time) in order to get all those .so files.

```bash
chmod
./dowload-programs.sh
```
This script grabs all the code for these programs and compiles it, and chucks it into your programs folder. Go grab some coffe because this will take a while/


We use ``docker-compose`` on some systems its ``docker compose``.
```bash
docker-compose build 

```
This builds the docker container for API and the Ingester components and will download the appropriate Redis, Postgres and Solana+plerkle docker images.
Keep in mind that the version `latest` on the Solana Validator image will match the latest version available on the docs, for other versions please change that version in your docker compose file.

```bash
docker-compose up 
```

When making changes you will need to ``docker compose up --force-recreate`` again to get the latest changes.
