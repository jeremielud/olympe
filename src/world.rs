//! État de la cité et simulation : routes, bâtiments, logements, nourriture, personnages.
//!
//! Les rythmes et les quantités (durée d'un jour, vitesse de marche, rendement des
//! fermes, consommation…) sont provisoires : ils ne sont pas encore tirés du jeu
//! d'origine. L'échoppe est ici un bâtiment autonome, alors que le jeu la place
//! dans une agora.

use crate::map::{City, EDGE_DRAW_TILE, FOREIGN, GRID, Map, RIVAL, Source, VASSAL};
use crate::model::HouseModel;
use std::collections::VecDeque;

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

/// Première tuile de route dans le catalogue de terrain ; 18 variantes selon les bords.
const ROAD_FIRST_IMAGE: usize = 897;
/// Eau profonde, quand la carte n'en a pas à imiter.
const WATER_IMAGE: usize = 764;
/// Variante de route pour chaque combinaison de bords sans route voisine
/// (bit 0 = nord-est, 1 = sud-est, 2 = sud-ouest, 3 = nord-ouest).
const ROAD_VARIANT: [usize; 16] = [17, 14, 15, 6, 16, 1, 7, 11, 13, 5, 0, 10, 4, 9, 8, 12];
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
const TICKS_PER_DAY: u64 = 50;
const DAYS_PER_MONTH: u64 = 16;
pub const TICKS_PER_MONTH: u64 = TICKS_PER_DAY * DAYS_PER_MONTH;
/// Images de marche d'un personnage, par direction.
const WALK_FRAMES: usize = 12;
/// Pas de simulation pour parcourir une case : les personnages des sauvegardes
/// d'origine avancent de 0 à 14 sur leur case (octet 57 de leur enregistrement).
const WALK_TICKS: usize = 15;
/// Intervalle entre deux arrivées d'immigrants, en pas de simulation.
const IMMIGRATION_INTERVAL: u64 = 20;
/// Taille d'un groupe d'immigrants.
const IMMIGRANT_GROUP: i32 = 4;
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
/// Chargements qu'emporte une acheteuse, ou un marchand ambulant en tournée.
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

/// Pourquoi une conquête n'a pas lieu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Conquest {
    NotForeign,
    AlreadyRuled,
    Away,
    TooWeak { army: i32, defence: i32 },
}

const CONQUEST_PER_SHIELD: i32 = 24;
/// Tribut annuel d'une vassale : une part fixe et une part par bouclier (Olympe).
const TRIBUTE_BASE: i32 = 250;
const TRIBUTE_PER_SHIELD: i32 = 50;

/// Palier de demande ou d'offre d'une cité, en unités par an (les niveaux du jeu).
const TRADE_STEP: i32 = 12;

/// Réserve que le commerce garde ou cherche à atteindre, par marchandise (Olympe).
const TRADE_RESERVE: i32 = 8;

/// Prix d'une unité de marchandise, en drachmes (valeurs d'Olympe, provisoires : le
/// jeu a ses prix, que l'on n'a pas encore trouvés).
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

/// Prix d'une route commerciale (Olympe : le jeu fixe un prix par cité).
pub const TRADE_ROUTE_COST: i32 = 1000;
const CAMPAIGN_TICKS: u64 = TICKS_PER_MONTH * 6;

/// Monstre sans cible, et repos d'un monstre entre deux destructions (Olympe). Le
/// Kraken et Scylla, monstres marins, ne marchent pas dans la cité.
const NO_TARGET: u16 = u16::MAX;
const MONSTER_REST_TICKS: u64 = TICKS_PER_MONTH * 2;
const SEA_MONSTERS: [u8; 2] = [1, 10];
/// Un dieu hostile reste six mois et détruit un bâtiment par mois (valeurs d'Olympe).
const GOD_STAY_TICKS: u64 = TICKS_PER_MONTH * 6;
const GOD_REST_TICKS: u64 = TICKS_PER_MONTH;
/// Séjour d'un dieu hostile quand un dieu favorable protège la cité (Olympe).
const GOD_REPELLED_TICKS: u64 = TICKS_PER_MONTH;

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
/// Images des monuments dans `poseidon_loaded.sg3` : blocs de marbre blanc de un à
/// quatre niveaux (quinze pixels chacun), sommet de pyramide, obélisque (pour les
/// statues et édifices qu'Olympe ne dessine pas encore), dalle, sol du chantier et
/// aire d'accès.
const MONUMENT_BLOCK_IMAGE: usize = 654;
const MONUMENT_BLOCK_LEVELS: u8 = 4;
const MONUMENT_LEVEL_PIXELS: u16 = 15;
const MONUMENT_TOPPER_IMAGE: usize = 620;
const MONUMENT_STATUE_IMAGE: usize = 628;
const MONUMENT_TILE_IMAGE: usize = 658;
const MONUMENT_FLOOR_IMAGE: usize = 663;
const MONUMENT_PAD_IMAGE: usize = 664;
/// Images de `zeus_general.sg3` qu'un sanctuaire montre dès son chantier : dalles
/// décorées et dallage uni ; le reste (autel, statues) n'apparaît qu'une fois achevé.
const SANCTUARY_FLOOR_IMAGES: std::ops::RangeInclusive<usize> = 1447..=1458;
/// Dallage uni (six variantes) qui tient leur place pendant le chantier.
const SANCTUARY_PLAIN_IMAGE: usize = 1453;
/// Part de la population en âge de travailler, en pour cent.
/// Durée de fabrication d'un chargement (bergerie, pressoir).
const PRODUCTION_TICKS: u64 = TICKS_PER_MONTH / 8;
const PRODUCER_STORAGE: i32 = 8;
/// Durée de fabrication d'un chargement de fromage : deux par mois (provisoire).
const DAIRY_TICKS: u64 = TICKS_PER_MONTH / 2;
/// Durée de fonte d'une sculpture : deux par mois (provisoire).
const SCULPTURE_TICKS: u64 = TICKS_PER_MONTH / 2;
/// Matière première qu'un atelier accepte d'avance.
const WORKSHOP_RAW_STORAGE: i32 = 8;
/// Réserve de laine ou d'huile qu'une maison cherche à garder, et mois que dure un chargement.
const HOUSE_GOODS_RESERVE: i32 = 2;
const GOODS_MONTHS: u64 = 4;
/// L'attrait ne rayonne jamais plus loin, quelles que soient les règles.
const APPEAL_MAX_RANGE: i32 = 6;
const APPEAL_LIMIT: i32 = 100;
/// Risques d'incendie et d'effondrement. Les sauvegardes d'origine montrent deux
/// compteurs par bâtiment (octets 70 et 72) qui montent par pas égaux aux valeurs des
/// règles et atteignent 154 et 120 sans sinistre. Le seuil de 200 et le rythme (chaque
/// jour, une chance sur huit de monter d'un cran) sont ceux de Caesar III, dont le
/// moteur descend ; ils ne sont pas vérifiés dans Zeus.
const RISK_LIMIT: i32 = 200;
const RISK_DAILY_CHANCE: u32 = 8;
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
/// Une baie ne montre jamais qu'une sculpture.
const BAY_FIRST_IMAGES: [usize; GOODS.len()] = [837, 861, 877, 881, 869, EMPTY_BAY_IMAGE, 845, 841, 849, 873];
const WORKFORCE_PERCENT: i32 = 40;
/// Impôt mensuel, en drachmes pour 100 habitants d'une maison visitée par un
/// percepteur (multiplié par le coefficient du niveau de logement).
const TAX_PER_100_PEOPLE: i32 = 25;
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

