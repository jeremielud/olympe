//! Le jeu. Pour l'instant : chargement d'une campagne ou d'une carte, vue isométrique
//! en temps réel, pose de routes et de quelques bâtiments.
//!
//! Glisser avec le bouton droit ou du milieu (ou flèches) pour se déplacer, molette
//! pour zoomer, F pour le plein écran. Touches 1 à 9 : choisir un outil (1 = route),
//! X : démolir, clic gauche : appliquer, Échap : lâcher l'outil, puis quitter.
//! Espace : pause. F5 : enregistrer la partie, F9 : la recharger.

use std::collections::HashMap;
use std::ffi::{CString, c_void};
use std::fs;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use zeus::font::Font;
use zeus::goals::{Goal, Kind as GoalKind, Status};
use zeus::jpeg;
use zeus::messages::{self, Messages};
use zeus::map::{GRID, Map, Source};
use zeus::model::Model;
use zeus::ambience::Ambience;
use zeus::music::{Music, Voice};
use zeus::heroes::{self, Check, Requirement};
use zeus::pak::{Campaign, Episode, Event, Settings, Step};
use zeus::paths;
use zeus::png;
use zeus::sdl;
use zeus::monument;
use zeus::sanctuary::{self, Layout, Piece};
use zeus::sg::Catalog;
use zeus::text::Text;
use zeus::world::{Cause, Need, TradePolicy, TERRAIN_BEACH, TERRAIN_MEADOW, TERRAIN_ROCK, TERRAIN_TREE, TERRAIN_WATER};
use zeus::world::{Appeal, Conquest, Function, HeroNews, Monster, Quest, QuestState, Raw, GOODS, Good, Kind, Part, SERVICES, Service, WalkerImages, World, for_houses};

const USAGE: &str = "usage :
  olympe                              lance le jeu (écran de choix d'aventure)
  olympe <aventure.pak | carte.map>   lance directement une aventure ou une carte
  olympe installer <source>…          installe les données du jeu d'origine ; une source
                                      est un dossier où le jeu est installé, le contenu
                                      d'un CD, une image de CD (Zeus, puis Poséidon), ou
                                      le programme d'un correctif officiel, donné en dernier
options :
  --donnees DOSSIER  dossier des données (par défaut celui de l'installateur)
  --vers DOSSIER     avec « installer » : où ranger les données
  --size LxH         taille de la fenêtre (1600x900 par défaut)
  --demo             bâtit une cité de démonstration (carte « Les plus jeunes jumeaux »)
  --mesure N         dessine N images, affiche la cadence obtenue et quitte
  --capture IMAGE    enregistre la première image rendue et quitte
  --sans-intro       ne joue pas la vidéo d'introduction
  --bilan            vérifie, pour chaque épisode de chaque aventure installée, que
                     ses objectifs sont suivis et réalisables, puis quitte
  --difficulte NIVEAU debutant, mortel, heros (par défaut), titan ou olympien
  --ticks N          nombre de pas de simulation joués avant la capture (1500 par défaut)
  --script \"…\"      essai scripté, une étape par image, avant la capture : m:x,y (pointeur),
                     d / u (bouton gauche), w:n (molette), k:1..9 | x | esc | espace | f5 | f9 | h | o | r | a | c | tab,
                     t:n (pas), p:ms (pause), fin (termine l'épisode en cours),
                     r:x1,y1,x2,y2 (route en équerre), b:x,y,nom (bâtiment, _ pour les
                     espaces), bilan (état de la cité et des objectifs), vue:x,y (vue
                     centrée sur une case), mc:x,y (pointeur sur une case)
en jeu : H affiche l'aide";

/// Demi-largeur et demi-hauteur d'une case à l'écran, à l'échelle 1.
const HALF_WIDTH: f32 = 30.0;
const HALF_HEIGHT: f32 = 15.0;
/// Hauteur d'un niveau de terrain surélevé, en pixels.
const LEVEL_HEIGHT: f32 = 30.0;
const ZOOM_MIN: f32 = 0.25;
const ZOOM_MAX: f32 = 6.0;
/// Déplacement d'une touche fléchée, en pixels d'écran.
const KEY_STEP: f32 = 120.0;
/// Hauteur maximale d'une image au-dessus de sa case, pour ne rien couper en bas d'écran.
const TALLEST_IMAGE: f32 = 400.0;
/// Durée d'une image d'animation de bâtiment, en pas de simulation (provisoire).
const ANIMATION_TICKS: u64 = 3;
/// Durée d'affichage d'un message passager, en millisecondes.
const NOTICE_MS: u64 = 4000;
/// Pas de simulation joués avant une capture d'écran.
const CAPTURE_WARMUP_TICKS: u32 = 1500;
/// Trésor de départ d'une carte sans réglages d'aventure.
const STARTING_MONEY: i32 = 5000;

/// Groupe de textes des noms de bâtiments, indexé par numéro de type.
const TEXT_BUILDINGS: usize = 28;
/// Numéro de type de la route.
const TYPE_ROAD: usize = 28;
/// Mur (règles : 5 drachmes la case) ; la catégorie Armée du menu le propose.
const TYPE_WALL: usize = 130;
/// Tour (règles : type 132) et son image dans le catalogue général.
const TYPE_TOWER: usize = 132;
/// Corps de garde (type 131), deux images : la route le traverse dans un sens ou l'autre.
const TYPE_GATEHOUSE: usize = 131;
const TYPE_ROADBLOCK: usize = 123;
/// Barrage routier (une case), dans `zeus_general.sg3`.
const ROADBLOCK_IMAGE: usize = 1150;
const GATEHOUSE_IMAGE: usize = 768;
const TOWER_IMAGE: usize = 774;
const MILITARY_CATEGORY: usize = 9;
/// Groupes de personnages dans `sprmain.sg3`. Le catalogue nomme ses planches
/// (Peddler, Actor, Ox…) dans l'ordre des groupes ; l'alignement a été fait sur les
/// dimensions annoncées par chaque planche, puis vérifié à l'œil.
const GROUP_ACTOR: usize = 2;
const GROUP_SUPERINTENDENT: usize = 10;
const GROUP_BUYER: usize = 16;
const GROUP_IMMIGRANT: usize = 26;
const GROUP_ELITE_IMMIGRANT: usize = 30;
const GROUP_FLEECE_PEDDLER: usize = 28;
const GROUP_WATCHMAN: usize = 34;
const GROUP_FOOD_PEDDLER: usize = 47;
const GROUP_CLERK: usize = 88;
const GROUP_CARRIER: usize = 90;
const GROUP_OIL_PEDDLER: usize = 99;
const GROUP_WINE_PEDDLER: usize = 101;
const GROUP_ARMS_PEDDLER: usize = 32;
const GROUP_HORSE_TRAINER: usize = 14;
const GROUP_WATER_BEARER: usize = 123;
const GROUP_HEALER: usize = 135;
const GROUP_ATHLETE: usize = 214;
const GROUP_PHILOSOPHER: usize = 240;

/// Bâtiments faits d'une seule image : numéro de type (rang dans les règles et dans
/// les noms), image dans `zeus_general.sg3` (identifiée à l'œil), rôle, marcheur.
/// La fonderie porte par erreur le numéro 10 dans le fichier de règles ; elle y occupe
/// bien le rang 50.
const TYPE_FOUNDRY: usize = 50;
/// Laiterie (la fermière et ses fromages) et pavillon de chasse (le boucher), dans la
/// planche de l'élevage ; les pontons voisins (544 à 551) sont les quais à oursins.
const DAIRY_IMAGE: usize = 565;
const HUNTING_LODGE_IMAGE: usize = 528;
/// Quai des navires de guerre (trières ou frégates), et l'image de pêcherie qui le
/// représente.
const TYPE_WHARF: usize = 136;
/// Section d'hippodrome, et sa première image (droite) dans `poseidon_loaded.sg3`.
const TYPE_RACETRACK: usize = 199;
const TYPE_ORICHALC_REFINERY: usize = 211;
const TYPE_BLACK_MARBLE_SHOP: usize = 213;
/// Fabrique et marchand de chars des cités atlantes : images de `poseidon_loaded.sg3`
/// relevées à l'œil (fabrique 4 × 4 ; échoppe vide, puis garnie).
const TYPE_CHARIOT_WORKSHOP: usize = 212;
const TYPE_CHARIOT_VENDOR: usize = 215;
const CHARIOT_WORKSHOP_IMAGE: usize = 679;
const CHARIOT_VENDOR_IMAGE: usize = 926;
/// Docks et poste de commerce : entrepôts à huit baies dont la cabane (une case de
/// `zeus_general.sg3`, groupes 125 et 137) a été attribuée à l'œil.
const TYPE_DOCK: usize = 60;
const TYPE_TRADING_POST: usize = 62;
const DOCK_HUT_IMAGE: usize = 469;
const TRADING_POST_HUT_IMAGE: usize = 467;
const RACETRACK_IMAGE: usize = 426;
const WHARF_IMAGE: usize = 721;
/// Science atlante : bibliothèque (comme le gymnase), université et observatoire
/// (comme collège et podium), atelier d'inventeurs et laboratoire (comme école de
/// théâtre et théâtre), musée (comme le stade).
const SCIENCE_KINDS: [(usize, usize, usize, Function, usize); 6] = [
    (202, 540, 2, Function::Service(Service::Athletics), GROUP_ATHLETE),
    (204, 436, 3, Function::School(Service::Philosophy), 0),
    (203, 576, 5, Function::Service(Service::Philosophy), GROUP_PHILOSOPHER),
    (206, 502, 3, Function::School(Service::Drama), 0),
    (205, 461, 4, Function::Service(Service::Drama), GROUP_ACTOR),
    (207, 597, 6, Function::Museum, 0),
];
/// Types que seule une cité grecque peut bâtir, et que seule une cité atlante peut
/// bâtir (guide de l'éditeur de Poséidon, annexe 7). Le quai des navires et le
/// dresseur de chevaux restent ouverts aux cités atlantes : le quai y arme des
/// frégates, avec les règles de la trière (son image propre reste à identifier), et
/// le dresseur y tient lieu du marchand de chars, qu'Olympe n'a pas.
const GREEK_ONLY: [usize; 6] = [77, 78, 79, 80, 81, 82];
const ATLANTEAN_ONLY: [usize; 8] = [202, 203, 204, 205, 206, 207, TYPE_CHARIOT_WORKSHOP, TYPE_CHARIOT_VENDOR];
const SIMPLE_KINDS: [(usize, usize, Function, usize); 37] = [
    (TYPE_HOUSE, 1006, Function::House, 0),
    (74, 1055, Function::Service(Service::Water), GROUP_WATER_BEARER),
    (121, 1075, Function::Service(Service::Maintenance), GROUP_SUPERINTENDENT),
    (122, 1139, Function::Service(Service::Tax), GROUP_CLERK),
    (124, 1120, Function::Service(Service::Security), GROUP_WATCHMAN),
    (76, 1062, Function::Service(Service::Health), GROUP_HEALER),
    (57, 473, Function::Granary, 0),
    (77, 332, Function::School(Service::Philosophy), 0),
    (81, 415, Function::Service(Service::Philosophy), GROUP_PHILOSOPHER),
    (79, 357, Function::Service(Service::Athletics), GROUP_ATHLETE),
    (78, 390, Function::School(Service::Drama), 0),
    (82, 440, Function::Service(Service::Drama), GROUP_ACTOR),
    (41, 552, Function::Producer(Good::Fleece), 0),
    // Laiterie : produit du fromage sans chèvres (raccourci).
    (42, DAIRY_IMAGE, Function::Producer(Good::Food), 0),
    // Pavillon de chasse : fournit de la viande sans chasseurs ni gibier (raccourci).
    (46, HUNTING_LODGE_IMAGE, Function::Producer(Good::Food), 0),
    // Pêcherie : posée sur la terre ferme au bord de l'eau, sans bateau (raccourci ;
    // dans le jeu d'origine elle s'avance sur l'eau).
    (44, 719, Function::Fishery, 0),
    (37, 587, Function::Supplier(Raw::Crops), 0),
    // Fonderie et armurerie : images attribuées à l'œil.
    (TYPE_FOUNDRY, 618, Function::Supplier(Raw::Bronze), 0),
    (135, 740, Function::Workshop(Good::Arms), 0),
    // Scierie et atelier de maçonnerie : posés n'importe où, sans bûcherons ni carrière
    // (raccourci, comme la fonderie) ; images attribuées à l'œil.
    (51, 598, Function::Producer(Good::Wood), 0),
    (49, 609, Function::Producer(Good::Marble), 0),
    // Atelier de sculpture : coule le bronze ; image attribuée à l'œil (tour de coulée).
    (55, 683, Function::Workshop(Good::Sculpture), 0),
    // Haras : le groupe d'images voisin de celui de l'enclos à chevaux.
    (133, 723, Function::Producer(Good::Horses), 0),
    // Stade : deux moitiés 5 × 5 ; l'outil pose l'annexe (au fond) puis celle-ci.
    (80, 441, Function::Stadium, 0),
    (80, 442, Function::Annex, 0),
    (54, 657, Function::Workshop(Good::Oil), 0),
    (53, 670, Function::Workshop(Good::Wine), 0),
    (128, 212, Function::None, 0),
    (15, 216, Function::None, 0),
    // Embellissements : labyrinthe, bassin et colonnes se reconnaissent sans peine ;
    // kiosque, topiaire et source sont attribués à l'œil.
    (16, 217, Function::None, 0),
    (21, 218, Function::None, 0),
    (22, 222, Function::None, 0),
    (17, 223, Function::None, 0),
    (18, 224, Function::None, 0),
    (129, 234, Function::None, 0),
    (145, 235, Function::None, 0),
    (146, 236, Function::None, 0),
];
/// Exploitation agricole : une maison 2 × 2 et cinq cases de champ.
const FARM_HOUSE_IMAGE: usize = 528;
/// Exploitations : type et première image de la culture (six stades de pousse) —
/// blé, carottes, oignons.
const FARMS: [(usize, usize); 3] = [(31, 510), (33, 516), (32, 522)];
/// Ressource (numéro du groupe de textes 23) de chaque exploitation, et des autres
/// productions soumises à l'aventure.
const FARM_RESOURCES: [u8; 3] = [7, 5, 6];
const RESOURCE_FISH: u8 = 2;
const RESOURCE_MEAT: u8 = 3;
const RESOURCE_CHEESE: u8 = 4;
const RESOURCE_WOOD: u8 = 9;
const RESOURCE_BRONZE: u8 = 10;
const RESOURCE_MARBLE: u8 = 11;
const RESOURCE_GRAPES: u8 = 12;
const RESOURCE_OLIVES: u8 = 13;
const RESOURCE_FLEECE: u8 = 14;
/// Ce que produit chaque bâtiment d'une seule image (numéro de type, ressource), pour
/// les objectifs de production et le tri des vivres au grenier.
const PRODUCTS: [(usize, u8); 12] = [
    (44, RESOURCE_FISH),
    (46, RESOURCE_MEAT),
    (42, RESOURCE_CHEESE),
    (41, RESOURCE_FLEECE),
    (TYPE_FOUNDRY, RESOURCE_BRONZE),
    (51, RESOURCE_WOOD),
    (49, RESOURCE_MARBLE),
    (55, 19),
    (135, 18),
    (133, 15),
    (54, 20),
    (53, 21),
];
const FARM_FIELDS: [(usize, usize); 5] = [(2, 0), (2, 1), (0, 2), (1, 2), (2, 2)];
/// Entrepôt : type, cabane (choisie à l'œil parmi les groupes d'images de la planche
/// de stockage) et baie vide.
const TYPE_STOREHOUSE: usize = 58;
const STOREHOUSE_HUT_IMAGE: usize = 471;
const EMPTY_BAY_IMAGE: usize = 934;
/// Palais : type et images (moitié du fond, moitié de devant, dalle, dalle d'angle).
const TYPE_PALACE: usize = 117;
const PALACE_BACK_IMAGE: usize = 1174;
const PALACE_FRONT_IMAGE: usize = 1163;
/// Moitiés du palais pour l'autre sens de la vue (relevées à l'œil).
const PALACE_BACK_TURNED_IMAGE: usize = 1175;
const PALACE_FRONT_TURNED_IMAGE: usize = 1176;
const PALACE_TILE_IMAGE: usize = 1187;
const PALACE_CORNER_IMAGE: usize = 1188;
/// Agora : type de l'agora et de ses emplacements, image d'un emplacement vide.
const TYPE_AGORA: usize = 63;
const TYPE_GRAND_AGORA: usize = 64;
const TYPE_PLOT: usize = 66;
const PLOT_IMAGE: usize = 460;
/// Attrait d'un emplacement ou d'une échoppe : celui de l'agora des règles, partagé
/// entre ses trois emplacements (répartition de mon cru).
const AGORA_PLOTS: i32 = 3;
/// Échoppes : type, image, marchandises en surimpression, marchandise, marchand ambulant.
const VENDORS: [(usize, usize, usize, Good, usize); 6] = [
    (68, 445, 446, Good::Food, GROUP_FOOD_PEDDLER),
    (69, 447, 448, Good::Fleece, GROUP_FLEECE_PEDDLER),
    (70, 449, 450, Good::Oil, GROUP_OIL_PEDDLER),
    (71, 451, 452, Good::Wine, GROUP_WINE_PEDDLER),
    (72, 453, 454, Good::Arms, GROUP_ARMS_PEDDLER),
    (73, 455, 456, Good::Horses, GROUP_HORSE_TRAINER),
];
/// Noms des marchandises dans le groupe de textes 23, dans l'ordre de `GOODS`.
const GOOD_TEXTS: [usize; 13] = [25, 14, 20, 21, 18, 15, 10, 9, 11, 19, 16, 17, 22];
/// Sanctuaires : temple d'un seul tenant (puis, deux plus loin, moitiés arrière et
/// avant des grands temples) dans `zeus_sanctuaryN.sg3`, grande statue (quatre
/// orientations) dans `zeus_godNN.sg3`, et dans `zeus_general.sg3` : petites statues
/// (quatre par dieu, par rang), dalles décorées, dallage uni (six variantes), autel.
const SANCTUARY_TEMPLE_IMAGE: usize = 201;
/// Temple des héros (4 × 4) et statues des six héros de Zeus, dans `zeus_general.sg3`.
const TYPE_HERO_HALL: usize = 125;
const HERO_HALL_IMAGE: usize = 1460;
const HERO_STATUE_IMAGE: usize = 1461;
const HERO_STATUES: usize = 6;
const GOD_STATUE_IMAGE: usize = 201;
const SMALL_STATUE_IMAGE: usize = 1399;
const SANCTUARY_TILE_IMAGE: usize = 1447;
const SANCTUARY_PLAIN_IMAGES: usize = 1453;
/// Sol d'un chantier de monument (`poseidon_loaded.sg3`).
const MONUMENT_FLOOR_IMAGE: usize = 663;
const ALTAR_IMAGE: usize = 1459;
/// Logement luxueux : type, et image d'un quart de terrain vacant (pour l'aperçu).
const TYPE_ELITE_HOUSE: usize = 9;
const ELITE_LOT_IMAGE: usize = 1021;
/// Type du logement modeste.
const TYPE_HOUSE: usize = 2;

/// Mer dans `zeus_terrain.sg3` : quinze tuiles (pleine mer et rivages), répétées six
/// fois à des instants successifs de la houle. Interprétation tirée de la comparaison
/// des images (deux images distantes de 15 diffèrent à peine) et des cartes, où la
/// pleine mer utilise à parts égales les six images de la première tuile.
const SEA_FIRST_IMAGE: usize = 775;
const SEA_TILES: usize = 15;
const SEA_FRAMES: usize = 6;
const SEA_FRAME_TICKS: u64 = 8;

/// Première flamme animée de `destruction.sg3` (planche « fire1 »), nombre d'images
/// et durée de chacune en pas de simulation.
const FIRE_FIRST_IMAGE: usize = 37;
const FIRE_FRAMES: usize = 26;
const FIRE_FRAME_TICKS: u64 = 2;

/// Intervalle entre deux bruitages d'ambiance, en millisecondes (valeur provisoire).
const AMBIENCE_MS: u64 = 2500;

/// Catégorie de bruitages (dans `sound_fxu.txt`) d'un bâtiment selon son rôle.
fn sound_category(function: Function) -> &'static str {
    match function {
        Function::House => "COMMON_HOUSING",
        Function::Service(Service::Water) => "FOUNTAIN",
        Function::Service(Service::Maintenance) => "MAINTENANCE_OFFICE",
        Function::Service(Service::Security) | Function::Tower | Function::Gatehouse | Function::Roadblock => "DEFENSIVE",
        Function::Service(Service::Health) => "INFIRMARY",
        Function::Service(Service::Tax) => "MUNICIPAL",
        Function::Palace => "PALACE",
        Function::Service(Service::Philosophy) | Function::School(_) => "PHILOSOPHY",
        Function::Service(Service::Athletics) => "GYMNASIUM",
        Function::Service(Service::Drama) => "THEATRE",
        Function::Farm => "FARMING",
        Function::Granary | Function::Storehouse | Function::TradePost { .. } => "STORAGE",
        Function::Producer(Good::Horses) => "HORSE_RANCH",
        Function::Producer(Good::Wood) => "TIMBER_MILL",
        Function::Producer(Good::Marble) => "MARBLE_QUARRY",
        Function::Producer(Good::Food) => "GOAT_FARM",
        Function::Fishery | Function::Wharf => "SEA",
        Function::Producer(_) => "SHEEP_FARM",
        Function::Supplier(Raw::Crops) => "ORCHARD",
        Function::Supplier(Raw::Bronze) => "FOUNDRY",
        Function::Workshop(_) => "WORKSHOPS",
        Function::Agora { .. } | Function::Plot | Function::Vendor(Good::Food) => "AGORA_FOOD",
        Function::Vendor(Good::Fleece) => "AGORA_FLEECE",
        Function::Vendor(Good::Oil) => "AGORA_OIL",
        Function::Vendor(Good::Wine) => "AGORA_WINE",
        Function::Vendor(Good::Arms | Good::Bronze | Good::Wood | Good::Marble | Good::Sculpture | Good::BlackMarble | Good::Orichalc) => "AGORA_ARMS",
        Function::Vendor(Good::Horses | Good::Chariot) => "AGORA_HORSE",
        Function::Stadium | Function::Annex | Function::Racetrack | Function::Museum => "STADIUM",
        Function::Sanctuary { .. } | Function::HeroHall(_) | Function::Monument { .. } => "SANCTUARY",
        Function::None | Function::Commemorative(_) => "BEAUTIFICATION",
    }
}

/// Catégorie de bruitages du terrain nu.
fn terrain_sound(terrain: u32) -> &'static str {
    match terrain {
        t if t & TERRAIN_WATER != 0 => "WATER",
        t if t & TERRAIN_TREE != 0 => "VEGETATION",
        t if t & TERRAIN_ROCK != 0 => "ROCKY",
        t if t & TERRAIN_BEACH != 0 => "BEACH",
        t if t & TERRAIN_MEADOW != 0 => "MEADOW",
        _ => "DESERT",
    }
}

/// Côté du terrain libre cherché pour ouvrir la vue, en cases.
const START_CLEARING: i32 = 12;

/// Images de mise en route ignorées par `--mesure`.
const MEASURE_WARMUP: u32 = 60;

/// Largeur maximale du récit d'ouverture, en unités d'interface.
const INTRO_WIDTH: f32 = 1000.0;

/// Aide affichée par la touche H.
const HELP: [&str; 19] = [
    "Glisser avec le clic droit ou flèches : déplacer la vue",
    "Molette : zoom",
    "Mini-carte : clic pour s'y rendre",
    "1 à 9 et 0 : ouvrir une catégorie, puis choisir un bâtiment",
    "Clic gauche : bâtir ; glisser pour tracer une route (bouton route sous les onglets)",
    "X : dégager",
    "Echap : lâcher l'outil, refermer la catégorie, quitter",
    "Espace ou P : pause",
    "Page préc. et Page suiv. : vitesse du jeu",
    "F5 : enregistrer ; F9 : recharger",
    "F : plein écran",
    "M : musique ; B : bruitages",
    "V : vues (eau, risques, attrait, criminalité, culture, santé, impôts) ; J : journal",
    "O : objectifs de l'épisode ; C : cités du monde (clic : choisir la compagnie) ; A : conseillers (Tab : page)",
    "R : mettre de côté les marchandises demandées",
    "+ et - : taux d'imposition",
    "Origine et Fin : pivoter la vue",
    "Survoler un bâtiment : son résumé ; clic droit : sa fiche ; clic sur un passant : il parle",
    "H : refermer cette aide",
];

/// Habillage d'origine (`zeus_interface.sg3`) : bandeau du haut, panneau latéral et son
/// prolongement, onglets des catégories (quatre états par onglet, dont trois servent).
const UI_TOP_BAR: usize = 1231;
const UI_TOP_BAR_WIDTH: f32 = 838.0;
const TOP_BAR_HEIGHT: f32 = 30.0;
const UI_SIDEBAR: usize = 1232;
const SIDEBAR_WIDTH: f32 = 186.0;
const UI_TABS: usize = 1319;
const UI_TAB_STATES: usize = 4;
/// Icône d'onglet de l'armée (grisée).
const MILITARY_TAB: usize = 9;
/// Icône ailée, onzième onglet de la colonne d'origine : les monuments commémoratifs.
const WINGED_TAB: usize = 0;
/// Réserves transparentes du panneau latéral, relevées sur son image : (x, y,
/// largeur, hauteur, premier état du bouton qui s'y loge, touche qu'il remplace).
/// Barrage routier, réserve de la pelle, rotation ; le parchemin des messages ; la
/// coche des objectifs, les conseillers et la carte du monde (globe). Sans touche, le
/// bouton est inactif.
/// Rangée des boutons ronds sous les onglets, comme dans le jeu : route, barrage
/// routier (non simulé), dégager (la pelle), rotation de la vue (non simulée) ; quatre
/// états par bouton (repos, survol, enfoncé, inactif).
const ROAD_BUTTON: usize = 1275;
const ROADBLOCK_BUTTON: usize = 1279;
const CLEAR_BUTTON: usize = 1283;
const ROTATE_BUTTON: usize = 1287;
const ROAD_SLOT: (f32, f32, f32, f32) = (50.0, 435.0, 35.0, 37.0);
const CLEAR_BUTTON_SLOT: (f32, f32, f32, f32) = (118.0, 435.0, 32.0, 37.0);
const ROADBLOCK_SLOT: (f32, f32, f32, f32) = (85.0, 435.0, 32.0, 37.0);
/// Onglets du haut du panneau (relevés sur son image) : constructions, carte générale.
/// Taille d'un bouton de groupe de bâtiments.
const GROUP_BUTTON: (f32, f32) = (58.0, 42.0);
/// Délai avant l'affichage d'une bulle d'aide.
const TOOLTIP_MS: u64 = 500;
const SIDEBAR_TABS: [(f32, f32, f32, f32); 2] = [(52.0, 2.0, 59.0, 19.0), (113.0, 2.0, 63.0, 19.0)];
/// Zone des messages, sous les boutons (relevée sur l'image du panneau).
const MESSAGE_SLOT: (f32, f32, f32, f32) = (8.0, 514.0, 172.0, 46.0);
const SIDEBAR_BUTTONS: [(f32, f32, f32, f32, Option<usize>, Option<u32>); 7] = [
    (85.0, 435.0, 32.0, 37.0, None, None),
    (118.0, 435.0, 32.0, 37.0, None, None),
    (151.0, 435.0, 35.0, 37.0, Some(ROTATE_BUTTON), None),
    (146.0, 483.0, 33.0, 28.0, Some(1291), Some(sdl::KEY_J)),
    (11.0, 565.0, 33.0, 33.0, Some(1267), Some(sdl::KEY_O)),
    (45.0, 565.0, 69.0, 33.0, Some(1263), Some(sdl::KEY_A)),
    (115.0, 565.0, 67.0, 33.0, Some(1271), Some(sdl::KEY_C)),
];
/// Emplacements des onglets dans le panneau latéral, relevés sur son image.
const TAB_LEFT: f32 = 5.0;
const TAB_TOP: f32 = 22.0;
const TAB_STEP: f32 = 41.0;
const TAB_WIDTH: f32 = 44.0;
const TAB_HEIGHT: f32 = 40.0;
/// Bord gauche de la grande zone du panneau, à droite des onglets.
const PANEL_LEFT: f32 = 54.0;
/// Icône d'onglet de chaque catégorie (rang dans la série des onglets d'origine :
/// maison, épis, rouage, amphore, chapiteau, personnage assis, discobole, cyprès) ;
/// l'attribution des icônes aux catégories est faite à l'œil.
const CATEGORY_TABS: [(usize, usize); 11] = [(1, 0), (2, 1), (3, 2), (4, 3), (5, 4), (6, 5), (7, 6), (8, 7), (10, 8), (MILITARY_TAB, 9), (WINGED_TAB, 10)];

/// Catégories du menu de construction : intitulé (rang dans le groupe de textes 4,
/// ou texte de secours quand le jeu n'en a pas) et rôles des bâtiments rangés dedans.
const CATEGORY_NAMES: [(Option<usize>, &str); 11] = [
    (None, "Population"),
    (Some(6), "Agriculture"),
    (Some(1), "Industrie"),
    (Some(5), "Distribution"),
    (Some(7), "Hygiène et sécurité"),
    (Some(11), "Administration"),
    (Some(9), "Culture"),
    (Some(10), "Mythologie"),
    (None, "Embellissement"),
    (Some(2), "Armée"),
    (None, "Monuments"),
];
/// Catégorie des monuments commémoratifs (onglet ailé).
const COMMEMORATIVE_CATEGORY: usize = 10;
/// Monuments commémoratifs : type de bâtiment, première image (les neuf modèles se
/// suivent dans `zeus_general.sg3`), groupe de textes de leurs descriptions.
const TYPE_COMMEMORATIVE: usize = 119;
const COMMEMORATIVE_IMAGE: usize = 225;
const TEXT_COMMEMORATIVES: usize = 90;
/// Ce que célèbre chaque modèle, pour les distinguer dans les listes.
const COMMEMORATIVE_SUBJECTS: [&str; zeus::world::COMMEMORATIVES] = ["croissance", "victoire", "colonie", "athlètes", "conquête", "âge d'or", "héros", "alliance", "sagesse"];

/// Rang, dans `CATEGORY_NAMES`, de la catégorie d'un bâtiment.
fn category_of(function: Function) -> usize {
    match function {
        Function::House => 0,
        Function::Supplier(Raw::Bronze) | Function::Producer(Good::Wood | Good::Marble | Good::BlackMarble | Good::Orichalc) => 2,
        Function::Farm | Function::Producer(_) | Function::Fishery | Function::Supplier(Raw::Crops) => 1,
        Function::Workshop(_) => 2,
        Function::Granary | Function::Storehouse | Function::TradePost { .. } | Function::Agora { .. } | Function::Vendor(_) => 3,
        // Les emplacements ne se bâtissent qu'à travers l'agora.
        Function::Plot => usize::MAX,
        Function::Service(Service::Water | Service::Maintenance | Service::Security | Service::Health) => 4,
        Function::Service(Service::Tax) | Function::Palace => 5,
        Function::Service(Service::Philosophy | Service::Athletics | Service::Drama) | Function::School(_) | Function::Stadium | Function::Museum => 6,
        // L'annexe du stade ne se bâtit qu'avec lui.
        Function::Annex => usize::MAX,
        Function::Sanctuary { .. } | Function::HeroHall(_) | Function::Monument { .. } => 7,
        Function::Wharf | Function::Tower | Function::Gatehouse => 9,
        Function::Racetrack => 5,
        Function::Commemorative(_) => COMMEMORATIVE_CATEGORY,
        // Le barrage se pose avec son bouton rond, sous les onglets.
        Function::Roadblock => usize::MAX,
        Function::None => 8,
    }
}

/// Taille en pixels d'une tuile de terrain.
const TILE_WIDTH: usize = 58;
const TILE_HEIGHT: usize = 30;

/// Mini-carte : la grille vue en losange, deux pixels de large par case.
const MINIMAP_WIDTH: usize = 2 * GRID;
const MINIMAP_HEIGHT: usize = GRID;
/// Images entre deux mises à jour de la mini-carte.
const MINIMAP_REFRESH: u32 = 30;
/// Place maximale de la mini-carte à l'écran.
const MINIMAP_MAX_WIDTH: f32 = 420.0;
const MINIMAP_MAX_HEIGHT: f32 = 230.0;
/// Vitesses de jeu : pourcentage affiché et durée d'un pas de simulation en
/// millisecondes. La table vient de `Zeus.exe` (0, 20, 35, 55, 80, 110, 160, 240, 350,
/// 500, 700 à l'adresse de fichier 0x23da18, la même que dans Caesar III), lue comme
/// dans ce dernier : rang = (100 − vitesse) / 10. À 100 %, l'original ne temporise
/// plus du tout ; on borne ici à 10 ms.
const SPEEDS: [(u64, u64); 10] =
    [(10, 500), (20, 350), (30, 240), (40, 160), (50, 110), (60, 80), (70, 55), (80, 35), (90, 20), (100, 10)];
/// Vitesse au lancement : 90 %. La routine de `Zeus.exe` qui pose les réglages par
/// défaut (adresse de fichier 0x1c4544) écrit 90 puis 60 dans deux réglages voisins,
/// comme Caesar III le fait pour la vitesse de jeu et celle du défilement.
const NORMAL_SPEED: usize = 8;

/// Couleur d'une case sur la mini-carte.
fn minimap_colour(world: &World, x: usize, y: usize) -> [u8; 4] {
    if world.map.image(x, y) == 0 {
        return [0, 0, 0, 255];
    }
    if let Some(building) = world.building_at(x as i32, y as i32) {
        return match world.kinds[building.kind].function {
            Function::House => [200, 70, 50, 255],
            Function::None => [120, 200, 110, 255],
            Function::Farm | Function::Supplier(Raw::Crops) | Function::Producer(_) => [225, 200, 80, 255],
            _ => [240, 240, 240, 255],
        };
    }
    if world.is_road(x as i32, y as i32) {
        return [235, 220, 185, 255];
    }
    match world.map.terrain(x, y) {
        t if t & TERRAIN_WATER != 0 => [40, 100, 160, 255],
        t if t & TERRAIN_TREE != 0 => [40, 90, 40, 255],
        t if t & TERRAIN_ROCK != 0 => [150, 150, 150, 255],
        t if t & TERRAIN_BEACH != 0 => [225, 210, 160, 255],
        t if t & TERRAIN_MEADOW != 0 => [110, 150, 60, 255],
        _ => [170, 150, 80, 255],
    }
}

/// Pixels de la mini-carte, en RVBA, tournée comme la vue.
fn minimap_pixels(world: &World) -> Vec<u8> {
    let mut pixels = vec![0u8; MINIMAP_WIDTH * MINIMAP_HEIGHT * 4];
    for y in 0..GRID {
        for x in 0..GRID {
            let (mx, my) = zeus::world::unrotate(orientation(), x as i32, y as i32);
            let colour = minimap_colour(world, mx as usize, my as usize);
            let (px, py) = (x + GRID - 1 - y, (x + y) / 2);
            for column in px..(px + 2).min(MINIMAP_WIDTH) {
                pixels[(py * MINIMAP_WIDTH + column) * 4..][..4].copy_from_slice(&colour);
            }
        }
    }
    pixels
}

/// Place de la mini-carte à l'écran : seule la zone jouable de la texture est
/// montrée, agrandie pour remplir le cadre.
#[derive(Clone, Copy, Default)]
struct Minimap {
    /// Coin haut-gauche à l'écran, et pixels d'écran par pixel de texture.
    x: f32,
    y: f32,
    scale: f32,
    /// Échelle de la mini-carte flottante, en bas à gauche.
    base: f32,
    /// Zone jouable dans la texture : coin et taille.
    left: f32,
    top: f32,
    width: f32,
    height: f32,
}

impl Minimap {
    /// Cadre de la zone jouable d'une carte dans la texture de la mini-carte.
    fn new(world: &World) -> Minimap {
        let (mut x0, mut y0, mut x1, mut y1) = (MINIMAP_WIDTH, MINIMAP_HEIGHT, 0, 0);
        for y in 0..GRID {
            for x in 0..GRID {
                if world.map.image(x, y) != 0 {
                    let (x, y) = zeus::world::rotate(orientation(), x as i32, y as i32);
                    let (x, y) = (x as usize, y as usize);
                    let (px, py) = (x + GRID - 1 - y, (x + y) / 2);
                    (x0, y0, x1, y1) = (x0.min(px), y0.min(py), x1.max(px + 2), y1.max(py + 1));
                }
            }
        }
        let (width, height) = (x1.saturating_sub(x0).max(1) as f32, y1.saturating_sub(y0).max(1) as f32);
        let scale = (MINIMAP_MAX_WIDTH / width).min(MINIMAP_MAX_HEIGHT / height);
        Minimap { x: 0.0, y: 0.0, scale, base: scale, left: x0 as f32, top: y0 as f32, width, height }
    }

    /// Taille à l'écran.
    fn size(&self) -> (f32, f32) {
        (self.width * self.scale, self.height * self.scale)
    }

    /// Point du monde désigné par un clic à l'écran, s'il tombe sur la mini-carte.
    fn target(&self, mx: f32, my: f32) -> Option<(f32, f32)> {
        let (rx, ry) = ((mx - self.x) / self.scale, (my - self.y) / self.scale);
        let inside = rx >= 0.0 && ry >= 0.0 && rx < self.width && ry < self.height;
        inside.then(|| ((rx + self.left - GRID as f32) * HALF_WIDTH, (ry + self.top) * 2.0 * HALF_HEIGHT))
    }

    /// Position à l'écran d'un point du monde, ramenée dans le cadre.
    fn place(&self, wx: f32, wy: f32) -> (f32, f32) {
        let x = (wx / HALF_WIDTH + GRID as f32 - self.left).clamp(0.0, self.width);
        let y = (wy / HALF_HEIGHT / 2.0 - self.top).clamp(0.0, self.height);
        (self.x + x * self.scale, self.y + y * self.scale)
    }
}

/// Centre du bandeau bleu sur le fond de l'écran de choix, en pixels de l'image.
const CHOOSER_BANNER: (f32, f32) = (512.0, 127.0);

/// Outils d'une catégorie, dans l'ordre du menu ; la route ouvre la première.
fn category_tools(world: &World, category: usize) -> Vec<Tool> {
    let road = match category {
        0 => Some(Tool::Road),
        MILITARY_CATEGORY => Some(Tool::Wall),
        _ => None,
    };
    // Un monument commémoratif n'est proposé que s'il a été gagné.
    let offered = |k: &usize| {
        world.locked.get(*k) != Some(&true)
            && !matches!(world.kinds[*k].function, Function::Commemorative(model) if !world.commemoratives.contains(&model))
    };
    let kinds = (0..world.kinds.len()).filter(|&k| category_of(world.kinds[k].function) == category).filter(offered).map(Tool::Build);
    road.into_iter().chain(kinds).collect()
}

/// Bouton du panneau (première de trois images de `zeus_interface.sg3` : repos,
/// survol, choisi) sous lequel le jeu range un outil ; attribution faite à l'œil sur les
/// images des boutons.
fn tool_button(world: &World, tool: Tool) -> usize {
    let Tool::Build(kind) = tool else { return 1585 };
    let k = &world.kinds[kind];
    match k.function {
        Function::House if k.size > 2 => 1507,
        Function::House => 1504,
        Function::Farm => 1510,
        Function::Supplier(Raw::Crops) => 1513,
        Function::Producer(Good::Fleece) => 1516,
        Function::Producer(Good::Food) if k.parts.first().is_some_and(|p| p.image == HUNTING_LODGE_IMAGE) => 1519,
        Function::Producer(Good::Food) => 1516,
        Function::Fishery => 1519,
        Function::Producer(Good::Wood | Good::Marble | Good::BlackMarble | Good::Orichalc) | Function::Supplier(Raw::Bronze) => 1522,
        Function::Workshop(Good::Arms) | Function::Producer(Good::Horses) | Function::Wharf => 1588,
        Function::Workshop(_) => 1525,
        Function::Granary => 1531,
        Function::Storehouse | Function::TradePost { .. } => 1534,
        Function::Agora { .. } | Function::Plot | Function::Vendor(_) => 1537,
        Function::Service(Service::Water) => 1546,
        Function::Service(Service::Maintenance) => 1543,
        Function::Service(Service::Health) => 1552,
        Function::Service(Service::Security) => 1555,
        Function::Service(Service::Tax) => 1558,
        Function::Palace => 1549,
        Function::School(Service::Philosophy) | Function::Service(Service::Philosophy) => 1567,
        Function::Service(Service::Athletics) => 1570,
        Function::School(_) | Function::Service(Service::Drama) => 1573,
        Function::Stadium | Function::Annex | Function::Museum | Function::Racetrack => 1576,
        Function::Sanctuary { .. } | Function::Monument { .. } => 1579,
        Function::HeroHall(_) => 1582,
        Function::Tower | Function::Gatehouse => 1585,
        Function::None if k.size == 1 => 1591,
        Function::None if k.size >= 3 => 1594,
        _ => 1597,
    }
}

/// Groupes de boutons d'une catégorie, dans l'ordre du menu : (bouton, outils).
fn category_groups(world: &World, category: usize) -> Vec<(usize, Vec<(usize, Tool)>)> {
    let mut groups: Vec<(usize, Vec<(usize, Tool)>)> = Vec::new();
    for (slot, tool) in category_tools(world, category).into_iter().enumerate() {
        if tool == Tool::Road {
            continue;
        }
        let button = tool_button(world, tool);
        match groups.iter_mut().find(|(b, _)| *b == button) {
            Some((_, tools)) => tools.push((slot, tool)),
            None => groups.push((button, vec![(slot, tool)])),
        }
    }
    groups
}

/// Ce que fait un clic sur une ligne du menu.
#[derive(Clone, Copy, PartialEq)]
enum MenuRow {
    /// Ouvre ou referme une catégorie.
    Category(usize),
    Pick(Tool),
    /// Prépare la compagnie à envoyer soumettre cette cité du monde, ou à la piller.
    Conquer(usize, bool),
    /// Change l'effectif d'un rang (0 à 2) ou des trières (3) de la compagnie préparée.
    CompanyCount(usize, i32),
    /// Remplit la compagnie préparée avec tout ce qui reste à la cité.
    CompanyAll,
    /// Envoie la compagnie préparée.
    CompanySend,
    /// Referme la composition sans envoyer.
    CompanyCancel,
    /// Fond de la composition : absorbe les clics destinés à la carte.
    CompanyShield,
    /// Satisfait la demande de ce rang.
    Fulfil(usize),
    /// Change ce que le commerce peut faire de cette ressource.
    Policy(usize),
    /// Bascule une vue en surimpression (boutons « Voir… » du panneau).
    Overlay(Overlay),
    /// Zone des messages du panneau : ouvre le journal.
    Journal,
    /// Menu du bandeau (Fichier, Options, Aide) : l'ouvre ou le referme.
    TopMenu(usize),
    /// Onglets du haut du panneau : constructions, ou carte générale.
    SidebarMap(bool),
    /// Bouton d'un groupe de bâtiments (image du bouton) dans le panneau.
    Group(usize),
    /// Bouton du panneau latéral : comme la touche donnée.
    Key(u32),
}

/// Compagnie en préparation pour une campagne lointaine : ce que le joueur a choisi
/// d'envoyer, cité par cité, avant de confirmer.
#[derive(Clone, Copy)]
struct CompanyDraft {
    city: usize,
    raid: bool,
    soldiers: [i32; 3],
    triremes: i32,
}

/// Outil tenu par le joueur.
#[derive(Clone, Copy, PartialEq)]
enum Tool {
    None,
    Road,
    /// Mur, tracé comme une route (catégorie Armée).
    Wall,
    Demolish,
    Build(usize),
}

fn main() {
    // Sous X11, SDL ne donne un titre de fenêtre accentué correct qu'avec la locale
    // de l'utilisateur ; un programme Rust reste sinon dans la locale « C ».
    #[cfg(target_os = "linux")]
    {
        unsafe extern "C" {
            fn setlocale(category: i32, locale: *const std::ffi::c_char) -> *mut std::ffi::c_char;
        }
        const LC_ALL: i32 = 6;
        // SAFETY : appelé au démarrage, avant tout autre fil ; la chaîne vide est valide.
        unsafe {
            setlocale(LC_ALL, c"".as_ptr());
        }
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|first| first == "installer") {
        install(&args[1..]);
        return;
    }
    let (mut demo, mut capture, mut warmup) = (false, None, CAPTURE_WARMUP_TICKS);
    let mut intro_video = true;
    let mut script = String::new();
    let mut size = (1600, 900);
    let mut measure = 0u32;
    let mut data = paths::data_dir();
    let mut map = None;
    let mut options = args.iter();
    while let Some(option) = options.next() {
        match (option.as_str(), options.clone().next()) {
            ("--aide" | "--help" | "-h", _) => {
                println!("{USAGE}");
                return;
            }
            ("--demo", _) => demo = true,
            ("--bilan", _) => AUDIT.store(true, std::sync::atomic::Ordering::Relaxed),
            ("--sans-intro", _) => intro_video = false,
            ("--difficulte", Some(level)) => {
                let wanted = ["debutant", "mortel", "heros", "titan", "olympien"].iter().position(|&n| n == level.to_lowercase().replace(['é', 'è'], "e"));
                match wanted {
                    Some(level) => {
                        DIFFICULTY.store(level, std::sync::atomic::Ordering::Relaxed);
                        options.next();
                    }
                    None => {
                        eprintln!("difficulté inconnue : {level} (debutant, mortel, heros, titan, olympien)");
                        std::process::exit(2);
                    }
                }
            }
            ("--donnees", Some(folder)) => {
                data = PathBuf::from(folder);
                options.next();
            }
            ("--capture", Some(file)) => {
                capture = Some(Path::new(file));
                options.next();
            }
            ("--size", Some(wanted)) => {
                let parsed = wanted.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)));
                size = parsed.unwrap_or(size);
                options.next();
            }
            ("--script", Some(steps)) => {
                script = steps.clone();
                options.next();
            }
            ("--mesure", Some(count)) if count.parse::<u32>().is_ok() => {
                measure = count.parse().unwrap();
                options.next();
            }
            ("--ticks", Some(count)) if count.parse::<u32>().is_ok() => {
                warmup = count.parse().unwrap();
                options.next();
            }
            // Sans tiret : la carte ou l'aventure à lancer.
            (file, _) if !file.starts_with("--") && map.is_none() => map = Some(file.to_string()),
            _ => {
                eprintln!("{USAGE}");
                std::process::exit(2)
            }
        }
    }
    let data = data.as_path();
    if !data.join("zeus_text.eng").exists() {
        eprintln!("Les données du jeu ne sont pas installées dans {}.", data.display());
        eprintln!("Installez-les depuis votre exemplaire du jeu : un dossier où il est installé,");
        eprintln!("ou les images (ou le contenu) des CD de Zeus puis de Poséidon :");
        eprintln!("    olympe installer <source> [<source>…]");
        std::process::exit(1);
    }
    // Bilan : la première aventure installée sert à charger les règles et les images.
    let map = match map {
        None if AUDIT.load(std::sync::atomic::Ordering::Relaxed) => adventures(data).into_iter().next().map(|(_, pak)| pak.to_string_lossy().into_owned()),
        map => map,
    };
    // Vidéo d'introduction du jeu, au lancement d'une vraie partie : lue par Olympe
    // sur l'écran des aventures si elle a été convertie, sinon confiée à un lecteur.
    let wants_intro = intro_video && capture.is_none() && script.is_empty() && measure == 0 && !AUDIT.load(std::sync::atomic::Ordering::Relaxed);
    let converted = data.join(zeus::install::INTRO_FRAMES).join("0001.jpg").exists();
    INTRO.store(wants_intro && converted && map.is_none(), std::sync::atomic::Ordering::Relaxed);
    if wants_intro && (!converted || map.is_some()) {
        if !converted {
            play_intro(data);
        }
    }
    let chosen = match map {
        // Un nom qui n'est pas un fichier désigne une aventure installée.
        Some(map) => {
            let direct = PathBuf::from(&map);
            let installed = adventures(data).into_iter().find(|(name, _)| name.to_lowercase() == map.to_lowercase());
            Ok(Some(match installed {
                Some((_, pak)) if !direct.exists() => pak,
                _ => direct,
            }))
        }
        // En capture sans carte, c'est l'écran de choix lui-même qu'on photographie.
        None => choose_adventure(data, capture, &script, size),
    };
    let outcome = match chosen {
        Ok(Some(map)) => run(data, &map, demo, capture, warmup, &script, size, measure),
        Ok(None) => Ok(()),
        Err(e) => Err(e),
    };
    if let Err(e) = outcome {
        eprintln!("erreur : {e}");
        std::process::exit(1);
    }
}

