//! Download the No-Intro DAT (`nointro.dat`, the file that gives games
//! their canonical titles — see `dirs::nointro_dat_path`). No-Intro's own
//! DAT-o-MATIC is a click-through flow with no stable URL, so the setup
//! screen fetches from the **libretro-database** mirror instead — the same
//! source (and CC BY-SA licence, see `THIRD-PARTY-NOTICES.md`) the embedded
//! cheat database already comes from, auto-updated as No-Intro revises
//! entries. Its flavour is clrmamepro text, which `NoIntroDat::load`
//! parses natively alongside the XML a manual DAT-o-MATIC export produces.
//! Same thread + `mpsc` shape as `core_update`'s worker, sharing its
//! `CoreUpdateMsg` (Progress/Done/Failed are not core-specific).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;

use crate::core_update::CoreUpdateMsg;

/// Where the setup screen / this module fetch `nointro.dat` from — the
/// libretro-database mirror's **Redump `Sony - PlayStation`** file, on the
/// repo's default branch. É o DAT que carrega o serial de fábrica por jogo
/// (`serial "SLUS-00402"`), a chave pela qual os discos deste app já se
/// identificam — o nome canônico "Tekken 3 (USA)" casa sem ler o disco.
pub fn dat_download_url() -> &'static str {
    "https://raw.githubusercontent.com/libretro/libretro-database/master/metadat/redump/Sony%20-%20PlayStation.dat"
}

/// Where the DAT is installed — `dirs::nointro_dat_path()`, restated here
/// so the caller can decide "is it already there" and this module stays the
/// only writer.
pub fn dat_path() -> PathBuf {
    crate::dirs::nointro_dat_path()
}

/// Whether a usable DAT is already installed (exists and is non-trivial —
/// guards against a zero-byte stub left by a failed download).
pub fn dat_installed() -> bool {
    std::fs::metadata(dat_path())
        .map(|m| m.len() > 1024)
        .unwrap_or(false)
}

/// Download the DAT to `dest`, replacing it atomically (write a `.part`
/// sibling, rename over the real path only once the whole body arrived and
/// parses as XML). Meant to run on a background thread; reports on `tx` in
/// the same shape the core downloader uses.
pub fn download_and_install(dest: &Path, tx: &Sender<CoreUpdateMsg>) {
    let msg = match try_download(dest, tx) {
        Ok(()) => CoreUpdateMsg::Done,
        Err(e) => CoreUpdateMsg::Failed(e),
    };
    let _ = tx.send(msg);
}

fn try_download(dest: &Path, tx: &Sender<CoreUpdateMsg>) -> Result<(), String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .build();
    let resp = agent
        .get(dat_download_url())
        .call()
        .map_err(|e| e.to_string())?;
    let total = resp
        .header("Content-Length")
        .and_then(|s| s.parse::<u64>().ok());

    let mut reader = resp.into_reader();
    let mut bytes = Vec::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        let _ = tx.send(CoreUpdateMsg::Progress {
            downloaded: bytes.len() as u64,
            total,
        });
    }

    // Cheap sanity check before touching the real path: it must at least
    // look like one of the two DAT flavours `NoIntroDat::load` accepts —
    // clrmamepro (what this mirror serves) or Logiqx XML (a DAT-o-MATIC
    // export). Anything else (an HTML 404 page is the likely impostor) is a
    // failure, not a DAT.
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]);
    let looks_like_dat =
        head.contains("clrmamepro") || head.contains("<?xml") || head.contains("<datafile");
    if !looks_like_dat {
        return Err("resposta não parece um DAT (clrmamepro/XML)".to_string());
    }

    let part = dest.with_extension("dat.part");
    std::fs::write(&part, &bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&part, dest).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_is_the_libretro_database_mirror() {
        assert!(dat_download_url()
            .starts_with("https://raw.githubusercontent.com/libretro/libretro-database/"));
        assert!(dat_download_url().ends_with("Sony%20-%20PlayStation.dat"));
    }

    /// Hits the real network — not run by default (`cargo test` skips
    /// `#[ignore]`d tests), only a manual sanity check that the mirror is
    /// up and serving something `NoIntroDat::load` can actually parse:
    /// `cargo test -p polystnx-app --lib dat_update -- --ignored`
    #[test]
    #[ignore]
    fn downloads_and_parses_the_real_dat() {
        let (tx, rx) = std::sync::mpsc::channel();
        let dest = std::env::temp_dir().join("polystnx-dat-update-test.dat");
        let worker_dest = dest.clone();
        std::thread::spawn(move || download_and_install(&worker_dest, &tx));
        loop {
            match rx.recv().unwrap() {
                CoreUpdateMsg::Progress { downloaded, .. } => {
                    log::info!("downloaded {} KB", downloaded / 1024)
                }
                CoreUpdateMsg::Done => break,
                CoreUpdateMsg::Failed(e) => panic!("download failed: {e}"),
            }
        }
        let dat = polystnx_domain::NoIntroDat::load(&dest).unwrap();
        // Chrono Trigger (USA)'s headerless CRC32 in this mirror.
        assert_eq!(
            dat.lookup("2D206BF7").map(|i| i.name.as_str()),
            Some("Chrono Trigger (USA)")
        );
        let _ = std::fs::remove_file(&dest);
    }
}
