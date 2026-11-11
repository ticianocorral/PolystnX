//! Salva o disco genérico do console em PNG: `cargo run -p polystnx-app
//! --example generic_disc_shot -- /tmp/disc.png "Tekken 3 (USA)"`.
fn main() -> anyhow::Result<()> {
    let mut a = std::env::args().skip(1);
    let out = a.next().unwrap_or_else(|| "/tmp/disc.png".into());
    let title = a.next().unwrap_or_else(|| "Tekken 3 (USA)".into());
    let (w, h, px) = polystnx_app::generic_disc::generic_disc(&title);
    image::save_buffer(&out, &px, w, h, image::ColorType::Rgba8)?;
    println!("{out}");
    Ok(())
}