pub const GOODS: [Good; 10] =
    [Good::Food, Good::Fleece, Good::Oil, Good::Wine, Good::Arms, Good::Horses, Good::Bronze, Good::Wood, Good::Marble, Good::Sculpture];

/// Ressources numérotées comme dans le jeu (groupe de textes 23, ressources et
/// objectifs des aventures) : 2 poisson, 7 blé, 14 laine, 20 huile d'olive…
pub const RESOURCES: usize = 24;
/// Les ressources 1 à 7 sont des vivres, rangés au grenier.
pub const FOODS: usize = 8;

/// Marchandise que les maisons consomment (les autres ne servent qu'à l'industrie
/// ou aux objectifs).
pub fn for_houses(good: Good) -> bool {
    !matches!(good, Good::Bronze | Good::Wood | Good::Marble | Good::Sculpture)
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
}

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
    /// Le bâtiment a reçu ses employés ce mois-ci ; sinon il est à l'arrêt.
    pub staffed: bool,
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
    /// Acheteuse en route vers un fournisseur pour le compte d'une échoppe.
    Fetch { source: u16, vendor: u16 },
    /// Monstre en marche vers le bâtiment `target`, qu'il détruira ; il se repose
    /// ensuite jusqu'à `rest_until`.
    Monster { monster: u8, target: u16, rest_until: u64 },
    /// Héros présent dans la cité, rattaché à son temple.
    Hero { hero: u8, hall: u16 },
    /// Dieu favorable en visite : il flâne près de son sanctuaire, puis repart.
    Visit { god: u8, leaves_at: u64 },
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

impl Figure {
    /// Image à dessiner : une rangée de 8 directions par étape de marche.
    pub fn image(&self) -> usize {
        self.image_base + self.direction + 8 * (self.frame * WALK_FRAMES / WALK_TICKS)
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
    Building(u16),
}

/// Sinistre qui a détruit un bâtiment.
#[derive(Clone, Copy, Debug)]
pub struct Disaster {
    /// Incendie, sinon effondrement.
    pub fire: bool,
    /// Type du bâtiment détruit.
    pub kind: usize,
    pub x: usize,
    pub y: usize,
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
    pub kinds: Vec<Kind>,
    pub buildings: Vec<Option<Building>>,
    pub figures: Vec<Figure>,
    pub money: i32,
    /// Pas de simulation écoulés depuis le début de la partie.
    pub ticks: u64,
    cells: Vec<Cell>,
    house_levels: Vec<HouseModel>,
    walkers: WalkerImages,
    /// Maison servie à la prochaine arrivée, pour répartir les immigrants.
    next_house: usize,
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
    /// Marchandises mises de côté par la cité précédente, que ses cadeaux apportent,
    /// et cadeaux reçus depuis la dernière lecture : (cité, ressource, quantité).
    pub carried: [i32; RESOURCES],
    pub gifts: Vec<(usize, u8, i32)>,
    /// Écart de chaque prix à son prix de base, et changements du commerce survenus
    /// depuis la dernière lecture : (cité, changement, ressource).
    pub price_shift: [i32; RESOURCES],
    pub trade_news: Vec<(usize, u8, u8)>,
    /// Écart du salaire au salaire normal (drachmes par an et par dix ouvriers), et
    /// armées rivales parties en campagne : (cité, retour).
    pub wage_shift: i32,
    pub armies_away: Vec<(usize, u64)>,
    /// Hausses (positives) et baisses des salaires survenues depuis la dernière lecture.
    pub wage_news: Vec<i32>,
    /// Catastrophes survenues depuis la dernière lecture : (nature, bâtiments détruits).
    pub disaster_news: Vec<(u8, i32)>,
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
    /// Attrait de chaque case, recalculé quand un bâtiment apparaît ou disparaît.
    appeal: Vec<i16>,
}

impl World {
    pub fn new(map: Map, kinds: Vec<Kind>, house_levels: Vec<HouseModel>, walkers: WalkerImages, money: i32) -> World {
        World {
            map,
            kinds,
            buildings: Vec::new(),
            figures: Vec::new(),
            money,
            ticks: 0,
            cells: vec![Cell::Terrain; GRID * GRID],
            house_levels,
            walkers,
            next_house: 0,
            random: 0x2545_F491,
            last_month: (0, 0),
            year_profit: 0,
            best_year_profit: None,
            produced: [0; RESOURCES],
            best_produced: [0; RESOURCES],
            set_aside: [0; RESOURCES],
            max_sanctuaries: MAX_SANCTUARIES,
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
            carried: [0; RESOURCES],
            gifts: Vec::new(),
            price_shift: [0; RESOURCES],
            trade_news: Vec::new(),
            wage_shift: 0,
            armies_away: Vec::new(),
            wage_news: Vec::new(),
            disaster_news: Vec::new(),
            floods: Vec::new(),
            monument_layouts: Vec::new(),
            locked: Vec::new(),
            fires: Vec::new(),
            disasters: Vec::new(),
            appeal: vec![0; GRID * GRID],
        }
    }

