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

/// Mensagens do update: o PolystnX fala só português — o par en/pt do
/// irmão SixteeN fica na fonte para o port seguir 1:1.
macro_rules! tr {
    ($en:expr, $pt:expr $(,)?) => {
        $pt
    };
}

const RELEASES_API: &str = "https://api.github.com/repos/ticianocorral/PolystnX/releases/latest";

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
    /// O asset desta plataforma (dmg/AppImage/zip), se houver — só um que
    /// venha do próprio repositório e com o SHA-256 publicado pelo GitHub.
    pub asset_url: Option<UpdateAsset>,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout(Duration::from_secs(8))
        .build()
}

/// Rodando dentro do sandbox do Flatpak? Nessas builds o app é atualizado
/// pelo próprio Flathub (`/app` é read-only — o self-update não teria onde
/// escrever, e baixar/substituir binários por conta própria duplicaria o
/// canal de atualização), então a checagem e o apply do update do **app**
/// ficam desligados. O check do core PCSX Rearmed (`core_stale`) continua.
pub fn running_under_flatpak() -> bool {
    std::env::var_os("FLATPAK_ID").is_some() || std::path::Path::new("/.flatpak-info").exists()
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
    let head: GhCommit = read_json(resp)?;
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
    /// `"sha256:<hex>"` — o GitHub calcula no upload e publica na API.
    #[serde(default)]
    digest: Option<String>,
}

/// Onde baixar o update e o hash que o arquivo TEM de ter.
#[derive(Clone, Debug, PartialEq)]
pub struct UpdateAsset {
    pub url: String,
    /// SHA-256 em hex minúsculo.
    pub sha256: String,
}

/// Só assets publicados nos releases deste repositório são aceitos — uma
/// resposta da API adulterada não aponta o update para outro lugar.
const RELEASE_DOWNLOAD_PREFIX: &str =
    "https://github.com/ticianocorral/PolystnX/releases/download/";

/// A URL é de um asset dos releases deste repositório? Além do prefixo,
/// o resto não pode ter nada que o parser de URL normalize ou desvie
/// (`..`, escapes `%`, `\`, query/fragmento): `…/download/../../outro/repo`
/// passaria no `starts_with` e baixaria de outro repositório.
fn is_release_url(url: &str) -> bool {
    // Dono/repositório no GitHub não distinguem caixa — o prefixo é
    // comparado sem ela; o resto segue com o filtro estrito abaixo.
    let Some(rest) = url
        .get(..RELEASE_DOWNLOAD_PREFIX.len())
        .filter(|p| p.eq_ignore_ascii_case(RELEASE_DOWNLOAD_PREFIX))
        .map(|_| &url[RELEASE_DOWNLOAD_PREFIX.len()..])
    else {
        return false;
    };
    !rest.is_empty()
        && rest
            .split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
        && rest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-+/".contains(c))
}

/// `"sha256:<64 hex>"` → o hex minúsculo; qualquer outra coisa, `None`.
fn parse_sha256_digest(digest: &str) -> Option<String> {
    let hex = digest.strip_prefix("sha256:")?;
    (hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| hex.to_ascii_lowercase())
}

/// O asset do release para a plataforma em que roda — `-macos.dmg`,
/// `-linux-*.AppImage` ou `-win.zip` pelo nome (o CI publica exatamente
/// esses sufixos).
fn asset_for_platform(assets: &[GhAsset]) -> Option<UpdateAsset> {
    let (needle, ext): (&str, &str) = if cfg!(target_os = "macos") {
        ("-macos", ".dmg")
    } else if cfg!(target_os = "linux") {
        ("-linux-", ".AppImage")
    } else {
        // Windows: não há apply automático (`apply_for_platform` recusa) —
        // baixar centenas de MB que ficariam largados em `update/` não
        // ajuda; a tela de update manda para a página de releases.
        return None;
    };
    let a = assets
        .iter()
        .find(|a| a.name.contains(needle) && a.name.ends_with(ext))?;
    if !is_release_url(&a.browser_download_url) {
        log::warn!(
            "update: asset fora do repositório, ignorado: {}",
            a.browser_download_url
        );
        return None;
    }
    // Sem o hash publicado não há como conferir o que chegou — o update
    // não é oferecido (baixar à mão continua possível).
    let Some(sha256) = a.digest.as_deref().and_then(parse_sha256_digest) else {
        log::warn!("update: asset {} sem digest sha256, ignorado", a.name);
        return None;
    };
    Some(UpdateAsset {
        url: a.browser_download_url.clone(),
        sha256,
    })
}

