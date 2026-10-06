//! Startup update checks (plan revision: "verificar se tem update para nova
//! versao" / "verificar se o PCSX Rearmed esta atualizado") — both best-effort and
//! silent on any network hiccup, gated by `Config::check_updates_on_start`.
//! Meant to run on a background thread (`std::thread::spawn`), reporting
//! back through an `mpsc::Sender` the same shape `core_update`'s download
//! worker already uses — `idle::run` drains it with `try_recv()`.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;

use serde::Deserialize;

const RELEASES_API: &str = "https://api.github.com/repos/ticianocorral/polystnx/releases/latest";

/// What's worth telling the player about at startup — `idle::run` only ever
/// receives one of these when there's actually something to say (see
/// `check`), so both fields are never "empty" at once.
pub struct UpdateNotice {
    /// The newer release, if GitHub has one — tag + changelog + the asset
    /// for this platform, everything the update screen needs (plan
    /// revision: "quando for update do app mostrar o changelog e o botão
    /// de atualizar").
    pub app_update: Option<AppUpdate>,
    /// Whether the installed PCSX Rearmed core was built from a commit older
    /// than `libretro/pcsx_rearmed` HEAD — `false` when the core doesn't
    /// report its commit (hand-placed cores are never flagged).
    pub core_stale: bool,
}

/// Uma atualização do app disponível no GitHub.
#[derive(Clone)]
pub struct AppUpdate {
    pub tag: String,
    /// O `body` do release (o changelog em markdown — mostrado como texto).
    pub changelog: String,
    /// URL do asset desta plataforma (dmg/AppImage/zip), se houver.
    pub asset_url: Option<String>,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout(Duration::from_secs(8))
        .build()
}

/// `true` if `libretro/pcsx_rearmed` HEAD moved past the commit the installed
/// core was built from — `false` with nothing to compare against (core that
/// doesn't report a commit) or on any network hiccup, never a false "yes".
/// (plan revision: "considere apenas novos commits (igual ao snes)" — the
/// ETag-of-the-zip check flagged the buildbot's nightly RECOMPILES, que
/// acontecem até sem commit novo, e gritava todo dia.)
const PCSX_COMMIT_API: &str = "https://api.github.com/repos/libretro/pcsx_rearmed/commits/HEAD";

fn core_is_stale(core_dir: &Path) -> bool {
    let core_path = core_dir.join(crate::core_update::core_file_name());
    let Some(installed) = crate::core_update::core_commit(&core_path) else {
        return false;
    };
    let Some(head) = latest_pcsx_commit() else {
        return false;
    };
    installed != head
}

/// `libretro/pcsx_rearmed` HEAD as a short lowercase sha — the exact
/// identifier the core embeds in its `library_version`. `None` on any
/// network/parse hiccup.
fn latest_pcsx_commit() -> Option<String> {
    let resp = agent()
        .get(PCSX_COMMIT_API)
        .set("User-Agent", "polystnx-update-check")
        .call()
        .ok()?;
    let head: GhCommit = resp.into_json().ok()?;
    let short = head.sha.get(..7)?;
    short
        .chars()
        .all(|c| c.is_ascii_hexdigit())
        .then(|| short.to_ascii_lowercase())
}

#[derive(Deserialize)]
struct GhCommit {
    sha: String,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

/// O asset do release para a plataforma em que roda — `-macos.dmg`,
/// `-linux-*.AppImage` ou `-win.zip` pelo nome (o CI publica exatamente
/// esses sufixos).
fn asset_for_platform(assets: &[GhAsset]) -> Option<String> {
    let (needle, ext): (&str, &str) = if cfg!(target_os = "macos") {
        ("-macos", ".dmg")
    } else if cfg!(target_os = "linux") {
        ("-linux-", ".AppImage")
    } else {
        ("-win", ".zip")
    };
    assets
        .iter()
        .find(|a| a.name.contains(needle) && a.name.ends_with(ext))
        .map(|a| a.browser_download_url.clone())
}

/// `Some(tag)` if GitHub's latest release is newer than `current` — `None`
/// on any network/parse hiccup, or when already current. Versions are
/// compared as dot-separated integers (a leading `v` is stripped first), not
/// full semver — good enough for this project's plain `MAJOR.MINOR.PATCH`
/// tags.
fn newer_release(current: &str) -> Option<AppUpdate> {
    let resp = agent()
        .get(RELEASES_API)
        .set("User-Agent", "psx-polystnx-update-check")
        .call()
        .ok()?;
    let release: GhRelease = resp.into_json().ok()?;
    let tag = release.tag_name.trim_start_matches('v');
    if parse_version(tag) <= parse_version(current) {
        return None;
    }
    Some(AppUpdate {
        tag: release.tag_name,
        changelog: release.body,
        asset_url: asset_for_platform(&release.assets),
    })
}

fn parse_version(v: &str) -> Vec<u32> {
    v.split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

/// Run both checks and report back on `tx` — but only if there's actually
/// something to say; a fully up-to-date app/core sends nothing at all, so
/// the receiver just sees the channel go quiet rather than an explicit
/// "you're fine" message.
pub fn check(core_dir: PathBuf, app_version: String, tx: Sender<UpdateNotice>) {
    let app_update = newer_release(&app_version);
    let core_stale = core_is_stale(&core_dir);
    if app_update.is_some() || core_stale {
        let _ = tx.send(UpdateNotice {
            app_update,
            core_stale,
        });
    }
}

/// Baixa o asset da atualização para `dest_dir` (o update view chama em
/// worker; progresso em percentual no canal). Devolve o caminho do arquivo.
pub fn download_asset(url: &str, dest_dir: &Path, tx: &Sender<u32>) -> Result<PathBuf, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(600))
        .build();
    let resp = agent.get(url).call().map_err(|e| e.to_string())?;
    let total = resp
        .header("Content-Length")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let file_name = url.rsplit('/').next().unwrap_or("update.bin").to_string();
    std::fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    let dest = dest_dir.join(&file_name);
    let mut file = std::fs::File::create(&dest).map_err(|e| e.to_string())?;
    let mut reader = resp.into_reader();
    let mut buf = [0u8; 64 * 1024];
    let mut got: u64 = 0;
    loop {
        use std::io::Read;
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        std::io::Write::write_all(&mut file, &buf[..n]).map_err(|e| e.to_string())?;
        got += n as u64;
        if let Some(pct) = (got * 100).checked_div(total) {
            let _ = tx.send(pct.min(100) as u32);
        }
    }
    Ok(dest)
}

