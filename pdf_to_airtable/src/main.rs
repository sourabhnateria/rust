use chrono::NaiveDate;
use log::{error, info, warn};
use pdf_extract::extract_text;
use reqwest::header;
use serde::{Deserialize, Serialize};
use std::env;
// std::num::ParseIntError is implicitly used by `parse::<u32>()`
// std::num::ParseFloatError is implicitly used by `parse::<f64>()`;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
enum ExtractionError {
    #[error("PDF extraction error: {0}")]
    PdfError(String),
    #[error("Environment variable error: {0}")]
    EnvError(String),
    #[error("HTTP request error: {0}")]
    RequestError(String),
    #[error("No invoice items found in PDF")]
    NoItemsFound,
    #[error("PDF file not found: {0}")]
    FileNotFound(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct AirtableRecord {
    fields: InvoiceItem,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct InvoiceItem {
    #[serde(rename = "No")]
    no: u32,
    #[serde(rename = "Description")]
    description: String,
    #[serde(rename = "Quantity")]
    quantity: u32,
    #[serde(rename = "Price")]
    price: f64, // MODIFIED: Changed to f64
    #[serde(rename = "Total")]
    total: f64, // MODIFIED: Changed to f64
    #[serde(rename = "Client Name")] // New Field
    client_name: String,
    #[serde(rename = "Invoice Date")] // New Field
    invoice_date: String,
}

#[derive(Debug, Serialize)]
struct AirtablePayload {
    records: Vec<AirtableRecord>,
}

// Helper function to clean and parse currency strings
fn clean_and_parse_currency(s: &str, field_name: &str, line_number: usize) -> Result<f64, String> {
    let cleaned = s.replace('$', "").replace(',', "").trim().to_string();
    if cleaned.is_empty() {
        // If the original string was also empty or just symbols, it's effectively 0 or an issue.
        // For price/total, an empty value might mean 0.0 or that parsing truly failed to find a number.
        warn!(
            "Empty or non-numeric string for {} on line {}: '{}'. Interpreting as 0.0.",
            field_name, line_number, s
        );
        return Ok(0.0);
    }
    cleaned.parse::<f64>().map_err(|e| {
        format!(
            "Failed to parse {} '{}' (cleaned: '{}') on line {}: {}",
            field_name, s, cleaned, line_number, e
        )
    })
}

async fn upload_to_airtable(items: Vec<InvoiceItem>) -> Result<(), ExtractionError> {
    let base_id =
        env::var("AIRTABLE_BASE_ID").map_err(|e| ExtractionError::EnvError(e.to_string()))?;
    let table_name = env::var("AIRTABLE_TABLE_NAME").unwrap_or_else(|_| "Invoices".to_string());
    let api_key =
        env::var("AIRTABLE_API_KEY").map_err(|e| ExtractionError::EnvError(e.to_string()))?;

    let client = reqwest::Client::new();
    let url = format!("https://api.airtable.com/v0/{}/{}", base_id, table_name);

    for chunk in items.chunks(10) {
        info!("Preparing to upload {} items to Airtable", chunk.len());
        let records: Vec<AirtableRecord> = chunk
            .iter()
            .map(|item| AirtableRecord {
                fields: item.clone(),
            })
            .collect();

        let payload = AirtablePayload { records };

        let response = client
            .post(&url)
            .json(&payload)
            .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
            .send()
            .await
            .map_err(|e| ExtractionError::RequestError(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let response_body = response
                .text()
                .await
                .map_err(|e| ExtractionError::RequestError(e.to_string()))?;
            error!("Airtable API error (status {}): {}", status, response_body);
            return Err(ExtractionError::RequestError(format!(
                "API returned status: {}, body: {}",
                status, response_body
            )));
        }
        info!("Successfully uploaded batch of {} items", chunk.len());
    }
    Ok(())
}

fn extract_text_from_pdf(path: &str) -> Result<String, ExtractionError> {
    if !Path::new(path).exists() {
        return Err(ExtractionError::FileNotFound(path.to_string()));
    }
    info!("Extracting text from PDF: {}", path);
    extract_text(path).map_err(|e| ExtractionError::PdfError(e.to_string()))
}

fn parse_invoice_items(text: &str) -> Result<Vec<InvoiceItem>, ExtractionError> {
    info!("Starting full invoice parsing including client and date.");
    let lines: Vec<&str> = text.lines().collect();

    // --- Phase 1: Extract Client Name and Invoice Date ---
    let mut client_name_str = String::new();
    let mut invoice_date_str = String::new(); // This will store "25 June 2022" directly

    let mut looking_for_client_name_after_to = false;

    for (line_idx, line_content) in lines.iter().enumerate() {
        let trimmed_line = line_content.trim();
        let lower_line = trimmed_line.to_lowercase();

        // DEBUG Line for Phase 1
        info!("[Phase 1 Debug] Line {}: '{}'", line_idx + 1, trimmed_line);

        // Client Name Logic
        if client_name_str.is_empty() {
            if looking_for_client_name_after_to && !trimmed_line.is_empty() {
                client_name_str = trimmed_line.to_string();
                info!("Client Name extracted: {}", client_name_str);
                looking_for_client_name_after_to = false;
            }
            if lower_line == "to" {
                looking_for_client_name_after_to = true;
            }
        }

        // Invoice Date Logic (Strategy for "ValueDate :")
        if invoice_date_str.is_empty() {
            let date_keyword_anchor = "date :";
            if let Some(keyword_pos) = lower_line.rfind(date_keyword_anchor) {
                let potential_date_segment = trimmed_line[..keyword_pos].trim();
                info!(
                    "Found '{}' anchor. Potential date segment before it: '{}'",
                    date_keyword_anchor, potential_date_segment
                );

                let months = [
                    "January",
                    "February",
                    "March",
                    "April",
                    "May",
                    "June",
                    "July",
                    "August",
                    "September",
                    "October",
                    "November",
                    "December",
                ];
                let mut extracted_date_candidate = String::new();

                for month_name in months.iter() {
                    if let Some(month_pos_in_segment) = potential_date_segment.rfind(month_name) {
                        let mut day_start_idx = month_pos_in_segment;
                        let mut day_chars = String::new();
                        for i in (0..month_pos_in_segment).rev() {
                            let char_before_month = potential_date_segment.as_bytes()[i];
                            if char_before_month.is_ascii_digit() {
                                day_chars.insert(0, char_before_month as char);
                                day_start_idx = i;
                            } else if char_before_month.is_ascii_whitespace()
                                && !day_chars.is_empty()
                            {
                                break;
                            } else if !char_before_month.is_ascii_whitespace()
                                && !day_chars.is_empty()
                            {
                                day_start_idx = i + 1;
                                break;
                            } else if !char_before_month.is_ascii_whitespace()
                                && day_chars.is_empty()
                            {
                                break;
                            }
                            if month_pos_in_segment.saturating_sub(i) > 4 && day_chars.is_empty() {
                                break;
                            }
                        }
                        if day_chars.is_empty() {
                            continue;
                        }

                        let mut year_end_idx = month_pos_in_segment + month_name.len();
                        let mut year_chars = String::new();
                        let mut space_after_month_found = false;
                        for char_after_month in potential_date_segment[year_end_idx..].chars() {
                            if char_after_month.is_ascii_whitespace() && year_chars.is_empty() {
                                space_after_month_found = true;
                                year_end_idx += 1;
                                continue;
                            }
                            if char_after_month.is_ascii_digit() {
                                if !space_after_month_found && year_chars.is_empty() { /* Year starting without space after month */
                                } else if !space_after_month_found && !year_chars.is_empty() {
                                    break;
                                } // Digit but no prior space after month started

                                year_chars.push(char_after_month);
                                year_end_idx += 1;
                                if year_chars.len() == 4 {
                                    break;
                                }
                            } else if !year_chars.is_empty() {
                                break;
                            } else if !char_after_month.is_ascii_whitespace() {
                                break;
                            }
                        }
                        if year_chars.len() != 4 {
                            continue;
                        }

                        // Construct the date string directly
                        let final_candidate =
                            format!("{} {} {}", day_chars, month_name, year_chars);

                        // Basic validation: does it look somewhat like "DD Month YYYY"?
                        // Check for presence of day, month, and year components found.
                        if !day_chars.is_empty() && !month_name.is_empty() && year_chars.len() == 4
                        {
                            extracted_date_candidate = final_candidate;
                            info!(
                                "Date candidate from segment: '{}'",
                                extracted_date_candidate
                            );
                            break;
                        }
                    }
                }

                if !extracted_date_candidate.is_empty() {
                    invoice_date_str = extracted_date_candidate; // Store the "25 June 2022" directly
                    info!("Invoice Date extracted: '{}'", invoice_date_str);
                } else {
                    warn!("Found 'date :' anchor, but could not reliably extract date from segment: '{}'", potential_date_segment);
                }
            }
        }
    }

    if client_name_str.is_empty() {
        warn!("Client Name could not be extracted.");
    }
    if invoice_date_str.is_empty() {
        warn!("Invoice Date could not be extracted.");
    }

    // --- Phase 2: Parse Invoice Items ---
    info!("Moving to parse invoice line items.");
    let mut items: Vec<InvoiceItem> = Vec::new();
    // (Your existing, working Phase 2 item parsing logic. Ensure it's complete here)
    // When pushing items, use `invoice_date_str.clone()`
    // For example:
    // items.push(InvoiceItem {
    //     // ... other fields ...
    //     client_name: client_name_str.clone(),
    //     invoice_date: invoice_date_str.clone(), // Use the raw extracted date string
    // });
    // THE FOLLOWING IS YOUR FULL WORKING VERSION OF PHASE 2
    let header_pos = lines.iter().position(|line_content| {
        let lower = line_content.to_lowercase();
        (lower.contains("description")
            && (lower.contains("qty") || lower.contains("quantity"))
            && lower.contains("price"))
            || (lower.contains("no.") && lower.contains("description"))
            || (lower.contains("item") && lower.contains("total"))
    });
    let item_start_index = match header_pos {
        Some(pos) => {
            info!(
                "Found item table header line at position {}: '{}'",
                pos, lines[pos]
            );
            pos + 1
        }
        None => {
            warn!("No clear item table header line found for items. Will attempt from start of text for item parsing.");
            0
        }
    };
    let mut still_parsing_actual_items = true;
    let mut current_line_scan_idx = item_start_index;

    while current_line_scan_idx < lines.len() {
        let line_to_process = lines[current_line_scan_idx];
        let current_line_number_for_log = current_line_scan_idx + 1;
        let trimmed = line_to_process.trim();
        let lower_trimmed = trimmed.to_lowercase();
        let mut advanced_by_lookahead = 0;

        if !still_parsing_actual_items {
            current_line_scan_idx += 1;
            continue;
        }
        if lower_trimmed.contains("subtotal")
            || lower_trimmed.contains("sub total")
            || lower_trimmed.contains("grand total")
            || lower_trimmed.contains("total due")
            || lower_trimmed.contains("vat")
            || lower_trimmed.contains("tax")
            || lower_trimmed.contains("discount")
            || lower_trimmed.contains("amount due")
        {
            info!(
                "Detected summary line {}: '{}'. Stopping item parsing.",
                current_line_number_for_log, trimmed
            );
            still_parsing_actual_items = false;
            current_line_scan_idx += 1;
            continue;
        }
        if trimmed.is_empty() {
            current_line_scan_idx += 1;
            continue;
        }

        let mut description_str = String::new();
        let mut price_str = String::new();
        let mut total_str = String::new();
        let quantity_val: u32 = 1;
        let mut current_item_has_strong_description_initially = false;

        if trimmed.contains('$') {
            if let Some(dollar_pos) = trimmed.find('$') {
                let before_dollar_part = trimmed[..dollar_pos].trim().to_string();
                if !before_dollar_part.is_empty() {
                    if let Some(idx_last_char_before_num) =
                        before_dollar_part.rfind(|c: char| !c.is_numeric())
                    {
                        let potential_desc = before_dollar_part[..=idx_last_char_before_num].trim();
                        if !potential_desc.is_empty() {
                            description_str = potential_desc.to_string();
                        }
                    } else if !before_dollar_part.chars().all(char::is_numeric) {
                        description_str = before_dollar_part;
                    }
                }
                let after_dollar_part = trimmed[dollar_pos..].trim();
                let money_parts: Vec<&str> = after_dollar_part
                    .split('$')
                    .filter(|s| !s.is_empty())
                    .collect();
                if !money_parts.is_empty() {
                    price_str = money_parts[0].trim().to_string();
                    if money_parts.len() >= 2 {
                        let second_money_part_raw = money_parts[1].trim();
                        let mut temp_total_numeric_part = second_money_part_raw
                            .chars()
                            .take_while(|&c| c.is_ascii_digit() || c == ',' || c == '.')
                            .collect::<String>();
                        let price_cleaned_for_heuristic = price_str.replace([',', '$'], "");
                        let qty_for_heuristic = quantity_val.to_string();
                        let temp_total_cleaned_for_heuristic =
                            temp_total_numeric_part.replace([',', '$'], "");
                        if temp_total_cleaned_for_heuristic
                            == format!("{}{}", price_cleaned_for_heuristic, qty_for_heuristic)
                        {
                            total_str = price_str.clone();
                            let assumed_price_qty_len =
                                price_cleaned_for_heuristic.len() + qty_for_heuristic.len();
                            if description_str.is_empty()
                                && assumed_price_qty_len < second_money_part_raw.len()
                            {
                                let remaining_text =
                                    second_money_part_raw[assumed_price_qty_len..].trim();
                                if !remaining_text.is_empty() {
                                    description_str = remaining_text.to_string();
                                }
                            }
                        } else {
                            total_str = temp_total_numeric_part;
                            if description_str.is_empty()
                                && total_str.len() < second_money_part_raw.len()
                            {
                                let remaining_text =
                                    second_money_part_raw[total_str.len()..].trim();
                                if !remaining_text.is_empty() {
                                    description_str = remaining_text.to_string();
                                }
                            }
                        }
                    } else {
                        total_str = price_str.clone();
                    }
                }
            }
            if !description_str.trim().is_empty()
                && !description_str.trim().chars().all(char::is_numeric)
            {
                current_item_has_strong_description_initially = true;
            }
            if !current_item_has_strong_description_initially && still_parsing_actual_items {
                let mut consumed_in_lookahead = 0;
                for i in 1..=2 {
                    let next_line_idx = current_line_scan_idx + i;
                    if next_line_idx < lines.len() {
                        let next_line_candidate = lines[next_line_idx].trim();
                        if next_line_candidate.is_empty() {
                            consumed_in_lookahead += 1;
                            continue;
                        }
                        let next_line_lower_candidate = next_line_candidate.to_lowercase();
                        if next_line_lower_candidate.contains("subtotal")
                            || next_line_lower_candidate.contains("grand total")
                            || next_line_candidate.contains('$')
                        {
                            break;
                        }
                        description_str = next_line_candidate.to_string();
                        consumed_in_lookahead += 1;
                        advanced_by_lookahead = consumed_in_lookahead;
                        info!(
                            "Used lookahead description '{}' for monetary line from line {}",
                            description_str,
                            next_line_idx + 1
                        );
                        break;
                    } else {
                        break;
                    }
                }
            }
            if !price_str.is_empty() {
                let price_val =
                    clean_and_parse_currency(&price_str, "Price", current_line_number_for_log)
                        .unwrap_or_else(|e| {
                            warn!("Price parse error: {}. Line: '{}'", e, trimmed);
                            0.0
                        });
                let total_val =
                    clean_and_parse_currency(&total_str, "Total", current_line_number_for_log)
                        .unwrap_or_else(|e| {
                            warn!(
                                "Total parse error: {}. Line: '{}', defaulting to price",
                                e, trimmed
                            );
                            price_val
                        });
                items.push(InvoiceItem {
                    no: 0,
                    description: description_str.trim().to_string(),
                    quantity: quantity_val,
                    price: price_val,
                    total: total_val,
                    client_name: client_name_str.clone(),
                    invoice_date: invoice_date_str.clone(), // Use the raw extracted date string
                });
            } else {
                warn!(
                    "Monetary line {} did not yield a price: '{}'",
                    current_line_number_for_log, trimmed
                );
            }
        } else if still_parsing_actual_items {
            let mut applied_as_description = false;
            for item_to_patch in items.iter_mut() {
                let desc_is_weak = item_to_patch.description.trim().is_empty()
                    || item_to_patch
                        .description
                        .trim()
                        .chars()
                        .all(char::is_numeric);
                if desc_is_weak {
                    info!(
                        "Applying textual line '{}' as description for item (Price: {:.2})",
                        trimmed, item_to_patch.price
                    );
                    item_to_patch.description = trimmed.to_string();
                    applied_as_description = true;
                    break;
                }
            }
            if !applied_as_description {
                warn!("Textual line '{}' found, but no prior items had a weak description or all item descriptions were already strong.", trimmed);
            }
        }
        current_line_scan_idx += 1 + advanced_by_lookahead;
    }

    if items.is_empty() {
        error!("No invoice items found after parsing all relevant lines.");
        return Err(ExtractionError::NoItemsFound);
    } else {
        for (i, item) in items.iter_mut().enumerate() {
            item.no = (i + 1) as u32;
            info!("Final Item {}: Client: '{}', Date: '{}', Desc: '{}', Qty: {}, Price: {:.2}, Total: {:.2}", 
                  item.no, item.client_name, item.invoice_date, item.description, item.quantity, item.price, item.total);
        }
        info!("Successfully parsed {} invoice items.", items.len());
        Ok(items)
    }
}

#[tokio::main]
async fn main() -> Result<(), ExtractionError> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    match dotenv::dotenv() {
        Ok(path) => info!("Loaded .env file from: {:?}", path),
        Err(_) => warn!("No .env file found or failed to load. Relying on environment variables."),
    }

    let pdf_path = env::var("PDF_PATH").unwrap_or_else(|_| "Invoice_Template.pdf".to_string());
    info!("Starting PDF to Airtable processor");
    info!("Loading PDF file: {}", pdf_path);

    let text = extract_text_from_pdf(&pdf_path)?;
    let items = parse_invoice_items(&text)?;

    if !items.is_empty() {
        upload_to_airtable(items).await?;
    } else {
        info!("No items were parsed from the PDF to upload.");
    }

    info!("Processing completed successfully");
    Ok(())
}
