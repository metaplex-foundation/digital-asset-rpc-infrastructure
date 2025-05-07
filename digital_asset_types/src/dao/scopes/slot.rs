use crate::dao::slot_metas;
use sea_orm::{entity::*, query::*, ConnectionTrait, DbErr};

pub async fn get_latest_slot(conn: &impl ConnectionTrait) -> Result<u64, DbErr> {
    let slot_meta = slot_metas::Entity::find()
        .order_by_desc(slot_metas::Column::Slot)
        .one(conn)
        .await?;

    let slot = slot_meta.map(|slot_meta| slot_meta.slot);

    slot.unwrap_or(0)
        .try_into()
        .map_err(|_e| DbErr::Custom("Unable serialize slot".to_string()))
}
