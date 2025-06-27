use reqwest::header::{self, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use std::error::Error;
use webbrowser;

#[derive(Debug, Serialize, Deserialize)]
struct Video {
    uri: String,
    name: String,
    link: String,
    created_time: String,
    duration: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct Pagination {
    next: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct VimeoResponse {
    data: Vec<Video>,
    paging: Pagination,
}

async fn fetch_videos(access_token: &str) -> Result<Vec<Video>, Box<dyn Error>> {
    let client = reqwest::Client::new();
    let mut all_videos = Vec::new();
    let mut url = "https://api.vimeo.com/me/videos?per_page=100".to_string();

    // Configure headers with API version
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", access_token))?,
    );
    headers.insert(
        header::ACCEPT,
        HeaderValue::from_static("application/vnd.vimeo.*+json;version=3.4"),
    );

    loop {
        let response = client.get(&url).headers(headers.clone()).send().await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await?;
            return Err(format!("API Error {}: {}", status, body).into());
        }

        let vimeo_response: VimeoResponse = response.json().await?;
        all_videos.extend(vimeo_response.data);

        url = match vimeo_response.paging.next {
            Some(next) => next,
            None => break,
        };
    }

    Ok(all_videos)
}

fn open_video(video_uri: &str) {
    let video_id = video_uri.split('/').last().unwrap_or_default();
    let url = format!("https://vimeo.com/{}", video_id);

    if webbrowser::open(&url).is_ok() {
        println!("Opened video: {}", url);
    } else {
        eprintln!("Failed to open browser for: {}", url);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Direct access token input (replace with your token)
    let access_token = "e7adc8085dd544e8819ace4b36eb57c6";

    // For production use, get from environment:
    // let access_token = std::env::var("VIMEO_ACCESS_TOKEN")
    //     .expect("VIMEO_ACCESS_TOKEN must be set");

    println!("Fetching videos...");
    let videos = fetch_videos(access_token).await?;
    println!("Successfully fetched {} videos", videos.len());

    // Save metadata
    let metadata = serde_json::to_string_pretty(&videos)?;
    std::fs::write("videos.json", metadata)?;
    println!("Metadata saved to videos.json");

    // Open all videos in browser
    println!("Opening videos...");
    for video in &videos {
        open_video(&video.uri);
        tokio::time::sleep(std::time::Duration::from_secs(1)).await; // Rate limit
    }

    Ok(())
}