/// Charge une carte seule ou une campagne : la carte de la cité mère, celles des
/// colonies dans l'ordre de leurs épisodes, et les réglages de l'aventure quand on les
/// trouve (dans le `.pak`, ou dans un `.set` voisin de la carte).
fn load_map(path: &Path) -> Result<(Map, Vec<Map>, Option<Settings>), String> {
    let file = fs::read(path).map_err(|e| format!("{} : {e}", path.display()))?;
    if file.starts_with(b"MAPS") {
        let neighbours = path.parent().and_then(|dir| fs::read_dir(dir).ok());
        let set = neighbours.into_iter().flatten().flatten().map(|entry| entry.path()).find(|p| p.extension().is_some_and(|e| e == "set"));
        let settings = set.and_then(|p| fs::read(p).ok()).and_then(|data| Settings::parse(&data));
        // Aventure en dossier : la cité mère est « …p.map », les colonies « …c1.map »,
        // « …c2.map »…
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let colony_files: Vec<PathBuf> = match stem.strip_suffix('p') {
            Some(base) => (1..).map(|k| path.with_file_name(format!("{base}c{k}.map"))).take_while(|p| p.exists()).collect(),
            None => Vec::new(),
        };
        let colonies = colony_files
            .iter()
            .map(|p| Map::parse(&fs::read(p).map_err(|e| format!("{} : {e}", p.display()))?))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok((Map::parse(&file)?, colonies, settings));
    }
    let mut campaign = Campaign::parse(&file)?;
    // Cartes : copie de travail, cité mère, puis les colonies.
    let colonies = campaign.maps.split_off(campaign.maps.len().min(2));
    let parent = if campaign.maps.len() > 1 { 1 } else { 0 };
    Ok((campaign.maps.swap_remove(parent), colonies, campaign.settings))
}

/// Contenu du fichier de textes d'une aventure.
fn adventure_texts(file: &Path) -> Option<String> {
    Some(fs::read(file).ok()?.iter().map(|&b| zeus::text::cp1252(b)).collect())
}

/// Valeur de l'entrée `clé="…"` d'un fichier de textes d'aventure ; elle peut
/// s'étendre sur plusieurs lignes, jusqu'au guillemet fermant.
fn text_entry(text: &str, key: &str) -> Option<String> {
    let opening = format!("{key}=\"");
    // La clé doit ouvrir une ligne, pour ne pas la trouver citée dans un commentaire.
    let start = text.match_indices(&opening).map(|(at, _)| at).find(|&at| at == 0 || text[..at].ends_with('\n'))? + opening.len();
    let value = &text[start..start + text[start..].find('"')?];
    Some(value.replace(['\r', '\n'], " ").trim().to_string()).filter(|value| !value.is_empty())
}

/// Moment d'une aventure qui a son récit.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Moment {
    Opening,
    Victory,
    /// Fin de l'aventure, après le récit de victoire du dernier épisode.
    End,
}

/// Récit d'un épisode : son titre et ses paragraphes. Les aventures livrées en dossier
/// l'apportent dans un fichier de textes ; les autres le prennent dans les messages du
/// jeu, au numéro donné par leur en-tête. L'ouverture du premier épisode commence par
/// la présentation de l'aventure.
fn story(data: &Path, map_path: &Path, settings: Option<&Settings>, step: Step, moment: Moment) -> Option<(String, Vec<String>)> {
    let folder = map_path.parent()?;
    let texts = fs::read_dir(folder).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e == "txt"));
    // Un fichier de textes ne compte que dans le dossier propre à une aventure.
    if let Some(file) = texts.filter(|_| folder.file_name().is_some_and(|name| name != "adventures")) {
        let text = adventure_texts(&file)?;
        let entry = |key: &str| text_entry(&text, key);
        if moment == Moment::End {
            return Some((entry("Adventure_Title")?, messages::paragraphs(&entry("Adventure_Complete")?)));
        }
        let prefix = match step {
            Step::Parent(i) => format!("Parent_Episode_{}", i + 1),
            Step::Colony(i) => format!("Colony_{}", i + 1),
        };
        let title = entry(&format!("{prefix}_Title"));
        let (heading, mut paragraphs) = match (moment, step) {
            (Moment::Opening, Step::Parent(0)) => {
                let mut paragraphs = messages::paragraphs(&entry("Adventure_Introduction").unwrap_or_default());
                paragraphs.extend(title);
                (entry("Adventure_Title")?, paragraphs)
            }
            _ => (title.unwrap_or_default(), Vec::new()),
        };
        let part = if moment == Moment::Opening { "Introduction" } else { "Complete" };
        paragraphs.extend(messages::paragraphs(&entry(&format!("{prefix}_{part}")).unwrap_or_default()));
        return Some((heading, paragraphs)).filter(|(_, paragraphs)| !paragraphs.is_empty());
    }
    let episode = settings?.episode(step)?;
    let id = match moment {
        Moment::Opening => episode.intro_message,
        Moment::Victory => episode.victory_message,
        Moment::End => return None,
    };
    let message = Messages::parse(fs::read(data.join("zeus_mm.eng")).ok()?).ok()?.get(usize::try_from(id).ok()?)?;
    Some((message.title, message.paragraphs))
}

/// Fichier de narration d'un récit. Aventures de Poséidon et aventures en dossier :
/// `<aventure>_p<n>_i.opus` (introduction de l'épisode n de la cité mère), `_v`
/// (victoire), `_c<n>_…` pour les colonies et `_a_v` pour la fin. Aventures de
/// Zeus : `c<k>_e<n>_i.opus`, les colonies à partir de l'épisode 11, `c<k>_v` pour la
/// fin ; la campagne k se déduit du premier récit (290, 318… par pas de 28 dans
/// `zeus_mm.eng`, dans l'ordre de la table de Zeus.exe).
fn narration(data: &Path, map_path: &Path, settings: Option<&Settings>, step: Step, moment: Moment) -> Option<PathBuf> {
    let voices = data.join("audio/voice/campaign");
    let kind = match moment {
        Moment::Opening => "i",
        _ => "v",
    };
    let folder = map_path.parent()?;
    let in_folder = folder.file_name().is_some_and(|name| name != "adventures");
    let stem = if in_folder { folder.file_name()?.to_string_lossy().into_owned() } else { map_path.file_stem()?.to_string_lossy().into_owned() };
    let named = match (moment, step) {
        (Moment::End, _) => format!("{stem}_a_v.opus"),
        (_, Step::Parent(i)) => format!("{stem}_p{}_{kind}.opus", i + 1),
        (_, Step::Colony(i)) => format!("{stem}_c{}_{kind}.opus", i + 1),
    };
    let candidates = [folder.join(&named), voices.join(&named)];
    if let Some(found) = candidates.into_iter().find(|p| p.exists()) {
        return Some(found);
    }
    let first = settings?.parent.first()?.intro_message;
    let campaign = (first - ZEUS_FIRST_CAMPAIGN_MESSAGE) / ZEUS_CAMPAIGN_MESSAGES + 1;
    if (first - ZEUS_FIRST_CAMPAIGN_MESSAGE) % ZEUS_CAMPAIGN_MESSAGES != 0 || !(1..=ZEUS_CAMPAIGNS).contains(&campaign) {
        return None;
    }
    let file = match (moment, step) {
        (Moment::End, _) => format!("c{campaign}_v.opus"),
        (_, Step::Parent(i)) => format!("c{campaign}_e{}_{kind}.opus", i + 1),
        (_, Step::Colony(i)) => format!("c{campaign}_e{}_{kind}.opus", i + 11),
    };
    Some(voices.join(file)).filter(|p| p.exists())
}

/// Premier récit de la première campagne de Zeus (Hercule), écart entre deux
/// campagnes, et nombre de campagnes.
const ZEUS_FIRST_CAMPAIGN_MESSAGE: i32 = 290;
const ZEUS_CAMPAIGN_MESSAGES: i32 = 28;
const ZEUS_CAMPAIGNS: i32 = 7;

/// Niveau de difficulté (groupe de textes 153 : Débutant, Mortel, Héros, Titan,
/// Olympien), choisi sur l'écran des aventures ou par `--difficulte` ; Héros par
/// défaut. Chacun a son fichier de règles, et le trésor initial est augmenté de
/// moitié en Débutant, diminué d'un cinquième en Olympien (manuel de l'éditeur).
static DIFFICULTY: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(2);
const DIFFICULTY_MODELS: [&str; 5] = ["veryeasy", "easy", "normal", "hard", "impossible"];
const DIFFICULTY_FUNDS_PERCENT: [i32; 5] = [150, 100, 100, 100, 80];
const TEXT_DIFFICULTY: usize = 153;

fn difficulty() -> usize {
    DIFFICULTY.load(std::sync::atomic::Ordering::Relaxed).min(DIFFICULTY_MODELS.len() - 1)
}

/// Joue l'introduction convertie à l'installation (images JPEG et son Opus) dans la
/// fenêtre du jeu ; un clic ou une touche l'interrompt. Rend `None` si elle n'est
/// pas là, sinon si le joueur a fermé la fenêtre.
///
/// SAFETY : `renderer` est un rendu SDL valide.
unsafe fn play_intro_frames(renderer: sdl::Renderer, data: &Path) -> Option<bool> {
    let folder = data.join(zeus::install::INTRO_FRAMES);
    let mut frames: Vec<PathBuf> = fs::read_dir(&folder).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "jpg")).collect();
    frames.sort();
    let first = frames.first().and_then(|f| fs::read(f).ok()).and_then(|f| jpeg::decode(&f).ok())?;
    let (w, h) = (first.width as i32, first.height as i32);
    // SAFETY : appels directs à SDL3 ; la texture est vérifiée puis détruite à la fin.
    unsafe {
        let texture = sdl::SDL_CreateTexture(renderer, sdl::PIXELFORMAT_ABGR8888, sdl::TEXTUREACCESS_STATIC, w, h);
        if texture.is_null() {
            return None;
        }
        let mut closed = false;
        sdl::SDL_SetTextureScaleMode(texture, sdl::SCALEMODE_LINEAR);
        let mut voice = Voice::open().ok();
        if let Some(voice) = voice.as_mut() {
            voice.play(&folder.join("son.opus"));
        }
        let start = sdl::SDL_GetTicks();
        let (mut event, mut shown) = (sdl::Event::new(), usize::MAX);
        'video: loop {
            while sdl::SDL_PollEvent(&mut event) {
                if matches!(event.kind(), sdl::EVENT_QUIT | sdl::EVENT_KEY_DOWN | sdl::EVENT_MOUSE_BUTTON_DOWN) {
                    closed = event.kind() == sdl::EVENT_QUIT;
                    break 'video;
                }
            }
            let index = ((sdl::SDL_GetTicks() - start) * zeus::install::INTRO_RATE / 1000) as usize;
            if index >= frames.len() {
                break;
            }
            if index != shown {
                if let Some(picture) = fs::read(&frames[index]).ok().and_then(|f| jpeg::decode(&f).ok()).filter(|p| (p.width as i32, p.height as i32) == (w, h)) {
                    sdl::SDL_UpdateTexture(texture, null(), picture.rgba.as_ptr() as *const c_void, w * 4);
                }
                shown = index;
            }
            if let Some(voice) = voice.as_mut() {
                voice.update();
            }
            // L'image garde ses proportions, centrée sur fond noir.
            let (mut ww, mut wh) = (0, 0);
            sdl::SDL_GetRenderOutputSize(renderer, &mut ww, &mut wh);
            sdl::SDL_SetRenderScale(renderer, 1.0, 1.0);
            let scale = (ww as f32 / w as f32).min(wh as f32 / h as f32);
            let place = sdl::FRect { x: (ww as f32 - w as f32 * scale) / 2.0, y: (wh as f32 - h as f32 * scale) / 2.0, w: w as f32 * scale, h: h as f32 * scale };
            sdl::SDL_SetRenderDrawColor(renderer, 0, 0, 0, 255);
            sdl::SDL_RenderClear(renderer);
            sdl::SDL_RenderTexture(renderer, texture, null(), &place);
            sdl::SDL_RenderPresent(renderer);
            sdl::SDL_Delay(5);
        }
        sdl::SDL_DestroyTexture(texture);
        Some(closed)
    }
}

/// Joue la vidéo d'introduction (`binks/zeus_intro_hiq.webm`, convertie à
/// l'installation) avec le premier lecteur trouvé sur le système ; Olympe ne décode
/// pas lui-même la vidéo. Échap ou Q l'interrompent. Sans lecteur, rien.
fn play_intro(data: &Path) {
    let Some(video) = ["binks/zeus_intro_hiq.webm", "binks/zeus_intro.webm"].iter().map(|f| data.join(f)).find(|p| p.exists()) else { return };
    let players: [(&str, &[&str]); 3] = [
        ("ffplay", &["-autoexit", "-fs", "-loglevel", "quiet", "-window_title", "Zeus"]),
        ("mpv", &["--fs", "--really-quiet", "--no-terminal"]),
        ("cvlc", &["--fullscreen", "--play-and-exit", "--quiet"]),
    ];
    for (player, options) in players {
        if let Ok(status) = std::process::Command::new(player).args(options).arg(&video).stdin(std::process::Stdio::null()).status() {
            if status.success() || status.code().is_some() {
                return;
            }
        }
    }
}

/// Introduction à jouer sur l'écran des aventures.
static INTRO: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// `--bilan` demandé.
static AUDIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Bilan de jouabilité : pour chaque épisode de chaque aventure installée, sur sa
/// carte et avec ses bâtiments permis, chaque objectif doit être suivi par Olympe et
/// avoir de quoi être atteint (un bâtiment permis qui produit la marchandise, une
/// cité qui la vend, le temple du héros de la quête…).
fn audit(data: &Path, base: &World, text: &Text) -> Result<(), String> {
    let (mut episodes, mut playable) = (0, 0);
    for (name, pak) in adventures(data) {
        let Ok((parent, colonies, Some(settings))) = load_map(&pak) else { continue };
        let mut colony = 0;
        for step in settings.order() {
            let Some(episode) = settings.episode(step) else { continue };
            let map = match step {
                Step::Parent(_) => parent.clone(),
                Step::Colony(_) => {
                    colony += 1;
                    colonies.get(colony - 1).cloned().unwrap_or_else(|| parent.clone())
                }
            };
            let mut world = base.new_city(map, settings.funds);
            if !parent.cities().is_empty() {
                world.cities = parent.cities();
                world.routes = parent.routes();
            }
            lock_resources(&mut world, episode);
            begin_events(&mut world, episode);
            let open = |f: &dyn Fn(&Function) -> bool| world.kinds.iter().enumerate().any(|(i, k)| f(&k.function) && !world.locked.get(i).copied().unwrap_or(false));
            let makes = |resource: i32| world.kinds.iter().enumerate().any(|(i, k)| k.product as i32 == resource && !world.locked.get(i).copied().unwrap_or(false));
            // Une cité inactive peut s'ouvrir au commerce par un événement.
            let sold = |resource: i32| world.cities.iter().any(|c| c.sells.iter().any(|&(r, _)| r as i32 == resource));
            // Un bâtiment permis qui produit la ressource trouve une place sur la carte.
            let placeable = |resource: i32| {
                world.kinds.iter().enumerate().filter(|(i, k)| k.product as i32 == resource && !world.locked.get(*i).copied().unwrap_or(false)).any(|(i, _)| {
                    (0..GRID as i32 * GRID as i32).any(|t| world.can_place(i, t % GRID as i32, t / GRID as i32))
                })
            };
            // Note de culture la plus haute que permettent les bâtiments autorisés.
            let culture_allowed = {
                let philosophy = open(&|f| *f == Function::Service(Service::Philosophy)) && open(&|f| *f == Function::School(Service::Philosophy));
                let athletics = open(&|f| *f == Function::Service(Service::Athletics));
                let drama = open(&|f| *f == Function::Service(Service::Drama)) && open(&|f| *f == Function::School(Service::Drama));
                let stadium = athletics && open(&|f| matches!(f, Function::Stadium | Function::Museum));
                let (p, a) = if episode.atlantean { (20, 15) } else { (15, 20) };
                p * philosophy as i32 + a * athletics as i32 + 25 * drama as i32 + 30 * stadium as i32
            };
            let mut problems = Vec::new();
            for goal in &episode.goals {
                let [first, second, _] = goal.values;
                let label = goal.describe(text, &[], matches!(step, Step::Colony(_)));
                let problem = match (goal.status(&world), goal.kind) {
                    (Status::Unsupported, _) => Some("non suivi"),
                    (_, GoalKind::Production) if !makes(first) => Some("aucun bâtiment permis ne le produit"),
                    (_, GoalKind::Production) if !placeable(first) => Some("aucun emplacement pour le produire"),
                    (_, GoalKind::SetAside) if first > 0 && !sold(first) && makes(first) && !placeable(first) => Some("aucun emplacement pour le produire"),
                    (_, GoalKind::SetAside) if first > 0 && !makes(first) && !sold(first) => Some("ni produit ni vendu par une cité"),
                    (_, GoalKind::Sanctuary) if first >= 0 && !open(&|f| matches!(f, Function::Sanctuary { god, .. } if *god as i32 == first)) => Some("sanctuaire non permis"),
                    (_, GoalKind::Pyramid) if !open(&|f| matches!(f, Function::Monument { .. })) => Some("monument non permis"),
                    (_, GoalKind::Hippodrome) if !open(&|f| *f == Function::Racetrack) => Some("hippodrome non permis"),
                    (_, GoalKind::Support) if second > 0 && first == 57 && !open(&|f| *f == Function::Wharf) => Some("quai non permis"),
                    (_, GoalKind::Quest | GoalKind::Slay) if !open(&|f| matches!(f, Function::HeroHall(_))) => Some("aucun temple de héros permis"),
                    (_, GoalKind::Housing) if culture_allowed < world.level_culture(zeus::goals::housing_level(first)) => Some("culture permise insuffisante"),
                    _ => None,
                };
                if let Some(problem) = problem {
                    problems.push(format!("{label} : {problem}"));
                }
            }
            episodes += 1;
            if problems.is_empty() {
                playable += 1;
            } else {
                println!("{name} {step:?} : {}", problems.join(" ; "));
            }
        }
    }
    println!("{playable} épisodes sur {episodes} ont tous leurs objectifs suivis et réalisables");
    Ok(())
}

/// L'épisode dit ce que la cité peut produire : les exploitations et ateliers des
/// autres ressources ne sont pas proposés. Tant que le moteur ne sait nourrir une cité
/// que par ses exploitations et ses laiteries, ses pavillons de chasse et ses
/// pêcheries, la restriction ne s'applique qu'aux épisodes qui en permettent au moins
/// une : les autres resteraient sans vivres.
/// Rangs de la liste « Bâtiments autorisés » de l'éditeur (groupe de textes 67 de
/// `zeus_editor_text.eng`) et types de bâtiments qu'ils couvrent. Les fermes, l'élevage
/// (pêcherie, pavillon de chasse, enclos compris, bien que la liste leur garde un rang)
/// et les matières premières n'en dépendent pas : seules les ressources en décident
/// (guide de l'éditeur de Poséidon).
const ALLOWED_TYPES: [(usize, &[usize]); 48] = [
    (1, &[48]),
    (3, &[54]),
    (4, &[64]),
    (7, &[57]),
    (8, &[63]),
    (9, &[58, 59]),
    (10, &[60, 62]),
    (11, &[80]),
    (12, &[82]),
    (13, &[79]),
    (14, &[53]),
    (15, &[55]),
    (16, &[81]),
    (17, &[77]),
    (18, &[74]),
    (19, &[133]),
    (20, &[78]),
    (21, &[121]),
    (22, &[76]),
    (23, &[122]),
    (24, &[124]),
    (25, &[117]),
    (26, &[125]),
    (27, &[123]),
    (28, &[120]),
    (29, &[129, 145, 146]),
    (30, &[128]),
    (31, &[118]),
    (32, &[126]),
    (33, &[130]),
    (34, &[132]),
    (35, &[131]),
    (36, &[127]),
    (37, &[16]),
    (38, &[15]),
    (39, &[17]),
    (40, &[18]),
    (41, &[135]),
    (42, &[136]),
    (43, &[9, 10, 11, 12, 13, 14]),
    (44, &[199]),
    (45, &[212]),
    (46, &[19]),
    (47, &[20]),
    (48, &[21]),
    (49, &[22]),
    (50, &[23]),
    (51, &[24]),
];

/// Rang de chaque type de bâtiment d'Olympe dans la liste des bâtiments autorisés,
/// retrouvé par le nom du type dans les textes du jeu ; fixé au chargement.
static ALLOWED_SLOT: std::sync::OnceLock<Vec<Option<usize>>> = std::sync::OnceLock::new();
/// Nation qui peut bâtir chaque type d'Olympe : 0 toutes, 1 grecque, 2 atlante.
static NATION: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();

fn nations(kinds: &[Kind], text: &Text) -> Vec<u8> {
    let named = |types: &[usize], kind: &Kind| types.iter().any(|&t| text.get(TEXT_BUILDINGS, t) == kind.name);
    kinds.iter().map(|kind| if named(&GREEK_ONLY, kind) { 1 } else if named(&ATLANTEAN_ONLY, kind) || kind.function == Function::Museum { 2 } else { 0 }).collect()
}

fn allowed_slots(kinds: &[Kind], text: &Text) -> Vec<Option<usize>> {
    kinds
        .iter()
        .map(|kind| match kind.function {
            // Échoppes et emplacements suivent l'agora ; le mur, la tour et le corps de
            // garde ont leur rang propre (leurs noms sont partagés par d'autres textes).
            Function::Plot | Function::Vendor(_) => Some(8),
            Function::Gatehouse => Some(35),
            Function::Tower => Some(34),
            _ => ALLOWED_TYPES.iter().find(|(_, types)| types.iter().any(|&t| text.get(TEXT_BUILDINGS, t) == kind.name)).map(|&(slot, _)| slot),
        })
        .collect()
}

fn lock_resources(world: &mut World, episode: &Episode) {
    // Un sanctuaire achevé apporte sa ressource (éditeur d'aventures, annexe 2).
    let granted: Vec<u8> = world.sanctuaries().iter().filter_map(|&god| SANCTUARY_RESOURCES.iter().find(|&&(g, _)| g == god).map(|&(_, r)| r)).collect();
    // Une marchandise qu'un objectif demande de produire ou de mettre de côté est
    // toujours permise : certains épisodes en réclament sans les nommer dans leur
    // liste, ni de cité qui les vende.
    let wanted: Vec<u8> = episode
        .goals
        .iter()
        .filter(|g| matches!(g.kind, GoalKind::Production | GoalKind::SetAside) && g.values[0] > 0)
        .map(|g| g.values[0] as u8)
        .collect();
    let has = |resource: u8| episode.resources.contains(&resource) || granted.contains(&resource) || wanted.contains(&resource);
    world.waived.clear();
    for &resource in &wanted {
        world.waive_missing_deposit(resource);
    }
    let farmable = FARM_RESOURCES.iter().any(|&crop| has(crop)) || [RESOURCE_FISH, RESOURCE_MEAT, RESOURCE_CHEESE].into_iter().any(has);
    world.max_sanctuaries = episode.max_sanctuaries;
    world.atlantean = episode.atlantean;
    world.games_from = episode.games.map(i32::from);
    // Ce que la liste des bâtiments autorisés de l'épisode interdit.
    let slots = ALLOWED_SLOT.get();
    let wrong_nation = |kind: usize| NATION.get().and_then(|n| n.get(kind)).is_some_and(|&n| n == if episode.atlantean { 1 } else { 2 });
    let forbidden = |kind: usize| -> bool {
        if wrong_nation(kind) {
            return true;
        }
        let slot = slots.and_then(|slots| slots.get(kind).copied().flatten());
        slot.is_some_and(|slot| !episode.allowed.is_empty() && !episode.allowed.get(slot).copied().unwrap_or(true))
    };
    world.locked = world
        .kinds
        .iter()
        .enumerate()
        .map(|(index, kind)| forbidden(index) || match kind.function {
            // Seuls les dieux favorables que l'épisode désigne reçoivent un sanctuaire.
            Function::Sanctuary { god, .. } => !episode.sanctuaries.contains(&god),
            Function::Racetrack => !episode.goals.iter().any(|goal| goal.kind == GoalKind::Hippodrome),
            // Un monument, seulement si un objectif le demande (n'importe lequel pour
            // « n'importe quelle pyramide »).
            Function::Monument { model, .. } => !episode.goals.iter().any(|goal| {
                goal.kind == GoalKind::Pyramid && (goal.values[1] == -1 || goal.values[1] == (monument::FIRST_TYPE + model as usize) as i32)
            }),
            // Un temple de héros, seulement si une quête ou un monstre le demande.
            Function::HeroHall(hero) => !episode.events.iter().any(|event| match *event {
                Event::Quest { hero: wanted, .. } => wanted == hero,
                Event::Monster { monster, .. } => heroes::hero_against(monster) == Some(hero),
                Event::World { .. } | Event::Request { .. } | Event::Gift { .. } | Event::TradeChange { .. } | Event::GodInvasion { .. } | Event::Invasion { .. } | Event::Wages { .. } | Event::ArmyAway { .. } | Event::Disaster { .. } | Event::Other => false,
            }),
            _ if !farmable => false,
            Function::Farm => !kind.parts.get(1).is_some_and(|field| FARMS.iter().zip(FARM_RESOURCES).any(|(farm, resource)| farm.1 == field.image && has(resource))),
            // Pêcherie et bergerie dépendent de la carte (eau, prés), pas de la liste :
            // des épisodes demandent du poisson ou de la laine sans les y nommer.
            // Laiterie et pavillon de chasse se distinguent par leur image.
            Function::Producer(Good::Food) => {
                let hunting = kind.parts.first().is_some_and(|part| part.image == HUNTING_LODGE_IMAGE);
                !has(if hunting { RESOURCE_MEAT } else { RESOURCE_CHEESE })
            }
            Function::Supplier(Raw::Crops) => !has(RESOURCE_OLIVES) && !has(RESOURCE_GRAPES),
            Function::Workshop(Good::Oil) => !has(RESOURCE_OLIVES),
            Function::Workshop(Good::Wine) => !has(RESOURCE_GRAPES),
            Function::Supplier(Raw::Bronze) => !has(RESOURCE_BRONZE),
            Function::Producer(Good::Wood) => !has(RESOURCE_WOOD),
            Function::Producer(Good::Marble) => !has(RESOURCE_MARBLE),
            Function::Producer(Good::BlackMarble) => !has(16),
            Function::Producer(Good::Orichalc) => !has(17),
            Function::Workshop(Good::Sculpture) => !has(RESOURCE_BRONZE),
            _ => false,
        })
        .collect::<Vec<bool>>();
}

/// Où en est l'aventure : l'ordre de ses épisodes, celui que l'on joue, et la cité
/// mère, mise de côté pendant un épisode de colonie.
struct Journey {
    order: Vec<Step>,
    position: usize,
    home: Option<World>,
    colony_maps: Vec<Map>,
}

impl Journey {
    fn step(&self) -> Step {
        self.order.get(self.position).copied().unwrap_or(Step::Parent(0))
    }

    /// Passe à l'épisode de ce rang, en changeant de cité s'il le faut ; rend vrai si
    /// la cité a changé. Une colonie part de sa carte vierge. Le trésor suit le joueur
    /// d'une cité à l'autre (on ignore encore comment le jeu dote une colonie), et le
    /// calendrier continue.
    fn travel(&mut self, world: &mut World, position: usize) -> bool {
        let (from, Some(&to)) = (self.step(), self.order.get(position)) else { return false };
        self.position = position;
        match (from, to) {
            (Step::Parent(_), Step::Colony(k)) => {
                let Some(map) = self.colony_maps.get(k).cloned() else { return false };
                let colony = world.new_city(map, world.money);
                self.home = Some(std::mem::replace(world, colony));
                true
            }
            (Step::Colony(_), Step::Parent(_)) => {
                let Some(mut home) = self.home.take() else { return false };
                home.money = world.money;
                home.ticks = home.ticks.max(world.ticks);
                *world = home;
                true
            }
            _ => false,
        }
    }

    /// Colonies parmi lesquelles le joueur choisit au rang `next`, quand l'aventure en
    /// offre plus qu'il ne lui reste d'épisodes de colonie (Jason en propose deux pour
    /// un seul épisode) : celles qu'il n'a pas encore fondées.
    fn colony_choices(&self, next: usize) -> Option<Vec<usize>> {
        let Some(Step::Colony(_)) = self.order.get(next) else { return None };
        let founded: Vec<usize> = self.order[..next].iter().filter_map(|step| if let Step::Colony(k) = step { Some(*k) } else { None }).collect();
        let left: Vec<usize> = (0..self.colony_maps.len()).filter(|k| !founded.contains(k)).collect();
        let steps = self.order[next..].iter().filter(|step| matches!(step, Step::Colony(_))).count();
        (left.len() > steps).then_some(left)
    }

    /// Le joueur fonde `pick` au rang `next` ; les épisodes de colonie suivants prennent
    /// les autres colonies, dans l'ordre de l'aventure.
    fn choose_colony(&mut self, next: usize, pick: usize) {
        let founded: Vec<usize> = self.order[..next].iter().filter_map(|step| if let Step::Colony(k) = step { Some(*k) } else { None }).collect();
        let mut others = (0..self.colony_maps.len()).filter(|k| *k != pick && !founded.contains(k));
        let mut first = Some(pick);
        for step in &mut self.order[next..] {
            if let Step::Colony(k) = step {
                *k = first.take().or_else(|| others.next()).unwrap_or(*k);
            }
        }
    }

    /// Colonies dans l'ordre où elles se fondent.
    fn colonies(&self) -> Vec<usize> {
        self.order.iter().filter_map(|step| if let Step::Colony(k) = step { Some(*k) } else { None }).collect()
    }

    /// Fichiers qui accompagnent une sauvegarde : le rang de l'épisode, et la cité
    /// mère pendant un épisode de colonie.
    fn companions(save: &Path) -> (PathBuf, PathBuf) {
        (save.with_extension("episode"), save.with_extension("mere"))
    }

