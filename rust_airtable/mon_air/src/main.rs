use reqwest::header;
use serde::{Deserialize, Serialize};
use std::{env, time::Duration};
use thiserror::Error;

#[derive(Error, Debug)]
enum TransferError {
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("HTTP request error: {0}")]
    RequestError(#[from] reqwest::Error),
    
    #[error("API error: {0}")]
    ApiError(String),
    
    #[error("JSON parsing error: {0}")]
    JsonError(#[from] serde_json::Error),
}

#[derive(Debug, Deserialize, Clone)]
struct MondayItem {
    id: String,
    name: String,
    column_values: Vec<ColumnValue>,
    
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Deserialize, Clone)]
struct ColumnValue {
    id: String,
    text: String,
    
}

#[derive(Debug, Serialize, Clone)]
struct AirtableRecord {
    fields: AirtableFields,
}

#[derive(Debug, Serialize, Clone)]
struct AirtableFields {
    id: String,
    name: String,
    description: Option<String>,
    created_at: String,
    updated_at: String,
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MondayResponse {
    data: MondayData,
}

#[derive(Debug, Deserialize)]
struct MondayData {
    boards: Vec<MondayBoard>,
}

#[derive(Debug, Deserialize)]
struct MondayBoard {
    items_page: MondayItemsPage,
}

#[derive(Debug, Deserialize)]
struct MondayItemsPage {
    items: Vec<MondayItem>,
    cursor: Option<String>,
}

#[derive(Debug)]
struct Config {
    monday_api_key: String,
    monday_board_id: String,
    airtable_api_key: String,
    airtable_base_id: String,
    airtable_table_name: String,
    batch_size: usize,
    request_timeout: u64,
    items_limit: i32,
}

impl Config {
    fn from_env() -> Result<Self, TransferError> {
        Ok(Self {
            monday_api_key: env::var("MONDAY_API_KEY")
                .map_err(|_| TransferError::ConfigError("MONDAY_API_KEY must be set".into()))?,
            monday_board_id: env::var("MONDAY_BOARD_ID")
                .map_err(|_| TransferError::ConfigError("MONDAY_BOARD_ID must be set".into()))?,
            airtable_api_key: env::var("AIRTABLE_API_KEY")
                .map_err(|_| TransferError::ConfigError("AIRTABLE_API_KEY must be set".into()))?,
            airtable_base_id: env::var("AIRTABLE_BASE_ID")
                .map_err(|_| TransferError::ConfigError("AIRTABLE_BASE_ID must be set".into()))?,
            airtable_table_name: env::var("AIRTABLE_TABLE_NAME")
                .unwrap_or_else(|_| "MondayItems".into()),
            batch_size: env::var("BATCH_SIZE")
                .map(|v| v.parse().unwrap_or(10))
                .unwrap_or(10),
            request_timeout: env::var("REQUEST_TIMEOUT")
                .map(|v| v.parse().unwrap_or(30))
                .unwrap_or(30),
            items_limit: env::var("ITEMS_LIMIT")
                .map(|v| v.parse().unwrap_or(100))
                .unwrap_or(100),
        })
    }
}

async fn fetch_monday_data(config: &Config) -> Result<Vec<MondayItem>, TransferError> {
    log::info!("Fetching data from Monday.com API...");
    
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.request_timeout))
        .build()?;
    
    let query = format!(
        r#"
        query {{
          boards(ids: [{}]) {{
            name
            items_page(limit: {}) {{
              items {{
                id
                name
                column_values {{
                  id
                  text
                  
                }}
                
                created_at
                updated_at
              }}
            }}
          }}
        }}
        "#,
        config.monday_board_id, config.items_limit
    );
    
    let response = client
        .post("https://api.monday.com/v2")
        .header("Authorization", &config.monday_api_key)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "query": query,
            "variables": {}
        }))
        .send()
        .await?;
    
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await?;
        return Err(TransferError::ApiError(format!(
            "Monday.com API request failed with status {}: {}",
            status, body
        )));
    }
    
    let response_data: MondayResponse = response.json().await?;
    
    if response_data.data.boards.is_empty() {
        return Err(TransferError::ApiError("No boards found".into()));
    }
    
    // Extract items by taking ownership from the response
    let items = response_data.data.boards.into_iter()
        .next()
        .map(|board| board.items_page.items)
        .unwrap_or_default();
    
    log::info!("Successfully fetched {} items from Monday.com", items.len());
    Ok(items)
}

async fn send_to_airtable(
    config: &Config,
    items: Vec<MondayItem>,
) -> Result<(), TransferError> {
    log::info!("Starting transfer to Airtable...");
    
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.request_timeout))
        .build()?;
    
    let url = format!(
        "https://api.airtable.com/v0/{}/{}",
        config.airtable_base_id, config.airtable_table_name
    );
    
    for chunk in items.chunks(config.batch_size) {
        let records: Vec<AirtableRecord> = chunk
            .iter()
            .map(|item| {
                let description = item.column_values.iter()
                    .find(|cv| cv.id == "description" || cv.id == "long_text")
                    .map(|cv| cv.text.clone());
                
                let status = item.column_values.iter()
                    .find(|cv| cv.id == "status")
                    .map(|cv| cv.text.clone());
                
                AirtableRecord {
                    fields: AirtableFields {
                        id: item.id.clone(),
                        name: item.name.clone(),
                        description,
                        created_at: item.created_at.clone(),
                        updated_at: item.updated_at.clone(),
                        status,
                    },
                }
            })
            .collect();
        
        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", config.airtable_api_key))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "records": records,
            }))
            .send()
            .await?;
        
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await?;
            return Err(TransferError::ApiError(format!(
                "Airtable API request failed with status {}: {}",
                status, body
            )));
        }
        
        log::debug!("Successfully transferred batch of {} items", chunk.len());
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    
    log::info!("Successfully transferred {} items to Airtable", items.len());
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), TransferError> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    dotenv::dotenv().ok();
    
    let config = Config::from_env()?;
    log::info!("Starting data transfer from Monday.com to Airtable");
    
    let monday_items = fetch_monday_data(&config).await?;
    
    if monday_items.is_empty() {
        log::warn!("No items found in Monday.com board");
        return Ok(());
    }
    
    send_to_airtable(&config, monday_items).await?;
    
    log::info!("Data transfer completed successfully!");
    Ok(())
}