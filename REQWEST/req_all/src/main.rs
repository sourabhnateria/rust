use reqwest;
use reqwest::redirect;
// use reqwest::Certificate;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{RetryTransientMiddleware, policies::ExponentialBackoff};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::time::Duration;
use tokio;
use tokio::io::AsyncWriteExt;

#[derive(Serialize, Deserialize)]
struct Post {
    title: String,
    body: String,
    user_id: i32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // // 1. Create a client with advanced configurations
    // let cert = fs::read("path/to/cert.pem")?;
    // let cert = Certificate::from_pem(&cert)?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10)) // Timeout
        .redirect(redirect::Policy::none()) // Disable redirects
        // .add_root_certificate(cert) // Custom TLS
        // .proxy(reqwest::Proxy::https("http://user:password@proxy.example.com:8080")?) // Proxy
        .build()?;

    // 2. Middleware with retries
    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
    let client_with_middleware = ClientBuilder::new(client)
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build();

    // 3. GET Request with custom headers and query parameters
    let mut params = HashMap::new();
    params.insert("user_id", "1");

    let response = client_with_middleware
        .get("https://jsonplaceholder.typicode.com/posts/1")
        .header("Authorization", "Bearer YOUR_TOKEN")
        .header("Accept", "application/json")
        .header("User-Agent", "MyRustApp/1.0")
        .query(&params)
        .send()
        .await?;

    println!("GET Response: {}", response.text().await?);

    // 4. POST Request with JSON
    let new_post = Post {
        title: "Hello, World!".to_string(),
        body: "This is a test post.".to_string(),
        user_id: 1,
    };

    let response = client_with_middleware
        .post("https://jsonplaceholder.typicode.com/posts")
        .json(&new_post)
        .send()
        .await?;

    println!("POST Response: {}", response.text().await?);

    // 5. Form Data
    let mut form_data = HashMap::new();
    form_data.insert("username", "user");
    form_data.insert("password", "pass");

    let response = client_with_middleware
        .post("https://example.com/login")
        .form(&form_data)
        .send()
        .await?;

    println!("Form Data Response: {}", response.text().await?);



    // 7. Streaming Large Responses
    let mut response = client_with_middleware
        .get("https://example.com/large-file")
        .send()
        .await?;

    let mut file = tokio::fs::File::create("large-file.txt").await?;

    while let Some(chunk) = response.chunk().await? {
        file.write_all(&chunk).await?;
    }

    println!("File downloaded successfully.");

    // 8. Mocking Requests for Testing
    let _m = mockito::mock("GET", "/posts/1")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"id": 1, "title": "Hello, World!"}"#)
        .create();

    let response = reqwest::get(&mockito::server_url()).await?;
    println!("Mock Response: {}", response.text().await?);

    Ok(())
}