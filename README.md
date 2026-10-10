# Sandbox fighting, in Bevy

A third-person fighting game with a physics-sandbox heart, written in Rust on
[Bevy](https://bevyengine.org) 0.19. Tight melee combat — rolls, stamina,
guard, hit-stop, chained weapons, per-hit damage — wrapped in a Garry's
Mod-flavored sandbox: props to throw, tools to play with, and a front end
where you pick singleplayer or multiplayer.

*Working title — the exe gets a real name when the boss names it.*

The character is a stick-and-capsule rig. The combat is a pure 60 Hz state
machine (`sim/`) with no engine types in it; the Bevy shell (`shell/`) draws
it and feeds it input.

https://github.com/user-attachments/assets/0b80042e-a40b-4a15-a64e-e75e20966ace

*Off-hand weapons, paired attacks and hit-stop, played by hand.*

https://github.com/user-attachments/assets/b58dbb0d-a3db-46d5-82ee-d9152503e40c

*The sandbox playing itself through everything it does (an earlier build).*

**Origin, honestly:** the combat state machine began as a rewrite of ELDEN
RING's player movement (the upstream project this forked from). Since then
every game-data value has been replaced with content authored for this
project in `sim/src/content.rs`. The repository carries no game files, needs
no extraction pipeline, and asks nothing of any installed game.

## What is in it

### Movement

- Walk, run and sprint, with the braking animation when you stop from a run.
- Crouching, with its own idle, walk, run, stop, and the transitions in and
  out of it.
- Lock-on: strafing around the target in four directions while facing it.
- Jumps from a standstill, a walk, a run and a sprint. Locked on, walking and
  running jumps go forward, back, left or right while you keep facing the
  target, and so do their landings.
- Falling: a separate start when you walk off a ledge, light and heavy
  landings, fall damage and fall death.
- Stairs and slopes, with each foot placed on the ground under it.

### Dodging

- Rolls come out when the button is released; holding it sprints instead.
- Light, medium and heavy equip-load rolls, backsteps and crouched rolls.
- Rolls in four directions while locked on, with i-frames and early roll-out
  of knockdowns.

### Combat

- Light attack chains, heavy and charged heavy attacks.
- Running, rolling, backstep, crouch, jump and guard-counter attacks.
- Multi-hit attacks land every hit, each with its own damage and stamina cost.
- Hits land when the weapon reaches the target, not when the swing starts:
  stand too far away, or beside a thrust, and it misses.
- Hit-stop: attacker and target freeze for an instant when a blow lands.
- Guarding with the shield, or with any weapon held in both hands; guard
  hits and guard break.
- The left hand: a shield, a torch, nothing, or a second weapon. Anything
  but the shield attacks on the guard button, with that weapon's own
  left-hand chain.
- Paired weapons ("power stance"): the same class in each hand turns the
  left button into a moveset that uses both, with its own chain and its own
  running, rolling, backstep and jump attacks.
- Hit reactions in four strengths and four directions: a flinch, a stagger, a
  large stagger, and a knockdown that throws you back, keeps you invincible
  while you are down and lets you roll out early.
- Changing grip or weapon plays on the upper body while you keep moving.

### Weapons

Eight classes so far, each one- or two-handed with its own moveset either
way: Shiv (fast), Longsword (standard), Claymore (heavy), Cudgel (strike),
Spear (thrusts), Fist (fast and short), Torch (light), plus a Shield.
Any of them can go in the left hand, and whatever is there can be two-handed
too. New classes are a data edit in `content.rs`, not engine work.

### The sparring dummy

It stands still until you press `T`. Hostile, it cycles through four attacks,
one for each hit reaction:

| Attack | Telegraph | Reaction |
|---|---|---|
| Overhead slam | orange | stagger |
| Low sweep (jump it) | blue | flinch |
| Harder slam | red | large stagger |
| Hardest slam | purple | knockdown |

### The arena

Crates stacked into a climb up to a wall you can walk along, a platform
across a gap from it, a slope, a row of pillars, a low dais, and a staircase
21 m high that passes every fall-damage threshold on the way up.

### The demo

Press `Enter` and the sandbox plays itself through everything above, with a
caption for each thing it shows: about five minutes, made for recording.
`Enter` again stops it. It resets the arena when it starts, and plays through
the same inputs a player has, so nothing in it is staged.

### Sound

Silent for now. The old build played baked sound files from the source
game via a pipeline that has been removed with the rest of it; the next
sound pass is this project's own assets.

## The data

Everything about actions — durations, i-frames, cancel windows, hit windows,
root motion, stamina costs, motion values, locomotion speeds, weapon stats —
is authored in `sim/src/content.rs`, in plain Rust, committed to the
repository. Tune a number there and the behaviour changes. The tests in
`sim/src/tests.rs` describe what the combat is supposed to do; run them with
`cargo test -p tarnished-sim`.

Values still marked `ESTIMATE` in `sim/src/data.rs` are engine-level
constants: how long the dodge button must be held for a sprint, stamina
regeneration and sprint drain, gravity after a jump's arc ends, fall-damage
thresholds, locomotion acceleration and turn rates, hit shapes, and the
dummy's made-up attacks.

Not included (yet): weapon skills, parrying, stat scaling, the two-handing
damage bonus, and being launched into the air by a hit.

## Controls

| Action | Keyboard / mouse | Gamepad |
|---|---|---|
| Move / look | WASD / mouse | Left stick / right stick |
| Walk | Left Alt | Tilt lightly |
| Roll or backstep | Tap Space | Tap B |
| Sprint | Hold Space | Hold B |
| Jump | F | A |
| Crouch | X | L3 |
| Light attack | Left click | RB |
| Heavy attack (hold to charge) | Shift + left click | RT |
| Guard, or left-hand attack | Right click | LB |
| Guard counter | Shift + left click after blocking a hit | RT after blocking a hit |
| Two-hand right weapon | E + left click | Y + RB |
| Two-hand left armament | E + right click | Y + LB |
| Next weapon | Right arrow | D-pad right |
| Next off-hand (shield, nothing, torch, each weapon) | Left arrow | D-pad left |
| Lock on | Q or middle click | R3 |

Sandbox keys: `1` / `2` / `3` set light / medium / heavy equip load, `T` makes
the dummy hostile, `F1` toggles the i-frame tint, `H` toggles the help overlay,
`Enter` plays the demo,
`Esc` releases the mouse. Click the window to capture the mouse.

## Setup

You need Rust and nothing else. No Python, no game files, no unpacking.

```bash
cargo run                    # build and play
cargo test -p tarnished-sim  # behaviour tests, headless, seconds
```

`cargo run` opens the title screen: pick **Singleplayer** and a loading
screen does real work in stages (bake the clips, build the rig, place the
props, settle the physics) before dropping you in the arena. Press `Enter`
for the demo tour. The character is posed entirely from code —
`shell/src/anim.rs` generates every clip from the skeleton and a library of
keyed poses, no clip files, nothing baked. Set `TARNISHED_BOOT=game` to skip
the menu (used by automated runs).

## Roadmap

1. ~~Front end~~ — title screen, loading screen, singleplayer / multiplayer
   picker (**done**).
2. ~~Sandbox layer~~ — physics props with `avian3d`: crates and balls that
   stack, roll, can be picked up with `E` and thrown (**done**; spawn menu and
   sandbox tools still to come).
3. **Multiplayer** — one machine runs the sim authoritative, others send
   inputs (`bevy_replicon`). The Multiplayer button is waiting for it.
4. **Shipping** — cross-compiled Windows exe, a Unity-style game folder,
   playable through Steam/Proton on Linux.

## Layout

The workspace is three crates: `sim/` holds the whole game with no engine types (it
depends on nothing but `bevy_math`, so `cargo test -p tarnished-sim` runs headless in
seconds), `shell/` is the Bevy app that feeds it input and draws what it says, and
`launcher/` is the setup wizard and Play button that will ship in front of the game.
`cargo run` from the project folder runs the shell.

| Path | What it is |
|---|---|
| `sim/src/` | The whole game as a pure 60 Hz state machine, with no engine types. |
| `sim/src/content.rs` | All game data: weapons, actions, timings, root motion, blades. Authored, committed, tune freely. |
| `sim/src/data.rs` | Action types, plus every value that is still an estimate. |
| `sim/src/player.rs`, `sim/src/dummy.rs`, `sim/src/level.rs` | The player, the sparring dummy and the arena. |
| `sim/src/tests.rs` | Behaviour tests; run with `cargo test`. |
| `shell/src/rig.rs`, `shell/src/anim.rs` | The rig, and the procedural animation it plays (skeleton + pose library, all clips generated in code at startup). |
| `shell/src/audio.rs` | Sound playback (pipeline removed; runs silent). |
| `shell/src/demo.rs` | The scripted demo, and a test that plays it through without a window. |
| `shell/src/camera.rs`, `shell/src/input.rs`, `shell/src/hud.rs`, `shell/src/view.rs` | Camera, bindings, HUD, arena and dummy visuals. |
| `shell/src/menu.rs` | Title screen, singleplayer/multiplayer picker, staged loading screen. |
| `shell/src/props.rs` | `avian3d` physics: world colliders, the player's push body, crates and balls you can grab (`E`) and throw (click). |
| `launcher/` | First-run setup wizard (OS, distro/Windows version, shortcuts, dependency command) and Play button. |

## License

The code is under the [MIT License](LICENSE.md). The provenance note at the
top stands: nothing from any game's files is in this repository.
