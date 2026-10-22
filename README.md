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
  comme dans le jeu ; des porteurs apportent au chantier, par la route, le marbre, le bois et les sculptures de l'entrepôt
  (quantités provisoires), et les objectifs de sanctuaire sont suivis ; achevés, ceux
  d'Athéna, de Dionysos et d'Héphaïstos apportent olives, raisin et bronze, et
  chaque année le dieu vient en personne laisser un bienfait (choix d'Olympe) ;
- monuments de Poséidon (pyramides, monuments, sanctuaires mineurs, autel et temple
  de l'Olympe…), proposés quand un objectif les demande : disposés et élevés étape
  par étape comme dans le jeu, à mesure que le marbre, le bois et les sculptures
  arrivent de l'entrepôt (quantités provisoires) ; les pentes sont dessinées en
  blocs, les statues en obélisques ;
- marbre noir et orichalque de Poséidon : atelier et raffinerie posés n'importe où
  (raccourci), marchandises rangées à l'entrepôt ;
- `olympe --bilan` vérifie que chaque épisode de chaque aventure installée a tous ses
  objectifs suivis et réalisables avec ses bâtiments permis, et une place sur la carte
  pour ce qu'il faut produire (152 sur 152) ;
- hippodrome (Administration, aventures de l'Atlantide) : sections de piste posées
  bout à bout, droites ou virages selon leurs voisines ; une boucle fermée accueille
  les courses et rapporte chaque mois (barème d'Olympe), et l'objectif compte un stade
  par section (hypothèse d'Olympe) ;
- quai des trières (onglet Armée), au bord de l'eau : il prend quatre charges de
  bois et une d'armement à l'entrepôt et lance une trière trois mois plus tard (barème d'Olympe ; le jeu
  n'a pas d'image reconnue du quai, une pêcherie en tient lieu) ; les objectifs
  d'entretien de trières sont suivis ;
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
  envoient leurs armées à la date prévue : les soldats entrent par un point
  d'invasion de la carte, les hoplites de la cité sortent du palais à leur rencontre
  et ils se battent au corps à corps (Grecs, Troyens, Perses, centaures, amazones,
  Égyptiens, Mayas, Phéniciens, Océanides ou Atlantes, selon la cité) ; si les envahisseurs l'emportent, ils brûlent
  des bâtiments et repartent avec un cinquième du trésor ; les
  cadeaux des cités arrivent à leur date, et ceux de la cité mère apportent à la
  colonie ce qu'elle avait mis de côté ;
  les événements de l'épisode changent le monde à leur date (une cité devient
  rivale, alliée, vassale, se rebelle, renforce ou perd son armée) et apportent les
  demandes des cités (marchandises, vivres, drachmes, soldats), que l'on satisfait
  depuis le panneau `C` : servir une cité accroît sa faveur, l'ignorer la fâche,
  jusqu'à la rendre rivale (barème d'Olympe) ; ils font aussi varier les prix,
  l'offre et la demande des cités, les salaires, et éloignent l'armée d'une rivale
  (sa défense est alors réduite de moitié) ; les catastrophes frappent les zones
  peintes dans l'éditeur à partir de leur point de départ : un tremblement de terre
  ouvre la fissure, la lave recouvre le pays, un raz-de-marée inonde la côte six mois
  (pour toujours si l'aventure le veut), un glissement couvre d'éboulis le pied de la
  pente, et les bâtiments touchés sont détruits ;
- enchaînement des épisodes dans l'ordre de l'aventure, avec leurs récits narrés
  (voix du jeu, retrouvées pour les aventures de Zeus, de Poséidon et en dossier), leurs
  objectifs et leurs ressources : départ pour une colonie sur sa propre carte, retour
  à la cité mère, fin de l'aventure ; la sauvegarde retient l'épisode en cours ;
- un clic sur un passant (porteur d'eau, garde, philosophe, dieu…) le fait parler,
  avec les voix du jeu ;
- cinq niveaux de difficulté (Débutant à Olympien, touche D sur l'écran des
  aventures ou `--difficulte`), avec leurs fichiers de règles et le trésor initial
  ajusté comme dans le jeu ;
- sauvegarde et rechargement d'une partie.

Toutes les aventures installées se jouent de bout en bout : `olympe --bilan` vérifie
que chacun des 152 épisodes a ses objectifs suivis et réalisables, et chaque aventure
s'enchaîne jusqu'à son récit de fin.

Ce qui reste approché ou absent : chaque nation envoie toutes ses unités (fantassins,
cavaliers, archers…) mais toutes combattent au corps à corps ; les exploitations se posent à moins de huit
cases de leur gisement (arbres, carrière de marbre, rochers de cuivre, marbre noir,
orichalque), sauf si la carte n'en a aucun alors qu'un objectif l'exige ; pentes
des pyramides en blocs ; les conseillers tiennent en un panneau de synthèse (touche
A) ; la vidéo d'introduction est jouée par un lecteur du système (ffplay, mpv ou
VLC), Olympe ne décodant pas la vidéo lui-même ;
l'interface est celle d'Olympe, habillée des images d'origine. Plusieurs réglages
d'équilibre (rendements, consommations, quantités de matériaux) sont approchés et non
repris du jeu.


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