/// `Some(tag)` if GitHub's latest release is newer than `current` — `None`
/// on any network/parse hiccup, or when already current. Versions are
/// compared as dot-separated integers (a leading `v` is stripped first), not
/// full semver — good enough for this project's plain `MAJOR.MINOR.PATCH`
/// tags.
fn newer_release(current: &str) -> Option<AppUpdate> {
    let resp = agent()
        .get(RELEASES_API)
        .set("User-Agent", "polystnx-update-check")
        .call()
        .ok()?;
    let release: GhRelease = read_json(resp)?;
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

/// JSON da API com teto de 4 MiB (o `into_json` do ureq 2 não tem limite).
fn read_json<T: serde::de::DeserializeOwned>(resp: ureq::Response) -> Option<T> {
    use std::io::Read;
    serde_json::from_reader(resp.into_reader().take(4 << 20)).ok()
}

fn parse_version(v: &str) -> Vec<u32> {
    v.split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

/// Run both checks and report back on `tx` — but only if there's actually
/// something to say; a fully up-to-date app/core sends nothing at all, so
/// the receiver just sees the channel go quiet rather than an explicit
/// "you're fine" message.
pub fn check(core_dir: PathBuf, app_version: String, tx: Sender<UpdateNotice>) {
    let app_update = if running_under_flatpak() {
        None
    } else {
        newer_release(&app_version)
    };
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
pub fn download_asset(
    asset: &UpdateAsset,
    dest_dir: &Path,
    tx: &Sender<u32>,
) -> Result<PathBuf, String> {
    let url = asset.url.as_str();
    if !is_release_url(url) {
        return Err(format!("update: url fora do repositório: {url}"));
    }
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(600))
        .build();
    let resp = agent.get(url).call().map_err(|e| e.to_string())?;
    let total = resp
        .header("Content-Length")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    // O nome vem da URL da API: só o último segmento, sem `\\` (separador
    // no Windows) nem `..`, e só caracteres de nome de arquivo comuns.
    let file_name = url
        .rsplit(['/', '\\'])
        .next()
        .filter(|n| {
            !n.is_empty()
                && !n.starts_with('.')
                && n.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._-+".contains(c))
        })
        .unwrap_or("update.bin")
        .to_string();
    std::fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    let dest = dest_dir.join(&file_name);
    // Baixa para `.part` e só renomeia completo: um download interrompido
    // (rede caiu, app fechado) nunca deixa um `.dmg`/`.AppImage` truncado
    // que o `apply_pending_update` tentaria aplicar sobre o executável.
    let part = dest_dir.join(format!("{file_name}.part"));
    let result = download_into(resp.into_reader(), &part, total, &asset.sha256, tx);
    if let Err(e) = result {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    // O hash vai junto: o arranque só aplica o que ainda bater com ele
    // (um pacote de versão antiga, truncado ou trocado na pasta é apagado).
    polystnx_domain::fsutil::write_atomic(sha_sidecar(&dest), &asset.sha256)
        .map_err(|e| e.to_string())?;
    std::fs::rename(&part, &dest).map_err(|e| {
        let _ = std::fs::remove_file(&part);
        e.to_string()
    })?;
    Ok(dest)
}

/// O corpo do `download_asset`: grava em `part`, confere o tamanho contra o
/// Content-Length (quando veio) e força o conteúdo ao disco antes do rename.
fn download_into(
    mut reader: impl std::io::Read,
    part: &Path,
    total: u64,
    expected_sha256: &str,
    tx: &Sender<u32>,
) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let mut file = std::fs::File::create(part).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 64 * 1024];
    let mut got: u64 = 0;
    // AppImages/DMGs ficam entre 20–150 MB; 512 MB é teto anti-disco-cheio,
    // não um tamanho esperado.
    const MAX_ASSET_DOWNLOAD: u64 = 512 * 1024 * 1024;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        std::io::Write::write_all(&mut file, &buf[..n]).map_err(|e| e.to_string())?;
        hasher.update(&buf[..n]);
        got += n as u64;
        if got > MAX_ASSET_DOWNLOAD {
            return Err(tr!(
                "download larger than expected — aborted",
                "download maior que o esperado — abortado"
            )
            .to_string());
        }
        if let Some(pct) = (got * 100).checked_div(total) {
            let _ = tx.send(pct.min(100) as u32);
        }
    }
    if total > 0 && got != total {
        return Err(tr!(
            format!("incomplete download ({got} of {total} bytes)"),
            format!("download incompleto ({got} de {total} bytes)")
        ));
    }
    // O arquivo tem de ser exatamente o que o GitHub publicou: nada é
    // renomeado para `.dmg`/`.AppImage` (e aplicado) sem o hash bater.
    let got_sha = hex::encode(hasher.finalize());
    if got_sha != expected_sha256 {
        return Err(tr!(
            format!("update file does not match the published hash ({got_sha})"),
            format!("o arquivo do update não bate com o hash publicado ({got_sha})")
        ));
    }
    file.sync_all().map_err(|e| e.to_string())?;
    Ok(())
}

