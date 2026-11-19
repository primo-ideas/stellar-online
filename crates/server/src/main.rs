use std::net::TcpListener;
use std::path::PathBuf;
use stellar_server::{Galaxy, Server};
use stellar_shared::{DEFAULT_PORT, VERSION};

/// Usage: stellar-server [save-file] [port] [galaxy-name]
fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let save = PathBuf::from(args.next().unwrap_or_else(|| "default.galaxy.json".into()));
    let port: u16 = args
        .next()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    let name = args.next().unwrap_or_else(|| "Milky Way".into());

    let galaxy = Galaxy::load_or_create(&save, &name)?;
    let listener = TcpListener::bind(("0.0.0.0", port))?;
    println!(
        "stellar-server {VERSION}: galaxy '{}' (id {:016x}) from {} on port {port}",
        galaxy.name,
        galaxy.id,
        save.display()
    );
    Server::new(galaxy, save).run(listener);
    Ok(())
}
