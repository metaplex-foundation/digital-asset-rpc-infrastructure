#!/bin/bash

# Output colors.
RED() { echo $'\e[1;31m'$1$'\e[0m'; }
GRN() { echo $'\e[1;32m'$1$'\e[0m'; }

SCENARIOS=("mint_transfer_burn.scenario" \
"mint_redeem_decompress.scenario"
"mint_redeem_cancel_redeem_redeem_decompress.scenario" \
"mint_transfer_transfer.scenario" \
"mint_delegate_transfer.scenario"
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
    # Read in the `asset` file for this scenario.
    EXPECTED_ASSET_FILE="$(basename "${SCENARIOS[$i]}" .scenario)_asset.txt"
    if [ ! -f "$EXPECTED_ASSET_FILE" ]; then
        echo $(RED "${SCENARIOS[$i]} missing asset file")
        STATUS=1
        continue
    fi
    EXPECTED_ASSET_VALUE=$(<"$EXPECTED_ASSET_FILE")

    # Parse out the asset ID.
    ASSET_ID=$(echo "$EXPECTED_ASSET_VALUE" | grep -oP '^(?!tree_id).*id\s+\|\s+\K[^ ]+')
    if [ ${#ASSET_ID} -ne 66 ]; then
        echo $(RED "${SCENARIOS[$i]} incorrect asset ID parsing")
        echo "Asset ID: $ASSET_ID"
        STATUS=1
        continue
    fi

    # Initially this asset should not be in `asset`` table.
    ASSET_SQL="SELECT * FROM asset WHERE id = '$ASSET_ID';"
    DATABASE_VAL=$(PGPASSWORD=solana psql -h localhost -U solana -x --command="$ASSET_SQL")
    if [ "(0 rows)" == "$DATABASE_VAL" ]; then
        echo $(GRN "${SCENARIOS[$i]} initial asset table passed")
    else
        echo $(RED "${SCENARIOS[$i]} initial asset table failed")
        STATUS=1
    fi

    # Read in the `asset_creators` file for this scenario.
    EXPECTED_ASSET_CREATORS_FILE="$(basename "${SCENARIOS[$i]}" .scenario)_asset_creators.txt"
    if [ ! -f "$EXPECTED_ASSET_CREATORS_FILE" ]; then
        echo $(RED "${SCENARIOS[$i]} missing asset_creators file")
        STATUS=1
        continue
    fi
    EXPECTED_ASSET_CREATORS=$(<"$EXPECTED_ASSET_CREATORS_FILE")

    # Initially this asset should not be in `asset_creators`` table.
    ASSET_CREATORS_SQL="SELECT asset_id, creator, share, verified, seq, slot_updated, position \
        FROM asset_creators \
        WHERE asset_id = '$ASSET_ID' \
        ORDER BY position;"
    DATABASE_VAL=$(PGPASSWORD=solana psql -h localhost -U solana -x --command="$ASSET_CREATORS_SQL")
    if [ "(0 rows)" == "$DATABASE_VAL" ]; then
        echo $(GRN "${SCENARIOS[$i]} initial asset_creators table passed")
    else
        echo $(RED "${SCENARIOS[$i]} initial asset_creators table failed")
        STATUS=1
    fi

    # Read in the `asset_grouping` file for this scenario.
    EXPECTED_ASSET_GROUPING_FILE="$(basename "${SCENARIOS[$i]}" .scenario)_asset_grouping.txt"
    if [ ! -f "$EXPECTED_ASSET_GROUPING_FILE" ]; then
        echo $(RED "${SCENARIOS[$i]} missing asset_grouping file")
        STATUS=1
        continue
    fi
    EXPECTED_ASSET_GROUPING=$(<"$EXPECTED_ASSET_GROUPING_FILE")

    # Initially this asset should not be in `asset_grouping`` table.
    ASSET_GROUPING_SQL="SELECT asset_id, group_key, group_value, seq, slot_updated, verified, group_info_seq \
        FROM asset_grouping \
        WHERE asset_id = '$ASSET_ID';"
    DATABASE_VAL=$(PGPASSWORD=solana psql -h localhost -U solana -x --command="$ASSET_GROUPING_SQL")
    if [ "(0 rows)" == "$DATABASE_VAL" ]; then
        echo $(GRN "${SCENARIOS[$i]} initial asset_grouping table passed")
    else
        echo $(RED "${SCENARIOS[$i]} initial asset_grouping table failed")
        STATUS=1
    fi

    # Read in the `cl_items` file for this scenario.
    EXPECTED_CL_ITEMS_FILE="$(basename "${SCENARIOS[$i]}" .scenario)_cl_items.txt"
    if [ ! -f "$EXPECTED_CL_ITEMS_FILE" ]; then
        echo $(RED "${SCENARIOS[$i]} missing cl_items file")
        STATUS=1
        continue
    fi
    EXPECTED_CL_ITEMS=$(<"$EXPECTED_CL_ITEMS_FILE")

    # Parse out the tree ID.
    TREE_ID=$(echo "$EXPECTED_CL_ITEMS" | grep -oP '^\s*\K\\x[0-9a-f]+' | head -n 1)
    if [ ${#TREE_ID} -ne 66 ]; then
        echo $(RED "${SCENARIOS[$i]} incorrect asset ID parsing")
        echo "Tree ID: $TREE_ID"
        STATUS=1
        continue
    fi

    # Initially this tree should not be in `cl_items`` table.
    SQL="select tree, node_idx, leaf_idx, seq, level, hash from cl_items where tree = '$TREE_ID' order by level;"
    DATABASE_VAL=$(PGPASSWORD=solana psql -h localhost -U solana -x --command="$SQL")
    if [ "(0 rows)" == "$DATABASE_VAL" ]; then
        echo $(GRN "${SCENARIOS[$i]} initial cl_items table passed")
    else
        echo $(RED "${SCENARIOS[$i]} initial cl_items table failed")
        STATUS=1
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

    sleep 3

    # Asset should now be in `asset` table and all fields except `created_at` date match.
    DATABASE_VAL=$(PGPASSWORD=solana psql -h localhost -U solana -x --command="$ASSET_SQL")
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

    # Asset should now be in `asset_creators` table and all fields match.
    DATABASE_VAL=$(PGPASSWORD=solana psql -h localhost -U solana -x --command="$ASSET_CREATORS_SQL")
    if [ "$EXPECTED_ASSET_CREATORS" == "$DATABASE_VAL" ]; then
        echo $(GRN "${SCENARIOS[$i]} asset_creators table passed")
    else
        echo $(RED "${SCENARIOS[$i]} asset_creators table failed")
        echo "Asset ID: $ASSET_ID"
        echo "Expected:"
        echo "$EXPECTED_ASSET_CREATORS"
        echo "Actual:"
        echo "$DATABASE_VAL"
        STATUS=1
    fi

    # Asset should now be in `asset_grouping` table and all fields match.
    DATABASE_VAL=$(PGPASSWORD=solana psql -h localhost -U solana -x --command="$ASSET_GROUPING_SQL")
    if [ "$EXPECTED_ASSET_GROUPING" == "$DATABASE_VAL" ]; then
        echo $(GRN "${SCENARIOS[$i]} asset_grouping table passed")
    else
        echo $(RED "${SCENARIOS[$i]} asset_grouping table failed")
        echo "Asset ID: $ASSET_ID"
        echo "Expected:"
        echo "$EXPECTED_ASSET_GROUPING"
        echo "Actual:"
        echo "$DATABASE_VAL"
        STATUS=1
    fi

    # Tree should now be in `cl_items`` table and all fields match.
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

    echo ""
done

echo ""
if [ $STATUS -eq 1 ]; then
    echo $(RED "SOME TESTS FAILED!")
else
    echo $(GRN "ALL TESTS PASSED!")
fi

exit $STATUS
