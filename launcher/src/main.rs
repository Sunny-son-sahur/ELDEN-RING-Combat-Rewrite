//! Tarnished launcher: the first run is a setup wizard. It asks what you are
//! on — Windows or Linux — then the version or the distro, writes the game's
//! config for that platform (graphics backend, display defaults), creates the
//! desktop / start-menu shortcut, and hands over the dependency command that
//! matches your package manager. After that it remembers you and goes
//! straight to Play.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use bevy::prelude::*;
use bevy::ui::BorderColor;

const ACCENT: Color = Color::srgb(0.40, 0.72, 0.95);
const PANEL: Color = Color::srgb(0.10, 0.115, 0.145);
const BACK: Color = Color::srgb(0.062, 0.070, 0.088);
const TEXT: Color = Color::srgb(0.79, 0.84, 0.88);
const DIM: Color = Color::srgb(0.50, 0.57, 0.63);
const GREEN: Color = Color::srgb(0.30, 0.62, 0.36);

// --- Choices the wizard collects --------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Os {
    Windows,
    Linux,
}

#[derive(Clone, Copy, PartialEq)]
enum Distro {
    Fedora,
    Ubuntu,
    Arch,
    Other,
}

impl Distro {
    /// (family name, dependency install command, backend)
    fn setup(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Distro::Fedora => (
                "Fedora / Nobara",
                "sudo dnf install -y vulkan-tools mesa-vulkan-drivers mesa-dri-drivers libdecor pipewire-pulseaudio alsa-lib libudev gamemode",
                "vulkan",
            ),
            Distro::Ubuntu => (
                "Ubuntu / Debian",
                "sudo apt install -y vulkan-tools mesa-vulkan-drivers libdecor-0-0 pipewire pipewire-pulseaudio libasound2 libudev1 gamemode",
                "vulkan",
            ),
            Distro::Arch => (
                "Arch / Manjaro",
                "sudo pacman -S --needed vulkan-icd-loader mesa libdecor pipewire pipewire-pulseaudio libudev gamemode",
                "vulkan",
            ),
            Distro::Other => (
                "your distribution",
                "# install: vulkan loader + drivers, pipewire (or pulseaudio), alsa, libudev, gamemode",
                "vulkan",
            ),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Os,
    Distro,
    Windows,
    Install,
    Ready,
}

/// What the wizard decided; also what it writes to disk.
#[derive(Resource)]
struct Wizard {
    screen: Screen,
    os: Option<Os>,
    distro: Option<Distro>,
    /// Windows major version, 10 or 11.
    win: Option<u32>,
    /// The line the install screen offers, and whether it ran.
    command: String,
    ran_command: bool,
    /// What the finished setup did, for the Ready screen's summary.
    notes: Vec<String>,
}

impl Default for Wizard {
    fn default() -> Self {
        // Straight to the right family for whatever machine this is.
        let os = match std::env::consts::OS {
            "windows" => Some(Os::Windows),
            "linux" => Some(Os::Linux),
            _ => None,
        };
        // A configured machine skips the questions; everyone else gets asked.
        let screen = if config_path().is_some_and(|p| p.exists()) { Screen::Ready } else { Screen::Os };
        Self { screen, os, distro: None, win: None, command: String::new(), ran_command: false, notes: Vec::new() }
    }
}

// --- Where things live ------------------------------------------------------

fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from)
}

/// The game reads this to pick backend and display defaults.
fn config_path() -> Option<PathBuf> {
    match std::env::consts::OS {
        "windows" => std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("tarnished").join("config.txt")),
        "linux" => Some(home().join(".config/tarnished/config.txt")),
        _ => None,
    }
}

/// The game binary ships next to the launcher.
fn game_binary() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let name = if cfg!(windows) { "tarnished.exe" } else { "tarnished" };
    let game = dir.join(name);
    game.is_file().then_some(game)
}

fn launcher_binary() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("tarnished-launcher"))
}

fn write_config(wizard: &mut Wizard) {
    let Some(path) = config_path() else {
        return;
    };
    let (backend, platform) = match wizard.os {
        Some(Os::Windows) => {
            let version = wizard.win.unwrap_or(10);
            ("dx12", format!("windows{version}"))
        }
        Some(Os::Linux) => {
            let distro = wizard.distro.unwrap_or(Distro::Other);
            (distro.setup().2, format!("linux:{}", distro.setup().0))
        }
        None => ("vulkan", "unknown".to_string()),
    };
    let body = format!(
        "platform={platform}\nbackend={backend}\nfullscreen=false\nvsync=true\n"
    );
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::write(&path, body) {
        Ok(()) => wizard.notes.push(format!("Wrote {}", path.display())),
        Err(error) => wizard.notes.push(format!("Could not write {}: {error}", path.display())),
    }
}

