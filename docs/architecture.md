# Architecture

## Crates

| Crate | Rôle |
|---|---|
| `crates/shared` (`stellar-shared`) | Protocole réseau commun : messages, `send`/`recv`, constantes (version, port). |
| `crates/server` (`stellar-server`) | Serveur de galaxie : bibliothèque (`Galaxy`, `Server`) + binaire. |
| `crates/client` (`stellar-client`) | Client joueur graphique (binaire `stellar-client`), outil texte `stellar-cli`, et bibliothèque commune (connexion, thread réseau, picking). |

Toutes les crates partagent la version du workspace (`Cargo.toml`, `[workspace.package]`).

## Galaxie = un serveur = un fichier

Un serveur gère exactement une galaxie, stockée dans un fichier de sauvegarde
(`<nom>.galaxy.json`). Le fichier contient l'identifiant de la galaxie, son nom, sa date de
création et les joueurs connus. Sauvegarde atomique : écriture dans `.tmp` puis renommage.

Décision : JSON pour commencer (lisible, débogable). On passera à un format binaire et/ou
découpé par secteur quand le monde contiendra des blocs.

Vision : plusieurs serveurs (galaxies), éventuellement de propriétaires différents, seront
reliés par des warpgates ; franchir une warpgate transfère le joueur sur l'autre serveur.
L'`id` de galaxie sert déjà à identifier une galaxie de façon unique pour ces liens.

## Monde : secteurs, vaisseaux, blocs

- **Secteur** : case d'une grille 3D entière (`Sector {x, y, z}`) dans la galaxie.
- **Vaisseau** (`Ship`) : entité située dans un secteur, appartenant à un joueur, faite de
  blocs aux positions entières relatives à son core.
- **Blocs** (`BlockKind`) : `Core`, `Hull`, `Thruster`, `Reactor` (pas encore d'effet).
- Règles de construction (`Ship::place` / `Ship::remove`, dans `stellar-shared` pour être
  utilisables par le client plus tard) : un seul core, non retirable ; un bloc posé doit
  toucher un bloc existant par une face ; case libre. La séparation d'un vaisseau coupé en
  deux n'est pas encore gérée.
- Les vaisseaux sont stockés dans la galaxie (`ships`, `next_ship_id`). Sauvegarde du fichier
  entier après chaque modification — suffisant pour l'instant, à remplacer par une sauvegarde
  par secteur quand le monde grossira.

## Joueurs et déplacements

- `PlayerState` (`stellar-shared`) : secteur, position (flottants, 1 bloc = 1), `yaw`, `pitch`.
- Le client simule le vol libre localement et envoie `Move` au plus 10 fois par seconde, et
  une dernière fois en quittant. Le serveur fait confiance au client (pas de contrôle de
  vitesse) mais rejette les valeurs hors secteur (±1000) ou hors bornes.
- Le serveur garde les positions en mémoire (`player_states` de la galaxie) et sauvegarde le
  fichier quand un joueur se déconnecte, pas à chaque `Move`. Le `Welcome` renvoie la position
  enregistrée.
- Les autres joueurs ne sont pas encore visibles : il faudra que le serveur diffuse les
  positions (itération temps réel).

## Client graphique

- Moteur : **Bevy 0.20** (features `3d` + `ui`, sans audio). Choisi pour durer : éclairage,
  ombres, ECS, UI. Contrepartie : compilation lourde ; les dépendances sont optimisées même
  en debug (`[profile.dev.package."*"] opt-level = 2`), et la CI Linux installe `libudev`,
  `libwayland` et `libxkbcommon`.
- Le rendu ne bloque jamais sur le réseau : `Session::into_net` lance un thread d'écriture
  (canal `tx`) et un thread de lecture (canal `rx`), vidé à chaque frame par le système
  `receive`. Ressource Bevy `Server` (le `Receiver` n'étant pas `Sync`, il est sous `Mutex`).
- État du jeu côté client : ressource `Game`. Systèmes enchaînés à chaque frame :
  `receive` → `pause` → `fly` → `send_move` → `update_camera` → `build` → `rebuild_ships`
  → `hud`.
- Caméra à la première personne, à la position du joueur. Souris capturée
  (`CursorGrabMode::Locked`) : `AccumulatedMouseMotion` oriente toujours la vue.
- Clavier : `KeyCode` de Bevy = touches physiques, donc WASD tombe sur ZQSD en AZERTY.
- Au démarrage : liste les vaisseaux du secteur (0,0,0) et crée un vaisseau au joueur s'il
  n'en a pas.
- Les vaisseaux n'ont pas encore de position dans leur secteur : ils sont alignés sur l'axe X
  (`ship_origin`, espacement `SHIP_SPACING`).
- Picking : rayon partant de l'œil dans la direction du regard (viseur au centre), puis
  `raycast` (slab test sur chaque cube, testé unitairement) donne le bloc visé et la face ;
  le nouveau bloc va en `bloc + normale`.
- Un bloc = une entité (cube partagé, un matériau par type de bloc), toutes recréées quand un
  vaisseau change : suffisant pour quelques centaines de blocs. À remplacer par un maillage
  par vaisseau (faces visibles seulement). Bloc visé et emplacement de pose surlignés avec
  des gizmos.
- Lumière directionnelle avec ombres + lumière ambiante.

## Modèle serveur

`std::net` bloquant, un thread par connexion, état partagé derrière un `Mutex`. Volontairement
simple et sans dépendance async ; à revoir quand il y aura une boucle de simulation à tick fixe.
