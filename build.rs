fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/glyph.ico");
        resource.set("ProductName", "Glyph");
        resource.set("FileDescription", "Glyph Markdown Editor");
        resource.compile().expect("windows resource");
    }
    println!("cargo:rerun-if-changed=assets/glyph.ico");
}
