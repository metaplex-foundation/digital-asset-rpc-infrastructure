use {
    crate::worker::{Worker, WorkerArgs},
    clap::Parser,
    das_core::{connect_db, setup_metrics, MetricsArgs, PoolArgs},
    digital_asset_types::dao::asset_data,
    log::info,
    reqwest::ClientBuilder,
    sea_orm::{entity::*, prelude::*, query::*, EntityTrait, SqlxPostgresConnector},
    tokio::time::Duration,
};

#[derive(Parser, Clone, Debug)]
pub struct BackfillArgs {
    #[clap(flatten)]
    database: PoolArgs,

    #[command(flatten)]
    metrics: MetricsArgs,

    #[command(flatten)]
    worker: WorkerArgs,

    #[arg(long, default_value = "1000")]
    timeout: u64,

    #[arg(long, default_value = "1000")]
    batch_size: u64,
}

pub async fn run(args: BackfillArgs) -> Result<(), anyhow::Error> {
    let batch_size = args.batch_size;

    let pool = connect_db(&args.database).await?;

    setup_metrics(&args.metrics)?;

    let client = ClientBuilder::new()
        .timeout(Duration::from_millis(args.timeout))
        .build()?;

    let worker = Worker::from(args.worker);

    let (tx, handle) = worker.start(pool.clone(), client.clone());

    let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);

    let mut condition = Condition::all();
    condition = condition.add(asset_data::Column::Reindex.eq(true));
    let query = asset_data::Entity::find()
        .filter(condition)
        .order_by(asset_data::Column::Id, Order::Asc);

    let mut after = None;

    loop {
        let mut query = query.clone().cursor_by(asset_data::Column::Id);
        let mut query = query.first(batch_size);

        if let Some(after) = after {
            query = query.after(after);
        }

        let assets = query.all(&conn).await?;
        let assets_count = assets.len();

        for asset in assets.clone() {
            tx.send(asset.id).await?;
        }

        if u64::try_from(assets_count)? < batch_size {
            break;
        }

        after = assets.last().cloned().map(|asset| asset.id);
    }

    drop(tx);

    info!("Waiting for tasks to finish");
    handle.await?;

    info!("Tasks finished");
    Ok(())
}
