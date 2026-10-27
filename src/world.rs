//! État de la cité et simulation : routes, bâtiments, logements, nourriture, personnages.
//!
//! La plupart des rythmes et des seuils viennent de `Zeus.exe` ou de mesures dans le
//! jeu (production, consommation, migrations, santé, impôts…). Ceux qui restent des
//! valeurs d'Olympe sont signalés au cas par cas. L'échoppe est ici un bâtiment
//! autonome, alors que le jeu la place dans une agora.

use crate::map::{City, EDGE_DRAW_TILE, FOREIGN, GRID, Map, RIVAL, Source, VASSAL};
use crate::model::HouseModel;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

/// Bits de la grille de terrain (hérités du moteur de Caesar III, vérifiés sur les cartes).
pub const TERRAIN_TREE: u32 = 0x1;
pub const TERRAIN_ROCK: u32 = 0x2;
pub const TERRAIN_WATER: u32 = 0x4;
pub const TERRAIN_BUILDING: u32 = 0x8;
pub const TERRAIN_ROAD: u32 = 0x40;
pub const TERRAIN_MEADOW: u32 = 0x80;
pub const TERRAIN_ELEVATION: u32 = 0x200;
pub const TERRAIN_RAMP: u32 = 0x400;
pub const TERRAIN_ORE: u32 = 0x800;
pub const TERRAIN_BEACH: u32 = 0x1_0000;
pub const TERRAIN_MARSH: u32 = 0x4_0000;
/// Zones de catastrophe peintes dans l'éditeur : lave, raz-de-marée et fissure de
/// tremblement de terre (Zeus.exe, 0x51eff0, choisit la catastrophe d'après ces bits
/// sous le point de départ).
pub const TERRAIN_LAVA_ZONE: u32 = 0x10;
pub const TERRAIN_FLOOD_ZONE: u32 = 0x100;
pub const TERRAIN_FISSURE: u32 = 0x200_0000;

/// Distance maximale entre une exploitation (scierie, carrière, fonderie) et sa
/// ressource (valeur d'Olympe).
const RESOURCE_REACH: i32 = 8;
/// Gisements, relevés en rapprochant les cartes des ressources que permettent leurs
/// épisodes : carrière de marbre (bit de terrain, planche `zeus_quarrytileset`),
/// rochers de cuivre de `zeus_terrain.sg3` (taches orangées), marbre noir et
/// orichalque de `poseidon_loaded.sg3`.
pub const TERRAIN_QUARRY: u32 = 0x2_0000;
const COPPER_ROCK_IMAGES: std::ops::RangeInclusive<usize> = 548..=561;
const BLACK_MARBLE_IMAGES: std::ops::RangeInclusive<usize> = 710..=869;
const ORICHALC_IMAGES: std::ops::RangeInclusive<usize> = 870..=877;

/// Ressource naturelle qu'exploite un bâtiment.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Deposit {
    Trees,
    Marble,
    Copper,
    BlackMarble,
    Orichalc,
}

/// Gisement d'où vient une ressource (numéro du jeu), s'il y en a un.
fn deposit_for(resource: u8) -> Option<Deposit> {
    match resource {
        9 => Some(Deposit::Trees),
        11 => Some(Deposit::Marble),
        10 => Some(Deposit::Copper),
        16 => Some(Deposit::BlackMarble),
        17 => Some(Deposit::Orichalc),
        _ => None,
    }
}

/// Gisement qu'exploite un bâtiment, s'il en exploite un.
fn deposit_of(function: Function) -> Option<Deposit> {
    match function {
        Function::Producer(Good::Wood) => Some(Deposit::Trees),
        Function::Producer(Good::Marble) => Some(Deposit::Marble),
        Function::Producer(Good::BlackMarble) => Some(Deposit::BlackMarble),
        Function::Producer(Good::Orichalc) => Some(Deposit::Orichalc),
        Function::Supplier(Raw::Bronze) => Some(Deposit::Copper),
        _ => None,
    }
}

/// Terrains sur lesquels on ne peut rien poser.
const BLOCKED: u32 = TERRAIN_TREE
    | TERRAIN_ROCK
    | TERRAIN_WATER
    | TERRAIN_BUILDING
    | TERRAIN_ROAD
    | TERRAIN_ELEVATION
    | TERRAIN_RAMP
    | TERRAIN_ORE
    | TERRAIN_MARSH;
/// Terrains qu'un piéton ne traverse pas.
const IMPASSABLE: u32 = TERRAIN_TREE | TERRAIN_ROCK | TERRAIN_WATER | TERRAIN_ELEVATION | TERRAIN_MARSH;

/// Murs (`zeus_general.sg3`, groupe 95) : l'image d'un mur dépend de ses huit voisins,
/// d'après la table du jeu (`Zeus.exe`, 0x636820, lue en 0x49f950 par 0x4a6920) : pour
/// chaque motif, l'état exigé de chaque voisin (0 : pas de mur, 1 : un mur, 2 : peu
/// importe), dans l'ordre nord, nord-est, est… (voir `WALL_NEIGHBOURS`), puis le
/// décalage d'image pour la vue d'origine. Le premier motif qui convient l'emporte.
const WALL_FIRST_IMAGE: usize = 777;
const WALL_PATTERNS: [([u8; 8], usize); 46] = [
    ([1, 2, 1, 2, 1, 2, 1, 2], 26),
    ([1, 2, 1, 2, 1, 2, 0, 2], 15),
    ([0, 2, 1, 2, 1, 2, 1, 2], 16),
    ([1, 2, 0, 2, 1, 2, 1, 2], 5),
    ([1, 2, 1, 2, 0, 2, 1, 2], 10),
    ([1, 2, 0, 2, 1, 2, 0, 2], 1),
    ([0, 2, 1, 2, 0, 2, 1, 2], 4),
    ([1, 2, 1, 2, 0, 0, 0, 2], 10),
    ([0, 2, 1, 2, 1, 2, 0, 0], 12),
    ([0, 0, 0, 2, 1, 2, 1, 2], 5),
    ([1, 2, 0, 0, 0, 2, 1, 2], 7),
    ([1, 2, 1, 2, 0, 1, 0, 2], 10),
    ([0, 2, 1, 2, 1, 2, 0, 1], 12),
    ([0, 1, 0, 2, 1, 2, 1, 2], 5),
    ([1, 2, 0, 1, 0, 2, 1, 2], 22),
    ([1, 2, 0, 0, 0, 0, 0, 2], 3),
    ([0, 2, 1, 2, 0, 0, 0, 0], 4),
    ([0, 0, 0, 2, 1, 2, 0, 0], 1),
    ([0, 0, 0, 0, 0, 2, 1, 2], 2),
    ([1, 2, 0, 1, 0, 0, 0, 2], 22),
    ([0, 2, 1, 2, 0, 1, 0, 0], 4),
    ([0, 0, 0, 2, 1, 2, 0, 1], 1),
    ([0, 1, 0, 0, 0, 2, 1, 2], 24),
    ([1, 2, 0, 0, 0, 1, 0, 2], 25),
    ([0, 2, 1, 2, 0, 0, 0, 1], 4),
    ([0, 1, 0, 2, 1, 2, 0, 0], 1),
    ([0, 0, 0, 1, 0, 2, 1, 2], 22),
    ([1, 2, 0, 1, 0, 1, 0, 2], 22),
    ([0, 2, 1, 2, 0, 1, 0, 1], 4),
    ([0, 1, 0, 2, 1, 2, 0, 1], 1),
    ([0, 1, 0, 1, 0, 2, 1, 2], 22),
    ([0, 1, 0, 1, 0, 1, 0, 1], 22),
    ([0, 1, 0, 1, 0, 1, 0, 0], 22),
    ([0, 0, 0, 1, 0, 1, 0, 1], 22),
    ([0, 1, 0, 0, 0, 1, 0, 1], 23),
    ([0, 1, 0, 1, 0, 0, 0, 1], 22),
    ([0, 1, 0, 0, 0, 1, 0, 0], 17),
    ([0, 0, 0, 1, 0, 0, 0, 1], 18),
    ([0, 1, 0, 1, 0, 0, 0, 0], 22),
    ([0, 0, 0, 1, 0, 1, 0, 0], 22),
    ([0, 0, 0, 0, 0, 1, 0, 1], 19),
    ([0, 1, 0, 0, 0, 0, 0, 1], 21),
    ([0, 1, 0, 0, 0, 0, 0, 0], 21),
    ([0, 0, 0, 1, 0, 0, 0, 0], 22),
    ([0, 0, 0, 0, 0, 1, 0, 0], 19),
    ([0, 0, 0, 0, 0, 0, 0, 1], 20),
];
/// Voisins dans l'ordre de la table des murs : la grille de `Zeus.exe` (nord = rang
/// précédent) a ses axes échangés par rapport à celle d'Olympe.
const WALL_NEIGHBOURS: [(i32, i32); 8] = [(-1, 0), (-1, 1), (0, 1), (1, 1), (1, 0), (1, -1), (0, -1), (-1, -1)];
/// Solidité d'un mur neuf, en coups de sape (un tous les huit pas) : la brèche est
/// faite en un mois et demi environ (barème d'Olympe).
const WALL_HEALTH: u8 = 255;

/// Première tuile de route dans le catalogue de terrain ; 18 variantes selon les bords.
const ROAD_FIRST_IMAGE: usize = 897;
/// Eau profonde, quand la carte n'en a pas à imiter.
const WATER_IMAGE: usize = 764;
/// Variante de route pour chaque combinaison de bords sans route voisine
/// (bit 0 = nord-est, 1 = sud-est, 2 = sud-ouest, 3 = nord-ouest).
const ROAD_VARIANT: [usize; 16] = [17, 14, 15, 6, 16, 1, 7, 11, 13, 5, 0, 10, 4, 9, 8, 12];
/// Monuments commémoratifs, dans l'ordre de leurs descriptions (groupe de textes 90) et
/// de leurs images : croissance, victoire, colonie, athlètes, conquête, âge d'or, héros,
/// alliance, philosophes et acteurs.
pub const COMMEMORATIVES: usize = 9;
pub const COMMEMORATIVE_GROWTH: u8 = 0;
pub const COMMEMORATIVE_VICTORY: u8 = 1;
pub const COMMEMORATIVE_ATHLETES: u8 = 3;
pub const COMMEMORATIVE_CONQUEST: u8 = 4;
pub const COMMEMORATIVE_WISDOM: u8 = 8;
pub const COMMEMORATIVE_HERO: u8 = 6;
pub const COMMEMORATIVE_ALLY: u8 = 7;
/// Paliers de population qui, atteints pour la première fois, valent un monument
/// (annonces `population_1000` à `population_25000` des messages du jeu).
const POPULATION_MILESTONES: [i32; 7] = [1000, 2000, 5000, 10000, 15000, 20000, 25000];
/// Jeux panhelléniques (Zeus.exe, objet 0x1082b78) : chaque année les siens, selon
/// `(année + (mois > 7)) & 3` (0x47b6e0) : Olympiques, Isthmiques (philosophes),
/// Néméens (athlètes), Pythiques (acteurs). Ils commencent au sixième mois et se
/// concluent au huitième (événements 0x23 et 0x25 posés par 0x5e7ca0).
pub const GAMES_OLYMPIAN: u8 = 0;
pub const GAMES_ISTHMIAN: u8 = 1;
pub const GAMES_NEMEAN: u8 = 2;
pub const GAMES_PYTHIAN: u8 = 3;
const GAMES_BEGIN_MONTH: u64 = 5;
const GAMES_END_MONTH: u64 = 7;
/// Score qui assure au moins la deuxième place (0x5e81ce).
const GAMES_SECOND: i32 = 30;
/// Bonus d'un bâtiment unique achevé (0x54be4f sur l'objet 0xf920d4), que nous
/// n'avons pas identifié : l'hippodrome fermé, par hypothèse.
const GAMES_BONUS: i32 = 20;

/// Nouvelles des jeux, pour prévenir le joueur.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GamesNews {
    /// Les jeux commencent ; `home` : ce sont ceux de la cité.
    Begin { kind: u8, home: bool },
    /// La cité n'a personne à envoyer.
    CantParticipate(u8),
    /// La cité devait accueillir les jeux, mais elle n'a pas de stade.
    HomeCancelled,
    /// Fin des jeux : 2 gagnés, 1 deuxièmes, 0 perdus.
    Concluded { kind: u8, home: bool, place: u8 },
}

/// Orientation de la vue (0, 2, 4 ou 6 comme dans le jeu) : case de la carte → case
/// à l'écran. La case porteuse d'un objet sur plusieurs cases reste la plus à gauche
/// à l'écran, comme dans `Zeus.exe` (0x4a04b0).
pub fn rotate(orientation: u8, x: i32, y: i32) -> (i32, i32) {
    let last = GRID as i32 - 1;
    match orientation & 6 {
        2 => (y, last - x),
        4 => (last - x, last - y),
        6 => (last - y, x),
        _ => (x, y),
    }
}

/// Case à l'écran → case de la carte.
pub fn unrotate(orientation: u8, x: i32, y: i32) -> (i32, i32) {
    let last = GRID as i32 - 1;
    match orientation & 6 {
        2 => (last - y, x),
        4 => (last - x, last - y),
        6 => (y, last - x),
        _ => (x, y),
    }
}

/// Position continue (personnages) dans le repère de l'écran.
pub fn rotate_point(orientation: u8, x: f32, y: f32) -> (f32, f32) {
    let last = GRID as f32 - 1.0;
    match orientation & 6 {
        2 => (y, last - x),
        4 => (last - x, last - y),
        6 => (last - y, x),
        _ => (x, y),
    }
}

/// Voisines dans l'ordre des bits ci-dessus.
const NEIGHBOURS: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];
/// Les huit directions de marche, dans l'ordre des images de personnage.
const DIRECTIONS: [(i32, i32); 8] = [(0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)];

/// Image de chaque niveau de logement modeste dans `zeus_general.sg3`.
const HOUSE_IMAGES: [usize; 7] = [1006, 1008, 1010, 1012, 1014, 1016, 1018];
/// Logements luxueux : leurs niveaux suivent les sept niveaux modestes dans les règles
/// (terrain vacant, délabré, résidence, hôtel particulier, manoir, domaine).
pub const ELITE_FIRST_LEVEL: usize = 7;
const ELITE_LEVELS: usize = 6;
const ELITE_DEVOLVED: usize = ELITE_FIRST_LEVEL + 1;
/// Chaque niveau a cinq morceaux 2 × 2 dans `zeus_general.sg3` : fond, droite, gauche,
/// puis deux variantes du morceau de devant (écurie, ou cour à portail). Disposition
/// établie en assemblant les images ; la cour sert ici, faute de chevaux.
const ELITE_FIRST_IMAGE: usize = 1021;
const ELITE_PIECES: usize = 5;
const ELITE_QUADRANT_PIECE: [usize; 4] = [0, 1, 2, 4];
/// Côté d'un logement luxueux, en cases.
const ELITE_SIZE: usize = 4;
/// Une maison doit avoir une route à cette distance au plus pour être habitée.
const ROAD_ACCESS_RANGE: i32 = 2;

/// Calendrier du jeu d'origine, relevé dans ses sauvegardes (blocs 37, 38 et 6 : pas
/// du jour de 0 à 49, jour de 0 à 15, mois de 0 à 11).
pub const TICKS_PER_DAY: u64 = 50;
const DAYS_PER_MONTH: u64 = 16;
pub const TICKS_PER_MONTH: u64 = TICKS_PER_DAY * DAYS_PER_MONTH;
/// Images de marche d'un personnage, par direction.
const WALK_FRAMES: usize = 12;
/// Pas de simulation pour parcourir une case : les personnages des sauvegardes
/// d'origine avancent de 0 à 14 sur leur case (octet 57 de leur enregistrement).
const WALK_TICKS: usize = 15;
/// Migrations (`Zeus.exe`, 0x567450, chaque jour) : taux selon l'humeur, de 100 au-delà
/// de 70 à -25 jusqu'à 15 ; un jour amène taux×12/100 personnes (arrondi au-dessus), ou en
/// fait partir autant si le taux est négatif. Les arrivants vont par groupes de quatre,
/// les partants aussi, quatre au plus par maison.
const MIGRATION_RATES: [(i32, i32); 6] = [(71, 100), (61, 75), (50, 50), (36, 0), (26, -8), (16, -17)];
const MIGRATION_LOW_RATE: i32 = -25;
const MIGRANTS_PER_DAY: i32 = 12;
const IMMIGRANT_GROUP: i32 = 4;
/// Une vague d'arrivées suspend les départs deux jours, et inversement.
const MIGRATION_PAUSE: u8 = 2;
/// Personne ne part d'une cité de 100 habitants ou moins, personne n'arrive au-delà de
/// 200 000 habitants ni après plus de trois ans de dettes.
const MIN_EMIGRATION_POPULATION: i32 = 100;
const MAX_POPULATION: i32 = 200_000;
const MAX_DEBT_YEARS: i32 = 3;
/// Les arrivants vont d'abord dans les maisons vides, puis dans celles qui ont au moins
/// huit places libres (`Zeus.exe`, 0x486860).
const ROOMY_HOUSE: i32 = 8;
/// Un bâtiment n'a qu'un marcheur dehors à la fois : les sauvegardes d'origine comptent
/// un surintendant par maintenance, un guetteur par poste de garde, un percepteur par
/// hôtel des impôts. Repos entre son retour et le départ suivant : quatre jours, d'après
/// le délai de Caesar III à plein effectif (non vérifié dans Zeus).
const ROUNDS_INTERVAL: u64 = TICKS_PER_DAY * 4;
/// Valeur de `next_walk` tant que le marcheur du bâtiment est dehors.
const WALKER_OUT: u64 = u64::MAX / 4;
/// Portée d'une tournée, en cases, relevée sur les marcheurs des sauvegardes
/// d'origine (octet 71 de leur enregistrement au moment du demi-tour).
fn rounds_length(rounds: &Rounds) -> u32 {
    match rounds {
        Rounds::Service(Service::Water | Service::Health) => 26,
        Rounds::Service(Service::Tax | Service::Philosophy | Service::Athletics | Service::Drama) => 35,
        Rounds::Service(Service::Maintenance | Service::Security) | Rounds::Goods { .. } => 43,
    }
}
/// Distance à laquelle un marcheur dessert les maisons.
const SERVICE_RANGE: i32 = 2;
/// Durée pendant laquelle une maison reste desservie après le passage d'un marcheur.
/// Dans les sauvegardes d'origine, les compteurs de service des maisons plafonnent à
/// 96 (octets 109 à 120 de leur enregistrement) ; à raison d'un point perdu par jour,
/// comme dans Caesar III dont le moteur descend, cela fait 96 jours, soit six mois.
const SERVICE_DURATION: u64 = TICKS_PER_DAY * 96;

/// Stades de pousse d'un champ, et durée d'une récolte.
const CROP_STAGES: u64 = 6;
const HARVEST_TICKS: u64 = TICKS_PER_MONTH * 3;
/// Chargements produits par récolte, et capacités de stockage.
const HARVEST_LOADS: i32 = 12;
const FARM_STORAGE: i32 = 12;
const GRANARY_STORAGE: i32 = 32;
const VENDOR_STORAGE: i32 = 16;
/// Chargements qu'emporte une acheteuse. Le marchand ambulant, lui, part avec tout le
/// stock de l'échoppe et rapporte le reste : comme la marchande de Caesar III, dont
/// descend le moteur, il puise dans la réserve et non dans une charge fixe ; avec huit
/// chargements, il ne nourrissait que trois maisons d'immeubles par tournée.
const VENDOR_LOAD: i32 = 8;
/// Chargement d'un porteur ou d'une acheteuse.
const CARRIER_LOAD: i32 = 4;
/// Une maison presque vide fait quand même des réserves pour ce nombre d'habitants.
const MIN_HOUSEHOLD: i32 = 8;
/// Réserve de nourriture maximale d'une maison, en mois de consommation.
const HOUSE_FOOD_MONTHS: i32 = 4;
/// Un chargement nourrit ce nombre d'habitants pendant un mois.
const PEOPLE_PER_LOAD: i32 = 64;
/// Soldats que fournit un logement plein, par niveau (annexe 4 de l'éditeur
/// d'aventures) : piétaille des logements modestes à partir de la masure, hoplites
/// des hôtels particuliers et manoirs, cavaliers des domaines.
const SOLDIERS: [(Soldier, i32); 13] = [
    (Soldier::Rabble, 0),
    (Soldier::Rabble, 0),
    (Soldier::Rabble, 5),
    (Soldier::Rabble, 6),
    (Soldier::Rabble, 10),
    (Soldier::Rabble, 12),
    (Soldier::Rabble, 15),
    (Soldier::Rabble, 0),
    (Soldier::Rabble, 0),
    (Soldier::Rabble, 0),
    (Soldier::Hoplite, 2),
    (Soldier::Hoplite, 4),
    (Soldier::Horseman, 4),
];

/// Soldats, du moins au plus aguerri.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Soldier {
    Rabble,
    Hoplite,
    Horseman,
}

/// Changements du monde qu'Olympe sait jouer : sous-types des événements du jeu, dans
/// l'ordre de leurs noms dans les textes de l'éditeur (le commerce cesse ou s'ouvre,
/// une cité redevient alliée, devient rivale ou vassale, une vassale se rebelle,
/// une armée se renforce ou s'affaiblit, une cité devient active ou inactive,
/// apparaît ou disparaît).
pub const TRADE_SHUTS: u8 = 2;
pub const TRADE_OPENS: u8 = 3;
pub const BECOMES_ALLY: u8 = 9;
pub const BECOMES_RIVAL: u8 = 10;
pub const BECOMES_VASSAL: u8 = 11;
pub const VASSAL_REBELS: u8 = 12;
pub const MILITARY_BUILDUP: u8 = 14;
pub const MILITARY_DECLINE: u8 = 15;
pub const BECOMES_ACTIVE: u8 = 18;
pub const BECOMES_INACTIVE: u8 = 19;
pub const APPEARS: u8 = 20;
pub const DISAPPEARS: u8 = 21;
/// Nature des événements programmés qui sont des invasions armées, et des cadeaux
/// (`GIFT` + ressource).
pub const INVASION: u8 = 99;
pub const GIFT: u8 = 130;
/// Changements du commerce programmés : `TRADE_CHANGE` + 0 à 5 (demande, offre, prix ;
/// hausse puis baisse).
pub const TRADE_CHANGE: u8 = 200;
/// Hausse et baisse des salaires, et départ en campagne d'une armée rivale.
pub const WAGE_RISE: u8 = 96;
pub const WAGE_FALL: u8 = 97;
pub const ARMY_AWAY: u8 = 98;
/// Catastrophes programmées : `cities` y désigne le point de catastrophe de la carte.
pub const EARTHQUAKE: u8 = 90;
pub const LAVA: u8 = 91;
pub const TIDAL_WAVE: u8 = 92;
/// Raz-de-marée dont les eaux ne se retirent pas.
pub const FLOOD: u8 = 93;
/// Glissement de terrain : les éboulis couvrent le pied de la pente.
pub const LANDSLIDE: u8 = 94;
/// Portée d'un glissement autour de son point, et distance au relief des cases
/// qu'il recouvre (valeurs d'Olympe).
const LANDSLIDE_REACH: i32 = 4;
const LANDSLIDE_SLOPE: i32 = 2;
/// Morceaux de fissure posés sur le sol, selon les côtés par où la fissure se
/// prolonge (bits dans l'ordre de `NEIGHBOURS` : nord-est, sud-est, sud-ouest,
/// nord-ouest), relevés sur les images 1139 à 1178 du terrain ; plusieurs variantes
/// par forme.
const CHASM_PIECES: [&[usize]; 16] = [
    &[1178],
    &[1143, 1165, 1172],
    &[1144],
    &[1149, 1158, 1163, 1174],
    &[1145, 1166, 1171],
    &[1141, 1142, 1154, 1156],
    &[1150, 1159, 1168, 1175],
    &[1167],
    &[1146, 1169],
    &[1157, 1177],
    &[1139, 1140, 1153],
    &[1164],
    &[1160, 1176],
    &[1151, 1152, 1173],
    &[1155, 1170],
    &[1161],
];
const CHASM_VARIANTS: usize = 12;
/// Lave refroidie.
const LAVA_FIRST_IMAGE: usize = 1101;
const LAVA_IMAGES: usize = 6;
/// Les eaux d'un raz-de-marée se retirent au bout de six mois (durée d'Olympe).
const FLOOD_TICKS: u64 = 6 * TICKS_PER_MONTH;
/// Distance, en cases, à laquelle une zone peinte est rattachée à son point de départ.
const DISASTER_REACH: i32 = 3;
/// Salaire normal du jeu : 30 drachmes par an et par dix ouvriers, auquel les
/// événements ajoutent ou retranchent.
const NORMAL_WAGE: i32 = 30;

/// Batailles : les forces, attaques, armures et portées viennent des règles des
/// personnages ; Olympe fixe le rythme des coups (un tous les tant de pas), la
/// distance du corps à corps (en cases) et le nombre de soldats de chaque camp.
const STRIKE_TICKS: u64 = 8;
const MELEE_REACH: f32 = 1.3;
const MAX_SOLDIERS: i32 = 64;
/// Animation d'un personnage dans son catalogue : images de marche par direction,
/// premier rang des images de combat, et leur nombre par direction.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Anim {
    pub walk: usize,
    pub fight: usize,
    pub fight_frames: usize,
}

/// Marche de douze images, mort (8), combat de huit images : hoplites de la cité
/// (`sprmain.sg3`), monstres et autres personnages.
pub const COMMON_ANIM: Anim = Anim { walk: WALK_FRAMES, fight: 104, fight_frames: 8 };
/// Unité d'une armée ennemie dans son catalogue : premier rang des images de marche,
/// nombre d'images de marche par direction, premier rang et nombre des images de
/// combat. Relevées sur les groupes des catalogues (marche, combat, mort).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Unit {
    pub walk: usize,
    pub walk_frames: usize,
    pub fight: usize,
    pub fight_frames: usize,
    /// Catégorie de l'unité dans les règles des personnages (`figure_model`) :
    /// infanterie, tireurs ou cavalerie, reconnue à ses images.
    pub kind: usize,
}

const INFANTRY: usize = 0;
const MISSILE: usize = 1;
const CAVALRY: usize = 2;
/// Unités par nation dans les règles des personnages (infanterie, tireurs,
/// cavalerie, navire de guerre, transport, réserve).
const FIGHTERS_PER_NATION: usize = 6;

const fn unit(walk: usize, walk_frames: usize, fight: usize, fight_frames: usize, kind: usize) -> Unit {
    Unit { walk, walk_frames, fight, fight_frames, kind }
}

/// Combattant de secours quand les règles des personnages manquent.
const FALLBACK_FIGHTER: crate::model::Fighter =
    crate::model::Fighter { health: 150, attack: 15, armor: 6, missile_armor: 2, missile_attack: 0, missile_range: 0, rate: 0, speed: 6, frequency: 0 };

/// Unités de chaque nation (ordre de l'éditeur) : grecs, troyens, perses, centaures,
/// amazones, égyptiens, mayas, phéniciens, océanides, atlantes.
pub const ARMY_UNITS: [&[Unit]; 10] = [
    &[unit(1, 12, 105, 8, INFANTRY), unit(169, 12, 273, 12, CAVALRY), unit(369, 12, 473, 12, MISSILE)],
    &[unit(1, 12, 105, 10, INFANTRY), unit(185, 12, 289, 10, MISSILE), unit(369, 12, 473, 12, CAVALRY)],
    &[unit(1, 15, 121, 15, INFANTRY), unit(257, 15, 377, 18, MISSILE), unit(532, 15, 652, 15, CAVALRY)],
    &[unit(1, 18, 145, 15, INFANTRY), unit(280, 17, 416, 15, MISSILE)],
    &[unit(1, 15, 121, 15, INFANTRY), unit(261, 15, 381, 18, MISSILE)],
    &[unit(1, 12, 105, 12, INFANTRY), unit(201, 12, 297, 9, CAVALRY), unit(369, 12, 465, 12, MISSILE)],
    &[unit(1, 12, 105, 12, INFANTRY), unit(201, 12, 305, 12, MISSILE)],
    &[unit(1, 12, 105, 12, CAVALRY), unit(201, 12, 305, 12, MISSILE)],
    &[unit(1, 12, 105, 12, INFANTRY), unit(201, 12, 305, 8, MISSILE)],
    &[unit(1, 12, 97, 9, CAVALRY), unit(169, 12, 265, 12, INFANTRY), unit(369, 15, 489, 12, MISSILE)],
];

/// Unité d'un soldat ennemi : `troop` regroupe la nation (seize par nation) et le
/// rang de l'unité.
fn army_unit(troop: u8) -> Unit {
    let units = ARMY_UNITS.get((troop / 16) as usize).copied().unwrap_or(ARMY_UNITS[0]);
    units.get((troop % 16) as usize).copied().unwrap_or(units[0])
}

const SOLDIER_FIGHT_FRAME_TICKS: usize = 4;
/// Un défenseur cherche à nouveau l'ennemi le plus proche tous les tant de pas ; un
/// envahisseur charge les défenseurs qu'il voit à moins de tant de cases.
const CHASE_TICKS: u64 = 20;
const ENEMY_SIGHT: f32 = 8.0;
/// Un raid repart après avoir détruit un bâtiment par tranche de huit soldats, deux
/// au moins (Olympe).
const SOLDIERS_PER_RUIN: i32 = 8;
const MIN_RUINS: i32 = 2;

/// Invasion en cours : la cité rivale, la taille de son armée, ce qu'elle a déjà
/// détruit et ce qu'elle détruira au plus avant de repartir avec son butin.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Raid {
    pub city: usize,
    pub soldiers: i32,
    pub buildings: i32,
    pub limit: i32,
}

/// Issue d'une invasion armée.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Battle {
    pub city: usize,
    pub soldiers: i32,
    pub repelled: bool,
    /// Pertes en cas de défaite : bâtiments détruits et drachmes pillées.
    pub buildings: i32,
    pub drachmas: i32,
}

/// Demande d'une cité : la ressource (23 drachmes, 24 troupes, 25 vivres, sinon une
/// marchandise), la quantité, et le pas de simulation où elle expire.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Request {
    pub city: usize,
    pub resource: u8,
    pub amount: i32,
    pub until: u64,
}

pub const DRACHMAS: u8 = 23;
pub const TROOPS: u8 = 24;
pub const ANY_FOOD: u8 = 25;
/// Faveur gagnée en satisfaisant une demande, perdue en l'ignorant ; en dessous du
/// seuil, une alliée devient rivale et une vassale se rebelle (barème d'Olympe).
const FAVOR_FOR_SERVICE: i32 = 15;
const FAVOR_FOR_NEGLECT: i32 = 20;
const FAVOR_THRESHOLD: i32 = 20;
/// Délai laissé pour satisfaire une demande (valeur d'Olympe).
const REQUEST_TICKS: u64 = TICKS_PER_MONTH * 12;
/// Nature des événements programmés qui sont des demandes.
pub const REQUEST: u8 = 100;

/// Nouvelles des demandes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RequestNews {
    Arrived(Request),
    Expired(Request),
}

/// Changement du monde à venir : quand, quoi, quelles cités, combien. Une demande
/// y figure avec `change` = `REQUEST + ressource`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Scheduled {
    pub at: u64,
    pub change: u8,
    pub cities: (i16, i16),
    pub amount: (i16, i16),
    /// Marchandise visée, pour un changement du commerce.
    pub resource: u8,
}

/// Pourquoi une campagne ne part pas.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Conquest {
    NotForeign,
    AlreadyRuled,
    Away,
    NoArmy,
    /// La compagnie demandée dépasse les soldats ou les navires présents.
    TooMany,
}

/// Campagne lointaine de l'armée : conquête, ou raid (`raid`) pour un butin. Les
/// soldats partis (piétaille, hoplites, cavaliers) manquent à la cité jusqu'au retour.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Expedition {
    pub city: usize,
    pub raid: bool,
    pub soldiers: [i32; 3],
    /// Trières parties avec l'armée ; leur quai les attend (stock −1).
    pub triremes: i32,
    /// Pas de l'arrivée devant la cité, puis du retour une fois la bataille livrée.
    pub arrives: u64,
    pub returns: u64,
    /// Issue de la bataille, une fois livrée.
    pub won: Option<bool>,
    /// Butin d'un raid victorieux : ressource (23 pour des drachmes) et quantité.
    pub loot: (u8, i32),
}

/// Nouvelles des campagnes lointaines, pour prévenir le joueur.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CampaignNews {
    Conquered(usize),
    ConquestFailed(usize),
    /// Raid anéanti : personne ne revient.
    WipedOut(usize),
    /// Retour de l'armée, victorieuse ou non.
    Returned { city: usize, won: bool },
    /// Butin rapporté d'un raid : ressource et quantité rangée.
    Plunder { city: usize, resource: u8, amount: i32 },
}

/// Défense d'une cité du monde (`Zeus.exe`, 0x42d980) : racine des années écoulées
/// (trente au plus), par un barème de difficulté, par son niveau militaire. Elle
/// remonte d'un point par mois jusque-là (0x42d540) après une bataille.
const DEFENCE_BY_DIFFICULTY: [f64; 5] = [3.0, 4.0, 4.5, 5.0, 6.0];
const DEFENCE_YEARS: f64 = 30.0;
/// Trajet (0x4625a0) : longueur de la route par son allure, sur 200, en mois, deux au
/// moins.
const PACE_DIVISOR: i32 = 200;
const MIN_TRAVEL_MONTHS: i32 = 2;
/// Butin d'un raid (0x42d380) : la quantité annuelle d'une marchandise que la cité
/// échange, divisée par un barème de difficulté, dans la limite de ce que portent les
/// soldats (0x42d2b0 : un hoplite 50, un cavalier 100, la piétaille rien) ; ou des
/// drachmes : (richesse + 1) × barème × 200 (0x42d870).
const PLUNDER_DIVISOR: [f64; 5] = [0.5, 0.66, 0.75, 1.0, 1.5];
const PLUNDER_DRACHMAS: [f64; 5] = [5.0, 4.0, 3.0, 2.5, 2.0];
const PLUNDER_DRACHMAS_UNIT: f64 = 200.0;
const CARRY: [i32; 3] = [0, 50, 100];
const TRIREME_CARRY: i32 = 600;
/// Un char demande une charge de bois et une de chevaux (valeurs d'Olympe).
const CHARIOT_WOOD: i32 = 1;
const CHARIOT_HORSES: i32 = 1;
/// Une trière compte pour six hommes dans une bataille au loin (0x4dea00).
const TRIREME_FORCE: i32 = 6;
/// Une marchandise pillée marque la cité de 15 ; au-delà de 40, elle ne peut plus
/// l'être ; la marque baisse d'un point par mois.
const PLUNDER_MARK: i32 = 15;
const PLUNDER_LIMIT: i32 = 40;
/// Pertes d'une bataille selon l'écart des forces en pour cent (0x461540) : moins de
/// 10, 25, 33, 50, 66, au plus 74, au-delà rien.
const CONQUEST_LOSSES: [i32; 6] = [70, 50, 33, 25, 20, 10];
const RAID_LOSSES: [i32; 6] = [25, 18, 12, 9, 6, 3];
/// Pertes de l'armée vaincue lors d'une conquête, et pertes infligées par une armée
/// victorieuse.
const CONQUEST_DEFEAT: i32 = 80;
const CONQUEST_BLOW: i32 = 50;
const RAID_BLOW: i32 = 25;

/// Perte selon l'écart des forces, en pour cent.
fn losses(margin: i32, table: [i32; 6]) -> i32 {
    match margin {
        ..10 => table[0],
        ..25 => table[1],
        ..33 => table[2],
        ..50 => table[3],
        ..66 => table[4],
        ..=74 => table[5],
        _ => 0,
    }
}

/// Pertes d'un raid repoussé, selon le rapport des forces (défense moins attaque, sur
/// l'attaque).
fn raid_defeat(ratio: i32) -> i32 {
    match ratio {
        ..1 => 30,
        ..3 => 40,
        ..5 => 50,
        ..7 => 60,
        ..9 => 70,
        ..=10 => 85,
        _ => 100,
    }
}
/// Genre d'une colonie dans la fiche d'une cité.
const COLONY_KIND: u8 = 1;
/// Tribut annuel d'une vassale dont la carte ne fixe pas le montant : une part fixe
/// et une part par bouclier (Olympe).
const TRIBUTE_BASE: i32 = 250;
const TRIBUTE_PER_SHIELD: i32 = 50;

/// Palier de demande ou d'offre d'une cité, en unités par an (les niveaux du jeu).
const TRADE_STEP: i32 = 12;

/// Réserve que le commerce garde ou cherche à atteindre, par marchandise (Olympe).
const TRADE_RESERVE: i32 = 8;

/// Prix d'une marchandise quand la carte n'en donne pas (valeurs d'Olympe) ; les
/// cartes portent ceux de l'éditeur (`Map::prices`).
fn price(resource: usize) -> i32 {
    match resource {
        1..=8 => 8,
        12 | 13 => 10,
        9 => 20,
        11 => 30,
        10 | 14 => 35,
        20 => 40,
        21 => 50,
        18 => 80,
        15 => 100,
        19 => 200,
        _ => 30,
    }
}

const CAMPAIGN_TICKS: u64 = TICKS_PER_MONTH * 6;

/// Monstre sans cible, et repos d'un monstre entre deux destructions (Olympe). Le
/// Kraken et Scylla, monstres marins, ne marchent pas dans la cité.
const NO_TARGET: u16 = u16::MAX;
const MONSTER_REST_TICKS: u64 = TICKS_PER_MONTH * 2;
const SEA_MONSTERS: [u8; 2] = [1, 10];
/// Un dieu hostile reste six mois, 96 jours comme dans le jeu (`Zeus.exe`, séjour posé
/// en 0x4ec15e…), et détruit un bâtiment par mois (rythme d'Olympe).
const GOD_STAY_TICKS: u64 = TICKS_PER_MONTH * 6;
const GOD_REST_TICKS: u64 = TICKS_PER_MONTH;
/// Séjour d'un dieu hostile quand un dieu favorable protège la cité (Olympe).
const GOD_REPELLED_TICKS: u64 = TICKS_PER_MONTH;
/// Dieux qui défendent la cité contre un dieu hostile (`Zeus.exe`, 0x482e0b), dans
/// l'ordre où le jeu les essaie, s'ils ont leur sanctuaire achevé : Zeus contre tous,
/// Aphrodite contre Arès, Héphaïstos, Hermès et Dionysos, Héra contre Zeus.
const GOD_DEFENDERS: [(u8, &[u8]); 3] = [(0, &[]), (6, &[5, 9, 7, 10]), (12, &[0])];

/// Chemin en ligne droite, case par case (diagonales permises), rangé à l'envers.
fn straight_path(from: (u16, u16), to: (u16, u16)) -> Vec<(u16, u16)> {
    let mut path = Vec::new();
    let (mut x, mut y) = (from.0 as i32, from.1 as i32);
    while (x, y) != (to.0 as i32, to.1 as i32) {
        x += (to.0 as i32 - x).signum();
        y += (to.1 as i32 - y).signum();
        path.push((x as u16, y as u16));
    }
    path.reverse();
    path
}

