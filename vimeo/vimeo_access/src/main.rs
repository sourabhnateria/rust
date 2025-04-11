use reqwest::header::{self, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Debug, Serialize, Deserialize)]
struct Video {
    uri: String,
    name: String,
    description: Option<String>,
    duration: u32,
    width: u32,
    height: u32,
    created_time: String,
    modified_time: String,
    stats: VideoStats,
    privacy: VideoPrivacy,
    files: Option<Vec<VideoFile>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct VideoStats {
    plays: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct VideoPrivacy {
    view: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct VideoFile {
    quality: String,
    type_: String,
    width: u32,
    height: u32,
    link: String,
    #[serde(rename = "created_time")]
    created_time: String,
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let access_token = "e7adc8085dd544e8819ace4b36eb57c6";

    println!("Fetching video metadata...");
    let videos = fetch_videos(access_token).await?;
    println!("Successfully fetched {} videos", videos.len());

    // Save comprehensive metadata
    let metadata = serde_json::to_string_pretty(&videos)?;
    std::fs::write("vimeo_metadata.json", metadata)?;
    println!("Metadata saved to vimeo_metadata.json");

    // Optional: Display summary
    println!("\nMetadata Summary:");
    for (i, video) in videos.iter().enumerate() {
        println!("\nVideo {}:", i + 1);
        println!("Title: {}", video.name);
        println!("Duration: {} seconds", video.duration);
        println!("Resolution: {}x{}", video.width, video.height);
        println!("Created: {}", video.created_time);
        println!("Privacy: {}", video.privacy.view);
        println!("Plays: {}", video.stats.plays.unwrap_or(0));
    }

    Ok(())
}
