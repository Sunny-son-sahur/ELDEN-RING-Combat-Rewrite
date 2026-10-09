//! Procedural animation: every pose is generated in code, keyed on
//! (clip name, frame). The rig asks `Clips` for a name and a frame and gets
//! model-space joint positions plus the axes of the joints whose orientation
//! matters — the same shape the old baked file had, computed from a skeleton
//! and a library of hand-authored pose functions instead.
//!
//! Two conventions the rig depends on:
//! - The head bone's X axis runs up the neck and its Z axis out of the face.
//! - Weapon bones point the blade along their Y axis (the left-hand bone is
//!   mirrored by the model itself, so its axes read the other way).

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

use tarnished_sim::data::*;

// Animation ids within a category (kept in step with the rig).
const IDLE: u32 = 0;
const GUARD: u32 = 100;
const WALK: u32 = 20000;
const RUN: u32 = 20100;
const SPRINT: u32 = 20200;
const RUN_STOP: u32 = 22100;
const CROUCH_IDLE: u32 = 300000;
const CROUCH_WALK: u32 = 320000;
const CROUCH_RUN: u32 = 320100;
const CROUCH_RUN_STOP: u32 = 322100;
const CROUCH_ENTER: u32 = 390000;
const CROUCH_EXIT: u32 = 390001;
const AIR_LOOP: u32 = 202040;
const FALL_START: u32 = 4000;
const DEATH: u32 = 17002;

pub struct Clip {
    /// Frames to cross-fade over when this clip starts.
    pub blend: f32,
    pub frames: usize,
    data: Vec<f32>,
}

#[derive(Resource)]
pub struct Clips {
    joints: Vec<String>,
    oriented: Vec<String>,
    /// Floats per frame.
    pub stride: usize,
    clips: HashMap<String, Clip>,
}

// --- Skeleton ---------------------------------------------------------------

/// One joint: its rest place in model space, its parent, and how it sits in
/// the parent's frame when everything is at rest (identity but for the head
/// and the weapon bones, whose axes carry meaning).
struct Bone {
    rest: Vec3,
    parent: Option<usize>,
    seat: Quat,
}

/// The game's skeleton, parents before children so one pass solves the chain.
fn skeleton() -> (Vec<String>, Vec<Bone>, Vec<String>) {
    let h = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
    let mirrored = |x: f32, y: f32, z: f32| Vec3::new(-x, y, z);
    // (name, parent name, rest position)
    let spec: Vec<(&str, Option<&str>, Vec3)> = vec![
        ("Pelvis", None, h(0.0, 0.95, 0.0)),
        ("Spine1", Some("Pelvis"), h(0.0, 1.10, 0.01)),
        ("Spine2", Some("Spine1"), h(0.0, 1.25, 0.0)),
        ("Neck", Some("Spine2"), h(0.0, 1.44, -0.01)),
        ("Head", Some("Neck"), h(0.0, 1.56, 0.0)),
        ("L_Thigh", Some("Pelvis"), h(0.10, 0.92, 0.0)),
        ("L_Calf", Some("L_Thigh"), h(0.11, 0.51, 0.02)),
        ("L_Foot", Some("L_Calf"), h(0.11, 0.10, -0.02)),
        ("L_Toe0", Some("L_Foot"), h(0.11, 0.04, 0.12)),
        ("R_Thigh", Some("Pelvis"), mirrored(0.10, 0.92, 0.0)),
        ("R_Calf", Some("R_Thigh"), mirrored(0.11, 0.51, 0.02)),
        ("R_Foot", Some("R_Calf"), mirrored(0.11, 0.10, -0.02)),
        ("R_Toe0", Some("R_Foot"), mirrored(0.11, 0.04, 0.12)),
        ("L_Clavicle", Some("Spine2"), h(0.05, 1.40, 0.0)),
        ("L_UpperArm", Some("L_Clavicle"), h(0.19, 1.39, 0.0)),
        ("L_Forearm", Some("L_UpperArm"), h(0.21, 1.12, 0.01)),
        ("L_Hand", Some("L_Forearm"), h(0.22, 0.87, 0.01)),
        ("L_Weapon", Some("L_Hand"), h(0.22, 0.79, 0.01)),
        ("L_Finger0", Some("L_Hand"), h(0.195, 0.855, 0.015)),
        ("L_Finger01", Some("L_Finger0"), h(0.180, 0.825, 0.030)),
        ("L_Finger02", Some("L_Finger01"), h(0.172, 0.800, 0.042)),
        ("L_Finger1", Some("L_Hand"), h(0.225, 0.840, 0.045)),
        ("L_Finger2", Some("L_Finger1"), h(0.225, 0.800, 0.045)),
        ("L_Finger21", Some("L_Finger2"), h(0.225, 0.765, 0.045)),
        ("L_Finger22", Some("L_Finger21"), h(0.225, 0.735, 0.045)),
        ("R_Clavicle", Some("Spine2"), mirrored(0.05, 1.40, 0.0)),
        ("R_UpperArm", Some("R_Clavicle"), mirrored(0.19, 1.39, 0.0)),
        ("R_Forearm", Some("R_UpperArm"), mirrored(0.21, 1.12, 0.01)),
        ("R_Hand", Some("R_Forearm"), mirrored(0.22, 0.87, 0.01)),
        ("R_Weapon", Some("R_Hand"), mirrored(0.22, 0.79, 0.01)),
        ("R_Finger0", Some("R_Hand"), mirrored(0.195, 0.855, 0.015)),
        ("R_Finger01", Some("R_Finger0"), mirrored(0.180, 0.825, 0.030)),
        ("R_Finger02", Some("R_Finger01"), mirrored(0.172, 0.800, 0.042)),
        ("R_Finger1", Some("R_Hand"), mirrored(0.225, 0.840, 0.045)),
        ("R_Finger2", Some("R_Finger1"), mirrored(0.225, 0.800, 0.045)),
        ("R_Finger21", Some("R_Finger2"), mirrored(0.225, 0.765, 0.045)),
        ("R_Finger22", Some("R_Finger21"), mirrored(0.225, 0.735, 0.045)),
    ];
    let index = |name: &str| spec.iter().position(|&(n, _, _)| n == name).expect("skeleton typo");
    // The head's frame: X up the neck, Z out of the face (Y across the skull).
    let head = Quat::from_mat3(&Mat3::from_cols(Vec3::Y, Vec3::NEG_X, Vec3::Z));
    let bones = spec
        .iter()
        .map(|&(name, parent, rest)| Bone {
            rest,
            parent: parent.map(index),
            seat: match name {
                "Head" => head,
                // The right-hand blade points along +Y of its bone, which the
                // model draws grip-to-tip; the left-hand models are mirrored
                // by the shell, so that bone's +Y is the other way.
                "R_Weapon" => Quat::from_rotation_z(std::f32::consts::PI),
                _ => Quat::IDENTITY,
            },
        })
        .collect();
    let names = spec.into_iter().map(|(name, _, _)| name.to_string()).collect();
    (names, bones, vec!["Head".into(), "L_Weapon".into(), "R_Weapon".into()])
}

/// A working pose: local joint rotations, solved to model space.
struct Pose<'a> {
    bones: &'a [Bone],
    index: HashMap<&'a str, usize>,
    local: Vec<Quat>,
    root: Vec3,
    pos: Vec<Vec3>,
    basis: Vec<Quat>,
}

impl<'a> Pose<'a> {
    fn new(bones: &'a [Bone], names: &'a [String]) -> Self {
        let index = names.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
        let n = bones.len();
        Self {
            bones,
            index,
            local: vec![Quat::IDENTITY; n],
            root: Vec3::ZERO,
            pos: vec![Vec3::ZERO; n],
            basis: vec![Quat::IDENTITY; n],
        }
    }

    fn reset(&mut self) {
        self.local.fill(Quat::IDENTITY);
        self.root = Vec3::ZERO;
    }

    /// Euler (pitch, yaw, roll) on a joint, replacing whatever was there.
    fn set(&mut self, joint: &str, x: f32, y: f32, z: f32) {
        let i = self.index[joint];
        self.local[i] = Quat::from_rotation_x(x) * Quat::from_rotation_y(y) * Quat::from_rotation_z(z);
    }

    /// Lift or drop the whole body without tipping it.
    fn height(&mut self, y: f32) {
        self.root = Vec3::new(0.0, y, 0.0);
    }

    /// Walk the chain: every joint's place and facing from its parent's.
    fn solve(&mut self) {
        for i in 0..self.bones.len() {
            let bone = &self.bones[i];
            match bone.parent {
                None => {
                    self.pos[i] = bone.rest + self.root;
                    self.basis[i] = self.local[i];
                }
                Some(p) => {
                    let offset = bone.rest - self.bones[p].rest;
                    self.pos[i] = self.pos[p] + self.basis[p] * (self.local[i] * offset);
                    self.basis[i] = self.basis[p] * self.local[i] * bone.seat;
                }
            }
        }
    }
}

// --- The pose vocabulary ----------------------------------------------------
// Every function paints a full pose from neutral. Sign notes: a bone hanging
// down swings forward under a negative X rotation and backward under a
// positive one; a knee (the calf) bends with a positive X; an elbow flexes
// negative; the spine leans forward positive; +Z swings a left-side bone out
// and a right-side bone in.

fn legs(p: &mut Pose, lt: f32, lk: f32, la: f32, ltoe: f32, rt: f32, rk: f32, ra: f32, rtoe: f32) {
    p.set("L_Thigh", lt, 0.0, 0.0);
    p.set("L_Calf", lk, 0.0, 0.0);
    p.set("L_Foot", la, 0.0, 0.0);
    p.set("L_Toe0", ltoe, 0.0, 0.0);
    p.set("R_Thigh", rt, 0.0, 0.0);
    p.set("R_Calf", rk, 0.0, 0.0);
    p.set("R_Foot", ra, 0.0, 0.0);
    p.set("R_Toe0", rtoe, 0.0, 0.0);
}