    fn save(&self, world: &World, save: &Path) -> std::io::Result<()> {
        let (episode, home) = Self::companions(save);
        fs::write(save, world.save())?;
        // Rang de l'épisode, niveau de difficulté de la partie, puis colonies choisies.
        let colonies: Vec<String> = self.colonies().iter().map(|k| k.to_string()).collect();
        fs::write(episode, format!("{}\n{}\n{}\n", self.position, difficulty(), colonies.join(" ")))?;
        match &self.home {
            Some(city) => fs::write(home, city.save()),
            None => fs::remove_file(home).or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) }),
        }
    }

    /// Recharge une partie et son épisode. Les sauvegardes sans rang d'épisode
    /// reprennent le premier. Rien ne change si la lecture échoue.
    fn load(&mut self, world: &mut World, save: &Path) -> Result<(), String> {
        let (episode, home) = Self::companions(save);
        let read = |path: &Path| fs::read_to_string(path).map_err(|e| format!("{} : {e}", path.display()));
        let text = read(save)?;
        let lines = fs::read_to_string(episode).unwrap_or_default();
        let mut lines = lines.lines().map(str::trim);
        let position = lines.next().and_then(|t| t.parse().ok()).filter(|&p| p < self.order.len()).unwrap_or(0);
        // Les règles de la difficulté sont chargées au lancement : une partie d'un autre
        // niveau se reprend en relançant à ce niveau. Sans niveau noté : Héros.
        let level = lines.next().and_then(|t| t.parse::<usize>().ok()).unwrap_or(2);
        if level != difficulty() {
            return Err(format!("partie enregistrée au niveau {}, relancez avec --difficulte {}", ["Débutant", "Mortel", "Héros", "Titan", "Olympien"].get(level).unwrap_or(&"?"), ["debutant", "mortel", "heros", "titan", "olympien"].get(level).unwrap_or(&"heros")));
        }
        // Colonies dans l'ordre choisi par le joueur (absent des anciennes sauvegardes).
        let chosen: Vec<usize> = lines.next().unwrap_or_default().split_whitespace().filter_map(|k| k.parse().ok()).filter(|&k| k < self.colony_maps.len()).collect();
        if chosen.len() == self.colonies().len() {
            let mut chosen = chosen.into_iter();
            for step in &mut self.order {
                if let Step::Colony(k) = step {
                    *k = chosen.next().unwrap_or(*k);
                }
            }
        }
        // Toutes les cités se bâtissent sur le modèle de la cité mère.
        let base = self.home.as_ref().unwrap_or(world);
        let mut parent = base.new_city(base.map.clone(), 0);
        let (city, home) = match self.order.get(position).copied().unwrap_or(Step::Parent(0)) {
            Step::Parent(_) => {
                parent.load(&text)?;
                (parent, None)
            }
            Step::Colony(k) => {
                parent.load(&read(&home)?)?;
                let mut colony = base.new_city(self.colony_maps.get(k).ok_or("carte de colonie absente")?.clone(), 0);
                colony.load(&text)?;
                (colony, Some(parent))
            }
        };
        *world = city;
        self.home = home;
        self.position = position;
        Ok(())
    }
}

/// Met de côté les marchandises que demandent les objectifs, s'il y en a assez en
/// stock ; rend le compte rendu à afficher.
fn set_aside(world: &mut World, goals: &[Goal], text: &Text) -> String {
    let mut report = Vec::new();
    for goal in goals.iter().filter(|goal| goal.kind == GoalKind::SetAside) {
        if goal.status(world) == Status::Met {
            continue;
        }
        let [resource, amount, _] = goal.values;
        let Ok(index) = usize::try_from(resource) else { continue };
        if !world.can_store(resource) {
            continue;
        }
        let name = text.get(23, index).to_lowercase();
        let missing = amount - world.set_aside[index];
        if world.set_aside(index, missing) {
            report.push(format!("{missing} {name} mis de côté"));
        } else {
            report.push(format!("{name} : {} en stock sur {missing}", world.stored(index)));
        }
    }
    if report.is_empty() {
        return "Rien à mettre de côté".to_string();
    }
    report.join(" ; ")
}

/// Ressource qu'apporte le sanctuaire d'un dieu : les oliviers d'Athéna, la vigne de
/// Dionysos, le bronze d'Héphaïstos (les orangers d'Héra ne sont pas simulés).
const SANCTUARY_RESOURCES: [(u8, u8); 3] = [(8, RESOURCE_OLIVES), (10, RESOURCE_GRAPES), (9, RESOURCE_BRONZE)];

/// Élément d'un sanctuaire : son image et son catalogue.
fn sanctuary_part(dx: usize, dy: usize, piece: Piece) -> Part {
    let plain = (Source::General, SANCTUARY_PLAIN_IMAGES + (dx + dy) % 6);
    let (source, image) = match piece {
        Piece::Temple => (Source::Sanctuary(2), SANCTUARY_TEMPLE_IMAGE),
        Piece::TempleBack => (Source::Sanctuary(2), SANCTUARY_TEMPLE_IMAGE + 2),
        Piece::TempleFront => (Source::Sanctuary(2), SANCTUARY_TEMPLE_IMAGE + 3),
        Piece::Altar => (Source::General, ALTAR_IMAGE),
        Piece::Sculpture { god, facing } => sanctuary::rank(god).map_or(plain, |rank| (Source::God(rank as u8), GOD_STATUE_IMAGE + facing)),
        Piece::Statue { god, facing } => sanctuary::rank(god).map_or(plain, |rank| (Source::General, SMALL_STATUE_IMAGE + 4 * rank + facing)),
        Piece::Tile(motif) => (Source::General, SANCTUARY_TILE_IMAGE + motif),
        Piece::Plain => plain,
    };
    Part { dx, dy, size: piece.size(), image, crop: false, bay: None, source }
}

/// Nom du bâtiment qui rend un service.
fn service_name(text: &Text, service: Service) -> String {
    let kind = match service {
        Service::Water => 74,
        Service::Maintenance => 121,
        Service::Security => 124,
        Service::Health => 76,
        Service::Tax => 122,
        Service::Philosophy => 81,
        Service::Athletics => 79,
        Service::Drama => 82,
    };
    text.get(TEXT_BUILDINGS, kind)
}

/// Nom d'un service d'après le bâtiment qui le rend dans l'épisode : dans une cité
/// atlante, l'observatoire plutôt que le podium.
fn service_label(world: &World, text: &Text, service: Service) -> String {
    let open = |k: &usize| world.locked.get(*k) != Some(&true);
    (0..world.kinds.len()).filter(open).find(|&k| world.kinds[k].function == Function::Service(service)).map_or_else(|| service_name(text, service), |k| world.kinds[k].name.clone())
}

/// Conseillers, comme les écrans du jeu : population, emploi, finances, services,
/// mythologie, armée et commerce ; chaque page a son titre et ses lignes.
/// Niveaux d'imposition du jeu, et leur taux.
const TAX_NAMES: [&str; 7] = ["aucun", "très bas", "bas", "normal", "haut", "très haut", "scandaleux"];

fn tax_name(world: &World) -> String {
    let name = TAX_NAMES.get(world.tax_level).copied().unwrap_or("normal");
    match world.taxes.levels.get(world.tax_level) {
        Some((rate, _)) => format!("{name} ({rate} %)"),
        None => name.to_string(),
    }
}

fn mood_note(sentiment: i32) -> &'static str {
    match sentiment {
        ..15 => " : les habitants s'en vont",
        15..30 => " : les immigrants ne viennent plus",
        30..50 => " : mécontentement",
        _ => "",
    }
}

fn advice(text: &Text, world: &World, good_names: &[String], level_names: &[String]) -> Vec<(String, Vec<String>)> {
    let houses: Vec<&zeus::world::House> = world.buildings.iter().flatten().filter_map(|b| b.house.as_ref()).filter(|h| h.population > 0).collect();
    let mut pages = Vec::new();
    // Population : habitants par niveau de logement.
    let mut population = vec![format!("{} habitants dans {} maisons", world.population(), houses.len())];
    for (level, name) in level_names.iter().enumerate() {
        let (count, people) = houses.iter().filter(|h| h.level == level).fold((0, 0), |(c, p), h| (c + 1, p + h.population));
        if count > 0 {
            population.push(format!("{name} : {count} ({people} habitants)"));
        }
    }
    pages.push(("Population".to_string(), population));
    // Emploi : postes par catégorie de bâtiments, et bâtiments à l'arrêt.
    let (needed, available) = (world.workers_needed(), world.workers_available());
    let mut employment = vec![match available - needed {
        spare if spare >= 0 => format!("{needed} postes pour {available} travailleurs ; {spare} sans emploi"),
        short => format!("{needed} postes pour {available} travailleurs ; il manque {} ouvriers", -short),
    }];
    let names: Vec<String> = CATEGORY_NAMES.iter().map(|&(index, fallback)| index.map(|i| text.get(4, i).trim().to_string()).filter(|n| !n.is_empty()).unwrap_or(fallback.to_string())).collect();
    for (category, name) in names.iter().enumerate() {
        let buildings: Vec<&zeus::world::Building> = world.buildings.iter().flatten().filter(|b| category_of(world.kinds[b.kind].function) == category && world.kinds[b.kind].employees > 0).collect();
        if buildings.is_empty() {
            continue;
        }
        let jobs: i32 = buildings.iter().map(|b| world.kinds[b.kind].employees).sum();
        let idle = buildings.iter().filter(|b| !b.staffed).count();
        employment.push(format!("{name} : {jobs} postes, {} bâtiments, {idle} à l'arrêt", buildings.len()));
    }
    pages.push(("Emploi".to_string(), employment));
    // Finances.
    let (sold, bought): (i32, i32) = world.cities.iter().fold((0, 0), |(s, b), c| (s + c.sold, b + c.bought));
    // Les percepteurs ne sortent pas sans palais.
    let unpaid = world.buildings.iter().flatten().find(|b| world.service_waits_for(b) == Some(Function::Palace));
    let palace_note = unpaid.and_then(|_| world.kinds.iter().find(|k| k.function == Function::Palace)).map(|k| format!("Sans {}, les percepteurs ne lèvent aucun impôt", k.name.to_lowercase()));
    pages.push((
        "Finances".to_string(),
        [
            format!("Trésor : {} dr.", world.money),
            format!("Dernier mois : impôts {} dr., salaires {} dr.", world.last_month.0, world.last_month.1),
            format!("Imposition (+ et -) : {}", tax_name(world)),
            format!("Humeur de la cité : {} sur 100{}", world.sentiment, mood_note(world.sentiment)),
            format!("Bénéfice de l'année en cours : {} dr.", world.year_profit),
            match world.best_year_profit {
                Some(best) => format!("Meilleure année achevée : {best} dr."),
                None => "Aucune année achevée".to_string(),
            },
            format!("Commerce depuis le début : ventes {sold} dr., achats {bought} dr."),
        ]
        .into_iter()
        .chain(palace_note)
        .collect(),
    ));
    // Services : part des maisons desservies, et réserves.
    let mut services: Vec<String> = SERVICES
        .iter()
        .map(|&service| {
            let served = houses.iter().filter(|h| h.has(service, world.ticks)).count();
            let percent = if houses.is_empty() { 0 } else { served * 100 / houses.len() };
            format!("{} : {served} maisons sur {} ({percent} %)", service_label(world, text, service), houses.len())
        })
        .collect();
    services.push(format!("Santé de la cité : {} sur 100", world.health));
    services.push(format!("Vivres aux greniers : {} chargements", (1..zeus::world::FOODS).map(|r| world.stored(r)).sum::<i32>()));
    let goods: Vec<String> = GOODS
        .iter()
        .enumerate()
        .filter_map(|(i, &good)| zeus::world::resource_of(good).map(|r| (i, world.stored(r as usize))))
        .filter(|&(_, n)| n > 0)
        .map(|(i, n)| format!("{} {n}", good_names[i].to_lowercase()))
        .collect();
    services.push(if goods.is_empty() { "Entrepôts : vides".to_string() } else { format!("Entrepôts : {}", goods.join(", ")) });
    pages.push(("Services et réserves".to_string(), services));
    // Mythologie : sanctuaires, héros, quêtes et monstres.
    let mut myth: Vec<String> = world.sanctuaries().iter().map(|&god| format!("Sanctuaire de {}", text.get(sanctuary::TEXT_GODS, god as usize))).collect();
    if myth.is_empty() {
        myth.push("Aucun sanctuaire achevé".to_string());
    }
    for (hero, &present) in world.heroes.iter().enumerate() {
        if present {
            myth.push(format!("{} est dans la cité", text.get(heroes::TEXT_HEROES, hero)));
        }
    }
    for quest in &world.quests {
        let hero = text.get(heroes::TEXT_HEROES, quest.hero as usize);
        myth.push(match quest.state {
            zeus::world::QuestState::Waiting => format!("Quête en attente de {hero}"),
            zeus::world::QuestState::Away { .. } => format!("{hero} est parti en quête"),
            zeus::world::QuestState::Done => format!("Quête de {hero} accomplie"),
        });
    }
    let monsters = world.monsters.iter().filter(|m| !m.slain).count();
    myth.push(format!("Monstres encore à vaincre : {monsters}"));
    pages.push(("Mythologie".to_string(), myth));
    // Armée et commerce.
    let rivals = world.cities.iter().filter(|c| c.active && c.kind == zeus::map::FOREIGN && c.status == zeus::map::RIVAL).count();
    pages.push((
        "Armée et commerce".to_string(),
        vec![
            format!("Force de l'armée : {}", world.army()),
            match world.raid {
                Some(raid) => format!("Invasion en cours : {} soldats de {}", raid.soldiers, city_name(text, world, raid.city)),
                None => format!("{rivals} cités rivales"),
            },
            format!("{} partenaires commerciaux, {} demandes en attente", world.partners(), world.requests.len()),
            format!("{} à quai : {}", capitalized(world.ship_plural()), world.triremes()),
        ],
    ));
    pages
}

/// Qualité d'une couverture, comme les appréciations du jeu.
fn coverage_word(percent: usize) -> &'static str {
    match percent {
        0 => "aucune",
        1..20 => "très faible",
        20..40 => "faible",
        40..60 => "moyenne",
        60..80 => "bonne",
        80..100 => "très bonne",
        _ => "parfaite",
    }
}

/// Ce que le panneau latéral dit d'une catégorie, comme le jeu sous ses onglets : des
/// lignes courtes, et les vues en surimpression qu'elle propose (« Voir… »).
fn category_info(world: &World, text: &Text, category: usize) -> (Vec<String>, Vec<(&'static str, Overlay)>) {
    let houses: Vec<&zeus::world::House> = world.buildings.iter().flatten().filter_map(|b| b.house.as_ref()).filter(|h| h.population > 0).collect();
    let cover = |service: Service| {
        let percent = if houses.is_empty() { 0 } else { houses.iter().filter(|h| h.has(service, world.ticks)).count() * 100 / houses.len() };
        format!("{} : {}", service_label(world, text, service), coverage_word(percent))
    };
    let count = |f: &dyn Fn(Function) -> bool| world.buildings.iter().flatten().filter(|b| f(world.kinds[b.kind].function)).count();
    match category {
        0 => {
            let room: i32 = world.buildings.iter().flatten().filter_map(|b| b.house.as_ref()).map(|h| (world.capacity(h) - h.population).max(0)).sum();
            let limit = if world.sentiment < 50 {
                "Immigration freinée : la cité est mal aimée"
            } else if room == 0 {
                "Immigration limitée : pas de logement libre"
            } else {
                "Des gens veulent venir dans la cité"
            };
            (vec![format!("Logements pour {room} habitants de plus"), limit.to_string()], vec![("Voir l'attrait", Overlay::Appeal)])
        }
        1 => (
            vec![
                format!("Vivres aux greniers : {}", (1..zeus::world::FOODS).map(|r| world.stored(r)).sum::<i32>()),
                format!("Exploitations : {}", count(&|f| matches!(f, Function::Farm | Function::Fishery | Function::Producer(Good::Food)))),
            ],
            Vec::new(),
        ),
        2 => {
            let idle = world.buildings.iter().flatten().filter(|b| category_of(world.kinds[b.kind].function) == 2 && !b.staffed).count();
            (vec![format!("Ateliers à l'arrêt : {idle}")], Vec::new())
        }
        3 => {
            let fed = if houses.is_empty() { 0 } else { houses.iter().filter(|h| h.ate).count() * 100 / houses.len() };
            (vec![format!("Échoppes : {}", count(&|f| matches!(f, Function::Vendor(_)))), format!("Maisons nourries : {fed} %")], Vec::new())
        }
        4 => (
            vec![cover(Service::Water), cover(Service::Maintenance), cover(Service::Health), cover(Service::Security), format!("Santé : {} sur 100", world.health)],
            vec![("Voir l'eau", Overlay::Water), ("Voir le feu", Overlay::Fire), ("Voir les dégâts", Overlay::Damage), ("Voir la santé", Overlay::Health), ("Voir le crime", Overlay::Crime)],
        ),
        5 => (
            vec![format!("Trésor : {} dr.", world.money), cover(Service::Tax), format!("Humeur : {} sur 100", world.sentiment)],
            vec![("Voir les impôts", Overlay::Tax)],
        ),
        6 => (vec![cover(Service::Philosophy), cover(Service::Athletics), cover(Service::Drama)], vec![("Voir la culture", Overlay::Culture)]),
        7 => (vec![format!("Sanctuaires : {}", world.sanctuaries().len()), format!("Monstres : {}", world.monsters.iter().filter(|m| !m.slain).count())], Vec::new()),
        8 => {
            let appeal = world.buildings.iter().flatten().filter(|b| b.house.is_some()).map(|b| world.appeal(b.x as i32, b.y as i32)).collect::<Vec<_>>();
            let mean = if appeal.is_empty() { 0 } else { appeal.iter().sum::<i32>() / appeal.len() as i32 };
            (vec![format!("Attrait moyen des maisons : {mean}")], vec![("Voir l'attrait", Overlay::Appeal)])
        }
        COMMEMORATIVE_CATEGORY => (
            vec![match world.commemoratives.len() {
                0 => "Aucun monument à ériger. La cité en gagne par ses exploits.".to_string(),
                1 => "Un monument à ériger".to_string(),
                n => format!("{n} monuments à ériger"),
            }],
            Vec::new(),
        ),
        _ => (
            vec![
                format!("Force de l'armée : {}", world.army()),
                match world.raid {
                    Some(raid) => format!("Invasion : {} soldats", raid.soldiers),
                    None => "Aucune invasion".to_string(),
                },
            ],
            Vec::new(),
        ),
    }
}

/// Nom anglais des dieux et des héros dans les clés des lettres d'événements.
const LETTER_GODS: [&str; 14] = ["zeus", "poseidon", "demeter", "apollo", "artemis", "ares", "aphrodite", "hermes", "athena", "hephaestus", "dionysus", "hades", "hera", "atlas"];
const LETTER_HEROES: [&str; 8] = ["achilles", "hercules", "jason", "odysseus", "perseus", "theseus", "atalanta", "bellerophon"];

/// Champs d'une lettre qui concerne une cité du monde.
fn city_fields(text: &Text, world: &World, leaders: &[u8], city: usize) -> Vec<(&'static str, String)> {
    let written = CITY_TITLES.read().ok().and_then(|t| t.get(city).and_then(|(_, leader)| leader.clone()));
    let leader = written.unwrap_or_else(|| leaders.get(city).map_or_else(|| "le chef".to_string(), |&l| text.get(139, l as usize)));
    vec![
        ("player_name", "Chef".to_string()),
        ("greeting", "Salutations".to_string()),
        ("city_name", city_name(text, world, city)),
        ("leader_name", leader),
        ("a_foreign_army", "une armée".to_string()),
    ]
}

/// Noms de cités et de chefs écrits en toutes lettres par la carte (Poséidon), fixés
/// au chargement de l'aventure.
static CITY_TITLES: std::sync::RwLock<Vec<(Option<String>, Option<String>)>> = std::sync::RwLock::new(Vec::new());

/// Nom d'une cité du monde.
fn city_name(text: &Text, world: &World, city: usize) -> String {
    let written = CITY_TITLES.read().ok().and_then(|t| t.get(city).and_then(|(name, _)| name.clone()));
    written.unwrap_or_else(|| world.cities.get(city).map_or(String::new(), |c| text.get(TEXT_CITIES, c.name as usize)))
}

/// Ce qu'une cité demande : drachmes, troupes, vivres ou une marchandise.
fn request_good(text: &Text, resource: u8) -> String {
    match resource {
        zeus::world::TROOPS => "soldats".to_string(),
        zeus::world::ANY_FOOD => "vivres".to_string(),
        r => text.get(23, r as usize).to_lowercase(),
    }
}

/// Catalogue de chaque dieu en personne, dans l'ordre du groupe de textes 157.
const GOD_FILES: [&str; 14] = [
    "zeus_zeus",
    "zeus_poseidon",
    "zeus_demeter",
    "zeus_apollo",
    "zeus_artemis",
    "zeus_ares",
    "zeus_aphrodite",
    "zeus_hermes",
    "zeus_athena",
    "zeus_hephaestus",
    "zeus_dionysus",
    "zeus_hades",
    "poseidon_hera",
    "poseidon_atlas",
];

/// Catalogue de chaque héros, dans l'ordre du groupe de textes 163.
const HERO_FILES: [&str; 8] =
    ["zeus_achilles", "zeus_heracles", "zeus_jason", "zeus_odysseus", "zeus_perseus", "zeus_theseus", "poseidon_atalanta", "poseidon_bellerophon"];

/// Catalogue de chaque monstre, dans l'ordre du groupe de textes 87.
/// Catalogues des armées ennemies, par nation de la cité (ordre de l'éditeur).
const ARMY_FILES: [&str; 10] = [
    "zeus_greek",
    "zeus_trojan",
    "zeus_persian",
    "zeus_centaur",
    "zeus_amazon",
    "poseidon_egyptian",
    "poseidon_mayan",
    "poseidon_phoenician",
    "poseidon_oceanid",
    "poseidon_atlantean",
];
/// Hoplite de la cité dans `sprmain.sg3` (marche, mort, combat).
const GROUP_HOPLITE: usize = 129;
/// Piétaille et cavalier de la cité (même disposition : marche, mort, combat).
const GROUP_RABBLE: usize = 80;
const GROUP_HORSEMAN: usize = 132;
/// Navire de transport des envahisseurs venus par la mer.
const GROUP_SHIP: usize = 118;
/// Soldats atlantes dans `poseidonimps.sg3` (relevés à l'œil, marche, mort, combat) :
/// archer, lancier, char.
const GROUPS_ATLANTEAN: [usize; 3] = [75, 69, 72];
const MONSTER_FILES: [&str; 16] = [
    "zeus_hydra",
    "zeus_kraken",
    "zeus_cerberus",
    "zeus_cyclops",
    "zeus_minotaur",
    "zeus_medusa",
    "zeus_calydonianboar",
    "zeus_dragon",
    "zeus_talos",
    "zeus_hector",
    "zeus_scylla",
    "zeus_maenads",
    "poseidon_sphinx",
    "poseidon_chimera",
    "poseidon_harpie",
    "poseidon_echidna",
];

/// Noms des cités du monde.
const TEXT_CITIES: usize = 21;

/// Quêtes et monstres d'un épisode qui commence.
fn begin_events(world: &mut World, episode: &Episode) {
    world.quests.clear();
    world.monsters.clear();
    world.scheduled.clear();
    world.requests.clear();
    world.invasions.clear();
    for (event, &kind) in episode.events.iter().enumerate() {
        match kind {
            Event::Quest { god, quest, hero } => {
                let hero = hero.min(heroes::HEROES as u8 - 1);
                world.quests.push(Quest { event, god, quest, hero, state: QuestState::Waiting });
            }
            Event::Monster { monster, arrives } => {
                let arrives_at = arrives.map_or(0, |(month, years)| world.date_in(month, years).max(1));
                world.monsters.push(Monster { event, monster, fight_until: None, slain: false, arrives_at });
            }
            Event::World { change, cities, amount, month, years } => world.schedule(change, cities, amount, month, years),
            Event::GodInvasion { god, month, years } => world.schedule_god(god, month, years),
            Event::TradeChange { change, resource, cities, amount, month, years } => world.schedule_trade(change, resource, cities, amount, month, years),
            Event::Gift { resource, cities, amount, month, years } => {
                world.schedule(zeus::world::GIFT.saturating_add(resource), cities, amount, month, years)
            }
            Event::Invasion { cities, soldiers, points, month, years } => world.schedule_invasion(cities, soldiers, points, month, years),
            Event::Wages { rise, amount, month, years } => {
                world.schedule(if rise { zeus::world::WAGE_RISE } else { zeus::world::WAGE_FALL }, (0, 0), amount, month, years)
            }
            Event::ArmyAway { cities, months, month, years } => world.schedule(zeus::world::ARMY_AWAY, cities, months, month, years),
            // Les terres englouties ne sont pas simulées (aucune aventure n'en a).
            Event::Disaster { kind, points, permanent, month, years } => {
                let change = match kind {
                    3 => zeus::world::EARTHQUAKE,
                    24 => zeus::world::LAVA,
                    25 if permanent => zeus::world::FLOOD,
                    25 => zeus::world::TIDAL_WAVE,
                    5 => zeus::world::LANDSLIDE,
                    _ => continue,
                };
                world.schedule(change, points, (0, 0), month, years)
            }
            Event::Request { resource, cities, amount, month, years } => {
                world.schedule(zeus::world::REQUEST.saturating_add(resource), cities, amount, month, years)
            }
            Event::Other => {}
        }
    }
}

/// Nom d'une quête : chaque dieu en a deux, rangées à 14 d'écart.
fn quest_name(text: &Text, god: u8, quest: u8) -> String {
    text.get(heroes::TEXT_QUESTS, god as usize + 14 * quest.min(1) as usize)
}

/// Annonce d'une nouvelle des héros.
fn hero_news(text: &Text, news: HeroNews, events: &[Event]) -> String {
    let hero_name = |hero: u8| text.get(heroes::TEXT_HEROES, hero as usize);
    let monster_name = |event: usize| match events.get(event) {
        Some(&Event::Monster { monster, .. }) => text.get(heroes::TEXT_MONSTERS, monster as usize),
        _ => String::new(),
    };
    let quest = |event: usize| match events.get(event) {
        Some(&Event::Quest { god, quest, hero }) => (hero_name(hero), quest_name(text, god, quest)),
        _ => (String::new(), String::new()),
    };
    match news {
        HeroNews::Arrived(hero) => text.get(heroes::TEXT_HALLS, 68).replace("[hero_name]", &hero_name(hero)),
        HeroNews::Departed(event) => {
            let (hero, quest) = quest(event);
            format!("{hero} part : {quest}")
        }
        HeroNews::QuestDone(event) => format!("Quête accomplie : {}", quest(event).1),
        HeroNews::Fighting(event) => format!("Un héros affronte {}", monster_name(event)),
        HeroNews::Slain(event) => format!("{} est vaincu", capitalized(&monster_name(event))),
        HeroNews::Invaded(event) => format!("{} entre dans la cité !", capitalized(&monster_name(event))),
    }
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or(String::new(), |first| first.to_uppercase().chain(chars).collect())
}

/// Ligne du panneau d'un temple de héros : une exigence et où elle en est.
fn requirement_line(text: &Text, requirement: Requirement, check: Check, atlantean: bool) -> String {
    let [a, b, c, d, e] = requirement.values;
    let resource = |id: i32| if id == 25 { text.get(23, 25).to_lowercase() } else { text.get(23, id.max(0) as usize).to_lowercase() };
    let hall = |index: usize| text.get(heroes::TEXT_HALLS, index);
    let what = match requirement.kind {
        // Cité atlante : hippodrome en activité (le jeu affiche 117 ou 118 selon
        // l'état). Cité grecque : une victoire aux jeux panhelléniques (arm plus bas).
        6 if atlantean => hall(if check.passes() { 117 } else { 118 }),
        // Stade actif, ou musée actif dans une cité atlante (textes 127 à 130).
        18 => hall(if atlantean {
            if check.passes() { 129 } else { 130 }
        } else if check.passes() {
            128
        } else {
            127
        }),
        0 => {
            let unit = if b == 19 { 400 } else { 100 };
            let alternative = if c > 0 { format!(" ou {}", resource(c)) } else { String::new() };
            format!("{} {}{alternative} en réserve", a / unit, resource(b))
        }
        1 if b == 43 => format!("{a} {} {}", resource(15).to_lowercase(), hall(34)),
        1 => format!("{a} {}", if atlantean { "frégates" } else { "trières" }),
        2 => format!("{a} manoirs ou mieux"),
        10 => format!("{a} {}", hall(35)),
        3 => {
            let gods: Vec<String> = [b, c, d, e].iter().filter(|&&g| g >= 0).map(|&g| text.get(sanctuary::TEXT_GODS, g as usize)).collect();
            if a > 1 { format!("{a} sanctuaires : {}", gods.join(", ")) } else { format!("{} {}", hall(47), gods.join(" ou ")) }
        }
        4 => hall(48),
        5 if b == -1 => hall(49),
        5 => hall(51),
        6 => hall(50),
        7 => format!("{a} {}", hall(52)),
        8 => hall(54),
        9 => hall(55),
        11 => format!("{a} drachmes {}", hall(36)),
        12 => hall(56),
        13 => hall(57),
        14 => hall(58),
        17 => hall(124),
        _ => "Condition particulière".to_string(),
    };
    // Ces deux exigences disent déjà leur état ; inutile d'ajouter « rempli ».
    if requirement.kind == 18 || (requirement.kind == 6 && atlantean) {
        return what;
    }
    let state = match check {
        Check::Met => "rempli".to_string(),
        Check::Missing { current, wanted } if matches!(requirement.kind, 5 | 8 | 9 | 17) => format!("{current} % sur {wanted} %"),
        Check::Missing { current, wanted } if requirement.kind == 4 => format!("criminalité {current}, sous {wanted} demandé"),
        Check::Missing { current, wanted } if wanted > 1 => format!("{current} sur {wanted}"),
        Check::Missing { .. } => "pas encore".to_string(),
        Check::Unsimulated => "non simulé, tenu pour rempli".to_string(),
    };
    format!("{what} ({state})")
}

/// Prépare l'affichage d'une cité où l'on arrive : son sol, la vue sur son terrain
/// libre ; rend le cadre de sa mini-carte.
fn arrive(world: &World, graphics: &mut Graphics, view: &mut View) -> Minimap {
    graphics.set_ground(plain_ground(world));
    let site = opening_site(world);
    (view.cx, view.cy) = tile_origin(site.0, site.1);
    Minimap::new(world)
}

/// Sol nu le plus courant de la carte, pour remplacer les tuiles sans image propre.
fn plain_ground(world: &World) -> (Source, usize) {
    let mut counts: HashMap<u32, u32> = HashMap::new();
    for y in 0..GRID {
        for x in 0..GRID {
            if world.map.terrain(x, y) == 0 && world.map.image(x, y) != 0 {
                *counts.entry(world.map.image(x, y)).or_default() += 1;
            }
        }
    }
    let commonest = counts.into_iter().max_by_key(|&(id, count)| (count, id)).map(|(id, _)| id);
    commonest.and_then(|id| world.map.resolve(id)).unwrap_or((Source::Terrain, 201))
}

/// Case où s'ouvre la vue : le terrain constructible le plus proche du centre de la
/// zone jouable ; le centre lui-même tombe parfois sur l'eau ou sur des falaises.
fn opening_site(world: &World) -> (i32, i32) {
    let centre = GRID as i32 / 2;
    let mut sites: Vec<(i32, i32)> = (0..GRID as i32).flat_map(|y| (0..GRID as i32).map(move |x| (x, y))).collect();
    sites.sort_by_key(|&(x, y)| (x - centre).abs() + (y - centre).abs());
    let open = sites.into_iter().find(|&(x, y)| world.can_build(x - START_CLEARING / 2, y - START_CLEARING / 2, START_CLEARING as usize));
    open.unwrap_or((centre, centre))
}

/// Image prête à dessiner : sa texture et ses dimensions.
#[derive(Clone, Copy)]
struct Sprite {
    texture: sdl::Texture,
    width: f32,
    height: f32,
}

/// Catalogues d'images et textures déjà envoyées à la carte graphique.
struct Graphics {
    renderer: sdl::Renderer,
    terrain: Catalog,
    poseidon: Catalog,
    general: Catalog,
    fonts: Catalog,
    figures: Catalog,
    imps: Catalog,
    destruction: Catalog,
    interface: Catalog,
    /// Temples des sanctuaires (deux chantiers puis l'achèvement), grandes statues des
    /// dieux par rang, et catalogue vide pour ceux qui manquent.
    sanctuaries: Vec<Catalog>,
    gods: Vec<Catalog>,
    /// Catalogues des monstres et des héros, chargés quand ils paraissent.
    monsters: HashMap<u8, Catalog>,
    heroes: HashMap<u8, Catalog>,
    deities: HashMap<u8, Catalog>,
    /// Armées ennemies (`ARMY_FILES`), chargées à leur première invasion.
    armies: HashMap<u8, Catalog>,
    empty: Catalog,
    /// Teinte appliquée au texte (blanc = couleur propre de la police).
    ink: (u8, u8, u8),
    /// Pixels du sol nu de la carte (une tuile), pour boucher les réserves vertes.
    ground: Option<Vec<u8>>,
    sprites: HashMap<(Source, usize), Option<Sprite>>,
}

impl Graphics {
    /// Retient la tuile de sol nu de la carte, quand elle a la taille d'une case.
    fn set_ground(&mut self, ground: (Source, usize)) {
        let catalog = self.catalog(ground.0);
        let fits = catalog.images.get(ground.1).is_some_and(|i| (i.width, i.height) == (TILE_WIDTH, TILE_HEIGHT));
        self.ground = fits.then(|| catalog.decode(ground.1).ok()).flatten();
    }

    fn catalog(&self, source: Source) -> &Catalog {
        match source {
            Source::Terrain => &self.terrain,
            Source::Poseidon => &self.poseidon,
            Source::General => &self.general,
            Source::Fonts => &self.fonts,
            Source::Figures => &self.figures,
            Source::Imps => &self.imps,
            Source::Destruction => &self.destruction,
            Source::Interface => &self.interface,
            Source::Sanctuary(stage) => self.sanctuaries.get(stage as usize).unwrap_or(&self.empty),
            Source::God(rank) => self.gods.get(rank as usize).unwrap_or(&self.empty),
            Source::Monster(monster) => self.monsters.get(&monster).unwrap_or(&self.empty),
            Source::Hero(hero) => self.heroes.get(&hero).unwrap_or(&self.empty),
            Source::Deity(god) => self.deities.get(&god).unwrap_or(&self.empty),
            Source::Army(army) => self.armies.get(&army).unwrap_or(&self.empty),
        }
    }

    /// Nombre de cases de côté occupées par une image isométrique.
    fn size(&self, source: Source, id: usize) -> i32 {
        self.catalog(source).images.get(id).map_or(1, |image| ((image.width as i32 + 2) / 60).max(1))
    }

    fn sprite(&mut self, source: Source, id: usize) -> Option<Sprite> {
        if let Some(&sprite) = self.sprites.get(&(source, id)) {
            return sprite;
        }
        let sprite = self.upload(source, id);
        self.sprites.insert((source, id), sprite);
        sprite
    }

    fn upload(&self, source: Source, id: usize) -> Option<Sprite> {
        let catalog = self.catalog(source);
        let image = catalog.images.get(id)?;
        let mut rgba = catalog.decode(id).ok()?;
        // Tuiles de relief : leur base en vert pur est une réserve où le jeu laisse voir
        // le sol. On y recopie le sol nu de la carte, calé sur le bas de l'image.
        if let (Source::Terrain | Source::Poseidon, Some(ground)) = (source, &self.ground) {
            let first_row = image.height.saturating_sub(TILE_HEIGHT);
            for (index, pixel) in rgba.chunks_exact_mut(4).enumerate() {
                // Le rouge pur marque de même quelques pixels au revers des falaises.
                if pixel[..] != [0, 255, 0, 255] && pixel[..] != [255, 0, 0, 255] {
                    continue;
                }
                let (x, y) = (index % image.width, index / image.width);
                let under = (x < TILE_WIDTH && y >= first_row).then(|| &ground[((y - first_row) * TILE_WIDTH + x) * 4..][..4]);
                pixel.copy_from_slice(under.unwrap_or(&[0, 0, 0, 0]));
            }
        }
        // Certaines images d'animation ne sont que des étiquettes vertes de substitution.
        let opaque = rgba.chunks_exact(4).filter(|p| p[3] > 0).count();
        let green = rgba.chunks_exact(4).filter(|p| p[3] > 0 && p[0] < 40 && p[1] > 200 && p[2] < 40).count();
        if green * 2 > opaque {
            return None;
        }
        let (w, h) = (image.width as i32, image.height as i32);
        // SAFETY : `rgba` contient w × h pixels de 4 octets et vit jusqu'au retour de l'appel.
        unsafe {
            let texture = sdl::SDL_CreateTexture(self.renderer, sdl::PIXELFORMAT_ABGR8888, sdl::TEXTUREACCESS_STATIC, w, h);
            if texture.is_null() || !sdl::SDL_UpdateTexture(texture, null(), rgba.as_ptr() as *const c_void, w * 4) {
                return None;
            }
            sdl::SDL_SetTextureBlendMode(texture, sdl::BLENDMODE_BLEND);
            Some(Sprite { texture, width: w as f32, height: h as f32 })
        }
    }
}

/// Polices de l'interface.
const FONT_TEXT: Font = Font(335);
const FONT_TITLE: Font = Font(871);
/// Police du texte posé sur l'habillage, et teinte bleu nuit qu'on lui donne.
const FONT_CHROME: Font = Font(335);
/// Petites polices du panneau latéral : blanche pour les libellés, dorée pour les
/// appréciations, comme sous les onglets du jeu.
const FONT_SMALL: Font = Font(1809);
const FONT_SMALL_GOLD: Font = Font(1943);
const CHROME_INK: (u8, u8, u8) = (20, 40, 90);

impl Graphics {
    /// Hauteur de ligne d'une police : celle de ses lettres sans accent.
    fn line_height(&self, font: Font) -> f32 {
        let height = |c: char| font.image(c).and_then(|i| self.fonts.images.get(i)).map_or(0, |image| image.height);
        ('A'..='Z').chain('a'..='z').map(height).max().unwrap_or(0) as f32
    }

    fn glyph_width(&self, font: Font, c: char) -> f32 {
        match font.image(c).and_then(|i| self.fonts.images.get(i)) {
            Some(image) => image.width as f32 + 1.0,
            // Espace, ou caractère sans glyphe : une demi-lettre.
            None => (self.line_height(font) / 3.0).ceil() + 1.0,
        }
    }

    fn text_width(&self, font: Font, text: &str) -> f32 {
        text.chars().map(|c| self.glyph_width(font, c)).sum()
    }

    /// Écrit `text` avec son coin haut-gauche en (x, y) ; rend la largeur occupée.
    unsafe fn draw_text(&mut self, font: Font, text: &str, x: f32, y: f32) -> f32 {
        let line = self.line_height(font);
        let mut pen = x;
        for c in text.chars() {
            let advance = self.glyph_width(font, c);
            if let Some(sprite) = font.image(c).and_then(|i| self.sprite(Source::Fonts, i)) {
                // Les glyphes accentués dépassent vers le haut.
                let rise = (sprite.height - line).max(0.0);
                let target = sdl::FRect { x: pen, y: y - rise, w: sprite.width, h: sprite.height };
                // Les glyphes à peu près blancs prennent la teinte demandée.
                let (r, g, b) = self.ink;
                unsafe {
                    sdl::SDL_SetTextureScaleMode(sprite.texture, sdl::SCALEMODE_NEAREST);
                    sdl::SDL_SetTextureColorMod(sprite.texture, r, g, b);
                    sdl::SDL_RenderTexture(self.renderer, sprite.texture, null(), &target);
                    sdl::SDL_SetTextureColorMod(sprite.texture, 255, 255, 255);
                }
            }
            pen += advance;
        }
        pen - x
    }

    /// Dessine une image de l'interface d'origine, coin haut-gauche en (x, y).
    unsafe fn blit(&mut self, image: usize, x: f32, y: f32) {
        if let Some(sprite) = self.sprite(Source::Interface, image) {
            let target = sdl::FRect { x, y, w: sprite.width, h: sprite.height };
            unsafe {
                sdl::SDL_SetTextureScaleMode(sprite.texture, sdl::SCALEMODE_NEAREST);
                sdl::SDL_RenderTexture(self.renderer, sprite.texture, null(), &target);
            }
        }
    }

    /// Panneau sombre translucide, pour garder le texte lisible sur la carte.
    unsafe fn panel(&self, x: f32, y: f32, w: f32, h: f32) {
        unsafe {
            sdl::SDL_SetRenderDrawBlendMode(self.renderer, sdl::BLENDMODE_BLEND);
            sdl::SDL_SetRenderDrawColor(self.renderer, 10, 20, 45, 200);
            sdl::SDL_RenderFillRect(self.renderer, &sdl::FRect { x, y, w, h });
        }
    }
}

/// Position du coin haut-gauche de la boîte d'une case, en pixels de monde.
/// Orientation de la vue (0, 2, 4 ou 6 comme dans le jeu), touches Origine et Fin.
static ORIENTATION: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

fn orientation() -> u8 {
    ORIENTATION.load(std::sync::atomic::Ordering::Relaxed)
}

/// Coin haut-gauche, dans le monde dessiné, de la case de carte (x, y) vue dans
/// l'orientation courante.
fn tile_origin(x: i32, y: i32) -> (f32, f32) {
    let (x, y) = zeus::world::rotate(orientation(), x, y);
    ((x - y) as f32 * HALF_WIDTH, (x + y) as f32 * HALF_HEIGHT)
}

/// Cases que couvre un geste de `from` à `to` : le tracé d'une route ou d'un mur, ou
/// le rectangle à démolir.
fn drag_tiles(world: &World, tool: Tool, from: (i32, i32), to: (i32, i32)) -> Vec<(i32, i32)> {
    match tool {
        Tool::Road => {
            let mut line = world.plan_line(from, to, false);
            // Un bout du tracé qui ne touche une route que par un coin s'y raccorde par la
            // case d'angle : sans elle, les deux routes ne se rejoindraient pas.
            let road = |(x, y): (i32, i32)| world.is_road(x, y) || line.contains(&(x, y));
            let mut corners = Vec::new();
            for end in [line.first().copied(), line.last().copied()].into_iter().flatten() {
                for (dx, dy) in [(1, 1), (1, -1), (-1, 1), (-1, -1)] {
                    let (corner_x, corner_y) = ((end.0 + dx, end.1), (end.0, end.1 + dy));
                    if world.is_road(end.0 + dx, end.1 + dy) && !line.contains(&(end.0 + dx, end.1 + dy)) && !road(corner_x) && !road(corner_y) {
                        corners.extend([corner_x, corner_y].into_iter().find(|&(x, y)| world.can_build(x, y, 1)));
                    }
                }
            }
            line.extend(corners);
            line
        }
        Tool::Wall => world.plan_line(from, to, true),
        Tool::Demolish => (from.1.min(to.1)..=from.1.max(to.1)).flat_map(|y| (from.0.min(to.0)..=from.0.max(to.0)).map(move |x| (x, y))).collect(),
        _ => Vec::new(),
    }
}

/// Coin de carte d'un bâtiment posé à partir de la case survolée, pour qu'il paraisse
/// à l'écran comme à l'orientation 0.
fn build_corner(world: &World, kind: usize, at: (i32, i32)) -> (i32, i32) {
    let k = &world.kinds[kind];
    World::footprint_corner(orientation(), at, k.size as i32, k.depth as i32)
}

/// Case désignée à la souris : une case surélevée de k niveaux se dessine à la place
/// de la case plate décalée de (−k, −k) à l'écran, et passe devant elle.
fn pick(world: &World, wx: f32, wy: f32) -> (i32, i32) {
    let u = (wx - HALF_WIDTH) / HALF_WIDTH;
    let v = (wy - HALF_HEIGHT) / HALF_HEIGHT;
    let flat = (((v + u) / 2.0).round() as i32, ((v - u) / 2.0).round() as i32);
    for k in [2, 1] {
        let tile = zeus::world::unrotate(orientation(), flat.0 + k, flat.1 + k);
        if in_grid(tile) && world.map.height(tile.0 as usize, tile.1 as usize) as i32 == k {
            return tile;
        }
    }
    zeus::world::unrotate(orientation(), flat.0, flat.1)
}


fn tile_at(wx: f32, wy: f32) -> (i32, i32) {
    let u = (wx - HALF_WIDTH) / HALF_WIDTH;
    let v = (wy - HALF_HEIGHT) / HALF_HEIGHT;
    zeus::world::unrotate(orientation(), ((v + u) / 2.0).round() as i32, ((v - u) / 2.0).round() as i32)
}

struct View {
    /// Point du monde placé au centre de la fenêtre.
    cx: f32,
    cy: f32,
    zoom: f32,
}

impl View {
    fn to_world(&self, sx: f32, sy: f32, w: f32, h: f32) -> (f32, f32) {
        (self.cx + (sx - w / 2.0) / self.zoom, self.cy + (sy - h / 2.0) / self.zoom)
    }

    fn to_screen(&self, wx: f32, wy: f32, w: f32, h: f32) -> (f32, f32) {
        ((wx - self.cx) * self.zoom + w / 2.0, (wy - self.cy) * self.zoom + h / 2.0)
    }
}

/// État de la cité pour les essais : logements par niveau, réserves, stocks, ruines,
/// finances, emploi et services.
fn city_report(world: &World) -> String {
    let saved = world.save();
    let houses: Vec<_> = world.buildings.iter().flatten().filter_map(|b| b.house.as_ref()).collect();
    let served: Vec<String> = SERVICES.iter().map(|&s| format!("{s:?} {}", houses.iter().filter(|h| h.has(s, world.ticks)).count())).collect();
    let mut by_level = [0; 13];
    houses.iter().for_each(|h| by_level[h.level.min(12)] += 1);
    let reserves = GOODS.map(|g| houses.iter().map(|h| h.goods[g as usize]).sum::<i32>());
    let stocks: Vec<String> = world
        .buildings
        .iter()
        .flatten()
        .filter(|b| b.house.is_none() && world.kinds[b.kind].function != Function::None)
        .map(|b| {
            let goods = if b.goods.iter().any(|&g| g > 0) { format!("{:?}", b.goods) } else { String::new() };
            let raw = if b.raw > 0 { format!("+{}", b.raw) } else { String::new() };
            format!("{}@{},{}={}{raw}{goods} {}/{}", world.kinds[b.kind].name, b.x, b.y, b.stock, b.workers, world.kinds[b.kind].employees)
        })
        .collect();
    let appeals: Vec<i32> = world.buildings.iter().flatten().filter(|b| b.house.is_some()).map(|b| world.appeal(b.x as i32, b.y as i32)).collect();
    [
        format!("maisons : par niveau {by_level:?}, réserves totales {reserves:?} | attrait {appeals:?}"),
        format!("stocks : {}", stocks.join(", ")),
        format!("bâtiments debout : {} | {} cases en ruine, {} en feu", world.buildings.iter().flatten().count(), saved.matches("\nruine ").count(), saved.matches("\nfeu ").count()),
        format!(
            "drachmes {} | dernier mois : impôts {}, salaires {} | employés {} demandés, {} disponibles | humeur {} | santé {}",
            world.money,
            world.last_month.0,
            world.last_month.1,
            world.workers_needed(),
            world.workers_available(),
            world.sentiment,
            world.health
        ),
        format!("population {}, {} maisons desservies [{}]", world.population(), houses.len(), served.join(", ")),
    ]
    .join("\n")
}

/// Groupes de textes des fiches de bâtiments du jeu : un titre, puis une phrase qui
/// décrit le bâtiment (puis l'état de son effectif, non repris ici).
const INFO_GROUPS: std::ops::Range<usize> = 60..200;
/// Longueur des lignes de la description dans la fiche.
const INFO_LINE: usize = 56;

/// Description de chaque type de bâtiment, prise dans les fiches du jeu : celle dont
/// le titre est son nom, ou à défaut le finit (« Bureau de maintenance »).
fn building_descriptions(text: &Text, kinds: &[Kind]) -> Vec<Option<String>> {
    let heads: Vec<(String, String)> = INFO_GROUPS.map(|g| (text.get(g, 0).to_lowercase(), text.get(g, 1))).filter(|(title, line)| !title.is_empty() && line.chars().count() > 30).collect();
    kinds
        .iter()
        .map(|kind| {
            let name = kind.name.to_lowercase();
            let exact = heads.iter().find(|(title, _)| *title == name);
            exact.or_else(|| heads.iter().find(|(title, _)| title.ends_with(&format!(" {name}")))).map(|(_, line)| line.clone())
        })
        .collect()
}

/// Coupe un texte en lignes d'au plus `width` points dans la police du texte.
fn wrap_width(graphics: &Graphics, font: Font, text: &str, width: f32) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in text.split_whitespace() {
        let line = lines.last_mut().unwrap();
        let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if !line.is_empty() && graphics.text_width(font, &candidate) > width {
            lines.push(word.to_string());
        } else {
            *line = candidate;
        }
    }
    lines
}

