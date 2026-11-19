//! Graphical client (Bevy): the player flies freely in a sector and builds ships.
//!
//! The mouse is captured (FPS style): moving it turns the view, and the crosshair aims.
//! Controls (physical keys, so ZQSD on AZERTY): WASD = move, Space / C = up / down,
//! Shift = boost, Tab = jump to the next ship, 1/2/3 = hull/thruster/reactor,
//! left click = place block, right click = remove block,
//! Escape = pause (frees the mouse; click to resume, Escape again to quit).

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};
use std::collections::HashMap;
use std::sync::Mutex;
use stellar_client::{raycast, ship_origin, Net};
use stellar_shared::{
    BlockKind, ClientMsg, PlayerState, Sector, ServerMsg, Ship, DEFAULT_PORT, VERSION,
};

/// Flight speed in blocks per second, and the boost multiplier.
const SPEED: f32 = 8.0;
const BOOST: f32 = 4.0;
/// Mouse-look sensitivity in radians per pixel.
const LOOK: f32 = 0.003;
/// Blocks further than this cannot be edited.
const REACH: f32 = 40.0;
/// How often the position is sent to the server, in seconds.
const MOVE_PERIOD: f32 = 0.1;

/// Connection to the server (the receiver is not `Sync`, hence the mutex).
#[derive(Resource)]
struct Server(Mutex<Net>);

impl Server {
    fn send(&self, msg: ClientMsg) {
        let _ = self.0.lock().unwrap().tx.send(msg);
    }
}

/// Client-side game state.
#[derive(Resource)]
struct Game {
    player: String,
    galaxy: String,
    sector: Sector,
    me: PlayerState,
    ships: Vec<Ship>,
    /// The ship entities must be rebuilt from `ships`.
    ships_dirty: bool,
    listed: bool,
    next_ship: usize,
    kind: BlockKind,
    status: String,
    paused: bool,
    sent: PlayerState,
    since_send: f32,
}

/// Shared mesh and materials for blocks.
#[derive(Resource)]
struct BlockAssets {
    cube: Handle<Mesh>,
    materials: HashMap<BlockKind, Handle<StandardMaterial>>,
}

#[derive(Component)]
struct BlockEntity;

#[derive(Component)]
struct Hud;

fn main() {
    let mut args = std::env::args().skip(1);
    let player = args.next().unwrap_or_else(|| "pilot".into());
    let addr = args
        .next()
        .unwrap_or_else(|| format!("127.0.0.1:{DEFAULT_PORT}"));
    let session = match stellar_client::connect(&player, &addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot join {addr}: {e}");
            std::process::exit(1);
        }
    };
    let me = session.you.clone();
    let game = Game {
        player,
        galaxy: session.galaxy.name.clone(),
        sector: me.sector,
        sent: me.clone(),
        me,
        ships: Vec::new(),
        ships_dirty: false,
        listed: false,
        next_ship: 0,
        kind: BlockKind::Hull,
        status: "connected".into(),
        paused: false,
        since_send: 0.0,
    };
    let net = session.into_net();
    let _ = net.tx.send(ClientMsg::ListShips {
        sector: game.sector,
    });

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: format!("Stellar Online {VERSION}"),
                resolution: (1280, 720).into(),
                ..default()
            }),
            primary_cursor_options: Some(CursorOptions {
                visible: false,
                grab_mode: CursorGrabMode::Locked,
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgb(0.02, 0.02, 0.06)))
        .insert_resource(GlobalAmbientLight {
            brightness: 300.0,
            ..default()
        })
        .insert_resource(Server(Mutex::new(net)))
        .insert_resource(game)
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                receive,
                pause,
                fly,
                send_move,
                update_camera,
                build,
                rebuild_ships,
                hud,
            )
                .chain(),
        )
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let colors = [
        (BlockKind::Core, Color::srgb(0.3, 0.75, 1.0)),
        (BlockKind::Hull, Color::srgb(0.6, 0.62, 0.66)),
        (BlockKind::Thruster, Color::srgb(1.0, 0.55, 0.15)),
        (BlockKind::Reactor, Color::srgb(0.45, 0.9, 0.35)),
    ];
    commands.insert_resource(BlockAssets {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        materials: colors
            .into_iter()
            .map(|(k, c)| (k, materials.add(c)))
            .collect(),
    });
    commands.spawn((Camera3d::default(), Transform::default()));
    commands.spawn((
        DirectionalLight {
            illuminance: 8000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 10.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(12),
            left: px(12),
            ..default()
        },
        children![(Hud, Text::new(""), TextFont::from_font_size(18.0))],
    ));
    // Crosshair.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: percent(50),
            top: percent(50),
            width: px(4),
            height: px(4),
            ..default()
        },
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.7)),
    ));
}

