use reqwest::Error;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
struct DatasetItem {
    url: String,
    #[serde(rename = "pageTitle")]
    pageTitle: String,
    h1: String,
    #[serde(rename = "first_h2")]
    first_h2: String,
    #[serde(rename = "random_text_from_the_page")]
    random_text_from_the_page: String,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Replace with your Apify API token and dataset ID
    let api_token = "apify_api_mthJfXCqSVCT0BzQ2yFcuhsIaLTOnV3BB17C";
    let dataset_id = "dGe4owCxGE9HHqUEd";

    // Apify API endpoint for fetching dataset items
    let url = format!(
        "https://api.apify.com/v2/datasets/{}/items?token={}",
        dataset_id, api_token
    );

    // Make a GET request to the Apify API
    let response = reqwest::get(&url).await?;

    // Check if the request was successful
    if response.status().is_success() {
        // Parse the JSON response into a vector of DatasetItem
        let items: Vec<DatasetItem> = response.json().await?;

        // Send each item to Airtable
        for item in items {
            send_to_airtable(&item).await?;
        }
    } else {
        eprintln!("Failed to fetch data: {}", response.status());
    }

    Ok(())
}

async fn send_to_airtable(item: &DatasetItem) -> Result<(), Error> {
    // Replace with your Airtable API key, base ID, and table name
    let airtable_api_key =
        "patT5M1NvhyB5YMU3.7361c39c29738053459dabd03d3b420efa1ad8e1c29cbb10f4bae2b51c16d149";
    let airtable_base_id = "app4mEvNuTL6pIRwE";
    let airtable_table_name = "data";

    // Airtable API endpoint
    let url = format!(
        "https://api.airtable.com/v0/{}/{}",
        airtable_base_id, airtable_table_name
    );

    // Create the JSON payload for Airtable
    let payload = json!({
        "records": [
            {
                "fields": {
                    "URL": item.url,
                    "Page Title": item.pageTitle,
                    "H1": item.h1,
                    "First H2": item.first_h2,
                    "Random Text": item.random_text_from_the_page,
                }
            }
        ]
    });

    // Make a POST request to Airtable
    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", airtable_api_key))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await?;

    // Check if the request was successful
    if response.status().is_success() {
        println!("Data sent to Airtable successfully!");
    } else {
        eprintln!("Failed to send data to Airtable: {}", response.status());
    }

    Ok(())
}
