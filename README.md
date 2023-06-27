## IMPORTANT: See Prerequisites below

## Digital Asset RPC API Infrastructure
This repo houses the API Ingester and Database Types components of the Metaplex Digital Asset RPC API. Together these 
components are responsible for the aggregation of Solana Validator Data into an extremely fast and well typed api. This 
api provides a nice interface on top of the Metaplex programs. It abstracts the byte layout on chain, allows for 
super-fast querying and searching, as well as serves the merkle proofs needed to operate over compressed nfts. 

### Components
1. Ingester -> A background processing system that gets messages from a [Messenger](https://github.com/metaplex-foundation/digital-asset-validator-plugin), and uses [BlockBuster](https://github.com/metaplex-foundation/blockbuster) Parsers to store the canonical representation of Metaplex types in a storage system. This system also holds the re-articulated Merkle tree that supports the compressed NFTs system.
2. Api -> A JSON Rpc api that serves Metaplex objects. This api allows filtering, pagination and searching over Metaplex data. This data includes serving the merkle proofs for the compressed NFTs system. It is intended to be run right alongside the Solana RPC and works in much the same way. Just like the solana RPC takes data from the validator and serves it in a new format, so this api takes data off the validator and serves it.

The API specification is located here https://github.com/metaplex-foundation/api-specifications
This spec is what providers of this api must implement against.

### Infrastructure and Deployment Examples
Along with the above rust binaries, this repo also maintains examples and best practice settings for running the entire infrastructure. 
The example infrastructure is as follows. 

* A Solana No-Vote Validator - This validator is configured to only have secure access to the validator ledger and account data under consensus.
* A Geyser Plugin (Plerkle) - The above validator is further configured to load this geyser plugin that sends Plerkle Serialized Messages over a messaging system.
* A Redis Cluster (Stream Optimized) - The example messaging system is a light weight redis deployment that supports the streaming configuration.
* A Kubernetes Cluster - The orchestration system for the API and Ingester processes. Probably overkill for a small installation, but it's a rock solid platform for critical software.

This repo houses Helm Charts, Docker files and Terraform files to assist in the deployment of the example infrastructure.

### Developing

#### Prerequisites:
You must clone the https://github.com/metaplex-foundation/blockbuster repo, this is un publishable for now due to active development in like 1000 branches and serious mathematics avoiding dependency hell.

Because this is a multi component system the easiest way to develop or locally test this system is with docker but developing locally without docker is possible.

#### Regenerating DB Types
Edit the init.sql, then run `docker compose up db`
Then with a local `DATABASE_URL` var exported like this `export DATABASE_URL=postgres://solana:solana@localhost/solana` you can run
` sea-orm-cli generate entity -o ./digital_asset_types/src/dao/generated/ --database-url $DATABASE_URL --with-serde both --expanded-format`

If you need to install `sea-orm-cli` run `cargo install sea-orm-cli`.

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
INGESTER_MESSENGER_CONFIG: '{messenger_type="Redis", connection_config={ redis_connection_str="redis://redis" } }' #your redis
INGESTER_RPC_CONFIG: '{url="http://validator:8899", commitment="finalized"}' # your solana validator or same network rpc, if local you must use your solana instance running localy
```

```bash
cargo run -p nft_ingester
```


When making changes you will need to stop the cargo process and re-run. Someday we will have auto rebuild for local cargo stuff but for now you are on your own.

#### NOTE
```
INGESTER_ROLE 
```
This environment variable can be used to split the work load.

All for a combined setup
Ingester for just the Listeners to txn and acct
Backfiller for just the backfiller scheduler and notifyer
Background for just the background tasks.

For production you should split the coponents up.

### Developing With Docker
Developing with Docker is much easier, but has some nuances to it. This test docker compose system relies on a programs folder being accessible, this folder needs to have the shared object files for the following programs
* Token Metadata
* Bubblegum
* Gummyroll
* Token 2022
* Latest version of the Associated token program

You need to run the following script (which takes a long time) in order to get all those .so files.

```bash
chmod +x ./prepare-local-docker-env.sh
./prepare-local-docker-env.sh
```
This script grabs all the code for these programs and compiles it, and chucks it into your programs folder. Go grab some coffe because this will take a while/
If you get some permissions errors, just sudo delete the programs directory and start again.

#### Authentication with Docker and AWS

```aws ecr-public get-login-password --region us-east-1 | docker login --username AWS --password-stdin {your aws container registry}```

#### Running the application

We use ``docker-compose`` to build the multi-container Docker application.  On some systems its ``docker compose``.
```bash
docker-compose build 
```
This builds the docker container for API and the Ingester components and will download the appropriate Redis, Postgres and Solana+plerkle docker images.
Keep in mind that the version `latest` on the Solana Validator image will match the latest version available on the docs, for other versions please change that version in your docker compose file.

```bash
docker-compose up 
```

#### Developing

When making changes you will need to ``docker compose up --build --force-recreate`` again to get the latest changes.
Also when mucking about with the docker file if your gut tells you that something is wrong, and you are getting build errors run `docker compose build --no-cache`

Sometimes you will want to delete the db do so with `sudo rm -rf db-data`.  You can also delete the ledger with `sudo rm -rf ledger`.

#### Logs
To get a reasonable amount of logs while running Docker, direct grafana logs to a file:
```
grafana:
    ...
    environment:
      ...
      - GF_LOG_MODE=file
```
and set Solana Rust logs to error level:
```
  solana:
    ...
    environment:
      RUST_LOG: error
```

#### Interacting with API

Once everything is working you can see that there is a api being served on
```
http://localhost:9090
```
And a Metrics System on
```
http://localhost:3000
```

Here are some example requests to the Read API:

```bash
curl --request POST --url http://localhost:9090 --header 'Content-Type: application/json' --data '{
    "jsonrpc": "2.0",
    "method": "getAssetsByOwner",
    "params": [
      "CMvMqPNKHikuGi7mrngvQzFeQ4rndDnopx3kc9drne8M",
      { "sortBy": "created", "sortDirection": "asc"},
      50,
      1,
      "",
      ""
    ],
    "id": 0
}' | json_pp

