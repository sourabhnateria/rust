use chrono::Local;
use jpeg_decoder::{Decoder as JpegDecoder, PixelFormat};
use lopdf::{
    Dictionary,
    Document,
    // Error, // Import Error for map_err
    Object,
    Stream,
    content::{Content, Operation},
};
use serde::Deserialize;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct InvoiceConfig {
    to: To,
    invoice: Invoice,
    items: Items,
    SubTotal: Subtotal,
    PaymentMethod: Payment,
    TermsandConditions: Terms,
}

#[derive(Debug, Deserialize)]
struct To {
    Name: String,
    address: String,
}

#[derive(Debug, Deserialize)]
struct Items {
    headers: Vec<String>,
    rows: Vec<ItemRow>,
}

#[derive(Debug, Deserialize)]
struct ItemRow {
    no: String,
    description: String,
    qty: String,
    price: String,
    total: String,
}

#[derive(Debug, Deserialize)]
struct Subtotal {
    subtotal_value: String,
    vat_percentage: f64,
    vat_value: String,
    discount_value: String,
    grand_total_value: String,
}

#[derive(Debug, Deserialize)]
struct Payment {
    bank_name: String,
    account_number: String,
}

#[derive(Debug, Deserialize)]
struct Terms {
    content: String,
}

#[derive(Debug, Deserialize)]
struct Invoice {
    invoice_number: String,
    date: String,
    currency: String,
}

// --- Helper function to load image data and create XObject ---
fn create_image_xobject(
    doc: &mut Document,
    image_path_str: &str,
) -> Result<Object, Box<dyn std::error::Error>> {
    let image_path = Path::new(image_path_str);
    let image_data = {
        let mut file = File::open(image_path)
            .map_err(|e| format!("Failed to open {}: {}", image_path.display(), e))?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        if data.len() < 2 || data[0] != 0xFF || data[1] != 0xD8 {
            return Err(
                format!("{} is not a valid JPEG (missing SOI)", image_path.display()).into(),
            );
        }
        if data.len() > 10_000_000 {
            return Err(format!("Image {} too large (>10MB)", image_path.display()).into());
        }
        data
    };

    let mut image_xobject_dict = Dictionary::new();
    image_xobject_dict.set(b"Type", Object::Name(b"XObject".to_vec()));
    image_xobject_dict.set(b"Subtype", Object::Name(b"Image".to_vec()));

    let mut decoder = JpegDecoder::new(&image_data[..]);
    decoder.read_info().map_err(|e| {
        format!(
            "Failed to read JPEG info from {}: {}",
            image_path.display(),
            e
        )
    })?;
    let info = decoder
        .info()
        .ok_or_else(|| format!("No JPEG info found in {}", image_path.display()))?;

    image_xobject_dict.set(b"Width", Object::Integer(info.width as i64));
    image_xobject_dict.set(b"Height", Object::Integer(info.height as i64));
    match info.pixel_format {
        PixelFormat::L8 => {
            image_xobject_dict.set(b"ColorSpace", Object::Name(b"DeviceGray".to_vec()));
            image_xobject_dict.set(b"BitsPerComponent", Object::Integer(8));
        }
        PixelFormat::RGB24 => {
            image_xobject_dict.set(b"ColorSpace", Object::Name(b"DeviceRGB".to_vec()));
            image_xobject_dict.set(b"BitsPerComponent", Object::Integer(8));
        }
        PixelFormat::CMYK32 => {
            image_xobject_dict.set(b"ColorSpace", Object::Name(b"DeviceCMYK".to_vec()));
            image_xobject_dict.set(b"BitsPerComponent", Object::Integer(8));
            image_xobject_dict.set(
                b"Decode",
                Object::Array(vec![
                    Object::Real(1.0),
                    Object::Real(0.0),
                    Object::Real(1.0),
                    Object::Real(0.0),
                    Object::Real(1.0),
                    Object::Real(0.0),
                    Object::Real(1.0),
                    Object::Real(0.0),
                ]),
            );
        }
        unsupported_format => {
            return Err(format!(
                "Unsupported JPEG pixel format in {}: {:?}",
                image_path.display(),
                unsupported_format
            )
            .into());
        }
    }
    image_xobject_dict.set(b"Filter", Object::Name(b"DCTDecode".to_vec()));
    let image_xobject_stream = Stream::new(image_xobject_dict, image_data);
    Ok(Object::Reference(
        doc.add_object(Object::Stream(image_xobject_stream)),
    ))
}

