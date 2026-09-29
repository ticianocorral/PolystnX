fn main() {
    let dir = "vendor/snes_ntsc";
    println!("cargo:rerun-if-changed={dir}");
    cc::Build::new()
        .include(dir)
        .file(format!("{dir}/snes_ntsc.c"))
        .file(format!("{dir}/shim.c"))
        .warnings(false)
        .opt_level(2)
        .compile("snes_ntsc");
}
