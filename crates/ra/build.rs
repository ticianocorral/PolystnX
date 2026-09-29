//! Compiles the vendored rcheevos (develop snapshot) — evaluation runtime
//! plus disc hashing (`rhash`): no LUA (`RC_DISABLE_LUA`), no zip hashing
//! (`RC_HASH_NO_ZIP`) and no encrypted-image hashing (`RC_HASH_NO_ENCRYPTED`).
//! CHD discs are read through the frontend's custom cdreader (`src/hash.rs`),
//! which rcheevos has no built-in reader for — see `docs/fase-0.md`.
fn main() {
    let mut b = cc::Build::new();
    b.include("vendor/rcheevos/include")
        .include("vendor/rcheevos/src")
        .include("vendor/rcheevos/src/rcheevos")
        .include("vendor/rcheevos/src/rhash")
        .define("RC_DISABLE_LUA", None)
        .define("RC_HASH_NO_ZIP", None)
        .define("RC_HASH_NO_ENCRYPTED", None)
        .flag_if_supported("-std=c99");
    let files = vec![
        "rcheevos/alloc.c",
        "rcheevos/condition.c",
        "rcheevos/condset.c",
        "rcheevos/consoleinfo.c",
        "rcheevos/format.c",
        "rcheevos/lboard.c",
        "rcheevos/memref.c",
        "rcheevos/operand.c",
        "rcheevos/rc_validate.c",
        "rcheevos/richpresence.c",
        "rcheevos/runtime.c",
        "rcheevos/runtime_progress.c",
        "rcheevos/trigger.c",
        "rcheevos/value.c",
        "rc_compat.c",
        "rc_util.c",
        "rhash/md5.c",
        "rhash/hash.c",
        "rhash/hash_disc.c",
        "rhash/hash_rom.c",
        "rhash/cdreader.c",
    ];
    for f in &files {
        let p = format!("vendor/rcheevos/src/{f}");
        println!("cargo:rerun-if-changed={p}");
        b.file(p);
    }
    println!("cargo:rerun-if-changed=vendor/rcheevos/include/rc_hash.h");
    // The layout-truth probe: offsetof(rc_trigger_t, state), compiled
    // against the very same vendored headers.
    println!("cargo:rerun-if-changed=src/state_offset.c");
    b.file("src/state_offset.c");
    b.compile("rcheevos");
}
