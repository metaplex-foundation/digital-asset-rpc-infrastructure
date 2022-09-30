#[cfg(test)]
mod get_candy_machine_by_id {
    use sea_orm::{entity::prelude::*, DatabaseBackend, MockDatabase};
    use solana_sdk::{signature::Keypair, signer::Signer};

    use crate::{
        dao::{
            candy_machine, candy_machine_data,
            prelude::CandyMachineData,
            sea_orm_active_enums::{EndSettingType, WhitelistMintMode},
        },
        tests::{create_candy_machine, create_candy_machine_data},
    };

    #[async_std::test]
    async fn get_candy_machine_by_id() -> Result<(), DbErr> {
        let id = Keypair::new().pubkey();
        let wallet = Keypair::new().pubkey();
        let authority = Keypair::new().pubkey();
        let creator_1 = Keypair::new().pubkey();
        let uri = Keypair::new().pubkey();

        let candy_machine = create_candy_machine(
            id.to_bytes().to_vec(),
            None,
            authority.to_bytes().to_vec(),
            None,
            Some(wallet.to_bytes().to_vec()),
            None,
            0,
            None,
            2,
        );

        let candy_machine_data = create_candy_machine_data(
            1,
            None,
            Some(1),
            String::from("TEST"),
            1000,
            399,
            true,
            None,
            None,
            399,
            id.to_bytes().to_vec(),
        );

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![vec![candy_machine_data.1.clone()]])
            .append_query_results(vec![vec![candy_machine.1.clone()]])
            .append_query_results(vec![vec![(
                candy_machine.1.clone(),
                candy_machine_data.1.clone(),
            )]])
            .into_connection();

        let insert_result = candy_machine_data::Entity::insert(candy_machine_data.0)
            .exec(&db)
            .await
            .unwrap();
        assert_eq!(insert_result.last_insert_id, 1);

        let insert_result = candy_machine::Entity::insert(candy_machine.0)
            .exec(&db)
            .await
            .unwrap();
        assert_eq!(insert_result.last_insert_id, id.to_bytes().to_vec());

        assert_eq!(
            candy_machine::Entity::find_by_id(id.to_bytes().to_vec())
                .find_also_related(CandyMachineData)
                .one(&db)
                .await?,
            Some((candy_machine.1.clone(), Some(candy_machine_data.1.clone())))
        );

        Ok(())
    }
}