/// `<pacote>.sha256` — o hash publicado, gravado ao lado no download.
fn sha_sidecar(pkg: &Path) -> PathBuf {
    let mut name = pkg.file_name().unwrap_or_default().to_os_string();
    name.push(".sha256");
    pkg.with_file_name(name)
}

/// `<pacote>.tentativas` — quantas vezes o apply falhou no arranque.
fn tries_sidecar(pkg: &Path) -> PathBuf {
    let mut name = pkg.file_name().unwrap_or_default().to_os_string();
    name.push(".tentativas");
    pkg.with_file_name(name)
}

/// Apaga o pacote e o que anda junto dele.
fn discard_package(pkg: &Path) {
    for p in [pkg.to_path_buf(), sha_sidecar(pkg), tries_sidecar(pkg)] {
        let _ = std::fs::remove_file(p);
    }
}

/// O pacote ainda é exatamente o que foi baixado e conferido?
fn package_matches_sidecar(pkg: &Path) -> bool {
    use sha2::{Digest, Sha256};
    let Ok(expected) = std::fs::read_to_string(sha_sidecar(pkg)) else {
        return false;
    };
    let Ok(mut f) = std::fs::File::open(pkg) else {
        return false;
    };
    let mut hasher = Sha256::new();
    if std::io::copy(&mut f, &mut hasher).is_err() {
        return false;
    }
    hex::encode(hasher.finalize()) == expected.trim()
}

/// Aplica um arquivo já baixado em `saves/update/` — no arranque (o loop de
/// [`apply_pending_update`]) e, desde a 1.1.4, na própria tela de update
/// logo após o download ("clicou, atualizou"). Sucesso remove o arquivo;
/// falha o mantém para o arranque seguinte tentar de novo.
pub fn apply_update_file(path: &Path) -> Result<(), String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let outcome = apply_for_platform(path, name);
    match outcome {
        Ok(()) => {
            log::info!("update: {name} aplicado — reinicie para valer");
            discard_package(path);
            Ok(())
        }
        Err(e) => {
            log::info!("update: {name} não aplicado ({e}) — tenta no próximo arranque");
            Err(e)
        }
    }
}

/// Aplica, no arranque, uma atualização baixada pela tela de update — o
/// "será atualizado ao reiniciar" (plan revision). Melhor-esforço: qualquer
/// falha loga e mantém o arquivo para tentar de novo. Antes do SDL, na
/// main do `polystnx`.
pub fn apply_pending_update() {
    if running_under_flatpak() {
        return;
    }
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
        // Download que não terminou (app fechado no meio): centenas de MB
        // que nada mais vai usar.
        if name.ends_with(".part") {
            let _ = std::fs::remove_file(&path);
            continue;
        }
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
        if !package_matches_sidecar(&path) {
            log::warn!("update: {name} sem hash conferido — descartado");
            discard_package(&path);
            continue;
        }
        if apply_update_file(&path).is_ok() {
            discard_package(&path);
            continue;
        }
        // Falhou: tenta nos próximos arranques, mas não para sempre (uma
        // cópia de centenas de MB a cada abertura).
        const MAX_TRIES: u32 = 3;
        let tries = std::fs::read_to_string(tries_sidecar(&path))
            .ok()
            .and_then(|t| t.trim().parse::<u32>().ok())
            .unwrap_or(0)
            + 1;
        if tries >= MAX_TRIES {
            log::warn!("update: {name} falhou {tries} vezes — desistindo");
            discard_package(&path);
        } else {
            let _ = std::fs::write(tries_sidecar(&path), tries.to_string());
        }
    }
}

