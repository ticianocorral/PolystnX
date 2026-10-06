//! Raw FFI mirror of the parts of `libretro.h` the frontend needs for Phase 0.
//!
//! Only what is exercised by booting a core, running frames, pumping audio and
//! reading input is declared here. The full API is large; this grows as later
//! phases need it (serialize/unserialize for run-ahead is already included).
#![allow(non_camel_case_types, dead_code)]

use std::os::raw::{c_char, c_uint, c_void};

pub const RETRO_API_VERSION: c_uint = 1;

// --- Environment command ids (subset) --------------------------------------
pub const RETRO_ENVIRONMENT_SET_ROTATION: c_uint = 1;
pub const RETRO_ENVIRONMENT_GET_OVERSCAN: c_uint = 2;
pub const RETRO_ENVIRONMENT_GET_CAN_DUPE: c_uint = 3;
pub const RETRO_ENVIRONMENT_SET_MESSAGE: c_uint = 6;
pub const RETRO_ENVIRONMENT_SHUTDOWN: c_uint = 7;
pub const RETRO_ENVIRONMENT_SET_PERFORMANCE_LEVEL: c_uint = 8;
pub const RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY: c_uint = 9;
pub const RETRO_ENVIRONMENT_SET_PIXEL_FORMAT: c_uint = 10;
pub const RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS: c_uint = 11;
pub const RETRO_ENVIRONMENT_SET_KEYBOARD_CALLBACK: c_uint = 12;
pub const RETRO_ENVIRONMENT_GET_VARIABLE: c_uint = 15;
pub const RETRO_ENVIRONMENT_SET_VARIABLES: c_uint = 16;
pub const RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE: c_uint = 17;
pub const RETRO_ENVIRONMENT_SET_SUPPORT_NO_GAME: c_uint = 18;
pub const RETRO_ENVIRONMENT_GET_LIBRETRO_PATH: c_uint = 19;
pub const RETRO_ENVIRONMENT_GET_LOG_INTERFACE: c_uint = 27;
pub const RETRO_ENVIRONMENT_GET_PERF_INTERFACE: c_uint = 28;
pub const RETRO_ENVIRONMENT_GET_CORE_ASSETS_DIRECTORY: c_uint = 30;
pub const RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY: c_uint = 31;
pub const RETRO_ENVIRONMENT_SET_SYSTEM_AV_INFO: c_uint = 32;
pub const RETRO_ENVIRONMENT_SET_GEOMETRY: c_uint = 37;
pub const RETRO_ENVIRONMENT_GET_INPUT_BITMASKS: c_uint = 51 | 0x10000;
pub const RETRO_ENVIRONMENT_GET_CORE_OPTIONS_VERSION: c_uint = 52;
pub const RETRO_ENVIRONMENT_SET_CORE_OPTIONS: c_uint = 53;
pub const RETRO_ENVIRONMENT_SET_CORE_OPTIONS_INTL: c_uint = 54;
/// O core pergunta o renderizador de HW preferido (`unsigned*` para
/// preencher). Respondemos `false` — o pipeline do app é 2D/software.
pub const RETRO_ENVIRONMENT_GET_PREFERRED_HW_RENDER: c_uint = 56;
/// Versão da interface de disk control que o frontend suporta (`unsigned*`):
/// 0 = só a legada (cmd 13), ≥ 1 = o core deve registrar a EXT (cmd 58).
pub const RETRO_ENVIRONMENT_GET_DISK_CONTROL_INTERFACE_VERSION: c_uint = 57;
/// A bandeja do drive: `retro_disk_control_ext_callback`.
pub const RETRO_ENVIRONMENT_SET_DISK_CONTROL_EXT_INTERFACE: c_uint = 58;
pub const RETRO_ENVIRONMENT_SET_CORE_OPTIONS_V2: c_uint = 67;
pub const RETRO_ENVIRONMENT_SET_CORE_OPTIONS_V2_INTL: c_uint = 68;

