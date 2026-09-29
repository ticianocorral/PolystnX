// Embeds the app icon into every binary this crate builds on Windows (there's
// no per-[[bin]] hook for this, so the CLI tools get the icon too — harmless).
// No-op everywhere else.
fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("../../packaging/windows/AppIcon.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=failed to embed Windows icon: {e}");
        }
    }
}
