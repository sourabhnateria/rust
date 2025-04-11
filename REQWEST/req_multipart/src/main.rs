use reqwest;
use tokio;
use reqwest::multipart;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let form = multipart::Form::new()
        .text("username", "user")
        .file("file", "path/to/file.txt")?;

    let response = client
        .post("https://example.com/upload")
        .multipart(form)
        .send()
        .await?;

    println!("Response body: {}", response.text().await?);
    Ok(())
}