fn v(a: [f32; 3]) -> Vec3 {
    Vec3::from_array(a)
}

fn vi(a: [i32; 3]) -> Vec3 {
    Vec3::new(a[0] as f32, a[1] as f32, a[2] as f32)
}

/// Unit vector the player looks along.
fn forward(me: &PlayerState) -> Vec3 {
    Vec3::new(
        me.pitch.cos() * me.yaw.cos(),
        me.pitch.sin(),
        me.pitch.cos() * me.yaw.sin(),
    )
}

/// Applies server messages to the game state.
fn receive(server: Res<Server>, mut game: ResMut<Game>) {
    let msgs: Vec<ServerMsg> = server.0.lock().unwrap().rx.try_iter().collect();
    for msg in msgs {
        match msg {
            ServerMsg::Ships { mut ships, .. } => {
                ships.sort_by_key(|s| s.id);
                if !game.listed && !ships.iter().any(|s| s.owner == game.player) {
                    let name = format!("{}'s ship", game.player);
                    let sector = game.sector;
                    server.send(ClientMsg::SpawnShip { name, sector });
                }
                game.ships = ships;
                game.listed = true;
                game.ships_dirty = true;
            }
            ServerMsg::ShipState { ship } => {
                match game.ships.iter_mut().find(|s| s.id == ship.id) {
                    Some(s) => *s = ship,
                    None => {
                        game.ships.push(ship);
                        game.ships.sort_by_key(|s| s.id);
                    }
                }
                game.ships_dirty = true;
            }
            ServerMsg::Error { message } => game.status = format!("error: {message}"),
            _ => {}
        }
    }
}

/// Escape pauses (frees the mouse); click resumes; Escape while paused quits.
fn pause(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut cursor: Single<&mut CursorOptions>,
    mut game: ResMut<Game>,
    server: Res<Server>,
    mut commands: Commands,
) {
    if game.paused {
        if keys.just_pressed(KeyCode::Escape) {
            server.send(ClientMsg::Move {
                state: game.me.clone(),
            });
            server.send(ClientMsg::Bye);
            // Let the writer thread flush before the process exits.
            std::thread::sleep(std::time::Duration::from_millis(100));
            commands.write_message(AppExit::Success);
        } else if mouse.just_pressed(MouseButton::Left) {
            game.paused = false;
            cursor.visible = false;
            cursor.grab_mode = CursorGrabMode::Locked;
        }
    } else if keys.just_pressed(KeyCode::Escape) {
        game.paused = true;
        cursor.visible = true;
        cursor.grab_mode = CursorGrabMode::None;
    }
}

/// Mouse look and free flight.
fn fly(
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    time: Res<Time>,
    mut game: ResMut<Game>,
) {
    if game.paused {
        return;
    }
    let me = &mut game.me;
    me.yaw += motion.delta.x * LOOK;
    me.pitch =
        (me.pitch - motion.delta.y * LOOK).clamp(-PlayerState::MAX_PITCH, PlayerState::MAX_PITCH);

    let fwd = forward(me);
    let right = Vec3::new(-me.yaw.sin(), 0.0, me.yaw.cos());
    let mut dir = Vec3::ZERO;
    for (key, d) in [
        (KeyCode::KeyW, fwd),
        (KeyCode::KeyS, -fwd),
        (KeyCode::KeyD, right),
        (KeyCode::KeyA, -right),
        (KeyCode::Space, Vec3::Y),
        (KeyCode::KeyC, -Vec3::Y),
    ] {
        if keys.pressed(key) {
            dir += d;
        }
    }
    if dir != Vec3::ZERO {
        let boost = if keys.pressed(KeyCode::ShiftLeft) {
            BOOST
        } else {
            1.0
        };
        let max = Vec3::splat(PlayerState::SECTOR_HALF_SIZE);
        let p = v(me.pos) + dir.normalize() * SPEED * boost * time.delta_secs();
        me.pos = p.clamp(-max, max).to_array();
    }

    if keys.just_pressed(KeyCode::Tab) && !game.ships.is_empty() {
        // Jump in front of the next ship, looking at its core.
        let i = game.next_ship % game.ships.len();
        let target = v(ship_origin(i));
        game.me.pos = (target + Vec3::new(6.0, 3.0, 8.0)).to_array();
        game.me.yaw = -2.214;
        game.me.pitch = -0.29;
        game.next_ship = i + 1;
    }
    for (key, kind) in [
        (KeyCode::Digit1, BlockKind::Hull),
        (KeyCode::Digit2, BlockKind::Thruster),
        (KeyCode::Digit3, BlockKind::Reactor),
    ] {
        if keys.just_pressed(key) {
            game.kind = kind;
        }
    }
}

