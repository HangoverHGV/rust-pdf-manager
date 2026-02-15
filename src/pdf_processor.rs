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

    pub fn load_pdf(&mut self, path: &str) -> Result<PdfFile, Box<dyn std::error::Error>> {
        let doc = Document::load(path)?;
        let page_count = doc.get_pages().len();

        let filename = std::path::Path::new(path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let pdf_file = PdfFile {
            path: path.to_string(),
            filename: filename.clone(),
            page_count,
        };

        // Render page previews using poppler + cairo
        let poppler_doc = PopplerDocument::new_from_file(path, None)?;

        for page_num in 0..page_count {
            let preview = match poppler_doc.get_page(page_num) {
                Some(page) => {
                    let (pw, ph) = page.get_size();
                    let scale = PREVIEW_WIDTH / pw;
                    let w = (pw * scale) as i32;
                    let h = (ph * scale) as i32;

                    match render_page_to_image(&page, w, h, scale) {
                        Ok(img) => img,
                        Err(e) => {
                            eprintln!("Failed to render page {}: {}", page_num + 1, e);
                            Image::default()
                        }
                    }
                }
                None => {
                    eprintln!("Failed to get page {}", page_num + 1);
                    Image::default()
                }
            };

            self.pages.push(PageItem {
                source_path: path.to_string(),
                source_file: filename.clone(),
                page_number: page_num + 1,
                display_text: format!("{} - Page {}", filename, page_num + 1),
                preview,
            });
        }

        self.files.push(pdf_file.clone());
        Ok(pdf_file)
    }

    pub fn save_pdf(&self, output_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        if self.pages.is_empty() {
            return Err("No pages to save".into());
        }

        let mut max_id = 1u32;
        // Ordered list of (page_object_id, page_object) for the final document
        let mut collected_pages: Vec<(ObjectId, Object)> = Vec::new();
        let mut collected_objects: BTreeMap<ObjectId, Object> = BTreeMap::new();
        let mut catalog_object: Option<(ObjectId, Object)> = None;
        let mut pages_object: Option<(ObjectId, Object)> = None;

        for page_item in &self.pages {
            let mut doc = Document::load(&page_item.source_path)?;

            // Get the page ObjectId for the requested page number (1-based)
            let page_ids: BTreeMap<u32, ObjectId> = doc.get_pages();
            let _page_object_id =
                page_ids
                    .get(&(page_item.page_number as u32))
                    .ok_or_else(|| {
                        format!(
                            "Page {} not found in {}",
                            page_item.page_number, page_item.source_path
                        )
                    })?;

            // Delete all pages except the one we want
            let pages_to_delete: Vec<u32> = page_ids
                .keys()
                .filter(|&&k| k != page_item.page_number as u32)
                .copied()
                .collect();
            doc.delete_pages(&pages_to_delete);

            // Renumber objects to avoid ID collisions
            doc.renumber_objects_with(max_id);
            max_id = doc.max_id + 1;

            // Collect the single remaining page
            let pages = doc.get_pages();
            for (_, object_id) in &pages {
                if let Ok(obj) = doc.get_object(*object_id) {
                    collected_pages.push((*object_id, obj.clone()));
                }
            }

            // Collect all other objects
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

        // Insert page objects with correct parent reference
        for (object_id, object) in &collected_pages {
            if let Ok(dictionary) = object.as_dict() {
                let mut dictionary = dictionary.clone();
                dictionary.set("Parent", page_id);
                document
                    .objects
                    .insert(*object_id, Object::Dictionary(dictionary));
            }
        }

        // Build Pages object
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

        // Build Catalog object
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

    pub fn clear(&mut self) {
        self.files.clear();
        self.pages.clear();
    }
}

fn render_page_to_image(
    page: &poppler::PopplerPage,
    w: i32,
    h: i32,
    scale: f64,
) -> Result<Image, Box<dyn std::error::Error>> {
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h)?;
    let ctx = cairo::Context::new(&surface)?;

    // White background
    ctx.set_source_rgb(1.0, 1.0, 1.0);
    ctx.paint()?;

    // Scale and render
    ctx.scale(scale, scale);
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
