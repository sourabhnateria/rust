use reqwest;
use tokio;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // URL of the file to download
    let url = "https://jsonplaceholder.typicode.com/posts/1";

    // Create a client
    let client = reqwest::Client::new();

    // Make a GET request to download the file
    let response = client.get(url).send().await?;

    // Check if the request was successful
    if response.status().is_success() {
        // Create a file to save the downloaded content
        let mut file = File::create("sample-file.txt").await?;

        // Write the response content to the file
        let content = response.bytes().await?;
        file.write_all(&content).await?;
        println!("File downloaded successfully.");
    } else {
        println!("Failed to download file: {}", response.status());
    }

    Ok(())
}