/// Right arm: swing at the shoulder, bend at the elbow.
fn arm_r(p: &mut Pose, x: f32, y: f32, z: f32, elbow: f32) {
    p.set("R_Clavicle", x * 0.12, y * 0.2, 0.0);
    p.set("R_UpperArm", x, y, z);
    p.set("R_Forearm", elbow, 0.0, 0.0);
}

fn arm_l(p: &mut Pose, x: f32, y: f32, z: f32, elbow: f32) {
    p.set("L_Clavicle", x * 0.12, y * 0.2, 0.0);
    p.set("L_UpperArm", x, y, z);
    p.set("L_Forearm", elbow, 0.0, 0.0);
}

/// Spine lean (1, 2), twist, side-bend; the head stays level-ish with it.
fn torso(p: &mut Pose, x1: f32, x2: f32, y: f32, z: f32) {
    p.set("Spine1", x1, y * 0.6, z * 0.6);
    p.set("Spine2", x2, y * 0.4, z * 0.4);
    p.set("Neck", -x1 * 0.35, -y * 0.3, -z * 0.3);
}

/// Fingers closed round a grip; the rig re-seats them on the weapon anyway,
/// so every clip carries the same hand.
fn grip(p: &mut Pose) {
    for (side, curl) in [("L_", -1.0), ("R_", 1.0)] {
        p.set(&format!("{side}Finger1"), 0.0, 0.0, 0.25 * curl);
        p.set(&format!("{side}Finger2"), 0.0, 0.0, 0.85 * curl);
        p.set(&format!("{side}Finger21"), 0.0, 0.0, 1.05 * curl);
        p.set(&format!("{side}Finger22"), 0.0, 0.0, 0.55 * curl);
        p.set(&format!("{side}Finger0"), 0.0, -0.3 * curl, -0.5 * curl);
        p.set(&format!("{side}Finger01"), 0.0, 0.0, -0.9 * curl);
        p.set(&format!("{side}Finger02"), 0.0, 0.0, -0.7 * curl);
    }
}

// Stands and carves ----------------------------------------------------------

fn neutral(p: &mut Pose) {
    arm_l(p, 0.0, 0.0, 0.10, -0.14);
    arm_r(p, 0.0, 0.0, -0.10, -0.14);
    grip(p);
}

/// One-handed rest: weapon down in the right hand, left hand easy.
fn carry_r(p: &mut Pose) {
    neutral(p);
    arm_r(p, -0.30, 0.0, -0.22, -0.50);
    arm_l(p, -0.16, 0.0, 0.12, -0.30);
    torso(p, 0.04, 0.0, -0.06, 0.0);
}

/// Shield stance: board held across the chest.
fn carry_shield(p: &mut Pose) {
    carry_r(p);
    arm_l(p, -0.65, 0.25, 0.18, -1.15);
}

/// Two-handed carry: both hands at the weapon, out right.
fn carry_two(p: &mut Pose) {
    neutral(p);
    arm_r(p, -0.50, -0.10, -0.30, -0.75);
    arm_l(p, -0.55, 0.45, -0.55, -1.05);
    torso(p, 0.08, 0.0, -0.14, 0.0);
}

/// One-handed guard: shield up.
fn guard_shield(p: &mut Pose) {
    carry_shield(p);
    p.height(-0.05);
    legs(p, -0.18, 0.34, -0.16, 0.0, -0.10, 0.22, -0.12, 0.0);
    arm_l(p, -1.35, 0.30, -0.18, -1.50);
    torso(p, 0.10, 0.05, 0.16, 0.0);
    p.set("Neck", -0.05, 0.25, 0.0);
}

/// Two-handed guard: weapon raised across.
fn guard_two(p: &mut Pose) {
    carry_two(p);
    p.height(-0.06);
    legs(p, -0.22, 0.38, -0.18, 0.0, -0.12, 0.24, -0.12, 0.0);
    arm_r(p, -1.10, -0.15, -0.35, -1.25);
    arm_l(p, -0.95, 0.55, -0.60, -1.10);
    torso(p, 0.14, 0.06, 0.22, 0.0);
}

fn crouch_base(p: &mut Pose) {
    neutral(p);
    p.height(-0.34);
    legs(p, -1.15, 1.85, -0.70, 0.10, -1.05, 1.70, -0.65, 0.10);
    arm_l(p, -0.45, 0.0, 0.30, -0.85);
    arm_r(p, -0.45, 0.0, -0.30, -0.85);
    torso(p, 0.42, 0.18, 0.0, 0.0);
    p.set("Neck", -0.42, 0.0, 0.0);
}

// Wind-ups and strikes -------------------------------------------------------

fn wind_slash(p: &mut Pose) {
    neutral(p);
    arm_r(p, 0.95, 0.25, 0.55, -2.20);
    arm_l(p, -0.45, 0.0, 0.35, -0.60);
    torso(p, -0.10, -0.05, -0.55, 0.0);
    legs(p, -0.10, 0.15, -0.05, 0.0, 0.14, 0.25, -0.15, 0.0);
}

fn strike_slash(p: &mut Pose) {
    neutral(p);
    arm_r(p, -1.35, -0.15, -0.45, -0.25);
    arm_l(p, 0.40, 0.0, 0.30, -0.50);
    torso(p, 0.30, 0.15, 0.50, -0.12);
    legs(p, -0.32, 0.35, -0.12, 0.10, 0.22, 0.35, -0.20, 0.0);
}

fn follow_slash(p: &mut Pose) {
    neutral(p);
    arm_r(p, -0.60, -0.30, -0.80, -0.70);
    arm_l(p, 0.20, 0.0, 0.25, -0.45);
    torso(p, 0.22, 0.10, 0.62, -0.20);
    legs(p, -0.22, 0.25, -0.08, 0.05, 0.16, 0.28, -0.15, 0.0);
}

fn wind_over(p: &mut Pose) {
    neutral(p);
    arm_r(p, -2.50, 0.0, 0.30, -0.55);
    arm_l(p, -2.30, 0.0, -0.35, -0.85);
    torso(p, -0.24, -0.10, 0.0, 0.0);
    legs(p, 0.10, 0.12, -0.22, 0.20, 0.16, 0.18, -0.25, 0.20);
}

fn mid_over(p: &mut Pose) {
    neutral(p);
    arm_r(p, -1.45, 0.0, 0.20, -0.30);
    arm_l(p, -1.35, 0.0, -0.25, -0.50);
    torso(p, 0.08, 0.04, 0.0, 0.0);
}

fn strike_over(p: &mut Pose) {
    neutral(p);
    p.height(-0.10);
    arm_r(p, -0.55, 0.0, 0.10, -0.10);
    arm_l(p, -0.50, 0.0, -0.15, -0.15);
    torso(p, 0.68, 0.32, 0.0, 0.0);
    legs(p, -0.55, 0.75, -0.25, 0.15, -0.50, 0.70, -0.22, 0.15);
    p.set("Neck", 0.10, 0.0, 0.0);
}

fn thrust_wind(p: &mut Pose) {
    guard_two(p);
    arm_r(p, -0.40, 0.10, -0.55, -1.60);
    torso(p, 0.10, 0.05, -0.35, 0.0);
}

fn thrust_out(p: &mut Pose) {
    neutral(p);
    arm_r(p, -1.50, 0.0, -0.05, -0.05);
    arm_l(p, 0.35, 0.0, 0.45, -0.70);
    torso(p, 0.22, 0.10, 0.38, -0.10);
    legs(p, -0.40, 0.40, -0.10, 0.12, 0.28, 0.40, -0.22, 0.0);
}

/// Left-hand mirror of the slash.
fn wind_slash_left(p: &mut Pose) {
    neutral(p);
    arm_l(p, 0.95, -0.25, -0.55, -2.20);
    arm_r(p, -0.45, 0.0, -0.35, -0.60);
    torso(p, -0.10, -0.05, 0.55, 0.0);
}

fn strike_slash_left(p: &mut Pose) {
    neutral(p);
    arm_l(p, -1.35, 0.15, 0.45, -0.25);
    arm_r(p, 0.40, 0.0, -0.30, -0.50);
    torso(p, 0.30, 0.15, -0.50, 0.12);
    legs(p, -0.30, 0.35, -0.12, 0.10, 0.20, 0.35, -0.20, 0.0);
}

fn follow_slash_left(p: &mut Pose) {
    neutral(p);
    arm_l(p, -0.60, 0.30, 0.80, -0.70);
    torso(p, 0.22, 0.10, -0.62, 0.20);
}

fn pair_wind(p: &mut Pose) {
    neutral(p);
    arm_r(p, -1.10, 0.10, 0.55, -2.00);
    arm_l(p, -1.00, -0.10, -0.55, -2.00);
    torso(p, -0.12, -0.05, 0.0, 0.0);
    legs(p, 0.06, 0.12, -0.18, 0.15, 0.10, 0.16, -0.20, 0.15);
}

fn pair_strike(p: &mut Pose) {
    neutral(p);
    arm_r(p, -1.20, -0.10, -0.35, -0.20);
    arm_l(p, -1.15, 0.10, 0.35, -0.20);
    torso(p, 0.34, 0.16, 0.0, 0.0);
    legs(p, -0.42, 0.45, -0.15, 0.12, -0.36, 0.40, -0.12, 0.12);
}

fn pair_follow(p: &mut Pose) {
    neutral(p);
    arm_r(p, -0.85, -0.20, -0.60, -0.55);
    arm_l(p, -0.80, 0.20, 0.60, -0.55);
    torso(p, 0.24, 0.10, 0.0, 0.10);
}

fn settle(p: &mut Pose) {
    carry_r(p);
}

fn settle_two(p: &mut Pose) {
    carry_two(p);
}

// Air, landings, reactions ---------------------------------------------------

