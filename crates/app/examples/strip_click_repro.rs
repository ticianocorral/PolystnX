//! Repro harness for the shelf strips' arrow buttons: builds a fake catalog
//! with several favorites, pushes real SDL mouse events into the queue
//! (clicking the ">" arrow of the favorites strip), then runs the shelf and
//! screenshots before/after so the scroll can be eyeballed.
//!
//! Usage: cargo run --example strip_click_repro -p xperience-app

use std::time::Duration;

use xperience_app::shelf::{self, Pick, ShelfOpts};
use xperience_domain::Catalog;
use xperience_platform::{Cabinet, Platform};

fn make_catalog(n_favs: usize) -> anyhow::Result<(tempdir::TempDir, Catalog)> {
    let dir = tempdir::TempDir::new()?;
    let roms = dir.path().join("roms");
    std::fs::create_dir_all(&roms)?;
    for i in 0..8 {
        let mut bytes = vec![0u8; 0x8000];
        bytes[0x7FC0..0x7FC0 + 6].copy_from_slice(format!("GAME{i:02}").as_bytes());
        std::fs::write(roms.join(format!("game{i:02}.sfc")), &bytes)?;
    }
    std::thread::sleep(Duration::from_millis(20));
    let store = dir.path().join("library.json");
    let cat = Catalog::open(&roms, &store)?;
    let entries = cat.list(xperience_domain::Order::Name)?;
    for e in entries.iter().take(n_favs) {
        cat.set_favorite(&e.rom.sha1, true)?;
    }
    Ok((dir, cat))
}

fn main() -> anyhow::Result<()> {
    let (_dir, catalog) = make_catalog(6)?;
    let mut plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("repro", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let mut opts = ShelfOpts {
        order: xperience_domain::Order::Name,
        max_frames: Some(3),
        shot: Some("/tmp/repro_before.bmp".into()),
        fade_in: None,
        preset_filter: None,
        ra: None,
    };
    let pick = shelf::run(&mut plat, &mut cab, &catalog, &opts)?;
    println!("first run pick ok (quit by frame cap)");
    // Where is the favorites ">" button, in screen-local coords? Mirror the
    // shelf's own math.
    let (scr_w, _scr_h) = cab.shelf_screen_size();
    let y0 = 28 + 28 + 24; // MARGIN + HEADER_H + FAV_LABEL_H
    let (left, right) = (xperience_app::shelf::strip_arrow_rects)(scr_w, y0);
    println!("left rect {left:?}, right rect {right:?}");
    let strip_vis = (scr_w as i32 - 28 * 2) as u32 / (160 + 14);
    println!("scr_w {scr_w}, strip_vis {strip_vis} (6 favorites)");

    let to_screen = |wx: i32, wy: i32| -> (i32, i32) {
        let (ox, oy) = cab.window_to_output(wx, wy);
        cab.hit_screen_point(ox, oy).unwrap_or((-1, -1))
    };
    // Aim by scanning window x/y for the point whose screen mapping best
    // matches the arrow's centre (the map is nonlinear after the
    // warp-aware hit_screen_point, so no closed-form inverse).
    let target = if std::env::args().any(|a| a == "tile") {
        (28 + 80, y0 + 60)
    } else if std::env::args().any(|a| a == "left") {
        (left.0 + left.2 as i32 / 2, left.1 + left.3 as i32 / 2)
    } else {
        (right.0 + right.2 as i32 / 2, right.1 + right.3 as i32 / 2)
    };
    let mut best = (0, 0);
    let mut best_d = i32::MAX;
    for wx in (0..1280).step_by(2) {
        for wy in (0..800).step_by(2) {
            let sp = to_screen(wx, wy);
            let d = (sp.0 - target.0).abs() + (sp.1 - target.1).abs();
            if d < best_d {
                best_d = d;
                best = (wx, wy);
            }
        }
    }
    let (win_x, win_y) = best;
    println!("clicking window coords ({win_x}, {win_y}) -> screen {target:?}");

    // Push the click (down+up) BEFORE running; the queue delivers it to the
    // shelf's poll loop. Give it one no-click frame first so the shelf has
    // drawn at least once, and one click AFTER a few frames too.
    let click = |plat: &Platform, x: i32, y: i32| plat.push_synthetic_click(x, y);

    click(&plat, win_x, win_y);

    opts.max_frames = Some(6);
    opts.shot = Some("/tmp/repro_after.bmp".into());
    let pick = shelf::run(&mut plat, &mut cab, &catalog, &opts)?;
    println!("second run pick ok");
    println!("shots in /tmp/repro_before.bmp and /tmp/repro_after.bmp");
    Ok(())
}

mod tempdir {
    use std::path::{Path, PathBuf};
    pub struct TempDir(PathBuf);
    impl TempDir {
        pub fn new() -> std::io::Result<Self> {
            let p = std::env::temp_dir().join(format!("strip-repro-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p)?;
            Ok(Self(p))
        }
        pub fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
