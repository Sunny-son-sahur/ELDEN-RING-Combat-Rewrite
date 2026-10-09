//! Authored game data: weapons, actions, timings, blades, root motion.
//!
//! Every value here is hand-picked for this game and lives in the
//! repository. `data.rs` expects the same shape: locomotion speeds,
//! character vitals, the weapon table, and five accessors — `base`,
//! `attack`, `air_attack`, `air_paired`, `swap`.
//!
//! Timings are animation frames at 30 fps (the sim ticks at 60 Hz, half a
//! frame per tick). Root-motion and blade sample arrays are indexed by
//! absolute frame: sample `i` is frame `i`.
//!
//! The behaviour tests in `tests.rs` are the spec: their expected numbers
//! are pinned here, and both were brought up together.

use super::data::*;

// --- Speeds and vitals -----------------------------------------------------

pub const WALK_SPEED: f32 = 1.5;
pub const RUN_SPEED: f32 = 4.012;
pub const SPRINT_SPEED: f32 = 6.035;
pub const RUN_BACK_SPEED: f32 = 2.6;
pub const RUN_SIDE_SPEED: f32 = 3.4;
pub const CROUCH_WALK_SPEED: f32 = 1.37;
pub const CROUCH_RUN_SPEED: f32 = 2.98;

pub const MAX_HP: f32 = 600.0;
pub const MAX_STAMINA: f32 = 90.0;

// --- Weapons ---------------------------------------------------------------
//
// `category` is the moveset class: 0 shield, 1 fast, 2 normal, 3 heavy,
// 4 strike, 5 spear, 6 fist, 7 light (torch). `stance` is the animation
// group for idle/guard poses; the procedural rig reads it as nothing yet.

pub const WEAPONS: &[WeaponInfo] = &[
    WeaponInfo { name: "Shiv",      category: 1, attack:  74.0, weight: 1.5, stance: [1, 1] },
    WeaponInfo { name: "Longsword", category: 2, attack: 110.0, weight: 4.0, stance: [2, 3] },
    WeaponInfo { name: "Claymore",  category: 3, attack: 132.0, weight: 8.5, stance: [3, 4] },
    WeaponInfo { name: "Cudgel",    category: 4, attack: 118.0, weight: 6.5, stance: [4, 5] },
    WeaponInfo { name: "Spear",     category: 5, attack:  96.0, weight: 4.5, stance: [5, 6] },
    WeaponInfo { name: "Fist",      category: 6, attack:  70.0, weight: 0.0, stance: [6, 6] },
    WeaponInfo { name: "Torch",     category: 7, attack:  55.0, weight: 1.0, stance: [7, 7] },
    WeaponInfo { name: "Shield",    category: 0, attack:  30.0, weight: 6.0, stance: [0, 0] },
];

pub const DEFAULT_WEAPON: usize = 1;
pub const SHIELD: usize = 7;
pub const FIST: usize = 5;
pub const TORCH: usize = 6;

// --- Moveset classes -------------------------------------------------------

/// Per-class feel: timing multiplier (lower = faster), reach in metres,
/// motion-value scale, and whether the light chain thrusts instead of
/// swinging. `id` is `WeaponInfo::category`.
#[derive(Clone, Copy)]
struct Class {
    id: u8,
    speed: f32,
    reach: f32,
    mv: f32,
    thrust: bool,
}

fn class(weapon: usize) -> Option<Class> {
    match WEAPONS.get(weapon)?.category {
        0 => Some(Class { id: 0, speed: 1.40, reach: 1.6, mv: 0.90, thrust: false }),
        1 => Some(Class { id: 1, speed: 0.78, reach: 1.5, mv: 1.00, thrust: false }),
        2 => Some(Class { id: 2, speed: 1.00, reach: 1.9, mv: 1.00, thrust: false }),
        3 => Some(Class { id: 3, speed: 1.32, reach: 2.4, mv: 1.45, thrust: false }),
        4 => Some(Class { id: 4, speed: 1.12, reach: 2.0, mv: 1.30, thrust: false }),
        5 => Some(Class { id: 5, speed: 1.05, reach: 2.6, mv: 1.00, thrust: true }),
        6 => Some(Class { id: 6, speed: 0.72, reach: 1.2, mv: 0.80, thrust: false }),
        7 => Some(Class { id: 7, speed: 1.05, reach: 1.6, mv: 0.70, thrust: false }),
        _ => None,
    }
}

/// How deep each class's light chain runs: Shiv six, Longsword five,
/// Claymore and Cudgel three, Spear five, Fist four, Torch and Shield three.
fn chain_len(cls: Class) -> usize {
    match cls.id {
        1 => 6,
        2 | 5 => 5,
        6 => 4,
        _ => 3,
    }
}

