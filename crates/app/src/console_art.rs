//! The console's own brand images, baked into the binary (plan revision:
//! "console.png e console-tag.png devem estar internos no app, sem precisar
//! de ter esses arquivos na pasta") — the idle brand logo and the slot base
//! wordmark. A real file in `assets/` still wins over the built-in one, the
//! same "local file wins" convention per-game art already follows; a broken
//! override falls back to the baked-in image instead of dropping it.

use xperience_platform::Cabinet;

use crate::dirs;

/// The idle screen's brand logo (the app's `assets/console.png` on disk).
const DEFAULT_CONSOLE_LOGO: &[u8] = include_bytes!("../assets/console_logo.png");
/// The slot base wordmark (the app's `assets/console-tag.png` on disk).
const DEFAULT_CONSOLE_TAG: &[u8] = include_bytes!("../assets/console_tag.png");

/// Load both brand images into the cabinet: the idle logo and the slot tag.
/// Called once per idle visit (and the slot tag again per game launch, so a
/// direct `emu-run` shows it too) — the textures stick with the cabinet
/// across screens after that.
pub(crate) fn load_brand_images(cab: &mut Cabinet) {
    load_console_logo(cab);
    load_slot_tag(cab);
}

/// The idle screen's brand logo: an optional `assets/console.png` override,
/// the baked-in image otherwise.
fn load_console_logo(cab: &mut Cabinet) {
    let console_logo = decode_preferred(
        &dirs::assets_dir().join("console.png"),
        DEFAULT_CONSOLE_LOGO,
        "logo do console",
        640,
    );
    cab.set_console_logo(
        console_logo
            .as_ref()
            .map(|(w, h, d)| (*w, *h, d.as_slice())),
    );
}

/// The slot base wordmark: an optional `assets/console-tag.png` override,
/// the baked-in image otherwise. Also called per game launch by the
/// runner, so a direct `emu-run` shows the tag without visiting idle.
pub(crate) fn load_slot_tag(cab: &mut Cabinet) {
    let tag = decode_preferred(
        &dirs::assets_dir().join("console-tag.png"),
        DEFAULT_CONSOLE_TAG,
        "console-tag",
        1024,
    );
    cab.set_slot_tag(tag.as_ref().map(|(w, h, d)| (*w, *h, d.as_slice())));
}

/// Decode the override file when it exists, the baked-in bytes otherwise —
/// a broken override logs a warning and falls back to the baked-in image
/// rather than dropping the brand art. `max` caps the longer side at decode
/// time (SDL scales the texture at draw time).
fn decode_preferred(
    override_path: &std::path::Path,
    default_bytes: &[u8],
    name: &str,
    max: u32,
) -> Option<(u32, u32, Vec<u8>)> {
    if override_path.exists() {
        match decode_bytes_at(override_path, max) {
            Some(img) => return Some(img),
            None => log::warn!(
                "console art: {name} override {} falhou, usando a embutida",
                override_path.display()
            ),
        }
    }
    match image::load_from_memory(default_bytes) {
        Ok(img) => {
            let img = img.thumbnail(max, max).to_rgba8();
            let (w, h) = img.dimensions();
            Some((w, h, img.into_raw()))
        }
        Err(e) => {
            log::warn!("console art: {name} embutida: {e}");
            None
        }
    }
}

/// Decode one image file to tightly-packed RGBA, downscaled for memory —
/// `None` on any failure (the callers log and fall back).
fn decode_bytes_at(path: &std::path::Path, max: u32) -> Option<(u32, u32, Vec<u8>)> {
    let img = image::open(path).ok()?.thumbnail(max, max).to_rgba8();
    let (w, h) = img.dimensions();
    Some((w, h, img.into_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_brand_images_decode() {
        // The whole point of the baked-in images: they must decode and keep
        // the wordmark's 4:1 landscape shape even after the thumbnail cap.
        let logo = image::load_from_memory(DEFAULT_CONSOLE_LOGO)
            .expect("built-in console logo")
            .thumbnail(640, 640);
        let tag = image::load_from_memory(DEFAULT_CONSOLE_TAG)
            .expect("built-in console tag")
            .thumbnail(1024, 1024);
        assert!(logo.width() > logo.height() * 4);
        assert!(tag.width() > tag.height() * 4);
        // The wordmarks carry alpha (transparent margins around the text).
        assert!(image::load_from_memory(DEFAULT_CONSOLE_LOGO)
            .expect("logo")
            .to_rgba8()
            .pixels()
            .any(|p| p.0[3] == 0));
    }
}
