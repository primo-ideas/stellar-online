//! Text client (debug tool): joins a galaxy, then reads commands from stdin.

use std::io::BufRead;
use std::time::Instant;
use stellar_shared::{
    recv, send, BlockKind, ClientMsg, Sector, ServerMsg, Ship, DEFAULT_PORT, VERSION,
};

const HELP: &str = "commands:
  spawn <name> [x y z]          new ship (one core) in a sector (default 0 0 0)
  ships [x y z]                 list ships in a sector
  place <ship> <x> <y> <z> <kind>   kind: hull | thruster | reactor
  remove <ship> <x> <y> <z>
  ping | help | quit";

/// Usage: stellar-cli [player-name] [host:port]
fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let player = args.next().unwrap_or_else(|| "pilot".into());
    let addr = args
        .next()
        .unwrap_or_else(|| format!("127.0.0.1:{DEFAULT_PORT}"));

    println!("stellar-cli {VERSION}: connecting to {addr} as {player}");
    let session = stellar_client::connect(&player, &addr)?;
    let galaxy = &session.galaxy;
    println!(
        "Welcome to galaxy '{}' (id {:016x}, {} known players, server {})",
        galaxy.name, galaxy.id, galaxy.known_players, session.server_version
    );
    let (mut w, mut r) = (session.writer, session.reader);
    println!("{HELP}");

    for line in std::io::stdin().lock().lines() {
        let line = line?;
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.is_empty() {
            continue;
        }
        if matches!(words[0], "quit" | "exit") {
            break;
        }
        let msg = match parse(&words) {
            Ok(msg) => msg,
            Err(e) => {
                println!("{e}");
                continue;
            }
        };
        let start = Instant::now();
        send(&mut w, &msg)?;
        match recv::<ServerMsg>(&mut r)? {
            Some(reply) => print_reply(reply, start),
            None => {
                println!("server closed the connection");
                return Ok(());
            }
        }
    }
    send(&mut w, &ClientMsg::Bye)
}

fn parse(words: &[&str]) -> Result<ClientMsg, String> {
    let num = |i: usize| -> Result<i32, String> {
        words
            .get(i)
            .ok_or("missing argument")?
            .parse()
            .map_err(|_| format!("'{}' is not a number", words[i]))
    };
    let sector_at = |i: usize| -> Result<Sector, String> {
        if words.len() <= i {
            return Ok(Sector::default());
        }
        Ok(Sector {
            x: num(i)?,
            y: num(i + 1)?,
            z: num(i + 2)?,
        })
    };
    let ship = || -> Result<u64, String> {
        words
            .get(1)
            .ok_or("missing ship id")?
            .trim_start_matches('#')
            .parse()
            .map_err(|_| "bad ship id".to_string())
    };
    match words[0] {
        "ping" => Ok(ClientMsg::Ping { nonce: 1 }),
        "spawn" => Ok(ClientMsg::SpawnShip {
            name: words.get(1).ok_or("missing name")?.to_string(),
            sector: sector_at(2)?,
        }),
        "ships" => Ok(ClientMsg::ListShips {
            sector: sector_at(1)?,
        }),
        "place" => Ok(ClientMsg::PlaceBlock {
            ship: ship()?,
            pos: [num(2)?, num(3)?, num(4)?],
            kind: BlockKind::parse(words.get(5).ok_or("missing kind")?).ok_or("unknown kind")?,
        }),
        "remove" => Ok(ClientMsg::RemoveBlock {
            ship: ship()?,
            pos: [num(2)?, num(3)?, num(4)?],
        }),
        _ => Err(HELP.to_string()),
    }
}

fn print_reply(reply: ServerMsg, start: Instant) {
    match reply {
        ServerMsg::Pong { .. } => println!("ping: {:?}", start.elapsed()),
        ServerMsg::ShipState { ship } => print_ship(&ship),
        ServerMsg::Ships { sector, ships } => {
            println!("{} ship(s) in sector {sector}", ships.len());
            ships.iter().for_each(print_ship);
        }
        ServerMsg::Error { message } => println!("error: {message}"),
        other => println!("{other:?}"),
    }
}

fn print_ship(s: &Ship) {
    println!(
        "  #{} '{}' owner {} sector {} — {} block(s)",
        s.id,
        s.name,
        s.owner,
        s.sector,
        s.blocks.len()
    );
}
