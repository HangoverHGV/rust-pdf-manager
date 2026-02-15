#![windows_subsystem = "windows"]

mod pdf_processor;

use pdf_processor::{
    load_pdfs_standalone, rgba_to_image, save_pdf_standalone, LoadResult, PageItem as ProcPageItem,
    PdfProcessor,
};
use slint::{ModelRc, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

slint::include_modules!();

fn show_error(ui: &AppWindow, msg: &str) {
    ui.set_error_message(msg.into());
    ui.set_error_visible(true);
}

fn update_ui(ui: &AppWindow, proc: &PdfProcessor) {
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
            preview: p.preview.clone(),
        })
        .collect();

    ui.set_pdf_files(ModelRc::new(VecModel::from(pdf_files)));
    ui.set_page_list(ModelRc::new(VecModel::from(page_list)));
}

fn apply_load_result(proc: &mut PdfProcessor, load_result: LoadResult) {
    for f in load_result.files {
        proc.files.push(pdf_processor::PdfFile {
            path: f.path,
            filename: f.filename,
            page_count: f.page_count,
        });
    }
    for p in load_result.pages {
        let preview = rgba_to_image(&p.rgba, p.width, p.height);
        proc.pages.push(ProcPageItem {
            source_path: p.source_path,
            source_file: p.source_file,
            page_number: p.page_number,
            display_text: p.display_text,
            preview,
            rotation: 0,
        });
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ui = AppWindow::new()?;
    let processor = Rc::new(RefCell::new(PdfProcessor::new()));

    // Handle upload button click
    let ui_weak = ui.as_weak();
    let processor_clone = processor.clone();
    ui.on_upload_clicked(move || {
        if let Some(files) = rfd::FileDialog::new()
            .add_filter("PDF Files", &["pdf"])
            .pick_files()
        {
            let paths: Vec<String> = files
                .iter()
                .map(|f| f.to_string_lossy().to_string())
                .collect();

            let result = load_pdfs_standalone(&paths);

            if let Some(ui) = ui_weak.upgrade() {
                match result {
                    Ok(load_result) => {
                        let mut proc = processor_clone.borrow_mut();
                        apply_load_result(&mut proc, load_result);
                        update_ui(&ui, &proc);
                    }
                    Err(e) => {
                        show_error(&ui, &e);
                    }
                }
            }
        }
    });

    // Handle delete page
    let ui_weak = ui.as_weak();
    let processor_clone = processor.clone();
    ui.on_delete_page(move |index| {
        let mut proc = processor_clone.borrow_mut();
        proc.remove_page(index as usize);

        if let Some(ui) = ui_weak.upgrade() {
            update_ui(&ui, &proc);
        }
    });

    // Handle move page (drag-and-drop reorder)
    let ui_weak = ui.as_weak();
    let processor_clone = processor.clone();
    ui.on_move_page(move |from, to| {
        let mut proc = processor_clone.borrow_mut();
        proc.move_page(from as usize, to as usize);

        if let Some(ui) = ui_weak.upgrade() {
            update_ui(&ui, &proc);
        }
    });

    // Handle convert (save) button click
    let ui_weak = ui.as_weak();
    let processor_clone = processor.clone();
    ui.on_convert_clicked(move || {
        let proc = processor_clone.borrow();
        if proc.pages.is_empty() {
            if let Some(ui) = ui_weak.upgrade() {
                show_error(&ui, "No pages to save");
            }
            return;
        }

        if let Some(path) = rfd::FileDialog::new()
            .add_filter("PDF Files", &["pdf"])
            .set_file_name("merged.pdf")
            .save_file()
        {
            let output = path.to_string_lossy().to_string();

            let pages_data: Vec<(String, usize, i32)> = proc
                .pages
                .iter()
                .map(|p| (p.source_path.clone(), p.page_number, p.rotation))
                .collect();

            drop(proc);

            let result: Result<(), String> =
                save_pdf_standalone(&pages_data, &output).map_err(|e| e.to_string());

            if let Some(ui) = ui_weak.upgrade() {
                match result {
                    Ok(_) => {
                        let mut proc = processor_clone.borrow_mut();
                        proc.clear();
                        update_ui(&ui, &proc);
                    }
                    Err(e) => {
                        show_error(&ui, &e);
                    }
                }
            }
        }
    });

    // Handle rotate page
    let ui_weak = ui.as_weak();
    let processor_clone = processor.clone();
    ui.on_rotate_page(move |index| {
        let mut proc = processor_clone.borrow_mut();
        proc.rotate_page(index as usize);

        if let Some(ui) = ui_weak.upgrade() {
            update_ui(&ui, &proc);
        }
    });

    // Handle clear button click
    let ui_weak = ui.as_weak();
    let processor_clone = processor.clone();
    ui.on_clear_clicked(move || {
        let mut proc = processor_clone.borrow_mut();
        proc.clear();
        if let Some(ui) = ui_weak.upgrade() {
            update_ui(&ui, &proc);
        }
    });

    ui.run()?;
    Ok(())
}
