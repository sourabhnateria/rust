use clap::Parser;
use image::{ImageBuffer, Rgba, RgbaImage};
// Corrected lopdf imports:
use lopdf::{
    content::{Content, Operation},
    dictionary, // The macro for creating dictionaries
    Dictionary, // The Dictionary type
    Document,
    Object,
    Stream,
    // Rect was removed
};
use rusttype::{Font, Scale, point};
use serde::Deserialize;
use std::fs;
use std::path::Path;
// use std::error::Error; // Needed for Box<dyn std::error::Error>
use chrono::Local; // For current date

// --- New Constants for Document Text ---
const PAGE_MARGIN: f32 = 72.0; 
const FONT_SIZE_TITLE: f32 = 18.0;
const FONT_SIZE_HEADING: f32 = 14.0;
const FONT_SIZE_NORMAL: f32 = 11.0;
const DOCUMENT_LINE_HEIGHT_FACTOR: f32 = 2.0; 

// --- Constants ---
const PAGE_WIDTH_PT: f32 = 595.0; 
const PAGE_HEIGHT_PT: f32 = 842.0; 
// const SIGNATURE_FONT_SIZE: f32 = 35.0; // Default, will be overridden by config if present - Defined in AppConfig or main
const IMAGE_SCALE_FACTOR: f32 = 2.0;
const SIGNATURE_IMAGE_PADDING: u32 = (5.0 * IMAGE_SCALE_FACTOR) as u32; 
const BACKGROUND_COLOR: Rgba<u8> = Rgba([255, 255, 255, 255]);
const TEXT_COLOR: Rgba<u8> = Rgba([0, 0, 0, 255]); 
const SIGNATURE_LINE_SPACING_RATIO: f32 = 1.3; 
const FONTS_DIR: &str = "fonts";
const CONFIG_FILE_NAME: &str = "config.toml";

// --- Structs for Document Content ---
#[derive(Debug)]
struct TextElement {
    text: String,
    size: f32,
    is_bold: bool,
    is_centered: bool,
    indent: f32,
    space_after: f32, 
    is_signature_line_trigger: bool, 
}

#[derive(Debug)]
enum ContentItemInternal { 
    Text(TextElement),
}

// --- App Configuration Struct ---
#[derive(Deserialize, Debug, Default)]
struct AppConfig {
    name: Option<String>,
    address: Option<String>,
    company: Option<String>, 

    signature_text: Option<String>, 
    font_filename: Option<String>,  
    signature_render_font_size: Option<f32>, 

    // default_placement_x: Option<f32>, 
    // default_placement_y: Option<f32>, 
    signature_image_x_offset: Option<f32>,
    signature_image_y_adjust: Option<f32>,
}

// --- Image Placement Enum ---
#[derive(Debug, Clone, Copy)]
pub enum ImagePlacement { 
    TopLeft, TopCenter, TopRight, CenterLeft, Center, CenterRight, BottomLeft, BottomCenter, BottomRight, Custom { x: f32, y: f32 },
}

// --- Command-Line Arguments ---
#[derive(Parser, Debug)]
#[clap(author, version, about = "Generates a PDF document with text and a signature image.")]
struct Args {
    #[clap(short = 't', long, help = "Text to render for the signature image.")]
    signature_image_text: Option<String>,
    #[clap(short = 'f', long, help = "Filename of the TTF/OTF font for the signature image.")]
    signature_font: Option<String>,
    #[clap(short, long, default_value = "output.pdf", help = "Output PDF filename.")] 
    output: String,
    #[clap(short, long, value_parser = parse_placement_arg, help = "Absolute position of signature image (if not flowed).")] 
    placement: Option<ImagePlacement>,
}