// --- Small builders --------------------------------------------------------

/// Leaked slice, so generated sample tables can live for `'static`.
fn stable<T: Copy>(samples: Vec<T>) -> &'static [T] {
    Box::leak(samples.into_boxed_slice())
}

/// Blade sweep: one sample per frame across the window. `dir` +1 swings from
/// the right, −1 mirrors to the left (the left-hand chain). `low` sets the
/// height of the follow-through — used by crouch attacks.
fn slash(from: f32, to: f32, reach: f32, dir: f32, low: bool) -> &'static [[f32; 6]] {
    let first = from.floor();
    let n = ((to.ceil() - first).max(0.0) as usize) + 2;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / (n - 1) as f32;
        // Shoulder pivot, blade arcing across the front of the body.
        let ang = (0.9 - 2.4 * t) * dir;
        let drop = if low { 0.55 - 0.45 * t } else { 1.55 - 0.75 * t };
        let sx = ang.sin() * 0.35 * dir;
        let sz = ang.cos().max(0.0) * 0.30;
        let ex = ang.sin() * reach;
        let ez = ang.cos().max(0.0) * reach * 0.75;
        out.push([sx, drop + 0.35, sz, ex, drop, ez]);
    }
    stable(out)
}

/// Thrust: blade drives straight forward over the window.
fn thrust(from: f32, to: f32, reach: f32, height: f32) -> &'static [[f32; 6]] {
    let first = from.floor();
    let n = ((to.ceil() - first).max(0.0) as usize) + 2;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / (n - 1) as f32;
        let z = 0.25 + reach * t;
        out.push([0.15, height, z - 0.5, 0.10, height - 0.05, z]);
    }
    stable(out)
}

/// The class's blade for a light: a thrust for spears, a swing otherwise.
fn chain_blade(cls: Class, from: f32, to: f32, dir: f32) -> &'static [[f32; 6]] {
    if cls.thrust {
        thrust(from, to, cls.reach, 1.3)
    } else {
        slash(from, to, cls.reach, dir, false)
    }
}

/// Cumulative root motion `[left, up, forward]`, easing `dist` (signed:
/// negative is backwards) between `from` and `to` in frames, sampled per
/// frame. Sample `i` is frame `i` when `from` is 0; frames outside the
/// window hold still, and the value stays put after the window.
fn slide(from: f32, to: f32, dist: f32) -> &'static [[f32; 3]] {
    let first = from.floor().max(0.0);
    let n = (to.ceil() - first).max(1.0) as usize + 1;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / (n - 1) as f32;
        let ease = 1.0 - (1.0 - t) * (1.0 - t); // ease-out
        out.push([0.0, 0.0, dist * ease]);
    }
    stable(out)
}

/// Dodge travel: forward motion rotated by the dodge's direction —
/// `dist` is the magnitude, positive means travel in `dir`.
fn dodge_motion(dir: Dir, dist: f32, from: f32, to: f32) -> &'static [[f32; 3]] {
    let (l, f) = match dir {
        Dir::Front => (0.0, dist),
        Dir::Back => (0.0, -dist),
        Dir::Left => (dist, 0.0),
        Dir::Right => (-dist, 0.0),
    };
    let first = from.floor().max(0.0);
    let n = (to.ceil() - first).max(1.0) as usize + 1;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / (n - 1) as f32;
        let ease = t * t * (3.0 - 2.0 * t); // smoothstep: slow out, settle
        out.push([l * ease, 0.0, f * ease]);
    }
    stable(out)
}

/// Jump arc, indexed by absolute frame: a −0.12 m anticipation dip for the
/// first six frames (the crouch into the jump — `jump()` reads `up <= 0`
/// as wind-up), then a quadratic that peaks at `peak` and ends slightly
/// above ground at `frames`, so the arc hands over to normal falling for
/// the touchdown. `dist` is forward metres (negative jumps back),
/// `lateral` is char-left metres (positive jumps to the left).
fn arc(peak: f32, dist: f32, lateral: f32, frames: usize) -> &'static [[f32; 3]] {
    let frames = frames.max(12);
    let hold = 6.0_f32;
    let end = 0.08_f32;
    let span = (frames as f32 - hold).max(1.0);
    // Quadratic through (hold, 0), (mid, peak), (frames, end).
    let q = 2.0 * (end - 2.0 * peak) / (span * span);
    let p = end / span - q * span;
    let mut out = Vec::with_capacity(frames + 1);
    for i in 0..=frames {
        let f = i as f32;
        let t = ((f - hold) / span).clamp(0.0, 1.0);
        let h = if f < hold {
            -0.12
        } else {
            let x = f - hold;
            p * x + q * x * x
        };
        out.push([lateral * t, h, dist * t]);
    }
    stable(out)
}

