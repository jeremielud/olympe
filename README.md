# Olympe

Moteur libre, écrit en Rust, pour *Zeus : le Maître de l'Olympe* et son extension
*Poséidon* (Impressions Games, 2000-2001). Il lit les données du jeu d'origine et les
fait tourner sur une machine d'aujourd'hui : toute définition d'écran, zoom libre,
interface agrandie sur les écrans à haute définition.

**Olympe ne contient aucune donnée du jeu.** Il vous faut votre propre exemplaire :
un jeu déjà installé, les CD, ou leurs images.

## État du projet

Olympe est une réécriture en cours, pas encore un remplaçant complet du jeu.

### Avancement : environ 75 %

Estimation par domaine (octobre 2026), pondérée par ce qu'un joueur rencontre :

| Domaine | Avancement |
|---|---|
| Installation et lecture des données (cartes, images, textes, sons, vidéos) | 100 % |
| Affichage de la cité (terrain, bâtiments, personnages, mini-carte, zoom) | 90 % |
| Aventures, épisodes, colonies et objectifs (152 épisodes sur 152 suivis) | 85 % |
| Musique, bruitages, voix et récits narrés | 85 % |
| Économie et logements (emploi, vivres, marchandises, évolution, impôts, risques) | 85 % |
| Interface (menus, fiches, vues, journal, superviseurs, panneaux) | 80 % |
| Commerce et cités du monde | 70 % |
| Mythologie (dieux, héros, monstres, quêtes) | 65 % |
| Armée et combats (murs, tours, corps de garde, campagnes et raids au loin) | 65 % |
| Langues (anglais, espéranto) et menu de triche | 0 % (prévus une fois le jeu entièrement jouable) |

Manque encore notamment : dans les cités atlantes la frégate
(la trière en tient lieu).

Vérifié en jouant : le premier tutoriel, celui de la colonie et le premier épisode de
*Persée et Méduse* se gagnent avec une cité bâtie par script (`essais/`).

Ce qui fonctionne :

- installation des données depuis un jeu installé, le contenu d'un CD ou une image de CD ;
- écran de choix parmi les 40 aventures, avec leur récit d'ouverture ;
- affichage de la carte, zoom, mini-carte, mer animée ;
- construction : routes, logements modestes et luxueux, agoras et échoppes, fermes,
  ateliers, entrepôt, grenier, culture, palais, stade, embellissements ;
- simulation : immigrants, emploi, tournées des marcheurs, vivres et marchandises,
  attrait des quartiers, évolution des logements (note de culture du jeu : 15 pour la
  philosophie, 20 pour l'athlétisme, 30 de plus avec un stade, 25 pour le théâtre,
  relevée dans ses sauvegardes), impôts, incendies et effondrements
  (rythmes et seuils de `Zeus.exe`) ; la main-d'œuvre se partage comme dans le jeu : en
  cas de pénurie, chaque bâtiment reçoit la même part de ses postes et travaille
  d'autant plus lentement ;
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
  arrivent de l'entrepôt (quantités provisoires) ; pentes, angles, marches et
  décors de marbre comme dans le jeu, statues, autel et temple des sanctuaires ;
- marbre noir et orichalque de Poséidon : atelier et raffinerie posés n'importe où
  (raccourci), marchandises rangées à l'entrepôt ;
- bâtiments autorisés de chaque épisode, lus dans l'aventure (le tableau du panneau
  « Bâtiments autorisés » de l'éditeur, en 0x902d30 dans `Zeus.exe`) : ce qu'un épisode
  ne permet pas n'apparaît pas dans les menus ; fermes, élevage et matières premières
  restent réglés par les ressources ;
- cités atlantes (drapeau à l'octet 648 de chaque épisode, lu en 0x902bb0 dans
  `Zeus.exe` et vérifié dans le jeu : les quatre aventures atlantes de Poséidon) : la
  science y remplace la culture grecque — bibliothèque, université et observatoire,
  atelier d'inventeurs et laboratoire, musée, aux images de Poséidon ; points de
  science mesurés dans le jeu : astronomie 20, apprentissage 15, recherche 25 (le
  musée compte comme le stade, non mesuré) ;
