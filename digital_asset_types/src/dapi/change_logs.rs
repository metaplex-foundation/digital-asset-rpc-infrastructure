use sea_orm::sea_query::Expr;
use sea_orm::{DatabaseConnection, DbBackend};
use std::collections::HashMap;
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
    tree: Vec<u8>,
}

#[derive(FromQueryResult, Debug, Default, Clone, Eq, PartialEq)]
struct LeafInfo {
    id: Vec<u8>,
    tree_id: Vec<u8>,
    leaf_idx: i64,
    node_idx: i64,
    hash: Vec<u8>,
}

#[derive(Hash, Debug, Default, Clone, Eq, PartialEq)]
struct Leaf {
    tree_id: Vec<u8>,
    leaf_idx: i64,
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
    let leaf: Option<cl_items::Model> = sel.one(db).await?;
    if leaf.is_none() {
        return Err(DbErr::RecordNotFound("Asset Proof Not Found".to_string()));
    }
    let leaf = leaf.unwrap();
    let req_indexes = get_required_nodes_for_proof(leaf.node_idx);
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
    let required_nodes: Vec<SimpleChangeLog> = db.query_all(query).await.map(|qr| {
        qr.iter()
            .map(|q| SimpleChangeLog::from_query_result(q, "").unwrap())
            .collect()
    })?;
    let asset_proof = build_asset_proof(
        leaf.tree,
        leaf.node_idx,
        leaf.hash,
        &req_indexes,
        &required_nodes,
    );
    Ok(asset_proof)
}

pub async fn get_asset_proofs(
    db: &DatabaseConnection,
    asset_ids: Vec<Vec<u8>>,
) -> Result<HashMap<String, AssetProof>, DbErr> {
    // get the leaves (JOIN with `asset` table to get the asset ids)
    let q = asset::Entity::find()
        .join(
            JoinType::InnerJoin,
            asset::Entity::belongs_to(cl_items::Entity)
                .from(asset::Column::Nonce)
                .to(cl_items::Column::LeafIdx)
                .into(),
        )
        .select_only()
        .column(asset::Column::Id)
        .column(asset::Column::TreeId)
        .column(cl_items::Column::LeafIdx)
        .column(cl_items::Column::NodeIdx)
        .column(cl_items::Column::Hash)
        .filter(Expr::cust("asset.tree_id = cl_items.tree"))
        // filter by user provided asset ids
        .filter(asset::Column::Id.is_in(asset_ids.clone()))
        .build(DbBackend::Postgres);
    let leaves: Vec<LeafInfo> = db.query_all(q).await.map(|qr| {
        qr.iter()
            .map(|q| LeafInfo::from_query_result(q, "").unwrap())
            .collect()
    })?;

    let mut asset_map: HashMap<Leaf, LeafInfo> = HashMap::new();
    for l in &leaves {
        let key = Leaf {
            tree_id: l.tree_id.clone(),
            leaf_idx: l.leaf_idx,
        };
        asset_map.insert(key, l.clone());
    }

    // map: (tree_id, leaf_idx) -> [...req_indexes]
    let mut tree_indexes: HashMap<Leaf, Vec<i64>> = HashMap::new();
    for leaf in &leaves {
        let key = Leaf {
            tree_id: leaf.tree_id.clone(),
            leaf_idx: leaf.leaf_idx,
        };
        let req_indexes = get_required_nodes_for_proof(leaf.node_idx);
        tree_indexes.insert(key, req_indexes);
    }

    // get the required nodes for all assets
    // SELECT * FROM cl_items WHERE (tree = ? AND node_idx IN (?)) OR (tree = ? AND node_idx IN (?)) OR ...
    let mut condition = Condition::any();
    for (leaf, req_indexes) in &tree_indexes {
        let cond = Condition::all()
            .add(cl_items::Column::Tree.eq(leaf.tree_id.clone()))
            .add(cl_items::Column::NodeIdx.is_in(req_indexes.clone()));
        condition = condition.add(cond);
    }
    let query = cl_items::Entity::find()
        .select_only()
        .column(cl_items::Column::Tree)
        .column(cl_items::Column::NodeIdx)
        .column(cl_items::Column::Level)
        .column(cl_items::Column::Seq)
        .column(cl_items::Column::Hash)
        .filter(condition)
        .build(DbBackend::Postgres);
    let nodes: Vec<SimpleChangeLog> = db.query_all(query).await.map(|qr| {
        qr.iter()
            .map(|q| SimpleChangeLog::from_query_result(q, "").unwrap())
            .collect()
    })?;

    // map: (tree, node_idx) -> SimpleChangeLog
    let mut node_map: HashMap<(Vec<u8>, i64), SimpleChangeLog> = HashMap::new();
    for node in nodes {
        let key = (node.tree.clone(), node.node_idx);
        node_map.insert(key, node);
    }

    // construct the proofs
    let mut asset_proofs: HashMap<String, AssetProof> = HashMap::new();
    for (leaf, req_indexes) in &tree_indexes {
        let required_nodes: Vec<SimpleChangeLog> = req_indexes
            .iter()
            .filter_map(|n| {
                let key = (leaf.tree_id.clone(), *n);
                node_map.get(&key).cloned()
            })
            .collect();

        let leaf_info = asset_map.get(leaf).unwrap();
        let asset_proof = build_asset_proof(
            leaf_info.tree_id.clone(),
            leaf_info.node_idx,
            leaf_info.hash.clone(),
            req_indexes,
            &required_nodes,
        );

        let asset_id = bs58::encode(leaf_info.id.to_owned()).into_string();
        asset_proofs.insert(asset_id, asset_proof);
    }

    Ok(asset_proofs)
}

fn build_asset_proof(
    tree_id: Vec<u8>,
    leaf_node_idx: i64,
    leaf_hash: Vec<u8>,
    req_indexes: &[i64],
    required_nodes: &[SimpleChangeLog],
) -> AssetProof {
    let mut final_node_list = vec![SimpleChangeLog::default(); req_indexes.len()];
    for node in required_nodes.iter() {
        if node.level < final_node_list.len().try_into().unwrap() {
            node.clone_into(&mut final_node_list[node.level as usize])
        }
    }
    for (i, (n, nin)) in final_node_list.iter_mut().zip(req_indexes).enumerate() {
        if *n == SimpleChangeLog::default() {
            *n = make_empty_node(i as i64, *nin, tree_id.clone());
        }
    }
    AssetProof {
        root: bs58::encode(final_node_list.pop().unwrap().hash).into_string(),
        leaf: bs58::encode(leaf_hash).into_string(),
        proof: final_node_list
            .iter()
            .map(|model| bs58::encode(&model.hash).into_string())
            .collect(),
        node_index: leaf_node_idx,
        tree_id: bs58::encode(tree_id).into_string(),
    }
}

fn make_empty_node(lvl: i64, node_index: i64, tree: Vec<u8>) -> SimpleChangeLog {
    SimpleChangeLog {
        node_idx: node_index,
        level: lvl,
        hash: empty_node(lvl as u32).to_vec(),
        seq: 0,
        tree,
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
    indexes
}
