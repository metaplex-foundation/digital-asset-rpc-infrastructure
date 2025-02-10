#!/bin/bash

# output colours
RED() { echo $'\e[1;31m'$1$'\e[0m'; }
GRN() { echo $'\e[1;32m'$1$'\e[0m'; }

CURRENT_DIR=$(pwd)
SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)
# go to parent folder
cd $(dirname $(dirname $SCRIPT_DIR))

EXTERNAL_ID_MAINNET=("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s" \
"cmtDvXumGCrqC1Age74AVPhSRVXJMd8PJS91L8KbNCK" \
"noopb9bkMVfRPU8AsbpTUg8AQkHtKwMYZiFUjNRtMmV" \
"ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL" \
"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA" \
"TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb" \
)

EXTERNAL_ID_DEVNET=("BGUMAp9Gq7iTEuizy4pqaxsTyUCBK68MDfK752saRPUY" \
"mcmt6YrQEMKw8Mw43FmpRLmf7BqRnFMKmAcbxE3xkAW" \
"mnoopTCrg4p8ry25e4bcWA9XZjbNjMTfgYVGGEdRsf3" \
)

RPC_MAINNET="https://api.mainnet-beta.solana.com"
RPC_DEVNET="https://api.devnet.solana.com"
OUTPUT=$CURRENT_DIR/programs

# check for command existence
command -v solana >/dev/null 2>&1 || { echo $(RED "[ ERROR ] solana CLI not found"); exit 1; }
command -v sha256sum >/dev/null 2>&1 || { echo $(RED "[ ERROR ] sha256sum not found"); exit 1; }

# creates the output directory if it doesn't exist
mkdir -p ${OUTPUT}

dump_programs() {
    local RPC_URL=$1
    shift
    local IDS=("$@")

    for i in "${!IDS[@]}"; do
        local ID=${IDS[$i]}
        local SO_FILE="${ID}.so"
        local ONCHAIN_SO_FILE="onchain-${SO_FILE}"

        if [ ! -f "${OUTPUT}/${SO_FILE}" ]; then
            solana program dump -u "$RPC_URL" "$ID" "${OUTPUT}/${SO_FILE}" || {
                echo $(RED "[  ERROR  ] Failed to dump program '${SO_FILE}'")
                exit 1
            }
        else
            solana program dump -u "$RPC_URL" "$ID" "${OUTPUT}/${ONCHAIN_SO_FILE}" > /dev/null || {
                echo $(RED "[  ERROR  ] Failed to dump program '${SO_FILE}'")
                exit 1
            }

            ON_CHAIN=$(sha256sum -b "${OUTPUT}/${ONCHAIN_SO_FILE}" | cut -d ' ' -f 1)
            LOCAL=$(sha256sum -b "${OUTPUT}/${SO_FILE}" | cut -d ' ' -f 1)

            if [[ "$ON_CHAIN" != "$LOCAL" ]]; then
                echo $(RED "[ WARNING ] on-chain and local binaries are different for '${SO_FILE}'")
            else
                echo "$(GRN "[ SKIPPED ]") on-chain and local binaries are the same for '${SO_FILE}'"
            fi


            rm "${OUTPUT}/${ONCHAIN_SO_FILE}"
        fi
    done
}

if [ ${#EXTERNAL_ID_MAINNET[@]} -gt 0 ]; then
    echo "Dumping external accounts from mainnet to '${OUTPUT}':"
    dump_programs "$RPC_MAINNET" "${EXTERNAL_ID_MAINNET[@]}"
fi

if [ ${#EXTERNAL_ID_DEVNET[@]} -gt 0 ]; then
    echo ""
    echo "Dumping external accounts from devnet to '${OUTPUT}':"
    dump_programs "$RPC_DEVNET" "${EXTERNAL_ID_DEVNET[@]}"
fi

# only prints this if we have external programs
if [ ${#EXTERNAL_ID_MAINNET[@]} -gt 0 ] || [ ${#EXTERNAL_ID_DEVNET[@]} -gt 0 ]; then
    echo ""
fi

cd ${CURRENT_DIR}
