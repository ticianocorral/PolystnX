//! Despeja páginas de um PDF como PNGs usando o parser do app —
//! ferramenta de diagnóstico (dev).
//!
//! Usage: cargo run -p polystnx-app --example manual_dump -- <pdf> <página inicial> <página final> <out-prefix>
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let pdf = PathBuf::from(&args[1]);
    let from: usize = args[2].parse().unwrap();
    let to: usize = args[3].parse().unwrap();
    let prefix = &args[4];
    let pages = polystnx_app::manual::page_count(&pdf).unwrap();
    println!("páginas: {pages}");
    for p in from..=to.min(pages) {
        match polystnx_app::manual::render_page(&pdf, p, 800) {
            Ok(Some(img)) => {
                let out = format!("{prefix}-p{p:03}.png");
                image::save_buffer(&out, &img.rgba, img.w, img.h, image::ColorType::Rgba8).unwrap();
                println!("p{p}: {}x{} -> {out}", img.w, img.h);
            }
            Ok(None) => println!("p{p}: sem imagem"),
            Err(e) => println!("p{p}: erro {e}"),
        }
    }
}