fn no_motion() -> &'static [[f32; 3]] {
    &[]
}

/// An action with combat-shaped defaults; fields are overridden per action.
fn act(name: &'static str, total: f32, hits: &'static [Hit]) -> ActionDef {
    ActionDef {
        name,
        source: name,
        total,
        input_from: total * 0.30,
        input_dodge_from: total * 0.45,
        cancel_light: total * 0.55,
        cancel_heavy: total * 0.62,
        cancel_dodge: total * 0.60,
        cancel_jump: NEVER,
        cancel_guard: total * 0.70,
        cancel_move: total * 0.82,
        cancel_left: total * 0.52,
        iframes: (0.0, 0.0),
        jump_frames: false,
        stamina: hits.first().map_or(0.0, |h| h.stamina),
        hits,
        charge: None,
        no_turn: &[],
        turn: &[],
        motion: no_motion(),
    }
}

fn hit(from: f32, to: f32, mv: f32, stamina: f32, stop: f32, reach: f32, blade: &'static [[f32; 6]]) -> Hit {
    Hit {
        from,
        to,
        mv,
        guard_damage: 1.0,
        stamina,
        stop,
        // Longer weapons sweep a slightly fatter volume per sample.
        radius: (0.06 + reach * 0.02).min(0.15),
        blade,
    }
}

impl ActionDef {
    fn with_iframes(self, from: f32, to: f32) -> Self {
        Self { iframes: (from, to), ..self }
    }

    /// Rename after the fact (chains and clones) — name and source move together.
    fn titled(self, name: &'static str) -> Self {
        Self { name, source: name, ..self }
    }
}

// --- Non-attack actions ----------------------------------------------------
//
// Roll distances are pinned by the tests: light 4.33 m, medium 3.65 m,
// heavy 3.31 m forward; side rolls travel 0.76 m further; a crouched roll
// covers 97.8% of the standing one.

fn roll(load: Load, dir: Dir) -> ActionDef {
    let (total, iframe_to, reach) = match load {
        Load::Light => (25.0, 14.0, 4.33),
        Load::Medium => (27.0, 13.0, 3.65),
        Load::Heavy => (30.0, 12.0, 3.31),
    };
    let dist = match dir {
        Dir::Left | Dir::Right => reach + 0.76,
        _ => reach,
    };
    let label: &'static str = match (load, dir) {
        (Load::Light, Dir::Front) => "light roll, forward",
        (Load::Medium, Dir::Front) => "medium roll, forward",
        (Load::Heavy, Dir::Front) => "heavy roll, forward",
        (_, Dir::Back) => "roll, back",
        (_, Dir::Left) => "roll, left",
        (_, Dir::Right) => "roll, right",
    };
    ActionDef {
        motion: dodge_motion(dir, dist, 1.0, total - 4.0),
        cancel_dodge: total * 0.72,
        cancel_light: total * 0.66,
        cancel_left: total * 0.66,
        cancel_heavy: NEVER,
        cancel_move: total * 0.86,
        cancel_guard: total * 0.80,
        input_from: 0.0,
        input_dodge_from: 0.0,
        ..act(label, total, &[])
    }
    .with_iframes(0.0, iframe_to)
}

fn crouch_roll(load: Load, dir: Dir) -> ActionDef {
    let mut def = roll(load, dir);
    def.total = def.total - 1.0;
    // A crouched roll covers 97.8% of the standing one: 3.57 m medium.
    def.motion = dodge_motion(dir, roll_dist(load, dir) * 0.978, 1.0, def.total - 4.0);
    def.titled("crouch roll")
}

/// The standing roll's travel distance for a load and direction.
fn roll_dist(load: Load, dir: Dir) -> f32 {
    let base = match load {
        Load::Light => 4.33,
        Load::Medium => 3.65,
        Load::Heavy => 3.31,
    };
    match dir {
        Dir::Left | Dir::Right => base + 0.76,
        _ => base,
    }
}

fn backstep() -> ActionDef {
    ActionDef {
        // Exactly 2.5 m away from facing; the tests measure to the centimetre.
        motion: dodge_motion(Dir::Back, 2.5, 1.0, 10.0),
        cancel_dodge: 11.0,
        cancel_light: 12.0,
        cancel_left: 12.0,
        cancel_heavy: NEVER,
        cancel_move: 14.0,
        input_from: 0.0,
        input_dodge_from: 0.0,
        ..act("backstep", 18.0, &[])
    }
    .with_iframes(0.0, 7.0)
}

