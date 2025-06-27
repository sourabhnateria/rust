use reqwest;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Make a GET request
    let response = reqwest::get("https://jsonplaceholder.typicode.com/invalid-url")
        .await;

    match response {
        Ok(res) => {
            println!("Response status: {}", res.status());
            let body = res.text().await?;
            println!("Response body: {}", body);
        }
        Err(err) => {
            eprintln!("Request failed: {}", err);
        }
    }

    Ok(())
}