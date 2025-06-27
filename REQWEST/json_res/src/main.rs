use reqwest;
use tokio;
use serde::Deserialize;

#[derive(Deserialize)]
struct Post {
    id: i32,
    title: String,
    body: String,
    userId: i32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Make a GET request
    let response = reqwest::get("https://jsonplaceholder.typicode.com/posts/1")
        .await?;

    // Check the response status
    println!("Response status: {}", response.status());

    // Print the raw response body for debugging
    let body = response.text().await?;
    println!("Raw response: {}", body);

    // Deserialize the JSON response into a Rust struct
    match serde_json::from_str::<Post>(&body) {
        Ok(post) => {
            println!("Post ID: {}", post.id);
            println!("Title: {}", post.title);
            println!("Body: {}", post.body);
        }
        Err(e) => {
            eprintln!("Failed to parse JSON: {}", e);
        }
    }

    Ok(())
}