//! The dev-mode menu (plan revision: "por enquanto criar menu em branco
//! apenas com o botão voltar", depois: "coloque uma opção no dev mode para
//! baixar e instalar esse zip - instalar assets exemplo - demonstrar
//! funcionamento do emulador") — behind the idle panel's "Dev" button,
//! which itself only exists once the Konami code has been entered on the
//! idle screen. Downloads the example package (one ROM + its cover/logo/
//! cartridge/back-cover art) into the app root so a fresh install has
//! something to play with.

use std::io::Read;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use anyhow::Result;
use polystnx_platform::{Cabinet, MenuMode, MenuNav, Platform, Screen};

use crate::dirs;

/// URL do pacote de exemplo (`exemplo-platformer-example.zip`: o
/// platformer open source + `roms/` + `assets/` dele, commitado no repositório
/// para demonstrar o emulador). É o arquivo cru do próprio repo — o repo
/// precisa estar público e o push em dia. `PSX_XPERIENCE_EXEMPLO_ZIP_URL`
/// sobrepõe (servidor local nos testes).
pub const EXEMPLO_ZIP_URL: &str = "";

fn exemplo_zip_url() -> String {
    std::env::var("PSX_XPERIENCE_EXEMPLO_ZIP_URL").unwrap_or_else(|_| EXEMPLO_ZIP_URL.to_string())
}

/// Progress/result reported by the install worker thread.
enum InstallMsg {
    Progress {
        downloaded: u64,
        total: Option<u64>,
    },
    /// Instalado com sucesso — quantidade de arquivos gravados.
    Done(usize),
    Failed(String),
}

const BG: (u8, u8, u8) = (18, 18, 20);
const TEXT: (u8, u8, u8) = (232, 232, 232);
const DIM: (u8, u8, u8) = (150, 150, 158);
const GREEN: (u8, u8, u8) = (60, 230, 70);

/// Same glyph metrics the platform font uses — `Screen::text` advances
/// `GLYPH_W * scale` per char, `GLYPH_H * scale` per line.
const CELL: i32 = 9;

/// The "voltar" button's rect in screen-buffer coordinates — the single
/// source both the draw closure and the click hit-test use (same pattern as
/// the idle setup screen's `setup_rects`).
fn voltar_rect(w: u32, h: u32) -> (i32, i32, u32, u32) {
    let (bw, bh) = (220u32, 44u32);
    (
        (w as i32 - bw as i32) / 2,
        h as i32 - bh as i32 - 52,
        bw,
        bh,
    )
}

/// The "instalar assets de exemplo" button's rect — the menu's one action.
fn install_rect(w: u32, h: u32) -> (i32, i32, u32, u32) {
    let (bw, bh) = (510u32, 44u32);
    (
        (w as i32 - bw as i32) / 2,
        h as i32 / 2 - bh as i32 - 10,
        bw,
        bh,
    )
}

fn in_rect(x: i32, y: i32, r: (i32, i32, u32, u32)) -> bool {
    x >= r.0 && y >= r.1 && (x - r.0) < r.2 as i32 && (y - r.1) < r.3 as i32
}