/// Coupe un texte en lignes d'au plus `width` caractères, entre les mots.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in text.split_whitespace() {
        let line = lines.last_mut().unwrap();
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(word.to_string());
        } else {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
    }
    lines
}

/// Annonces gardées au journal, et réponses aux commandes du joueur qui n'y entrent pas.
const JOURNAL_LINES: usize = 20;
const JOURNAL_SKIPS: [&str; 13] =
    ["H : aide", "Vue", "Vitesse", "Musique", "Bruitages", "Imposition", "Partie", "Enregistrement", "Chargement", "Rien à mettre", "Route commerciale", "Pas de route", "Pas assez de drachmes"];

/// Vues en surimpression (touche V), comme celles du jeu : chaque bâtiment est teinté
/// du vert au rouge selon ce qu'elles mesurent.
#[derive(Clone, Copy, PartialEq)]
enum Overlay {
    None,
    Water,
    Fire,
    Damage,
    Appeal,
    Crime,
    Culture,
    Health,
    Tax,
}

const OVERLAYS: [(Overlay, &str); 9] = [
    (Overlay::None, "Vue normale"),
    (Overlay::Water, "Vue : eau"),
    (Overlay::Fire, "Vue : risque d'incendie"),
    (Overlay::Damage, "Vue : risque d'effondrement"),
    (Overlay::Appeal, "Vue : attrait"),
    (Overlay::Crime, "Vue : criminalité"),
    (Overlay::Culture, "Vue : culture"),
    (Overlay::Health, "Vue : santé"),
    (Overlay::Tax, "Vue : impôts"),
];

/// Teinte d'un bâtiment dans une vue : de 0 (mauvais, rouge) à 100 (bon, vert) ; gris
/// pour ce que la vue ne concerne pas.
fn overlay_tint(world: &World, overlay: Overlay, building: &zeus::world::Building) -> Option<(u8, u8, u8)> {
    const NEUTRAL: (u8, u8, u8) = (110, 110, 110);
    let house = building.house.as_ref();
    let served = |service: Service| house.map(|h| if h.has(service, world.ticks) { 100 } else { 0 });
    let score = match overlay {
        Overlay::None => return None,
        Overlay::Fire => Some(100 - world.risks(building).0),
        Overlay::Damage => Some(100 - world.risks(building).1),
        Overlay::Water => served(Service::Water),
        Overlay::Health => served(Service::Health),
        Overlay::Tax => served(Service::Tax),
        // Attrait de -20 (rouge) à +40 (vert).
        Overlay::Appeal => house.map(|_| ((world.appeal(building.x as i32, building.y as i32) + 20) * 100 / 60).clamp(0, 100)),
        Overlay::Crime => house.map(|h| 100 - h.crime.clamp(0, 100)),
        // Note de culture, jusqu'à 90 (les trois disciplines et un stade).
        Overlay::Culture => house.map(|h| world.house_culture(h) * 100 / 90),
    };
    Some(score.map_or(NEUTRAL, |score| {
        // Rouge, jaune, vert.
        let score = score.clamp(0, 100) as u32;
        let red = if score < 50 { 255 } else { 255 * (100 - score) / 50 };
        let green = if score > 50 { 255 } else { 255 * score / 50 };
        (red as u8, green as u8, 60)
    }))
}

/// Pose une cité complète autour d'un îlot bouclé, comme en bâtissent les joueurs : une
/// route en rectangle de 42 cases de tour, reliée à la grand-rue par un seul passage,
/// pour qu'un marcheur parti de la boucle en fasse le tour. Maisons, agoras et services
/// des habitants touchent la boucle ; vivres, ateliers, écoles, palais et stade
/// bordent la grand-rue. Rend la case à montrer au départ.
/// Chantier de la démonstration : (population à atteindre, type, x, y), dans l'ordre.
type DemoPlan = Vec<(i32, usize, i32, i32)>;

/// Bâtit ce que la population atteinte permet, comme un joueur qui agrandit sa cité.
fn advance_demo(world: &mut World, plan: &mut DemoPlan) {
    while let Some(&(population, kind, at, row)) = plan.first() {
        if world.population() < population {
            break;
        }
        if !world.place_building(kind, at, row) {
            eprintln!("démo : pas de place pour « {} » en ({at}, {row})", world.kinds[kind].name);
        }
        plan.remove(0);
    }
}

fn demo_scene(world: &mut World, road_cost: i32) -> Option<((i32, i32), DemoPlan)> {
    const LENGTH: i32 = 46;
    // Terrain réservé autour de l'origine (extrémité ouest de la grand-rue).
    const WEST: i32 = -1;
    const NORTH: i32 = -20;
    const SOUTH: i32 = 13;
    let kind_of = |world: &World, wanted: &dyn Fn(&Kind) -> bool| world.kinds.iter().position(wanted);
    let role = |function: Function| move |k: &Kind| k.function == function;
    let house = kind_of(world, &|k| k.function == Function::House && k.size == 2)?;
    let farms: Vec<usize> = (0..world.kinds.len()).filter(|&k| world.kinds[k].function == Function::Farm).collect();
    let garden = kind_of(world, &|k| k.function == Function::None && k.size == 2 && k.appeal.value > 0)?;

    // Premier site, en partant du centre, où tout tient, avec de la prairie pour les fermes.
    let farm = *farms.first()?;
    let fits = |world: &World, x: i32, y: i32| {
        (NORTH..SOUTH).all(|j| (WEST..LENGTH).all(|i| world.can_build(x + i, y + j, 1)))
            && world.can_place(farm, x + 28, y + 1)
            && world.can_place(farm, x + 32, y + 1)
    };
    let centre = GRID as i32 / 2;
    let mut sites: Vec<(i32, i32)> = (0..GRID as i32).flat_map(|y| (0..GRID as i32).map(move |x| (x, y))).collect();
    sites.sort_by_key(|&(x, y)| (x - centre).abs() + (y - centre).abs());
    let (x, y) = sites.into_iter().find(|&(x, y)| fits(world, x, y))?;

    world.money += 30_000;
    // Grand-rue, boucle (colonnes x+1 et x+13, rangées y-12 et y-3) et passage.
    for i in 0..LENGTH {
        world.place_road(x + i, y, road_cost);
    }
    for i in 1..=13 {
        world.place_road(x + i, y - 12, road_cost);
        world.place_road(x + i, y - 3, road_cost);
    }
    for j in -11..=-4 {
        world.place_road(x + 1, y + j, road_cost);
        world.place_road(x + 13, y + j, road_cost);
    }
    world.place_road(x + 8, y - 2, road_cost);
    world.place_road(x + 8, y - 1, road_cost);
    // Une ruelle longe le stade, pour que la maintenance en atteigne les deux moitiés.
    for j in 1..=12 {
        world.place_road(x + 26, y + j, road_cost);
    }

    // Rien n'est bâti ici : chaque pose s'inscrit au chantier, avec la population à
    // partir de laquelle un joueur l'entreprendrait (`stage`).
    let (stage, plan) = (std::cell::Cell::new(0), std::cell::RefCell::new(DemoPlan::new()));
    let put = |_: &mut World, kind: Option<usize>, at: i32, row: i32| {
        if let Some(kind) = kind {
            plan.borrow_mut().push((stage.get(), kind, at, row));
        }
    };
    let service = |world: &World, service: Service| kind_of(world, &role(Function::Service(service)));
    let vendor = |world: &World, good: Good| kind_of(world, &role(Function::Vendor(good)));

    // Tout est posé d'un coup, sans habitants, et les bâtiments reçoivent leurs
    // employés dans l'ordre de construction : d'abord la maintenance (sur la boucle
    // et sur la grand-rue), puis les vivres, l'eau, la culture, enfin le reste.
    // La cité se bâtit par étapes, à mesure que la population fournit des employés
    // (40 % des habitants) : les treize huttes du départ n'en donnent que 41.
    let maintenance = service(world, Service::Maintenance);
    let (farm_row, makers_row) = (y + 1, x + 22);
    // Emplacements d'exploitation : partout où la prairie le permet au sud de la
    // grand-rue, serrés de gauche à droite, plus deux le long de la ruelle du stade.
    let mut farm_sites: Vec<(i32, i32)> = Vec::new();
    let mut at = x + 27;
    while at + 3 <= x + 43 {
        if world.can_place(farm, at, farm_row) {
            farm_sites.push((at, farm_row));
            at += 3;
        } else {
            at += 1;
        }
    }
    farm_sites.extend([(x + 27, y + 4), (x + 27, y + 7)].into_iter().filter(|&(fx, fy)| world.can_place(farm, fx, fy)));
    let put_farm = |world: &mut World, rank: usize| {
        if let Some(&(fx, fy)) = farm_sites.get(rank) {
            put(world, Some(farms[rank % farms.len()]), fx, fy);
        }
    };
    // Rang d'un atelier le long de la grand-rue, côté nord : chacun a sa place fixe.
    let maker_at = |world: &World, wanted: &dyn Fn(&Kind) -> bool| -> Option<(usize, i32)> {
        let makers: [&dyn Fn(&Kind) -> bool; 8] = [
            &|k| k.function == Function::Producer(Good::Fleece),
            &|k| k.function == Function::Supplier(Raw::Crops),
            &|k| k.function == Function::Workshop(Good::Oil),
            &|k| k.function == Function::Workshop(Good::Wine),
            &|k| k.function == Function::Supplier(Raw::Bronze),
            &|k| k.function == Function::Workshop(Good::Arms),
            &|k| k.function == Function::Storehouse,
            &|k| k.function == Function::Producer(Good::Horses),
        ];
        let mut at = makers_row;
        for maker in makers {
            let kind = world.kinds.iter().position(maker)?;
            if wanted(&world.kinds[kind]) {
                return Some((kind, at));
            }
            at += world.kinds[kind].size as i32;
        }
        None
    };
    let put_maker = |world: &mut World, function: Function| {
        if let Some((kind, at)) = maker_at(world, &|k| k.function == function) {
            let size = world.kinds[kind].size as i32;
            put(world, Some(kind), at, y - size);
        }
    };
    // Au départ : maintenance, vivres, eau.
    put(world, maintenance, x - 1, y - 9);
    // Le grenier borde la grand-rue : sans surintendant qui y passe, il s'effondre en
    // deux mois.
    put(world, maintenance, x + 20, y - 2);
    put_farm(world, 0);
    put(world, kind_of(world, &role(Function::Granary)), x + 9, y + 1);
    put(world, kind_of(world, &role(Function::Agora { grand: true })), x + 2, y - 5);
    put(world, kind_of(world, &role(Function::Agora { grand: false })), x + 2, y - 11);
    put(world, vendor(world, Good::Food), x + 2, y - 5);
    put(world, service(world, Service::Water), x - 1, y - 11);
    // Masures : seconde fontaine (un porteur d'eau ne fait pas le tour entier),
    // philosophie et sport, deuxième exploitation.
    stage.set(150);
    put(world, service(world, Service::Water), x + 14, y - 4);
    put(world, kind_of(world, &role(Function::School(Service::Philosophy))), x + 40, y - 3);
    put(world, service(world, Service::Philosophy), x - 1, y - 7);
    put(world, service(world, Service::Athletics), x + 14, y - 12);
    put_farm(world, 1);
    // Taudis : seconde épicière, laine, école de théâtre.
    stage.set(280);
    put(world, vendor(world, Good::Food), x + 2, y - 11);
    put_maker(world, Function::Producer(Good::Fleece));
    put_maker(world, Function::Storehouse);
    put(world, vendor(world, Good::Fleece), x + 4, y - 5);
    put(world, kind_of(world, &role(Function::School(Service::Drama))), x + 16, y - 3);
    // Maisons : théâtre, troisième exploitation, impôts.
    stage.set(380);
    put(world, service(world, Service::Drama), x + 14, y - 9);
    put_farm(world, 2);
    put_farm(world, 3);
    // La ruelle du stade a sa maintenance dès qu'une exploitation la borde.
    put(world, maintenance, x + 24, y + 11);
    put(world, service(world, Service::Tax), x - 1, y - 3);
    put(world, kind_of(world, &role(Function::Palace)), x + 14, y + 1);
    // Immeubles : huile, garde, hôpital.
    stage.set(480);
    put_maker(world, Function::Supplier(Raw::Crops));
    put_maker(world, Function::Workshop(Good::Oil));
    put(world, vendor(world, Good::Oil), x + 6, y - 5);
    // Une maintenance pour l'est de la grand-rue : un seul surintendant ne la couvre pas.
    put(world, maintenance, x + 43, farm_row);
    put_farm(world, 4);
    put_farm(world, 5);
    put(world, service(world, Service::Security), x - 1, y - 5);
    put(world, service(world, Service::Health), x + 27, y + 8);
    // Appartements : le quartier riche et ce qu'il consomme.
    stage.set(600);
    put(world, kind_of(world, &|k| k.function == Function::House && k.size == 4), x + 8, y - 16);
    let pond = kind_of(world, &|k| k.function == Function::None && k.size == 4);
    for (at, row) in [(12, -16), (8, -20), (12, -20), (4, -20), (0, -20)] {
        put(world, pond, x + at, y + row);
    }
    put_maker(world, Function::Workshop(Good::Wine));
    put(world, vendor(world, Good::Wine), x + 2, y - 2);
    put_maker(world, Function::Supplier(Raw::Bronze));
    put_maker(world, Function::Workshop(Good::Arms));
    put(world, vendor(world, Good::Arms), x + 4, y - 2);
    put_maker(world, Function::Producer(Good::Horses));
    put(world, vendor(world, Good::Horses), x + 6, y - 2);
    stage.set(700);
    put(world, kind_of(world, &role(Function::Stadium)), x + 21, y + 1);
    stage.set(0);

    // Maisons le long de la boucle, dedans et dehors, jardins au cœur de l'îlot et
    // derrière les maisons du nord.
    for (at, row) in [(8, -11), (10, -11), (2, -9), (2, -7), (11, -9), (11, -7), (11, -5), (9, -5), (9, -2), (11, -2), (2, -14), (4, -14), (6, -14), (14, -2), (-1, -14)] {
        put(world, Some(house), x + at, y + row);
    }
    for (at, row) in [(4, -9), (6, -9), (8, -9), (4, -7), (6, -7), (8, -7), (2, -16), (4, -16), (6, -16)] {
        put(world, Some(garden), x + at, y + row);
    }
    // Le chantier se déroule par population croissante, dans l'ordre d'inscription.
    let mut plan = plan.into_inner();
    plan.sort_by_key(|&(population, ..)| population);
    advance_demo(world, &mut plan);
    Some(((x + 12, y - 6), plan))
}

/// `olympe installer <source>… [--vers dossier]` : copie les données du jeu d'origine.
fn install(args: &[String]) {
    let mut target = paths::data_dir();
    let mut sources = Vec::new();
    let mut options = args.iter();
    while let Some(option) = options.next() {
        match option.as_str() {
            "--vers" => match options.next() {
                Some(folder) => target = PathBuf::from(folder),
                None => sources.clear(),
            },
            source => sources.push(PathBuf::from(source)),
        }
    }
    if sources.is_empty() {
        eprintln!("{USAGE}");
        std::process::exit(2);
    }
    println!("Installation dans {}", target.display());
    match zeus::install::install(&sources, &target, &mut |line| println!("{line}")) {
        Ok(report) => {
            println!("{} fichiers installés ({} Mo).", report.copied, report.bytes / 1_000_000);
            if report.patched > 0 {
                println!("{} fichiers mis à jour par un correctif.", report.patched);
            }
            if report.converted > 0 {
                println!("{} fichiers audio convertis en Opus.", report.converted);
            }
            if report.unconverted > 0 {
                println!("{} fichiers audio laissés en MP3 (non lus par le jeu).", report.unconverted);
            }
            println!("Lancez le jeu avec : olympe");
        }
        Err(e) => {
            eprintln!("erreur : {e}");
            std::process::exit(1);
        }
    }
}

/// Aventures jouables : (nom affiché, fichier), les grandes aventures d'abord.
fn adventures(data: &Path) -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    let messages = fs::read(data.join("zeus_mm.eng")).ok().and_then(|file| Messages::parse(file).ok());
    let folder = data.join("adventures");
    for entry in fs::read_dir(&folder).into_iter().flatten().flatten() {
        let path = entry.path();
        // Une aventure est un `.pak`, seul ou rangé dans un dossier de même nom.
        let pak = match path.is_dir() {
            true => path.join(format!("{}.pak", entry.file_name().to_string_lossy())),
            false => path,
        };
        if pak.extension().is_some_and(|e| e == "pak") && pak.is_file() {
            let stem = pak.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            // Le jeu préfixe ses aventures d'un symbole qui fixe leur ordre dans sa liste.
            let name = stem.trim_start_matches(|c: char| !c.is_alphanumeric()).to_string();
            let mut letters = name.chars();
            let mut name: String = letters.next().map(|first| first.to_uppercase().chain(letters).collect()).unwrap_or_default();
            // Une aventure livrée avec son fichier de textes y donne son vrai titre.
            let titled = adventure_texts(&pak.with_extension("txt")).and_then(|text| text_entry(&text, "Adventure_Title"));
            let folder_name = name.to_lowercase();
            if let Some(title) = titled {
                name = title;
            }
            // Les leçons du tutoriel portent un simple mot anglais : en fin de liste.
            let lesson = stem.is_ascii() && !stem.contains(' ') && stem == folder_name;
            // Une leçon prend le titre de son récit d'ouverture (« Didacticiel :
            // construire une cité »…), et les leçons se suivent dans l'ordre des récits.
            let intro = lesson
                .then(|| fs::read(&pak).ok().and_then(|file| Settings::parse(&file)))
                .flatten()
                .and_then(|settings| settings.parent.first().map(|episode| episode.intro_message))
                .unwrap_or(0);
            if let Some(message) = messages.as_ref().filter(|_| intro > 0).and_then(|m| m.get(intro as usize)) {
                name = message.title.trim().to_string();
            }
            found.push((lesson, intro, name, pak));
        }
    }
    found.sort();
    found.into_iter().map(|(_, _, name, pak)| (name, pak)).collect()
}

