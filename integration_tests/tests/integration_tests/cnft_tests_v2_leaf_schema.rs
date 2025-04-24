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

    let asset_id = "86c2p9hZJTPWmFRKz1znX3yx9E8vhESk9wr9soTMET8Y";

    let seeds: Vec<SeedEvent> = vec![
        // mpl-core collection
        seed_account("9r8CKUNFd3zGV5cDcQNf9iiMgkavYqGLBFLLiT4bgwA9"),
        // create_tree_v2
        seed_txn("3YhVTyruarmcBkXqLnsayvWnTdr4mePotTMoZpbR1zCwwPRjpWeKyhsyZghRwreFb6tWWW7fDwKEhVAfgutdfytB"),
        // mint_v2 to collection
        seed_txn("5cGMXY7TtktrjWqCvLCfj7c9dLK3tLBEpRfE8z5z5z9y2nGrtDy2LiHWhD8F2Rrw1kzyJp8M6DMM1W9jGX18uWe6"),
        // transfer_v2
        seed_txn("3c6n4rpEJVda9uzf2wQ1VRWTCJgPkxQsyZ9evkJBfF16mK7cjRiJK7Cc2RKsfLyomLAtKTaGPFJ7V1bbMGjDdFy2"),
    ];

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

    let asset_id = "FJDXSjkTpx7zZm3mcxzFzHPJ4EMLzgU9DaMtMvHPY6em";

    let seeds: Vec<SeedEvent> = vec![
        // mpl-core collection
        seed_account("4eej2HLDBinhs3z8im3gZXAxXCaMYRWvWvKh8krYfks1"),
        // create_tree_v2
        seed_txn("3A7Dz5FKNG8W66oEhtgCeSiZ8sK6ccGX1VEj2Y8eQgqNpaC5dX3mTUH2kPDJU6P1ffCp8A8nGg6AZi29Pk1tna7C"),
         // mint_v2 to collection
        seed_txn("KKhuXFHtCVLHzoxxcbfwbihMWymGkhmk8E3ziFVxU8PnMAyzdumoQxwWWF5iWs6HBGpUYqBT4Qu8iqVpheJRzaR"),
        // burn_v2
        seed_txn("4NN4jhuk3ZGAW5FGirkXq4DKDH7M21zquv31XPMBpG9PRQLw7v8fZ4KnbB82prNsHo3aAw8WrEZL1rvDgxRsnG7d"),
    ];

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

    let asset_id = "5ryKjZ554B9aSKWyEH4kprdjQSvRk3ioAVPgsxRqcDAf";

    let seeds: Vec<SeedEvent> = seed_txns([
        // create_tree_v2
        "4qEfnrrthh9AboGJW3vgSpZqSKTpme2kNwtW6vaJ7RxkGxP7Vrj3M9ZCLTnbM5JvJ3ZzrsqbgtLG5sN8NP9r1g3G",
        // mint_v2
        "21fSPgkdn3yRq98cFPxwmVpPHSNv6ohvB6MX9YUtnUXqzLYQQGS2jgQrYUh3FZt7zF3kJg83R682MkM4cMNagg8p",
        // delegate_and_freeze_v2
        "4FNqBuntwzK33bm7uym2euWJDhDneg7QWFfuysLY9JtKaZDLX7S6n4mrfmNUm3RhvCac2WAUBUm1nShjEUpF9Y4y",
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

    let asset_id = "J1iH8MiPeW6rFZP8hjM8PEXFAwaKpcHyb5mgsB1pji5r";

    let seeds: Vec<SeedEvent> = seed_txns([
        // create_tree_v2
        "5euRNfzK9rbBN8VoZLZRxYt7fLc9LwVuX1HCr1qUF9bmw5dDyBXHhDuKEJZPdr889qWep7Td1fYQZUFATfYybbeo",
        // mint_v2
        "549nExWBsWjgk7iiBucfqCThZoQPB8sFvFDfDEpq5EKoxgVNJFaNFgaov7HWy1ukoroPJUZRKGGBjEbYiKQSCBzD",
        // delegate_and_freeze_v2
        "3ixwwBUfHCnQxXr4LSAhnsBXqysATJKcDJ1ennEH7YYud8oeZ9tP44mAq5hkRb87XmrbNBDNex8Jpd1E8tKHmzxs",
        // thaw_and_revoke_v2
        "35mLcvhiNG1CdmoQFWFdFBgPPMqebsQLXooP3Sp6o5mLggZd8tM6LPH6i6ZAgW5Ln6xRMbiFd76VyzWioawccpfo",
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

    let asset_id = "6F3Dv73qYxay78WtGMqjB4voY7E8GSmYvbq8nDNroAeC";

    let seeds: Vec<SeedEvent> = seed_txns([
        // create_tree_v2
        "3W7UhsaTafHMNmtAChG6QR5CufT2bTr8zedsdPcfRqpSyGcoLNai7gdzoiU9EKStRSeH9drr4YhPhatrR678AXXs",
        // mint_v2
        "3JUUyeNBqkeY29dcJWHoqvRBcg8BjmoN1zhhbh9Wj37huQF4T516LLBcjUKdEjg5GbpD4C6g7xu7N65CEGZGwVin",
        // delegate_v2
        "3ASo447BVktq1ttrGYyFh6SKGH5CnCJDTtQYmc9ohXEDnTJahyf23m17fHu1oR7zCSdJgqYG7odeyr2Q7fVkzB1B",
    ]);

    run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
}

// #[tokio::test]
// #[serial]
// #[named]
// async fn test_freeze_v2() {
//     let name = trim_test_name(function_name!());
//     let setup = TestSetup::new_with_options(
//         name.clone(),
//         TestSetupOptions {
//             network: Some(Network::Devnet),
//         },
//     )
//     .await;

//     let asset_id = "CTs4CB81qEZCfkJeBNc4K4XyRKvAGQK5mQmmfdLenqVP";

//     let seeds: Vec<SeedEvent> = seed_txns([
//         // mint_v2
//         //"4xRavuyJ8Sa1nEhyqrUeNpYHgopkpnEWWSYYoTut48CDJRrT46aN1CnU2KCgNUBfpLngNvG9SUkMfHGGNR5x5pZa",
//         // delegate_v2
//         //"7z3eHy8z6UsGLedCxPihTdUtpQS14Fd9aLkKbRZfjcZRCf9nX2x8cyvtV9QGQJTkhpmk11HoYpzF26AvSwuUpQx",
//         // freeze_v2
//         //"5tRgiuLGebPVQu6ytPNsW6RVWyvM3pkvSPgRc72Qkdvu7rwoBKvaenHkND4qD3JQAWL1CkuCMwnAm91w42tUYg77",
//     ]);

//     run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
// }

// #[tokio::test]
// #[serial]
// #[named]
// async fn test_freeze_v2_using_permanent_delegate() {
//     let name = trim_test_name(function_name!());
//     let setup = TestSetup::new_with_options(
//         name.clone(),
//         TestSetupOptions {
//             network: Some(Network::Devnet),
//         },
//     )
//     .await;

//     let asset_id = "GE4t6bZza7p8dx7aRmR6S5NcbNjmZ8SePZwgoSVSuWds";

//     let seeds: Vec<SeedEvent> = seed_txns([
//         // mint_v2
//         //"256W5Pf9mvKEnLZdFZCZiXaLhLUveJw8KYxUz7pj4AGAvNV1G8XsuFL99PTKSjJg8VaNfuEG3ybkah2D6xBWszDp",
//         // freeze_v2
//         //"3ScdX5stXc8xvLRdQdVVZxms1iqC9srVEGiRbJsEGM5nieizCvpCSJx1Fgz53DBt34ZxcX7T1ePbDtCmVTVfZ374",
//     ]);

//     run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
// }

#[tokio::test]
#[serial]
#[named]
async fn test_mint_v2_not_to_collection() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let asset_id = "8FD8ZTQtPzPzADnSUs1sb3vb54MxQ29mJkcEDTGzDANA";

    let seeds: Vec<SeedEvent> = seed_txns([
        // create_tree_v2
        "3pZiE42rP6UboZDfKQQQy6iepSM7y6H4LxUtENdwfdWHYi47aorefqZFFyQnv89AjraHEAVnzBbfjyY25T4vLUxS",
        // mint_v2
        "2TACVad54zBTy747ZfHfKmMMhqkhqMSwzQxYfMhJJHiVVgj2fYZNGrFxCAM5nn75WB1fw6aGGnEX4K8Qfzun4NTg",
    ]);

    run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
}

#[tokio::test]
#[serial]
#[named]
async fn test_mint_v2_to_collection_only() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let asset_id = "HGu4vJMLCboGNNVnH6UXrQZKVVXBX4xMZFxfmFgQD9pG";

    let seeds: Vec<SeedEvent> = vec![
        // mpl-core collection
        seed_account("7KdCe4Wbjdt6HLAW8LtgHg4c1pTHkmw4R48JW3EBRPLz"),
        // create_tree_v2
        seed_txn("5cLbxjgHkfqqnoSnceS1hMVHqX5h2yB7kTWEtYr4WWX8oLdmLSB4gceAqZaeXZxUW4avC9yVGqX48gTMqDeDdZ5b"),
        // mint_v2 to collection
        seed_txn("2mxbw9TdN7TszyPSmEWPEJPx3eGRJoURabwPvAPyrvYxwoq8Z9zfEYHdFc8YkkHb2jzk5SCfq8WJ4Vv64KgLFye4"),
    ];

    run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
}

// #[tokio::test]
// #[serial]
// #[named]
// async fn remove_from_collection_using_set_collection_v2() {
//     let name = trim_test_name(function_name!());
//     let setup = TestSetup::new_with_options(
//         name.clone(),
//         TestSetupOptions {
//             network: Some(Network::Devnet),
//         },
//     )
//     .await;

//     let asset_id = "GiDMyLuE1atPPvXHQSsCJ5T8Jdx7S3B3eD98WTQYVEXw";

//     let seeds: Vec<SeedEvent> = seed_txns([
//         // mint_v2
//         //"CaZKRiS7uqe5oA6puGFYzkhQhK6KdxnjBMxff63yPAv2Vv6MDKqLWeuMK81dEVyFQAXYQfA4noe8LB6aU3zBB9j",
//         // set_collection_v2
//         //"4A1XJgXsPpBDEN5p8p6zfZA6QsirhvUtJtM2VW1CrNkUhL73gguJt5HooCYQMX9L7nVtPEwfmhSyqjUBqXFGmhds",
//     ]);

//     run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
// }

// #[tokio::test]
// #[serial]
// #[named]
// async fn add_asset_to_collection_using_set_collection_v2() {
//     let name = trim_test_name(function_name!());
//     let setup = TestSetup::new_with_options(
//         name.clone(),
//         TestSetupOptions {
//             network: Some(Network::Devnet),
//         },
//     )
//     .await;

//     let asset_id = "H8aUdS5GPKtEt75nt4rrSc5SzkUZWxMfujYs2ZNfEXpp";

//     let seeds: Vec<SeedEvent> = seed_txns([
//         // mint_v2
//         //"48TH8Tffc2pi2my17gv2uc3a4dzqdgfvZmte3BUjJox1o75yUkTa7QYPFfYrMb2CR6Hc3w5qFLSvEemafvpn2s79",
//         // set_collection_v2
//         //"2XVxLofFvhymE9zHvEYckikBSfwDukmEFDgQqLNBMbrf6UJxnLVNR55N7ompAu2cp3WENBh53vxY7iUmKWyQwJeX",
//     ]);

//     run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
// }

// #[tokio::test]
// #[serial]
// #[named]
// async fn change_collection_using_set_collection_v2() {
//     let name = trim_test_name(function_name!());
//     let setup = TestSetup::new_with_options(
//         name.clone(),
//         TestSetupOptions {
//             network: Some(Network::Devnet),
//         },
//     )
//     .await;

//     let asset_id = "CHj5SK2Hmd5qpSxtf4UK93wAm93NFVRc27C2LafkP9tA";

//     let seeds: Vec<SeedEvent> = seed_txns([
//         // mint_v2
//         //"2kcu2LCRRdBNTkejqVJRuPMKGsi3bGjFAhy2G2BmkqR4RpevWmQRg82caUAddZ1CpVPkqc215eosemAnvCH4o5bV",
//         // set_collection_v2
//         //"65PHY53oRZkqPGfaXpy47MKV9nS5v6kDHdJd2apxVPPsYhcAoTNPzfyYsiuawRmYLqot1av6sb4EK8F1p3jiq1pQ",
//     ]);

//     run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
// }

// #[tokio::test]
// #[serial]
// #[named]
// async fn test_set_non_transferable_v2() {
//     let name = trim_test_name(function_name!());
//     let setup = TestSetup::new_with_options(
//         name.clone(),
//         TestSetupOptions {
//             network: Some(Network::Devnet),
//         },
//     )
//     .await;

//     let asset_id = "D519CjLrU3c24j2fmKXaraBbUYMN2tYZZHJzE3NpJBa9";

//     let seeds: Vec<SeedEvent> = seed_txns([
//         // mint_v2
//         //"3SqRh3aaXM2mj6DNTCNcHbcbc957qCatXuYS6QpZfhz7hpYMUEA7tTUGT2wFusCeXpVEYX7jyTB5YKQyy3jFZ4P8",
//         // set_non_transferable_v2
//         //"2URh6VofoPcPmKm7a5KsYwu3NociNZZdsxeaMUHQhz9D4am5YLhdkkpr5myx47bfp3AaBngzMsMSCPbN4W6xYN53",
//     ]);

//     run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
// }

// #[tokio::test]
// #[serial]
// #[named]
// async fn test_mint_v2_transfer_v2() {
//     let name = trim_test_name(function_name!());
//     let setup = TestSetup::new_with_options(
//         name.clone(),
//         TestSetupOptions {
//             network: Some(Network::Devnet),
//         },
//     )
//     .await;

//     let asset_id = "6H3vEmt8dy8cZnsikDzkZSSvuH7yJvbeKAWTYYvvt2es";

//     let seeds: Vec<SeedEvent> = seed_txns([
//         // mint_v2
//         //"LAfk7AmWYGV7UC3vGnWok1ctFSDWT4Extch7YQzTbeX8LFp52UkceMavzqnqN3fxHakKfiFHbcnLjpJi9nwLzTo",
//         // transfer_v2
//         //"3puKBnzpwnG7FcyAanib3MTHimz3hdSjBVUhQXMGiW4Jj79V2WaMHVp83D9E2HrCiNvkrjxq1Po17rAVHAp1NLGG",
//     ]);

//     run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
// }

// #[tokio::test]
// #[serial]
// #[named]
// async fn test_verify_creator_v2() {
//     let name = trim_test_name(function_name!());
//     let setup = TestSetup::new_with_options(
//         name.clone(),
//         TestSetupOptions {
//             network: Some(Network::Devnet),
//         },
//     )
//     .await;

//     let asset_id = "xBSehSHSvAFfyFfKNVdUnfKAQRT6yWvhbKogJTsGDDS";

//     let seeds: Vec<SeedEvent> = seed_txns([
//         // mint_v2
//         //"3WnSUUv3w3abdRiEKoxxnNV2CEoSFA7uxQRQHPPmi917AS5vea6d8XeioGXok9eUsxxNEW7ZXPpAvEJxMjjMPCgq",
//         // verify_creator_v2
//         //"Z8cRXqVVKuTe25xkFFwkPYB5hCoFgGZC4vZRuVsUp8LdKSczBqJaDESZgs4BJQAnPJzySwrSja9xevSfiqcjRxp",
//     ]);

//     run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
// }

// #[tokio::test]
// #[serial]
// #[named]
// async fn test_unverify_creator_v2() {
//     let name = trim_test_name(function_name!());
//     let setup = TestSetup::new_with_options(
//         name.clone(),
//         TestSetupOptions {
//             network: Some(Network::Devnet),
//         },
//     )
//     .await;

//     let asset_id = "AmV5tqeSncCzdMVQPFVBz9q1Jh6VgCv6WDBAMwKGgBsh";

//     let seeds: Vec<SeedEvent> = seed_txns([
//         // mint_v2
//         //"31eEawaWc6NffQr4w1fKrtjBqjjNFyFHUnnBxox4vap5M7kqC6z1QH3EMCserGPVWd5yu8nYPSy5nUbd5u4d43Pf",
//         // Two verifyCreatorV2
//         //"3yerUamgZLAsDHbp4WuPcBxpjfEbFtkUXuum1GgHdPKk2hKb7auPYb7i6gVJmHNeQ9ocMW1JFN5pY9hucssP6y4B",
//         // unverify_creator_v2
//         //"27dyWCkK9z6KfY6Z1EP2Bkw7uR1JqFyGpnAQPDvFL7SYiRDvA231idzVMHAiMGsRtcjBwJd7tJe3dbcqGTeufwYa",
//     ]);

//     run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
// }

// #[tokio::test]
// #[serial]
// #[named]
// async fn test_update_metadata_v2_new_name_new_uri() {
//     let name = trim_test_name(function_name!());
//     let setup = TestSetup::new_with_options(
//         name.clone(),
//         TestSetupOptions {
//             network: Some(Network::Devnet),
//         },
//     )
//     .await;

//     let asset_id = "4TBxZUkV9eDjFoi4HeJX924fkExCFXLSTKkdoNy7iiT8";

//     let seeds: Vec<SeedEvent> = seed_txns([
//         // mint_v2
//         //"3t9a7eknQGqqtcHvQuEhPr388RsMqqCQToYbCPm9vkyQFusaT7EqyNc4wakmF1LDYWDV46BtGSmSdYSZivKXuh2o",
//         // update_metadata_v2
//         //"5vJJgNV4tbS3ChjwGZg5SzM21K1xNA5wRH3xtbc2nifEDDv6CWP64ocE6Qgqk6f4gGEWWZEWLXz2qbUJCzQGLCie",
//     ]);

//     run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
// }

// #[tokio::test]
// #[serial]
// #[named]
// async fn test_mint_v2_to_collection_transfer_v2_mpl_account_compression() {
//     let name = trim_test_name(function_name!());
//     let setup = TestSetup::new_with_options(
//         name.clone(),
//         TestSetupOptions {
//             network: Some(Network::Devnet),
//         },
//     )
//     .await;

//     let asset_id = "JCLoUck9M22zMZVpygMkDkECk8xnHc1aRHgHigEqp2jD";

//     let seeds: Vec<SeedEvent> = vec![
//         // mpl-core collection
//         //seed_account("247DBFyKV41na1fPYNX3nSeBc4S6dPFR4s7K7wy9itw1"),
//         // mint_v2 to collection
//         //seed_txn("2WgBmzbdeLpEh1VsatQWY9XVrPGPbB9JWubKr7rHYgrDzv9rqrKNCBW6BikPWJzxM3mNZXPXvsovr6mkEPf4Nicc"),
//         // transfer_v2
//         //seed_txn("58RL8d1rU7gxkBaY11z4ktf2z6hpqRFJKTFJpRXX7Vc7o6L4GQFEk5zJxBQrQXWA4nSDyFfVPRasuZEy7zYfWsx1"),
//     ];

//     run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
// }
