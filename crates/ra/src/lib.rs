//! Safe wrapper over the vendored rcheevos evaluation runtime — the phase 3
//! heart of the RetroAchievements plan (`docs/plano-retroachievements.md`,
//! adaptado pelo `docs/plano-psx-xperience.md`).
//! Only what the app needs: activate this game's achievements (definition
//! strings straight from the RA API), tick them per frame against the
//! SwanStation work RAM, collect triggered ids, and save/load session progress.
//! Plus `hash`: PSX disc hashing in the RA standard (`rc_hash_psx` + custom
//! CHD cdreader).
pub mod hash;
pub mod runtime;
pub mod sys;
