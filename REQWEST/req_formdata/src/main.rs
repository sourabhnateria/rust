use reqwest;
use tokio;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let mut form_data = HashMap::new();
    form_data.insert("username", "user");
    form_data.insert("password", "pass");

    let response = client
        .post("https://example.com/login")
        .form(&form_data)
        .send()
        .await?;

    println!("Response body: {}", response.text().await?);
    Ok(())
}