fn parse_placement_arg(s: &str) -> Result<ImagePlacement, String> { 
    match s.to_lowercase().as_str() {
        "topleft" | "top-left" => Ok(ImagePlacement::TopLeft),
        "topcenter" | "top-center" => Ok(ImagePlacement::TopCenter),
        "topright" | "top-right" => Ok(ImagePlacement::TopRight),
        "centerleft" | "center-left" => Ok(ImagePlacement::CenterLeft),
        "center" => Ok(ImagePlacement::Center),
        "centerright" | "center-right" => Ok(ImagePlacement::CenterRight),
        "bottomleft" | "bottom-left" => Ok(ImagePlacement::BottomLeft),
        "bottomcenter" | "bottom-center" => Ok(ImagePlacement::BottomCenter),
        "bottomright" | "bottom-right" => Ok(ImagePlacement::BottomRight),
        custom if custom.starts_with("custom:") => {
            let coords_str = custom.trim_start_matches("custom:");
            let parts: Vec<&str> = coords_str.split(',').collect();
            if parts.len() == 2 {
                if let (Ok(x), Ok(y)) = (parts[0].trim().parse::<f32>(), parts[1].trim().parse::<f32>()) {
                    Ok(ImagePlacement::Custom { x, y })
                } else { Err("Invalid coordinates for custom placement. Expected numbers, e.g., 'custom:100.0,200.5'".to_string()) }
            } else { Err("Invalid format for custom placement. Expected 'custom:X,Y'".to_string()) }
        }
        _ => Err(format!("Unknown placement: '{}'. Valid options: TopLeft, Center, BottomRight, custom:X,Y etc.", s)),
    }
}

fn load_app_config(path: &str) -> Result<AppConfig, Box<dyn std::error::Error>> {
    let config_content = fs::read_to_string(path)?;
    let config: AppConfig = toml::from_str(&config_content)?;
    Ok(config)
}