fn encode_unicode_text(text: &str) -> Object {
    // Convert to UTF-16BE with BOM for all text to ensure consistency
    let mut bytes = vec![0xFE, 0xFF]; // UTF-16BE BOM
    for c in text.encode_utf16() {
        bytes.extend(&c.to_be_bytes());
    }
    Object::String(bytes, lopdf::StringFormat::Hexadecimal)
}

fn format_currency(currency: &str, amount: f64) -> String {
    format!("{}{:.2}", currency, amount)
}

fn parse_currency_value(value: &str) -> f64 {
    value
        .chars()
        .filter(|c| c.is_numeric() || *c == '.')
        .collect::<String>()
        .parse::<f64>()
        .unwrap_or(0.0)
}

fn create_to_unicode_cmap() -> Object {
    let cmap = r#"
        /CIDInit /ProcSet findresource begin
        12 dict begin
        begincmap
        /CIDSystemInfo <<
          /Registry (Adobe)
          /Ordering (UCS)
          /Supplement 0
        >> def
        /CMapName /Adobe-Identity-UCS def
        1 begincodespacerange
        <0000> <FFFF>
        endcodespacerange
        1 beginbfrange
        <0000> <FFFF> <0000>
        endbfrange
        endcmap
        CMapName currentdict /CMap defineresource pop
        end
        end
    "#
    .as_bytes()
    .to_vec();

    let mut dict = Dictionary::new();
    dict.set(b"Length", Object::Integer(cmap.len() as i64));
    Object::Stream(Stream::new(dict, cmap))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Read config file with explicit UTF-8 encoding
    let config_content = fs::read_to_string("config.toml")
        .map_err(|e| format!("Failed to read config file: {}", e))?;
    let mut config: InvoiceConfig =
        toml::from_str(&config_content).map_err(|e| format!("Failed to parse config: {}", e))?;

    // Calculate current date in desired format
    let current_date = Local::now().format("%d %B %Y").to_string();
    config.invoice.date = current_date;

    // --- MODIFICATION: Calculate item totals (price * qty) and update in config ---
    for item_row in config.items.rows.iter_mut() {
        let price_val = parse_currency_value(&item_row.price);
        let qty_val = item_row.qty.parse::<f64>().unwrap_or_else(|e| {
            eprintln!(
                "Warning: Could not parse QTY '{}' for item '{}' as a number: {}. Assuming QTY=1.",
                item_row.qty, item_row.description, e
            );
            1.0 // Default to 1 if QTY parsing fails
        });

        let calculated_total_val = price_val * qty_val;
        item_row.total = format_currency(&config.invoice.currency, calculated_total_val);
    }
    // --- END MODIFICATION ---

    // Calculate subtotal from items (remove any existing currency symbols)
    let subtotal: f64 = config
        .items
        .rows
        .iter()
        .map(|item| parse_currency_value(&item.total))
        .sum();

    // Calculate VAT (10% of subtotal)
    if config.SubTotal.vat_percentage < 0.0 {
        eprintln!(
            "Warning: VAT percentage ({}) is negative. Treating as 0%.",
            config.SubTotal.vat_percentage
        );
        config.SubTotal.vat_percentage = 0.0;
    }
    let vat = subtotal * (config.SubTotal.vat_percentage / 100.0);

    // Get discount from config (remove $ and commas)
    let discount = config
        .SubTotal
        .discount_value
        .replace('$', "")
        .replace(',', "")
        .parse::<f64>()
        .unwrap_or(0.0);

    // Calculate grand total
    let grand_total = subtotal + vat - discount;

    // Format the calculated values back to strings with $ and commas
    config.SubTotal.subtotal_value = format_currency(&config.invoice.currency, subtotal);
    config.SubTotal.vat_value = format_currency(&config.invoice.currency, vat);
    config.SubTotal.grand_total_value = format_currency(&config.invoice.currency, grand_total);

    let mut doc = Document::new();

    let mut pages_dict = Dictionary::new();
    pages_dict.set(b"Type", Object::Name(b"Pages".to_vec()));
    pages_dict.set(b"Kids", Object::Array(vec![]));
    pages_dict.set(b"Count", Object::Integer(0));
    let pages_id_tuple = doc.add_object(Object::Dictionary(pages_dict));

    let content_id = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), Vec::new())));

    let mut font_regular_dict = Dictionary::new();
    font_regular_dict.set(b"Type", Object::Name(b"Font".to_vec()));
    font_regular_dict.set(b"Subtype", Object::Name(b"TrueType".to_vec()));
    font_regular_dict.set(b"Name", Object::Name(b"F1".to_vec()));
    font_regular_dict.set(b"BaseFont", Object::Name(b"Helvetica".to_vec()));
    font_regular_dict.set(b"Encoding", Object::Name(b"Identity-H".to_vec()));
    font_regular_dict.set(
        b"ToUnicode",
        Object::Reference(doc.add_object(create_to_unicode_cmap())),
    );
    let font_regular_id_tuple = doc.add_object(Object::Dictionary(font_regular_dict));
    let font_regular_id = Object::Reference(font_regular_id_tuple);

    let mut font_bold_dict = Dictionary::new();
    font_bold_dict.set(b"Type", Object::Name(b"Font".to_vec()));
    font_bold_dict.set(b"Subtype", Object::Name(b"TrueType".to_vec()));
    font_bold_dict.set(b"Name", Object::Name(b"F3Bold".to_vec()));
    font_bold_dict.set(b"BaseFont", Object::Name(b"Helvetica-Bold".to_vec()));
    font_bold_dict.set(b"Encoding", Object::Name(b"Identity-H".to_vec()));
    font_bold_dict.set(
        b"ToUnicode",
        Object::Reference(doc.add_object(create_to_unicode_cmap())),
    );

    let font_bold_id_tuple = doc.add_object(Object::Dictionary(font_bold_dict));
    let font_bold_id = Object::Reference(font_bold_id_tuple);

    let image1_id = create_image_xobject(&mut doc, "example.jpg")?;
    let new_image_paths = [
        "new_image1.jpg",
        "new_image2.jpg",
        "new_image3.jpg",
        "new_image4.jpg",
    ];
    let mut new_image_ids = Vec::new();
    for (i, path_str) in new_image_paths.iter().enumerate() {
        match create_image_xobject(&mut doc, path_str) {
            Ok(id) => new_image_ids.push(id),
            Err(e) => return Err(format!("Error processing new_image{}: {}", i + 1, e).into()),
        }
    }

    let mut page_dict = Dictionary::new();
    page_dict.set(b"Type", Object::Name(b"Page".to_vec()));
    page_dict.set(b"Parent", Object::Reference(pages_id_tuple));
    page_dict.set(
        b"MediaBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(595),
            Object::Integer(842),
        ]),
    );
    page_dict.set(b"Contents", Object::Reference(content_id));

    let mut resources_dict = Dictionary::new();
    let mut xobject_res_dict = Dictionary::new();
    xobject_res_dict.set(b"Im1", image1_id.clone());
    for (i, img_id) in new_image_ids.iter().enumerate() {
        let name = format!("ImNew{}", i + 1);
        xobject_res_dict.set(name.into_bytes(), img_id.clone());
    }
    resources_dict.set(b"XObject", Object::Dictionary(xobject_res_dict));

    let mut font_res_dict = Dictionary::new();
    font_res_dict.set(b"F1", font_regular_id.clone());
    font_res_dict.set(b"F3Bold", font_bold_id.clone());
    resources_dict.set(b"Font", Object::Dictionary(font_res_dict));

    page_dict.set(b"Resources", Object::Dictionary(resources_dict));
    let page_id_tuple = doc.add_object(Object::Dictionary(page_dict));
    let page_id = Object::Reference(page_id_tuple);

    // 5. Update page tree
    let pages_obj = doc.get_object_mut(pages_id_tuple)?;
    let pages_dict = pages_obj.as_dict_mut().map_err(|e| {
        drop(lopdf::Error::Type);
        format!(
            "Page tree root object is not a dictionary (expected Dict type). Original error: {}",
            e
        )
    })?;

    // Get or create Kids array
    let kids_array = match pages_dict.get(b"Kids") {
        Ok(obj) => obj.as_array().map(|a| a.clone()).unwrap_or_else(|_| vec![]),
        Err(_) => vec![],
    };
    let mut new_kids = kids_array;
    new_kids.push(page_id.clone());
    pages_dict.set(b"Kids", Object::Array(new_kids));

    // Update Count
    let new_count = pages_dict
        .get(b"Kids")
        .and_then(|kids| kids.as_array())
        .map(|kids| kids.len() as i64)
        .unwrap_or(0);

    match pages_dict.get_mut(b"Count") {
        Ok(obj) => {
            if let Object::Integer(ref mut val) = *obj {
                *val = new_count;
            } else {
                *obj = Object::Integer(new_count);
                eprintln!(
                    "Warning: Page tree 'Count' was not an Integer. Setting to current Kids length {}",
                    new_count
                );
            }
        }
        Err(_) => {
            pages_dict.set(b"Count", Object::Integer(new_count));
        }
    }

    let mut operations = Vec::new();
    let img_width_on_page = 400.0;
    let img_height_on_page = 150.0;
    let img_x_pos = 100.0;
    let img_y_pos = 690.0;
    operations.push(Operation::new("q", vec![]));
    operations.push(Operation::new(
        "cm",
        vec![
            Object::Real(img_width_on_page),
            Object::Real(0.0),
            Object::Real(0.0),
            Object::Real(img_height_on_page),
            Object::Real(img_x_pos),
            Object::Real(img_y_pos),
        ],
    ));
    operations.push(Operation::new("Do", vec![Object::Name(b"Im1".to_vec())]));
    operations.push(Operation::new("Q", vec![]));

    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F3Bold".to_vec()), Object::Real(26.0)],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(50.0), Object::Real(660.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal("INVOICE")],
    ));
    operations.push(Operation::new("ET", vec![]));

    // To section
    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), Object::Real(12.0)],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(50.0), Object::Real(620.0)],
    ));
    operations.push(Operation::new("Tj", vec![Object::string_literal("To")]));
    operations.push(Operation::new("ET", vec![]));

    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F3Bold".to_vec()), Object::Real(14.0)],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(50.0), Object::Real(605.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal(&*config.to.Name)],
    ));
    operations.push(Operation::new("ET", vec![]));

    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), Object::Real(12.0)],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(50.0), Object::Real(590.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal(&*config.to.address)],
    ));
    operations.push(Operation::new("ET", vec![]));

    // Items table headers
    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F3Bold".to_vec()), Object::Real(14.0)],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(50.0), Object::Real(550.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal(&*config.items.headers[0])],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(50.0), Object::Real(0.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal(&*config.items.headers[1])],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(200.0), Object::Real(0.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal(&*config.items.headers[2])],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(120.0), Object::Real(0.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal(&*config.items.headers[3])],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(70.0), Object::Real(0.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal(&*config.items.headers[4])],
    ));
    operations.push(Operation::new("ET", vec![]));

    // Items table rows
    let mut y_pos_table = 530.0;
    for item in &*config.items.rows {
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), Object::Real(12.0)],
        ));
        operations.push(Operation::new(
            "Td",
            vec![Object::Real(50.0), Object::Real(y_pos_table)],
        ));
        operations.push(Operation::new(
            "Tj",
            vec![Object::string_literal(&*item.no)],
        ));
        operations.push(Operation::new(
            "Td",
            vec![Object::Real(50.0), Object::Real(0.0)],
        ));
        operations.push(Operation::new(
            "Tj",
            vec![Object::string_literal(&*item.description)],
        ));
        operations.push(Operation::new(
            "Td",
            vec![Object::Real(200.0), Object::Real(0.0)],
        ));
        operations.push(Operation::new(
            "Tj",
            vec![Object::string_literal(&*item.qty)],
        ));
        operations.push(Operation::new(
            "Td",
            vec![Object::Real(120.0), Object::Real(0.0)],
        ));
        // For item price:
        let formatted_price =
            format_currency(&config.invoice.currency, parse_currency_value(&*item.price));
        operations.push(Operation::new(
            "Tj",
            vec![Object::string_literal(&*formatted_price)],
        ));

        operations.push(Operation::new(
            "Td",
            vec![Object::Real(70.0), Object::Real(0.0)],
        ));
        let formatted_total =
            format_currency(&config.invoice.currency, parse_currency_value(&item.total));
        operations.push(Operation::new(
            "Tj",
            vec![Object::string_literal(&*formatted_total)],
        ));
        operations.push(Operation::new("ET", vec![]));
        y_pos_table -= 25.0;
    }

    // Payment Method
    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F3Bold".to_vec()), Object::Real(14.0)],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(50.0), Object::Real(390.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal("Payment Method")],
    ));
    operations.push(Operation::new("ET", vec![]));

    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), Object::Real(12.0)],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(50.0), Object::Real(370.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal(&*config.PaymentMethod.bank_name)],
    ));
    operations.push(Operation::new("ET", vec![]));

    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), Object::Real(12.0)],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(50.0), Object::Real(355.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal(
            &*config.PaymentMethod.account_number,
        )],
    ));
    operations.push(Operation::new("ET", vec![]));

    // Terms and Conditions
    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F3Bold".to_vec()), Object::Real(14.0)],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(50.0), Object::Real(290.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal("Term and Conditions :")],
    ));
    operations.push(Operation::new("ET", vec![]));

    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), Object::Real(12.0)],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(50.0), Object::Real(270.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal(&*config.TermsandConditions.content)],
    ));
    operations.push(Operation::new("ET", vec![]));

    // Invoice number and date
    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F3Bold".to_vec()), Object::Real(12.0)],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(350.0), Object::Real(605.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal("Invoice no :")],
    ));
    operations.push(Operation::new("ET", vec![]));

    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), Object::Real(12.0)],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(350.0), Object::Real(590.0)],
    ));
    operations.push(Operation::new("Tj", vec![Object::string_literal("Date :")]));
    operations.push(Operation::new("ET", vec![]));

    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F3Bold".to_vec()), Object::Real(10.0)],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(450.0), Object::Real(605.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal(&*config.invoice.invoice_number)],
    ));
    operations.push(Operation::new("ET", vec![]));

    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), Object::Real(10.0)],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(450.0), Object::Real(590.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal(&*config.invoice.date)],
    ));
    operations.push(Operation::new("ET", vec![]));

    // Subtotal section
    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F3Bold".to_vec()), Object::Real(12.0)],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(390.0), Object::Real(400.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal("Sub Total")],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(100.0), Object::Real(0.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal(&*config.SubTotal.subtotal_value)],
    ));
    operations.push(Operation::new("ET", vec![]));

    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F3Bold".to_vec()), Object::Real(12.0)],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(390.0), Object::Real(380.0)],
    ));
    operations.push(Operation::new("Tj", vec![Object::string_literal("VAT")]));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(100.0), Object::Real(0.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal(&*config.SubTotal.vat_value)],
    ));
    operations.push(Operation::new("ET", vec![]));

    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F3Bold".to_vec()), Object::Real(12.0)],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(390.0), Object::Real(360.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal("Discount")],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(100.0), Object::Real(0.0)],
    ));
    // For item total:
    let formatted_discount = format_currency(
        &config.invoice.currency,
        parse_currency_value(&*config.SubTotal.discount_value),
    );
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal(&*formatted_discount)],
    ));
    operations.push(Operation::new("ET", vec![]));

    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F3Bold".to_vec()), Object::Real(14.0)],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(360.0), Object::Real(330.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal("GRAND TOTAL")],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(120.0), Object::Real(0.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal(&*config.SubTotal.grand_total_value)],
    ));
    operations.push(Operation::new("ET", vec![]));

    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F3Bold".to_vec()), Object::Real(12.0)],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(150.0), Object::Real(80.0)],
    ));
    operations.push(Operation::new("Tj", vec![Object::string_literal("Mail:")]));
    operations.push(Operation::new("ET", vec![]));
    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), Object::Real(12.0)],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(150.0), Object::Real(70.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal("hello@reallygreatsite.com")],
    ));
    operations.push(Operation::new("ET", vec![]));
    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F3Bold".to_vec()), Object::Real(12.0)],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(350.0), Object::Real(80.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal("Address:")],
    ));
    operations.push(Operation::new("ET", vec![]));
    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), Object::Real(12.0)],
    ));
    operations.push(Operation::new(
        "Td",
        vec![Object::Real(350.0), Object::Real(70.0)],
    ));
    operations.push(Operation::new(
        "Tj",
        vec![Object::string_literal("123 Anywhere St, Any City")],
    ));
    operations.push(Operation::new("ET", vec![]));

    let img_new1_width = 50.0;
    let img_new1_height = 30.0;
    let bottom_margin = 615.0;
    let spacing = 10.0;
    let img_new1_x = 0.0;
    let img_new1_y = bottom_margin + img_new1_height + spacing;
    operations.push(Operation::new("q", vec![]));
    operations.push(Operation::new(
        "cm",
        vec![
            Object::Real(img_new1_width),
            Object::Real(0.0),
            Object::Real(0.0),
            Object::Real(img_new1_height),
            Object::Real(img_new1_x),
            Object::Real(img_new1_y),
        ],
    ));
    operations.push(Operation::new("Do", vec![Object::Name(b"ImNew1".to_vec())]));
    operations.push(Operation::new("Q", vec![]));
    let img_new2_width = 80.0;
    let img_new2_height = 60.0;
    let img_new2_x = 0.0;
    let img_new2_y = 0.0;
    operations.push(Operation::new("q", vec![]));
    operations.push(Operation::new(
        "cm",
        vec![
            Object::Real(img_new2_width),
            Object::Real(0.0),
            Object::Real(0.0),
            Object::Real(img_new2_height),
            Object::Real(img_new2_x),
            Object::Real(img_new2_y),
        ],
    ));
    operations.push(Operation::new("Do", vec![Object::Name(b"ImNew2".to_vec())]));
    operations.push(Operation::new("Q", vec![]));
    let img_new3_width = 25.0;
    let img_new3_height = 20.0;
    let img_new3_x = 120.0;
    let img_new3_y = 70.0;
    operations.push(Operation::new("q", vec![]));
    operations.push(Operation::new(
        "cm",
        vec![
            Object::Real(img_new3_width),
            Object::Real(0.0),
            Object::Real(0.0),
            Object::Real(img_new3_height),
            Object::Real(img_new3_x),
            Object::Real(img_new3_y),
        ],
    ));
    operations.push(Operation::new("Do", vec![Object::Name(b"ImNew3".to_vec())]));
    operations.push(Operation::new("Q", vec![]));
    let img_new4_width = 20.0;
    let img_new4_height = 20.0;
    let img_new4_x = 330.0;
    let img_new4_y = 70.0;
    operations.push(Operation::new("q", vec![]));
    operations.push(Operation::new(
        "cm",
        vec![
            Object::Real(img_new4_width),
            Object::Real(0.0),
            Object::Real(0.0),
            Object::Real(img_new4_height),
            Object::Real(img_new4_x),
            Object::Real(img_new4_y),
        ],
    ));
    operations.push(Operation::new("Do", vec![Object::Name(b"ImNew4".to_vec())]));
    operations.push(Operation::new("Q", vec![]));

    operations.push(Operation::new("q", vec![]));
    operations.push(Operation::new("1 w", vec![]));
    operations.push(Operation::new(
        "m",
        vec![Object::Real(50.0), Object::Real(565.0)],
    ));
    operations.push(Operation::new(
        "l",
        vec![Object::Real(545.0), Object::Real(565.0)],
    ));
    operations.push(Operation::new("S", vec![]));
    operations.push(Operation::new("Q", vec![]));
    operations.push(Operation::new("q", vec![]));
    operations.push(Operation::new("1 w", vec![]));
    operations.push(Operation::new(
        "m",
        vec![Object::Real(50.0), Object::Real(545.0)],
    ));
    operations.push(Operation::new(
        "l",
        vec![Object::Real(545.0), Object::Real(545.0)],
    ));
    operations.push(Operation::new("S", vec![]));
    operations.push(Operation::new("Q", vec![]));
    operations.push(Operation::new("q", vec![]));
    operations.push(Operation::new("0.5 w", vec![]));
    operations.push(Operation::new(
        "m",
        vec![Object::Real(50.0), Object::Real(520.0)],
    ));
    operations.push(Operation::new(
        "l",
        vec![Object::Real(545.0), Object::Real(520.0)],
    ));
    operations.push(Operation::new("S", vec![]));
    operations.push(Operation::new("Q", vec![]));
    operations.push(Operation::new("q", vec![]));
    operations.push(Operation::new("0.5 w", vec![]));
    operations.push(Operation::new(
        "m",
        vec![Object::Real(50.0), Object::Real(495.0)],
    ));
    operations.push(Operation::new(
        "l",
        vec![Object::Real(545.0), Object::Real(495.0)],
    ));
    operations.push(Operation::new("S", vec![]));
    operations.push(Operation::new("Q", vec![]));
    operations.push(Operation::new("q", vec![]));
    operations.push(Operation::new("0.5 w", vec![]));
    operations.push(Operation::new(
        "m",
        vec![Object::Real(50.0), Object::Real(470.0)],
    ));
    operations.push(Operation::new(
        "l",
        vec![Object::Real(545.0), Object::Real(470.0)],
    ));
    operations.push(Operation::new("S", vec![]));
    operations.push(Operation::new("Q", vec![]));
    operations.push(Operation::new("q", vec![]));
    operations.push(Operation::new("0.5 w", vec![]));
    operations.push(Operation::new(
        "m",
        vec![Object::Real(50.0), Object::Real(445.0)],
    ));
    operations.push(Operation::new(
        "l",
        vec![Object::Real(545.0), Object::Real(445.0)],
    ));
    operations.push(Operation::new("S", vec![]));
    operations.push(Operation::new("Q", vec![]));
    operations.push(Operation::new("q", vec![]));
    operations.push(Operation::new("1 w", vec![]));
    operations.push(Operation::new(
        "m",
        vec![Object::Real(50.0), Object::Real(420.0)],
    ));
    operations.push(Operation::new(
        "l",
        vec![Object::Real(545.0), Object::Real(420.0)],
    ));
    operations.push(Operation::new("S", vec![]));
    operations.push(Operation::new("Q", vec![]));
    operations.push(Operation::new("q", vec![]));
    operations.push(Operation::new("1 w", vec![]));
    operations.push(Operation::new(
        "m",
        vec![Object::Real(50.0), Object::Real(100.0)],
    ));
    operations.push(Operation::new(
        "l",
        vec![Object::Real(545.0), Object::Real(100.0)],
    ));
    operations.push(Operation::new("S", vec![]));
    operations.push(Operation::new("Q", vec![]));

    let content = Content { operations };
    let content_bytes = content.encode()?;
    let content_stream_obj_ref = doc.get_object_mut(content_id)?;
    *content_stream_obj_ref = Object::Stream(Stream::new(Dictionary::new(), content_bytes));

    let mut catalog_dict = Dictionary::new();
    catalog_dict.set(b"Type", Object::Name(b"Catalog".to_vec()));
    catalog_dict.set(b"Pages", Object::Reference(pages_id_tuple));
    let catalog_id_tuple = doc.add_object(Object::Dictionary(catalog_dict));
    doc.trailer
        .set(b"Root", Object::Reference(catalog_id_tuple));

    doc.compress();
    doc.save("output_with_5_images_and_bold_invoice.pdf")?;
    println!("PDF created successfully: output_with_5_images_and_bold_invoice.pdf");
    Ok(())
}
