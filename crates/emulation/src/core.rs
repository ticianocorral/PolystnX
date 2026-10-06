//! Safe-ish wrapper around a dynamically loaded libretro core.
//!
//! libretro's callbacks are bare C function pointers with no user-data slot, so
//! the frontend state they need is reached through a thread-local pointer that
//! is only non-null for the duration of a `retro_run` / `retro_load_game` call.

use std::cell::Cell;
use std::ffi::{c_void, CStr, CString};
use std::os::raw::{c_char, c_uint};
use std::path::{Path, PathBuf};
use std::ptr;

use libloading::{Library, Symbol};

use crate::sys;
use crate::sys::*;

/// libretro joypad button, in libretro id order. `Frame`-independent.
/// L2/R2 close the 14 digital buttons of the PSX pad; L3/R3 ride on the
/// analog sticks, which go through [`Core::set_analog`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Button {
    B = RETRO_DEVICE_ID_JOYPAD_B,
    Y = RETRO_DEVICE_ID_JOYPAD_Y,
    Select = RETRO_DEVICE_ID_JOYPAD_SELECT,
    Start = RETRO_DEVICE_ID_JOYPAD_START,
    Up = RETRO_DEVICE_ID_JOYPAD_UP,
    Down = RETRO_DEVICE_ID_JOYPAD_DOWN,
    Left = RETRO_DEVICE_ID_JOYPAD_LEFT,
    Right = RETRO_DEVICE_ID_JOYPAD_RIGHT,
    A = RETRO_DEVICE_ID_JOYPAD_A,
    X = RETRO_DEVICE_ID_JOYPAD_X,
    L = RETRO_DEVICE_ID_JOYPAD_L,
    R = RETRO_DEVICE_ID_JOYPAD_R,
    L2 = RETRO_DEVICE_ID_JOYPAD_L2,
    R2 = RETRO_DEVICE_ID_JOYPAD_R2,
}

impl Button {
    pub const ALL: [Button; 14] = [
        Button::B,
        Button::Y,
        Button::Select,
        Button::Start,
        Button::Up,
        Button::Down,
        Button::Left,
        Button::Right,
        Button::A,
        Button::X,
        Button::L,
        Button::R,
        Button::L2,
        Button::R2,
    ];
}

/// Which analog stick, in libretro index order (left, right).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalogStick {
    Left = 0,
    Right = 1,
}

pub const MAX_PORTS: usize = 2;

/// Battery-backed cartridge SRAM (`retro_get_memory_data` id).
pub const MEMORY_SAVE_RAM: c_uint = RETRO_MEMORY_SAVE_RAM;
/// Console work RAM.
pub const MEMORY_SYSTEM_RAM: c_uint = RETRO_MEMORY_SYSTEM_RAM;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Rgb1555,
    Xrgb8888,
    Rgb565,
}

impl PixelFormat {
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            PixelFormat::Rgb1555 | PixelFormat::Rgb565 => 2,
            PixelFormat::Xrgb8888 => 4,
        }
    }
}

