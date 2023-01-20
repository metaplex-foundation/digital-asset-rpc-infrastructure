use sea_orm::sea_query::Expr;
use sea_orm::{DatabaseConnection, DbBackend};
use {
    crate::dao::asset,
    crate::dao::cl_items,
    crate::rpc::AssetProof,
    sea_orm::{entity::*, query::*, DbErr, FromQueryResult},
    spl_concurrent_merkle_tree::node::empty_node,
};

#[derive(FromQueryResult, Debug, Default, Clone, Eq, PartialEq)]
struct SimpleChangeLog {
    hash: Vec<u8>,
    level: i64,
    node_idx: i64,
    seq: i64,
}

pub async fn get_proof_for_asset(
    db: &DatabaseConnection,
    asset_id: Vec<u8>,
) -> Result<AssetProof, DbErr> {
    let sel = cl_items::Entity::find()
        .join_rev(
            JoinType::InnerJoin,
            asset::Entity::belongs_to(cl_items::Entity)
                .from(asset::Column::Nonce)
                .to(cl_items::Column::LeafIdx)
                .into(),
        )
        .order_by_desc(cl_items::Column::Seq)
        .filter(Expr::cust("asset.tree_id = cl_items.tree"))
        .filter(Expr::cust_with_values(
            "asset.id = $1::bytea",
            vec![asset_id],
        ))
        .filter(cl_items::Column::Level.eq(0i64));
    let sql = sel.clone().build(DbBackend::Postgres);
    println!("find leaf sql {} ", sql.sql);
    let leaf: Option<cl_items::Model> = sel.one(db).await?;
    if leaf.is_none() {
        return Err(DbErr::RecordNotFound("Asset Proof Not Found".to_string()));
    }
    let leaf = leaf.unwrap();
    let req_indexes = get_required_nodes_for_proof(leaf.node_idx);
    let expected_proof_size = req_indexes.len();
    let mut final_node_list: Vec<SimpleChangeLog> =
        vec![SimpleChangeLog::default(); expected_proof_size];
    let mut query = cl_items::Entity::find()
        .select_only()
        .column(cl_items::Column::NodeIdx)
        .column(cl_items::Column::Hash)
        .column(cl_items::Column::Level)
        .column(cl_items::Column::Seq)
        .column(cl_items::Column::Tree)
        .filter(cl_items::Column::NodeIdx.is_in(req_indexes.clone()))
        .filter(cl_items::Column::Tree.eq(leaf.tree.clone()))
        .order_by_desc(cl_items::Column::NodeIdx)
        .order_by_desc(cl_items::Column::Id)
        .order_by_desc(cl_items::Column::Seq)
        .build(DbBackend::Postgres);
    query.sql = query
        .sql
        .replace("SELECT", "SELECT DISTINCT ON (cl_items.node_idx)");
    println!("sql {} ", query.sql);
    let nodes: Vec<SimpleChangeLog> = db.query_all(query).await.map(|qr| {
        qr.iter()
            .map(|q| SimpleChangeLog::from_query_result(q, "").unwrap())
            .collect()
    })?;
    if nodes.len() != expected_proof_size {
        for node in nodes.iter() {
            if node.level < final_node_list.len().try_into().unwrap() {
                final_node_list[node.level as usize] = node.to_owned();
            }
        }
        for (i, (n, nin)) in final_node_list.iter_mut().zip(req_indexes).enumerate() {
            if *n == SimpleChangeLog::default() {
                *n = make_empty_node(i as i64, nin);
            }
        }
    }
    for n in final_node_list.iter() {
        println!(
            "level {} index {} seq {} hash {}",
            n.level,
            n.node_idx,
            n.seq,
            bs58::encode(&n.hash).into_string()
        );
    }
    Ok(AssetProof {
        root: bs58::encode(final_node_list.pop().unwrap().hash).into_string(),
        leaf: bs58::encode(&leaf.hash).into_string(),
        proof: final_node_list
            .iter()
            .map(|model| bs58::encode(&model.hash).into_string())
            .collect(),
        node_index: leaf.node_idx,
        tree_id: bs58::encode(&leaf.tree).into_string(),
    })
}

fn make_empty_node(lvl: i64, node_index: i64) -> SimpleChangeLog {
    SimpleChangeLog {
        node_idx: node_index,
        level: lvl,
        hash: empty_node(lvl as u32).to_vec(),
        seq: 0,
    }
}

pub fn get_required_nodes_for_proof(index: i64) -> Vec<i64> {
    let mut indexes = vec![];
    let mut idx = index;
    while idx > 1 {
        if idx % 2 == 0 {
            indexes.push(idx + 1)
        } else {
            indexes.push(idx - 1)
        }
        idx >>= 1
    }
    indexes.push(1);
    return indexes;
}