// --- Pixel format --------------------------------------------------------------
pub const RETRO_PIXEL_FORMAT_0RGB1555: c_uint = 0;
pub const RETRO_PIXEL_FORMAT_XRGB8888: c_uint = 1;
pub const RETRO_PIXEL_FORMAT_RGB565: c_uint = 2;

// --- Input devices -----------------------------------------------------------
pub const RETRO_DEVICE_NONE: c_uint = 0;
pub const RETRO_DEVICE_JOYPAD: c_uint = 1;
pub const RETRO_DEVICE_ANALOG: c_uint = 5;

pub const RETRO_DEVICE_INDEX_ANALOG_LEFT: c_uint = 0;
pub const RETRO_DEVICE_INDEX_ANALOG_RIGHT: c_uint = 1;
pub const RETRO_DEVICE_ID_ANALOG_X: c_uint = 0;
pub const RETRO_DEVICE_ID_ANALOG_Y: c_uint = 1;

pub const RETRO_DEVICE_ID_JOYPAD_B: c_uint = 0;
pub const RETRO_DEVICE_ID_JOYPAD_Y: c_uint = 1;
pub const RETRO_DEVICE_ID_JOYPAD_SELECT: c_uint = 2;
pub const RETRO_DEVICE_ID_JOYPAD_START: c_uint = 3;
pub const RETRO_DEVICE_ID_JOYPAD_UP: c_uint = 4;
pub const RETRO_DEVICE_ID_JOYPAD_DOWN: c_uint = 5;
pub const RETRO_DEVICE_ID_JOYPAD_LEFT: c_uint = 6;
pub const RETRO_DEVICE_ID_JOYPAD_RIGHT: c_uint = 7;
pub const RETRO_DEVICE_ID_JOYPAD_A: c_uint = 8;
pub const RETRO_DEVICE_ID_JOYPAD_X: c_uint = 9;
pub const RETRO_DEVICE_ID_JOYPAD_L: c_uint = 10;
pub const RETRO_DEVICE_ID_JOYPAD_R: c_uint = 11;
pub const RETRO_DEVICE_ID_JOYPAD_L2: c_uint = 12;
pub const RETRO_DEVICE_ID_JOYPAD_R2: c_uint = 13;
pub const RETRO_DEVICE_ID_JOYPAD_L3: c_uint = 14;
pub const RETRO_DEVICE_ID_JOYPAD_R3: c_uint = 15;
pub const RETRO_DEVICE_ID_JOYPAD_MASK: c_uint = 256;

pub const RETRO_REGION_NTSC: c_uint = 0;
pub const RETRO_REGION_PAL: c_uint = 1;

// --- Memory region ids -----------------------------------------------------
pub const RETRO_MEMORY_SAVE_RAM: c_uint = 0;
pub const RETRO_MEMORY_RTC: c_uint = 1;
pub const RETRO_MEMORY_SYSTEM_RAM: c_uint = 2;
pub const RETRO_MEMORY_VIDEO_RAM: c_uint = 3;

pub const RETRO_LOG_DEBUG: c_uint = 0;
pub const RETRO_LOG_INFO: c_uint = 1;
pub const RETRO_LOG_WARN: c_uint = 2;
pub const RETRO_LOG_ERROR: c_uint = 3;

