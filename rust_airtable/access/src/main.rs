use reqwest::blocking::Client;
use serde_json::Value;
use std::error::Error;
use urlencoding::encode; // Import the encode function

fn main() -> Result<(), Box<dyn Error>> {
    // Use a URL-encoded redirect URI that matches your app registration
    let redirect_uri = "https://saveefforts.com"; // Change to your registered URI

    // Step 1: Generate authorization URL
    let auth_url = format!(
        "https://oauth.employmenthero.com/oauth2/authorize?\
        client_id=mux-NE0jFxbsssctO2O3zU4fELtfit2JjEx0O99UxpM\
        redirect_uri={}&\
        response_type=code&\
        scope=api",
        encode(redirect_uri) // Properly encode the URI
    );

    println!("Visit this URL in your browser:\n{}", auth_url);

    // Step 2: Get the authorization code
    println!("Paste the full redirect URL you received after authentication:");
    let mut redirect_url = String::new();
    std::io::stdin().read_line(&mut redirect_url)?;

    // Extract code from URL
    let code = redirect_url
        .split("code=")
        .nth(1)
        .and_then(|s| s.split('&').next())
        .ok_or("No authorization code found in URL")?
        .trim();

    // Step 3: Exchange code for token
    let client = Client::new();
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", "mux-NE0jFxbsssctO2O3zU4fELtfit2JjEx0O99UxpM"),
        (
            "client_secret",
            "BG4pyDRr87O4UPw6Hm-Zwh1jXmYajI9xfR3IA02QXcA",
        ),
    ];

    println!("Requesting access token...");
    let response = client
        .post("https://oauth.employmenthero.com/oauth2/token")
        .form(&params)
        .send()?;

    let json: Value = response.json()?;

    match json.get("access_token") {
        Some(token) => {
            println!("Success! Access Token: {}", token);
            if let Some(refresh) = json.get("refresh_token") {
                println!("Refresh Token: {}", refresh);
            }
        }
        None => {
            eprintln!("Error response:\n{}", serde_json::to_string_pretty(&json)?);
            return Err("Failed to obtain access token".into());
        }
    }

    Ok(())
}
