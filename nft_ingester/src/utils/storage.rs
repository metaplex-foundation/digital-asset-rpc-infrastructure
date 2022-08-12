use {
    crate::error::IngesterError,
    std::{fs::File, io::Write},
};

#[derive(sqlx::FromRow, Clone, Debug)]
pub struct AppSpecificRev {
    pub revision: i64,
}

pub async fn write_assets_to_file(
    uri: &str,
    tree_id: &str,
    key: &str,
) -> Result<String, IngesterError> {
    println!("Requesting to see arweave link for {}", key);
    let fname = format!("{}-{}.csv", tree_id, key);
    let body = reqwest::get(uri).await?.text().await?;
    let mut file = File::create(&fname)?;
    println!("{:?}", body.len());
    file.write_all(body.as_bytes())?;
    println!("Wrote response to {}", &fname);
    Ok(fname.to_string())
}
