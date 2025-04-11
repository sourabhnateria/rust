use dotenv::dotenv;
use reqwest;
use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();

    // Get credentials from environment
    let client_id = std::env::var("CLICKUP_CLIENT_ID")?;
    let client_secret = std::env::var("CLICKUP_CLIENT_SECRET")?;
    let redirect_uri = std::env::var("CLICKUP_REDIRECT_URI")?;

    // Step 1: Generate authorization URL
    let auth_url = format!(
        "https://app.clickup.com/api?client_id={}&redirect_uri={}&response_type=code",
        client_id,
        urlencoding::encode(&redirect_uri)
    );

    println!("Visit this URL to authorize your application:");
    println!("{}", auth_url);

    // Step 2: Get the authorization code from user
    println!("Enter the authorization code from the redirect URL:");
    let mut code = String::new();
    std::io::stdin().read_line(&mut code)?;
    let code = code.trim();

    // Step 3: Exchange code for token
    let token = exchange_code_for_token(&client_id, &client_secret, &redirect_uri, code).await?;

    println!("\nAuthentication successful!");
    println!("Access Token: {}", token.access_token);
    if let Some(refresh_token) = token.refresh_token {
        println!("Refresh Token: {}", refresh_token);
    }
    if let Some(expires_in) = token.expires_in {
        println!("Expires in: {} seconds", expires_in);
    }

    Ok(())
}

async fn exchange_code_for_token(
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
    code: &str,
) -> Result<TokenResponse, Box<dyn Error>> {
    let client = reqwest::Client::new();
    let token_url = "https://api.clickup.com/api/v2/oauth/token";

    // Create form parameters manually since we can't use the 'form' feature
    let params = [
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
    ];

    // Build URL with query parameters
    let mut url = reqwest::Url::parse(token_url)?;
    url.query_pairs_mut().extend_pairs(params);

    let response = client
        .post(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let error_body = response.text().await?;
        return Err(format!(
            "Token exchange failed with status: {}\nError details: {}",
            status, error_body
        )
        .into());
    }

    let token_response = response.json().await?;
    Ok(token_response)
}
