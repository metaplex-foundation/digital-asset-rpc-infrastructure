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

    let asset_id = "4ynkSgr8S9pq3a2ynNHVfJbAcBrBxJUFkRCkwmjEc3TS";

    let seeds: Vec<SeedEvent> = seed_txns([
        // create_tree_v2
        "5a2F9FDRPJQGZCKTWabBM6seHph5gkhTh3cVg5GFTnvzHz7uJEfkYK5tSb88KFpzLhxgRkiPY68Fo6qBC3xG8NjZ",
        // mint_v2
        "5hYjCyZERQ5yfNqop8TbehaGWPLGnSYFkFe4oDQEXPLEHpnjagZbzG4V88pcZdubxs9fK2Wu44VQfkbrUMoi7rE",
        // delegate_v2
        "5Hf1xFwVce9C3VqxdXsPZnUXU2EkoQ6x4LADXhPcbPR1yFkkPq6b3irvwLL5vgRbffTWaNq5SR4nuKdvsZbRDHr9",
        // freeze_v2
        "k2HvVWRq9ctFaGrJURNipaTFMXZQH9AScqKxJFHQdpdeF4umWd5y2XbE1WCMP8Pe8gpQdLTeZsurpUA9wiwt4YX",
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

    let asset_id = "9j8QfKwJvzjiHwcRyi1TCK7sZgm8iEBpGn8sRyGF5Wy8";

    let seeds: Vec<SeedEvent> = vec![
        // mpl-core collection
        seed_account("4GkzAdHxkE5txX8rLBKJebKzswcyM6LKGMi5J1joqdmg"),
        // create_tree_v2
        seed_txn("4VJEGTrFtjrs8zovYWPtm86a9fEojZryD3UR34uiHKerbkYggt3YWbXurC7z7k3pT8u9gFmS5SJMLGPRR8vUWZeF"),
        // mint_v2 to collection
        seed_txn("3Dyx2b37moZUuzUeT6ieoWKgUdZ88MeeXyB21VzijUQhaCtMz9akUxVmKQyjgsEWB3iCrvPvK9LNH6aTHsnQqqCz"),
        // freeze_v2
        seed_txn("35PfMghxXF4emD2vx8FsfHNzXLpCbcENRJoqyBNHtMBsZeges1NjRM34UuUBXHwc8XGUqfVip9TSVrxqjhkb9hPk"),
    ];

    run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
}

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

    let asset_id = "HN6V2risGpYADNKyRoceBwgrUJDnSm6h21uSTLbLVaPY";

    let seeds: Vec<SeedEvent> = vec![
        // mpl-core collection
        seed_account("3qVHqRQLt2QRRLiorv4YtJDdeJxLv8kf5ZxPWnVatUES"),
        // create_tree_v2
        seed_txn("2yd7zctRbpevqdCsKWwUX1W8CeXngQ9cphk9nMTpQK4ifLXoEjbkiD8eQWWhZn28rGHHjzZ6N2tEgEWQThodeN2Q"),
        // mint_v2 to collection
        seed_txn("5uAg2XeFEhTD5VV73VVMRX4QHbdonmH5aB1YkQ5kbHoENkmqrb2CJaRMeWb2xE76zMi95Hv9HxS9Ea2xyTanfSfE"),
        // set_collection_v2
        seed_txn("3qpp1MZNxLW8PQcfjKz3hniVc34CktHnVmRzG7jak8PayfZHJqzh5BArC8ZRN7uyXcq9BdU6Leyfzt1SsKNAqrVM"),
    ];

    run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
}