fn leap(p: &mut Pose) {
    neutral(p);
    p.height(0.05);
    legs(p, -0.55, 1.00, 0.30, 0.25, -0.35, 0.70, 0.25, 0.25);
    arm_r(p, 0.55, 0.0, -0.25, -0.60);
    arm_l(p, 0.50, 0.0, 0.25, -0.55);
    torso(p, 0.15, 0.05, 0.0, 0.0);
}

fn tuck_air(p: &mut Pose) {
    neutral(p);
    legs(p, -1.00, 1.35, -0.20, 0.10, -0.70, 1.00, -0.10, 0.10);
    arm_r(p, -0.70, 0.0, -0.60, -0.90);
    arm_l(p, -0.65, 0.0, 0.60, -0.85);
    torso(p, 0.20, 0.08, 0.0, 0.0);
}

fn legs_fwd_air(p: &mut Pose) {
    neutral(p);
    legs(p, -0.85, 0.55, 0.15, 0.20, -0.55, 0.40, 0.10, 0.20);
    torso(p, 0.30, 0.12, 0.0, 0.0);
}

fn fall_a(p: &mut Pose) {
    neutral(p);
    legs(p, -0.45, 0.60, 0.10, 0.10, 0.30, 0.85, -0.15, 0.10);
    arm_r(p, -1.20, 0.0, -0.75, -0.80);
    arm_l(p, -0.60, 0.0, 0.95, -1.10);
    torso(p, -0.12, 0.0, 0.10, -0.08);
}

fn fall_b(p: &mut Pose) {
    neutral(p);
    legs(p, 0.30, 0.85, -0.15, 0.10, -0.45, 0.60, 0.10, 0.10);
    arm_r(p, -0.60, 0.0, -0.95, -1.10);
    arm_l(p, -1.20, 0.0, 0.75, -0.80);
    torso(p, -0.12, 0.0, -0.10, 0.08);
}

fn fall_start(p: &mut Pose) {
    neutral(p);
    legs(p, -0.30, 0.45, -0.35, 0.35, 0.35, 0.55, -0.10, 0.0);
    arm_r(p, -1.60, 0.0, -0.50, -0.50);
    arm_l(p, -1.50, 0.0, 0.50, -0.45);
    torso(p, -0.20, 0.0, 0.0, 0.0);
}

fn land_soft(p: &mut Pose) {
    neutral(p);
    p.height(-0.06);
    legs(p, -0.45, 0.60, -0.22, 0.10, -0.40, 0.55, -0.20, 0.10);
    arm_r(p, -0.50, 0.0, -0.35, -0.75);
    arm_l(p, -0.45, 0.0, 0.35, -0.70);
    torso(p, 0.30, 0.12, 0.0, 0.0);
}

fn land_run(p: &mut Pose) {
    neutral(p);
    p.height(-0.10);
    legs(p, -0.75, 0.90, -0.30, 0.12, 0.35, 0.55, -0.28, 0.0);
    arm_r(p, -0.70, 0.0, -0.50, -0.85);
    arm_l(p, 0.30, 0.0, 0.45, -0.60);
    torso(p, 0.42, 0.16, 0.15, 0.0);
}

fn land_hard(p: &mut Pose) {
    neutral(p);
    p.height(-0.28);
    legs(p, -1.00, 1.55, -0.60, 0.15, -0.85, 1.35, -0.50, 0.15);
    arm_l(p, -1.60, 0.0, -0.35, -0.25);
    arm_r(p, 0.30, 0.0, -0.55, -0.90);
    torso(p, 0.62, 0.25, 0.10, -0.10);
    p.set("Neck", 0.15, 0.0, 0.0);
}

fn land_fall(p: &mut Pose) {
    neutral(p);
    p.height(-0.18);
    legs(p, -0.70, 1.05, -0.40, 0.12, -0.60, 0.95, -0.35, 0.12);
    arm_r(p, -0.60, 0.0, -0.70, -1.00);
    arm_l(p, -0.55, 0.0, 0.70, -0.95);
    torso(p, 0.45, 0.18, 0.0, 0.0);
}

fn flinch(p: &mut Pose) {
    carry_r(p);
    torso(p, -0.24, -0.10, 0.15, 0.10);
    p.set("Neck", -0.30, 0.1, 0.0);
    arm_r(p, -0.55, 0.0, -0.45, -0.95);
    arm_l(p, -0.50, 0.0, 0.45, -0.90);
}

fn stagger(p: &mut Pose) {
    neutral(p);
    p.height(-0.08);
    torso(p, -0.45, -0.18, -0.20, -0.22);
    arm_r(p, -1.20, 0.0, -0.60, -0.70);
    arm_l(p, -0.85, 0.0, 0.80, -0.55);
    legs(p, 0.25, 0.35, -0.30, 0.25, -0.40, 0.60, -0.15, 0.10);
}

fn stagger_large(p: &mut Pose) {
    neutral(p);
    p.height(-0.22);
    torso(p, -0.60, -0.25, -0.35, -0.35);
    arm_r(p, -0.40, 0.0, -1.10, -0.40);
    arm_l(p, -1.30, 0.0, 0.55, -0.35);
    legs(p, 0.45, 0.70, -0.40, 0.30, -0.75, 1.10, -0.30, 0.15);
}

fn to_knee(p: &mut Pose) {
    neutral(p);
    p.height(-0.40);
    legs(p, -1.30, 2.10, -0.75, 0.15, 0.35, 0.30, -0.55, 0.30);
    torso(p, -0.30, -0.10, -0.10, -0.15);
    arm_r(p, -0.55, 0.0, -0.55, -0.85);
    arm_l(p, -0.90, 0.0, 0.45, -0.60);
}

fn braced(p: &mut Pose) {
    guard_shield(p);
    p.height(-0.10);
    arm_l(p, -1.55, 0.35, -0.30, -1.60);
    arm_r(p, -0.30, 0.0, -0.25, -1.00);
    torso(p, 0.05, 0.02, 0.20, 0.0);
}

fn flung(p: &mut Pose) {
    neutral(p);
    p.height(-0.06);
    arm_r(p, -0.50, 0.0, -1.25, -0.35);
    arm_l(p, -0.45, 0.0, 1.25, -0.35);
    torso(p, -0.38, -0.15, 0.0, 0.0);
    legs(p, 0.20, 0.30, -0.30, 0.25, 0.25, 0.35, -0.32, 0.25);
    p.set("Neck", -0.45, 0.0, 0.0);
}

fn tuck_ball(p: &mut Pose) {
    neutral(p);
    p.height(-0.16);
    legs(p, -1.70, 2.25, -0.50, 0.15, -1.65, 2.20, -0.48, 0.15);
    arm_r(p, -0.95, 0.0, -0.30, -1.65);
    arm_l(p, -0.90, 0.0, 0.30, -1.60);
    torso(p, 0.95, 0.45, 0.0, 0.0);
    p.set("Neck", 0.50, 0.0, 0.0);
}

fn tuck_ball_tight(p: &mut Pose) {
    tuck_ball(p);
    p.height(-0.24);
    legs(p, -2.00, 2.55, -0.55, 0.15, -1.95, 2.50, -0.52, 0.15);
    torso(p, 1.15, 0.55, 0.0, 0.0);
    arm_r(p, -1.15, 0.0, -0.25, -1.85);
    arm_l(p, -1.10, 0.0, 0.25, -1.80);
}

fn back_lean(p: &mut Pose) {
    neutral(p);
    torso(p, -0.30, -0.12, 0.0, 0.0);
    arm_r(p, -0.80, 0.0, -0.50, -0.60);
    arm_l(p, -0.75, 0.0, 0.50, -0.55);
    legs(p, 0.15, 0.25, -0.22, 0.20, -0.25, 0.40, -0.12, 0.10);
}

fn hop(p: &mut Pose) {
    neutral(p);
    p.height(0.03);
    legs(p, -0.30, 0.60, 0.20, 0.20, -0.25, 0.55, 0.18, 0.20);
    arm_r(p, 0.30, 0.0, -0.45, -0.50);
    arm_l(p, 0.28, 0.0, 0.45, -0.48);
    torso(p, -0.16, 0.0, 0.0, 0.0);
}

fn plant(p: &mut Pose) {
    neutral(p);
    p.height(-0.08);
    legs(p, -0.70, 0.55, -0.25, 0.12, 0.50, 0.95, -0.35, 0.0);
    arm_r(p, -0.30, 0.0, -0.75, -0.60);
    arm_l(p, -0.55, 0.0, 0.60, -0.70);
    torso(p, 0.38, 0.14, 0.10, 0.0);
}

fn crumple(p: &mut Pose) {
    neutral(p);
    p.height(-0.38);
    p.set("Pelvis", -0.55, 0.0, 0.0);
    legs(p, -0.85, 1.30, -0.35, 0.10, -0.75, 1.20, -0.30, 0.10);
    torso(p, 0.35, 0.15, 0.15, 0.10);
    arm_r(p, -0.30, 0.0, -0.25, -0.40);
    arm_l(p, -0.25, 0.0, 0.30, -0.35);
    p.set("Neck", 0.35, 0.1, 0.0);
}

fn dead_flat(p: &mut Pose) {
    neutral(p);
    p.height(-0.60);
    p.set("Pelvis", -1.45, 0.0, 0.0);
    legs(p, -0.10, 0.15, 0.35, 0.0, -0.05, 0.10, 0.32, 0.0);
    arm_r(p, 0.15, 0.0, -1.30, -0.20);
    arm_l(p, 0.15, 0.0, 1.30, -0.20);
    p.set("Neck", 0.25, 0.15, 0.0);
}

fn low_wind(p: &mut Pose) {
    crouch_base(p);
    arm_r(p, 0.65, 0.15, -0.45, -1.90);
    torso(p, 0.50, 0.20, -0.45, 0.0);
}