/// One video frame handed up by the core. Owns its pixels.
#[derive(Debug, Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// Row stride in bytes, as reported by the core (may exceed width * bpp).
    pub pitch: usize,
    pub format: PixelFormat,
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub struct AvInfo {
    pub base_width: u32,
    pub base_height: u32,
    pub max_width: u32,
    pub max_height: u32,
    pub aspect_ratio: f32,
    pub fps: f64,
    pub sample_rate: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("could not open core library at {path}: {source}")]
    Open {
        path: PathBuf,
        #[source]
        source: libloading::Error,
    },
    #[error("core is missing symbol `{0}`")]
    MissingSymbol(&'static str),
    #[error("core reports libretro API version {found}, frontend speaks {expected}")]
    ApiMismatch { found: u32, expected: u32 },
    #[error("core rejected the ROM (retro_load_game returned false)")]
    LoadRejected,
    #[error("core requires a real file path for this ROM but none was given")]
    NeedsFullPath,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Frontend state the C callbacks read and write. Boxed so its address is stable.
struct CallbackState {
    pixel_format: PixelFormat,
    system_directory: CString,
    save_directory: CString,
    /// Button matrix the frontend fills before each `run()`.
    input: [[bool; 16]; MAX_PORTS],
    /// Analog sticks the frontend fills before each `run()`:
    /// `[port][stick] = (x, y)`, libretro's full ±i16 range.
    analog: [[(i16, i16); 2]; MAX_PORTS],
    /// Latest frame, taken out by the frontend after `run()`.
    frame: Option<Frame>,
    /// `true` when the core signalled a duped frame (no new video this run).
    frame_duped: bool,
    /// Interleaved S16 stereo, accumulated across the run, drained by frontend.
    audio: Vec<i16>,
    /// A bandeja do drive (`SET_DISK_CONTROL_EXT_INTERFACE`, 56): ejetar por
    /// aqui é o que faz os JOGOS verem a tampa abrir — as telas de erro de
    /// leitura deles disparam de verdade.
    disk: Option<sys::retro_disk_control_ext_callback>,
    /// Legacy core-option variables: name -> chosen value (first listed option).
    variables: Vec<(CString, CString)>,
    variables_dirty: bool,
    av_info: AvInfo,
    av_info_dirty: bool,
}

thread_local! {
    static CB: Cell<*mut CallbackState> = const { Cell::new(ptr::null_mut()) };
}

fn with_cb<R>(f: impl FnOnce(&mut CallbackState) -> R) -> Option<R> {
    CB.with(|c| {
        let p = c.get();
        if p.is_null() {
            None
        } else {
            // Safety: set_scope guarantees the pointer outlives the closure and
            // that no other &mut alias exists on this thread while it is set.
            Some(f(unsafe { &mut *p }))
        }
    })
}

pub struct Core {
    // `lib` must outlive every symbol; declared last so it drops last.
    api: CoreApi,
    state: Box<CallbackState>,
    system_name: String,
    system_version: String,
    needs_fullpath: bool,
    valid_extensions: Vec<String>,
    loaded: bool,
    /// `retro_init` já rodou — `retro_deinit` só é válido depois dele (o
    /// PCSX Rearmed SEGFAULTA em `retro_deinit` sem `init`; o contrato
    /// libretro manda exatamente isso).
    inited: bool,
    #[allow(dead_code)]
    lib: Library,
}

impl Core {
    /// dlopen the core and resolve its entry points. Does not init it yet.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let path = path.as_ref().to_path_buf();
        // Safety: loading arbitrary native code. The path comes from the user.
        let lib = unsafe { Library::new(&path) }.map_err(|source| CoreError::Open {
            path: path.clone(),
            source,
        })?;

        macro_rules! sym {
            ($name:literal, $t:ty) => {{
                let s: Symbol<$t> = unsafe { lib.get(concat!($name, "\0").as_bytes()) }
                    .map_err(|_| CoreError::MissingSymbol($name))?;
                *s
            }};
        }

        let api = CoreApi {
            retro_api_version: sym!("retro_api_version", sys::FnU32),
            retro_get_system_info: sym!("retro_get_system_info", sys::FnGetSystemInfo),
            retro_get_system_av_info: sym!("retro_get_system_av_info", sys::FnGetSystemAvInfo),
            retro_set_environment: sym!("retro_set_environment", sys::FnSetEnvironment),
            retro_set_video_refresh: sym!("retro_set_video_refresh", sys::FnSetVideoRefresh),
            retro_set_audio_sample: sym!("retro_set_audio_sample", sys::FnSetAudioSample),
            retro_set_audio_sample_batch: sym!(
                "retro_set_audio_sample_batch",
                sys::FnSetAudioSampleBatch
            ),
            retro_set_input_poll: sym!("retro_set_input_poll", sys::FnSetInputPoll),
            retro_set_input_state: sym!("retro_set_input_state", sys::FnSetInputState),
            retro_set_controller_port_device: sym!(
                "retro_set_controller_port_device",
                sys::FnSetControllerPortDevice
            ),
            retro_init: sym!("retro_init", sys::FnVoid),
            retro_deinit: sym!("retro_deinit", sys::FnVoid),
            retro_load_game: sym!("retro_load_game", sys::FnLoadGame),
            retro_unload_game: sym!("retro_unload_game", sys::FnVoid),
            retro_run: sym!("retro_run", sys::FnVoid),
            retro_reset: sym!("retro_reset", sys::FnVoid),
            retro_get_region: sym!("retro_get_region", sys::FnU32),
            retro_serialize_size: sym!("retro_serialize_size", sys::FnUsize),
            retro_serialize: sym!("retro_serialize", sys::FnSerialize),
            retro_unserialize: sym!("retro_unserialize", sys::FnUnserialize),
            retro_get_memory_data: sym!("retro_get_memory_data", sys::FnGetMemoryData),
            retro_get_memory_size: sym!("retro_get_memory_size", sys::FnGetMemorySize),
            retro_cheat_reset: sym!("retro_cheat_reset", sys::FnCheatReset),
            retro_cheat_set: sym!("retro_cheat_set", sys::FnCheatSet),
        };

        let found = unsafe { (api.retro_api_version)() };
        if found != RETRO_API_VERSION {
            return Err(CoreError::ApiMismatch {
                found,
                expected: RETRO_API_VERSION,
            });
        }

        let mut info: retro_system_info = unsafe { std::mem::zeroed() };
        unsafe { (api.retro_get_system_info)(&mut info) };
        let system_name = cstr_to_string(info.library_name);
        let system_version = cstr_to_string(info.library_version);
        let valid_extensions = cstr_to_string(info.valid_extensions)
            .split('|')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_lowercase())
            .collect();

        let state = Box::new(CallbackState {
            pixel_format: PixelFormat::Rgb565,
            system_directory: CString::new(".").unwrap(),
            save_directory: CString::new(".").unwrap(),
            input: [[false; 16]; MAX_PORTS],
            analog: [[(0, 0); 2]; MAX_PORTS],
            frame: None,
            frame_duped: false,
            audio: Vec::with_capacity(4096),
            disk: None,
            variables: Vec::new(),
            variables_dirty: false,
            av_info: AvInfo {
                base_width: 256,
                base_height: 224,
                max_width: 512,
                max_height: 512,
                aspect_ratio: 0.0,
                fps: 60.0,
                sample_rate: 32040.0,
            },
            av_info_dirty: false,
        });

        Ok(Core {
            api,
            state,
            system_name,
            system_version,
            needs_fullpath: info.need_fullpath,
            valid_extensions,
            loaded: false,
            inited: false,
            lib,
        })
    }

    pub fn system_name(&self) -> &str {
        &self.system_name
    }
    pub fn system_version(&self) -> &str {
        &self.system_version
    }
    pub fn valid_extensions(&self) -> &[String] {
        &self.valid_extensions
    }

    /// Point the core at the folders it uses for BIOS / SRAM / configs.
    pub fn set_directories(&mut self, system: &Path, save: &Path) {
        self.state.system_directory =
            CString::new(system.to_string_lossy().into_owned()).unwrap_or_default();
        self.state.save_directory =
            CString::new(save.to_string_lossy().into_owned()).unwrap_or_default();
    }

    /// Override a core option by key (e.g. `pcsx_rearmed_nocdaudio` =
    /// `Software`). Upserts into the variable table and flags it dirty so the
    /// core re-reads it on the next `run()`. Call after `init()` — the option
    /// definitions the core pushes at init populate the table with defaults,
    /// and this overrides one of them (or pre-seeds one the core hasn't
    /// declared yet: it still reads unknown keys via `GET_VARIABLE`).
    pub fn set_variable(&mut self, key: &str, value: &str) {
        let (Ok(k), Ok(v)) = (CString::new(key), CString::new(value)) else {
            return;
        };
        if let Some(slot) = self.state.variables.iter_mut().find(|(ek, _)| *ek == k) {
            slot.1 = v;
        } else {
            self.state.variables.push((k, v));
        }
        self.state.variables_dirty = true;
    }

    /// Current value of a core option, if set.
    pub fn variable(&self, key: &str) -> Option<&str> {
        let k = CString::new(key).ok()?;
        self.state
            .variables
            .iter()
            .find(|(ek, _)| *ek == k)
            .and_then(|(_, v)| v.to_str().ok())
    }

    /// Every core option the core declared or the frontend set, as
    /// `(key, value)` pairs — diagnosis and the settings screen.
    pub fn variables(&self) -> Vec<(String, String)> {
        self.state
            .variables
            .iter()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.to_string_lossy().into_owned(),
                )
            })
            .collect()
    }

    /// Register callbacks and call `retro_init`. Idempotent-unsafe: call once.
    pub fn init(&mut self) {
        self.inited = true;
        self.enter(|api| unsafe {
            (api.retro_set_environment)(environment_cb);
            (api.retro_set_video_refresh)(video_refresh_cb);
            (api.retro_set_audio_sample)(audio_sample_cb);
            (api.retro_set_audio_sample_batch)(audio_sample_batch_cb);
            (api.retro_set_input_poll)(input_poll_cb);
            (api.retro_set_input_state)(input_state_cb);
            (api.retro_init)();
        });
    }

    /// Load a ROM. `data` is the ROM bytes; `path` is its on-disk location,
    /// which the core keeps reading from (discs are need_fullpath).
    pub fn load_game(&mut self, path: &Path, data: &[u8]) -> Result<(), CoreError> {
        let c_path = CString::new(path.to_string_lossy().into_owned()).unwrap_or_default();
        let info = if self.needs_fullpath {
            retro_game_info {
                path: c_path.as_ptr(),
                data: ptr::null(),
                size: 0,
                meta: ptr::null(),
            }
        } else {
            retro_game_info {
                path: c_path.as_ptr(),
                data: data.as_ptr() as *const c_void,
                size: data.len(),
                meta: ptr::null(),
            }
        };

        let ok = self.enter(|api| unsafe { (api.retro_load_game)(&info) });
        if !ok {
            return Err(CoreError::LoadRejected);
        }
        self.loaded = true;

        // Default both ports to a standard pad.
        self.enter(|api| unsafe {
            (api.retro_set_controller_port_device)(0, RETRO_DEVICE_JOYPAD);
            (api.retro_set_controller_port_device)(1, RETRO_DEVICE_JOYPAD);
        });
        self.refresh_av_info();
        Ok(())
    }

    pub fn refresh_av_info(&mut self) {
        let mut av: retro_system_av_info = unsafe { std::mem::zeroed() };
        self.enter(|api| unsafe { (api.retro_get_system_av_info)(&mut av) });
        self.state.av_info = AvInfo {
            base_width: av.geometry.base_width,
            base_height: av.geometry.base_height,
            max_width: av.geometry.max_width.max(av.geometry.base_width),
            max_height: av.geometry.max_height.max(av.geometry.base_height),
            aspect_ratio: av.geometry.aspect_ratio,
            fps: av.timing.fps,
            sample_rate: av.timing.sample_rate,
        };
        self.state.av_info_dirty = false;
    }

    pub fn av_info(&self) -> AvInfo {
        self.state.av_info
    }

    pub fn av_info_dirty(&self) -> bool {
        self.state.av_info_dirty
    }

    pub fn region(&mut self) -> c_uint {
        self.enter(|api| unsafe { (api.retro_get_region)() })
    }

    /// Set one button on one port for the frames that follow.
    pub fn set_button(&mut self, port: usize, button: Button, pressed: bool) {
        if port < MAX_PORTS {
            self.state.input[port][button as usize] = pressed;
        }
    }

    /// Set one analog stick on one port for the frames that follow — full
    /// libretro range (−32768..32767; the frontend applies its own deadzone).
    pub fn set_analog(&mut self, port: usize, stick: AnalogStick, x: i16, y: i16) {
        if port < MAX_PORTS {
            self.state.analog[port][stick as usize] = (x, y);
        }
    }

    /// Advance one frame. Drains previous video/audio first.
    pub fn run(&mut self) {
        self.state.frame_duped = false;
        self.state.audio.clear();
        self.enter(|api| unsafe { (api.retro_run)() });
        if self.state.av_info_dirty {
            self.refresh_av_info();
        }
    }

    /// The most recent frame, if the last `run()` produced a fresh one.
    pub fn take_frame(&mut self) -> Option<Frame> {
        self.state.frame.take()
    }

    pub fn frame_duped(&self) -> bool {
        self.state.frame_duped
    }

    /// Interleaved S16LE stereo produced by the last `run()`.
    pub fn audio(&self) -> &[i16] {
        &self.state.audio
    }

    // --- run-ahead scaffolding (used from Phase 1 on) ----------------------
    pub fn serialize_size(&mut self) -> usize {
        self.enter(|api| unsafe { (api.retro_serialize_size)() })
    }

    pub fn save_state(&mut self) -> Option<Vec<u8>> {
        let mut buf = Vec::new();
        self.save_state_into(&mut buf).then_some(buf)
    }

    /// Serialize into a reused buffer (resized as needed). Cheap enough to call
    /// every frame for run-ahead. Returns false if the core has no state.
    pub fn save_state_into(&mut self, buf: &mut Vec<u8>) -> bool {
        let size = self.serialize_size();
        if size == 0 {
            return false;
        }
        buf.clear();
        buf.resize(size, 0);
        self.enter(|api| unsafe { (api.retro_serialize)(buf.as_mut_ptr() as *mut c_void, size) })
    }

    pub fn load_state(&mut self, buf: &[u8]) -> bool {
        self.enter(|api| unsafe {
            (api.retro_unserialize)(buf.as_ptr() as *const c_void, buf.len())
        })
    }

    pub fn reset(&mut self) {
        self.enter(|api| unsafe { (api.retro_reset)() });
    }

    /// Abre (`true`) ou fecha (`false`) a BANDEJA do drive pela interface de
    /// disk control que o core registrou — o jogo EMULADO enxerga a tampa
    /// abrir e reage com o próprio código (telas de erro reais). `None`
    /// quando o core não anunciou a interface.
    pub fn set_eject_state(&mut self, ejected: bool) -> Option<bool> {
        let cb = self.state.disk?;
        Some(self.enter(|_| unsafe { (cb.set_eject_state)(ejected) }))
    }

    /// Estado atual da bandeja segundo o core (`get_eject_state`). `None`
    /// sem disk control interface.
    pub fn get_eject_state(&mut self) -> Option<bool> {
        let cb = self.state.disk?;
        Some(self.enter(|_| unsafe { (cb.get_eject_state)() }))
    }

    /// Quantos discos o core tem na lista (`get_num_images`). `None` sem
    /// disk control interface.
    pub fn num_images(&mut self) -> Option<u32> {
        let cb = self.state.disk?;
        Some(self.enter(|_| unsafe { (cb.get_num_images)() }))
    }

    /// Troca o disco no drive com a sessão viva (`SET_DISK_CONTROL_EXT`,
    /// plano §6): ejeta o índice atual, insere `image` e reinicia pelo
    /// `retro_load_game` do novo caminho. O core recusa se o jogo não
    /// anunciou suporte a troca — o caller decide o fallback.
    pub fn load_disc(&mut self, path: &Path) -> Result<(), CoreError> {
        let c_path = CString::new(path.to_string_lossy().into_owned()).unwrap_or_default();
        let info = retro_game_info {
            path: c_path.as_ptr(),
            data: ptr::null(),
            size: 0,
            meta: ptr::null(),
        };
        let ok = self.enter(|api| unsafe { (api.retro_load_game)(&info) });
        if !ok {
            self.loaded = false;
            return Err(CoreError::LoadRejected);
        }
        self.loaded = true;
        self.refresh_av_info();
        Ok(())
    }

    // --- core memory (battery SRAM, work RAM) ---------------------------
    /// Snapshot a core memory region. `id` is one of `MEMORY_*`.
    pub fn memory(&mut self, id: c_uint) -> Option<Vec<u8>> {
        let (ptr, size) = self.enter(|api| unsafe {
            (
                (api.retro_get_memory_data)(id),
                (api.retro_get_memory_size)(id),
            )
        });
        if ptr.is_null() || size == 0 {
            return None;
        }
        // Safety: the core owns this buffer for the life of the loaded game.
        Some(unsafe { std::slice::from_raw_parts(ptr as *const u8, size) }.to_vec())
    }

    /// Run `f` against the live contents of a core memory region — no copy
    /// (the RetroAchievements runtime reads this every frame). `false` when
    /// the region isn't available.
    pub fn with_memory(&mut self, id: c_uint, f: impl FnOnce(&[u8])) -> bool {
        let (ptr, size) = self.enter(|api| unsafe {
            (
                (api.retro_get_memory_data)(id),
                (api.retro_get_memory_size)(id),
            )
        });
        if ptr.is_null() || size == 0 {
            return false;
        }
        // Safety: the core owns this buffer for the life of the loaded game;
        // `f` must not retain the slice past the call (enforced by lifetime).
        let live = unsafe { std::slice::from_raw_parts(ptr as *const u8, size) };
        f(live);
        true
    }

    /// Copy `bytes` back into a core memory region (e.g. restore battery SRAM
    /// after `load_game`). Extra bytes are ignored; a short slice leaves the
    /// tail untouched. Returns the number of bytes written.
    /// O ponteiro BRUTO da memória `id` e o tamanho — SEM cópia. Válido
    /// enquanto o jogo carregado não muda (load/BootBios realocam:
    /// re-consulte depois deles). A leitura na main só acontece com o
    /// worker bloqueado no recv (entre o consumir o CoreOut e enviar o
    /// próximo Run) — sem mutação concorrente.
    pub fn memory_ptr(&mut self, id: c_uint) -> Option<(*const u8, usize)> {
        let (ptr, size) = self.enter(|api| unsafe {
            (
                (api.retro_get_memory_data)(id),
                (api.retro_get_memory_size)(id),
            )
        });
        if ptr.is_null() || size == 0 {
            return None;
        }
        Some((ptr as *const u8, size))
    }

    pub fn write_memory(&mut self, id: c_uint, bytes: &[u8]) -> usize {
        let (ptr, size) = self.enter(|api| unsafe {
            (
                (api.retro_get_memory_data)(id),
                (api.retro_get_memory_size)(id),
            )
        });
        if ptr.is_null() || size == 0 {
            return 0;
        }
        let n = bytes.len().min(size);
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, n) };
        n
    }

    /// Battery-backed cartridge SRAM, if this game has any.
    pub fn sram(&mut self) -> Option<Vec<u8>> {
        self.memory(RETRO_MEMORY_SAVE_RAM)
    }

    pub fn load_sram(&mut self, bytes: &[u8]) -> usize {
        self.write_memory(RETRO_MEMORY_SAVE_RAM, bytes)
    }

    /// Drop every cheat the core is currently tracking. Call once before the
    /// first `cheat_set` of a session; not needed between later toggles —
    /// `cheat_set` replaces a slot in place.
    pub fn cheat_reset(&mut self) {
        self.enter(|api| unsafe { (api.retro_cheat_reset)() });
    }

    /// Set (or replace) the cheat at `index`. `code` is the raw/Game-Genie/Pro
    /// Action Replay string the core expects — passed through unmodified.
    pub fn cheat_set(&mut self, index: u32, enabled: bool, code: &str) {
        let Ok(c) = CString::new(code) else {
            return;
        };
        self.enter(|api| unsafe { (api.retro_cheat_set)(index, enabled, c.as_ptr()) });
    }

    /// Run `body` with the thread-local callback pointer set to our state.
    fn enter<R>(&mut self, body: impl FnOnce(&CoreApi) -> R) -> R {
        let ptr: *mut CallbackState = &mut *self.state;
        let prev = CB.with(|c| c.replace(ptr));
        let out = body(&self.api);
        CB.with(|c| c.set(prev));
        out
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        if self.loaded {
            self.enter(|api| unsafe { (api.retro_unload_game)() });
        }
        if self.inited {
            self.enter(|api| unsafe { (api.retro_deinit)() });
        }
    }
}

