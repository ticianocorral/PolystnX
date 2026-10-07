//! The app-facing runtime: one `Session` per game insertion, driven from
//! the frame loop (`runner`). Achievements are activated from the cached
//! `API_GetGameExtended` JSON (their `MemAddr` definition strings are the
//! rcheevos grammar verbatim); `tick` evaluates one frame; triggered ids
//! come back out. Memory reads go through the caller's slice so the
//! session never knows about the emulator.

use crate::sys;
use std::ffi::{c_void, CString};
use std::sync::OnceLock;

/// One achievement as the runtime needs it — parsed out of the cached API
/// JSON by the app crate (which owns serde).
#[derive(Debug, Clone)]
pub struct Achievement {
    pub id: u32,
    /// The notification's headline ("CONQUISTA DESBLOQUEADA" block).
    pub title: String,
    pub description: String,
    pub points: u32,
    /// BadgeName from the API — the badge image id on the RA CDN.
    pub badge: String,
    /// The rcheevos definition (`MemAddr`), e.g. "0xH0042=10".
    pub memaddr: String,
}

/// C's own answer for where `state` lives in `struct rc_trigger_t` —
/// resolved once, so the Rust reader can never drift from the header the
/// vendored C actually compiled against.
fn state_offset() -> usize {
    static OFF: OnceLock<u32> = OnceLock::new();
    *OFF.get_or_init(|| unsafe { crate::sys::ra_trigger_state_offset() }) as usize
}

/// A live evaluation session. Every method works even with achievements
/// that failed to parse (they're skipped at `from_parts`).
pub struct Session {
    rt: *mut sys::rc_runtime_t,
    pub achievements: Vec<Achievement>,
}

// The runtime pointer only ever moves between the frame-loop thread and
// the (de)serialization helpers; no shared mutable state crosses threads.
unsafe impl Send for Session {}

impl Session {
    /// Activate each achievement's definition in a fresh rcheevos runtime.
    /// An achievement whose `memaddr` fails to parse is skipped with a log
    /// line — never fatal to the session.
    pub fn from_parts(achievements: Vec<Achievement>) -> Session {
        unsafe {
            let rt = sys::rc_runtime_alloc();
            let s = Session { rt, achievements };
            for a in &s.achievements {
                let Ok(c) = CString::new(a.memaddr.as_str()) else {
                    continue;
                };
                let rc = sys::rc_runtime_activate_achievement(
                    s.rt,
                    a.id,
                    c.as_ptr(),
                    std::ptr::null_mut(),
                    0,
                );
                if rc != 0 {
                    log::info!("ra: ativando conquista {}: rc {rc}", a.id);
                }
            }
            s
        }
    }

    /// Evaluate one frame against `mem` (the emulator work RAM). Returns the
    /// ids that triggered this frame.
    pub fn tick(&mut self, mem: &[u8]) -> Vec<u32> {
        // The do_frame callbacks carry no ud that reaches the event handler
        // (`rc_runtime_event_handler_t` gets only the event), so the frame
        // context rides in a slot — the frame loop is single-threaded, and
        // the slot is empty outside `tick`. The slot is per-thread (not
        // global) so two Sessions ticking on different threads — the test
        // binary runs each #[test] on its own — never see each other's
        // pointers: a global slot here once null-derefs'd under exactly
        // that parallelism.
        struct Ctx<'a> {
            mem: &'a [u8],
            triggered: Vec<u32>,
        }
        thread_local! {
            static CTX: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
        }
        let mut ctx = Ctx {
            mem,
            triggered: Vec::new(),
        };
        CTX.with(|slot| slot.set(&mut ctx as *mut Ctx as usize));

        unsafe extern "C" fn peek(
            address: u32,
            buffer: *mut u8,
            num_bytes: u32,
            _ud: *mut c_void,
        ) -> u32 {
            let ctx = unsafe { &mut *(CTX.with(|slot| slot.get()) as *mut Ctx) };
            let addr = address as usize;
            for i in 0..num_bytes as usize {
                unsafe {
                    *buffer.add(i) = ctx.mem.get(addr + i).copied().unwrap_or(0);
                }
            }
            0
        }
        unsafe extern "C" fn handler(event: *const sys::rc_runtime_event_t) {
            let ev = unsafe { &*event };
            if ev.type_ == sys::RC_RUNTIME_EVENT_ACHIEVEMENT_TRIGGERED {
                let ctx = unsafe { &mut *(CTX.with(|slot| slot.get()) as *mut Ctx) };
                ctx.triggered.push(ev.id);
            }
        }
        unsafe {
            sys::rc_runtime_do_frame(
                self.rt,
                handler,
                peek,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
        }
        CTX.with(|slot| slot.set(0));
        std::mem::take(&mut ctx.triggered)
    }

    /// Which of the session's achievements are in TRIGGERED state — after
    /// loading progress or receiving events, for the unlocked bookkeeping.
    pub fn triggered_ids(&self) -> Vec<u32> {
        unsafe {
            self.achievements
                .iter()
                .map(|a| a.id)
                .filter(|&id| {
                    let t = sys::rc_runtime_get_achievement(self.rt, id);
                    sys::trigger_state(t, state_offset()) == sys::RC_TRIGGER_STATE_TRIGGERED
                })
                .collect()
        }
    }

    /// Hard reset of all hit counts / trigger state (console power-on).
    pub fn reset(&mut self) {
        unsafe { sys::rc_runtime_reset(self.rt) }
    }

    /// Serialize the session (hit counts, trigger states) for save-on-exit.
    /// Empty on nothing-to-save.
    pub fn save_progress(&mut self) -> Vec<u8> {
        unsafe {
            let size = sys::rc_runtime_progress_size(self.rt, std::ptr::null_mut());
            if size == 0 {
                return Vec::new();
            }
            let mut buf = vec![0u8; size as usize];
            let rc = sys::rc_runtime_serialize_progress_sized(
                buf.as_mut_ptr(),
                size,
                self.rt,
                std::ptr::null_mut(),
            );
            if rc != 0 {
                buf.clear();
            }
            buf
        }
    }

    /// Restore a previous session. Tolerates garbage (wrong game, corrupt):
    /// rcheevos validates an internal checksum and returns non-zero.
    pub fn load_progress(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        unsafe {
            sys::rc_runtime_deserialize_progress_sized(
                self.rt,
                data.as_ptr(),
                data.len() as u32,
                std::ptr::null_mut(),
            );
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if !self.rt.is_null() {
            unsafe { sys::rc_runtime_destroy(self.rt) };
        }
    }
}