/// Durée d'un combat de héros contre un monstre, et d'une quête (valeurs d'Olympe).
const HERO_FIGHT_TICKS: u64 = TICKS_PER_MONTH;
const QUEST_TICKS: u64 = TICKS_PER_MONTH * 6;
/// Une cité n'a jamais plus de quatre sanctuaires à la fois.
pub const MAX_SANCTUARIES: usize = 4;
/// Marchandises qu'on livre à un sanctuaire en chantier, dans l'ordre de ses besoins.
pub const SANCTUARY_MATERIALS: [Good; 3] = [Good::Marble, Good::Wood, Good::Sculpture];
/// Images des monuments dans `poseidon_loaded.sg3`, relevées en recomposant les
/// dispositions du jeu : pentes de marbre selon leurs côtés extérieurs (bits de
/// `NEIGHBOURS` : nord-est, sud-est, sud-ouest, nord-ouest), angles compris ;
/// faces décorées (marches, triangle, cercle) au sud-ouest et au sud-est ; bloc
/// plein pour les formes rares ; dalle, obélisque, sol du chantier et aire d'accès.
const MONUMENT_SLOPES: [(u8, usize); 8] = [
    (0b0100, 635),
    (0b0010, 630),
    (0b1000, 627),
    (0b0001, 621),
    (0b0110, 624),
    (0b0011, 622),
    (0b1100, 626),
    (0b1001, 620),
];
const MONUMENT_DECORATIONS: [(u16, u8, usize); 6] = [(118, 0b0100, 636), (118, 0b0010, 629), (120, 0b0100, 634), (120, 0b0010, 631), (121, 0b0100, 633), (121, 0b0010, 632)];
const MONUMENT_SOLID_IMAGE: usize = 657;
const MONUMENT_LEVEL_PIXELS: u16 = 30;
const MONUMENT_STATUE_IMAGE: usize = 628;
const MONUMENT_TILE_IMAGE: usize = 658;
const MONUMENT_FLOOR_IMAGE: usize = 663;
const MONUMENT_PAD_IMAGE: usize = 664;
/// Images de `zeus_general.sg3` qu'un sanctuaire montre dès son chantier : dalles
/// décorées et dallage uni ; le reste (autel, statues) n'apparaît qu'une fois achevé.
const SANCTUARY_FLOOR_IMAGES: std::ops::RangeInclusive<usize> = 1447..=1458;
/// Dallage uni (six variantes) qui tient leur place pendant le chantier.
const SANCTUARY_PLAIN_IMAGE: usize = 1453;
/// Production (`Zeus.exe`, 0x485360 et 0x48cfb0) : la progression d'un bâtiment au
/// complet monte de 10 par jour ; un producteur livre 100 unités (un chargement de
/// marchandise) à 160, soit tous les 16 jours, et un atelier, qui consomme 100 unités
/// de matière (400 de bronze pour une sculpture), à 320. Chaque chargement part
/// aussitôt en charrette.
const PRODUCTION_TICKS: u64 = TICKS_PER_MONTH;
const WORKSHOP_TICKS: u64 = 2 * TICKS_PER_MONTH;
const SCULPTURE_BRONZE: i32 = 4;
const GOODS_CART: i32 = 1;
const PRODUCER_STORAGE: i32 = 8;
/// Une pêcherie livre 100 unités par sortie de son bateau, soit six chargements de
/// vivres d'Olympe (de 16 unités : un chargement nourrit 64 habitants un mois, et le jeu
/// donne 25 unités pour 100 habitants par mois). Mesuré dans le jeu : le bateau parcourt
/// environ 33 cases en 9 jours jusqu'au banc le plus proche, pêche 8 jours (deux tiers
/// de ce temps avec Poséidon, `Zeus.exe` 0x4b624c), revient et décharge en 4 jours. Sans
/// banc sur la carte, une sortie dure 20 jours (Olympe).
const FISHERY_TICKS: u64 = 20 * TICKS_PER_DAY;
const FOOD_CART_LOADS: i32 = 6;
const BOAT_TILES: u64 = 33;
const BOAT_DAYS: u64 = 9;
const FISHING_TICKS: u64 = 8 * TICKS_PER_DAY;
const UNLOADING_TICKS: u64 = 4 * TICKS_PER_DAY;
/// Quai des navires (`Zeus.exe`, 0x4901ea) : le chantier commence quand le quai a 200
/// unités de bois et 200 d'armement (deux charges de chaque) ; sa progression gagne 10
/// par jour à effectif complet et la trière est lancée à 320, soit au bout de 32 jours,
/// en consommant ces matériaux.
const TRIREME_WOOD: i32 = 2;
const TRIREME_ARMS: i32 = 2;
/// Sections de l'hippodrome (`poseidon_loaded.sg3`) selon les côtés où la piste se
/// prolonge (bits de `NEIGHBOURS`) : droites le long de x et de y (deux variantes,
/// stalles de départ ou tribunes), virages ; relevées sur les images 422 à 429.
pub const RACETRACK_SIZE: usize = 4;
const RACETRACK_ALONG_X: [usize; 2] = [426, 422];
const RACETRACK_ALONG_Y: [usize; 2] = [424, 428];
const RACETRACK_CURVES: [(u8, usize); 4] = [(0b1100, 423), (0b1001, 425), (0b0011, 427), (0b0110, 429)];
/// Recette mensuelle d'un hippodrome achevé, par section (barème d'Olympe).
const RACETRACK_INCOME: i32 = 10;
const TRIREME_TICKS: u64 = 32 * TICKS_PER_DAY;
/// Laiterie et pavillon de chasse, mesurés dans le jeu (troupeau d'une quinzaine de
/// chèvres, sangliers à proximité) : 100 unités, soit une charrette de six chargements,
/// environ tous les 10 jours (9 pour la chasse). Olympe ne fait pas paraître les bêtes.
const DAIRY_TICKS: u64 = 10 * TICKS_PER_DAY;
/// Matière première qu'un atelier accepte d'avance.
const WORKSHOP_RAW_STORAGE: i32 = 8;
/// Réserve de laine, d'huile ou de vin qu'une maison cherche à garder (200 unités
/// du jeu), et mois que dure un chargement : une maison en consomme une unité aux
/// jours 0 et 7 de chaque mois (`Zeus.exe`, 0x484c50), soit 100 unités en 50 mois.
/// L'armement ne s'use pas.
const HOUSE_GOODS_RESERVE: i32 = 2;
const GOODS_MONTHS: u64 = 50;
/// L'attrait ne rayonne jamais plus loin, quelles que soient les règles.
const APPEAL_MAX_RANGE: i32 = 6;
const APPEAL_LIMIT: i32 = 100;
/// Risques d'incendie et d'effondrement (`Zeus.exe`, 0x492260, chaque jour) : le risque
/// d'effondrement monte chaque jour de la valeur des règles (colonne 7), sauf pour les
/// huttes et les baraques ; celui d'incendie, un jour sur huit en moyenne, de la valeur
/// des règles (colonne 6) multipliée par un nombre tiré de 1 à 5, sauf dans une maison
/// vide. Le sinistre survient à 1 000. La maintenance remet les deux à zéro.
const RISK_LIMIT: i32 = 1000;
const RISK_DAILY_CHANCE: u32 = 8;
const FIRE_DRAW: u32 = 5;
const STURDY_HOUSE_LEVELS: usize = 2;
/// Criminalité (`Zeus.exe`) : humeur à laquelle elle ne bouge que de l'incrément du
/// logement, baisse au passage d'un garde (réducteur de risque des règles), et seuil
/// au-delà duquel la maison « va bientôt produire un criminel ». Un voleur qui ne trouve
/// pas de maison luxueuse à piller emporte deux drachmes par habitant de sa maison
/// (Olympe).
const CRIME_NEUTRAL_SENTIMENT: i32 = 40;
const GUARD_CRIME_REDUCTION: i32 = 50;
const CRIMINAL_CRIME: i32 = 80;
const THEFT_PER_PERSON: i32 = 2;
/// Bienfaits permanents de deux sanctuaires (`Zeus.exe`, objets des dieux en 0xef5dd8,
/// 0x34 octets par dieu) : celui de Dionysos fait baisser la criminalité de 20 à chaque
/// mise à jour au lieu de la faire monter (0x566f20), celui d'Aphrodite retient tous
/// les habitants (0x486ab0). Le jeu les accorde dès la première étape du chantier ;
/// Olympe, qui ne suit pas les étapes, attend son achèvement.
const DIONYSUS: u8 = 10;
const DIONYSUS_CRIME_RELIEF: i32 = 20;
const APHRODITE: u8 = 6;
/// Autres bienfaits permanents : Héphaïstos ôte tout risque d'incendie (règles des
/// bâtiments, colonne 6, 0x433d50) ; Aphrodite abaisse de 10 l'attrait que demandent
/// les logements pour ne pas régresser et pour évoluer (0x433d80) ; les bateaux de pêche
/// de Poséidon reviennent en 66 % du temps (0x4b624c) ; Héra ajoute 3 points au croît
/// annuel de la population (0x566d40).
const HEPHAISTOS: u8 = 9;
const APHRODITE_APPEAL: i32 = 10;
const POSEIDON: u8 = 1;
const POSEIDON_FISHING_PERCENT: u64 = 66;
const HERA: u8 = 12;
const HERA_GROWTH: i32 = 3;
/// Atlas accélère les chantiers : `Zeus.exe` 0x48caf0 rend 2 au lieu de 1, et
/// 0x48f8e1 ramène la durée à 66 %.
const ATLAS: u8 = 13;
/// Apollon ajoute 20 à chacune des quatre notes de cité (culture, athlétisme, théâtre,
/// philosophie), lues par `Zeus.exe` 0x565d40.
const APOLLO: u8 = 3;
const APOLLO_NOTE: i32 = 20;
/// Caractéristiques des personnages (`Zeus.exe`, 0x4e1b20) : Hermès rend charrettes et
/// livreurs plus rapides d'un tiers (et caravanes et navires marchands de moitié, qu'Olympe
/// ne fait pas circuler) ; Athéna donne 4 points d'armure contre les projectiles aux
/// soldats de la cité.
const HERMES: u8 = 7;
const HERMES_SPEED_PERCENT: usize = 133;
const ATHENA: u8 = 8;
const ATHENA_MISSILE_ARMOR: i32 = 4;
/// Croît naturel, une fois l'an (`Zeus.exe`, 0x566d40) : pour cent de la population
/// selon la santé (au-delà de 90, 80, 70, 60, 50, 40) ; en dessous, -1 tant que la santé
/// visée dépasse 30, sinon -2, -3 ou -5 au-delà de 20, 10 ou 0. Les naissances
/// remplissent les logements modestes déjà habités (0x486ef0), les décès les vident
/// (0x487020).
const HEALTHY_GROWTH: [(i32, i32); 6] = [(90, 6), (80, 4), (70, 3), (60, 2), (50, 1), (40, 0)];
const DECLINE_TARGET: i32 = 30;
const SICKLY_DECLINE: [(i32, i32); 3] = [(20, -2), (10, -3), (0, -5)];
/// Santé de la cité (`Zeus.exe`, 0x566c00) : chaque mois, la cible est la moyenne, par
/// habitant, de 50 pour une maison que visite un médecin et 50 pour une maison qui a
/// mangé, plus un bonus pour les petites cités ; la santé s'en approche d'un point par
/// jour (0x566b90). Elle part de 80.
const START_HEALTH: i32 = 80;
const HEALER_HEALTH: i32 = 50;
const FED_HEALTH: i32 = 50;
const HEALTH_POPULATION_BONUS: [(i32, i32); 10] = [(100, 80), (200, 80), (300, 70), (400, 60), (500, 50), (600, 40), (700, 30), (800, 20), (900, 10), (1000, 0)];
/// Épidémies (0x447ff0) : chances sur 10 000, chaque jour, selon la santé ; au plus une
/// par mois, chances×384/10 000 sur deux années, et la moitié de ce nombre dans l'année.
/// Une épidémie frappe 40 % des maisons habitées, les plus exposées d'abord (sans
/// médecin, puis sans vivres) : la première moitié à coup sûr, les autres une fois sur
/// deux (0x448490) ; elle emporte une part des habitants selon la difficulté (0x447f70).
const PLAGUE_ODDS: [(i32, i32); 7] = [(10, 300), (20, 250), (30, 200), (45, 100), (60, 50), (75, 25), (100, 0)];
const PLAGUE_DAYS: i32 = 384;
const PLAGUE_HOUSES_PERCENT: i32 = 40;
const FED_PLAGUE_SHIELD: i32 = 20;
const PLAGUE_DEATHS: [(i32, i32); 5] = [(1, 4), (1, 3), (1, 2), (2, 3), (1, 1)];
/// Durée d'un incendie avant qu'il ne reste que des décombres.
const FIRE_TICKS: u64 = TICKS_PER_MONTH;
/// Images de décombres dans `zeus_terrain.sg3`.
const RUBBLE_FIRST_IMAGE: usize = 615;
const RUBBLE_IMAGES: u8 = 8;
/// Agora : nombre d'emplacements, et dallage de sa route dans `zeus_general.sg3`.
const AGORA_PLOTS: i32 = 3;
const AGORA_ROAD_FIRST_IMAGE: usize = 457;
const AGORA_ROAD_IMAGES: usize = 3;
/// Entrepôt : nombre de baies, chargements par baie, et images des baies dans
/// `zeus_general.sg3` (vide, puis première image de chaque marchandise ; les suivantes
/// montrent la baie de plus en plus pleine).
pub const STOREHOUSE_BAYS: usize = 8;
const BAY_LOADS: i32 = 4;
const EMPTY_BAY_IMAGE: usize = 934;
/// Une baie ne montre jamais qu'une sculpture. Le marbre noir, l'orichalque et les
/// chars ont leurs baies dans `poseidon_loaded.sg3` (relevées à l'œil).
const BAY_FIRST_IMAGES: [(Source, usize); GOODS.len()] = [
    (Source::General, 837),
    (Source::General, 861),
    (Source::General, 877),
    (Source::General, 881),
    (Source::General, 869),
    (Source::General, EMPTY_BAY_IMAGE),
    (Source::General, 845),
    (Source::General, 841),
    (Source::General, 849),
    (Source::General, 873),
    (Source::Poseidon, 968),
    (Source::Poseidon, 972),
    (Source::Poseidon, 987),
];
/// Part des habitants des logements modestes qui travaillent (`Zeus.exe`, 0x4865a0),
/// en pour cent : selon la difficulté, plus selon le niveau de salaire le plus proche
/// (`WAGE_LEVELS`), plus selon la santé (dès 20, 35, 65 et 80).
const WORKFORCE_BY_DIFFICULTY: [i32; 5] = [50, 45, 40, 37, 35];
const WORKFORCE_BY_WAGE: [i32; 6] = [-10, -6, -3, 0, 3, 5];
const WORKFORCE_BY_HEALTH: [(i32, i32); 4] = [(80, 2), (65, 1), (35, 0), (20, -1)];
const WORKFORCE_SICK: i32 = -2;
/// Impôt mensuel au taux normal, en drachmes pour 100 habitants d'une maison visitée
/// par un percepteur (multiplié par le coefficient du niveau de logement) ; les
/// autres taux du jeu le font varier en proportion.
const TAX_PER_100_PEOPLE: i32 = 25;
/// Niveau d'imposition normal (taux 9 du jeu) et humeur de départ.
pub const NORMAL_TAX_LEVEL: usize = 3;
const START_SENTIMENT: i32 = 60;
/// Part des maisons visitées par un percepteur sous laquelle l'imposition compte
/// comme nulle pour l'humeur, et population à partir de laquelle les effets négatifs
/// comptent (`Zeus.exe`, 0x567120 et 0x5671a0).
const MIN_TAX_COVERAGE_PERCENT: i32 = 11;
const GROWN_POPULATION: i32 = 350;
/// Niveaux de salaire du jeu et leur effet sur l'humeur (`Zeus.exe`, 0x641f20).
const WAGE_LEVELS: [i32; 6] = [0, 20, 26, 30, 34, 40];
const WAGE_MOOD: [i32; 6] = [-10, -5, -2, 0, 2, 4];
/// Salaire mensuel, en drachmes pour 100 employés.
const WAGES_PER_100_WORKERS: i32 = 30;

/// Services apportés aux maisons par des marcheurs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Service {
    Water,
    Maintenance,
    Security,
    Health,
    Tax,
    Philosophy,
    Athletics,
    Drama,
}

pub const SERVICES: [Service; 8] = [
    Service::Water,
    Service::Maintenance,
    Service::Security,
    Service::Health,
    Service::Tax,
    Service::Philosophy,
    Service::Athletics,
    Service::Drama,
];
/// Les trois disciplines qui comptent pour la culture d'une maison.
pub const CULTURE: [Service; 3] = [Service::Philosophy, Service::Athletics, Service::Drama];

/// Marchandises distribuées aux maisons par les échoppes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Good {
    Food,
    Fleece,
    Oil,
    /// Bu seulement dans les logements luxueux.
    Wine,
    /// Armement, exigé par les logements luxueux à partir de l'hôtel particulier.
    Arms,
    /// Chevaux, exigés par le domaine ; ils ne passent pas par l'entrepôt.
    Horses,
    /// Bronze de la fonderie que l'armurerie ne prend pas : il attend à l'entrepôt.
    Bronze,
    /// Bois de la scierie et marbre de l'atelier de maçonnerie, rangés à l'entrepôt.
    Wood,
    Marble,
    /// Sculptures de bronze de l'atelier de sculpture.
    Sculpture,
    /// Marbre noir et orichalque de Poséidon, rangés à l'entrepôt.
    BlackMarble,
    Orichalc,
    /// Chars de Poséidon : la fabrique les assemble avec du bois et des chevaux, les
    /// logements luxueux atlantes les exigent à la place des chevaux.
    Chariot,
}

/// Matière première d'un atelier.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Raw {
    /// Olives et raisins de la métairie.
    Crops,
    /// Bronze de la fonderie.
    Bronze,
}

/// Ce qu'un atelier transforme pour fabriquer cette marchandise.
fn raw_for(good: Good) -> Raw {
    match good {
        Good::Arms | Good::Sculpture => Raw::Bronze,
        _ => Raw::Crops,
    }
}

pub const GOODS: [Good; 13] = [
    Good::Food,
    Good::Fleece,
    Good::Oil,
    Good::Wine,
    Good::Arms,
    Good::Horses,
    Good::Bronze,
    Good::Wood,
    Good::Marble,
    Good::Sculpture,
    Good::BlackMarble,
    Good::Orichalc,
    Good::Chariot,
];

/// Ressources numérotées comme dans le jeu (groupe de textes 23, ressources et
/// objectifs des aventures) : 2 poisson, 7 blé, 14 laine, 20 huile d'olive…
pub const RESOURCES: usize = 24;
/// Les ressources 1 à 7 sont des vivres, rangés au grenier.
pub const FOODS: usize = 8;

/// Marchandise que les maisons consomment (les autres ne servent qu'à l'industrie
/// ou aux objectifs).
pub fn for_houses(good: Good) -> bool {
    !matches!(good, Good::Bronze | Good::Wood | Good::Marble | Good::Sculpture | Good::BlackMarble | Good::Orichalc)
}

/// Numéro de ressource d'une marchandise rangée à l'entrepôt.
pub fn resource_of(good: Good) -> Option<u8> {
    match good {
        Good::Fleece => Some(14),
        Good::Oil => Some(20),
        Good::Wine => Some(21),
        Good::Arms => Some(18),
        Good::Horses => Some(15),
        Good::Bronze => Some(10),
        Good::Wood => Some(9),
        Good::Marble => Some(11),
        Good::Sculpture => Some(19),
        Good::BlackMarble => Some(16),
        Good::Orichalc => Some(17),
        Good::Chariot => Some(22),
        Good::Food => None,
    }
}

/// Rôle d'un type de bâtiment dans la simulation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Function {
    None,
    House,
    /// Envoie des marcheurs rendre ce service.
    Service(Service),
    /// Cultive ses champs et livre la récolte au grenier.
    /// Forme les artistes d'un lieu de culture ; sans école en activité, le lieu reste vide.
    School(Service),
    Farm,
    Granary,
    /// Outil « agora » : pose trois emplacements d'échoppe le long d'une route, ou
    /// six de part et d'autre pour une grande agora.
    Agora { grand: bool },
    /// Emplacement d'agora, sur lequel on bâtit une échoppe.
    Plot,
    /// Pêcherie : produit du poisson, et doit toucher l'eau.
    Fishery,
    /// Stade : la moitié qui porte la fonction. L'outil pose aussi son annexe.
    Stadium,
    /// Palais : sans lui, les hôtels des impôts ne perçoivent rien.
    Palace,
    /// Seconde moitié d'un stade, sans rôle propre.
    Annex,
    /// Entrepose laine et huile entre producteurs et échoppes.
    Storehouse,
    /// Poste de commerce, ou docks (`sea`) au bord de l'eau : un entrepôt qui ouvre les
    /// échanges avec une cité partenaire dont la route est de la même voie.
    TradePost { sea: bool },
    /// Produit seule une marchandise finie (bergerie).
    Producer(Good),
    /// Produit une matière première et la livre aux ateliers qui la transforment
    /// (métairie, fonderie).
    Supplier(Raw),
    /// Transforme la matière première reçue en marchandise (pressoir).
    Workshop(Good),
    /// Se fournit auprès d'un producteur et distribue la marchandise aux maisons.
    Vendor(Good),
    /// Sanctuaire d'un dieu (numéro du jeu), et ce qu'il faut y livrer pour l'achever :
    /// marbre, bois, sculptures.
    Sanctuary { god: u8, needs: [u16; 3] },
    /// Temple d'un héros : le héros y vient quand ses exigences sont remplies.
    HeroHall(u8),
    /// Section de l'hippodrome (4 × 4 cases), raccordée aux sections voisines.
    Racetrack,
    /// Quai des navires : posé au bord de l'eau, il arme un navire avec le bois de
    /// l'entrepôt.
    Wharf,
    /// Tour : ses sentinelles tirent sur les envahisseurs à portée.
    Tower,
    /// Musée des cités atlantes : il tient lieu de stade pour les logements.
    Museum,
    /// Corps de garde : posé sur une route, il laisse passer les citoyens à travers les
    /// murs et barre la route aux envahisseurs.
    Gatehouse,
    /// Monument commémoratif (type 119), de l'un des neuf modèles : il ne se pose que
    /// si la cité en a gagné un.
    Commemorative(u8),
    /// Barrage routier : posé sur une route, il arrête les marcheurs en tournée mais
    /// laisse passer ceux qui vont quelque part (porteurs, immigrants).
    Roadblock,
    /// Monument de Poséidon (rang 0 à 14, type de bâtiment 100 + rang), et ce qu'il
    /// faut y livrer : marbre, bois, sculptures.
    Monument { model: u8, needs: [u16; 3] },
}

/// Quête de l'épisode, désignée par son événement.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Quest {
    pub event: usize,
    pub god: u8,
    pub quest: u8,
    pub hero: u8,
    pub state: QuestState,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum QuestState {
    /// Le héros n'est pas encore parti.
    Waiting,
    /// Le héros est en route ; il revient, quête accomplie, à ce pas de simulation.
    Away { until: u64 },
    Done,
}

/// Monstre de l'épisode, désigné par son événement.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Monster {
    pub event: usize,
    pub monster: u8,
    /// Le héros l'affronte jusqu'à ce pas de simulation.
    pub fight_until: Option<u64>,
    pub slain: bool,
    /// Pas de simulation où il arrive dans la cité (0 : dès le début).
    pub arrives_at: u64,
}

/// Ce qui vient d'arriver aux héros, pour prévenir le joueur.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HeroNews {
    Arrived(u8),
    Departed(usize),
    QuestDone(usize),
    Fighting(usize),
    Slain(usize),
    /// Un monstre arrive dans la cité.
    Invaded(usize),
}

/// Attrait qu'un bâtiment donne à son voisinage, tel que décrit dans les règles.
#[derive(Clone, Copy, Default)]
pub struct Appeal {
    /// Valeur sur le premier anneau de cases autour du bâtiment.
    pub value: i32,
    /// Nombre d'anneaux après lequel la valeur change de `step_size`.
    pub step: i32,
    pub step_size: i32,
    /// Nombre d'anneaux touchés.
    pub range: i32,
}

/// Élément d'un bâtiment : une image posée à un endroit de son emprise.
#[derive(Clone, Copy)]
pub struct Part {
    /// Coin de l'élément, relatif à celui du bâtiment.
    pub dx: usize,
    pub dy: usize,
    pub size: usize,
    pub image: usize,
    /// Champ : l'image avance avec la pousse.
    pub crop: bool,
    /// Baie d'entrepôt (rang) : l'image montre la marchandise qui s'y trouve.
    pub bay: Option<u8>,
    /// Catalogue de l'image (`zeus_general.sg3` le plus souvent).
    pub source: Source,
}

/// Type de bâtiment constructible.
#[derive(Clone)]
pub struct Kind {
    pub name: String,
    pub cost: i32,
    /// Côté de l'emprise, en cases (largeur, sur l'axe x).
    pub size: usize,
    /// Profondeur de l'emprise, sur l'axe y ; égale à `size` pour un bâtiment carré.
    pub depth: usize,
    pub parts: Vec<Part>,
    pub function: Function,
    /// Première image, dans `sprmain.sg3`, du marcheur que le bâtiment envoie.
    pub walker: usize,
    /// Image posée par-dessus quand le bâtiment a du stock (marchandises d'une échoppe).
    pub stock_overlay: Option<usize>,
    /// Employés nécessaires pour fonctionner.
    pub employees: i32,
    pub appeal: Appeal,
    /// Risque d'incendie et d'effondrement gagné chaque mois sans visite de la maintenance.
    pub fire_risk: i32,
    pub damage_risk: i32,
    /// Ressource que le bâtiment produit (numéro du jeu), 0 s'il n'en produit pas :
    /// sert aux objectifs de production et à trier les vivres du grenier.
    pub product: u8,
}

impl Kind {
    /// Bâtiment fait d'une seule image.
    pub fn simple(name: String, cost: i32, size: usize, image: usize, function: Function, walker: usize) -> Kind {
        let parts = vec![Part { dx: 0, dy: 0, size, image, crop: false, bay: None, source: Source::General }];
        Kind { name, cost, size, depth: size, parts, function, walker, stock_overlay: None, employees: 0, appeal: Appeal::default(), fire_risk: 0, damage_risk: 0, product: 0 }
    }
}

/// Premières images, dans `sprmain.sg3`, des personnages sans bâtiment attitré.
#[derive(Clone, Copy, Default)]
pub struct WalkerImages {
    pub immigrant: usize,
    /// Nouvel habitant d'un logement luxueux.
    pub elite_immigrant: usize,
    /// Porteur qui livre les récoltes.
    pub carrier: usize,
    /// Acheteuse qui approvisionne une échoppe.
    pub buyer: usize,
    /// Soldats de la cité (marche, mort, combat) qui défendent contre les invasions :
    /// piétaille, hoplite, cavalier.
    pub hoplite: usize,
    pub rabble: usize,
    pub horseman: usize,
    /// Navire de transport des envahisseurs venus par la mer.
    pub ship: usize,
    /// Soldats d'une cité atlante dans `poseidonimps.sg3`, par rang : archer, lancier,
    /// char.
    pub atlantean: [usize; 3],
}

/// Animations des soldats atlantes, par rang : l'archer frappe en quinze images, le
/// lancier en huit, le char (neuf images de mort par direction) en douze.
const ATLANTEAN_ANIMS: [Anim; 3] = [
    Anim { walk: WALK_FRAMES, fight: 104, fight_frames: 15 },
    Anim { walk: WALK_FRAMES, fight: 104, fight_frames: 8 },
    Anim { walk: WALK_FRAMES, fight: 168, fight_frames: 12 },
];

pub struct House {
    /// Rang dans les niveaux de logement (0 = hutte).
    pub level: usize,
    pub population: i32,
    /// Habitants en route vers la maison.
    arriving: i32,
    /// Pour chaque service, pas de simulation jusqu'auquel la maison est desservie.
    covered_until: [u64; SERVICES.len()],
    /// Réserves de la maison, en chargements, dans l'ordre de `GOODS`.
    pub goods: [i32; GOODS.len()],
    /// Risques de criminalité et de maladie accumulés depuis le dernier passage d'un
    /// garde ou d'un médecin.
    pub crime: i32,
    /// La maison a mangé au dernier mois ; sinon, depuis combien de mises à jour de
    /// l'humeur elle a faim (au plus 3).
    pub ate: bool,
    pub hungry: u8,
}

impl House {
    pub fn has(&self, service: Service, now: u64) -> bool {
        self.covered_until[service as usize] > now
    }
}

pub struct Building {
    pub kind: usize,
    /// Coin de l'emprise le plus éloigné du spectateur.
    pub x: usize,
    pub y: usize,
    pub house: Option<House>,
    /// Chargements en stock (ferme, grenier, échoppe).
    pub stock: i32,
    /// Matière première en attente de transformation (atelier).
    pub raw: i32,
    /// Contenu d'un entrepôt, en chargements, dans l'ordre de `GOODS`.
    pub goods: [i32; GOODS.len()],
    /// Vivres d'un grenier par ressource (le rang 0 reçoit ceux d'origine inconnue) ;
    /// leur somme est `stock`.
    pub foods: [i32; FOODS],
    /// Prochain départ d'un marcheur en tournée.
    next_walk: u64,
    /// Début de la pousse en cours (ferme).
    sown_at: u64,
    /// Risques accumulés depuis la dernière visite de la maintenance.
    pub fire: i32,
    pub damage: i32,
    /// Nombre de tournées parties d'ici : fait tourner le cap des marcheurs.
    walks: u8,
    /// Un porteur ou une acheteuse du bâtiment est déjà en route.
    errand_out: bool,
    /// Le bâtiment a reçu des employés ce mois-ci ; sinon il est à l'arrêt.
    pub staffed: bool,
    /// Employés présents ; en dessous de son effectif, il travaille d'autant plus
    /// lentement.
    pub workers: i32,
}

/// Ce que distribue un marcheur en tournée.
#[derive(Clone, Copy)]
enum Rounds {
    Service(Service),
    /// Marchandise restante, et échoppe d'origine.
    Goods { good: Good, carried: i32, vendor: u16 },
}

/// Ce que fait un personnage.
#[derive(Clone, Copy)]
enum Role {
    /// Groupe d'immigrants en route vers sa maison.
    Immigrant { home: u16, people: i32 },
    /// Marcheur en tournée sur les routes. Aux carrefours il oblique vers `heading`
    /// (rang dans `NEIGHBOURS`), qui change à chaque départ pour varier les quartiers servis.
    Rounds { rounds: Rounds, steps_left: u32, heading: u8, home: u16 },
    /// Marcheur qui rentre à pied à son bâtiment après sa tournée, avec ce qu'il lui
    /// reste de marchandise (comme dans le jeu d'origine).
    Return { home: u16, good: Good, carried: i32 },
    /// Porteur qui livre des chargements à un bâtiment.
    Deliver { to: u16, load: i32, from: u16 },
    /// Porteur parti de l'entrepôt avec des matériaux pour un chantier (sanctuaire,
    /// monument, quai des navires).
    Supply { to: u16, good: Good, load: i32 },
    /// Acheteuse en route vers un fournisseur pour le compte d'une échoppe.
    Fetch { source: u16, vendor: u16 },
    /// Monstre en marche vers le bâtiment `target`, qu'il détruira ; il se repose
    /// ensuite jusqu'à `rest_until`.
    Monster { monster: u8, target: u16, rest_until: u64 },
    /// Héros présent dans la cité, rattaché à son temple.
    Hero { hero: u8, hall: u16 },
    /// Dieu favorable en visite : il flâne près de son sanctuaire, puis repart.
    Visit { god: u8, leaves_at: u64 },
    /// Soldat d'une bataille : envahisseur (`enemy`) ou défenseur de la cité, ses
    /// forces, le bâtiment visé par un envahisseur, et s'il est au corps à corps.
    /// `army` : nation et unité d'un envahisseur (voir `army_unit`).
    Soldier { enemy: bool, army: u8, health: i32, target: u16, fighting: bool },
    /// Navire qui a débarqué une armée ; il attend la fin de l'invasion.
    Ship,
    /// Dieu hostile venu en personne : il ravage comme un monstre, puis repart.
    God { god: u8, target: u16, rest_until: u64, leaves_at: u64 },
}

pub struct Figure {
    /// Première image de marche du personnage, dans le catalogue `source`.
    image_base: usize,
    pub source: Source,
    /// Position en cases, fractionnaire pendant un déplacement.
    pub x: f32,
    pub y: f32,
    direction: usize,
    frame: usize,
    /// Cases restant à parcourir, la prochaine en dernier. Pour une tournée, ne
    /// contient que la prochaine case.
    path: Vec<(u16, u16)>,
    from: (u16, u16),
    /// Case précédente, pour ne pas rebrousser chemin en tournée.
    came_from: Option<(u16, u16)>,
    role: Role,
}

/// Paroles des passants (`audio/voice/walker`) : préfixe des fichiers de chaque dieu,
/// dans l'ordre du jeu, Héra et Atlas compris.
const GOD_VOICES: [&str; 14] = ["zeu", "pos", "dem", "apo", "atm", "are", "aph", "her", "atn", "hep", "dio", "had", "hra", "ats"];

impl Figure {
    /// Préfixe des paroles du personnage dans `audio/voice/walker`, quand il en a :
    /// porteur d'eau, garde, guérisseur, percepteur, philosophe, athlète, acteur,
    /// intendant, immigrant, charretier, dieux.
    pub fn voice(&self) -> Option<&'static str> {
        match self.role {
            Role::Immigrant { .. } => Some("imm"),
            Role::Rounds { rounds: Rounds::Service(service), .. } => Some(match service {
                Service::Water => "wtb",
                Service::Maintenance => "sup",
                Service::Security => "wtc",
                Service::Health => "hlr",
                Service::Tax => "clk",
                Service::Philosophy => "phl",
                Service::Athletics => "ath",
                Service::Drama => "act",
            }),
            Role::Deliver { .. } | Role::Supply { .. } => Some("crt"),
            Role::Visit { god, .. } | Role::God { god, .. } => GOD_VOICES.get(god as usize).copied(),
            _ => None,
        }
    }

    /// Image à dessiner : une rangée de 8 directions par étape de marche.
    pub fn image(&self) -> usize {
        self.image_toward(self.direction)
    }

    /// Image du personnage vu dans une vue tournée : sa direction à l'écran.
    pub fn image_facing(&self, orientation: u8) -> usize {
        self.image_toward((self.direction + 8 - (orientation as usize & 6)) & 7)
    }

    fn image_toward(&self, direction: usize) -> usize {
        let anim = match self.role {
            Role::Soldier { enemy: true, army, .. } => {
                let unit = army_unit(army);
                Anim { walk: unit.walk_frames, fight: unit.fight - unit.walk, fight_frames: unit.fight_frames }
            }
            Role::Soldier { enemy: false, army, .. } if self.source == Source::Imps => ATLANTEAN_ANIMS[(army as usize).min(2)],
            // Piétaille et cavaliers de la cité frappent en douze images, l'hoplite en huit.
            Role::Soldier { enemy: false, army, .. } if army != 1 => Anim { fight_frames: 12, ..COMMON_ANIM },
            _ => COMMON_ANIM,
        };
        if let Role::Soldier { fighting: true, .. } = self.role {
            return self.image_base + anim.fight + direction + 8 * ((self.frame / SOLDIER_FIGHT_FRAME_TICKS) % anim.fight_frames);
        }
        self.image_base + direction + 8 * (self.frame * anim.walk / WALK_TICKS)
    }

    /// Soldat d'une bataille : `Some(true)` pour un envahisseur.
    pub fn soldier(&self) -> Option<bool> {
        match self.role {
            Role::Soldier { enemy, .. } => Some(enemy),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Cell {
    Terrain,
    /// Décombres d'un bâtiment détruit (variante d'image) : on peut rebâtir dessus.
    Rubble(u8),
    /// Case en flammes ; deviendra des décombres.
    Fire(u8),
    /// Fissure ouverte par un tremblement de terre, lave refroidie, eaux d'un
    /// raz-de-marée : on n'y bâtit plus.
    Chasm(u8),
    Lava(u8),
    Flood,
    Road,
    /// Route dallée d'une agora.
    AgoraRoad,
    /// Mur, avec ce qu'il lui reste de solidité.
    Wall(u8),
    Building(u16),
}

/// Sinistre qui a détruit un bâtiment.
#[derive(Clone, Copy, Debug)]
pub struct Disaster {
    pub cause: Cause,
    /// Type du bâtiment détruit.
    pub kind: usize,
    pub x: usize,
    pub y: usize,
}

/// Commerce permis pour une ressource : vendre le surplus et acheter ce qui manque,
/// l'un des deux seulement, ou rien.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TradePolicy {
    Both,
    SellOnly,
    BuyOnly,
    Neither,
}

impl TradePolicy {
    pub const ALL: [TradePolicy; 4] = [TradePolicy::Both, TradePolicy::SellOnly, TradePolicy::BuyOnly, TradePolicy::Neither];

    pub fn sells(self) -> bool {
        matches!(self, TradePolicy::Both | TradePolicy::SellOnly)
    }

    pub fn buys(self) -> bool {
        matches!(self, TradePolicy::Both | TradePolicy::BuyOnly)
    }

    /// La suivante, pour le panneau du commerce.
    pub fn next(self) -> TradePolicy {
        let at = TradePolicy::ALL.iter().position(|&p| p == self).unwrap_or(0);
        TradePolicy::ALL[(at + 1) % TradePolicy::ALL.len()]
    }
}

/// Ce qui a détruit un bâtiment.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cause {
    Fire,
    Collapse,
    /// Envahisseurs qui l'ont incendié.
    Raid,
    /// Monstre (numéro du groupe de textes 87) ou dieu hostile (numéro du jeu).
    Monster(u8),
    God(u8),
}

/// Méfait ou épidémie dans une maison.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Incident {
    /// Un voleur a pris ces drachmes au trésor.
    Theft { amount: i32, x: usize, y: usize },
    /// Des voleurs ont pillé les réserves d'une maison luxueuse.
    Pillage { x: usize, y: usize },
    /// La maladie a emporté ces habitants.
    Plague { dead: i32, x: usize, y: usize },
}

/// Ce qu'il faut dessiner sur une case.
pub struct Draw {
    pub source: Source,
    pub image: usize,
    /// La case brûle : des flammes sont à dessiner par-dessus.
    pub burning: bool,
    /// Image supplémentaire à poser à l'ancrage de la première.
    pub overlay: Option<usize>,
    /// Sol à dessiner d'abord, quand l'image ne couvre pas toute la case (jeunes pousses).
    pub ground: Option<(Source, usize)>,
    /// Hauteur, en pixels à l'échelle 1, dont l'image est soulevée (étages d'un
    /// monument), et image posée au-dessus, soulevée de tant de pixels.
    pub lift: u16,
    pub above: Option<(Source, usize, u16)>,
}

