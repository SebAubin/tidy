//! Windows: the icon and version info Explorer, the taskbar and Alt-Tab show, built into tidy.exe.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon/Tidy.ico");
        res.set("ProductName", "Tidy");
        res.set("FileDescription", "Tidy");
        res.set("LegalCopyright", "© 2026 Sebastien Aubin. MIT License.");
        if let Err(e) = res.compile() {
            println!("cargo:warning=no Windows resources (icon) compiled: {e}");
        }
    }
    println!("cargo:rerun-if-changed=assets/icon/Tidy.ico");
}