/// Écran de choix d'aventure, sur le fond d'origine du jeu. Rend le fichier choisi, ou
/// `None` si le joueur quitte. Avec `capture`, enregistre l'écran et quitte aussitôt.
fn choose_adventure(data: &Path, capture: Option<&Path>, script: &str, size: (i32, i32)) -> Result<Option<PathBuf>, String> {
    const ROWS: usize = 16;
    let list = adventures(data);
    if list.is_empty() {
        return Err("aucune aventure dans le dossier de données".into());
    }
    let background = fs::read(data.join("data/zeus_fe_choosegame.jpg")).map_err(|e| e.to_string()).and_then(|file| jpeg::decode(&file));
    let text = Text::parse(&fs::read(data.join("zeus_text.eng")).map_err(|e| e.to_string())?)?;
    let level_name = |level: usize| text.get(TEXT_DIFFICULTY, level + 1);
    let mut chosen = None;
    // SAFETY : appels directs à SDL3 ; chaque pointeur rendu est vérifié avant usage.
    unsafe {
        if !sdl::SDL_Init(sdl::INIT_VIDEO) {
            return Err(sdl::error("initialisation de SDL"));
        }
        let title = CString::new("Zeus").unwrap_or_default();
        let window = sdl::SDL_CreateWindow(title.as_ptr(), size.0, size.1, sdl::WINDOW_RESIZABLE | sdl::WINDOW_HIGH_PIXEL_DENSITY);
        let renderer = if window.is_null() { null_mut() } else { sdl::SDL_CreateRenderer(window, null()) };
        if renderer.is_null() {
            return Err(sdl::error("création de la fenêtre"));
        }
        sdl::SDL_SetRenderVSync(renderer, 1);
        // Vidéo d'introduction, lue par Olympe s'il l'a convertie à l'installation.
        if INTRO.load(std::sync::atomic::Ordering::Relaxed) && capture.is_none() && script.is_empty() {
            INTRO.store(false, std::sync::atomic::Ordering::Relaxed);
            // Fenêtre fermée pendant l'introduction : on s'arrête là.
            if play_intro_frames(renderer, data) == Some(true) {
                sdl::SDL_Quit();
                return Ok(None);
            }
        }
        let load = |name: &str| Catalog::load(&data.join("data").join(name));
        let mut graphics = Graphics {
            renderer,
            terrain: load("zeus_terrain.sg3")?,
            poseidon: load("poseidon_loaded.sg3")?,
            general: load("zeus_general.sg3")?,
            fonts: load("zeus_fonts.sg3")?,
            figures: load("sprmain.sg3")?,
            imps: load("poseidonimps.sg3").unwrap_or_else(|_| Catalog::empty()),
            destruction: load("destruction.sg3")?,
            interface: load("zeus_interface.sg3")?,
            sanctuaries: Vec::new(),
            gods: Vec::new(),
            monsters: HashMap::new(),
            heroes: HashMap::new(),
            deities: HashMap::new(),
            armies: HashMap::new(),
            empty: Catalog::empty(),
            ink: (255, 255, 255),
            ground: None,
            sprites: HashMap::new(),
        };
        // Le fond est facultatif : sans lui, la liste s'affiche sur fond noir.
        let backdrop = background.ok().and_then(|picture| {
            let (w, h) = (picture.width as i32, picture.height as i32);
            let texture = sdl::SDL_CreateTexture(renderer, sdl::PIXELFORMAT_ABGR8888, sdl::TEXTUREACCESS_STATIC, w, h);
            let filled = !texture.is_null() && sdl::SDL_UpdateTexture(texture, null(), picture.rgba.as_ptr() as *const c_void, w * 4);
            filled.then_some((texture, picture.width as f32, picture.height as f32))
        });
        let (mut first, mut pointer) = (0usize, (0.0f32, 0.0f32));
        let mut event = sdl::Event::new();
        let mut running = true;
        // Essai scripté : seuls le déplacement (m:x,y), le clic (d) et la molette (w:n) ont un sens ici.
        let mut script: std::collections::VecDeque<&str> = script.split_whitespace().collect();
        while running {
            if let Some(step) = script.pop_front() {
                let numbers: Vec<f32> = step.split_once(':').map_or("", |(_, n)| n).split(',').filter_map(|n| n.parse().ok()).collect();
                let mut injected = match (step.chars().next(), numbers.as_slice()) {
                    (Some('m'), &[x, y]) => {
                        let moved = sdl::Event::mouse_motion(pointer, (x, y), 0);
                        pointer = (x, y);
                        Some(moved)
                    }
                    (Some('d'), _) => Some(sdl::Event::mouse_button(true, sdl::BUTTON_LEFT, pointer)),
                    (Some('w'), &[steps]) => Some(sdl::Event::mouse_wheel(steps, pointer)),
                    _ => None,
                };
                if let Some(injected) = injected.as_mut() {
                    sdl::SDL_PushEvent(injected);
                }
            }
            let (mut w, mut h) = (0, 0);
            sdl::SDL_GetRenderOutputSize(renderer, &mut w, &mut h);
            // Même agrandissement que dans le jeu sur les écrans à haute définition ; tant
            // que l'échelle est en place, SDL rend aussi la souris dans ces unités.
            let ui = (h as f32 / 1080.0).round().max(1.0);
            sdl::SDL_SetRenderScale(renderer, ui, ui);
            let (w, h) = (w as f32 / ui, h as f32 / ui);
            let row = graphics.line_height(FONT_TEXT) + 10.0;
            let rows = ROWS.min(list.len());
            let width = list.iter().map(|(name, _)| graphics.text_width(FONT_TEXT, name)).fold(0.0, f32::max) + 64.0;
            let (left, top) = ((w - width) / 2.0, (h - row * rows as f32) / 2.0 + 30.0);
            let hovered = (pointer.0 >= left && pointer.0 < left + width && pointer.1 >= top)
                .then(|| first + ((pointer.1 - top) / row) as usize)
                .filter(|&index| index < (first + rows).min(list.len()));
            while sdl::SDL_PollEvent(&mut event) {
                // Sur un écran mis à l'échelle, la souris est en points et le rendu en pixels.
                sdl::SDL_ConvertEventToRenderCoordinates(renderer, &mut event);
                match event.kind() {
                    sdl::EVENT_QUIT => running = false,
                    sdl::EVENT_KEY_DOWN => match event.key() {
                        sdl::KEY_ESCAPE | sdl::KEY_Q => running = false,
                        sdl::KEY_UP => first = first.saturating_sub(1),
                        sdl::KEY_DOWN => first = (first + 1).min(list.len() - rows),
                        sdl::KEY_D => DIFFICULTY.store((difficulty() + 1) % DIFFICULTY_MODELS.len(), std::sync::atomic::Ordering::Relaxed),
                        _ => {}
                    },
                    sdl::EVENT_MOUSE_MOTION => pointer = event.position(),
                    sdl::EVENT_MOUSE_WHEEL => {
                        let steps = event.wheel().0;
                        first = if steps > 0.0 { first.saturating_sub(2) } else { (first + 2).min(list.len() - rows) };
                    }
                    sdl::EVENT_MOUSE_BUTTON_DOWN if event.button() == sdl::BUTTON_LEFT => {
                        pointer = event.position();
                        let index = first + ((pointer.1 - top) / row).max(0.0) as usize;
                        if pointer.0 >= left && pointer.0 < left + width && pointer.1 >= top && index < (first + rows).min(list.len()) {
                            chosen = Some(list[index].1.clone());
                            running = false;
                        }
                        // Ligne de la difficulté, sous la liste : un clic passe au niveau suivant.
                        let below = top + row * rows as f32 + 16.0;
                        if pointer.0 >= left && pointer.0 < left + width && pointer.1 >= below && pointer.1 < below + row {
                            DIFFICULTY.store((difficulty() + 1) % DIFFICULTY_MODELS.len(), std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    _ => {}
                }
            }

            sdl::SDL_SetRenderDrawColor(renderer, 0, 0, 0, 255);
            sdl::SDL_RenderClear(renderer);
            // Le fond couvre toute la fenêtre, quitte à déborder d'un côté.
            let mut banner = (w / 2.0, 40.0);
            if let Some((texture, bw, bh)) = backdrop {
                let scale = (w / bw).max(h / bh);
                let place = sdl::FRect { x: (w - bw * scale) / 2.0, y: (h - bh * scale) / 2.0, w: bw * scale, h: bh * scale };
                sdl::SDL_SetTextureScaleMode(texture, sdl::SCALEMODE_LINEAR);
                sdl::SDL_RenderTexture(renderer, texture, null(), &place);
                // Centre du bandeau bleu peint sur le fond.
                banner = (place.x + CHOOSER_BANNER.0 * scale, place.y + CHOOSER_BANNER.1 * scale);
            }
            let heading = "Choisissez une aventure";
            let heading_width = graphics.text_width(FONT_TITLE, heading);
            graphics.draw_text(FONT_TITLE, heading, banner.0 - heading_width / 2.0, banner.1 - graphics.line_height(FONT_TITLE) / 2.0);
            graphics.panel(left - 8.0, top - 8.0, width + 16.0, row * rows as f32 + 16.0);
            for (line, (name, _)) in list.iter().enumerate().skip(first).take(rows) {
                let y = top + row * (line - first) as f32;
                if hovered == Some(line) {
                    sdl::SDL_SetRenderDrawColor(renderer, 200, 160, 40, 140);
                    sdl::SDL_RenderFillRect(renderer, &sdl::FRect { x: left, y, w: width, h: row - 2.0 });
                }
                graphics.draw_text(FONT_TEXT, name, left + 32.0, y + 4.0);
            }
            let below = top + row * rows as f32 + 16.0;
            graphics.panel(left - 8.0, below - 4.0, width + 16.0, row + 8.0);
            let label = format!("{} : {} (D)", text.get(TEXT_DIFFICULTY, 0), level_name(difficulty()));
            graphics.draw_text(FONT_TEXT, &label, left + 32.0, below + 4.0);
            if let (Some(file), true, true) = (capture, script.is_empty(), running) {
                save_frame(renderer, file)?;
                running = false;
            }
            sdl::SDL_RenderPresent(renderer);
        }
        sdl::SDL_Quit();
    }
    Ok(chosen)
}

fn run(
    data: &Path,
    map_path: &Path,
    demo: bool,
    capture: Option<&Path>,
    warmup: u32,
    script: &str,
    size: (i32, i32),
    measure: u32,
) -> Result<(), String> {
    let (map, colony_maps, settings) = load_map(map_path)?;
    let read = |file: &str| fs::read(data.join(file)).map_err(|e| format!("{file} : {e}"));
    let text = Text::parse(&read("zeus_text.eng")?)?;
    let model = Model::parse(&read(&format!("model/zeus_model_{}.txt", DIFFICULTY_MODELS[difficulty()]))?)?;
    let general = Catalog::load(&data.join("data/zeus_general.sg3"))?;
    let figures = Catalog::load(&data.join("data/sprmain.sg3"))?;
    let imps = Catalog::load(&data.join("data/poseidonimps.sg3")).unwrap_or_else(|_| Catalog::empty());
    let road_cost = model.buildings.get(TYPE_ROAD).map_or(0, |b| b.cost);
    let road_name = text.get(TEXT_BUILDINGS, TYPE_ROAD);
    let wall_cost = model.buildings.get(TYPE_WALL).map_or(0, |b| b.cost);
    let wall_name = text.get(TEXT_BUILDINGS, TYPE_WALL);
    let demolish_name = text.get(88, 13);
    let months: Vec<String> = (0..12).map(|m| text.get(25, m)).collect();

    let group = |index: usize| figures.groups.get(index).copied().unwrap_or(0);
    let rules = |kind: usize| model.buildings.get(kind).ok_or(format!("type {kind} absent des règles"));
    let size_of = |image: usize| general.images.get(image).map(|i| ((i.width + 2) / 60).max(1)).ok_or(format!("image {image} absente"));
    let appeal_of = |kind: usize| {
        rules(kind).map(|r| Appeal { value: r.desirability, step: r.desirability_step, step_size: r.desirability_step_size, range: r.desirability_range })
    };
    let mut kinds = Vec::new();
    for (kind, image, function, walker) in SIMPLE_KINDS {
        let mut entry = Kind::simple(text.get(TEXT_BUILDINGS, kind), rules(kind)?.cost, size_of(image)?, image, function, group(walker));
        entry.employees = rules(kind)?.employees;
        entry.appeal = appeal_of(kind)?;
        (entry.fire_risk, entry.damage_risk) = (rules(kind)?.fire_risk, rules(kind)?.damage_risk);
        entry.product = PRODUCTS.iter().find(|&&(k, _)| k == kind).map_or(0, |&(_, product)| product);
        kinds.push(entry);
    }
    for ((kind, crop), product) in FARMS.into_iter().zip(FARM_RESOURCES) {
        let mut parts = vec![Part { dx: 0, dy: 0, size: size_of(FARM_HOUSE_IMAGE)?, image: FARM_HOUSE_IMAGE, crop: false, bay: None, source: Source::General }];
        parts.extend(FARM_FIELDS.map(|(dx, dy)| Part { dx, dy, size: 1, image: crop, crop: true, bay: None, source: Source::General }));
        kinds.push(Kind {
            name: text.get(TEXT_BUILDINGS, kind),
            cost: rules(kind)?.cost,
            size: 3,
            depth: 3,
            parts,
            function: Function::Farm,
            walker: 0,
            stock_overlay: None,
            employees: rules(kind)?.employees,
            appeal: appeal_of(kind)?,
            fire_risk: rules(kind)?.fire_risk,
            damage_risk: rules(kind)?.damage_risk,
            product,
        });
    }
    // Entrepôt : une cabane au coin du fond et huit baies.
    let bays = (0..3).flat_map(|dy| (0..3).map(move |dx| (dx, dy))).filter(|&at| at != (0, 0));
    let mut storehouse_parts: Vec<Part> = vec![Part { dx: 0, dy: 0, size: 1, image: STOREHOUSE_HUT_IMAGE, crop: false, bay: None, source: Source::General }];
    storehouse_parts.extend(bays.enumerate().map(|(bay, (dx, dy))| Part { dx, dy, size: 1, image: EMPTY_BAY_IMAGE, crop: false, bay: Some(bay as u8), source: Source::General }));
    kinds.push(Kind {
        name: text.get(TEXT_BUILDINGS, TYPE_STOREHOUSE),
        cost: rules(TYPE_STOREHOUSE)?.cost,
        size: 3,
        depth: 3,
        parts: storehouse_parts.clone(),
        function: Function::Storehouse,
        walker: 0,
        stock_overlay: None,
        employees: rules(TYPE_STOREHOUSE)?.employees,
        appeal: appeal_of(TYPE_STOREHOUSE)?,
        fire_risk: rules(TYPE_STOREHOUSE)?.fire_risk,
        damage_risk: rules(TYPE_STOREHOUSE)?.damage_risk,
        product: 0,
    });
    let mut plot_appeal = appeal_of(TYPE_AGORA)?;
    plot_appeal.value /= AGORA_PLOTS;
    // Logement luxueux : quatre quarts 2 × 2, dont l'image suit le niveau.
    let quarters = [(0, 0), (2, 0), (0, 2), (2, 2)].into_iter().enumerate();
    kinds.push(Kind {
        name: text.get(TEXT_BUILDINGS, TYPE_ELITE_HOUSE),
        cost: rules(TYPE_ELITE_HOUSE)?.cost,
        size: 4,
        depth: 4,
        parts: quarters.map(|(quarter, (dx, dy))| Part { dx, dy, size: 2, image: ELITE_LOT_IMAGE, crop: false, bay: Some(quarter as u8), source: Source::General }).collect(),
        function: Function::House,
        walker: 0,
        stock_overlay: None,
        employees: 0,
        appeal: appeal_of(TYPE_ELITE_HOUSE)?,
        fire_risk: rules(TYPE_ELITE_HOUSE)?.fire_risk,
        damage_risk: rules(TYPE_ELITE_HOUSE)?.damage_risk,
        product: 0,
    });
    // Palais : la disposition est celle de `model/palace.txt` — deux moitiés 4 × 4
    // l'une derrière l'autre, bordées de dalles sur trois côtés et de deux dalles
    // d'angle (6 × 9 cases).
    let mut palace_parts = vec![
        Part { dx: 1, dy: 1, size: 4, image: PALACE_BACK_IMAGE, crop: false, bay: None, source: Source::General },
        Part { dx: 1, dy: 5, size: 4, image: PALACE_FRONT_IMAGE, crop: false, bay: None, source: Source::General },
    ];
    let border = (0..6).map(|dx| (dx, 0)).chain((1..9).flat_map(|dy| [(0, dy), (5, dy)]));
    palace_parts.extend(border.map(|(dx, dy)| {
        let image = if dy == 0 && (dx == 0 || dx == 5) { PALACE_CORNER_IMAGE } else { PALACE_TILE_IMAGE };
        Part { dx, dy, size: 1, image, crop: false, bay: None, source: Source::General }
    }));
    kinds.push(Kind {
        name: text.get(TEXT_BUILDINGS, TYPE_PALACE),
        cost: rules(TYPE_PALACE)?.cost,
        size: 6,
        depth: 9,
        parts: palace_parts,
        function: Function::Palace,
        walker: 0,
        stock_overlay: None,
        employees: rules(TYPE_PALACE)?.employees,
        appeal: appeal_of(TYPE_PALACE)?,
        fire_risk: rules(TYPE_PALACE)?.fire_risk,
        damage_risk: rules(TYPE_PALACE)?.damage_risk,
        product: 0,
    });
    for (kind, grand) in [(TYPE_AGORA, false), (TYPE_GRAND_AGORA, true)] {
        kinds.push(Kind::simple(text.get(TEXT_BUILDINGS, kind), rules(kind)?.cost, 2, PLOT_IMAGE, Function::Agora { grand }, 0));
    }
    let mut plot = Kind::simple(text.get(TEXT_BUILDINGS, TYPE_PLOT), 0, 2, PLOT_IMAGE, Function::Plot, 0);
    plot.appeal = plot_appeal;
    kinds.push(plot);
    for (kind, image, goods_image, good, peddler) in VENDORS {
        let mut vendor =
            Kind::simple(text.get(TEXT_BUILDINGS, kind), rules(kind)?.cost, size_of(image)?, image, Function::Vendor(good), group(peddler));
        vendor.stock_overlay = Some(goods_image);
        vendor.employees = rules(kind)?.employees;
        vendor.appeal = plot_appeal;
        (vendor.fire_risk, vendor.damage_risk) = (rules(kind)?.fire_risk, rules(kind)?.damage_risk);
        kinds.push(vendor);
    }
    // Sanctuaires : un par dieu, disposés comme dans les fichiers du jeu.
    for god in 0..sanctuary::GODS {
        let kind = sanctuary::FIRST_TYPE + god;
        let layout = fs::read(data.join(format!("model/sanctuary_{}.txt", sanctuary::LAYOUTS[god]))).ok();
        let (Some(layout), Ok(rule)) = (layout.and_then(|t| Layout::parse(&String::from_utf8_lossy(&t))), rules(kind)) else { continue };
        let [marble, wood, sculptures] = layout.materials().map(|n| n as u16);
        kinds.push(Kind {
            name: text.get(TEXT_BUILDINGS, kind),
            cost: rule.cost,
            size: layout.width,
            depth: layout.depth,
            parts: layout.pieces.iter().map(|&(dx, dy, piece)| sanctuary_part(dx, dy, piece)).collect(),
            function: Function::Sanctuary { god: god as u8, needs: [marble, wood, sculptures] },
            walker: 0,
            stock_overlay: None,
            employees: rule.employees,
            appeal: appeal_of(kind)?,
            fire_risk: rule.fire_risk,
            damage_risk: rule.damage_risk,
            product: 0,
        });
    }
    // Temples des héros : un par héros, la statue du héros devant (les six de Zeus).
    for hero in 0..heroes::HEROES {
        let rule = rules(TYPE_HERO_HALL)?;
        let mut parts = vec![Part { dx: 0, dy: 0, size: 4, image: HERO_HALL_IMAGE, crop: false, bay: None, source: Source::General }];
        if hero < HERO_STATUES {
            parts.push(Part { dx: 1, dy: 3, size: 1, image: HERO_STATUE_IMAGE + hero, crop: false, bay: None, source: Source::General });
        }
        kinds.push(Kind {
            name: text.get(heroes::TEXT_HALLS, heroes::FIRST_HALL_TEXT + hero),
            cost: rule.cost,
            size: 4,
            depth: 4,
            parts,
            function: Function::HeroHall(hero as u8),
            walker: 0,
            stock_overlay: None,
            employees: rule.employees,
            appeal: appeal_of(TYPE_HERO_HALL)?,
            fire_risk: rule.fire_risk,
            damage_risk: rule.damage_risk,
            product: 0,
        });
    }
    // Monuments de Poséidon, ajoutés en dernier pour que les sauvegardes antérieures
    // gardent leurs numéros de types.
    let mut monument_layouts = Vec::new();
    for model in 0..monument::MONUMENTS {
        let kind = monument::FIRST_TYPE + model;
        let layout = fs::read(data.join(format!("model/poseidon_pyramid_{:02}.txt", model + 1))).ok();
        let (Some(layout), Ok(rule)) = (layout.and_then(|t| monument::Layout::parse(&String::from_utf8_lossy(&t))), rules(kind)) else { break };
        // Statues, autel et temple posés sur le monument : images des sanctuaires ; les
        // statues au choix du joueur (141 à 148) sont celles de Zeus.
        let parts = layout
            .tiles
            .iter()
            .map(|t| match t.piece {
                2 | 7..=54 | 61..=108 | 110 => sanctuary_part(t.dx, t.dy, Piece::from_type(t.piece as i32)),
                141..=148 => sanctuary_part(t.dx, t.dy, Piece::Statue { god: 0, facing: (t.piece as usize - 141) % 4 }),
                _ => Part { dx: t.dx, dy: t.dy, size: 1, image: MONUMENT_FLOOR_IMAGE, crop: false, bay: None, source: Source::Poseidon },
            })
            .collect();
        kinds.push(Kind {
            name: text.get(TEXT_BUILDINGS, kind),
            cost: rule.cost,
            size: layout.width,
            depth: layout.depth,
            parts,
            function: Function::Monument { model: model as u8, needs: layout.materials() },
            walker: 0,
            stock_overlay: None,
            employees: rule.employees,
            appeal: appeal_of(kind)?,
            fire_risk: rule.fire_risk,
            damage_risk: rule.damage_risk,
            product: 0,
        });
        monument_layouts.push(layout);
    }
    // Tour : l'image du catalogue général, les règles du type 132.
    let mut tower = Kind::simple(text.get(TEXT_BUILDINGS, TYPE_TOWER), rules(TYPE_TOWER)?.cost, size_of(TOWER_IMAGE)?, TOWER_IMAGE, Function::Tower, 0);
    tower.employees = rules(TYPE_TOWER)?.employees;
    tower.appeal = appeal_of(TYPE_TOWER)?;
    (tower.fire_risk, tower.damage_risk) = (rules(TYPE_TOWER)?.fire_risk, rules(TYPE_TOWER)?.damage_risk);
    // Quai des trières : le jeu n'en a pas d'image reconnue ; Olympe prend celle d'une
    // pêcherie.
    let mut wharf = Kind::simple(text.get(TEXT_BUILDINGS, TYPE_WHARF), rules(TYPE_WHARF)?.cost, 3, WHARF_IMAGE, Function::Wharf, 0);
    wharf.employees = rules(TYPE_WHARF)?.employees;
    wharf.appeal = appeal_of(TYPE_WHARF)?;
    (wharf.fire_risk, wharf.damage_risk) = (rules(TYPE_WHARF)?.fire_risk, rules(TYPE_WHARF)?.damage_risk);
    kinds.push(wharf);
    // Section d'hippodrome, raccordée aux voisines.
    let mut racetrack = Kind::simple(text.get(TEXT_BUILDINGS, TYPE_RACETRACK), rules(TYPE_RACETRACK)?.cost, zeus::world::RACETRACK_SIZE, RACETRACK_IMAGE, Function::Racetrack, 0);
    racetrack.parts[0].source = Source::Poseidon;
    racetrack.employees = rules(TYPE_RACETRACK)?.employees;
    racetrack.appeal = appeal_of(TYPE_RACETRACK)?;
    kinds.push(racetrack);
    // Atelier de marbre noir et raffinerie d'orichalque (Poséidon) : posés n'importe
    // où, sans carrière ni mine (raccourci, comme l'atelier de maçonnerie) ; images
    // attribuées à l'œil dans `poseidon_loaded.sg3`.
    for (kind, image, good) in [(TYPE_BLACK_MARBLE_SHOP, 700, Good::BlackMarble), (TYPE_ORICHALC_REFINERY, 665, Good::Orichalc)] {
        let mut entry = Kind::simple(text.get(TEXT_BUILDINGS, kind), rules(kind)?.cost, 2, image, Function::Producer(good), 0);
        entry.parts[0].source = Source::Poseidon;
        entry.employees = rules(kind)?.employees;
        entry.appeal = appeal_of(kind)?;
        (entry.fire_risk, entry.damage_risk) = (rules(kind)?.fire_risk, rules(kind)?.damage_risk);
        entry.product = zeus::world::resource_of(good).unwrap_or(0);
        kinds.push(entry);
    }
    // Ajoutés en dernier : les sauvegardes antérieures gardent leurs numéros de types.
    kinds.push(tower);
    let mut gatehouse = Kind::simple(text.get(TEXT_BUILDINGS, TYPE_GATEHOUSE), rules(TYPE_GATEHOUSE)?.cost, size_of(GATEHOUSE_IMAGE)?, GATEHOUSE_IMAGE, Function::Gatehouse, 0);
    gatehouse.appeal = appeal_of(TYPE_GATEHOUSE)?;
    kinds.push(gatehouse);
    // Science des cités atlantes (Poséidon) : chaque bâtiment tient le rôle d'un lieu
    // de culture grecque et sert la même discipline (images de `poseidon_loaded.sg3`
    // attribuées à l'œil, tailles d'après leur largeur).
    for (kind, image, size, function, walker) in SCIENCE_KINDS {
        let mut entry = Kind::simple(text.get(TEXT_BUILDINGS, kind), rules(kind)?.cost, size, image, function, group(walker));
        entry.parts[0].source = Source::Poseidon;
        entry.employees = rules(kind)?.employees;
        entry.appeal = appeal_of(kind)?;
        (entry.fire_risk, entry.damage_risk) = (rules(kind)?.fire_risk, rules(kind)?.damage_risk);
        kinds.push(entry);
    }
    // Monuments commémoratifs, ajoutés après les autres pour la même raison.
    for model in 0..zeus::world::COMMEMORATIVES {
        let name = format!("{}, {}", text.get(TEXT_BUILDINGS, TYPE_COMMEMORATIVE), COMMEMORATIVE_SUBJECTS[model]);
        let mut monument = Kind::simple(name, rules(TYPE_COMMEMORATIVE)?.cost, 3, COMMEMORATIVE_IMAGE + model, Function::Commemorative(model as u8), 0);
        monument.appeal = appeal_of(TYPE_COMMEMORATIVE)?;
        kinds.push(monument);
    }
    // Barrage routier, ajouté après les autres pour la même raison.
    let mut roadblock = Kind::simple(text.get(TEXT_BUILDINGS, TYPE_ROADBLOCK), rules(TYPE_ROADBLOCK)?.cost, 1, ROADBLOCK_IMAGE, Function::Roadblock, 0);
    roadblock.appeal = appeal_of(TYPE_ROADBLOCK)?;
    kinds.push(roadblock);
    // Chars de Poséidon : la fabrique, puis le marchand (son marchand ambulant est celui
    // des chevaux, faute d'avoir relevé le sien).
    let mut factory = Kind::simple(text.get(TEXT_BUILDINGS, TYPE_CHARIOT_WORKSHOP), rules(TYPE_CHARIOT_WORKSHOP)?.cost, 4, CHARIOT_WORKSHOP_IMAGE, Function::Workshop(Good::Chariot), 0);
    factory.parts[0].source = Source::Poseidon;
    factory.employees = rules(TYPE_CHARIOT_WORKSHOP)?.employees;
    factory.appeal = appeal_of(TYPE_CHARIOT_WORKSHOP)?;
    (factory.fire_risk, factory.damage_risk) = (rules(TYPE_CHARIOT_WORKSHOP)?.fire_risk, rules(TYPE_CHARIOT_WORKSHOP)?.damage_risk);
    factory.product = zeus::world::resource_of(Good::Chariot).unwrap_or(0);
    kinds.push(factory);
    let mut chariots = Kind::simple(text.get(TEXT_BUILDINGS, TYPE_CHARIOT_VENDOR), rules(TYPE_CHARIOT_VENDOR)?.cost, 2, CHARIOT_VENDOR_IMAGE, Function::Vendor(Good::Chariot), group(GROUP_HORSE_TRAINER));
    chariots.parts[0].source = Source::Poseidon;
    chariots.stock_overlay = Some(CHARIOT_VENDOR_IMAGE + 1);
    chariots.employees = rules(TYPE_CHARIOT_VENDOR)?.employees;
    chariots.appeal = plot_appeal;
    (chariots.fire_risk, chariots.damage_risk) = (rules(TYPE_CHARIOT_VENDOR)?.fire_risk, rules(TYPE_CHARIOT_VENDOR)?.damage_risk);
    kinds.push(chariots);
    for (kind, hut, sea) in [(TYPE_TRADING_POST, TRADING_POST_HUT_IMAGE, false), (TYPE_DOCK, DOCK_HUT_IMAGE, true)] {
        let mut parts = storehouse_parts.clone();
        parts[0].image = hut;
        kinds.push(Kind {
            name: text.get(TEXT_BUILDINGS, kind),
            cost: rules(kind)?.cost,
            size: 3,
            depth: 3,
            parts,
            function: Function::TradePost { sea },
            walker: 0,
            stock_overlay: None,
            employees: rules(kind)?.employees,
            appeal: appeal_of(kind)?,
            fire_risk: rules(kind)?.fire_risk,
            damage_risk: rules(kind)?.damage_risk,
            product: 0,
        });
    }
    let _ = ALLOWED_SLOT.set(allowed_slots(&kinds, &text));
    let _ = NATION.set(nations(&kinds, &text));
    let walkers = WalkerImages { immigrant: group(GROUP_IMMIGRANT), elite_immigrant: group(GROUP_ELITE_IMMIGRANT), carrier: group(GROUP_CARRIER), hoplite: group(GROUP_HOPLITE), rabble: group(GROUP_RABBLE), horseman: group(GROUP_HORSEMAN), ship: group(GROUP_SHIP), buyer: group(GROUP_BUYER), atlantean: GROUPS_ATLANTEAN.map(|g| imps.groups.get(g).copied().unwrap_or(0)) };
    // Les niveaux de logement se suivent dans les noms de bâtiments : modestes, puis luxueux.
    let level_names: Vec<String> = (0..13).map(|level| text.get(TEXT_BUILDINGS, TYPE_HOUSE + level)).collect();
    let good_names = GOOD_TEXTS.map(|index| text.get(23, index));
    let funds = settings.as_ref().map_or(STARTING_MONEY, |s| s.funds) * DIFFICULTY_FUNDS_PERCENT[difficulty()] / 100;
    let mut world = World::new(map, kinds, model.houses.clone(), walkers, funds);
    world.cities = world.map.cities();
    world.monument_layouts = monument_layouts;
    // Caractéristiques de combat du niveau de difficulté (à défaut, valeurs de secours).
    match read(&format!("model/figure_model_{}.txt", DIFFICULTY_MODELS[difficulty()])).and_then(|f| zeus::model::Fighters::parse(&f)) {
        Ok(fighters) => world.fighters = fighters,
        Err(e) => eprintln!("règles des personnages : {e}"),
    }
    // Impôts : taux du jeu et leur effet sur l'humeur à ce niveau de difficulté.
    match read("model/tax_sentiment_model.txt").and_then(|f| zeus::model::Taxes::parse(&f)) {
        Ok(taxes) => world.taxes = taxes,
        Err(e) => eprintln!("impôts : {e}"),
    }
    world.difficulty = difficulty();
    if AUDIT.load(std::sync::atomic::Ordering::Relaxed) {
        return audit(data, &world, &text);
    }
    let start_year = settings.as_ref().map_or(0, |s| s.start_year);
    world.start_year = start_year;
    let mut journey = Journey { order: settings.as_ref().map(Settings::order).unwrap_or_default(), position: 0, home: None, colony_maps };
    let episode = |step: Step| settings.as_ref().and_then(|s| s.episode(step));
    // La démonstration a droit à tout.
    if let Some(first) = episode(Step::Parent(0)).filter(|_| !demo) {
        lock_resources(&mut world, first);
        begin_events(&mut world, first);
    }
    let mut tool = Tool::None;
    let name = map_path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    // Nom affiché : le titre de l'aventure quand elle le donne, sinon le nom du fichier
    // sans le symbole dont le jeu préfixe ses aventures.
    let display_name = {
        let folder = map_path.parent().filter(|dir| dir.file_name().is_some_and(|n| n != "adventures"));
        let texts = folder.and_then(|dir| fs::read_dir(dir).ok()).into_iter().flatten().flatten().map(|e| e.path());
        let titled = texts
            .filter(|p| p.extension().is_some_and(|e| e == "txt"))
            .find_map(|file| text_entry(&adventure_texts(&file)?, "Adventure_Title"));
        let cleaned = name.trim_start_matches(|c: char| !c.is_alphanumeric());
        let mut letters = cleaned.chars();
        // Le nom de l'écran des aventures (titre des leçons du tutoriel compris).
        let canonical = |path: &Path| fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let listed = adventures(data).into_iter().find(|(_, pak)| canonical(pak) == canonical(map_path)).map(|(name, _)| name);
        listed.or(titled).unwrap_or_else(|| letters.next().map(|first| first.to_uppercase().chain(letters).collect()).unwrap_or_default())
    };
    let title = |extra: &str| CString::new(format!("Zeus — {display_name}{extra}")).unwrap_or_default();

    // SAFETY : appels directs à SDL3 ; chaque pointeur rendu est vérifié avant usage.
    unsafe {
        if !sdl::SDL_Init(sdl::INIT_VIDEO) {
            return Err(sdl::error("initialisation de SDL"));
        }
        let flags = sdl::WINDOW_RESIZABLE | sdl::WINDOW_HIGH_PIXEL_DENSITY;
        let window = sdl::SDL_CreateWindow(title("").as_ptr(), size.0, size.1, flags);
        if window.is_null() {
            return Err(sdl::error("création de la fenêtre"));
        }
        let renderer = sdl::SDL_CreateRenderer(window, null());
        if renderer.is_null() {
            return Err(sdl::error("création du moteur de rendu"));
        }
        // En mesure, la synchronisation verticale est coupée pour connaître la cadence
        // que la machine peut tenir, et non celle de l'écran.
        sdl::SDL_SetRenderVSync(renderer, if measure > 0 { 0 } else { 1 });

        let mut graphics = Graphics {
            renderer,
            terrain: Catalog::load(&data.join("data/zeus_terrain.sg3"))?,
            poseidon: Catalog::load(&data.join("data/poseidon_loaded.sg3"))?,
            general,
            fonts: Catalog::load(&data.join("data/zeus_fonts.sg3"))?,
            figures,
            imps,
            destruction: Catalog::load(&data.join("data/destruction.sg3"))?,
            interface: Catalog::load(&data.join("data/zeus_interface.sg3"))?,
            // Facultatifs : sans eux, les sanctuaires se dessinent sans temple ni statues.
            sanctuaries: (1..=3).map(|n| Catalog::load(&data.join(format!("data/zeus_sanctuary{n}.sg3"))).unwrap_or_else(|_| Catalog::empty())).collect(),
            gods: (1..=12).map(|n| Catalog::load(&data.join(format!("data/zeus_god{n:02}.sg3"))).unwrap_or_else(|_| Catalog::empty())).collect(),
            monsters: HashMap::new(),
            heroes: HashMap::new(),
            deities: HashMap::new(),
            armies: HashMap::new(),
            empty: Catalog::empty(),
            ink: (255, 255, 255),
            ground: None,
            sprites: HashMap::new(),
        };

        // Départ au centre de la zone jouable, ou sur la scène de démonstration.
        let (start, mut demo_plan) = match demo.then(|| demo_scene(&mut world, road_cost)).flatten() {
            Some((start, plan)) => (start, plan),
            None => (opening_site(&world), DemoPlan::new()),
        };

        let (start_x, start_y) = tile_origin(start.0, start.1);
        let mut view = View { cx: start_x, cy: start_y, zoom: 1.0 };
        // Échelle de l'interface : 1 jusqu'à 1440 lignes, 2 en 4K, etc. Fixée à la
        // première image, avec le zoom de départ.
        let mut ui = 0.0f32;
        let mut fullscreen = false;
        let mut pointer = (0.0f32, 0.0f32);
        let mut hovered = (-1, -1);
        // Dernière case d'un glissement en cours (route ou démolition).
        let mut drag_from: Option<(i32, i32)> = None;
        let mut event = sdl::Event::new();
        let mut running = true;
        // (profondeur, couche, coin haut-gauche en pixels de monde, catalogue, image)
        let mut draw_list: Vec<(i32, i32, f32, f32, Source, usize, Option<(u8, u8, u8)>)> = Vec::new();
        // Emplacement de la liste des outils : (x, y de la première ligne, largeur, hauteur de ligne).
        // Action de chaque ligne du menu, telle qu'affichée à la dernière image.
        let mut menu_rows: Vec<((f32, f32, f32, f32), MenuRow)> = Vec::new();
        let mut open_category: Option<usize> = None;
        // Groupe de bâtiments déplié dans la catégorie ouverte (image de son bouton).
        let mut open_group: Option<usize> = None;
        // Élément survolé et depuis quand, pour les bulles d'aide.
        let mut tooltip: (Option<i64>, u64) = (None, 0);
        // Fiche ouverte par un clic droit (case du bâtiment), et où le bouton droit a été
        // enfoncé avec le chemin parcouru depuis, pour distinguer un clic d'un glissé.
        let mut info_pinned: Option<(i32, i32)> = None;
        let mut right_press: Option<f32> = None;
        let category_names: Vec<String> =
            CATEGORY_NAMES.iter().map(|&(index, fallback)| index.map(|i| text.get(4, i).trim().to_string()).filter(|n| !n.is_empty()).unwrap_or(fallback.to_string())).collect();
        // Une sauvegarde par carte, à côté de l'exécutable lancé.
        // Les parties sont rangées avec les données, une par carte.
        let saves = paths::saves_dir(data);
        let _ = fs::create_dir_all(&saves);
        let save_file = saves.join(format!("{name}.zsauv"));
        // Message passager en bas d'écran, et instant où il est apparu.
        let mut notice = ("H : aide".to_string(), sdl::SDL_GetTicks());
        let mut help = false;
        // Menu du bandeau ouvert (0 Fichier, 1 Options, 2 Aide).
        let mut top_menu: Option<usize> = None;
        // Second onglet du panneau : la carte générale y remplace les informations.
        let mut sidebar_map = false;
        // Images dessinées, et instant de la première, pour `--mesure`.
        let (mut frames, mut measure_start, mut slowest, mut previous_frame) = (0u32, 0u64, 0u64, 0u64);
        // Récit d'ouverture, affiché jusqu'au premier clic ou à la première touche. Pas
        // en capture, sauf si le script commence par « intro ».
        let mut intro = story(data, map_path, settings.as_ref(), Step::Parent(0), Moment::Opening)
            .filter(|_| capture.is_none() || script.split_whitespace().next() == Some("intro"));
        // Objectifs de l'épisode en cours ; ceux déjà atteints, pour n'annoncer que les
        // nouveaux ; panneau ouvert par O.
        let mut goals: Vec<Goal> = episode(Step::Parent(0)).map(|e| e.goals.clone()).unwrap_or_default();
        let mut goals_met = vec![false; goals.len()];
        let (mut goals_panel, mut won) = (false, false);
        // Lettres d'événements du jeu, en attente d'affichage (titre, paragraphes) ; elles
        // s'ouvrent comme les récits, et le temps s'arrête tant qu'on ne les a pas lues.
        let letters = zeus::letters::Letters::parse(&fs::read(data.join("model/zeus eventmsg.txt")).unwrap_or_default());
        let mut letter_queue: std::collections::VecDeque<(String, Vec<String>)> = std::collections::VecDeque::new();
        let leaders = world.map.city_leaders();
        if let Ok(mut titles) = CITY_TITLES.write() {
            *titles = world.map.city_titles();
        }
        // Colonies proposées au choix du joueur, et celle qu'il a choisie.
        let (mut colony_choice, mut colony_pick): (Option<Vec<usize>>, Option<usize>) = (None, None);
        // Panneau des cités du monde, ouvert par C.
        let mut cities_panel = false;
        // Compagnie en préparation, ouverte depuis le panneau des cités.
        let mut company: Option<CompanyDraft> = None;
        // Panneau des conseillers, ouvert par A.
        let mut advisor_panel = false;
        let mut advisor_page = 0usize;
        // Invasion en cours à l'image précédente, pour l'annoncer à son début.
        let mut raid_seen = world.raid.is_some();
        // Sanctuaires achevés, pour débloquer ce qu'ils apportent dès leur achèvement.
        let mut sanctuaries_built = world.sanctuaries().len();
        // Étape « fin » d'un essai scripté : l'épisode se termine comme si ses objectifs
        // étaient atteints.
        let mut force_end = false;
        let mut clock = sdl::SDL_GetTicks();
        let mut paused = false;
        // Vue en surimpression (touche V).
        let mut overlay = Overlay::None;
        // Description de chaque type de bâtiment, pour sa fiche.
        let mut descriptions = building_descriptions(&text, &world.kinds);
        // Chaque monument commémoratif dit ce qu'il célèbre.
        for (kind, description) in world.kinds.iter().zip(descriptions.iter_mut()) {
            if let Function::Commemorative(model) = kind.function {
                *description = Some(text.get(TEXT_COMMEMORATIVES, 1 + model as usize));
            }
        }
        // Journal des annonces (touche J) : date et texte, la plus récente en dernier.
        let mut journal: Vec<(String, String)> = Vec::new();
        let (mut journal_panel, mut logged) = (false, notice.clone());
        // Annonces arrivées depuis la dernière ouverture du journal.
        let mut unread = 0usize;
        // Monuments commémoratifs à poser : un de plus s'annonce.
        let mut commemoratives = world.commemoratives.len();
        let mut speed = NORMAL_SPEED;
        graphics.set_ground(plain_ground(&world));
        // Mini-carte : texture, emplacement à l'écran (x, y) et images écoulées depuis sa mise à jour.
        let minimap = sdl::SDL_CreateTexture(renderer, sdl::PIXELFORMAT_ABGR8888, sdl::TEXTUREACCESS_STATIC, MINIMAP_WIDTH as i32, MINIMAP_HEIGHT as i32);
        let (mut minimap_at, mut minimap_age) = (Minimap::new(&world), MINIMAP_REFRESH);
        // Image intermédiaire de la scène aux zooms non entiers : (texture, largeur, hauteur).
        let mut scene: Option<(sdl::Texture, i32, i32)> = None;
        // Essai scripté (`--script "k:2 m:800,450 d u t:500"`) : étapes restantes, pointeur et boutons simulés.
        let mut script: std::collections::VecDeque<&str> = script.split_whitespace().collect();
        let (mut script_pointer, mut script_buttons) = ((0.0f32, 0.0f32), 0u32);
        // La musique est facultative : sans sortie son, le jeu continue en silence.
        let mut music = match capture {
            Some(_) => None,
            None => Music::open(&data.join("audio"), sdl::SDL_GetTicks() as usize)
                .inspect_err(|e| eprintln!("pas de musique : {e}"))
                .ok(),
        };
        // En capture, on laisse d'abord la cité vivre un moment.
        if capture.is_some() || measure > 0 {
            for _ in 0..warmup {
                world.tick();
                advance_demo(&mut world, &mut demo_plan);
            }
            // Vérification : une sauvegarde rechargée redonne exactement le même état.
            let saved = world.save();
            world.load(&saved)?;
            println!("sauvegarde : {} octets, aller-retour {}", saved.len(), if world.save() == saved { "identique" } else { "DIFFÉRENT" });
            let nearest = world.figures.iter().map(|f| format!("({:.0},{:.0})", f.x, f.y)).collect::<Vec<_>>().join(" ");
            println!("{}", city_report(&world));
            println!("après {warmup} pas : {} personnages {nearest}, scène en {start:?}", world.figures.len());
        }

        let mut ambience = match capture.is_none() || !script.is_empty() {
            true => Ambience::open(&data.join("audio")).inspect_err(|e| eprintln!("pas d'ambiance : {e}")).ok(),
            false => None,
        };
        let (mut ambience_at, mut ambience_seed) = (sdl::SDL_GetTicks(), sdl::SDL_GetTicks() as u32 | 1);
        // Narration du récit affiché ; `narrated` dit si celui-ci a déjà sa voix.
        let mut voice = match capture {
            None => Voice::open().inspect_err(|e| eprintln!("pas de narration : {e}")).ok(),
            Some(_) => None,
        };
        let mut narrated = false;
        let mut told = (Step::Parent(0), Moment::Opening);
        while running {
            if let Some(music) = music.as_mut() {
                music.update();
            }
            if let Some(voice) = voice.as_mut() {
                match (intro.is_some(), narrated) {
                    (true, false) => {
                        if let Some(file) = narration(data, map_path, settings.as_ref(), told.0, told.1) {
                            voice.play(&file);
                        }
                        narrated = true;
                    }
                    (false, true) => {
                        voice.stop();
                        narrated = false;
                    }
                    _ => {}
                }
                voice.update();
            }
            let (mut w, mut h) = (0, 0);
            sdl::SDL_GetRenderOutputSize(renderer, &mut w, &mut h);
            let (w, h) = (w as f32, h as f32);
            if ui == 0.0 {
                ui = (h / 1080.0).round().max(1.0);
                view.zoom = ui;
            }

            // La simulation avance à pas fixes, quel que soit le rythme d'affichage.
            let now = sdl::SDL_GetTicks();
            let tick_ms = SPEEDS[speed].1;
            while now - clock >= tick_ms {
                clock += tick_ms;
                if !paused && intro.is_none() {
                    world.tick();
                    advance_demo(&mut world, &mut demo_plan);
                }
            }
            if now - clock > 20 * tick_ms {
                clock = now;
            }

            // Essai scripté : une étape par image, injectée comme un vrai événement.
            if let Some(step) = script.pop_front() {
                let (verb, argument) = step.split_once(':').unwrap_or((step, ""));
                let numbers: Vec<f32> = argument.split(',').filter_map(|n| n.parse().ok()).collect();
                let mut injected = match (verb, numbers.as_slice()) {
                    ("m", &[x, y]) => {
                        let moved = sdl::Event::mouse_motion(script_pointer, (x, y), script_buttons);
                        script_pointer = (x, y);
                        Some(moved)
                    }
                    ("d", _) => {
                        script_buttons |= sdl::BUTTON_LEFT_MASK;
                        Some(sdl::Event::mouse_button(true, sdl::BUTTON_LEFT, script_pointer))
                    }
                    ("u", _) => {
                        script_buttons &= !sdl::BUTTON_LEFT_MASK;
                        Some(sdl::Event::mouse_button(false, sdl::BUTTON_LEFT, script_pointer))
                    }
                    ("w", &[steps]) => Some(sdl::Event::mouse_wheel(steps, script_pointer)),
                    // Clic droit : enfoncé, relâché.
                    ("rd", _) => Some(sdl::Event::mouse_button(true, sdl::BUTTON_RIGHT, script_pointer)),
                    ("ru", _) => Some(sdl::Event::mouse_button(false, sdl::BUTTON_RIGHT, script_pointer)),
                    // Vue centrée sur une case, pointeur sur le centre d'une case.
                    ("vue", &[x, y]) => {
                        (view.cx, view.cy) = tile_origin(x as i32, y as i32);
                        None
                    }
                    ("mc", &[x, y]) => {
                        let (wx, wy) = tile_origin(x as i32, y as i32);
                        let wy = wy - LEVEL_HEIGHT * if in_grid((x as i32, y as i32)) { world.map.height(x as usize, y as usize) as f32 } else { 0.0 };
                        let at = view.to_screen(wx + HALF_WIDTH, wy + HALF_HEIGHT, w / ui, h / ui);
                        let at = (at.0 * ui, at.1 * ui);
                        let moved = sdl::Event::mouse_motion(script_pointer, at, script_buttons);
                        script_pointer = at;
                        Some(moved)
                    }
                    // Chiffre : la touche à cet emplacement du clavier, quelle que soit la disposition.
                    ("k", &[digit]) if (1.0..=9.0).contains(&digit) => Some(sdl::Event::key_down(sdl::SCANCODE_1 + digit as u32 - 1, 0)),
                    ("k", _) => match argument {
                        "x" => Some(sdl::Event::key_down(0, sdl::KEY_X)),
                        "esc" => Some(sdl::Event::key_down(0, sdl::KEY_ESCAPE)),
                        "espace" => Some(sdl::Event::key_down(0, sdl::KEY_SPACE)),
                        "f5" => Some(sdl::Event::key_down(0, sdl::KEY_F5)),
                        "f9" => Some(sdl::Event::key_down(0, sdl::KEY_F9)),
                        "h" => Some(sdl::Event::key_down(0, sdl::KEY_H)),
                        "o" => Some(sdl::Event::key_down(0, sdl::KEY_O)),
                        "r" => Some(sdl::Event::key_down(0, sdl::KEY_R)),
                        "a" => Some(sdl::Event::key_down(0, sdl::KEY_A)),
                        "tab" => Some(sdl::Event::key_down(0, sdl::KEY_TAB)),
                        "+" => Some(sdl::Event::key_down(0, sdl::KEY_PLUS)),
                        "-" => Some(sdl::Event::key_down(0, sdl::KEY_MINUS)),
                        "c" => Some(sdl::Event::key_down(0, sdl::KEY_C)),
                        "pivot" => Some(sdl::Event::key_down(0, sdl::KEY_END)),
                        "pivotg" => Some(sdl::Event::key_down(0, sdl::KEY_HOME)),
                        "v" => Some(sdl::Event::key_down(0, sdl::KEY_V)),
                        "j" => Some(sdl::Event::key_down(0, sdl::KEY_J)),
                        _ => None,
                    },
                    // Pause en temps réel, pour laisser vivre ce qui en dépend (son).
                    ("p", &[ms]) => {
                        sdl::SDL_Delay(ms as u32);
                        None
                    }
                    ("intro", _) => None,
                    ("fin", _) => {
                        force_end = true;
                        None
                    }
                    ("t", &[ticks]) => {
                        // Journal des sinistres et des événements survenus pendant ces pas.
                        let standing = world.buildings.iter().flatten().count();
                        let mut journal = Vec::new();
                        for _ in 0..ticks as u32 {
                            world.tick();
                            advance_demo(&mut world, &mut demo_plan);
                            let day = world.ticks / 50;
                            journal.extend(world.disasters.drain(..).map(|d| format!("jour {day} : {} {} en {},{}", format!("{:?}", d.cause), world.kinds[d.kind].name, d.x, d.y)));
                            journal.extend(world.incidents.drain(..).map(|i| format!("jour {day} : {i:?}")));
                            journal.extend(world.battles.drain(..).map(|b| format!("jour {day} : {b:?}")));
                            journal.extend(world.god_news.drain(..).map(|g| format!("jour {day} : dieu {g:?}")));
                        }
                        for line in &journal {
                            eprintln!("{line}");
                        }
                        eprintln!("{} bâtiments debout, {} au départ de ces {ticks} pas", world.buildings.iter().flatten().count(), standing);
                        None
                    }
                    // Route en équerre d'une case à l'autre : d'abord en x, puis en y.
                    ("r", &[x1, y1, x2, y2]) => {
                        let (x1, y1, x2, y2) = (x1 as i32, y1 as i32, x2 as i32, y2 as i32);
                        let across = (x1.min(x2)..=x1.max(x2)).map(|x| (x, y1));
                        let down = (y1.min(y2)..=y1.max(y2)).map(|y| (x2, y));
                        let refused: Vec<String> = across.chain(down).filter(|&(x, y)| !world.is_road(x, y) && !world.place_road(x, y, road_cost)).map(|(x, y)| format!("{x},{y}")).collect();
                        if !refused.is_empty() {
                            eprintln!("route refusée en {}", refused.join(" "));
                        }
                        None
                    }
                    // Mur en équerre, comme la route.
                    ("mur", &[x1, y1, x2, y2]) => {
                        let (x1, y1, x2, y2) = (x1 as i32, y1 as i32, x2 as i32, y2 as i32);
                        let across = (x1.min(x2)..=x1.max(x2)).map(|x| (x, y1));
                        let down = (y1.min(y2)..=y1.max(y2)).map(|y| (x2, y));
                        for (x, y) in across.chain(down) {
                            if !world.is_wall(x, y) && !world.place_wall(x, y, wall_cost) {
                                eprintln!("mur refusé en {x},{y}");
                            }
                        }
                        None
                    }
                    // Bâtiment par son nom (soulignés pour les espaces), coin nord en x,y.
                    ("b", _) => {
                        let mut parts = argument.splitn(3, ',');
                        let (x, y) = (parts.next().and_then(|n| n.parse().ok()), parts.next().and_then(|n| n.parse().ok()));
                        let name = parts.next().unwrap_or("").replace('_', " ").to_lowercase();
                        match (x, y, world.kinds.iter().position(|k| k.name.to_lowercase() == name)) {
                            (Some(x), Some(y), Some(kind)) if world.place_building(kind, x, y) => {}
                            (Some(x), Some(y), Some(kind)) => eprintln!("{name} refusé en {x},{y} : {}", world.placement_hint(kind, x, y).unwrap_or("terrain occupé ou interdit")),
                            _ => eprintln!("bâtiment inconnu : {step}"),
                        }
                        None
                    }
                    // Plan du terrain : . constructible, : pré, ~ eau, T arbres, ^ rochers,
                    // # bâtiment, = route, ? autre.
                    ("carte", &[x1, y1, x2, y2]) => {
                        println!("carte de {x1},{y1} à {x2},{y2}");
                        for y in y1 as i32..=y2 as i32 {
                            let row: String = (x1 as i32..=x2 as i32)
                                .map(|x| {
                                    if world.is_road(x, y) {
                                        return '=';
                                    }
                                    if !(0..GRID as i32).contains(&x) || !(0..GRID as i32).contains(&y) {
                                        return ' ';
                                    }
                                    let t = world.map.terrain(x as usize, y as usize);
                                    match () {
                                        _ if world.building_at(x, y).is_some() => '#',
                                        _ if t & zeus::world::TERRAIN_WATER != 0 => '~',
                                        _ if t & zeus::world::TERRAIN_TREE != 0 => 'T',
                                        _ if t & (zeus::world::TERRAIN_ROCK | zeus::world::TERRAIN_QUARRY | zeus::world::TERRAIN_ORE) != 0 => '^',
                                        _ if world.can_build(x, y, 1) && t & zeus::world::TERRAIN_MEADOW != 0 => ':',
                                        _ if world.can_build(x, y, 1) => '.',
                                        _ => '?',
                                    }
                                })
                                .collect();
                            println!("{y:4} {row}");
                        }
                        None
                    }
                    // Chaque maison : niveau, habitants, attrait, réserves et services.
                    ("maisons", _) => {
                        for (level, model) in model.houses.iter().enumerate() {
                            println!(
                                "niveau {level} {} : attrait {}..{}, culture {}, eau {}, vivres {}, laine {}, huile {}, vin {}, capacité {}",
                                model.name, model.devolve_desirability, model.evolve_desirability, model.culture, model.water, model.food_types, model.fleece, model.olive_oil, model.wine, model.capacity
                            );
                        }
                        for building in world.buildings.iter().flatten() {
                            let Some(house) = &building.house else { continue };
                            let served: Vec<String> = SERVICES.iter().filter(|&&s| house.has(s, world.ticks)).map(|s| format!("{s:?}")).collect();
                            println!(
                                "  maison {},{} : niveau {}, {} habitants, attrait {}, réserves {:?}, {}",
                                building.x,
                                building.y,
                                house.level,
                                house.population,
                                world.appeal(building.x as i32, building.y as i32),
                                &house.goods[..4],
                                served.join(" ")
                            );
                        }
                        None
                    }
                    ("types", _) => {
                        for (i, kind) in world.kinds.iter().enumerate() {
                            let locked = if world.locked.get(i) == Some(&true) { ", interdit" } else { "" };
                            println!("{} ({}×{}, {} dr., {} employés{locked})", kind.name, kind.size, kind.depth, kind.cost, kind.employees);
                        }
                        None
                    }
                    ("bilan", _) => {
                        println!("{}", city_report(&world));
                        for goal in &goals {
                            let label = goal.describe(&text, &level_names, matches!(journey.step(), Step::Colony(_)));
                            println!("  objectif {label} : {:?}", goal.status(&world));
                        }
                        None
                    }
                    _ => {
                        eprintln!("étape de script inconnue : {step}");
                        None
                    }
                };
                if let Some(injected) = injected.as_mut() {
                    sdl::SDL_PushEvent(injected);
                }
                if script.is_empty() {
                    let built: Vec<String> = world.buildings.iter().flatten().map(|b| format!("{}@{},{}", world.kinds[b.kind].name, b.x, b.y)).collect();
                    let saved = world.save();
                    println!(
                        "script terminé : population {}, drachmes {}, {} cases en feu, {} en ruine, bâtiments [{}]",
                        world.population(),
                        world.money,
                        saved.matches("\nfeu ").count(),
                        saved.matches("\nruine ").count(),
                        built.join(", ")
                    );
                }
            }

            // Sinistres survenus pendant la simulation : on prévient le joueur.
            // Lettre d'événement : titre et texte du jeu, datés.
            let date = {
                let (day, month, year) = world.date();
                let year = if start_year == 0 { year as i64 + 1 } else { start_year as i64 + year as i64 };
                format!("{} {} {}{}", day + 1, months[month as usize], year.abs(), if year < 0 { " av. J.-C." } else { "" })
            };
            let mut post = |title_key: &str, body_key: &str, fields: &[(&str, String)]| {
                let fields: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (*k, v.as_str())).collect();
                if let Some((title, body)) = letters.letter(title_key, body_key, &fields) {
                    letter_queue.push_back((title, vec![date.clone(), body]));
                }
            };
            // Un sanctuaire vient d'être achevé : son dieu apporte sa ressource.
            let built = world.sanctuaries();
            if built.len() != sanctuaries_built {
                sanctuaries_built = built.len();
                if let Some(episode) = episode(journey.step()).filter(|_| !demo) {
                    lock_resources(&mut world, episode);
                }
                if let Some(&god) = built.last() {
                    let gift = SANCTUARY_RESOURCES.iter().find(|&&(g, _)| g == god).map(|&(_, r)| text.get(23, r as usize).to_lowercase());
                    let name = text.get(sanctuary::TEXT_GODS, god as usize);
                    if let Some(key) = LETTER_GODS.get(god as usize) {
                        post(&format!("{key}_sanctuary_complete_title"), &format!("{key}_sanctuary_complete_initial_announcement"), &[("player_name", "Chef".to_string()), ("god", name.clone())]);
                    }
                    notice = match gift {
                        Some(gift) => (format!("Sanctuaire de {name} achevé : {name} vous offre {gift}"), now),
                        None => (format!("Sanctuaire de {name} achevé"), now),
                    };
                }
            }
            // Héros : arrivées, départs en quête, combats.
            if let Some(news) = world.hero_news.pop() {
                if let zeus::world::HeroNews::Arrived(hero) = news {
                    if let Some(key) = LETTER_HEROES.get(hero as usize) {
                        post(&format!("{key}_arrives_title"), &format!("{key}_arrives_initial_announcement"), &[("player_name", "Chef".to_string())]);
                    }
                }
                let events = episode(journey.step()).map_or(&[][..], |e| &e.events[..]);
                notice = (hero_news(&text, news, events), now);
                world.hero_news.clear();
            }
            // Changements dans le monde.
            if let Some((city, change, resource)) = world.trade_news.pop() {
                let (name, good) = (city_name(&text, &world, city), request_good(&text, resource));
                let line = match change {
                    0 => format!("{name} demande davantage de {good}"),
                    1 => format!("{name} demande moins de {good}"),
                    2 => format!("{name} vend davantage de {good}"),
                    3 => format!("{name} vend moins de {good}"),
                    4 => format!("Le prix de {good} monte : {} dr.", world.price(resource as usize)),
                    _ => format!("Le prix de {good} baisse : {} dr.", world.price(resource as usize)),
                };
                notice = (line, now);
                world.trade_news.clear();
            }
            if !world.gifts.is_empty() {
                let gifts = std::mem::take(&mut world.gifts);
                for &(city, resource, amount) in &gifts {
                    let mut fields = city_fields(&text, &world, &leaders, city);
                    let good = request_good(&text, resource);
                    // Sans place pour le recevoir, le jeu en parle sans quantité.
                    if amount > 0 {
                        fields.push(("amount", amount.to_string()));
                    }
                    fields.extend([("item", good.clone()), ("itemshort", good)]);
                    post("gift_title", if amount > 0 { "gift_granted" } else { "gift_insufficient_space" }, &fields);
                }
                let lines: Vec<String> = gifts.iter().filter(|g| g.2 > 0).map(|&(city, resource, amount)| format!("{amount} {} de {}", request_good(&text, resource), city_name(&text, &world, city))).collect();
                if !lines.is_empty() {
                    notice = (format!("Cadeaux reçus : {}", lines.join(", ")), now);
                }
            }
            // Début d'une invasion : l'armée ennemie vient d'entrer sur la carte.
            if world.raid.is_some() != raid_seen {
                raid_seen = world.raid.is_some();
                if let Some(raid) = world.raid {
                    let defenders = world.figures.iter().filter(|f| f.soldier() == Some(false)).count();
                    let name = city_name(&text, &world, raid.city);
                    post("invasion_title", "invasion_city_attacked_alert", &city_fields(&text, &world, &leaders, raid.city));
                    notice = (format!("Invasion ! L'armée de {name} ({} soldats) marche sur la cité ; {defenders} hoplites la défendent", raid.soldiers), now);
                }
            }
            if let Some(battle) = world.battles.pop() {
                let name = city_name(&text, &world, battle.city);
                notice = if battle.repelled {
                    (format!("L'armée de {name} ({} soldats) est repoussée", battle.soldiers), now)
                } else {
                    (format!("L'armée de {name} pille la cité : {} drachmes et {} bâtiments perdus", battle.drachmas, battle.buildings), now)
                };
                world.battles.clear();
            }
            if let Some((god, resource, amount)) = world.blessings.pop() {
                let name = text.get(sanctuary::TEXT_GODS, god as usize);
                notice = (format!("{name} vous rend visite et vous offre {amount} {}", request_good(&text, resource)), now);
                world.blessings.clear();
            }
            if !world.tributes.is_empty() {
                let tributes = std::mem::take(&mut world.tributes);
                let lines: Vec<String> = tributes.into_iter().map(|(city, amount)| format!("{} {amount} dr.", city_name(&text, &world, city))).collect();
                notice = (format!("Tributs reçus : {}", lines.join(", ")), now);
            }
            if let Some((god, arrived)) = world.god_news.pop() {
                let name = text.get(sanctuary::TEXT_GODS, god as usize);
                if let (true, Some(key)) = (arrived, LETTER_GODS.get(god as usize)) {
                    post(&format!("{key}_invades_title"), &format!("{key}_invades_city_attacked_alert"), &[("player_name", "Chef".to_string()), ("god", name.clone())]);
                }
                let defenders: Vec<String> = world.sanctuaries().iter().map(|&g| text.get(sanctuary::TEXT_GODS, g as usize)).collect();
                notice = match (arrived, defenders.is_empty()) {
                    (true, true) => (format!("{name} s'abat sur la cité !"), now),
                    (true, false) => (format!("{name} s'abat sur la cité, mais {} le chassera bientôt", defenders.join(" et ")), now),
                    (false, _) => (format!("{name} quitte la cité"), now),
                };
                world.god_news.clear();
            }
            if let Some(news) = world.request_news.pop() {
                let (zeus::world::RequestNews::Arrived(r) | zeus::world::RequestNews::Expired(r)) = news;
                let (name, good) = (city_name(&text, &world, r.city), request_good(&text, r.resource));
                if let zeus::world::RequestNews::Arrived(_) = news {
                    // Ton de la demande : celle d'une rivale est un ordre, celle d'une vassale
                    // une supplique, les autres une demande ordinaire.
                    let tone = match world.cities.get(r.city).map(|c| c.status) {
                        Some(zeus::map::RIVAL) => "D",
                        Some(zeus::map::VASSAL) => "P",
                        _ => "S",
                    };
                    let mut fields = city_fields(&text, &world, &leaders, r.city);
                    let months_left = r.until.saturating_sub(world.ticks) / zeus::world::TICKS_PER_MONTH;
                    fields.extend([("amount", r.amount.to_string()), ("item", good.clone()), ("itemshort", good.clone()), ("time_allotted", months_left.to_string())]);
                    post(&format!("general_request_title_{tone}"), &format!("general_request_initial_announcement_{tone}"), &fields);
                }
                notice = match news {
                    zeus::world::RequestNews::Arrived(_) => (format!("{name} demande {} {good} (touche C)", r.amount), now),
                    zeus::world::RequestNews::Expired(_) => (format!("La demande de {name} n'a pas été satisfaite"), now),
                };
                world.request_news.clear();
            }
            if let Some((change, destroyed)) = world.disaster_news.pop() {
                let what = match change {
                    zeus::world::EARTHQUAKE => "Tremblement de terre ! La terre s'ouvre",
                    zeus::world::LAVA => "Éruption ! La lave recouvre le pays",
                    zeus::world::TIDAL_WAVE => "Raz-de-marée ! Les eaux envahissent la côte",
                    zeus::world::LANDSLIDE => "Glissement de terrain ! Des éboulis dévalent la pente",
                    _ => "Inondation ! La mer engloutit la côte pour toujours",
                };
                let toll = match destroyed {
                    0 => String::new(),
                    1 => " : un bâtiment détruit".to_string(),
                    n => format!(" : {n} bâtiments détruits"),
                };
                notice = (format!("{what}{toll}"), now);
                world.disaster_news.clear();
            }
            if let Some(change) = world.wage_news.pop() {
                let tone = if change > 0 { "I" } else { "D" };
                post(&format!("wage_change_title_{tone}"), &format!("wage_change_initial_announcement_{tone}"), &[("player_name", "Chef".to_string()), ("amount", change.abs().to_string())]);
                let (verb, normal) = (if change > 0 { "montent" } else { "baissent" }, 30 + world.wage_shift);
                notice = (format!("Les salaires {verb} de {} dr. : {normal} dr. par an pour dix ouvriers", change.abs()), now);
                world.wage_news.clear();
            }
            if let Some((city, change)) = world.world_news.pop() {
                let name = city_name(&text, &world, city);
                let key = match change {
                    zeus::world::TRADE_SHUTS => Some("trade_shuts_down"),
                    zeus::world::TRADE_OPENS => Some("trade_opens_up"),
                    zeus::world::ARMY_AWAY => Some("rival_away"),
                    _ => None,
                };
                if let Some(key) = key {
                    post(&format!("{key}_title"), &format!("{key}_initial_announcement"), &city_fields(&text, &world, &leaders, city));
                }
                let line = match change {
                    zeus::world::BECOMES_ALLY => format!("{name} redevient votre alliée"),
                    zeus::world::BECOMES_RIVAL => format!("{name} devient votre rivale"),
                    zeus::world::BECOMES_VASSAL => format!("{name} devient votre vassale"),
                    zeus::world::VASSAL_REBELS => format!("{name} se rebelle"),
                    zeus::world::MILITARY_BUILDUP => format!("{name} renforce son armée"),
                    zeus::world::MILITARY_DECLINE => format!("L'armée de {name} s'affaiblit"),
                    zeus::world::TRADE_SHUTS => format!("{name} cesse de commercer avec vous"),
                    zeus::world::TRADE_OPENS => format!("{name} ouvre son commerce avec vous"),
                    zeus::world::ARMY_AWAY => format!("L'armée de {name} part en campagne : la cité est vulnérable"),
                    zeus::world::BECOMES_INACTIVE | zeus::world::DISAPPEARS => format!("{name} se retire du monde"),
                    _ => format!("{name} reparaît dans le monde"),
                };
                notice = (line, now);
                world.world_news.clear();
            }
            // Campagnes lointaines de l'armée : les lettres du jeu.
            for news in std::mem::take(&mut world.campaign_news) {
                use zeus::world::CampaignNews;
                let city = match news {
                    CampaignNews::Conquered(c) | CampaignNews::ConquestFailed(c) | CampaignNews::WipedOut(c) => c,
                    CampaignNews::Returned { city, .. } | CampaignNews::Plunder { city, .. } => city,
                };
                let mut fields = city_fields(&text, &world, &leaders, city);
                fields.push(("nationality", "ennemie".to_string()));
                let name = city_name(&text, &world, city);
                let (title, body, line) = match news {
                    CampaignNews::Conquered(_) => ("city_conquered_by_player_title", "city_conquered_by_player_initial_announcement", format!("{name} est conquise")),
                    CampaignNews::ConquestFailed(_) => ("players_conquest_fails_title", "players_conquest_fails_initial_announcement", format!("Notre armée est vaincue à {name}")),
                    CampaignNews::WipedOut(_) => ("raiding_party_wiped_out_title", "raiding_party_wiped_out_initial_announcement", format!("Nos pillards sont anéantis à {name}")),
                    CampaignNews::Returned { won: true, .. } => ("legion_returns_player_action_good_title", "legion_returns_player_action_good_initial_announcement", "L'armée rentre victorieuse".to_string()),
                    CampaignNews::Returned { won: false, .. } => ("legion_returns_player_action_bad_title", "legion_returns_player_action_bad_initial_announcement", "L'armée rentre vaincue".to_string()),
                    CampaignNews::Plunder { resource, amount, .. } => {
                        fields.push(("amount", amount.to_string()));
                        fields.push(("item", text.get(23, resource as usize).to_lowercase()));
                        let body = if resource == zeus::world::DRACHMAS { "raiding_party_returns_cash_granted" } else { "raiding_party_returns_granted" };
                        ("raiding_party_returns_title", body, format!("Butin de {name} : {amount}"))
                    }
                };
                post(title, body, &fields);
                notice = (line, now);
            }
            // Jeux panhelléniques : les lettres du jeu, et un mot dans la zone des messages.
            for news in world.games_news.drain(..) {
                let games = |kind: u8| ["olympian", "isthmian", "nemean", "pythian"][kind as usize & 3];
                let chief = [("player_name", "Chef".to_string())];
                let (title, body, line) = match news {
                    zeus::world::GamesNews::Begin { home: true, .. } => ("home_games_begin_title".to_string(), "home_games_begin_initial_announcement".to_string(), "Les jeux de la cité commencent"),
                    zeus::world::GamesNews::Begin { kind, .. } => (format!("{}_games_begin_title", games(kind)), format!("{}_games_begin_initial_announcement_go", games(kind)), "Nos concurrents partent aux jeux"),
                    zeus::world::GamesNews::CantParticipate(kind) => (format!("{}_games_cant_participate_title", games(kind)), format!("{}_games_cant_participate_initial_announcement", games(kind)), "Personne à envoyer aux jeux"),
                    zeus::world::GamesNews::HomeCancelled => ("home_games_cancelled_title".to_string(), "home_games_cancelled_initial_announcement".to_string(), "Jeux de la cité annulés : il faut un stade"),
                    zeus::world::GamesNews::Concluded { kind, home, place } => {
                        let outcome = ["lost", "second", "won"][place.min(2) as usize];
                        let name = if home { "home" } else { games(kind) };
                        (format!("{name}_games_conclude_title"), format!("{name}_games_conclude_initial_announcement_{outcome}"), ["Jeux perdus", "Deuxièmes aux jeux", "Jeux gagnés !"][place.min(2) as usize])
                    }
                };
                post(&title, &body, &chief);
                notice = (line.to_string(), now);
            }
            if let Some(incident) = world.incidents.pop() {
                if let zeus::world::Incident::Plague { .. } = incident {
                    post("plague_title", "plague_initial_announcement", &[("player_name", "Chef".to_string())]);
                }
                notice = match incident {
                    zeus::world::Incident::Pillage { .. } => ("Des voleurs ont pillé une maison luxueuse : postez des gardes".to_string(), now),
                    zeus::world::Incident::Theft { amount, .. } => (format!("Un voleur a dérobé {amount} dr. au trésor : postez des gardes"), now),
                    zeus::world::Incident::Plague { dead, .. } => (format!("Une épidémie a emporté {dead} habitants : bâtissez des infirmeries"), now),
                };
                world.incidents.clear();
            }
            if let Some(disaster) = world.disasters.pop() {
                let name = &world.kinds[disaster.kind].name;
                let line = match disaster.cause {
                    Cause::Fire => format!("Incendie : {name}"),
                    Cause::Collapse => format!("Effondrement : {name}"),
                    Cause::Raid => format!("Les envahisseurs incendient : {name}"),
                    Cause::Monster(monster) => format!("{} détruit : {name}", text.get(heroes::TEXT_MONSTERS, monster as usize)),
                    Cause::God(god) => format!("{} détruit : {name}", text.get(sanctuary::TEXT_GODS, god as usize)),
                };
                notice = (line, now);
                world.disasters.clear();
            }

            // Une lettre en attente s'ouvre quand rien d'autre n'est affiché.
            if intro.is_none() && !won {
                intro = letter_queue.pop_front();
            }
            // Récit de victoire refermé : place à l'épisode suivant, dans la même cité ou
            // dans une autre, ou à la fin de l'aventure.
            let choices = if won && intro.is_none() && colony_pick.is_none() { journey.colony_choices(journey.position + 1) } else { None };
            if let Some(choices) = choices {
                // Plusieurs colonies possibles : le joueur choisit au clavier.
                let lines = choices
                    .iter()
                    .enumerate()
                    .map(|(i, &k)| {
                        let name = story(data, map_path, settings.as_ref(), Step::Colony(k), Moment::Opening).map_or_else(|| format!("Colonie {}", k + 1), |(title, _)| title);
                        format!("{} : {name}", i + 1)
                    })
                    .collect();
                intro = Some((format!("Choisissez votre colonie : touche 1 à {}", choices.len()), lines));
                colony_choice = Some(choices);
            } else if won && intro.is_none() {
                won = false;
                let next = journey.position + 1;
                if let Some(pick) = colony_pick.take() {
                    journey.choose_colony(next, pick);
                }
                if next < journey.order.len() {
                    // Ce qui a été mis de côté part avec le joueur : les cadeaux de
                    // l'épisode suivant l'apportent.
                    let carried = world.set_aside;
                    if journey.travel(&mut world, next) {
                        minimap_at = arrive(&world, &mut graphics, &mut view);
                        sanctuaries_built = world.sanctuaries().len();
                        (minimap_age, tool, hovered) = (MINIMAP_REFRESH, Tool::None, (-1, -1));
                    }
                    let step = journey.step();
                    eprintln!("épisode suivant : {step:?}");
                    world.start_episode();
                    world.carried = carried;
                    if let Some(episode) = episode(step).filter(|_| !demo) {
                        lock_resources(&mut world, episode);
                        begin_events(&mut world, episode);
                    }
                    goals = episode(step).map(|e| e.goals.clone()).unwrap_or_default();
                    intro = story(data, map_path, settings.as_ref(), step, Moment::Opening);
                    (told, narrated) = ((step, Moment::Opening), false);
                } else {
                    journey.position = journey.order.len();
                    goals.clear();
                    intro = story(data, map_path, settings.as_ref(), journey.step(), Moment::End);
                    (told, narrated) = ((journey.step(), Moment::End), false);
                    notice = ("Aventure terminée : la partie continue sans objectif".to_string(), now);
                    eprintln!("aventure terminée");
                }
                goals_met = vec![false; goals.len()];
            }

            // Objectifs atteints depuis la dernière image, puis fin de l'épisode quand
            // tous le sont. Un épisode sans objectif se joue sans fin.
            let statuses: Vec<Status> = goals.iter().map(|goal| goal.status(&world)).collect();
            for (i, status) in statuses.iter().enumerate() {
                let met = *status == Status::Met;
                if met && !goals_met[i] {
                    notice = (goals[i].achieved(&text), now);
                }
                goals_met[i] = met;
            }
            if !won && journey.position < journey.order.len().max(1) && (std::mem::take(&mut force_end) || !goals.is_empty() && goals_met.iter().all(|&met| met)) {
                won = true;
                let fallback = ("Épisode terminé".to_string(), vec!["Tous les objectifs de l'épisode sont atteints.".to_string()]);
                intro = Some(story(data, map_path, settings.as_ref(), journey.step(), Moment::Victory).unwrap_or(fallback));
                (told, narrated) = ((journey.step(), Moment::Victory), false);
            }

            // Les annonces de la cité entrent au journal ; pas les réponses aux commandes.
            if world.commemoratives.len() > commemoratives {
                let model = *world.commemoratives.last().unwrap() as usize;
                notice = (format!("Un monument commémoratif est disponible (onglet ailé) : {}", text.get(TEXT_COMMEMORATIVES, 1 + model)), now);
            }
            commemoratives = world.commemoratives.len();
            if notice != logged {
                logged = notice.clone();
                if !JOURNAL_SKIPS.iter().any(|skip| notice.0.starts_with(skip)) {
                    let (day, month, year) = world.date();
                    let year = if start_year == 0 { year as i64 + 1 } else { start_year as i64 + year as i64 };
                    let date = format!("{} {} {}{}", day + 1, months[month as usize], year.abs(), if year < 0 { " av. J.-C." } else { "" });
                    journal.push((date, notice.0.clone()));
                    unread += 1;
                    if journal.len() > JOURNAL_LINES {
                        journal.remove(0);
                    }
                }
            }

            while sdl::SDL_PollEvent(&mut event) {
                // Sur un écran mis à l'échelle, la souris est en points et le rendu en pixels.
                sdl::SDL_ConvertEventToRenderCoordinates(renderer, &mut event);
                // Le récit d'ouverture se referme au premier clic ou à la première touche.
                if let Some(choices) = &colony_choice {
                    let picked = (event.kind() == sdl::EVENT_KEY_DOWN && (sdl::SCANCODE_1..=sdl::SCANCODE_9).contains(&event.scancode()))
                        .then(|| choices.get((event.scancode() - sdl::SCANCODE_1) as usize).copied())
                        .flatten();
                    if picked.is_some() {
                        (colony_pick, colony_choice, intro) = (picked, None, None);
                    }
                    continue;
                }
                if intro.is_some() && matches!(event.kind(), sdl::EVENT_KEY_DOWN | sdl::EVENT_MOUSE_BUTTON_DOWN) {
                    intro = None;
                    continue;
                }
                match event.kind() {
                    sdl::EVENT_QUIT => running = false,
                    sdl::EVENT_KEY_DOWN => match event.key() {
                        sdl::KEY_ESCAPE if company.is_some() => company = None,
                        sdl::KEY_ESCAPE if tool != Tool::None => {
                            tool = Tool::None;
                            hovered = (-1, -1);
                        }
                        sdl::KEY_ESCAPE if info_pinned.is_some() => info_pinned = None,
                        sdl::KEY_ESCAPE if open_group.is_some() => open_group = None,
                        sdl::KEY_ESCAPE if open_category.is_some() => open_category = None,
                        sdl::KEY_ESCAPE | sdl::KEY_Q => running = false,
                        sdl::KEY_SPACE | sdl::KEY_P => paused = !paused,
                        sdl::KEY_V => {
                            let next = (OVERLAYS.iter().position(|&(o, _)| o == overlay).unwrap_or(0) + 1) % OVERLAYS.len();
                            overlay = OVERLAYS[next].0;
                            notice = (OVERLAYS[next].1.to_string(), now);
                        }
                        sdl::KEY_F5 => {
                            notice = match journey.save(&world, &save_file) {
                                Ok(()) => (format!("Partie enregistrée : {}", save_file.display()), now),
                                Err(e) => (format!("Enregistrement impossible : {e}"), now),
                            };
                        }
                        sdl::KEY_F9 => {
                            notice = match journey.load(&mut world, &save_file) {
                                Ok(()) => {
                                    minimap_at = arrive(&world, &mut graphics, &mut view);
                        sanctuaries_built = world.sanctuaries().len();
                                    (minimap_age, tool, hovered) = (MINIMAP_REFRESH, Tool::None, (-1, -1));
                                    let step = journey.step();
                                    if let Some(episode) = episode(step).filter(|_| !demo) {
                                        lock_resources(&mut world, episode);
                                        // Sauvegardes d'avant les héros : quêtes et monstres repartent.
                                        if world.quests.is_empty() && world.monsters.is_empty() {
                                            begin_events(&mut world, episode);
                                        }
                                    }
                                    // Les objectifs déjà atteints ne sont pas annoncés de nouveau.
                                    goals = episode(step).map(|e| e.goals.clone()).unwrap_or_default();
                                    goals_met = goals.iter().map(|goal| goal.status(&world) == Status::Met).collect();
                                    won = false;
                                    ("Partie chargée".to_string(), now)
                                }
                                Err(e) => (format!("Chargement impossible : {e}"), now),
                            };
                        }
                        sdl::KEY_X => {
                            tool = Tool::Demolish;
                            hovered = (-1, -1);
                        }
                        _ if (sdl::SCANCODE_1..=sdl::SCANCODE_9).contains(&event.scancode()) => {
                            let slot = (event.scancode() - sdl::SCANCODE_1) as usize;
                            match open_category {
                                // Catégorie ouverte : le chiffre y choisit un bâtiment.
                                Some(category) => {
                                    if let Some(&picked) = category_tools(&world, category).get(slot) {
                                        tool = picked;
                                        hovered = (-1, -1);
                                    }
                                }
                                None if slot < CATEGORY_NAMES.len() => (open_category, open_group) = (Some(slot), None),
                                None => {}
                            }
                        }
                        sdl::KEY_F => {
                            fullscreen = !fullscreen;
                            sdl::SDL_SetWindowFullscreen(window, fullscreen);
                        }
                        sdl::KEY_M => {
                            if let Some(music) = music.as_mut() {
                                let on = music.toggle();
                                notice = (format!("Musique : {}", text.get(18, on as usize)), sdl::SDL_GetTicks());
                            }
                        }
                        sdl::KEY_H => help = !help,
                        sdl::KEY_J => journal_panel = !journal_panel,
                        sdl::KEY_O => goals_panel = !goals_panel,
                        sdl::KEY_C => cities_panel = !cities_panel,
                        // Pivoter la vue vers la droite (Fin) ou vers la gauche (Origine),
                        // autour de la case du centre (`Zeus.exe` 0x44a620 / 0x44a6e0).
                        sdl::KEY_END | sdl::KEY_HOME => {
                            let centre = tile_at(view.cx, view.cy);
                            let step = if event.key() == sdl::KEY_END { 2 } else { 6 };
                            ORIENTATION.store((orientation() + step) & 6, std::sync::atomic::Ordering::Relaxed);
                            (view.cx, view.cy) = tile_origin(centre.0, centre.1);
                        }
                        sdl::KEY_A => advisor_panel = !advisor_panel,
                        sdl::KEY_TAB if advisor_panel => advisor_page += 1,
                        sdl::KEY_PLUS | sdl::KEY_EQUALS | sdl::KEY_KP_PLUS | sdl::KEY_MINUS | sdl::KEY_KP_MINUS => {
                            let up = matches!(event.key(), sdl::KEY_PLUS | sdl::KEY_EQUALS | sdl::KEY_KP_PLUS);
                            let level = if up { world.tax_level + 1 } else { world.tax_level.saturating_sub(1) };
                            world.set_tax_level(level);
                            notice = (format!("Imposition : {}", tax_name(&world)), now);
                        }
                        sdl::KEY_R => notice = (set_aside(&mut world, &goals, &text), now),
                        sdl::KEY_B => {
                            if let Some(ambience) = ambience.as_mut() {
                                ambience.enabled = !ambience.enabled;
                                notice = (format!("Bruitages : {}", text.get(18, ambience.enabled as usize)), sdl::SDL_GetTicks());
                            }
                        }
                        sdl::KEY_PAGEUP | sdl::KEY_PAGEDOWN => {
                            speed = if event.key() == sdl::KEY_PAGEUP { (speed + 1).min(SPEEDS.len() - 1) } else { speed.saturating_sub(1) };
                            notice = (format!("Vitesse : {} %", SPEEDS[speed].0), now);
                        }
                        sdl::KEY_LEFT => view.cx -= KEY_STEP / view.zoom,
                        sdl::KEY_RIGHT => view.cx += KEY_STEP / view.zoom,
                        sdl::KEY_UP => view.cy -= KEY_STEP / view.zoom,
                        sdl::KEY_DOWN => view.cy += KEY_STEP / view.zoom,
                        _ => {}
                    },
                    sdl::EVENT_MOUSE_MOTION => {
                        pointer = event.position();
                        let (dx, dy, buttons) = event.motion();
                        if buttons & (sdl::BUTTON_RIGHT_MASK | sdl::BUTTON_MIDDLE_MASK) != 0 {
                            view.cx -= dx / view.zoom;
                            view.cy -= dy / view.zoom;
                            if let Some(moved) = right_press.as_mut() {
                                *moved += dx.abs() + dy.abs();
                            }
                        }
                        // Glisser avec le bouton gauche sur la mini-carte : la parcourir.
                        if let (true, Some(target)) = (buttons & sdl::BUTTON_LEFT_MASK != 0, minimap_at.target(pointer.0 / ui, pointer.1 / ui)) {
                            (view.cx, view.cy) = target;
                        }
                    }
                    // Relâcher pose le tracé montré pendant le geste, comme dans le jeu : la
                    // route ou le mur le long du chemin, ou la démolition du rectangle.
                    sdl::EVENT_MOUSE_BUTTON_UP if event.button() == sdl::BUTTON_LEFT => {
                        if let Some(from) = drag_from.take() {
                            let to = { let (wx, wy) = view.to_world(pointer.0, pointer.1, w, h); pick(&world, wx, wy) };
                            let tiles = drag_tiles(&world, tool, from, to);
                            if tiles.is_empty() && matches!(tool, Tool::Road | Tool::Wall) {
                                notice = ("Aucun chemin praticable jusque-là".to_string(), now);
                            }
                            for (tx, ty) in tiles {
                                match tool {
                                    Tool::Road => drop(world.place_road(tx, ty, road_cost)),
                                    Tool::Wall => drop(world.place_wall(tx, ty, wall_cost)),
                                    Tool::Demolish => drop(world.demolish(tx, ty)),
                                    _ => {}
                                }
                            }
                            hovered = (-1, -1);
                        }
                    }
                    sdl::EVENT_MOUSE_BUTTON_DOWN if event.button() == sdl::BUTTON_RIGHT => {
                        right_press = Some(0.0);
                        // Le clic droit lâche aussi l'outil, comme dans le jeu, et annule le
                        // geste en cours.
                        drag_from = None;
                        if tool != Tool::None {
                            (tool, hovered) = (Tool::None, (-1, -1));
                            right_press = None;
                        }
                    }
                    sdl::EVENT_MOUSE_BUTTON_UP if event.button() == sdl::BUTTON_RIGHT => {
                        // Un clic droit sans glisser ouvre la fiche du bâtiment visé.
                        if right_press.take().is_some_and(|moved| moved < 6.0) {
                            let (wx, wy) = view.to_world(pointer.0, pointer.1, w, h);
                            let at = pick(&world, wx, wy);
                            info_pinned = world.building_at(at.0, at.1).map(|_| at);
                        }
                    }
                    sdl::EVENT_MOUSE_BUTTON_DOWN if event.button() == sdl::BUTTON_LEFT && info_pinned.is_some() => info_pinned = None,
                    sdl::EVENT_MOUSE_BUTTON_DOWN if event.button() == sdl::BUTTON_LEFT => {
                        let (mx, my) = event.position();
                        // Un clic sur la mini-carte y transporte la vue.
                        // L'interface se mesure dans ses propres unités.
                        let (ix, iy) = (mx / ui, my / ui);
                        if let Some(target) = minimap_at.target(ix, iy) {
                            (view.cx, view.cy) = target;
                            continue;
                        }
                        // Un clic sur un onglet ou sur une ligne du menu.
                        let hit = menu_rows.iter().rev().find(|((x, y, w, h), _)| ix >= *x && ix < x + w && iy >= *y && iy < y + h);
                        match hit.map(|&(_, action)| action) {
                            Some(MenuRow::Category(index)) => {
                                open_category = (open_category != Some(index)).then_some(index);
                                open_group = None;
                                continue;
                            }
                            Some(MenuRow::Pick(picked)) => {
                                tool = picked;
                                open_group = None;
                                hovered = (-1, -1);
                                continue;
                            }
                            Some(MenuRow::Group(button)) => {
                                // Un seul bâtiment : il est choisi ; plusieurs : leur liste s'ouvre.
                                let group = open_category.and_then(|c| category_groups(&world, c).into_iter().find(|(b, _)| *b == button));
                                match group.map(|(_, tools)| tools) {
                                    Some(tools) if tools.len() == 1 => {
                                        (tool, open_group, hovered) = (tools[0].1, None, (-1, -1));
                                    }
                                    Some(_) => open_group = if open_group == Some(button) { None } else { Some(button) },
                                    None => {}
                                }
                                continue;
                            }
                            Some(MenuRow::SidebarMap(wanted)) => {
                                sidebar_map = wanted;
                                continue;
                            }
                            Some(MenuRow::TopMenu(index)) => {
                                top_menu = if top_menu == Some(index) { None } else { Some(index) };
                                continue;
                            }
                            Some(MenuRow::Key(key)) => {
                                top_menu = None;
                                let mut pressed = sdl::Event::key_down(0, key);
                                sdl::SDL_PushEvent(&mut pressed);
                                continue;
                            }
                            Some(MenuRow::Fulfil(index)) => {
                                let request = world.requests.get(index).copied();
                                notice = match request {
                                    Some(r) if world.fulfil(index) => (format!("Demande de {} satisfaite", city_name(&text, &world, r.city)), now),
                                    Some(r) => (format!("Pas assez de {} : {} sur {}", request_good(&text, r.resource), world.available_for(r.resource), r.amount), now),
                                    None => continue,
                                };
                                continue;
                            }
                            Some(MenuRow::Overlay(wanted)) => {
                                overlay = if overlay == wanted { Overlay::None } else { wanted };
                                continue;
                            }
                            Some(MenuRow::Journal) => {
                                journal_panel = !journal_panel;
                                continue;
                            }
                            Some(MenuRow::Policy(resource)) => {
                                world.trade_policy[resource] = world.trade_policy[resource].next();
                                continue;
                            }
                            Some(MenuRow::Conquer(city, raid)) => {
                                if world.expedition.is_some() {
                                    notice = ("L'armée est déjà en campagne".to_string(), now);
                                } else if world.army() == 0 {
                                    notice = ("La cité n'a pas de soldats à envoyer".to_string(), now);
                                } else {
                                    company = Some(CompanyDraft { city, raid, soldiers: world.ranks(), triremes: world.triremes() });
                                }
                                continue;
                            }
                            Some(MenuRow::CompanyCount(slot, delta)) => {
                                if let Some(mut draft) = company {
                                    let max = if slot < 3 { world.ranks()[slot] } else { world.triremes() };
                                    let value = if slot < 3 { &mut draft.soldiers[slot] } else { &mut draft.triremes };
                                    *value = (*value + delta).clamp(0, max);
                                    company = Some(draft);
                                }
                                continue;
                            }
                            Some(MenuRow::CompanyAll) => {
                                if let Some(mut draft) = company {
                                    draft.soldiers = world.ranks();
                                    draft.triremes = world.triremes();
                                    company = Some(draft);
                                }
                                continue;
                            }
                            Some(MenuRow::CompanySend) => {
                                let Some(draft) = company.take() else { continue };
                                let name = city_name(&text, &world, draft.city);
                                notice = match world.conquer_with(draft.city, draft.raid, draft.soldiers, draft.triremes) {
                                    Ok(months) => (format!("La compagnie part {} {name} : {months} mois de marche", if draft.raid { "piller" } else { "attaquer" }), now),
                                    Err(Conquest::NoArmy) => ("Aucune troupe choisie".to_string(), now),
                                    Err(Conquest::TooMany) => (format!("Pas assez de soldats ou de {} pour cette compagnie", world.ship_plural()), now),
                                    Err(Conquest::Away) => ("L'armée est déjà en campagne".to_string(), now),
                                    Err(_) => (format!("{name} ne peut pas être attaquée"), now),
                                };
                                continue;
                            }
                            Some(MenuRow::CompanyCancel) => {
                                company = None;
                                continue;
                            }
                            Some(MenuRow::CompanyShield) => continue,
                            None if top_menu.is_some() => {
                                // Un clic hors du menu déroulé le referme.
                                top_menu = None;
                                continue;
                            }
                            None => {}
                        }
                        // La composition ouverte garde le clic pour elle.
                        if company.is_some() {
                            continue;
                        }
                        // Le bandeau et le panneau latéral ne laissent pas passer le clic.
                        if iy < TOP_BAR_HEIGHT || ix >= w / ui - SIDEBAR_WIDTH {
                            continue;
                        }
                        let (wx, wy) = view.to_world(mx, my, w, h);
                        let (x, y) = pick(&world, wx, wy);
                        match tool {
                            // Sans outil, un passant cliqué dit quelques mots.
                            Tool::None => {
                                // Le passant le plus proche du clic.
                                let near = world.figures.iter().filter(|f| (f.x - x as f32).abs() <= 1.0 && (f.y - y as f32).abs() <= 1.0);
                                if let (Some(prefix), Some(voice)) = (near.filter_map(|f| f.voice()).next(), voice.as_mut()) {
                                    // Répliques d'humeur égale (`_g_`), sinon n'importe lesquelles.
                                    let files: Vec<PathBuf> = fs::read_dir(data.join("audio/voice/walker")).into_iter().flatten().flatten().map(|e| e.path()).collect();
                                    let starting = |start: String| -> Vec<PathBuf> {
                                        files.iter().filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(&start))).cloned().collect()
                                    };
                                    let mut lines = starting(format!("{prefix}_g_"));
                                    if lines.is_empty() {
                                        lines = starting(format!("{prefix}_"));
                                    }
                                    if !lines.is_empty() {
                                        voice.play(&lines[sdl::SDL_GetTicks() as usize % lines.len()]);
                                    }
                                }
                            }
                            // Le geste commence ; il se pose au relâchement.
                            Tool::Road | Tool::Wall | Tool::Demolish => drag_from = Some((x, y)),
                            Tool::Build(kind) => {
                                let (x, y) = build_corner(&world, kind, (x, y));
                                if !world.place_building(kind, x, y) {
                                    if world.money < world.kinds[kind].cost {
                                        notice = (format!("Pas assez de drachmes pour {}", world.kind_name(kind)), now);
                                    } else if let Some(hint) = world.placement_hint(kind, x, y) {
                                        notice = (format!("{} : {hint}", world.kind_name(kind)), now);
                                    }
                                }
                            }
                        }
                        hovered = (-1, -1);
                    }
                    // Molette : zoom en gardant fixe le point sous le pointeur.
                    sdl::EVENT_MOUSE_WHEEL => {
                        let (steps, mx, my) = event.wheel();
                        let before = view.to_world(mx, my, w, h);
                        view.zoom = (view.zoom * 1.25f32.powf(steps)).clamp(ZOOM_MIN, ZOOM_MAX);
                        let after = view.to_world(mx, my, w, h);
                        view.cx += before.0 - after.0;
                        view.cy += before.1 - after.1;
                    }
                    _ => {}
                }
            }
            let limit = GRID as f32;
            view.cx = view.cx.clamp(-limit * HALF_WIDTH, limit * HALF_WIDTH);
            view.cy = view.cy.clamp(0.0, 2.0 * limit * HALF_HEIGHT);

            // Case survolée, affichée dans le titre avec son type de terrain.
            let (wx, wy) = view.to_world(pointer.0, pointer.1, w, h);
            let under = pick(&world, wx, wy);
            // Geste en cours : ce qu'il poserait si on relâchait maintenant.
            let drag_plan = drag_from.map(|from| drag_tiles(&world, tool, from, under)).unwrap_or_default();
            if under != hovered {
                hovered = under;
                let held = match tool {
                    Tool::None => String::new(),
                    Tool::Road => format!(" — {road_name} ({road_cost})"),
                    Tool::Wall => format!(" — {wall_name} ({wall_cost})"),
                    Tool::Demolish => " — Démolir".to_string(),
                    Tool::Build(kind) => format!(" — {} ({})", world.kind_name(kind), world.kinds[kind].cost),
                };
                let place = match in_grid(under) {
                    true => format!(
                        " — case {},{} terrain {:#x}",
                        under.0,
                        under.1,
                        world.map.terrain(under.0 as usize, under.1 as usize)
                    ),
                    false => String::new(),
                };
                let info = format!(" — {} {}{held}{place}", world.money, text.get(8, 0));
                sdl::SDL_SetWindowTitle(window, title(&info).as_ptr());
            }

            // Vue tournée : le terrain se repave pour l'orientation (la carte ne le donne
            // que pour l'orientation 0).
            if world.orientation != orientation() {
                world.orientation = orientation();
                world.terrain_view = (world.orientation != 0).then(|| zeus::tiling::tile(&world.map, &graphics.terrain.groups, &graphics.poseidon.groups, world.orientation));
                // La mini-carte tourne aussi.
                minimap_at = Minimap::new(&world);
                minimap_age = MINIMAP_REFRESH;
                // Le palais vu de côté : ses deux moitiés pour l'autre sens.
                world.turned_images = vec![(PALACE_BACK_IMAGE, PALACE_BACK_TURNED_IMAGE), (PALACE_FRONT_IMAGE, PALACE_FRONT_TURNED_IMAGE)];
            }
            // Cases visibles : on parcourt le rectangle de l'écran en diagonales.
            let (left, top) = view.to_world(0.0, 0.0, w, h);
            let (right, bottom) = view.to_world(w, h, w, h);
            // Rectangle du monde visible, pour le cadre de la mini-carte.
            let visible = (left, top, right, bottom);

            // Ambiance : de temps en temps, un bruitage lié à un bâtiment visible, ou
            // au terrain du centre de l'écran.
            if let (Some(ambience), true) = (ambience.as_mut(), now - ambience_at >= AMBIENCE_MS && !paused) {
                ambience_at = now;
                let mut random = || {
                    ambience_seed ^= ambience_seed << 13;
                    ambience_seed ^= ambience_seed >> 17;
                    ambience_seed ^= ambience_seed << 5;
                    ambience_seed
                };
                let visible: Vec<Function> = world
                    .buildings
                    .iter()
                    .flatten()
                    .filter(|b| {
                        let (bx, by) = tile_origin(b.x as i32, b.y as i32);
                        bx > left && bx < right && by > top && by < bottom
                    })
                    .map(|b| world.kinds[b.kind].function)
                    .collect();
                let category = if visible.is_empty() || random() % 4 == 0 {
                    let (cx, cy) = tile_at(view.cx, view.cy);
                    terrain_sound(if in_grid((cx, cy)) { world.map.terrain(cx as usize, cy as usize) } else { 0 })
                } else {
                    sound_category(visible[random() as usize % visible.len()])
                };
                ambience.play(category, random);
            }
            let first_sum = (top / HALF_HEIGHT).floor() as i32 - 2;
            let last_sum = ((bottom + TALLEST_IMAGE) / HALF_HEIGHT).ceil() as i32 + 1;
            let first_diff = (left / HALF_WIDTH).floor() as i32 - 8;
            let last_diff = (right / HALF_WIDTH).ceil() as i32 + 1;
            draw_list.clear();
            if matches!(tool, Tool::Road | Tool::Wall) {
                world.show_ghosts(&drag_plan, tool == Tool::Wall);
            }
            for sum in first_sum.max(0)..=last_sum.min(2 * GRID as i32 - 2) {
                for diff in first_diff..=last_diff {
                    if (sum + diff) % 2 != 0 {
                        continue;
                    }
                    let screen_tile = ((sum + diff) / 2, (sum - diff) / 2);
                    let tile = zeus::world::unrotate(orientation(), screen_tile.0, screen_tile.1);
                    if !in_grid(tile) {
                        continue;
                    }
                    if let Some(draw) = world.draw(tile.0 as usize, tile.1 as usize) {
                        // Un objet de n × n cases se dessine au rang de son coin le plus proche.
                        let n = graphics.size(draw.source, draw.image);
                        let height = graphics.catalog(draw.source).images.get(draw.image).map_or(0, |i| i.height) as f32;
                        // L'image part du bord gauche de sa case ; son bas est celui de l'emprise.
                        // Le terrain surélevé se dessine 30 pixels plus haut par niveau
                        // (`Zeus.exe` 0x44af13).
                        let (ox, oy) = tile_origin(tile.0, tile.1);
                        let oy = oy - LEVEL_HEIGHT * world.map.height(tile.0 as usize, tile.1 as usize) as f32;
                        let top = oy + (n as f32 + 1.0) * HALF_HEIGHT - height - draw.lift as f32;
                        if let Some((source, image)) = draw.ground {
                            let ground_height = graphics.catalog(source).images.get(image).map_or(0, |i| i.height) as f32;
                            draw_list.push((sum, screen_tile.0 * 3, ox, oy + 2.0 * HALF_HEIGHT - ground_height, source, image, None));
                        }
                        // Mer : la même tuile revient tous les quinze numéros, à un autre instant
                        // de la houle ; la carte donne à chaque case son instant de départ.
                        let sea = draw.source == Source::Terrain && (SEA_FIRST_IMAGE..SEA_FIRST_IMAGE + SEA_FRAMES * SEA_TILES).contains(&draw.image);
                        let image = match sea {
                            true => {
                                let (frame, tile) = ((draw.image - SEA_FIRST_IMAGE) / SEA_TILES, (draw.image - SEA_FIRST_IMAGE) % SEA_TILES);
                                SEA_FIRST_IMAGE + tile + SEA_TILES * ((frame + (world.ticks / SEA_FRAME_TICKS) as usize) % SEA_FRAMES)
                            }
                            false => draw.image,
                        };
                        let tint = world.building_at(tile.0, tile.1).and_then(|b| overlay_tint(&world, overlay, b));
                        draw_list.push((sum + n - 1, screen_tile.0 * 3 + 1, ox, top, draw.source, image, tint));
                        // Pièce qui couronne un étage de monument.
                        if let Some((source, image, lift)) = draw.above {
                            let above_height = graphics.catalog(source).images.get(image).map_or(0, |i| i.height) as f32;
                            draw_list.push((sum + n - 1, screen_tile.0 * 3 + 2, ox, oy + 2.0 * HALF_HEIGHT - above_height - lift as f32, source, image, tint));
                        }
                        // Incendie : une flamme animée, le pied au centre de la case, décalée
                        // d'une case à l'autre pour ne pas battre à l'unisson.
                        if draw.burning {
                            let phase = (tile.0 * 7 + tile.1 * 13) as usize;
                            let frame = FIRE_FIRST_IMAGE + ((world.ticks / FIRE_FRAME_TICKS) as usize + phase) % FIRE_FRAMES;
                            if let Some(flame) = graphics.destruction.images.get(frame) {
                                let (fx, fy) = (ox + HALF_WIDTH - flame.width as f32 / 2.0, oy + 1.5 * HALF_HEIGHT - flame.height as f32);
                                draw_list.push((sum, screen_tile.0 * 3 + 2, fx, fy, Source::Destruction, frame, None));
                            }
                        }
                        // Bâtiment animé : l'image d'animation se pose sur la base, à l'ancrage indiqué.
                        let base = &graphics.catalog(draw.source).images[draw.image];
                        if let Some(overlay) = draw.overlay {
                            let (ax, ay) = (ox + base.offset_x as f32, top + base.offset_y as f32);
                            draw_list.push((sum + n - 1, screen_tile.0 * 3 + 2, ax, ay, draw.source, overlay, tint));
                        } else if draw.source == Source::General && base.animation_frames > 0 {
                            let frame = (world.ticks / ANIMATION_TICKS) as usize % base.animation_frames;
                            let (ax, ay) = (ox + base.offset_x as f32, top + base.offset_y as f32);
                            draw_list.push((sum + n - 1, screen_tile.0 * 3 + 2, ax, ay, draw.source, draw.image + 1 + frame, tint));
                        }
                    }
                }
            }
            // Catalogue d'un monstre qui vient de paraître.
            for figure in &world.figures {
                let (loaded, files, id) = match figure.source {
                    Source::Monster(monster) => (&mut graphics.monsters, &MONSTER_FILES[..], monster),
                    Source::Hero(hero) => (&mut graphics.heroes, &HERO_FILES[..], hero),
                    Source::Deity(god) => (&mut graphics.deities, &GOD_FILES[..], god),
                    Source::Army(army) => (&mut graphics.armies, &ARMY_FILES[..], army),
                    _ => continue,
                };
                if !loaded.contains_key(&id) {
                    let file = files.get(id as usize).map_or(String::new(), |f| format!("data/{f}.sg3"));
                    loaded.insert(id, Catalog::load(&data.join(file)).unwrap_or_else(|_| Catalog::empty()));
                }
            }
            // Personnages : ancrés au centre de leur case, par-dessus le sol de même rang.
            for figure in &world.figures {
                let image = figure.image_facing(orientation());
                let Some(info) = graphics.catalog(figure.source).images.get(image) else { continue };
                let (fx, fy) = zeus::world::rotate_point(orientation(), figure.x, figure.y);
                let wx = (fx - fy) * HALF_WIDTH + HALF_WIDTH - 1.0 - info.offset_x as f32;
                let level = world.map.height((figure.x.round() as usize).min(GRID - 1), (figure.y.round() as usize).min(GRID - 1)) as f32;
                let wy = (fx + fy) * HALF_HEIGHT + HALF_HEIGHT - info.offset_y as f32 - LEVEL_HEIGHT * level;
                if wx < right && wx + info.width as f32 > left && wy < bottom && wy + info.height as f32 > top {
                    draw_list.push(((fx + fy).ceil() as i32, i32::MAX, wx, wy, figure.source, image, None));
                }
            }
            draw_list.sort_unstable_by_key(|&(depth, layer, ..)| (depth, layer));
            world.hide_ghosts();

            sdl::SDL_SetRenderDrawColor(renderer, 0, 0, 0, 255);
            sdl::SDL_RenderClear(renderer);
            // À un zoom non entier, étirer chaque tuile séparément laisse voir des joints
            // sombres. La scène est alors dessinée nette, à l'échelle entière supérieure,
            // dans une image intermédiaire qui est ensuite réduite d'un bloc.
            let crisp = view.zoom >= 1.0 && view.zoom.fract() == 0.0;
            let window_size = (w, h);
            let window_view = View { cx: view.cx, cy: view.cy, zoom: view.zoom };
            let scale = view.zoom.ceil().max(1.0);
            let mut staged = false;
            if !crisp {
                let (tw, th) = ((w * scale / view.zoom).ceil() as i32, (h * scale / view.zoom).ceil() as i32);
                if scene.is_none_or(|(_, sw, sh)| (sw, sh) != (tw, th)) {
                    if let Some((old, ..)) = scene.take() {
                        sdl::SDL_DestroyTexture(old);
                    }
                    let texture = sdl::SDL_CreateTexture(renderer, sdl::PIXELFORMAT_ABGR8888, sdl::TEXTUREACCESS_TARGET, tw, th);
                    // Trop grande pour la carte graphique : on dessinera directement, joints compris.
                    scene = (!texture.is_null()).then_some((texture, tw, th));
                }
                if let Some((texture, ..)) = scene {
                    staged = sdl::SDL_SetRenderTarget(renderer, texture);
                    sdl::SDL_RenderClear(renderer);
                }
            }
            let (view, w, h) = match (staged, scene) {
                (true, Some((_, tw, th))) => (View { cx: view.cx, cy: view.cy, zoom: scale }, tw as f32, th as f32),
                _ => (window_view, w, h),
            };
            let scale_mode = if crisp || staged { sdl::SCALEMODE_NEAREST } else { sdl::SCALEMODE_LINEAR };
            for &(_, _, wx, wy, source, image, tint) in &draw_list {
                let Some(sprite) = graphics.sprite(source, image) else { continue };
                let (sx, sy) = view.to_screen(wx, wy, w, h);
                let target = sdl::FRect {
                    x: sx.floor(),
                    y: sy.floor(),
                    w: (sprite.width * view.zoom).ceil(),
                    h: (sprite.height * view.zoom).ceil(),
                };
                sdl::SDL_SetTextureScaleMode(sprite.texture, scale_mode);
                if let Some((r, g, b)) = tint {
                    sdl::SDL_SetTextureColorMod(sprite.texture, r, g, b);
                }
                sdl::SDL_RenderTexture(renderer, sprite.texture, null(), &target);
                if tint.is_some() {
                    sdl::SDL_SetTextureColorMod(sprite.texture, 255, 255, 255);
                }
            }

            // Traits des archers ennemis : un court trait sombre qui file vers sa cible.
            sdl::SDL_SetRenderDrawColor(renderer, 60, 40, 20, 255);
            for ((fx, fy), (tx, ty), progress) in world.missiles() {
                let at = |t: f32| {
                    let (x, y) = zeus::world::rotate_point(orientation(), fx + (tx - fx) * t, fy + (ty - fy) * t);
                    // Centre de la case, à hauteur d'homme ; le trait monte puis redescend.
                    let arc = 30.0 * (t * (1.0 - t)) * 4.0;
                    view.to_screen((x - y) * HALF_WIDTH + HALF_WIDTH, (x + y) * HALF_HEIGHT + HALF_HEIGHT - 20.0 - arc, w, h)
                };
                let (a, b) = (at(progress), at((progress + 0.12).min(1.0)));
                sdl::SDL_RenderLine(renderer, a.0, a.1, b.0, b.1);
            }

            // Aperçu de ce que l'outil poserait : translucide, rougi si c'est impossible.
            if let (Tool::Build(kind), true) = (tool, in_grid(under)) {
                let corner = build_corner(&world, kind, under);
                let allowed = world.can_place(kind, corner.0, corner.1) && world.money >= world.kinds[kind].cost;
                // Le bâtiment se montre comme à l'orientation 0, à partir de la case survolée.
                let start = zeus::world::rotate(orientation(), under.0, under.1);
                for part in world.kinds[kind].parts.clone() {
                    let Some(sprite) = graphics.sprite(part.source, part.image) else { continue };
                    let carrier = zeus::world::unrotate(orientation(), start.0 + part.dx as i32, start.1 + part.dy as i32 + part.size as i32 - 1);
                    let (ox, oy) = tile_origin(carrier.0, carrier.1);
                    let oy = oy - if in_grid(carrier) { LEVEL_HEIGHT * world.map.height(carrier.0 as usize, carrier.1 as usize) as f32 } else { 0.0 };
                    let (sx, sy) = view.to_screen(ox, oy + (part.size as f32 + 1.0) * HALF_HEIGHT - sprite.height, w, h);
                    let target = sdl::FRect {
                        x: sx.floor(),
                        y: sy.floor(),
                        w: (sprite.width * view.zoom).ceil(),
                        h: (sprite.height * view.zoom).ceil(),
                    };
                    sdl::SDL_SetTextureAlphaMod(sprite.texture, 170);
                    sdl::SDL_SetTextureColorMod(sprite.texture, 255, if allowed { 255 } else { 90 }, if allowed { 255 } else { 90 });
                    sdl::SDL_RenderTexture(renderer, sprite.texture, null(), &target);
                    sdl::SDL_SetTextureAlphaMod(sprite.texture, 255);
                    sdl::SDL_SetTextureColorMod(sprite.texture, 255, 255, 255);
                }
            } else if tool == Tool::Demolish && !drag_plan.is_empty() {
                for &tile in &drag_plan {
                    outline_tile(renderer, &view, &world, tile, w, h, (255, 60, 40));
                }
            } else if in_grid(under) {
                outline_tile(renderer, &view, &world, under, w, h, (255, 255, 0));
            }

            if let (true, Some((texture, ..))) = (staged, scene) {
                sdl::SDL_SetRenderTarget(renderer, null_mut());
                sdl::SDL_SetTextureScaleMode(texture, sdl::SCALEMODE_LINEAR);
                let whole = sdl::FRect { x: 0.0, y: 0.0, w: window_size.0, h: window_size.1 };
                sdl::SDL_RenderTexture(renderer, texture, null(), &whole);
            }
            let (w, h) = window_size;

            // Interface : dessinée agrandie sur les écrans à haute définition, pour rester
            // lisible. Tout ce qui suit se mesure en unités d'interface.
            sdl::SDL_SetRenderScale(renderer, ui, ui);
            let (w, h) = (w / ui, h / ui);
            // Coût du tracé, près du pointeur.
            if matches!(tool, Tool::Road | Tool::Wall) && drag_from.is_some() {
                let unit = if tool == Tool::Road { road_cost } else { wall_cost };
                let fresh = drag_plan.iter().filter(|&&(x, y)| !world.is_road(x, y) && !world.is_wall(x, y)).count() as i32;
                let label = if drag_plan.is_empty() { "—".to_string() } else { format!("{} {}", fresh * unit, text.get(8, 0)) };
                let (lx, ly) = (pointer.0 / ui + 16.0, pointer.1 / ui - 8.0);
                let width = graphics.text_width(FONT_SMALL, &label) + 8.0;
                sdl::SDL_SetRenderDrawColor(renderer, 20, 20, 20, 200);
                sdl::SDL_SetRenderDrawBlendMode(renderer, sdl::BLENDMODE_BLEND);
                sdl::SDL_RenderFillRect(renderer, &sdl::FRect { x: lx - 4.0, y: ly - 2.0, w: width, h: graphics.line_height(FONT_SMALL) + 4.0 });
                let color = if fresh * unit > world.money { FONT_SMALL } else { FONT_SMALL_GOLD };
                graphics.draw_text(color, &label, lx, ly);
            }
            // Habillage d'origine : le bandeau du haut, répété sur la largeur, et le
            // panneau latéral à droite, prolongé vers le bas sur les grands écrans.
            let line = graphics.line_height(FONT_TEXT);
            let title_line = graphics.line_height(FONT_TITLE);
            let row = line + 8.0;
            let side = w - SIDEBAR_WIDTH;
            let mut x = 0.0;
            while x < side {
                graphics.blit(UI_TOP_BAR, x, 0.0);
                x += UI_TOP_BAR_WIDTH;
            }
            // Un fond uni derrière le panneau : son image a des réserves transparentes
            // (emplacements de boutons), et elle ne descend pas au-delà de 768 lignes.
            sdl::SDL_SetRenderDrawColor(renderer, 9, 62, 104, 255);
            sdl::SDL_RenderFillRect(renderer, &sdl::FRect { x: side, y: 0.0, w: SIDEBAR_WIDTH, h });
            graphics.blit(UI_SIDEBAR, side, 0.0);
            // Sur le bleu clair de l'habillage, le texte s'écrit en bleu nuit.
            graphics.ink = CHROME_INK;
            let (day, month, year) = world.date();
            let status = format!(
                "{} : {}     {} : {} / {}     {} : {}     {} {} {}{}{}",
                text.get(8, 3),
                world.population(),
                text.get(8, 13),
                world.workers_needed().min(world.workers_available()),
                world.workers_needed(),
                text.get(8, 0),
                world.money,
                day + 1,
                months[month as usize],
                // Sans réglages d'aventure, on compte les années depuis la fondation.
                if start_year == 0 { year as i64 + 1 } else { (start_year as i64 + year as i64).abs() },
                if start_year as i64 + (year as i64) < 0 { " av. J.-C." } else { "" },
                if paused { "  (pause)" } else { "" }
            );
            let status_width = graphics.text_width(FONT_CHROME, &status);
            graphics.draw_text(FONT_CHROME, &status, side - status_width - 16.0, (TOP_BAR_HEIGHT - line) / 2.0);
            // Menus du bandeau, comme dans le jeu : Fichier, Options, Aide.
            let menu_titles = [text.get(1, 0), text.get(2, 0), text.get(3, 0)];
            let mut menu_x = 12.0;
            let mut menu_slots = Vec::new();
            for title in &menu_titles {
                let width = graphics.text_width(FONT_CHROME, title) + 20.0;
                let slot = (menu_x - 6.0, 0.0, width, TOP_BAR_HEIGHT);
                graphics.draw_text(FONT_CHROME, title, menu_x, (TOP_BAR_HEIGHT - line) / 2.0);
                menu_slots.push(slot);
                menu_x += width;
            }
            // Le titre ne s'affiche que s'il tient entre les menus et les chiffres : dans
            // une fenêtre étroite, ce sont eux qui comptent.
            if menu_x + 12.0 + graphics.text_width(FONT_CHROME, &display_name) + 24.0 <= side - status_width - 16.0 {
                graphics.draw_text(FONT_CHROME, &display_name, menu_x + 12.0, (TOP_BAR_HEIGHT - line) / 2.0);
            }

            // Onglets des catégories dans la colonne du panneau, avec leurs icônes
            // d'origine ; l'onglet ouvert déploie ses bâtiments à gauche du panneau.
            // Les chiffres ouvrent une catégorie, puis y choisissent un bâtiment.
            menu_rows.clear();
            let pointer_ui = (pointer.0 / ui, pointer.1 / ui);
            let inside = |(x, y, w, h): (f32, f32, f32, f32)| pointer_ui.0 >= x && pointer_ui.0 < x + w && pointer_ui.1 >= y && pointer_ui.1 < y + h;
            for (index, &(icon, _)) in CATEGORY_TABS.iter().enumerate() {
                let slot = (side + TAB_LEFT, TAB_TOP + TAB_STEP * index as f32, TAB_WIDTH, TAB_HEIGHT);
                // Trois états se suivent dans le catalogue : repos, survol, choisi.
                let state = if open_category == Some(index) { 2 } else if inside(slot) { 1 } else { 0 };
                graphics.blit(UI_TABS + UI_TAB_STATES * icon + state, slot.0, slot.1);
                menu_rows.push((slot, MenuRow::Category(index)));
            }
            // Les autres réserves du panneau reçoivent les boutons d'origine : un fond
            // sombre, puis le bouton (repos, survol, enfoncé, inactif).
            for &(x, y, w, h, image, key) in &SIDEBAR_BUTTONS {
                let slot = (side + x, y, w, h);
                sdl::SDL_SetRenderDrawColor(renderer, 4, 30, 52, 255);
                sdl::SDL_RenderFillRect(renderer, &sdl::FRect { x: slot.0, y: slot.1, w: slot.2, h: slot.3 });
                let Some(image) = image else { continue };
                let open = match key {
                    Some(sdl::KEY_O) => goals_panel,
                    Some(sdl::KEY_C) => cities_panel,
                    Some(sdl::KEY_A) => advisor_panel,
                    Some(sdl::KEY_H) => help,
                    Some(sdl::KEY_J) => journal_panel,
                    _ => false,
                };
                let state = match key {
                    None => 3,
                    Some(_) if open => 2,
                    Some(_) if inside(slot) => 1,
                    Some(_) => 0,
                };
                graphics.blit(image + state, slot.0, slot.1);
                // Sur le parchemin des messages, le nombre de nouvelles annonces.
                if key == Some(sdl::KEY_J) {
                    if journal_panel {
                        unread = 0;
                    }
                    if unread > 0 {
                        let count = unread.min(99).to_string();
                        let width = graphics.text_width(FONT_SMALL, &count);
                        graphics.draw_text(FONT_SMALL, &count, slot.0 + (slot.2 - width) / 2.0, slot.1 + (slot.3 - graphics.line_height(FONT_SMALL)) / 2.0);
                    }
                }
                if let Some(key) = key {
                    menu_rows.push((slot, MenuRow::Key(key)));
                }
            }
            // Rangée des boutons ronds : route et dégager, comme dans le jeu.
            sdl::SDL_SetRenderDrawColor(renderer, 4, 30, 52, 255);
            let road_slot = (side + ROAD_SLOT.0, ROAD_SLOT.1, ROAD_SLOT.2, ROAD_SLOT.3);
            let clear_slot = (side + CLEAR_BUTTON_SLOT.0, CLEAR_BUTTON_SLOT.1, CLEAR_BUTTON_SLOT.2, CLEAR_BUTTON_SLOT.3);
            for (slot, image, held, picked) in [(road_slot, ROAD_BUTTON, Tool::Road, Tool::Road), (clear_slot, CLEAR_BUTTON, Tool::Demolish, Tool::Demolish)] {
                sdl::SDL_RenderFillRect(renderer, &sdl::FRect { x: slot.0, y: slot.1, w: slot.2, h: slot.3 });
                let state = if tool == held { 2 } else if inside(slot) { 1 } else { 0 };
                graphics.blit(image + state, slot.0, slot.1);
                menu_rows.push((slot, MenuRow::Pick(picked)));
            }
            // Barrage routier : inactif quand l'épisode ne le permet pas.
            let roadblock_slot = (side + ROADBLOCK_SLOT.0, ROADBLOCK_SLOT.1, ROADBLOCK_SLOT.2, ROADBLOCK_SLOT.3);
            let roadblock = world.kinds.iter().position(|k| k.function == Function::Roadblock).filter(|&k| !world.locked.get(k).copied().unwrap_or(false));
            let state = match roadblock {
                None => 3,
                Some(k) if tool == Tool::Build(k) => 2,
                Some(_) if inside(roadblock_slot) => 1,
                Some(_) => 0,
            };
            graphics.blit(ROADBLOCK_BUTTON + state, roadblock_slot.0, roadblock_slot.1);
            if let Some(k) = roadblock {
                menu_rows.push((roadblock_slot, MenuRow::Pick(Tool::Build(k))));
            }
            // Nom de la catégorie ouverte ou survolée, dans le haut du panneau.
            let named = open_category.or_else(|| menu_rows.iter().find_map(|&(slot, action)| match action {
                MenuRow::Category(index) if inside(slot) => Some(index),
                _ => None,
            }));
            if named.is_none() && (tool == Tool::Demolish || inside(clear_slot)) {
                graphics.draw_text(FONT_CHROME, &demolish_name, side + PANEL_LEFT, TAB_TOP + 6.0);
            }
            let small_line = graphics.line_height(FONT_SMALL) + 1.0;
            // Hauteur du bouton dont la liste est dépliée.
            let mut group_top = TAB_TOP;
            // Onglets du haut du panneau ; l'onglet inactif est assombri.
            for (wanted, slot) in [(false, SIDEBAR_TABS[0]), (true, SIDEBAR_TABS[1])] {
                let slot = (side + slot.0, slot.1, slot.2, slot.3);
                if sidebar_map != wanted {
                    sdl::SDL_SetRenderDrawBlendMode(renderer, sdl::BLENDMODE_BLEND);
                    sdl::SDL_SetRenderDrawColor(renderer, 0, 20, 40, if inside(slot) { 60 } else { 110 });
                    sdl::SDL_RenderFillRect(renderer, &sdl::FRect { x: slot.0, y: slot.1, w: slot.2, h: slot.3 });
                }
                menu_rows.push((slot, MenuRow::SidebarMap(wanted)));
            }
            let named = if sidebar_map { None } else { named };
            if let Some(index) = named {
                let mut top = TAB_TOP + 6.0;
                for word in category_names[index].split(' ') {
                    graphics.draw_text(FONT_CHROME, word, side + PANEL_LEFT, top);
                    top += row;
                }
                // Boutons des groupes de bâtiments, deux par rang, au bas du panneau.
                let groups = if open_category == Some(index) { category_groups(&world, index) } else { Vec::new() };
                let rows = groups.len().div_ceil(2) as f32;
                let buttons_top = SIDEBAR_BUTTONS[0].1 - 6.0 - rows * (GROUP_BUTTON.1 + 4.0);
                for (i, (button, tools)) in groups.iter().enumerate() {
                    let x = side + PANEL_LEFT + (i % 2) as f32 * (GROUP_BUTTON.0 + 4.0);
                    let y = buttons_top + (i / 2) as f32 * (GROUP_BUTTON.1 + 4.0);
                    let slot = (x, y, GROUP_BUTTON.0, GROUP_BUTTON.1);
                    if open_group == Some(*button) {
                        group_top = y;
                    }
                    let chosen = open_group == Some(*button) || tools.iter().any(|&(_, t)| t == tool);
                    let state = if chosen { 2 } else if inside(slot) { 1 } else { 0 };
                    graphics.blit(button + state, x, y);
                    menu_rows.push((slot, MenuRow::Group(*button)));
                }
                let info_bottom = if groups.is_empty() { SIDEBAR_BUTTONS[0].1 - 4.0 } else { buttons_top - 4.0 };
                // Ce que la catégorie dit de la cité, puis ses vues en surimpression.
                let (lines, views) = category_info(&world, &text, index);
                let width = SIDEBAR_WIDTH - PANEL_LEFT - 6.0;
                top += 6.0;
                for (label, wanted) in views {
                    let slot = (side + PANEL_LEFT - 2.0, top, width, small_line + 8.0);
                    if top + slot.3 > info_bottom {
                        break;
                    }
                    let lit = overlay == wanted || inside(slot);
                    sdl::SDL_SetRenderDrawBlendMode(renderer, sdl::BLENDMODE_BLEND);
                    sdl::SDL_SetRenderDrawColor(renderer, 10, 40, 80, if lit { 220 } else { 140 });
                    sdl::SDL_RenderFillRect(renderer, &sdl::FRect { x: slot.0, y: slot.1, w: slot.2, h: slot.3 });
                    graphics.ink = (255, 255, 255);
                    let font = if overlay == wanted { FONT_SMALL_GOLD } else { FONT_SMALL };
                    let label_width = graphics.text_width(font, label);
                    graphics.draw_text(font, label, slot.0 + (slot.2 - label_width) / 2.0, slot.1 + (slot.3 - small_line) / 2.0);
                    graphics.ink = CHROME_INK;
                    menu_rows.push((slot, MenuRow::Overlay(wanted)));
                    top += slot.3 + 3.0;
                }
                top += 6.0;
                // « Libellé : valeur » : le libellé en blanc, la valeur en doré dessous.
                graphics.ink = (255, 255, 255);
                for text_line in lines {
                    let (label, value) = text_line.split_once(" : ").map_or((text_line.as_str(), None), |(l, v)| (l, Some(v)));
                    let pieces: Vec<(Font, String)> = wrap_width(&graphics, FONT_SMALL, label, width)
                        .into_iter()
                        .map(|p| (FONT_SMALL, p))
                        .chain(value.into_iter().flat_map(|v| wrap_width(&graphics, FONT_SMALL_GOLD, v, width).into_iter().map(|p| (FONT_SMALL_GOLD, p))))
                        .collect();
                    for (font, piece) in pieces {
                        if top + small_line > info_bottom {
                            break;
                        }
                        graphics.draw_text(font, &piece, side + PANEL_LEFT, top);
                        top += small_line;
                    }
                    top += 4.0;
                }
                graphics.ink = CHROME_INK;
            }
            // Zone des messages : la dernière annonce ; un clic ouvre le journal.
            let message_slot = (side + MESSAGE_SLOT.0, MESSAGE_SLOT.1, MESSAGE_SLOT.2, MESSAGE_SLOT.3);
            if let Some((_, last)) = journal.last() {
                let mut top = message_slot.1 + 4.0;
                graphics.ink = (255, 255, 255);
                for piece in wrap_width(&graphics, FONT_SMALL, last, message_slot.2 - 8.0).into_iter().take(((message_slot.3 - 4.0) / small_line) as usize) {
                    graphics.draw_text(FONT_SMALL, &piece, message_slot.0 + 4.0, top);
                    top += small_line;
                }
                graphics.ink = CHROME_INK;
            }
            menu_rows.push((message_slot, MenuRow::Journal));
            graphics.ink = (255, 255, 255);
            if let (Some(index), Some(group)) = (open_category, open_group) {
                let listed: Vec<(usize, Tool)> = category_groups(&world, index).into_iter().find(|(b, _)| *b == group).map(|(_, t)| t).unwrap_or_default();
                let tools: Vec<Tool> = listed.iter().map(|&(_, t)| t).collect();
                let labels: Vec<String> = listed
                    .iter()
                    .map(|&(slot, ref picked)| {
                        let key = if slot < 9 { (slot + 1).to_string() } else { "-".to_string() };
                        match *picked {
                            Tool::Build(kind) => format!("{key}  {}  ({})", world.kind_name(kind), world.kinds[kind].cost),
                            Tool::Wall => format!("{key}  {wall_name}  ({wall_cost})"),
                            _ => format!("{key}  {road_name}  ({road_cost})"),
                        }
                    })
                    .collect();
                let width = labels.iter().map(|label| graphics.text_width(FONT_TEXT, label)).fold(0.0, f32::max) + 32.0;
                let height = row * labels.len() as f32 + 10.0;
                // À hauteur de l'onglet, sans sortir de l'écran par le bas.
                let _ = index;
                let top = group_top.min(h - height - 8.0).max(TOP_BAR_HEIGHT + 4.0);
                let left = side - width - 6.0;
                graphics.panel(left, top, width, height);
                for (i, (label, picked)) in labels.iter().zip(&tools).enumerate() {
                    let slot = (left, top + 5.0 + row * i as f32, width, row);
                    if *picked == tool || inside(slot) {
                        sdl::SDL_SetRenderDrawColor(renderer, 200, 160, 40, if *picked == tool { 140 } else { 70 });
                        sdl::SDL_RenderFillRect(renderer, &sdl::FRect { x: slot.0 + 4.0, y: slot.1 - 1.0, w: width - 8.0, h: row - 2.0 });
                    }
                    graphics.draw_text(FONT_TEXT, label, left + 16.0, slot.1 + 3.0);
                    menu_rows.push((slot, MenuRow::Pick(*picked)));
                }
            }

            // Mini-carte en bas à gauche, avec le cadre de ce que montre l'écran.
            if !minimap.is_null() {
                minimap_age += 1;
                if minimap_age >= MINIMAP_REFRESH {
                    minimap_age = 0;
                    let pixels = minimap_pixels(&world);
                    sdl::SDL_UpdateTexture(minimap, null(), pixels.as_ptr() as *const c_void, (MINIMAP_WIDTH * 4) as i32);
                }
                // Dans le panneau (carte générale) ou flottante en bas à gauche.
                let area = (side + PANEL_LEFT - 2.0, TAB_TOP + 4.0, SIDEBAR_WIDTH - PANEL_LEFT - 2.0, SIDEBAR_BUTTONS[0].1 - TAB_TOP - 12.0);
                minimap_at.scale = if sidebar_map { (area.2 / minimap_at.width).min(area.3 / minimap_at.height) } else { minimap_at.base };
                let (mw, mh) = minimap_at.size();
                (minimap_at.x, minimap_at.y) = if sidebar_map { (area.0 + (area.2 - mw) / 2.0, area.1 + (area.3 - mh) / 2.0) } else { (12.0, h - mh - 12.0) };
                graphics.panel(minimap_at.x - 4.0, minimap_at.y - 4.0, mw + 8.0, mh + 8.0);
                let part = sdl::FRect { x: minimap_at.left, y: minimap_at.top, w: minimap_at.width, h: minimap_at.height };
                let place = sdl::FRect { x: minimap_at.x, y: minimap_at.y, w: mw, h: mh };
                sdl::SDL_SetTextureScaleMode(minimap, sdl::SCALEMODE_NEAREST);
                sdl::SDL_RenderTexture(renderer, minimap, &part, &place);
                let ((x0, y0), (x1, y1)) = (minimap_at.place(visible.0, visible.1), minimap_at.place(visible.2, visible.3));
                sdl::SDL_SetRenderDrawColor(renderer, 255, 255, 255, 255);
                for (ax, ay, bx, by) in [(x0, y0, x1, y0), (x1, y0, x1, y1), (x1, y1, x0, y1), (x0, y1, x0, y0)] {
                    sdl::SDL_RenderLine(renderer, ax, ay, bx, by);
                }
            }

            // Fiche d'un bâtiment : complète dans une fenêtre après un clic droit (comme
            // dans le jeu), abrégée au survol.
            let pinned_building = info_pinned.and_then(|(x, y)| world.building_at(x, y));
            if pinned_building.is_none() {
                info_pinned = None;
            }
            if let Some(building) = pinned_building.or_else(|| world.building_at(under.0, under.1)) {
                let full = pinned_building.is_some();
                let mut lines = vec![world.kind_name(building.kind)];
                if let Some(Some(description)) = descriptions.get(building.kind) {
                    lines.extend(wrap(description, INFO_LINE));
                }
                let employees = world.kinds[building.kind].employees;
                if employees > 0 {
                    lines.push(format!("{} : {} / {}", text.get(8, 13), building.workers, employees));
                }
                let stocked = match world.kinds[building.kind].function {
                    Function::Farm | Function::Granary | Function::Fishery => Some(Good::Food),
                    Function::Producer(good) | Function::Workshop(good) | Function::Vendor(good) => Some(good),
                    _ => None,
                };
                if building.fire + building.damage > 0 {
                    let (fire, damage) = world.risks(building);
                    lines.push(format!("Risques : feu {fire} %, dégâts {damage} %"));
                }
                // Temple de héros : où en sont les exigences du héros.
                if let Function::HeroHall(hero) = world.kinds[building.kind].function {
                    let hall = (building.x, building.y, world.kinds[building.kind].size);
                    if world.heroes[hero as usize] {
                        lines.push(text.get(heroes::TEXT_HALLS, 68).replace("[hero_name]", &text.get(heroes::TEXT_HEROES, hero as usize)));
                    }
                    for requirement in heroes::requirements(hero) {
                        lines.push(requirement_line(&text, requirement, requirement.check(&world, hall), world.atlantean));
                    }
                }
                if world.kinds[building.kind].function == Function::Racetrack {
                    let (sections, closed) = world.hippodrome();
                    lines.push(format!("Hippodrome : {sections} stades, {}", if closed { "les courses ont lieu" } else { "piste pas encore fermée" }));
                }
                if world.kinds[building.kind].function == Function::Wharf {
                    lines.push(match building.stock {
                        0 => format!(
                            "{} en chantier : {} {} / 4, {} {} / 1",
                            capitalized(world.ship_singular()),
                            good_names[Good::Wood as usize],
                            building.goods[Good::Wood as usize],
                            good_names[Good::Arms as usize],
                            building.goods[Good::Arms as usize]
                        ),
                        ..0 => format!("Sa {} est partie en campagne", world.ship_singular()),
                        _ => format!("Une {} est à quai", world.ship_singular()),
                    });
                }
                // Chantier de sanctuaire ou de monument : matériaux livrés sur demandés.
                if let Function::Sanctuary { needs, .. } | Function::Monument { needs, .. } = world.kinds[building.kind].function {
                    if !world.is_complete(building) {
                        let delivered: Vec<String> = zeus::world::SANCTUARY_MATERIALS
                            .iter()
                            .zip(needs)
                            .filter(|&(_, need)| need > 0)
                            .map(|(&good, need)| format!("{} {} / {need}", good_names[good as usize], building.goods[good as usize].min(need as i32)))
                            .collect();
                        lines.push(format!("Chantier : {}", delivered.join(", ")));
                    }
                }
                if world.kinds[building.kind].function == Function::Storehouse {
                    for good in GOODS.into_iter().filter(|&good| building.goods[good as usize] > 0) {
                        lines.push(format!("{} : {}", good_names[good as usize], building.goods[good as usize]));
                    }
                }
                if let Some(good) = stocked {
                    lines.push(format!("{} : {}", good_names[good as usize], building.stock));
                }
                if let Some(house) = &building.house {
                    lines[0] = level_names[house.level.min(level_names.len() - 1)].clone();
                    lines.push(format!("{} : {} / {}", text.get(8, 3), house.population, world.capacity(house)));
                    lines.push(format!("Attrait : {}", world.appeal(building.x as i32, building.y as i32)));
                    lines.push(format!("Culture : {}", world.house_culture(house)));
                    // Les marchandises en réserve ; ce qui manque est dit plus bas.
                    for good in GOODS.into_iter().filter(|&good| for_houses(good) && (good == Good::Food || house.goods[good as usize] > 0)) {
                        lines.push(format!("{} : {}", good_names[good as usize], house.goods[good as usize]));
                    }
                    for service in SERVICES {
                        let label = service_label(&world, &text, service);
                        lines.push(format!("{label} : {}", text.get(18, if house.has(service, world.ticks) { 1 } else { 0 })));
                    }
                    let needs: Vec<String> = world
                        .house_needs(building)
                        .into_iter()
                        .map(|need| match need {
                            Need::Food => good_names[Good::Food as usize].to_lowercase(),
                            Need::Water => service_name(&text, Service::Water).to_lowercase(),
                            Need::Goods(good) => good_names[good as usize].to_lowercase(),
                            Need::Stadium => "un stade".to_string(),
                            Need::Culture(n) => format!("culture {n}"),
                            Need::Appeal(n) => format!("attrait {n}"),
                            Need::Top => String::new(),
                        })
                        .collect();
                    match needs.as_slice() {
                        [] => lines.push("Va évoluer".to_string()),
                        [top] if top.is_empty() => {}
                        _ => lines.push(format!("Pour évoluer : {}", needs.join(", "))),
                    }
                }
                if let Some(missing) = world.service_waits_for(building) {
                    if let Some(kind) = world.kinds.iter().find(|k| k.function == missing) {
                        lines.push(format!("En attente : {}", kind.name.to_lowercase()));
                    }
                }
                if full {
                    // Fenêtre au centre : le nom en titre, le reste dessous.
                    let heading = lines.remove(0);
                    lines.push(String::new());
                    lines.push("Clic ou Echap : fermer".to_string());
                    let width = lines.iter().map(|l| graphics.text_width(FONT_TEXT, l)).fold(graphics.text_width(FONT_TITLE, &heading), f32::max) + 48.0;
                    let height = title_line + 32.0 + row * lines.len() as f32;
                    let (px, py) = ((side - width) / 2.0, (h - height) / 2.0);
                    graphics.panel(px, py, width, height);
                    graphics.panel(px, py, width, height);
                    let heading_width = graphics.text_width(FONT_TITLE, &heading);
                    graphics.draw_text(FONT_TITLE, &heading, px + (width - heading_width) / 2.0, py + 12.0);
                    for (i, l) in lines.iter().enumerate() {
                        graphics.draw_text(FONT_TEXT, l, px + 24.0, py + title_line + 24.0 + row * i as f32);
                    }
                } else {
                    // Au survol : le nom, l'effectif et ce qui manque ou bloque.
                    let description = descriptions.get(building.kind).cloned().flatten().map(|d| wrap(&d, INFO_LINE).len()).unwrap_or(0);
                    let mut short: Vec<String> = lines.iter().take(1).cloned().collect();
                    short.extend(lines.iter().skip(1 + description).filter(|l| l.starts_with(&text.get(8, 13)) || l.starts_with(&text.get(8, 3)) || l.starts_with("Pour évoluer") || l.starts_with("Va évoluer") || l.starts_with("En attente") || l.starts_with("Risques")).cloned());
                    short.push("Clic droit : détails".to_string());
                    let width = short.iter().map(|l| graphics.text_width(FONT_TEXT, l)).fold(0.0, f32::max) + 32.0;
                    let height = row * short.len() as f32 + 12.0;
                    let (px, py) = (12.0, TOP_BAR_HEIGHT + 12.0);
                    graphics.panel(px, py, width, height);
                    for (i, l) in short.iter().enumerate() {
                        graphics.draw_text(FONT_TEXT, l, px + 16.0, py + 8.0 + row * i as f32);
                    }
                }
            }

            // Récit d'ouverture : titre et paragraphes, coupés à la largeur du panneau.
            if let Some((heading, paragraphs)) = &intro {
                let width = (w - 80.0).min(INTRO_WIDTH);
                let mut lines: Vec<String> = Vec::new();
                for paragraph in paragraphs {
                    let mut current = String::new();
                    for word in paragraph.split(' ') {
                        let longer = if current.is_empty() { word.to_string() } else { format!("{current} {word}") };
                        if graphics.text_width(FONT_TEXT, &longer) > width - 48.0 && !current.is_empty() {
                            lines.push(std::mem::replace(&mut current, word.to_string()));
                        } else {
                            current = longer;
                        }
                    }
                    lines.push(current);
                    lines.push(String::new());
                }
                // Ce qui dépasse la hauteur de l'écran est coupé.
                let room = ((h - title_line - 140.0) / row).max(1.0) as usize;
                lines.truncate(room);
                let height = title_line + 36.0 + row * lines.len() as f32;
                let (px, py) = ((w - width) / 2.0, (h - height) / 2.0);
                graphics.panel(px, py, width, height);
                graphics.panel(px, py, width, height);
                let heading_width = graphics.text_width(FONT_TITLE, heading);
                graphics.draw_text(FONT_TITLE, heading, px + (width - heading_width) / 2.0, py + 12.0);
                for (i, l) in lines.iter().enumerate() {
                    graphics.draw_text(FONT_TEXT, l, px + 24.0, py + title_line + 28.0 + row * i as f32);
                }
            }

            // Objectifs de l'épisode et où ils en sont.
            if goals_panel {
                let heading = text.get(44, 45);
                let mut lines: Vec<String> = goals
                    .iter()
                    .zip(&statuses)
                    .map(|(goal, status)| {
                        // La police du jeu n'a pas de tiret long : l'état va entre parenthèses.
                        let state = match status {
                            Status::Met => text.get(194, 62).to_lowercase(),
                            Status::Pending(Some(done)) if goal.kind == GoalKind::SetAside => {
                                format!("{done} mis de côté, {} en stock", world.stored(goal.values[0] as usize))
                            }
                            Status::Pending(Some(current)) => format!("actuellement {current}"),
                            Status::Pending(None) => text.get(194, 52),
                            Status::Unsupported => "pas encore simulé par Olympe".to_string(),
                        };
                        let event = episode(journey.step()).and_then(|e| e.events.get(goal.values[0].max(0) as usize).copied());
                        let label = match (goal.kind, event) {
                            (GoalKind::Quest, Some(Event::Quest { god, quest, hero })) => {
                                format!("{} ({})", quest_name(&text, god, quest), text.get(heroes::TEXT_HEROES, hero as usize))
                            }
                            (GoalKind::Rule, _) => text.get(194, 31).replace("[city]", &city_name(&text, &world, goal.values[0].max(0) as usize)),
                            (GoalKind::Slay, Some(Event::Monster { monster, .. })) => {
                                text.get(194, 28).replace("[monster]", &text.get(heroes::TEXT_MONSTERS, monster as usize))
                            }
                            _ => goal.describe(&text, &level_names, matches!(journey.step(), Step::Colony(_))),
                        };
                        format!("{label} ({state})")
                    })
                    .collect();
                if lines.is_empty() {
                    lines.push(text.get(44, 280));
                }
                if goals.iter().zip(&statuses).any(|(goal, status)| goal.kind == GoalKind::SetAside && *status != Status::Met) {
                    lines.push("R : mettre de côté ce qui est en stock".to_string());
                }
                let width = lines.iter().map(|l| graphics.text_width(FONT_TEXT, l)).fold(graphics.text_width(FONT_TITLE, &heading), f32::max) + 48.0;
                let height = title_line + 32.0 + row * lines.len() as f32;
                let (px, py) = ((w - width) / 2.0, (h - height) / 2.0);
                graphics.panel(px, py, width, height);
                graphics.panel(px, py, width, height);
                let heading_width = graphics.text_width(FONT_TITLE, &heading);
                graphics.draw_text(FONT_TITLE, &heading, px + (width - heading_width) / 2.0, py + 12.0);
                for (i, l) in lines.iter().enumerate() {
                    graphics.draw_text(FONT_TEXT, l, px + 24.0, py + title_line + 24.0 + row * i as f32);
                }
            }

            // Cités du monde : statut, force, et conquête d'un clic.
            if advisor_panel {
                let pages = advice(&text, &world, &good_names, &level_names);
                advisor_page %= pages.len();
                let (title, lines) = &pages[advisor_page];
                let heading = format!("Conseiller : {title} ({}/{}, Tab)", advisor_page + 1, pages.len());
                let width = lines.iter().map(|l| graphics.text_width(FONT_TEXT, l)).fold(graphics.text_width(FONT_TITLE, &heading), f32::max) + 48.0;
                let height = title_line + 32.0 + row * lines.len() as f32;
                let (px, py) = ((w - width) / 2.0, (h - height) / 2.0);
                graphics.panel(px, py, width, height);
                let heading_width = graphics.text_width(FONT_TITLE, &heading);
                graphics.draw_text(FONT_TITLE, &heading, px + (width - heading_width) / 2.0, py + 12.0);
                for (i, line) in lines.iter().enumerate() {
                    graphics.draw_text(FONT_TEXT, line, px + 24.0, py + title_line + 24.0 + row * i as f32);
                }
            }
            if cities_panel {
                let heading = "Cités du monde".to_string();
                let statuses = (0..4).map(|s| text.get(253, s)).collect::<Vec<_>>();
                let served = world.trade_posts();
                // Chaque ligne : la cité (clic : la soumettre), le raid (clic : la piller),
                // puis le commerce (clic : ouvrir une route).
                let rows: Vec<(usize, String, String, String)> = world
                    .cities
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.active && (c.kind == zeus::map::FOREIGN || c.kind == zeus::map::DISTANT))
                    .map(|(i, c)| {
                        let status = if c.kind == zeus::map::DISTANT { "lointaine".to_string() } else { statuses.get(c.status.clamp(0, 3) as usize).cloned().unwrap_or_default() };
                        let open = c.kind == zeus::map::FOREIGN && c.status <= zeus::map::RIVAL;
                        let action = if open { format!(", défense {}, {} mois de marche) [soumettre]", world.defence(i), world.travel_months(i)) } else { ")".to_string() };
                        let raid = if open { "  [piller]".to_string() } else { String::new() };
                        let names = |goods: &[(u8, i32)]| goods.iter().map(|&(g, _)| text.get(23, g as usize).to_lowercase()).collect::<Vec<_>>().join(", ");
                        let goods = format!("Vend {} ; achète {}", names(&c.sells), names(&c.buys));
                        let post = if world.sea_route(i) { "des docks" } else { "un poste de commerce" };
                        let trade = match (world.trades_with(i), served.contains(&i)) {
                            (true, true) => format!("{goods} (ventes {} dr., achats {} dr.)", c.sold, c.bought),
                            (true, false) => format!("{goods} : construisez {post} pour commercer"),
                            (false, _) => String::new(),
                        };
                        let away = if world.army_away(i) { ", armée partie" } else { "" };
                        (i, format!("{} ({status}, faveur {}, force {}{away}{action}", city_name(&text, &world, i), c.favor, c.military), raid, trade)
                    })
                    .collect();
                // Le commerce d'une cité s'écrit en retrait sur une seconde ligne.
                let indent = 24.0;
                let line_width = |(_, city, raid, trade): &(usize, String, String, String)| {
                    (graphics.text_width(FONT_TEXT, city) + graphics.text_width(FONT_TEXT, raid)).max(indent + graphics.text_width(FONT_TEXT, trade))
                };
                let lines = rows.iter().map(|r| if r.3.is_empty() { 1 } else { 2 }).sum::<usize>();
                let width = rows.iter().map(line_width).fold(graphics.text_width(FONT_TITLE, &heading), f32::max) + 48.0;
                let height = title_line + 32.0 + row * lines.max(1) as f32;
                let (px, py) = ((w - width) / 2.0, (h - height) / 2.0);
                graphics.panel(px, py, width, height);
                graphics.panel(px, py, width, height);
                let heading_width = graphics.text_width(FONT_TITLE, &heading);
                graphics.draw_text(FONT_TITLE, &heading, px + (width - heading_width) / 2.0, py + 12.0);
                if rows.is_empty() {
                    graphics.draw_text(FONT_TEXT, "Aucune cité étrangère", px + 24.0, py + title_line + 24.0);
                }
                // Demandes en attente, sous les cités : un clic les satisfait.
                let months_left = |until: u64| until.saturating_sub(world.ticks) / zeus::world::TICKS_PER_MONTH;
                for (i, request) in world.requests.iter().enumerate() {
                    let y = py + height + 8.0 + row * i as f32;
                    let line = format!(
                        "{} demande {} {} (vous avez {}, encore {} mois) [envoyer]",
                        city_name(&text, &world, request.city),
                        request.amount,
                        request_good(&text, request.resource),
                        world.available_for(request.resource),
                        months_left(request.until)
                    );
                    let line_width = graphics.text_width(FONT_TEXT, &line) + 48.0;
                    graphics.panel(px, y, line_width.max(width), row);
                    graphics.draw_text(FONT_TEXT, &line, px + 24.0, y + 4.0);
                    menu_rows.push(((px, y, line_width.max(width), row), MenuRow::Fulfil(i)));
                }
                // Réglage du commerce, une ligne par ressource échangée avec une cité
                // partenaire : un clic passe au réglage suivant.
                let mut traded: Vec<usize> = world
                    .cities
                    .iter()
                    .filter(|c| c.active && c.trade)
                    .flat_map(|c| c.sells.iter().chain(&c.buys).map(|&(r, _)| r as usize))
                    .filter(|&r| r < world.trade_policy.len())
                    .collect();
                traded.sort_unstable();
                traded.dedup();
                let below = py + height + 8.0 + row * world.requests.len() as f32;
                for (i, &resource) in traded.iter().enumerate() {
                    let policy = match world.trade_policy[resource] {
                        TradePolicy::Both => "vente du surplus et achat",
                        TradePolicy::SellOnly => "vente du surplus seulement",
                        TradePolicy::BuyOnly => "achat seulement",
                        TradePolicy::Neither => "ni vente ni achat",
                    };
                    let line = format!("Commerce de {} : {policy} [changer]", text.get(23, resource).to_lowercase());
                    let y = below + row * i as f32;
                    let line_width = (graphics.text_width(FONT_TEXT, &line) + 48.0).max(width);
                    graphics.panel(px, y, line_width, row);
                    graphics.draw_text(FONT_TEXT, &line, px + 24.0, y + 4.0);
                    menu_rows.push(((px, y, line_width, row), MenuRow::Policy(resource)));
                }
                let mut y = py + title_line + 24.0;
                for (city, line, raid, trade) in &rows {
                    let middle = px + 24.0 + graphics.text_width(FONT_TEXT, line);
                    let end = middle + graphics.text_width(FONT_TEXT, raid);
                    graphics.draw_text(FONT_TEXT, line, px + 24.0, y);
                    graphics.draw_text(FONT_TEXT, raid, middle, y);
                    menu_rows.push(((px, y, middle - px, row), MenuRow::Conquer(*city, false)));
                    if !raid.is_empty() {
                        menu_rows.push(((middle, y, end - middle, row), MenuRow::Conquer(*city, true)));
                    }
                    y += row;
                    if !trade.is_empty() {
                        graphics.draw_text(FONT_TEXT, trade, px + 24.0 + indent, y);
                        y += row;
                    }
                }
            }

            // Journal : les dernières annonces, la plus récente en haut.
            if journal_panel {
                let heading = "Journal".to_string();
                let lines: Vec<String> = if journal.is_empty() { vec!["Aucune annonce".to_string()] } else { journal.iter().rev().map(|(date, line)| format!("{date} : {line}")).collect() };
                let width = lines.iter().map(|l| graphics.text_width(FONT_TEXT, l)).fold(graphics.text_width(FONT_TITLE, &heading), f32::max) + 48.0;
                let height = title_line + 32.0 + row * lines.len() as f32;
                let (px, py) = ((w - width) / 2.0, (h - height) / 2.0);
                graphics.panel(px, py, width, height);
                graphics.panel(px, py, width, height);
                let heading_width = graphics.text_width(FONT_TITLE, &heading);
                graphics.draw_text(FONT_TITLE, &heading, px + (width - heading_width) / 2.0, py + 12.0);
                for (i, l) in lines.iter().enumerate() {
                    graphics.draw_text(FONT_TEXT, l, px + 24.0, py + title_line + 24.0 + row * i as f32);
                }
            }

            // Aide : les commandes, au centre de l'écran.
            if help {
                let width = HELP.iter().map(|l| graphics.text_width(FONT_TEXT, l)).fold(0.0, f32::max) + 48.0;
                let height = row * HELP.len() as f32 + 24.0;
                let (px, py) = ((w - width) / 2.0, (h - height) / 2.0);
                graphics.panel(px, py, width, height);
                graphics.panel(px, py, width, height);
                for (i, l) in HELP.iter().enumerate() {
                    graphics.draw_text(FONT_TEXT, l, px + 24.0, py + 12.0 + row * i as f32);
                }
            }

            // Menu du bandeau déroulé : chaque ligne fait comme sa touche.
            for (index, &slot) in menu_slots.iter().enumerate() {
                menu_rows.push((slot, MenuRow::TopMenu(index)));
            }
            if let Some(index) = top_menu {
                let items: Vec<(String, u32)> = match index {
                    0 => vec![(format!("{} (F5)", text.get(1, 4)), sdl::KEY_F5), (format!("{} (F9)", text.get(1, 3)), sdl::KEY_F9), (format!("{} (Q)", text.get(1, 5)), sdl::KEY_Q)],
                    1 => vec![
                        ("Musique (M)".to_string(), sdl::KEY_M),
                        ("Bruitages (B)".to_string(), sdl::KEY_B),
                        ("Plein écran (F)".to_string(), sdl::KEY_F),
                        ("Plus vite (Page préc.)".to_string(), sdl::KEY_PAGEUP),
                        ("Plus lent (Page suiv.)".to_string(), sdl::KEY_PAGEDOWN),
                        ("Pause (P)".to_string(), sdl::KEY_P),
                        ("Vues (V)".to_string(), sdl::KEY_V),
                        ("Mettre de côté (R)".to_string(), sdl::KEY_R),
                    ],
                    _ => vec![
                        (format!("{} (H)", text.get(3, 0)), sdl::KEY_H),
                        ("Objectifs (O)".to_string(), sdl::KEY_O),
                        ("Conseillers (A)".to_string(), sdl::KEY_A),
                        ("Cités du monde (C)".to_string(), sdl::KEY_C),
                        ("Journal (J)".to_string(), sdl::KEY_J),
                    ],
                };
                let width = items.iter().map(|(label, _)| graphics.text_width(FONT_TEXT, label)).fold(0.0, f32::max) + 32.0;
                let (px, py) = (menu_slots[index].0, TOP_BAR_HEIGHT);
                let height = row * items.len() as f32 + 10.0;
                graphics.panel(px, py, width, height);
                graphics.panel(px, py, width, height);
                for (i, (label, key)) in items.iter().enumerate() {
                    let slot = (px, py + 5.0 + row * i as f32, width, row);
                    if inside(slot) {
                        sdl::SDL_SetRenderDrawColor(renderer, 200, 160, 40, 90);
                        sdl::SDL_RenderFillRect(renderer, &sdl::FRect { x: slot.0 + 4.0, y: slot.1 - 1.0, w: width - 8.0, h: row - 2.0 });
                    }
                    graphics.draw_text(FONT_TEXT, label, px + 16.0, slot.1 + 3.0);
                    menu_rows.push((slot, MenuRow::Key(*key)));
                }
            }

            // Bandeau de pause, en haut de la carte comme dans le jeu.
            if paused {
                let banner = text.get(13, 2);
                let width = graphics.text_width(FONT_TEXT, &banner) + 48.0;
                graphics.panel((side - width) / 2.0, TOP_BAR_HEIGHT + 12.0, width, row + 12.0);
                graphics.draw_text(FONT_TEXT, &banner, (side - width) / 2.0 + 24.0, TOP_BAR_HEIGHT + 18.0);
            }
            // Composition d'une campagne : les compagnies et les trières à envoyer.
            if let Some(mut draft) = company {
                let ranks = world.ranks();
                let ships = world.triremes();
                for rank in 0..3 {
                    draft.soldiers[rank] = draft.soldiers[rank].clamp(0, ranks[rank]);
                }
                draft.triremes = draft.triremes.clamp(0, ships);
                company = Some(draft);

                let action = if draft.raid { "piller" } else { "soumettre" };
                let title = format!("Compagnie pour {action} {}", city_name(&text, &world, draft.city));
                let detail = format!("Défense {} — {} mois de marche", world.defence(draft.city), world.travel_months(draft.city));
                let ships_label = if world.atlantean { "Frégates" } else { "Trières" };
                let units = [("Piétaille", 0), ("Hoplites", 1), ("Cavaliers", 2), (ships_label, 3)];
                let lines: Vec<String> = units
                    .iter()
                    .map(|&(name, slot)| {
                        let (count, max) = if slot < 3 { (draft.soldiers[slot], ranks[slot]) } else { (draft.triremes, ships) };
                        format!("{name} : {count} / {max}")
                    })
                    .collect();
                let force_line = format!("Force envoyée : {}", World::force(draft.soldiers, draft.triremes));
                let footer = ["Tout", "Envoyer", "Annuler"];
                let footer_widths: Vec<f32> = footer.iter().map(|label| graphics.text_width(FONT_TEXT, label) + 24.0).collect();
                let footer_total = footer_widths.iter().sum::<f32>() + 8.0 * (footer.len() - 1) as f32;
                let content = lines
                    .iter()
                    .map(|line| graphics.text_width(FONT_TEXT, line))
                    .fold(graphics.text_width(FONT_TITLE, &title), f32::max)
                    .max(graphics.text_width(FONT_TEXT, &detail))
                    .max(graphics.text_width(FONT_TEXT, &force_line))
                    .max(footer_total);
                let width = content + 24.0 + 90.0 + 24.0;
                let height = 36.0 + title_line + 7.0 * row;
                let (px, py) = ((w - width) / 2.0, (h - height) / 2.0);
                // Deux couches, comme les autres panneaux : le texte reste lisible sur
                // la carte et sur le panneau des cités resté derrière.
                graphics.panel(px, py, width, height);
                graphics.panel(px, py, width, height);
                graphics.ink = (255, 255, 255);
                // Fond plein écran : la carte et les panneaux derrière ne reçoivent plus
                // les clics tant que la composition est ouverte.
                menu_rows.push(((0.0, 0.0, w, h), MenuRow::CompanyShield));
                let title_width = graphics.text_width(FONT_TITLE, &title);
                graphics.draw_text(FONT_TITLE, &title, px + (width - title_width) / 2.0, py + 12.0);
                let mut y = py + 12.0 + title_line + 4.0;
                graphics.draw_text(FONT_TEXT, &detail, px + 24.0, y);
                y += row;
                for (index, line) in lines.iter().enumerate() {
                    graphics.draw_text(FONT_TEXT, line, px + 24.0, y);
                    let (count, max) = if index < 3 { (draft.soldiers[index], ranks[index]) } else { (draft.triremes, ships) };
                    let minus = (px + width - 24.0 - 68.0, y - 2.0, 30.0, row - 2.0);
                    let plus = (minus.0 + 34.0, minus.1, 30.0, minus.3);
                    for (slot, delta) in [(minus, -1), (plus, 1)] {
                        let active = if delta < 0 { count > 0 } else { count < max };
                        sdl::SDL_SetRenderDrawBlendMode(renderer, sdl::BLENDMODE_BLEND);
                        sdl::SDL_SetRenderDrawColor(renderer, 40, 90, 150, if active { 240 } else { 90 });
                        sdl::SDL_RenderFillRect(renderer, &sdl::FRect { x: slot.0, y: slot.1, w: slot.2, h: slot.3 });
                        let glyph = if delta < 0 { "-" } else { "+" };
                        let glyph_width = graphics.text_width(FONT_TEXT, glyph);
                        graphics.draw_text(FONT_TEXT, glyph, slot.0 + (slot.2 - glyph_width) / 2.0, slot.1 + 2.0);
                        menu_rows.push((slot, MenuRow::CompanyCount(index, delta)));
                    }
                    y += row;
                }
                graphics.draw_text(FONT_TEXT, &force_line, px + 24.0, y);
                y += row + 8.0;
                let mut bx = px + 24.0;
                for (index, &label) in footer.iter().enumerate() {
                    let slot = (bx, y, footer_widths[index], row - 2.0);
                    let (r, g, b) = match index {
                        0 => (60, 90, 130),
                        1 => (30, 110, 60),
                        _ => (110, 60, 60),
                    };
                    sdl::SDL_SetRenderDrawBlendMode(renderer, sdl::BLENDMODE_BLEND);
                    sdl::SDL_SetRenderDrawColor(renderer, r, g, b, 240);
                    sdl::SDL_RenderFillRect(renderer, &sdl::FRect { x: slot.0, y: slot.1, w: slot.2, h: slot.3 });
                    let label_width = graphics.text_width(FONT_TEXT, label);
                    graphics.draw_text(FONT_TEXT, label, slot.0 + (slot.2 - label_width) / 2.0, slot.1 + 2.0);
                    let action = match index {
                        0 => MenuRow::CompanyAll,
                        1 => MenuRow::CompanySend,
                        _ => MenuRow::CompanyCancel,
                    };
                    menu_rows.push((slot, action));
                    bx += footer_widths[index] + 8.0;
                }
            }

            // Bulle d'aide : ce que fait l'élément d'interface survolé, après un instant.
            let hovered_row = menu_rows.iter().rev().find(|&&(slot, _)| inside(slot)).map(|&(slot, action)| (slot, action));
            let hovered_key = hovered_row.map(|(slot, _)| ((slot.0 * 7.0 + slot.1) as i64));
            if hovered_key != tooltip.0 {
                tooltip = (hovered_key, now);
            }
            if let (Some((_, action)), true) = (hovered_row, now - tooltip.1 > TOOLTIP_MS) {
                let label = match action {
                    MenuRow::Category(index) => Some(category_names[index].clone()),
                    MenuRow::Group(button) => open_category.and_then(|c| category_groups(&world, c).into_iter().find(|(b, _)| *b == button)).map(|(_, tools)| {
                        {
                        let names: Vec<String> = tools.iter().map(|&(_, t)| match t { Tool::Build(k) => world.kind_name(k), Tool::Wall => wall_name.clone(), _ => road_name.clone() }).collect();
                        if names.len() > 2 { format!("{}, {} et {} autres", names[0], names[1], names.len() - 2) } else { names.join(", ") }
                    }
                    }),
                    MenuRow::Pick(Tool::Road) => Some(format!("{road_name} ({road_cost})")),
                    MenuRow::Pick(Tool::Demolish) => Some(demolish_name.clone()),
                    MenuRow::Key(sdl::KEY_H) => Some("Aide (H)".to_string()),
                    MenuRow::Key(sdl::KEY_O) => Some("Objectifs de l'épisode (O)".to_string()),
                    MenuRow::Key(sdl::KEY_J) => Some("Journal des messages (J)".to_string()),
                    MenuRow::Key(sdl::KEY_A) => Some("Conseillers (A)".to_string()),
                    MenuRow::Key(sdl::KEY_C) => Some("Cités du monde (C)".to_string()),
                    MenuRow::SidebarMap(true) => Some("Carte générale".to_string()),
                    MenuRow::SidebarMap(false) => Some("Constructions".to_string()),
                    MenuRow::Journal => Some("Journal des annonces (J)".to_string()),
                    _ => None,
                };
                if let Some(label) = label.filter(|l| !l.is_empty()) {
                    let width = graphics.text_width(FONT_SMALL, &label) + 12.0;
                    let (mut tx, mut ty) = (pointer_ui.0 + 14.0, pointer_ui.1 + 18.0);
                    tx = tx.min(w - width - 4.0);
                    ty = ty.min(h - small_line - 10.0);
                    sdl::SDL_SetRenderDrawBlendMode(renderer, sdl::BLENDMODE_BLEND);
                    sdl::SDL_SetRenderDrawColor(renderer, 250, 240, 200, 240);
                    sdl::SDL_RenderFillRect(renderer, &sdl::FRect { x: tx, y: ty, w: width, h: small_line + 6.0 });
                    graphics.ink = (40, 30, 10);
                    graphics.draw_text(FONT_SMALL, &label, tx + 6.0, ty + 3.0);
                    graphics.ink = (255, 255, 255);
                }
            }
            if !notice.0.is_empty() && now - notice.1 < NOTICE_MS {
                let width = graphics.text_width(FONT_TEXT, &notice.0) + 32.0;
                graphics.panel((w - width) / 2.0, h - row - 28.0, width, row + 12.0);
                graphics.draw_text(FONT_TEXT, &notice.0, (w - width) / 2.0 + 16.0, h - row - 20.0);
            }

            sdl::SDL_SetRenderScale(renderer, 1.0, 1.0);
            // Mesure de fluidité : après le nombre d'images demandé, on donne le temps
            // moyen par image, le pilote d'affichage et le moteur de rendu, puis on quitte.
            // Les premières images chargent les sprites : on ne les compte pas.
            frames += 1;
            let instant = sdl::SDL_GetTicks();
            if frames == MEASURE_WARMUP {
                measure_start = instant;
            } else if frames > MEASURE_WARMUP {
                slowest = slowest.max(instant - previous_frame);
            }
            previous_frame = instant;
            let measured = measure > 0 && frames >= MEASURE_WARMUP + measure;
            if measured {
                let elapsed = (instant - measure_start).max(1);
                let frames = frames - MEASURE_WARMUP;
                let name = |text: *const std::ffi::c_char| match text.is_null() {
                    true => "?".to_string(),
                    false => std::ffi::CStr::from_ptr(text).to_string_lossy().into_owned(),
                };
                println!(
                    "mesure : {frames} images en {elapsed} ms, soit {:.1} images/s ({:.2} ms par image, la plus lente {slowest} ms) ; {} × {} pixels, interface × {ui}, affichage « {} », rendu « {} », {} sprites en mémoire",
                    frames as f64 * 1000.0 / elapsed as f64,
                    elapsed as f64 / frames as f64,
                    window_size.0,
                    window_size.1,
                    name(sdl::SDL_GetCurrentVideoDriver()),
                    name(sdl::SDL_GetRendererName(renderer)),
                    graphics.sprites.len()
                );
            }
            if let (Some(file), true) = (capture, script.is_empty() && (measure == 0 || measured)) {
                save_frame(renderer, file)?;
                running = false;
            }
            if measured {
                running = false;
            }
            sdl::SDL_RenderPresent(renderer);
        }
        // Les flux audio se ferment avant SDL, qui les détruirait sous eux.
        drop(music);
        drop(ambience);
        drop(voice);
        sdl::SDL_Quit();
    }
    Ok(())
}