pub struct World {
    pub map: Map,
    /// Cases d'un tracé en cours, posées le temps de l'affichage : (case, contenu d'avant).
    ghosts: Vec<(usize, Cell)>,
    pub kinds: Vec<Kind>,
    pub buildings: Vec<Option<Building>>,
    pub figures: Vec<Figure>,
    pub money: i32,
    /// Pas de simulation écoulés depuis le début de la partie.
    pub ticks: u64,
    cells: Vec<Cell>,
    house_levels: Vec<HouseModel>,
    pub walkers: WalkerImages,
    /// Orientation de la vue, et terrain repavé pour elle (le pavage de la carte vaut
    /// pour l'orientation 0).
    pub orientation: u8,
    pub terrain_view: Option<crate::tiling::Tiling>,
    /// Images d'un bâtiment non carré pour l'autre sens (palais) : (image, image tournée).
    pub turned_images: Vec<(usize, usize)>,
    /// Migrants pas encore assez nombreux pour former un groupe (arrivants, partants),
    /// et jours pendant lesquels arrivées et départs restent suspendus.
    migrants: (i32, i32),
    migration_pause: (u8, u8),
    /// État du générateur pseudo-aléatoire (tournées des marcheurs).
    random: u32,
    /// Dernier bilan mensuel : (impôts perçus, salaires versés).
    pub last_month: (i32, i32),
    /// Bénéfice de l'année civile en cours, et meilleur bénéfice d'une année achevée.
    pub year_profit: i32,
    pub best_year_profit: Option<i32>,
    /// Production de l'année civile en cours et meilleure année achevée, par ressource.
    pub produced: [i32; RESOURCES],
    pub best_produced: [i32; RESOURCES],
    /// Marchandises mises de côté pour la cité suivante, par ressource.
    pub set_aside: [i32; RESOURCES],
    /// Nombre de sanctuaires que l'épisode permet d'avoir à la fois (quatre au plus).
    pub max_sanctuaries: usize,
    /// Cité atlante : la science y remplace la culture, avec ses propres points.
    pub atlantean: bool,
    /// Quêtes et monstres de l'épisode, héros présents dans la cité, et nouvelles.
    pub quests: Vec<Quest>,
    pub monsters: Vec<Monster>,
    pub heroes: [bool; crate::heroes::HEROES],
    pub hero_news: Vec<HeroNews>,
    /// Cités du monde et leur statut ; l'armée est en campagne jusqu'à ce pas.
    pub cities: Vec<City>,
    pub campaign_until: u64,
    /// Changements du monde à venir dans l'épisode, et ceux qui viennent d'avoir lieu
    /// (cité, changement) pour prévenir le joueur.
    pub scheduled: Vec<Scheduled>,
    pub world_news: Vec<(usize, u8)>,
    /// Demandes des cités en attente, et leurs nouvelles.
    pub requests: Vec<Request>,
    pub request_news: Vec<RequestNews>,
    /// Venues de dieux hostiles à venir (pas de simulation, dieu), et nouvelles
    /// (dieu, arrivée ou départ).
    pub invasions: Vec<(u64, u8)>,
    pub god_news: Vec<(u8, bool)>,
    /// Tributs reçus depuis la dernière lecture : (cité, drachmes).
    pub tributes: Vec<(usize, i32)>,
    /// Bienfaits des dieux depuis la dernière lecture : (dieu, ressource, quantité).
    pub blessings: Vec<(u8, u8, i32)>,
    /// Invasions armées livrées depuis la dernière lecture.
    pub battles: Vec<Battle>,
    /// Invasion en cours, soldats dans la cité.
    pub raid: Option<Raid>,
    /// Caractéristiques de combat des personnages (`figure_model`), fournies avec les
    /// règles ; vides, le combat prend des valeurs de secours.
    pub fighters: crate::model::Fighters,
    /// Impôts du jeu, niveau d'imposition choisi (rang dans `taxes.levels`, normal par
    /// défaut), niveau de difficulté (0 à 4) et humeur de la cité (0 à 100).
    pub taxes: crate::model::Taxes,
    pub tax_level: usize,
    pub difficulty: usize,
    pub sentiment: i32,
    /// La cité a dépassé 349 habitants : mécontentements et criminalité comptent
    /// désormais (avant, le jeu les tient pour nuls).
    pub grown: bool,
    /// Mois consécutifs de trésor négatif.
    pub months_in_debt: u32,
    /// Santé de la cité et santé visée (0 à 100), épidémies de l'année en cours et de
    /// la précédente, et épidémie survenue ce mois-ci.
    pub health: i32,
    pub health_target: i32,
    plagues: (u32, u32),
    plague_month: bool,
    /// Marchandises mises de côté par la cité précédente, que ses cadeaux apportent,
    /// et cadeaux reçus depuis la dernière lecture : (cité, ressource, quantité).
    pub carried: [i32; RESOURCES],
    pub gifts: Vec<(usize, u8, i32)>,
    /// Écart de chaque prix à son prix de base, et changements du commerce survenus
    /// depuis la dernière lecture : (cité, changement, ressource).
    pub price_shift: [i32; RESOURCES],
    /// Ce que le joueur permet de faire de chaque ressource avec les cités partenaires.
    pub trade_policy: [TradePolicy; RESOURCES],
    /// Monuments commémoratifs gagnés et pas encore posés (modèles).
    pub commemoratives: Vec<u8>,
    /// Paliers de population déjà atteints.
    milestones: u8,
    /// Jeux panhelléniques : années avant les premiers (aucun si `None`), année de
    /// départ de l'épisode (le type de jeux en dépend), cité désignée pour accueillir
    /// les prochains Olympiques, jeux en cours (type, chez soi, place), nouvelles.
    pub games_from: Option<i32>,
    pub start_year: i32,
    games_host: bool,
    games_running: Option<(u8, bool, u8)>,
    pub games_news: Vec<GamesNews>,
    /// Prix de base des marchandises, lus dans la carte (0 : pas de prix réglé).
    pub base_prices: [i32; RESOURCES],
    pub trade_news: Vec<(usize, u8, u8)>,
    /// Écart du salaire au salaire normal (drachmes par an et par dix ouvriers), et
    /// armées rivales parties en campagne : (cité, retour).
    pub wage_shift: i32,
    pub armies_away: Vec<(usize, u64)>,
    /// Campagne lointaine de l'armée du joueur, routes du monde, marques de pillage
    /// (cité, ressource, marque) et nouvelles des campagnes.
    pub expedition: Option<Expedition>,
    pub routes: Vec<crate::map::Route>,
    plundered: Vec<(usize, u8, i32)>,
    pub campaign_news: Vec<CampaignNews>,
    /// Hausses (positives) et baisses des salaires survenues depuis la dernière lecture.
    pub wage_news: Vec<i32>,
    /// Catastrophes survenues depuis la dernière lecture : (nature, bâtiments détruits).
    pub disaster_news: Vec<(u8, i32)>,
    /// Ressources que l'on peut exploiter sans gisement : la carte n'en a pas, mais
    /// un objectif de l'épisode les exige.
    pub waived: Vec<u8>,
    /// Disposition de chaque monument, par rang (fournie avec les types de bâtiments).
    pub monument_layouts: Vec<crate::monument::Layout>,
    /// Cases inondées dont l'eau se retire à ce pas de simulation.
    floods: Vec<(u16, u16, u64)>,
    /// Types de bâtiments que le scénario n'offre pas (indexé comme `kinds` ; vide =
    /// tout est permis).
    pub locked: Vec<bool>,
    /// Cases en flammes : (x, y, pas de simulation où le feu s'éteint).
    fires: Vec<(u16, u16, u64)>,
    /// Sinistres survenus depuis la dernière lecture, pour avertir le joueur.
    pub disasters: Vec<Disaster>,
    /// Bâtiments que monstres et dieux ont abattus pendant le pas : ils tombent à la fin
    /// de la marche des personnages, dont leur chute emporte certains.
    ravaged: Vec<(u16, Cause)>,
    /// Vols et épidémies survenus depuis la dernière lecture.
    pub incidents: Vec<Incident>,
    /// Attrait de chaque case, recalculé quand un bâtiment apparaît ou disparaît.
    appeal: Vec<i16>,
}

impl World {
    pub fn new(map: Map, kinds: Vec<Kind>, house_levels: Vec<HouseModel>, walkers: WalkerImages, money: i32) -> World {
        let routes = map.routes();
        let mut world = World {
            map,
            ghosts: Vec::new(),
            kinds,
            buildings: Vec::new(),
            figures: Vec::new(),
            money,
            ticks: 0,
            cells: vec![Cell::Terrain; GRID * GRID],
            house_levels,
            walkers,
            orientation: 0,
            terrain_view: None,
            turned_images: Vec::new(),
            migrants: (0, 0),
            migration_pause: (0, 0),
            random: 0x2545_F491,
            last_month: (0, 0),
            year_profit: 0,
            best_year_profit: None,
            produced: [0; RESOURCES],
            best_produced: [0; RESOURCES],
            set_aside: [0; RESOURCES],
            max_sanctuaries: MAX_SANCTUARIES,
            atlantean: false,
            quests: Vec::new(),
            monsters: Vec::new(),
            heroes: [false; crate::heroes::HEROES],
            hero_news: Vec::new(),
            cities: Vec::new(),
            campaign_until: 0,
            scheduled: Vec::new(),
            world_news: Vec::new(),
            requests: Vec::new(),
            request_news: Vec::new(),
            invasions: Vec::new(),
            god_news: Vec::new(),
            tributes: Vec::new(),
            blessings: Vec::new(),
            battles: Vec::new(),
            raid: None,
            fighters: crate::model::Fighters::default(),
            taxes: crate::model::Taxes::default(),
            tax_level: NORMAL_TAX_LEVEL,
            difficulty: 2,
            sentiment: START_SENTIMENT,
            grown: false,
            months_in_debt: 0,
            health: START_HEALTH,
            health_target: START_HEALTH,
            plagues: (0, 0),
            plague_month: false,
            carried: [0; RESOURCES],
            gifts: Vec::new(),
            price_shift: [0; RESOURCES],
            trade_policy: [TradePolicy::Both; RESOURCES],
            commemoratives: Vec::new(),
            milestones: 0,
            games_from: None,
            start_year: 0,
            games_host: false,
            games_running: None,
            games_news: Vec::new(),
            base_prices: [0; RESOURCES],
            trade_news: Vec::new(),
            wage_shift: 0,
            armies_away: Vec::new(),
            expedition: None,
            routes,
            plundered: Vec::new(),
            campaign_news: Vec::new(),
            wage_news: Vec::new(),
            disaster_news: Vec::new(),
            floods: Vec::new(),
            monument_layouts: Vec::new(),
            waived: Vec::new(),
            locked: Vec::new(),
            fires: Vec::new(),
            disasters: Vec::new(),
            ravaged: Vec::new(),
            incidents: Vec::new(),
            appeal: vec![0; GRID * GRID],
        };
        // Prix réglés dans l'éditeur pour cette carte.
        for (resource, price) in world.map.prices() {
            if let Some(slot) = world.base_prices.get_mut(resource as usize) {
                *slot = price;
            }
        }
        world
    }

    /// Cité vierge sur une autre carte, avec les mêmes types de bâtiments et les mêmes
    /// règles ; le calendrier continue là où en est celle-ci.
    pub fn new_city(&self, map: Map, money: i32) -> World {
        let mut city = World::new(map, self.kinds.clone(), self.house_levels.clone(), self.walkers, money);
        city.ticks = self.ticks;
        // Les cités du monde restent celles de la cité mère, avec leurs routes.
        city.cities = self.cities.clone();
        city.routes = self.routes.clone();
        city.monument_layouts = self.monument_layouts.clone();
        city.fighters = self.fighters.clone();
        (city.taxes, city.tax_level, city.difficulty) = (self.taxes.clone(), self.tax_level, self.difficulty);
        city
    }

    /// Attrait de la case, somme de l'influence des bâtiments voisins.
    pub fn appeal(&self, x: i32, y: i32) -> i32 {
        if Self::in_grid(x, y) { self.appeal[y as usize * GRID + x as usize] as i32 } else { 0 }
    }

    /// Recalcule l'attrait : chaque bâtiment rayonne par anneaux carrés autour de son
    /// emprise, la valeur changeant de `step_size` tous les `step` anneaux.
    fn rebuild_appeal(&mut self) {
        let mut appeal = vec![0i16; GRID * GRID];
        for building in self.buildings.iter().flatten() {
            let kind = &self.kinds[building.kind];
            let (x, y, size, depth) = (building.x as i32, building.y as i32, kind.size as i32, kind.depth as i32);
            let Appeal { mut value, step, step_size, range } = kind.appeal;
            let mut in_step = 0;
            for ring in 1..=range.min(APPEAL_MAX_RANGE) {
                for ty in y - ring..y + depth + ring {
                    for tx in x - ring..x + size + ring {
                        let on_ring = tx == x - ring || tx == x + size + ring - 1 || ty == y - ring || ty == y + depth + ring - 1;
                        if on_ring && Self::in_grid(tx, ty) {
                            let cell = &mut appeal[ty as usize * GRID + tx as usize];
                            *cell = (*cell as i32 + value).clamp(-APPEAL_LIMIT, APPEAL_LIMIT) as i16;
                        }
                    }
                }
                in_step += 1;
                if in_step >= step {
                    value += step_size;
                    in_step = 0;
                }
            }
        }
        self.appeal = appeal;
    }

    fn cell(&self, x: usize, y: usize) -> Cell {
        self.cells[y * GRID + x]
    }

    pub fn in_grid(x: i32, y: i32) -> bool {
        (0..GRID as i32).contains(&x) && (0..GRID as i32).contains(&y)
    }

    /// La case fait partie de la zone jouable (le losange inscrit dans la grille).
    fn in_map(&self, x: i32, y: i32) -> bool {
        Self::in_grid(x, y) && self.map.image(x as usize, y as usize) != 0
    }

    /// Route posée par le joueur ou déjà présente sur la carte.
    pub fn is_road(&self, x: i32, y: i32) -> bool {
        Self::in_grid(x, y)
            && (matches!(self.cell(x as usize, y as usize), Cell::Road | Cell::AgoraRoad)
                || self.is_gate(x, y)
                || (self.cell(x as usize, y as usize) == Cell::Terrain
                    && self.map.terrain(x as usize, y as usize) & TERRAIN_ROAD != 0))
    }

    /// Route posée par le joueur, ou route déjà présente sur la carte (bit de terrain,
    /// sans cellule `Cell::Road`).
    fn is_built_road(&self, x: i32, y: i32) -> bool {
        Self::in_grid(x, y)
            && (self.cell(x as usize, y as usize) == Cell::Road
                || (self.cell(x as usize, y as usize) == Cell::Terrain && self.map.terrain(x as usize, y as usize) & TERRAIN_ROAD != 0))
    }

    /// Une case isolée de terrain nu, dans la zone jouable.
    fn is_free(&self, x: i32, y: i32) -> bool {
        if !self.in_map(x, y) {
            return false;
        }
        let (x, y) = (x as usize, y as usize);
        matches!(self.cell(x, y), Cell::Terrain | Cell::Rubble(_))
            && self.map.edge(x, y) == EDGE_DRAW_TILE
            && self.map.terrain(x, y) & BLOCKED == 0
    }

    /// La case appartient à un corps de garde.
    fn is_gate(&self, x: i32, y: i32) -> bool {
        Self::in_grid(x, y)
            && matches!(self.cell(x as usize, y as usize), Cell::Building(id) if matches!(self.function(id as usize), Function::Gatehouse | Function::Roadblock))
    }

    /// Barrage routier : les tournées n'y passent pas.
    fn is_roadblock(&self, x: i32, y: i32) -> bool {
        Self::in_grid(x, y) && matches!(self.cell(x as usize, y as usize), Cell::Building(id) if self.function(id as usize) == Function::Roadblock)
    }

    fn is_walkable(&self, x: i32, y: i32) -> bool {
        if self.is_gate(x, y) {
            return true;
        }
        if !self.in_map(x, y) || matches!(self.cell(x as usize, y as usize), Cell::Building(_) | Cell::Chasm(_) | Cell::Flood | Cell::Wall(_)) {
            return false;
        }
        let terrain = self.map.terrain(x as usize, y as usize);
        // Une rampe se franchit bien qu'elle porte aussi le bit de relief.
        terrain & IMPASSABLE == 0 || (terrain & TERRAIN_RAMP != 0 && terrain & (IMPASSABLE & !TERRAIN_ELEVATION) == 0)
    }

    /// Pourquoi un bâtiment ne se pose pas en (x, y), quand la raison tient à son
    /// emplacement (ressource trop loin, eau ou pré manquants).
    pub fn placement_hint(&self, kind: usize, x: i32, y: i32) -> Option<&'static str> {
        let k = self.kinds.get(kind)?;
        let size = k.size as i32;
        match k.function {
            function if deposit_of(function).is_some_and(|wanted| !self.waived(wanted) && !self.near_deposit(x, y, size, wanted)) => Some(match deposit_of(function) {
                Some(Deposit::Trees) => "il faut des arbres à moins de huit cases",
                Some(Deposit::Marble) => "il faut une carrière de marbre à moins de huit cases",
                Some(Deposit::Copper) => "il faut des rochers de cuivre à moins de huit cases",
                Some(Deposit::BlackMarble) => "il faut un gisement de marbre noir à moins de huit cases",
                _ => "il faut un gisement d'orichalque à moins de huit cases",
            }),
            Function::Fishery | Function::Wharf | Function::TradePost { sea: true } => Some("il se pose au bord de l'eau, sur un terrain dégagé"),
            Function::Farm => Some("elle se pose sur un pré fleuri"),
            Function::Racetrack => Some("une section se pose dans le prolongement de la piste"),
            Function::Agora { grand: false } => Some("elle se pose le long d'une route droite de six cases, libre d'un côté sur deux cases"),
            Function::Agora { grand: true } => Some("elle se pose de part et d'autre d'une route droite de six cases, libre des deux côtés sur deux cases"),
            _ => None,
        }
    }

    /// Gisement d'une case : il se lit à son terrain (arbres, carrière de marbre) ou
    /// à son image (rochers de cuivre, gisements de Poséidon).
    fn deposit_at(&self, x: usize, y: usize) -> Option<Deposit> {
        let terrain = self.map.terrain(x, y);
        if terrain & TERRAIN_TREE != 0 {
            return Some(Deposit::Trees);
        }
        if terrain & TERRAIN_QUARRY != 0 {
            return Some(Deposit::Marble);
        }
        match self.map.resolve(self.map.image(x, y))? {
            (Source::Terrain, image) if COPPER_ROCK_IMAGES.contains(&image) => Some(Deposit::Copper),
            (Source::Poseidon, image) if BLACK_MARBLE_IMAGES.contains(&image) => Some(Deposit::BlackMarble),
            (Source::Poseidon, image) if ORICHALC_IMAGES.contains(&image) => Some(Deposit::Orichalc),
            _ => None,
        }
    }

    /// Lève la règle du gisement pour une ressource dont la carte n'a aucun gisement
    /// (un objectif l'exige quand même) ; vrai si elle est levée.
    pub fn waive_missing_deposit(&mut self, resource: u8) -> bool {
        let Some(wanted) = deposit_for(resource) else { return false };
        let present = (0..GRID * GRID).any(|i| self.in_map((i % GRID) as i32, (i / GRID) as i32) && self.deposit_at(i % GRID, i / GRID) == Some(wanted));
        if !present && !self.waived.contains(&resource) {
            self.waived.push(resource);
        }
        !present
    }

    /// La règle du gisement est-elle levée pour ce genre de gisement ?
    fn waived(&self, deposit: Deposit) -> bool {
        self.waived.iter().any(|&resource| deposit_for(resource) == Some(deposit))
    }

    /// Un gisement de ce genre à moins de `RESOURCE_REACH` cases de l'emprise.
    fn near_deposit(&self, x: i32, y: i32, size: i32, wanted: Deposit) -> bool {
        (y - RESOURCE_REACH..y + size + RESOURCE_REACH).any(|ty| {
            (x - RESOURCE_REACH..x + size + RESOURCE_REACH).any(|tx| self.in_map(tx, ty) && self.deposit_at(tx as usize, ty as usize) == Some(wanted))
        })
    }

    pub fn is_meadow(&self, x: i32, y: i32) -> bool {
        Self::in_grid(x, y) && self.map.terrain(x as usize, y as usize) & TERRAIN_MEADOW != 0
    }

    pub fn can_build(&self, x: i32, y: i32, size: usize) -> bool {
        (0..size as i32).all(|dy| (0..size as i32).all(|dx| self.is_free(x + dx, y + dy)))
    }

    /// Le bâtiment peut être posé là : terrain libre, et prairie pour une ferme.
    pub fn can_place(&self, kind: usize, x: i32, y: i32) -> bool {
        let Some(k) = self.kinds.get(kind) else { return false };
        let size = k.size as i32;
        match k.function {
            Function::Agora { grand } => self.agora_layout(x, y, grand).is_some(),
            // Une échoppe prend la place d'un emplacement d'agora, exactement.
            Function::Vendor(_) => self.plot_at(x, y).is_some(),
            Function::Stadium => self.can_build(x, y, k.size) && self.can_build(x, y + size, k.size),
            // Une seule piste : la première section se pose n'importe où, les suivantes
            // dans le prolongement d'une section déjà posée.
            Function::Racetrack => {
                let any = self.buildings.iter().flatten().any(|b| self.kinds[b.kind].function == Function::Racetrack);
                self.can_build(x, y, k.size) && (!any || self.racetrack_links(x, y) != 0)
            }
            // Sur une route, qu'il recouvre : route posée par le joueur, ou route déjà
            // présente sur la carte (bit de terrain, sans cellule `Cell::Road`).
            Function::Roadblock => self.is_built_road(x, y),
            Function::Gatehouse => {
                let tiles: Vec<(i32, i32)> = (0..size).flat_map(|dy| (0..size).map(move |dx| (x + dx, y + dy))).collect();
                tiles.iter().all(|&(tx, ty)| self.is_free(tx, ty) || self.is_built_road(tx, ty)) && tiles.iter().any(|&(tx, ty)| self.is_built_road(tx, ty))
            }
            Function::Fishery | Function::Wharf | Function::TradePost { sea: true } => {
                let near_water = (y - 1..=y + size).any(|ty| (x - 1..=x + size).any(|tx| Self::in_grid(tx, ty) && self.map.terrain(tx as usize, ty as usize) & TERRAIN_WATER != 0));
                self.can_build(x, y, k.size) && near_water
            }
            Function::Farm => self.can_build(x, y, k.size) && (0..size).all(|dy| (0..size).all(|dx| self.is_meadow(x + dx, y + dy))),
            // Chaque exploitation près de sa ressource, à moins de huit cases (ses
            // ouvriers vont y travailler) : arbres, carrière de marbre, cuivre, gisements
            // de marbre noir et d'orichalque.
            function if deposit_of(function).is_some_and(|wanted| !self.waived(wanted)) => {
                self.can_build(x, y, k.size) && deposit_of(function).is_some_and(|wanted| self.near_deposit(x, y, size, wanted))
            }
            // Un temple par héros.
            Function::HeroHall(hero) => {
                !self.buildings.iter().flatten().any(|b| self.kinds[b.kind].function == Function::HeroHall(hero))
                    && (0..k.depth as i32).all(|dy| (0..size).all(|dx| self.is_free(x + dx, y + dy)))
            }
            Function::Commemorative(model) => self.commemoratives.contains(&model) && self.can_build(x, y, k.size),
            // Un sanctuaire par dieu, et pas plus que l'épisode n'en permet.
            Function::Sanctuary { god, .. } => {
                let sanctuaries: Vec<u8> = self.buildings.iter().flatten().filter_map(|b| self.sanctuary_god(b.kind)).collect();
                sanctuaries.len() < self.max_sanctuaries
                    && !sanctuaries.contains(&god)
                    && (0..k.depth as i32).all(|dy| (0..size).all(|dx| self.is_free(x + dx, y + dy)))
            }
            _ => (0..k.depth as i32).all(|dy| (0..size).all(|dx| self.is_free(x + dx, y + dy))),
        }
    }

    /// Emplacement d'agora dont le coin est exactement cette case.
    fn plot_at(&self, x: i32, y: i32) -> Option<u16> {
        if !Self::in_grid(x, y) {
            return None;
        }
        let Cell::Building(id) = self.cell(x as usize, y as usize) else { return None };
        let building = self.buildings[id as usize].as_ref()?;
        (self.kinds[building.kind].function == Function::Plot && (building.x, building.y) == (x as usize, y as usize)).then_some(id)
    }

    /// Disposition d'une agora dont le premier emplacement a son coin en (x, y) : les
    /// coins des emplacements, alignés le long d'une route qui les borde sur toute leur
    /// longueur, et les cases de cette route. Une grande agora a une seconde rangée
    /// d'emplacements de l'autre côté de la route.
    fn agora_layout(&self, x: i32, y: i32, grand: bool) -> Option<(Vec<(i32, i32)>, Vec<(i32, i32)>)> {
        const PLOT: i32 = 2;
        for (dx, dy) in [(1, 0), (0, 1)] {
            // Rangée d'emplacements décalée de `across` cases en travers de la route.
            let row = |across: i32| -> Vec<(i32, i32)> {
                (0..AGORA_PLOTS).map(|i| (x + PLOT * i * dx + across * dy, y + PLOT * i * dy + across * dx)).collect()
            };
            let free = |plots: &[(i32, i32)]| plots.iter().all(|&(px, py)| self.can_build(px, py, PLOT as usize));
            let mut plots = row(0);
            if !free(&plots) {
                continue;
            }
            // La route longe l'un ou l'autre côté de la rangée.
            for side in [PLOT, -1] {
                let road: Vec<(i32, i32)> = (0..PLOT * AGORA_PLOTS).map(|i| (x + i * dx + side * dy, y + i * dy + side * dx)).collect();
                if !road.iter().all(|&(rx, ry)| self.is_road(rx, ry)) {
                    continue;
                }
                if grand {
                    let opposite = row(if side == PLOT { PLOT + 1 } else { -1 - PLOT });
                    if !free(&opposite) {
                        continue;
                    }
                    plots.extend(opposite);
                }
                return Some((plots, road));
            }
        }
        None
    }

    /// Pose une agora : trois emplacements, et la route qui les longe est dallée.
    fn place_agora(&mut self, kind: usize, x: i32, y: i32, grand: bool) -> bool {
        let Some(plot) = self.kinds.iter().position(|k| k.function == Function::Plot) else { return false };
        let Some((plots, road)) = self.agora_layout(x, y, grand) else { return false };
        let cost = self.kinds[kind].cost;
        if self.money < cost {
            return false;
        }
        // Les emplacements eux-mêmes sont gratuits : c'est l'agora qu'on paie.
        self.money += self.kinds[plot].cost * plots.len() as i32;
        for (px, py) in plots {
            self.place_building(plot, px, py);
        }
        for (rx, ry) in road {
            self.cells[ry as usize * GRID + rx as usize] = Cell::AgoraRoad;
        }
        self.money -= cost;
        true
    }

    /// Pose une route ; `false` si la case ne s'y prête pas ou si l'argent manque.
    pub fn place_road(&mut self, x: i32, y: i32, cost: i32) -> bool {
        if !self.is_free(x, y) || self.money < cost {
            return false;
        }
        self.cells[y as usize * GRID + x as usize] = Cell::Road;
        self.money -= cost;
        true
    }

    /// Pose un mur ; `false` si la case ne s'y prête pas ou si l'argent manque.
    pub fn place_wall(&mut self, x: i32, y: i32, cost: i32) -> bool {
        if !self.is_free(x, y) || self.money < cost {
            return false;
        }
        self.cells[y as usize * GRID + x as usize] = Cell::Wall(WALL_HEALTH);
        self.money -= cost;
        true
    }

    /// Tracé d'une route (ou d'un mur) de `from` à `to`, comme quand on glisse dans le
    /// jeu : le plus court chemin par les cases libres ou déjà de ce genre, en tournant
    /// le moins possible. Vide si `to` est inaccessible.
    pub fn plan_line(&self, from: (i32, i32), to: (i32, i32), wall: bool) -> Vec<(i32, i32)> {
        const STEP: u32 = 10;
        const TURN: u32 = 4;
        const MARGIN: i32 = 12;
        let passable = |x: i32, y: i32| self.is_free(x, y) || if wall { self.is_wall(x, y) } else { self.is_road(x, y) && !self.is_roadblock(x, y) };
        if !passable(from.0, from.1) || !passable(to.0, to.1) {
            return Vec::new();
        }
        let (left, top) = (from.0.min(to.0) - MARGIN, from.1.min(to.1) - MARGIN);
        let (width, height) = ((from.0 - to.0).abs() + 2 * MARGIN + 1, (from.1 - to.1).abs() + 2 * MARGIN + 1);
        const AXES: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
        // État : case et direction d'arrivée (4 = départ).
        let state = |x: i32, y: i32, d: usize| (((y - top) * width + (x - left)) as usize) * 5 + d;
        let mut cost = vec![u32::MAX; (width * height) as usize * 5];
        let mut previous = vec![usize::MAX; cost.len()];
        let mut queue = BinaryHeap::new();
        cost[state(from.0, from.1, 4)] = 0;
        queue.push(Reverse((0, from.0, from.1, 4usize)));
        while let Some(Reverse((c, x, y, d))) = queue.pop() {
            if c > cost[state(x, y, d)] {
                continue;
            }
            if (x, y) == to {
                let mut line = Vec::new();
                let mut at = state(x, y, d);
                loop {
                    let tile = at / 5;
                    line.push((left + (tile as i32 % width), top + tile as i32 / width));
                    at = previous[at];
                    if at == usize::MAX {
                        break;
                    }
                }
                line.reverse();
                return line;
            }
            for (nd, (dx, dy)) in AXES.iter().enumerate() {
                let (nx, ny) = (x + dx, y + dy);
                if nx < left || ny < top || nx >= left + width || ny >= top + height || !passable(nx, ny) {
                    continue;
                }
                let next = c + STEP + if d != 4 && d != nd { TURN } else { 0 };
                let index = state(nx, ny, nd);
                if next < cost[index] {
                    cost[index] = next;
                    previous[index] = state(x, y, d);
                    queue.push(Reverse((next, nx, ny, nd)));
                }
            }
        }
        Vec::new()
    }

    /// Montre un tracé en cours comme s'il était posé (sans rien payer), jusqu'à
    /// `hide_ghosts`.
    pub fn show_ghosts(&mut self, tiles: &[(i32, i32)], wall: bool) {
        self.hide_ghosts();
        for &(x, y) in tiles {
            if self.is_free(x, y) {
                let index = y as usize * GRID + x as usize;
                self.ghosts.push((index, self.cells[index]));
                self.cells[index] = if wall { Cell::Wall(WALL_HEALTH) } else { Cell::Road };
            }
        }
    }

    pub fn hide_ghosts(&mut self) {
        for (index, cell) in self.ghosts.drain(..).rev() {
            self.cells[index] = cell;
        }
    }

    /// La case porte un mur.
    pub fn is_wall(&self, x: i32, y: i32) -> bool {
        Self::in_grid(x, y) && matches!(self.cell(x as usize, y as usize), Cell::Wall(_))
    }

    pub fn place_building(&mut self, kind: usize, x: i32, y: i32) -> bool {
        if !self.can_place(kind, x, y) {
            return false;
        }
        let mut y = y;
        match self.kinds[kind].function {
            Function::Agora { grand } => return self.place_agora(kind, x, y, grand),
            // Stade : l'annexe au fond, la moitié qui compte devant elle.
            Function::Stadium => {
                let Some(annex) = self.kinds.iter().position(|k| k.function == Function::Annex) else { return false };
                if self.money < self.kinds[kind].cost + self.kinds[annex].cost || !self.place_building(annex, x, y) {
                    return false;
                }
                y += self.kinds[kind].size as i32;
            }
            Function::Vendor(_) => {
                if self.money < self.kinds[kind].cost {
                    return false;
                }
                // L'emplacement cède la place à l'échoppe.
                if let Some(plot) = self.plot_at(x, y) {
                    self.remove_building(plot, Cell::Terrain);
                }
            }
            _ => {}
        }
        let k = &self.kinds[kind];
        let (size, depth, cost, function) = (k.size, k.depth, k.cost, k.function);
        if self.money < cost || self.buildings.len() >= u16::MAX as usize {
            return false;
        }
        let id = self.buildings.len() as u16;
        let first_level = if size == ELITE_SIZE { ELITE_FIRST_LEVEL } else { 0 };
        let house = (function == Function::House)
            .then_some(House { level: first_level, population: 0, arriving: 0, covered_until: [0; SERVICES.len()], goods: [0; GOODS.len()], crime: 0, ate: false, hungry: 0 });
        self.buildings.push(Some(Building {
            kind,
            x: x as usize,
            y: y as usize,
            house,
            stock: 0,
            raw: 0,
            goods: [0; GOODS.len()],
            foods: [0; FOODS],
            walks: 0,
            fire: 0,
            damage: 0,
            next_walk: self.ticks + ROUNDS_INTERVAL / 4,
            sown_at: self.ticks,
            errand_out: false,
            staffed: self.workers_available() >= self.workers_needed() + self.kinds[kind].employees,
            workers: if self.workers_available() >= self.workers_needed() + self.kinds[kind].employees { self.kinds[kind].employees } else { 0 },
        }));
        for dy in 0..depth {
            for dx in 0..size {
                self.cells[(y as usize + dy) * GRID + x as usize + dx] = Cell::Building(id);
            }
        }
        self.money -= cost;
        // Un monument commémoratif posé n'est plus à poser.
        if let Function::Commemorative(model) = self.kinds[kind].function {
            if let Some(i) = self.commemoratives.iter().position(|&m| m == model) {
                self.commemoratives.remove(i);
            }
        }
        self.rebuild_appeal();
        true
    }

    /// Retire la route ou le bâtiment de la case ; `false` s'il n'y avait rien.
    pub fn demolish(&mut self, x: i32, y: i32) -> bool {
        if !Self::in_grid(x, y) {
            return false;
        }
        match self.cell(x as usize, y as usize) {
            Cell::Terrain | Cell::Fire(_) | Cell::Chasm(_) | Cell::Lava(_) | Cell::Flood => false,
            Cell::Rubble(_) | Cell::Road | Cell::AgoraRoad | Cell::Wall(_) => {
                self.cells[y as usize * GRID + x as usize] = Cell::Terrain;
                true
            }
            Cell::Building(id) => {
                // Une échoppe démolie rend son emplacement à l'agora.
                let vendor = self.buildings[id as usize].as_ref().filter(|b| matches!(self.kinds[b.kind].function, Function::Vendor(_))).map(|b| (b.x, b.y));
                // Un barrage démoli rend sa route.
                let left = if self.function(id as usize) == Function::Roadblock { Cell::Road } else { Cell::Terrain };
                self.remove_building(id, left);
                if let (Some((bx, by)), Some(plot)) = (vendor, self.kinds.iter().position(|k| k.function == Function::Plot)) {
                    self.money += self.kinds[plot].cost;
                    self.place_building(plot, bx as i32, by as i32);
                }
                true
            }
        }
    }

    /// Fait disparaître un bâtiment et les personnages qui en dépendent ; son emprise
    /// devient `left` (terrain nu, décombres ou flammes, la variante étant tirée au sort).
    fn remove_building(&mut self, id: u16, left: Cell) {
        self.buildings[id as usize] = None;
        for index in 0..self.cells.len() {
            if self.cells[index] != Cell::Building(id) {
                continue;
            }
            let variant = (self.next_random() % RUBBLE_IMAGES as u32) as u8;
            self.cells[index] = match left {
                Cell::Rubble(_) => Cell::Rubble(variant),
                Cell::Fire(_) => {
                    self.fires.push(((index % GRID) as u16, (index / GRID) as u16, self.ticks + FIRE_TICKS));
                    Cell::Fire(variant)
                }
                other => other,
            };
        }
        // Les personnages liés à ce bâtiment disparaissent avec lui.
        self.figures.retain(|f| match f.role {
            Role::Immigrant { home, .. } => home != id,
            Role::Deliver { to, from, .. } => to != id && from != id,
            Role::Supply { to, .. } => to != id,
            Role::Fetch { source, vendor } => source != id && vendor != id,
            Role::Rounds { home, .. } | Role::Return { home, .. } => home != id,
            Role::Monster { .. } | Role::Soldier { .. } | Role::Ship => true,
            Role::Hero { hall, .. } => hall != id,
            Role::God { .. } | Role::Visit { .. } => true,
        });
        self.rebuild_appeal();
    }

    /// Stade de pousse des champs d'une ferme (0 = semis).
    fn crop_stage(&self, building: &Building) -> usize {
        let grown = (self.ticks - building.sown_at).min(HARVEST_TICKS - 1);
        (grown * CROP_STAGES / HARVEST_TICKS) as usize
    }

    /// Image portée par la case, ou `None` si elle est couverte par un objet voisin.
    pub fn draw(&self, x: usize, y: usize) -> Option<Draw> {
        match self.cell(x, y) {
            Cell::Terrain => {
                let (id, edge) = self.terrain_at(x, y);
                // Route déjà présente sur la carte : son image est recalculée d'après ses
                // voisines, comme une route posée par le joueur, sinon une route nouvelle
                // qui s'y raccorde ne paraît pas jointe (et le repavage d'une vue tournée
                // ne la dessine pas).
                if self.map.terrain(x, y) & TERRAIN_ROAD != 0 {
                    return Some(self.road_draw(x, y));
                }
                if id == 0 || edge & EDGE_DRAW_TILE == 0 {
                    return None;
                }
                self.map.resolve(id).map(|(source, image)| Draw { source, image, burning: false, overlay: None, ground: None, lift: 0, above: None })
            }
            Cell::Rubble(variant) | Cell::Fire(variant) => Some(Draw {
                source: Source::Terrain,
                image: RUBBLE_FIRST_IMAGE + variant as usize,
                burning: matches!(self.cell(x, y), Cell::Fire(_)),
                overlay: None,
                ground: None,
                lift: 0,
                above: None,
            }),
            Cell::Chasm(variant) => Some(Draw {
                source: Source::Terrain,
                image: {
                    let open = |dx: i32, dy: i32| {
                        let (nx, ny) = self.beside(x, y, dx, dy);
                        Self::in_grid(nx, ny) && matches!(self.cell(nx as usize, ny as usize), Cell::Chasm(_))
                    };
                    let shape = NEIGHBOURS.iter().enumerate().filter(|&(_, &(dx, dy))| open(dx, dy)).fold(0, |mask, (bit, _)| mask | 1 << bit);
                    let pieces = CHASM_PIECES[shape];
                    pieces[variant as usize % pieces.len()]
                },
                burning: false,
                overlay: None,
                ground: self.map.resolve(self.terrain_at(x, y).0),
                lift: 0,
                above: None,
            }),
            Cell::Lava(variant) => Some(Draw {
                source: Source::Terrain,
                image: LAVA_FIRST_IMAGE + variant as usize % LAVA_IMAGES,
                burning: false,
                overlay: None,
                ground: None,
                lift: 0,
                above: None,
            }),
            Cell::Flood => Some(Draw { source: self.water_image().0, image: self.water_image().1, burning: false, overlay: None, ground: None, lift: 0, above: None }),
            Cell::AgoraRoad => Some(Draw {
                source: Source::General,
                image: AGORA_ROAD_FIRST_IMAGE + (x + y) % AGORA_ROAD_IMAGES,
                burning: false,
                overlay: None,
                ground: None,
                lift: 0,
                above: None,
            }),
            Cell::Wall(_) => {
                let around = WALL_NEIGHBOURS.map(|(dx, dy)| {
                    let (nx, ny) = self.beside(x, y, dx, dy);
                    self.is_wall(nx, ny)
                });
                let offset = WALL_PATTERNS.iter().find(|(wanted, _)| wanted.iter().zip(around).all(|(&w, wall)| w == 2 || (w == 1) == wall)).map_or(0, |&(_, offset)| offset);
                Some(Draw { source: Source::General, image: WALL_FIRST_IMAGE + offset, burning: false, overlay: None, ground: None, lift: 0, above: None })
            }
            Cell::Road => Some(self.road_draw(x, y)),
            Cell::Building(id) => {
                let building = self.buildings[id as usize].as_ref()?;
                let kind = &self.kinds[building.kind];
                // Position de la case dans le rectangle du bâtiment tel qu'il paraît à
                // l'écran : les bâtiments n'ont qu'une vue, la rotation ne tourne que leur
                // emprise.
                let (rx, ry) = self.on_screen(building, x, y);
                // Sanctuaire en chantier : autel et statues ne sont pas encore là, chacune
                // de leurs cases montre du dallage.
                if matches!(kind.function, Function::Sanctuary { .. }) && !self.is_complete(building) {
                    let hidden = kind.parts.iter().any(|p| {
                        (p.dx..p.dx + p.size).contains(&rx)
                            && (p.dy..p.dy + p.size).contains(&ry)
                            && !matches!(p.source, Source::Sanctuary(_))
                            && !SANCTUARY_FLOOR_IMAGES.contains(&p.image)
                    });
                    if hidden {
                        let image = SANCTUARY_PLAIN_IMAGE + (x + y) % 6;
                        return Some(Draw { source: Source::General, image, burning: false, overlay: None, ground: None, lift: 0, above: None });
                    }
                }
                if kind.function == Function::Racetrack {
                    if (rx, ry) != (0, RACETRACK_SIZE - 1) {
                        return None;
                    }
                    let links = self.racetrack_links(building.x as i32, building.y as i32);
                    let variant = (building.x / RACETRACK_SIZE + building.y / RACETRACK_SIZE) % 2;
                    let image = match RACETRACK_CURVES.iter().find(|&&(mask, _)| mask == links) {
                        Some(&(_, image)) => image,
                        None if links & 0b0101 != 0 && links & 0b1010 == 0 => RACETRACK_ALONG_Y[variant],
                        None => RACETRACK_ALONG_X[variant],
                    };
                    return Some(Draw { source: Source::Poseidon, image, burning: false, overlay: None, ground: None, lift: 0, above: None });
                }
                // Corps de garde : la seconde image quand la route le traverse le long de x.
                if kind.function == Function::Gatehouse {
                    let part = kind.parts.first()?;
                    if (rx, ry) != (0, part.size - 1) {
                        return None;
                    }
                    let (bx, by, size) = (building.x as i32, building.y as i32, kind.size as i32);
                    let along_x = (0..size).any(|dy| self.is_road(bx - 1, by + dy) || self.is_road(bx + size, by + dy)) != (self.orientation & 2 != 0);
                    let image = part.image + along_x as usize;
                    return Some(Draw { source: part.source, image, burning: false, overlay: None, ground: None, lift: 0, above: None });
                }
                if let Function::Monument { model, needs } = kind.function {
                    let layout = self.monument_layouts.get(model as usize)?;
                    let tile = layout.tiles.iter().find(|t| (t.dx, t.dy) == (rx, ry))?;
                    let stage = self.monument_stage(building, needs, layout.stages());
                    return Some(self.monument_draw(layout, &kind.parts, tile, stage));
                }
                // Chaque élément est porté par sa case la plus à gauche à l'écran. Un
                // bâtiment non carré vu de côté prend sa disposition transposée.
                let turned = kind.size != kind.depth && self.orientation & 2 != 0;
                let part = if turned {
                    kind.parts.iter().find(|p| rx == p.dy && ry == p.dx + p.size - 1)?
                } else {
                    kind.parts.iter().find(|p| rx == p.dx && ry == p.dy + p.size - 1)?
                };
                let mut bay_source = None;
                let image = match (&building.house, part.crop, part.bay) {
                    // Logement luxueux : le rang de la baie désigne le quart du bâtiment.
                    (Some(house), _, Some(quadrant)) if house.level >= ELITE_FIRST_LEVEL => {
                        let level = (house.level - ELITE_FIRST_LEVEL).min(ELITE_LEVELS - 1);
                        ELITE_FIRST_IMAGE + ELITE_PIECES * level + ELITE_QUADRANT_PIECE[quadrant as usize % 4]
                    }
                    (Some(house), ..) => HOUSE_IMAGES[house.level.min(HOUSE_IMAGES.len() - 1)],
                    (None, true, _) => part.image + self.crop_stage(building),
                    (None, false, Some(bay)) => {
                        let (source, image) = bay_image(&building.goods, bay as usize);
                        bay_source = Some(source);
                        image
                    }
                    (None, false, None) if turned => self.turned_images.iter().find(|&&(from, _)| from == part.image).map_or(part.image, |&(_, to)| to),
                    (None, false, None) => part.image,
                };
                let overlay = kind.stock_overlay.filter(|_| building.stock > 0);
                let ground = part.crop.then(|| self.map.resolve(self.terrain_at(x, y).0)).flatten();
                let (source, image) = match kind.function {
                    Function::Sanctuary { needs, .. } if !self.is_complete(building) => {
                        // Chantier : le temple passe par deux états d'échafaudage ; le
                        // reste n'est que dallage.
                        let delivered: i32 = SANCTUARY_MATERIALS.iter().map(|&good| building.goods[good as usize]).sum();
                        let needed = needs.iter().map(|&n| n as i32).sum::<i32>().max(1);
                        match part.source {
                            Source::Sanctuary(_) => (Source::Sanctuary((2 * delivered >= needed) as u8), image),
                            _ => (Source::General, image),
                        }
                    }
                    _ => (bay_source.unwrap_or(part.source), image),
                };
                Some(Draw { source, image, burning: false, overlay, ground, lift: 0, above: None })
            }
        }
    }

    /// Route : son image suit les routes voisines à l'écran.
    fn road_draw(&self, x: usize, y: usize) -> Draw {
        let mut borders = 0;
        for (bit, &(dx, dy)) in NEIGHBOURS.iter().enumerate() {
            let (nx, ny) = self.beside(x, y, dx, dy);
            if !self.is_road(nx, ny) {
                borders |= 1 << bit;
            }
        }
        Draw { source: Source::Terrain, image: ROAD_FIRST_IMAGE + ROAD_VARIANT[borders], burning: false, overlay: None, ground: None, lift: 0, above: None }
    }

    /// Image et bords du terrain d'une case pour l'orientation de la vue.
    pub fn terrain_at(&self, x: usize, y: usize) -> (u32, u8) {
        match &self.terrain_view {
            Some(view) if self.orientation != 0 => (view.images[y * GRID + x], view.edges[y * GRID + x]),
            _ => (self.map.image(x, y), self.map.edge(x, y)),
        }
    }

    /// Coin de carte d'une emprise de `w` × `d` cases qui commence, à l'écran, sur la case
