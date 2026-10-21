# Olympe

Moteur libre, écrit en Rust, pour *Zeus : le Maître de l'Olympe* et son extension
*Poséidon* (Impressions Games, 2000-2001). Il lit les données du jeu d'origine et les
fait tourner sur une machine d'aujourd'hui : toute définition d'écran, zoom libre,
interface agrandie sur les écrans à haute définition.

**Olympe ne contient aucune donnée du jeu.** Il vous faut votre propre exemplaire :
un jeu déjà installé, les CD, ou leurs images.

## État du projet

Olympe est une réécriture en cours, pas encore un remplaçant complet du jeu.

Ce qui fonctionne :

- installation des données depuis un jeu installé, le contenu d'un CD ou une image de CD ;
- écran de choix parmi les 40 aventures, avec leur récit d'ouverture ;
- affichage de la carte, zoom, mini-carte, mer animée ;
- construction : routes, logements modestes et luxueux, agoras et échoppes, fermes,
  ateliers, entrepôt, grenier, culture, palais, stade, embellissements ;
- simulation : immigrants, emploi, tournées des marcheurs, vivres et marchandises,
  attrait des quartiers, évolution des logements, impôts, incendies et effondrements ;
- musique et bruitages d'ambiance d'origine ;
- objectifs de l'épisode, lus dans l'aventure (touche `O`) : population, trésor,
  logement, profit et production annuels sont suivis, et les marchandises à
  réserver pour la cité suivante se mettent de côté par la touche `R` (vivres du
  grenier ; laine, huile, vin, armement, bronze, bois, marbre et sculptures de
  l'entrepôt) ; l'épisode se termine sur son récit de victoire quand tous sont atteints ;
- sanctuaires des dieux que l'épisode désigne (catégorie Mythologie), disposés
  comme dans le jeu ; le chantier prend marbre, bois et sculptures à l'entrepôt
  (quantités provisoires), et les objectifs de sanctuaire sont suivis ; achevés, ceux
  d'Athéna, de Dionysos et d'Héphaïstos apportent olives, raisin et bronze, et
  chaque année le dieu vient en personne laisser un bienfait (choix d'Olympe) ;
- héros, quêtes et monstres : temple de chaque héros (catégorie Mythologie), avec
  ses exigences lues dans le jeu ; un héros venu dans la cité affronte le monstre
  qui lui revient puis part pour sa quête, et les objectifs correspondants sont
  suivis ; les monstres entrent dans la cité, où ils détruisent un bâtiment tous les
  deux mois jusqu'à ce que leur héros, venu de son temple, les tue (les monstres
  marins restent au large) ; un monstre lâché ou envahisseur arrive à sa date ;
- dieux hostiles en personne : à la date de leur événement, ils entrent dans la
  cité et la ravagent six mois, un seul si un dieu favorable y a son sanctuaire
  (durées et rythme d'Olympe) ;
- soldats fournis par les logements (piétaille, hoplites, cavaliers, selon les
  chiffres de l'éditeur d'aventures) pour les objectifs militaires, sans trières ;
- cités du monde lues dans la carte (statut, force militaire) ; panneau `C`, d'où
  l'on soumet une cité en un clic quand l'armée est assez forte (simple comparaison
  des forces, sans bataille), ce qui suit les objectifs « Régner », et l'on y ouvre
  des routes commerciales : chaque mois, on vend à la cité ce qu'elle achète au-delà
  d'une réserve et l'on achète ce qu'elle vend quand les réserves sont basses
  (marchandises et quantités lues dans la carte, prix d'Olympe) ; les vassales
  paient un tribut annuel en drachmes (montant d'Olympe) ; les cités rivales
  envoient leurs armées à la date prévue : repoussées si l'armée de la cité
  l'emporte en force, sinon elles pillent le trésor et brûlent des bâtiments ;
  les événements de l'épisode changent le monde à leur date (une cité devient
  rivale, alliée, vassale, se rebelle, renforce ou perd son armée) et apportent les
  demandes des cités (marchandises, vivres, drachmes, soldats), que l'on satisfait
  depuis le panneau `C` : servir une cité accroît sa faveur, l'ignorer la fâche,
  jusqu'à la rendre rivale (barème d'Olympe) ;
- enchaînement des épisodes dans l'ordre de l'aventure, avec leurs récits, leurs
  objectifs et leurs ressources : départ pour une colonie sur sa propre carte, retour
  à la cité mère, fin de l'aventure ; la sauvegarde retient l'épisode en cours ;
- sauvegarde et rechargement d'une partie.

Ce qui manque encore : animations de combat, batailles et colonies (une colonie
part de sa carte vierge, le trésor suit le joueur), les objectifs qui en dépendent
; scierie et atelier de maçonnerie se posent sans forêt ni carrière, et les
marchandises mises de côté ne parviennent pas à la cité suivante ; conseillers,
voix, vidéos, et l'interface d'origine (celle d'Olympe est provisoire). Plusieurs
réglages d'équilibre (rendements, consommations) sont approchés et non repris du
jeu.

## Installation

Il faut :

- [Rust](https://www.rust-lang.org/fr/tools/install) 1.85 ou plus récent ;
- les bibliothèques **SDL3** et **Opus** avec leurs fichiers de développement
  (`SDL3-devel` et `libopus-devel`, ou `libsdl3-dev` et `libopus-dev`, selon le système) ;
- **ffmpeg**, facultatif, pour convertir la musique du jeu à l'installation des données.
  Sans lui, le jeu fonctionne mais sans musique.

Sous Linux et les autres Unix :

```sh
./installer.sh
```

Le script compile Olympe et pose le programme dans `~/.local/bin`, avec une entrée de
menu et une icône. Sur tout système, on peut aussi faire :

```sh
cargo install --path . --bin olympe
```

## Installer les données du jeu

```sh
olympe installer <source> [<source>…]
```

Une source est, au choix :

- le dossier où le jeu est installé ;
- un dossier contenant le contenu d'un CD ;
- une image de CD (`.iso`, `.bin`, brute ou non).

Avec les CD, donnez celui de Zeus puis celui de Poséidon :

```sh
olympe installer "CD zeus.iso" "CD poséidon.iso"
```

Un correctif officiel (le programme `mise_a_jour_2_1_fr_poseidon.exe`, par exemple) se
donne comme une source de plus, en dernier, ou seul après coup :

```sh
olympe installer mise_a_jour_2_1_fr_poseidon.exe
```

Olympe y trouve les fichiers de données qu'il corrige et les met à jour ; le programme
Windows lui-même n'est jamais exécuté.

Les données vont dans le dossier habituel du système (`~/.local/share/olympe` sous
Linux, `~/Library/Application Support/olympe` sous macOS, `%APPDATA%\olympe` sous
Windows), ou dans celui que désigne `--vers`. Comptez environ 700 Mo et une minute.

## Jouer

```sh
olympe                          # écran de choix d'aventure
olympe "les aventures de jason" # une aventure par son nom
olympe chemin/vers/carte.map    # une carte précise
```

En jeu, la touche `H` affiche les commandes. `olympe --aide` liste les options.

Les parties s'enregistrent avec `F5` et se rechargent avec `F9` ; elles sont rangées
dans le sous-dossier `sauvegardes` des données.

## Développement

```sh
cargo test --release
```

Les tests qui portent sur les données du jeu cherchent un dossier `donnees/` à la
racine du dépôt et ne font rien s'il est absent. Pour le créer :

```sh
olympe installer <sources> --vers donnees
```

Le code est commenté en français. Les formats du jeu d'origine (images, cartes,
aventures, textes, archives des CD) sont lus par des modules sans dépendance externe ;
seuls l'affichage et le son passent par SDL3 et Opus.

Outils de mise au point : `zeus-iso`, `zeus-cab`, `zeus-sprites`, `zeus-map`,
`zeus-text`, `zeus-jpeg`, `zeus-opus` (voir `src/bin/`). `olympe --mesure 600`
dessine 600 images sans synchronisation verticale et donne la cadence obtenue.

## Systèmes

Olympe est développé et essayé sous Linux (Wayland, rendu OpenGL par SDL3).

Sur la machine de développement (processeur graphique intégré), le jeu a été essayé
avec chaque combinaison que SDL3 propose : affichage Wayland ou X11, rendu OpenGL,
OpenGL ES 2, Vulkan, « gpu » de SDL, et rendu logiciel sans carte graphique. Tous
fonctionnent ; une cité se dessine en 1 à 2 ms par image avec accélération, 6 ms en
rendu logiciel. Pour forcer un choix : `SDL_VIDEODRIVER=x11` ou
`SDL_RENDER_DRIVER=software`, par exemple.

Pour Windows (`x86_64-pc-windows-gnu`) et macOS (`aarch64-apple-darwin`), le code passe
le contrôle du compilateur (`cargo check --target …`) : rien n'y est propre à un
système. L'édition de liens avec SDL3 et Opus, et le jeu lui-même, n'y ont pas encore
été essayés.

## Licence

Olympe est distribué sous licence GNU GPL, version 3 ou ultérieure (voir `LICENSE`).

*Zeus : le Maître de l'Olympe* et *Poséidon* sont des marques et des œuvres de leurs
ayants droit. Ce projet n'est ni affilié à eux ni approuvé par eux.
