#!/bin/bash

# Output colors.
RED() { echo $'\e[1;31m'$1$'\e[0m'; }
GRN() { echo $'\e[1;32m'$1$'\e[0m'; }

SCENARIOS=("mint_transfer_burn.scenario" \
"mint_redeem_decompress.scenario"
)
EXPECTED_ASSET_VALUES=("mint_transfer_burn_asset.txt" \
"mint_redeem_decompress_asset.txt"
)
EXPECTED_CL_ITEMS_VALUES=("mint_transfer_burn_cl_items.txt"
"mint_redeem_decompress_cl_items.txt"
)

if [ ${#SCENARIOS[@]} -gt 0 ]; then
    echo "Running ${#SCENARIOS[@]} scenarios"
else
    echo "NO SCENARIOS FOUND!"
    exit 1
fi

# 0 is pass, 1 is fail.
STATUS=0

# Run each scenario and check for expected database result.
for i in ${!SCENARIOS[@]}; do

    # Initially this asset should not be in `asset`` table.
    if [ -f "${EXPECTED_ASSET_VALUES[$i]}" ]; then
        EXPECTED_ASSET_VALUE=$(<"${EXPECTED_ASSET_VALUES[$i]}")
        ASSET_ID=$(echo "$EXPECTED_ASSET_VALUE" | grep -oP '^(?!tree_id).*id\s+\|\s+\K[^ ]+')
        if [ ${#ASSET_ID} -ne 66 ]; then
            echo $(RED "${SCENARIOS[$i]} incorrect asset ID parsing")
            STATUS=1
            continue
        fi

        SQL="select * from asset where id = '$ASSET_ID';"
        DATABASE_VAL=$(PGPASSWORD=solana psql -h localhost -U solana -x --command="$SQL")
        
        if [ "(0 rows)" == "$DATABASE_VAL" ]; then
            echo $(GRN "${SCENARIOS[$i]} initial asset table passed")
        else
            echo $(RED "${SCENARIOS[$i]} initial asset table failed")
            STATUS=1
        fi
    fi

    # Initially this tree should not be in `cl_items`` table.
    if [ -f "${EXPECTED_CL_ITEMS_VALUES[$i]}" ]; then
        EXPECTED_CL_ITEMS=$(<"${EXPECTED_CL_ITEMS_VALUES[$i]}")
        TREE_ID=$(echo "$EXPECTED_CL_ITEMS" | grep -oP '^\s*\K\\x[0-9a-f]+' | head -n 1)
        if [ ${#TREE_ID} -ne 66 ]; then
            echo $(RED "${SCENARIOS[$i]} incorrect asset ID parsing")
            STATUS=1
            continue
        fi

        SQL="select tree, node_idx, leaf_idx, seq, level, hash from cl_items where tree = '$TREE_ID' order by level;"
        DATABASE_VAL=$(PGPASSWORD=solana psql -h localhost -U solana -x --command="$SQL")
        
        if [ "(0 rows)" == "$DATABASE_VAL" ]; then
            echo $(GRN "${SCENARIOS[$i]} initial cl_items table passed")
        else
            echo $(RED "${SCENARIOS[$i]} initial cl_items table failed")
            STATUS=1
        fi
    fi

    # Run the scenario file that indexes the asset.  These are done with separate calls to the `txn_forwarder`
    # in order to enforce order.  Just calling the `txn_forwarder` with the file results in random ordering.
    readarray -t TXS < "${SCENARIOS[$i]}"

    for TX in ${TXS[@]}; do
        (cd .. && \
        cargo run -- \
        --redis-url 'redis://localhost/' \
        --rpc-url 'https://api.devnet.solana.com' \
        single \
        --txn  "$TX" \
        2>&1 | grep -v "Group already exists: BUSYGROUP: Consumer Group name already exists")
    done

    sleep 5

    # Asset should now be in `asset`` table and all fields except `created_at` date match.
    if [ -f "${EXPECTED_ASSET_VALUES[$i]}" ]; then
        SQL="select * from asset where id = '$ASSET_ID';"
        DATABASE_VAL=$(PGPASSWORD=solana psql -h localhost -U solana -x --command="$SQL")
        DATABASE_VAL_NO_DATE=$(sed '/^created_at/d' <<< "$DATABASE_VAL")

        if [ "$EXPECTED_ASSET_VALUE" == "$DATABASE_VAL_NO_DATE" ]; then
            echo $(GRN "${SCENARIOS[$i]} asset table passed")
        else
            echo $(RED "${SCENARIOS[$i]} asset table failed")
            echo "Asset ID: $ASSET_ID"
            echo "Expected:"
            echo "$EXPECTED_ASSET_VALUE"
            echo "Actual:"
            echo "$DATABASE_VAL_NO_DATE"
            STATUS=1
        fi
    fi

    # Tree should now be in `cl_items`` table and all fields match.
    if [ -f "${EXPECTED_CL_ITEMS_VALUES[$i]}" ]; then
        SQL="select tree, node_idx, leaf_idx, seq, level, hash from cl_items where tree = '$TREE_ID' order by level;"
        DATABASE_VAL=$(PGPASSWORD=solana psql -h localhost -U solana --command="$SQL")

        if [ "$EXPECTED_CL_ITEMS" == "$DATABASE_VAL" ]; then
            echo $(GRN "${SCENARIOS[$i]} cl_items table passed")
        else
            echo $(RED "${SCENARIOS[$i]} cl_items table failed")
            echo "Tree ID: $TREE_ID"
            echo "Expected:"
            echo "$EXPECTED_CL_ITEMS"
            echo "Actual:"
            echo "$DATABASE_VAL"

            STATUS=1
        fi
    fi

    echo ""
done

echo ""
if [ $STATUS -eq 1 ]; then
    echo $(RED "SOME TESTS FAILED!")
else
    echo $(GRN "ALL TESTS PASSED!")
fi

exit $STATUS
