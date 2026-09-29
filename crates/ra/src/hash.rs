//! PSX disc hashing in the RetroAchievements standard: the vendored rhash
//! (`rc_hash_psx`) driven through a custom cdreader this crate registers once.
//!
//! rcheevos ships readers for cue/bin/iso but not CHD — the frontend plugs its
//! own in (`rc_hash_init_custom_cdreader`, the same hook RetroArch uses for
//! libchdr). Ours reads CHD with the pure-Rust `chd` crate and delegates every
//! other extension to rcheevos' default reader.
//!
//! The PSX hash itself is rcheevos' algorithm, unchanged: MD5 of the boot
//! executable's name (parsed from SYSTEM.CNF, without the `;1`) followed by
//! the executable's bytes (size field in the PS-X EXE header + the 2048-byte
//! header itself) — the exact string the RA server hashes PSX sets against.
//! Proven against a synthetic cue+bin and a real CHD in `tests/hash.rs` and
//! `docs/fase-0.md`.

use crate::sys;
use std::ffi::{c_char, c_uint, c_void, CStr, CString};
use std::fs::File;
use std::path::Path;
use std::sync::OnceLock;

/// The RA disc hash for the PSX image at `path` (`.chd`, `.cue`, `.bin`,
/// `.iso` — whatever the registered readers open). MD5, 32 hex chars.
pub fn psx_disc_hash(path: impl AsRef<Path>) -> Result<String, String> {
    ensure_cdreader();
    let path = path.as_ref();
    let c_path = CString::new(path.to_string_lossy().into_owned())
        .map_err(|_| "caminho com NUL interno".to_string())?;
    let mut hash = [0 as c_char; sys::RC_HASH_SIZE];
    // rcheevos reports each attempt's failures through these once-only hooks;
    // route them to the log so an unhashable disc is diagnosable in the field.
    static MESSAGES: OnceLock<()> = OnceLock::new();
    MESSAGES.get_or_init(|| unsafe {
        sys::rc_hash_init_error_message_callback(Some(rhash_log_error));
        sys::rc_hash_init_verbose_message_callback(Some(rhash_log_verbose));
    });
    let ok = unsafe {
        sys::rc_hash_generate_from_file(
            hash.as_mut_ptr(),
            sys::RC_CONSOLE_PLAYSTATION,
            c_path.as_ptr(),
        )
    };
    if ok == 0 {
        return Err(format!("rhash não gerou hash para {}", path.display()));
    }
    let s = unsafe { CStr::from_ptr(hash.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    if s.len() != 32 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("hash inesperado do rhash: {s:?}"));
    }
    Ok(s)
}

/// O serial de fábrica do disco (`SLUS-00402`), lido do `SYSTEM.CNF` dentro
/// do CHD — a chave da identificação de discos do domínio (plano §2). Mesmo
/// caminho do `rc_hash_psx` (PVD → diretório raiz → SYSTEM.CNF → linha
/// `BOOT = cdrom:\SERIAL;1`), normalizado para o formato Redump.
pub fn psx_serial(path: impl AsRef<Path>) -> Result<String, String> {
    let mut track = ChdTrack::open(path.as_ref().to_string_lossy().as_ref())?;
    let raw = boot_executable_name(&mut track)?;
    // Jogos com o exe em subdiretório (`BOOT = cdrom:\TEKKEN3\SLUS_004.02`)
    // — o serial é o nome de arquivo, não o caminho (o hash da RA usa o
    // caminho inteiro; o serial, só o arquivo).
    let file = raw.rsplit('\\').next().unwrap_or(&raw);
    Ok(normalize_serial(file))
}

/// `SLUS_004.02` → `SLUS-00402`: maiúsculas, `_` vira `-`, ponto some — o
/// formato canônico que os bancos de serial (Redump/DuckStation) usam.
pub fn normalize_serial(raw: &str) -> String {
    raw.trim()
        .to_ascii_uppercase()
        .replace('_', "-")
        .replace('.', "")
}