#[tokio::test]
#[serial]
#[named]
async fn add_asset_to_collection_using_set_collection_v2() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let asset_id = "5fADezYRRBaiLcRrSMC5Wsqj4mpd4BJkJvjTTQBWaB5T";

    let seeds: Vec<SeedEvent> = vec![
        // create_tree_v2
        seed_txn("2rymYRvQ9hm3q8SnnHiYfWCijkxC3GwoQNKWiny1LXtDSvpPYdPZZhi7rv1gmD3eghdp34DiodNkoFdsjaTBps7i"),
        // mint_v2
        seed_txn("kgRx5MeTguqpkQaoFmYxrV2NZeqNNcNmvs6bugwkZBV75WFoC9kvPGvZrcoAu3GDCdsRSf9a7o4pmcM7hHpK6MB"),
        // mpl-core collection
        seed_account("7un2exbR66CmLX1LTnLeDFL83PtMmB5cBaRAH9A6vyMv"),
        // set_collection_v2
        seed_txn("3rMP69ScCyFGx6DBPvHfPC6ZDxqRpMQEZ2UNbBE3wZsMLX1McK1ryNDT1XP4kBfyAi6SaV75pR8QcDUVfDWfFbME"),
    ];

    run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
}

#[tokio::test]
#[serial]
#[named]
async fn change_collection_using_set_collection_v2() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let asset_id = "7xNZ8rh4zH9EoACqEWymt5FSwgjbkDM1uFQ6MSB6kwc8";

    let seeds: Vec<SeedEvent> = vec![
        // create_tree_v2
        seed_txn("5dizJkarp8HnDVuruziQZ8DJaguFmWddvkijq6RRM8uGfVW96woGcSusMpvBFQBFDquEcD4VzrDNE7pcEmy7any2"),
        // original mpl-core collection
        seed_account("H3jL76p7G1yRkBWo41Ymp9xk6oG4xiGw5xvAPgu54ees"),
        // mint_v2
        seed_txn("4Ym9s4n7FASrVerN4iDcyKZvxBLEsemW84o9PKwMwmTQ81TRePmVn21XKfeMeUtFJ7vdW5MXVUc8MH4XmnBpPdPU"),
        // new mpl-core collection
        seed_account("4hxACbE2jHf8uCaRAhnUxAEZ6NrwS9SBNJqGm8LbBiwE"),
        // set_collection_v2
        seed_txn("ZmdGG5drfsPbvVLU1Fin2FLxD9gWhDPsqRYjGnD99pLr6Q1NVNTiKhJGWfNGbC2UTomibCp8FrMUFKBncDB3nXs"),
    ];

    run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
}

#[tokio::test]
#[serial]
#[named]
async fn test_set_non_transferable_v2() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let asset_id = "9JZWk34GEsxNWRF6XRxL6LYDhs5W7AuUVX2VHo9UV7ds";

    let seeds: Vec<SeedEvent> = vec![
        // create_tree_v2
        seed_txn("62hqBJ52oPg8WdR6jR6LNqw2mFsai9Q6K6iLoYLnr1yF4rki8xajxSbycuDCQpyXaCZjuShnhgtekn1cr134qc8G"),
        // mpl-core collection
        seed_account("CFBZCGCs61JzV2FbMucP5uGMN7tjJ3Rt3gjGAVt8J6SR"),
        // mint_v2
        seed_txn("5MbALbTYQXsfN9eQ76zBFpAPGzAod3dQPDFNiPBNYQovh8Grj9g3oANQKyC7gceRi5c8fqqYPGMLgeeq7M9N6U1e"),
        // set_non_transferable_v2
        seed_txn("5G7q1VYG1Rg7Lu2M9NjidYj4HFNHEhpqsok3ph4URuawAhtNpNUf563FXuxfyG1oGekVUfVQVcExnroKB2gHYyHL"),
    ];

    run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
}

#[tokio::test]
#[serial]
#[named]
async fn test_mint_v2_transfer_v2() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let asset_id = "H98ZTBg5XdWRxt35XDznbX2Z1m7nCYeYqoBTdi5qAQBg";

    let seeds: Vec<SeedEvent> = seed_txns([
        // create_tree_v2
        "tgmTSFiDAGWmWqDg5QkaHfJaX7KRzB6R9catUSpCFbMFSh4RvDVmfJA9idDPhfJTX5XzFEGJ8wtvT8oKktohTon",
        // mint_v2
        "2yyNYTb2ojt4CmTMWqV3szZ5muRx5MWfYTpq1ph2t7xDeAmxvnfVDgZLvcYK1oHzj2K8GWJ2eKvdT9tLxcr1yT4P",
        // transfer_v2
        "4VibmNjZzcJ3PNybVvkdRFqJCNJvJnx3Wy1suJzpS2JMCD4V18Gt4BgX532xnePp3ivZ7HBVu3PmXxT3sDFXvJ4A",
    ]);

    run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
}

