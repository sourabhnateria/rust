use anyhow::{Context, Result};
use reqwest::header;
use serde::{Deserialize, Serialize};
use std::env;
use log::{info, error};
use thiserror::Error;

#[derive(Debug, Error)]
enum AppError {
    #[error("Missing environment variable: {0}")]
    EnvVarError(String),
    #[error("API request failed: {0}")]
    ApiError(String),
    #[error("JSON parsing error: {0}")]
    JsonError(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct ClickUpTaskResponse {
    tasks: Vec<ClickUpTask>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ClickUpTask {
    id: String,
    name: String,
    description: Option<String>,
    status: ClickUpStatus,
    date_created: Option<String>,
    date_updated: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ClickUpStatus {
    status: String,
    color: String,
}

#[derive(Debug, Serialize)]
struct AirtablePayload<'a> {
    records: Vec<AirtableRecord<'a>>,
    typecast: bool,
}

#[derive(Debug, Serialize)]
struct AirtableRecord<'a> {
    fields: AirtableFields<'a>,
}

#[derive(Debug, Serialize)]
struct AirtableFields<'a> {
    task_id: &'a str,
    name: &'a str,
    description: Option<&'a String>,
    status: &'a str,
    status_color: &'a str,
    created_at: Option<&'a String>,
    updated_at: Option<&'a String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    info!("Starting ClickUp to Airtable sync");

    dotenv::dotenv().ok();

    let clickup_tasks = fetch_clickup_tasks().await?;
    info!("Fetched {} tasks from ClickUp", clickup_tasks.len());

    send_to_airtable(&clickup_tasks).await?;
    info!("Successfully sent data to Airtable");

    Ok(())
}

async fn fetch_clickup_tasks() -> Result<Vec<ClickUpTask>> {
    let access_token = env::var("CLICKUP_ACCESS_TOKEN")
        .map_err(|_| AppError::EnvVarError("CLICKUP_ACCESS_TOKEN".to_string()))?;
    let list_id = env::var("CLICKUP_LIST_ID")
        .map_err(|_| AppError::EnvVarError("CLICKUP_LIST_ID".to_string()))?;

    let client = reqwest::Client::new();
    let url = format!(
        "https://api.clickup.com/api/v2/list/{}/task?subtasks=true&include_closed=true",
        list_id
    );

    info!("Fetching tasks from ClickUp list: {}", list_id);

    let response = client
        .get(&url)
        .header(header::AUTHORIZATION, access_token)
        .send()
        .await
        .map_err(|e| AppError::ApiError(format!("ClickUp request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::ApiError(format!(
            "ClickUp API error: {} - {}",
            status, body
        )).into());
    }

    let task_response: ClickUpTaskResponse = response
        .json()
        .await
        .map_err(|e| AppError::JsonError(format!("Failed to parse ClickUp response: {}", e)))?;

    Ok(task_response.tasks)
}

async fn send_to_airtable(tasks: &[ClickUpTask]) -> Result<()> {
    let airtable_api_key = env::var("AIRTABLE_API_KEY")
        .map_err(|_| AppError::EnvVarError("AIRTABLE_API_KEY".to_string()))?;
    let airtable_base_id = env::var("AIRTABLE_BASE_ID")
        .map_err(|_| AppError::EnvVarError("AIRTABLE_BASE_ID".to_string()))?;
    let airtable_table_name = env::var("AIRTABLE_TABLE_NAME")
        .map_err(|_| AppError::EnvVarError("AIRTABLE_TABLE_NAME".to_string()))?;

    let client = reqwest::Client::new();
    let url = format!(
        "https://api.airtable.com/v0/{}/{}",
        airtable_base_id, airtable_table_name
    );

    info!("Preparing to send {} tasks to Airtable", tasks.len());

    let records: Vec<AirtableRecord> = tasks
        .iter()
        .map(|task| AirtableRecord {
            fields: AirtableFields {
                task_id: &task.id,
                name: &task.name,
                description: task.description.as_ref(),
                status: &task.status.status,
                status_color: &task.status.color,
                created_at: task.date_created.as_ref(),
                updated_at: task.date_updated.as_ref(),
            },
        })
        .collect();

    let payload = AirtablePayload {
        records,
        typecast: true,
    };

    let response = client
        .post(&url)
        .header(header::AUTHORIZATION, format!("Bearer {}", airtable_api_key))
        .header(header::CONTENT_TYPE, "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| AppError::ApiError(format!("Airtable request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::ApiError(format!(
            "Airtable API error: {} - {}",
            status, body
        )).into());
    }

    Ok(())
}