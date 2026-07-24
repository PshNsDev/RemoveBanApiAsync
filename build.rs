fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        println!("cargo:rerun-if-changed={}/manifest.xml", manifest_dir);
        let mut res = winres::WindowsResource::new();
        res.set_icon("icon.ico");
        res.set_manifest_file("manifest.xml");
        res.compile().unwrap();
    }
}