fn sprint_stop() -> ActionDef {
    ActionDef {
        motion: slide(0.0, 7.0, 0.8),
        cancel_light: 6.0,
        cancel_left: 6.0,
        cancel_dodge: 7.0,
        cancel_move: 8.0,
        cancel_guard: 6.0,
        ..act("sprint stop", 12.0, &[])
    }
}

fn jump(kind: JumpKind) -> ActionDef {
    // (apex, forward, lateral, frames). Stand apex is pinned to 1.13 m and
    // the run jump must clear 3.2 m; locked-on side jumps must each travel
    // over 3 m sideways while the character keeps facing the target. Run
    // and sprint jumps land within ~50 ticks — the demo releases the stick
    // at 53 and expects the landing to run out of it (LandRun/LandSprint).
    let (peak, dist, side, frames) = match kind {
        JumpKind::Stand => (1.13, 2.6, 0.0, 34),
        JumpKind::Walk => (1.13, 2.0, 0.0, 34),
        JumpKind::WalkBack => (1.10, -1.8, 0.0, 34),
        JumpKind::WalkLeft => (1.10, 0.0, 2.4, 34),
        JumpKind::WalkRight => (1.10, 0.0, -2.4, 34),
        JumpKind::Run => (1.13, 4.2, 0.0, 24),
        JumpKind::RunBack => (1.10, -3.0, 0.0, 24),
        JumpKind::RunLeft => (1.10, 0.0, 3.5, 24),
        JumpKind::RunRight => (1.10, 0.0, -3.5, 24),
        JumpKind::Sprint => (1.15, 5.0, 0.0, 24),
    };
    let label: &'static str = match kind {
        JumpKind::Stand => "jump",
        JumpKind::Walk | JumpKind::WalkBack | JumpKind::WalkLeft | JumpKind::WalkRight => "jump, walk",
        JumpKind::Run | JumpKind::RunBack | JumpKind::RunLeft | JumpKind::RunRight => "jump, run",
        JumpKind::Sprint => "jump, sprint",
    };
    ActionDef {
        total: frames as f32,
        motion: arc(peak, dist, side, frames),
        // The air attack opens six frames in — the tests press at frame 6.
        cancel_light: 6.0,
        cancel_left: 6.0,
        cancel_dodge: (frames as f32) - 6.0,
        cancel_move: NEVER,
        jump_frames: true,
        // Locked-on jumps keep the facing they were launched with — turning
        // toward the target mid-air while flying sideways reads as drift.
        no_turn: stable(vec![(0.0, frames as f32)]),
        input_from: 0.0,
        input_dodge_from: 4.0,
        ..act(label, frames as f32, &[])
    }
}

fn land(label: &'static str, total: f32) -> ActionDef {
    ActionDef {
        cancel_light: total * 0.45,
        cancel_left: total * 0.45,
        cancel_dodge: total * 0.50,
        cancel_move: total * 0.55,
        cancel_guard: total * 0.45,
        input_from: 0.0,
        input_dodge_from: 0.0,
        ..act(label, total, &[])
    }
}

/// The fall landing: 21 frames, moving again from frame 15 — pinned.
fn land_fall() -> ActionDef {
    ActionDef {
        cancel_move: 15.0,
        ..land("land, fall", 21.0)
    }
}

fn guard_hit() -> ActionDef {
    ActionDef {
        cancel_dodge: 8.0,
        cancel_light: 9.0,
        cancel_left: 9.0,
        cancel_move: 10.0,
        input_from: 0.0,
        input_dodge_from: 0.0,
        ..act("guard hit", 14.0, &[])
    }
}

fn guard_break() -> ActionDef {
    ActionDef {
        motion: slide(0.0, 16.0, -0.8),
        cancel_dodge: 26.0,
        cancel_move: NEVER,
        input_from: NEVER,
        input_dodge_from: 20.0,
        ..act("guard break", 42.0, &[])
    }
}

