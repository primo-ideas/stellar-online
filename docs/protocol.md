# Protocole réseau

TCP, un message JSON par ligne (`\n`), objets étiquetés par un champ `type`.
Défini dans `crates/shared/src/lib.rs`. Port par défaut : `7777`.
`PROTOCOL_VERSION` (actuellement `1`) est incrémenté à chaque changement incompatible.

Après le handshake, chaque message du client reçoit exactement une réponse, sauf `Move`
(aucune réponse). Le client graphique lit et écrit sur deux threads séparés, donc il ne
suppose plus d'appariement requête/réponse.

Les nombres non finis (NaN, infini) ne passent pas en JSON (`null`) : un tel message est
rejeté comme malformé et le serveur ferme la connexion.
Types du monde (`Sector`, `Ship`, `Block`, `BlockKind`) : `crates/shared/src/world.rs`.

## Client → serveur

| Message | Champs | Rôle | Réponse |
|---|---|---|---|
| `Hello` | `protocol`, `player` | Premier message obligatoire. | `Welcome` / `Error` |
| `Ping` | `nonce` | Mesure de latence. | `Pong` |
| `Move` | `state {sector, pos, yaw, pitch}` | Position du joueur (~10/s). Ignoré si invalide (hors secteur, pitch hors bornes). | aucune |
| `SpawnShip` | `name`, `sector {x,y,z}` | Crée un vaisseau (un core) au joueur. | `ShipState` |
| `ListShips` | `sector` | Vaisseaux d'un secteur. | `Ships` |
| `PlaceBlock` | `ship`, `pos [x,y,z]`, `kind` | Ajoute un bloc (propriétaire seulement). | `ShipState` / `Error` |
| `RemoveBlock` | `ship`, `pos` | Retire un bloc (propriétaire seulement). | `ShipState` / `Error` |
| `Bye` | — | Déconnexion propre. | — |

## Serveur → client

| Message | Champs | Rôle |
|---|---|---|
| `Welcome` | `galaxy {id, name, known_players}`, `server_version`, `you` (`PlayerState`) | Réponse à un `Hello` valide ; `you` = dernière position du joueur ou point d'apparition. |
| `Pong` | `nonce` | Réponse à `Ping`. |
| `ShipState` | `ship` | État complet d'un vaisseau après création/modification. |
| `Ships` | `sector`, `ships` | Liste des vaisseaux d'un secteur. |
| `Error` | `message` | Erreur (version de protocole, message inattendu). |

Exemple : `{"type":"Hello","protocol":1,"player":"bob"}`
