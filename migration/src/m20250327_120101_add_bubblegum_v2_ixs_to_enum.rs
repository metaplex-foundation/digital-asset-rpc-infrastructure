use sea_orm_migration::{prelude::*, sea_query::extension::postgres::Type};

use crate::model::table::ClAuditsV2;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_type(
                Type::alter()
                    .name(ClAuditsV2::Instruction)
                    .add_value(Alias::new("burn_v2"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_type(
                Type::alter()
                    .name(ClAuditsV2::Instruction)
                    .add_value(Alias::new("delegate_v2"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_type(
                Type::alter()
                    .name(ClAuditsV2::Instruction)
                    .add_value(Alias::new("delegate_and_freeze_v2"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_type(
                Type::alter()
                    .name(ClAuditsV2::Instruction)
                    .add_value(Alias::new("freeze_v2"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_type(
                Type::alter()
                    .name(ClAuditsV2::Instruction)
                    .add_value(Alias::new("mint_v2"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_type(
                Type::alter()
                    .name(ClAuditsV2::Instruction)
                    .add_value(Alias::new("set_collection_v2"))
                    .to_owned(),
            )
            .await?;

        manager
            .alter_type(
                Type::alter()
                    .name(ClAuditsV2::Instruction)
                    .add_value(Alias::new("set_non_transferable_v2"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_type(
                Type::alter()
                    .name(ClAuditsV2::Instruction)
                    .add_value(Alias::new("thaw_v2"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_type(
                Type::alter()
                    .name(ClAuditsV2::Instruction)
                    .add_value(Alias::new("thaw_and_revoke_v2"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_type(
                Type::alter()
                    .name(ClAuditsV2::Instruction)
                    .add_value(Alias::new("transfer_v2"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_type(
                Type::alter()
                    .name(ClAuditsV2::Instruction)
                    .add_value(Alias::new("unverify_creator_v2"))
                    .to_owned(),
            )
            .await?;

        manager
            .alter_type(
                Type::alter()
                    .name(ClAuditsV2::Instruction)
                    .add_value(Alias::new("verify_creator_v2"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_type(
                Type::alter()
                    .name(ClAuditsV2::Instruction)
                    .add_value(Alias::new("update_metadata_v2"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_type(
                Type::alter()
                    .name(ClAuditsV2::Instruction)
                    .add_value(Alias::new("update_asset_data_v2"))
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // cannot rollback altering a type
        Ok(())
    }
}
