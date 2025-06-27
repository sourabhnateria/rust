use flate2::Compression;
use flate2::write::ZlibEncoder;
use image::{ImageBuffer, Rgba};
use imageproc::drawing::draw_text_mut;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};
use rusttype::{Font, Scale};
use std::error::Error;
use std::fs;
use std::io::Write;

// Struct (if you have one, e.g., ImagePlacement) would go here
// struct ImagePlacement { ... }

fn main() -> Result<(), Box<dyn Error>> {
    let input_pdf_path = "D:\\rust\\dsign\\input.pdf"; // Adjust as needed
    let font_path = "fonts/AnandaBlackPersonalUseRegular-rg9Rx.ttf"; // Adjust as needed
    let output_pdf_path = "filled_contract_rasterized.pdf";

    // Create dummy input.pdf if it doesn't exist for minimal testing
    if !std::path::Path::new(input_pdf_path).exists() {
        println!("Creating dummy PDF: {}", input_pdf_path);
        let mut doc = Document::with_version("1.5");

        // 1. Add dependent objects for Page 2's dummy content (font, content stream)
        let f1_id = doc.add_object(
            dictionary! {"Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica"},
        );
        let p2_content_id = doc.add_object(Stream::new(
            Dictionary::new(),
            b"BT /F1 12 Tf 100 700 Td (Page 2 Existing Dummy Content) Tj ET".to_vec(),
        ));
        let p2_resources =
            dictionary! { "Font" => dictionary! { "F1" => Object::Reference(f1_id) } };

        // 2. Create Page Dictionaries (Parent will be set after Pages ID is known)
        let page1_obj = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            // "Parent" will be set later
        };
        let page1_id = doc.add_object(page1_obj);

        let page2_obj = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => p2_resources.clone(),
            "Contents" => Object::Reference(p2_content_id),
            // "Parent" will be set later
        };
        let page2_id = doc.add_object(page2_obj);

        // 3. Create Pages Dictionary
        let pages_kids = vec![Object::Reference(page1_id), Object::Reference(page2_id)];
        let pages_obj = dictionary! {
            "Type" => "Pages",
            "Kids" => pages_kids,
            "Count" => 2_i64, // Use i64 for numbers
        };
        let pages_id = doc.add_object(pages_obj);

        // 4. Update Parent entry in Page objects now that Pages ID is known
        doc.get_object_mut(page1_id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("Parent", Object::Reference(pages_id));
        doc.get_object_mut(page2_id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("Parent", Object::Reference(pages_id));

        // 5. Create Catalog Dictionary
        let catalog_obj = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(catalog_obj);

        // 6. Set Root in Trailer
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc.save(input_pdf_path)?;
        println!("Created dummy {} for testing.", input_pdf_path);
    }

    // Create dummy font directory if it doesn't exist (font file itself needs to be present)
    let font_dir = std::path::Path::new(font_path)
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    if !font_dir.exists() {
        fs::create_dir_all(font_dir)?;
    }
    if !std::path::Path::new(font_path).exists() {
        println!(
            "⚠️ Warning: Font path {} does not exist. `create_text_image` will fail.",
            font_path
        );
        println!(
            "Please place a valid .ttf font at that location for the text generation to work."
        );
        // You might want to return an error here or skip the text part if font is critical.
        // return Err(format!("Font file not found: {}", font_path).into());
    }

    println!("Loading PDF: {}", input_pdf_path);
    let mut doc = Document::load(input_pdf_path)?;
    let pages = doc.get_pages();
    println!("PDF loaded. Found {} page(s).", pages.len());

    let page_number_to_modify = 2; // Example: modify the second page
    if let Some(&target_page_id) = pages.get(&page_number_to_modify) {
        println!(
            "Found page {} (ID: {:?}). Adding signature for Pramod Rai.",
            page_number_to_modify, target_page_id
        );

        let signature_text = "Pramod";
        let signature_xobject_name = "PramodSign"; // PDF XObject name
        let signature_x_pdf = 110.0; // X coordinate in PDF page units
        let signature_y_pdf = 150.0; // Y coordinate in PDF page units
        let signature_scale_in_pdf = 1.0; // Scale of the rasterized image when placed in PDF
        let use_transparency = true;

        match add_text_image_to_pdf(
            &mut doc,
            target_page_id,
            signature_text,
            font_path,
            signature_xobject_name,
            signature_x_pdf,
            signature_y_pdf,
            signature_scale_in_pdf,
            use_transparency,
        ) {
            Ok(_) => println!(
                "Signature for {} processed for page {}.",
                signature_text, page_number_to_modify
            ),
            Err(e) => {
                // If font was missing, this error will show up here.
                eprintln!("Error adding text image for {}: {}", signature_text, e);
                // return Err(e); // Optionally propagate the error
            }
        }
    } else {
        eprintln!("⚠️ Page {} not found in PDF!", page_number_to_modify);
        return Err(format!("Page {} not found in PDF!", page_number_to_modify).into());
    }

    println!("Saving PDF to: {}", output_pdf_path);
    doc.save(output_pdf_path)?;
    println!("✅ PDF saved successfully!");
    Ok(())
}

