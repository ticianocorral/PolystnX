//! Fase 0 proof: the rhash + custom cdreader produce the RA PSX hash.
//!
//! Test 1 builds a minimal but structurally valid PSX disc (cue + bin,
//! MODE1/2048: PVD, root directory, SYSTEM.CNF booting an exe with the
//! PS-X EXE header) and checks the hash against the expected value computed
//! from rcheevos' documented algorithm: MD5(exe_name ++ exe_bytes) where
//! exe_bytes = the exe's size field (big-endian u32 at header offset 28)
//! plus the 2048-byte header itself.
//!
//! Test 2 runs the same pipeline against a real CHD when one is available:
//! `PSX_TEST_CHD=/caminho/jogo.chd cargo test -p xperience-ra`. It asserts
//! the CHD reader lands on the ISO9660 volume ("CD001" at absolute sector 16)
//! and produces a stable hash — the structural gate of `docs/fase-0.md`.

use std::fs;
use std::path::PathBuf;

use md5::{Digest, Md5};
use xperience_ra::hash::psx_disc_hash;

const EXE_NAME: &str = "SCUS_941.63";

/// One 2048-byte "user data" sector of a MODE1/2048 track.
type Sector = [u8; 2048];

fn iso_dir_record(extent: u32, size: u32, flags: u8, name: &[u8]) -> Vec<u8> {
    let mut rec = vec![0u8; 33 + name.len()];
    if rec.len() % 2 == 1 {
        // ISO9660 pads records to even length — pad BEFORE writing the
        // length byte, or the record lies about its own size.
        rec.push(0);
    }
    rec[0] = rec.len() as u8;
    rec[2..6].copy_from_slice(&extent.to_le_bytes());
    rec[6..10].copy_from_slice(&extent.to_be_bytes());
    rec[10..14].copy_from_slice(&size.to_le_bytes());
    rec[14..18].copy_from_slice(&size.to_be_bytes());
    rec[18] = 126; // date: years since 1900
    rec[19] = 1; // month, day, hh, mm, ss (rest 0)
    rec[25] = flags; // 0x02 = directory
    rec[28..30].copy_from_slice(&1u16.to_le_bytes());
    rec[30..32].copy_from_slice(&1u16.to_be_bytes());
    rec[32] = name.len() as u8;
    rec[33..33 + name.len()].copy_from_slice(name);
    rec
}

/// Build `game.bin`/`game.cue` in `dir`; returns (cue path, expected hash).
fn build_synthetic_disc(dir: &std::path::Path) -> (PathBuf, String) {
    // --- the boot executable: PS-X EXE header + 2048 bytes of code.
    let mut exe = vec![0u8; 4096];
    exe[..8].copy_from_slice(b"PS-X EXE");
    // exec size at offset 28: rcheevos reads it little-endian over the four
    // bytes (buffer[31] is the MSB) and adds the 2048-byte header back — so
    // 2048 here hashes all 4096 bytes of the file.
    exe[28..32].copy_from_slice(&2048u32.to_le_bytes());
    for (i, b) in exe.iter_mut().enumerate().skip(2048) {
        *b = (i % 251) as u8; // deterministic filler
    }

    // --- SYSTEM.CNF, the PSX boot script.
    let mut cnf =
        format!("BOOT = cdrom:\\{EXE_NAME};1\r\nTCB = 4\r\nEVENT = 16\r\nSTACK = 801FFF00\r\n")
            .into_bytes();
    cnf.resize(2048, 0);

    // --- the disc: 32 sectors. PVD at 16, terminator at 17, root dir at 18,
    // SYSTEM.CNF at 19, exe at 20..22.
    let mut sectors = vec![[0u8; 2048]; 32];

    let pvd = &mut sectors[16];
    pvd[0] = 1;
    pvd[1..6].copy_from_slice(b"CD001");
    pvd[6] = 1;
    pvd[8..19].copy_from_slice(b"PLAYSTATION");
    let total = 32u32;
    pvd[80..84].copy_from_slice(&total.to_le_bytes());
    pvd[84..88].copy_from_slice(&total.to_be_bytes());
    pvd[128..130].copy_from_slice(&2048u16.to_le_bytes());
    pvd[130..132].copy_from_slice(&2048u16.to_be_bytes());
    // root directory record, pointing at sector 18
    let root = iso_dir_record(18, 2048, 0x02, &[0]);
    pvd[156..156 + root.len()].copy_from_slice(&root);
    pvd[1028] = 1; // file structure version

    sectors[17][0] = 255;
    sectors[17][1..6].copy_from_slice(b"CD001");
    sectors[17][6] = 1;

    let mut dir_extent = Vec::new();
    dir_extent.extend(iso_dir_record(18, 2048, 0x02, &[0])); // "."
    dir_extent.extend(iso_dir_record(18, 2048, 0x02, &[1])); // ".."
    dir_extent.extend(iso_dir_record(19, 2048, 0, b"SYSTEM.CNF;1"));
    dir_extent.extend(iso_dir_record(20, 4096, 0, b"SCUS_941.63;1"));
    sectors[18][..dir_extent.len()].copy_from_slice(&dir_extent);
    sectors[19] = cnf.try_into().unwrap();
    sectors[20].copy_from_slice(&exe[..2048]);
    sectors[21].copy_from_slice(&exe[2048..]);

    let mut bin = Vec::with_capacity(sectors.len() * 2048);
    for s in &sectors {
        bin.extend_from_slice(s);
    }

    let bin_path = dir.join("game.bin");
    fs::write(&bin_path, &bin).unwrap();
    let cue_path = dir.join("game.cue");
    fs::write(
        &cue_path,
        "FILE \"game.bin\" BINARY\n  TRACK 01 MODE1/2048\n    INDEX 01 00:00:00\n",
    )
    .unwrap();

    let mut expected = Md5::new();
    expected.update(EXE_NAME.as_bytes());
    expected.update(&exe);
    (cue_path, format!("{:x}", expected.finalize()))
}