/// Caminha PVD → raiz → `SYSTEM.CNF` e devolve o nome do executável de boot
/// (`SLUS_004.02`), cru como está no arquivo.
fn boot_executable_name(track: &mut ChdTrack) -> Result<String, String> {
    // PVD: setor 16, registro do diretório raiz em +156.
    let mut pvd = [0u8; 2048];
    if track.read_sector(16, &mut pvd) < 256 || &pvd[1..6] != b"CD001" {
        return Err("ISO9660: PVD não encontrado no setor 16".to_string());
    }
    let le = |b: &[u8]| u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    let root_extent = le(&pvd[156 + 2..156 + 6]);
    let root_size = le(&pvd[156 + 10..156 + 14]);

    // Diretório raiz: registros encadeados, nenhum cruza setor.
    let mut dir = vec![0u8; root_size.max(2048) as usize];
    for (i, chunk) in dir.chunks_mut(2048).enumerate() {
        track.read_sector(root_extent + i as u32, chunk);
    }
    let cnf = find_dir_entry(&dir, b"SYSTEM.CNF").ok_or("SYSTEM.CNF não está no diretório raiz")?;

    let mut text = vec![0u8; cnf.size.min(2048) as usize];
    track.read_sector(cnf.extent, &mut text);
    let text = String::from_utf8_lossy(&text);
    let boot = text
        .split('\n')
        .find(|l| l.trim_start().to_ascii_uppercase().starts_with("BOOT"))
        .ok_or("SYSTEM.CNF sem linha BOOT")?;
    let value = boot.split('=').nth(1).ok_or("linha BOOT sem valor")?;
    let value = value.trim();
    let value = value
        .strip_prefix("cdrom:")
        .unwrap_or(value)
        .trim_start_matches('\\');
    let end = value
        .find(|c: char| c == ';' || c.is_whitespace())
        .unwrap_or(value.len());
    if end == 0 {
        return Err("linha BOOT sem executável".to_string());
    }
    Ok(value[..end].to_string())
}

/// Um arquivo do diretório: extensão no disco e tamanho.
struct DirEntry {
    extent: u32,
    size: u32,
}

/// Procura `name` (sem sufixo `;versão`, caixa insensível) nos registros do
/// diretório já lido.
fn find_dir_entry(dir: &[u8], name: &[u8]) -> Option<DirEntry> {
    let mut pos = 0usize;
    while pos + 33 <= dir.len() {
        let len = dir[pos] as usize;
        if len == 0 {
            break; // fim dos registros neste setor/extent
        }
        let rec = &dir[pos..pos + len];
        let name_len = rec[32] as usize;
        let rec_name = &rec[33..33 + name_len];
        let base = match rec_name.iter().position(|&b| b == b';') {
            Some(i) => &rec_name[..i],
            None => rec_name,
        };
        let is_dir = rec[25] & 0b10 != 0;
        if !is_dir && base.eq_ignore_ascii_case(name) {
            let le = |b: &[u8]| u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            return Some(DirEntry {
                extent: le(&rec[2..6]),
                size: le(&rec[10..14]),
            });
        }
        pos += len;
    }
    None
}

// --- the custom cdreader ----------------------------------------------------

unsafe extern "C" fn rhash_log_error(msg: *const c_char) {
    if !msg.is_null() {
        let m = unsafe { CStr::from_ptr(msg) }.to_string_lossy();
        eprintln!("ra: rhash: {m}");
    }
}
unsafe extern "C" fn rhash_log_verbose(msg: *const c_char) {
    if !msg.is_null() {
        let m = unsafe { CStr::from_ptr(msg) }.to_string_lossy();
        log::debug!("ra: rhash: {m}");
    }
}

/// Captured rcheevos default reader — cue/bin/iso still hash through it.
static DEFAULT: OnceLock<sys::rc_hash_cdreader_t> = OnceLock::new();
static REGISTERED: OnceLock<()> = OnceLock::new();

fn ensure_cdreader() {
    DEFAULT.get_or_init(|| {
        let mut def = sys::rc_hash_cdreader_t {
            open_track: None,
            read_sector: None,
            close_track: None,
            first_track_sector: None,
            open_track_iterator: None,
        };
        unsafe { sys::rc_hash_get_default_cdreader(&mut def) };
        def
    });
    REGISTERED.get_or_init(|| unsafe {
        sys::rc_hash_init_custom_cdreader(&CDREADER);
    });
}

static CDREADER: sys::rc_hash_cdreader_t = sys::rc_hash_cdreader_t {
    open_track: Some(open_track),
    read_sector: Some(read_sector),
    close_track: Some(close_track),
    first_track_sector: Some(first_track_sector),
    open_track_iterator: Some(open_track_iterator),
};

/// A track handle flowing through the callbacks. Ours wrap CHD state; every
/// other extension produced a handle inside rcheevos' default reader and is
/// passed back to it untouched.
enum Handle {
    Chd(Box<ChdTrack>),
    Default(*mut c_void),
}

