use crate::IngesterError;
use sea_orm::{entity::*, query::*, DbErr, DatabaseTransaction};
use digital_asset_types::dao::asset;

pub(crate) async fn update_asset(
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