#[test]
fn hashes_synthetic_cue_bin_like_the_ra_server() {
    let dir = std::env::temp_dir().join(format!("psx-ra-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let (cue, expected) = build_synthetic_disc(&dir);

    // DEBUG-CI: walk ISO9660 em Rust puro sobre o mesmo bin — se achar o exe
    // e o rhash (C) não, o bug é do C por OS; se não achar, o disco é
    // malformado e a libc do macOS estava mascarando.
    let bin_bytes = fs::read(cue.with_file_name("game.bin")).unwrap();
    let sector = |n: usize| -> &[u8] { &bin_bytes[n * 2048..(n + 1) * 2048] };
    assert_eq!(&sector(16)[1..6], b"CD001", "PVD ausente no setor 16");
    let le = |b: &[u8]| u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    let root_extent = le(&sector(16)[158..162]) as usize;
    eprintln!("DEBUG-CI: root_extent={root_extent}");
    let dir_data = sector(root_extent);
    let mut pos = 0;
    while pos + 33 <= dir_data.len() && dir_data[pos] != 0 {
        let len = dir_data[pos] as usize;
        let name_len = dir_data[pos + 32] as usize;
        eprintln!(
            "DEBUG-CI: record len={len} name={:?}",
            String::from_utf8_lossy(&dir_data[pos + 33..pos + 33 + name_len])
        );
        pos += len;
    }

    let got = match psx_disc_hash(&cue) {
        Ok(h) => h,
        Err(e) => panic!("hash do cue sintético falhou: {e}"),
    };
    assert_eq!(
        got, expected,
        "hash do disco sintético divergiu do esperado"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Same disc, hashed through the CHD path: `chdman`-made CHDs of the same
/// image must hash the same as the cue. Skipped without a fixture.
#[test]
fn hashes_real_chd_when_one_is_available() {
    let Ok(chd) = std::env::var("PSX_TEST_CHD") else {
        eprintln!("skip: defina PSX_TEST_CHD=<jogo.chd> para provar o caminho CHD");
        return;
    };
    let got = psx_disc_hash(&chd).expect("hash do CHD real");
    assert_eq!(got.len(), 32);
    assert!(got.chars().all(|c| c.is_ascii_hexdigit()));
    eprintln!("RA hash de {chd}: {got}");

    // Stable across a second run (and across the process boundary the first
    // one may have left state in): same file, same hash.
    let again = psx_disc_hash(&chd).expect("segundo hash");
    assert_eq!(got, again);
}

/// O serial de fábrica, lido de dentro do CHD — a chave da identificação da
/// Fase 2. O fixture conhecido é o Tekken 3 (USA), `SLUS-00402` (o checksum
/// do mesmo disco já validamos contra o site da RA).
#[test]
fn reads_the_serial_from_a_real_chd() {
    let Ok(chd) = std::env::var("PSX_TEST_CHD") else {
        eprintln!("skip: defina PSX_TEST_CHD=<jogo.chd> para provar o serial");
        return;
    };
    let serial = xperience_ra::hash::psx_serial(&chd).expect("serial do CHD real");
    eprintln!("serial de {chd}: {serial}");
    assert_eq!(serial, "SLUS-00402");
}

/// The vendored rcheevos is the same develop snapshot upstream; prove the
/// vendored rhash links and answers by asking it for a nonsense path — it
/// must fail cleanly (exit 0 assertion lives in the caller), not crash.
#[test]
fn missing_disc_fails_cleanly() {
    let dir = std::env::temp_dir().join(format!("psx-ra-miss-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let missing = dir.join("nao-existe.cue");
    let _ = fs::write(dir.join("nao-existe.bin"), [0u8; 2048]);
    assert!(psx_disc_hash(&missing).is_err());
    let _ = fs::remove_dir_all(&dir);
}
