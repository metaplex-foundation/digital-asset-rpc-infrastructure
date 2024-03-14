use cadence_macros::statsd_gauge;
use clap::Parser;
use das_core::{connect_db, setup_metrics, MetricsArgs, PoolArgs};
use digital_asset_types::dao::asset_data::{Column, Entity};
use log::{error, info};
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, SqlxPostgresConnector};
use std::thread;
use std::time::Duration;

#[derive(Parser, Clone, Debug)]
pub struct ReportArgs {
    #[clap(flatten)]
    metrics: MetricsArgs,

    #[clap(flatten)]
    database: PoolArgs,

    /// Interval in minutes to report the status
    #[arg(long, default_value = "15")]
    interval: u64,
}

pub async fn run(args: ReportArgs) -> Result<(), anyhow::Error> {
    let pool = connect_db(args.database).await?;

    setup_metrics(args.metrics)?;

    loop {
        {
            let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool.clone());

            match Entity::find()
                .filter(Column::Reindex.eq(true))
                .count(&conn)
                .await
            {
                Ok(count) => {
                    info!("Count of asset_data with reindex=true: {}", count);
                    statsd_gauge!("report.status", 1);

                    statsd_gauge!("download.pending", count);
                }
                Err(e) => {
                    error!("Failed to count asset_data with reindex=true: {}", e);
                    statsd_gauge!("report.status", 0);
                }
            };
        }

        thread::sleep(Duration::from_secs(args.interval * 60));
    }
}