/// Linux only: menu entries for the game and for running setup again.
#[cfg(target_os = "linux")]
fn create_shortcuts(wizard: &mut Wizard) {
    use std::os::unix::fs::PermissionsExt;
    let dir = home().join(".local/share/applications");
    if std::fs::create_dir_all(&dir).is_err() {
        wizard.notes.push("No ~/.local/share/applications - shortcut skipped".to_string());
        return;
    }
    let game = game_binary().unwrap_or_else(|| PathBuf::from("tarnished"));
    let launch = launcher_binary();
    let entries = [
        ("tarnished.desktop", "Tarnished", "movement & combat sandbox", &game),
        ("tarnished-setup.desktop", "Tarnished Setup", "Run first-time setup again", &launch),
    ];
    let mut made = Vec::new();
    for (file, name, comment, target) in entries {
        // Paths can hold spaces; the desktop spec takes them inside quotes.
        let body = format!(
            "[Desktop Entry]\nType=Application\nName={name}\nComment={comment}\nExec=\"{}\"\nIcon=input-gaming\nTerminal=false\nCategories=Game;\n",
            target.display()
        );
        let path = dir.join(file);
        if std::fs::write(&path, body).is_ok() {
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
            made.push(name);
        }
    }
    if made.is_empty() {
        wizard.notes.push("Shortcut creation failed".to_string());
    } else {
        wizard.notes.push(format!("Added {} to your applications menu", made.join(" and ")));
    }
    let _ = Command::new("update-desktop-database").arg(&dir).stdout(Stdio::null()).stderr(Stdio::null()).status();
}

#[cfg(not(target_os = "linux"))]
fn create_shortcuts(wizard: &mut Wizard) {
    // The Windows build drops a Start Menu link on first run; written here as
    // a starter script until the exe does it natively.
    wizard.notes.push("Start Menu shortcut: created on the Windows build".to_string());
}

/// Try to run the dependency line in a terminal so the user can watch it.
fn run_in_terminal(command: &str) -> bool {
    let script = format!("{command}; echo; echo 'Done. Close this window.'; read -r _");
    let terminals: &[(&str, &[&str])] = &[
        ("gnome-terminal", &["--"]),
        ("konsole", &["-e"]),
        ("foot", &["-e"]),
        ("xterm", &["-e"]),
    ];
    for (term, split) in terminals {
        if which(term).is_none() {
            continue;
        }
        let spawned = Command::new(term)
            .args(*split)
            .arg("bash")
            .arg("-c")
            .arg(&script)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if spawned.is_ok() {
            return true;
        }
    }
    false
}

fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|dir| dir.join(program)).find(|p| p.is_file())
}

/// OS-release guesswork so Fedora users do not have to answer twice.
#[cfg(target_os = "linux")]
fn detect_distro() -> Option<(&'static str, Distro)> {
    let text = std::fs::read_to_string("/etc/os-release").ok()?;
    let field = |key: &str| text.lines().find_map(|l| l.strip_prefix(&format!("{key}=")).map(|v| v.trim_matches('"')));
    let name = field("NAME")?;
    let like = field("ID_LIKE").unwrap_or("").to_string();
    let id = field("ID").unwrap_or("").to_string();
    let combined = format!("{id} {like} {name}").to_lowercase();
    for (needle, label, distro) in [
        ("nobara", "Nobara", Distro::Fedora),
        ("fedora", "Fedora", Distro::Fedora),
        ("ubuntu", "Ubuntu", Distro::Ubuntu),
        ("debian", "Debian", Distro::Ubuntu),
        ("arch", "Arch", Distro::Arch),
        ("manjaro", "Manjaro", Distro::Arch),
    ] {
        if combined.contains(needle) {
            return Some((label, distro));
        }
    }
    None
}

#[cfg(not(target_os = "linux"))]
fn detect_distro() -> Option<(&'static str, Distro)> {
    None
}

// --- UI ---------------------------------------------------------------------

/// One button in the wizard.
#[derive(Component)]
enum Action {
    PickOs(Os),
    PickDistro(Distro),
    PickWindows(u32),
    Install,
    Play,
    SetupAgain,
    Quit,
}

#[derive(Component)]
struct ScreenRoot;

