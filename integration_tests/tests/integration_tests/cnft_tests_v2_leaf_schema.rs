use function_name::named;

use das_api::api::{self, ApiContract};

use itertools::Itertools;

use serial_test::serial;

use super::common::*;

// TODO: Adjust this so that it can be run from anywhere.
// Do not move this test name or tests will break because the snapshot name and location will change.
pub async fn run_get_asset_scenario_test(
    setup: &TestSetup,
    asset_id: &str,
    seeds: Vec<SeedEvent>,
    order: Order,
) {
    let seed_permutations: Vec<Vec<&SeedEvent>> = match order {
        Order::AllPermutations => seeds.iter().permutations(seeds.len()).collect::<Vec<_>>(),
        Order::Forward => vec![seeds.iter().collect_vec()],
    };

    for events in seed_permutations {
        apply_migrations_and_delete_data(setup.db.clone()).await;
        index_seed_events(setup, events).await;
        let request = api::GetAsset {
            id: asset_id.to_string(),
            ..api::GetAsset::default()
        };

        let response = setup.das_api.get_asset(request).await.unwrap();
        insta::assert_json_snapshot!(setup.name.clone(), response);
    }
}

#[tokio::test]
#[serial]
#[named]
async fn test_mint_v2_to_collection_transfer_v2() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let asset_id = "75YC6nCeRDTQcHyPiXAXC3qov4u8gyBYCR4tci3ZrCPt";

    let seeds: Vec<SeedEvent> = seed_txns([
        // mint_v2 to collection
        "WsRbKgNXubCZUB4Pv9QmZDnxFJeZWxcvwbCnUUG7KgbByRML6pAosSYTdFddNkU1puqRG3E3evNqmWNB5EX4m2g",
        // transfer_v2
        "4MxRc1TakjW3cDtX7ZSMdGu5owDTSWTvfFepBLi3x9SE6xfeD1parzQC7hmfN664NgLocPdfpHTADNWg3n8VbatZ",
    ]);

    run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
}

#[tokio::test]
#[serial]
#[named]
async fn test_mint_v2_to_collection_burn_v2() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let asset_id = "74jWt9uZrs6aJ1eEqswhSMSbJYTnysiAuBjYgh6ikcAD";

    let seeds: Vec<SeedEvent> = seed_txns([
        // mint_v2 to collection
        "29Sfa4vsABmq8PAgoNmAmJ3ittgJGjNjoZa5hnwPwwsm7ctdnFLa2ug81whhiN7mXyvwFZWyGzNU7n8dd55ktn3H",
        // burn_v2
        "utGWqqYXpU2XjNPHyfP8t3J5CYLyfEEpiXrNrvtDdoUQLcA6uB1zkXwNcPGydckdZWXnsS3TRkE5d2TV9GGjK8E",
    ]);

    run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
}

#[tokio::test]
#[serial]
#[named]
async fn test_delegate_and_freeze_v2() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let asset_id = "39yBMNhayqjPPoyCoaHsMTpzZEqHSh6j9UmHG1ZeJvNm";

    let seeds: Vec<SeedEvent> = seed_txns([
        // mint_v2
        "5EGtGA3pys7en1m79CFpRPieTshEtkiYrqfgpZUHxbU2AmzcjsjTpGq8aop4FW4A53JnuvZcqLnC9a3vZiKvkJh7",
        // delegate_and_freeze_v2
        "3FEjLPikuzqL67MYJyjYQLGyLyVRYQ1TcxDdEPQQjhLZBKgWcHNB2RiYqYSmozwjYvgoGLBHo7fcY5Ja4YPDLGAe",
    ]);

    run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
}

