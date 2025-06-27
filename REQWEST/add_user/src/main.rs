use mongodb::{Client, options::ClientOptions};
use serde::{Deserialize, Serialize};
use reqwest::Client as HttpClient;
use std::error::Error;
use std::io::{self, Write};

#[derive(Serialize, Deserialize, Debug)]
struct User {
    name: String,
    email: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Connect to MongoDB
    let client_options = ClientOptions::parse("mongodb://localhost:27017").await?;
    let client = Client::with_options(client_options)?;
    let db = client.database("user_db");
    let collection = db.collection::<User>("users");

    // Connect to Chrome DevTools Protocol
    let http_client = HttpClient::new();
    let cdp_url = "http://localhost:9222/json/version"; // Use /json/version or /json/list
    let response = http_client.get(cdp_url).send().await?;

    // Check if the response is valid JSON
    if !response.status().is_success() {
        eprintln!("Failed to connect to Chrome DevTools Protocol: {}", response.status());
        return Ok(());
    }

    let cdp_response: serde_json::Value = response.json().await?;
    let ws_url = cdp_response["webSocketDebuggerUrl"]
        .as_str()
        .ok_or("Failed to parse WebSocket URL from Chrome DevTools Protocol response")?;

    println!("WebSocket URL: {}", ws_url);

    // Take user input
    let mut name = String::new();
    let mut email = String::new();

    print!("Enter user name: ");
    io::stdout().flush()?; // Ensure the prompt is displayed immediately
    io::stdin().read_line(&mut name)?;
    let name = name.trim().to_string(); // Remove trailing newline

    print!("Enter user email: ");
    io::stdout().flush()?;
    io::stdin().read_line(&mut email)?;
    let email = email.trim().to_string(); // Remove trailing newline

    // Create a User struct from the input
    let user = User { name, email };

    // Insert the user into MongoDB
    let insert_result = collection.insert_one(user, None).await?;
    println!("Inserted user with id: {:?}", insert_result.inserted_id);

    Ok(())
}