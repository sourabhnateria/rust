// use chromiumoxide::page::ScreenshotParams;
use chromiumoxide::Browser;
use dotenv::dotenv;
use futures::StreamExt;
use serde_json::Value;
use std::env;
use async_std::fs; // For file operations
use async_std::task; // For spawning tasks
use async_std::task::sleep;
use std::time::Duration;

#[async_std::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    // URL of the Wikipedia page for Information Technology
    let desired_url = "https://en.wikipedia.org/wiki/Information_technology";

    // Query the browser's debugging endpoint to get the list of targets
    let debug_url = "http://localhost:9222/json";
    let response: Vec<Value> = surf::get(debug_url)
        .await?
        .body_json()
        .await?;

    // Use the WebSocket URL of the first target
    let ws_url = response
        .get(0) // Get the first target
        .and_then(|target| target["webSocketDebuggerUrl"].as_str())
        .ok_or("No valid WebSocket URL found")?;

    // Connect to the browser using the WebSocket URL
    let (browser, mut handler) = Browser::connect(ws_url).await?;

    // Spawn a task to handle browser events
    let _handle = task::spawn(async move {
        while let Some(event) = handler.next().await {
            if let Err(_e) = event {
                // Log the error but continue processing
            }
        }
    });

    // Open a new blank page
    let page = browser.new_page("about:blank").await?;

    // Fetch the WebSocket URL of the newly opened page
    let debug_url = "http://localhost:9222/json";
    let response: Vec<Value> = surf::get(debug_url)
        .await?
        .body_json()
        .await?;

    // Find the WebSocket URL of the newly opened page
    let new_ws_url = response
        .iter()
        .find(|target| target["url"].as_str() == Some("about:blank"))
        .and_then(|target| target["webSocketDebuggerUrl"].as_str())
        .ok_or("No valid WebSocket URL found for the new page")?;

    // Connect to the new WebSocket URL
    let (_new_browser, mut new_handler) = Browser::connect(new_ws_url).await?;

    // Spawn a task to handle browser events for the new connection
    let _new_handle = task::spawn(async move {
        while let Some(event) = new_handler.next().await {
            if let Err(_e) = event {
                // Log the error but continue processing
            }
        }
    });

    // Navigate to the Wikipedia page for Information Technology
    // println!("Navigating to {}", desired_url);
    page.goto(desired_url).await?;
    page.wait_for_navigation().await?;
    println!("Page loaded successfully.");

    // Wait for the page to load completely
    sleep(Duration::from_secs(5)).await;

    // Extract the first paragraph of the article using a more general selector
    let paragraph = page
        .find_element("th.sidebar-heading")
        .await
        .map_err(|e| format!("Failed to find the paragraph element: {}", e))?;

    let paragraph_text = paragraph.inner_text().await?;

    // Handle the Option<String> returned by inner_text
    if let Some(text) = paragraph_text {
        let trimmed_text = text.trim(); // Trim leading/trailing whitespace
        println!("First paragraph text: {}", trimmed_text);

        // Compare the paragraph text with a fixed string from .env
        let fixed_text = env::var("FIXED_TEXT").expect("FIXED_TEXT not found in .env file");
        if trimmed_text == fixed_text.trim() {
            println!("The paragraph text matches the fixed text.");
        } else {
            println!("The paragraph text does not match the fixed text.");
        }
    } else {
        println!("The paragraph element has no text content.");
    }

    // Take a screenshot of the page
    // let screenshot_params = ScreenshotParams::builder()
    //     .format(chromiumoxide::page::ScreenshotFormat::Png) // Use ScreenshotFormat::Png
    //     .build();

    // let screenshot_data = page.screenshot(screenshot_params).await?;
    // fs::write("wikipedia_screenshot.png", screenshot_data).await?;
    // println!("Screenshot saved as wikipedia_screenshot.png");

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