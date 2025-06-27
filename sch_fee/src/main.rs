use dotenv::dotenv;
use lopdf::Document;
use regex::Regex;
use reqwest::blocking::Client;
use serde_json::json;
use std::error::Error;
use std::str::FromStr;
use std::thread;
use std::time::{Duration, Instant};

const AIRTABLE_TABLE_NAME: &str = "Fee Schedules";
const AIRTABLE_BATCH_SIZE: usize = 10;
const AIRTABLE_REQUEST_DELAY_MS: u64 = 250;

#[derive(Debug, Clone)]
struct FeeItem {
    name: String,
    price_str: String,
    price_num: Option<f64>,
    category: String,
}

fn extract_first_number_from_price(price_text: &str) -> Option<f64> {
    let num_regex = Regex::new(r"\$?\s*([0-9,]+(?:[.,]\d+)?)").unwrap();
    if let Some(caps) = num_regex.captures(price_text) {
        if let Some(num_match) = caps.get(1) {
            let num_str = num_match.as_str().replace(',', "");
            return f64::from_str(&num_str).ok();
        }
    }
    None
}

fn main() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    let start_time = Instant::now();
    println!("Starting PDF processing...");

    let pdf_path = "building-fee-schedule.pdf";
    if !std::path::Path::new(pdf_path).exists() {
        eprintln!("Error: PDF file '{}' not found.", pdf_path);
        return Err(format!("PDF file not found: {}", pdf_path).into());
    }
    let doc = Document::load(pdf_path)?;
    println!("PDF loaded successfully");

    let mut fee_items: Vec<FeeItem> = Vec::new();
    let mut current_category = String::new();
    let pages = doc.get_pages();
    println!(
        "Found {} pages in PDF. Will process up to the first 6 pages.",
        pages.len()
    );

    let category_regex = Regex::new(r"^[A-Z][A-Z0-9\s\-/&',\.]{3,}$")?;
    let skip_line_regex = Regex::new(
        r"(?i)^(page\s*\d+|effective\s*date|revision\s*date|document\s*control|table\s*of\s*contents)",
    )?;
    let potential_new_item_name_regex = Regex::new(r"^[A-Z][a-zA-Z\s,]{5,}[a-zA-Z]$").unwrap();

    let non_category_phrases: Vec<String> = [
        "ALL FEES ARE",
        "CITY OF",
        "BUILDING DEPARTMENT",
        "LOCAL BUSINESS TAX RECEIPT",
        "CERTIFICATE OF USE",
        "DISHONORED CHECK FEE",
        "REFUNDABLE",
        "NOTES:",
        "NOTE:",
        "SUBTOTAL",
        "TOTAL DUE",
        "SEE SECTION",
        "LICE",
        // Removed "PERMIT FEE WITH PRIVATE PROVIDER" and "LOCAL"
        // Add back specific non-category phrases if needed, e.g., "PERMIT FEE WITH PRIVATE" (the partial)
        // if the full phrase is NOT a category.
        "PROVIDER",
        "DISHON",
        "ORED CHECK FEE",
    ]
    .iter()
    .map(|s| s.to_uppercase())
    .collect();

    for (page_index, (page_num, _)) in pages.iter().enumerate().take(6) {
        // Process only first 6 pages
        match doc.extract_text(&[*page_num]) {
            Ok(text) => {
                let lines_vec: Vec<&str> = text.split('\n').collect();
                let mut lines_iter = lines_vec.iter().peekable();
                let mut potential_item_name_buffer: Vec<String> = Vec::new();

                println!(
                    "\n--- Processing Page {} (PDF Page ID: {}) ---",
                    page_index + 1,
                    page_num
                );

                while let Some(line_raw) = lines_iter.next() {
                    let line = line_raw.trim();
                    // println!("[main] Current line: '{}'", line);

                    if line.is_empty() {
                        continue;
                    }

                    if skip_line_regex.is_match(line)
                        || line.chars().all(|c| c.is_numeric() || c.is_whitespace())
                    {
                        potential_item_name_buffer.clear();
                        continue;
                    }

                    let upper_line_for_check = line.to_uppercase();
                    let is_potential_category_line = category_regex.is_match(line)
                        && !line.contains('$')
                        && line.len() > 3
                        && !line.ends_with(',')
                        && !line.ends_with(';');

                    if is_potential_category_line {
                        if !non_category_phrases.iter().any(|phrase| {
                            phrase == &upper_line_for_check || upper_line_for_check.contains(phrase)
                        }) {
                            if !potential_item_name_buffer.is_empty() {
                                // println!("[main] Category change, buffer had: {:?}.", potential_item_name_buffer);
                                potential_item_name_buffer.clear();
                            }
                            if current_category != line {
                                current_category = line.to_string();
                                println!("[main] >> New Category Set: {}", current_category);
                            }
                            continue;
                        }
                    }

                    if line.contains('$')
                        || (!potential_item_name_buffer.is_empty() && is_price_like(line))
                    {
                        let mut item_name_str = potential_item_name_buffer.join(" ");
                        item_name_str = item_name_str
                            .replace("?Identity-H Unimplemented?", "")
                            .split_whitespace()
                            .collect::<Vec<&str>>()
                            .join(" ");

                        let mut price_lines: Vec<String> = Vec::new();

                        if line.contains('$') {
                            if let Some(dollar_index) = line.rfind('$') {
                                // Use rfind to get the last '$'
                                let name_part_on_this_line = line[..dollar_index].trim();
                                if !name_part_on_this_line.is_empty() {
                                    if !item_name_str.is_empty() {
                                        item_name_str.push(' ');
                                    }
                                    item_name_str.push_str(name_part_on_this_line);
                                }
                                price_lines.push(line[dollar_index..].trim().to_string());
                            } else {
                                // Should not happen if line.contains('$')
                                price_lines.push(line.to_string());
                            }
                        } else {
                            // No '$' on current line, but buffer had name and current line is_price_like
                            price_lines.push(line.to_string());
                        }
                        potential_item_name_buffer.clear(); // Name part used or current line started price

                        // Greedily consume subsequent lines for price string
                        while let Some(next_line_raw) = lines_iter.peek() {
                            let next_line = next_line_raw.trim();
                            if next_line.is_empty()
                                || skip_line_regex.is_match(next_line)
                                || (category_regex.is_match(next_line)
                                    && !next_line.contains('$')
                                    && !non_category_phrases
                                        .iter()
                                        .any(|p| next_line.to_uppercase().contains(p)))
                                || (potential_new_item_name_regex.is_match(next_line)
                                    && !next_line.contains('$')
                                    && !is_continuation_phrase(next_line))
                            {
                                // println!("[main] Price continuation stopped. Next line: '{}'", next_line);
                                break;
                            }
                            // println!("[main] Appending to price: '{}'", next_line);
                            price_lines.push(lines_iter.next().unwrap().trim().to_string()); // Consume and add
                        }

                        let full_price_description = price_lines
                            .join(" ")
                            .split_whitespace()
                            .collect::<Vec<&str>>()
                            .join(" ");
                        let numeric_price =
                            extract_first_number_from_price(&full_price_description);

                        if !full_price_description.is_empty()
                            && (full_price_description.contains('$')
                                || is_price_like(&price_lines.first().unwrap_or(&String::new())))
                        {
                            // println!("[main] Creating Item: C:'{}', N:'{}', P_str:'{}', P_num:'{:?}'", current_category, item_name_str, full_price_description, numeric_price);
                            fee_items.push(FeeItem {
                                name: item_name_str.clone(),
                                price_str: full_price_description,
                                price_num: numeric_price,
                                category: current_category.clone(),
                            });
                        } else {
                            // println!("[main] Price string empty or invalid. Price lines: {:?}", price_lines);
                            if !item_name_str.is_empty() {
                                potential_item_name_buffer.push(item_name_str);
                            }
                        }
                    } else {
                        // Line does not contain '$' and buffer was empty or line doesn't look like price
                        // println!("[main] Buffering for name: '{}'", line);
                        potential_item_name_buffer.push(line.to_string());
                    }
                }
                if !potential_item_name_buffer.is_empty() {
                    // println!("[main] End of page, buffer has: {:?}.", potential_item_name_buffer);
                }
            }
            Err(e) => eprintln!(
                "Warning: Could not extract text from page {}: {}",
                page_num, e
            ),
        }
    }
    println!("Finished processing specified pages.");

    let valid_fee_items: Vec<_> = fee_items
        .into_iter()
        .filter(|item| {
            item.price_num.is_some()
                && (!item.name.is_empty() || !item.category.is_empty())
                && item.price_str.contains('$')
        })
        .collect();

    println!("\nPDF Processing completed in {:?}", start_time.elapsed());
    println!(
        "Found {} valid fee items (from first 6 pages, with parseable numerical price):",
        valid_fee_items.len()
    );
    for (idx, item) in valid_fee_items.iter().enumerate().take(15) {
        println!(
            "{}. C: '{}' N: '{}': P_num: {:?}, P_str: '{}'",
            idx + 1,
            item.category,
            item.name,
            item.price_num,
            item.price_str
        );
    }
    if valid_fee_items.len() > 15 {
        println!("... and {} more items.", valid_fee_items.len() - 15);
    }

    println!("\nPreparing for Airtable upload...");
    let api_key_present = std::env::var("AIRTABLE_API_KEY").is_ok();
    let base_id_present = std::env::var("AIRTABLE_BASE_ID").is_ok();

    if !api_key_present || !base_id_present {
        eprintln!(
            "Error: Airtable credentials not fully set in environment variables (or .env file)."
        );
        if !api_key_present {
            eprintln!("- AIRTABLE_API_KEY is missing.");
        }
        if !base_id_present {
            eprintln!("- AIRTABLE_BASE_ID is missing.");
        }
        eprintln!(
            "Please ensure they are defined in a .env file in the project root or set in your shell environment."
        );
        eprintln!(
            "Example .env file content:\n  AIRTABLE_API_KEY=\"your_api_key_here\"\n  AIRTABLE_BASE_ID=\"your_base_id_here\""
        );
        eprintln!("Upload aborted due to missing credentials.");
        return Ok(());
    }

    if !valid_fee_items.is_empty() {
        println!(
            "Attempting to upload {} items to Airtable...",
            valid_fee_items.len()
        );
        match upload_to_airtable(&valid_fee_items) {
            Ok(_) => println!("Airtable upload process completed."),
            Err(e) => {
                eprintln!("Airtable upload failed: {}", e);
                eprintln!(
                    "Ensure your Airtable base '{}' and table '{}' exist and column names match.",
                    std::env::var("AIRTABLE_BASE_ID").unwrap_or_default(),
                    AIRTABLE_TABLE_NAME
                );
                eprintln!(
                    "Required columns: 'Permit Name'(Text), 'Price'(Number), 'Category'(Text), 'Import Date'(Date), and optionally 'Price Description'(Text)."
                );
            }
        }
    } else {
        println!("No valid fee items found to upload with numerical prices.");
    }

    Ok(())
}

