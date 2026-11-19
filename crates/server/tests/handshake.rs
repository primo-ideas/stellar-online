use std::io::BufReader;
use std::net::{TcpListener, TcpStream};
use stellar_server::{Galaxy, Server};
use stellar_shared::{
    recv, send, BlockKind, ClientMsg, PlayerState, Sector, ServerMsg, PROTOCOL_VERSION,
};

#[test]
fn hello_ping_bye_and_persistence() {
    let dir = std::env::temp_dir().join(format!("stellar-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let save = dir.join("test.galaxy.json");
    let _ = std::fs::remove_file(&save);

    let galaxy = Galaxy::load_or_create(&save, "Test Galaxy").unwrap();
    let id = galaxy.id;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = Server::new(galaxy, save.clone());
    std::thread::spawn(move || server.run(listener));

    let stream = TcpStream::connect(addr).unwrap();
    let mut w = stream.try_clone().unwrap();
    let mut r = BufReader::new(stream);
    let hello = ClientMsg::Hello {
        protocol: PROTOCOL_VERSION,
        player: "alice".into(),
    };
    send(&mut w, &hello).unwrap();
    match recv::<ServerMsg>(&mut r).unwrap().unwrap() {
        ServerMsg::Welcome { galaxy, .. } => {
            assert_eq!(galaxy.name, "Test Galaxy");
            assert_eq!(galaxy.known_players, 1);
        }
        other => panic!("unexpected {other:?}"),
    }
    send(&mut w, &ClientMsg::Ping { nonce: 7 }).unwrap();
    let pong = recv::<ServerMsg>(&mut r).unwrap().unwrap();
    assert_eq!(pong, ServerMsg::Pong { nonce: 7 });

    // Spawn a ship, extend it, list the sector.
    let sector = Sector { x: 1, y: 2, z: 3 };
    let spawn = ClientMsg::SpawnShip {
        name: "Arrow".into(),
        sector,
    };
    send(&mut w, &spawn).unwrap();
    let ship_id = match recv::<ServerMsg>(&mut r).unwrap().unwrap() {
        ServerMsg::ShipState { ship } => ship.id,
        other => panic!("unexpected {other:?}"),
    };
    let place = ClientMsg::PlaceBlock {
        ship: ship_id,
        pos: [0, 0, 1],
        kind: BlockKind::Thruster,
    };
    send(&mut w, &place).unwrap();
    assert!(matches!(
        recv::<ServerMsg>(&mut r).unwrap().unwrap(),
        ServerMsg::ShipState { ship } if ship.blocks.len() == 2
    ));
    send(&mut w, &ClientMsg::ListShips { sector }).unwrap();
    assert!(matches!(
        recv::<ServerMsg>(&mut r).unwrap().unwrap(),
        ServerMsg::Ships { ships, .. } if ships.len() == 1
    ));

    // Move (no reply), then an invalid move that must be ignored.
    let moved = PlayerState {
        sector: Sector::default(),
        pos: [1.0, 2.0, 3.0],
        yaw: 0.5,
        pitch: 0.1,
    };
    send(
        &mut w,
        &ClientMsg::Move {
            state: moved.clone(),
        },
    )
    .unwrap();
    let mut bad = moved.clone();
    bad.pos[0] = 1e9; // outside the sector
    send(&mut w, &ClientMsg::Move { state: bad }).unwrap();
    // Messages are handled in order: once Pong arrives, both moves were processed.
    send(&mut w, &ClientMsg::Ping { nonce: 8 }).unwrap();
    let pong = recv::<ServerMsg>(&mut r).unwrap().unwrap();
    assert_eq!(pong, ServerMsg::Pong { nonce: 8 });
    send(&mut w, &ClientMsg::Bye).unwrap();

    // Reconnecting puts the player back where they were.
    let stream = TcpStream::connect(addr).unwrap();
    let mut w = stream.try_clone().unwrap();
    let mut r = BufReader::new(stream);
    send(&mut w, &hello).unwrap();
    match recv::<ServerMsg>(&mut r).unwrap().unwrap() {
        ServerMsg::Welcome { you, .. } => assert_eq!(you, moved),
        other => panic!("unexpected {other:?}"),
    }
    send(&mut w, &ClientMsg::Bye).unwrap();

    // The player and the ship were persisted in the galaxy save file.
    let reloaded = Galaxy::load_or_create(&save, "ignored").unwrap();
    assert_eq!(reloaded.id, id);
    assert!(reloaded.players.contains("alice"));
    assert_eq!(reloaded.ships[0].blocks.len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}
