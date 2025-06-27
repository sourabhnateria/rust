use reqwest;
use tokio;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a client
    let client = reqwest::Client::new();

    // Define query parameters
    let mut params = HashMap::new();
    params.insert("userId", "2");

    // Make a GET request with query parameters
    let response = client
        .get("https://jsonplaceholder.typicode.com/posts")
        .query(&params)
        .send()
        .await?;

    // Print the response body
    let body = response.text().await?;
    println!("Response body: {}", body);

    Ok(())
}