fn font(size: f32) -> TextFont {
    TextFont { font_size: FontSize::Px(size), ..default() }
}

fn px(v: f32) -> Val {
    Val::Px(v)
}

fn percent(v: f32) -> Val {
    Val::Percent(v)
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let root = commands
        .spawn((
            Node {
                width: percent(100.0),
                height: percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: px(18.0),
                ..default()
            },
            BackgroundColor(BACK),
            ScreenRoot,
        ))
        .id();
    // The window title block lives outside the swapping screens.
    commands.spawn((
        Text::new("TARNISHED"),
        font(38.0),
        TextColor(TEXT),
        Node { position_type: PositionType::Absolute, top: px(46.0), ..default() },
        ChildOf(root),
    ));
    commands.spawn((
        Text::new("launcher - one-time setup"),
        font(14.0),
        TextColor(DIM),
        Node { position_type: PositionType::Absolute, top: px(96.0), ..default() },
        ChildOf(root),
    ));
}

/// Big clickable card used for every choice.
fn card(commands: &mut Commands, parent: Entity, action: Action, title: &str, sub: &str, width: f32) -> Entity {
    let id = commands
        .spawn((
            Node {
                width: px(width),
                height: px(86.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                padding: UiRect::all(px(18.0)),
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
    commands.spawn((Text::new(title), font(20.0), TextColor(TEXT), ChildOf(id)));
    if !sub.is_empty() {
        commands.spawn((Text::new(sub), font(13.0), TextColor(DIM), ChildOf(id)));
    }
    id
}

fn heading(commands: &mut Commands, parent: Entity, text: &str) {
    commands.spawn((Text::new(text), font(24.0), TextColor(TEXT), ChildOf(parent)));
}

fn note(commands: &mut Commands, parent: Entity, text: &str, colour: Color) {
    commands.spawn((Text::new(text), font(13.0), TextColor(colour), ChildOf(parent)));
}

/// Rebuild whichever screen is current.
fn build_screen(
    wizard: Res<Wizard>,
    mut commands: Commands,
    root: Query<Entity, With<ScreenRoot>>,
    kids: Query<&Children>,
) {
    if !wizard.is_changed() {
        return;
    }
    let Ok(root) = root.single() else {
        return;
    };
    // despawn() in 0.19 takes the subtree with it — clear, then rebuild.
    if let Ok(children) = kids.get(root) {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }
    let column = |commands: &mut Commands, parent: Entity| {
        commands
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(14.0),
                    ..default()
                },
                ChildOf(parent),
            ))
            .id()
    };
    match wizard.screen {
        Screen::Os => {
            let col = column(&mut commands, root);
            heading(&mut commands, col, "What are you playing on?");
            card(&mut commands, col, Action::PickOs(Os::Windows), "Windows", "10 or 11", 340.0);
            card(&mut commands, col, Action::PickOs(Os::Linux), "Linux", "pick your distribution next", 340.0);
        }
        Screen::Distro => {
            let col = column(&mut commands, root);
            heading(&mut commands, col, "Which distribution?");
            if let Some((label, distro)) = detect_distro() {
                note(&mut commands, col, &format!("detected: {label}"), ACCENT);
                card(&mut commands, col, Action::PickDistro(distro), &format!("{label} - that's me"), "", 340.0);
            }
            for (distro, title, sub) in [
                (Distro::Fedora, "Fedora / Nobara", "dnf"),
                (Distro::Ubuntu, "Ubuntu / Debian", "apt"),
                (Distro::Arch, "Arch / Manjaro", "pacman"),
                (Distro::Other, "Something else", "you get the generic advice"),
            ] {
                card(&mut commands, col, Action::PickDistro(distro), title, sub, 340.0);
            }
        }
        Screen::Windows => {
            let col = column(&mut commands, root);
            heading(&mut commands, col, "Which version of Windows?");
            card(&mut commands, col, Action::PickWindows(10), "Windows 10", "DirectX 12", 340.0);
            card(&mut commands, col, Action::PickWindows(11), "Windows 11", "DirectX 12, auto-HDR if you have it", 340.0);
        }
        Screen::Install => {
            let col = column(&mut commands, root);
            heading(&mut commands, col, "Setup");
            for line in &wizard.notes {
                note(&mut commands, col, line, DIM);
            }
            if !wizard.command.is_empty() {
                note(&mut commands, col, "Dependencies for your system (needs your password):", TEXT);
                note(&mut commands, col, &wizard.command, ACCENT);
                if !wizard.ran_command {
                    card(&mut commands, col, Action::Install, "Install them now", "opens a terminal", 340.0);
                } else {
                    note(&mut commands, col, "install started - watch the terminal", GREEN);
                }
            }
            card(&mut commands, col, Action::Play, "Done - take me in", "", 340.0);
        }
        Screen::Ready => {
            let col = column(&mut commands, root);
            heading(&mut commands, col, "Ready.");
            for line in &wizard.notes {
                note(&mut commands, col, line, DIM);
            }
            if game_binary().is_some() {
                card(&mut commands, col, Action::Play, "Play", "", 380.0);
            } else {
                note(&mut commands, col, "tarnished binary not found next to the launcher", Color::srgb(0.85, 0.4, 0.35));
            }
            card(&mut commands, col, Action::SetupAgain, "Run setup again", "", 380.0);
            card(&mut commands, col, Action::Quit, "Quit", "", 380.0);
        }
    }
}

/// Finish the questions: write config, shortcuts, and line up the install step.
fn finish_questions(wizard: &mut Wizard) {
    match wizard.os {
        Some(Os::Linux) => {
            let distro = wizard.distro.unwrap_or(Distro::Other);
            wizard.command = distro.setup().1.to_string();
            wizard.notes.clear();
            write_config(wizard);
            create_shortcuts(wizard);
        }
        Some(Os::Windows) => {
            wizard.notes.clear();
            write_config(wizard);
            create_shortcuts(wizard);
        }
        None => {}
    }
}

fn clicks(
    mut interactions: Query<(&Interaction, &Action), Changed<Interaction>>,
    mut wizard: ResMut<Wizard>,
    mut exit: MessageWriter<AppExit>,
) {
    for (interaction, action) in &mut interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            Action::PickOs(os) => {
                wizard.os = Some(*os);
                wizard.screen = match os {
                    Os::Windows => Screen::Windows,
                    Os::Linux => Screen::Distro,
                };
            }
            Action::PickDistro(distro) => {
                wizard.distro = Some(*distro);
                finish_questions(&mut wizard);
                wizard.screen = Screen::Install;
            }
            Action::PickWindows(version) => {
                wizard.win = Some(*version);
                finish_questions(&mut wizard);
                wizard.screen = Screen::Install;
            }
            Action::Install => {
                wizard.ran_command = run_in_terminal(&wizard.command);
            }
            Action::Play => {
                play(&wizard);
                exit.write(AppExit::Success);
            }
            Action::SetupAgain => {
                let _ = config_path().map(|p| std::fs::remove_file(p));
                wizard.screen = Screen::Os;
                wizard.os = None;
                wizard.distro = None;
                wizard.win = None;
                wizard.command.clear();
                wizard.ran_command = false;
                wizard.notes.clear();
            }
            Action::Quit => {
                exit.write(AppExit::Success);
            }
        }
    }
}