fn main() {
    let args = Args::parse();

    let app_config = load_app_config(CONFIG_FILE_NAME).unwrap_or_else(|err| {
        let mut log_warning = true;
        if let Some(io_err) = err.downcast_ref::<std::io::Error>() { 
            if io_err.kind() == std::io::ErrorKind::NotFound { log_warning = false; }
        }
        if log_warning {
            eprintln!("Warning: Could not load or parse '{}' (Error: {}). Using program defaults/CLI args.", CONFIG_FILE_NAME, err);
        }
        AppConfig::default()
    });

    let doc_name = app_config.name.clone().unwrap_or_else(|| "[Your Name]".to_string());
    let doc_address = app_config.address.clone().unwrap_or_else(|| "[Your Address]".to_string());
    let doc_company = app_config.company.clone().unwrap_or_else(|| "[Your Company]".to_string());
    let current_date_str = Local::now().format("%B %d, %Y").to_string();

    let signature_img_text_to_render = args.signature_image_text
        .or_else(|| app_config.signature_text.clone())
        .unwrap_or_else(|| "Signature".to_string()); 

    let signature_font_filename = args.signature_font
        .or_else(|| app_config.font_filename.clone())
        .unwrap_or_else(|| "StylishCalligraphyDemo-XPZZ.ttf".to_string()); 
    
    let signature_render_font_size = app_config.signature_render_font_size.unwrap_or(35.0); 

    let signature_image_x_offset_from_text = app_config.signature_image_x_offset.unwrap_or(5.0); 
    let signature_image_y_adjustment = app_config.signature_image_y_adjust.unwrap_or(-5.0); 


    let signature_font_path_str = format!("{}/{}", FONTS_DIR, signature_font_filename);
    let signature_font_path = Path::new(&signature_font_path_str);

    if !Path::new(FONTS_DIR).exists() { /* ... */ return; }
    if !signature_font_path.exists() { /* ... */ return; }

    let mut doc = Document::with_version("1.5");
    doc.trailer.set("Creator", Object::string_literal("Rust PDF Generator"));
    let pages_id = doc.add_object(dictionary! {"Type" => "Pages", "Kids" => vec![], "Count" => 0}); 
    let catalog_id = doc.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages_id});
    doc.trailer.set("Root", catalog_id);

    let estimated_signature_img_height_pt = signature_render_font_size * 1.2; 
    let signature_line_text_trigger = "Signature: ".to_string();

    let content_items: Vec<ContentItemInternal> = vec![
        ContentItemInternal::Text(TextElement { text: "Affidavit & Non-Disclosure Statement".to_string(), size: FONT_SIZE_TITLE, is_bold: true, is_centered: true, indent: 0.0, space_after: FONT_SIZE_TITLE * DOCUMENT_LINE_HEIGHT_FACTOR * 1.5, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: format!("Name: {}", doc_name), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 0.2, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: format!("Address: {}", doc_address), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 0.2, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: format!("Companies: {}", doc_company), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 1.5, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: format!("I, {}, acknowledge and reaffirm my obligations to maintain the confidentiality", doc_name), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: 0.0, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "of all sensitive, proprietary, and confidential information obtained during my engagement/term".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: 0.0, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "with Save Efforts LLC and Qualyval Ltd. By signing this Non-Disclosure Statement, I agree to".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: 0.0, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "the following terms:".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 1.5, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "1. Non-Disclosure of Confidential Information".to_string(), size: FONT_SIZE_HEADING, is_bold: true, is_centered: false, indent: 0.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 0.5, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "I confirm that during my working term, I will have access to confidential and proprietary".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: 0.0, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "information belonging to Save Efforts LLC and Qualyval Ltd, including but not limited to:".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 0.5, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "-  Client identities, business relationships, and personal or professional connections.".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 20.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 0.2, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "-  Source codes, projects, algorithms, and proprietary software.".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 20.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 0.2, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "-  Accounts, usernames, passwords, and access credentials for company systems.".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 20.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 0.2, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "-  Project plans, strategies, and other materials marked or intended as confidential.".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 20.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 0.5, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "I agree not to disclose, share, or transmit any such information to any individual, business,".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: 0.0, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "or entity under any circumstances, both during and after my term.".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 1.5, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "2. Restrictions on Data Usage and Retention".to_string(), size: FONT_SIZE_HEADING, is_bold: true, is_centered: false, indent: 0.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 0.5, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "I confirm that I will not download, copy, transfer, or retain any company data, information, or".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: 0.0, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "content in any form. This includes, but is not limited to, digital files, hard copies, and data".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: 0.0, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "stored on personal devices, external storage, or cloud platforms.".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 0.5, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "I further confirm that all access credentials, materials, and hardware provided during my work".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: 0.0, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "tenure will be returned, and I have not and will not retain copies in any form.".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 1.5, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "3. Intellectual Property".to_string(), size: FONT_SIZE_HEADING, is_bold: true, is_centered: false, indent: 0.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 0.5, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "I acknowledge that all work products created during my engagement, including software,".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: 0.0, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "documents, and designs, are the sole and exclusive property of Save Efforts LLC and".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: 0.0, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "Qualyval Ltd. I disclaim any ownership or rights to these works and agree to assist the".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: 0.0, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "company in protecting its intellectual property rights if required.".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 1.5, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "4. Non-Disparagement".to_string(), size: FONT_SIZE_HEADING, is_bold: true, is_centered: false, indent: 0.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 0.5, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "I agree not to make any statements or engage in conduct that may harm the reputation or".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: 0.0, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "goodwill of Save Efforts LLC, Qualyval Ltd, or their affiliates. This includes refraining from".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: 0.0, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "any defamatory or negative remarks about the company, its employees, or its business operations.".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 1.5, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "5. Post-Employment Obligations".to_string(), size: FONT_SIZE_HEADING, is_bold: true, is_centered: false, indent: 0.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 0.5, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "I agree not to solicit, engage with, or attempt to initiate business with any of the company's".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: 0.0, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "clients, partners, or contacts for a period of ten years after I leave the organisation. My".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: 0.0, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "commitment to maintaining confidentiality remains binding and enforceable beyond my period".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: 0.0, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "of engagement.".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 1.5, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "6. Consequences of Unauthorized Disclosure or Misconduct".to_string(), size: FONT_SIZE_HEADING, is_bold: true, is_centered: false, indent: 0.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 0.5, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "I understand and accept that any unauthorised disclosure or misuse of proprietary information".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: 0.0, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "or breach of this declaration may result in:".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 0.5, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "-  A financial penalty of $1,000,000 per breach.".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 20.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 0.2, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "-  Legal action by the company, including injunctive relief or other remedies.".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 20.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 0.2, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "-  Notification of misconduct to future employers, institutions, or government authorities.".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 20.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 1.5, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "7. Declaration of Data Deletion".to_string(), size: FONT_SIZE_HEADING, is_bold: true, is_centered: false, indent: 0.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 0.5, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "I declare that I will not retain or preserve any company data, files, or documents in any form.".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: 0.0, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "This includes information stored on personal devices, external drives, or cloud accounts.".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: 0.0, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "I confirm that all copies will be permanently deleted or destroyed.".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 1.5, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "Acknowledgment".to_string(), size: FONT_SIZE_HEADING, is_bold: true, is_centered: false, indent: 0.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 0.5, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "By signing below, I confirm that I have read, understood, and agreed to this Non-Disclosure".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: 0.0, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "Statement. I understand that failure to comply with these terms may result in penalties and".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: 0.0, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { text: "enforcement actions as deemed necessary by Save Efforts LLC or Qualyval Ltd.".to_string(), size: FONT_SIZE_NORMAL, is_bold: false, is_centered: false, indent: 0.0, space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 4.0, is_signature_line_trigger: false }),
        ContentItemInternal::Text(TextElement { 
            text: signature_line_text_trigger.clone(), 
            size: FONT_SIZE_NORMAL, 
            is_bold: true, 
            is_centered: false, 
            indent: 0.0, 
            space_after: estimated_signature_img_height_pt.min(FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 0.5) + FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 1.0, 
            is_signature_line_trigger: true, 
        }),
        ContentItemInternal::Text(TextElement { 
            text: format!("Name : {}", doc_name), 
            size: FONT_SIZE_NORMAL, 
            is_bold: true, 
            is_centered: false, 
            indent: 0.0, 
            space_after: FONT_SIZE_NORMAL * DOCUMENT_LINE_HEIGHT_FACTOR * 1.5,
            is_signature_line_trigger: false,
        }),
        ContentItemInternal::Text(TextElement { 
            text: format!("Date: {}", current_date_str), 
            size: FONT_SIZE_NORMAL, 
            is_bold: true, 
            is_centered: false, 
            indent: 0.0, 
            space_after: 0.0,
            is_signature_line_trigger: false,
        }),
    ];

    match fs::read(signature_font_path) {
        Ok(font_data) => {
            if let Some(signature_render_font) = Font::try_from_vec(font_data) {
                println!("Rendering signature image \"{}\" with font: {}", signature_img_text_to_render, signature_font_filename);

                match render_text_to_image_for_signature(
                    &signature_render_font, 
                    &signature_img_text_to_render,
                    signature_render_font_size, 
                ) {
                    Some((signature_image_buffer, sig_img_width_px, sig_img_height_px)) => {
                        if sig_img_width_px == 0 || sig_img_height_px == 0 { /* ... */ return; }
                        println!("Rendered signature image: {}x{} pixels.", sig_img_width_px, sig_img_height_px);
                        if let Err(e) = signature_image_buffer.save("debug_signature_render.png") {
                             eprintln!("Failed to save debug_signature_render.png: {}", e);
                        }

                        if let Err(e) = create_document_with_text_and_signature(
                            &mut doc,
                            pages_id,
                            &content_items,
                            &signature_image_buffer,
                            sig_img_width_px,
                            sig_img_height_px,
                            signature_image_x_offset_from_text, 
                            signature_image_y_adjustment,    
                            signature_line_text_trigger.clone(), 
                        ) {
                            eprintln!("Error creating PDF document: {}", e);
                        } else {
                            if let Err(e) = doc.save(&args.output) {
                                eprintln!("Failed to save PDF '{}': {}", args.output, e);
                            } else {
                                println!("Successfully created PDF: {}", args.output);
                            }
                        }
                    }
                    None => eprintln!("Failed to render signature text to image."),
                }
            } else { eprintln!("Failed to parse signature font data from {}.", signature_font_filename); }
        }
        Err(e) => { eprintln!("Failed to read signature font file {}: {}", signature_font_filename, e); }
    }
}

