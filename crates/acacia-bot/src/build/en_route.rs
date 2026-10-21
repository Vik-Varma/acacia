//! The builder's side of placing while walking (`pathfind::process::run::en_route` drives it): which cells may
//! be placed on the way, and the knob that turns it on.

/// Reach margin for a click made a tick after it was planned, from an eye up to ~0.22 blocks on (4.3 b/s).
pub(super) const SLACK: f32 = 0.25;

// TODO: default on once A/B'd live (tools/build-ab.sh, `BUILD_ENROUTE=1`).
pub(super) fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("BUILD_ENROUTE").is_ok_and(|v| v == "1"))
}

/// The next step of the drive is one a click can ride on: on the ground, a plain walk (no macro, no work). Else
/// the click is made by stopping, as without en-route placing (live: offers on runs and leaps were dropped and
/// the cells walked past, 116 placed en route and no time saved).
pub(super) fn plain_step(cx: &crate::pathfind::process::Ctx) -> bool {
    cx.on_ground && cx.path.first().is_some_and(|n| n.kind == crate::pathfind::MoveKind::Walk && n.work.is_empty() && n.mac.is_none())
}

/// The placed state does not follow the look (BDS takes facing blocks' direction from the last rotation it
/// got, which mid-walk is the aim, not the stand's): these wait for a stand.
pub(super) fn look_free(name: &str) -> bool {
    let n = name.trim_start_matches("minecraft:");
    !["stairs", "furnace", "observer", "piston", "dispenser", "dropper", "chest", "barrel", "door", "hopper"].iter().any(|k| n.contains(k))
}
