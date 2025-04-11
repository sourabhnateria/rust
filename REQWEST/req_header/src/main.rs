use reqwest;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a client
    let client = reqwest::Client::new();

    // Make a GET request with custom headers
    let response = client
        .get("https://jsonplaceholder.typicode.com/posts/1")
        .header("Authorization", "Bearer YOUR_TOKEN")
        .header("Accept", "application/json")
        .send()
        .await?;

    // Print the response body
    let body = response.text().await?;
    println!("Response body: {}", body);

    Ok(())
}