/// macOS: monta o dmg, copia o .app por cima do bundle em execução
/// (`ditto` preserva a estrutura), desmonta. O binário em execução pode
/// ser substituído — o inode vivo continua rodando.
#[cfg(target_os = "macos")]
fn apply_macos_dmg(dmg: &Path) -> Result<(), String> {
    use std::process::Command;
    // Ponto de montagem na pasta do PRÓPRIO app, não no temp: sem `TMPDIR`
    // (app aberto por script) o temp vira o `/tmp` compartilhado, e outro
    // usuário poderia plantar um link entre o remove e o create.
    let cache = crate::dirs::app_root().join("cache");
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let mnt = cache.join("update-mnt");
    let _ = std::fs::remove_dir_all(&mnt);
    std::fs::create_dir(&mnt).map_err(|e| e.to_string())?;
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
            .ok_or_else(|| {
                tr!(
                    "couldn't find the running .app",
                    "não achei o .app em execução"
                )
                .to_string()
            })?;
        let new_app = std::fs::read_dir(&mnt)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e == "app"))
            .ok_or_else(|| tr!("no .app inside the dmg", "dmg sem .app dentro").to_string())?;
        let status = Command::new("ditto")
            .arg(new_app)
            .arg(&app_bundle)
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err(tr!("ditto failed", "ditto falhou").to_string())
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
/// a cópia vai para um temporário e entra por **rename**: o AppImage montado
/// (FUSE) lê o arquivo sob demanda enquanto o app roda, e sobrescrever
/// in-place serviria conteúdo misturado (o inode novo entra inteiro ou não
/// entra — quem tem o inode antigo aberto continua bem servido).
#[cfg(target_os = "linux")]
fn apply_linux_appimage(appimage: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    // Rodando de um AppImage, `current_exe()` aponta para o squashfs
    // montado (somente leitura) — o arquivo a trocar é o `$APPIMAGE`.
    let exe = match std::env::var_os("APPIMAGE") {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::current_exe().map_err(|e| e.to_string())?,
    };
    let tmp = exe.with_extension("new");
    let result = (|| {
        std::fs::copy(appimage, &tmp)?;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
        // No disco antes do rename: uma queda de energia (bateria do Deck)
        // logo depois não pode deixar o executável vazio.
        std::fs::File::open(&tmp)?.sync_all()?;
        std::fs::rename(&tmp, &exe)
    })();
    if result.is_err() {
        // Nada de uma cópia de centenas de MB largada ao lado do AppImage.
        let _ = std::fs::remove_file(&tmp);
    }
    result.map_err(|e| e.to_string())
}

/// O despacho por plataforma é por `#[cfg]` (não `cfg!()`): as funções de
/// outras plataformas nem chegam a compilar — `std::os::unix` quebrava o
/// build do Windows na release 1.1.0.
#[cfg(target_os = "macos")]
fn apply_for_platform(path: &Path, name: &str) -> Result<(), String> {
    if name.ends_with(".dmg") {
        apply_macos_dmg(path)
    } else {
        Err(tr!(
            "not an update for this platform",
            "não é um update desta plataforma"
        )
        .to_string())
    }
}

#[cfg(target_os = "linux")]
fn apply_for_platform(path: &Path, name: &str) -> Result<(), String> {
    if name.ends_with(".AppImage") {
        apply_linux_appimage(path)
    } else {
        Err(tr!(
            "not an update for this platform",
            "não é um update desta plataforma"
        )
        .to_string())
    }
}

