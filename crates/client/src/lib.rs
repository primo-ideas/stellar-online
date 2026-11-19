//! Client networking shared by the graphical client and the text CLI.

use std::io::{self, BufReader};
use std::net::TcpStream;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use stellar_shared::{
    recv, send, BlockPos, ClientMsg, GalaxyInfo, PlayerState, ServerMsg, Ship, PROTOCOL_VERSION,
};

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_shared::{BlockKind, Sector};

    #[test]
    fn raycast_hits_nearest_face() {
        let mut ship = Ship::new(1, "t", "a", Sector::default());
        ship.place([1, 0, 0], BlockKind::Hull).unwrap();
        // Looking along -x from the right: hits block [1,0,0] on its +x face.
        let hit = raycast(&[ship.clone()], [10.0, 0.2, 0.1], [-1.0, 0.0, 0.0]).unwrap();
        assert_eq!((hit.block, hit.normal), ([1, 0, 0], [1, 0, 0]));
        // Looking down from above at the core: +y face.
        let hit = raycast(&[ship.clone()], [0.1, 5.0, 0.0], [0.0, -1.0, 0.0]).unwrap();
        assert_eq!((hit.block, hit.normal), ([0, 0, 0], [0, 1, 0]));
        // Missing everything.
        assert!(raycast(&[ship], [0.0, 5.0, 0.0], [0.0, 1.0, 0.0]).is_none());
    }
}

/// Distance between ships laid out side by side in a sector (ships have no position
/// inside their sector yet).
pub const SHIP_SPACING: f32 = 24.0;

/// World-space origin of the `index`-th ship of a sector.
pub fn ship_origin(index: usize) -> [f32; 3] {
    [index as f32 * SHIP_SPACING, 0.0, 0.0]
}

/// Result of a ray hitting a block (unit cube centred on its position).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub ship: u64,
    pub block: BlockPos,
    /// Outward face normal: `block + normal` is where a new block would go.
    pub normal: BlockPos,
    pub t: f32,
}

/// Casts a ray against every block of `ships` (laid out with [`ship_origin`]) and
/// returns the closest hit.
pub fn raycast(ships: &[Ship], origin: [f32; 3], dir: [f32; 3]) -> Option<Hit> {
    let mut best: Option<Hit> = None;
    for (i, ship) in ships.iter().enumerate() {
        let o = ship_origin(i);
        for b in &ship.blocks {
            let c = [
                o[0] + b.pos[0] as f32,
                o[1] + b.pos[1] as f32,
                o[2] + b.pos[2] as f32,
            ];
            let (mut tmin, mut tmax) = (f32::NEG_INFINITY, f32::INFINITY);
            for a in 0..3 {
                let (lo, hi) = (c[a] - 0.5 - origin[a], c[a] + 0.5 - origin[a]);
                if dir[a].abs() < 1e-8 {
                    if lo > 0.0 || hi < 0.0 {
                        tmin = f32::INFINITY;
                    }
                    continue;
                }
                let (t1, t2) = (lo / dir[a], hi / dir[a]);
                tmin = tmin.max(t1.min(t2));
                tmax = tmax.min(t1.max(t2));
            }
            if tmin > tmax || tmax < 0.0 || best.is_some_and(|h| h.t <= tmin) {
                continue;
            }
            // The hit face is the axis where the hit point is furthest from the centre.
            let p: Vec<f32> = (0..3).map(|a| origin[a] + dir[a] * tmin - c[a]).collect();
            let axis = (0..3)
                .max_by(|&x, &y| p[x].abs().total_cmp(&p[y].abs()))
                .unwrap();
            let mut normal = [0; 3];
            normal[axis] = if p[axis] > 0.0 { 1 } else { -1 };
            best = Some(Hit {
                ship: ship.id,
                block: b.pos,
                normal,
                t: tmin,
            });
        }
    }
    best
}

/// A connected, greeted session.
pub struct Session {
    pub galaxy: GalaxyInfo,
    pub server_version: String,
    /// The player's saved position, given by the server.
    pub you: PlayerState,
    pub writer: TcpStream,
    pub reader: BufReader<TcpStream>,
}

/// Connects to `addr` and performs the `Hello` / `Welcome` handshake.
pub fn connect(player: &str, addr: &str) -> io::Result<Session> {
    let stream = TcpStream::connect(addr)?;
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);
    let hello = ClientMsg::Hello {
        protocol: PROTOCOL_VERSION,
        player: player.to_string(),
    };
    send(&mut writer, &hello)?;
    match recv::<ServerMsg>(&mut reader)? {
        Some(ServerMsg::Welcome {
            galaxy,
            server_version,
            you,
        }) => Ok(Session {
            galaxy,
            server_version,
            you,
            writer,
            reader,
        }),
        other => Err(io::Error::other(format!("handshake failed: {other:?}"))),
    }
}

/// Non-blocking, full-duplex access to a session: one thread writes what is put in
/// `tx`, another forwards everything the server sends to `rx`. A render loop never
/// waits on the network, and messages need not be paired (e.g. `Move` has no reply).
pub struct Net {
    pub tx: Sender<ClientMsg>,
    pub rx: Receiver<ServerMsg>,
}

impl Session {
    pub fn into_net(self) -> Net {
        let (tx, cmd_rx) = channel::<ClientMsg>();
        let (msg_tx, rx) = channel::<ServerMsg>();
        let Session {
            mut writer,
            mut reader,
            ..
        } = self;
        thread::spawn(move || -> io::Result<()> {
            while let Ok(msg) = cmd_rx.recv() {
                let bye = msg == ClientMsg::Bye;
                send(&mut writer, &msg)?;
                if bye {
                    break;
                }
            }
            Ok(())
        });
        thread::spawn(move || -> io::Result<()> {
            while let Some(msg) = recv::<ServerMsg>(&mut reader)? {
                if msg_tx.send(msg).is_err() {
                    break;
                }
            }
            Ok(())
        });
        Net { tx, rx }
    }
}
