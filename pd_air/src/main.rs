use chrono::NaiveDate; // Used for date string validation in Phase 1
use log::{error, info, warn};
use reqwest::header;
use serde::{Deserialize, Serialize};
use std::env;
use std::path::Path;
use thiserror::Error;

// lopdf related imports
use lopdf::{Dictionary, Document, Object, Stream as LopdfStream};

#[derive(Error, Debug)]
enum ExtractionError {
    #[error("PDF parsing error (lopdf): {0}")]
    LopdfError(#[from] lopdf::Error),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Environment variable error: {0}")]
    EnvError(String),
    #[error("HTTP request error: {0}")]
    RequestError(String),
    #[error("No invoice items found in PDF")]
    NoItemsFound,
    #[error("PDF file not found: {0}")]
    FileNotFound(String),
    // #[error("Content decoding error: {0}")]
    // ContentDecodeError(String),
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
    price: f64,
    #[serde(rename = "Total")]
    total: f64,
    #[serde(rename = "Client Name")]
    client_name: String,
    #[serde(rename = "Invoice Date")]
    invoice_date: String,
}

#[derive(Debug, Serialize)]
struct AirtablePayload {
    records: Vec<AirtableRecord>,
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

fn clean_and_parse_currency(s: &str, field_name: &str, line_number: usize) -> Result<f64, String> {
    let cleaned = s.replace('$', "").replace(',', "").trim().to_string();
    if cleaned.is_empty() {
        return Err(format!(
            "Empty string after cleaning for {} on line {}",
            field_name, line_number
        ));
    }
    cleaned.parse::<f64>().map_err(|e| {
        format!(
            "Failed to parse {} '{}' (cleaned: '{}') on line {}: {}",
            field_name, s, cleaned, line_number, e
        )
    })
}

fn decode_text_bytes(bytes: &[u8], _font_info: Option<&lopdf::Dictionary>) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        if let Ok(s) = String::from_utf16(
            &bytes[2..]
                .chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect::<Vec<u16>>(),
        ) {
            return s;
        }
    }
    if let Ok(s) = String::from_utf8(bytes.to_vec()) {
        return s;
    }
    if bytes.len() % 2 == 0 && bytes.len() > 0 {
        let utf16_words: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
            .collect();
        if let Ok(s_utf16) = String::from_utf16(&utf16_words) {
            if s_utf16.chars().any(|c| !c.is_control() && c != '\u{FFFD}') {
                return s_utf16;
            }
        }
    }
    String::from_utf8_lossy(bytes).to_string()
}