fn render_text_to_image_for_signature(
    font: &Font, 
    text: &str,
    target_font_size_pt: f32, 
) -> Option<(RgbaImage, u32, u32)> {
    let rendering_font_size_pixels = target_font_size_pt * IMAGE_SCALE_FACTOR; 
    let scale = Scale::uniform(rendering_font_size_pixels);
    
    let lines: Vec<&str> = text.lines().collect();
    let display_lines = if lines.is_empty() || lines.iter().all(|l| l.trim().is_empty()) {
        let min_dim = SIGNATURE_IMAGE_PADDING.saturating_mul(2).max(1);
        return Some((ImageBuffer::from_pixel(min_dim, min_dim, BACKGROUND_COLOR), min_dim, min_dim));
    } else {
        lines
    };

    let mut min_x_overall = i32::MAX;
    let mut max_x_overall = i32::MIN;
    let mut min_y_overall = i32::MAX;
    let mut max_y_overall = i32::MIN;
    let mut has_glyphs_with_bb = false;
    let line_height_pixels = (rendering_font_size_pixels * SIGNATURE_LINE_SPACING_RATIO).ceil();
    let v_metrics = font.v_metrics(scale);

    let mut current_baseline_y_for_layout = 0.0; 

    for line_text in display_lines.iter() {
        let glyphs = font.layout(line_text, scale, point(0.0, current_baseline_y_for_layout));
        let mut line_has_glyphs = false;
        for glyph in glyphs {
            if let Some(bb) = glyph.pixel_bounding_box() {
                has_glyphs_with_bb = true;
                line_has_glyphs = true;
                min_x_overall = min_x_overall.min(bb.min.x);
                max_x_overall = max_x_overall.max(bb.max.x);
                min_y_overall = min_y_overall.min(bb.min.y); 
                max_y_overall = max_y_overall.max(bb.max.y);
            }
        }
        if !line_has_glyphs && !line_text.trim().is_empty() {
             min_y_overall = min_y_overall.min(current_baseline_y_for_layout as i32 + v_metrics.descent as i32);
             max_y_overall = max_y_overall.max(current_baseline_y_for_layout as i32 + v_metrics.ascent as i32);
             has_glyphs_with_bb = true; 
        }
        current_baseline_y_for_layout += line_height_pixels;
    }
    
    if !display_lines.is_empty() && has_glyphs_with_bb {
        let last_line_baseline = current_baseline_y_for_layout - line_height_pixels;
        max_y_overall = max_y_overall.max( (last_line_baseline + v_metrics.ascent) as i32 );
        min_y_overall = min_y_overall.min( (last_line_baseline + v_metrics.descent) as i32);
    }

    if !has_glyphs_with_bb { 
        let min_dim = SIGNATURE_IMAGE_PADDING.saturating_mul(2).max(1);
        let height = if !display_lines.is_empty() { (line_height_pixels * display_lines.len() as f32) as u32 } else {min_dim};
        return Some((ImageBuffer::from_pixel(min_dim, height.max(min_dim), BACKGROUND_COLOR), min_dim, height.max(min_dim)));
    }
    
    let text_content_width_actual = (max_x_overall - min_x_overall).max(0) as u32;
    let text_content_height_actual = (max_y_overall - min_y_overall).max(0) as u32;

    let final_width = text_content_width_actual.saturating_add(SIGNATURE_IMAGE_PADDING.saturating_mul(2)).max(1);
    let final_height = text_content_height_actual.saturating_add(SIGNATURE_IMAGE_PADDING.saturating_mul(2)).max(1);
    let mut image = ImageBuffer::from_pixel(final_width, final_height, BACKGROUND_COLOR);

    let x_draw_canvas_offset = SIGNATURE_IMAGE_PADDING as i32 - min_x_overall;
    let y_draw_canvas_offset = SIGNATURE_IMAGE_PADDING as i32 - min_y_overall;

    current_baseline_y_for_layout = 0.0; 
    for line_text in display_lines.iter() {
        let positioned_glyphs = font.layout(line_text, scale, point(0.0, current_baseline_y_for_layout));
        for glyph in positioned_glyphs {
            if let Some(bounding_box) = glyph.pixel_bounding_box() { 
                glyph.draw(|x_glyph_offset, y_glyph_offset, v_coverage| {
                    let target_px_i32 = bounding_box.min.x + x_glyph_offset as i32 + x_draw_canvas_offset;
                    let target_py_i32 = bounding_box.min.y + y_glyph_offset as i32 + y_draw_canvas_offset;
                    
                    if target_px_i32 >= 0 && target_px_i32 < (final_width as i32) && target_py_i32 >= 0 && target_py_i32 < (final_height as i32) {
                        let px = target_px_i32 as u32;
                        let py = target_py_i32 as u32;
                        let current_bg_pixel = image.get_pixel(px, py);
                        let mut output_pixel = *current_bg_pixel;
                        for i in 0..3 { 
                            output_pixel[i] = ((TEXT_COLOR[i] as f32 * v_coverage) + (current_bg_pixel[i] as f32 * (1.0 - v_coverage))).round() as u8;
                        }
                        output_pixel[3] = 255; 
                        image.put_pixel(px, py, output_pixel);
                    }
                });
            }
        }
        current_baseline_y_for_layout += line_height_pixels;
    }
    Some((image, final_width, final_height))
}