fn in_grid((x, y): (i32, i32)) -> bool {
    (0..GRID as i32).contains(&x) && (0..GRID as i32).contains(&y)
}

/// Trace le losange d'une case.
unsafe fn outline_tile(renderer: sdl::Renderer, view: &View, world: &World, (x, y): (i32, i32), w: f32, h: f32, (r, g, b): (u8, u8, u8)) {
    let (ox, oy) = tile_origin(x, y);
    let oy = oy - LEVEL_HEIGHT * if in_grid((x, y)) { world.map.height(x as usize, y as usize) as f32 } else { 0.0 };
    let corner = |dx: f32, dy: f32| view.to_screen(ox + dx, oy + dy, w, h);
    let points = [
        corner(HALF_WIDTH, 0.0),
        corner(2.0 * HALF_WIDTH, HALF_HEIGHT),
        corner(HALF_WIDTH, 2.0 * HALF_HEIGHT),
        corner(0.0, HALF_HEIGHT),
    ];
    unsafe {
        sdl::SDL_SetRenderDrawColor(renderer, r, g, b, 255);
        for i in 0..4 {
            let (a, b) = (points[i], points[(i + 1) % 4]);
            sdl::SDL_RenderLine(renderer, a.0, a.1, b.0, b.1);
        }
    }
}

