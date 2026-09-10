fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("evertydisplay.ico");
        res.set("FileDescription", "EvertyDisplay Background Service");
        res.set("ProductName", "EvertyDisplay");
        res.set("CompanyName", "Артур Валиев (Arthur Valiev)");
        res.set("LegalCopyright", "Copyright (C) 2026 Артур Валиев (Arthur Valiev)");
        res.set("Comments", "Created by Артур Валиев (Arthur Valiev)");
        res.set("OriginalFilename", "multitor-service.exe");
        res.set("FileVersion", env!("CARGO_PKG_VERSION"));
        res.set("ProductVersion", env!("CARGO_PKG_VERSION"));
        res.compile()
            .expect("failed to compile EvertyDisplay service Windows resources");
    }
}
