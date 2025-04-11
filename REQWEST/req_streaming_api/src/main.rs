use reqwest;
use tokio;
use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a client
    let client = reqwest::Client::new();

    // Make a GET request
    let mut response = client
        .get("https://example.com/large-file")
        .send()
        .await?;

    // Create a file to save the downloaded content
    let mut file = tokio::fs::File::create("large-file.txt").await?;

    // Stream the response content to the file
    while let Some(chunk) = response.chunk().await? {
        file.write_all(&chunk).await?;
    }

    println!("File downloaded successfully.");

    Ok(())
}