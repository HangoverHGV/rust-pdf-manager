pub fn merge_pdfs(files: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    println!("Merging PDFs: {:?}", files);
    // Your merge logic here
    Ok(())
}

pub fn split_pdf(file: &str, pages: &[usize]) -> Result<(), Box<dyn std::error::Error>> {
    println!("Splitting PDF: {}", file);
    // Your split logic here
    Ok(())
}