/// Relit l'image que SDL vient de composer et l'enregistre en PNG.
unsafe fn save_frame(renderer: sdl::Renderer, file: &Path) -> Result<(), String> {
    unsafe {
        let raw = sdl::SDL_RenderReadPixels(renderer, null_mut());
        if raw.is_null() {
            return Err(sdl::error("lecture de l'image rendue"));
        }
        let surface = sdl::SDL_ConvertSurface(raw, sdl::PIXELFORMAT_ABGR8888);
        sdl::SDL_DestroySurface(raw);
        if surface.is_null() {
            return Err(sdl::error("conversion de l'image rendue"));
        }
        let (w, h, pitch) = ((*surface).w as usize, (*surface).h as usize, (*surface).pitch as usize);
        let pixels = std::slice::from_raw_parts((*surface).pixels as *const u8, pitch * h);
        let rgba: Vec<u8> = pixels.chunks_exact(pitch).flat_map(|row| &row[..w * 4]).copied().collect();
        sdl::SDL_DestroySurface(surface);
        fs::write(file, png::encode_rgba(w, h, &rgba)).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod narration_tests {
    use super::*;

    #[test]
    fn each_story_finds_its_voice() {
        let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("donnees");
        if !data.join("audio/voice/campaign").exists() {
            return;
        }
        let name = |p: Option<PathBuf>| p.and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
        let jason = data.join("adventures/[les aventures de jason.pak");
        let (_, _, settings) = load_map(&jason).unwrap();
        assert_eq!(name(narration(&data, &jason, settings.as_ref(), Step::Parent(0), Moment::Opening)).as_deref(), Some("c2_e1_i.opus"));
        assert_eq!(name(narration(&data, &jason, settings.as_ref(), Step::Colony(1), Moment::Victory)).as_deref(), Some("c2_e12_v.opus"));
        assert_eq!(name(narration(&data, &jason, settings.as_ref(), Step::Parent(3), Moment::End)).as_deref(), Some("c2_v.opus"));
        let worlds = data.join("adventures/^deux mondes s'affrontent.pak");
        let (_, _, settings) = load_map(&worlds).unwrap();
        assert_eq!(name(narration(&data, &worlds, settings.as_ref(), Step::Colony(1), Moment::Victory)).as_deref(), Some("^deux mondes s'affrontent_c2_v.opus"));
        let odyssey = data.join("adventures/l'odyssée/l'odysséep.map");
        assert_eq!(name(narration(&data, &odyssey, None, Step::Parent(2), Moment::Opening)).as_deref(), Some("l'odyssée_p3_i.opus"));
    }
}