// Helper function for pagination
fn finalize_page_and_create_new(
    doc: &mut Document,
    operations: Vec<Operation>,
    parent_pages_id: (u32, u16),
    font_normal_name: &str,
    font_bold_name: &str,
    signature_xobject_id: Option<(u32, u16)>, 
) -> Result<(Vec<Operation>, f32), String> { 
    if !operations.is_empty() { 
        let content = Content { operations };
        let encoded_content = content.encode().map_err(|e| format!("Failed to encode content stream: {}", e))?;
        let content_stream_id = doc.add_object(Stream::new(Dictionary::new(), encoded_content));

        let mut resources_dict_content = dictionary! {
            "Font" => dictionary! {
                font_normal_name => dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" },
                font_bold_name => dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica-Bold" },
            },
        };
        if let Some(sig_id) = signature_xobject_id {
            resources_dict_content.set("XObject", dictionary! { "Im0" => sig_id });
        }

        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => parent_pages_id,
            "Resources" => resources_dict_content, 
            "MediaBox" => vec![0.0.into(), 0.0.into(), PAGE_WIDTH_PT.into(), PAGE_HEIGHT_PT.into()],
            "Contents" => content_stream_id,
        });

        let pages_dict_obj = doc
            .get_object_mut(parent_pages_id)
            .map_err(|e| format!("lopdf::Error getting parent pages object: {} - During page finalization", e))?;
        
        let pages_dict = pages_dict_obj.as_dict_mut()
            .map_err(|e| format!("Parent pages object is not a dictionary (lopdf::Error: {}): During page finalization", e))?;

        let kids = pages_dict
            .get_mut(b"Kids")
            .map_err(|e| format!("lopdf::Error getting Kids field: {} - During page finalization", e))?
            .as_array_mut()
            .map_err(|e| format!("'Kids' field is not an array (lopdf::Error: {}): During page finalization", e))?;

        kids.push(Object::Reference(page_id));
    }
    Ok((Vec::new(), PAGE_HEIGHT_PT - PAGE_MARGIN))
}


