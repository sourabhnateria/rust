use flate2::Compression;
use flate2::write::ZlibEncoder;
use image::{GenericImageView, ImageBuffer, Rgba};
use imageproc::drawing::draw_text_mut;
use lopdf::{Document, Object, Stream, dictionary};
use rusttype::{Font, Scale};
use std::fs;
use std::io::Write;

struct ImagePlacement<'a> {
    path: &'a str,
    name: &'a str,
    page: u32,
    x: f64,
    y: f64,
    scale: f64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = Document::load("D:\\rust\\sign\\input.pdf")?;
    let pages = doc.get_pages();

    // Text overlays on page 1
    if let Some(&page2_id) = pages.get(&2) {
        add_text_image_to_pdf(
            &mut doc,
            page2_id,
            "Pramod",
            "fonts/Motterdam-K74zp.ttf",
            "Im1",
            100.0,
            3010.0,
        )?;

        //     add_text_image_to_pdf(
        //         &mut doc,
        //         page1_id,
        //         "john doe",
        //         "fonts/LovelyHome-9aBZ.ttf",
        //         "Im2",
        //         1300.0,
        //         3000.0,
        //     )?;
        //     add_text_image_to_pdf(
        //         &mut doc,
        //         page1_id,
        //         "john doe",
        //         "fonts/SacrificeDemo-8Ox1B.ttf",
        //         "Im3",
        //         800.0,
        //         3010.0,
        //     )?;
        // }

        // if let Some(&page2_id) = pages.get(&2) {
        //     add_text_image_to_pdf(
        //         &mut doc,
        //         page2_id,
        //         "john doe",
        //         "fonts/StylishCalligraphyDemo-XPZZ.ttf",
        //         "Im4",
        //         200.0,
        //         2990.0,
        //     )?;

        //     add_text_image_to_pdf(
        //         &mut doc,
        //         page2_id,
        //         "john doe",
        //         "fonts/SweetHipster-PzlE.ttf",
        //         "Im5",
        //         1300.0,
        //         3000.0,
        //     )?;
        // }

        // if let Some(&page3_id) = pages.get(&3) {
        //     add_text_image_to_pdf(
        //         &mut doc,
        //         page3_id,
        //         "john doe",
        //         "fonts/BrotherSignature-7BWnK.otf",
        //         "Im6",
        //         300.0,
        //         2900.0,
        //     )?;

        //     add_text_image_to_pdf(
        //         &mut doc,
        //         page3_id,
        //         "john doe",
        //         "fonts/Motterdam-K74zp.ttf",
        //         "Im7",
        //         1300.0,
        //         2900.0,
        //     )?;
        // }

        // ✅ Multiple images on multiple pages
        // let images = vec![
        //     ImagePlacement {
        //         path: "D:\\rust\\sign\\tick.png",
        //         name: "Tick1",
        //         page: 1,
        //         x: 400.0,
        //         y: 200.0,
        //         scale: 1.5,
        //     },
        // ImagePlacement {
        //     path: "D:\\rust\\dsign\\tanent.png",
        //     name: "tanentSign",
        //     page: 2,
        //     x: 150.0,
        //     y: 3000.0,
        //     scale: 1.2,
        // },
        // ImagePlacement {
        //     path: "D:\\rust\\pdf1\\lessor.png",
        //     name: "lessorSign",
        //     page: 3,
        //     x: 1300.0,
        //     y: 3000.0,
        //     scale: 1.0,
        // },
        // ];

        // for image in images {
        //     if let Some(&page_id) = pages.get(&image.page) {
        //         add_png_image_to_pdf(
        //             &mut doc,
        //             page_id,
        //             image.path,
        //             image.name,
        //             image.x,
        //             image.y,
        //             image.scale,
        //         )?;
        //     } else {
        //         println!("⚠️ Page {} not found in PDF!", image.page);
        //     }
        // }
    }
    doc.save("filled_contract_rasterized.pdf")?;
    println!("✅ PDF saved with text and images on multiple pages!");
    Ok(())
}