fn is_price_like(line: &str) -> bool {
    let trimmed = line.trim().to_lowercase();
    trimmed.contains('$')
        || trimmed.starts_with("see")
        || trimmed.starts_with("varies")
        || trimmed.starts_with("minimum")
        || trimmed.starts_with("plus")
        || trimmed.starts_with("per ")
        || trimmed.chars().next().map_or(false, |c| c.is_digit(10))
}

fn is_continuation_phrase(line: &str) -> bool {
    let l = line.to_lowercase();
    l.starts_with("per ")
        || l.starts_with("plus ")
        || l.starts_with("and ")
        || l.starts_with("or ")
        || l.starts_with("up to")
        || l.starts_with("for each")
}

fn upload_to_airtable(items: &[FeeItem]) -> Result<(), Box<dyn Error>> {
    let client = Client::new();
    let api_key = std::env::var("AIRTABLE_API_KEY")?;
    let base_id = std::env::var("AIRTABLE_BASE_ID")?;
    let num_items = items.len();
    let mut uploaded_count = 0;
    let total_batches = (num_items as f64 / AIRTABLE_BATCH_SIZE as f64).ceil() as usize;

    for (batch_num, chunk) in items.chunks(AIRTABLE_BATCH_SIZE).enumerate() {
        let records_to_upload: Vec<_> = chunk
            .iter()
            .filter_map(|item| {
                item.price_num.map(|price_val| {
                    let mut fields = serde_json::Map::new();
                    fields.insert("Permit Name".to_string(), json!(&item.name));
                    fields.insert("Price".to_string(), json!(price_val));
                    fields.insert("Category".to_string(), json!(&item.category));
                    fields.insert(
                        "Import Date".to_string(),
                        json!(chrono::Local::now().to_rfc3339()),
                    );
                    // Optionally, send the full price string to another column named "Price Description"
                    fields.insert("Price Description".to_string(), json!(&item.price_str));
                    json!({ "fields": fields })
                })
            })
            .collect();

        if records_to_upload.is_empty() {
            // This can happen if a chunk has items but none have a parseable price_num
            println!(
                "Batch {}/{} has no records with numerical prices to upload. Skipping.",
                batch_num + 1,
                total_batches
            );
            continue;
        }

        let payload = json!({ "records": records_to_upload });
        println!(
            "Uploading batch {}/{} ({} items)...",
            batch_num + 1,
            total_batches,
            records_to_upload.len()
        );
        match client
            .post(format!(
                "https://api.airtable.com/v0/{}/{}",
                base_id, AIRTABLE_TABLE_NAME
            ))
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
        {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    uploaded_count += records_to_upload.len();
                    println!(
                        "Batch {} uploaded successfully. Total uploaded: {}/{}",
                        batch_num + 1,
                        uploaded_count,
                        num_items
                    );
                } else {
                    let error_body = response
                        .text()
                        .unwrap_or_else(|e| format!("Could not get error text: {}", e));
                    eprintln!(
                        "Failed to upload batch {}: {} - Body: {}",
                        batch_num + 1,
                        status,
                        error_body
                    );
                }
            }
            Err(e) => {
                eprintln!("Error sending request for batch {}: {}", batch_num + 1, e);
            }
        }
        if batch_num + 1 < total_batches && total_batches > 1 {
            thread::sleep(Duration::from_millis(AIRTABLE_REQUEST_DELAY_MS));
        }
    }
    println!(
        "Airtable upload process finished. {} items processed for upload.",
        uploaded_count
    );
    Ok(())
}
