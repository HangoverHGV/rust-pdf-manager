mod pdf_processor;

use pdf_processor::PdfProcessor;
use slint::*;

slint::include_modules!();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ui = AppWindow::new()?;
    let processor = std::rc::Rc::new(std::cell::RefCell::new(PdfProcessor::new()));

    // Handle upload button click
    let ui_weak = ui.as_weak();
    let processor_clone = processor.clone();
    ui.on_upload_clicked(move || {
        if let Some(files) = rfd::FileDialog::new()
            .add_filter("PDF Files", &["pdf"])
            .pick_files()
        {
            let mut proc = processor_clone.borrow_mut();

            // Clear existing
            proc.clear();

            // Load all selected PDFs
            for file in files {
                let path = file.to_string_lossy().to_string();
                match proc.load_pdf(&path) {
                    Ok(_) => println!("Loaded: {}", path),
                    Err(e) => eprintln!("Error loading {}: {}", path, e),
                }
            }

            // Update UI
            if let Some(ui) = ui_weak.upgrade() {
                // Convert to Slint structs
                let pdf_files: Vec<PdfFile> = proc
                    .files
                    .iter()
                    .map(|f| PdfFile {
                        path: f.path.clone().into(),
                        filename: f.filename.clone().into(),
                        page_count: f.page_count as i32,
                    })
                    .collect();

                let page_list: Vec<PageItem> = proc
                    .pages
                    .iter()
                    .map(|p| PageItem {
                        source_file: p.source_file.clone().into(),
                        page_number: p.page_number as i32,
                        display_text: p.display_text.clone().into(),
                    })
                    .collect();

                ui.set_pdf_files(ModelRc::new(VecModel::from(pdf_files)));
                ui.set_page_list(ModelRc::new(VecModel::from(page_list)));
            }
        }
    });

    ui.run()?;
    Ok(())
}
