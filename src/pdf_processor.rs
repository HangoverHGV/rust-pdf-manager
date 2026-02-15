use lopdf::Document;
use pdfium_render::prelude::*;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

const PREVIEW_WIDTH: i32 = 200;

#[derive(Clone)]
pub struct PdfFile {
    pub path: String,
    pub filename: String,
    pub page_count: usize,
}

pub struct PageItem {
    pub source_file: String,
    pub page_number: usize,
    pub display_text: String,
    pub preview: Image,
}

pub struct PdfProcessor {
    pub files: Vec<PdfFile>,
    pub pages: Vec<PageItem>,
    pdfium: Pdfium,
}

impl PdfProcessor {
    pub fn new() -> Self {
        let pdfium = Pdfium::default();
        Self {
            files: Vec::new(),
            pages: Vec::new(),
            pdfium,
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

        // Render page previews using pdfium
        let pdf_doc = self.pdfium.load_pdf_from_file(path, None)?;
        let render_config = PdfRenderConfig::new().set_target_width(PREVIEW_WIDTH);

        for page_num in 0..page_count {
            let preview = match pdf_doc.pages().get(page_num as u16) {
                Ok(page) => match page.render_with_config(&render_config) {
                    Ok(bitmap) => {
                        let width = bitmap.width() as u32;
                        let height = bitmap.height() as u32;
                        let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                            bitmap.as_rgba_bytes().as_slice(),
                            width,
                            height,
                        );
                        Image::from_rgba8(buf)
                    }
                    Err(e) => {
                        eprintln!("Failed to render page {}: {}", page_num + 1, e);
                        Image::default()
                    }
                },
                Err(e) => {
                    eprintln!("Failed to get page {}: {}", page_num + 1, e);
                    Image::default()
                }
            };

            self.pages.push(PageItem {
                source_file: filename.clone(),
                page_number: page_num + 1,
                display_text: format!("{} - Page {}", filename, page_num + 1),
                preview,
            });
        }

        self.files.push(pdf_file.clone());
        Ok(pdf_file)
    }

    pub fn remove_page(&mut self, index: usize) {
        if index < self.pages.len() {
            self.pages.remove(index);
        }
    }

    pub fn clear(&mut self) {
        self.files.clear();
        self.pages.clear();
    }
}
