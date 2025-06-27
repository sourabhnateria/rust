use anyhow::{Context, Result};
use docx_rs::*;
use pdf_extract::extract_text;
use regex::Regex;
use std::fs::File;
use std::path::Path;
use unicode_normalization::UnicodeNormalization;

fn clean_and_structure_text(text: &str) -> Vec<String> {
    // Normalize Unicode characters (e.g., convert ﬁ to fi)
    let normalized = text.nfc().collect::<String>();

    // Remove PDF control characters and artifacts
    let re_control = Regex::new(r"[\x00-\x1F\x7F]").unwrap();
    let cleaned = re_control.replace_all(&normalized, " ");

    // Normalize whitespace (convert all whitespace to single spaces)
    let re_whitespace = Regex::new(r"\s+").unwrap();
    let cleaned = re_whitespace.replace_all(&cleaned, " ");

    // Split into paragraphs at double line breaks
    let re_paragraphs = Regex::new(r"(?:\n\s*){2,}").unwrap();
    re_paragraphs
        .split(&cleaned)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn create_word_document(paragraphs: Vec<String>) -> Docx {
    let mut docx = Docx::new();

    for (i, para) in paragraphs.iter().enumerate() {
        // Add page break after every 30 paragraphs (simulate PDF pages)
        if i > 0 && i % 30 == 0 {
            docx =
                docx.add_paragraph(Paragraph::new().add_run(Run::new().add_break(BreakType::Page)));
        }

        docx = docx.add_paragraph(Paragraph::new().add_run(Run::new().add_text(para)));
    }

    docx
}

fn pdf_to_structured_word(input_path: &str, output_path: &str) -> Result<()> {
    // Extract text from PDF
    let raw_text = extract_text(input_path)
        .with_context(|| format!("Failed to extract text from PDF: {}", input_path))?;

    // Process the text
    let paragraphs = clean_and_structure_text(&raw_text);

    // Create Word document
    let docx = create_word_document(paragraphs);

    // Save to file
    let file = File::create(Path::new(output_path))
        .with_context(|| format!("Failed to create output file: {}", output_path))?;
    docx.build()
        .pack(file)
        .with_context(|| format!("Failed to write Word document: {}", output_path))?;

    Ok(())
}

fn main() -> Result<()> {
    let input_pdf = "input.pdf";
    let output_docx = "output.docx";

    pdf_to_structured_word(input_pdf, output_docx)?;

    println!("Successfully converted {} to {}", input_pdf, output_docx);
    Ok(())
}
