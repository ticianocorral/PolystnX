//! Gravação atômica: o arquivo final nunca fica pela metade. `fs::write`
//! trunca antes de escrever — um travamento, queda de energia ou disco
//! cheio no meio deixava cards, savestates ou library.json cortados/zerados.
//! Aqui o conteúdo vai para um `.tmp` no mesmo diretório, desce para o
//! disco (`sync_all`) e só então substitui o original por `rename`, que é
//! atômico no mesmo sistema de arquivos.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Grava `contents` em `path` de forma atômica.
pub fn write_atomic(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> io::Result<()> {
    write_inner(path.as_ref(), contents.as_ref(), false)
}

/// Como [`write_atomic`], com o arquivo final em 0600 no Unix — para o que
/// guarda credenciais (o cfg com os tokens do RetroAchievements).
pub fn write_atomic_private(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> io::Result<()> {
    write_inner(path.as_ref(), contents.as_ref(), true)
}

fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

fn write_inner(path: &Path, contents: &[u8], private: bool) -> io::Result<()> {
    let tmp = tmp_path(path);
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    if private {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    #[cfg(not(unix))]
    let _ = private;
    let result = (|| {
        let mut f = opts.open(&tmp)?;
        // `.mode()` só vale na criação: um .tmp esquecido de outra vez
        // também é fechado.
        #[cfg(unix)]
        if private {
            use std::os::unix::fs::PermissionsExt;
            f.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        f.write_all(contents)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("polystnx-fsutil-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn replaces_contents_and_leaves_no_tmp() {
        let d = dir("replace");
        let p = d.join("save.srm");
        write_atomic(&p, b"antigo").unwrap();
        write_atomic(&p, b"novo").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"novo");
        assert!(!tmp_path(&p).exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn private_is_0600_even_over_an_existing_file() {
        use std::os::unix::fs::PermissionsExt;
        let d = dir("private");
        let p = d.join("polystnx.cfg");
        fs::write(&p, "x").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap();
        write_atomic_private(&p, "token").unwrap();
        let mode = fs::metadata(&p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn failure_keeps_the_original() {
        let d = dir("fail");
        let p = d.join("nao-existe").join("x.bin");
        assert!(write_atomic(&p, b"y").is_err());
        let _ = fs::remove_dir_all(&d);
    }
}