fn low_strike(p: &mut Pose) {
    crouch_base(p);
    arm_r(p, -1.15, -0.10, -0.35, -0.20);
    torso(p, 0.72, 0.30, 0.40, -0.10);
    legs(p, -1.30, 1.95, -0.72, 0.12, -0.55, 0.85, -0.35, 0.10);
}

fn lunge_wind(p: &mut Pose) {
    neutral(p);
    legs(p, -0.30, 0.50, -0.15, 0.10, 0.60, 0.45, -0.30, 0.0);
    arm_r(p, 0.70, 0.20, 0.45, -1.90);
    arm_l(p, -0.40, 0.0, 0.40, -0.60);
    torso(p, -0.05, 0.0, -0.35, 0.0);
}

fn lunge_strike(p: &mut Pose) {
    neutral(p);
    legs(p, -0.95, 0.70, -0.20, 0.15, 0.70, 0.55, -0.35, 0.0);
    arm_r(p, -1.30, -0.10, -0.40, -0.25);
    arm_l(p, 0.45, 0.0, 0.35, -0.55);
    torso(p, 0.42, 0.18, 0.45, -0.10);
}

fn swap_high(p: &mut Pose) {
    neutral(p);
    arm_r(p, -0.75, 0.0, -0.15, -1.25);
    arm_l(p, -0.85, 0.35, -0.50, -1.30);
    torso(p, 0.12, 0.05, -0.20, 0.0);
    p.set("Neck", 0.15, -0.1, 0.0);
}

fn swap_low(p: &mut Pose) {
    neutral(p);
    arm_r(p, -0.35, 0.0, -0.30, -1.10);
    arm_l(p, -0.30, 0.0, 0.45, -0.80);
    torso(p, 0.10, 0.05, -0.10, 0.0);
}

fn swap_belt(p: &mut Pose) {
    carry_r(p);
    arm_r(p, 0.05, 0.0, 0.35, -1.75);
    arm_l(p, -0.20, 0.0, 0.30, -1.20);
    p.set("Neck", 0.25, -0.15, 0.0);
}

fn swap_belt_up(p: &mut Pose) {
    carry_r(p);
    arm_r(p, -0.55, 0.0, 0.15, -1.50);
    arm_l(p, -0.60, 0.20, -0.20, -1.30);
    p.set("Neck", 0.10, 0.0, 0.0);
}

// --- Keyframe playback ------------------------------------------------------

type Key = (f32, fn(&mut Pose));

/// Play a key table at `t` (0..1 for loops, 0..1 across a one-shot's frames).
/// Looped tables wrap their last key back to the first.
fn keyframe(p: &mut Pose, keys: &[Key], t: f32, wrap: bool) {
    let t = if wrap { t.fract() } else { t.clamp(0.0, 1.0) };
    let last = keys.len() - 1;
    let mut i = 0;
    while i < last && keys[i + 1].0 <= t {
        i += 1;
    }
    let (ta, a) = keys[i];
    let (tb, b) = if i == last {
        if !wrap {
            p.reset();
            a(p);
            grip(p);
            p.solve();
            return;
        }
        (1.0, keys[0].1)
    } else {
        (keys[i + 1].0, keys[i + 1].1)
    };
    let span = (tb - ta).max(1e-4);
    let mut u = ((t - ta) / span).clamp(0.0, 1.0);
    u = u * u * (3.0 - 2.0 * u);
    p.reset();
    a(p);
    let mut base = p.local.clone();
    let root_a = p.root;
    p.reset();
    b(p);
    for (slot, to) in base.iter_mut().zip(p.local.iter()) {
        *slot = slot.slerp(*to, u);
    }
    p.local = base;
    p.root = root_a.lerp(p.root, u);
    grip(p);
    p.solve();
}

// Stance poses (full body, per category) ------------------------------------

fn idle(p: &mut Pose, t: f32, cat: u8) {
    // A slow breath over the carry, so standing is never dead still.
    let breathe = (t * std::f32::consts::TAU).sin();
    let sway = (t * std::f32::consts::TAU * 0.5).sin();
    let mut draw: fn(&mut Pose) = if cat == 0 { carry_shield } else { carry_r };
    if cat == 3 {
        // Doubles as the longsword's two-handed rest.
        draw = carry_two;
    }
    p.reset();
    draw(p);
    torso(p, 0.04 + 0.02 * breathe, 0.0, -0.06 + 0.02 * sway, 0.0);
    arm_r(p, -0.30 - 0.03 * breathe, 0.0, -0.22, -0.50 - 0.04 * breathe);
    p.height(0.008 * breathe);
    p.set("Neck", 0.02 * sway, 0.05 * sway, 0.0);
    grip(p);
    p.solve();
}

fn guard(p: &mut Pose, t: f32, cat: u8) {
    let breath = (t * std::f32::consts::TAU).sin();
    let mut draw: fn(&mut Pose) = if cat == 0 { guard_shield } else { guard_two };
    if cat == 2 {
        draw = guard_two; // the longsword's one-handed guard still holds with both
    }
    p.reset();
    draw(p);
    // Guard shifts weight side to side, waiting.
    let lean = 0.04 * (t * std::f32::consts::TAU * 2.0).sin();
    torso(p, 0.10 + 0.02 * breath, 0.05, (if cat == 0 { 0.16 } else { 0.22 }) + lean, 0.0);
    p.height(-0.05 - 0.01 * breath);
    grip(p);
    p.solve();
}

// Locomotion -----------------------------------------------------------------

static WALK_FWD: &[Key] = &[
    (0.00, walk_l_strike),
    (0.17, walk_l_loading),
    (0.34, walk_l_mid),
    (0.50, walk_r_strike),
    (0.67, walk_r_loading),
    (0.84, walk_r_mid),
];

fn walk_l_strike(p: &mut Pose) {
    legs(p, -0.52, 0.12, -0.05, 0.0, 0.34, 0.28, -0.35, 0.30);
    arms_walk(p, 1.0);
}

fn walk_l_loading(p: &mut Pose) {
    legs(p, -0.35, 0.32, -0.10, 0.02, 0.45, 0.62, -0.45, 0.25);
    arms_walk(p, 0.6);
}

fn walk_l_mid(p: &mut Pose) {
    legs(p, -0.05, 0.06, -0.03, 0.0, -0.55, 1.15, -0.15, 0.05);
    arms_walk(p, 0.0);
}

fn walk_r_strike(p: &mut Pose) {
    legs(p, 0.34, 0.28, -0.35, 0.30, -0.52, 0.12, -0.05, 0.0);
    arms_walk(p, -1.0);
}

fn walk_r_loading(p: &mut Pose) {
    legs(p, 0.45, 0.62, -0.45, 0.25, -0.35, 0.32, -0.10, 0.02);
    arms_walk(p, -0.6);
}

fn walk_r_mid(p: &mut Pose) {
    legs(p, -0.55, 1.15, -0.15, 0.05, -0.05, 0.06, -0.03, 0.0);
    arms_walk(p, 0.0);
}

fn arms_walk(p: &mut Pose, swing: f32) {
    arm_r(p, -0.30 * swing - 0.05, 0.0, -0.14, -0.45);
    arm_l(p, 0.30 * swing - 0.10, 0.0, 0.14, -0.30);
    torso(p, 0.07, 0.02, 0.06 * swing, 0.0);
}

static RUN_FWD: &[Key] = &[
    (0.00, run_l_strike),
    (0.17, run_l_load),
    (0.34, run_l_mid),
    (0.50, run_r_strike),
    (0.67, run_r_load),
    (0.84, run_r_mid),
];

fn run_base(p: &mut Pose) {
    torso(p, 0.30, 0.10, 0.0, 0.0);
}

fn run_l_strike(p: &mut Pose) {
    run_base(p);
    p.height(0.0);
    legs(p, -0.75, 0.35, -0.15, 0.15, 0.55, 0.85, -0.55, 0.35);
    arm_r(p, -0.55, 0.0, -0.20, -1.35);
    arm_l(p, 0.50, 0.0, 0.20, -1.30);
}

fn run_l_load(p: &mut Pose) {
    run_base(p);
    p.height(-0.03);
    legs(p, -0.45, 0.60, -0.25, 0.10, 0.80, 1.30, -0.65, 0.25);
    arm_r(p, -0.30, 0.0, -0.20, -1.30);
    arm_l(p, 0.25, 0.0, 0.20, -1.35);
}

fn run_l_mid(p: &mut Pose) {
    run_base(p);
    p.height(0.05);
    legs(p, -0.10, 0.10, -0.10, 0.15, -0.85, 1.75, -0.30, 0.10);
    arm_r(p, 0.10, 0.0, -0.20, -1.40);
    arm_l(p, -0.15, 0.0, 0.20, -1.30);
}

fn run_r_strike(p: &mut Pose) {
    run_base(p);
    p.height(0.0);
    legs(p, 0.55, 0.85, -0.55, 0.35, -0.75, 0.35, -0.15, 0.15);
    arm_r(p, -0.55, 0.0, -0.20, -1.35);
    arm_l(p, 0.50, 0.0, 0.20, -1.30);
}

fn run_r_load(p: &mut Pose) {
    run_base(p);
    p.height(-0.03);
    legs(p, 0.80, 1.30, -0.65, 0.25, -0.45, 0.60, -0.25, 0.10);
    arm_r(p, -0.30, 0.0, -0.20, -1.30);
    arm_l(p, 0.25, 0.0, 0.20, -1.35);
}

fn run_r_mid(p: &mut Pose) {
    run_base(p);
    p.height(0.05);
    legs(p, -0.85, 1.75, -0.30, 0.10, -0.10, 0.10, -0.10, 0.15);
    arm_r(p, 0.10, 0.0, -0.20, -1.40);
    arm_l(p, -0.15, 0.0, 0.20, -1.30);
}