fn create_text_image(
    text: &str,
    path: &str,
    font_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let font_data = fs::read(font_path)?;
    let font = Font::try_from_vec(font_data).unwrap();
    let scale = Scale::uniform(33.0);
    let width = 300;
    let height = 50;

    let mut image: ImageBuffer<Rgba<u8>, _> =
        ImageBuffer::from_pixel(width, height, Rgba([0, 0, 0, 0]));
    draw_text_mut(&mut image, Rgba([0, 0, 0, 255]), 5, 2, scale, &font, text);
    image.save(path)?;
    Ok(())
}

fn add_text_image_to_pdf(
    doc: &mut Document,
    page_id: lopdf::ObjectId,
    text: &str,
    font_path: &str,
    image_name: &str,
    x: f64,
    y: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    let image_path = format!("{image_name}.png");
    create_text_image(text, &image_path, font_path)?;
    add_png_image_to_pdf(doc, page_id, &image_path, image_name, x, y, 3.0)
}

fn add_png_image_to_pdf(
    doc: &mut Document,
    page_id: lopdf::ObjectId,
    image_path: &str,
    image_name: &str,
    x: f64,
    y: f64,
    scale: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut img = image::open(image_path)?.to_rgba8();
    image::imageops::flip_vertical_in_place(&mut img);
    let (width, height) = img.dimensions();
    let width_scaled = (width as f64 * scale) as i64;
    let height_scaled = (height as f64 * scale) as i64;

    let mut alpha_buf = Vec::with_capacity((width * height) as usize);
    let mut rgb_buf = Vec::with_capacity((width * height * 3) as usize);

    for pixel in img.pixels() {
        let [r, g, b, a] = pixel.0;
        rgb_buf.extend_from_slice(&[r, g, b]);
        alpha_buf.push(a);
    }

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&rgb_buf)?;
    let compressed_rgb = encoder.finish()?;

    let mut alpha_encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    alpha_encoder.write_all(&alpha_buf)?;
    let compressed_alpha = alpha_encoder.finish()?;

    let smask_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => width as i64,
            "Height" => height as i64,
            "ColorSpace" => "DeviceGray",
            "BitsPerComponent" => 8,
            "Filter" => "FlateDecode",
        },
        compressed_alpha,
    ));

    let xobject_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => width as i64,
            "Height" => height as i64,
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
            "Filter" => "FlateDecode",
            "SMask" => Object::Reference(smask_id),
        },
        compressed_rgb,
    ));

    {
        let page = doc.get_object_mut(page_id)?;
        let dict = page.as_dict_mut()?;

        if !dict.has(b"Resources") {
            dict.set("Resources", Object::Dictionary(dictionary! {}));
        }

        let resources = dict.get_mut(b"Resources")?.as_dict_mut()?;
        if !resources.has(b"XObject") {
            resources.set("XObject", Object::Dictionary(dictionary! {}));
        }

        let xobjects = resources.get_mut(b"XObject")?.as_dict_mut()?;
        xobjects.set(
            image_name.as_bytes().to_vec(),
            Object::Reference(xobject_id),
        );
    }

    let draw_ops = format!(
        "q\n{} 0 0 {} {} {} cm\n/{} Do\nQ\n",
        width_scaled, height_scaled, x, y, image_name
    );
    let stream = Stream::new(dictionary! {}, draw_ops.as_bytes().to_vec());
    let stream_id = doc.add_object(stream);

    {
        let page = doc.get_object_mut(page_id)?;
        let dict = page.as_dict_mut()?;

        let new_contents = match dict.remove(b"Contents") {
            Some(Object::Reference(existing)) => Object::Array(vec![
                Object::Reference(existing),
                Object::Reference(stream_id),
            ]),
            Some(Object::Array(mut array)) => {
                array.push(Object::Reference(stream_id));
                Object::Array(array)
            }
            _ => Object::Reference(stream_id),
        };
        dict.set("Contents", new_contents);
    }

    Ok(())
}
