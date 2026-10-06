//! Download/update the PCSX Rearmed libretro core from the official libretro
//! buildbot (`buildbot.libretro.com`) — a settings-screen action. The core is
//! still never bundled with the app itself (GPL — see the license text,
//! see `THIRD-PARTY-NOTICES.md`); this just automates what used to be "drop
//! the file into `core/` by hand".

use std::io::Read;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::Duration;

/// The core file's name on this platform — what the app looks for in
/// `core/` at launch, and what a download is saved as.
pub fn core_file_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "pcsx_rearmed_libretro.dylib"
    } else if cfg!(target_os = "windows") {
        "pcsx_rearmed_libretro.dll"
    } else {
        "pcsx_rearmed_libretro.so"
    }
}

/// The buildbot URL for this platform/architecture's latest nightly build —
/// `None` for a combination with no known build (e.g. Linux on arm64), where
/// the settings screen falls back to "baixe manualmente".
pub fn core_download_url() -> Option<&'static str> {
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        other => other,
    };
    match (std::env::consts::OS, arch) {
        ("macos", "arm64") => Some(
            "https://buildbot.libretro.com/nightly/apple/osx/arm64/latest/pcsx_rearmed_libretro.dylib.zip",
        ),
        ("macos", "x86_64") => Some(
            "https://buildbot.libretro.com/nightly/apple/osx/x86_64/latest/pcsx_rearmed_libretro.dylib.zip",
        ),
        ("windows", "x86_64") => Some(
            "https://buildbot.libretro.com/nightly/windows/x86_64/latest/pcsx_rearmed_libretro.dll.zip",
        ),
        ("linux", "x86_64") => Some(
            "https://buildbot.libretro.com/nightly/linux/x86_64/latest/pcsx_rearmed_libretro.so.zip",
        ),
        _ => None,
    }
}

/// Progress/result reported back from the download thread.
pub enum CoreUpdateMsg {
    Progress { downloaded: u64, total: Option<u64> },
    Done,
    Failed(String),
}

/// The cabinet's nameplate text (plan revision: "mostrar versao do app e
/// versao do PCSX Rearmed, onde esta o nome do app na tv") — the app's own
/// version on the first line and, if a core is installed, the core's
/// version on a second line below it (`draw_brand` splits on '\n'). Lives
/// here rather than in the app crate so the idle screen can rebuild it the
/// moment a setup-screen download finishes — the nameplate used to stay
/// without the PCSX Rearmed line until the player left the screen.
/// `Core::load` only resolves symbols and reads that info (no `retro_
/// init`), so peeking at it here and dropping the `Core` right after is
/// cheap and side-effect-free.
pub fn nameplate_text(core_path: Option<&std::path::Path>) -> String {
    let app_version = env!("CARGO_PKG_VERSION");
    let core_version = core_path
        .and_then(|p| polystnx_emulation::Core::load(p).ok())
        .map(|c| c.system_version().to_string())
        .filter(|v| !v.is_empty());
    match core_version {
        Some(v) => format!(
            "{} v{app_version}\nPCSX Rearmed {v}",
            polystnx_platform::BRAND
        ),
        None => format!("{} v{app_version}", polystnx_platform::BRAND),
    }
}

/// The default core location, resolved the same way the app does at
/// startup (`--core`/`$PSX_XPERIENCE_CORE` aside) — used to refresh the
/// nameplate after an in-screen download.
pub fn default_core_path() -> Option<std::path::PathBuf> {
    let p = crate::dirs::core_dir().join(core_file_name());
    p.is_file().then_some(p)
}

/// O commit embutido no `library_version` do core — o PCSX Rearmed se
/// apresenta como `r26 c881679` (upstream + commit). `None` se o core não
/// reportar um sha.
pub fn commit_from_version(version: &str) -> Option<String> {
    let last = version.split_whitespace().next_back()?;
    let looks_like_sha =
        (7..=40).contains(&last.len()) && last.chars().all(|c| c.is_ascii_hexdigit());
    looks_like_sha.then(|| last.to_ascii_lowercase())
}

/// The commit the core at `core_path` was built from — `None` if it doesn't
/// load or doesn't report one. Like `nameplate_text`, this `Core::load`s
/// (no `retro_init`), so it's read-only and cheap; safe to call from a
/// background thread (`update_check` does).
pub fn core_commit(core_path: &std::path::Path) -> Option<String> {
    let core = polystnx_emulation::Core::load(core_path).ok()?;
    commit_from_version(core.system_version())
}

/// Download `url` and unzip the core into `dest_dir/core_file_name()`,
/// reporting progress on `tx`. Meant to run on a background thread (the app's
/// standard spawn + `mpsc` + per-frame `try_recv()` pattern) — a
/// `.call()`/full read can take a few seconds.
pub fn download_and_install(url: &str, dest_dir: &Path, tx: &Sender<CoreUpdateMsg>) {
    let msg = match try_download(url, dest_dir, tx) {
        Ok(()) => CoreUpdateMsg::Done,
        Err(e) => CoreUpdateMsg::Failed(e),
    };
    let _ = tx.send(msg);
}

fn try_download(url: &str, dest_dir: &Path, tx: &Sender<CoreUpdateMsg>) -> Result<(), String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .build();
    let resp = agent.get(url).call().map_err(|e| e.to_string())?;
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

    let core_ext = Path::new(core_file_name())
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut index = None;
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let matches = entry
            .name()
            .rsplit('.')
            .next()
            .is_some_and(|ext| ext.eq_ignore_ascii_case(core_ext));
        if matches {
            index = Some(i);
            break;
        }
    }
    let index = index.ok_or_else(|| format!("nenhum arquivo .{core_ext} dentro do zip"))?;
    let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    entry.read_to_end(&mut out).map_err(|e| e.to_string())?;

    std::fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    std::fs::write(dest_dir.join(core_file_name()), out).map_err(|e| e.to_string())?;
    Ok(())
}
