//! Decode de imagem com teto: capas, artes, prints e badges vêm do disco
//! (ou da rede, no caso dos badges do RA) e um PNG/JPEG de poucos MB pode
//! declarar dimensões gigantes — o `max_alloc` padrão do `image` deixa
//! passar ~512 MB, o bastante para travar a estante e estourar a RAM do
//! Steam Deck. Tudo que abre imagem de arquivo passa por aqui.

use std::path::Path;

use image::{DynamicImage, ImageReader, ImageResult, Limits};

/// Lado máximo de uma arte/capa/print/página de manual: 8K cobre scans de
/// contracapa em alta resolução (tudo é reduzido para ≤ 2048 na tela); com
/// o `max_alloc` em RGBA8 o pior caso fica em ~256 MB transitórios, e uma
/// imagem de 16 bits por canal desse tamanho é recusada.
pub const ART_MAX_SIDE: u32 = 8192;
/// Lado máximo de um badge do RA (os de verdade têm 64×64).
pub const BADGE_MAX_SIDE: u32 = 512;

fn limits(max_side: u32) -> Limits {
    let mut l = Limits::default();
    l.max_image_width = Some(max_side);
    l.max_image_height = Some(max_side);
    // RGBA8 no lado máximo, mais folga para o decoder. Uma imagem de 16
    // bits por canal nesse tamanho é recusada — nenhuma arte real precisa.
    l.max_alloc = Some(u64::from(max_side) * u64::from(max_side) * 4 + (16 << 20));
    l
}

/// Abre `path` recusando imagens com algum lado acima de `max_side`.
pub fn open_limited(path: &Path, max_side: u32) -> ImageResult<DynamicImage> {
    let mut reader = ImageReader::open(path)?.with_guessed_format()?;
    reader.limits(limits(max_side));
    reader.decode()
}

/// Decodifica bytes em memória (ex.: a imagem embutida num PDF de manual)
/// com o mesmo teto de [`open_limited`].
pub fn decode_limited(bytes: &[u8], max_side: u32) -> ImageResult<DynamicImage> {
    let mut reader = ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    reader.limits(limits(max_side));
    reader.decode()
}

/// Só confere as dimensões declaradas de uma imagem em memória (sem
/// decodificar os pixels): `true` se cabe em `max_side`.
pub fn fits(bytes: &[u8], max_side: u32) -> bool {
    ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_dimensions().ok())
        .is_some_and(|(w, h)| w <= max_side && h <= max_side)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::new(w, h);
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn oversized_images_are_refused() {
        assert!(fits(&png(64, 64), BADGE_MAX_SIDE));
        assert!(!fits(&png(600, 10), BADGE_MAX_SIDE));
        assert!(!fits(b"not an image", BADGE_MAX_SIDE));

        let dir = std::env::temp_dir().join(format!("polystnx-img-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let big = dir.join("big.png");
        std::fs::write(&big, png(600, 10)).unwrap();
        assert!(open_limited(&big, BADGE_MAX_SIDE).is_err());
        assert!(open_limited(&big, ART_MAX_SIDE).is_ok());
        assert!(decode_limited(&png(600, 10), BADGE_MAX_SIDE).is_err());
        assert!(decode_limited(&png(64, 64), BADGE_MAX_SIDE).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
