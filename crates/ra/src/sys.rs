//! Hand-written bindings for the vendored rcheevos runtime subset the app
//! uses — a dozen functions, small enough that bindgen isn't a dependency
//! worth carrying. Layouts mirror `include/rc_runtime.h` /
//! `rc_runtime_types.h` (develop snapshot); the runtime itself is opaque.
#![allow(non_camel_case_types, dead_code)]

use std::ffi::{c_char, c_int, c_uchar, c_uint, c_void};

/// Opaque: real definition in `rc_runtime.h`; only ever passed back to C.
pub enum rc_runtime_t {}

pub type rc_runtime_read_memory_func_t = unsafe extern "C" fn(
    address: c_uint,
    buffer: *mut c_uchar,
    num_bytes: c_uint,
    ud: *mut c_void,
) -> c_uint;

pub type rc_runtime_event_handler_t = unsafe extern "C" fn(event: *const rc_runtime_event_t);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct rc_runtime_event_t {
    pub id: c_uint,
    pub value: c_int,
    pub type_: c_uchar,
}

// `rc_runtime_event_type_t` — the enum starts at 0 (ACTIVATED).
pub const RC_RUNTIME_EVENT_ACHIEVEMENT_ACTIVATED: c_uchar = 0;
pub const RC_RUNTIME_EVENT_ACHIEVEMENT_PAUSED: c_uchar = 1;
pub const RC_RUNTIME_EVENT_ACHIEVEMENT_RESET: c_uchar = 2;
pub const RC_RUNTIME_EVENT_ACHIEVEMENT_TRIGGERED: c_uchar = 3;
pub const RC_RUNTIME_EVENT_ACHIEVEMENT_PRIMED: c_uchar = 4;

extern "C" {
    pub fn rc_runtime_alloc() -> *mut rc_runtime_t;
    pub fn rc_runtime_destroy(runtime: *mut rc_runtime_t);
    pub fn rc_runtime_activate_achievement(
        runtime: *mut rc_runtime_t,
        id: c_uint,
        memaddr: *const c_char,
        unused_l: *mut c_void,
        unused_funcs_idx: c_int,
    ) -> c_int;
    pub fn rc_runtime_deactivate_achievement(runtime: *mut rc_runtime_t, id: c_uint);
    pub fn rc_runtime_get_achievement(runtime: *const rc_runtime_t, id: c_uint) -> *mut c_void;
    pub fn rc_runtime_do_frame(
        runtime: *mut rc_runtime_t,
        event_handler: rc_runtime_event_handler_t,
        read_memory: rc_runtime_read_memory_func_t,
        ud: *mut c_void,
        unused_l: *mut c_void,
    );
    pub fn rc_runtime_reset(runtime: *mut rc_runtime_t);
    pub fn rc_runtime_progress_size(runtime: *const rc_runtime_t, unused_l: *mut c_void) -> c_uint;
    pub fn rc_runtime_serialize_progress_sized(
        buffer: *mut c_uchar,
        buffer_size: c_uint,
        runtime: *const rc_runtime_t,
        unused_l: *mut c_void,
    ) -> c_int;
    pub fn rc_runtime_deserialize_progress_sized(
        runtime: *mut rc_runtime_t,
        serialized: *const c_uchar,
        serialized_size: c_uint,
        unused_l: *mut c_void,
    ) -> c_int;
}

/// `rc_trigger_t.state`, read through the accessor the header layout gives
/// us: `state` sits after two pointers + two u32s in `struct rc_trigger_t`.
/// Only ever read via `trigger_state`, which re-reads the same layout the
/// vendored header compiles against — the crate builds that header, so a
/// mismatch would be caught by this crate's own unit test.
pub const RC_TRIGGER_STATE_INACTIVE: u8 = 0;
pub const RC_TRIGGER_STATE_WAITING: u8 = 1;
pub const RC_TRIGGER_STATE_ACTIVE: u8 = 2;
pub const RC_TRIGGER_STATE_PAUSED: u8 = 3;
pub const RC_TRIGGER_STATE_RESET: u8 = 4;
pub const RC_TRIGGER_STATE_TRIGGERED: u8 = 5;
pub const RC_TRIGGER_STATE_PRIMED: u8 = 6;
pub const RC_TRIGGER_STATE_DISABLED: u8 = 7;

// Byte offset of `state` in `struct rc_trigger_t` (see header: requirement
// ptr, alternative ptr, measured_value u32, measured_target u32, state).
extern "C" {
    /// Compiled from `state_offset.c` against the same vendored header —
    /// the one source of truth for the layout.
    pub fn ra_trigger_state_offset() -> u32;
}

/// # Safety
/// `trigger` must be a live `rc_trigger_t*` returned by
/// `rc_runtime_get_achievement` (or null), and `state_offset` the C-side
/// `offsetof(rc_trigger_t, state)`.
pub unsafe fn trigger_state(trigger: *mut c_void, state_offset: usize) -> u8 {
    if trigger.is_null() {
        return RC_TRIGGER_STATE_INACTIVE;
    }
    unsafe { *(trigger as *const u8).add(state_offset) }
}

// --- rhash (disc hashing; include/rc_hash.h) -------------------------------

/// `RC_CONSOLE_PLAYSTATION` — the console id the RA server hashes PSX discs
/// against (`include/rc_consoles.h`).
pub const RC_CONSOLE_PLAYSTATION: c_uint = 12;

/// The hash buffer `rc_hash_*` writes into: 32 hex chars + NUL.
pub const RC_HASH_SIZE: usize = 33;

pub type rc_hash_open_track_fn =
    unsafe extern "C" fn(path: *const c_char, track: c_uint) -> *mut c_void;
pub type rc_hash_read_sector_fn = unsafe extern "C" fn(
    track_handle: *mut c_void,
    sector: c_uint,
    buffer: *mut c_void,
    requested_bytes: usize,
) -> usize;
pub type rc_hash_close_track_fn = unsafe extern "C" fn(track_handle: *mut c_void);
pub type rc_hash_first_track_sector_fn = unsafe extern "C" fn(track_handle: *mut c_void) -> c_uint;
pub type rc_hash_open_track_iterator_fn = unsafe extern "C" fn(
    path: *const c_char,
    track: c_uint,
    iterator: *const c_void,
) -> *mut c_void;

/// `struct rc_hash_cdreader` — field order is the ABI.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct rc_hash_cdreader_t {
    pub open_track: Option<rc_hash_open_track_fn>,
    pub read_sector: Option<rc_hash_read_sector_fn>,
    pub close_track: Option<rc_hash_close_track_fn>,
    pub first_track_sector: Option<rc_hash_first_track_sector_fn>,
    pub open_track_iterator: Option<rc_hash_open_track_iterator_fn>,
}

pub type rc_hash_message_callback_t = unsafe extern "C" fn(msg: *const c_char);

extern "C" {
    pub fn rc_hash_generate_from_file(
        hash: *mut c_char,
        console_id: c_uint,
        path: *const c_char,
    ) -> c_int;
    pub fn rc_hash_init_custom_cdreader(reader: *const rc_hash_cdreader_t);
    pub fn rc_hash_get_default_cdreader(cdreader: *mut rc_hash_cdreader_t);
    pub fn rc_hash_init_error_message_callback(callback: Option<rc_hash_message_callback_t>);
    pub fn rc_hash_init_verbose_message_callback(callback: Option<rc_hash_message_callback_t>);
}
