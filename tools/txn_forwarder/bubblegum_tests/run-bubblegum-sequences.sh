#!/bin/bash

# Pass `reverse` to run scenarios in reverse.
case $1 in
    "reverse"|"--reverse"|"-r")
        REVERSE="true"
        ;;
    *)
        REVERSE="false"
        ;;
esac

SCENARIOS=("mint_transfer_burn.scenario" \
"mint_redeem_decompress.scenario"
"mint_redeem_cancel_redeem_redeem_decompress.scenario" \
"mint_transfer_transfer.scenario" \
"mint_delegate_transfer.scenario" \
"mint_verify_creator.scenario" \
"mint_unverify_creator.scenario" \
"mint_verify_collection.scenario" \
"mint_verify_collection_unverify_collection.scenario" \
"mint_set_and_verify_collection.scenario" \
"mint_to_collection_unverify_collection.scenario" \
"mint_verify_creator_unverify_creator_update_metadata.scenario" \
"mint_update_metadata_creator_position_3_to_2.scenario" \
"mint_update_metadata_remove_all_creators.scenario"
)

TEST_SCENARIO_DATA_DIR="test_scenario_data"

# Output text in colors.
# $1 is output text.
RED() { echo $'\e[1;31m'$1$'\e[0m'; }
GRN() { echo $'\e[1;32m'$1$'\e[0m'; }

# Read from database using psql and compare to expected value.
# $1 is SQL command.
# $2 extra CLI args to send to psql.
# $3 is expected value.
# $4 is topic for the pass/fail message.
# Returns 0 if database value matches expected value, otherwise returns 1.
CHECK_DATABASE() {
    local DATABASE_VAL=$(PGPASSWORD=solana psql -h localhost -U solana "$2" --command="$1")
    # Remove `created_at` since the date changes for `asset` table entries.
    local DATABASE_VAL=$(sed '/^created_at/d' <<< "$DATABASE_VAL")
    if [ "$3" = "$DATABASE_VAL" ]; then
        echo $(GRN "${SCENARIOS[$i]} $4 passed") >&2
        return 0
    else
        echo $(RED "${SCENARIOS[$i]} $4 failed") >&2
        echo "Asset ID: $ASSET_ID" >&2
        echo "Expected:" >&2
        echo "$3" >&2
        echo ""
        echo "Actual:" >&2
        echo "$DATABASE_VAL" >&2
        return 1
    fi
}

# Read in expected data from test data file.  If the $REVERSE flag is set to "true" then first
# look for a file with the `_reverse` suffix. If one does not exist, use the default filename
# for that suffix.
# $1 is scenario file to use as a base name.
# $2 is the suffix for the type of test data, i.e. "asset", "cl_items", etc.
# If successful, prints contents of file on stdout and returns 0, otherwise returns 1.
READ_IN_EXPECTED_DATA() {
    local BASE_NAME=$(basename "$1" .scenario)

    local EXPECTED_DATA_FILE="$TEST_SCENARIO_DATA_DIR/${BASE_NAME}_"$2".txt"
    if [ -f "$EXPECTED_DATA_FILE" ]; then
        cat "$EXPECTED_DATA_FILE"
        return 0
    else
        echo $(RED "$1 missing $2 file") >&2
        return 1
    fi
}

if [ "$REVERSE" = "true" ]; then
    echo "Running ${#SCENARIOS[@]} scenarios in reverse"
else
    echo "Running ${#SCENARIOS[@]} scenarios forwards"
fi

# 0 is pass, 1 is fail.
STATUS=0