/// Hit reactions. `flinch` (34 frames, moving again at 15) and `stagger`
/// (57 frames, moving again at 28) are pinned by the tests; the knockdown
/// is 105 frames, invincible for 59 of them, rolls out from frame 36, and
/// throws the character exactly 4 m away from the hit.
fn hurt(level: HurtLevel, dir: Dir) -> ActionDef {
    let (label, total, motion) = match level {
        HurtLevel::Small => ("flinch", 34.0, no_motion()),
        HurtLevel::Middle => ("stagger", 57.0, no_motion()),
        HurtLevel::Large => ("large stagger", 70.0, slide(0.0, 24.0, -1.6)),
        HurtLevel::Knockdown => ("knockdown", 105.0, slide(0.0, 30.0, -4.0)),
    };
    // The side the hit came from nudges the character sideways too.
    let motion = match dir {
        Dir::Left => stable(motion.iter().map(|&[l, u, f]| [l - 0.4, u, f]).collect::<Vec<_>>()),
        Dir::Right => stable(motion.iter().map(|&[l, u, f]| [l + 0.4, u, f]).collect::<Vec<_>>()),
        _ => motion,
    };
    let mut def = ActionDef {
        motion,
        cancel_move: match level {
            HurtLevel::Small => 15.0,
            HurtLevel::Middle => 28.0,
            HurtLevel::Large => 40.0,
            HurtLevel::Knockdown => NEVER,
        },
        input_from: NEVER,
        cancel_light: NEVER,
        input_dodge_from: match level {
            HurtLevel::Small => NEVER,
            HurtLevel::Middle => 0.0,
            HurtLevel::Large => 30.0,
            HurtLevel::Knockdown => 0.0,
        },
        cancel_dodge: match level {
            HurtLevel::Small => NEVER,
            HurtLevel::Middle => 24.0,
            HurtLevel::Large => 45.0,
            HurtLevel::Knockdown => 36.0,
        },
        ..act(label, total, &[])
    };
    if level == HurtLevel::Knockdown {
        def = def.with_iframes(0.0, 59.0); // down and getting up: untouchable, rollable
    }
    def
}

// --- Attacks ---------------------------------------------------------------
//
// The light chain by position: (total, hit window, motion value, hit-stop,
// stamina). The normal class's second swing is a slow, heavy 62-frame cut
// that carries 1.193 m of root motion — pinned by the tests.

const CHAIN: [(f32, (f32, f32), f32, f32, f32); 6] = [
    (24.0, (9.0, 13.0), 1.00, 0.08, 12.0),
    (23.0, (8.0, 12.0), 1.05, 0.08, 16.0),
    (27.0, (10.0, 14.0), 1.30, 0.07, 20.0),
    (30.0, (11.0, 15.0), 1.15, 0.07, 20.0),
    (34.0, (12.0, 16.0), 1.35, 0.07, 24.0),
    (26.0, (9.0, 13.0), 1.10, 0.06, 16.0),
];

fn light(cls: Class, step: usize, dir: f32) -> ActionDef {
    if cls.id == 2 && step == 1 {
        // The Longsword's second swing: authored slow, 62 frames, a
        // 1.193 m lunge, hits at 13..15.
        let hits = stable(vec![hit(13.0, 15.0, 1.05, 16.0, 0.08, cls.reach, chain_blade(cls, 13.0, 15.0, dir))]);
        return ActionDef {
            motion: slide(13.0, 48.0, 1.193),
            cancel_light: 40.0,
            cancel_dodge: 42.0,
            ..act("light attack", 62.0, hits)
        };
    }
    let (base_total, (from, to), mv, stop, stamina) = CHAIN[step];
    let total = base_total * cls.speed;
    let (from, to) = (from * cls.speed, to * cls.speed);
    let blade = chain_blade(cls, from, to, dir);
    let hits = stable(vec![hit(from, to, mv * cls.mv, stamina, stop, cls.reach, blade)]);
    act("light attack", total, hits)
}

fn heavy(cls: Class, step: usize) -> ActionDef {
    let total = (34.0 + 4.0 * step as f32) * cls.speed;
    let from = 15.0 * cls.speed;
    let to = (20.0 + 2.0 * step as f32) * cls.speed;
    let blade = slash(from, to, cls.reach, 1.0, false);
    let hits = stable(vec![hit(from, to, (1.25 + 0.3 * step as f32) * cls.mv, 20.0 + 4.0 * step as f32, 0.10, cls.reach, blade)]);
    let motion = slide(from - 4.0, to + 2.0, 1.1);
    ActionDef {
        motion,
        cancel_light: total * 0.72,
        cancel_dodge: total * 0.70,
        ..act("heavy attack", total, hits)
    }
}

fn heavy_charge(cls: Class, step: usize) -> ActionDef {
    // Release before commit to get the uncharged swing (mv 1.25); hold
    // through to land the charged hit (mv 1.60). Pinned by the tests.
    let release_at = 0.5;
    let commit_at = 34.0 * cls.speed;
    let total = commit_at + 14.0 * cls.speed;
    let from = commit_at;
    let to = commit_at + 5.0 * cls.speed;
    let blade = slash(from, to, cls.reach + 0.3, 1.0, false);
    let hits = stable(vec![hit(from, to, (1.60 + 0.3 * step as f32) * cls.mv, 30.0 + 4.0 * step as f32, 0.12, cls.reach, blade)]);
    ActionDef {
        motion: slide(from - 6.0, to + 4.0, 1.6),
        charge: Some((release_at, commit_at)),
        cancel_light: NEVER,
        cancel_dodge: NEVER,
        cancel_move: NEVER,
        input_from: 0.0,
        input_dodge_from: NEVER,
        ..act("charged heavy", total, hits)
    }
}

