use function_name::named;

use das_api::api::{self, ApiContract};
use digital_asset_types::dao::SELLER_FEE_BASIS_POINTS_INHERIT;
use digital_asset_types::rpc::RoyaltyModel;

use itertools::Itertools;

use serial_test::serial;

use super::common::*;

/// Integration test for inherited SFBP (65535) read-side resolution.
///
/// Fixtures recorded from devnet mpl-bubblegum (see DAS-Script
/// `recordInheritedSfbpFixture.ts`). Cached bytes live under
/// `integration_tests/tests/data/`.
pub async fn run_inherited_sfbp_scenario_test(
    setup: &TestSetup,
    asset_id: &str,
    seeds: Vec<SeedEvent>,
    order: Order,
    expected_basis_points: u32,
    expected_collection_creator: &str,
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

        let royalty = response
            .royalty
            .as_ref()
            .expect("royalty should be present");
        assert_eq!(royalty.royalty_model, RoyaltyModel::Creators);
        assert_eq!(royalty.target, None);
        assert_eq!(royalty.basis_points, expected_basis_points);
        assert_eq!(
            royalty.basis_points_raw,
            Some(SELLER_FEE_BASIS_POINTS_INHERIT as u32)
        );
        assert_eq!(royalty.sfbp_inherited, Some(true));

        let creators = response
            .creators
            .as_ref()
            .expect("creators should be present");
        assert_eq!(creators.len(), 1);
        assert_eq!(creators[0].address, expected_collection_creator);
        assert_eq!(creators[0].share, 100);

        let creators_raw = response
            .creators_raw
            .as_ref()
            .expect("creators_raw should be present");
        assert!(creators_raw.is_empty());

        insta::assert_json_snapshot!(setup.name.clone(), response);
    }
}

#[tokio::test]
#[serial]
#[named]
async fn test_mint_v2_inherited_sfbp_resolves_collection_royalty_and_creators() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    // Populated by recordInheritedSfbpFixture.ts on devnet.
    let asset_id = "BzQPqccY1c88XeVfvvqrJsryHYMcPuxKiCVo4R6MDznv";
    let collection = "HgLnYtaZ9Nd1PqoGXh9rPXSMo6xUpUd6D5tsV8RUs7Qf";
    let collection_creator = "CJkzXwVwqiaSvMuRb3obrZHdrPFjCMBJBDrjspn72tDv";
    let tree_sig =
        "5aFu7ARYYaLXRLQAQXMfpxK26oeqA3LzuxjPJ7rXNdsXn9kySgLSQYxvpPHxje8n4yGrWjMw4Z4jjyr39W2NyEB7";
    let mint_sig =
        "4CS8k5FDLm8p1EYWyhF16nZXdG4dFmDjUah3SzSxU4QLpbthivNT26dwiAzPoXJPi3jEmXobciDpd1G71yTWci4F";

    let seeds: Vec<SeedEvent> = vec![
        seed_txn(tree_sig),
        seed_account(collection),
        seed_txn(mint_sig),
    ];

    run_inherited_sfbp_scenario_test(
        &setup,
        asset_id,
        seeds,
        Order::AllPermutations,
        750,
        collection_creator,
    )
    .await;
}