// Using the improved create_text_image that calculates bounds more robustly
fn create_text_image(text: &str, path: &str, font_path: &str) -> Result<(), Box<dyn Error>> {
    println!(
        "Creating text image for '{}' using font '{}', saving to '{}'",
        text, font_path, path
    );
    let font_data = fs::read(font_path)
        .map_err(|e| format!("Failed to read font file '{}': {}", font_path, e))?;
    let font = Font::try_from_vec(font_data).ok_or_else(|| {
        Box::<dyn Error>::from(format!("Failed to load font from data: {}", font_path))
    })?;

    let text_scale_value = 33.0; // Controls the font size in the raster image
    let scale = Scale::uniform(text_scale_value);

    // Calculate text bounding box using pixel_bounding_box for more accuracy
    let glyphs: Vec<_> = font
        .layout(text, scale, rusttype::point(0.0, 0.0))
        .collect();

    let min_x = glyphs
        .iter()
        .filter_map(|g| g.pixel_bounding_box().map(|bb| bb.min.x))
        .min()
        .unwrap_or(0) as f32;
    let max_x = glyphs
        .iter()
        .filter_map(|g| g.pixel_bounding_box().map(|bb| bb.max.x))
        .max()
        .unwrap_or(0) as f32;
    let min_y = glyphs
        .iter()
        .filter_map(|g| g.pixel_bounding_box().map(|bb| bb.min.y))
        .min()
        .unwrap_or(0) as f32;
    let max_y = glyphs
        .iter()
        .filter_map(|g| g.pixel_bounding_box().map(|bb| bb.max.y))
        .max()
        .unwrap_or(0) as f32;

    // Ensure text_render_width/height are not zero, which can happen for empty strings or problematic fonts
    let text_render_width = (max_x - min_x).ceil() as u32;
    let text_render_height = (max_y - min_y).ceil() as u32;

    if text.is_empty() {
        // Specifically handle empty text case
        println!("Warning: Text is empty. Creating a minimal placeholder image.");
        // Create a small transparent image if text is empty
        let image: ImageBuffer<Rgba<u8>, _> = ImageBuffer::from_pixel(1, 1, Rgba([0, 0, 0, 0]));
        image.save(path)?;
        println!("Empty text image saved: {}", path);
        return Ok(());
    }

    if text_render_width == 0 || text_render_height == 0 {
        // If text is not empty but dimensions are zero, it's an issue.
        return Err(format!("Calculated text render dimensions are zero for text: '{}'. Width: {}, Height: {}. Check font or text characters.", text, text_render_width, text_render_height).into());
    }

    let padding = 10; // Padding around the text in the image
    let image_width = text_render_width + 2 * padding;
    let image_height = text_render_height + 2 * padding;
    println!(
        "Generated image dimensions (before PDF scaling): width={}, height={}",
        image_width, image_height
    );

    let mut image: ImageBuffer<Rgba<u8>, _> =
        ImageBuffer::from_pixel(image_width, image_height, Rgba([0, 0, 0, 0])); // Transparent background

    // Adjust drawing position: draw relative to (padding - min_x, padding - min_y)
    // This effectively translates the glyphs so that their collective bounding box starts at (padding, padding)
    draw_text_mut(
        &mut image,
        Rgba([0, 0, 0, 255]), // Black text
        (padding as f32 - min_x) as i32,
        (padding as f32 - min_y) as i32,
        scale,
        &font,
        text,
    );

    image.save(path)?;
    println!("Text image saved: {}", path);
    Ok(())
}