static SPRINT_KEYS: &[Key] = &[
    (0.00, sprint_l_strike),
    (0.17, sprint_l_load),
    (0.34, sprint_l_mid),
    (0.50, sprint_r_strike),
    (0.67, sprint_r_load),
    (0.84, sprint_r_mid),
];

fn sprint_base(p: &mut Pose) {
    torso(p, 0.52, 0.18, 0.0, 0.0);
    p.set("Neck", -0.30, 0.0, 0.0);
}

fn sprint_l_strike(p: &mut Pose) {
    sprint_base(p);
    legs(p, -0.85, 0.45, -0.15, 0.18, 0.65, 1.05, -0.65, 0.40);
    arm_r(p, -0.65, 0.0, -0.15, -1.60);
    arm_l(p, 0.60, 0.0, 0.15, -1.55);
}

fn sprint_l_load(p: &mut Pose) {
    sprint_base(p);
    p.height(-0.02);
    legs(p, -0.50, 0.70, -0.30, 0.12, 0.95, 1.55, -0.70, 0.30);
    arm_r(p, -0.35, 0.0, -0.15, -1.55);
    arm_l(p, 0.30, 0.0, 0.15, -1.60);
}

fn sprint_l_mid(p: &mut Pose) {
    sprint_base(p);
    p.height(0.06);
    legs(p, -0.12, 0.12, -0.10, 0.18, -1.00, 2.05, -0.35, 0.12);
    arm_r(p, 0.15, 0.0, -0.15, -1.65);
    arm_l(p, -0.20, 0.0, 0.15, -1.55);
}

fn sprint_r_strike(p: &mut Pose) {
    sprint_base(p);
    legs(p, 0.65, 1.05, -0.65, 0.40, -0.85, 0.45, -0.15, 0.18);
    arm_r(p, -0.65, 0.0, -0.15, -1.60);
    arm_l(p, 0.60, 0.0, 0.15, -1.55);
}

fn sprint_r_load(p: &mut Pose) {
    sprint_base(p);
    p.height(-0.02);
    legs(p, 0.95, 1.55, -0.70, 0.30, -0.50, 0.70, -0.30, 0.12);
    arm_r(p, -0.35, 0.0, -0.15, -1.55);
    arm_l(p, 0.30, 0.0, 0.15, -1.60);
}

fn sprint_r_mid(p: &mut Pose) {
    sprint_base(p);
    p.height(0.06);
    legs(p, -1.00, 2.05, -0.35, 0.12, -0.12, 0.12, -0.10, 0.18);
    arm_r(p, 0.15, 0.0, -0.15, -1.65);
    arm_l(p, -0.20, 0.0, 0.15, -1.55);
}

static SHUFFLE: &[Key] = &[(0.0, shuffle_a), (0.5, shuffle_b)];

fn shuffle_a(p: &mut Pose) {
    neutral(p);
    p.height(-0.04);
    torso(p, 0.10, 0.04, 0.0, -0.12);
    legs(p, -0.16, 0.22, -0.10, 0.0, 0.10, 0.18, -0.10, 0.0);
    p.set("L_Thigh", -0.16, 0.0, 0.30);
    p.set("R_Thigh", 0.10, 0.0, -0.10);
    arm_r(p, -0.35, 0.0, -0.30, -0.60);
    arm_l(p, -0.30, 0.0, 0.30, -0.55);
}

fn shuffle_b(p: &mut Pose) {
    neutral(p);
    p.height(-0.04);
    torso(p, 0.10, 0.04, 0.0, 0.12);
    legs(p, 0.10, 0.18, -0.10, 0.0, -0.16, 0.22, -0.10, 0.0);
    p.set("L_Thigh", 0.10, 0.0, 0.10);
    p.set("R_Thigh", -0.16, 0.0, -0.30);
    arm_r(p, -0.35, 0.0, -0.30, -0.60);
    arm_l(p, -0.30, 0.0, 0.30, -0.55);
}

static CROUCH_WALK_KEYS: &[Key] = &[(0.0, cw_a), (0.25, cw_b), (0.5, cw_c), (0.75, cw_d)];

fn cw_a(p: &mut Pose) {
    crouch_base(p);
    legs(p, -1.35, 1.95, -0.68, 0.10, -0.85, 1.45, -0.60, 0.10);
}

fn cw_b(p: &mut Pose) {
    crouch_base(p);
    legs(p, -1.15, 1.85, -0.66, 0.10, -1.10, 1.80, -0.66, 0.12);
}

fn cw_c(p: &mut Pose) {
    crouch_base(p);
    legs(p, -0.85, 1.45, -0.60, 0.10, -1.35, 1.95, -0.68, 0.10);
}

fn cw_d(p: &mut Pose) {
    crouch_base(p);
    legs(p, -1.10, 1.80, -0.66, 0.12, -1.15, 1.85, -0.66, 0.10);
}

static CROUCH_RUN_KEYS: &[Key] = &[(0.0, cr_a), (0.25, cr_b), (0.5, cr_c), (0.75, cr_d)];

fn cr_a(p: &mut Pose) {
    crouch_base(p);
    p.height(-0.42);
    torso(p, 0.55, 0.22, 0.0, 0.0);
    legs(p, -1.55, 2.10, -0.70, 0.12, -0.55, 0.95, -0.50, 0.15);
}

fn cr_b(p: &mut Pose) {
    crouch_base(p);
    p.height(-0.40);
    torso(p, 0.55, 0.22, 0.0, 0.0);
    legs(p, -1.20, 1.95, -0.68, 0.12, -1.25, 2.05, -0.66, 0.12);
}

fn cr_c(p: &mut Pose) {
    crouch_base(p);
    p.height(-0.42);
    torso(p, 0.55, 0.22, 0.0, 0.0);
    legs(p, -0.55, 0.95, -0.50, 0.15, -1.55, 2.10, -0.70, 0.12);
}

fn cr_d(p: &mut Pose) {
    crouch_base(p);
    p.height(-0.40);
    torso(p, 0.55, 0.22, 0.0, 0.0);
    legs(p, -1.25, 2.05, -0.66, 0.12, -1.20, 1.95, -0.68, 0.12);
}

static CROUCH_ENTER_KEYS: &[Key] = &[(0.0, neutral_hold), (0.35, half_crouch), (1.0, crouch_base)];
static CROUCH_EXIT_KEYS: &[Key] = &[(0.0, crouch_base), (0.65, half_crouch), (1.0, neutral_hold)];

fn neutral_hold(p: &mut Pose) {
    carry_r(p);
}

fn half_crouch(p: &mut Pose) {
    crouch_base(p);
    p.height(-0.16);
    legs(p, -0.60, 0.95, -0.35, 0.05, -0.55, 0.90, -0.33, 0.05);
    torso(p, 0.22, 0.10, 0.0, 0.0);
}

// Air loops ------------------------------------------------------------------

static FALL_LOOP: &[Key] = &[(0.0, fall_a), (0.5, fall_b)];

// Action tables --------------------------------------------------------------

static SLASH: &[Key] = &[
    (0.000, settle),
    (0.088, wind_slash),
    (0.143, strike_slash),
    (0.220, follow_slash),
    (0.375, settle),
    (1.000, settle),
];

static SLASH_LEFT: &[Key] = &[
    (0.000, settle),
    (0.088, wind_slash_left),
    (0.143, strike_slash_left),
    (0.220, follow_slash_left),
    (0.375, settle),
    (1.000, settle),
];

static HEAVY: &[Key] = &[
    (0.000, settle),
    (0.110, wind_over),
    (0.244, strike_over),
    (0.360, follow_slash),
    (0.578, settle),
    (1.000, settle),
];

static CHARGED: &[Key] = &[
    (0.000, settle),
    (0.110, wind_over),
    (0.330, wind_over),
    (0.400, mid_over),
    (0.440, strike_over),
    (0.489, follow_slash),
    (0.667, settle),
    (1.000, settle),
];

static RUN_SLASH: &[Key] = &[
    (0.000, settle),
    (0.250, lunge_wind),
    (0.520, lunge_strike),
    (0.700, follow_slash),
    (1.000, settle),
];

static RUN_HEAVY: &[Key] = &[
    (0.000, settle),
    (0.220, lunge_wind),
    (0.480, lunge_strike),
    (0.660, follow_slash),
    (1.000, settle),
];

static LOW_SLASH: &[Key] = &[
    (0.000, crouch_base),
    (0.280, low_wind),
    (0.470, low_strike),
    (0.700, half_crouch),
    (1.000, half_crouch),
];

static RIPOSTE: &[Key] = &[
    (0.000, braced),
    (0.280, thrust_wind),
    (0.430, thrust_out),
    (0.700, settle),
    (1.000, settle),
];

static SLAM_HEAVY: &[Key] = &[
    (0.000, wind_over),
    (0.180, mid_over),
    (0.300, strike_over),
    (0.480, land_hard),
    (0.780, settle),
    (1.000, settle),
];

static SLAM_LIGHT: &[Key] = &[
    (0.000, wind_slash),
    (0.250, strike_slash),
    (0.430, land_soft),
    (0.750, settle),
    (1.000, settle),
];

static PAIRED: &[Key] = &[
    (0.000, settle_two),
    (0.090, pair_wind),
    (0.150, pair_strike),
    (0.240, pair_follow),
    (0.390, settle_two),
    (1.000, settle_two),
];

static PAIRED_LUNGE: &[Key] = &[
    (0.000, settle_two),
    (0.230, pair_wind),
    (0.480, pair_strike),
    (0.680, pair_follow),
    (1.000, settle_two),
];

static AIR_LIGHT: &[Key] = &[
    (0.000, tuck_air),
    (0.180, wind_slash),
    (0.330, strike_slash),
    (0.500, legs_fwd_air),
    (1.000, legs_fwd_air),
];

static AIR_HEAVY: &[Key] = &[
    (0.000, tuck_air),
    (0.280, wind_over),
    (0.550, strike_over),
    (1.000, legs_fwd_air),
];

