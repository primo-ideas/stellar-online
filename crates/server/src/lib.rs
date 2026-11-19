//! Galaxy server: owns one galaxy save file and serves players over TCP.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, BufReader};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use stellar_shared::{
    recv, send, ClientMsg, GalaxyInfo, PlayerState, ServerMsg, Ship, PROTOCOL_VERSION, VERSION,
};

/// A galaxy: the whole persistent state of one server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Galaxy {
    pub id: u64,
    pub name: String,
    pub created_at: u64,
    pub players: BTreeSet<String>,
    /// `default`: fields added later must not break existing saves.
    #[serde(default)]
    pub ships: Vec<Ship>,
    #[serde(default)]
    pub next_ship_id: u64,
    /// Where each player is.
    #[serde(default)]
    pub player_states: BTreeMap<String, PlayerState>,
}

impl Galaxy {
    pub fn new(name: &str) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        Galaxy {
            // Not cryptographic: only needs to differ between galaxies.
            id: now.as_nanos() as u64 ^ std::process::id() as u64,
            name: name.to_string(),
            created_at: now.as_secs(),
            players: BTreeSet::new(),
            ships: Vec::new(),
            next_ship_id: 0,
            player_states: BTreeMap::new(),
        }
    }

    /// Applies a game command from `player`. Returns the reply and whether the
    /// galaxy changed (and must be saved).
    pub fn command(&mut self, player: &str, msg: ClientMsg) -> (ServerMsg, bool) {
        let err = |message: String| (ServerMsg::Error { message }, false);
        match msg {
            ClientMsg::SpawnShip { name, sector } => {
                self.next_ship_id += 1;
                let ship = Ship::new(self.next_ship_id, &name, player, sector);
                self.ships.push(ship.clone());
                (ServerMsg::ShipState { ship }, true)
            }
            ClientMsg::ListShips { sector } => {
                let ships = self
                    .ships
                    .iter()
                    .filter(|s| s.sector == sector)
                    .cloned()
                    .collect();
                (ServerMsg::Ships { sector, ships }, false)
            }
            ClientMsg::PlaceBlock { ship, pos, kind } => match self.owned_ship(player, ship) {
                Ok(s) => match s.place(pos, kind) {
                    Ok(()) => (ServerMsg::ShipState { ship: s.clone() }, true),
                    Err(e) => err(e),
                },
                Err(e) => err(e),
            },
            ClientMsg::RemoveBlock { ship, pos } => match self.owned_ship(player, ship) {
                Ok(s) => match s.remove(pos) {
                    Ok(_) => (ServerMsg::ShipState { ship: s.clone() }, true),
                    Err(e) => err(e),
                },
                Err(e) => err(e),
            },
            other => err(format!("unexpected message {other:?}")),
        }
    }

    fn owned_ship(&mut self, player: &str, id: u64) -> Result<&mut Ship, String> {
        let ship = self
            .ships
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| format!("no ship #{id}"))?;
        if ship.owner != player {
            return Err(format!("ship #{id} belongs to {}", ship.owner));
        }
        Ok(ship)
    }

    /// Loads the galaxy at `path`, or creates and saves a new one.
    pub fn load_or_create(path: &Path, name: &str) -> io::Result<Self> {
        if path.exists() {
            let data = std::fs::read_to_string(path)?;
            serde_json::from_str(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        } else {
            let g = Galaxy::new(name);
            g.save(path)?;
            Ok(g)
        }
    }

    /// Saves atomically (write to a temp file, then rename).
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(tmp, path)
    }

    pub fn info(&self) -> GalaxyInfo {
        GalaxyInfo {
            id: self.id,
            name: self.name.clone(),
            known_players: self.players.len(),
        }
    }
}

/// Shared server state.
pub struct Server {
    galaxy: Mutex<Galaxy>,
    save_path: PathBuf,
}

impl Server {
    pub fn new(galaxy: Galaxy, save_path: PathBuf) -> Arc<Self> {
        Arc::new(Server {
            galaxy: Mutex::new(galaxy),
            save_path,
        })
    }

    /// Accepts connections forever, one thread per player.
    pub fn run(self: Arc<Self>, listener: TcpListener) {
        for stream in listener.incoming().flatten() {
            let server = Arc::clone(&self);
            thread::spawn(move || {
                let peer = stream.peer_addr().ok();
                if let Err(e) = server.handle(stream) {
                    eprintln!("[server] {peer:?}: {e}");
                }
            });
        }
    }

    fn handle(&self, stream: TcpStream) -> io::Result<()> {
        let mut w = stream.try_clone()?;
        let mut r = BufReader::new(stream);
        let player = match recv::<ClientMsg>(&mut r)? {
            Some(ClientMsg::Hello { protocol, player }) if protocol == PROTOCOL_VERSION => player,
            Some(ClientMsg::Hello { protocol, .. }) => {
                let message =
                    format!("protocol {protocol} unsupported, server speaks {PROTOCOL_VERSION}");
                return send(&mut w, &ServerMsg::Error { message });
            }
            _ => {
                let message = "expected Hello".to_string();
                return send(&mut w, &ServerMsg::Error { message });
            }
        };
        let info = {
            let mut g = self.galaxy.lock().unwrap();
            if g.players.insert(player.clone()) {
                g.save(&self.save_path)?;
            }
            let you = g.player_states.get(&player).cloned().unwrap_or_default();
            (g.info(), you)
        };
        let (info, you) = info;
        println!("[server] {player} joined galaxy '{}'", info.name);
        let welcome = ServerMsg::Welcome {
            galaxy: info,
            server_version: VERSION.into(),
            you,
        };
        send(&mut w, &welcome)?;
        let result = self.session(&player, &mut r, &mut w);
        // Positions are only kept in memory while playing; persist them on leave.
        self.galaxy.lock().unwrap().save(&self.save_path)?;
        println!("[server] {player} left");
        result
    }

    fn session(
        &self,
        player: &str,
        r: &mut BufReader<TcpStream>,
        w: &mut TcpStream,
    ) -> io::Result<()> {
        while let Some(msg) = recv::<ClientMsg>(r)? {
            match msg {
                ClientMsg::Ping { nonce } => send(w, &ServerMsg::Pong { nonce })?,
                // No reply: sent ~10 times per second by the client.
                ClientMsg::Move { state } => {
                    if state.is_valid() {
                        let mut g = self.galaxy.lock().unwrap();
                        g.player_states.insert(player.to_string(), state);
                    }
                }
                ClientMsg::Bye => break,
                ClientMsg::Hello { .. } => {
                    let message = "already greeted".to_string();
                    send(w, &ServerMsg::Error { message })?
                }
                cmd => {
                    let reply = {
                        let mut g = self.galaxy.lock().unwrap();
                        let (reply, dirty) = g.command(player, cmd);
                        if dirty {
                            g.save(&self.save_path)?;
                        }
                        reply
                    };
                    send(w, &reply)?
                }
            }
        }
        Ok(())
    }
}