/// Run the dev menu until the player backs out. Returns `Ok(true)` when the
/// app should quit altogether (window closed here), `Ok(false)` to return to
/// the idle screen — same contract as the settings screen's `run`.
pub fn run(plat: &mut Platform, cab: &mut Cabinet) -> Result<bool> {
    let frame = Duration::from_millis(16);
    let mut next = Instant::now() + frame;

    let mut rx: Option<Receiver<InstallMsg>> = None;
    // Status line under the button — hint while idle, live progress while
    // downloading, outcome afterwards (kept until the next attempt).
    let mut status = String::from("baixa um pacote com uma rom e os assets dela");
    let mut status_color = DIM;
    let mut busy = false;

    loop {
        if let Some(r) = &rx {
            match r.try_recv() {
                Ok(InstallMsg::Progress { downloaded, total }) => {
                    let mb = downloaded as f64 / 1_048_576.0;
                    status = match total {
                        Some(t) => format!("baixando... {mb:.1}/{:.1} MB", t as f64 / 1_048_576.0),
                        None => format!("baixando... {mb:.1} MB"),
                    };
                }
                Ok(InstallMsg::Done(n)) => {
                    rx = None;
                    busy = false;
                    status_color = GREEN;
                    status = format!(
                        "instalado! {n} arquivos — clique atualizar na estante para ver o jogo"
                    );
                }
                Ok(InstallMsg::Failed(e)) => {
                    rx = None;
                    busy = false;
                    status_color = (240, 120, 110);
                    status = format!("falha: {e}");
                }
                Err(TryRecvError::Disconnected) => {
                    rx = None;
                    busy = false;
                }
                Err(TryRecvError::Empty) => {}
            }
        }

        let m = plat.poll_menu(MenuMode::Nav);
        if m.quit {
            return Ok(true);
        }
        let mut start = false;
        if !busy {
            // Confirm (gamepad A) tem o mesmo efeito do clique no botão — é
            // a única ação do menu.
            if m.nav.iter().any(|n| matches!(n, MenuNav::Confirm)) {
                start = true;
            }
            if m.nav.iter().any(|n| matches!(n, MenuNav::Back)) {
                return Ok(false);
            }
            if let Some((x, y)) = m.click {
                let (ox, oy) = cab.window_to_output(x, y);
                if cab.hit_close_button(ox, oy) {
                    return Ok(true);
                }
                if let Some((sx, sy)) = cab.hit_screen_point(ox, oy) {
                    let (w, h) = cab.screen_size();
                    if in_rect(sx, sy, install_rect(w, h)) {
                        start = true;
                    }
                    if in_rect(sx, sy, voltar_rect(w, h)) {
                        return Ok(false);
                    }
                }
            }
        } else if m.nav.iter().any(|n| matches!(n, MenuNav::Back)) {
            return Ok(false);
        }
        if start {
            let (tx, r) = mpsc::channel();
            std::thread::spawn(move || install_worker(tx));
            rx = Some(r);
            busy = true;
            status_color = DIM;
            status = "baixando...".to_string();
        }

        let status = status.clone();
        let status_color = status_color;
        let busy = busy;
        let render = move |d: &mut Screen| {
            let (w, h) = d.size();
            let (ix, iy, iw, ih) = install_rect(w, h);
            let label = "instalar assets de exemplo";
            let color = if busy { DIM } else { TEXT };
            d.outline(ix, iy, iw, ih, 2, (color.0, color.1, color.2, 255));
            d.text(
                ix + (iw as i32 - label.chars().count() as i32 * CELL * 2) / 2,
                iy + (ih as i32 - 40) / 2,
                2,
                color,
                label,
            );
            // Uma linha centrada quando cabe (a dica/status são curtos); em
            // tela estreita cai para text_wrapped na margem.
            let single = status.chars().count() as i32 * CELL;
            if single <= w as i32 - 40 {
                d.text(
                    (w as i32 - single) / 2,
                    iy + ih as i32 + 18,
                    1,
                    status_color,
                    &status,
                );
            } else {
                d.text_wrapped(40, iy + ih as i32 + 18, w - 80, 1, status_color, &status);
            }

            let (bx, by, bw, bh) = voltar_rect(w, h);
            d.outline(bx, by, bw, bh, 2, (TEXT.0, TEXT.1, TEXT.2, 255));
            let label = "voltar";
            d.text(
                bx + (bw as i32 - label.chars().count() as i32 * CELL * 2) / 2,
                by + (bh as i32 - 40) / 2,
                2,
                TEXT,
                label,
            );
        };
        cab.frame_2d(BG, render);
        crate::runner::pace_frame(&mut next, frame);
    }
}

fn install_worker(tx: mpsc::Sender<InstallMsg>) {
    let msg = match try_install(&exemplo_zip_url(), &dirs::app_root(), &tx) {
        Ok(n) => InstallMsg::Done(n),
        Err(e) => InstallMsg::Failed(e),
    };
    let _ = tx.send(msg);
}

/// Download the example package and hand the bytes to `install_bytes`.
fn try_install(
    url: &str,
    root: &Path,
    tx: &mpsc::Sender<InstallMsg>,
) -> std::result::Result<usize, String> {
    if url.is_empty() {
        return Err(
            "pacote de exemplo ainda sem URL — defina EXEMPLO_ZIP_URL em devmenu.rs".to_string(),
        );
    }
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .build();
    let resp = agent.get(url).call().map_err(|e| e.to_string())?;
    let total = resp
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok());
    let mut bytes = Vec::new();
    let mut reader = resp.into_reader();
    let mut buf = [0u8; 65_536];
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        let _ = tx.send(InstallMsg::Progress {
            downloaded: bytes.len() as u64,
            total,
        });
    }

    install_bytes(&bytes, root, tx)
}

/// Unpack an already-downloaded example package into `root` — only entries
/// under `roms/` and `assets/` are installed (o LEIA-ME é instrução de
/// extração manual, não conteúdo), and every path is checked against `..`/
/// absolute escapes before anything touches the disk. Overwrites: instalar
/// de novo é idempotente por design.
fn install_bytes(
    bytes: &[u8],
    root: &Path,
    _tx: &mpsc::Sender<InstallMsg>,
) -> std::result::Result<usize, String> {
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut installed = 0usize;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = file.name().to_string();
        if file.is_dir() || name.starts_with('.') || name.contains("__MACOSX") {
            continue;
        }
        let rel = Path::new(&name);
        if rel.is_absolute() || name.contains("..") {
            continue;
        }
        if !(rel.starts_with("roms") || rel.starts_with("assets")) {
            continue;
        }
        let dest = root.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = std::fs::File::create(&dest).map_err(|e| e.to_string())?;
        std::io::copy(&mut file, &mut out).map_err(|e| e.to_string())?;
        installed += 1;
    }
    if installed == 0 {
        return Err("o zip não tem roms/ nem assets/ — formato inesperado".to_string());
    }
    Ok(installed)
}

