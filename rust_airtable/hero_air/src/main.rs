use anyhow::{Context, Result};
use reqwest::header;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Configuration structure
#[derive(Debug)]
struct Config {
    employment_hero: EmploymentHeroConfig,
    airtable: AirtableConfig,
}

#[derive(Debug)]
struct EmploymentHeroConfig {
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    access_token: String,
    organisation_id: String,
}

#[derive(Debug)]
struct AirtableConfig {
    api_key: String,
    base_id: String,
    table_name: String,
}

// Data structures for Employment Hero API
#[derive(Debug, Serialize, Deserialize)]
struct EmployeeFile {
    id: String,
    name: String,
    file_type: String,
    file_size: i64,
    created_at: String,
    updated_at: String,
    // Add other fields you need
}

#[derive(Debug, Serialize, Deserialize)]
struct EmployeeFileResponse {
    data: Vec<EmployeeFile>,
    // Add pagination fields if needed
}

// Data structure for Airtable API
#[derive(Debug, Serialize)]
struct AirtableRecord<'a> {
    fields: HashMap<&'a str, serde_json::Value>,
}

impl Config {
    fn from_env() -> Result<Self> {
        dotenv::dotenv().ok();

        Ok(Config {
            employment_hero: EmploymentHeroConfig {
                client_id: std::env::var("EMPLOYMENT_HERO_CLIENT_ID")
                    .context("Missing EMPLOYMENT_HERO_CLIENT_ID")?,
                client_secret: std::env::var("EMPLOYMENT_HERO_CLIENT_SECRET")
                    .context("Missing EMPLOYMENT_HERO_CLIENT_SECRET")?,
                redirect_uri: std::env::var("EMPLOYMENT_HERO_REDIRECT_URI")
                    .context("Missing EMPLOYMENT_HERO_REDIRECT_URI")?,
                access_token: std::env::var("EMPLOYMENT_HERO_ACCESS_TOKEN")
                    .context("Missing EMPLOYMENT_HERO_ACCESS_TOKEN")?,
                organisation_id: std::env::var("EMPLOYMENT_HERO_ORGANISATION_ID")
                    .context("Missing EMPLOYMENT_HERO_ORGANISATION_ID")?,
            },
            airtable: AirtableConfig {
                api_key: std::env::var("AIRTABLE_API_KEY").context("Missing AIRTABLE_API_KEY")?,
                base_id: std::env::var("AIRTABLE_BASE_ID").context("Missing AIRTABLE_BASE_ID")?,
                table_name: std::env::var("AIRTABLE_TABLE_NAME")
                    .context("Missing AIRTABLE_TABLE_NAME")?,
            },
        })
    }
}

async fn fetch_employee_files(config: &EmploymentHeroConfig) -> Result<Vec<EmployeeFile>> {
    let client = reqwest::Client::new();

    let url = format!(
        "https://api.employmenthero.com/v2/organisations/{}/employee_files",
        config.organisation_id
    );

    let response = client
        .get(&url)
        .bearer_auth(&config.access_token)
        .send()
        .await
        .context("Failed to send request to Employment Hero")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await?;
        anyhow::bail!(
            "Employment Hero API request failed with status {}: {}",
            status,
            body
        );
    }

    let file_response: EmployeeFileResponse = response
        .json()
        .await
        .context("Failed to parse Employment Hero response")?;

    Ok(file_response.data)
}

async fn upload_to_airtable(
    files: Vec<EmployeeFile>,
    config: &AirtableConfig,
) -> Result<(), anyhow::Error> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://api.airtable.com/v0/{}/{}",
        config.base_id, config.table_name
    );

    for file in files {
        let mut fields = HashMap::new();
        fields.insert("id", serde_json::Value::String(file.id));
        fields.insert("name", serde_json::Value::String(file.name));
        fields.insert("file_type", serde_json::Value::String(file.file_type));
        fields.insert("file_size", serde_json::Value::from(file.file_size));
        fields.insert("created_at", serde_json::Value::String(file.created_at));
        fields.insert("updated_at", serde_json::Value::String(file.updated_at));

        let record = AirtableRecord { fields };

        let response = client
            .post(&url)
            .json(&serde_json::json!({
                "records": [{
                    "fields": record.fields
                }]
            }))
            .header(header::AUTHORIZATION, format!("Bearer {}", config.api_key))
            .send()
            .await
            .context("Failed to send request to Airtable")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await?;
            anyhow::bail!(
                "Airtable API request failed with status {}: {}",
                status,
                body
            );
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::from_env()?;

    println!("Fetching employee files from Employment Hero...");
    let files = fetch_employee_files(&config.employment_hero).await?;
    println!("Found {} employee files", files.len());

    println!("Uploading to Airtable...");
    upload_to_airtable(files, &config.airtable).await?;
    println!("Data transfer completed successfully!");

    Ok(())
}
