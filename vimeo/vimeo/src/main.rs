use reqwest::header::{self, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs::File;
use std::io::Write;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Serialize, Deserialize)]
struct Video {
    uri: String,
    name: String,
    description: Option<String>,
    duration: u32,
    width: u32,
    height: u32,
    created_time: String,
    files: Option<Vec<VideoFile>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct VideoFile {
    quality: String,
    type_: String,
    width: u32,
    height: u32,
    link: String,
    size: Option<u64>,
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

async fn download_video(
    client: &reqwest::Client,
    access_token: &str,
    video: &Video,
) -> Result<(), Box<dyn Error>> {
    let files = match &video.files {
        Some(f) => f,
        _ => {
            println!(
                "\nVideo '{}' download failed. Possible reasons:",
                video.name
            );
            println!("- Video privacy settings prevent downloads");
            println!("- Account type doesn't allow downloads (needs Vimeo Pro/Business)");
            println!("- Video is still processing");
            println!("- No download permission in access token scopes");
            return Err("Download unavailable".into());
        }
    };

    // Get the highest quality version
    let best_file = files
        .iter()
        .max_by_key(|f| f.width * f.height)
        .ok_or("No valid video files found")?;

    let video_url = &best_file.link;
    let file_name = sanitize_filename::sanitize(format!("{}.mp4", video.name));

    println!("Downloading: {} ({})", video.name, best_file.quality);

    let mut response = client
        .get(video_url)
        .header(header::AUTHORIZATION, format!("Bearer {}", access_token))
        .send()
        .await?;

    let mut file = tokio::fs::File::create(&file_name).await?;
    let mut downloaded: u64 = 0;

    while let Some(chunk) = response.chunk().await? {
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        print!("\rDownloaded: {} KB", downloaded / 1024);
    }

    println!("\nFinished downloading: {}", file_name);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let access_token = "e7adc8085dd544e8819ace4b36eb57c6";

    // Create download directory
    tokio::fs::create_dir_all("downloads").await?;

    println!("Fetching video metadata...");
    let videos = fetch_videos(access_token).await?;
    println!("Successfully fetched {} videos", videos.len());

    // Save metadata
    let metadata = serde_json::to_string_pretty(&videos)?;
    std::fs::write("downloads/metadata.json", metadata)?;
    println!("Metadata saved to downloads/metadata.json");

    // Download videos
    let client = reqwest::Client::new();
    for video in &videos {
        match download_video(&client, access_token, video).await {
            Ok(_) => (),
            Err(e) => eprintln!("Failed to download {}: {}", video.name, e),
        }
    }

    Ok(())
}