- campagnes lointaines (panneau des cités du monde, touche `C`) : un panneau choisit
  les compagnies qui partent — piétaille, hoplites, cavaliers et trières, chacun de
  zéro au maximum présent — et le reste garde la cité ; la compagnie part
  soumettre ou piller une cité étrangère, marche le temps que donne la route du monde
  (longueur × allure / 200 mois, deux au moins), livre bataille puis rentre, comme dans
  `Zeus.exe` : un homme de piétaille ou un hoplite compte pour un, un cavalier pour
  deux, une trière pour six (elle peut couler, et son quai l'attend sans en bâtir
  d'autre) ; la défense d'une cité vaut √(années, trente au plus) × barème de difficulté
  × son niveau militaire, moitié moins si son armée est partie, et remonte d'un point
  par mois ; pertes des deux camps selon l'écart des forces (barèmes du jeu), soldats
  tombés retirés des logements qui les fournissaient ; un raid victorieux rapporte une
  marchandise que la cité vend (dans la limite de ce que portent hoplites,
  cavaliers et trières) ou des drachmes selon sa richesse ;
- soldats des cités atlantes : archers, lanciers et chars de `poseidonimps.sg3`
  (mêmes règles que piétaille, hoplites et cavaliers, comme dans les règles des
  personnages du jeu), avec leurs animations de marche et de combat ;
- chars des cités atlantes : l'usine à chars se fait livrer du bois par l'entrepôt et
  des chevaux par le haras (un de chaque par char, quantités d'Olympe), range ses
  chars à l'entrepôt (baies de Poséidon), et le marchand de chars de l'agora les
  porte aux logements luxueux, qui les exigent à la place des chevaux ; le marbre
  noir et l'orichalque ont aussi retrouvé leurs vraies baies ;
- rotation de la vue (touches Origine et Fin), comme dans le jeu : le terrain est
  repavé pour l'orientation par une transcription de la routine de `Zeus.exe`
  (0x521520 : eau, falaises et rampes d'après les hauteurs, arbres, rochers,
  carrières, plages, marais, broussailles, prés, herbe en plaques), qui retrouve
  99,9 % des images des cartes à l'orientation 0 ; les bâtiments gardent leur seule
  vue et c'est leur emprise qui tourne, le palais prend ses images de l'autre sens,
  la mini-carte et les personnages tournent avec la vue ; le terrain surélevé se
  dessine 30 pixels plus haut par niveau, comme dans le jeu (hauteurs du bloc 23 des
  cartes), et la souris désigne la case surélevée dessinée sous elle ;
- jeux panhelléniques (épisodes qui les prévoient, octet 808) : chaque année les
  Olympiques, Isthmiques, Néméens ou Pythiques selon l'année, comme dans `Zeus.exe` ;
  départ au sixième mois, résultat au huitième, d'après la couverture et le nombre de
  lieux de la discipline (un héros présent aide) ; une victoire vaut un monument
  commémoratif et la faveur des cités du monde, une victoire aux Olympiques fait
  accueillir les suivants, ce qui exige un stade ;
- `olympe --bilan` vérifie que chaque épisode de chaque aventure installée a tous ses
  objectifs suivis et réalisables avec ses bâtiments permis, et une place sur la carte
  pour ce qu'il faut produire, et assez de culture permise pour les logements
  demandés (152 sur 152) ;
- hippodrome (Administration, aventures de l'Atlantide) : sections de piste posées
  bout à bout, droites ou virages selon leurs voisines ; une boucle fermée accueille
  les courses et rapporte chaque mois (barème d'Olympe), et l'objectif compte un stade
  par section (hypothèse d'Olympe) ;
- murs (onglet Armée, tracés comme les routes, 5 drachmes la case) : l'image de chaque
  case suit ses voisins d'après la table du jeu ; les envahisseurs les contournent et,
  sans passage, sapent le plus proche jusqu'à la brèche (solidité d'Olympe). Les murs
  arrêtent aussi les citoyens : le corps de garde (type 131, deux images selon le sens
  de la route), posé sur une route, les laisse passer et barre la route aux
  envahisseurs ;
- tours (onglet Armée, règles du type 132) : en activité, leurs sentinelles tirent sur
  l'envahisseur le plus proche à portée, avec la portée, l'attaque à distance et la
  cadence de la « sentinelle de tour » des règles des personnages ;
- quai des trières (onglet Armée), au bord de l'eau : il prend deux charges de
  bois et deux d'armement à l'entrepôt et lance une trière 32 jours plus tard (comme le jeu ; celui-ci
  n'a pas d'image reconnue du quai, une pêcherie en tient lieu) ; les objectifs
  d'entretien de trières sont suivis ;
- héros, quêtes et monstres : temple de chaque héros (catégorie Mythologie), avec
  ses exigences lues dans le jeu, dont l'hippodrome en activité (cités atlantes) et
  le stade ou le musée actif ; un héros venu dans la cité affronte le monstre
  qui lui revient puis part pour sa quête, et les objectifs correspondants sont
  suivis ; les monstres entrent dans la cité, où ils détruisent un bâtiment tous les
  deux mois jusqu'à ce que leur héros, venu de son temple, les tue (les monstres
  marins restent au large) ; un monstre lâché ou envahisseur arrive à sa date ;
- dieux hostiles en personne : à la date de leur événement, ils entrent dans la
  cité et la ravagent six mois, un seul si un dieu défenseur y a son sanctuaire
  (comme dans le jeu, Zeus contre tous, Aphrodite contre Arès, Héphaïstos, Hermès
  et Dionysos, Héra contre Zeus ; six mois de séjour comme dans le jeu, mais durée
  écourtée et rythme des destructions d'Olympe) ;
- soldats fournis par les logements (piétaille, hoplites, cavaliers, selon les
  chiffres de l'éditeur d'aventures) pour les objectifs militaires, sans trières ;
- cités du monde lues dans la carte (statut, force militaire, commerce ouvert) ;
  panneau `C`, d'où l'on envoie l'armée (voir les campagnes lointaines), ce qui suit
  les objectifs « Régner » ; le commerce passe, comme dans le jeu, par un poste de
  commerce pour une cité reliée par la terre ou par des docks au bord de l'eau pour
  une cité reliée par la mer (voie de la route du monde, octet 0x140), entrepôts à
  huit baies attribués chacun à une partenaire (le jeu la fait choisir, Olympe prend
  la première qui n'en a pas) ; les cités lointaines commercent aussi : chaque mois, on vend à la cité ce qu'elle achète au-delà
  d'une réserve et l'on achète ce qu'elle vend quand les réserves sont basses
  (marchandises, quantités et prix lus dans la carte) ; le panneau dit ce que chaque
  cité vend et achète, et règle par marchandise ce que le commerce peut en faire
  (vendre le surplus et acheter, l'un des deux, ou rien) ; les vassales
  et les colonies fondées
  paient chaque année le tribut réglé dans l'éditeur ; les cités rivales
  envoient leurs armées à la date prévue : les soldats entrent par le point
  d'invasion de l'événement, ou débarquent d'un navire s'il est en mer, les hoplites de la cité sortent du palais à leur rencontre
  et ils se battent avec les points de vie, attaques, armures et portées des règles
  du jeu (`figure_model`), l'armée ennemie composée selon les fréquences de ses unités (Grecs, Troyens, Perses, centaures, amazones,
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
  objectifs et leurs ressources : départ pour une colonie sur sa propre carte (au
  choix du joueur quand l'aventure en propose plus qu'elle n'a d'épisodes de colonie),
  retour à la cité mère, fin de l'aventure ; la sauvegarde retient l'épisode en cours
  et les colonies choisies ;
- interface du jeu : bandeau avec menus Fichier, Options et Aide ; panneau latéral
  avec onglets de catégories, informations de la catégorie (couverture des services,
  logements libres…), boutons « Voir… », boutons illustrés des groupes de bâtiments,
  onglet de la carte générale, zone des messages ; bulles d'aide au survol ; fiche
  d'un bâtiment au clic droit ; pause par `P` avec son bandeau ;
- vues en surimpression (touche `V`) : eau, risques d'incendie et d'effondrement,
  attrait, criminalité, culture, santé et impôts, chaque bâtiment teinté du vert au
  rouge ;
- fiche des maisons : ce qui leur manque pour évoluer ; un service bloqué dit ce qu'il
  attend (palais pour les percepteurs, école pour les philosophes et les acteurs) ;
- superviseurs (touche A, Tab pour changer de page) : population par niveau de
  logement, emploi par catégorie, finances, services et réserves, mythologie, armée
  et commerce, en panneaux de texte comme dans le jeu ;
- lettres d'événements du jeu (`model/zeus eventmsg.txt`) : demandes des cités (au
  ton de leur statut), cadeaux, invasions, dieux, héros, sanctuaires achevés,
  commerce, salaires, peste s'ouvrent comme les récits, datées, avec le nom de la
  cité et de son chef (en toutes lettres dans les cartes de Poséidon) ;
- un clic sur un passant (porteur d'eau, garde, philosophe, dieu…) le fait parler,
  avec les voix du jeu ;
- la vidéo d'introduction, convertie par l'installateur (avec ffmpeg) en images et en
  son qu'Olympe lit lui-même, au lancement (`--sans-intro` pour la passer) ;
- cinq niveaux de difficulté (Débutant à Olympien, touche D sur l'écran des
  aventures ou `--difficulte`), avec leurs fichiers de règles et le trésor initial
  ajusté comme dans le jeu ;
- taux d'imposition réglable (touches `+` et `-`), d'« aucun » à « scandaleux » avec
  les taux du jeu, et humeur de la cité calculée comme dans `Zeus.exe`, deux fois par
  mois : impôts selon la difficulté (`tax_sentiment_model.txt`), salaires, chômage,
  vivres de chaque maison et dettes, amortis hors de la zone 41–60, et sans effet
  négatif tant que la cité n'a pas dépassé 349 habitants ; chaque jour, l'humeur
  règle les migrations comme dans le jeu : de 12 arrivants au-delà de 70 à aucun
  entre 36 et 49, puis jusqu'à 3 départs par jour sous 16 (pas sous 100 habitants),
  les arrivants allant d'abord aux maisons vides, les partants quittant d'abord les
  logements les plus modestes ; plus d'arrivants après trois ans de dettes ;
- criminalité des logements modestes comme dans le jeu (incrément et base de chaque
  niveau, écart de l'humeur à 40, passage des gardes) : au-delà de 80 une maison
  produit un voleur, qui pille une maison luxueuse ou le trésor ;
- bienfaits permanents des sanctuaires, comme dans le jeu : Dionysos fait baisser la
  criminalité, Aphrodite retient tous les habitants et abaisse de 10 l'attrait que
  demandent les logements, Héphaïstos ôte tout risque d'incendie, Poséidon fait
  pêcher en deux tiers du temps, Héra ajoute 3 points au croît de la population,
  Hermès rend les porteurs plus rapides d'un tiers, Athéna donne 4 points d'armure
  contre les projectiles aux soldats de la cité ;
- santé de la cité comme dans le jeu (conseiller des services) : elle tend chaque
  mois vers la part des habitants que visitent les médecins et qui ont mangé, plus un
  bonus pour les cités de moins de 1 000 habitants ; sous 75, une épidémie peut
  éclater (jusqu'à 3 chances sur 100 par jour sous 10, une par mois au plus) et
  frapper 40 % des maisons, les moins soignées d'abord, emportant d'un quart des
  habitants en très facile à tous en très difficile ; chaque année, la santé fait
  naître (jusqu'à 6 % au-delà de 90) ou mourir (jusqu'à 5 %) une part des habitants ;
- sauvegarde et rechargement d'une partie.

Toutes les aventures installées se jouent de bout en bout : `olympe --bilan` vérifie
que chacun des 152 épisodes a ses objectifs suivis et réalisables, et chaque aventure
s'enchaîne jusqu'à son récit de fin.

Ce qui reste approché ou absent : les tirs des archers sont figurés par des traits,
faute d'images de flèches dans les catalogues ; les exploitations se posent à moins de huit
cases de leur gisement (arbres, carrière de marbre, rochers de cuivre, marbre noir,
orichalque), sauf si la carte n'en a aucun alors qu'un objectif l'exige ; le
sommet des pyramides, l'observatoire et le musée posés sur les monuments sont
figurés par des obélisques ; 
l'interface est celle d'Olympe, habillée des images d'origine. Les rythmes des
producteurs et des ateliers, et l'usure de la laine, de l'huile et du vin dans les
maisons, sont ceux du jeu ; ceux de la pêcherie (selon la distance au banc de poissons
le plus proche placé dans l'éditeur), de la laiterie et du pavillon de chasse ont été
mesurés dans le jeu, mais Olympe ne fait paraître ni bateaux, ni chèvres, ni gibier.
D'autres réglages d'équilibre (quantités de matériaux) sont approchés et non repris du
jeu, de même que le butin des voleurs.


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