/// Sends the position at most every `MOVE_PERIOD` seconds, when it changed.
fn send_move(time: Res<Time>, server: Res<Server>, mut game: ResMut<Game>) {
    game.since_send += time.delta_secs();
    if game.me != game.sent && game.since_send >= MOVE_PERIOD {
        server.send(ClientMsg::Move {
            state: game.me.clone(),
        });
        game.sent = game.me.clone();
        game.since_send = 0.0;
    }
}

fn update_camera(game: Res<Game>, mut camera: Single<&mut Transform, With<Camera3d>>) {
    let eye = v(game.me.pos);
    **camera = Transform::from_translation(eye).looking_to(forward(&game.me), Vec3::Y);
}

/// Highlights the aimed block; left click places, right click removes.
fn build(
    mouse: Res<ButtonInput<MouseButton>>,
    server: Res<Server>,
    mut game: ResMut<Game>,
    mut gizmos: Gizmos,
) {
    if game.paused {
        return;
    }
    let Some(hit) =
        raycast(&game.ships, game.me.pos, forward(&game.me).to_array()).filter(|h| h.t <= REACH)
    else {
        return;
    };
    let i = game
        .ships
        .iter()
        .position(|s| s.id == hit.ship)
        .unwrap_or(0);
    let centre = v(ship_origin(i)) + vi(hit.block);
    let outline = |c: Vec3, size: f32| Transform::from_translation(c).with_scale(Vec3::splat(size));
    gizmos.cube(outline(centre, 1.02), Color::srgb(1.0, 0.9, 0.1));
    gizmos.cube(
        outline(centre + vi(hit.normal), 0.98),
        Color::srgba(1.0, 1.0, 1.0, 0.4),
    );

    let msg = if mouse.just_pressed(MouseButton::Left) {
        let pos = [0, 1, 2].map(|a| hit.block[a] + hit.normal[a]);
        ClientMsg::PlaceBlock {
            ship: hit.ship,
            pos,
            kind: game.kind,
        }
    } else if mouse.just_pressed(MouseButton::Right) {
        ClientMsg::RemoveBlock {
            ship: hit.ship,
            pos: hit.block,
        }
    } else {
        return;
    };
    game.status.clear();
    server.send(msg);
}

/// Respawns every block entity when the ships changed (fine for small ships).
fn rebuild_ships(
    mut commands: Commands,
    mut game: ResMut<Game>,
    assets: Res<BlockAssets>,
    blocks: Query<Entity, With<BlockEntity>>,
) {
    if !game.ships_dirty {
        return;
    }
    game.ships_dirty = false;
    for e in &blocks {
        commands.entity(e).despawn();
    }
    for (i, ship) in game.ships.iter().enumerate() {
        let origin = v(ship_origin(i));
        for b in &ship.blocks {
            commands.spawn((
                BlockEntity,
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(assets.materials[&b.kind].clone()),
                Transform::from_translation(origin + vi(b.pos)),
            ));
        }
    }
}

fn hud(game: Res<Game>, mut text: Single<&mut Text, With<Hud>>) {
    let p = game.me.pos;
    let mut s = format!(
        "Galaxy '{}' - sector {} - {} ship(s)\n\
         {} at ({:.1}, {:.1}, {:.1})\n\
         Block: {:?}   [1] hull [2] thruster [3] reactor\n\
         WASD/ZQSD move - Space/C up/down - Shift boost - mouse look - Tab next ship\n\
         LMB place - RMB remove - Esc pause\n{}",
        game.galaxy,
        game.sector,
        game.ships.len(),
        game.player,
        p[0],
        p[1],
        p[2],
        game.kind,
        game.status
    );
    if game.paused {
        s.push_str("\n\nPAUSED - click to resume, Esc to quit");
    }
    text.0 = s;
}