#[cfg(target_os = "windows")]
fn apply_for_platform(_path: &Path, _name: &str) -> Result<(), String> {
    Err(tr!(
        "automatic update not supported on this platform — install from the release",
        "atualização automática não suportada nesta plataforma — instale do release"
    )
    .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_rejects_unrelated_files_and_keeps_them() {
        let dir = std::env::temp_dir().join("polystnx-apply-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("nao-e-update.txt");
        std::fs::write(&p, b"x").unwrap();
        assert!(apply_update_file(&p).is_err());
        assert!(p.exists(), "falha mantém o arquivo para o próximo arranque");
        let _ = std::fs::remove_dir_all(&dir);
    }

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

    fn gh(name: &str, url: &str, digest: Option<&str>) -> GhAsset {
        GhAsset {
            name: name.into(),
            browser_download_url: url.into(),
            digest: digest.map(Into::into),
        }
    }

    #[test]
    fn sha256_digest_parses_only_the_real_shape() {
        let hex = "7d3936bc9d414f22f4a9b2357de52ad0aaa9a5b3c1639c95aff643acb2e139e9";
        assert_eq!(
            parse_sha256_digest(&format!("sha256:{hex}")).as_deref(),
            Some(hex)
        );
        assert_eq!(
            parse_sha256_digest(&format!("sha256:{}", hex.to_uppercase())).as_deref(),
            Some(hex)
        );
        assert_eq!(parse_sha256_digest(hex), None);
        assert_eq!(parse_sha256_digest("sha256:abc"), None);
        assert_eq!(parse_sha256_digest(&format!("sha1:{hex}")), None);
    }

    #[test]
    fn update_asset_needs_repo_origin_and_digest() {
        let (name, ext) = if cfg!(target_os = "macos") {
            ("PolystnX-9.9.9-macos", ".dmg")
        } else if cfg!(target_os = "linux") {
            ("PolystnX-9.9.9-linux-x86_64", ".AppImage")
        } else {
            // Windows não oferece update automático.
            let url = format!("{RELEASE_DOWNLOAD_PREFIX}v9.9.9/PolystnX-9.9.9-windows-x86_64.zip");
            let digest = format!("sha256:{}", "a".repeat(64));
            assert!(asset_for_platform(&[gh(
                "PolystnX-9.9.9-windows-x86_64.zip",
                &url,
                Some(&digest)
            )])
            .is_none());
            return;
        };
        let file = format!("{name}{ext}");
        let good_url = format!("{RELEASE_DOWNLOAD_PREFIX}v9.9.9/{file}");
        let digest = format!("sha256:{}", "a".repeat(64));

        let ok = asset_for_platform(&[gh(&file, &good_url, Some(&digest))]).unwrap();
        assert_eq!(ok.url, good_url);
        // A API devolve o nome canônico do repositório; caixa não importa.
        let lower = good_url.replace("PolystnX/releases", "polystnx/releases");
        assert!(asset_for_platform(&[gh(&file, &lower, Some(&digest))]).is_some());
        assert_eq!(ok.sha256, "a".repeat(64));
        // Sem digest, ou de outro lugar: nada é oferecido.
        assert!(asset_for_platform(&[gh(&file, &good_url, None)]).is_none());
        let evil = format!("https://example.com/{file}");
        assert!(asset_for_platform(&[gh(&file, &evil, Some(&digest))]).is_none());
    }

    #[test]
    fn download_rejects_wrong_hash_and_short_body() {
        let dir = std::env::temp_dir().join(format!("polystnx-upd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let part = dir.join("x.part");
        let (tx, _rx) = std::sync::mpsc::channel();
        let body: &[u8] = b"conteudo do update";
        use sha2::{Digest, Sha256};
        let right = hex::encode(Sha256::digest(body));

        assert!(download_into(body, &part, body.len() as u64, &right, &tx).is_ok());
        assert!(download_into(body, &part, body.len() as u64, &"0".repeat(64), &tx).is_err());
        assert!(download_into(body, &part, body.len() as u64 + 1, &right, &tx).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn release_url_rejects_traversal_and_escapes() {
        let ok = format!("{RELEASE_DOWNLOAD_PREFIX}v1.8.0/PolystnX-1.8.0-macos.dmg");
        assert!(is_release_url(&ok));
        for bad in [
            format!("{RELEASE_DOWNLOAD_PREFIX}../../../../outro/repo/releases/download/v1/x.dmg"),
            format!("{RELEASE_DOWNLOAD_PREFIX}v1/%2e%2e/x.dmg"),
            format!("{RELEASE_DOWNLOAD_PREFIX}v1\\..\\x.dmg"),
            format!("{RELEASE_DOWNLOAD_PREFIX}v1/x.dmg?y=1"),
            RELEASE_DOWNLOAD_PREFIX.to_string(),
            "https://example.com/x.dmg".to_string(),
        ] {
            assert!(!is_release_url(&bad), "{bad}");
        }
    }

    #[test]
    fn package_needs_matching_sidecar() {
        let dir = std::env::temp_dir().join(format!("polystnx-pkg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pkg = dir.join("PolystnX-9-macos.dmg");
        std::fs::write(&pkg, b"pacote").unwrap();
        assert!(!package_matches_sidecar(&pkg), "sem .sha256");
        use sha2::{Digest, Sha256};
        std::fs::write(sha_sidecar(&pkg), hex::encode(Sha256::digest(b"pacote"))).unwrap();
        assert!(package_matches_sidecar(&pkg));
        std::fs::write(&pkg, b"trocado").unwrap();
        assert!(!package_matches_sidecar(&pkg));
        discard_package(&pkg);
        assert!(!pkg.exists() && !sha_sidecar(&pkg).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