static AIR_PAIRED: &[Key] = &[
    (0.000, tuck_air),
    (0.220, pair_wind),
    (0.480, pair_strike),
    (1.000, legs_fwd_air),
];

static ROLL_FWD: &[Key] = &[
    (0.00, settle),
    (0.18, tuck_ball),
    (0.42, tuck_ball_tight),
    (0.66, tuck_ball),
    (0.86, half_crouch),
    (1.00, settle),
];

static ROLL_BACK: &[Key] = &[
    (0.00, settle),
    (0.20, back_lean),
    (0.44, tuck_ball_tight),
    (0.70, tuck_ball),
    (0.88, half_crouch),
    (1.00, settle),
];

static ROLL_CROUCH: &[Key] = &[
    (0.00, crouch_base),
    (0.20, tuck_ball),
    (0.45, tuck_ball_tight),
    (0.70, tuck_ball),
    (0.90, crouch_base),
    (1.00, crouch_base),
];

static BACKSTEP_KEYS: &[Key] = &[
    (0.00, settle),
    (0.25, hop),
    (0.60, back_lean),
    (1.00, settle),
];

static SPRINT_STOP_KEYS: &[Key] = &[
    (0.00, plant),
    (0.45, settle),
    (1.00, settle),
];

static JUMP_KEYS: &[Key] = &[
    (0.00, settle),
    (0.14, half_crouch),
    (0.30, leap),
    (0.60, tuck_air),
    (1.00, tuck_air),
];

static LAND_SOFT_KEYS: &[Key] = &[
    (0.00, land_soft),
    (0.45, settle),
    (1.00, settle),
];

static LAND_RUN_KEYS: &[Key] = &[
    (0.00, land_run),
    (0.50, settle),
    (1.00, settle),
];

static LAND_HARD_KEYS: &[Key] = &[
    (0.00, land_hard),
    (0.30, half_crouch),
    (0.65, settle),
    (1.00, settle),
];

static LAND_FALL_KEYS: &[Key] = &[
    (0.00, land_fall),
    (0.30, half_crouch),
    (0.70, settle),
    (1.00, settle),
];

static GUARD_HIT_KEYS: &[Key] = &[
    (0.00, braced),
    (0.35, braced),
    (1.00, guard_shield),
];

static GUARD_BREAK_KEYS: &[Key] = &[
    (0.00, braced),
    (0.18, flung),
    (0.50, stagger),
    (1.00, settle),
];

static FLINCH_KEYS: &[Key] = &[
    (0.00, settle),
    (0.16, flinch),
    (0.45, settle),
    (1.00, settle),
];

static STAGGER_KEYS: &[Key] = &[
    (0.00, settle),
    (0.12, stagger),
    (0.40, back_lean),
    (0.70, settle),
    (1.00, settle),
];

static LARGE_STAGGER_KEYS: &[Key] = &[
    (0.00, settle),
    (0.10, stagger_large),
    (0.38, to_knee),
    (0.65, stagger),
    (1.00, settle),
];

static KNOCKDOWN_KEYS: &[Key] = &[
    (0.00, settle),
    (0.09, stagger_large),
    (0.28, crumple),
    (0.50, dead_flat),
    (0.78, dead_flat),
    (1.00, to_knee),
];

static DEATH_KEYS: &[Key] = &[
    (0.00, settle),
    (0.10, flinch),
    (0.25, crumple),
    (0.55, dead_flat),
    (1.00, dead_flat),
];

static SWAP_2H: &[Key] = &[
    (0.00, settle),
    (0.40, swap_belt),
    (1.00, swap_high),
];

static SWAP_2H_END: &[Key] = &[
    (0.00, swap_high),
    (0.45, settle_two),
    (1.00, settle_two),
];

static SWAP_1H: &[Key] = &[
    (0.00, swap_high),
    (0.45, swap_low),
    (1.00, settle),
];

static SWAP_1H_END: &[Key] = &[
    (0.00, settle),
    (1.00, settle),
];

static SWAP_WEAPON: &[Key] = &[
    (0.00, settle),
    (0.45, swap_belt),
    (1.00, swap_belt_up),
];

static SWAP_WEAPON_END: &[Key] = &[
    (0.00, swap_belt_up),
    (0.40, settle),
    (1.00, settle),
];

// --- Clip dispatch ----------------------------------------------------------

/// What a source-named clip does, and how many frames it bakes to.
#[derive(Clone, Copy, PartialEq)]
enum Act {
    Slash,
    SlashLeft,
    Overhead,
    Charged,
    RunSlash,
    RunHeavy,
    LowSlash,
    Riposte,
    SlamHeavy,
    SlamLight,
    Paired,
    PairedLunge,
    AirLight,
    AirHeavy,
    AirPaired,
    Roll { back: bool, side: f32, crouch: bool },
    Backstep,
    SprintStop,
    JumpStand,
    JumpWalk,
    JumpRun,
    JumpSprint,
    LandSoft,
    LandRun,
    LandSprint,
    LandHard,
    LandFall,
    GuardHit,
    GuardBreak,
    Flinch,
    Stagger,
    LargeStagger,
    Knockdown,
    Swap2hStart,
    Swap2hEnd,
    Swap1hStart,
    Swap1hEnd,
    SwapWeaponStart,
    SwapWeaponEnd,
}

impl Act {
    fn frames(self) -> usize {
        match self {
            Act::Slash | Act::SlashLeft | Act::Paired | Act::PairedLunge => 91,
            Act::Overhead => 91,
            Act::Charged => 91,
            Act::RunSlash | Act::RunHeavy => 41,
            Act::LowSlash => 46,
            Act::Riposte => 34,
            Act::SlamHeavy => 46,
            Act::SlamLight => 40,
            Act::AirLight | Act::AirHeavy | Act::AirPaired => 64,
            Act::Roll { crouch, .. } => {
                if crouch {
                    28
                } else {
                    31
                }
            }
            Act::Backstep => 19,
            Act::SprintStop => 13,
            Act::JumpStand | Act::JumpWalk => 35,
            Act::JumpRun | Act::JumpSprint => 25,
            Act::LandSoft => 15,
            Act::LandRun => 17,
            Act::LandSprint => 23,
            Act::LandHard => 31,
            Act::LandFall => 22,
            Act::GuardHit => 15,
            Act::GuardBreak => 43,
            Act::Flinch => 35,
            Act::Stagger => 58,
            Act::LargeStagger => 71,
            Act::Knockdown => 106,
            Act::Swap2hStart | Act::SwapWeaponStart => 9,
            Act::Swap2hEnd => 20,
            Act::Swap1hStart => 8,
            Act::Swap1hEnd => 14,
            Act::SwapWeaponEnd => 16,
        }
    }

    fn blend(self) -> f32 {
        match self {
            Act::Slash | Act::SlashLeft | Act::Paired | Act::PairedLunge => 4.0,
            Act::Overhead | Act::Charged => 5.0,
            Act::RunSlash | Act::RunHeavy => 4.0,
            Act::LowSlash | Act::Riposte => 3.0,
            Act::SlamHeavy | Act::SlamLight => 2.0,
            Act::AirLight | Act::AirHeavy | Act::AirPaired => 3.0,
            Act::Roll { .. } => 3.0,
            Act::Backstep => 3.0,
            Act::SprintStop => 4.0,
            Act::JumpStand | Act::JumpWalk | Act::JumpRun | Act::JumpSprint => 3.0,
            Act::LandSoft | Act::LandRun | Act::LandSprint => 2.0,
            Act::LandHard | Act::LandFall => 3.0,
            Act::GuardHit => 2.0,
            Act::GuardBreak => 3.0,
            Act::Flinch => 3.0,
            Act::Stagger | Act::LargeStagger | Act::Knockdown => 3.0,
            Act::Swap2hStart | Act::Swap2hEnd | Act::Swap1hStart | Act::Swap1hEnd | Act::SwapWeaponStart | Act::SwapWeaponEnd => 4.0,
        }
    }

    fn play(self, p: &mut Pose, t: f32) {
        let keys: &[Key] = match self {
            Act::Slash => SLASH,
            Act::SlashLeft => SLASH_LEFT,
            Act::Overhead => HEAVY,
            Act::Charged => CHARGED,
            Act::RunSlash => RUN_SLASH,
            Act::RunHeavy => RUN_HEAVY,
            Act::LowSlash => LOW_SLASH,
            Act::Riposte => RIPOSTE,
            Act::SlamHeavy => SLAM_HEAVY,
            Act::SlamLight => SLAM_LIGHT,
            Act::Paired => PAIRED,
            Act::PairedLunge => PAIRED_LUNGE,
            Act::AirLight => AIR_LIGHT,
            Act::AirHeavy => AIR_HEAVY,
            Act::AirPaired => AIR_PAIRED,
            Act::Roll { back: true, .. } => ROLL_BACK,
            Act::Roll { crouch: true, .. } => ROLL_CROUCH,
            Act::Roll { .. } => ROLL_FWD,
            Act::Backstep => BACKSTEP_KEYS,
            Act::SprintStop => SPRINT_STOP_KEYS,
            Act::JumpStand | Act::JumpWalk | Act::JumpRun | Act::JumpSprint => JUMP_KEYS,
            Act::LandSoft => LAND_SOFT_KEYS,
            Act::LandRun | Act::LandSprint => LAND_RUN_KEYS,
            Act::LandHard => LAND_HARD_KEYS,
            Act::LandFall => LAND_FALL_KEYS,
            Act::GuardHit => GUARD_HIT_KEYS,
            Act::GuardBreak => GUARD_BREAK_KEYS,
            Act::Flinch => FLINCH_KEYS,
            Act::Stagger => STAGGER_KEYS,
            Act::LargeStagger => LARGE_STAGGER_KEYS,
            Act::Knockdown => KNOCKDOWN_KEYS,
            Act::Swap2hStart => SWAP_2H,
            Act::Swap2hEnd => SWAP_2H_END,
            Act::Swap1hStart => SWAP_1H,
            Act::Swap1hEnd => SWAP_1H_END,
            Act::SwapWeaponStart => SWAP_WEAPON,
            Act::SwapWeaponEnd => SWAP_WEAPON_END,
        };
        keyframe(p, keys, t, false);
        // A roll turns the whole body over; the axis picks which way it goes.
        if let Act::Roll { back, side, .. } = self {
            let spin = t * std::f32::consts::TAU * if back { -1.0 } else { 1.0 };
            let (x, z) = if side != 0.0 { (0.0, side * spin) } else { (spin, 0.0) };
            let seat = Quat::from_rotation_x(x) * Quat::from_rotation_z(z);
            let i = p.index["Pelvis"];
            p.local[i] = seat * p.local[i];
            p.solve();
        }
    }
}