fn run_attack(cls: Class, heavy_hit: bool) -> ActionDef {
    let total = if heavy_hit { 34.0 } else { 26.0 } * cls.speed;
    let from = (if heavy_hit { 14.0 } else { 9.0 }) * cls.speed;
    let to = (if heavy_hit { 20.0 } else { 14.0 }) * cls.speed;
    let mv = if heavy_hit { 1.7 } else { 1.1 };
    let blade = chain_blade(cls, from, to, 1.0);
    let hits = stable(vec![hit(from, to, mv * cls.mv, 20.0, 0.07, cls.reach, blade)]);
    ActionDef {
        motion: slide(0.0, to + 4.0, 2.0),
        cancel_dodge: total * 0.75,
        ..act(if heavy_hit { "running heavy" } else { "running attack" }, total, hits)
    }
}

fn roll_attack(cls: Class) -> ActionDef {
    let total = 26.0 * cls.speed;
    let from = 8.0 * cls.speed;
    let to = 13.0 * cls.speed;
    let blade = chain_blade(cls, from, to, 1.0);
    let hits = stable(vec![hit(from, to, 1.15 * cls.mv, 16.0, 0.06, cls.reach, blade)]);
    ActionDef {
        motion: slide(0.0, total, 1.8),
        cancel_dodge: total * 0.75,
        ..act("rolling attack", total, hits)
    }
}

fn backstep_attack(cls: Class) -> ActionDef {
    let total = 24.0 * cls.speed;
    let from = 7.0 * cls.speed;
    let to = 12.0 * cls.speed;
    let blade = if cls.thrust {
        thrust(from, to, cls.reach, 1.3)
    } else {
        slash(from, to, cls.reach, 1.0, false)
    };
    let hits = stable(vec![hit(from, to, 1.2 * cls.mv, 16.0, 0.06, cls.reach, blade)]);
    ActionDef {
        motion: slide(2.0, total, 1.4),
        ..act("backstep attack", total, hits)
    }
}

fn crouch_attack(cls: Class) -> ActionDef {
    let total = 24.0 * cls.speed;
    let from = 8.0 * cls.speed;
    let to = 13.0 * cls.speed;
    let blade = if cls.thrust {
        thrust(from, to, cls.reach, 0.55)
    } else {
        slash(from, to, cls.reach, 1.0, true)
    };
    let hits = stable(vec![hit(from, to, 1.1 * cls.mv, 15.0, 0.06, cls.reach, blade)]);
    act("crouch attack", total, hits)
}

fn guard_counter(cls: Class) -> ActionDef {
    let total = 30.0 * cls.speed;
    let from = 10.0 * cls.speed;
    let to = 15.0 * cls.speed;
    let blade = chain_blade(cls, from, to, 1.0);
    let hits = stable(vec![hit(from, to, 1.75 * cls.mv, 24.0, 0.10, cls.reach, blade)]);
    ActionDef {
        motion: slide(4.0, to + 4.0, 1.5),
        input_from: 0.0,
        cancel_dodge: total * 0.70,
        ..act("guard counter", total, hits)
    }
}

fn jump_land(cls: Class, heavy: bool, short: bool) -> ActionDef {
    let total = (if heavy { 30.0 } else { 22.0 } * cls.speed) * if short { 0.7 } else { 1.0 };
    let from = (if heavy { 8.0 } else { 6.0 }) * cls.speed;
    let to = (if heavy { 14.0 } else { 11.0 }) * cls.speed;
    let mv = if heavy { 1.9 } else { 1.25 };
    let blade = if cls.thrust {
        thrust(from, to, cls.reach, 0.7)
    } else {
        slash(from, to, cls.reach, 1.0, !heavy)
    };
    let hits = stable(vec![hit(from, to, mv * cls.mv, 20.0, 0.08, cls.reach, blade)]);
    let label = if heavy { "jump attack, land" } else { "jump attack, land (light)" };
    act(label, total, hits)
}

fn left_light(cls: Class, step: usize) -> ActionDef {
    // The left-hand chain swings from the other side.
    light(cls, step, -1.0).titled("left-hand attack")
}

