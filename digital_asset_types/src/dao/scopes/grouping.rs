use crate::dao::{asset_grouping, GroupingSize};
use sea_orm::{
    sea_query::Condition, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, PaginatorTrait,
    QueryFilter,
};

pub async fn get_grouping(
    conn: &impl ConnectionTrait,
    group_key: String,
    group_value: String,
) -> Result<GroupingSize, DbErr> {
    let size = asset_grouping::Entity::find()
        .filter(
            Condition::all()
                .add(asset_grouping::Column::GroupKey.eq(group_key))
                .add(asset_grouping::Column::GroupValue.eq(group_value))
                .add(
                    Condition::any()
                        .add(asset_grouping::Column::Verified.eq(true))
                        .add(asset_grouping::Column::Verified.is_null()),
                ),
        )
        .count(conn)
        .await?;
    Ok(GroupingSize { size })
}