fn add_text_image_to_pdf(
    doc: &mut Document,
    page_id: ObjectId,
    text: &str,
    font_path: &str,
    image_name: &str, // This will be the XObject name in PDF
    x: f64,
    y: f64,
    image_scale_in_pdf: f64,
    with_transparency: bool,
) -> Result<(), Box<dyn Error>> {
    let temp_image_path = format!("{}.png", image_name); // e.g., "PramodSign.png"
    println!(
        "Preparing to add text '{}' as image '{}' (file: {}) to PDF page ID {:?}",
        text, image_name, temp_image_path, page_id
    );

    create_text_image(text, &temp_image_path, font_path)?;

    add_png_image_to_pdf(
        doc,
        page_id,
        &temp_image_path,
        image_name, // Use the base name for PDF XObject
        x,
        y,
        image_scale_in_pdf,
        with_transparency,
    )?;

    match fs::remove_file(&temp_image_path) {
        Ok(_) => println!("Removed temporary image: {}", temp_image_path),
        Err(e) => eprintln!(
            "Warning: Failed to remove temporary image '{}': {}",
            temp_image_path, e
        ),
    }
    Ok(())
}

// add_png_image_to_pdf function (from previous correct version, unchanged by this error fix)
fn add_png_image_to_pdf(
    doc: &mut Document,
    page_id: ObjectId,
    image_path: &str,
    pdf_xobject_name: &str,
    x_coord_pdf: f64,
    y_coord_pdf: f64,
    scale_in_pdf: f64,
    with_transparency: bool,
) -> Result<(), Box<dyn Error>> {
    println!(
        "Adding PNG '{}' as XObject '{}' to PDF at ({}, {}) with scale {}, transparency: {}",
        image_path, pdf_xobject_name, x_coord_pdf, y_coord_pdf, scale_in_pdf, with_transparency
    );
    let img_file = fs::File::open(image_path)
        .map_err(|e| format!("Failed to open image file '{}': {}", image_path, e))?;
    let mut img_rgba = image::load(std::io::BufReader::new(img_file), image::ImageFormat::Png)
        .map_err(|e| format!("Failed to decode image file '{}': {}", image_path, e))?
        .to_rgba8();

    image::imageops::flip_vertical_in_place(&mut img_rgba);

    let (img_width_pixels, img_height_pixels) = img_rgba.dimensions();

    if img_width_pixels == 0 || img_height_pixels == 0 {
        return Err(format!(
            "Image '{}' has zero dimensions ({}x{}). Cannot process.",
            image_path, img_width_pixels, img_height_pixels
        )
        .into());
    }

    let final_width_in_pdf = (img_width_pixels as f64 * scale_in_pdf) as i64;
    let final_height_in_pdf = (img_height_pixels as f64 * scale_in_pdf) as i64;

    let mut rgb_buf = Vec::with_capacity((img_width_pixels * img_height_pixels * 3) as usize);
    let mut alpha_channel_data = if with_transparency {
        Some(Vec::with_capacity(
            (img_width_pixels * img_height_pixels) as usize,
        ))
    } else {
        None
    };

    for pixel in img_rgba.pixels() {
        let [r, g, b, a] = pixel.0;
        rgb_buf.extend_from_slice(&[r, g, b]);
        if let Some(alpha_vec) = alpha_channel_data.as_mut() {
            alpha_vec.push(a);
        }
    }

    let mut rgb_encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    rgb_encoder.write_all(&rgb_buf)?;
    let compressed_rgb_data = rgb_encoder.finish()?;

    let mut image_xobject_dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => img_width_pixels as i64,
        "Height" => img_height_pixels as i64,
        "ColorSpace" => "DeviceRGB",
        "BitsPerComponent" => 8,
        "Filter" => "FlateDecode",
    };

    if with_transparency && alpha_channel_data.is_some() {
        let alpha_bytes = alpha_channel_data.as_ref().unwrap();
        if !alpha_bytes.is_empty() {
            let mut alpha_encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            alpha_encoder.write_all(alpha_bytes)?;
            let compressed_alpha_data = alpha_encoder.finish()?;

            let smask_object_id: ObjectId = doc.add_object(Stream::new(
                dictionary! {
                    "Type" => "XObject", "Subtype" => "Image",
                    "Width" => img_width_pixels as i64, "Height" => img_height_pixels as i64,
                    "ColorSpace" => "DeviceGray",
                    "BitsPerComponent" => 8,
                    "Filter" => "FlateDecode",
                },
                compressed_alpha_data,
            ));
            image_xobject_dict.set("SMask", Object::Reference(smask_object_id));
            println!(
                "SMask (ID: {:?}) created and added to image XObject dictionary.",
                smask_object_id
            );
        } else {
            println!("Alpha channel data was empty, skipping SMask creation.");
        }
    }

    let final_image_xobject_id: ObjectId =
        doc.add_object(Stream::new(image_xobject_dict, compressed_rgb_data));
    println!("Image XObject (ID: {:?}) created.", final_image_xobject_id);

    {
        let page_object_mut_ref = doc.get_object_mut(page_id).map_err(|e| {
            format!(
                "Failed to get page object (ID: {:?}) for Resources update: {}",
                page_id, e
            )
        })?;
        let page_dict = match page_object_mut_ref.as_dict_mut() {
            Ok(dict) => dict,
            Err(lopdf::Error::Type) => {
                let actual_type_name = doc
                    .get_object(page_id)
                    .map(|obj| obj.type_name().unwrap_or("Unknown"))
                    .unwrap_or("Not Found");
                return Err(format!(
                    "Page object (ID: {:?}) for Resources update was expected to be a Dictionary, but it is of type '{}'.",
                    page_id, actual_type_name
                ).into());
            }
            Err(e) => {
                return Err(
                    format!("Page object (ID: {:?}) not a dictionary: {}", page_id, e).into(),
                );
            }
        };

        let resources_key = b"Resources".to_vec();
        if page_dict
            .get(&resources_key)
            .map_or(true, |obj| obj.as_dict().is_err())
        {
            page_dict.set(resources_key.clone(), Object::Dictionary(Dictionary::new()));
            println!(
                "Initialized new Resources dictionary for page ID {:?}",
                page_id
            );
        }

        let resources_obj_mut = page_dict.get_mut(&resources_key).unwrap();
        let resources_dict = resources_obj_mut.as_dict_mut().map_err(|e| {
            format!(
                "Resources object is not a dictionary for page {:?}: {}",
                page_id, e
            )
        })?;

        let xobject_key = b"XObject".to_vec();
        if resources_dict
            .get(&xobject_key)
            .map_or(true, |obj| obj.as_dict().is_err())
        {
            resources_dict.set(xobject_key.clone(), Object::Dictionary(Dictionary::new()));
            println!(
                "Initialized new XObject dictionary in Resources for page ID {:?}",
                page_id
            );
        }
        let xobjects_obj_mut = resources_dict.get_mut(&xobject_key).unwrap();
        let xobjects_dict = xobjects_obj_mut.as_dict_mut().map_err(|e| {
            format!(
                "XObject in Resources is not a dictionary for page {:?}: {}",
                page_id, e
            )
        })?;

        xobjects_dict.set(
            pdf_xobject_name.as_bytes().to_vec(),
            Object::Reference(final_image_xobject_id),
        );
        println!(
            "Image XObject reference added to page Resources under name '{}'.",
            pdf_xobject_name
        );
    }

    let draw_ops = format!(
        "q\n{} 0 0 {} {} {} cm\n/{} Do\nQ\n",
        final_width_in_pdf, final_height_in_pdf, x_coord_pdf, y_coord_pdf, pdf_xobject_name
    );
    println!("PDF drawing operations: {}", draw_ops.replace('\n', "\\n"));

    let new_drawing_stream_object = Stream::new(dictionary! {}, draw_ops.into_bytes());
    let new_drawing_stream_id: ObjectId = doc.add_object(new_drawing_stream_object);
    println!(
        "New drawing content stream (ID: {:?}) created.",
        new_drawing_stream_id
    );

    {
        let page_obj_for_write = doc.get_object_mut(page_id).map_err(|e| {
            format!(
                "Failed to get page object (ID: {:?}) for Contents update: {}",
                page_id, e
            )
        })?;
        let page_dict_for_write = match page_obj_for_write.as_dict_mut() {
            Ok(dict) => dict,
            Err(lopdf::Error::Type) => {
                let actual_type = doc
                    .get_object(page_id)
                    .map(|obj| obj.type_name().unwrap_or("Unknown"))
                    .unwrap_or("Not Found");
                return Err(format!(
                    "Page object (ID: {:?}) for Contents write was expected to be a Dictionary, but it is of type '{}'.",
                    page_id, actual_type
                ).into());
            }
            Err(e) => {
                return Err(format!(
                    "Page object (ID: {:?}) not a dictionary for Contents update: {}",
                    page_id, e
                )
                .into());
            }
        };

        let contents_key = b"Contents".to_vec();
        let new_stream_object_ref = Object::Reference(new_drawing_stream_id);
        let current_contents_val = page_dict_for_write.get(&contents_key).cloned();

        let final_contents_object: Object = match current_contents_val {
            Ok(Object::Array(mut arr)) => {
                println!(
                    "Original /Contents is an array. Appending new stream ID {:?}.",
                    new_drawing_stream_id
                );
                arr.push(new_stream_object_ref);
                Object::Array(arr)
            }
            Ok(Object::Reference(old_stream_id)) => {
                println!(
                    "Original /Contents is a single stream ref {:?}. Converting to array and appending new stream ID {:?}.",
                    old_stream_id, new_drawing_stream_id
                );
                Object::Array(vec![
                    Object::Reference(old_stream_id),
                    new_stream_object_ref,
                ])
            }
            Ok(other_type_obj) => {
                eprintln!(
                    "Warning: Page ID {:?} /Contents was an unexpected type: {:?}. Overwriting with new stream reference array.",
                    page_id,
                    other_type_obj.type_name().unwrap_or("Unknown")
                );
                Object::Array(vec![new_stream_object_ref])
            }
            Err(_) => {
                println!(
                    "/Contents key not found or unreadable for page ID {:?}. Setting new stream ID {:?} as /Contents array.",
                    page_id, new_drawing_stream_id
                );
                Object::Array(vec![new_stream_object_ref])
            }
        };

        page_dict_for_write.set(contents_key, final_contents_object);
        println!(
            "Successfully updated /Contents for page ID {:?} using non-destructive method.",
            page_id
        );
    }
    Ok(())
}
