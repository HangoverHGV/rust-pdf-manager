fn main() {
    println!("cargo:rerun-if-changed=ui/");
    println!("cargo:rerun-if-changed=build.rs");

    slint_build::compile("ui/main.slint").expect("Slint build failed");

    // Embed icon and metadata on Windows builds
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        // Convert Logo.png to .ico for Windows resource embedding
        let png_path = std::path::Path::new("ui/images/Logo.png");
        let ico_path = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("Logo.ico");

        let img = image::open(png_path).expect("Failed to open Logo.png");
        let resized = img.resize(256, 256, image::imageops::FilterType::Lanczos3);

        let mut ico_file = std::fs::File::create(&ico_path).expect("Failed to create ico file");
        resized
            .write_to(&mut ico_file, image::ImageFormat::Ico)
            .expect("Failed to write ico file");

        let mut res = winresource::WindowsResource::new();
        res.set_icon(ico_path.to_str().unwrap());
        res.set("ProductName", "PDF Manager");
        res.set("FileDescription", "PDF Manager");
        res.compile().expect("Failed to compile Windows resources");
    }
}