fn create_document_with_text_and_signature(
    doc: &mut Document,
    parent_pages_id: (u32, u16),
    content_items: &[ContentItemInternal], 
    signature_image_data: &RgbaImage,
    signature_image_width_px: u32,
    signature_image_height_px: u32,
    signature_image_x_offset_from_text: f32, 
    signature_image_y_adjustment: f32,    
    signature_line_text_trigger: String, 
) -> Result<(), String> {
    let mut current_operations: Vec<Operation> = Vec::new();
    let mut current_y = PAGE_HEIGHT_PT - PAGE_MARGIN;

    let font_normal_name = "F1";
    let font_bold_name = "F2";

    use image::codecs::jpeg::JpegEncoder;
    let mut jpeg_bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg_bytes, 90)
        .encode(signature_image_data.as_raw(), signature_image_width_px, signature_image_height_px, image::ColorType::Rgba8)
        .map_err(|e| format!("Signature JPEG encoding failed: {}", e))?;

    let signature_xobject_id = doc.add_object(Stream::new(
        dictionary! { 
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => signature_image_width_px as i64,
            "Height" => signature_image_height_px as i64,
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
            "Filter" => "DCTDecode",
        },
        jpeg_bytes,
    ));

    for (idx, item) in content_items.iter().enumerate() {
        match item {
            ContentItemInternal::Text(el) => { 
                let item_height_estimate: f32;
                let space_after_item = el.space_after; 
                let is_current_item_signature_trigger = el.text == signature_line_text_trigger;


                if is_current_item_signature_trigger {
                    let sig_intended_height_pt = signature_image_height_px as f32 / IMAGE_SCALE_FACTOR;
                    item_height_estimate = el.size.max(sig_intended_height_pt); 
                } else {
                    item_height_estimate = el.size; 
                }
                
                if current_y - item_height_estimate < PAGE_MARGIN { 
                    if !current_operations.is_empty() { 
                        println!("Page break triggered before item {} starting with: {:?}", idx, el.text.chars().take(30).collect::<String>());
                        let (new_ops, new_y) = finalize_page_and_create_new(
                            doc, current_operations, parent_pages_id, font_normal_name, font_bold_name, Some(signature_xobject_id)
                        )?;
                        current_operations = new_ops;
                        current_y = new_y;
                    }
                    if item_height_estimate > (PAGE_HEIGHT_PT - 2.0 * PAGE_MARGIN) {
                        eprintln!("Warning: Item at index {} ('{}') is too tall for a single page and may be cut.", idx, el.text.chars().take(30).collect::<String>());
                    }
                }
                
                current_y -= el.size; 

                current_operations.push(Operation::new("BT", vec![])); 
                let font_to_use = if el.is_bold { font_bold_name } else { font_normal_name };
                current_operations.push(Operation::new("Tf", vec![font_to_use.into(), el.size.into()]));
                
                let mut x_pos = PAGE_MARGIN + el.indent;
                if el.is_centered {
                    let estimated_text_width = el.text.chars().count() as f32 * el.size * 0.6; 
                    x_pos = (PAGE_WIDTH_PT - estimated_text_width) / 2.0;
                    if x_pos < PAGE_MARGIN { x_pos = PAGE_MARGIN; }
                }
                current_operations.push(Operation::new("Td", vec![x_pos.into(), current_y.into()]));
                current_operations.push(Operation::new("Tj", vec![Object::string_literal(el.text.as_str())]));
                current_operations.push(Operation::new("ET", vec![])); 

                if is_current_item_signature_trigger {
                    let signature_label_text_width_estimate = el.text.chars().count() as f32 * el.size * 0.60; 
                    let signature_image_x_start = x_pos + signature_label_text_width_estimate + signature_image_x_offset_from_text; 

                    let sig_intended_width_pt = signature_image_width_px as f32 / IMAGE_SCALE_FACTOR;
                    let sig_intended_height_pt = signature_image_height_px as f32 / IMAGE_SCALE_FACTOR;
                    
                    let signature_image_y_start = current_y + signature_image_y_adjustment; 

                    if signature_image_x_start + sig_intended_width_pt > PAGE_WIDTH_PT - PAGE_MARGIN {
                        eprintln!("Warning: Signature image (x pos: {:.1}, width: {:.1}) might go off the right margin ({:.1})!", 
                                  signature_image_x_start, sig_intended_width_pt, PAGE_WIDTH_PT - PAGE_MARGIN);
                    }

                    current_operations.push(Operation::new("q", vec![])); 
                    current_operations.push(Operation::new("cm", vec![
                        sig_intended_width_pt.into(), 0.into(), 0.into(), sig_intended_height_pt.into(), 
                        signature_image_x_start.into(), signature_image_y_start.into() 
                    ]));
                    current_operations.push(Operation::new("Do", vec!["Im0".into()])); 
                    current_operations.push(Operation::new("Q", vec![])); 
                }
                
                if current_y - space_after_item < PAGE_MARGIN && space_after_item > 0.0 { 
                    println!("Page break triggered by space_after item {}", idx);
                     let (new_ops, new_y) = finalize_page_and_create_new(
                        doc, current_operations, parent_pages_id, font_normal_name, font_bold_name, Some(signature_xobject_id)
                    )?;
                    current_operations = new_ops;
                    current_y = new_y;
                } else {
                    current_y -= space_after_item;
                }
            }
        }
    }

    if !current_operations.is_empty() { 
        let (_new_ops, _new_y) = finalize_page_and_create_new( 
            doc,
            current_operations,
            parent_pages_id,
            font_normal_name,
            font_bold_name,
            Some(signature_xobject_id),
        )?;
    }
    
    // THIS IS THE CORRECTED SECTION FOR THE COMPILER ERROR E0599
    let pages_dict_obj_for_count = doc
        .get_object_mut(parent_pages_id)
        .map_err(|e| format!("lopdf::Error getting parent pages object for final count: {}", e))?;
    
    let pages_dict_for_count = pages_dict_obj_for_count.as_dict_mut()
         .map_err(|e| format!("Parent pages object is not a dictionary for final count (lopdf::Error: {}): Error setting final count", e))?;

    // Assuming pages_dict_for_count.get(b"Kids") is giving a Result based on the E0599 error
    let kids_obj = pages_dict_for_count 
        .get(b"Kids") 
        .map_err(|e_lopdf: lopdf::Error| { // Type annotation for e_lopdf can help compiler
            format!("Error accessing 'Kids' (lopdf::Error {}). Key might be missing or other dictionary error.", e_lopdf)
        })?;

    let kids = kids_obj
        .as_array() 
        .map_err(|e| format!("'Kids' field is not an array (lopdf::Error: {}): Error setting final count", e))?;
    
    pages_dict_for_count.set("Count", kids.len() as i64);

    Ok(())
}

// ContentItemInternal does not have a text_preview method unless you define it.
// Removed for now to avoid potential new errors. If you need it, define it:
// impl ContentItemInternal {
//     fn text_preview(&self) -> String {
//         match self {
//             ContentItemInternal::Text(el) => el.text.chars().take(40).collect::<String>() + "...",
//         }
//     }
// }