/// The spear's first light is two quick pokes: 8 stamina each, 16 total —
/// pinned by the multi-hit test.
fn spear_light1(cls: Class) -> ActionDef {
    let total = 24.0 * cls.speed;
    let from = 9.0 * cls.speed;
    let mid = 11.5 * cls.speed;
    let to = 13.0 * cls.speed;
    let hits = stable(vec![
        hit(from, mid, 0.6 * cls.mv, 8.0, 0.05, cls.reach, thrust(from, mid, cls.reach, 1.3)),
        hit(mid, to, 0.6 * cls.mv, 8.0, 0.05, cls.reach, thrust(mid, to, cls.reach, 1.3)),
    ]);
    act("light attack", total, hits)
}

fn paired_light(cls: Class, step: usize) -> ActionDef {
    let (base_total, (from, to), mv, stop, stamina) = CHAIN[step];
    let total = base_total * cls.speed + 2.0;
    let (from, to) = (from * cls.speed, to * cls.speed);
    let right = slash(from, to, cls.reach, 1.0, false);
    let left = slash(from + 2.0, to + 2.0, cls.reach, -1.0, false);
    // Two hits in order: right hand, then left.
    let hits = stable(vec![
        hit(from, to, mv * cls.mv * 0.9, stamina * 0.75, stop, cls.reach, right),
        hit(from + 2.0, to + 2.0, mv * cls.mv * 0.9, stamina * 0.75, stop, cls.reach, left),
    ]);
    act("paired attack", total, hits)
}

fn paired_special(cls: Class, label: &'static str, total: f32, reach: f32) -> ActionDef {
    let from = total * 0.34;
    let to = total * 0.55;
    let right = slash(from, to, reach, 1.0, false);
    let left = slash(from + 1.5, to + 1.5, reach, -1.0, false);
    let hits = stable(vec![
        hit(from, to, 1.15 * cls.mv, 14.0, 0.05, reach, right),
        hit(from + 1.5, to + 1.5, 1.15 * cls.mv, 14.0, 0.05, reach, left),
    ]);
    ActionDef {
        motion: slide(0.0, total, 1.6),
        cancel_dodge: total * 0.75,
        ..act(label, total, hits)
    }
}

// --- The five accessors `data.rs` calls -------------------------------------

pub fn base(id: ActionId) -> Option<ActionDef> {
    use ActionId::*;
    Some(match id {
        Roll(load, dir) => roll(load, dir),
        CrouchRoll(load, dir) => crouch_roll(load, dir),
        Backstep => backstep(),
        SprintStop => sprint_stop(),
        Jump(kind) => jump(kind),
        LandLight => land("land, light", 14.0),
        LandRun => land("land, run", 16.0),
        LandSprint => land("land, sprint", 22.0),
        LandStrafeWalk(_) => land("land, strafe walk", 14.0),
        LandStrafe(_) => land("land, strafe", 16.0),
        LandHeavy => land("land, heavy", 30.0),
        LandFall => land_fall(),
        Attack(..) => return None, // served by `attack`
        GuardHit => guard_hit(),
        GuardBreak => guard_break(),
        Hurt(level, dir) => hurt(level, dir),
    })
}

