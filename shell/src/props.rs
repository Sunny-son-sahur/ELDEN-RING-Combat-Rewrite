//! Physics props — the sandbox half of the sandbox.
//!
//! Crates and balls you can shove around, pick up with `E`, and throw with
//! the mouse. The world gets colliders here too (ground, arena blocks) and a
//! kinematic body that follows the player so walking into a crate moves it.
//!
//! Everything is [avian3d]; the simulation itself stays engine-free.

use bevy::prelude::*;

use avian3d::prelude::{
    AngularVelocity, Collider, LinearVelocity, Position, RigidBody, Rotation,
};

use crate::Rendered;

/// Anything the player can pick up and throw.
#[derive(Component)]
pub struct Prop;

/// The player's stand-in body: kinematic, so it pushes dynamics but is
/// never pushed back.
#[derive(Component)]
pub struct PlayerBody;

/// The prop currently in the player's hands, if any.
#[derive(Resource, Default)]
pub struct Held(pub Option<Entity>);

const REACH: f32 = 2.6;

/// One-shot: colliders for the world, the player body, and the props.
///
/// Runs as a loading stage, after the rig exists.
pub fn spawn(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    sim: Res<crate::Sim>,
) {
    // The arena's visual blocks are scenery until they get colliders.
    commands.spawn((
        RigidBody::Static,
        Collider::cuboid(400.0, 1.0, 400.0),
        Transform::from_xyz(0.0, -0.5, 0.0),
    ));
    for block in &sim.0.level.blocks {
        let size = block.max - block.min;
        let centre = (block.min + block.max) / 2.0;
        commands.spawn((
            RigidBody::Static,
            Collider::cuboid(size.x, block.top, size.y),
            Transform::from_xyz(centre.x, block.top / 2.0, centre.y),
        ));
    }

    // The body itself follows `Rendered` every frame in [`follow_player`].
    let spawn = sim.0.player.pos;
    commands.spawn((
        PlayerBody,
        RigidBody::Kinematic,
        Collider::cuboid(0.6, 1.7, 0.45),
        Transform::from_xyz(spawn.x, spawn.y + 0.85, spawn.z),
        Position(Vec3::new(spawn.x, spawn.y + 0.85, spawn.z)),
        LinearVelocity::ZERO,
    ));

    // --- Props --------------------------------------------------------------
    let crate_mesh = |meshes: &mut Assets<Mesh>, w: f32, h: f32| Mesh3d(meshes.add(Cuboid::new(w, h, w)));
    let ball_mesh = |meshes: &mut Assets<Mesh>, r: f32| Mesh3d(meshes.add(Sphere::new(r)));

    let wood = materials.add(StandardMaterial {
        base_color: Color::srgb(0.45, 0.3, 0.16),
        perceptual_roughness: 0.85,
        ..default()
    });
    let dark = materials.add(StandardMaterial {
        base_color: Color::srgb(0.32, 0.22, 0.13),
        perceptual_roughness: 0.9,
        ..default()
    });
    let red = materials.add(StandardMaterial {
        base_color: Color::srgb(0.5, 0.15, 0.12),
        perceptual_roughness: 0.7,
        ..default()
    });
    let steel = materials.add(StandardMaterial {
        base_color: Color::srgb(0.5, 0.5, 0.52),
        perceptual_roughness: 0.35,
        metallic: 0.7,
        ..default()
    });
    let iron = materials.add(StandardMaterial {
        base_color: Color::srgb(0.24, 0.24, 0.27),
        perceptual_roughness: 0.4,
        metallic: 0.8,
        ..default()
    });

    // (position, size, mesh kind, material index)
    let props: &[(Vec3, f32, bool, u8)] = &[
        // A three-high stack to knock over.
        (Vec3::new(6.0, 0.35, -3.0), 0.66, true, 0),
        (Vec3::new(6.0, 1.05, -3.0), 0.66, true, 1),
        (Vec3::new(6.0, 1.75, -3.0), 0.66, true, 2),
        // Loose crates around the yard.
        (Vec3::new(5.0, 0.4, -1.0), 0.75, true, 1),
        (Vec3::new(7.2, 0.4, 1.0), 0.75, true, 0),
        (Vec3::new(4.5, 0.35, 2.5), 0.66, true, 2),
        (Vec3::new(6.5, 0.4, 4.0), 0.8, true, 0),
        (Vec3::new(-2.0, 0.4, -4.0), 0.75, true, 1),
        (Vec3::new(-4.5, 0.4, 2.0), 0.7, true, 2),
        (Vec3::new(2.0, 0.4, -5.5), 0.8, true, 0),
        (Vec3::new(8.5, 0.4, -4.5), 0.7, true, 1),
        // Loose crates sit flat; balls roll.
        (Vec3::new(3.0, 0.5, 0.0), 0.4, false, 4),
        (Vec3::new(5.2, 0.5, 3.2), 0.4, false, 4),
        (Vec3::new(-3.0, 0.5, -2.0), 0.4, false, 3),
        (Vec3::new(7.5, 0.5, -6.0), 0.4, false, 4),
    ];
    let palette = [wood, dark, red, steel, iron];

    for (at, size, is_crate, tint) in props {
        let material = palette[usize::from(*tint)].clone();
        let (mesh, collider) = if *is_crate {
            (crate_mesh(&mut meshes, *size, *size), Collider::cuboid(*size, *size, *size))
        } else {
            (ball_mesh(&mut meshes, *size), Collider::sphere(*size))
        };
        commands.spawn((
            Prop,
            RigidBody::Dynamic,
            collider,
            mesh,
            MeshMaterial3d(material),
            Transform::from_translation(*at),
        ));
    }
}

