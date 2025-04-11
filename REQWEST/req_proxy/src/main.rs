use reqwest;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a client without a proxy
    let client = reqwest::Client::new();

    // Make a GET request
    let response = client
        .get("https://jsonplaceholder.typicode.com/posts/1")
        .send()
        .await;

    match response {
        Ok(res) => {
            // Print the response body
            let body = res.text().await?;
            println!("Response body: {}", body);
        }
        Err(err) => {
            eprintln!("Request failed: {}", err);
            if err.is_connect() {
                eprintln!("Failed to connect to the server");
            } else if err.is_timeout() {
                eprintln!("Request timed out");
            } else {
                eprintln!("Error: {}", err);
            }
        }
    }

    Ok(())
}