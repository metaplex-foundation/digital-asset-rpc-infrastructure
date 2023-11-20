#!/bin/bash

# output colours
RED() { echo $'\e[1;31m'$1$'\e[0m'; }
GRN() { echo $'\e[1;32m'$1$'\e[0m'; }

CURRENT_DIR=$(pwd)
SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)
# go to parent folder
cd $(dirname $(dirname $SCRIPT_DIR))

EXTERNAL_ID=("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s" \
"BGUMAp9Gq7iTEuizy4pqaxsTyUCBK68MDfK752saRPUY" \
"cmtDvXumGCrqC1Age74AVPhSRVXJMd8PJS91L8KbNCK" \
"noopb9bkMVfRPU8AsbpTUg8AQkHtKwMYZiFUjNRtMmV" \
"ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL" \
"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA" \
"TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb" \
)
EXTERNAL_SO=("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s.so" \
"BGUMAp9Gq7iTEuizy4pqaxsTyUCBK68MDfK752saRPUY.so" \
"cmtDvXumGCrqC1Age74AVPhSRVXJMd8PJS91L8KbNCK.so" \
"noopb9bkMVfRPU8AsbpTUg8AQkHtKwMYZiFUjNRtMmV.so"
"ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL.so" \
"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA.so" \
"TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb.so" \
)

if [ -z ${RPC+x} ]; then
    RPC="https://api.devnet.solana.com"
fi

if [ -z "$OUTPUT" ]; then
    OUTPUT=$CURRENT_DIR/programs
fi

# creates the output directory if it doesn't exist
if [ ! -d ${OUTPUT} ]; then
    mkdir ${OUTPUT}
fi

# only prints this if we have external programs
if [ ${#EXTERNAL_ID[@]} -gt 0 ]; then
    echo "Dumping external programs to: '${OUTPUT}'"
fi

# dump external programs binaries if needed
for i in ${!EXTERNAL_ID[@]}; do
    if [ ! -f "${OUTPUT}/${EXTERNAL_SO[$i]}" ]; then
        solana program dump -u $RPC ${EXTERNAL_ID[$i]} ${OUTPUT}/${EXTERNAL_SO[$i]}
    else
        solana program dump -u $RPC ${EXTERNAL_ID[$i]} ${OUTPUT}/onchain-${EXTERNAL_SO[$i]} > /dev/null
        ON_CHAIN=`sha256sum -b ${OUTPUT}/onchain-${EXTERNAL_SO[$i]} | cut -d ' ' -f 1`
        LOCAL=`sha256sum -b ${OUTPUT}/${EXTERNAL_SO[$i]} | cut -d ' ' -f 1`

        if [ "$ON_CHAIN" != "$LOCAL" ]; then
            echo $(RED "[ WARNING ] on-chain and local binaries are different for '${EXTERNAL_SO[$i]}'")
        else
            echo "$(GRN "[ SKIPPED ]") on-chain and local binaries are the same for '${EXTERNAL_SO[$i]}'"
        fi

        rm ${OUTPUT}/onchain-${EXTERNAL_SO[$i]}
    fi
done

# only prints this if we have external programs
if [ ${#EXTERNAL_ID[@]} -gt 0 ]; then
    echo ""
fi

cd ${CURRENT_DIR}