unsafe extern "C" fn open_track(path: *const c_char, track: c_uint) -> *mut c_void {
    // The default reader only fills `open_track_iterator`; the non-iterator
    // variant is legacy and our flow never reaches it — but CHD works without
    // an iterator, so honor it anyway.
    open_track_impl(path, track, None)
}

unsafe extern "C" fn open_track_iterator(
    path: *const c_char,
    track: c_uint,
    iterator: *const c_void,
) -> *mut c_void {
    open_track_impl(path, track, Some(iterator))
}

fn open_track_impl(
    path: *const c_char,
    track: c_uint,
    iterator: Option<*const c_void>,
) -> *mut c_void {
    let Some(path) = (unsafe { path.as_ref() }) else {
        return std::ptr::null_mut();
    };
    let path = unsafe { CStr::from_ptr(path) }
        .to_string_lossy()
        .into_owned();
    if path.to_ascii_lowercase().ends_with(".chd") {
        return match ChdTrack::open(&path) {
            Ok(t) => Box::into_raw(Box::new(Handle::Chd(Box::new(t)))) as *mut c_void,
            Err(e) => {
                eprintln!("ra: cdreader: {path}: {e}");
                std::ptr::null_mut()
            }
        };
    }
    let open = || -> Option<*mut c_void> {
        let def = DEFAULT.get()?;
        let f = def.open_track_iterator?;
        let inner = unsafe { f(path.as_ptr() as *const c_char, track, iterator?) };
        Some(Box::into_raw(Box::new(Handle::Default(inner))) as *mut c_void)
    };
    open().unwrap_or(std::ptr::null_mut())
}

unsafe extern "C" fn read_sector(
    handle: *mut c_void,
    sector: c_uint,
    buffer: *mut c_void,
    requested_bytes: usize,
) -> usize {
    let Some(handle) = (unsafe { (handle as *mut Handle).as_mut() }) else {
        return 0;
    };
    match handle {
        Handle::Chd(t) => t.read_sector(sector, unsafe {
            std::slice::from_raw_parts_mut(buffer as *mut u8, requested_bytes)
        }),
        Handle::Default(inner) => {
            let f = DEFAULT
                .get()
                .and_then(|d| d.read_sector)
                .expect("default cdreader captured");
            unsafe { f(*inner, sector, buffer, requested_bytes) }
        }
    }
}

unsafe extern "C" fn close_track(handle: *mut c_void) {
    let Some(handle) = (unsafe { (handle as *mut Handle).as_mut() }) else {
        return;
    };
    match handle {
        Handle::Chd(_) => unsafe { drop(Box::from_raw(handle as *mut Handle)) },
        Handle::Default(inner) => {
            if let Some(f) = DEFAULT.get().and_then(|d| d.close_track) {
                unsafe { f(*inner) };
            }
            unsafe { drop(Box::from_raw(handle as *mut Handle)) };
        }
    }
}

unsafe extern "C" fn first_track_sector(handle: *mut c_void) -> c_uint {
    let Some(handle) = (unsafe { (handle as *mut Handle).as_mut() }) else {
        return 0;
    };
    match handle {
        Handle::Chd(t) => t.first_sector(),
        Handle::Default(inner) => {
            let f = DEFAULT
                .get()
                .and_then(|d| d.first_track_sector)
                .expect("default cdreader captured");
            unsafe { f(*inner) }
        }
    }
}

// --- CHD reading (the `chd` crate) ------------------------------------------

/// Bytes per stored frame in a v5 CD CHD: 2352 raw sector + 96 subcode.
const CD_FRAME_SIZE: usize = 2448;
/// User data within the frame, per sector type — `(offset, len)`.
fn user_data_span(sector_type: &str) -> Option<(usize, usize)> {
    match sector_type {
        // sync(12) + header(4) → data
        "MODE1_RAW" => Some((16, 2048)),
        // sync(12) + header(4) + subheader(8) → data
        "MODE2_RAW" => Some((24, 2048)),
        // cooked images: data only
        "MODE1" => Some((0, 2048)),
        // subheader(8) → data
        "MODE2" => Some((8, 2048)),
        _ => None, // AUDIO etc.: not hashed for PSX
    }
}

