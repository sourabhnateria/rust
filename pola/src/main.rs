use polars::prelude::*;
use reqwest::header;
use scraper::{Html, Selector};
use std::{error::Error, fmt, time::Duration};

#[derive(Debug)]
struct Product {
    rank: u32,
    name: String,
    price: String,
    rating: String,
    reviews: String,
    url: String,
}

#[derive(Debug)]
enum ScraperError {
    Reqwest(reqwest::Error),
    SelectorParse(String),
    Polars(PolarsError),
    Io(std::io::Error),
    NoProducts,
}

impl fmt::Display for ScraperError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ScraperError::Reqwest(e) => write!(f, "Request failed: {}", e),
            ScraperError::SelectorParse(s) => write!(f, "Selector parse error: {}", s),
            ScraperError::Polars(e) => write!(f, "Polars error: {}", e),
            ScraperError::Io(e) => write!(f, "IO error: {}", e),
            ScraperError::NoProducts => write!(f, "No products found"),
        }
    }
}

impl Error for ScraperError {}

impl From<reqwest::Error> for ScraperError {
    fn from(err: reqwest::Error) -> Self {
        ScraperError::Reqwest(err)
    }
}

impl From<PolarsError> for ScraperError {
    fn from(err: PolarsError) -> Self {
        ScraperError::Polars(err)
    }
}

impl From<std::io::Error> for ScraperError {
    fn from(err: std::io::Error) -> Self {
        ScraperError::Io(err)
    }
}

fn parse_selector(selector: &str) -> Result<Selector, ScraperError> {
    Selector::parse(selector).map_err(|_| ScraperError::SelectorParse(selector.to_string()))
}

fn main() -> Result<(), ScraperError> {
    println!("🛍️  Fetching Amazon's Top 10 Bestsellers...\n");

    let products = scrape_amazon_bestsellers()?;

    // Display in terminal
    println!("{:-^60}", " AMAZON TOP 10 BESTSELLERS ");
    println!("{:<4} {:<40} {:<12}", "Rank", "Product Name", "Price");
    println!("{:-<4} {:-<40} {:-<12}", "", "", "");

    for product in &products {
        println!(
            "{:<4} {:<40.37} {:<12}",
            product.rank,
            product.name,
            if product.price.is_empty() {
                "N/A"
            } else {
                &product.price
            }
        );
    }
    println!("{:-^60}\n", "");

    // Create and save DataFrame
    let df = create_dataframe(products)?;
    save_to_csv(&df, "amazon_bestsellers.csv")?;

    println!("✅ Data saved to amazon_bestsellers.csv");
    Ok(())
}

fn scrape_amazon_bestsellers() -> Result<Vec<Product>, ScraperError> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
        .timeout(Duration::from_secs(10))
        .build()?;

    let response = client
        .get("https://www.amazon.com/Best-Sellers/zgbs")
        .header(header::ACCEPT_LANGUAGE, "en-US,en;q=0.9")
        .send()?
        .text()?;

    let document = Html::parse_document(&response);

    let selectors = (
        parse_selector(".a-carousel-card")?, // Product card
        parse_selector(".zg-bdg-text")?,     // Rank badge
        parse_selector("._cDEzb_p13n-sc-css-line-clamp-3_g3dy1")?, // Name
        parse_selector(".a-price .a-offscreen")?, // Price
        parse_selector(".a-icon-alt")?,      // Rating
        parse_selector(".a-size-small")?,    // Reviews
        parse_selector("a.a-link-normal")?,  // Link
    );

    let mut products = Vec::new();
    for (i, product) in document.select(&selectors.0).take(10).enumerate() {
        products.push(Product {
            rank: product
                .select(&selectors.1)
                .next()
                .and_then(|e| e.text().next())
                .and_then(|t| t.trim().parse().ok())
                .unwrap_or((i + 1) as u32),

            name: product
                .select(&selectors.2)
                .next()
                .map(|e| e.text().collect::<Vec<_>>())
                .unwrap_or_default()
                .trim()
                .to_string(),

            price: product
                .select(&selectors.3)
                .next()
                .map(|e| e.text().collect())
                .unwrap_or_default()
                .trim()
                .to_string(),

            rating: product
                .select(&selectors.4)
                .next()
                .map(|e| e.text().collect())
                .unwrap_or_default()
                .trim()
                .to_string(),

            reviews: product
                .select(&selectors.5)
                .next()
                .map(|e| e.text().collect())
                .unwrap_or_default()
                .trim()
                .to_string(),

            url: product
                .select(&selectors.6)
                .next()
                .and_then(|e| e.value().attr("href"))
                .map(|s| format!("https://www.amazon.com{}", s.replace(" ", "%20")))
                .unwrap_or_default(),
        });
    }

    if products.is_empty() {
        return Err(ScraperError::NoProducts);
    }
    Ok(products)
}

fn create_dataframe(products: Vec<Product>) -> Result<DataFrame, PolarsError> {
    DataFrame::new(vec![
        Series::new("Rank", products.iter().map(|p| p.rank).collect::<Vec<_>>()),
        Series::new(
            "Product",
            products.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        ),
        Series::new(
            "Price",
            products
                .iter()
                .map(|p| p.price.as_str())
                .collect::<Vec<_>>(),
        ),
        Series::new(
            "Rating",
            products
                .iter()
                .map(|p| p.rating.as_str())
                .collect::<Vec<_>>(),
        ),
        Series::new(
            "Reviews",
            products
                .iter()
                .map(|p| p.reviews.as_str())
                .collect::<Vec<_>>(),
        ),
        Series::new(
            "URL",
            products.iter().map(|p| p.url.as_str()).collect::<Vec<_>>(),
        ),
    ])
}

fn save_to_csv(df: &DataFrame, path: &str) -> Result<(), ScraperError> {
    let mut file = std::fs::File::create(path)?;
    CsvWriter::new(&mut file)
        .include_header(true)
        .finish(&mut df.clone())?;
    Ok(())
}
