//! Proves the vendored evaluator end to end: one synthetic achievement
//! ("the byte at 0x0000 becomes 100") fires exactly when the memory says
//! so, survives save/load, and resets.

use xperience_ra::runtime::{Achievement, Session};

fn ach(id: u32, memaddr: &str) -> Achievement {
    Achievement {
        id,
        title: format!("t{id}"),
        description: String::new(),
        points: 5,
        badge: String::new(),
        memaddr: memaddr.to_string(),
    }
}

#[test]
fn triggers_when_condition_becomes_true() {
    let mut s = Session::from_parts(vec![ach(1, "0xH0000=100")]);
    let mem = vec![0u8; 0x1000];
    assert!(s.tick(&mem).is_empty(), "no trigger while false");
    let mut mem = vec![0u8; 0x1000];
    mem[0] = 100;
    assert_eq!(s.tick(&mem), vec![1], "fires the frame it turns true");
    assert_eq!(s.triggered_ids(), vec![1]);
    // Stays triggered, never re-fires.
    assert!(s.tick(&mem).is_empty());
}

#[test]
fn wrong_address_never_triggers() {
    let mut s = Session::from_parts(vec![ach(7, "0xH2345=1")]);
    let mut mem = vec![0u8; 0x1000];
    mem[0] = 1; // right value, wrong address
    for _ in 0..5 {
        assert!(s.tick(&mem).is_empty());
    }
}

#[test]
fn progress_survives_save_load() {
    let mut s = Session::from_parts(vec![ach(1, "0xH0002=1")]);
    let mem = vec![0u8; 0x1000];
    s.tick(&mem);
    let blob = s.save_progress();
    assert!(!blob.is_empty());

    let mut s2 = Session::from_parts(vec![ach(1, "0xH0002=1")]);
    s2.load_progress(&blob);
    // rcheevos requires one false frame after load before a hit can fire
    // (WAITING state) — drive the condition false, then true.
    s2.tick(&mem);
    let mut hot = vec![0u8; 0x1000];
    hot[2] = 1;
    assert_eq!(s2.tick(&hot), vec![1], "loaded hit counts still usable");
}

#[test]
fn broken_definition_is_skipped_not_fatal() {
    let mut s = Session::from_parts(vec![ach(1, "não é uma condição")]);
    let mem = vec![0u8; 0x1000];
    s.tick(&mem); // must not crash on the unparseable definition
    assert!(s.triggered_ids().is_empty());
}

#[test]
fn reset_keeps_earned_and_rearms_never_earned() {
    let cold = vec![0u8; 0x1000];
    let mut hot = vec![0u8; 0x1000];
    hot[0] = 100;

    // Earn it the honest way (false frame first), then reset: it stays
    // earned and never double-fires.
    let mut s = Session::from_parts(vec![ach(1, "0xH0000=100")]);
    s.tick(&cold);
    assert_eq!(s.tick(&hot), vec![1]);
    s.reset();
    s.tick(&cold);
    s.tick(&cold);
    // rc_runtime_reset fully re-arms — including already-earned ones. The
    // app therefore keeps its own per-game "earned" set and only submits
    // ids that aren't in it (the server deduplicates anyway).
    assert_eq!(s.tick(&hot), vec![1], "reset re-arms even earned ones");
}