#[tokio::test]
#[serial]
#[named]
async fn test_thaw_and_revoke_v2() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let asset_id = "E59c8LNXhYD9Gh93UqKoQXdKD7qCUmKnzT3p3EQLYeHj";

    let seeds: Vec<SeedEvent> = seed_txns([
        // mint_v2
        "rBMrxLUJqy2veGRSy82MLEYqcAo6FrvsovSkFTwRoKWJJXjzgZkrrU7YqGXitLNAaPoiaf7ETCyENcQdxe3qyfL",
        // delegate_and_freeze_v2
        "3bMpRY9Eq8eLMsq4WzpVTPXM8PHFauPH7DjNQCQca8qtrRihmSgCUrsUaVx7Nydn76m2mCUzDMKs327TeQps2jdP",
        // thaw_and_revoke_v2
        "5QMtikA2DUNWrAESBgwvS8zTHhj6rAejpgQg3HCuURiYdAB4XLvJT7xrj9NkKPN7vTXcDjXWzLP9J4oxiW2JipMJ",
    ]);

    run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
}

#[tokio::test]
#[serial]
#[named]
async fn test_delegate_v2() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let asset_id = "YnBykqbzHvNsC4q3L9DK1W9J1te57FV3eB3NMERvyB8";

    let seeds: Vec<SeedEvent> = seed_txns([
        // mint_v2
        "2sZbpWUHiL2TEA48CsYGKwYBeefo54PTMVT8Pc9PMrRNnPBpJm4uUHUsfob6FtU58pEpnYhrKmhqEcYwDJYjntW7",
        // delegate_v2
        "38ErTmuwXXX3NZ3pwK81hTzjHyMQH7yC65mppPHUMW83KE6cSZrYtp23nmv58U7QUMQy4WmfvEQZ8GHQibt1y2hz",
    ]);

    run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
}

#[tokio::test]
#[serial]
#[named]
async fn test_freeze_v2() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let asset_id = "CTs4CB81qEZCfkJeBNc4K4XyRKvAGQK5mQmmfdLenqVP";

    let seeds: Vec<SeedEvent> = seed_txns([
        // mint_v2
        "4xRavuyJ8Sa1nEhyqrUeNpYHgopkpnEWWSYYoTut48CDJRrT46aN1CnU2KCgNUBfpLngNvG9SUkMfHGGNR5x5pZa",
        // delegate_v2
        "7z3eHy8z6UsGLedCxPihTdUtpQS14Fd9aLkKbRZfjcZRCf9nX2x8cyvtV9QGQJTkhpmk11HoYpzF26AvSwuUpQx",
        // freeze_v2
        "5tRgiuLGebPVQu6ytPNsW6RVWyvM3pkvSPgRc72Qkdvu7rwoBKvaenHkND4qD3JQAWL1CkuCMwnAm91w42tUYg77",
    ]);

    run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
}

#[tokio::test]
#[serial]
#[named]
async fn test_freeze_v2_using_permanent_delegate() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let asset_id = "GE4t6bZza7p8dx7aRmR6S5NcbNjmZ8SePZwgoSVSuWds";

    let seeds: Vec<SeedEvent> = seed_txns([
        // mint_v2
        "256W5Pf9mvKEnLZdFZCZiXaLhLUveJw8KYxUz7pj4AGAvNV1G8XsuFL99PTKSjJg8VaNfuEG3ybkah2D6xBWszDp",
        // freeze_v2
        "3ScdX5stXc8xvLRdQdVVZxms1iqC9srVEGiRbJsEGM5nieizCvpCSJx1Fgz53DBt34ZxcX7T1ePbDtCmVTVfZ374",
    ]);

    run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
}

#[tokio::test]
#[serial]
#[named]
async fn remove_from_collection_using_set_collection_v2() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let asset_id = "Beter35aQLYBh1JbaNX8XM4qiTpwHDRiAxCCNKcS3goF";

    let seeds: Vec<SeedEvent> = seed_txns([
        // mint_v2
        "4mSnGCxijXiLySxnotCDgVfU8b3xh6zskKvRiHktDxaJ38SnKpvtosz6vKNNqMAYRHTmtnkhS6LFN6hQYVYyM6pp",
        // set_collection_v2
        "3LEkoAGe5ZL2bN2BbiV2np66X15Zr1TfzBGXG9fMXwBRswFL5GLHGSZvBFjpzJKxtQutMh5sVbuZcKgdbgckym8n",
    ]);

    run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
}
