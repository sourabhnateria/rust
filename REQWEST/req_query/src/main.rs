use reqwest;
use tokio;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let mut params = HashMap::new();
    params.insert("userId", "1");

    let response = client
        .get("https://jsonplaceholder.typicode.com/posts")
        .query(&params)
        .send()
        .await?;

    println!("Response body: {}", response.text().await?);
    Ok(())
}