/// Hand off to the game with the backend its platform picked.
fn play(wizard: &Wizard) {
    let Some(game) = game_binary() else {
        eprintln!("tarnished binary not found next to the launcher");
        return;
    };
    let saved = config_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|text| {
            text.lines()
                .find_map(|l| l.strip_prefix("backend="))
                .map(|v| v.trim().to_string())
        });
    let backend = saved.unwrap_or_else(|| match wizard.os {
        Some(Os::Windows) => "dx12".to_string(),
        Some(Os::Linux) => wizard.distro.unwrap_or(Distro::Other).setup().2.to_string(),
        None => "vulkan".to_string(),
    });
    let dir = game.parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let spawned = Command::new(&game)
        .current_dir(dir)
        .env("WGPU_BACKEND", backend)
        .spawn();
    if let Err(error) = spawned {
        eprintln!("could not start {}: {error}", game.display());
    }
}

/// Hover feedback on whatever button is under the cursor.
fn hover(mut interactions: Query<(&Interaction, &mut BorderColor), (Changed<Interaction>, With<Button>)>) {
    for (interaction, mut border) in &mut interactions {
        *border = BorderColor::all(match interaction {
            Interaction::Hovered => ACCENT,
            Interaction::Pressed => GREEN,
            Interaction::None => Color::srgba(0.0, 0.0, 0.0, 0.0),
        });
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Tarnished - Setup".into(),
                resolution: (960, 580).into(),
                ..default()
            }),
            ..default()
        }))
        .init_resource::<Wizard>()
        .add_systems(Startup, setup)
        .add_systems(Update, (build_screen, clicks, hover))
        .run();
}
