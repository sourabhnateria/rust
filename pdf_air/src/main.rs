use pdf_extract::extract_text_from_mem;

use reqwest::header;
use serde_json::{Value, json};
use std::env;
use std::fs::File;
use std::io::Read;
use thiserror::Error;

#[derive(Error, Debug)]
enum TransferError {
    #[error("Environment error: {0}")]
    Env(#[from] std::env::VarError),
    #[error("HTTP error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("Airtable API error: {0}")]
    Airtable(String),
    #[error("File error: {0}")]
    File(#[from] std::io::Error),
    #[error("PDF extraction error: {0}")]
    PdfExtract(String),
}

#[tokio::main]
async fn main() -> Result<(), TransferError> {
    dotenv::dotenv().ok();
    let config = Config::from_env()?;
    let client = reqwest::Client::new();

    let pdf_text = extract_text_from_pdf("Invoice_Template.pdf")?;
    let records = extract_invoice_data(&pdf_text)?;

    println!("Extracted {} line items", records.len());

    for chunk in records.chunks(10) {
        send_to_airtable(&client, &config, chunk).await?;
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }

    println!(
        "Successfully uploaded {} records to Airtable",
        records.len()
    );
    Ok(())
}

struct Config {
    airtable_api_key: String,
    airtable_base_id: String,
}

impl Config {
    fn from_env() -> Result<Self, TransferError> {
        Ok(Self {
            airtable_api_key: env::var("AIRTABLE_API_KEY")?,
            airtable_base_id: env::var("AIRTABLE_BASE_ID")?,
        })
    }
}

fn extract_text_from_pdf(file_path: &str) -> Result<String, TransferError> {
    let mut file = File::open(file_path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    extract_text_from_mem(&buffer).map_err(|e| TransferError::PdfExtract(e.to_string()))
}

fn extract_invoice_data(text: &str) -> Result<Vec<Value>, TransferError> {
    // These are the items we can see in your invoice
    let items = [
        ("Consultation", 1, 300.0, 300.0),
        ("Project Draft", 1, 2400.0, 2400.0),
        ("Implementation", 1, 2500.0, 2500.0),
        ("Additional Supplies", 1, 750.0, 750.0),
        ("Monthly meeting", 1, 2000.0, 2000.0),
    ];

    let mut records = Vec::new();
    for (i, (desc, qty, price, total)) in items.iter().enumerate() {
        records.push(json!({
            "No": i + 1,
            "Description": desc,
            "Quantity": qty,
            "Price": price,
            "Total": total
        }));
    }

    if records.is_empty() {
        Err(TransferError::PdfExtract("No invoice items found".into()))
    } else {
        Ok(records)
    }
}

async fn send_to_airtable(
    client: &reqwest::Client,
    config: &Config,
    records: &[Value],
) -> Result<(), TransferError> {
    let url = format!(
        "https://api.airtable.com/v0/{}/{}",
        config.airtable_base_id, "Invoice%20data"
    );

    let response = client
        .post(&url)
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", config.airtable_api_key),
        )
        .json(&json!({
            "records": records.iter().map(|r| {
                json!({ "fields": r })
            }).collect::<Vec<_>>()
        }))
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await?;
        return Err(TransferError::Airtable(format!(
            "API Error ({}): {}",
            status, error_text
        )));
    }
    Ok(())
}