#[repr(C)]
pub struct retro_system_info {
    pub library_name: *const c_char,
    pub library_version: *const c_char,
    pub valid_extensions: *const c_char,
    pub need_fullpath: bool,
    pub block_extract: bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct retro_game_geometry {
    pub base_width: c_uint,
    pub base_height: c_uint,
    pub max_width: c_uint,
    pub max_height: c_uint,
    pub aspect_ratio: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct retro_system_timing {
    pub fps: f64,
    pub sample_rate: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct retro_system_av_info {
    pub geometry: retro_game_geometry,
    pub timing: retro_system_timing,
}

#[repr(C)]
pub struct retro_game_info {
    pub path: *const c_char,
    pub data: *const c_void,
    pub size: usize,
    pub meta: *const c_char,
}

#[repr(C)]
pub struct retro_log_callback {
    pub log: Option<unsafe extern "C" fn(level: c_uint, fmt: *const c_char, ...)>,
}

// --- Core options (v1/v2; libretro.h) ---------------------------------------
/// Verbatim from `libretro.h` — the arrays are inline and NULL-terminated, so
/// the struct layout is the ABI. Parsing these lets the frontend enumerate a
/// core's options (settings screen) and take over defaults by key.
pub const RETRO_NUM_CORE_OPTION_VALUES_MAX: usize = 128;

#[repr(C)]
pub struct retro_core_option_value {
    pub value: *const c_char,
    pub label: *const c_char,
}

#[repr(C)]
pub struct retro_core_option_definition {
    pub key: *const c_char,
    pub desc: *const c_char,
    pub info: *const c_char,
    pub values: [retro_core_option_value; RETRO_NUM_CORE_OPTION_VALUES_MAX],
    pub default_value: *const c_char,
}

#[repr(C)]
pub struct retro_core_option_v2_category {
    pub key: *const c_char,
    pub desc: *const c_char,
    pub info: *const c_char,
}

#[repr(C)]
pub struct retro_core_option_v2_definition {
    pub key: *const c_char,
    pub desc: *const c_char,
    pub desc_categorized: *const c_char,
    pub info: *const c_char,
    pub info_categorized: *const c_char,
    pub category_key: *const c_char,
    pub values: [retro_core_option_value; RETRO_NUM_CORE_OPTION_VALUES_MAX],
    pub default_value: *const c_char,
}

#[repr(C)]
pub struct retro_core_options_v2 {
    pub categories: *mut retro_core_option_v2_category,
    pub definitions: *mut retro_core_option_v2_definition,
}

/// `SET_CORE_OPTIONS_V2_INTL` (68): o mesmo par de tabelas em duas línguas —
/// `us` é a canônica (sempre preenchida), `local` a traduzida.
#[repr(C)]
pub struct retro_core_options_v2_intl {
    pub us: *mut retro_core_options_v2,
    pub local: *mut retro_core_options_v2,
}

pub type retro_environment_t = unsafe extern "C" fn(cmd: c_uint, data: *mut c_void) -> bool;
pub type retro_video_refresh_t =
    unsafe extern "C" fn(data: *const c_void, width: c_uint, height: c_uint, pitch: usize);
pub type retro_audio_sample_t = unsafe extern "C" fn(left: i16, right: i16);
pub type retro_audio_sample_batch_t =
    unsafe extern "C" fn(data: *const i16, frames: usize) -> usize;
pub type retro_input_poll_t = unsafe extern "C" fn();
pub type retro_input_state_t =
    unsafe extern "C" fn(port: c_uint, device: c_uint, index: c_uint, id: c_uint) -> i16;

// Function-pointer types for every entry point we resolve. Spelled out so the
// symbol-loading macro can name each one.
pub type FnVoid = unsafe extern "C" fn();
pub type FnU32 = unsafe extern "C" fn() -> c_uint;
pub type FnUsize = unsafe extern "C" fn() -> usize;
pub type FnGetSystemInfo = unsafe extern "C" fn(*mut retro_system_info);
pub type FnGetSystemAvInfo = unsafe extern "C" fn(*mut retro_system_av_info);
pub type FnSetEnvironment = unsafe extern "C" fn(retro_environment_t);
pub type FnSetVideoRefresh = unsafe extern "C" fn(retro_video_refresh_t);
pub type FnSetAudioSample = unsafe extern "C" fn(retro_audio_sample_t);
pub type FnSetAudioSampleBatch = unsafe extern "C" fn(retro_audio_sample_batch_t);
pub type FnSetInputPoll = unsafe extern "C" fn(retro_input_poll_t);
pub type FnSetInputState = unsafe extern "C" fn(retro_input_state_t);
pub type FnSetControllerPortDevice = unsafe extern "C" fn(c_uint, c_uint);
pub type FnLoadGame = unsafe extern "C" fn(*const retro_game_info) -> bool;
pub type FnSerialize = unsafe extern "C" fn(*mut c_void, usize) -> bool;
pub type FnUnserialize = unsafe extern "C" fn(*const c_void, usize) -> bool;
pub type FnGetMemoryData = unsafe extern "C" fn(c_uint) -> *mut c_void;
pub type FnGetMemorySize = unsafe extern "C" fn(c_uint) -> usize;
pub type FnCheatReset = unsafe extern "C" fn();
pub type FnCheatSet = unsafe extern "C" fn(index: c_uint, enabled: bool, code: *const c_char);

/// Symbols resolved from the shared object. Names match `libretro.h` exactly.
pub struct CoreApi {
    pub retro_api_version: FnU32,
    pub retro_get_system_info: FnGetSystemInfo,
    pub retro_get_system_av_info: FnGetSystemAvInfo,
    pub retro_set_environment: FnSetEnvironment,
    pub retro_set_video_refresh: FnSetVideoRefresh,
    pub retro_set_audio_sample: FnSetAudioSample,
    pub retro_set_audio_sample_batch: FnSetAudioSampleBatch,
    pub retro_set_input_poll: FnSetInputPoll,
    pub retro_set_input_state: FnSetInputState,
    pub retro_set_controller_port_device: FnSetControllerPortDevice,
    pub retro_init: FnVoid,
    pub retro_deinit: FnVoid,
    pub retro_load_game: FnLoadGame,
    pub retro_unload_game: FnVoid,
    pub retro_run: FnVoid,
    pub retro_reset: FnVoid,
    pub retro_get_region: FnU32,
    pub retro_serialize_size: FnUsize,
    pub retro_serialize: FnSerialize,
    pub retro_unserialize: FnUnserialize,
    pub retro_get_memory_data: FnGetMemoryData,
    pub retro_get_memory_size: FnGetMemorySize,
    pub retro_cheat_reset: FnCheatReset,
    pub retro_cheat_set: FnCheatSet,
}

// --- disk control ext (a bandeja do drive) ----------------------------------

pub type retro_set_eject_state_t = unsafe extern "C" fn(ejected: bool) -> bool;
pub type retro_get_eject_state_t = unsafe extern "C" fn() -> bool;
pub type retro_set_image_index_t = unsafe extern "C" fn(index: c_uint) -> bool;
pub type retro_get_image_index_t = unsafe extern "C" fn() -> c_uint;
pub type retro_get_num_images_t = unsafe extern "C" fn() -> c_uint;
pub type retro_replace_image_index_t =
    unsafe extern "C" fn(index: c_uint, info: *const retro_game_info) -> bool;
pub type retro_add_image_index_t = unsafe extern "C" fn() -> bool;
pub type retro_set_initial_image_t =
    unsafe extern "C" fn(index: c_uint, path: *const c_char) -> bool;
pub type retro_get_image_path_t = unsafe extern "C" fn(index: c_uint) -> *const c_char;
pub type retro_get_image_label_t = unsafe extern "C" fn(index: c_uint) -> *const c_char;

/// `RETRO_ENVIRONMENT_SET_DISK_CONTROL_EXT_INTERFACE` (56): a bandeja que o
/// O core registra — ejetar/insertar por aqui é o que faz os jogos
/// VEREM a bandeja abrir (as telas de erro de leitura deles são reais).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct retro_disk_control_ext_callback {
    pub set_eject_state: retro_set_eject_state_t,
    pub get_eject_state: retro_get_eject_state_t,
    pub set_image_index: retro_set_image_index_t,
    pub get_image_index: retro_get_image_index_t,
    pub get_num_images: retro_get_num_images_t,
    pub replace_image_index: retro_replace_image_index_t,
    pub add_image_index: retro_add_image_index_t,
    pub set_initial_image: Option<retro_set_initial_image_t>,
    pub get_image_path: Option<retro_get_image_path_t>,
    pub get_image_label: Option<retro_get_image_label_t>,
}