fn cstr_to_string(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

// --- C callbacks -----------------------------------------------------------

unsafe extern "C" fn environment_cb(cmd: c_uint, data: *mut c_void) -> bool {
    match cmd {
        RETRO_ENVIRONMENT_GET_CAN_DUPE => {
            if !data.is_null() {
                *(data as *mut bool) = true;
            }
            true
        }
        RETRO_ENVIRONMENT_GET_OVERSCAN => {
            if !data.is_null() {
                *(data as *mut bool) = false;
            }
            true
        }
        RETRO_ENVIRONMENT_SET_SUPPORT_NO_GAME => true,
        RETRO_ENVIRONMENT_SET_PIXEL_FORMAT => {
            if data.is_null() {
                return false;
            }
            let fmt = *(data as *const c_uint);
            let mapped = match fmt {
                RETRO_PIXEL_FORMAT_0RGB1555 => Some(PixelFormat::Rgb1555),
                RETRO_PIXEL_FORMAT_XRGB8888 => Some(PixelFormat::Xrgb8888),
                RETRO_PIXEL_FORMAT_RGB565 => Some(PixelFormat::Rgb565),
                _ => None,
            };
            match mapped {
                Some(f) => {
                    with_cb(|s| s.pixel_format = f);
                    true
                }
                None => false,
            }
        }
        RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY | RETRO_ENVIRONMENT_GET_CORE_ASSETS_DIRECTORY => {
            if data.is_null() {
                return false;
            }
            with_cb(|s| {
                *(data as *mut *const c_char) = s.system_directory.as_ptr();
            });
            true
        }
        RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY => {
            if data.is_null() {
                return false;
            }
            with_cb(|s| {
                *(data as *mut *const c_char) = s.save_directory.as_ptr();
            });
            true
        }
        RETRO_ENVIRONMENT_GET_VARIABLE => {
            if data.is_null() {
                return false;
            }
            let var = &mut *(data as *mut retro_variable);
            let key = if var.key.is_null() {
                return false;
            } else {
                CStr::from_ptr(var.key)
            };
            with_cb(|s| {
                for (k, v) in &s.variables {
                    if k.as_c_str() == key {
                        var.value = v.as_ptr();
                        return true;
                    }
                }
                false
            })
            .unwrap_or(false)
        }
        RETRO_ENVIRONMENT_SET_VARIABLES => {
            if data.is_null() {
                return true;
            }
            let mut p = data as *const retro_variable;
            with_cb(|s| {
                s.variables.clear();
                while !(*p).key.is_null() {
                    let key = CStr::from_ptr((*p).key).to_owned();
                    // value looks like "Description; opt_a|opt_b|opt_c"
                    let raw = CStr::from_ptr((*p).value).to_string_lossy();
                    let default = raw
                        .split(';')
                        .nth(1)
                        .unwrap_or("")
                        .trim()
                        .split('|')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if let Ok(v) = CString::new(default) {
                        s.variables.push((key, v));
                    }
                    p = p.add(1);
                }
                s.variables_dirty = true;
            });
            true
        }
        RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE => {
            with_cb(|s| {
                let was = s.variables_dirty;
                s.variables_dirty = false;
                if !data.is_null() {
                    *(data as *mut bool) = was;
                }
            });
            true
        }
        RETRO_ENVIRONMENT_GET_CORE_OPTIONS_VERSION => {
            // We speak v2: the core pushes its definitions through
            // SET_CORE_OPTIONS_V2 and we take the table from there (the
            // legacy SET_VARIABLES path stays as the fallback for cores
            // that ignore versions, like snes9x did).
            if !data.is_null() {
                *(data as *mut c_uint) = 2;
            }
            true
        }
        RETRO_ENVIRONMENT_SET_CORE_OPTIONS => {
            if data.is_null() {
                return true;
            }
            let defs = data as *const sys::retro_core_option_definition;
            with_cb(|s| unsafe { take_options_v1(s, defs) });
            true
        }
        RETRO_ENVIRONMENT_SET_CORE_OPTIONS_V2 => {
            if data.is_null() {
                return true;
            }
            let opts = data as *const sys::retro_core_options_v2;
            with_cb(|s| unsafe { take_options_v2(s, (*opts).definitions) });
            true
        }
        RETRO_ENVIRONMENT_GET_PREFERRED_HW_RENDER => {
            // Software-only: sem preferência de HW, o core fica no padrão
            // (o app fixa `pcsx_rearmed_nocdaudio`/`show_bios_bootlogo` por cima).
            false
        }
        RETRO_ENVIRONMENT_GET_DISK_CONTROL_INTERFACE_VERSION => {
            if data.is_null() {
                return false;
            }
            // Falamos a interface EXT (v1) — sem isto o core registra só a
            // legada (cmd 13) e a bandeja não existe.
            unsafe { *(data as *mut c_uint) = 1 };
            true
        }
        sys::RETRO_ENVIRONMENT_SET_DISK_CONTROL_EXT_INTERFACE => {
            if data.is_null() {
                return false;
            }
            let cb = data as *const sys::retro_disk_control_ext_callback;
            with_cb(|st| unsafe {
                st.disk = Some(std::ptr::read(cb));
            });
            true
        }
        sys::RETRO_ENVIRONMENT_SET_CORE_OPTIONS_V2_INTL => {
            if data.is_null() {
                return true;
            }
            let intl = data as *const sys::retro_core_options_v2_intl;
            with_cb(|s| unsafe {
                // A tabela canônica `us` é sempre preenchida; usamos a
                // localizada só quando a canônica não vier.
                let opts = if !(*intl).us.is_null() {
                    (*intl).us
                } else {
                    (*intl).local
                };
                if !opts.is_null() {
                    take_options_v2(s, (*opts).definitions);
                }
            });
            true
        }
        RETRO_ENVIRONMENT_SET_SYSTEM_AV_INFO | RETRO_ENVIRONMENT_SET_GEOMETRY => {
            with_cb(|s| s.av_info_dirty = true);
            true
        }
        RETRO_ENVIRONMENT_GET_INPUT_BITMASKS => true,
        RETRO_ENVIRONMENT_SHUTDOWN => true,
        _ => false,
    }
}

#[repr(C)]
struct retro_variable {
    key: *const c_char,
    value: *const c_char,
}

/// A core pushed its option definitions (v1 shape): seed the variable table
/// with each option's default, keeping any value the frontend already set.
unsafe fn take_options_v1(s: &mut CallbackState, defs: *const sys::retro_core_option_definition) {
    if defs.is_null() {
        return;
    }
    let mut count = 0usize;
    let mut i = 0usize;
    unsafe {
        while !(*defs.add(i)).key.is_null() {
            let d = &*defs.add(i);
            upsert_option(
                s,
                CStr::from_ptr(d.key),
                default_of(&d.values, d.default_value),
            );
            count += 1;
            i += 1;
        }
    }
    log::debug!("core options (v1): {count} chaves");
    s.variables_dirty = true;
}

/// Same for the v2 shape (`SET_CORE_OPTIONS_V2` — PCSX Rearmed's path).
unsafe fn take_options_v2(
    s: &mut CallbackState,
    defs: *const sys::retro_core_option_v2_definition,
) {
    if defs.is_null() {
        return;
    }
    let mut count = 0usize;
    let mut i = 0usize;
    unsafe {
        while !(*defs.add(i)).key.is_null() {
            let d = &*defs.add(i);
            upsert_option(
                s,
                CStr::from_ptr(d.key),
                default_of(&d.values, d.default_value),
            );
            count += 1;
            i += 1;
        }
    }
    log::debug!("core options (v2): {count} chaves");
    s.variables_dirty = true;
}

/// Upsert `key → default` unless the frontend already pinned a value for it
/// (a pre-seeded override survives the definitions arriving later).
fn upsert_option(s: &mut CallbackState, key: &std::ffi::CStr, default: Option<String>) {
    let Some(default) = default else { return };
    if let Some(slot) = s.variables.iter_mut().find(|(k, _)| k.as_c_str() == key) {
        if slot.1.to_string_lossy().is_empty() {
            slot.1 = CString::new(default).unwrap_or_default();
        }
        return;
    }
    if let Ok(v) = CString::new(default) {
        s.variables.push((key.to_owned(), v));
    }
}

/// The option's default: `default_value` when set, else the first listed
/// value (libretro's documented fallback).
fn default_of(
    values: &[sys::retro_core_option_value; sys::RETRO_NUM_CORE_OPTION_VALUES_MAX],
    default_value: *const c_char,
) -> Option<String> {
    unsafe {
        if !default_value.is_null() {
            return Some(CStr::from_ptr(default_value).to_string_lossy().into_owned());
        }
        // O primeiro valor listado é o fallback documentado do libretro
        // (o closure já roda dentro do `unsafe` externo).
        values
            .iter()
            .find(|v| !v.value.is_null())
            .map(|v| CStr::from_ptr(v.value).to_string_lossy().into_owned())
    }
}

unsafe extern "C" fn video_refresh_cb(
    data: *const c_void,
    width: c_uint,
    height: c_uint,
    pitch: usize,
) {
    if data.is_null() {
        // Duped frame: core is telling us to repeat the previous one.
        with_cb(|s| s.frame_duped = true);
        return;
    }
    with_cb(|s| {
        let bpp = s.pixel_format.bytes_per_pixel();
        let row_bytes = width as usize * bpp;
        let mut pixels = Vec::with_capacity(row_bytes * height as usize);
        let src = data as *const u8;
        for row in 0..height as usize {
            let start = row * pitch;
            let line = std::slice::from_raw_parts(src.add(start), row_bytes);
            pixels.extend_from_slice(line);
        }
        s.frame = Some(Frame {
            width,
            height,
            pitch: row_bytes,
            format: s.pixel_format,
            pixels,
        });
    });
}

unsafe extern "C" fn audio_sample_cb(left: i16, right: i16) {
    with_cb(|s| {
        s.audio.push(left);
        s.audio.push(right);
    });
}

unsafe extern "C" fn audio_sample_batch_cb(data: *const i16, frames: usize) -> usize {
    if !data.is_null() {
        let slice = std::slice::from_raw_parts(data, frames * 2);
        with_cb(|s| s.audio.extend_from_slice(slice));
    }
    frames
}

unsafe extern "C" fn input_poll_cb() {
    // Frontend refreshes its matrix before run(); nothing to do here.
}

unsafe extern "C" fn input_state_cb(
    port: c_uint,
    device: c_uint,
    index: c_uint,
    id: c_uint,
) -> i16 {
    let port = port as usize;
    if port >= MAX_PORTS {
        return 0;
    }
    if device == RETRO_DEVICE_ANALOG {
        // DualShock sticks: index picks the stick, id the axis.
        let (x, y) = with_cb(|s| s.analog[port][index as usize]).unwrap_or((0, 0));
        return match id {
            RETRO_DEVICE_ID_ANALOG_X => x,
            RETRO_DEVICE_ID_ANALOG_Y => y,
            _ => 0,
        };
    }
    if device != RETRO_DEVICE_JOYPAD {
        return 0;
    }
    with_cb(|s| {
        if id == RETRO_DEVICE_ID_JOYPAD_MASK {
            let mut mask: i16 = 0;
            for (i, held) in s.input[port].iter().enumerate() {
                if *held {
                    mask |= 1 << i;
                }
            }
            mask
        } else if (id as usize) < 16 && s.input[port][id as usize] {
            1
        } else {
            0
        }
    })
    .unwrap_or(0)
}