#[tokio::test]
#[serial]
#[named]
async fn test_verify_creator_v2() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let asset_id = "7zKon1FrkcHr8cQnEmch9fWn6mfNgihxQz7kqqTVtDE2";

    let seeds: Vec<SeedEvent> = seed_txns([
        // create_tree_v2
        "29FVHxPbGtyY7upPGYgFqspEXFjepfz1ywubxS69QyhF5AbECGb6DBbr2C2vMa3KxzDHf6cdFkPW6L8pG32KCtwm",
        // mint_v2
        "5WJf1SRsBU8pcaCseT1MMPsJXRwrg1psfkuwWk6j99CrzmLDcoZNxR2vGXWPFRentWVNb44Ub8HN89B5MmQfidN9",
        // verify_creator_v2
        "VFhu5q3VwbRdgT2hw4xhMagxxQTveASxaUPxLDNQreKW45bNvZk5HM2izUxTmVRRtCGDx6QoDfYuxqFZhMs37dR",
    ]);

    run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
}

#[tokio::test]
#[serial]
#[named]
async fn test_unverify_creator_v2() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let asset_id = "Eoz1EtUHHpNPXU8UHM5dP93npGWzLx6uh19ExexhMwBm";

    let seeds: Vec<SeedEvent> = seed_txns([
        // create_tree_v2
        "2K29ACNcYbHqxDghb9saQrhfENZvCJ5rN4XNecw86a4fmBCBJWpadvRjJwieDE4kaE7B9noTXet3xD6oTsANDWg",
        // mint_v2
        "3fVNQkyjggPrP186PHhhLLAZ4vsVXTwaGUoiKfYwQAt2kVrgHR99Eb37vTE4eqbuL3nj62Mdn41QWtiFJWEzrqmx",
        // Two verifyCreatorV2
        "5GiuAsrrPVz9fdE6rDpgCKDQiRbDPmcBeX66zdVj2mHeYL2nfFmc3Yti7s7xvRh7MkXSAy1PbWVr6yzpMY3REjtJ",
        // unverify_creator_v2
        "5RsMkokDuu1E8PjPTjdi9MVkcr57P4rmvBd9s2DDMn4ZVczGcJdgN9Sp5wMR9r9iYtGxjya8NQ1qQFxqSZmT7HdQ",
    ]);

    run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
}

#[tokio::test]
#[serial]
#[named]
async fn test_update_metadata_v2_new_name_new_uri() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let asset_id = "38pabFkaBAVUBwPotd1Wync8ECDbVe8iZLxJyDgZG8jo";

    let seeds: Vec<SeedEvent> = seed_txns([
        // create_tree_v2
        "41AzJiVPJdwguR6PZFgKfXee55ufD7m4PNS5uXHH7AVP6BFkJzjUz2GfwGgU9xsinfBv4eSHrkD7F5KxXmSXxoGK",
        // mint_v2
        "2ZYWdwC2cHsNU9Y8UTWzfQgSneGEKB1cnhaqdk7PfToUQMnuAQE82kLCTAU42af5RFtHFKXH2MH6pWzXoTSWTyKK",
        // update_metadata_v2
        "4dmwEU9sCsSPBoSnnx6hoMdk5u4jGrMugWKoNniVqDvxbckpHuZphVztrpUNZRvJ3kRgPFtsbAj4xYj8WzZYZHEm",
    ]);

    run_get_asset_scenario_test(&setup, asset_id, seeds, Order::AllPermutations).await;
}