    /// Cité vierge sur une autre carte, avec les mêmes types de bâtiments et les mêmes
    /// règles ; le calendrier continue là où en est celle-ci.
    pub fn new_city(&self, map: Map, money: i32) -> World {
        let mut city = World::new(map, self.kinds.clone(), self.house_levels.clone(), self.walkers, money);
        city.ticks = self.ticks;
        city.cities = self.cities.clone();
        city.monument_layouts = self.monument_layouts.clone();
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
                || (self.cell(x as usize, y as usize) == Cell::Terrain
                    && self.map.terrain(x as usize, y as usize) & TERRAIN_ROAD != 0))
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

    fn is_walkable(&self, x: i32, y: i32) -> bool {
        if !self.in_map(x, y) || matches!(self.cell(x as usize, y as usize), Cell::Building(_) | Cell::Chasm(_) | Cell::Flood) {
            return false;
        }
        let terrain = self.map.terrain(x as usize, y as usize);
        // Une rampe se franchit bien qu'elle porte aussi le bit de relief.
        terrain & IMPASSABLE == 0 || (terrain & TERRAIN_RAMP != 0 && terrain & (IMPASSABLE & !TERRAIN_ELEVATION) == 0)
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
            Function::Fishery => {
                let near_water = (y - 1..=y + size).any(|ty| (x - 1..=x + size).any(|tx| Self::in_grid(tx, ty) && self.map.terrain(tx as usize, ty as usize) & TERRAIN_WATER != 0));
                self.can_build(x, y, k.size) && near_water
            }
            Function::Farm => self.can_build(x, y, k.size) && (0..size).all(|dy| (0..size).all(|dx| self.is_meadow(x + dx, y + dy))),
            // Un temple par héros.
            Function::HeroHall(hero) => {
                !self.buildings.iter().flatten().any(|b| self.kinds[b.kind].function == Function::HeroHall(hero))
                    && (0..k.depth as i32).all(|dy| (0..size).all(|dx| self.is_free(x + dx, y + dy)))
            }
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
            .then_some(House { level: first_level, population: 0, arriving: 0, covered_until: [0; SERVICES.len()], goods: [0; GOODS.len()] });
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
        }));
        for dy in 0..depth {
            for dx in 0..size {
                self.cells[(y as usize + dy) * GRID + x as usize + dx] = Cell::Building(id);
            }
        }
        self.money -= cost;
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
            Cell::Rubble(_) | Cell::Road | Cell::AgoraRoad => {
                self.cells[y as usize * GRID + x as usize] = Cell::Terrain;
                true
            }
            Cell::Building(id) => {
                // Une échoppe démolie rend son emplacement à l'agora.
                let vendor = self.buildings[id as usize].as_ref().filter(|b| matches!(self.kinds[b.kind].function, Function::Vendor(_))).map(|b| (b.x, b.y));
                self.remove_building(id, Cell::Terrain);
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
            Role::Fetch { source, vendor } => source != id && vendor != id,
            Role::Rounds { home, .. } | Role::Return { home, .. } => home != id,
            Role::Monster { .. } => true,
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
                let id = self.map.image(x, y);
                if id == 0 || self.map.edge(x, y) & EDGE_DRAW_TILE == 0 {
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
                        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                        Self::in_grid(nx, ny) && matches!(self.cell(nx as usize, ny as usize), Cell::Chasm(_))
                    };
                    let shape = NEIGHBOURS.iter().enumerate().filter(|&(_, &(dx, dy))| open(dx, dy)).fold(0, |mask, (bit, _)| mask | 1 << bit);
                    let pieces = CHASM_PIECES[shape];
                    pieces[variant as usize % pieces.len()]
                },
                burning: false,
                overlay: None,
                ground: self.map.resolve(self.map.image(x, y)),
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
            Cell::Road => {
                let mut borders = 0;
                for (bit, (dx, dy)) in NEIGHBOURS.iter().enumerate() {
                    if !self.is_road(x as i32 + dx, y as i32 + dy) {
                        borders |= 1 << bit;
                    }
                }
                Some(Draw { source: Source::Terrain, image: ROAD_FIRST_IMAGE + ROAD_VARIANT[borders], burning: false, overlay: None, ground: None, lift: 0, above: None })
            }
            Cell::Building(id) => {
                let building = self.buildings[id as usize].as_ref()?;
                let kind = &self.kinds[building.kind];
                // Sanctuaire en chantier : autel et statues ne sont pas encore là, chacune
                // de leurs cases montre du dallage.
                if matches!(kind.function, Function::Sanctuary { .. }) && !self.is_complete(building) {
                    let (rx, ry) = (x - building.x, y - building.y);
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
                if let Function::Monument { model, needs } = kind.function {
                    let layout = self.monument_layouts.get(model as usize)?;
                    let tile = layout.tiles.iter().find(|t| (building.x + t.dx, building.y + t.dy) == (x, y))?;
                    let stage = self.monument_stage(building, needs, layout.stages());
                    return Some(self.monument_draw(tile, tile.order <= stage));
                }
                // Chaque élément est porté par sa case la plus à gauche à l'écran.
                let part = kind.parts.iter().find(|p| x == building.x + p.dx && y == building.y + p.dy + p.size - 1)?;
                let image = match (&building.house, part.crop, part.bay) {
                    // Logement luxueux : le rang de la baie désigne le quart du bâtiment.
                    (Some(house), _, Some(quadrant)) if house.level >= ELITE_FIRST_LEVEL => {
                        let level = (house.level - ELITE_FIRST_LEVEL).min(ELITE_LEVELS - 1);
                        ELITE_FIRST_IMAGE + ELITE_PIECES * level + ELITE_QUADRANT_PIECE[quadrant as usize % 4]
                    }
                    (Some(house), ..) => HOUSE_IMAGES[house.level.min(HOUSE_IMAGES.len() - 1)],
                    (None, true, _) => part.image + self.crop_stage(building),
                    (None, false, Some(bay)) => bay_image(&building.goods, bay as usize),
                    (None, false, None) => part.image,
                };
                let overlay = kind.stock_overlay.filter(|_| building.stock > 0);
                let ground = part.crop.then(|| self.map.resolve(self.map.image(x, y))).flatten();
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
                    _ => (part.source, image),
                };
                Some(Draw { source, image, burning: false, overlay, ground, lift: 0, above: None })
            }
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
            .map(|b| match (self.kinds[b.kind].function, good) {
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
                Some(b) => self.kinds[b.kind].function,
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

    /// Le sanctuaire a reçu tous ses matériaux.
    pub fn is_complete(&self, building: &Building) -> bool {
        match self.kinds[building.kind].function {
            Function::Sanctuary { needs, .. } | Function::Monument { needs, .. } => {
                SANCTUARY_MATERIALS.iter().zip(needs).all(|(&good, need)| building.goods[good as usize] >= need as i32)
            }
            _ => true,
        }
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

    /// Case d'un monument : la pierre empilée jusqu'à sa hauteur (blocs de un à
    /// quatre niveaux de `poseidon_loaded.sg3`), et la pièce qui la couronne. Olympe
    /// dessine les pentes comme des blocs ; une case pas encore élevée montre le
    /// dallage du chantier.
    fn monument_draw(&self, tile: &crate::monument::Tile, built: bool) -> Draw {
        use crate::monument::*;
        let plain = |image: usize| Draw { source: Source::Poseidon, image, burning: false, overlay: None, ground: None, lift: 0, above: None };
        if tile.piece == ACCESS_PAD {
            return plain(MONUMENT_PAD_IMAGE);
        }
        if !built || tile.piece == 0 {
            return plain(MONUMENT_FLOOR_IMAGE);
        }
        let (stone, cap) = match tile.piece {
            STEEP_FIRST..=STEEP_LAST | LIFT => (tile.top(), None),
            TOPPER => (tile.elevation, Some(MONUMENT_TOPPER_IMAGE)),
            TILE_PLAIN | TILE_ORICHALC | 55 => (tile.elevation, Some(MONUMENT_TILE_IMAGE)),
            _ => (tile.elevation, Some(MONUMENT_STATUE_IMAGE)),
        };
        let block = |levels: u8| MONUMENT_BLOCK_IMAGE + levels.clamp(1, MONUMENT_BLOCK_LEVELS) as usize - 1;
        let above = cap.map(|image| (Source::Poseidon, image, stone as u16 * MONUMENT_LEVEL_PIXELS));
        match stone {
            0 => plain(cap.unwrap_or(MONUMENT_FLOOR_IMAGE)),
            s if s <= MONUMENT_BLOCK_LEVELS => Draw { above, ..plain(block(s)) },
            s => Draw {
                ground: Some((Source::Poseidon, block(MONUMENT_BLOCK_LEVELS))),
                lift: MONUMENT_BLOCK_LEVELS as u16 * MONUMENT_LEVEL_PIXELS,
                above,
                ..plain(block(s - MONUMENT_BLOCK_LEVELS))
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
            if (EARTHQUAKE..=FLOOD).contains(&event.change) {
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
            if event.change == INVASION {
                if target.status == RIVAL {
                    let soldiers = amount.max(1);
                    let repelled = self.army() >= 2 * soldiers;
                    let (mut buildings, mut drachmas) = (0, 0);
                    if !repelled {
                        drachmas = self.money.max(0) / 5;
                        self.money -= drachmas;
                        for _ in 0..(soldiers / 16).max(1) {
                            let standing: Vec<usize> = (0..self.buildings.len()).filter(|&i| self.buildings[i].is_some()).collect();
                            if standing.is_empty() {
                                break;
                            }
                            let id = standing[self.next_random() as usize % standing.len()];
                            let b = self.buildings[id].as_ref().unwrap();
                            let (kind, x, y) = (b.kind, b.x, b.y);
                            self.remove_building(id as u16, Cell::Rubble(0));
                            self.disasters.push(Disaster { fire: true, kind, x, y });
                            buildings += 1;
                        }
                    }
                    self.battles.push(Battle { city, soldiers, repelled, buildings, drachmas });
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
            }
        }
    }

    /// Force de l'armée : un homme de piétaille compte pour un, un hoplite pour
    /// deux, un cavalier pour trois (barème d'Olympe).
    pub fn army(&self) -> i32 {
        let (rabble, hoplites, horsemen) = (self.soldiers(Soldier::Rabble), self.soldiers(Soldier::Hoplite), self.soldiers(Soldier::Horseman));
        (rabble - hoplites) + 2 * (hoplites - horsemen) + 3 * horsemen
    }

    /// Force qu'il faut opposer à une cité : 24 par bouclier (barème d'Olympe).
    /// Une cité dont l'armée est partie ne garde que la moitié de ses boucliers.
    pub fn defence(&self, city: usize) -> i32 {
        let away = self.armies_away.iter().any(|&(c, until)| c == city && until > self.ticks);
        self.cities.get(city).map_or(0, |c| c.military as i32 * CONQUEST_PER_SHIELD / if away { 2 } else { 1 })
    }

    /// L'armée de cette cité est-elle partie en campagne ?
    pub fn army_away(&self, city: usize) -> bool {
        self.armies_away.iter().any(|&(c, until)| c == city && until > self.ticks)
    }

    /// Envoie l'armée soumettre une cité étrangère, alliée ou rivale : si elle est
    /// assez forte, la cité devient vassale et l'armée rentre six mois plus tard. Le
    /// jeu livre bataille ; Olympe se contente de comparer les forces.
    pub fn conquer(&mut self, city: usize) -> Result<(), Conquest> {
        let Some(target) = self.cities.get(city).cloned().filter(|c| c.active && c.kind == FOREIGN) else { return Err(Conquest::NotForeign) };
        if target.status > RIVAL {
            return Err(Conquest::AlreadyRuled);
        }
        if self.ticks < self.campaign_until {
            return Err(Conquest::Away);
        }
        let (army, defence) = (self.army(), self.defence(city));
        if army < defence {
            return Err(Conquest::TooWeak { army, defence });
        }
        self.cities[city].status = VASSAL;
        self.campaign_until = self.ticks + CAMPAIGN_TICKS;
        Ok(())
    }

    /// Ouvre une route commerciale avec une cité étrangère qui n'est pas rivale, au
    /// prix d'Olympe ; faux si c'est impossible ou trop cher.
    pub fn open_trade(&mut self, city: usize) -> bool {
        let open = self.cities.get(city).is_some_and(|c| c.active && c.kind == FOREIGN && c.status != RIVAL && !c.trade);
        if !open || self.money < TRADE_ROUTE_COST {
            return false;
        }
        self.money -= TRADE_ROUTE_COST;
        self.cities[city].trade = true;
        true
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
                Some(b) => self.kinds[b.kind].function,
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
        for city in 0..self.cities.len() {
            let c = &self.cities[city];
            if !(c.active && c.trade && c.status != RIVAL) {
                continue;
            }
            let (buys, sells) = (c.buys.clone(), c.sells.clone());
            let (mut sold, mut bought) = (0, 0);
            for (resource, quantity) in buys {
                let (resource, monthly) = (resource as usize, (quantity / 12).max(1));
                let spare = (self.stored(resource) - TRADE_RESERVE).min(monthly);
                if spare > 0 && resource < RESOURCES {
                    let taken = self.take(resource, spare);
                    self.money += taken * self.price(resource);
                    sold += taken * self.price(resource);
                }
            }
            for (resource, quantity) in sells {
                let (resource, monthly) = (resource as usize, (quantity / 12).max(1));
                let wanted = (TRADE_RESERVE - self.stored(resource)).min(monthly).min(self.money / self.price(resource).max(1));
                if wanted > 0 && resource < RESOURCES {
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
        (price(resource) + self.price_shift.get(resource).copied().unwrap_or(0)).max(1)
    }

    /// Programme un changement du commerce.
    pub fn schedule_trade(&mut self, change: u8, resource: u8, cities: (i16, i16), amount: (i16, i16), month: u8, years: (i16, i16)) {
        let at = self.date_in(month, years);
        self.scheduled.push(Scheduled { at, change: TRADE_CHANGE + change, cities, amount, resource });
    }

    /// Partenaires commerciaux : cités non rivales avec qui une route est ouverte
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
            if let Some(building) = self.buildings.get(target as usize).and_then(|b| b.as_ref()) {
                let (kind, x, y) = (building.kind, building.x, building.y);
                self.remove_building(target, Cell::Rubble(0));
                self.disasters.push(Disaster { fire: false, kind, x, y });
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
        let path = straight_path(at, (building.x as u16, building.y as u16));
        let figure = &mut self.figures[index];
        if let Some(&next) = path.last() {
            figure.direction = direction(at, next);
        }
        figure.path = path;
        (target, rest_until)
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
        // Un dieu favorable dont le sanctuaire est achevé chasse vite l'envahisseur.
        let stay = if self.sanctuaries().is_empty() { GOD_STAY_TICKS } else { GOD_REPELLED_TICKS };
        for god in due {
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

    /// Chantiers des sanctuaires : chaque mois, ceux qui ont leurs ouvriers prennent
    /// dans les entrepôts ce qui leur manque (sans porteurs, pour l'instant).
    fn supply_sanctuaries(&mut self) {
        for id in 0..self.buildings.len() {
            let Some(building) = &self.buildings[id] else { continue };
            let (Function::Sanctuary { needs, .. } | Function::Monument { needs, .. }) = self.kinds[building.kind].function else { continue };
            if !building.staffed {
                continue;
            }
            for (&good, need) in SANCTUARY_MATERIALS.iter().zip(needs) {
                let Some(resource) = resource_of(good) else { continue };
                let missing = need as i32 - self.buildings[id].as_ref().map_or(0, |b| b.goods[good as usize]);
                if missing > 0 {
                    let taken = self.take(resource as usize, missing);
                    if let Some(building) = self.buildings[id].as_mut() {
                        building.goods[good as usize] += taken;
                    }
                }
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

    /// Capacité d'accueil d'une maison à son niveau actuel.
    pub fn capacity(&self, house: &House) -> i32 {
        self.house_levels.get(house.level).map_or(0, |level| level.capacity)
    }

    /// Avance la simulation d'un pas.
    pub fn tick(&mut self) {
        self.ticks += 1;
        self.move_figures();
        if self.ticks % IMMIGRATION_INTERVAL == 0 {
            self.send_immigrants();
        }
        self.run_buildings();
        self.burn_out();
        self.recede();
        if self.ticks % TICKS_PER_DAY == 0 {
            self.update_risks();
        }
        if self.ticks % TICKS_PER_MONTH == 0 {
            self.update_houses();
            self.assign_workers();
            self.settle_month();
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
        for id in 0..self.buildings.len() {
            let Some(building) = self.buildings[id].as_ref() else { continue };
            // Une maison vide ne risque rien.
            if building.house.as_ref().is_some_and(|house| house.population == 0) {
                continue;
            }
            if self.next_random() % RISK_DAILY_CHANCE != 0 {
                continue;
            }
            let Some(building) = self.buildings[id].as_mut() else { continue };
            let kind = &self.kinds[building.kind];
            building.fire += kind.fire_risk;
            building.damage += kind.damage_risk;
            let (burns, collapses) = (building.fire > RISK_LIMIT, building.damage > RISK_LIMIT);
            let (kind, x, y) = (building.kind, building.x, building.y);
            if burns || collapses {
                self.remove_building(id as u16, if burns { Cell::Fire(0) } else { Cell::Rubble(0) });
                self.disasters.push(Disaster { fire: burns, kind, x, y });
            }
        }
    }

    /// Habitants en âge de travailler.
    pub fn workers_available(&self) -> i32 {
        // Les habitants des logements luxueux ne travaillent pas.
        let houses = self.buildings.iter().flatten().filter_map(|b| b.house.as_ref());
        let common: i32 = houses.filter(|h| h.level < ELITE_FIRST_LEVEL).map(|h| h.population).sum();
        common * WORKFORCE_PERCENT / 100
    }

    /// Employés demandés par l'ensemble des bâtiments.
    pub fn workers_needed(&self) -> i32 {
        self.buildings.iter().flatten().map(|b| self.kinds[b.kind].employees).sum()
    }

    /// Répartit la main-d'œuvre : les bâtiments sont servis dans l'ordre de
    /// construction, ceux qui restent sans employés s'arrêtent. Partager la pénurie
    /// entre tous (comme Caesar III) a été essayé : une cité posée d'un coup, comme la
    /// démo, n'a alors plus ni vivres ni maintenance à temps plein et s'effondre.
    fn assign_workers(&mut self) {
        let mut left = self.workers_available();
        for id in 0..self.buildings.len() {
            let Some(building) = self.buildings[id].as_mut() else { continue };
            let needed = self.kinds[building.kind].employees;
            building.staffed = needed <= left;
            if building.staffed {
                left -= needed;
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
        let taxes = taxes / 100;
        let employed: i32 = self.buildings.iter().flatten().filter(|b| b.staffed).map(|b| self.kinds[b.kind].employees).sum();
        let wages = employed * WAGES_PER_100_WORKERS * (NORMAL_WAGE + self.wage_shift).max(0) / (100 * NORMAL_WAGE);
        self.money += taxes - wages;
        self.last_month = (taxes, wages);
        self.supply_sanctuaries();
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
            for (best, year) in self.best_produced.iter_mut().zip(&mut self.produced) {
                *best = (*best).max(std::mem::take(year));
            }
            // Visite annuelle des dieux favorables qui ont un sanctuaire achevé.
            for god in self.sanctuaries() {
                self.bless(god);
            }
            // Tribut annuel des vassales.
            for city in 0..self.cities.len() {
                let c = &self.cities[city];
                if c.active && c.status == VASSAL {
                    let tribute = TRIBUTE_BASE + TRIBUTE_PER_SHIELD * c.military as i32;
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

    fn function(&self, id: usize) -> Function {
        self.buildings[id].as_ref().map_or(Function::None, |b| self.kinds[b.kind].function)
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
            if !building.staffed {
                // À l'arrêt : la pousse des champs est suspendue elle aussi.
                if matches!(function, Function::Farm | Function::Supplier(_) | Function::Producer(_) | Function::Fishery | Function::Workshop(_)) {
                    self.buildings[id].as_mut().unwrap().sown_at += 1;
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
                Function::Producer(_) | Function::Fishery => {
                    let good = if let Function::Producer(good) = function { good } else { Good::Food };
                    // Une laiterie met plus de temps à faire un chargement de vivres.
                    let needed = if good == Good::Food { DAIRY_TICKS } else { PRODUCTION_TICKS };
                    if self.ticks - building.sown_at >= needed {
                        let building = self.buildings[id].as_mut().unwrap();
                        building.sown_at = self.ticks;
                        let before = building.stock;
                        building.stock = (building.stock + 1).min(PRODUCER_STORAGE);
                        let (kind, made) = (building.kind, building.stock - before);
                        self.count_production(kind, made);
                    }
                    if stock >= CARRIER_LOAD && !errand_out {
                        self.send_harvest(id, function);
                    }
                }
                Function::Workshop(_) => {
                    if building.raw == 0 || building.stock >= PRODUCER_STORAGE {
                        self.buildings[id].as_mut().unwrap().sown_at = self.ticks;
                    } else if self.ticks - building.sown_at >= if function == Function::Workshop(Good::Sculpture) { SCULPTURE_TICKS } else { PRODUCTION_TICKS } {
                        let building = self.buildings[id].as_mut().unwrap();
                        building.sown_at = self.ticks;
                        building.raw -= 1;
                        building.stock += 1;
                        let kind = building.kind;
                        self.count_production(kind, 1);
                    }
                    if stock >= CARRIER_LOAD && !errand_out {
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
                        let loads = stock.min(VENDOR_LOAD);
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
        let mut heading = 0;
        if let Some(building) = self.buildings[id].as_mut() {
            building.next_walk = self.ticks + ROUNDS_INTERVAL;
            building.walks = building.walks.wrapping_add(1);
            heading = building.walks % NEIGHBOURS.len() as u8;
        }
        let Some(from) = self.road_next_to(id) else { return false };
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
                match (function, world.kinds[building.kind].function) {
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
        let load = building.stock.min(CARRIER_LOAD);
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
            world.is_road(x + dx, y + dy).then(|| ((x + dx) as u16, (y + dy) as u16))
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
                Rounds::Service(service) => house.covered_until[*service as usize] = until,
                Rounds::Goods { good, carried, .. } => {
                    // La maison prend de quoi tenir quelques mois.
                    let reserve = match good {
                        Good::Food => HOUSE_FOOD_MONTHS * house.population.max(MIN_HOUSEHOLD),
                        // Seuls les logements luxueux prennent du vin, des armes et des chevaux.
                        Good::Wine | Good::Arms | Good::Horses if house.level < ELITE_FIRST_LEVEL => 0,
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
        // Personnages qui viennent d'atteindre une case.
        let mut reached = Vec::new();
        for (index, figure) in self.figures.iter_mut().enumerate() {
            figure.frame += 1;
            let Some(&(tx, ty)) = figure.path.last() else {
                reached.push(index);
                continue;
            };
            let progress = figure.frame as f32 / WALK_TICKS as f32;
            let (fx, fy) = (figure.from.0 as f32, figure.from.1 as f32);
            figure.x = fx + (tx as f32 - fx) * progress;
            figure.y = fy + (ty as f32 - fy) * progress;
            if figure.frame >= WALK_TICKS {
                figure.frame = 0;
                figure.came_from = Some(figure.from);
                figure.from = (tx, ty);
                figure.path.pop();
                match figure.path.last() {
                    Some(&next) => figure.direction = direction(figure.from, next),
                    None => reached.push(index),
                }
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
        let stadium_open = self.buildings.iter().flatten().any(|b| b.staffed && self.kinds[b.kind].function == Function::Stadium);
        for id in 0..self.buildings.len() {
            let Some(building) = self.buildings[id].as_ref() else { continue };
            let appeal = self.appeal(building.x as i32, building.y as i32);
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
            house.goods[Good::Food as usize] = (house.goods[Good::Food as usize] - needed).max(0);
            if goods_month {
                for good in [Good::Fleece, Good::Oil, Good::Wine, Good::Arms] {
                    house.goods[good as usize] = (house.goods[good as usize] - 1).max(0);
                }
            }
            let culture = CULTURE.iter().filter(|&&service| house.has(service, now)).count() as i32;

            // Les exigences d'un niveau.
            let meets = |level: &HouseModel| {
                (level.food_types == 0 || fed)
                    && (level.stadium == 0 || stadium_open)
                    && (level.horses == 0 || had[Good::Horses as usize])
                    && (level.arms == 0 || had[Good::Arms as usize])
                    && (level.water == 0 || house.has(Service::Water, now))
                    && culture >= disciplines_for(level.culture)
                    && (level.fleece == 0 || had[Good::Fleece as usize])
                    && (level.olive_oil == 0 || had[Good::Oil as usize])
                    && (level.wine == 0 || had[Good::Wine as usize])
            };
            let Some(current) = self.house_levels.get(house.level) else { continue };
            // Un logement modeste va de 0 à 6. Un logement luxueux part du terrain vacant,
            // saute l'état délabré en montant et ne retombe pas plus bas que lui.
            let elite = house.level >= ELITE_FIRST_LEVEL;
            let (floor, top) = if elite { (ELITE_DEVOLVED, ELITE_FIRST_LEVEL + ELITE_LEVELS - 1) } else { (0, HOUSE_IMAGES.len() - 1) };
            let next = if house.level == ELITE_FIRST_LEVEL { house.level + 2 } else { house.level + 1 };
            let next_ok = next <= top && self.house_levels.get(next).is_some_and(meets);
            if (!meets(current) || appeal < current.devolve_desirability) && house.level > floor {
                house.level -= 1;
                // Les habitants en trop quittent la cité.
                let capacity = self.house_levels[house.level].capacity;
                house.population = house.population.min(capacity);
            } else if next_ok && appeal >= current.evolve_desirability {
                house.level = next;
            }
        }
    }

    /// Envoie un groupe d'immigrants vers la prochaine maison qui a de la place.
    fn send_immigrants(&mut self) {
        let count = self.buildings.len();
        for step in 0..count {
            let id = (self.next_house + step) % count;
            let Some(building) = &self.buildings[id] else { continue };
            let Some(house) = &building.house else { continue };
            let room = self.capacity(house) - house.population - house.arriving;
            let size = self.kinds[building.kind].size as i32;
            let (x, y) = (building.x as i32, building.y as i32);
            if room <= 0 || !self.has_road_access(x, y, size) {
                continue;
            }
            let Some(path) = self.path_from_edge(x, y, size) else { continue };
            let people = room.min(IMMIGRANT_GROUP);
            let image = if house.level >= ELITE_FIRST_LEVEL { self.walkers.elite_immigrant } else { self.walkers.immigrant };
            self.spawn_on_path(image, path, Role::Immigrant { home: id as u16, people });
            if let Some(house) = self.buildings[id].as_mut().and_then(|b| b.house.as_mut()) {
                house.arriving += people;
            }
            self.next_house = id + 1;
            return;
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
fn bay_image(goods: &[i32; GOODS.len()], bay: usize) -> usize {
    let mut first = 0;
    for (good, &loads) in goods.iter().enumerate() {
        let bays = ((loads + BAY_LOADS - 1) / BAY_LOADS) as usize;
        if bay < first + bays {
            let inside = if GOODS[good] == Good::Sculpture { 1 } else { (loads - (bay - first) as i32 * BAY_LOADS).clamp(1, BAY_LOADS) };
            return BAY_FIRST_IMAGES[good] + inside as usize - 1;
        }
        first += bays;
    }
    EMPTY_BAY_IMAGE
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
        Good::Fleece | Good::Oil | Good::Wine | Good::Arms | Good::Horses | Good::Bronze | Good::Wood | Good::Marble | Good::Sculpture => 1,
    }
}

/// Nombre de disciplines (philosophie, sport, théâtre) qu'exige un seuil de culture
/// des règles. Interprétation provisoire des seuils 15, 35 et 45.
fn disciplines_for(culture: i32) -> i32 {
    match culture {
        ..=0 => 0,
        1..=15 => 1,
        16..=35 => 2,
        _ => 3,
    }
}

/// Direction de marche (rang dans `DIRECTIONS`) pour aller d'une case à sa voisine.
fn direction(from: (u16, u16), to: (u16, u16)) -> usize {
    let step = ((to.0 as i32 - from.0 as i32).signum(), (to.1 as i32 - from.1 as i32).signum());
    DIRECTIONS.iter().position(|&d| d == step).unwrap_or(0)
}

/// En-tête des fichiers de sauvegarde ; le numéro change quand le format change.
const SAVE_HEADER: &str = "zeus-sauvegarde 12";
/// Versions précédentes, encore lues, et nombre de marchandises qu'elles connaissaient
/// (les plus récentes s'ajoutent à la fin de `GOODS`).
const OLD_SAVE_HEADERS: [(&str, usize); 3] = [("zeus-sauvegarde 9", 6), ("zeus-sauvegarde 10", 7), ("zeus-sauvegarde 11", 9)];

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
            self.next_house,
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
        for &(city, until) in &self.armies_away {
            let _ = writeln!(out, "absente {city} {until}");
        }
        let present: Vec<String> = self.heroes.iter().map(|&h| (h as u8).to_string()).collect();
        let _ = writeln!(out, "heros {}", present.join(" "));
        // Statut de chaque cité, plus 10 quand une route commerciale est ouverte.
        let cities: Vec<String> =
            std::iter::once(self.campaign_until.to_string()).chain(self.cities.iter().map(|c| (c.status + 10 * c.trade as i32).to_string())).collect();
        let _ = writeln!(out, "cites {}", cities.join(" "));
        for s in &self.scheduled {
            let _ = writeln!(out, "changement {} {} {} {} {} {} {}", s.at, s.change, s.cities.0, s.cities.1, s.amount.0, s.amount.1, s.resource);
        }
        for &(at, god) in &self.invasions {
            let _ = writeln!(out, "invasion {at} {god}");
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
                b.kind, b.x, b.y, b.stock, b.next_walk, b.sown_at, b.errand_out as u8, b.staffed as u8, b.raw, b.walks, b.fire, b.damage
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
                Role::Fetch { source, vendor } => format!("achat {source} {vendor} 0 0 0"),
                Role::Monster { monster, target, rest_until } => format!("bete {monster} {target} {rest_until} 0 0"),
                Role::Hero { hero, hall } => format!("heros {hero} {hall} 0 0 0"),
                Role::God { god, target, rest_until, leaves_at } => format!("dieu {god} {target} {rest_until} {leaves_at} 0"),
                Role::Visit { god, leaves_at } => format!("visite {god} {leaves_at} 0 0 0"),
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
        let mut favors: Vec<i32> = Vec::new();
        let mut traded: Vec<i32> = Vec::new();
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
                "production" | "meilleure" | "reserve" | "emporte" | "prix" => {
                    let row = ["production", "meilleure", "reserve", "emporte", "prix"].iter().position(|&k| k == kind).unwrap_or(0);
                    for (i, slot) in counters[row].iter_mut().enumerate() {
                        *slot = number_at(i)? as i32;
                    }
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
                            })
                        }
                        _ => None,
                    };
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
                        staffed: number_at(7)? != 0,
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
                        "achat" => Role::Fetch { source: a as u16, vendor: b as u16 },
                        "bete" => Role::Monster { monster: a as u8, target: b as u16, rest_until: c as u64 },
                        "heros" => Role::Hero { hero: a as u8, hall: b as u16 },
                        "dieu" => Role::God { god: a as u8, target: b as u16, rest_until: c as u64, leaves_at: d as u64 },
                        "visite" => Role::Visit { god: a as u8, leaves_at: b as u64 },
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
                            _ => Source::Figures,
                        },
                    });
                }
                "" => {}
                _ => return Err(bad()),
            }
        }
        let (ticks, money, next_house, random, taxes, wages) = state.ok_or("sauvegarde sans ligne d'état")?;
        // Un personnage ne doit pas désigner un bâtiment absent.
        let exists = |id: u16| buildings.get(id as usize).is_some_and(|b: &Option<Building>| b.is_some());
        figures.retain(|f| match f.role {
            Role::Immigrant { home, .. } => exists(home),
            Role::Deliver { to, from, .. } => exists(to) && exists(from),
            Role::Fetch { source, vendor } => exists(source) && exists(vendor),
            Role::Rounds { home, .. } | Role::Return { home, .. } => exists(home),
            Role::Monster { .. } => true,
            Role::Hero { hall, .. } => exists(hall),
            Role::God { .. } | Role::Visit { .. } => true,
        });
        self.cells = cells;
        self.fires = fires;
        (self.floods, self.disaster_news) = (floods, Vec::new());
        self.disasters.clear();
        self.buildings = buildings;
        self.figures = figures;
        self.ticks = ticks as u64;
        self.money = money as i32;
        self.next_house = next_house as usize;
        self.random = (random as u32).max(1);
        self.last_month = (taxes as i32, wages as i32);
        (self.year_profit, self.best_year_profit) = profits;
        [self.produced, self.best_produced, self.set_aside, self.carried, self.price_shift] = counters;
        (self.heroes, self.quests, self.monsters) = (heroes, quests, monsters);
        self.campaign_until = campaign;
        self.scheduled = scheduled;
        (self.wage_shift, self.armies_away, self.wage_news) = (wage_shift, away, Vec::new());
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
