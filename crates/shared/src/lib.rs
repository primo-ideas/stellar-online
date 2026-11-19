//! Types shared by the server and the client: the network protocol.
//!
//! Wire format: one JSON message per line over TCP (newline-delimited JSON).

use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};

pub mod world;
pub use world::{Block, BlockKind, BlockPos, PlayerState, Sector, Ship};

/// Game version, shared by every crate of the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Protocol version. Bumped when messages change incompatibly.
pub const PROTOCOL_VERSION: u32 = 1;

/// Default TCP port of a galaxy server.
pub const DEFAULT_PORT: u16 = 7777;

/// Messages sent by the client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMsg {
    Hello {
        protocol: u32,
        player: String,
    },
    Ping {
        nonce: u64,
    },
    /// The player's current position. No reply.
    Move {
        state: PlayerState,
    },
    /// Creates a new ship (a single core) owned by the player.
    SpawnShip {
        name: String,
        sector: Sector,
    },
    /// Lists the ships in a sector.
    ListShips {
        sector: Sector,
    },
    PlaceBlock {
        ship: u64,
        pos: BlockPos,
        kind: BlockKind,
    },
    RemoveBlock {
        ship: u64,
        pos: BlockPos,
    },
    Bye,
}

/// Messages sent by the server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMsg {
    Welcome {
        galaxy: GalaxyInfo,
        server_version: String,
        /// Where the player was when they last left (or the spawn point).
        you: PlayerState,
    },
    Pong {
        nonce: u64,
    },
    /// Current state of a ship, after a spawn or an edit.
    ShipState {
        ship: Ship,
    },
    Ships {
        sector: Sector,
        ships: Vec<Ship>,
    },
    Error {
        message: String,
    },
}

/// Public description of a galaxy (one server = one galaxy).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GalaxyInfo {
    pub id: u64,
    pub name: String,
    pub known_players: usize,
}

/// Writes one message as a JSON line.
pub fn send<T: Serialize>(w: &mut impl Write, msg: &T) -> io::Result<()> {
    let mut line = serde_json::to_string(msg)?;
    line.push('\n');
    w.write_all(line.as_bytes())?;
    w.flush()
}

/// Reads one JSON-line message. Returns `Ok(None)` at end of stream.
pub fn recv<T: for<'de> Deserialize<'de>>(r: &mut impl BufRead) -> io::Result<Option<T>> {
    let mut line = String::new();
    if r.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    serde_json::from_str(line.trim_end())
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let mut buf = Vec::new();
        send(&mut buf, &ClientMsg::Ping { nonce: 42 }).unwrap();
        let msg: ClientMsg = recv(&mut buf.as_slice()).unwrap().unwrap();
        assert_eq!(msg, ClientMsg::Ping { nonce: 42 });
    }
}