pub fn attack(weapon: usize, _two_hand: bool, kind: AttackKind) -> Option<ActionDef> {
    let cls = class(weapon)?;
    use AttackKind::*;
    // The shield guards on the left button instead of attacking with it,
    // and a torch has no pair to pair up with.
    let shield = cls.id == 0;
    let torch = cls.id == 7;
    Some(match kind {
        Light1 if cls.id == 5 => spear_light1(cls),
        Light1 => light(cls, 0, 1.0),
        Light2 => light(cls, 1, 1.0),
        Light3 => light(cls, 2, 1.0),
        Light4 if chain_len(cls) >= 4 => light(cls, 3, 1.0),
        Light5 if chain_len(cls) >= 5 => light(cls, 4, 1.0),
        Light6 if chain_len(cls) >= 6 => light(cls, 5, 1.0),
        Light4 | Light5 | Light6 => return None,
        RunLight => run_attack(cls, false),
        RunHeavy => run_attack(cls, true),
        RollAttack => roll_attack(cls),
        BackstepAttack => backstep_attack(cls),
        CrouchAttack => crouch_attack(cls),
        Heavy1Charge => heavy_charge(cls, 0),
        Heavy1 => heavy(cls, 0),
        Heavy2Charge => heavy_charge(cls, 1),
        Heavy2 => heavy(cls, 1),
        GuardCounter => guard_counter(cls),
        JumpLightLand => jump_land(cls, false, false),
        JumpLightLandShort => jump_land(cls, false, true),
        JumpHeavyLand => jump_land(cls, true, false),
        JumpHeavyLandShort => jump_land(cls, true, true),
        LeftLight1 if shield => return None,
        LeftLight1 if cls.id == 5 => spear_light1(cls).titled("left-hand attack"),
        LeftLight1 => left_light(cls, 0),
        LeftLight2 => left_light(cls, 1),
        LeftLight3 => left_light(cls, 2),
        LeftLight4 if chain_len(cls) >= 4 => left_light(cls, 3),
        LeftLight5 if chain_len(cls) >= 5 => left_light(cls, 4),
        LeftLight6 if chain_len(cls) >= 6 => left_light(cls, 5),
        LeftLight4 | LeftLight5 | LeftLight6 => return None,
        PairedLight1 if shield | torch => return None,
        PairedLight1 => paired_light(cls, 0),
        PairedLight2 => paired_light(cls, 1),
        PairedLight3 => paired_light(cls, 2),
        PairedLight4 if shield | torch => return None,
        PairedLight4 => paired_light(cls, 3),
        PairedLight5 | PairedLight6 => return None,
        PairedRun if shield | torch => return None,
        PairedRun => paired_special(cls, "paired, running", 28.0 * cls.speed, cls.reach),
        PairedRoll if shield | torch => return None,
        PairedRoll => paired_special(cls, "paired, rolling", 28.0 * cls.speed, cls.reach),
        PairedBackstep if shield | torch => return None,
        PairedBackstep => paired_special(cls, "paired, backstep", 26.0 * cls.speed, cls.reach),
        PairedJumpLand if shield | torch => return None,
        PairedJumpLand => paired_special(cls, "paired, jump land", 30.0 * cls.speed, cls.reach),
        PairedJumpLandShort if shield | torch => return None,
        PairedJumpLandShort => paired_special(cls, "paired, jump land (short)", 22.0 * cls.speed, cls.reach),
    })
}

pub fn air_attack(weapon: usize, _two_hand: bool, heavy: bool) -> Option<AirAttackDef> {
    let cls = class(weapon)?;
    // The heavy swing is still coming down when the feet touch — the landing
    // carries it (JumpHeavyLand, not the short variant). A stand jump presses
    // at frame 6+ and touches down ~27 frames later.
    let (from, to) = if heavy { (12.0, 30.0) } else { (8.0, 14.0) };
    let (from, to) = (from * cls.speed, to * cls.speed);
    Some(AirAttackDef {
        from,
        to,
        // The charged landing costs JUMP_COST + 20 — pinned by the tests.
        stamina: if heavy { 20.0 } else { 15.0 },
        source: if heavy { "jump_attack_heavy" } else { "jump_attack_light" },
        radius: 0.10,
        blade: if cls.thrust {
            thrust(from, to, cls.reach, 0.7)
        } else {
            slash(from, to, cls.reach, 1.0, true)
        },
    })
}

pub fn air_paired(weapon: usize) -> Option<AirAttackDef> {
    let cls = class(weapon)?;
    if cls.id == 0 || cls.id == 7 {
        return None; // shield and torch have no paired moveset
    }
    let mut def = air_attack(weapon, true, false)?;
    def.source = "jump_attack_paired";
    Some(def)
}

pub fn swap(kind: SwapKind) -> SwapDef {
    // The two-hand change is pinned: (start_len, end_len, apply, free_from)
    // = (5, 17, 3, 7), and the change is fully played after start+end.
    match kind {
        SwapKind::ToTwoHandRight => SwapDef {
            start: "grip_2h_right_start", end: "grip_2h_right_end",
            start_len: 5.0, end_len: 17.0, apply: 3.0, free_from: 7.0,
        },
        SwapKind::ToTwoHandLeft => SwapDef {
            start: "grip_2h_left_start", end: "grip_2h_left_end",
            start_len: 5.0, end_len: 17.0, apply: 3.0, free_from: 7.0,
        },
        SwapKind::ToOneHandFromRight => SwapDef {
            start: "grip_1h_from_right_start", end: "grip_1h_from_right_end",
            start_len: 4.0, end_len: 13.0, apply: 3.0, free_from: 6.0,
        },
        SwapKind::ToOneHandFromLeft => SwapDef {
            start: "grip_1h_from_left_start", end: "grip_1h_from_left_end",
            start_len: 4.0, end_len: 13.0, apply: 3.0, free_from: 6.0,
        },
        SwapKind::NextWeapon => SwapDef {
            start: "weapon_swap_start", end: "weapon_swap_end",
            start_len: 5.0, end_len: 15.0, apply: 3.0, free_from: 7.0,
        },
        SwapKind::NextLeft => SwapDef {
            start: "offhand_swap_start", end: "offhand_swap_end",
            start_len: 4.0, end_len: 12.0, apply: 3.0, free_from: 6.0,
        },
    }
}
