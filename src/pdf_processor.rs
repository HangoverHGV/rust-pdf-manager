use std::collections::BTreeMap;

use lopdf::{Document, Object, ObjectId};
use poppler::cairo;
use poppler::PopplerDocument;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

const PREVIEW_WIDTH: f64 = 200.0;

#[derive(Clone)]
pub struct PdfFile {
    pub path: String,
    pub filename: String,
    pub page_count: usize,
}

pub struct PageItem {
    pub source_path: String,
    pub source_file: String,
    pub page_number: usize,
    pub display_text: String,
    pub preview: Image,
    pub rotation: i32,
}

pub struct PdfProcessor {
    pub files: Vec<PdfFile>,
    pub pages: Vec<PageItem>,
}

impl PdfProcessor {
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            pages: Vec::new(),
        }
    }

    pub fn remove_page(&mut self, index: usize) {
        if index < self.pages.len() {
            self.pages.remove(index);
        }
    }

    pub fn move_page(&mut self, from: usize, to: usize) {
        if from < self.pages.len() && to < self.pages.len() && from != to {
            let page = self.pages.remove(from);
            self.pages.insert(to, page);
        }
    }

    pub fn rotate_page(&mut self, index: usize) {
        if index >= self.pages.len() {
            return;
        }

        let page_item = &mut self.pages[index];
        page_item.rotation = (page_item.rotation + 90) % 360;

        // Re-render the preview with the new rotation
        let poppler_doc = match PopplerDocument::new_from_file(&page_item.source_path, None) {
            Ok(doc) => doc,
            Err(e) => {
                eprintln!("Failed to load PDF for rotation preview: {}", e);
                return;
            }
        };

        let page_index = page_item.page_number - 1;
        if let Some(page) = poppler_doc.get_page(page_index) {
            let (pw, ph) = page.get_size();
            let rotation = page_item.rotation;

            // For 90/270 rotations, swap dimensions
            let (ew, eh) = if rotation == 90 || rotation == 270 {
                (ph, pw)
            } else {
                (pw, ph)
            };

            let scale = PREVIEW_WIDTH / ew;
            let w = (ew * scale) as i32;
            let h = (eh * scale) as i32;

            match render_page_to_image(&page, w, h, scale, rotation) {
                Ok(img) => {
                    self.pages[index].preview = img;
                }
                Err(e) => {
                    eprintln!("Failed to render rotated preview: {}", e);
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.files.clear();
        self.pages.clear();
    }
}

/// Raw page data returned from background thread (no slint::Image since it's not Send).
pub struct RawPageData {
    pub source_path: String,
    pub source_file: String,
    pub page_number: usize,
    pub display_text: String,
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Raw file data returned from background thread.
pub struct RawFileData {
    pub path: String,
    pub filename: String,
    pub page_count: usize,
}

/// Result of loading PDFs on a background thread.
pub struct LoadResult {
    pub files: Vec<RawFileData>,
    pub pages: Vec<RawPageData>,
}

/// Standalone load function that can be called from a background thread.
/// Returns raw RGBA pixel data instead of slint::Image.
pub fn load_pdfs_standalone(paths: &[String]) -> Result<LoadResult, String> {
    let mut files = Vec::new();
    let mut pages = Vec::new();

    for path in paths {
        let doc = Document::load(path).map_err(|e| format!("Error loading {}: {}", path, e))?;
        let page_count = doc.get_pages().len();

        let filename = std::path::Path::new(path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        files.push(RawFileData {
            path: path.clone(),
            filename: filename.clone(),
            page_count,
        });

        let poppler_doc = PopplerDocument::new_from_file(path, None)
            .map_err(|e| format!("Error rendering {}: {}", path, e))?;

        for page_num in 0..page_count {
            let (rgba, width, height) = match poppler_doc.get_page(page_num) {
                Some(page) => {
                    let (pw, ph) = page.get_size();
                    let scale = PREVIEW_WIDTH / pw;
                    let w = (pw * scale) as i32;
                    let h = (ph * scale) as i32;

                    match render_page_to_rgba(&page, w, h, scale, 0) {
                        Ok(data) => data,
                        Err(_) => (Vec::new(), 0, 0),
                    }
                }
                None => (Vec::new(), 0, 0),
            };

            pages.push(RawPageData {
                source_path: path.clone(),
                source_file: filename.clone(),
                page_number: page_num + 1,
                display_text: format!("{} - Page {}", filename, page_num + 1),
                rgba,
                width,
                height,
            });
        }
    }

    Ok(LoadResult { files, pages })
}

/// Convert raw RGBA data to a slint::Image (must be called on main thread).
pub fn rgba_to_image(rgba: &[u8], width: u32, height: u32) -> Image {
    if rgba.is_empty() {
        return Image::default();
    }
    let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(rgba, width, height);
    Image::from_rgba8(buf)
}

/// Standalone save function that can be called from a background thread.
/// Takes a list of (source_path, page_number, rotation) tuples.
pub fn save_pdf_standalone(
    pages: &[(String, usize, i32)],
    output_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if pages.is_empty() {
        return Err("No pages to save".into());
    }

    let mut max_id = 1u32;
    let mut collected_pages: Vec<(ObjectId, Object)> = Vec::new();
    let mut collected_objects: BTreeMap<ObjectId, Object> = BTreeMap::new();
    let mut catalog_object: Option<(ObjectId, Object)> = None;
    let mut pages_object: Option<(ObjectId, Object)> = None;

    for (source_path, page_number, _rotation) in pages {
        let mut doc = Document::load(source_path)?;

        let page_ids: BTreeMap<u32, ObjectId> = doc.get_pages();
        let _page_object_id = page_ids
            .get(&(*page_number as u32))
            .ok_or_else(|| format!("Page {} not found in {}", page_number, source_path))?;

        let pages_to_delete: Vec<u32> = page_ids
            .keys()
            .filter(|&&k| k != *page_number as u32)
            .copied()
            .collect();
        doc.delete_pages(&pages_to_delete);

        doc.renumber_objects_with(max_id);
        max_id = doc.max_id + 1;

        let remaining_pages = doc.get_pages();
        for (_, object_id) in &remaining_pages {
            if let Ok(obj) = doc.get_object(*object_id) {
                collected_pages.push((*object_id, obj.clone()));
            }
        }

        for (object_id, object) in doc.objects.into_iter() {
            match object.type_name().unwrap_or(b"") {
                b"Catalog" => {
                    if catalog_object.is_none() {
                        catalog_object = Some((object_id, object));
                    }
                }
                b"Pages" => {
                    if let Ok(dictionary) = object.as_dict() {
                        let mut dictionary = dictionary.clone();
                        if let Some((_, ref existing)) = pages_object {
                            if let Ok(old_dict) = existing.as_dict() {
                                dictionary.extend(old_dict);
                            }
                        }
                        pages_object = Some((
                            if let Some((id, _)) = pages_object {
                                id
                            } else {
                                object_id
                            },
                            Object::Dictionary(dictionary),
                        ));
                    }
                }
                b"Page" => {}
                b"Outlines" | b"Outline" => {}
                _ => {
                    collected_objects.insert(object_id, object);
                }
            }
        }
    }

    let pages_object = pages_object.ok_or("No Pages object found in source documents")?;
    let catalog_object = catalog_object.ok_or("No Catalog object found in source documents")?;

    let mut document = Document::with_version("1.5");
    document.objects = collected_objects;

    let (page_id, page_object) = pages_object;
    let (catalog_id, catalog_object) = catalog_object;

    for (i, (object_id, object)) in collected_pages.iter().enumerate() {
        if let Ok(dictionary) = object.as_dict() {
            let mut dictionary = dictionary.clone();
            dictionary.set("Parent", page_id);
            let rotation = pages[i].2;
            if rotation != 0 {
                dictionary.set("Rotate", Object::Integer(rotation as i64));
            }
            document
                .objects
                .insert(*object_id, Object::Dictionary(dictionary));
        }
    }

    if let Ok(dictionary) = page_object.as_dict() {
        let mut dictionary = dictionary.clone();
        dictionary.set("Count", collected_pages.len() as u32);
        dictionary.set(
            "Kids",
            collected_pages
                .iter()
                .map(|(id, _)| Object::Reference(*id))
                .collect::<Vec<_>>(),
        );
        document
            .objects
            .insert(page_id, Object::Dictionary(dictionary));
    }

    if let Ok(dictionary) = catalog_object.as_dict() {
        let mut dictionary = dictionary.clone();
        dictionary.set("Pages", page_id);
        dictionary.remove(b"Outlines");
        document
            .objects
            .insert(catalog_id, Object::Dictionary(dictionary));
    }

    document.trailer.set("Root", catalog_id);
    document.max_id = document.objects.len() as u32;
    document.renumber_objects();
    document.adjust_zero_pages();

    document.save(output_path)?;
    Ok(())
}

/// Render a page to raw RGBA bytes (thread-safe, no slint types).
fn render_page_to_rgba(
    page: &poppler::PopplerPage,
    w: i32,
    h: i32,
    scale: f64,
    rotation: i32,
) -> Result<(Vec<u8>, u32, u32), Box<dyn std::error::Error>> {
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h)?;
    let ctx = cairo::Context::new(&surface)?;

    ctx.set_source_rgb(1.0, 1.0, 1.0);
    ctx.paint()?;

    let (pw, ph) = page.get_size();
    match rotation {
        90 => {
            ctx.translate(w as f64, 0.0);
            ctx.rotate(std::f64::consts::FRAC_PI_2);
            ctx.scale(w as f64 / ph, h as f64 / pw);
        }
        180 => {
            ctx.translate(w as f64, h as f64);
            ctx.rotate(std::f64::consts::PI);
            ctx.scale(scale, scale);
        }
        270 => {
            ctx.translate(0.0, h as f64);
            ctx.rotate(-std::f64::consts::FRAC_PI_2);
            ctx.scale(w as f64 / ph, h as f64 / pw);
        }
        _ => {
            ctx.scale(scale, scale);
        }
    }
    page.render(&ctx);

    ctx.show_page()?;
    drop(ctx);
    surface.flush();

    let data = surface.data()?;
    let width = w as u32;
    let height = h as u32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

    for pixel in data.chunks_exact(4) {
        let b = pixel[0];
        let g = pixel[1];
        let r = pixel[2];
        let a = pixel[3];

        if a == 0 {
            rgba.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            let un_r = ((r as u16 * 255) / a as u16).min(255) as u8;
            let un_g = ((g as u16 * 255) / a as u16).min(255) as u8;
            let un_b = ((b as u16 * 255) / a as u16).min(255) as u8;
            rgba.extend_from_slice(&[un_r, un_g, un_b, a]);
        }
    }

    Ok((rgba, width, height))
}

fn render_page_to_image(
    page: &poppler::PopplerPage,
    w: i32,
    h: i32,
    scale: f64,
    rotation: i32,
) -> Result<Image, Box<dyn std::error::Error>> {
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h)?;
    let ctx = cairo::Context::new(&surface)?;

    // White background
    ctx.set_source_rgb(1.0, 1.0, 1.0);
    ctx.paint()?;

    // Apply rotation around center then scale
    let (pw, ph) = page.get_size();
    match rotation {
        90 => {
            ctx.translate(w as f64, 0.0);
            ctx.rotate(std::f64::consts::FRAC_PI_2);
            ctx.scale(w as f64 / ph, h as f64 / pw);
        }
        180 => {
            ctx.translate(w as f64, h as f64);
            ctx.rotate(std::f64::consts::PI);
            ctx.scale(scale, scale);
        }
        270 => {
            ctx.translate(0.0, h as f64);
            ctx.rotate(-std::f64::consts::FRAC_PI_2);
            ctx.scale(w as f64 / ph, h as f64 / pw);
        }
        _ => {
            ctx.scale(scale, scale);
        }
    }
    page.render(&ctx);

    ctx.show_page()?;
    drop(ctx);
    surface.flush();

    // Convert Cairo ARGB32 (native-endian premultiplied) to Slint RGBA8
    let data = surface.data()?;
    let width = w as u32;
    let height = h as u32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

    for pixel in data.chunks_exact(4) {
        // Cairo ARGB32 on little-endian: [B, G, R, A] (premultiplied)
        let b = pixel[0];
        let g = pixel[1];
        let r = pixel[2];
        let a = pixel[3];

        // Un-premultiply
        if a == 0 {
            rgba.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            let un_r = ((r as u16 * 255) / a as u16).min(255) as u8;
            let un_g = ((g as u16 * 255) / a as u16).min(255) as u8;
            let un_b = ((b as u16 * 255) / a as u16).min(255) as u8;
            rgba.extend_from_slice(&[un_r, un_g, un_b, a]);
        }
    }

    let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&rgba, width, height);
    Ok(Image::from_rgba8(buf))
}