/// What each source-named clip does. Names come straight from `content`.
fn act_for(name: &str) -> Option<Act> {
    let act = match name {
        "light attack" => Act::Slash,
        "left-hand attack" => Act::SlashLeft,
        "heavy attack" => Act::Overhead,
        "charged heavy" => Act::Charged,
        "running attack" => Act::RunSlash,
        "running heavy" => Act::RunHeavy,
        "rolling attack" | "backstep attack" | "crouch attack" => Act::LowSlash,
        "guard counter" => Act::Riposte,
        "jump attack, land" => Act::SlamHeavy,
        "jump attack, land (light)" => Act::SlamLight,
        "paired attack" => Act::Paired,
        "paired, running" | "paired, rolling" | "paired, backstep" | "paired, jump land" | "paired, jump land (short)" => Act::PairedLunge,
        "jump_attack_light" => Act::AirLight,
        "jump_attack_heavy" => Act::AirHeavy,
        "jump_attack_paired" => Act::AirPaired,
        "light roll, forward" | "medium roll, forward" | "heavy roll, forward" => Act::Roll { back: false, side: 0.0, crouch: false },
        "roll, back" => Act::Roll { back: true, side: 0.0, crouch: false },
        "roll, left" => Act::Roll { back: false, side: 1.0, crouch: false },
        "roll, right" => Act::Roll { back: false, side: -1.0, crouch: false },
        "crouch roll" => Act::Roll { back: false, side: 0.0, crouch: true },
        "backstep" => Act::Backstep,
        "sprint stop" => Act::SprintStop,
        "jump" => Act::JumpStand,
        "jump, walk" => Act::JumpWalk,
        "jump, run" => Act::JumpRun,
        "jump, sprint" => Act::JumpSprint,
        "land, light" | "land, strafe walk" => Act::LandSoft,
        "land, run" | "land, strafe" => Act::LandRun,
        "land, sprint" => Act::LandSprint,
        "land, heavy" => Act::LandHard,
        "land, fall" => Act::LandFall,
        "guard hit" => Act::GuardHit,
        "guard break" => Act::GuardBreak,
        "flinch" => Act::Flinch,
        "stagger" => Act::Stagger,
        "large stagger" => Act::LargeStagger,
        "knockdown" => Act::Knockdown,
        "grip_2h_right_start" | "grip_2h_left_start" => Act::Swap2hStart,
        "grip_2h_right_end" | "grip_2h_left_end" => Act::Swap2hEnd,
        "grip_1h_from_right_start" | "grip_1h_from_left_start" => Act::Swap1hStart,
        "grip_1h_from_right_end" | "grip_1h_from_left_end" => Act::Swap1hEnd,
        "weapon_swap_start" | "offhand_swap_start" => Act::SwapWeaponStart,
        "weapon_swap_end" | "offhand_swap_end" => Act::SwapWeaponEnd,
        _ => return None,
    };
    Some(act)
}

/// Every clip name the simulation can wear as an action, gathered from the
/// content itself so nothing new can slip past unbaked.
fn all_sources() -> Vec<String> {
    let kinds = [
        AttackKind::Light1, AttackKind::Light2, AttackKind::Light3, AttackKind::Light4, AttackKind::Light5, AttackKind::Light6,
        AttackKind::RunLight, AttackKind::RunHeavy, AttackKind::RollAttack, AttackKind::CrouchAttack, AttackKind::BackstepAttack,
        AttackKind::Heavy1Charge, AttackKind::Heavy1, AttackKind::Heavy2Charge, AttackKind::Heavy2, AttackKind::GuardCounter,
        AttackKind::JumpLightLand, AttackKind::JumpLightLandShort, AttackKind::JumpHeavyLand, AttackKind::JumpHeavyLandShort,
        AttackKind::LeftLight1, AttackKind::LeftLight2, AttackKind::LeftLight3, AttackKind::LeftLight4, AttackKind::LeftLight5, AttackKind::LeftLight6,
        AttackKind::PairedLight1, AttackKind::PairedLight2, AttackKind::PairedLight3, AttackKind::PairedLight4, AttackKind::PairedLight5, AttackKind::PairedLight6,
        AttackKind::PairedRun, AttackKind::PairedRoll, AttackKind::PairedBackstep, AttackKind::PairedJumpLand, AttackKind::PairedJumpLandShort,
    ];
    let mut seen = HashSet::new();
    let mut names = Vec::new();
    let mut add = |name: &'static str| {
        if seen.insert(name.to_string()) {
            names.push(name.to_string());
        }
    };
    for weapon in 0..WEAPONS.len() as u8 {
        for two_hand in [false, true] {
            let moveset = Moveset { weapon, two_hand };
            for kind in kinds {
                if let Some(def) = moveset.attack(kind) {
                    add(def.source);
                }
            }
            for heavy in [false, true] {
                if let Some(def) = moveset.air(heavy) {
                    add(def.source);
                }
            }
            if let Some(def) = moveset.air_paired() {
                add(def.source);
            }
        }
    }
    let mut base = Vec::new();
    for load in [Load::Light, Load::Medium, Load::Heavy] {
        for dir in [Dir::Front, Dir::Back, Dir::Left, Dir::Right] {
            base.push(ActionId::Roll(load, dir));
            base.push(ActionId::CrouchRoll(load, dir));
        }
    }
    base.push(ActionId::Backstep);
    base.push(ActionId::SprintStop);
    for kind in [
        JumpKind::Stand, JumpKind::Walk, JumpKind::WalkBack, JumpKind::WalkLeft, JumpKind::WalkRight,
        JumpKind::Run, JumpKind::RunBack, JumpKind::RunLeft, JumpKind::RunRight, JumpKind::Sprint,
    ] {
        base.push(ActionId::Jump(kind));
    }
    base.push(ActionId::LandLight);
    base.push(ActionId::LandRun);
    base.push(ActionId::LandSprint);
    base.push(ActionId::LandHeavy);
    base.push(ActionId::LandFall);
    base.push(ActionId::GuardHit);
    base.push(ActionId::GuardBreak);
    for dir in [Dir::Front, Dir::Back, Dir::Left, Dir::Right] {
        base.push(ActionId::LandStrafeWalk(dir));
        base.push(ActionId::LandStrafe(dir));
        for level in [HurtLevel::Small, HurtLevel::Middle, HurtLevel::Large, HurtLevel::Knockdown] {
            base.push(ActionId::Hurt(level, dir));
        }
    }
    for id in base {
        add(id.def().source);
    }
    for kind in [
        SwapKind::ToTwoHandRight, SwapKind::ToTwoHandLeft, SwapKind::ToOneHandFromRight,
        SwapKind::ToOneHandFromLeft, SwapKind::NextWeapon, SwapKind::NextLeft,
    ] {
        let def = kind.def();
        add(def.start);
        add(def.end);
    }
    names
}

fn clip_name(category: u8, id: u32) -> String {
    format!("a{category:03}_{id:06}")
}

// --- Stance and gait draws --------------------------------------------------

/// Both hands stay on the weapon; the legs do the moving (two-handed loops).
fn two_hand_arms(p: &mut Pose) {
    arm_r(p, -0.50, -0.10, -0.30, -0.75);
    arm_l(p, -0.55, 0.45, -0.55, -1.05);
}

enum Gait {
    Walk,
    Run,
    Sprint,
}

fn gait(p: &mut Pose, f: f32, gait: Gait, dir: u32, cat: u8) {
    let keys = match gait {
        Gait::Walk => WALK_FWD,
        Gait::Run => RUN_FWD,
        Gait::Sprint => SPRINT_KEYS,
    };
    let t = f.fract();
    let t = match dir {
        // Backwards: the same strides played in reverse.
        1 => 1.0 - t,
        // Sideways: which foot leads is the only difference.
        3 => (t + 0.5).fract(),
        _ => t,
    };
    if dir >= 2 {
        keyframe(p, SHUFFLE, t, true);
        p.set("Spine1", 0.10, 0.0, if dir == 2 { 0.14 } else { -0.14 });
        p.set("Spine2", 0.04, 0.0, if dir == 2 { 0.08 } else { -0.08 });
    } else {
        keyframe(p, keys, t, true);
        if dir == 1 {
            torso(p, -0.06, -0.02, 0.0, 0.0);
        }
    }
    if cat != 0 {
        two_hand_arms(p);
        p.solve();
    }
}

fn run_stop(p: &mut Pose, f: f32, dir: u32, cat: u8) {
    keyframe(p, SPRINT_STOP_KEYS, f, false);
    if dir >= 2 {
        p.set("Spine1", 0.30, 0.0, if dir == 2 { 0.18 } else { -0.18 });
        p.solve();
    }
    if cat != 0 {
        two_hand_arms(p);
        p.solve();
    }
}

fn crouch_idle(p: &mut Pose, f: f32) {
    let t = f;
    crouch_base(p);
    let breathe = (t * std::f32::consts::TAU).sin();
    p.height(-0.34 - 0.008 * breathe);
    torso(p, 0.42 + 0.02 * breathe, 0.18, 0.03 * (t * std::f32::consts::TAU * 0.5).sin(), 0.0);
    grip(p);
    p.solve();
}

