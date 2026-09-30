//! Portable app layout: every folder the app uses lives in one root — no
//! database. `roms/` (drop ROMs here), `core/` (the SwanStation core), `assets/`
//! (local cover/logo art), `saves/`, `notes/`, plus `psx-xperience.cfg` and
//! `library.json` at the root.
//!
//! macOS special case: the `.app` on this platform ships in `/Applications`
//! (or wherever Finder drags it, often read-only-ish and not somewhere a
//! user expects an app to scribble folders into). So on macOS the root
//! isn't next to the executable at all — it's `~/Documents/PSX Xperience`,
//! created on first launch, same spirit as how a normal Mac app keeps its
//! user data.
//!
//! Linux special case: AppImage is a read-only container that extracts to a
//! temp directory. So on Linux the root is `~/.local/share/PSX Xperience`
//! (following XDG directory conventions), created on first launch. This
//! also supports regular Linux builds next to the executable.
//!
//! Windows keeps the simpler "next to the executable" portable layout, since
//! a `.exe` anywhere the user put it is already writable and exactly where
//! they'd look for `roms/` next to it.

#[cfg(not(target_os = "linux"))]
use std::path::Path;
use std::path::PathBuf;

/// The folder the app treats as its root — see the module doc for the macOS
/// special case.
pub fn app_root() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        macos_root_for(&home)
    }
    #[cfg(target_os = "linux")]
    {
        linux_app_root()
    }
    #[cfg(target_os = "windows")]
    {
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
        exe.parent().unwrap_or_else(|| Path::new(".")).to_path_buf()
    }
}

/// `~/Documents/PSX Xperience` — split out from `app_root` so it can be
/// unit-tested with a synthetic home directory.
#[cfg(target_os = "macos")]
fn macos_root_for(home: &Path) -> PathBuf {
    home.join("Documents").join("PSX Xperience")
}

/// XDG_DATA_HOME/.../PSX Xperience, falling back to ~/.local/share if unset.
#[cfg(target_os = "linux")]
fn linux_app_root() -> PathBuf {
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    data_home.join("PSX Xperience")
}

pub fn roms_dir() -> PathBuf {
    app_root().join("roms")
}

/// Onde o download do update do app fica até ser aplicado no arranque
/// seguinte — na raiz do app (plan revision: "colocar o update na raiz das
/// pastas do app nao dentro dos saves"; saves guardam progresso de jogo).
pub fn update_dir() -> PathBuf {
    let p = app_root().join("update");
    let _ = std::fs::create_dir_all(&p);
    p
}

/// Igual a [`update_dir`], sem criar — o apply no arranque não deve ter
/// efeito colateral em disco.
pub fn update_dir_opt() -> Option<PathBuf> {
    Some(app_root().join("update"))
}

pub fn core_dir() -> PathBuf {
    app_root().join("core")
}

/// A BIOS do console (SCPH*.BIN, fornecida por quem roda — nunca baixada).
/// É o system directory que o core espera (plano §1.1).
pub fn bios_dir() -> PathBuf {
    app_root().join("bios")
}

pub fn assets_dir() -> PathBuf {
    app_root().join("assets")
}

pub fn saves_dir() -> PathBuf {
    app_root().join("saves")
}

/// A biblioteca de memory cards compartilhados (plano §3) — arquivos `.mcr`
/// de 128 KB, cards de verdade que existem independentes de jogo, como os
/// cartões físicos do console.
pub fn memcards_dir() -> PathBuf {
    app_root().join("memcards")
}

/// Tudo do RetroAchievements (cache por jogo, ids ganhos, sessões de
/// progresso, envios pendentes, badges) — plan revision: "dados do
/// retroachievements da pasta save devem ficar em uma pasta
/// 'retroachievements' na raiz - pasta save apenas são os saves dos jogos".
pub fn retroachievements_dir() -> PathBuf {
    app_root().join("retroachievements")
}

pub fn notes_dir() -> PathBuf {
    app_root().join("notes")
}

/// Os arquivos de configuração/identificação num lugar só — plan revision:
/// "criar pasta config e colocar o cfg, o dat, library e o hash".
pub fn config_dir() -> PathBuf {
    app_root().join("config")
}

pub fn config_path() -> PathBuf {
    config_dir().join("psx-xperience.cfg")
}

/// Play counts / added-at / last-played-at, keyed by ROM hash — the only
/// state that needs to survive between runs (everything else is recomputed
/// by scanning `roms/` fresh each launch). O hash cache do catálogo
/// (`hashcache.json`) vive ao lado, derivado deste path por
/// `with_file_name`.
pub fn library_path() -> PathBuf {
    config_dir().join("library.json")
}

/// A No-Intro DAT (XML) for canonical ROM titles — optional, supplied by
/// whoever runs the app (no direct download link exists on No-Intro's own
/// site to fetch it automatically).
pub fn nointro_dat_path() -> PathBuf {
    config_dir().join("nointro.dat")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_root_is_documents_psx_xperience() {
        assert_eq!(
            super::macos_root_for(Path::new("/Users/rex")),
            Path::new("/Users/rex/Documents/PSX Xperience")
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_root_uses_xdg_data_home() {
        assert_eq!(
            super::linux_app_root(),
            Path::new(
                std::env::var("XDG_DATA_HOME")
                    .as_deref()
                    .unwrap_or(&format!(
                        "{}/.local/share",
                        std::env::var("HOME").unwrap_or_default()
                    ))
            )
            .join("PSX Xperience")
        );
    }

    #[test]
    fn update_dir_lives_at_the_app_root_not_saves() {
        // Plan revision: "colocar o update na raiz das pastas do app nao
        // dentro dos saves".
        let dir = super::update_dir_opt().expect("update dir");
        assert_eq!(dir, super::app_root().join("update"));
        assert!(!dir.starts_with(super::saves_dir()));
    }
}