#[derive(Clone, Copy)]
struct TrackInfo {
    /// First frame of this track's data in CHD hunk space. PREGAP/POSTGAP
    /// frames are metadata-only — proven against a real CHD (`docs/fase-0.md`):
    /// the track extents must fit the hunk capacity, and they only do when
    /// pregaps don't count.
    start_frame: u32,
    frames: u32,
    data_offset: usize,
}

struct ChdTrack {
    chd: chd::Chd<File>,
    compressed: Vec<u8>,
    hunk_buf: Vec<u8>,
    frames_per_hunk: u32,
    tracks: Vec<TrackInfo>,
}

impl ChdTrack {
    fn open(path: &str) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| e.to_string())?;
        let mut meta_file = file.try_clone().map_err(|e| e.to_string())?;
        let mut chd = chd::Chd::open(file, None).map_err(|e| e.to_string())?;
        let hunk_size = chd.header().hunk_size() as usize;
        if hunk_size == 0 || !hunk_size.is_multiple_of(CD_FRAME_SIZE) {
            return Err(format!(
                "hunk de {hunk_size} bytes não é múltiplo de {CD_FRAME_SIZE} (não é CHD de CD-ROM?)"
            ));
        }
        let frames_per_hunk = (hunk_size / CD_FRAME_SIZE) as u32;

        let mut tracks = Vec::new();
        let mut next_frame = 0u32;
        let mut refs = chd.metadata_refs();
        #[allow(clippy::while_let_on_iterator)] // lending iterator: sem IntoIterator
        while let Some(entry) = refs.next() {
            let md = entry
                .read(&mut meta_file)
                .map_err(|e| format!("metadado CHD: {e}"))?;
            if md.metatag != chd::metadata::KnownMetadata::CdRomTrack as u32
                && md.metatag != chd::metadata::KnownMetadata::CdRomTrack2 as u32
            {
                continue;
            }
            let text = String::from_utf8_lossy(&md.value);
            let get = |key: &str| -> Option<&str> {
                // "TRACK:1 TYPE:MODE2_RAW SUBTYPE:NONE FRAMES:… PREGAP:… …"
                text.split_whitespace().find_map(|kv| kv.strip_prefix(key))
            };
            let (Some(fty), Some(frames)) = (get("TYPE:"), get("FRAMES:")) else {
                continue;
            };
            let Some((data_offset, _)) = user_data_span(fty) else {
                continue; // audio/subcode-only track
            };
            let frames: u32 = frames.parse().map_err(|e| format!("FRAMES: {e}"))?;
            tracks.push(TrackInfo {
                start_frame: next_frame,
                frames,
                data_offset,
            });
            next_frame = next_frame
                .checked_add(frames)
                .ok_or("disco com frames demais")?;
        }
        if tracks.is_empty() {
            return Err("nenhuma trilha de dados no CHD".to_string());
        }

        Ok(ChdTrack {
            chd,
            compressed: Vec::new(),
            hunk_buf: vec![0; hunk_size],
            frames_per_hunk,
            tracks,
        })
    }

    fn first_sector(&self) -> c_uint {
        // Track 1's data starts at hunk-space frame 0 — the absolute sector
        // space the rhash queries (PVD at 16) lines up with.
        self.tracks[0].start_frame
    }

    fn read_sector(&mut self, sector: c_uint, out: &mut [u8]) -> usize {
        let Some(track) = self
            .tracks
            .iter()
            .find(|t| sector >= t.start_frame && sector < t.start_frame + t.frames)
            .copied()
        else {
            return 0;
        };
        if out.is_empty() {
            return 0;
        }
        let (off, len) = (track.data_offset, 2048);
        let mut done = 0usize;
        let mut sector = sector;
        while done < out.len() {
            let frame = (sector - track.start_frame) as usize;
            let hunk_n = frame / self.frames_per_hunk as usize;
            let in_hunk = (frame % self.frames_per_hunk as usize) * CD_FRAME_SIZE;
            if self.load_hunk(hunk_n).is_err() {
                break;
            }
            let src = &self.hunk_buf[in_hunk + off..in_hunk + off + len];
            let n = src.len().min(out.len() - done);
            out[done..done + n].copy_from_slice(&src[..n]);
            done += n;
            sector += 1;
        }
        done
    }

    fn load_hunk(&mut self, hunk_n: usize) -> Result<(), String> {
        let mut hunk = self.chd.hunk(hunk_n as u32).map_err(|e| e.to_string())?;
        hunk.read_hunk_in(&mut self.compressed, &mut self.hunk_buf)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