/// de carte `at` (vue d'orientation `orientation`).
pub fn footprint_corner(orientation: u8, at: (i32, i32), w: i32, d: i32) -> (i32, i32) {
    let (hx, hy) = rotate(orientation, at.0, at.1);
    let n = GRID as i32;
    match orientation & 6 {
        2 => (n - w - hy, hx),
        4 => (n - w - hx, n - d - hy),
        6 => (hy, n - d - hx),
        _ => (hx, hy),
    }
}

/// Case voisine de (x, y) dans la direction (dx, dy) vue à l'écran.
    fn beside(&self, x: usize, y: usize, dx: i32, dy: i32) -> (i32, i32) {
        let (sx, sy) = rotate(self.orientation, x as i32, y as i32);
        unrotate(self.orientation, sx + dx, sy + dy)
    }

    /// Position d'une case dans le rectangle qu'occupe le bâtiment à l'écran.
    fn on_screen(&self, building: &Building, x: usize, y: usize) -> (usize, usize) {
        let kind = &self.kinds[building.kind];
        let (w, d) = (kind.size, kind.depth);
        let (mx, my) = (x - building.x, y - building.y);
        match self.orientation & 6 {
            2 => (my, w - 1 - mx),
            4 => (w - 1 - mx, d - 1 - my),
            6 => (d - 1 - my, mx),
            _ => (mx, my),
        }
    }

    pub fn population(&self) -> i32 {
        self.buildings.iter().flatten().filter_map(|b| b.house.as_ref()).map(|h| h.population).sum()
    }

    /// Habitants des maisons de ce niveau de logement ou d'un niveau supérieur.
    pub fn people_housed_from(&self, level: usize) -> i32 {
        self.buildings.iter().flatten().filter_map(|b| b.house.as_ref()).filter(|h| h.level >= level).map(|h| h.population).sum()
    }

    /// Meilleure production d'une année de cette ressource, l'année en cours comprise.
    pub fn best_production(&self, resource: usize) -> i32 {
        match (self.best_produced.get(resource), self.produced.get(resource)) {
            (Some(&best), Some(&now)) => best.max(now),
            _ => 0,
        }
    }

    /// Un nouvel épisode commence : comme le jeu, on oublie les meilleures années et
    /// ce qui a été mis de côté.
    pub fn start_episode(&mut self) {
        self.best_year_profit = None;
        self.best_produced = [0; RESOURCES];
        self.set_aside = [0; RESOURCES];
    }

    /// Un bâtiment de la cité sait produire cette ressource.
    pub fn produces(&self, resource: i32) -> bool {
        resource > 0 && self.kinds.iter().any(|k| k.product as i32 == resource)
    }

    /// La ressource se range au grenier ou à l'entrepôt (les chevaux restent au haras).
    pub fn can_store(&self, resource: i32) -> bool {
        (1..FOODS as i32).contains(&resource) && self.produces(resource)
            || GOODS.iter().any(|&g| g != Good::Horses && resource_of(g).is_some_and(|r| r as i32 == resource))
    }

    /// Quantité d'une ressource en réserve : vivres des greniers, marchandises des entrepôts.
    pub fn stored(&self, resource: usize) -> i32 {
        let good = GOODS.iter().position(|&g| resource_of(g) == Some(resource as u8));
        self.buildings
            .iter()
            .flatten()
            .map(|b| match (storage(self.kinds[b.kind].function), good) {
                (Function::Granary, _) if (1..FOODS).contains(&resource) => b.foods[resource],
                (Function::Storehouse, Some(good)) => b.goods[good],
                _ => 0,
            })
            .sum()
    }

    /// Met de côté `amount` d'une ressource, prise dans les greniers ou les entrepôts.
    /// Comme dans le jeu, tout ou rien : faux si la réserve ne suffit pas.
    pub fn set_aside(&mut self, resource: usize, amount: i32) -> bool {
        if amount <= 0 || resource >= RESOURCES || self.stored(resource) < amount {
            return false;
        }
        self.take(resource, amount);
        self.set_aside[resource] += amount;
        true
    }

    /// Prend jusqu'à `amount` d'une ressource dans les greniers ou les entrepôts ; rend
    /// ce qui a été pris.
    fn take(&mut self, resource: usize, amount: i32) -> i32 {
        let good = GOODS.iter().position(|&g| resource_of(g) == Some(resource as u8));
        let mut left = amount;
        for id in 0..self.buildings.len() {
            let function = match &self.buildings[id] {
                Some(b) => storage(self.kinds[b.kind].function),
                None => continue,
            };
            let Some(b) = self.buildings[id].as_mut() else { continue };
            let taken = match (function, good) {
                (Function::Granary, _) if resource < FOODS => {
                    let taken = b.foods[resource].min(left);
                    b.foods[resource] -= taken;
                    b.stock -= taken;
                    taken
                }
                (Function::Storehouse, Some(good)) => {
                    let taken = b.goods[good].min(left);
                    b.goods[good] -= taken;
                    taken
                }
                _ => 0,
            };
            left -= taken;
        }
        amount - left
    }

    /// Dieu du sanctuaire qu'est ce type de bâtiment.
    fn sanctuary_god(&self, kind: usize) -> Option<u8> {
        match self.kinds[kind].function {
            Function::Sanctuary { god, .. } => Some(god),
            _ => None,
        }
    }

    /// La cité a achevé le sanctuaire de ce dieu.
    fn has_sanctuary(&self, god: u8) -> bool {
        self.buildings.iter().flatten().any(|b| self.sanctuary_god(b.kind) == Some(god) && self.is_complete(b))
    }

    /// Le sanctuaire a reçu tous ses matériaux.
    pub fn is_complete(&self, building: &Building) -> bool {
        match self.kinds[building.kind].function {
            Function::Sanctuary { needs, .. } | Function::Monument { needs, .. } => {
                SANCTUARY_MATERIALS.iter().zip(needs).all(|(&good, need)| building.goods[good as usize] >= need as i32)
            }
            _ => true,
        }
    }

    /// Côtés d'une section posée en (x, y) qui touchent une autre section, alignée.
    fn racetrack_links(&self, x: i32, y: i32) -> u8 {
        let step = RACETRACK_SIZE as i32;
        NEIGHBOURS.iter().enumerate().fold(0, |mask, (bit, &(dx, dy))| {
            let (nx, ny) = (x + dx * step, y + dy * step);
            let linked = self.building_at(nx, ny).is_some_and(|b| self.kinds[b.kind].function == Function::Racetrack && (b.x as i32, b.y as i32) == (nx, ny));
            mask | (linked as u8) << bit
        })
    }

    /// Hippodrome : nombre de sections (un stade chacune, d'après Olympe) et piste
    /// fermée — chaque section reliée à deux autres, toutes d'un seul tenant.
    pub fn hippodrome(&self) -> (i32, bool) {
        let sections: Vec<(i32, i32)> = self.buildings.iter().flatten().filter(|b| self.kinds[b.kind].function == Function::Racetrack).map(|b| (b.x as i32, b.y as i32)).collect();
        if sections.len() < 4 || sections.iter().any(|&(x, y)| self.racetrack_links(x, y).count_ones() != 2) {
            return (sections.len() as i32, false);
        }
        let step = RACETRACK_SIZE as i32;
        let mut seen = vec![sections[0]];
        let mut queue = vec![sections[0]];
        while let Some((x, y)) = queue.pop() {
            for &(dx, dy) in &NEIGHBOURS {
                let next = (x + dx * step, y + dy * step);
                if sections.contains(&next) && !seen.contains(&next) {
                    seen.push(next);
                    queue.push(next);
                }
            }
        }
        (sections.len() as i32, seen.len() == sections.len())
    }

    /// Navires à quai : trières, ou frégates dans une cité atlante.
    pub fn triremes(&self) -> i32 {
        self.buildings.iter().flatten().filter(|b| self.kinds[b.kind].function == Function::Wharf).map(|b| b.stock.clamp(0, 1)).sum()
    }

    /// Nom d'un navire de guerre, au singulier, selon la nation de la cité.
    pub fn ship_singular(&self) -> &'static str {
        if self.atlantean { "frégate" } else { "trière" }
    }

    /// Nom d'un navire de guerre, au pluriel, selon la nation de la cité.
    pub fn ship_plural(&self) -> &'static str {
        if self.atlantean { "frégates" } else { "trières" }
    }

    /// Nom d'un bâtiment tel que la cité l'appelle : dans une cité atlante, le quai
    /// des navires est un quai à frégates.
    pub fn kind_name(&self, kind: usize) -> String {
        let kind = &self.kinds[kind];
        if self.atlantean && kind.function == Function::Wharf { "quai à frégates".to_string() } else { kind.name.clone() }
    }

    /// Monuments achevés de ce rang, ou de tous les rangs.
    pub fn monuments_built(&self, model: Option<u8>) -> i32 {
        let built = self.buildings.iter().flatten().filter(|b| self.is_complete(b));
        built.filter(|b| matches!(self.kinds[b.kind].function, Function::Monument { model: m, .. } if model.is_none_or(|wanted| wanted == m))).count() as i32
    }

    /// Étapes d'un monument déjà élevées, d'après la part des matériaux livrée.
    fn monument_stage(&self, building: &Building, needs: [u16; 3], stages: u16) -> u16 {
        if self.is_complete(building) {
            return stages;
        }
        let delivered: i32 = SANCTUARY_MATERIALS.iter().zip(needs).map(|(&good, need)| building.goods[good as usize].min(need as i32)).sum();
        let needed = needs.iter().map(|&n| n as i32).sum::<i32>().max(1);
        (delivered * stages as i32 / needed) as u16
    }

    /// Case d'un monument, à sa hauteur (un niveau d'élévation vaut 30 pixels) :
    /// une pente raide prend l'image tournée vers ses côtés extérieurs (ceux où la
    /// voisine est plus basse ; faces au sud-ouest et au nord-ouest claires, au
    /// sud-est et au nord-est grises), une pente sans côté extérieur est le sommet
    /// plat de son étage, une plateforme une dalle. Les étages intérieurs, cachés
    /// par ceux qui les entourent, n'ont pas besoin de socle. Une case pas encore
    /// élevée montre le sol du chantier.
    fn monument_draw(&self, layout: &crate::monument::Layout, parts: &[Part], tile: &crate::monument::Tile, stage: u16) -> Draw {
        use crate::monument::*;
        let plain = |image: usize, lift: u16| Draw { source: Source::Poseidon, image, burning: false, overlay: None, ground: None, lift, above: None };
        let level = |levels: u8| levels as u16 * MONUMENT_LEVEL_PIXELS;
        if tile.piece == ACCESS_PAD {
            return plain(MONUMENT_PAD_IMAGE, 0);
        }
        if tile.order > stage {
            return plain(MONUMENT_FLOOR_IMAGE, 0);
        }
        // Hauteur du sommet d'une case voisine, déjà élevée ; hors du monument, le sol.
        let top_at = |dx: i32, dy: i32| -> i32 {
            let (x, y) = (tile.dx as i32 + dx, tile.dy as i32 + dy);
            layout.tiles.iter().find(|t| (t.dx as i32, t.dy as i32) == (x, y) && t.order <= stage).map_or(-1, |t| t.top() as i32)
        };
        match tile.piece {
            STEEP_FIRST..=STEEP_LAST | LIFT => {
                let outward = NEIGHBOURS.iter().enumerate().filter(|&(_, &(dx, dy))| top_at(dx, dy) < tile.top() as i32).fold(0u8, |mask, (bit, _)| mask | 1 << bit);
                if outward == 0 {
                    return plain(MONUMENT_TILE_IMAGE, level(tile.top()));
                }
                let decorated = MONUMENT_DECORATIONS.iter().find(|&&(piece, side, _)| piece == tile.piece && outward == side).map(|&(_, _, image)| image);
                // Forme rare (pente ouverte sur des côtés opposés, au bord d'une
                // plateforme intérieure) : la face la plus visible, sud-ouest d'abord.
                let image = decorated
                    .or_else(|| MONUMENT_SLOPES.iter().find(|&&(side, _)| side == outward).map(|&(_, image)| image))
                    .or_else(|| [0b0100u8, 0b0010, 0b1000, 0b0001].into_iter().find(|&side| outward & side != 0).and_then(|side| MONUMENT_SLOPES.iter().find(|&&(s, _)| s == side)).map(|&(_, image)| image))
                    .unwrap_or(MONUMENT_SOLID_IMAGE);
                plain(image, level(tile.elevation))
            }
            // Grande pièce (temple, autel, grande statue) : portée par sa case la plus à
            // gauche à l'écran, comme un bâtiment.
            0 if parts.iter().any(|p| p.size > 1 && (p.dx, p.dy + p.size - 1) == (tile.dx, tile.dy)) => {
                let part = parts.iter().find(|p| p.size > 1 && (p.dx, p.dy + p.size - 1) == (tile.dx, tile.dy)).unwrap();
                Draw { source: part.source, image: part.image, ..plain(0, level(tile.elevation)) }
            }
            0 | TILE_PLAIN | TILE_ORICHALC | 55 if tile.elevation == 0 => plain(MONUMENT_FLOOR_IMAGE, 0),
            0 | TILE_PLAIN | TILE_ORICHALC | 55 => plain(MONUMENT_TILE_IMAGE, level(tile.elevation)),
            // Petite statue d'une case : posée sur sa dalle. Les grandes pièces sont
            // dessinées depuis leur case la plus à gauche ; leur case d'ancrage n'a que
            // la dalle.
            _ => match parts.iter().find(|p| (p.dx, p.dy) == (tile.dx, tile.dy) && p.source != Source::Poseidon) {
                Some(part) if part.size == 1 => Draw { above: Some((part.source, part.image, level(tile.elevation))), ..plain(MONUMENT_TILE_IMAGE, level(tile.elevation)) },
                Some(_) => plain(MONUMENT_TILE_IMAGE, level(tile.elevation)),
                // Sommet de pyramide, observatoire, musée : obélisque.
                None => Draw { above: Some((Source::Poseidon, MONUMENT_STATUE_IMAGE, level(tile.elevation))), ..plain(MONUMENT_TILE_IMAGE, level(tile.elevation)) },
            },
        }
    }

    /// Dieux dont la cité a un sanctuaire achevé.
    pub fn sanctuaries(&self) -> Vec<u8> {
        self.buildings.iter().flatten().filter(|b| self.is_complete(b)).filter_map(|b| self.sanctuary_god(b.kind)).collect()
    }

    /// Héros : chaque mois, ceux dont le temple est bâti et les exigences remplies
    /// viennent dans la cité ; un héros présent affronte le monstre qui lui revient,
    /// puis part pour sa quête. Le jeu laisse le joueur l'y envoyer ; Olympe le fait
    /// partir de lui-même, et les durées (un mois de combat, six de quête) sont les
    /// siennes.
    fn call_heroes(&mut self) {
        let now = self.ticks;
        for building in self.buildings.iter().flatten() {
            let Function::HeroHall(hero) = self.kinds[building.kind].function else { continue };
            if self.heroes[hero as usize] {
                continue;
            }
            let hall = (building.x, building.y, self.kinds[building.kind].size);
            if crate::heroes::requirements(hero).all(|r| r.check(self, hall).passes()) {
                self.heroes[hero as usize] = true;
                self.hero_news.push(HeroNews::Arrived(hero));
            }
        }
        let mut busy = [false; crate::heroes::HEROES];
        for monster in self.monsters.iter_mut().filter(|m| !m.slain && m.arrives_at <= now) {
            let Some(hero) = crate::heroes::hero_against(monster.monster).filter(|&h| self.heroes[h as usize]) else { continue };
            busy[hero as usize] = true;
            match monster.fight_until {
                None => {
                    monster.fight_until = Some(now + HERO_FIGHT_TICKS);
                    self.hero_news.push(HeroNews::Fighting(monster.event));
                }
                Some(until) if until <= now => {
                    monster.slain = true;
                    self.hero_news.push(HeroNews::Slain(monster.event));
                }
                Some(_) => {}
            }
        }
        for quest in &mut self.quests {
            match quest.state {
                QuestState::Waiting if self.heroes[quest.hero as usize] && !busy[quest.hero as usize] => {
                    quest.state = QuestState::Away { until: now + QUEST_TICKS };
                    busy[quest.hero as usize] = true;
                    self.hero_news.push(HeroNews::Departed(quest.event));
                }
                QuestState::Away { until } if until <= now => {
                    quest.state = QuestState::Done;
                    self.hero_news.push(HeroNews::QuestDone(quest.event));
                    self.commemoratives.push(COMMEMORATIVE_HERO);
                }
                QuestState::Away { .. } => busy[quest.hero as usize] = true,
                _ => {}
            }
        }
    }

    /// Soldats que la cité entretient de ce rang ou d'un rang supérieur ; un logement
    /// en fournit au prorata de ses habitants.
    pub fn soldiers(&self, at_least: Soldier) -> i32 {
        self.buildings
            .iter()
            .flatten()
            .filter_map(|b| b.house.as_ref())
            .filter_map(|house| {
                let &(soldier, full) = SOLDIERS.get(house.level)?;
                let capacity = self.capacity(house).max(1);
                (soldier >= at_least).then(|| full * house.population.min(capacity) / capacity)
            })
            .sum()
    }

    /// Pas de simulation de `month` (0 à 11) d'une année tirée entre `years`, comptées
    /// depuis l'année en cours.
    pub fn date_in(&mut self, month: u8, years: (i16, i16)) -> u64 {
        let year_now = self.ticks / (TICKS_PER_MONTH * 12);
        let (low, high) = (years.0.max(0) as u64, years.1.max(years.0).max(0) as u64);
        let year = low + self.next_random() as u64 % (high - low + 1);
        ((year_now + year) * 12 + month as u64) * TICKS_PER_MONTH
    }

    /// Programme une invasion : le point d'invasion est tiré entre `points` (0 : au
    /// choix d'Olympe) et rangé avec le changement.
    pub fn schedule_invasion(&mut self, cities: (i16, i16), soldiers: (i16, i16), points: (i16, i16), month: u8, years: (i16, i16)) {
        let at = self.date_in(month, years);
        let (low, high) = (points.0.max(0) as u32, points.1.max(points.0).max(0) as u32);
        let point = (low + self.next_random() % (high - low + 1)).min(u8::MAX as u32) as u8;
        self.scheduled.push(Scheduled { at, change: INVASION, cities, amount: soldiers, resource: point });
    }

    /// Programme un changement du monde : `month` (0 à 11) d'une année tirée entre
    /// `years` (comptées depuis l'année en cours) ; tout de suite s'il est déjà passé.
    pub fn schedule(&mut self, change: u8, cities: (i16, i16), amount: (i16, i16), month: u8, years: (i16, i16)) {
        let at = self.date_in(month, years);
        self.scheduled.push(Scheduled { at, change, cities, amount, resource: 0 });
    }

    /// Satisfait la demande de ce rang : paie, envoie l'armée ou prend les
    /// marchandises en réserve ; faux si la cité ne peut pas être servie.
    pub fn fulfil(&mut self, index: usize) -> bool {
        let Some(request) = self.requests.get(index).copied() else { return false };
        let done = match request.resource {
            DRACHMAS if self.money >= request.amount => {
                self.money -= request.amount;
                true
            }
            TROOPS if self.army() >= request.amount && self.ticks >= self.campaign_until => {
                self.campaign_until = self.ticks + CAMPAIGN_TICKS;
                true
            }
            ANY_FOOD if (1..FOODS).map(|r| self.stored(r)).sum::<i32>() >= request.amount => {
                let mut left = request.amount;
                for food in 1..FOODS {
                    left -= self.take(food, left);
                }
                true
            }
            r if (r as usize) < RESOURCES && r != DRACHMAS && r != TROOPS && r != ANY_FOOD && self.stored(r as usize) >= request.amount => {
                self.take(r as usize, request.amount);
                true
            }
            _ => false,
        };
        if done {
            self.requests.remove(index);
            if let Some(city) = self.cities.get_mut(request.city) {
                city.favor = (city.favor + FAVOR_FOR_SERVICE).min(100);
            }
        }
        done
    }

    /// Ce qu'il y a pour une demande : drachmes, armée, vivres ou marchandise en réserve.
    pub fn available_for(&self, resource: u8) -> i32 {
        match resource {
            DRACHMAS => self.money,
            TROOPS => self.army(),
            ANY_FOOD => (1..FOODS).map(|r| self.stored(r)).sum(),
            r => self.stored(r as usize),
        }
    }

    /// Changements du monde arrivés à échéance.
    fn change_world(&mut self) {
        let now = self.ticks;
        // Demandes restées sans réponse.
        let expired: Vec<Request> = self.requests.iter().filter(|r| r.until <= now).copied().collect();
        self.requests.retain(|r| r.until > now);
        for request in expired {
            self.request_news.push(RequestNews::Expired(request));
            let Some(city) = self.cities.get_mut(request.city) else { continue };
            city.favor = (city.favor - FAVOR_FOR_NEGLECT).max(0);
            // Une cité fâchée se détourne : l'alliée devient rivale, la vassale se rebelle.
            if city.favor < FAVOR_THRESHOLD && (city.status == crate::map::ALLY || city.status == VASSAL) {
                let change = if city.status == VASSAL { VASSAL_REBELS } else { BECOMES_RIVAL };
                city.status = RIVAL;
                self.world_news.push((request.city, change));
            }
        }
        let due: Vec<Scheduled> = self.scheduled.iter().filter(|s| s.at <= now).copied().collect();
        self.scheduled.retain(|s| s.at > now);
        for event in due {
            let (low, high) = (event.cities.0.max(0) as u32, event.cities.1.max(event.cities.0).max(0) as u32);
            let city = (low + self.next_random() % (high - low + 1)) as usize;
            let amount = {
                let (a, b) = (event.amount.0.max(0) as u32, event.amount.1.max(event.amount.0).max(0) as u32);
                (a + self.next_random() % (b - a + 1)) as i32
            };
            // Cadeau : de toute cité active, y compris la cité mère ; une quantité
            // négative est ce qui a été mis de côté pour la cité.
            // Commerce : un prix pour tous, ou la demande et l'offre d'une cité, par
            // paliers de douze unités par an.
            if event.change >= TRADE_CHANGE {
                let resource = event.resource as usize;
                if resource >= RESOURCES {
                    continue;
                }
                match event.change - TRADE_CHANGE {
                    4 => self.price_shift[resource] += amount.max(1),
                    5 => self.price_shift[resource] -= amount.max(1),
                    kind => {
                        let Some(target) = self.cities.get_mut(city).filter(|c| c.active && c.kind == FOREIGN) else { continue };
                        let list = if kind < 2 { &mut target.buys } else { &mut target.sells };
                        let step = if kind % 2 == 0 { TRADE_STEP } else { -TRADE_STEP };
                        match list.iter_mut().position(|e| e.0 == event.resource) {
                            Some(i) => {
                                list[i].1 += step;
                                if list[i].1 <= 0 {
                                    list.remove(i);
                                }
                            }
                            None if step > 0 => list.push((event.resource, step)),
                            None => {}
                        }
                    }
                }
                self.trade_news.push((city, event.change - TRADE_CHANGE, event.resource));
                continue;
            }
            if (GIFT..TRADE_CHANGE).contains(&event.change) {
                let resource = (event.change - GIFT) as usize;
                if !self.cities.get(city).is_some_and(|c| c.active) || resource >= RESOURCES {
                    continue;
                }
                let amount = if event.amount.0 < 0 { self.carried[resource] } else { amount };
                if amount <= 0 {
                    continue;
                }
                let given = match resource as u8 {
                    DRACHMAS => {
                        self.money += amount;
                        amount
                    }
                    ANY_FOOD => self.store(7, amount),
                    _ => self.store(resource, amount),
                };
                self.gifts.push((city, resource as u8, given));
                continue;
            }
            if (EARTHQUAKE..=LANDSLIDE).contains(&event.change) {
                self.strike(event.change, city);
                continue;
            }
            if matches!(event.change, WAGE_RISE | WAGE_FALL) {
                let change = if event.change == WAGE_RISE { amount.max(1) } else { -amount.max(1) };
                self.wage_shift = (self.wage_shift + change).max(-NORMAL_WAGE);
                self.wage_news.push(change);
                continue;
            }
            // Armée en campagne : seule une rivale est concernée ; un nouveau départ
            // prolonge l'absence.
            if event.change == ARMY_AWAY {
                if self.cities.get(city).is_some_and(|c| c.active && c.kind == FOREIGN && c.status == RIVAL) {
                    let until = now + amount.max(1) as u64 * TICKS_PER_MONTH;
                    self.armies_away.retain(|&(c, _)| c != city);
                    self.armies_away.push((city, until));
                    self.world_news.push((city, ARMY_AWAY));
                }
                continue;
            }
            // Une cité inactive peut reparaître ; les autres changements veulent une
            // cité active.
            if matches!(event.change, BECOMES_ACTIVE | APPEARS) {
                if let Some(target) = self.cities.get_mut(city).filter(|c| !c.active && c.kind == FOREIGN) {
                    target.active = true;
                    self.world_news.push((city, event.change));
                }
                continue;
            }
            let Some(target) = self.cities.get_mut(city).filter(|c| c.active && c.kind == FOREIGN) else { continue };
            // Invasion : seule une cité rivale attaque. Olympe compare les forces : un
            // envahisseur vaut un hoplite (deux points) ; vaincue, la cité perd un
            // bâtiment par tranche de seize soldats et un cinquième de son trésor.
            // Invasion : seule une cité rivale attaque, et une à la fois.
            if event.change == INVASION {
                if target.status == RIVAL && self.raid.is_none() {
                    self.invade(city, amount.max(1), event.resource as usize);
                }
                continue;
            }
            // Demande : seule une cité qui n'est pas rivale en fait.
            if (REQUEST..GIFT).contains(&event.change) {
                if target.status != RIVAL {
                    let request = Request { city, resource: event.change - REQUEST, amount: amount.max(1), until: now + REQUEST_TICKS };
                    self.requests.push(request);
                    self.request_news.push(RequestNews::Arrived(request));
                }
                continue;
            }
            let changed = match event.change {
                BECOMES_ALLY if target.status == RIVAL => std::mem::replace(&mut target.status, crate::map::ALLY) != crate::map::ALLY,
                BECOMES_RIVAL if target.status == crate::map::ALLY => std::mem::replace(&mut target.status, RIVAL) != RIVAL,
                BECOMES_VASSAL if target.status <= RIVAL => std::mem::replace(&mut target.status, VASSAL) != VASSAL,
                VASSAL_REBELS if target.status == VASSAL => std::mem::replace(&mut target.status, RIVAL) != RIVAL,
                MILITARY_BUILDUP => {
                    target.military = (target.military as i32 + amount.max(1)).min(6) as u8;
                    true
                }
                MILITARY_DECLINE => {
                    target.military = (target.military as i32 - amount.max(1)).max(1) as u8;
                    true
                }
                TRADE_SHUTS if target.trade => {
                    target.trade = false;
                    true
                }
                TRADE_OPENS if !target.trade && target.status != RIVAL => {
                    target.trade = true;
                    true
                }
                BECOMES_INACTIVE | DISAPPEARS => {
                    target.active = false;
                    true
                }
                _ => false,
            };
            if changed {
                self.world_news.push((city, event.change));
                // Un nouvel allié offre un monument commémoratif.
                if event.change == BECOMES_ALLY {
                    self.commemoratives.push(COMMEMORATIVE_ALLY);
                }
            }
        }
    }

    /// Soldats présents dans la cité, par rang (piétaille, hoplites, cavaliers) :
    /// ceux que les logements entretiennent, moins ceux partis en campagne.
    pub fn ranks(&self) -> [i32; 3] {
        let (rabble, hoplites, horsemen) = (self.soldiers(Soldier::Rabble), self.soldiers(Soldier::Hoplite), self.soldiers(Soldier::Horseman));
        let away = self.expedition.map_or([0; 3], |e| e.soldiers);
        let home = [rabble - hoplites, hoplites - horsemen, horsemen];
        std::array::from_fn(|rank| (home[rank] - away[rank]).max(0))
    }

    /// Force comme `Zeus.exe` la compte (0x4dea00) : un par homme de piétaille ou
    /// hoplite, deux par cavalier, six par trière.
    pub fn force(soldiers: [i32; 3], triremes: i32) -> i32 {
        soldiers[0] + soldiers[1] + 2 * soldiers[2] + TRIREME_FORCE * triremes
    }

    /// Force de l'armée et de la flotte présentes dans la cité.
    pub fn army(&self) -> i32 {
        Self::force(self.ranks(), self.triremes())
    }

    /// Trières qui partent (quai à −1) ou qui rentrent : `count` quais passent de
    /// `from` à `to`.
    fn moor(&mut self, count: i32, from: i32, to: i32) -> i32 {
        let mut moved = 0;
        for building in self.buildings.iter_mut().flatten() {
            if moved < count && self.kinds[building.kind].function == Function::Wharf && building.stock == from {
                building.stock = to;
                moved += 1;
            }
        }
        moved
    }

    /// Monture des logements luxueux : chevaux, ou chars dans une cité atlante.
    fn mount(&self) -> Good {
        if self.atlantean { Good::Chariot } else { Good::Horses }
    }

    /// Défense la plus haute d'une cité du monde à ce jour.
    fn max_strength(&self, city: usize) -> i32 {
        let years = (self.ticks / TICKS_PER_MONTH) as f64 / 12.0;
        let scale = DEFENCE_BY_DIFFICULTY[self.difficulty.min(DEFENCE_BY_DIFFICULTY.len() - 1)];
        self.cities.get(city).map_or(0, |c| (years.min(DEFENCE_YEARS).sqrt() * scale * c.military as f64) as i32)
    }

    /// Force qu'une armée doit égaler pour vaincre une cité. Une cité dont l'armée
    /// est partie n'en oppose que la moitié (0x461540).
    pub fn defence(&self, city: usize) -> i32 {
        let Some(c) = self.cities.get(city) else { return 0 };
        let strength = if c.strength < 0 { self.max_strength(city) } else { c.strength };
        if self.army_away(city) { strength / 2 } else { strength }
    }

    /// L'armée de cette cité est-elle partie en campagne ?
    pub fn army_away(&self, city: usize) -> bool {
        self.armies_away.iter().any(|&(c, until)| c == city && until > self.ticks)
    }

    /// Mois de marche de la cité du joueur jusqu'à celle-ci, par la route du monde.
    pub fn travel_months(&self, city: usize) -> i32 {
        let home = self.cities.iter().position(|c| c.active && c.kind == 0).map(|h| h as i16);
        let route = self.routes.iter().find(|r| home.is_some_and(|h| (r.from, r.to) == (h, city as i16) || (r.to, r.from) == (h, city as i16)));
        route.map_or(MIN_TRAVEL_MONTHS, |r| (r.length * r.pace as i32 / PACE_DIVISOR).max(MIN_TRAVEL_MONTHS))
    }

    /// Envoie toute l'armée présente soumettre une cité étrangère, alliée ou rivale,
    /// ou la piller (`raid`) ; rend la durée du trajet en mois. La bataille se livre à
    /// l'arrivée, puis l'armée rentre par le même chemin.
    pub fn conquer(&mut self, city: usize, raid: bool) -> Result<i32, Conquest> {
        let (soldiers, triremes) = (self.ranks(), self.triremes());
        self.conquer_with(city, raid, soldiers, triremes)
    }

    /// Envoie une compagnie choisie — `soldiers` par rang (piétaille, hoplites,
    /// cavaliers) et `triremes` — soumettre une cité étrangère, alliée ou rivale, ou
    /// la piller (`raid`). Ce qui reste à la cité ne part pas. Rend la durée du trajet
    /// en mois ; la bataille se livre à l'arrivée, puis la compagnie rentre par le même
    /// chemin.
    pub fn conquer_with(&mut self, city: usize, raid: bool, soldiers: [i32; 3], triremes: i32) -> Result<i32, Conquest> {
        let Some(target) = self.cities.get(city).filter(|c| c.active && c.kind == FOREIGN) else { return Err(Conquest::NotForeign) };
        if target.status > RIVAL {
            return Err(Conquest::AlreadyRuled);
        }
        if self.expedition.is_some() || self.ticks < self.campaign_until {
            return Err(Conquest::Away);
        }
        let (available, docked) = (self.ranks(), self.triremes());
        if soldiers.iter().any(|&n| n < 0) || triremes < 0 || soldiers.iter().zip(available).any(|(&n, max)| n > max) || triremes > docked {
            return Err(Conquest::TooMany);
        }
        if Self::force(soldiers, triremes) == 0 {
            return Err(Conquest::NoArmy);
        }
        // Chaque trière choisie quitte son quai ; il l'attendra jusqu'au retour.
        let sent = self.moor(triremes, 1, -1);
        debug_assert_eq!(sent, triremes);
        let months = self.travel_months(city);
        let arrives = self.ticks + months as u64 * TICKS_PER_MONTH;
        self.expedition = Some(Expedition { city, raid, soldiers, triremes: sent, arrives, returns: arrives, won: None, loot: (0, 0) });
        Ok(months)
    }

    /// Campagne en cours : bataille à l'arrivée, retour et butin.
    fn run_expedition(&mut self) {
        let Some(mut expedition) = self.expedition else { return };
        let city = expedition.city;
        if expedition.won.is_none() && self.ticks >= expedition.arrives {
            let (attack, defence) = (Self::force(expedition.soldiers, expedition.triremes), self.defence(city));
            let won = attack >= defence;
            let margin = |stronger: i32, weaker: i32| if stronger == 0 { 0 } else { (stronger - weaker) * 100 / stronger };
            // Pertes en pour cent : celles de l'armée, celles de la cité (0x461540).
            let (ours, theirs) = match (expedition.raid, won) {
                (false, true) => (losses(margin(attack, defence), CONQUEST_LOSSES), CONQUEST_BLOW),
                (false, false) => (CONQUEST_DEFEAT, losses(margin(defence, attack), CONQUEST_LOSSES)),
                (true, true) => (losses(margin(attack, defence), RAID_LOSSES), RAID_BLOW),
                (true, false) => (raid_defeat(if attack == 0 { 0 } else { (defence - attack) / attack }), losses(margin(defence, attack), RAID_LOSSES)),
            };
            for rank in 0..3 {
                let dead = expedition.soldiers[rank] * ours / 100;
                self.bury_soldiers(rank, dead);
                expedition.soldiers[rank] -= dead;
            }
            // Les navires coulés laissent leur quai libre pour en bâtir d'autres.
            let sunk = expedition.triremes * ours / 100;
            self.moor(sunk, -1, 0);
            expedition.triremes -= sunk;
            if let Some(c) = self.cities.get_mut(city) {
                let strength = if c.strength < 0 { defence } else { c.strength };
                c.strength = (strength - theirs * defence / 100).max(0);
            }
            expedition.won = Some(won);
            expedition.returns = self.ticks + self.travel_months(city) as u64 * TICKS_PER_MONTH;
            match (expedition.raid, won) {
                (false, true) => {
                    self.cities[city].status = VASSAL;
                    self.commemoratives.push(COMMEMORATIVE_CONQUEST);
                    self.campaign_news.push(CampaignNews::Conquered(city));
                }
                (false, false) => self.campaign_news.push(CampaignNews::ConquestFailed(city)),
                (true, true) => expedition.loot = self.plunder(city, expedition.soldiers, expedition.triremes),
                (true, false) if expedition.soldiers.iter().all(|&n| n == 0) && expedition.triremes == 0 => {
                    self.campaign_news.push(CampaignNews::WipedOut(city));
                    self.expedition = None;
                    return;
                }
                (true, false) => {}
            }
            self.expedition = Some(expedition);
            return;
        }
        if expedition.won.is_some() && self.ticks >= expedition.returns {
            self.expedition = None;
            self.moor(expedition.triremes, -1, 1);
            let (resource, amount) = expedition.loot;
            if amount > 0 {
                let stored = if resource == DRACHMAS {
                    self.money += amount;
                    amount
                } else {
                    self.store(resource as usize, amount)
                };
                self.campaign_news.push(CampaignNews::Plunder { city, resource, amount: stored });
            } else {
                self.campaign_news.push(CampaignNews::Returned { city, won: expedition.won == Some(true) });
            }
        }
    }

    /// Butin d'un raid victorieux sur cette cité, porté par les survivants : une
    /// marchandise qu'elle vend, ou des drachmes, tirées au sort parmi celles
    /// qu'elle n'a pas trop souvent perdues (0x42d380).
    fn plunder(&mut self, city: usize, soldiers: [i32; 3], triremes: i32) -> (u8, i32) {
        let Some(c) = self.cities.get(city) else { return (DRACHMAS, 0) };
        let marked = |resource: u8| self.plundered.iter().find(|p| (p.0, p.1) == (city, resource)).map_or(0, |p| p.2);
        // Seul ce que la cité vend se pille (guide de l'éditeur de Poséidon).
        let mut choices: Vec<(u8, i32)> = c.sells.iter().copied().filter(|&(r, q)| q > 0 && r != DRACHMAS && marked(r) <= PLUNDER_LIMIT).collect();
        if marked(DRACHMAS) <= PLUNDER_LIMIT {
            choices.push((DRACHMAS, 0));
        }
        let wealth = c.wealth;
        if choices.is_empty() {
            return (DRACHMAS, 2);
        }
        let (resource, quantity) = choices[self.next_random() as usize % choices.len()];
        let difficulty = self.difficulty.min(PLUNDER_DIVISOR.len() - 1);
        let amount = if resource == DRACHMAS {
            ((wealth as f64 + 1.0) * PLUNDER_DRACHMAS[difficulty] * PLUNDER_DRACHMAS_UNIT) as i32
        } else {
            let carried: i32 = (0..3).map(|rank| soldiers[rank] * CARRY[rank]).sum::<i32>() + TRIREME_CARRY * triremes;
            ((quantity as f64 / PLUNDER_DIVISOR[difficulty]) as i32).min(carried)
        };
        match self.plundered.iter_mut().find(|p| (p.0, p.1) == (city, resource)) {
            Some(p) => p.2 += PLUNDER_MARK,
            None => self.plundered.push((city, resource, PLUNDER_MARK)),
        }
        (resource, amount.max(1))
    }

    /// Les cités du monde, chaque mois : leur défense remonte d'un point, les
    /// marques de pillage s'effacent d'un point (0x42d540).
    fn cities_month(&mut self) {
        for city in 0..self.cities.len() {
            let top = self.max_strength(city);
            let c = &mut self.cities[city];
            if c.strength < 0 {
                c.strength = top;
            } else if c.strength < top {
                c.strength += 1;
            }
        }
        for mark in &mut self.plundered {
            mark.2 -= 1;
        }
        self.plundered.retain(|p| p.2 > 0);
    }

    /// Soldats tombés au loin : les logements qui les fournissaient perdent autant
    /// d'hommes, au prorata de ceux qu'ils entretiennent.
    fn bury_soldiers(&mut self, rank: usize, dead: i32) {
        if dead <= 0 {
            return;
        }
        let wanted = [Soldier::Rabble, Soldier::Hoplite, Soldier::Horseman][rank];
        let mut houses: Vec<(usize, i32)> = Vec::new();
        for (id, building) in self.buildings.iter().enumerate() {
            let Some(house) = building.as_ref().and_then(|b| b.house.as_ref()) else { continue };
            let Some(&(soldier, full)) = SOLDIERS.get(house.level) else { continue };
            let capacity = self.capacity(house).max(1);
            let provided = full * house.population.min(capacity) / capacity;
            if soldier == wanted && provided > 0 {
                houses.push((id, provided));
            }
        }
        let total: i32 = houses.iter().map(|h| h.1).sum();
        let mut left = dead.min(total);
        for (i, &(id, provided)) in houses.iter().enumerate() {
            let share = if i + 1 == houses.len() { left } else { (dead * provided / total.max(1)).min(left) };
            left -= share;
            if let Some(house) = self.buildings[id].as_mut().and_then(|b| b.house.as_mut()) {
                house.population -= (share * house.population / provided).min(house.population);
            }
        }
    }


    /// Range `amount` d'une ressource au grenier (vivres) ou à l'entrepôt, dans la
    /// limite de la place ; rend ce qui a été rangé.
    fn store(&mut self, resource: usize, amount: i32) -> i32 {
        let good = GOODS.iter().position(|&g| resource_of(g) == Some(resource as u8));
        let mut left = amount;
        for id in 0..self.buildings.len() {
            if left <= 0 {
                break;
            }
            let function = match &self.buildings[id] {
                Some(b) => storage(self.kinds[b.kind].function),
                None => continue,
            };
            let Some(b) = self.buildings[id].as_mut() else { continue };
            let put = match (function, good) {
                (Function::Granary, _) if (1..FOODS).contains(&resource) => (GRANARY_STORAGE - b.stock).clamp(0, left),
                (Function::Storehouse, Some(good)) => {
                    // Une marchandise n'occupe jamais plus de la moitié de l'entrepôt.
                    let room = BAY_LOADS * (STOREHOUSE_BAYS as i32 / 2) - b.goods[good];
                    let free_bays = STOREHOUSE_BAYS.saturating_sub(bays_used(&b.goods)) as i32 * BAY_LOADS + (BAY_LOADS - b.goods[good] % BAY_LOADS) % BAY_LOADS;
                    room.min(free_bays).clamp(0, left)
                }
                _ => 0,
            };
            if put > 0 {
                match good {
                    Some(good) if function == Function::Storehouse => b.goods[good] += put,
                    _ => {
                        b.stock += put;
                        b.foods[resource] += put;
                    }
                }
                left -= put;
            }
        }
        amount - left
    }

    /// Commerce du mois avec les cités partenaires : on leur vend ce qu'elles
    /// achètent au-delà d'une réserve gardée, et l'on achète ce qu'elles vendent
    /// quand les réserves sont basses, au prix d'Olympe, dans la limite d'un
    /// douzième de leur quantité annuelle.
    fn trade(&mut self) {
        let served = self.trade_posts();
        for city in 0..self.cities.len() {
            let c = &self.cities[city];
            if !(c.active && c.trade && c.status != RIVAL) || !served.contains(&city) {
                continue;
            }
            let (buys, sells) = (c.buys.clone(), c.sells.clone());
            let (mut sold, mut bought) = (0, 0);
            for (resource, quantity) in buys {
                let (resource, monthly) = (resource as usize, (quantity / 12).max(1));
                let spare = (self.stored(resource) - TRADE_RESERVE).min(monthly);
                if spare > 0 && resource < RESOURCES && self.trade_policy[resource].sells() {
                    let taken = self.take(resource, spare);
                    self.money += taken * self.price(resource);
                    sold += taken * self.price(resource);
                }
            }
            for (resource, quantity) in sells {
                let (resource, monthly) = (resource as usize, (quantity / 12).max(1));
                let wanted = (TRADE_RESERVE - self.stored(resource)).min(monthly).min(self.money / self.price(resource).max(1));
                if wanted > 0 && resource < RESOURCES && self.trade_policy[resource].buys() {
                    let put = self.store(resource, wanted);
                    self.money -= put * self.price(resource);
                    bought += put * self.price(resource);
                }
            }
            let c = &mut self.cities[city];
            (c.sold, c.bought) = (c.sold + sold, c.bought + bought);
        }
    }

    /// Prix courant d'une marchandise : prix de base, déplacé par les événements.
    pub fn price(&self, resource: usize) -> i32 {
        let base = self.base_prices.get(resource).copied().filter(|&p| p > 0).unwrap_or_else(|| price(resource));
        (base + self.price_shift.get(resource).copied().unwrap_or(0)).max(1)
    }

    /// Programme un changement du commerce.
    pub fn schedule_trade(&mut self, change: u8, resource: u8, cities: (i16, i16), amount: (i16, i16), month: u8, years: (i16, i16)) {
        let at = self.date_in(month, years);
        self.scheduled.push(Scheduled { at, change: TRADE_CHANGE + change, cities, amount, resource });
    }

    /// Cités partenaires que dessert un poste de commerce (route de terre) ou des docks
    /// (route de mer) en activité. Le jeu fait choisir la cité à la pose ; Olympe donne
    /// à chaque poste, dans l'ordre de construction, la première partenaire de sa voie
    /// qui n'en a pas.
    pub fn trade_posts(&self) -> Vec<usize> {
        let mut served = Vec::new();
        for sea in [false, true] {
            let posts = self.buildings.iter().flatten().filter(|b| b.staffed && self.kinds[b.kind].function == Function::TradePost { sea }).count();
            let partners = (0..self.cities.len()).filter(|&c| self.trades_with(c) && self.sea_route(c) == sea);
            served.extend(partners.take(posts));
        }
        served
    }

    /// Cité avec qui l'on peut commercer : commerce ouvert, ni rivale, ni lieu enchanté
    /// ou cité détruite (0x42c9c0).
    pub fn trades_with(&self, city: usize) -> bool {
        self.cities.get(city).is_some_and(|c| c.active && c.trade && c.status != RIVAL && (1..=crate::map::DISTANT).contains(&c.kind))
    }

    /// La route de la cité du joueur à celle-ci passe par la mer.
    pub fn sea_route(&self, city: usize) -> bool {
        let home = self.cities.iter().position(|c| c.kind == 0).map(|h| h as i16);
        self.routes.iter().find(|r| home.is_some_and(|h| (r.from, r.to) == (h, city as i16) || (r.to, r.from) == (h, city as i16))).is_some_and(|r| r.sea)
    }

    /// Partenaires commerciaux : cités non rivales avec qui le commerce est ouvert
    /// (0x525ec0).
    pub fn partners(&self) -> i32 {
        self.cities.iter().filter(|c| c.active && c.trade && c.status != RIVAL).count() as i32
    }

    /// Monstres dans la cité : chaque monstre vivant (sauf ceux de la mer) y marche ;
    /// un monstre tué disparaît.
    fn release_monsters(&mut self) {
        let now = self.ticks;
        // Monstres arrivés depuis le dernier passage.
        let arriving: Vec<usize> = self.monsters.iter().filter(|m| !m.slain && m.arrives_at > 0 && m.arrives_at <= now && m.arrives_at + TICKS_PER_MONTH > now).map(|m| m.event).collect();
        self.hero_news.extend(arriving.into_iter().map(HeroNews::Invaded));
        let alive: Vec<u8> = self.monsters.iter().filter(|m| !m.slain && m.arrives_at <= now && !SEA_MONSTERS.contains(&m.monster)).map(|m| m.monster).collect();
        self.figures.retain(|f| !matches!(f.role, Role::Monster { monster, .. } if !alive.contains(&monster)));
        for monster in alive {
            if self.figures.iter().any(|f| matches!(f.role, Role::Monster { monster: m, .. } if m == monster)) {
                continue;
            }
            // Il arrive par l'entrée de la carte.
            let at = self.city_entrance();
            self.figures.push(Figure {
                image_base: 1,
                x: at.0 as f32,
                y: at.1 as f32,
                direction: 0,
                frame: 0,
                path: Vec::new(),
                from: at,
                came_from: None,
                role: Role::Monster { monster, target: NO_TARGET, rest_until: self.ticks + MONSTER_REST_TICKS },
                source: Source::Monster(monster),
            });
        }
    }

    /// Ravage d'un monstre ou d'un dieu arrivé au bout de son chemin : il détruit sa
    /// cible, se repose `rest` pas, puis marche vers un autre bâtiment. Rend la
    /// nouvelle cible et la fin du repos.
    fn rampage(&mut self, index: usize, at: (u16, u16), target: u16, rest_until: u64, rest: u64) -> (u16, u64) {
        let now = self.ticks;
        if target != NO_TARGET {
            if self.buildings.get(target as usize).is_some_and(|b| b.is_some()) {
                let cause = match self.figures[index].role {
                    Role::God { god, .. } => Cause::God(god),
                    Role::Monster { monster, .. } => Cause::Monster(monster),
                    _ => Cause::Collapse,
                };
                self.ravaged.push((target, cause));
            }
            return (NO_TARGET, now + rest);
        }
        if now < rest_until {
            return (NO_TARGET, rest_until);
        }
        let standing: Vec<u16> = (0..self.buildings.len()).filter(|&i| self.buildings[i].is_some()).map(|i| i as u16).collect();
        if standing.is_empty() {
            return (NO_TARGET, rest_until);
        }
        let target = standing[self.next_random() as usize % standing.len()];
        let building = self.buildings[target as usize].as_ref().unwrap();
        let path = self.land_path(at, (building.x as u16, building.y as u16));
        let figure = &mut self.figures[index];
        if let Some(&next) = path.last() {
            figure.direction = direction(at, next);
        }
        figure.path = path;
        (target, rest_until)
    }

    /// Chemin à pied à travers la campagne, de `from` à `to` (exclu le départ, la
    /// prochaine case en dernier) : on contourne l'eau, les rochers, les falaises,
    /// les gouffres et les eaux d'un raz-de-marée, mais on traverse bâtiments, arbres
    /// et routes. Sans passage, la ligne droite.
    fn land_path(&self, from: (u16, u16), to: (u16, u16)) -> Vec<(u16, u16)> {
        self.land_route(from, to, false).unwrap_or_else(|| straight_path(from, to))
    }

    /// Chemin à pied par la campagne, s'il en existe un.
    /// Une enceinte protège la case : aucun envahisseur venu d'un point d'invasion ou
    /// d'entrée de la carte ne peut l'atteindre sans franchir de mur.
    pub fn walled_in(&self, x: usize, y: usize) -> bool {
        let starts: Vec<(usize, usize)> = self.map.invasion_points().into_iter().chain(self.map.entry_points()).filter(|&(sx, sy)| self.in_map(sx as i32, sy as i32)).collect();
        !starts.is_empty() && self.cells.iter().any(|c| matches!(c, Cell::Wall(_))) && starts.iter().all(|&(sx, sy)| self.land_route((sx as u16, sy as u16), (x as u16, y as u16), true).is_none())
    }

    /// Mur le plus proche d'une case, s'il y en a.
    fn nearest_wall(&self, at: (u16, u16)) -> Option<(u16, u16)> {
        let distance = |index: usize| ((index % GRID) as i32 - at.0 as i32).abs().max(((index / GRID) as i32 - at.1 as i32).abs());
        (0..self.cells.len()).filter(|&i| matches!(self.cells[i], Cell::Wall(_))).min_by_key(|&i| distance(i)).map(|i| ((i % GRID) as u16, (i / GRID) as u16))
    }

    /// Avec `walls`, les murs barrent le passage, sauf celui où l'on se rend.
    fn land_route(&self, from: (u16, u16), to: (u16, u16), walls: bool) -> Option<Vec<(u16, u16)>> {
        const UNSEEN: u32 = u32::MAX;
        let blocked = |x: i32, y: i32| {
            if !self.in_map(x, y) {
                return true;
            }
            if walls && (x as u16, y as u16) != to && (matches!(self.cell(x as usize, y as usize), Cell::Wall(_)) || self.is_gate(x, y)) {
                return true;
            }
            let terrain = self.map.terrain(x as usize, y as usize);
            let cliff = terrain & TERRAIN_ELEVATION != 0 && terrain & TERRAIN_RAMP == 0;
            terrain & (TERRAIN_WATER | TERRAIN_ROCK | TERRAIN_MARSH) != 0 || cliff || matches!(self.cell(x as usize, y as usize), Cell::Chasm(_) | Cell::Flood)
        };
        let index = |at: (u16, u16)| at.1 as usize * GRID + at.0 as usize;
        let mut previous = vec![UNSEEN; GRID * GRID];
        previous[index(from)] = index(from) as u32;
        let mut queue = VecDeque::from([from]);
        while let Some(at) = queue.pop_front() {
            if at == to {
                let mut path = Vec::new();
                let mut step = index(to);
                while step != index(from) {
                    path.push(((step % GRID) as u16, (step / GRID) as u16));
                    step = previous[step] as usize;
                }
                return Some(path);
            }
            for (dx, dy) in DIRECTIONS {
                let (nx, ny) = (at.0 as i32 + dx, at.1 as i32 + dy);
                if !Self::in_grid(nx, ny) {
                    continue;
                }
                let next = (nx as u16, ny as u16);
                if previous[index(next)] == UNSEEN && (next == to || !blocked(nx, ny)) {
                    previous[index(next)] = index(at) as u32;
                    queue.push_back(next);
                }
            }
        }
        None
    }

    /// Une armée rivale entre par un point d'invasion de la carte (à défaut, par
    /// l'entrée des voyageurs) ; les hoplites de la cité sortent du palais (à défaut,
    /// d'une maison), autant que sa force militaire en fournit.
    fn invade(&mut self, city: usize, soldiers: i32, point: usize) {
        // Par la mer (points 9 à 16) : un navire au point d'invasion, l'armée débarque
        // sur la terre la plus proche.
        let by_sea = (9..=16).contains(&point).then(|| self.map.invasion_point(point)).flatten();
        let landing = by_sea.and_then(|(sx, sy)| {
            (1..24i32).find_map(|r| {
                (-r..=r).flat_map(|dy| (-r..=r).map(move |dx| (dx, dy))).filter(|&(dx, dy)| dx.abs().max(dy.abs()) == r).find_map(|(dx, dy)| {
                    let (x, y) = (sx as i32 + dx, sy as i32 + dy);
                    self.is_walkable(x, y).then_some((x as usize, y as usize))
                })
            })
        });
        // Par la terre : le point de l'événement s'il mène à la cité par la terre, sinon
        // le premier point qui y mène, sinon le premier point de la carte.
        let heart = self.buildings.iter().flatten().next().map(|b| (b.x as u16, b.y as u16));
        let leads = |w: &World, (x, y): (usize, usize)| w.in_map(x as i32, y as i32) && heart.is_none_or(|c| w.land_route((x as u16, y as u16), c, false).is_some());
        let wanted = (1..=8).contains(&point).then(|| self.map.invasion_point(point)).flatten().filter(|&p| leads(self, p));
        let candidates: Vec<(usize, usize)> = self.map.invasion_points().into_iter().chain(self.map.entry_points()).collect();
        let land = wanted.or_else(|| candidates.iter().copied().find(|&p| leads(self, p)));
        let entry = landing.or(land).or_else(|| candidates.iter().copied().find(|&(x, y)| self.in_map(x as i32, y as i32)));
        let Some(entry) = entry else { return };
        if let (Some((sx, sy)), Some(_)) = (by_sea, landing) {
            self.figures.push(Figure {
                image_base: self.walkers.ship,
                x: sx as f32,
                y: sy as f32,
                direction: 0,
                frame: 0,
                path: Vec::new(),
                from: (sx as u16, sy as u16),
                came_from: None,
                role: Role::Ship,
                source: Source::Figures,
            });
        }
        let limit = (soldiers / SOLDIERS_PER_RUIN).max(MIN_RUINS);
        self.raid = Some(Raid { city, soldiers, buildings: 0, limit });
        let nation = self.cities.get(city).map_or(0, |c| c.army).min(ARMY_UNITS.len() as u8 - 1);
        // Chaque soldat reçoit son unité (`army`) : pour l'ennemi, tirée selon la
        // fréquence de chaque unité dans les règles ; pour la cité, son rang.
        let spawn = |world: &mut World, at: (usize, usize), enemy: bool, ranks: &[u8]| {
            for (i, &army) in ranks.iter().enumerate().take(MAX_SOLDIERS as usize) {
                let i = i as i32;
                // Les soldats se rangent en carré autour de leur point de départ.
                let side = 6;
                let (dx, dy) = (i % side - side / 2, i / side - side / 2);
                let (ox, oy) = (at.0 as i32 + dx, at.1 as i32 + dy);
                // Pas de soldat dans l'eau ni sur la falaise : il reste au point de départ.
                let (x, y) = if world.is_walkable(ox, oy) { (ox as u16, oy as u16) } else { (at.0 as u16, at.1 as u16) };
                let citizen = [world.walkers.rabble, world.walkers.hoplite, world.walkers.horseman];
                let atlantean = world.walkers.atlantean.get(army as usize).copied().filter(|&b| world.atlantean && b > 0);
                let (source, image_base) = if enemy {
                    (Source::Army(nation), army_unit(army).walk)
                } else if let Some(base) = atlantean {
                    (Source::Imps, base)
                } else {
                    (Source::Figures, citizen.get(army as usize).copied().filter(|&b| b > 0).unwrap_or(world.walkers.hoplite))
                };
                let health = world.fighter(enemy, army).health;
                world.figures.push(Figure {
                    image_base,
                    x: x as f32,
                    y: y as f32,
                    direction: 0,
                    frame: 0,
                    path: Vec::new(),
                    from: (x, y),
                    came_from: None,
                    role: Role::Soldier { enemy, army, health, target: NO_TARGET, fighting: false },
                    source,
                });
            }
        };
        let units = ARMY_UNITS[nation as usize];
        let weights: Vec<i32> = units.iter().map(|u| self.fighters.enemies.get(nation as usize * FIGHTERS_PER_NATION + u.kind).map_or(0, |f| f.frequency)).collect();
        let total: i32 = weights.iter().sum();
        let count = soldiers.min(MAX_SOLDIERS);
        let invaders: Vec<u8> = (0..count)
            .map(|i| {
                // Répartition selon les fréquences (à défaut, chacune à son tour).
                let rank = if total > 0 {
                    let mut point = (i * total + total / 2) / count.max(1);
                    weights.iter().position(|&w| {
                        point -= w;
                        point < 0
                    }).unwrap_or(0)
                } else {
                    i as usize % units.len()
                };
                nation * 16 + rank as u8
            })
            .collect();
        spawn(self, entry, true, &invaders);
        let home = self.buildings.iter().flatten().find(|b| self.kinds[b.kind].function == Function::Palace).or_else(|| self.buildings.iter().flatten().find(|b| b.house.is_some()));
        if let Some(home) = home.map(|b| (b.x, b.y)) {
            // Les soldats de la cité, par rang : piétaille et archers, hoplites, cavaliers.
            let counts = self.ranks();
            let total: i32 = counts.iter().sum();
            let scale = |n: i32| if total > MAX_SOLDIERS { n * MAX_SOLDIERS / total } else { n };
            let defenders: Vec<u8> = counts.iter().enumerate().flat_map(|(rank, &n)| std::iter::repeat(rank as u8).take(scale(n).max(0) as usize)).collect();
            spawn(self, home, false, &defenders);
        }
    }

    /// Caractéristiques de combat d'un soldat : unité ennemie de sa nation, ou soldat
    /// de la cité de son rang (piétaille et archers, hoplites, cavaliers).
    fn fighter(&self, enemy: bool, army: u8) -> crate::model::Fighter {
        let found = if enemy {
            self.fighters.enemies.get((army / 16) as usize * FIGHTERS_PER_NATION + army_unit(army).kind).copied()
        } else {
            self.fighters.citizens.get(army as usize).copied()
        };
        let mut fighter = found.filter(|f| f.health > 0).unwrap_or(FALLBACK_FIGHTER);
        if !enemy && self.has_sanctuary(ATHENA) {
            fighter.missile_armor += ATHENA_MISSILE_ARMOR;
        }
        fighter
    }

    /// Portée d'un soldat, en cases : celle de ses projectiles (les règles la donnent
    /// en demi-cases), ou le corps à corps.
    fn reach(&self, enemy: bool, army: u8) -> f32 {
        let fighter = self.fighter(enemy, army);
        if fighter.missile_attack > 0 && fighter.missile_range > 0 { (fighter.missile_range as f32 / 2.0).max(MELEE_REACH) } else { MELEE_REACH }
    }

    /// Traits en vol : pour chaque tireur ennemi qui combat, sa position, celle du
    /// défenseur le plus proche (en cases) et l'avancement du trait (0 à 1) entre deux
    /// coups.
    pub fn missiles(&self) -> Vec<((f32, f32), (f32, f32), f32)> {
        let defenders: Vec<(f32, f32)> = self.figures.iter().filter(|f| f.soldier() == Some(false)).map(|f| (f.x, f.y)).collect();
        let progress = (self.ticks % STRIKE_TICKS) as f32 / STRIKE_TICKS as f32;
        self.figures
            .iter()
            .filter_map(|f| match f.role {
                Role::Soldier { enemy: true, army, fighting: true, .. } if self.reach(true, army) > MELEE_REACH => Some((f.x, f.y)),
                _ => None,
            })
            .filter_map(|from| {
                let to = defenders.iter().copied().min_by(|a, b| (a.0 - from.0).hypot(a.1 - from.1).total_cmp(&(b.0 - from.0).hypot(b.1 - from.1)))?;
                Some((from, to, progress))
            })
            .chain(self.raid.is_some().then(|| self.tower_shots()).unwrap_or_default().into_iter().filter_map(|(shot, target)| {
                let tower = self.buildings.iter().flatten().filter(|b| b.staffed && self.kinds[b.kind].function == Function::Tower).nth(shot)?;
                let half = self.kinds[tower.kind].size as f32 / 2.0;
                let enemy = &self.figures[target];
                Some(((tower.x as f32 + half, tower.y as f32 + half), (enemy.x, enemy.y), progress))
            }))
            .collect()
    }

    /// Tirs des tours en activité : pour chacune, l'envahisseur le plus proche à portée
    /// de ses sentinelles (rang du personnage). La portée des règles est en demi-cases.
    fn tower_shots(&self) -> Vec<(usize, usize)> {
        let reach = self.fighters.sentry.missile_range as f32 / 2.0;
        let enemies: Vec<(usize, f32, f32)> = self.figures.iter().enumerate().filter(|(_, f)| f.soldier() == Some(true)).map(|(i, f)| (i, f.x, f.y)).collect();
        self.buildings
            .iter()
            .flatten()
            .filter(|b| b.staffed && self.kinds[b.kind].function == Function::Tower)
            .enumerate()
            .filter_map(|(shot, b)| {
                let half = self.kinds[b.kind].size as f32 / 2.0;
                let (cx, cy) = (b.x as f32 + half, b.y as f32 + half);
                enemies.iter().map(|&(i, x, y)| (i, (x - cx).hypot(y - cy))).filter(|&(_, d)| d <= reach).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| (shot, i))
            })
            .collect()
    }

    /// Un pas de bataille : corps à corps entre soldats proches, marche des
    /// envahisseurs vers le bâtiment le plus proche, poursuite des défenseurs. Quand
    /// le dernier envahisseur tombe, l'invasion est repoussée ; quand ils ont assez
    /// détruit, ils repartent avec un cinquième du trésor.
    fn fight(&mut self) {
        let Some(mut raid) = self.raid else { return };
        let soldiers: Vec<(usize, bool, f32, f32)> =
            self.figures.iter().enumerate().filter_map(|(i, f)| f.soldier().map(|enemy| (i, enemy, f.x, f.y))).collect();
        let strike = self.ticks % STRIKE_TICKS == 0;
        let mut wounds = vec![0; self.figures.len()];
        // Coups (attaque, attaque à distance, cadence de tir), défense (armure, armure
        // contre les projectiles) et portée de chaque soldat, d'après les règles.
        let (mut blows, mut defense, mut reach_of) = (vec![None; self.figures.len()], vec![(0, 0); self.figures.len()], vec![MELEE_REACH; self.figures.len()]);
        for &(i, ..) in &soldiers {
            if let Role::Soldier { enemy, army, .. } = self.figures[i].role {
                let f = self.fighter(enemy, army);
                blows[i] = Some((f.attack, f.missile_attack, f.rate));
                defense[i] = (f.armor, f.missile_armor);
                reach_of[i] = self.reach(enemy, army);
            }
        }
        for &(i, enemy, x, y) in &soldiers {
            let foe = soldiers
                .iter()
                .filter(|o| o.1 != enemy)
                .map(|o| (o.0, (o.2 - x).hypot(o.3 - y), o.2, o.3))
                .min_by(|a, b| a.1.total_cmp(&b.1));
            // Un défenseur sans adversaire à portée le poursuit, en recalculant son
            // chemin de temps en temps.
            let sees = |distance: f32| !enemy || distance <= ENEMY_SIGHT;
            let chase = match foe {
                Some((_, distance, ox, oy)) if sees(distance) && distance > MELEE_REACH && (self.figures[i].path.is_empty() || self.ticks % CHASE_TICKS == 0) => {
                    Some(self.land_path(self.figures[i].from, (ox.round() as u16, oy.round() as u16)))
                }
                _ => None,
            };
            let figure = &mut self.figures[i];
            let Role::Soldier { enemy, army, health, target, .. } = figure.role else { continue };
            let reach = reach_of[i];
            match foe {
                Some((other, distance, ox, oy)) if distance <= reach => {
                    // Au corps à corps : on s'arrête, face à l'adversaire, et on frappe.
                    figure.path.clear();
                    figure.direction = direction((x.round() as u16, y.round() as u16), (ox.round() as u16, oy.round() as u16));
                    figure.role = Role::Soldier { enemy, army, health, target, fighting: true };
                    if strike {
                        wounds[other] += blows[i].map_or(0, |(melee, missile, missile_rate)| {
                            let target = defense[other];
                            if distance <= MELEE_REACH {
                                (melee - target.0).max(1)
                            } else if (self.ticks / STRIKE_TICKS * 37 + i as u64 * 13) % 100 < missile_rate as u64 {
                                (missile - target.1).max(1)
                            } else {
                                0
                            }
                        });
                    }
                }
                Some(_) if chase.is_some() => {
                    // Un envahisseur qui charge abandonne le bâtiment qu'il visait.
                    let target = if enemy { NO_TARGET } else { target };
                    figure.role = Role::Soldier { enemy, army, health, target, fighting: false };
                    let at = figure.from;
                    let path = chase.unwrap_or_default();
                    if let Some(&next) = path.last() {
                        figure.direction = direction(at, next);
                    }
                    figure.path = path;
                }
                _ => figure.role = Role::Soldier { enemy, army, health, target, fighting: false },
            }
        }
        // Les tours tirent sur l'envahisseur le plus proche à portée de leurs sentinelles.
        if strike {
            for (shot, target) in self.tower_shots() {
                let defense = defense[target].1;
                if (self.ticks / STRIKE_TICKS * 37 + shot as u64 * 13) % 100 < self.fighters.sentry.rate.max(0) as u64 {
                    wounds[target] += (self.fighters.sentry.missile_attack - defense).max(1);
                }
            }
        }
        // Les blessés perdent leurs forces ; ceux qui n'en ont plus tombent.
        for (figure, &wound) in self.figures.iter_mut().zip(&wounds) {
            if let Role::Soldier { enemy, army, health, target, fighting } = figure.role {
                figure.role = Role::Soldier { enemy, army, health: health - wound, target, fighting };
            }
        }
        self.figures.retain(|f| !matches!(f.role, Role::Soldier { health, .. } if health <= 0));
        // Envahisseurs libres : vers le bâtiment le plus proche, qu'ils détruisent. Les
        // bâtiments tombent après la boucle : leur disparition emporte des personnages.
        // Un mur sapé perd un point à chaque coup, quel que soit le nombre de sapeurs.
        let (mut burnt, mut sapped) = (Vec::new(), Vec::new());
        for i in 0..self.figures.len() {
            let Role::Soldier { enemy: true, army, health, target, fighting: false } = self.figures[i].role else { continue };
            if !self.figures[i].path.is_empty() {
                continue;
            }
            let at = self.figures[i].from;
            // Arrivé sur un mur : il le sape jusqu'à la brèche (voir `sapped`).
            if let Cell::Wall(_) = self.cell(at.0 as usize, at.1 as usize) {
                if strike && !sapped.contains(&at) {
                    sapped.push(at);
                }
                continue;
            }
            if self.buildings.get(target as usize).is_some_and(|b| b.is_some()) {
                burnt.push(target);
                self.figures[i].role = Role::Soldier { enemy: true, army, health, target: NO_TARGET, fighting: false };
                continue;
            }
            // Le corps de garde fait partie de l'enceinte : on le contourne comme un mur.
            let nearest = (0..self.buildings.len())
                .filter(|&id| self.function(id) != Function::Gatehouse)
                .filter_map(|id| self.buildings[id].as_ref().map(|b| (id, b.x as i32 - at.0 as i32, b.y as i32 - at.1 as i32)))
                .min_by_key(|&(_, dx, dy)| dx.abs().max(dy.abs()));
            let Some((id, ..)) = nearest else { continue };
            let goal = self.buildings[id].as_ref().map(|b| (b.x as u16, b.y as u16)).unwrap();
            // Les murs barrent la route : sans passage, l'envahisseur va saper le plus proche.
            let (path, target) = match self.land_route(at, goal, true) {
                Some(path) => (path, id as u16),
                None => match self.nearest_wall(at) {
                    Some(wall) => (self.land_route(at, wall, true).unwrap_or_else(|| straight_path(at, wall)), NO_TARGET),
                    None => (straight_path(at, goal), id as u16),
                },
            };
            let figure = &mut self.figures[i];
            if let Some(&next) = path.last() {
                figure.direction = direction(at, next);
            }
            figure.path = path;
            figure.role = Role::Soldier { enemy: true, army, health, target, fighting: false };
        }
        for (x, y) in sapped {
            let index = y as usize * GRID + x as usize;
            if let Cell::Wall(left) = self.cells[index] {
                self.cells[index] = if left > 1 { Cell::Wall(left - 1) } else { Cell::Rubble(0) };
            }
        }
        for target in burnt {
            let Some(building) = self.buildings[target as usize].as_ref() else { continue };
            let (kind, x, y) = (building.kind, building.x, building.y);
            self.remove_building(target, Cell::Fire(0));
            self.disasters.push(Disaster { cause: Cause::Raid, kind, x, y });
            raid.buildings += 1;
        }
        let invaders = self.figures.iter().filter(|f| f.soldier() == Some(true)).count();
        let nothing_left = self.buildings.iter().all(|b| b.is_none());
        let (repelled, over) = (invaders == 0, invaders == 0 || raid.buildings >= raid.limit || nothing_left);
        if over {
            let drachmas = if repelled { 0 } else { self.money.max(0) / 5 };
            self.money -= drachmas;
            self.figures.retain(|f| f.soldier().is_none() && !matches!(f.role, Role::Ship));
            self.battles.push(Battle { city: raid.city, soldiers: raid.soldiers, repelled, buildings: raid.buildings, drachmas });
            if repelled {
                self.commemoratives.push(COMMEMORATIVE_VICTORY);
            }
            self.raid = None;
        } else {
            self.raid = Some(raid);
        }
    }

    /// Visite d'un dieu favorable : il paraît un mois près de son sanctuaire et laisse
    /// un bienfait (Olympe : vivres de Déméter, huile d'Athéna, vin de Dionysos, armes
    /// d'Héphaïstos, drachmes des autres).
    fn bless(&mut self, god: u8) {
        let (resource, amount) = match god {
            2 => (7, 32),
            8 => (20, 16),
            10 => (21, 16),
            9 => (18, 8),
            _ => (DRACHMAS as usize, 300),
        };
        let given = if resource == DRACHMAS as usize {
            self.money += amount;
            amount
        } else {
            self.store(resource, amount)
        };
        self.blessings.push((god, resource as u8, given));
        let sanctuary = self.buildings.iter().flatten().find(|b| matches!(self.kinds[b.kind].function, Function::Sanctuary { god: g, .. } if g == god));
        let Some(at) = sanctuary.map(|b| (b.x as u16, b.y as u16)) else { return };
        let now = self.ticks;
        self.figures.push(Figure {
            image_base: 1,
            x: at.0 as f32,
            y: at.1 as f32,
            direction: 0,
            frame: 0,
            path: Vec::new(),
            from: at,
            came_from: None,
            role: Role::Visit { god, leaves_at: now + TICKS_PER_MONTH },
            source: Source::Deity(god),
        });
    }

    /// Programme la venue d'un dieu hostile.
    pub fn schedule_god(&mut self, god: u8, month: u8, years: (i16, i16)) {
        let at = self.date_in(month, years);
        self.invasions.push((at, god));
    }

    /// Dieux hostiles dont la venue est échue : ils entrent par l'entrée de la carte.
    fn send_gods(&mut self) {
        let now = self.ticks;
        let due: Vec<u8> = self.invasions.iter().filter(|&&(at, _)| at <= now).map(|&(_, god)| god).collect();
        self.invasions.retain(|&(at, _)| at > now);
        let built = self.sanctuaries();
        for god in due {
            // Un dieu défenseur dont le sanctuaire est achevé chasse vite l'envahisseur.
            let defended = GOD_DEFENDERS.iter().any(|&(defender, foes)| built.contains(&defender) && (foes.is_empty() || foes.contains(&god)));
            let stay = if defended { GOD_REPELLED_TICKS } else { GOD_STAY_TICKS };
            let at = self.city_entrance();
            self.figures.push(Figure {
                image_base: 1,
                x: at.0 as f32,
                y: at.1 as f32,
                direction: 0,
                frame: 0,
                path: Vec::new(),
                from: at,
                came_from: None,
                role: Role::God { god, target: NO_TARGET, rest_until: now, leaves_at: now + stay },
                source: Source::Deity(god),
            });
            self.god_news.push((god, true));
        }
    }

    /// Première case dessinée en venant de l'entrée de la carte vers son centre.
    fn city_entrance(&self) -> (u16, u16) {
        let start = self.map.entry_points().first().copied().unwrap_or((GRID / 2, GRID / 2));
        let inward = straight_path((start.0 as u16, start.1 as u16), (GRID as u16 / 2, GRID as u16 / 2));
        inward
            .iter()
            .rev()
            .copied()
            .find(|&(x, y)| self.map.image(x as usize, y as usize) != 0 && self.map.edge(x as usize, y as usize) & EDGE_DRAW_TILE != 0)
            .unwrap_or((GRID as u16 / 2, GRID as u16 / 2))
    }

    /// Où va un héros : vers le monstre vivant qu'il doit combattre, sinon devant son
    /// temple.
    fn hero_goal(&self, hero: u8, hall: u16) -> Option<(u16, u16)> {
        let foe = self.monsters.iter().find(|m| !m.slain && m.arrives_at <= self.ticks && crate::heroes::hero_against(m.monster) == Some(hero));
        if let Some(foe) = foe {
            if let Some(f) = self.figures.iter().find(|f| matches!(f.role, Role::Monster { monster, .. } if monster == foe.monster)) {
                return Some(f.from);
            }
        }
        let building = self.buildings.get(hall as usize)?.as_ref()?;
        self.road_next_to(hall as usize).or(Some((building.x as u16, building.y as u16)))
    }

    /// Héros à l'écran : chaque héros présent et qui n'est pas parti en quête se tient
    /// près de son temple ; celui qui est en quête n'est plus dans la cité.
    fn place_heroes(&mut self) {
        let away: Vec<u8> = self.quests.iter().filter(|q| matches!(q.state, QuestState::Away { .. })).map(|q| q.hero).collect();
        let present = self.heroes;
        self.figures.retain(|f| !matches!(f.role, Role::Hero { hero, .. } if !present[hero as usize] || away.contains(&hero)));
        for hero in 0..crate::heroes::HEROES as u8 {
            if !present[hero as usize] || away.contains(&hero) || self.figures.iter().any(|f| matches!(f.role, Role::Hero { hero: h, .. } if h == hero)) {
                continue;
            }
            let Some(hall) = (0..self.buildings.len()).find(|&i| self.buildings[i].as_ref().is_some_and(|b| self.kinds[b.kind].function == Function::HeroHall(hero))) else { continue };
            let Some(at) = self.road_next_to(hall).or_else(|| self.buildings[hall].as_ref().map(|b| (b.x as u16, b.y as u16))) else { continue };
            self.figures.push(Figure {
                image_base: 1,
                x: at.0 as f32,
                y: at.1 as f32,
                direction: 0,
                frame: 0,
                path: Vec::new(),
                from: at,
                came_from: None,
                role: Role::Hero { hero, hall: hall as u16 },
                source: Source::Hero(hero),
            });
        }
    }

    /// La quête ou le monstre de cet événement est venu à bout.
    pub fn quest_done(&self, event: usize) -> bool {
        self.quests.iter().any(|q| q.event == event && q.state == QuestState::Done)
    }

    pub fn monster_slain(&self, event: usize) -> bool {
        self.monsters.iter().any(|m| m.event == event && m.slain)
    }

    /// Ce qui manque encore à un chantier : sanctuaire ou monument inachevé (s'il a ses
    /// ouvriers), quai des navires sans navire.
    fn site_needs(&self, building: &Building) -> Vec<(Good, i32)> {
        let wanted: Vec<(Good, i32)> = match self.kinds[building.kind].function {
            Function::Sanctuary { needs, .. } | Function::Monument { needs, .. } if building.staffed => {
                SANCTUARY_MATERIALS.iter().zip(needs).map(|(&good, need)| (good, need as i32)).collect()
            }
            Function::Wharf if building.staffed && building.stock == 0 => vec![(Good::Wood, TRIREME_WOOD), (Good::Arms, TRIREME_ARMS)],
            Function::Workshop(Good::Chariot) if building.staffed => vec![(Good::Wood, CHARIOT_WOOD), (Good::Horses, CHARIOT_HORSES)],
            _ => Vec::new(),
        };
        wanted.into_iter().map(|(good, need)| (good, need - building.goods[good as usize])).filter(|&(_, missing)| missing > 0).collect()
    }

    /// Chantiers : chaque jour, un chantier relié par la route à un entrepôt qui a ce
    /// qui lui manque y fait partir un porteur, un seul à la fois par matériau.
    fn supply_sites(&mut self) {
        for id in 0..self.buildings.len() {
            let Some(building) = &self.buildings[id] else { continue };
            for (good, missing) in self.site_needs(building) {
                let on_the_way = self.figures.iter().any(|f| matches!(f.role, Role::Supply { to, good: g, .. } if to as usize == id && g == good));
                if on_the_way {
                    continue;
                }
                let Some(start) = self.road_next_to(id) else { continue };
                // Les chevaux ne passent pas par l'entrepôt : on va les prendre au haras.
                let has = |world: &World, store: u16| {
                    let Some(b) = world.buildings[store as usize].as_ref() else { return false };
                    match world.function(store as usize) {
                        Function::Storehouse => b.goods[good as usize] > 0,
                        Function::Producer(made) => made == good && good == Good::Horses && b.stock > 0,
                        _ => false,
                    }
                };
                let Some((store, mut path)) = self.road_path(start, has) else { continue };
                let from_ranch = self.function(store as usize) != Function::Storehouse;
                let Some(shelf) = self.buildings[store as usize].as_mut().map(|b| if from_ranch { &mut b.stock } else { &mut b.goods[good as usize] }) else { continue };
                let load = (*shelf).min(missing).min(CARRIER_LOAD);
                *shelf -= load;
                // Le chemin va du chantier à l'entrepôt : le porteur le suit à rebours.
                path.reverse();
                self.spawn_on_path(self.walkers.carrier, path, Role::Supply { to: id as u16, good, load });
            }
        }
    }

    /// Compte la production d'un bâtiment pour les objectifs.
    fn count_production(&mut self, kind: usize, amount: i32) {
        let product = self.kinds[kind].product as usize;
        if product != 0 && product < RESOURCES {
            self.produced[product] += amount;
        }
    }

    /// Date courante : (jour du mois, mois, année), tous à partir de 0.
    pub fn date(&self) -> (u64, u64, u64) {
        let days = self.ticks / TICKS_PER_DAY;
        (days % DAYS_PER_MONTH, days / DAYS_PER_MONTH % 12, days / DAYS_PER_MONTH / 12)
    }

    /// Bâtiment occupant la case, s'il y en a un.
    pub fn building_at(&self, x: i32, y: i32) -> Option<&Building> {
        if !Self::in_grid(x, y) {
            return None;
        }
        match self.cell(x as usize, y as usize) {
            Cell::Building(id) => self.buildings[id as usize].as_ref(),
            _ => None,
        }
    }

    /// Avancement des risques d'un bâtiment, en pourcentage du seuil de sinistre :
    /// (incendie, effondrement).
    pub fn risks(&self, building: &Building) -> (i32, i32) {
        ((building.fire * 100 / RISK_LIMIT).min(100), (building.damage * 100 / RISK_LIMIT).min(100))
    }

    /// Ce qui manque à une maison pour évoluer, d'après ses réserves et les services
    /// reçus (comme à la revue mensuelle) ; vide si elle évoluera.
    pub fn house_needs(&self, building: &Building) -> Vec<Need> {
        let Some(house) = &building.house else { return Vec::new() };
        let (Some(current), (_, Some(next))) = (self.house_levels.get(house.level), house_steps(house.level)) else { return vec![Need::Top] };
        let Some(wanted) = self.house_levels.get(next) else { return vec![Need::Top] };
        let stadium_open = self.stadium_open();
        let state = HouseState {
            mount: self.mount(),
            fed: house.goods[Good::Food as usize] >= house.population.max(1),
            had: house.goods.map(|amount| amount > 0),
            water: house.has(Service::Water, self.ticks),
            culture: culture_of(house, self.ticks, stadium_open, self.atlantean, self.has_sanctuary(APOLLO)),
            stadium_open,
        };
        let mut needs = state.missing(wanted);
        let relief = if self.has_sanctuary(APHRODITE) { APHRODITE_APPEAL } else { 0 };
        if self.appeal(building.x as i32, building.y as i32) + relief < current.evolve_desirability {
            needs.push(Need::Appeal(current.evolve_desirability));
        }
        needs
    }

    /// Pourquoi un bâtiment de service n'envoie pas son marcheur : percepteurs sans
    /// palais, philosophes et acteurs sans école.
    pub fn service_waits_for(&self, building: &Building) -> Option<Function> {
        let has = |function: Function| self.buildings.iter().flatten().any(|b| b.staffed && self.kinds[b.kind].function == function);
        match self.kinds[building.kind].function {
            Function::Service(Service::Tax) if !self.buildings.iter().flatten().any(|b| self.kinds[b.kind].function == Function::Palace) => Some(Function::Palace),
            Function::Service(service @ (Service::Philosophy | Service::Drama)) if !has(Function::School(service)) => Some(Function::School(service)),
            _ => None,
        }
    }

    /// Un stade, ou dans une cité atlante un musée, a son personnel.
    pub fn stadium_open(&self) -> bool {
        self.buildings.iter().flatten().any(|b| b.staffed && matches!(self.kinds[b.kind].function, Function::Stadium | Function::Museum))
    }

    /// Note de culture qu'exige un niveau de logement.
    pub fn level_culture(&self, level: usize) -> i32 {
        self.house_levels.get(level).map_or(0, |l| l.culture)
    }

    /// Note de culture d'une maison (philosophie, athlétisme, théâtre, stade).
    pub fn house_culture(&self, house: &House) -> i32 {
        let stadium_open = self.stadium_open();
        culture_of(house, self.ticks, stadium_open, self.atlantean, self.has_sanctuary(APOLLO))
    }

    /// Capacité d'accueil d'une maison à son niveau actuel.
    pub fn capacity(&self, house: &House) -> i32 {
        self.house_levels.get(house.level).map_or(0, |level| level.capacity)
    }

    /// Avance la simulation d'un pas.
    pub fn tick(&mut self) {
        self.ticks += 1;
        self.move_figures();
        self.fight();
        if self.ticks % TICKS_PER_DAY == 0 {
            self.migrate();
        }
        self.run_buildings();
        self.burn_out();
        self.recede();
        if self.ticks % TICKS_PER_DAY == 0 {
            self.update_risks();
            self.supply_sites();
            self.update_health();
            self.run_expedition();
        }
        // Humeur et criminalité : aussi en milieu de mois.
        if self.ticks % TICKS_PER_MONTH == TICKS_PER_MONTH / 2 {
            self.update_sentiment();
        }
        if self.ticks % TICKS_PER_MONTH == 0 {
            self.months_in_debt = if self.money < 0 { self.months_in_debt + 1 } else { 0 };
            self.update_houses();
            self.reach_milestones();
            self.run_games();
            self.cities_month();
            self.assign_workers();
            self.settle_month();
            self.aim_health();
        }
    }

    /// Jeux panhelléniques, au début de chaque mois : départ des concurrents au sixième
    /// mois, résultat au huitième.
    fn run_games(&mut self) {
        let (_, month, year) = self.date();
        if self.games_from.is_none_or(|from| (year as i32) < from) {
            return;
        }
        let kind = ((self.start_year + year as i32) & 3) as u8;
        match month {
            GAMES_BEGIN_MONTH => {
                let home = kind == GAMES_OLYMPIAN && self.games_host;
                let count = |f: Function| self.buildings.iter().flatten().filter(|b| self.kinds[b.kind].function == f).count() as i32;
                let (podiums, colleges) = (count(Function::Service(Service::Philosophy)), count(Function::School(Service::Philosophy)));
                let (gymnasiums, stadiums) = (count(Function::Service(Service::Athletics)), count(Function::Stadium));
                let (theatres, drama_schools) = (count(Function::Service(Service::Drama)), count(Function::School(Service::Drama)));
                if home && stadiums == 0 {
                    self.games_host = false;
                    self.games_news.push(GamesNews::HomeCancelled);
                    return;
                }
                // Qui peut concourir (0x5e7e57).
                let able = match kind {
                    GAMES_ISTHMIAN => podiums > 0,
                    GAMES_NEMEAN => gymnasiums + stadiums > 0,
                    GAMES_PYTHIAN => theatres > 0,
                    _ => podiums + gymnasiums + stadiums + theatres > 0,
                };
                if !able {
                    self.games_news.push(GamesNews::CantParticipate(kind));
                    return;
                }
                // Score (0x5e8082) : le quart de la part des habitants desservis, plus les
                // lieux de la discipline ; un stade compte cinq fois.
                let covered = |service: Service| {
                    let (mut served, mut all) = (0, 0);
                    for house in self.buildings.iter().flatten().filter_map(|b| b.house.as_ref()) {
                        all += house.population;
                        if house.has(service, self.ticks) {
                            served += house.population;
                        }
                    }
                    if all == 0 { 0 } else { served * 100 / all }
                };
                let philosophy = covered(Service::Philosophy) / 4 + colleges + podiums;
                let athletics = covered(Service::Athletics) / 4 + gymnasiums + 5 * stadiums;
                let drama = covered(Service::Drama) / 4 + drama_schools + theatres;
                let bonus = if self.hippodrome().1 { GAMES_BONUS } else { 0 };
                let mut score = bonus
                    + match kind {
                        GAMES_ISTHMIAN => philosophy,
                        GAMES_NEMEAN => athletics,
                        GAMES_PYTHIAN => drama,
                        _ => (philosophy + athletics + drama) / 3 + 4 * stadiums,
                    };
                // Un héros présent dans la cité porte le score au carré, divisé par 6,25
                // (0x5e816e, d'après le temple des héros).
                let away: Vec<u8> = self.quests.iter().filter(|q| matches!(q.state, QuestState::Away { .. })).map(|q| q.hero).collect();
                if (0..self.heroes.len()).any(|h| self.heroes[h] && !away.contains(&(h as u8))) {
                    score = score * score * 4 / 25;
                }
                let roll = (self.next_random() % 100) as i32;
                let place = if roll < score { 2 } else if score >= GAMES_SECOND { 1 } else { 0 };
                self.games_running = Some((kind, home, place));
                self.games_news.push(GamesNews::Begin { kind, home });
            }
            GAMES_END_MONTH => {
                let Some((kind, home, place)) = self.games_running.take() else { return };
                self.games_news.push(GamesNews::Concluded { kind, home, place });
                // Les vainqueurs d'Olympie accueillent les prochains jeux (0x5e8238).
                if kind == GAMES_OLYMPIAN {
                    self.games_host = place == 2;
                }
                if place != 2 {
                    return;
                }
                self.commemoratives.push(if matches!(kind, GAMES_OLYMPIAN | GAMES_NEMEAN) { COMMEMORATIVE_ATHLETES } else { COMMEMORATIVE_WISDOM });
                // Les cités du monde en sont plus favorables (0x5e8998) : alliées et
                // vassales de 5, les autres de 10 ; 10 de plus pour les Olympiques.
                let extra = if kind == GAMES_OLYMPIAN { 10 } else { 0 };
                for city in self.cities.iter_mut().filter(|c| c.active && c.kind == FOREIGN) {
                    let gain = if matches!(city.status, crate::map::ALLY | VASSAL) { 5 } else { 10 } + extra;
                    city.favor = (city.favor + gain).min(100);
                }
            }
            _ => {}
        }
    }

    /// Chaque palier de population atteint pour la première fois vaut un monument.
    fn reach_milestones(&mut self) {
        let population = self.population();
        while POPULATION_MILESTONES.get(self.milestones as usize).is_some_and(|&m| population >= m) {
            self.milestones += 1;
            self.commemoratives.push(COMMEMORATIVE_GROWTH);
        }
    }

    /// Les eaux des raz-de-marée se retirent à leur date.
    fn recede(&mut self) {
        if self.floods.iter().all(|&(.., until)| until > self.ticks) {
            return;
        }
        let now = self.ticks;
        for &(x, y, _) in self.floods.iter().filter(|&&(.., until)| until <= now) {
            let cell = &mut self.cells[y as usize * GRID + x as usize];
            if *cell == Cell::Flood {
                *cell = Cell::Terrain;
            }
        }
        self.floods.retain(|&(.., until)| until > now);
    }

    /// Image de l'eau de la carte : la plus fréquente parmi ses cases d'eau.
    fn water_image(&self) -> (Source, usize) {
        let mut counts = std::collections::HashMap::new();
        for y in (0..GRID).step_by(3) {
            for x in (0..GRID).step_by(3) {
                if self.map.terrain(x, y) & TERRAIN_WATER != 0 && self.map.image(x, y) != 0 {
                    *counts.entry(self.map.image(x, y)).or_insert(0) += 1;
                }
            }
        }
        let commonest = counts.into_iter().max_by_key(|&(id, n)| (n, id)).map(|(id, _)| id);
        commonest.and_then(|id| self.map.resolve(id)).unwrap_or((Source::Terrain, WATER_IMAGE))
    }

    /// Catastrophe partie du point `point` de la carte : la zone peinte dans
    /// l'éditeur (fissure, lave ou eau) qui passe à moins de trois cases du point, ou
    /// toute la zone de la carte si le point n'en touche aucune. Le jeu la fait
    /// progresser case à case ; Olympe la frappe d'un coup. Les bâtiments touchés sont
    /// détruits ; la lave et la fissure rendent les cases inutilisables, l'eau d'un
    /// raz-de-marée se retire au bout de six mois, sauf inondation.
    pub fn strike(&mut self, change: u8, point: usize) -> i32 {
        if change == LANDSLIDE {
            return self.landslide(point);
        }
        let bit = match change {
            EARTHQUAKE => TERRAIN_FISSURE,
            LAVA => TERRAIN_LAVA_ZONE,
            _ => TERRAIN_FLOOD_ZONE,
        };
        let in_zone = |w: &World, x: i32, y: i32| {
            w.in_map(x, y)
                && w.map.terrain(x as usize, y as usize) & bit != 0
                && (bit != TERRAIN_FLOOD_ZONE || w.map.terrain(x as usize, y as usize) & TERRAIN_WATER == 0)
                && !matches!(w.cell(x as usize, y as usize), Cell::Chasm(_) | Cell::Lava(_) | Cell::Flood)
        };
        let mut zone = vec![false; GRID * GRID];
        let mut queue: VecDeque<(i32, i32)> = VecDeque::new();
        if let Some((px, py)) = self.map.disaster_point(point) {
            let (px, py) = (px as i32, py as i32);
            for y in py - DISASTER_REACH..=py + DISASTER_REACH {
                for x in px - DISASTER_REACH..=px + DISASTER_REACH {
                    if in_zone(self, x, y) {
                        queue.push_back((x, y));
                    }
                }
            }
        }
        if queue.is_empty() {
            queue = (0..GRID as i32 * GRID as i32).map(|i| (i % GRID as i32, i / GRID as i32)).filter(|&(x, y)| in_zone(self, x, y)).collect();
        }
        for &(x, y) in &queue {
            zone[y as usize * GRID + x as usize] = true;
        }
        while let Some((x, y)) = queue.pop_front() {
            for (nx, ny) in (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (x + dx, y + dy))) {
                if in_zone(self, nx, ny) && !std::mem::replace(&mut zone[ny as usize * GRID + nx as usize], true) {
                    queue.push_back((nx, ny));
                }
            }
        }
        let mut struck: Vec<u16> = zone.iter().enumerate().filter(|&(_, &z)| z).filter_map(|(i, _)| match self.cells[i] {
            Cell::Building(id) => Some(id),
            _ => None,
        }).collect();
        struck.sort_unstable();
        struck.dedup();
        for &id in &struck {
            let left = if change == LAVA { Cell::Fire(0) } else { Cell::Rubble(0) };
            self.remove_building(id, left);
        }
        let until = if change == TIDAL_WAVE { self.ticks + FLOOD_TICKS } else { 0 };
        for (i, _) in zone.iter().enumerate().filter(|&(_, &z)| z) {
            let (x, y) = (i % GRID, i / GRID);
            self.cells[i] = match change {
                EARTHQUAKE => Cell::Chasm(((x * 31 + y * 17) % CHASM_VARIANTS) as u8),
                LAVA => Cell::Lava(((x * 7 + y * 3) % LAVA_IMAGES) as u8),
                _ => Cell::Flood,
            };
            if until > 0 {
                self.floods.push((x as u16, y as u16, until));
            }
        }
        // Les flammes de la lave s'éteignent sous elle.
        self.fires.retain(|&(x, y, _)| !zone[y as usize * GRID + x as usize]);
        self.disaster_news.push((change, struck.len() as i32));
        struck.len() as i32
    }

    /// Glissement de terrain au point `point` : les cases au pied du relief, à moins
    /// de quatre cases du point, se couvrent d'éboulis (dessinés comme la lave
    /// refroidie) ; les bâtiments touchés s'effondrent.
    fn landslide(&mut self, point: usize) -> i32 {
        let Some((px, py)) = self.map.disaster_point(point) else { return 0 };
        let (px, py) = (px as i32, py as i32);
        let elevated = |w: &World, x: i32, y: i32| Self::in_grid(x, y) && w.map.terrain(x as usize, y as usize) & TERRAIN_ELEVATION != 0;
        let mut tiles = Vec::new();
        for y in py - LANDSLIDE_REACH..=py + LANDSLIDE_REACH {
            for x in px - LANDSLIDE_REACH..=px + LANDSLIDE_REACH {
                if !self.in_map(x, y) || elevated(self, x, y) || self.map.terrain(x as usize, y as usize) & TERRAIN_WATER != 0 {
                    continue;
                }
                let near = (-LANDSLIDE_SLOPE..=LANDSLIDE_SLOPE).any(|dy| (-LANDSLIDE_SLOPE..=LANDSLIDE_SLOPE).any(|dx| elevated(self, x + dx, y + dy)));
                if near {
                    tiles.push(y as usize * GRID + x as usize);
                }
            }
        }
        let mut struck: Vec<u16> = tiles.iter().filter_map(|&i| if let Cell::Building(id) = self.cells[i] { Some(id) } else { None }).collect();
        struck.sort_unstable();
        struck.dedup();
        for &id in &struck {
            self.remove_building(id, Cell::Rubble(0));
        }
        for &i in &tiles {
            self.cells[i] = Cell::Lava(((i * 7) % LAVA_IMAGES) as u8);
        }
        self.disaster_news.push((LANDSLIDE, struck.len() as i32));
        struck.len() as i32
    }

    /// Les incendies arrivés à leur terme ne laissent que des décombres.
    fn burn_out(&mut self) {
        if self.fires.iter().all(|&(.., until)| until > self.ticks) {
            return;
        }
        let now = self.ticks;
        for &(x, y, _) in self.fires.iter().filter(|&&(.., until)| until <= now) {
            let cell = &mut self.cells[y as usize * GRID + x as usize];
            if let Cell::Fire(variant) = *cell {
                *cell = Cell::Rubble(variant);
            }
        }
        self.fires.retain(|&(.., until)| until > now);
    }

    /// Chaque jour, les bâtiments que la maintenance ne visite pas risquent de se
    /// dégrader ; passé le seuil, ils brûlent ou s'effondrent.
    fn update_risks(&mut self) {
        let fireproof = self.has_sanctuary(HEPHAISTOS);
        for id in 0..self.buildings.len() {
            let Some(building) = self.buildings[id].as_ref() else { continue };
            let house = building.house.as_ref().map(|h| (h.population, h.level));
            let fire_day = self.next_random() % RISK_DAILY_CHANCE == 0;
            let draw = 1 + (self.next_random() % FIRE_DRAW) as i32;
            let Some(building) = self.buildings[id].as_mut() else { continue };
            let kind = &self.kinds[building.kind];
            building.damage += kind.damage_risk;
            // Les huttes et les baraques ne s'effondrent pas.
            if house.is_some_and(|(_, level)| level < STURDY_HOUSE_LEVELS) {
                building.damage = 0;
            }
            if fire_day && !fireproof {
                building.fire += kind.fire_risk * draw;
            }
            // Une maison vide ne risque pas l'incendie.
            if house.is_some_and(|(population, _)| population == 0) {
                building.fire = 0;
            }
            let (burns, collapses) = (building.fire > RISK_LIMIT, building.damage > RISK_LIMIT);
            let (kind, x, y) = (building.kind, building.x, building.y);
            if burns || collapses {
                self.remove_building(id as u16, if burns { Cell::Fire(0) } else { Cell::Rubble(0) });
                self.disasters.push(Disaster { cause: if burns { Cause::Fire } else { Cause::Collapse }, kind, x, y });
            }
        }
    }

    /// Santé visée pour le mois (`Zeus.exe`, 0x566c00) ; une nouvelle épidémie redevient
    /// possible.
    fn aim_health(&mut self) {
        let now = self.ticks;
        let (mut people, mut weighted) = (0, 0);
        for house in self.buildings.iter().flatten().filter_map(|b| b.house.as_ref()).filter(|h| h.population > 0) {
            let healed = if house.has(Service::Health, now) { HEALER_HEALTH } else { 0 };
            let fed = if house.ate { FED_HEALTH } else { 0 };
            people += house.population;
            weighted += house.population * (healed + fed);
        }
        let average = if people > 0 { weighted / people } else { 0 };
        let bonus = HEALTH_POPULATION_BONUS.iter().find(|(below, _)| people < *below).map_or(0, |&(_, bonus)| bonus);
        self.health_target = (average + bonus).clamp(0, 100);
        self.plague_month = false;
    }

    /// Chaque jour, la santé se rapproche d'un point de sa cible, puis une épidémie peut
    /// éclater (`Zeus.exe`, 0x566b90 et 0x447ff0).
    fn update_health(&mut self) {
        self.health = (self.health + (self.health_target - self.health).signum()).clamp(0, 100);
        let Some(&(_, odds)) = PLAGUE_ODDS.iter().find(|(below, _)| self.health < *below) else { return };
        let most = ((odds * PLAGUE_DAYS + 9_999) / 10_000) as u32;
        let (this_year, last_year) = self.plagues;
        if this_year + last_year >= most || self.plague_month || this_year >= most.div_ceil(2) {
            return;
        }
        if (self.next_random() % 10_000) < odds as u32 {
            self.plague();
        }
    }

    /// Naissances et décès de l'année (`Zeus.exe`, 0x566d40).
    fn natural_growth(&mut self) {
        let healthy = HEALTHY_GROWTH.iter().find(|(above, _)| self.health > *above).map(|&(_, rate)| rate);
        let rate = healthy.unwrap_or(if self.health_target > DECLINE_TARGET {
            -1
        } else {
            SICKLY_DECLINE.iter().find(|(above, _)| self.health > *above).map_or(0, |&(_, rate)| rate)
        }) + if self.has_sanctuary(HERA) { HERA_GROWTH } else { 0 };
        let mut count = self.population() * rate.abs() / 100;
        for id in 0..self.buildings.len() {
            if count == 0 {
                break;
            }
            let capacity = self.buildings[id].as_ref().and_then(|b| b.house.as_ref()).map_or(0, |h| self.capacity(h));
            let Some(house) = self.buildings[id].as_mut().and_then(|b| b.house.as_mut()).filter(|h| h.level < ELITE_FIRST_LEVEL) else { continue };
            let change = if rate > 0 {
                if house.population == 0 {
                    continue;
                }
                (capacity - house.population - house.arriving).clamp(0, count)
            } else {
                -house.population.min(count)
            };
            house.population += change;
            count -= change.abs();
        }
    }

    fn plague(&mut self) {
        self.plague_month = true;
        self.plagues.0 += 1;
        let now = self.ticks;
        // Les maisons habitées, des plus exposées aux mieux protégées.
        let mut houses: Vec<(i32, usize)> = (0..self.buildings.len())
            .filter_map(|id| {
                let house = self.buildings[id].as_ref()?.house.as_ref().filter(|h| h.population > 0)?;
                let care = house.covered_until[Service::Health as usize].saturating_sub(now).div_ceil(TICKS_PER_DAY) as i32;
                Some((if house.ate { FED_PLAGUE_SHIELD } else { 0 } + care, id))
            })
            .collect();
        houses.sort();
        let stricken = houses.len() as i32 * 2 * PLAGUE_HOUSES_PERCENT / 200;
        let (part, whole) = PLAGUE_DEATHS[self.difficulty.min(PLAGUE_DEATHS.len() - 1)];
        let (mut dead, mut at) = (0, None);
        for (rank, &(_, id)) in houses.iter().take(stricken as usize).enumerate() {
            if rank as i32 >= stricken / 2 && self.next_random() % 2 == 0 {
                continue;
            }
            let Some(building) = self.buildings[id].as_mut() else { continue };
            let (x, y) = (building.x, building.y);
            let Some(house) = building.house.as_mut() else { continue };
            let killed = house.population * part / whole;
            house.population -= killed;
            dead += killed;
            at.get_or_insert((x, y));
        }
        if let Some((x, y)) = at.filter(|_| dead > 0) {
            self.incidents.push(Incident::Plague { dead, x, y });
        }
    }

    /// Habitants en âge de travailler.
    pub fn workers_available(&self) -> i32 {
        // Les habitants des logements luxueux ne travaillent pas.
        let houses = self.buildings.iter().flatten().filter_map(|b| b.house.as_ref());
        let common: i32 = houses.filter(|h| h.level < ELITE_FIRST_LEVEL).map(|h| h.population).sum();
        let wage = NORMAL_WAGE + self.wage_shift;
        let wage_level = WAGE_LEVELS.iter().enumerate().min_by_key(|(_, level)| (**level - wage).abs()).map_or(3, |(i, _)| i);
        let health = WORKFORCE_BY_HEALTH.iter().find(|(from, _)| self.health >= *from).map_or(WORKFORCE_SICK, |&(_, bonus)| bonus);
        let percent = WORKFORCE_BY_DIFFICULTY[self.difficulty.min(4)] + WORKFORCE_BY_WAGE[wage_level] + health;
        common * percent / 100
    }

    /// Employés demandés par l'ensemble des bâtiments.
    pub fn workers_needed(&self) -> i32 {
        self.buildings.iter().flatten().map(|b| self.kinds[b.kind].employees).sum()
    }

    /// Répartit la main-d'œuvre comme `Zeus.exe` (0x487750 puis 0x487ba0) : si elle
    /// suffit, chaque bâtiment a tout son effectif ; sinon la pénurie est partagée,
    /// chacun recevant la même part de ses postes (arrondie au-dessous), et le reste
    /// va un par un aux bâtiments, dans l'ordre. Le jeu sert ses catégories par
    /// priorité, toutes égales par défaut, ce qui revient à ce partage.
    fn assign_workers(&mut self) {
        let available = self.workers_available().max(0);
        let needed = self.workers_needed();
        let mut left = available;
        for building in self.buildings.iter_mut().flatten() {
            let wanted = self.kinds[building.kind].employees;
            building.workers = if needed <= available { wanted } else { wanted * available / needed.max(1) };
            left -= building.workers;
        }
        for building in self.buildings.iter_mut().flatten() {
            if left <= 0 {
                break;
            }
            if building.workers < self.kinds[building.kind].employees {
                building.workers += 1;
                left -= 1;
            }
        }
        for building in self.buildings.iter_mut().flatten() {
            building.staffed = building.workers > 0 || self.kinds[building.kind].employees == 0;
        }
    }

    /// Niveau d'imposition, dans les bornes de la table du jeu.
    pub fn set_tax_level(&mut self, level: usize) {
        // Sans la table du jeu, les sept niveaux habituels.
        let levels = if self.taxes.levels.is_empty() { 2 * NORMAL_TAX_LEVEL + 1 } else { self.taxes.levels.len() };
        self.tax_level = level.min(levels - 1);
    }

    /// Humeur de la cité, deux fois par mois, comme `Zeus.exe` (0x5671a0) : somme des
    /// effets des impôts, des salaires, du chômage, des vivres et des dettes, amortie
    /// quand elle éloigne l'humeur de la zone de 41 à 60. Tant que la cité n'a pas
    /// dépassé 349 habitants, les effets négatifs (hors dettes) ne comptent pas.
    fn update_sentiment(&mut self) {
        self.grown |= self.population() >= GROWN_POPULATION;
        self.update_crime();
        let grace = |effect: i32| if effect < 0 && !self.grown { 0 } else { effect };
        let houses: Vec<&House> = self.buildings.iter().flatten().filter_map(|b| b.house.as_ref()).filter(|h| h.population > 0).collect();
        // Impôts : sans percepteurs (moins de 11 % des maisons), comme s'il n'y en avait pas.
        let taxed = houses.iter().filter(|h| h.has(Service::Tax, self.ticks)).count() as i32;
        let level = if taxed * 100 < MIN_TAX_COVERAGE_PERCENT * houses.len() as i32 { 0 } else { self.tax_level };
        let taxes = grace(self.taxes.levels.get(level).map_or(0, |l| l.1[self.difficulty.min(4)]));
        // Salaires : effet du niveau le plus proche du salaire actuel.
        let wage = NORMAL_WAGE + self.wage_shift;
        let wages = grace(WAGE_LEVELS.iter().zip(WAGE_MOOD).min_by_key(|(level, _)| (*level - wage).abs()).map_or(0, |(_, mood)| mood));
        // Chômage, en part des travailleurs.
        let (needed, available) = (self.workers_needed(), self.workers_available());
        let jobless = if available > 0 { (available - needed).max(0) * 100 / available } else { 0 };
        let unemployment = grace(match jobless {
            ..5 => 1,
            5..11 => 0,
            11..18 => -1,
            18..26 => -2,
            _ => -3,
        });
        // Vivres : +2 par maison qui a mangé, -1, -2 puis -3 par maison affamée depuis
        // une, deux, trois mises à jour ; moyenne arrondie au plus proche.
        let mut votes = Vec::new();
        for house in self.buildings.iter_mut().flatten().filter_map(|b| b.house.as_mut()).filter(|h| h.population > 0 && h.level < ELITE_FIRST_LEVEL) {
            if house.ate {
                house.hungry = 0;
                votes.push(2);
            } else {
                house.hungry = (house.hungry + 1).min(3);
                votes.push(-(house.hungry as i32));
            }
        }
        let food = match votes.len() as i32 {
            0 => 0,
            n => {
                let sum: i32 = votes.iter().sum();
                let (mean, rest) = (sum / n, sum % n);
                grace(if rest.abs() > n / 2 { mean + sum.signum() } else { mean })
            }
        };
        // Dettes : -2, et -2, -5 puis -9 après une, deux, trois années de dettes.
        let years = self.months_in_debt as i32 / 12;
        let debt = [0, -2, -5, -9][years.min(3) as usize] + if self.money < 0 { -2 } else { 0 };
        let change = taxes + wages + unemployment + food + debt;
        let damping = match self.sentiment {
            ..11 => 4,
            11..21 => 3,
            21..31 => 2,
            31..41 => 1,
            41..61 => 0,
            61..71 => -1,
            71..81 => -2,
            81..91 => -3,
            _ => -4,
        };
        let change = match self.sentiment {
            ..41 if change < 0 => (change + damping).min(0),
            61.. if change > 0 => (change + damping).max(0),
            _ => change,
        };
        self.sentiment = (self.sentiment + change).clamp(0, 100);
    }

    /// Criminalité des logements modestes (`Zeus.exe`, 0x566f20) : une fois la cité
    /// grande, chaque maison habitée ajoute l'incrément de son niveau et la moitié de
    /// l'écart entre 40 et l'humeur ; elle reste entre la base du niveau et 100.
    /// Au-delà de 80, la maison produit un criminel, qui pille une maison luxueuse ou,
    /// à défaut, le trésor (Olympe), et sa criminalité retombe à la base.
    fn update_crime(&mut self) {
        let relieved = self.has_sanctuary(DIONYSUS);
        let mut criminals = Vec::new();
        for building in self.buildings.iter_mut().flatten() {
            let (x, y) = (building.x, building.y);
            let Some(house) = building.house.as_mut().filter(|h| h.level < ELITE_FIRST_LEVEL) else { continue };
            if house.population == 0 {
                house.crime = 0;
                continue;
            }
            let Some(model) = self.house_levels.get(house.level) else { continue };
            if self.grown {
                house.crime += if relieved { -DIONYSUS_CRIME_RELIEF } else { model.crime + (CRIME_NEUTRAL_SENTIMENT - self.sentiment) / 2 };
            }
            house.crime = house.crime.max(model.crime_base).min(100);
            if house.crime > CRIMINAL_CRIME {
                house.crime = model.crime_base;
                criminals.push((x, y, house.population));
            }
        }
        for (x, y, population) in criminals {
            let victim = self.buildings.iter_mut().flatten().filter_map(|b| b.house.as_mut().map(|h| (b.x, b.y, h))).find(|(.., h)| h.level > ELITE_DEVOLVED && h.goods.iter().any(|&g| g > 0));
            if let Some((vx, vy, house)) = victim {
                house.goods = [0; GOODS.len()];
                self.incidents.push(Incident::Pillage { x: vx, y: vy });
                continue;
            }
            let amount = (population * THEFT_PER_PERSON).min(self.money.max(0));
            if amount > 0 {
                self.money -= amount;
                self.incidents.push(Incident::Theft { amount, x, y });
            }
        }
    }

    /// Impôts et salaires du mois écoulé.
    fn settle_month(&mut self) {
        let now = self.ticks;
        let mut taxes = 0;
        for building in self.buildings.iter().flatten() {
            let Some(house) = &building.house else { continue };
            if house.has(Service::Tax, now) {
                let multiplier = self.house_levels.get(house.level).map_or(1, |level| level.tax_multiplier);
                taxes += house.population * TAX_PER_100_PEOPLE * multiplier;
            }
        }
        // Taux choisi, rapporté au taux normal du jeu.
        let rate = |level: usize| self.taxes.levels.get(level).map(|l| l.0);
        let taxes = match (rate(self.tax_level), rate(NORMAL_TAX_LEVEL)) {
            (Some(chosen), Some(normal)) if normal > 0 => taxes * chosen / normal / 100,
            _ => taxes / 100,
        };
        self.update_sentiment();
        let employed: i32 = self.buildings.iter().flatten().map(|b| b.workers).sum();
        let wages = employed * WAGES_PER_100_WORKERS * (NORMAL_WAGE + self.wage_shift).max(0) / (100 * NORMAL_WAGE);
        self.money += taxes - wages;
        let (sections, closed) = self.hippodrome();
        if closed {
            self.money += sections * RACETRACK_INCOME;
        }
        self.last_month = (taxes, wages);
        self.call_heroes();
        self.trade();
        self.release_monsters();
        self.place_heroes();
        self.send_gods();
        self.change_world();
        // Le bilan qui vient d'être fait est celui de décembre : l'année est close.
        self.year_profit += taxes - wages;
        if (self.ticks / TICKS_PER_MONTH) % 12 == 0 {
            self.best_year_profit = Some(self.best_year_profit.map_or(self.year_profit, |best| best.max(self.year_profit)));
            self.year_profit = 0;
            self.plagues = (0, self.plagues.0);
            self.natural_growth();
            for (best, year) in self.best_produced.iter_mut().zip(&mut self.produced) {
                *best = (*best).max(std::mem::take(year));
            }
            // Visite annuelle des dieux favorables qui ont un sanctuaire achevé.
            for god in self.sanctuaries() {
                self.bless(god);
            }
            // Tribut annuel des vassales et des colonies fondées : le montant réglé
            // dans l'éditeur, ou à défaut le barème d'Olympe.
            for city in 0..self.cities.len() {
                let c = &self.cities[city];
                let colony = c.kind == COLONY_KIND && c.status == crate::map::COLONY && c.tribute_paid > 0;
                if c.active && (c.status == VASSAL || colony) {
                    let tribute = if c.tribute_paid > 0 { c.tribute_paid } else { TRIBUTE_BASE + TRIBUTE_PER_SHIELD * c.military as i32 };
                    self.money += tribute;
                    self.tributes.push((city, tribute));
                }
            }
        }
    }

    fn next_random(&mut self) -> u32 {
        // xorshift32
        self.random ^= self.random << 13;
        self.random ^= self.random >> 17;
        self.random ^= self.random << 5;
        self.random
    }

    /// Rôle d'un bâtiment dans les échanges de marchandises : un poste de commerce y
    /// est un entrepôt.
    fn function(&self, id: usize) -> Function {
        self.buildings[id].as_ref().map_or(Function::None, |b| storage(self.kinds[b.kind].function))
    }

    /// Case de route touchant l'emprise d'un bâtiment.
    fn road_next_to(&self, id: usize) -> Option<(u16, u16)> {
        let building = self.buildings[id].as_ref()?;
        let kind = &self.kinds[building.kind];
        let (x, y, size, depth) = (building.x as i32, building.y as i32, kind.size as i32, kind.depth as i32);
        let along = (0..size).flat_map(|i| [(x + i, y - 1), (x + i, y + depth)]);
        let across = (0..depth).flat_map(|i| [(x - 1, y + i), (x + size, y + i)]);
        along.chain(across).find(|&(tx, ty)| self.is_road(tx, ty)).map(|(tx, ty)| (tx as u16, ty as u16))
    }

    /// Bâtiments dont l'emprise touche la case.
    fn buildings_next_to(&self, at: (u16, u16)) -> Vec<u16> {
        let mut found = Vec::new();
        for (dx, dy) in NEIGHBOURS {
            let (x, y) = (at.0 as i32 + dx, at.1 as i32 + dy);
            if !Self::in_grid(x, y) {
                continue;
            }
            if let Cell::Building(id) = self.cell(x as usize, y as usize) {
                if !found.contains(&id) {
                    found.push(id);
                }
            }
        }
        found
    }

    /// Activité de chaque bâtiment : pousse, livraisons, achats, tournées.
    fn run_buildings(&mut self) {
        let fishing = if self.has_sanctuary(POSEIDON) { FISHING_TICKS * POSEIDON_FISHING_PERCENT / 100 } else { FISHING_TICKS };
        let ship_ticks = if self.has_sanctuary(ATLAS) { TRIREME_TICKS / 2 } else { TRIREME_TICKS };
        let fish = self.map.fish_points();
        // Disciplines dont une école est en activité.
        let palace = self.buildings.iter().flatten().any(|b| self.kinds[b.kind].function == Function::Palace);
        let mut taught = [false; SERVICES.len()];
        for building in self.buildings.iter().flatten().filter(|b| b.staffed) {
            if let Function::School(service) = self.kinds[building.kind].function {
                taught[service as usize] = true;
            }
        }
        for id in 0..self.buildings.len() {
            let Some(building) = &self.buildings[id] else { continue };
            let kind = &self.kinds[building.kind];
            let (function, walker) = (kind.function, kind.walker);
            let (stock, errand_out, due) = (building.stock, building.errand_out, building.next_walk <= self.ticks);
            // En sous-effectif, le bâtiment perd la part des pas qui revient aux postes
            // vacants : production et marcheurs y attendent (comme la progression du jeu,
            // proportionnelle aux employés, `Zeus.exe` 0x4854d8).
            let (wanted, present) = (kind.employees.max(0) as u64, building.workers.max(0) as u64);
            let idle_tick = present < wanted && {
                let vacant = wanted - present;
                (self.ticks * vacant) / wanted != (self.ticks.saturating_sub(1) * vacant) / wanted
            };
            if !building.staffed || idle_tick {
                let walking = building.next_walk != WALKER_OUT && building.next_walk > self.ticks;
                let building = self.buildings[id].as_mut().unwrap();
                // À l'arrêt : la pousse des champs est suspendue elle aussi.
                if matches!(function, Function::Farm | Function::Supplier(_) | Function::Producer(_) | Function::Fishery | Function::Workshop(_) | Function::Wharf) {
                    building.sown_at += 1;
                }
                if idle_tick && walking {
                    building.next_walk += 1;
                }
                continue;
            }
            match function {
                Function::Service(service) if due => {
                    // Philosophes et acteurs sortent d'une école ; les athlètes s'entraînent sur place.
                    let needs_school = matches!(service, Service::Philosophy | Service::Drama);
                    // Les percepteurs dépendent du palais.
                    let allowed = service != Service::Tax || palace;
                    if allowed && (!needs_school || taught[service as usize]) {
                        self.start_rounds(id, walker, Rounds::Service(service));
                    }
                }
                // Quai sans navire : ses porteurs lui apportent bois et armement ; le
                // chantier commence quand les deux sont réunis et le navire est lancé au
                // bout de 32 jours (16 avec Atlas). Il reste à quai, compté dans la flotte.
                Function::Wharf if stock == 0 => {
                    let goods = building.goods;
                    let building = self.buildings[id].as_mut().unwrap();
                    if goods[Good::Wood as usize] < TRIREME_WOOD || goods[Good::Arms as usize] < TRIREME_ARMS {
                        building.sown_at = self.ticks;
                    } else if self.ticks - building.sown_at >= ship_ticks {
                        building.goods[Good::Wood as usize] = 0;
                        building.goods[Good::Arms as usize] = 0;
                        building.stock = 1;
                    }
                }
                Function::Producer(_) | Function::Fishery => {
                    let good = if let Function::Producer(good) = function { good } else { Good::Food };
                    // Pêcherie, laiterie et pavillon de chasse : rythmes mesurés dans le
                    // jeu ; autres producteurs : ceux de `Zeus.exe`.
                    let (needed, loads, cart) = match function {
                        Function::Fishery => {
                            // Distance au banc le plus proche, en cases (le bateau va en diagonale).
                            let (x, y) = (building.x as i64, building.y as i64);
                            let near = fish.iter().map(|&(fx, fy)| (fx as i64 - x).abs().max((fy as i64 - y).abs()) as u64).min();
                            let trip = near.map_or(FISHERY_TICKS, |d| 2 * d * BOAT_DAYS * TICKS_PER_DAY / BOAT_TILES + fishing + UNLOADING_TICKS);
                            (trip, FOOD_CART_LOADS, FOOD_CART_LOADS)
                        }
                        _ if good == Good::Food => (DAIRY_TICKS, FOOD_CART_LOADS, FOOD_CART_LOADS),
                        _ => (PRODUCTION_TICKS, 1, GOODS_CART),
                    };
                    if self.ticks - building.sown_at >= needed {
                        let building = self.buildings[id].as_mut().unwrap();
                        building.sown_at = self.ticks;
                        let before = building.stock;
                        building.stock = (building.stock + loads).min(PRODUCER_STORAGE.max(loads));
                        let (kind, made) = (building.kind, building.stock - before);
                        self.count_production(kind, made);
                    }
                    if stock >= cart && !errand_out {
                        self.send_harvest(id, function);
                    }
                }
                Function::Workshop(Good::Chariot) => {
                    let ready = building.goods[Good::Wood as usize] >= CHARIOT_WOOD && building.goods[Good::Horses as usize] >= CHARIOT_HORSES;
                    if !ready || building.stock >= PRODUCER_STORAGE {
                        self.buildings[id].as_mut().unwrap().sown_at = self.ticks;
                    } else if self.ticks - building.sown_at >= WORKSHOP_TICKS {
                        let building = self.buildings[id].as_mut().unwrap();
                        building.sown_at = self.ticks;
                        building.goods[Good::Wood as usize] -= CHARIOT_WOOD;
                        building.goods[Good::Horses as usize] -= CHARIOT_HORSES;
                        building.stock += 1;
                        let kind = building.kind;
                        self.count_production(kind, 1);
                    }
                    if stock >= GOODS_CART && !errand_out {
                        self.send_harvest(id, function);
                    }
                }
                Function::Workshop(_) => {
                    let raw = if function == Function::Workshop(Good::Sculpture) { SCULPTURE_BRONZE } else { 1 };
                    if building.raw < raw || building.stock >= PRODUCER_STORAGE {
                        self.buildings[id].as_mut().unwrap().sown_at = self.ticks;
                    } else if self.ticks - building.sown_at >= WORKSHOP_TICKS {
                        let building = self.buildings[id].as_mut().unwrap();
                        building.sown_at = self.ticks;
                        building.raw -= raw;
                        building.stock += 1;
                        let kind = building.kind;
                        self.count_production(kind, 1);
                    }
                    if stock >= GOODS_CART && !errand_out {
                        self.send_harvest(id, function);
                    }
                }
                Function::Farm | Function::Supplier(_) => {
                    // Récolte : le champ repart de zéro, la ferme garde les chargements.
                    if self.ticks - building.sown_at >= HARVEST_TICKS {
                        let building = self.buildings[id].as_mut().unwrap();
                        building.sown_at = self.ticks;
                        let before = building.stock;
                        building.stock = (building.stock + HARVEST_LOADS).min(FARM_STORAGE);
                        let (kind, made) = (building.kind, building.stock - before);
                        self.count_production(kind, made);
                    }
                    if stock > 0 && !errand_out {
                        self.send_harvest(id, function);
                    }
                }
                Function::Vendor(good) => {
                    if stock <= VENDOR_STORAGE - VENDOR_LOAD && !errand_out {
                        self.send_buyer(id, good);
                    }
                    if stock > 0 && due {
                        let loads = stock;
                        let carried = loads * units_per_load(good);
                        if self.start_rounds(id, walker, Rounds::Goods { good, carried, vendor: id as u16 }) {
                            self.buildings[id].as_mut().unwrap().stock -= loads;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Fait partir un marcheur en tournée depuis la route qui longe le bâtiment.
    fn start_rounds(&mut self, id: usize, image_base: usize, rounds: Rounds) -> bool {
        let mut walks = 0;
        if let Some(building) = self.buildings[id].as_mut() {
            building.next_walk = self.ticks + ROUNDS_INTERVAL;
            building.walks = building.walks.wrapping_add(1);
            walks = building.walks;
        }
        let Some(from) = self.road_next_to(id) else { return false };
        // Les tournées partent tour à tour dans chacune des directions où la route
        // continue : un bâtiment au bord d'une rue droite envoie son marcheur une fois
        // d'un côté, une fois de l'autre.
        let open: Vec<u8> = (0..NEIGHBOURS.len() as u8)
            .filter(|&h| {
                let (dx, dy) = NEIGHBOURS[h as usize];
                self.is_road(from.0 as i32 + dx, from.1 as i32 + dy) && !self.is_roadblock(from.0 as i32 + dx, from.1 as i32 + dy)
            })
            .collect();
        let heading = if open.is_empty() { 0 } else { open[walks as usize % open.len()] };
        let Some(next) = self.next_road_step(from, None, heading) else { return false };
        self.figures.push(Figure {
            image_base,
            x: from.0 as f32,
            y: from.1 as f32,
            direction: direction(from, next),
            frame: 0,
            path: vec![next],
            from,
            came_from: None,
            role: Role::Rounds { rounds, steps_left: rounds_length(&rounds), heading, home: id as u16 },
            source: Source::Figures,
        });
        if let Some(building) = self.buildings[id].as_mut() {
            building.next_walk = WALKER_OUT;
        }
        true
    }

    /// Envoie un porteur livrer la production au plus proche preneur qui a de la place :
    /// un grenier pour une ferme, l'atelier correspondant pour un pavillon, un entrepôt
    /// pour une marchandise finie.
    fn send_harvest(&mut self, farm: usize, function: Function) {
        let Some(start) = self.road_next_to(farm) else { return };
        let wanted = |to_storehouse: bool| {
            move |world: &World, id: u16| {
                let Some(building) = world.buildings[id as usize].as_ref() else { return false };
                match (function, storage(world.kinds[building.kind].function)) {
                    (Function::Farm | Function::Producer(Good::Food) | Function::Fishery, Function::Granary) => building.stock < GRANARY_STORAGE,
                    (Function::Supplier(raw), Function::Workshop(made)) => raw == raw_for(made) && building.raw < WORKSHOP_RAW_STORAGE,
                    // Une baie entamée de la même marchandise, ou une baie libre.
                    (_, Function::Storehouse) if to_storehouse => stored_good(function).is_some_and(|good| {
                        // Une marchandise n'occupe jamais plus de la moitié de l'entrepôt.
                        let loads = building.goods[good as usize];
                        loads < BAY_LOADS * (STOREHOUSE_BAYS as i32 / 2)
                            && (loads % BAY_LOADS != 0 || bays_used(&building.goods) < STOREHOUSE_BAYS)
                    }),
                    _ => false,
                }
            }
        };
        // Le bronze va d'abord à l'armurerie, et à l'entrepôt seulement faute de preneur.
        let found = match function {
            Function::Supplier(Raw::Bronze) => self.road_path(start, wanted(false)).or_else(|| self.road_path(start, wanted(true))),
            _ => self.road_path(start, wanted(true)),
        };
        let Some((granary, path)) = found else { return };
        let building = self.buildings[farm].as_mut().unwrap();
        // Pêcherie, laiterie et pavillon de chasse chargent une charrette de 100 unités du jeu.
        let load = building.stock.min(if matches!(function, Function::Fishery | Function::Producer(Good::Food)) { FOOD_CART_LOADS } else { CARRIER_LOAD });
        building.stock -= load;
        building.errand_out = true;
        let role = Role::Deliver { to: granary, load, from: farm as u16 };
        self.spawn_on_path(self.walkers.carrier, path, role);
    }

    /// Envoie une acheteuse se fournir au plus proche bâtiment qui a la marchandise en stock.
    fn send_buyer(&mut self, vendor: usize, good: Good) {
        let Some(start) = self.road_next_to(vendor) else { return };
        let wanted = |world: &World, id: u16| {
            let supplies = match world.function(id as usize) {
                Function::Granary => good == Good::Food,
                Function::Producer(made) | Function::Workshop(made) => made == good,
                Function::Storehouse => return world.buildings[id as usize].as_ref().is_some_and(|b| b.goods[good as usize] > 0),
                _ => false,
            };
            supplies && world.buildings[id as usize].as_ref().is_some_and(|b| b.stock > 0)
        };
        let Some((source, path)) = self.road_path(start, wanted) else { return };
        self.buildings[vendor].as_mut().unwrap().errand_out = true;
        self.spawn_on_path(self.walkers.buyer, path, Role::Fetch { source, vendor: vendor as u16 });
    }

    /// Crée un personnage au début d'un chemin (rangé à l'envers : départ en dernier).
    fn spawn_on_path(&mut self, image_base: usize, mut path: Vec<(u16, u16)>, role: Role) {
        let Some(start) = path.pop() else { return };
        let direction = path.last().map_or(0, |&next| direction(start, next));
        self.figures.push(Figure {
            image_base,
            x: start.0 as f32,
            y: start.1 as f32,
            direction,
            frame: 0,
            path,
            from: start,
            came_from: None,
            role,
            source: Source::Figures,
        });
    }

    /// Plus court chemin par les routes depuis `start` jusqu'à une case qui touche un
    /// bâtiment accepté par `wanted`. Rendu à l'envers : le départ est en dernier.
    fn road_path(&self, start: (u16, u16), wanted: impl Fn(&World, u16) -> bool) -> Option<(u16, Vec<(u16, u16)>)> {
        const UNSEEN: u32 = u32::MAX;
        let index = |at: (u16, u16)| at.1 as usize * GRID + at.0 as usize;
        let mut previous = vec![UNSEEN; GRID * GRID];
        previous[index(start)] = index(start) as u32;
        let mut queue = VecDeque::from([start]);
        while let Some(at) = queue.pop_front() {
            if let Some(target) = self.buildings_next_to(at).into_iter().find(|&id| wanted(self, id)) {
                let mut path = Vec::new();
                let mut cursor = index(at);
                loop {
                    path.push(((cursor % GRID) as u16, (cursor / GRID) as u16));
                    let before = previous[cursor] as usize;
                    if before == cursor {
                        break;
                    }
                    cursor = before;
                }
                return Some((target, path));
            }
            for (dx, dy) in NEIGHBOURS {
                let (nx, ny) = (at.0 as i32 + dx, at.1 as i32 + dy);
                if self.is_road(nx, ny) && previous[ny as usize * GRID + nx as usize] == UNSEEN {
                    previous[ny as usize * GRID + nx as usize] = index(at) as u32;
                    queue.push_back((nx as u16, ny as u16));
                }
            }
        }
        None
    }

    /// Prochaine case d'une tournée : vers le cap du marcheur si une route y mène,
    /// sinon tout droit, sinon au hasard ; demi-tour seulement dans une impasse.
    fn next_road_step(&mut self, at: (u16, u16), came_from: Option<(u16, u16)>, heading: u8) -> Option<(u16, u16)> {
        let (x, y) = (at.0 as i32, at.1 as i32);
        let road_towards = |world: &World, (dx, dy): (i32, i32)| {
            (world.is_road(x + dx, y + dy) && !world.is_roadblock(x + dx, y + dy)).then(|| ((x + dx) as u16, (y + dy) as u16))
        };
        let roads: Vec<(u16, u16)> = NEIGHBOURS.iter().filter_map(|&step| road_towards(self, step)).collect();
        let ahead: Vec<(u16, u16)> = roads.iter().copied().filter(|&road| Some(road) != came_from).collect();
        if ahead.is_empty() {
            return roads.first().copied();
        }
        if let Some(road) = road_towards(self, NEIGHBOURS[heading as usize % NEIGHBOURS.len()]).filter(|road| ahead.contains(road)) {
            return Some(road);
        }
        if let Some(back) = came_from {
            let straight = ((2 * x - back.0 as i32) as u16, (2 * y - back.1 as i32) as u16);
            if ahead.contains(&straight) {
                return Some(straight);
            }
        }
        let pick = self.next_random() as usize % ahead.len();
        Some(ahead[pick])
    }

    /// Passage d'un marcheur en tournée : il sert les maisons proches. Rend ce qu'il
    /// lui reste à distribuer.
    fn serve_houses(&mut self, at: (u16, u16), rounds: Rounds) -> Rounds {
        let until = self.ticks + SERVICE_DURATION;
        let (x, y) = (at.0 as i32, at.1 as i32);
        let mut seen = Vec::new();
        for ty in y - SERVICE_RANGE..=y + SERVICE_RANGE {
            for tx in x - SERVICE_RANGE..=x + SERVICE_RANGE {
                if !Self::in_grid(tx, ty) {
                    continue;
                }
                if let Cell::Building(id) = self.cell(tx as usize, ty as usize) {
                    if !seen.contains(&id) {
                        seen.push(id);
                    }
                }
            }
        }
        let mut rounds = rounds;
        for id in seen {
            // La maintenance remet en état tout bâtiment, pas seulement les maisons.
            if let (Rounds::Service(Service::Maintenance), Some(building)) = (&rounds, self.buildings[id as usize].as_mut()) {
                building.fire = 0;
                building.damage = 0;
            }
            let Some(house) = self.buildings[id as usize].as_mut().and_then(|b| b.house.as_mut()) else { continue };
            match &mut rounds {
                Rounds::Service(service) => {
                    house.covered_until[*service as usize] = until;
                    // Le garde chasse les voleurs.
                    if *service == Service::Security {
                        let base = self.house_levels.get(house.level).map_or(0, |l| l.crime_base);
                        house.crime = (house.crime - GUARD_CRIME_REDUCTION).max(base).min(100);
                    }
                }
                Rounds::Goods { good, carried, .. } => {
                    // La maison prend de quoi tenir quelques mois.
                    let reserve = match good {
                        Good::Food => HOUSE_FOOD_MONTHS * house.population.max(MIN_HOUSEHOLD),
                        // Seuls les logements luxueux prennent du vin, des armes et des chevaux.
                        Good::Wine | Good::Arms | Good::Horses | Good::Chariot if house.level < ELITE_FIRST_LEVEL => 0,
                        _ => HOUSE_GOODS_RESERVE,
                    };
                    let given = (reserve - house.goods[*good as usize]).clamp(0, *carried);
                    house.goods[*good as usize] += given;
                    *carried -= given;
                }
            }
        }
        rounds
    }

    fn move_figures(&mut self) {
        let carts_hasten = self.has_sanctuary(HERMES);
        // Personnages qui viennent d'atteindre une case, et marcheurs qui rentrent d'une
        // tournée.
        let (mut reached, mut homeward) = (Vec::new(), Vec::new());
        for (index, figure) in self.figures.iter_mut().enumerate() {
            figure.frame += 1;
            let Some(&(tx, ty)) = figure.path.last() else {
                reached.push(index);
                continue;
            };
            let carrier = matches!(figure.role, Role::Deliver { .. } | Role::Supply { .. });
            let tile_ticks = if carts_hasten && carrier { WALK_TICKS * 100 / HERMES_SPEED_PERCENT } else { WALK_TICKS };
            let progress = (figure.frame as f32 / tile_ticks as f32).min(1.0);
            let (fx, fy) = (figure.from.0 as f32, figure.from.1 as f32);
            figure.x = fx + (tx as f32 - fx) * progress;
            figure.y = fy + (ty as f32 - fy) * progress;
            if figure.frame >= tile_ticks {
                figure.frame = 0;
                figure.came_from = Some(figure.from);
                figure.from = (tx, ty);
                figure.path.pop();
                if matches!(figure.role, Role::Return { .. }) {
                    homeward.push(index);
                }
                match figure.path.last() {
                    Some(&next) => figure.direction = direction(figure.from, next),
                    None => reached.push(index),
                }
            }
        }
        // Sur le chemin du retour, le marcheur dessert encore ce qu'il longe : sans cela,
        // un grenier à mi-parcours d'une tournée peut attendre la maintenance plus de
        // deux mois et s'effondrer.
        for index in homeward {
            let Role::Return { home, good, carried } = self.figures[index].role else { continue };
            let rounds = match self.buildings[home as usize].as_ref().map(|b| self.kinds[b.kind].function) {
                Some(Function::Service(service)) => Rounds::Service(service),
                Some(Function::Vendor(_)) => Rounds::Goods { good, carried, vendor: home },
                _ => continue,
            };
            if let Rounds::Goods { carried, .. } = self.serve_houses(self.figures[index].from, rounds) {
                self.figures[index].role = Role::Return { home, good, carried };
            }
        }

        let mut finished = Vec::new();
        for index in reached {
            let (at, came_from, role) = (self.figures[index].from, self.figures[index].came_from, self.figures[index].role);
            // Un chemin non vide : simple étape, rien à faire.
            if !self.figures[index].path.is_empty() {
                continue;
            }
            match role {
                // Les soldats sont menés par `fight` ; le navire attend.
                Role::Soldier { .. } | Role::Ship => {}
                // Héros arrivé au bout de son chemin : il repart vers le monstre qu'il
                // doit combattre, ou rentre à son temple.
                Role::Hero { hero, hall } => {
                    let goal = self.hero_goal(hero, hall);
                    if let Some(goal) = goal.filter(|&g| g != at) {
                        let path = straight_path(at, goal);
                        let figure = &mut self.figures[index];
                        if let Some(&next) = path.last() {
                            figure.direction = direction(at, next);
                        }
                        figure.path = path;
                    }
                }
                // Monstre arrivé : il détruit sa cible, se repose, puis en choisit une autre.
                Role::Monster { monster, target, rest_until } => {
                    let (target, rest_until) = self.rampage(index, at, target, rest_until, MONSTER_REST_TICKS);
                    self.figures[index].role = Role::Monster { monster, target, rest_until };
                }
                // Dieu en visite : il fait quelques pas autour de son sanctuaire.
                Role::Visit { leaves_at, .. } => {
                    if self.ticks >= leaves_at {
                        finished.push(index);
                    } else if self.next_random() % 40 == 0 {
                        let (dx, dy) = ((self.next_random() % 7) as i32 - 3, (self.next_random() % 7) as i32 - 3);
                        let goal = ((at.0 as i32 + dx).clamp(0, GRID as i32 - 1) as u16, (at.1 as i32 + dy).clamp(0, GRID as i32 - 1) as u16);
                        let path = straight_path(at, goal);
                        let figure = &mut self.figures[index];
                        if let Some(&next) = path.last() {
                            figure.direction = direction(at, next);
                        }
                        figure.path = path;
                    }
                }
                // Dieu hostile : il ravage de même, plus vite, jusqu'à son départ.
                Role::God { god, target, rest_until, leaves_at } => {
                    if self.ticks >= leaves_at {
                        self.god_news.push((god, false));
                        finished.push(index);
                    } else {
                        let (target, rest_until) = self.rampage(index, at, target, rest_until, GOD_REST_TICKS);
                        self.figures[index].role = Role::God { god, target, rest_until, leaves_at };
                    }
                }
                Role::Immigrant { home, people } => {
                    if let Some(house) = self.buildings.get_mut(home as usize).and_then(|b| b.as_mut()).and_then(|b| b.house.as_mut()) {
                        house.arriving -= people;
                        house.population += people;
                    }
                    finished.push(index);
                }
                Role::Rounds { rounds, steps_left, heading, home } => {
                    let rounds = self.serve_houses(at, rounds);
                    let next = if steps_left > 0 { self.next_road_step(at, came_from, heading) } else { None };
                    match next {
                        Some(next) => {
                            let figure = &mut self.figures[index];
                            figure.direction = direction(at, next);
                            figure.path.push(next);
                            figure.role = Role::Rounds { rounds, steps_left: steps_left - 1, heading, home };
                        }
                        None => {
                            // Fin de tournée : retour à pied, avec ce qui n'a pas été distribué.
                            let (good, carried) = match rounds {
                                Rounds::Goods { good, carried, .. } => (good, carried),
                                Rounds::Service(_) => (Good::Food, 0),
                            };
                            let mut way = self.road_path(at, |_, id| id == home).map(|(_, path)| path).unwrap_or_default();
                            way.pop();
                            let figure = &mut self.figures[index];
                            figure.role = Role::Return { home, good, carried };
                            if let Some(&next) = way.last() {
                                figure.direction = direction(at, next);
                            }
                            figure.path = way;
                            // Déjà devant sa porte, ou plus de chemin : il rentre aussitôt.
                            if figure.path.is_empty() {
                                self.come_home(home, good, carried);
                                finished.push(index);
                            }
                        }
                    }
                }
                Role::Return { home, good, carried } => {
                    self.come_home(home, good, carried);
                    finished.push(index);
                }
                Role::Supply { to, good, load } => {
                    if let Some(building) = self.buildings[to as usize].as_mut() {
                        building.goods[good as usize] += load;
                    }
                    finished.push(index);
                }
                Role::Deliver { to, load, from } => {
                    let raw = matches!(self.function(to as usize), Function::Workshop(_)) && from != to;
                    // Dans un entrepôt, la marchandise est celle que fabrique l'expéditeur.
                    let stored = match self.function(to as usize) {
                        Function::Storehouse => stored_good(self.function(from as usize)),
                        _ => None,
                    };
                    // Au grenier, les vivres sont rangés selon ce que produit l'expéditeur.
                    let granary = self.function(to as usize) == Function::Granary;
                    let food = self.buildings[from as usize].as_ref().map_or(0, |b| self.kinds[b.kind].product as usize);
                    if let Some(building) = self.buildings[to as usize].as_mut() {
                        // Un atelier reçoit de la matière première, les autres du stock.
                        match stored {
                            Some(good) => building.goods[good as usize] += load,
                            None if raw => building.raw += load,
                            None => building.stock += load,
                        }
                        if granary {
                            building.foods[if food < FOODS { food } else { 0 }] += load;
                        }
                    }
                    if let Some(building) = self.buildings[from as usize].as_mut() {
                        building.errand_out = false;
                    }
                    finished.push(index);
                }
                Role::Fetch { source, vendor } => {
                    // Chez le fournisseur : l'acheteuse charge puis rentre par le même chemin.
                    // Dans un entrepôt, elle prend la marchandise que vend son échoppe.
                    let wanted = match (self.function(source as usize), self.function(vendor as usize)) {
                        (Function::Storehouse, Function::Vendor(good)) => Some(good as usize),
                        _ => None,
                    };
                    let mut load = 0;
                    if let Some(building) = self.buildings[source as usize].as_mut() {
                        let shelf = match wanted {
                            Some(good) => &mut building.goods[good],
                            None => &mut building.stock,
                        };
                        load = (*shelf).min(VENDOR_LOAD);
                        *shelf -= load;
                        // Au grenier, l'acheteuse prend d'abord les vivres les plus abondants.
                        let mut left = load;
                        while left > 0 && wanted.is_none() {
                            let Some(most) = (0..FOODS).max_by_key(|&f| building.foods[f]).filter(|&f| building.foods[f] > 0) else { break };
                            building.foods[most] -= 1;
                            left -= 1;
                        }
                    }
                    let back = self.road_path(at, |_, id| id == vendor);
                    match back {
                        Some((_, mut path)) => {
                            path.pop();
                            let figure = &mut self.figures[index];
                            figure.role = Role::Deliver { to: vendor, load, from: vendor };
                            match path.last() {
                                Some(&next) => {
                                    figure.direction = direction(at, next);
                                    figure.path = path;
                                }
                                // Grenier et échoppe sur la même case de route : livraison immédiate.
                                None => figure.path.clear(),
                            }
                        }
                        None => {
                            if let Some(building) = self.buildings[vendor as usize].as_mut() {
                                building.errand_out = false;
                            }
                            finished.push(index);
                        }
                    }
                }
            }
        }
        finished.sort_unstable();
        finished.dedup();
        for index in finished.into_iter().rev() {
            self.figures.swap_remove(index);
        }
        for (target, cause) in std::mem::take(&mut self.ravaged) {
            let Some(building) = self.buildings.get(target as usize).and_then(|b| b.as_ref()) else { continue };
            let (kind, x, y) = (building.kind, building.x, building.y);
            self.remove_building(target, Cell::Rubble(0));
            self.disasters.push(Disaster { cause, kind, x, y });
        }
    }

    /// Retour d'un marcheur : son bâtiment pourra en renvoyer un après un temps de
    /// repos. Un marchand rend à son échoppe ce qu'il n'a pas distribué ; un chargement
    /// entamé est perdu.
    fn come_home(&mut self, home: u16, good: Good, carried: i32) {
        let rest_until = self.ticks + ROUNDS_INTERVAL;
        if let Some(building) = self.buildings[home as usize].as_mut() {
            building.stock += carried / units_per_load(good);
            building.next_walk = rest_until;
        }
    }

    /// Chaque mois : les maisons consomment, puis évoluent ou régressent.
    fn update_houses(&mut self) {
        let now = self.ticks;
        let goods_month = (now / TICKS_PER_MONTH) % GOODS_MONTHS == 0;
        // Le stade profite à toute la cité dès qu'il a son personnel (lecture provisoire
        // de l'exigence « stade » des règles).
        let stadium_open = self.stadium_open();
        let apollo = self.has_sanctuary(APOLLO);
        let appeal_relief = if self.has_sanctuary(APHRODITE) { APHRODITE_APPEAL } else { 0 };
        for id in 0..self.buildings.len() {
            let Some(building) = self.buildings[id].as_ref() else { continue };
            let appeal = self.appeal(building.x as i32, building.y as i32) + appeal_relief;
            let Some(house) = self.buildings[id].as_mut().and_then(|b| b.house.as_mut()) else { continue };
            // Une maison vide attend ses habitants ; un terrain de luxe, lui, attend
            // d'abord d'être desservi pour les attirer.
            if house.population == 0 && house.level < ELITE_FIRST_LEVEL {
                continue;
            }
            let needed = house.population;
            // Ce que la maison avait en réserve avant de consommer.
            let had = house.goods.map(|amount| amount > 0);
            let fed = house.goods[Good::Food as usize] >= needed.max(1);
            // Les niveaux qui ne demandent aucun vivre ne mangent pas (`Zeus.exe`, 0x48b8d0).
            house.ate = fed && self.house_levels.get(house.level).is_some_and(|l| l.food_types > 0);
            house.goods[Good::Food as usize] = (house.goods[Good::Food as usize] - needed).max(0);
            // Seules s'usent les marchandises que le niveau exige (colonnes 9 à 11).
            if let Some(level) = self.house_levels.get(house.level).filter(|_| goods_month) {
                for (good, used) in [(Good::Fleece, level.fleece), (Good::Oil, level.olive_oil), (Good::Wine, level.wine)] {
                    house.goods[good as usize] = (house.goods[good as usize] - used.max(0)).max(0);
                }
            }
            let state = HouseState { mount: if self.atlantean { Good::Chariot } else { Good::Horses }, fed, had, water: house.has(Service::Water, now), culture: culture_of(house, now, stadium_open, self.atlantean, apollo), stadium_open };
            let meets = |level: &HouseModel| state.missing(level).is_empty();
            let Some(current) = self.house_levels.get(house.level) else { continue };
            let (floor, next) = house_steps(house.level);
            let next_ok = next.and_then(|next| self.house_levels.get(next)).is_some_and(meets);
            if (!meets(current) || appeal < current.devolve_desirability) && house.level > floor {
                house.level -= 1;
                // Les habitants en trop quittent la cité.
                let capacity = self.house_levels[house.level].capacity;
                house.population = house.population.min(capacity);
            } else if next_ok && appeal >= current.evolve_desirability {
                house.level = next.unwrap_or(house.level);
            }
        }
    }

    /// Migrations du jour (`Zeus.exe`, 0x567450 et 0x486190).
    fn migrate(&mut self) {
        let rate = MIGRATION_RATES.iter().find(|(from, _)| self.sentiment >= *from).map_or(MIGRATION_LOW_RATE, |&(_, rate)| rate);
        let population = self.population();
        let in_debt = self.months_in_debt as i32 / 12 > MAX_DEBT_YEARS;
        if rate == 0 || population >= MAX_POPULATION || (rate > 0 && in_debt) {
            return;
        }
        let count = (rate.abs() * MIGRANTS_PER_DAY + 99) / 100;
        let (arrivals, departures) = &mut self.migration_pause;
        let (pause, other) = if rate > 0 { (arrivals, departures) } else { (departures, arrivals) };
        if *pause > 0 {
            *pause -= 1;
            return;
        }
        if rate < 0 && population <= MIN_EMIGRATION_POPULATION {
            return;
        }
        *other = MIGRATION_PAUSE;
        // Moins de quatre migrants attendent les suivants pour faire un groupe.
        let waiting = if rate > 0 { &mut self.migrants.0 } else { &mut self.migrants.1 };
        let count = if count >= IMMIGRANT_GROUP {
            count
        } else if *waiting + count >= IMMIGRANT_GROUP {
            std::mem::take(waiting) + count
        } else {
            *waiting += count;
            return;
        };
        if rate > 0 { self.send_immigrants(count) } else { self.send_away(count) }
    }

    /// Répartit les arrivants entre les maisons qui n'en attendent pas déjà : quatre
    /// dans chaque maison vide, puis dans celles qui ont huit places, puis de quoi
    /// remplir les autres. Ceux qui ne trouvent pas de place ne viennent pas.
    fn send_immigrants(&mut self, mut count: i32) {
        for pass in 0..3 {
            for id in 0..self.buildings.len() {
                if count == 0 {
                    return;
                }
                let Some(building) = &self.buildings[id] else { continue };
                let Some(house) = &building.house else { continue };
                let room = self.capacity(house) - house.population - house.arriving;
                let wanted = match pass {
                    0 => house.population == 0,
                    1 => room >= ROOMY_HOUSE,
                    _ => true,
                };
                if room <= 0 || house.arriving > 0 || !wanted {
                    continue;
                }
                let size = self.kinds[building.kind].size as i32;
                let (x, y) = (building.x as i32, building.y as i32);
                if !self.has_road_access(x, y, size) {
                    continue;
                }
                let Some(path) = self.path_from_edge(x, y, size) else { continue };
                let people = count.min(if pass < 2 { IMMIGRANT_GROUP } else { room });
                let image = if house.level >= ELITE_FIRST_LEVEL { self.walkers.elite_immigrant } else { self.walkers.immigrant };
                self.spawn_on_path(image, path, Role::Immigrant { home: id as u16, people });
                if let Some(house) = self.buildings[id].as_mut().and_then(|b| b.house.as_mut()) {
                    house.arriving += people;
                }
                count -= people;
            }
        }
    }

    /// Fait partir des habitants, quatre au plus par maison, en commençant par les
    /// logements les plus modestes (`Zeus.exe`, 0x486ab0) ; personne tant que la cité a
    /// le sanctuaire d'Aphrodite.
    fn send_away(&mut self, mut count: i32) {
        if self.has_sanctuary(APHRODITE) {
            return;
        }
        let levels = self.house_levels.len();
        for level in 0..levels {
            for house in self.buildings.iter_mut().flatten().filter_map(|b| b.house.as_mut()) {
                if count == 0 {
                    return;
                }
                if house.level == level && house.population > 0 {
                    let leaving = house.population.min(IMMIGRANT_GROUP).min(count);
                    house.population -= leaving;
                    count -= leaving;
                }
            }
        }
    }

    fn has_road_access(&self, x: i32, y: i32, size: i32) -> bool {
        let range = ROAD_ACCESS_RANGE;
        (y - range..y + size + range).any(|ty| (x - range..x + size + range).any(|tx| self.is_road(tx, ty)))
    }

    /// Plus court chemin à pied jusqu'à une case voisine de l'emprise donnée, depuis
    /// le point d'entrée de la carte ou, à défaut, depuis le bord. Rendu à l'envers :
    /// le départ est en dernier.
    fn path_from_edge(&self, x: i32, y: i32, size: i32) -> Option<Vec<(u16, u16)>> {
        const UNSEEN: u32 = u32::MAX;
        let entry = self.map.entry_points().into_iter().find(|&(ex, ey)| self.is_walkable(ex as i32, ey as i32));
        let mut fallback = None;
        let mut previous = vec![UNSEEN; GRID * GRID];
        let mut queue = VecDeque::new();
        // On part de la maison et on cherche l'entrée : le chemin sort déjà dans le bon sens.
        for ty in y - 1..=y + size {
            for tx in x - 1..=x + size {
                let outside = tx < x || tx >= x + size || ty < y || ty >= y + size;
                if outside && self.is_walkable(tx, ty) {
                    previous[ty as usize * GRID + tx as usize] = (ty as usize * GRID + tx as usize) as u32;
                    queue.push_back((tx, ty));
                }
            }
        }
        // Remonte de la case d'arrivée vers la maison, puis retourne : le départ finit en dernier.
        let trace = |previous: &[u32], (cx, cy): (i32, i32)| {
            let mut path = Vec::new();
            let mut at = cy as usize * GRID + cx as usize;
            loop {
                path.push(((at % GRID) as u16, (at / GRID) as u16));
                let before = previous[at] as usize;
                if before == at {
                    break;
                }
                at = before;
            }
            path.reverse();
            path
        };
        while let Some((cx, cy)) = queue.pop_front() {
            match entry {
                Some((ex, ey)) if (cx, cy) == (ex as i32, ey as i32) => return Some(trace(&previous, (cx, cy))),
                // Sans entrée connue, la première case de bord fait l'affaire ; avec, elle
                // ne sert que si l'entrée se révèle inaccessible.
                _ if fallback.is_none() && NEIGHBOURS.iter().any(|(dx, dy)| !self.in_map(cx + dx, cy + dy)) => {
                    if entry.is_none() {
                        return Some(trace(&previous, (cx, cy)));
                    }
                    fallback = Some((cx, cy));
                }
                _ => {}
            }
            for (dx, dy) in DIRECTIONS {
                let (nx, ny) = (cx + dx, cy + dy);
                // Pas de passage en diagonale entre deux obstacles.
                let clear = self.is_walkable(nx, ny)
                    && (dx == 0 || dy == 0 || (self.is_walkable(cx + dx, cy) && self.is_walkable(cx, cy + dy)));
                if clear && previous[ny as usize * GRID + nx as usize] == UNSEEN {
                    previous[ny as usize * GRID + nx as usize] = (cy as usize * GRID + cx as usize) as u32;
                    queue.push_back((nx, ny));
                }
            }
        }
        fallback.map(|at| trace(&previous, at))
    }
}

/// Image d'une baie d'entrepôt : les marchandises remplissent les baies dans l'ordre
/// de `GOODS`, chacune commençant une nouvelle baie.
fn bay_image(goods: &[i32; GOODS.len()], bay: usize) -> (Source, usize) {
    let mut first = 0;
    for (good, &loads) in goods.iter().enumerate() {
        let bays = ((loads + BAY_LOADS - 1) / BAY_LOADS) as usize;
        if bay < first + bays {
            let inside = if GOODS[good] == Good::Sculpture { 1 } else { (loads - (bay - first) as i32 * BAY_LOADS).clamp(1, BAY_LOADS) };
            let (source, image) = BAY_FIRST_IMAGES[good];
            return (source, image + inside as usize - 1);
        }
        first += bays;
    }
    (Source::General, EMPTY_BAY_IMAGE)
}

/// Fonction d'un bâtiment pour le stockage : postes de commerce et docks rangent leurs
/// marchandises comme un entrepôt.
fn storage(function: Function) -> Function {
    match function {
        Function::TradePost { .. } => Function::Storehouse,
        other => other,
    }
}

/// Marchandise qu'un bâtiment de cette fonction range à l'entrepôt.
fn stored_good(function: Function) -> Option<Good> {
    match function {
        Function::Producer(good) | Function::Workshop(good) if !matches!(good, Good::Horses | Good::Food) => Some(good),
        Function::Supplier(Raw::Bronze) => Some(Good::Bronze),
        _ => None,
    }
}

/// Baies qu'occupe le contenu d'un entrepôt.
fn bays_used(goods: &[i32; GOODS.len()]) -> usize {
    goods.iter().map(|&loads| ((loads + BAY_LOADS - 1) / BAY_LOADS) as usize).sum()
}

/// Parts que contient un chargement une fois chez l'habitant. La nourriture se
/// compte en rations (une par habitant et par mois), le reste en chargements entiers.
fn units_per_load(good: Good) -> i32 {
    match good {
        Good::Food => PEOPLE_PER_LOAD,
        _ => 1,
    }
}

/// Ce qui manque à une maison pour passer au niveau suivant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Need {
    Food,
    Water,
    /// Laine, huile, vin, armement ou chevaux.
    Goods(Good),
    Stadium,
    /// Note de culture exigée.
    Culture(i32),
    /// Attrait exigé.
    Appeal(i32),
    /// Plus haut niveau atteint.
    Top,
}

/// Ce dont dispose une maison au regard des exigences des niveaux.
struct HouseState {
    /// Monture qu'exige la colonne « chevaux » des règles : des chars dans une cité
    /// atlante.
    mount: Good,
    fed: bool,
    had: [bool; GOODS.len()],
    water: bool,
    culture: i32,
    stadium_open: bool,
}

impl HouseState {
    /// Exigences d'un niveau qui ne sont pas remplies.
    fn missing(&self, level: &HouseModel) -> Vec<Need> {
        [
            (level.food_types > 0 && !self.fed, Need::Food),
            (level.water > 0 && !self.water, Need::Water),
            (level.stadium > 0 && !self.stadium_open, Need::Stadium),
            (self.culture < level.culture, Need::Culture(level.culture)),
            (level.fleece > 0 && !self.had[Good::Fleece as usize], Need::Goods(Good::Fleece)),
            (level.olive_oil > 0 && !self.had[Good::Oil as usize], Need::Goods(Good::Oil)),
            (level.wine > 0 && !self.had[Good::Wine as usize], Need::Goods(Good::Wine)),
            (level.arms > 0 && !self.had[Good::Arms as usize], Need::Goods(Good::Arms)),
            (level.horses > 0 && !self.had[self.mount as usize], Need::Goods(self.mount)),
        ]
        .into_iter()
        .filter_map(|(missing, need)| missing.then_some(need))
        .collect()
    }
}

/// Note de culture d'une maison, comparée aux exigences des niveaux (colonne 2 des
/// règles). Relevée sur les maisons des sauvegardes des tutoriels du jeu : 15 pour la
/// philosophie, 20 pour l'athlétisme, 25 pour le théâtre ; l'athlétisme vaut 30 de
/// plus quand la cité a un stade. Les autres combinaisons s'en déduisent (35, 60, 75,
/// 90 relevés).
const CULTURE_POINTS: [(Service, i32); 3] = [(Service::Philosophy, 15), (Service::Athletics, 20), (Service::Drama, 25)];
/// Science des cités atlantes, mesurée dans Poséidon (compteurs de passage écrits dans
/// la mémoire du jeu, note lue en +0x80) : astronomie (université, observatoire) 20,
/// apprentissage (bibliothèque) 15, recherche (atelier d'inventeurs, laboratoire) 25.
/// Olympe range ces disciplines sous philosophie, athlétisme et théâtre. Le musée n'a
/// pas pu être mesuré : il compte comme le stade (hypothèse).
const SCIENCE_POINTS: [(Service, i32); 3] = [(Service::Philosophy, 20), (Service::Athletics, 15), (Service::Drama, 25)];
const STADIUM_ATHLETICS_POINTS: i32 = 30;

fn culture_of(house: &House, now: u64, stadium_open: bool, atlantean: bool, apollo: bool) -> i32 {
    let table = if atlantean { &SCIENCE_POINTS } else { &CULTURE_POINTS };
    // Apollon ajoute 20 à chaque note que la maison reçoit, comme `Zeus.exe` 0x565d40.
    let bonus = if apollo { APOLLO_NOTE } else { 0 };
    let points: i32 = table.iter().filter(|&&(service, _)| house.has(service, now)).map(|&(_, points)| points + bonus).sum();
    let stadium = if stadium_open && house.has(Service::Athletics, now) { STADIUM_ATHLETICS_POINTS } else { 0 };
    points + stadium
}

/// Niveau le plus bas où une maison peut retomber, et niveau suivant s'il y en a un. Un
/// logement modeste va de 0 à 6. Un logement luxueux part du terrain vacant, saute
/// l'état délabré en montant et ne retombe pas plus bas que lui.
fn house_steps(level: usize) -> (usize, Option<usize>) {
    let elite = level >= ELITE_FIRST_LEVEL;
    let (floor, top) = if elite { (ELITE_DEVOLVED, ELITE_FIRST_LEVEL + ELITE_LEVELS - 1) } else { (0, HOUSE_IMAGES.len() - 1) };
    let next = if level == ELITE_FIRST_LEVEL { level + 2 } else { level + 1 };
    (floor, (next <= top).then_some(next))
}

/// Direction de marche (rang dans `DIRECTIONS`) pour aller d'une case à sa voisine.
fn direction(from: (u16, u16), to: (u16, u16)) -> usize {
    let step = ((to.0 as i32 - from.0 as i32).signum(), (to.1 as i32 - from.1 as i32).signum());
    DIRECTIONS.iter().position(|&d| d == step).unwrap_or(0)
}

/// En-tête des fichiers de sauvegarde ; le numéro change quand le format change.
const SAVE_HEADER: &str = "zeus-sauvegarde 14";
/// Versions précédentes, encore lues, et nombre de marchandises qu'elles connaissaient
/// (les plus récentes s'ajoutent à la fin de `GOODS`).
const OLD_SAVE_HEADERS: [(&str, usize); 5] = [("zeus-sauvegarde 9", 6), ("zeus-sauvegarde 10", 7), ("zeus-sauvegarde 11", 9), ("zeus-sauvegarde 12", 12), ("zeus-sauvegarde 13", 12)];

impl World {
    /// État de la partie sous forme de texte : une ligne par route, bâtiment ou
    /// personnage. La carte elle-même n'est pas enregistrée.
    pub fn save(&self) -> String {
        use std::fmt::Write;
        let mut out = format!("{SAVE_HEADER}\n");
        let _ = writeln!(
            out,
            "etat {} {} {} {} {} {} {} {} {}",
            self.ticks,
            self.money,
            0,
            self.random,
            self.last_month.0,
            self.last_month.1,
            self.kinds.len(),
            self.year_profit,
            self.best_year_profit.map_or("-".to_string(), |best| best.to_string())
        );
        let list = |values: &[i32; RESOURCES]| values.iter().map(i32::to_string).collect::<Vec<_>>().join(" ");
        let _ = writeln!(out, "production {}", list(&self.produced));
        let _ = writeln!(out, "meilleure {}", list(&self.best_produced));
        let _ = writeln!(out, "reserve {}", list(&self.set_aside));
        let _ = writeln!(out, "emporte {}", list(&self.carried));
        let _ = writeln!(out, "prix {}", list(&self.price_shift));
        let _ = writeln!(out, "salaires {}", self.wage_shift);
        let policies: Vec<String> = self.trade_policy.iter().map(|p| TradePolicy::ALL.iter().position(|q| q == p).unwrap_or(0).to_string()).collect();
        let _ = writeln!(out, "commerce {}", policies.join(" "));
        let monuments: Vec<String> = self.commemoratives.iter().map(|m| m.to_string()).collect();
        let _ = writeln!(out, "commemoratifs {} {}", self.milestones, monuments.join(" "));
        let running = self.games_running.map_or("-".to_string(), |(kind, home, place)| format!("{kind} {} {place}", home as u8));
        let _ = writeln!(out, "jeux {} {running}", self.games_host as u8);
        let _ = writeln!(out, "humeur {} {} {} {}", self.tax_level, self.sentiment, self.grown as u8, self.months_in_debt);
        let _ = writeln!(out, "migrants {} {} {} {}", self.migrants.0, self.migrants.1, self.migration_pause.0, self.migration_pause.1);
        let _ = writeln!(out, "sante {} {} {} {} {}", self.health, self.health_target, self.plagues.0, self.plagues.1, self.plague_month as u8);
        for &(city, until) in &self.armies_away {
            let _ = writeln!(out, "absente {city} {until}");
        }
        let present: Vec<String> = self.heroes.iter().map(|&h| (h as u8).to_string()).collect();
        let _ = writeln!(out, "heros {}", present.join(" "));
        // Statut de chaque cité, plus 10 quand une route commerciale est ouverte.
        let cities: Vec<String> =
            std::iter::once(self.campaign_until.to_string()).chain(self.cities.iter().map(|c| (c.status + 10 * c.trade as i32).to_string())).collect();
        let _ = writeln!(out, "cites {}", cities.join(" "));
        let strengths: Vec<String> = self.cities.iter().map(|c| c.strength.to_string()).collect();
        let _ = writeln!(out, "defenses {}", strengths.join(" "));
        for &(city, resource, mark) in &self.plundered {
            let _ = writeln!(out, "pillage {city} {resource} {mark}");
        }
        if let Some(e) = self.expedition {
            let [rabble, hoplites, horsemen] = e.soldiers;
            let won = e.won.map_or(-1, |w| w as i32);
            let _ = writeln!(out, "campagne {} {} {rabble} {hoplites} {horsemen} {} {} {won} {} {} {}", e.city, e.raid as u8, e.arrives, e.returns, e.loot.0, e.loot.1, e.triremes);
        }
        for s in &self.scheduled {
            let _ = writeln!(out, "changement {} {} {} {} {} {} {}", s.at, s.change, s.cities.0, s.cities.1, s.amount.0, s.amount.1, s.resource);
        }
        for &(at, god) in &self.invasions {
            let _ = writeln!(out, "invasion {at} {god}");
        }
        if let Some(r) = self.raid {
            let _ = writeln!(out, "raid {} {} {} {}", r.city, r.soldiers, r.buildings, r.limit);
        }
        for r in &self.requests {
            let _ = writeln!(out, "demande {} {} {} {}", r.city, r.resource, r.amount, r.until);
        }
        let military: Vec<String> = self.cities.iter().map(|c| c.military.to_string()).collect();
        if !military.is_empty() {
            let _ = writeln!(out, "forces {}", military.join(" "));
            let favor: Vec<String> = self.cities.iter().map(|c| c.favor.to_string()).collect();
            let _ = writeln!(out, "faveurs {}", favor.join(" "));
            let traded: Vec<String> = self.cities.iter().map(|c| format!("{} {}", c.sold, c.bought)).collect();
            let _ = writeln!(out, "echanges {}", traded.join(" "));
        }
        for q in &self.quests {
            let (state, until) = match q.state {
                QuestState::Waiting => (0, 0),
                QuestState::Away { until } => (1, until),
                QuestState::Done => (2, 0),
            };
            let _ = writeln!(out, "quete {} {} {} {} {state} {until}", q.event, q.god, q.quest, q.hero);
        }
        for m in &self.monsters {
            let _ = writeln!(out, "monstre {} {} {} {} {}", m.event, m.monster, m.fight_until.map_or(-1, |t| t as i64), m.slain as u8, m.arrives_at);
        }
        for (index, cell) in self.cells.iter().enumerate() {
            let _ = match *cell {
                Cell::Road => writeln!(out, "route {} {}", index % GRID, index / GRID),
                Cell::AgoraRoad => writeln!(out, "dallage {} {}", index % GRID, index / GRID),
                Cell::Wall(health) => writeln!(out, "mur {} {} {health}", index % GRID, index / GRID),
                Cell::Rubble(variant) => writeln!(out, "ruine {} {} {variant}", index % GRID, index / GRID),
                Cell::Chasm(variant) => writeln!(out, "gouffre {} {} {variant}", index % GRID, index / GRID),
                Cell::Lava(variant) => writeln!(out, "lave {} {} {variant}", index % GRID, index / GRID),
                Cell::Flood => {
                    let until = self.floods.iter().find(|&&(x, y, _)| y as usize * GRID + x as usize == index).map_or(0, |f| f.2);
                    writeln!(out, "inondation {} {} {until}", index % GRID, index / GRID)
                }
                _ => Ok(()),
            };
        }
        for &(x, y, until) in &self.fires {
            if let Cell::Fire(variant) = self.cell(x as usize, y as usize) {
                let _ = writeln!(out, "feu {x} {y} {variant} {until}");
            }
        }
        for building in &self.buildings {
            let Some(b) = building else {
                out.push_str("vide\n");
                continue;
            };
            let _ = write!(
                out,
                "batiment {} {} {} {} {} {} {} {} {} {} {} {}",
                b.kind, b.x, b.y, b.stock, b.next_walk, b.sown_at, b.errand_out as u8, b.workers, b.raw, b.walks, b.fire, b.damage
            );
            let _ = write!(
                out,
                " {}",
                b.goods.iter().map(i32::to_string).collect::<Vec<_>>().join(" ")
            );
            if let Some(h) = &b.house {
                let covered: Vec<String> = h.covered_until.iter().map(u64::to_string).collect();
                let goods: Vec<String> = h.goods.iter().map(i32::to_string).collect();
                let _ = write!(out, " maison {} {} {} {} {}", h.level, h.population, h.arriving, goods.join(" "), covered.join(" "));
            }
            if let Some(h) = b.house.as_ref().filter(|h| h.crime != 0 || h.ate || h.hungry != 0) {
                let _ = write!(out, " risques {} 0 {} {}", h.crime, h.ate as u8, h.hungry);
            }
            if b.foods.iter().any(|&f| f != 0) {
                let _ = write!(out, " vivres {}", b.foods.iter().map(i32::to_string).collect::<Vec<_>>().join(" "));
            }
            out.push('\n');
        }
        for f in &self.figures {
            let role = match f.role {
                Role::Immigrant { home, people } => format!("immigrant {home} {people} 0 0 0"),
                Role::Rounds { rounds: Rounds::Service(service), steps_left, heading, home } => {
                    format!("service {} {steps_left} {heading} {home} 0", service as usize)
                }
                Role::Return { home, good, carried } => format!("retour {home} {} {carried} 0 0", good as usize),
                Role::Rounds { rounds: Rounds::Goods { good, carried, vendor }, steps_left, heading, .. } => {
                    format!("marchand {carried} {vendor} {steps_left} {} {heading}", good as usize)
                }
                Role::Deliver { to, load, from } => format!("livraison {to} {load} {from} 0 0"),
                Role::Supply { to, good, load } => format!("chantier {to} {} {load} 0 0", good as usize),
                Role::Fetch { source, vendor } => format!("achat {source} {vendor} 0 0 0"),
                Role::Monster { monster, target, rest_until } => format!("bete {monster} {target} {rest_until} 0 0"),
                Role::Hero { hero, hall } => format!("heros {hero} {hall} 0 0 0"),
                Role::God { god, target, rest_until, leaves_at } => format!("dieu {god} {target} {rest_until} {leaves_at} 0"),
                Role::Visit { god, leaves_at } => format!("visite {god} {leaves_at} 0 0 0"),
                Role::Soldier { enemy, army, health, target, fighting } => format!("soldat {} {health} {target} {} {army}", enemy as u8, fighting as u8),
                Role::Ship => "navire 0 0 0 0 0".to_string(),
            };
            let came = f.came_from.map_or("- -".to_string(), |(x, y)| format!("{x} {y}"));
            let path: Vec<String> = f.path.iter().map(|(x, y)| format!("{x} {y}")).collect();
            let _ = writeln!(
                out,
                "personnage {} {} {} {} {} {} {came} {role} {}",
                f.image_base, f.from.0, f.from.1, f.direction, f.frame, f.path.len(), path.join(" ")
            );
        }
        out
    }

    /// Remplace l'état de la partie par celui d'une sauvegarde faite sur la même carte.
    pub fn load(&mut self, text: &str) -> Result<(), String> {
        let mut lines = text.lines();
        let header = lines.clone().next();
        let counts_workers = matches!(header, Some(SAVE_HEADER | "zeus-sauvegarde 13"));
        let goods_len = match lines.next() {
            Some(SAVE_HEADER) => GOODS.len(),
            Some(header) => OLD_SAVE_HEADERS.iter().find(|(old, _)| *old == header).map(|&(_, len)| len).ok_or("ce n'est pas une sauvegarde de cette version")?,
            None => return Err("sauvegarde vide".into()),
        };
        let mut cells = vec![Cell::Terrain; GRID * GRID];
        let (mut buildings, mut figures, mut fires) = (Vec::new(), Vec::new(), Vec::new());
        let mut state = None;
        let mut profits = (0, None);
        // Compteurs des objectifs : absents des sauvegardes plus anciennes.
        let mut counters = [[0; RESOURCES]; 5];
        // Héros, quêtes et monstres : absents des sauvegardes plus anciennes.
        let (mut heroes, mut quests, mut monsters) = ([false; crate::heroes::HEROES], Vec::new(), Vec::new());
        // Statuts des cités du monde : sans eux, ceux de la carte restent.
        let (mut campaign, mut statuses): (u64, Vec<i32>) = (0, Vec::new());
        let (mut scheduled, mut military): (Vec<Scheduled>, Vec<u8>) = (Vec::new(), Vec::new());
        let mut requests = Vec::new();
        let mut invasions = Vec::new();
        let (mut wage_shift, mut away, mut floods) = (0, Vec::new(), Vec::new());
        let mut raid = None;
        let mut mood = None;
        let mut migration = None;
        let mut health = None;
        let mut favors: Vec<i32> = Vec::new();
        let mut traded: Vec<i32> = Vec::new();
        // Réglages du commerce : absents des sauvegardes plus anciennes.
        let mut trade_policy = [TradePolicy::Both; RESOURCES];
        let mut commemoratives = (0, Vec::new());
        let mut games = (false, None);
        let (mut strengths, mut plundered, mut expedition): (Vec<i32>, Vec<(usize, u8, i32)>, Option<Expedition>) = (Vec::new(), Vec::new(), None);
        for (number, line) in lines.enumerate() {
            let bad = || format!("sauvegarde illisible à la ligne {}", number + 2);
            let mut words = line.split(' ');
            let kind = words.next().unwrap_or("");
            // Tous les champs restants sont des nombres, sauf les mots-clés repérés à part.
            let rest: Vec<&str> = words.collect();
            let number_at = |i: usize| rest.get(i).and_then(|w| w.parse::<i64>().ok()).ok_or_else(bad);
            match kind {
                "etat" => {
                    // Les types ajoutés depuis (en fin de liste) ne changent pas les numéros.
                    if number_at(6)? as usize > self.kinds.len() {
                        return Err("sauvegarde faite avec une autre liste de bâtiments".into());
                    }
                    state = Some((number_at(0)?, number_at(1)?, number_at(2)?, number_at(3)?, number_at(4)?, number_at(5)?));
                    // Bénéfices annuels : absents des sauvegardes plus anciennes.
                    profits = (rest.get(7).and_then(|w| w.parse().ok()).unwrap_or(0), rest.get(8).and_then(|w| w.parse().ok()));
                }
                "jeux" => {
                    let running = (rest.get(1) != Some(&"-")).then(|| Ok::<_, String>((number_at(1)? as u8, number_at(2)? != 0, number_at(3)? as u8))).transpose()?;
                    games = (number_at(0)? != 0, running);
                }
                "commemoratifs" => {
                    let numbers: Vec<u8> = rest.iter().filter(|w| !w.is_empty()).map(|w| w.parse().map_err(|_| bad())).collect::<Result<_, _>>()?;
                    commemoratives = (*numbers.first().ok_or_else(bad)?, numbers[1..].to_vec());
                }
                "commerce" => {
                    for (i, policy) in trade_policy.iter_mut().enumerate() {
                        *policy = *TradePolicy::ALL.get(number_at(i)? as usize).ok_or_else(bad)?;
                    }
                }
                "production" | "meilleure" | "reserve" | "emporte" | "prix" => {
                    let row = ["production", "meilleure", "reserve", "emporte", "prix"].iter().position(|&k| k == kind).unwrap_or(0);
                    for (i, slot) in counters[row].iter_mut().enumerate() {
                        *slot = number_at(i)? as i32;
                    }
                }
                "defenses" => strengths = rest.iter().filter(|w| !w.is_empty()).map(|w| w.parse().map_err(|_| bad())).collect::<Result<_, _>>()?,
                "pillage" => plundered.push((number_at(0)? as usize, number_at(1)? as u8, number_at(2)? as i32)),
                "campagne" => {
                    let won = number_at(7)?;
                    expedition = Some(Expedition {
                        city: number_at(0)? as usize,
                        raid: number_at(1)? != 0,
                        soldiers: [number_at(2)? as i32, number_at(3)? as i32, number_at(4)? as i32],
                        arrives: number_at(5)? as u64,
                        returns: number_at(6)? as u64,
                        won: (won >= 0).then_some(won != 0),
                        loot: (number_at(8)? as u8, number_at(9)? as i32),
                        triremes: number_at(10).unwrap_or(0) as i32,
                    });
                }
                "cites" => {
                    campaign = number_at(0)? as u64;
                    statuses = (1..rest.len()).map(|i| number_at(i).map(|s| s as i32)).collect::<Result<_, _>>()?;
                }
                "changement" => scheduled.push(Scheduled {
                    at: number_at(0)? as u64,
                    change: number_at(1)? as u8,
                    cities: (number_at(2)? as i16, number_at(3)? as i16),
                    amount: (number_at(4)? as i16, number_at(5)? as i16),
                    resource: rest.get(6).and_then(|w| w.parse().ok()).unwrap_or(0),
                }),
                "invasion" => invasions.push((number_at(0)? as u64, number_at(1)? as u8)),
                "salaires" => wage_shift = number_at(0)? as i32,
                "sante" => health = Some((number_at(0)? as i32, number_at(1)? as i32, (number_at(2)? as u32, number_at(3)? as u32), number_at(4)? != 0)),
                "migrants" => migration = Some(((number_at(0)? as i32, number_at(1)? as i32), (number_at(2)? as u8, number_at(3)? as u8))),
                "humeur" => mood = Some((number_at(0)? as usize, number_at(1)? as i32, number_at(2).unwrap_or(0) != 0, number_at(3).unwrap_or(0) as u32)),
                "raid" => raid = Some(Raid { city: number_at(0)? as usize, soldiers: number_at(1)? as i32, buildings: number_at(2)? as i32, limit: number_at(3)? as i32 }),
                "absente" => away.push((number_at(0)? as usize, number_at(1)? as u64)),
                "demande" => requests.push(Request {
                    city: number_at(0)? as usize,
                    resource: number_at(1)? as u8,
                    amount: number_at(2)? as i32,
                    until: number_at(3)? as u64,
                }),
                "echanges" => traded = (0..rest.len()).map(|i| number_at(i).map(|n| n as i32)).collect::<Result<_, _>>()?,
                "faveurs" => favors = (0..rest.len()).map(|i| number_at(i).map(|f| f as i32)).collect::<Result<_, _>>()?,
                "forces" => military = (0..rest.len()).map(|i| number_at(i).map(|m| m as u8)).collect::<Result<_, _>>()?,
                "heros" => {
                    for (i, slot) in heroes.iter_mut().enumerate() {
                        *slot = number_at(i)? != 0;
                    }
                }
                "quete" => quests.push(Quest {
                    event: number_at(0)? as usize,
                    god: number_at(1)? as u8,
                    quest: number_at(2)? as u8,
                    hero: (number_at(3)? as u8).min(crate::heroes::HEROES as u8 - 1),
                    state: match number_at(4)? {
                        0 => QuestState::Waiting,
                        1 => QuestState::Away { until: number_at(5)? as u64 },
                        _ => QuestState::Done,
                    },
                }),
                "monstre" => monsters.push(Monster {
                    event: number_at(0)? as usize,
                    monster: number_at(1)? as u8,
                    fight_until: u64::try_from(number_at(2)?).ok(),
                    slain: number_at(3)? != 0,
                    // Absent des sauvegardes plus anciennes : déjà là.
                    arrives_at: rest.get(4).and_then(|w| w.parse().ok()).unwrap_or(0),
                }),
                "mur" => {
                    let (x, y) = (number_at(0)? as usize, number_at(1)? as usize);
                    *cells.get_mut(y * GRID + x).ok_or_else(bad)? = Cell::Wall(number_at(2)?.clamp(1, WALL_HEALTH as i64) as u8);
                }
                "route" | "dallage" => {
                    let (x, y) = (number_at(0)? as usize, number_at(1)? as usize);
                    *cells.get_mut(y * GRID + x).ok_or_else(bad)? = if kind == "route" { Cell::Road } else { Cell::AgoraRoad };
                }
                "gouffre" | "lave" | "inondation" => {
                    let (x, y, value) = (number_at(0)? as usize, number_at(1)? as usize, number_at(2)?);
                    *cells.get_mut(y * GRID + x).ok_or_else(bad)? = match kind {
                        "gouffre" => Cell::Chasm(value as u8),
                        "lave" => Cell::Lava(value as u8),
                        _ => Cell::Flood,
                    };
                    if kind == "inondation" && value > 0 {
                        floods.push((x as u16, y as u16, value as u64));
                    }
                }
                "ruine" | "feu" => {
                    let (x, y, variant) = (number_at(0)? as usize, number_at(1)? as usize, number_at(2)? as u8 % RUBBLE_IMAGES);
                    let cell = cells.get_mut(y * GRID + x).ok_or_else(bad)?;
                    if kind == "feu" {
                        *cell = Cell::Fire(variant);
                        fires.push((x as u16, y as u16, number_at(3)? as u64));
                    } else {
                        *cell = Cell::Rubble(variant);
                    }
                }
                "vide" => buildings.push(None),
                "batiment" => {
                    let (kind, x, y) = (number_at(0)? as usize, number_at(1)? as usize, number_at(2)? as usize);
                    let (size, depth) = self.kinds.get(kind).map(|k| (k.size, k.depth)).ok_or_else(bad)?;
                    if x + size > GRID || y + depth > GRID {
                        return Err(bad());
                    }
                    let house = match rest.get(12 + goods_len) {
                        Some(&"maison") => {
                            let mut goods = [0; GOODS.len()];
                            for (i, slot) in goods.iter_mut().take(goods_len).enumerate() {
                                *slot = number_at(16 + goods_len + i)? as i32;
                            }
                            let mut covered_until = [0; SERVICES.len()];
                            for (i, slot) in covered_until.iter_mut().enumerate() {
                                *slot = number_at(16 + 2 * goods_len + i)? as u64;
                            }
                            Some(House {
                                level: (number_at(13 + goods_len)? as usize).min(ELITE_FIRST_LEVEL + ELITE_LEVELS - 1),
                                population: number_at(14 + goods_len)? as i32,
                                arriving: number_at(15 + goods_len)? as i32,
                                goods,
                                covered_until,
                                crime: 0,
                                ate: false,
                                hungry: 0,
                            })
                        }
                        _ => None,
                    };
                    let mut house = house;
                    if let (Some(house), Some(at)) = (house.as_mut(), rest.iter().position(|&w| w == "risques")) {
                        house.crime = number_at(at + 1)? as i32;
                        (house.ate, house.hungry) = (number_at(at + 3).unwrap_or(0) != 0, number_at(at + 4).unwrap_or(0).min(3) as u8);
                    }
                    // Vivres du grenier par ressource : absents des sauvegardes plus
                    // anciennes, où tout le stock passe pour d'origine inconnue.
                    let stock = number_at(3)? as i32;
                    let mut foods = [0; FOODS];
                    match rest.iter().position(|&w| w == "vivres") {
                        Some(at) => {
                            for (i, slot) in foods.iter_mut().enumerate() {
                                *slot = number_at(at + 1 + i)? as i32;
                            }
                        }
                        None if self.kinds[kind].function == Function::Granary => foods[0] = stock,
                        None => {}
                    }
                    let id = buildings.len() as u16;
                    for dy in 0..depth {
                        for dx in 0..size {
                            cells[(y + dy) * GRID + x + dx] = Cell::Building(id);
                        }
                    }
                    buildings.push(Some(Building {
                        kind,
                        x,
                        y,
                        house,
                        stock,
                        foods,
                        raw: number_at(8)? as i32,
                        goods: {
                            let mut goods = [0; GOODS.len()];
                            for (i, slot) in goods.iter_mut().take(goods_len).enumerate() {
                                *slot = number_at(12 + i)? as i32;
                            }
                            goods
                        },
                        walks: number_at(9)? as u8,
                        fire: number_at(10)? as i32,
                        damage: number_at(11)? as i32,
                        next_walk: number_at(4)? as u64,
                        sown_at: number_at(5)? as u64,
                        errand_out: number_at(6)? != 0,
                        staffed: number_at(7)? != 0 || self.kinds.get(number_at(0)? as usize).is_some_and(|k| k.employees == 0),
                        // Les sauvegardes plus anciennes ne retiennent que « pourvu ou non ».
                        workers: match (counts_workers, number_at(7)? as i32) {
                            (true, workers) => workers,
                            (false, 0) => 0,
                            (false, _) => self.kinds.get(number_at(0)? as usize).map_or(0, |k| k.employees),
                        },
                    }));
                }
                "personnage" => {
                    let from = (number_at(1)? as u16, number_at(2)? as u16);
                    let length = number_at(5)? as usize;
                    let came_from = match rest.get(6) {
                        Some(&"-") => None,
                        _ => Some((number_at(6)? as u16, number_at(7)? as u16)),
                    };
                    let (a, b, c, d, e) = (number_at(9)?, number_at(10)?, number_at(11)?, number_at(12)?, number_at(13)?);
                    let role = match *rest.get(8).ok_or_else(bad)? {
                        "immigrant" => Role::Immigrant { home: a as u16, people: b as i32 },
                        "service" => Role::Rounds {
                            rounds: Rounds::Service(*SERVICES.get(a as usize).ok_or_else(bad)?),
                            steps_left: b as u32,
                            heading: c as u8 % NEIGHBOURS.len() as u8,
                            home: d as u16,
                        },
                        "retour" => Role::Return { home: a as u16, good: *GOODS.get(b as usize).ok_or_else(bad)?, carried: c as i32 },
                        "marchand" => Role::Rounds {
                            rounds: Rounds::Goods { good: *GOODS.get(d as usize).ok_or_else(bad)?, carried: a as i32, vendor: b as u16 },
                            steps_left: c as u32,
                            heading: e as u8 % NEIGHBOURS.len() as u8,
                            home: b as u16,
                        },
                        "livraison" => Role::Deliver { to: a as u16, load: b as i32, from: c as u16 },
                        "chantier" => Role::Supply { to: a as u16, good: *GOODS.get(b as usize).ok_or_else(bad)?, load: c as i32 },
                        "achat" => Role::Fetch { source: a as u16, vendor: b as u16 },
                        "bete" => Role::Monster { monster: a as u8, target: b as u16, rest_until: c as u64 },
                        "heros" => Role::Hero { hero: a as u8, hall: b as u16 },
                        "dieu" => Role::God { god: a as u8, target: b as u16, rest_until: c as u64, leaves_at: d as u64 },
                        "visite" => Role::Visit { god: a as u8, leaves_at: b as u64 },
                        "navire" => Role::Ship,
                        "soldat" => Role::Soldier { enemy: a != 0, army: e as u8, health: b as i32, target: c as u16, fighting: d != 0 },
                        _ => return Err(bad()),
                    };
                    let mut path = Vec::with_capacity(length);
                    for i in 0..length {
                        path.push((number_at(14 + 2 * i)? as u16, number_at(15 + 2 * i)? as u16));
                    }
                    figures.push(Figure {
                        image_base: number_at(0)? as usize,
                        x: from.0 as f32,
                        y: from.1 as f32,
                        direction: (number_at(3)? as usize).min(7),
                        frame: (number_at(4)? as usize).min(WALK_TICKS - 1),
                        path,
                        from,
                        came_from,
                        role,
                        source: match role {
                            Role::Monster { monster, .. } => Source::Monster(monster),
                            Role::Hero { hero, .. } => Source::Hero(hero),
                            Role::God { god, .. } | Role::Visit { god, .. } => Source::Deity(god),
                            Role::Soldier { enemy: true, army, .. } => Source::Army(army / 16),
                            _ => Source::Figures,
                        },
                    });
                }
                "" => {}
                _ => return Err(bad()),
            }
        }
        let (ticks, money, _, random, taxes, wages) = state.ok_or("sauvegarde sans ligne d'état")?;
        // Un personnage ne doit pas désigner un bâtiment absent.
        let exists = |id: u16| buildings.get(id as usize).is_some_and(|b: &Option<Building>| b.is_some());
        figures.retain(|f| match f.role {
            Role::Immigrant { home, .. } => exists(home),
            Role::Deliver { to, from, .. } => exists(to) && exists(from),
            Role::Supply { to, .. } => exists(to),
            Role::Fetch { source, vendor } => exists(source) && exists(vendor),
            Role::Rounds { home, .. } | Role::Return { home, .. } => exists(home),
            Role::Monster { .. } | Role::Soldier { .. } | Role::Ship => true,
            Role::Hero { hall, .. } => exists(hall),
            Role::God { .. } | Role::Visit { .. } => true,
        });
        self.cells = cells;
        self.fires = fires;
        (self.floods, self.disaster_news) = (floods, Vec::new());
        self.disasters.clear();
        self.incidents.clear();
        self.buildings = buildings;
        self.figures = figures;
        self.ticks = ticks as u64;
        self.money = money as i32;
        (self.migrants, self.migration_pause) = migration.unwrap_or_default();
        (self.health, self.health_target, self.plagues, self.plague_month) = health.unwrap_or((START_HEALTH, START_HEALTH, (0, 0), false));
        self.random = (random as u32).max(1);
        self.last_month = (taxes as i32, wages as i32);
        (self.year_profit, self.best_year_profit) = profits;
        [self.produced, self.best_produced, self.set_aside, self.carried, self.price_shift] = counters;
        self.trade_policy = trade_policy;
        (self.milestones, self.commemoratives) = commemoratives;
        (self.games_host, self.games_running) = games;
        for (city, strength) in self.cities.iter_mut().zip(&strengths) {
            city.strength = *strength;
        }
        (self.plundered, self.expedition) = (plundered, expedition);
        (self.heroes, self.quests, self.monsters) = (heroes, quests, monsters);
        self.campaign_until = campaign;
        self.scheduled = scheduled;
        (self.wage_shift, self.armies_away, self.wage_news) = (wage_shift, away, Vec::new());
        self.raid = raid;
        if let Some((level, sentiment, grown, months_in_debt)) = mood {
            self.set_tax_level(level);
            self.sentiment = sentiment.clamp(0, 100);
            (self.grown, self.months_in_debt) = (grown, months_in_debt);
        }
        self.world_news.clear();
        (self.requests, self.request_news) = (requests, Vec::new());
        (self.invasions, self.god_news) = (invasions, Vec::new());
        for (city, strength) in self.cities.iter_mut().zip(military) {
            city.military = strength;
        }
        for (city, favor) in self.cities.iter_mut().zip(favors) {
            city.favor = favor;
        }
        for (city, pair) in self.cities.iter_mut().zip(traded.chunks_exact(2)) {
            (city.sold, city.bought) = (pair[0], pair[1]);
        }
        for (city, status) in self.cities.iter_mut().zip(statuses) {
            (city.status, city.trade) = (status % 10, status >= 10);
        }
        self.hero_news.clear();
        self.rebuild_appeal();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apollo_adds_twenty_to_each_culture_note() {
        let mut house = House {
            level: 5,
            population: 10,
            arriving: 0,
            covered_until: [0; SERVICES.len()],
            goods: [0; GOODS.len()],
            crime: 0,
            ate: false,
            hungry: 0,
        };
        house.covered_until[Service::Philosophy as usize] = 100;
        assert_eq!(culture_of(&house, 10, false, false, false), 15);
        assert_eq!(culture_of(&house, 10, false, false, true), 35);
    }
}
