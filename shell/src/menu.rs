//! The front door: title screen, the singleplayer / multiplayer picker, and
//! the loading bar between them and the arena.
//!
//! Nothing in the game runs until a mode is chosen — the simulation, input and
//! camera systems are all gated on [`Phase::Game`]. The loading screen does
//! real work in stages: it bakes the animation clips, builds the rig, places
//! the props, and lets physics settle them before dropping you in.

use bevy::prelude::*;
use bevy::ui::BorderColor;

use crate::props;
use crate::rig;

const ACCENT: Color = Color::srgb(0.40, 0.72, 0.95);
const PANEL: Color = Color::srgb(0.10, 0.115, 0.145);
const TEXT: Color = Color::srgb(0.79, 0.84, 0.88);
const DIM: Color = Color::srgb(0.50, 0.57, 0.63);
const TRACK: Color = Color::srgb(0.16, 0.18, 0.22);

/// Where the app is in its life. The whole game only exists in `Game`.
#[derive(Resource, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Big title, mode picker.
    Title,
    /// The multiplayer card leads here until servers exist.
    Multi,
    /// Staged load into the arena.
    Loading,
    /// In the arena.
    Game,
}

/// How far along the staged load is. Stage work happens in [`loader`].
#[derive(Resource, Default)]
pub struct Loading {
    stage: u8,
    /// Frames spent waiting for props to come to rest in the settle stage.
    settled_frames: u16,
}

/// The four things the load actually does, in order.
const STAGES: [&str; 4] = [
    "baking animation clips",
    "building the rig",
    "placing props",
    "settling physics",
];

/// Progress as 0..1 across the stages; the settle stage crawls with
/// `sub`, everything else jumps a quarter at a time.
pub fn progress(stage: u8, sub: f32) -> f32 {
    let stages = STAGES.len() as f32;
    ((stage as f32 + sub.clamp(0.0, 1.0)) / stages).min(1.0)
}

pub fn stage_label(stage: u8) -> &'static str {
    STAGES.get(stage as usize).copied().unwrap_or("ready")
}

#[derive(Component)]
pub struct MenuRoot;

/// What a button on the menu does.
#[derive(Component)]
pub enum Action {
    Single,
    Multi,
    Back,
    Quit,
}

/// The stage caption under the bar.
#[derive(Component)]
pub struct StageCaption;

/// The coloured part of the loading bar.
#[derive(Component)]
pub struct Fill;

fn font(size: f32) -> TextFont {
    TextFont { font_size: FontSize::Px(size), ..default() }
}

fn px(v: f32) -> Val {
    Val::Px(v)
}

fn percent(v: f32) -> Val {
    Val::Percent(v)
}

pub fn setup(mut commands: Commands) {
    // Skip the title when asked to — handy for automated runs.
    let skip = std::env::var("TARNISHED_BOOT").is_ok_and(|v| v == "game");
    commands.insert_resource(if skip { Phase::Loading } else { Phase::Title });
    commands.insert_resource(Loading::default());

    commands.spawn((
        Node {
            width: percent(100.0),
            height: percent(100.0),
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            ..default()
        },
        // Almost opaque: the arena shows through as a faint backdrop.
        BackgroundColor(Color::srgba(0.035, 0.04, 0.055, 0.965)),
        MenuRoot,
    ));
}

/// Rebuild the menu whenever the phase changes.
pub fn screen(
    phase: Res<Phase>,
    mut commands: Commands,
    mut root: Query<(Entity, &mut Visibility), With<MenuRoot>>,
    kids: Query<&Children>,
) {
    if !phase.is_changed() {
        return;
    }
    let Ok((root, mut visibility)) = root.single_mut() else {
        return;
    };
    if *phase == Phase::Game {
        // In the arena the menu is simply gone; the HUD owns the screen.
        *visibility = Visibility::Hidden;
        return;
    }
    *visibility = Visibility::Visible;
    // Clear whatever the previous screen drew — despawn takes the subtree.
    if let Ok(children) = kids.get(root) {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }

    match *phase {
        Phase::Title => title(&mut commands, root),
        Phase::Multi => multiplayer(&mut commands, root),
        Phase::Loading => loading(&mut commands, root),
        Phase::Game => {}
    }
}