/// Aplica, no arranque, uma atualização baixada pela tela de update — o
/// "será atualizado ao reiniciar" (plan revision). Melhor-esforço: qualquer
/// falha loga e mantém o arquivo para tentar de novo. Antes do SDL, na
/// main do `polystnx`.
pub fn apply_pending_update() {
    let Some(dir) = crate::dirs::update_dir_opt() else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // Só o formato desta plataforma é candidato a update; qualquer outro
        // arquivo na pasta de updates é ignorado em silêncio.
        let relevante = if cfg!(target_os = "macos") {
            name.ends_with(".dmg")
        } else if cfg!(target_os = "linux") {
            name.ends_with(".AppImage")
        } else {
            false
        };
        if !relevante {
            continue;
        }
        let outcome = apply_for_platform(&path, name);
        match outcome {
            Ok(()) => {
                log::info!("update: {name} aplicado — reinicie para valer");
                let _ = std::fs::remove_file(&path);
            }
            Err(e) => log::info!("update: {name} não aplicado ({e}) — tenta no próximo arranque"),
        }
    }
}

/// macOS: monta o dmg, copia o .app por cima do bundle em execução
/// (`ditto` preserva a estrutura), desmonta. O binário em execução pode
/// ser substituído — o inode vivo continua rodando.
#[cfg(target_os = "macos")]
fn apply_macos_dmg(dmg: &Path) -> Result<(), String> {
    use std::process::Command;
    let mnt = std::env::temp_dir().join("polystnx-update-mnt");
    let _ = std::fs::remove_dir_all(&mnt);
    std::fs::create_dir_all(&mnt).map_err(|e| e.to_string())?;
    let out = Command::new("hdiutil")
        .args(["attach", "-nobrowse", "-readonly", "-mountpoint"])
        .arg(&mnt)
        .arg(dmg)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "hdiutil attach: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let result = (|| -> Result<(), String> {
        let app_bundle = std::env::current_exe()
            .ok()
            .and_then(|exe| {
                exe.ancestors()
                    .find(|a| a.extension().is_some_and(|e| e == "app"))
                    .map(|a| a.to_path_buf())
            })
            .ok_or_else(|| "não achei o .app em execução".to_string())?;
        let new_app = std::fs::read_dir(&mnt)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e == "app"))
            .ok_or_else(|| "dmg sem .app dentro".to_string())?;
        let status = Command::new("ditto")
            .arg(new_app)
            .arg(&app_bundle)
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err("ditto falhou".to_string())
        }
    })();
    let _ = Command::new("hdiutil")
        .args(["detach", "-force"])
        .arg(&mnt)
        .status();
    let _ = std::fs::remove_dir_all(&mnt);
    result
}

/// Linux (Steam Deck incluído): substitui o binário/AppImage em execução —
/// rename sobre o arquivo rodando é seguro no Linux (o inode segue vivo).
#[cfg(target_os = "linux")]
fn apply_linux_appimage(appimage: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    std::fs::copy(appimage, &exe).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// O despacho por plataforma é por `#[cfg]` (não `cfg!()`): as funções de
/// outras plataformas nem chegam a compilar — `std::os::unix` quebrava o
/// build do Windows na release 1.1.0.
#[cfg(target_os = "macos")]
fn apply_for_platform(path: &Path, name: &str) -> Result<(), String> {
    if name.ends_with(".dmg") {
        apply_macos_dmg(path)
    } else {
        Err("não é um update desta plataforma".to_string())
    }
}

#[cfg(target_os = "linux")]
fn apply_for_platform(path: &Path, name: &str) -> Result<(), String> {
    if name.ends_with(".AppImage") {
        apply_linux_appimage(path)
    } else {
        Err("não é um update desta plataforma".to_string())
    }
}

#[cfg(target_os = "windows")]
fn apply_for_platform(_path: &Path, _name: &str) -> Result<(), String> {
    Err("atualização automática não suportada nesta plataforma — instale do release".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison_is_numeric_not_lexicographic() {
        assert!(parse_version("0.10.0") > parse_version("0.9.0"));
        assert!(parse_version("1.0.0") > parse_version("0.99.0"));
        assert!(parse_version("0.9.0") == parse_version("0.9.0"));
        assert!(parse_version("0.9") < parse_version("0.9.1"));
    }

    /// Hits the real GitHub API — not run by default (`cargo test` skips
    /// `#[ignore]`d tests), only a manual sanity check:
    /// `cargo test -p polystnx-app --lib -- --ignored hits_the_real_github_api`.
    #[test]
    #[ignore]
    fn hits_the_real_github_api() {
        assert!(newer_release("0.0.0").is_some());
        assert!(newer_release("999.0.0").is_none());
    }
}
