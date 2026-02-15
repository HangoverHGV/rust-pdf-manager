use std::collections::{BTreeMap, HashMap};

use hayro::hayro_interpret::hayro_syntax::Pdf;
use hayro::hayro_interpret::InterpreterSettings;
use hayro::{render, RenderSettings};
use lopdf::{Document, Object, ObjectId};
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

const PREVIEW_WIDTH: f64 = 120.0;

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

        let data = match std::fs::read(&page_item.source_path) {
            Ok(d) => d,
            Err(_) => return,
        };

        let pdf = match Pdf::new(&data) {
            Ok(p) => p,
            Err(_) => return,
        };

        let page_index = page_item.page_number - 1;
        let pages: Vec<_> = pdf.pages().collect();
        if let Some(Ok(page)) = pages.get(page_index) {
            let rotation = page_item.rotation;
            if let Ok((rgba, width, height)) = render_page_to_rgba(page, rotation) {
                self.pages[index].preview = rgba_to_image(&rgba, width, height);
            }
        }
    }

    pub fn clear(&mut self) {
        self.files.clear();
        self.pages.clear();
    }
}

pub struct RawPageData {
    pub source_path: String,
    pub source_file: String,
    pub page_number: usize,
    pub display_text: String,
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub struct RawFileData {
    pub path: String,
    pub filename: String,
    pub page_count: usize,
}

pub struct LoadResult {
    pub files: Vec<RawFileData>,
    pub pages: Vec<RawPageData>,
}

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

        let data = std::fs::read(path).map_err(|e| format!("Error reading {}: {}", path, e))?;

        let pdf = Pdf::new(&data).map_err(|e| format!("Error parsing {}: {}", path, e))?;

        for (page_num, page_result) in pdf.pages().enumerate() {
            let (rgba, width, height) = match page_result {
                Ok(ref page) => match render_page_to_rgba(page, 0) {
                    Ok(data) => data,
                    Err(_) => (Vec::new(), 0, 0),
                },
                Err(_) => (Vec::new(), 0, 0),
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

/// Save merged PDF. Caches loaded documents so each source file is only read once.
pub fn save_pdf_standalone(
    pages: &[(String, usize, i32)],
    output_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if pages.is_empty() {
        return Err("No pages to save".into());
    }

    // Cache: load each unique source file only once
    let mut doc_cache: HashMap<String, Document> = HashMap::new();
    for (source_path, _, _) in pages {
        if !doc_cache.contains_key(source_path) {
            let doc = Document::load(source_path)?;
            doc_cache.insert(source_path.clone(), doc);
        }
    }

    let mut max_id = 1u32;
    let mut collected_pages: Vec<(ObjectId, Object)> = Vec::new();
    let mut collected_objects: BTreeMap<ObjectId, Object> = BTreeMap::new();
    let mut catalog_object: Option<(ObjectId, Object)> = None;
    let mut pages_object: Option<(ObjectId, Object)> = None;

    for (source_path, page_number, _rotation) in pages {
        // Clone from cache so we can mutate (delete pages, renumber)
        let mut doc = doc_cache.get(source_path).unwrap().clone();

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

/// Render a PDF page to raw RGBA bytes using hayro.
fn render_page_to_rgba(
    page: &hayro::hayro_interpret::hayro_syntax::page::Page<'_>,
    rotation: i32,
) -> Result<(Vec<u8>, u32, u32), Box<dyn std::error::Error>> {
    let media_box = page.media_box();
    let (pw, ph) = (media_box[2] - media_box[0], media_box[3] - media_box[1]);

    let (ew, eh) = if rotation == 90 || rotation == 270 {
        (ph as f64, pw as f64)
    } else {
        (pw as f64, ph as f64)
    };

    let scale = PREVIEW_WIDTH / ew;
    let w = (ew * scale) as u16;
    let h = (eh * scale) as u16;

    let render_settings = RenderSettings {
        x_scale: scale as f32,
        y_scale: scale as f32,
        width: Some(w),
        height: Some(h),
        ..Default::default()
    };

    let interpreter_settings = InterpreterSettings::default();
    let pixmap = render(page, &interpreter_settings, &render_settings);

    let src_data = pixmap.data_as_u8_slice();
    let src_w = pixmap.width() as u32;
    let src_h = pixmap.height() as u32;

    // Unpremultiply alpha (hayro outputs premultiplied RGBA8)
    let mut rgba = Vec::with_capacity(src_data.len());
    for pixel in src_data.chunks_exact(4) {
        let r = pixel[0];
        let g = pixel[1];
        let b = pixel[2];
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

    // Apply rotation to the pixel buffer
    let (final_rgba, final_w, final_h) = rotate_rgba(&rgba, src_w, src_h, rotation);

    Ok((final_rgba, final_w, final_h))
}

/// Rotate an RGBA pixel buffer by 0, 90, 180, or 270 degrees.
fn rotate_rgba(data: &[u8], width: u32, height: u32, rotation: i32) -> (Vec<u8>, u32, u32) {
    match rotation {
        90 => {
            let new_w = height;
            let new_h = width;
            let mut out = vec![0u8; (new_w * new_h * 4) as usize];
            for y in 0..height {
                for x in 0..width {
                    let src = ((y * width + x) * 4) as usize;
                    let dst_x = (height - 1 - y) as usize;
                    let dst_y = x as usize;
                    let dst = (dst_y * new_w as usize + dst_x) * 4;
                    out[dst..dst + 4].copy_from_slice(&data[src..src + 4]);
                }
            }
            (out, new_w, new_h)
        }
        180 => {
            let mut out = vec![0u8; data.len()];
            for y in 0..height {
                for x in 0..width {
                    let src = ((y * width + x) * 4) as usize;
                    let dst_x = (width - 1 - x) as usize;
                    let dst_y = (height - 1 - y) as usize;
                    let dst = (dst_y * width as usize + dst_x) * 4;
                    out[dst..dst + 4].copy_from_slice(&data[src..src + 4]);
                }
            }
            (out, width, height)
        }
        270 => {
            let new_w = height;
            let new_h = width;
            let mut out = vec![0u8; (new_w * new_h * 4) as usize];
            for y in 0..height {
                for x in 0..width {
                    let src = ((y * width + x) * 4) as usize;
                    let dst_x = y as usize;
                    let dst_y = (width - 1 - x) as usize;
                    let dst = (dst_y * new_w as usize + dst_x) * 4;
                    out[dst..dst + 4].copy_from_slice(&data[src..src + 4]);
                }
            }
            (out, new_w, new_h)
        }
        _ => (data.to_vec(), width, height),
    }
}