fn extract_text_from_pdf_lopdf(path: &str) -> Result<String, ExtractionError> {
    info!("Extracting text from PDF using lopdf: {}", path);
    if !Path::new(path).exists() {
        return Err(ExtractionError::FileNotFound(path.to_string()));
    }

    let doc = Document::load(path)?;
    let mut full_text = String::new();

    for page_id_tuple in doc.page_iter() {
        let mut current_page_text = String::new();
        match doc.get_page_content(page_id_tuple) {
            Ok(content_data) => {
                let stream = LopdfStream::new(Dictionary::new(), content_data);
                match stream.decode_content() {
                    Ok(decoded_content) => {
                        let operations = decoded_content.operations;
                        for operation in operations {
                            match operation.operator.as_str() {
                                "Tj" | "'" => {
                                    if let Some(Object::String(text_bytes, _)) =
                                        operation.operands.get(0)
                                    {
                                        current_page_text
                                            .push_str(&decode_text_bytes(text_bytes, None));
                                    }
                                    if operation.operator == "'" {
                                        current_page_text.push('\n');
                                    }
                                }
                                "TJ" => {
                                    if let Some(Object::Array(arr)) = operation.operands.get(0) {
                                        for obj in arr {
                                            if let Object::String(text_bytes, _) = obj {
                                                current_page_text
                                                    .push_str(&decode_text_bytes(text_bytes, None));
                                            }
                                        }
                                    }
                                }
                                "\"" => {
                                    if let Some(Object::String(text_bytes, _)) =
                                        operation.operands.get(2)
                                    {
                                        current_page_text
                                            .push_str(&decode_text_bytes(text_bytes, None));
                                    }
                                    current_page_text.push('\n');
                                }
                                "Td" | "TD" | "T*" => {
                                    if !current_page_text.is_empty()
                                        && !current_page_text.ends_with('\n')
                                    {
                                        current_page_text.push('\n');
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Could not decode content stream for page {:?}: {:?}",
                            page_id_tuple, e
                        );
                    }
                }
            }
            Err(e) => {
                warn!(
                    "Could not get content for page {:?}: {:?}",
                    page_id_tuple, e
                );
            }
        }
        full_text.push_str(&current_page_text);
        if !current_page_text.is_empty() {
            full_text.push_str("\n\n--- Page Break ---\n\n");
        }
    }

    if full_text.is_empty() && doc.page_iter().count() > 0 {
        warn!("lopdf extraction resulted in empty text for a document with pages.");
    } else {
        info!(
            "lopdf extraction: First 500 chars: {}",
            full_text.chars().take(500).collect::<String>()
        );
    }
    Ok(full_text)
}

fn parse_invoice_items(text: &str) -> Result<Vec<InvoiceItem>, ExtractionError> {
    info!("Starting full invoice parsing including client and date.");
    let lines: Vec<&str> = text.lines().collect();

    // --- Phase 1: Extract Client Name and Invoice Date ---
    let mut client_name_str = String::new();
    let mut invoice_date_str = String::new();
    let mut looking_for_client_name_after_to = false;
    let mut found_date_keyword_line = false; // Flag: true if "Date :" line is found

    for (line_idx, line_content) in lines.iter().enumerate() {
        let trimmed_line = line_content.trim();
        let lower_line = trimmed_line.to_lowercase();

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

        // Invoice Date Logic (for lopdf output: "Date :" then value on a subsequent line)
        if invoice_date_str.is_empty() {
            if found_date_keyword_line {
                if !trimmed_line.is_empty() {
                    // The line after "Date :" might be Invoice No or the Date itself.
                    // Check if current line is a parseable date.
                    if NaiveDate::parse_from_str(trimmed_line, "%d %B %Y").is_ok() {
                        invoice_date_str = trimmed_line.to_string();
                        info!("Invoice Date extracted: '{}'", invoice_date_str);
                        found_date_keyword_line = false; // Date found, stop this specific search path
                    } else {
                        info!("Line '{}' after 'Date :' was not the date (e.g. might be Invoice No), still looking for date.", trimmed_line);
                        // Keep found_date_keyword_line = true to check the *next* non-empty line.
                        // This handles the case: Date : \n InvoiceNo \n ActualDate
                    }
                }
                // If trimmed_line is empty, flag remains true to process next non-empty line.
            } else if trimmed_line == "Date :" {
                // Exact match for "Date :" on its own line
                info!("Found 'Date :' keyword on line {}.", line_idx + 1);
                found_date_keyword_line = true;
            }
            // Note: The "ValueDate :" pattern from previous PDF is removed as logs show "Date :" is on its own line now.
        }
    }

    if client_name_str.is_empty() {
        warn!("Client Name could not be extracted.");
    }
    if invoice_date_str.is_empty() {
        warn!("Invoice Date could not be extracted.");
    }

    // --- Phase 2: Parse Invoice Items ---
    info!("Moving to parse invoice line items based on Price/Total on separate lines.");
    let mut items: Vec<InvoiceItem> = Vec::new();

    let header_pos_items = lines.iter().position(|line_content| {
        let lower = line_content.to_lowercase().replace(" ", "");
        lower == "total"
    });
    let item_start_index = match header_pos_items {
        Some(pos) => {
            info!(
                "Found end-of-header line ('TOTAL') at position {}: '{}'",
                pos, lines[pos]
            );
            pos + 1
        }
        None => {
            warn!("Could not find 'TOTAL' as end-of-header line. Trying to find first numeric item number.");
            // Fallback: find first line that is purely numeric and short (likely an item number)
            lines
                .iter()
                .skip_while(|l| {
                    l.trim().is_empty()
                        || !l
                            .trim()
                            .chars()
                            .next()
                            .map_or(false, |c| c.is_numeric() || l.trim().len() > 3)
                })
                .position(|l| l.trim().chars().all(char::is_numeric) && l.trim().len() < 3)
                .unwrap_or(0)
        }
    };
    info!(
        "Item parsing will start from line index: {}",
        item_start_index
    );

    let mut current_item_no_str = String::new();
    let mut current_description_str = String::new();
    let mut current_quantity_str = String::new();
    let mut current_price_str = String::new();
    // current_total_str will be the line after price

    let mut line_idx = item_start_index;
    while line_idx < lines.len() {
        let trimmed_line = lines[line_idx].trim();
        info!("[Item Parse] Line {}: '{}'", line_idx + 1, trimmed_line);

        if trimmed_line.is_empty() {
            line_idx += 1;
            continue;
        }
        // More robust stop condition based on typical summary section keywords
        let lower_trimmed_line = trimmed_line.to_lowercase();
        if lower_trimmed_line.starts_with("payment method")
            || lower_trimmed_line.starts_with("sub total")
            || lower_trimmed_line.starts_with("vat")
            || lower_trimmed_line.starts_with("discount")
            || lower_trimmed_line.starts_with("grand total")
            || lower_trimmed_line.starts_with("term and conditions")
        {
            info!(
                "Stopping item parsing at summary section: '{}'",
                trimmed_line
            );
            break;
        }

        // State machine for item parts
        if current_item_no_str.is_empty()
            && trimmed_line.chars().all(char::is_numeric)
            && trimmed_line.len() < 3
        {
            current_item_no_str = trimmed_line.to_string();
            info!("  -> ItemNo: {}", current_item_no_str);
        } else if !current_item_no_str.is_empty()
            && current_description_str.is_empty()
            && !trimmed_line.contains('$')
            && !trimmed_line.chars().all(char::is_numeric)
        {
            current_description_str = trimmed_line.to_string();
            info!("  -> Description: {}", current_description_str);
        } else if !current_description_str.is_empty()
            && current_quantity_str.is_empty()
            && trimmed_line.chars().all(char::is_numeric)
        {
            current_quantity_str = trimmed_line.to_string();
            info!("  -> Quantity: {}", current_quantity_str);
        } else if !current_quantity_str.is_empty()
            && current_price_str.is_empty()
            && trimmed_line.contains('$')
        {
            current_price_str = trimmed_line.to_string();
            info!("  -> Price: {}", current_price_str);
        } else if !current_price_str.is_empty() && trimmed_line.contains('$') {
            let current_total_str = trimmed_line.to_string();
            info!("  -> Total: {}", current_total_str);

            let quantity_val: u32 = current_quantity_str.parse().unwrap_or_else(|e| {
                warn!(
                    "Failed to parse quantity '{}': {}. Defaulting to 1.",
                    current_quantity_str, e
                );
                1
            });

            let price_val = clean_and_parse_currency(&current_price_str, "Price", line_idx)
                .unwrap_or_else(|e| {
                    warn!("Error parsing price '{}': {}", current_price_str, e);
                    0.0
                });
            let total_val = clean_and_parse_currency(&current_total_str, "Total", line_idx + 1)
                .unwrap_or_else(|e| {
                    warn!("Error parsing total '{}': {}", current_total_str, e);
                    price_val
                });

            items.push(InvoiceItem {
                no: current_item_no_str
                    .parse()
                    .unwrap_or((items.len() + 1) as u32),
                description: current_description_str.clone(),
                quantity: quantity_val,
                price: price_val,
                total: total_val,
                client_name: client_name_str.clone(),
                invoice_date: invoice_date_str.clone(),
            });
            info!("  SUCCESSFULLY PARSED ITEM: {:?}", items.last().unwrap());

            current_item_no_str.clear();
            current_description_str.clear();
            current_quantity_str.clear();
            current_price_str.clear();
        } else {
            if !current_item_no_str.is_empty() {
                warn!("  -> Unexpected line '{}' while parsing item starting with No '{}'. Resetting current item state.", trimmed_line, current_item_no_str);
                current_item_no_str.clear();
                current_description_str.clear();
                current_quantity_str.clear();
                current_price_str.clear();
            } else if !trimmed_line.starts_with("--- Page Break ---") {
                // Don't warn for page breaks
                info!(
                    "  -> Skipping line '{}' as it doesn't fit expected item structure start.",
                    trimmed_line
                );
            }
        }
        line_idx += 1;
    }

    if items.is_empty() {
        error!("No invoice items found after parsing all relevant lines.");
        return Err(ExtractionError::NoItemsFound);
    } else {
        for item in items.iter() {
            info!("Final Item: Client: '{}', Date: '{}', No: {}, Desc: '{}', Qty: {}, Price: {:.2}, Total: {:.2}", 
                  item.client_name, item.invoice_date, item.no, item.description, item.quantity, item.price, item.total);
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

    let text = extract_text_from_pdf_lopdf(&pdf_path)?;

    let items = parse_invoice_items(&text)?;

    if !items.is_empty() {
        upload_to_airtable(items).await?;
    } else {
        info!("No items were parsed from the PDF to upload.");
    }

    info!("Processing completed successfully");
    Ok(())
}