# Run each scenario and check for expected database result.
for i in ${!SCENARIOS[@]}; do
    # Read in the expected database data for this scenario.
    EXPECTED_ASSET_VALUE=$(READ_IN_EXPECTED_DATA "${SCENARIOS[$i]}" "asset") || { STATUS=1; continue; }
    EXPECTED_ASSET_DATA_VALUE=$(READ_IN_EXPECTED_DATA "${SCENARIOS[$i]}" "asset_data") || { STATUS=1; continue; }
    EXPECTED_ASSET_CREATORS=$(READ_IN_EXPECTED_DATA "${SCENARIOS[$i]}" "asset_creators") || { STATUS=1; continue; }
    EXPECTED_ASSET_GROUPING=$(READ_IN_EXPECTED_DATA "${SCENARIOS[$i]}" "asset_grouping") || { STATUS=1; continue; }
    EXPECTED_CL_ITEMS=$(READ_IN_EXPECTED_DATA "${SCENARIOS[$i]}" "cl_items") || { STATUS=1; continue; }

    # Parse out the asset ID.
    ASSET_ID=$(echo "$EXPECTED_ASSET_VALUE" | grep -oP '^(?!tree_id).*id\s+\|\s+\K[^ ]+')
    if [ ${#ASSET_ID} -ne 66 ]; then
        echo $(RED "${SCENARIOS[$i]} incorrect asset ID parsing")
        echo "Asset ID: $ASSET_ID"
        STATUS=1
        continue
    fi

    # Parse out the tree ID.
    TREE_ID=$(echo "$EXPECTED_CL_ITEMS" | grep -oP '^\s*\K\\x[0-9a-f]+' | head -n 1)
    if [ ${#TREE_ID} -ne 66 ]; then
        echo $(RED "${SCENARIOS[$i]} incorrect asset ID parsing")
        echo "Tree ID: $TREE_ID"
        STATUS=1
        continue
    fi

    # Initially this asset should not be in any database tables.
    ASSET_SQL="SELECT * FROM asset WHERE id = '$ASSET_ID';"
    CHECK_DATABASE "$ASSET_SQL" "-x" "(0 rows)" "initial asset table state" || STATUS=1

    ASSET_DATA_SQL="SELECT * FROM asset_data WHERE id = '$ASSET_ID';"
    CHECK_DATABASE "$ASSET_DATA_SQL" "-x" "(0 rows)" "initial asset table state" || STATUS=1

    ASSET_CREATORS_SQL="SELECT asset_id, creator, share, verified, seq, slot_updated, position \
        FROM asset_creators \
        WHERE asset_id = '$ASSET_ID' \
        ORDER BY position;"
    CHECK_DATABASE "$ASSET_CREATORS_SQL" "-x" "(0 rows)" "initial asset_creators table state" || STATUS=1

    ASSET_GROUPING_SQL="SELECT asset_id, group_key, group_value, seq, slot_updated, verified, group_info_seq \
        FROM asset_grouping \
        WHERE asset_id = '$ASSET_ID';"
    CHECK_DATABASE "$ASSET_GROUPING_SQL" "-x" "(0 rows)" "initial asset_grouping table state" || STATUS=1

    CL_ITEMS_SQL="select tree, node_idx, leaf_idx, seq, level, hash from cl_items where tree = '$TREE_ID' order by level;"
    CHECK_DATABASE "$CL_ITEMS_SQL" "-x" "(0 rows)" "initial cl_items table state" || STATUS=1

    # Run the scenario file that indexes the asset.  These are done with separate calls to the `txn_forwarder`
    # in order to enforce order.  Just calling the `txn_forwarder` with the file results in random ordering.
    readarray -t TXS < "$TEST_SCENARIO_DATA_DIR/${SCENARIOS[$i]}"

    if [ ${#TXS[@]} = 0 ]; then
        echo $(RED "No scenarios found for  ${SCENARIOS[$i]}!")
        STATUS=1
        continue
    fi

    # Reverse transactions if necessary.
    if [ "$REVERSE" = "true" ]; then
        REVERSED_TXS=()
        for ((j = ${#TXS[@]} - 1; j >= 0; j--)); do
            REVERSED_TXS+=("${TXS[j]}")
        done
        TXS=("${REVERSED_TXS[@]}")
    fi

    for TX in ${TXS[@]}; do
        (cd .. && \
        cargo run -- \
        --redis-url 'redis://localhost/' \
        --rpc-url 'https://api.devnet.solana.com' \
        single \
        --txn  "$TX" \
        2>&1 | grep -v "Group already exists: BUSYGROUP: Consumer Group name already exists")
    done

    sleep 2

    # Asset should now be in the database and all fields match (except `created_at` in `asset`` table).
    CHECK_DATABASE "$ASSET_SQL" "-x" "$EXPECTED_ASSET_VALUE" "asset table" || STATUS=1
    CHECK_DATABASE "$ASSET_DATA_SQL" "-x" "$EXPECTED_ASSET_DATA_VALUE" "asset_data table" || STATUS=1
    CHECK_DATABASE "$ASSET_CREATORS_SQL" "-x" "$EXPECTED_ASSET_CREATORS" "asset_creators table" || STATUS=1
    CHECK_DATABASE "$ASSET_GROUPING_SQL" "-x" "$EXPECTED_ASSET_GROUPING" "asset_grouping table" || STATUS=1
    CHECK_DATABASE "$CL_ITEMS_SQL" "" "$EXPECTED_CL_ITEMS" "cl_items table" || STATUS=1

    echo ""
done

if [ "$REVERSE" = "true" ]; then
    SUFFIX="IN REVERSE"
else
    SUFFIX="FORWARDS"
fi

if [ $STATUS -eq 1 ]; then
    echo $(RED "SOME TESTS FAILED $SUFFIX!")
else
    echo $(GRN "ALL TESTS PASSED $SUFFIX!")
fi

exit $STATUS