/// Headless preview of the dev menu (dev/testing) — one frame captured
/// through the tube into a BMP, exactly as `run` draws it.
pub fn capture_preview(cab: &mut Cabinet, path: &std::path::Path) -> Result<()> {
    let render = |d: &mut Screen| {
        let (w, h) = d.size();
        let (ix, iy, iw, ih) = install_rect(w, h);
        let label = "instalar assets de exemplo";
        d.outline(ix, iy, iw, ih, 2, (TEXT.0, TEXT.1, TEXT.2, 255));
        d.text(
            ix + (iw as i32 - label.chars().count() as i32 * CELL * 2) / 2,
            iy + (ih as i32 - 40) / 2,
            2,
            TEXT,
            label,
        );
        let hint = "baixa um pacote com uma rom e os assets dela";
        d.text(
            (w as i32 - hint.chars().count() as i32 * CELL) / 2,
            iy + ih as i32 + 18,
            1,
            DIM,
            hint,
        );

        let (bx, by, bw, bh) = voltar_rect(w, h);
        d.outline(bx, by, bw, bh, 2, (TEXT.0, TEXT.1, TEXT.2, 255));
        let label = "voltar";
        d.text(
            bx + (bw as i32 - label.chars().count() as i32 * CELL * 2) / 2,
            by + (bh as i32 - 40) / 2,
            2,
            TEXT,
            label,
        );
    };
    cab.capture_2d(BG, render, path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{install_bytes, try_install, InstallMsg};
    use std::io::{Read as _, Write as _};
    use std::sync::mpsc;

    fn zip_in_memory(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            for (name, data) in entries {
                w.start_file(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                std::io::Write::write_all(&mut w, data).unwrap();
            }
            w.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn install_extracts_roms_and_assets_only() {
        let zip = zip_in_memory(&[
            ("roms/Demo.zip", b"PK-ROM" as &[u8]),
            ("assets/cover/Demo.png", b"PNG"),
            ("LEIA-ME.txt", b"instrucoes"),
            ("__MACOSX/._lixo", b"lixo"),
            ("../escapou.txt", b"nao"),
        ]);
        let root = tempfile_root("extract");
        std::fs::create_dir_all(&root).unwrap();
        let (tx, _rx) = mpsc::channel();
        let n = install_bytes(&zip, &root, &tx).unwrap();
        assert_eq!(n, 2, "só roms/ e assets/ contam");
        assert_eq!(
            std::fs::read(root.join("roms/Demo.zip")).unwrap(),
            b"PK-ROM"
        );
        assert!(root.join("assets/cover/Demo.png").is_file());
        assert!(!root.join("LEIA-ME.txt").exists());
        assert!(!root.join("escapou.txt").exists());
        assert!(!root.parent().unwrap().join("escapou.txt").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn install_reports_empty_zip_shape() {
        let zip = zip_in_memory(&[("LEIA-ME.txt", b"x")]);
        let root = tempfile_root("empty");
        std::fs::create_dir_all(&root).unwrap();
        let (tx, _rx) = mpsc::channel();
        let err = install_bytes(&zip, &root, &tx).unwrap_err();
        assert!(err.contains("roms/"), "erro inesperado: {err}");
        std::fs::remove_dir_all(&root).ok();
    }

    /// Serve `payload` uma vez num socket 127.0.0.1 efêmero — o suficiente
    /// para o `try_install` fazer um HTTP real de ponta a ponta.
    fn serve_once(payload: Vec<u8>) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut req = [0u8; 4096];
            let _ = stream.read(&mut req);
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                payload.len()
            );
            stream.write_all(header.as_bytes()).unwrap();
            stream.write_all(&payload).unwrap();
        });
        (format!("http://{addr}/exemplo.zip"), handle)
    }

    #[test]
    fn install_downloads_over_http_with_progress() {
        // asset gordo o bastante para gerar mais de um chunk de progresso
        let big = vec![0xABu8; 300_000];
        let zip = zip_in_memory(&[
            ("roms/Demo.zip", b"PK-ROM" as &[u8]),
            ("assets/cover/Demo.png", &big),
        ]);
        let (url, server) = serve_once(zip.clone());
        let root = tempfile_root("http");
        std::fs::create_dir_all(&root).unwrap();
        let (tx, rx) = mpsc::channel();
        let n = try_install(&url, &root, &tx).unwrap();
        server.join().unwrap();
        assert_eq!(n, 2);
        assert_eq!(
            std::fs::read(root.join("assets/cover/Demo.png"))
                .unwrap()
                .len(),
            300_000
        );
        // o canal trouxe progresso com o tamanho vindo do Content-Length
        let mut progress = 0;
        let mut saw_total = None;
        for m in rx.try_iter() {
            if let InstallMsg::Progress {
                downloaded: _,
                total,
            } = m
            {
                progress += 1;
                saw_total = total;
            }
        }
        assert!(progress >= 1, "nenhum Progress no canal");
        assert_eq!(saw_total, Some(zip.len() as u64));
        std::fs::remove_dir_all(&root).ok();
    }

    /// raiz temporária única por teste (sem dep de tempfile)
    fn tempfile_root(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "polystnx-devmenu-test-{}-{tag}",
            std::process::id()
        ))
    }
}
