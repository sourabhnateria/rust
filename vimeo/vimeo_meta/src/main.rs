use reqwest::header;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use webbrowser;

#[derive(Debug, Serialize, Deserialize)]
struct OAuthResponse {
    access_token: String,
    token_type: String,
    scope: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client_id = "f24a464ab58c20dfc71c4a6aea4eb348119ba5d8";
    let client_secret = "tob2N5u22Us3GN4rCQQSUfb7CAmZUmrX3p31S8FdNW3kd78AQmUnADzqtzX5ZldyTQOjktKZ0OiYobpekGRY8JXdk0SeHUXgDW8eGkwBoDnaIu9t3L4qyGEy4iTxlY93";
    let video_id = "1070578147";

    println!("🔑 Getting OAuth access token...");
    let access_token = get_access_token(client_id, client_secret).await?;
    println!("✅ Successfully obtained access token");

    println!("🎥 Fetching video metadata...");
    let video_data = fetch_video_metadata(&access_token, video_id).await?;

    // Save metadata to JSON file
    let metadata_filename = format!("vimeo_video_{}_metadata.json", video_id);
    save_metadata_to_file(&video_data, &metadata_filename)?;
    println!("💾 Saved metadata to: {}", metadata_filename);

    // Try to get direct stream first
    if let Some(stream_url) = extract_best_stream_url(&video_data) {
        println!("🔗 Found direct stream URL: {}", stream_url);
        if let Err(e) = play_with_local_player(&stream_url) {
            println!("⚠️ Couldn't use local player, opening in browser: {}", e);
            open_in_browser(&stream_url)?;
        }
    }
    // Fall back to embed URL
    else if let Some(embed_url) = video_data["player_embed_url"].as_str() {
        println!("⚠️ No direct stream available, using embed URL");
        open_in_browser(embed_url)?;
    } else {
        println!("❌ No playable stream found. Video may have viewing restrictions.");
        println!("ℹ️ Privacy settings: {:?}", video_data["privacy"]["view"]);
    }

    Ok(())
}

async fn get_access_token(
    client_id: &str,
    client_secret: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let token_url = "https://api.vimeo.com/oauth/authorize/client";

    let params = [
        ("grant_type", "client_credentials"),
        ("scope", "public private"),
    ];

    let client = reqwest::Client::new();
    let response = client
        .post(token_url)
        .basic_auth(client_id, Some(client_secret))
        .form(&params)
        .send()
        .await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        return Err(format!("Failed to get access token: {}", error_text).into());
    }

    let oauth_response: OAuthResponse = response.json().await?;
    Ok(oauth_response.access_token)
}

async fn fetch_video_metadata(
    access_token: &str,
    video_id: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let video_url = format!("https://api.vimeo.com/videos/{}", video_id);
    let client = reqwest::Client::new();

    let response = client
        .get(&video_url)
        .bearer_auth(access_token)
        .send()
        .await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        return Err(format!("Failed to fetch video metadata: {}", error_text).into());
    }

    let video_data: serde_json::Value = response.json().await?;
    Ok(video_data)
}

fn save_metadata_to_file(
    metadata: &serde_json::Value,
    filename: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(filename);
    let mut file = File::create(path)?;

    let pretty_json = serde_json::to_string_pretty(metadata)?;
    file.write_all(pretty_json.as_bytes())?;

    Ok(())
}

fn extract_best_stream_url(video_data: &serde_json::Value) -> Option<String> {
    if let Some(files) = video_data["files"].as_array() {
        files
            .iter()
            .find(|f| f["quality"].as_str() == Some("hd"))
            .or_else(|| files.first())
            .and_then(|f| f["link"].as_str().map(String::from))
    } else {
        None
    }
}

fn play_with_local_player(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let players = ["mpv", "vlc", "open"];

    for player in players.iter() {
        if Command::new(player).arg(url).spawn().is_ok() {
            println!("🎬 Playing with {}...", player);
            return Ok(());
        }
    }

    Err("No local player found".into())
}

fn open_in_browser(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("🌐 Opening in default browser...");
    webbrowser::open(url)?;
    Ok(())
}
