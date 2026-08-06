use das_api::api::{ApiContract, DasApi};

#[tokio::main]
async fn main() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://solana:solana@localhost/solana".to_string());

    let config = das_api::config::Config {
        database_url,
        ..Default::default()
    };

    let api = DasApi::from_config(config)
        .await
        .expect("failed to connect to database for schema generation");

    let schema = api.schema();
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}
