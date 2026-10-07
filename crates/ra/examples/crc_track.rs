//! Diagnóstico: CRC32 da Track 1 de um CHD no formato do .bin do Redump
//! (primeiros 2352 bytes de cada frame). Temporário.
//!
//!   cargo run -p polystnx-ra --example crc_track -- <file.chd> <frames-track1>

use std::fs::File;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: crc_track <chd> <frames>")?;
    let frames: u64 = std::env::args().nth(2).unwrap_or_default().parse()?;
    let mut chd = chd::Chd::open(File::open(&path)?, None)?;
    let hunk_size = chd.header().hunk_size() as usize;
    let frames_per_hunk = hunk_size / 2448;
    let hunks = (frames as usize).div_ceil(frames_per_hunk);

    // CRC-32 (IEEE, tabela padrão)
    let mut table = [0u32; 256];
    for (i, t) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *t = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    let mut buf = vec![0u8; hunk_size];
    let mut compressed = vec![0u8; hunk_size * 2];
    let mut done = 0u64;
    for h in 0..hunks {
        let mut hunk = chd.hunk(h as u32)?;
        hunk.read_hunk_in(&mut compressed, &mut buf)?;
        for f in 0..frames_per_hunk {
            let fi = (h * frames_per_hunk + f) as u64;
            if fi >= frames {
                break;
            }
            for &b in &buf[f * 2448..f * 2448 + 2352] {
                crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
            }
            done += 1;
        }
    }
    crc ^= 0xFFFF_FFFF;
    println!(
        "{path}: {done} frames → CRC32 {:08X} (bytes {})",
        crc,
        done * 2352
    );
    Ok(())
}