fn crouch_gait(p: &mut Pose, f: f32, running: bool, dir: u32) {
    let keys = if running { CROUCH_RUN_KEYS } else { CROUCH_WALK_KEYS };
    let t = f.fract();
    let t = if dir == 1 { 1.0 - t } else { t };
    keyframe(p, keys, t, true);
}

fn crouch_stop(p: &mut Pose, f: f32) {
    keyframe(p, CROUCH_ENTER_KEYS, f, false);
}

static FALL_START_KEYS: &[Key] = &[(0.0, fall_start), (0.55, fall_a), (1.0, fall_a)];

fn fall_start_draw(p: &mut Pose, f: f32) {
    keyframe(p, FALL_START_KEYS, f, false);
}

// --- Generation -------------------------------------------------------------

/// Pose the skeleton at `t` (0..1 either way) and store the result.
fn bake(clips: &mut HashMap<String, Clip>, pose: &mut Pose, oriented: &[String], name: String, blend: f32, frames: usize, draw: &dyn Fn(&mut Pose, f32)) {
    let stride = pose.bones.len() * 3 + oriented.len() * 9;
    let mut data = Vec::with_capacity(frames * stride);
    let cycle = (frames - 1) as f32;
    for f in 0..frames {
        pose.reset();
        draw(pose, f as f32 / cycle);
        for pos in &pose.pos {
            out(pos, &mut data);
        }
        for joint in oriented {
            let basis = pose.basis[pose.index[joint.as_str()]];
            for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                out(&(basis * axis), &mut data);
            }
        }
    }
    clips.insert(name, Clip { blend, frames, data });
}

fn out(v: &Vec3, data: &mut Vec<f32>) {
    data.extend_from_slice(&v.to_array());
}

impl Clips {
    /// Every clip the rig can ask for, generated from the skeleton and the
    /// pose library. Nothing is read from disk.
    pub fn procedural() -> Self {
        let (names, bones, oriented) = skeleton();
        let mut pose = Pose::new(&bones, &names);
        let mut clips = HashMap::new();

        for cat in 0..=7u8 {
            bake(&mut clips, &mut pose, &oriented, clip_name(cat, IDLE), 6.0, 61, &|p, f| idle(p, f, cat));
            bake(&mut clips, &mut pose, &oriented, clip_name(cat, GUARD), 5.0, 31, &|p, f| guard(p, f, cat));
            for dir in 0..4 {
                bake(&mut clips, &mut pose, &oriented, clip_name(cat, WALK + dir), 6.0, 31, &|p, f| gait(p, f, Gait::Walk, dir, cat));
                bake(&mut clips, &mut pose, &oriented, clip_name(cat, RUN + dir), 6.0, 25, &|p, f| gait(p, f, Gait::Run, dir, cat));
                bake(&mut clips, &mut pose, &oriented, clip_name(cat, RUN_STOP + dir), 4.0, 16, &|p, f| run_stop(p, f, dir, cat));
            }
            bake(&mut clips, &mut pose, &oriented, clip_name(cat, SPRINT), 6.0, 23, &|p, f| gait(p, f, Gait::Sprint, 0, cat));
        }
        // Crouching lives on the base category.
        bake(&mut clips, &mut pose, &oriented, clip_name(0, CROUCH_IDLE), 6.0, 61, &|p, f| crouch_idle(p, f));
        for dir in 0..4 {
            bake(&mut clips, &mut pose, &oriented, clip_name(0, CROUCH_WALK + dir), 6.0, 31, &|p, f| crouch_gait(p, f, false, dir));
            bake(&mut clips, &mut pose, &oriented, clip_name(0, CROUCH_RUN + dir), 6.0, 25, &|p, f| crouch_gait(p, f, true, dir));
        }
        bake(&mut clips, &mut pose, &oriented, clip_name(0, CROUCH_RUN_STOP), 4.0, 16, &|p, f| crouch_stop(p, f));
        bake(&mut clips, &mut pose, &oriented, clip_name(0, CROUCH_ENTER), 5.0, 17, &|p, f| keyframe(p, CROUCH_ENTER_KEYS, f, false));
        bake(&mut clips, &mut pose, &oriented, clip_name(0, CROUCH_EXIT), 5.0, 17, &|p, f| keyframe(p, CROUCH_EXIT_KEYS, f, false));
        bake(&mut clips, &mut pose, &oriented, clip_name(0, AIR_LOOP), 6.0, 31, &|p, f| keyframe(p, FALL_LOOP, f, true));
        bake(&mut clips, &mut pose, &oriented, clip_name(0, FALL_START), 3.0, 25, &|p, f| fall_start_draw(p, f));
        bake(&mut clips, &mut pose, &oriented, clip_name(0, DEATH), 8.0, 121, &|p, f| keyframe(p, DEATH_KEYS, f, false));
        // Every action the simulation can name.
        for name in all_sources() {
            let Some(act) = act_for(&name) else {
                continue;
            };
            let frames = act.frames();
            bake(&mut clips, &mut pose, &oriented, name, act.blend(), frames, &|p, f| act.play(p, f));
        }
        let stride = bones.len() * 3 + oriented.len() * 9;
        Self { joints: names, oriented, stride, clips }
    }

    pub fn get(&self, name: &str) -> Option<&Clip> {
        self.clips.get(name)
    }

    /// Index of a joint's position within a pose.
    pub fn joint(&self, name: &str) -> usize {
        self.joints.iter().position(|j| j == name).unwrap_or_else(|| panic!("no joint {name}")) * 3
    }

    /// Index of an oriented joint's X axis within a pose; Y and Z follow.
    pub fn axes(&self, name: &str) -> usize {
        let i = self.oriented.iter().position(|j| j == name).unwrap_or_else(|| panic!("no axes for {name}"));
        self.joints.len() * 3 + i * 9
    }

    /// Writes the pose at `frame` into `out`, interpolating between samples.
    pub fn sample(&self, clip: &Clip, frame: f32, looped: bool, out: &mut Vec<f32>) {
        let last = (clip.frames - 1) as f32;
        let frame = if looped && last > 0.0 { frame.rem_euclid(last) } else { frame.clamp(0.0, last) };
        let i = (frame.floor() as usize).min(clip.frames - 1);
        let j = (i + 1).min(clip.frames - 1);
        let t = frame - i as f32;
        let (a, b) = (&clip.data[i * self.stride..][..self.stride], &clip.data[j * self.stride..][..self.stride]);
        out.clear();
        out.extend(a.iter().zip(b).map(|(a, b)| a + (b - a) * t));
    }
}

pub fn vec3(pose: &[f32], at: usize) -> Vec3 {
    Vec3::new(pose[at], pose[at + 1], pose[at + 2])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rig asks for clips by name; a missing one freezes the character,
    /// so every name it can reach has to exist.
    #[test]
    fn every_clip_the_rig_asks_for_exists() {
        let clips = Clips::procedural();
        for cat in 0..=7u8 {
            for id in [
                IDLE, GUARD, SPRINT, WALK, WALK + 1, WALK + 2, WALK + 3, RUN, RUN + 1, RUN + 2, RUN + 3,
                RUN_STOP, RUN_STOP + 1, RUN_STOP + 2, RUN_STOP + 3,
            ] {
                assert!(clips.get(&clip_name(cat, id)).is_some(), "missing a{cat:03}_{id:06}");
            }
        }
        for id in [
            CROUCH_IDLE, CROUCH_WALK, CROUCH_WALK + 1, CROUCH_WALK + 2, CROUCH_WALK + 3,
            CROUCH_RUN, CROUCH_RUN + 1, CROUCH_RUN + 2, CROUCH_RUN + 3, CROUCH_RUN_STOP,
            CROUCH_ENTER, CROUCH_EXIT, AIR_LOOP, FALL_START, DEATH,
        ] {
            assert!(clips.get(&clip_name(0, id)).is_some(), "missing base clip {id}");
        }
        for name in all_sources() {
            assert!(act_for(&name).is_some(), "no pose for action source {name:?}");
            assert!(clips.get(&name).is_some(), "no clip for action source {name:?}");
        }
    }

    /// Poses stay finite, in scale, and above ground.
    #[test]
    fn poses_are_sane() {
        let clips = Clips::procedural();
        for (name, clip) in &clips.clips {
            let mut out = Vec::new();
            for f in [0.0, clip.frames as f32 * 0.37, clip.frames as f32] {
                clips.sample(clip, f, false, &mut out);
                assert_eq!(out.len(), clips.stride);
                assert!(out.iter().all(|v| v.is_finite()), "{name} has a non-finite pose");
                let head = out[clips.joint("Head") + 1];
                let pelvis = out[clips.joint("Pelvis") + 1];
                assert!(head > 0.15 && head < 2.2, "{name} puts the head at {head}");
                assert!(pelvis > -0.05 && pelvis < 1.2, "{name} puts the pelvis at {pelvis}");
            }
        }
    }

    /// Loops must not pop where they wrap.
    #[test]
    fn loops_wrap() {
        let clips = Clips::procedural();
        for name in [
            clip_name(0, IDLE), clip_name(0, GUARD), clip_name(0, WALK), clip_name(0, RUN),
            clip_name(0, SPRINT), clip_name(0, AIR_LOOP), clip_name(0, CROUCH_IDLE), clip_name(0, CROUCH_WALK),
        ] {
            let clip = clips.get(&name).expect("loop clip");
            let (mut a, mut b) = (Vec::new(), Vec::new());
            clips.sample(clip, 0.0, true, &mut a);
            clips.sample(clip, (clip.frames - 1) as f32 - 1e-3, true, &mut b);
            let gap = a.iter().zip(&b).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
            assert!(gap < 0.05, "{name} pops on wrap: {gap}");
        }
    }
}