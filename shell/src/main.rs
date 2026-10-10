//! Elden Ring-style player movement and combat on a primitive rig.
//!
//! `sim` is the whole game: a pure 60 Hz state machine with no engine types.
//! Everything else here only feeds it input and draws what it says.

use bevy::prelude::*;

mod anim;
mod audio;
mod camera;
mod demo;
mod hud;
mod input;
mod menu;
mod props;
mod rig;
mod view;

pub use menu::Phase;

use tarnished_sim::level::Level;
use tarnished_sim::player::Input as SimInput;

#[derive(Resource)]
pub struct Sim(pub tarnished_sim::World);

/// Input accumulated since the last simulation tick. Button edges are OR-ed in
/// so a tap shorter than a tick is never lost.
#[derive(Resource, Default)]
pub struct Pending(pub SimInput);

/// Where the player was one tick ago, and where they are drawn this frame.
#[derive(Resource, Default)]
pub struct Rendered {
    prev_pos: Vec3,
    prev_yaw: f32,
    pub pos: Vec3,
    pub yaw: f32,
    /// How far this frame sits between the last two simulation ticks, 0..1.
    pub alpha: f32,
}

#[derive(Resource)]
pub struct Options {
    /// Tint the character while invincible.
    pub show_iframes: bool,
    pub show_help: bool,
}

fn main() {
    // Sounds are optional: without them the sandbox runs silent.
    let sounds = match audio::Sounds::load() {
        Ok(sounds) => Some(sounds),
        Err(error) => {
            eprintln!("No sound ({}: {error}); the sandbox runs silent for now.", audio::PATH);
            None
        }
    };
    let mut app = App::new();
    if let Some(sounds) = sounds {
        app.insert_resource(sounds);
    }
    app
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Tarnished - movement & combat sandbox".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(avian3d::prelude::PhysicsPlugins::default())
        .insert_resource(Time::<Fixed>::from_hz(tarnished_sim::data::TICK_HZ))
        .insert_resource(ClearColor(Color::srgb(0.07, 0.08, 0.1)))
        .insert_resource(Sim(tarnished_sim::World::new(Level::arena())))
        .insert_resource(Options { show_iframes: true, show_help: true })
        .init_resource::<Pending>()
        .init_resource::<Rendered>()
        .init_resource::<camera::CamRig>()
        .init_resource::<demo::Demo>()
        .init_resource::<props::Held>()
        // The rig and its clips are built by the loading screen, not here.
        .add_systems(Startup, (view::setup, hud::setup, camera::setup, menu::setup).chain())
        // Nothing reads the mouse or steps the world until a mode is picked.
        .add_systems(
            RunFixedMainLoop,
            (input::grab_cursor, camera::look, input::gather)
                .chain()
                .run_if(in_game)
                .in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
        )
        .add_systems(FixedUpdate, (demo::drive, tick).chain().run_if(in_game))
        .add_systems(
            Update,
            (
                demo::control.run_if(in_game),
                interpolate,
                input::debug_keys.run_if(in_game),
                rig::animate.run_if(rig_ready),
                audio::play.run_if(rig_ready),
                view::animate_dummy,
                view::draw_grid,
                camera::follow,
                hud::update,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                menu::screen,
                menu::loader,
                menu::progress_ui,
                menu::clicks,
                menu::hover,
                props::follow_player.run_if(in_game),
                props::grab_keys.run_if(in_game).chain(),
            ),
        )
        .run();
}

fn tick(
    mut sim: ResMut<Sim>,
    mut pending: ResMut<Pending>,
    mut rendered: ResMut<Rendered>,
    mut cam: ResMut<camera::CamRig>,
) {
    rendered.prev_pos = sim.0.player.pos;
    rendered.prev_yaw = sim.0.player.yaw;
    sim.0.step(&pending.0);
    if sim.0.recenter_camera {
        cam.recenter_on(sim.0.player.yaw);
    }

    // Edges have been consumed; held state carries over.
    let inp = &mut pending.0;
    for button in [&mut inp.dodge, &mut inp.jump, &mut inp.light, &mut inp.heavy, &mut inp.guard] {
        button.pressed = false;
        button.released = false;
    }
    inp.crouch = false;
    inp.lock = false;
    inp.two_hand_right = false;
    inp.two_hand_left = false;
    inp.next_weapon = false;
    inp.next_left = false;
}

fn interpolate(sim: Res<Sim>, time: Res<Time<Fixed>>, mut rendered: ResMut<Rendered>) {
    let alpha = time.overstep_fraction();
    let player = &sim.0.player;
    // A respawn is a teleport, not something to glide across.
    let alpha = if rendered.prev_pos.distance(player.pos) > 3.0 { 1.0 } else { alpha };
    rendered.alpha = alpha;
    rendered.pos = rendered.prev_pos.lerp(player.pos, alpha);
    rendered.yaw = rendered.prev_yaw + tarnished_sim::angle_diff(rendered.prev_yaw, player.yaw) * alpha;
}

/// The game proper only runs in [`Phase::Game`].
fn in_game(phase: Res<Phase>) -> bool {
    *phase == Phase::Game
}

/// The rig animation system needs the clips and the rig, which the loading
/// screen builds partway through. Before that it simply has nothing to say.
fn rig_ready(clips: Option<Res<anim::Clips>>, rig: Option<Res<rig::Rig>>) -> bool {
    clips.is_some() && rig.is_some()
}
