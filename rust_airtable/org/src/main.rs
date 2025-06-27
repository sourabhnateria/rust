use reqwest::header::{AUTHORIZATION, ACCEPT, HeaderValue};
use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Debug, Serialize, Deserialize)]
struct Organization {
    id: String,
    name: Option<String>,
    // Add other fields based on API response
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiResponse {
    data: Vec<Organization>,
    // Handle pagination if needed (next_page, total, etc.)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let access_token = "eyJraWQiOiJ0SFprNkVpcXBSdTErcXBmVDRReVp4MG5kVWsyWG5NWEJhcldTTE5LRzlrPSIsImFsZyI6IlJTMjU2In0.eyJhcHBsaWNhdGlvbl9pZCI6ImE5YzgxMmVlLTRlZjItNGNmYS04ZjExLTMyMTE5YWYzYmU1ZCIsInVzZXIiOnsiaWQiOiJiMjBhNzQzYy1mZjFmLTQ1MTEtODA1NS0zZDBjODgxYjNhNzMiLCJlbWFpbCI6IiIsInRpbWVfem9uZSI6IiIsImxvY2FsZSI6IiJ9LCJzc29fbWVtYmVyX2lkcyI6W10sImV4cGlyZWRfYXQiOjE3NDI5MTU5ODkzOTAsImlzcyI6ImF1dGgtc2VydmljZSIsImV4cCI6MTc0MjkxNTk4OX0.EQCMUX8IviLmvAjhO9MViec_Lf8kIfUHT0D2LyYpQkcmzBAx0sffCNJ_InqkySyQFWpGY0Rb7XhiU5skyuyVSYU0MMisL7608B3oQ6B2PPrzp4CyZhH8WQ4MT0uCsBTlvTfuiG9yn_RPCeZl6fIuZ74XmZo1NKkMvBE-KbbdbWQ_gK_UHOrScOBkCMmypAjYiP_aKn8WEMfpYXMpPsSljxRzJsrKHzrKZtbD_QIimE1WU6DGnDklyY49K9nxok2qMeKnb6VOu8gWpWiMiE5VwL9RrTRsml7CdYxBQ5BM4KCGyu3Sr6MdIUFN9cN8e9gMsYx-zOFcO4WqhBmBj0FH5g"; // Replace with your valid token
    let url = "https://api.employmenthero.com/api/v1/organisations";

    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", access_token.trim()))?,
        )
        .header(ACCEPT, "application/json")
        .send()
        .await?;

    match response.status() {
        reqwest::StatusCode::OK => {
            let api_response: ApiResponse = response.json().await?;
            println!("Found {} organizations:", api_response.data.len());
            for org in api_response.data {
                println!("- ID: {}, Name: {:?}", org.id, org.name);
            }
        }
        reqwest::StatusCode::UNAUTHORIZED => {
            eprintln!("Error: Invalid or expired access token");
        }
        _ => {
            let error_body = response.text().await?;
            eprintln!("API Error: {}", error_body);
        }
    }

    Ok(())
}