fn card(commands: &mut Commands, parent: Entity, action: Action, title: &str, sub: &str, width: f32) {
    let id = commands
        .spawn((
            Node {
                width: px(width),
                height: px(84.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                padding: UiRect::all(px(20.0)),
                row_gap: px(4.0),
                    ..default()
            },
            BackgroundColor(PANEL),
            BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            Button,
            action,
            ChildOf(parent),
        ))
        .id();
    commands.spawn((Text::new(title), font(22.0), TextColor(TEXT), ChildOf(id)));
    if !sub.is_empty() {
        commands.spawn((Text::new(sub), font(13.0), TextColor(DIM), ChildOf(id)));
    }
}

fn title_text(commands: &mut Commands, parent: Entity, text: &str, size: f32, colour: Color) {
    commands.spawn((Text::new(text), font(size), TextColor(colour), ChildOf(parent)));
}

/// The column every screen lays its content out in.
fn column(commands: &mut Commands, parent: Entity, top_pad: f32) -> Entity {
    commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(14.0),
                padding: UiRect { top: px(top_pad), ..default() },
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

fn title(commands: &mut Commands, root: Entity) {
    let col = column(commands, root, 90.0);
    title_text(commands, col, "TARNISHED", 64.0, TEXT);
    title_text(commands, col, "movement & combat sandbox", 16.0, DIM);
    // A breath of space before the choices.
    commands.spawn((Node { height: px(26.0), ..default() }, ChildOf(col)));
    card(commands, col, Action::Single, "Singleplayer", "the arena - fight, climb, throw things", 380.0);
    card(commands, col, Action::Multi, "Multiplayer", "with friends - next build", 380.0);
    card(commands, col, Action::Quit, "Quit", "", 380.0);
    title_text(commands, col, "v0.3 - phase 3", 12.0, Color::srgba(0.4, 0.45, 0.5, 0.7));
}

fn multiplayer(commands: &mut Commands, root: Entity) {
    let col = column(commands, root, 150.0);
    title_text(commands, col, "MULTIPLAYER", 34.0, TEXT);
    title_text(
        commands,
        col,
        "Server browser, direct connect, and synchronized fights land next build.",
        15.0,
        DIM,
    );
    title_text(commands, col, "For now the arena is yours alone.", 15.0, DIM);
    commands.spawn((Node { height: px(20.0), ..default() }, ChildOf(col)));
    card(commands, col, Action::Back, "Back", "", 380.0);
}

fn loading(commands: &mut Commands, root: Entity) {
    let col = column(commands, root, 170.0);
    title_text(commands, col, "TARNISHED", 30.0, Color::srgba(0.79, 0.84, 0.88, 0.55));
    commands.spawn((Node { height: px(30.0), ..default() }, ChildOf(col)));

    let track = commands
        .spawn((
            Node {
                width: px(440.0),
                height: px(10.0),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(TRACK),
            ChildOf(col),
        ))
        .id();
    commands.spawn((
        Node {
            width: percent(0.0),
            height: percent(100.0),
            ..default()
        },
        BackgroundColor(ACCENT),
        Fill,
        ChildOf(track),
    ));

    commands.spawn((Text::new(stage_label(0)), font(15.0), TextColor(TEXT), StageCaption, ChildOf(col)));
    commands.spawn((
        Text::new("tip - E picks up whatever you're near, click throws it"),
        font(13.0),
        TextColor(DIM),
        ChildOf(col),
    ));
}

/// Advance the load one stage per frame; the settle stage waits for the
/// props to stop tumbling (or a generous timeout, so it can't hang).
pub fn loader(
    mut phase: ResMut<Phase>,
    mut loading: ResMut<Loading>,
    mut commands: Commands,
    props: Query<&avian3d::prelude::LinearVelocity, With<props::Prop>>,
) {
    if *phase != Phase::Loading {
        return;
    }
    match loading.stage {
        0 => {
            // The clip bake is the single biggest chunk of work.
            let clips = crate::anim::Clips::procedural();
            commands.insert_resource(clips);
            loading.stage = 1;
        }
        1 => {
            commands.run_system_cached(rig::setup);
            loading.stage = 2;
        }
        2 => {
            commands.run_system_cached(props::spawn);
            loading.stage = 3;
        }
        3 => {
            let resting = props
                .iter()
                .all(|v| v.0.length() < 0.12 || v.0.length().is_nan());
            loading.settled_frames = loading.settled_frames.saturating_add(1);
            // Hold long enough that the screen is actually readable.
            if resting && loading.settled_frames > 130 || loading.settled_frames > 300 {
                loading.stage = 4;
            }
        }
        _ => {
            *phase = Phase::Game;
        }
    }
}

/// Keeps the caption honest and glides the bar toward the real progress.
pub fn progress_ui(
    phase: Res<Phase>,
    loading: Res<Loading>,
    mut caption: Query<&mut Text, With<StageCaption>>,
    mut fill: Query<&mut Node, With<Fill>>,
    mut shown: Local<f32>,
) {
    if *phase != Phase::Loading {
        return;
    }
    // While settling, `sub` counts elapsed frames so the bar keeps moving.
    let sub = if loading.stage == 3 {
        f32::from(loading.settled_frames) / 300.0
    } else {
        0.0
    };
    let target = progress(loading.stage, sub);
    *shown += (target - *shown) * 0.15;
    if let Ok(mut node) = fill.single_mut() {
        node.width = percent(*shown * 100.0);
    }
    let label = stage_label(loading.stage);
    if let Ok(mut text) = caption.single_mut() {
        if text.0 != label {
            text.0 = label.to_string();
        }
    }
}

pub fn clicks(
    mut interactions: Query<(&Interaction, &Action), Changed<Interaction>>,
    mut phase: ResMut<Phase>,
    mut loading: ResMut<Loading>,
    mut exit: MessageWriter<AppExit>,
) {
    for (interaction, action) in &mut interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            Action::Single => {
                loading.stage = 0;
                loading.settled_frames = 0;
                *phase = Phase::Loading;
            }
            Action::Multi => *phase = Phase::Multi,
            Action::Back => *phase = Phase::Title,
            Action::Quit => {
                exit.write(AppExit::Success);
            }
        }
    }
}

/// Hover highlight, same language as the launcher.
pub fn hover(mut interactions: Query<(&Interaction, &mut BorderColor), (Changed<Interaction>, With<Button>)>) {
    for (interaction, mut border) in &mut interactions {
        *border = BorderColor::all(match interaction {
            Interaction::Hovered => ACCENT,
            Interaction::Pressed => Color::srgb(0.3, 0.7, 0.4),
            Interaction::None => Color::srgba(0.0, 0.0, 0.0, 0.0),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_climbs_and_caps() {
        assert!((progress(0, 0.0) - 0.0).abs() < 1.0e-6);
        assert!((progress(2, 0.5) - 0.625).abs() < 1.0e-4);
        // A stage past the end still reads as finished, never over.
        assert_eq!(progress(9, 1.0), 1.0);
    }

    #[test]
    fn every_stage_has_a_caption() {
        assert_eq!(stage_label(0), "baking animation clips");
        assert_eq!(stage_label(1), "building the rig");
        assert_eq!(stage_label(2), "placing props");
        assert_eq!(stage_label(3), "settling physics");
        // Past the end: honest, not a panic.
        assert_eq!(stage_label(200), "ready");
    }
}
