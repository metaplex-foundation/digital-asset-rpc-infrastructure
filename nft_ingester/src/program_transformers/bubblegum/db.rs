use crate::IngesterError;
use digital_asset_types::dao::{asset, asset_creators};
use sea_orm::{entity::*, query::*, ColumnTrait, DatabaseTransaction, DbErr, EntityTrait};

pub async fn update_asset(
    txn: &DatabaseTransaction,
    id: Vec<u8>,
    seq: Option<u64>,
    model: asset::ActiveModel,
) -> Result<(), IngesterError> {
    let update_one = if let Some(seq) = seq {
        asset::Entity::update(model)
            .filter(asset::Column::Id.eq(id))
            .filter(asset::Column::Seq.lte(seq))
    } else {
        asset::Entity::update(model).filter(asset::Column::Id.eq(id))
    };

    match update_one.exec(txn).await {
        Ok(_) => Ok(()),
        Err(err) => match err {
            DbErr::RecordNotFound(ref s) => {
                if s.find("None of the database rows are affected") != None {
                    Ok(())
                } else {
                    Err(IngesterError::from(err))
                }
            }
            _ => Err(IngesterError::from(err)),
        },
    }
}

pub async fn update_creator(
    txn: &DatabaseTransaction,
    asset_id: Vec<u8>,
    creator: Vec<u8>,
    seq: u64,
    model: asset_creators::ActiveModel,
) -> Result<(), IngesterError> {
    // Using `update_many` to avoid having to supply the primary key as well within `model`.
    // We still effectively end up updating a single row at most, which is uniquely identified
    // by the `(asset_id, creator)` pair. Is there any reason why we should not use
    // `update_many` here?
    let update = asset_creators::Entity::update_many()
        .filter(asset_creators::Column::AssetId.eq(asset_id))
        .filter(asset_creators::Column::Creator.eq(creator))
        .filter(asset_creators::Column::Seq.lte(seq))
        .set(model);

    update.exec(txn).await.map_err(IngesterError::from)?;

    Ok(())
}
