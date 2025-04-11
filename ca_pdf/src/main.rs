use chromiumoxide::Browser;
use dotenv::dotenv;
use futures::StreamExt;
use reqwest::Client;
use serde_json::Value;
use std::env;
use std::path::Path;
use tokio::fs; // For file operations
use tokio::io::AsyncWriteExt;
use tokio::time::{Duration, sleep}; // For write_all

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    // URL of the page
    let desired_url =
        "https://www.nextias.com/ca/headlines-of-the-day/18-03-2025/headlines-of-the-day-18-3-2025";

    // Path to save the downloaded file
    let download_path = "./downloads";

    // Ensure the download directory exists
    if !Path::new(download_path).exists() {
        fs::create_dir_all(download_path).await?;
    }

    // Query the browser's debugging endpoint to get the list of targets
    let debug_url = "http://localhost:9222/json";
    let response: Vec<Value> = surf::get(debug_url).await?.body_json().await?;

    // Use the WebSocket URL of the first target
    let ws_url = response
        .get(0) // Get the first target
        .and_then(|target| target["webSocketDebuggerUrl"].as_str())
        .ok_or("No valid WebSocket URL found")?;

    // Connect to the browser using the WebSocket URL
    let (browser, mut handler) = Browser::connect(ws_url).await?;

    // Spawn a task to handle browser events
    let _handle = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if let Err(_e) = event {
                // Log the error but continue processing
            }
        }
    });

    // Open a new blank page
    let page = browser.new_page("about:blank").await?;

    // Navigate to the specified URL
    println!("Navigating to {}", desired_url);
    if let Err(e) = page.goto(desired_url).await {
        eprintln!("Failed to navigate to the URL: {}", e);
        return Err(e.into());
    }
    if let Err(e) = page.wait_for_navigation().await {
        eprintln!("Failed to wait for navigation: {}", e);
        return Err(e.into());
    }
    println!("Page loaded successfully.");

    // Print the page title
    let page_title = page.get_title().await?;
    if let Some(title) = page_title {
        println!("Page title: {}", title);
    } else {
        println!("Page has no title.");
    }

    // Wait for the page to load completely
    sleep(Duration::from_secs(20)).await;

    // Get the download link
    let download_button_selector = "a.wp-block-button__link.wp-element-button"; // Update this selector
    let download_button = match page.find_element(download_button_selector).await {
        Ok(button) => button,
        Err(e) => {
            eprintln!("Failed to find the download button: {}", e);
            return Err(e.into());
        }
    };

    let download_link = match download_button.attribute("href").await {
        Ok(Some(link)) => link,
        Ok(None) => {
            eprintln!("Download button has no 'href' attribute.");
            return Ok(());
        }
        Err(e) => {
            eprintln!("Failed to get 'href' attribute: {}", e);
            return Err(e.into());
        }
    };

    // Download the file using reqwest
    let client = Client::new();
    let response = match client.get(&download_link).send().await {
        Ok(response) => response,
        Err(e) => {
            eprintln!("Failed to download the file: {}", e);
            return Err(e.into());
        }
    };

    let mut file = match fs::File::create(format!("{}/file.pdf", download_path)).await {
        Ok(file) => file,
        Err(e) => {
            eprintln!("Failed to create the file: {}", e);
            return Err(e.into());
        }
    };

    let content = match response.bytes().await {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Failed to read the response content: {}", e);
            return Err(e.into());
        }
    };

    if let Err(e) = file.write_all(&content).await {
        eprintln!("Failed to write the file: {}", e);
        return Err(e.into());
    }

    println!("File downloaded successfully.");

    // Attempt to close the page and handle the error if it's already closed
    match page.close().await {
        Ok(_) => println!("\nPage closed successfully."),
        Err(e) => {
            if e.to_string().contains("Not attached to an active page") {
                // Page is already closed
            } else {
                eprintln!("Failed to close the page: {}", e);
            }
        }
    }

    Ok(())
}