curl --request POST --url http://localhost:9090 --header 'Content-Type: application/json' --data '{
    "jsonrpc": "2.0",
    "method": "getAsset",
    "params": [
      "8vw7tdLGE3FBjaetsJrZAarwsbc8UESsegiLyvWXxs5A"
    ],
    "id": 0
}' | json_p

curl --request POST --url http://localhost:9090 --header 'Content-Type: application/json' --data '{
    "jsonrpc": "2.0",
    "method": "getAssetProof",
    "params": [
      "8vw7tdLGE3FBjaetsJrZAarwsbc8UESsegiLyvWXxs5A"
    ],
    "id": 0
}' | json_pp
```

# Deploying to Kubernetes 
Using skaffold you can deploy to k8s, make sure you authenticate with your docker registry

Make sure you have the env vars you need to satisfy this part of the skaffold.yaml
```yaml
...
    setValueTemplates:
      ingest.db_url: "{{.DATABASE_URL}}"
      ingest.rpc_url: "{{.RPC_URL}}"
      ingest.redis_url: "{{.REDIS_URL}}"
      metrics.data_dog_api_key: "{{.DATA_DOG_API}}"
      load.seed: "{{.LOAD_SEED}}"
      load.rpc_url: "{{.RPC_URL}}"
      valuesFiles:
        - ./helm/ingest/values.yaml
  - name: das-api
    chartPath: helm/api
    artifactOverrides:
      image: public.ecr.aws/k2z7t6t6/metaplex-rpc-api
    setValueTemplates:
      api.db_url: "{{.DATABASE_URL}}"
      api.redis_url: "{{.REDIS_URL}}"
      metrics.data_dog_api_key: "{{.DATA_DOG_API}}"
...
```
```bash
skaffold build --file-output skaffold-state.json --cache-artifacts=false
## Your namepsace may differ.
skaffold deploy -p devnet --build-artifacts skaffold-state.json --namespace devnet-read-api --tail=true
```

# METRICS
Here are the metrics that various parts of ths system expose;

## NFT INGESTER
### ACKING
count ingester.ack - number of messages acked tagged by stream

count ingester.stream.ack_error - error acking a message
count ingester.stream.receive_error - error getting stream data

### Stream Metrics
ingester.stream_redelivery - Stream tagged of messages re delivered
ingester.stream_size - Size of stream, tagged by stream
ingester.stream_size_error - Error getting the stream size

### Stream Specific Metrics
All these metrics are tagged by stream
count ingester.seen
time ingester.proc_time
count ingester.ingest_success
count ingester.ingest_redeliver_success
count ingester.not_implemented
count ingester.ingest_error

### BG Tasks
time ingester.bgtask.proc_time
count ingester.bgtask.success
count ingester.bgtask.error
count ingester.bgtask.network_error
time ingester.bgtask.bus_time
count ingester.bgtask.identical

### BACKFILLER
count ingester.backfiller.task_panic
count ingester.backfiller.task_error
guage ingester.backfiller.missing_trees

### Startup
ingester.startup

## API
api_call





