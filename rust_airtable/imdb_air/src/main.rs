use reqwest::Client;
use scraper::{Html, Selector};
use serde::Serialize;
use std::error::Error;

#[derive(Serialize)]
struct AirtableRecord {
    fields: AirtableFields,
}

#[derive(Serialize)]
struct AirtableFields {
    name: String,
    content: String,
    
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Fetch HTML content from IMDb
    let url = "https://www.imdb.com/search/title/?release_date=2024-01-01,2024-12-31";
    let client = Client::new();
    let response = client.get(url).send().await?;
    let html_content = response.text().await?;

    // Parse HTML
    let document = Html::parse_document(&html_content);
    let movie_selector = Selector::parse("h3").unwrap();
    let content_selector = Selector::parse("div.ipc-html-content-inner-div[role=presentation]").unwrap();
   

    let mut movies = Vec::new();

    // Collect all elements into vectors
    let movie_elements: Vec<_> = document.select(&movie_selector).collect();
    let content_elements: Vec<_> = document.select(&content_selector).collect();
   

    // Determine the minimum length to avoid out-of-bounds access
    let min_length = movie_elements
        .len()
        .min(content_elements.len());
        

    // Iterate over the elements and extract data
    for i in 0..min_length {
        let name = movie_elements[i].text().collect::<String>().trim().to_string();
        let content = content_elements[i].text().collect::<String>().trim().to_string();

       

        movies.push(AirtableRecord {
            fields: AirtableFields { name, content },
        });
    }

    // Send data to Airtable
    let airtable_url = "https://api.airtable.com/v0/appZgahRtxMMmAFCU/Movies";
    let airtable_token = "patE8GK6DDQgY9l3g.4f92b90b3bf0bc3c531fcbc1bf3cd73e6b03fc800d5059ad0fcedc16014adb0d";

    for movie in movies {
        let response = client
            .post(airtable_url)
            .header("Authorization", format!("Bearer {}", airtable_token))
            .header("Content-Type", "application/json")
            .json(&movie)
            .send()
            .await?;

        println!("Airtable Response: {:?}", response.status());
    }

    Ok(())
}