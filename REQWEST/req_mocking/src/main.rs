use mockito::mock;
use reqwest;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _m = mock("GET", "/posts/1")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"id": 1, "title": "Hello, World!"}"#)
        .create();

    let response = reqwest::get(&mockito::server_url()).await?;
    println!("Response body: {}", response.text().await?);

    Ok(())
}