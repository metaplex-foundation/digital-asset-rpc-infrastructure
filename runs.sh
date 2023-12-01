#!/usr/bin/env bash
#
# Run a minimal Solana cluster.  Ctrl-C to exit.
#
# Before running this script ensure standard Solana programs are available
# in the PATH, or that `cargo build` ran successfully
# change
#

set -e
cat << EOL > config.yaml
json_rpc_url: http://localhost:8899
websocket_url: ws://localhost:8899
commitment: finalized
EOL

mkdir plugin-config && true
if [[ ! -f /plugin-config/accountsdb-plugin-config.json ]]
then
cat << EOL > /plugin-config/accountsdb-plugin-config.json
    {
        "libpath": "/plugin/plugin.so",
        "accounts_selector" : {
            "owners" : [
                "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s",
                "GRoLLMza82AiYN7W9S9KCCtCyyPRAQP2ifBy4v4D5RMD",
                "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb",
                "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
                "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL",
                "BGUMAp9Gq7iTEuizy4pqaxsTyUCBK68MDfK752saRPUY",
                "cndy3Z4yapfJBmL3ShUp5exZKqR3z33thTzeNMm2gRZ",
                "CndyV3LdqHUfDLmE5naZjVN8rBZz4tqhdefbAnjHG3JR",
                "Guard1JwRhJkVH6XZhzoYxeBVQe872VH6QggF4BWmS9g"
            ]
        },
        "transaction_selector" : {
            "mentions" : [
                "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s",
                "GRoLLMza82AiYN7W9S9KCCtCyyPRAQP2ifBy4v4D5RMD",
                "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb",
                "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
                "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL",
                "BGUMAp9Gq7iTEuizy4pqaxsTyUCBK68MDfK752saRPUY",
                "cndy3Z4yapfJBmL3ShUp5exZKqR3z33thTzeNMm2gRZ",
                "CndyV3LdqHUfDLmE5naZjVN8rBZz4tqhdefbAnjHG3JR",
                "Guard1JwRhJkVH6XZhzoYxeBVQe872VH6QggF4BWmS9g"
            ]
        }
    }
EOL
fi

programs=()
if [ "$(ls -A /so)" ]; then
  for prog in /so/*; do
      programs+=("--bpf-program" "$(basename $prog .so)" "$prog")
  done
fi

clone=("EtXbhgWbWEWamyoNbSRyN5qFXjFbw8utJDHvBkQKXLSL" "UgfWEZpFdY1hPhEnuYHe24u9xR74xTSvT1uZxxiwymM")
clones=()
if [[ -n "${clone[@]}" ]]; then
    for address in "${clone[@]}"; do
        clones+=("--clone" "$address")
    done
fi

export RUST_BACKTRACE=1
dataDir=$PWD/config/"$(basename "$0" .sh)"
ledgerDir=$PWD/config/ledger
mkdir -p "$dataDir" "$ledgerDir"
echo $ledgerDir
echo $dataDir
echo ${clones[@]}
ls -la /so/
args=(
  --config config.yaml
  --reset
  --limit-ledger-size 10000000000000000
  --rpc-port 8899
  --geyser-plugin-config /plugin-config/accountsdb-plugin-config.json
  --clone EtXbhgWbWEWamyoNbSRyN5qFXjFbw8utJDHvBkQKXLSL --clone UgfWEZpFdY1hPhEnuYHe24u9xR74xTSvT1uZxxiwymM
  --url https://devnet.helius-rpc.com/?api-key=ccca5bb2-58dc-4b94-838b-664df478cf45
)

# args+=("--url devnet")

# shellcheck disable=SC2086
cat /plugin-config/accountsdb-plugin-config.json
ls -la /so/

apt update && apt install ca-certificates -y && update-ca-certificates
solana-test-validator  "${programs[@]}" "${clones[@]}" "${args[@]}" $SOLANA_RUN_SH_VALIDATOR_ARGS > /dev/null