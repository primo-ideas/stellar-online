# Build, test, lancement

Prérequis : Rust stable (`cargo`).

```bash
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
```

## Lancer

Le plus simple (Windows, PowerShell) : `run.ps1` compile, démarre un serveur local dans sa
propre fenêtre, attend qu'il écoute, ouvre le client graphique, et arrête le serveur quand
tous les clients sont fermés.

```powershell
./run.ps1                          # serveur + client "pilot" sur 127.0.0.1:7777
./run.ps1 -Player bob -Clients 2   # serveur + 2 clients (bob, bob2)
./run.ps1 -ServerOnly              # serveur seul, dans la console
./run.ps1 -ClientOnly -Address 1.2.3.4:7777 -Player bob
./run.ps1 -Release                 # build optimisé
```

Autres paramètres : `-Port`, `-Save` (fichier galaxie), `-Galaxy` (nom à la création).

À la main :

Serveur — `stellar-server [fichier-sauvegarde] [port] [nom-galaxie]`
(défauts : `default.galaxy.json`, `7777`, `Milky Way`) :

```bash
cargo run -p stellar-server -- andromeda.galaxy.json 7777 Andromeda
```

Client graphique — `stellar-client [joueur] [hôte:port]` :

```bash
cargo run -p stellar-client --bin stellar-client -- bob 127.0.0.1:7777
```

Contrôles (vol libre à la première personne, souris capturée comme dans un FPS) :
la souris oriente la vue, on vise avec le viseur au centre ; WASD ou ZQSD (AZERTY) =
avancer/gauche/reculer/droite, Espace / C = monter / descendre, Shift = boost, Tab = se placer
devant le vaisseau suivant, 1/2/3 = hull/thruster/reactor, clic gauche = poser un bloc sur la
face visée (portée 40), clic droit = retirer le bloc visé, Échap = pause (libère la souris ;
clic pour reprendre, Échap à nouveau pour quitter).

Outil texte (debug) — `stellar-cli [joueur] [hôte:port]` :

```bash
cargo run -p stellar-client --bin stellar-cli -- bob 127.0.0.1:7777
```

Commandes : `spawn <nom> [x y z]`, `ships [x y z]`,
`place <vaisseau> <x> <y> <z> <hull|thruster|reactor>`, `remove <vaisseau> <x> <y> <z>`,
`ping`, `help`, `quit`.

## CI

`.github/workflows/ci.yml` : fmt, clippy, build et tests à chaque push sur `main`. Pas de
déploiement depuis la CI.

## Versions

Chaque itération bumpe la version mineure dans `Cargo.toml` (`[workspace.package] version`).
