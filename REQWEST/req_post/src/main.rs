use reqwest;
use tokio;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Post {
    title: String,
    body: String,
    userId: i32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a client
    let client = reqwest::Client::new();

    // Define the request body
    let new_post = Post {
        title: "Hello, World! this is someone".to_string(),
        body: "This is a test post.".to_string(),
        userId: 1,
    };

    // Make a POST request with a JSON body
    let response = client
        .post("https://jsonplaceholder.typicode.com/posts")
        .json(&new_post)
        .send()
        .await?;

    // Print the response body
    let body = response.text().await?;
    println!("Response body: {}", body);

    Ok(())
}