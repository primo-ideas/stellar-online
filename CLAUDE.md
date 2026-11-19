# CLAUDE.md — stellar-online

Mémoire du projet pour les sessions Claude. La doc technique est dans [docs/](docs/README.md).

## Contexte

Jeu de construction spatiale par blocs, inspiré de StarMade (« un Minecraft dans l'espace »).
Projet original : on reprend le genre et les mécaniques, pas le contenu de StarMade.

## Règles et décisions de l'utilisateur

- 2026-11-19 — Tout en Rust (serveur, client, outils).
- 2026-11-19 — Crédits limités : travailler par **petites itérations**. Fin de chaque itération :
  tester, bumper la version **mineure** (`[workspace.package] version` dans `Cargo.toml`),
  committer et pousser sur `main`.
- 2026-11-19 — Un fichier de sauvegarde géré par un serveur = une **galaxie**.
- 2026-11-19 — Plus tard : relier des serveurs entre eux (warpgates…) pour faire passer un
  joueur d'une galaxie à une autre, sur un serveur distant appartenant à quelqu'un d'autre,
  selon des règles à définir.
- 2026-11-19 — Déploiement sur serveur : plus tard, manuel (pas depuis la CI).
- 2026-11-19 — Client graphique avec **Bevy** (l'utilisateur l'a demandé après un premier jet
  macroquad). Le premier jet a été fusionné dans un seul commit v0.1.0.
- 2026-11-19 — C'est un jeu : la souris pilote la caméra en permanence (souris capturée,
  style FPS), sans bouton à maintenir.
- Tests de l'interface : ne jamais simuler clavier/souris sur le PC de l'utilisateur (une
  touche est déjà partie dans une autre application). Captures d'écran seulement, sinon
  demander à l'utilisateur d'essayer.

## État actuel (v0.1.0)

- Workspace Cargo : `stellar-shared` (protocole + modèle du monde), `stellar-server`,
  `stellar-client` (client Bevy + outil texte `stellar-cli`).
- Serveur TCP, JSON par ligne, un thread par joueur ; charge/crée la galaxie
  (`*.galaxy.json`) et y enregistre joueurs, positions et vaisseaux. Protocole v1.
- Monde : secteurs (grille entière), vaisseaux de blocs (core/hull/thruster/reactor) avec
  règles de construction ; seul le propriétaire modifie son vaisseau.
- Client Bevy : vol libre à la première personne (WASD = ZQSD en AZERTY, touches physiques ;
  Espace/C, Shift), souris capturée, viseur central ; clic gauche pose, clic droit retire,
  Échap = pause. Éclairage directionnel + ombres. Position envoyée au serveur (`Move`),
  sauvegardée à la déconnexion, restaurée au `Welcome`. Les autres joueurs ne sont pas encore
  affichés.
- `run.ps1` à la racine : build + serveur local + client(s) graphique(s) (voir docs/build.md).

## Pistes pour les prochaines itérations

1. Synchronisation temps réel (tick serveur, diffusion des positions et des changements de
   blocs aux autres joueurs du secteur — aujourd'hui requête/réponse).
2. Rendu : maillage par vaisseau (faces visibles seulement) au lieu d'une entité par bloc,
   choix du secteur, position des vaisseaux dans le secteur (aujourd'hui alignés sur X).
3. Stations, séparation d'un vaisseau coupé en deux, effets des blocs (poussée, énergie).
4. Warpgates inter-serveurs.
