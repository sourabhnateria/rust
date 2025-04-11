use reqwest::header;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use thiserror::Error;
use base64::Engine;

#[derive(Error, Debug)]
enum TransferError {
    #[error("Environment error: {0}")]
    Env(#[from] std::env::VarError),
    
    #[error("HTTP error: {0}")]
    Reqwest(#[from] reqwest::Error),
    
    #[error("MongoDB API error: {0}")]
    MongoDB(String),
    
    #[error("Airtable API error: {0}")]
    Airtable(String),
    
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Deserialize)]
struct MongoDBResponse {
    documents: Vec<Value>,
}

#[derive(Debug, Serialize)]
struct AirtableRecord {
    fields: Value,
}

#[tokio::main]
async fn main() -> Result<(), TransferError> {
    dotenv::dotenv().ok();

    // Load configuration from environment variables
    let config = Config::from_env()?;
    
    // Create HTTP client
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    // Fetch data from MongoDB Atlas Admin API
    let mongo_data = fetch_mongodb_data(&client, &config).await?;
    
    // Transfer to Airtable in batches
    for chunk in mongo_data.chunks(10) { // Airtable rate limit: 5 requests/sec
        send_to_airtable(&client, &config, chunk).await?;
        tokio::time::sleep(std::time::Duration::from_millis(250)).await; // Rate limiting
    }

    println!("Data transfer completed successfully!");
    Ok(())
}

struct Config {
    mongodb_public_key: String,
    mongodb_private_key: String,
    mongodb_project_id: String,
    mongodb_cluster_name: String,
    mongodb_db_name: String,
    mongodb_collection: String,
    airtable_api_key: String,
    airtable_base_id: String,
    airtable_table_name: String,
}

impl Config {
    fn from_env() -> Result<Self, TransferError> {
        Ok(Self {
            mongodb_public_key: env::var("MONGODB_PUBLIC_KEY")?,
            mongodb_private_key: env::var("MONGODB_PRIVATE_KEY")?,
            mongodb_project_id: env::var("MONGODB_PROJECT_ID")?,
            mongodb_cluster_name: env::var("MONGODB_CLUSTER_NAME")?,
            mongodb_db_name: env::var("MONGODB_DB_NAME")?,
            mongodb_collection: env::var("MONGODB_COLLECTION")?,
            airtable_api_key: env::var("AIRTABLE_API_KEY")?,
            airtable_base_id: env::var("AIRTABLE_BASE_ID")?,
            airtable_table_name: env::var("AIRTABLE_TABLE_NAME")?,
        })
    }
}

async fn fetch_mongodb_data(
    client: &reqwest::Client,
    config: &Config,
) -> Result<Vec<Value>, TransferError> {
    let url = format!(
        "https://cloud.mongodb.com/api/atlas/v2/groups/{}/clusters/{}/database/find",
        config.mongodb_project_id, config.mongodb_cluster_name
    );

    let auth = base64::engine::general_purpose::STANDARD.encode(
        format!("{}:{}", config.mongodb_public_key, config.mongodb_private_key)
    );

    let response = client
        .post(&url)
        .header("Authorization", format!("Basic {}", auth))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&json!({
            "database": config.mongodb_db_name,
            "collection": config.mongodb_collection,
            "filter": {},
            "limit": 1000 // Adjust as needed
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(TransferError::MongoDB(response.text().await?));
    }

    let body: MongoDBResponse = response.json().await?;
    Ok(body.documents)
}

async fn send_to_airtable(
    client: &reqwest::Client,
    config: &Config,
    records: &[Value],
) -> Result<(), TransferError> {
    let url = format!(
        "https://api.airtable.com/v0/{}/{}",
        config.airtable_base_id, config.airtable_table_name
    );

    let airtable_records: Vec<AirtableRecord> = records
        .iter()
        .map(|doc| {
            // Transform MongoDB document to Airtable format
            AirtableRecord {
                fields: json!({
                    "Name": doc["name"].as_str().unwrap_or_default(),
                    "Description": doc["description"].as_str().unwrap_or_default(),
                    // Add other field mappings as needed
                }),
            }
        })
        .collect();

    let response = client
        .post(&url)
        .header(header::AUTHORIZATION, format!("Bearer {}", config.airtable_api_key))
        .json(&json!({ "records": airtable_records }))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(TransferError::Airtable(response.text().await?));
    }

    Ok(())
}