/// Keep the player's body where the simulation says they are, so walking
/// into a prop shoves it. Velocity is written too — kinematic contact
/// response reads it, which is what actually moves the crate.
pub fn follow_player(
    rendered: Res<Rendered>,
    time: Res<Time>,
    mut body: Query<(&mut Transform, &mut Position, &mut LinearVelocity), With<PlayerBody>>,
) {
    let Ok((mut transform, mut position, mut velocity)) = body.single_mut() else {
        return;
    };
    let target = rendered.pos + Vec3::new(0.0, 0.85, 0.0);
    let dt = time.delta_secs().max(1.0e-4);
    velocity.0 = (target - transform.translation) / dt;
    transform.translation = target;
    position.0 = target;
}

/// `E` grabs the nearest prop or drops what you hold; mouse throws it.
pub fn grab_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut held: ResMut<Held>,
    rendered: Res<Rendered>,
    time: Res<Time>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    props: Query<(Entity, &Transform), With<Prop>>,
    mut commands: Commands,
) {
    let Ok(cam) = camera.single() else {
        return;
    };
    let (_, cam_rot, cam_pos) = cam.to_scale_rotation_translation();
    let forward = cam_rot * Vec3::NEG_Z;
    let throw_dir = cam_rot * Vec3::NEG_Z;

    if keys.just_pressed(KeyCode::KeyE) {
        if let Some(what) = held.0.take() {
            // Let go: gravity does the rest.
            commands
                .entity(what)
                .insert((RigidBody::Dynamic, LinearVelocity::ZERO, AngularVelocity::ZERO));
        } else {
            // Nearest thing within reach.
            let mut best: Option<(f32, Entity)> = None;
            for (entity, transform) in &props {
                let dist = transform.translation.distance(rendered.pos);
                if dist < REACH && best.is_none_or(|(d, _)| dist < d) {
                    best = Some((dist, entity));
                }
            }
            if let Some((_, what)) = best {
                held.0 = Some(what);
                commands.entity(what).insert((
                    RigidBody::Kinematic,
                    LinearVelocity::ZERO,
                    AngularVelocity::ZERO,
                ));
            }
        }
    }

    let Some(what) = held.0 else {
        return;
    };

    // Carried: parked in front of the camera, facing the way you face.
    let carry = cam_pos + forward * 1.6;
    let spin = {
        let t = time.elapsed_secs();
        Vec3::new(t.sin(), t.cos(), (t * 1.7).sin()).normalize_or(Vec3::X)
    };
    if mouse.just_pressed(MouseButton::Left) {
        // Throw: dynamic, flung along the view direction with a little lift.
        commands.entity(what).insert((
            RigidBody::Dynamic,
            LinearVelocity(throw_dir * 15.0 + Vec3::Y * 2.5),
            AngularVelocity(spin * 7.0),
        ));
        held.0 = None;
        return;
    }
    commands
        .entity(what)
        .insert((
            Transform::from_translation(carry).with_rotation(cam_rot),
            Position(carry),
            Rotation(cam_rot),
            LinearVelocity::ZERO,
            AngularVelocity::ZERO,
        ));
}
