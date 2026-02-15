use lopdf::Document;

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

        // Add all pages from this PDF to the page list
        for page_num in 0..page_count {
            self.pages.push(PageItem {
                source_file: filename.clone(),
                page_number: page_num + 1,
                display_text: format!("{} - Page {}", filename, page_num + 1),
            });
        }

        self.files.push(pdf_file.clone());
        Ok(pdf_file)
    }

    pub fn clear(&mut self) {
        self.files.clear();
        self.pages.clear();
    }
}
