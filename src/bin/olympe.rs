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
use zeus::jpeg;
use zeus::messages::{self, Messages};
use zeus::map::{GRID, Map, Source};
use zeus::model::Model;
use zeus::ambience::Ambience;
use zeus::music::Music;
use zeus::pak::{Campaign, Settings};
use zeus::paths;
use zeus::png;
use zeus::sdl;
use zeus::sg::Catalog;
use zeus::text::Text;
use zeus::world::{TERRAIN_BEACH, TERRAIN_MEADOW, TERRAIN_ROCK, TERRAIN_TREE, TERRAIN_WATER};
use zeus::world::{Appeal, Function, Raw, GOODS, Good, Kind, Part, SERVICES, Service, WalkerImages, World};

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
  --ticks N          nombre de pas de simulation joués avant la capture (1500 par défaut)
  --script \"…\"      essai scripté, une étape par image, avant la capture : m:x,y (pointeur),
                     d / u (bouton gauche), w:n (molette), k:1..9 | x | esc | espace | f5 | f9 | h,
                     t:n (pas), p:ms (pause)
en jeu : H affiche l'aide";

/// Demi-largeur et demi-hauteur d'une case à l'écran, à l'échelle 1.
const HALF_WIDTH: f32 = 30.0;
const HALF_HEIGHT: f32 = 15.0;
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
const DAIRY_IMAGE: usize = 548;
const HUNTING_LODGE_IMAGE: usize = 544;
const SIMPLE_KINDS: [(usize, usize, Function, usize); 34] = [
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
    // Laiterie : produit du fromage sans chèvres (raccourci) ; image attribuée à l'œil.
    (42, DAIRY_IMAGE, Function::Producer(Good::Food), 0),
    // Pavillon de chasse : fournit de la viande sans chasseurs ni gibier (raccourci) ;
    // image attribuée à l'œil.
    (46, HUNTING_LODGE_IMAGE, Function::Producer(Good::Food), 0),
    // Pêcherie : posée sur la terre ferme au bord de l'eau, sans bateau (raccourci ;
    // dans le jeu d'origine elle s'avance sur l'eau).
    (44, 719, Function::Fishery, 0),
    (37, 587, Function::Supplier(Raw::Crops), 0),
    // Fonderie et armurerie : images attribuées à l'œil.
    (TYPE_FOUNDRY, 618, Function::Supplier(Raw::Bronze), 0),
    (135, 740, Function::Workshop(Good::Arms), 0),
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
const RESOURCE_BRONZE: u8 = 10;
const RESOURCE_GRAPES: u8 = 12;
const RESOURCE_OLIVES: u8 = 13;
const RESOURCE_FLEECE: u8 = 14;
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
const GOOD_TEXTS: [usize; 6] = [25, 14, 20, 21, 18, 15];
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
        Function::Service(Service::Security) => "DEFENSIVE",
        Function::Service(Service::Health) => "INFIRMARY",
        Function::Service(Service::Tax) => "MUNICIPAL",
        Function::Palace => "PALACE",
        Function::Service(Service::Philosophy) | Function::School(_) => "PHILOSOPHY",
        Function::Service(Service::Athletics) => "GYMNASIUM",
        Function::Service(Service::Drama) => "THEATRE",
        Function::Farm => "FARMING",
        Function::Granary | Function::Storehouse => "STORAGE",
        Function::Producer(Good::Horses) => "HORSE_RANCH",
        Function::Producer(Good::Food) => "GOAT_FARM",
        Function::Fishery => "SEA",
        Function::Producer(_) => "SHEEP_FARM",
        Function::Supplier(Raw::Crops) => "ORCHARD",
        Function::Supplier(Raw::Bronze) => "FOUNDRY",
        Function::Workshop(_) => "WORKSHOPS",
        Function::Agora { .. } | Function::Plot | Function::Vendor(Good::Food) => "AGORA_FOOD",
        Function::Vendor(Good::Fleece) => "AGORA_FLEECE",
        Function::Vendor(Good::Oil) => "AGORA_OIL",
        Function::Vendor(Good::Wine) => "AGORA_WINE",
        Function::Vendor(Good::Arms) => "AGORA_ARMS",
        Function::Vendor(Good::Horses) => "AGORA_HORSE",
        Function::Stadium | Function::Annex => "STADIUM",
        Function::None => "BEAUTIFICATION",
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
const HELP: [&str; 14] = [
    "Clic droit ou flèches : déplacer la vue",
    "Molette : zoom",
    "Mini-carte : clic pour s'y rendre",
    "1 à 9 : ouvrir une catégorie, puis choisir un bâtiment",
    "Clic gauche : bâtir ; glisser pour tracer une route",
    "X : dégager",
    "Echap : lâcher l'outil, refermer la catégorie, quitter",
    "Espace : pause",
    "Page préc. et Page suiv. : vitesse du jeu",
    "F5 : enregistrer ; F9 : recharger",
    "F : plein écran",
    "M : musique ; B : bruitages",
    "Survoler un bâtiment : ses informations",
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
const CATEGORY_TABS: [(usize, usize); 8] = [(1, 0), (2, 1), (3, 2), (4, 3), (5, 4), (6, 5), (7, 6), (10, 7)];

/// Catégories du menu de construction : intitulé (rang dans le groupe de textes 4,
/// ou texte de secours quand le jeu n'en a pas) et rôles des bâtiments rangés dedans.
const CATEGORY_NAMES: [(Option<usize>, &str); 8] = [
    (None, "Population"),
    (Some(6), "Agriculture"),
    (Some(1), "Industrie"),
    (Some(5), "Distribution"),
    (Some(7), "Hygiène et sécurité"),
    (Some(11), "Administration"),
    (Some(9), "Culture"),
    (None, "Embellissement"),
];

/// Rang, dans `CATEGORY_NAMES`, de la catégorie d'un bâtiment.
fn category_of(function: Function) -> usize {
    match function {
        Function::House => 0,
        Function::Farm | Function::Producer(_) | Function::Fishery | Function::Supplier(Raw::Crops) => 1,
        Function::Supplier(Raw::Bronze) => 2,
        Function::Workshop(_) => 2,
        Function::Granary | Function::Storehouse | Function::Agora { .. } | Function::Vendor(_) => 3,
        // Les emplacements ne se bâtissent qu'à travers l'agora.
        Function::Plot => usize::MAX,
        Function::Service(Service::Water | Service::Maintenance | Service::Security | Service::Health) => 4,
        Function::Service(Service::Tax) | Function::Palace => 5,
        Function::Service(Service::Philosophy | Service::Athletics | Service::Drama) | Function::School(_) | Function::Stadium => 6,
        // L'annexe du stade ne se bâtit qu'avec lui.
        Function::Annex => usize::MAX,
        Function::None => 7,
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

/// Pixels de la mini-carte, en RVBA.
fn minimap_pixels(world: &World) -> Vec<u8> {
    let mut pixels = vec![0u8; MINIMAP_WIDTH * MINIMAP_HEIGHT * 4];
    for y in 0..GRID {
        for x in 0..GRID {
            let colour = minimap_colour(world, x, y);
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
                    let (px, py) = (x + GRID - 1 - y, (x + y) / 2);
                    (x0, y0, x1, y1) = (x0.min(px), y0.min(py), x1.max(px + 2), y1.max(py + 1));
                }
            }
        }
        let (width, height) = (x1.saturating_sub(x0).max(1) as f32, y1.saturating_sub(y0).max(1) as f32);
        let scale = (MINIMAP_MAX_WIDTH / width).min(MINIMAP_MAX_HEIGHT / height);
        Minimap { x: 0.0, y: 0.0, scale, left: x0 as f32, top: y0 as f32, width, height }
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
    let road = (category == 0).then_some(Tool::Road);
    let offered = |k: &usize| world.locked.get(*k) != Some(&true);
    let kinds = (0..world.kinds.len()).filter(|&k| category_of(world.kinds[k].function) == category).filter(offered).map(Tool::Build);
    road.into_iter().chain(kinds).collect()
}

/// Ce que fait un clic sur une ligne du menu.
#[derive(Clone, Copy, PartialEq)]
enum MenuRow {
    /// Ouvre ou referme une catégorie.
    Category(usize),
    Pick(Tool),
}

/// Outil tenu par le joueur.
#[derive(Clone, Copy, PartialEq)]
enum Tool {
    None,
    Road,
    Demolish,
    Build(usize),
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|first| first == "installer") {
        install(&args[1..]);
        return;
    }
    let (mut demo, mut capture, mut warmup) = (false, None, CAPTURE_WARMUP_TICKS);
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

/// Charge une carte seule ou la cité mère d'une campagne, avec les réglages de
/// l'aventure quand on les trouve (dans le `.pak`, ou dans un `.set` voisin de la carte).
fn load_map(path: &Path) -> Result<(Map, Option<Settings>), String> {
    let file = fs::read(path).map_err(|e| format!("{} : {e}", path.display()))?;
    if file.starts_with(b"MAPS") {
        let neighbours = path.parent().and_then(|dir| fs::read_dir(dir).ok());
        let set = neighbours.into_iter().flatten().flatten().map(|entry| entry.path()).find(|p| p.extension().is_some_and(|e| e == "set"));
        let settings = set.and_then(|p| fs::read(p).ok()).and_then(|data| Settings::parse(&data));
        return Ok((Map::parse(&file)?, settings));
    }
    let mut campaign = Campaign::parse(&file)?;
    let parent = if campaign.maps.len() > 1 { 1 } else { 0 };
    Ok((campaign.maps.swap_remove(parent), campaign.settings))
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

/// Récit d'ouverture d'une aventure : son titre et ses paragraphes. Les aventures
/// livrées en dossier l'apportent dans un fichier de textes ; les autres le prennent
/// dans les messages du jeu, au numéro donné par leur en-tête.
fn briefing(data: &Path, map_path: &Path, settings: Option<Settings>) -> Option<(String, Vec<String>)> {
    let folder = map_path.parent()?;
    let texts = fs::read_dir(folder).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e == "txt"));
    // Un fichier de textes ne compte que dans le dossier propre à une aventure.
    if let Some(file) = texts.filter(|_| folder.file_name().is_some_and(|name| name != "adventures")) {
        let text = adventure_texts(&file)?;
        let entry = |key: &str| text_entry(&text, key);
        let mut paragraphs = messages::paragraphs(&entry("Adventure_Introduction").unwrap_or_default());
        if let Some(episode) = entry("Parent_Episode_1_Title") {
            paragraphs.push(episode);
        }
        paragraphs.extend(messages::paragraphs(&entry("Parent_Episode_1_Introduction").unwrap_or_default()));
        return Some((entry("Adventure_Title")?, paragraphs)).filter(|(_, paragraphs)| !paragraphs.is_empty());
    }
    let id = usize::try_from(settings?.first_message).ok()?;
    let message = Messages::parse(fs::read(data.join("zeus_mm.eng")).ok()?).ok()?.get(id)?;
    Some((message.title, message.paragraphs))
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
    destruction: Catalog,
    interface: Catalog,
    /// Teinte appliquée au texte (blanc = couleur propre de la police).
    ink: (u8, u8, u8),
    /// Pixels du sol nu de la carte (une tuile), pour boucher les réserves vertes.
    ground: Option<Vec<u8>>,
    sprites: HashMap<(Source, usize), Option<Sprite>>,
}

impl Graphics {
    fn catalog(&self, source: Source) -> &Catalog {
        match source {
            Source::Terrain => &self.terrain,
            Source::Poseidon => &self.poseidon,
            Source::General => &self.general,
            Source::Fonts => &self.fonts,
            Source::Figures => &self.figures,
            Source::Destruction => &self.destruction,
            Source::Interface => &self.interface,
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
fn tile_origin(x: i32, y: i32) -> (f32, f32) {
    ((x - y) as f32 * HALF_WIDTH, (x + y) as f32 * HALF_HEIGHT)
}

/// Case située sous un point du monde.
fn tile_at(wx: f32, wy: f32) -> (i32, i32) {
    let u = (wx - HALF_WIDTH) / HALF_WIDTH;
    let v = (wy - HALF_HEIGHT) / HALF_HEIGHT;
    (((v + u) / 2.0).round() as i32, ((v - u) / 2.0).round() as i32)
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
    put_farm(world, 0);
    put(world, kind_of(world, &role(Function::Granary)), x + 9, y + 1);
    put(world, kind_of(world, &role(Function::Agora { grand: true })), x + 2, y - 5);
    put(world, kind_of(world, &role(Function::Agora { grand: false })), x + 2, y - 11);
    put(world, vendor(world, Good::Food), x + 2, y - 5);
    put(world, service(world, Service::Water), x - 1, y - 11);
    // Masures : seconde fontaine (un porteur d'eau ne fait pas le tour entier),
    // philosophie et sport, maintenance de la grand-rue, deuxième exploitation.
    stage.set(150);
    put(world, service(world, Service::Water), x + 14, y - 4);
    put(world, kind_of(world, &role(Function::School(Service::Philosophy))), x + 40, y - 3);
    put(world, service(world, Service::Philosophy), x - 1, y - 7);
    put(world, service(world, Service::Athletics), x + 14, y - 12);
    put(world, maintenance, x + 20, y - 2);
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
    put(world, service(world, Service::Tax), x - 1, y - 3);
    put(world, kind_of(world, &role(Function::Palace)), x + 14, y + 1);
    // Immeubles : huile, garde.
    stage.set(480);
    put_maker(world, Function::Supplier(Raw::Crops));
    put_maker(world, Function::Workshop(Good::Oil));
    put(world, vendor(world, Good::Oil), x + 6, y - 5);
    // Une maintenance pour l'est de la grand-rue : un seul surintendant ne la couvre pas.
    put(world, maintenance, x + 43, farm_row);
    put_farm(world, 4);
    put_farm(world, 5);
    put(world, service(world, Service::Security), x - 1, y - 5);
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
    // Le stade a sa propre maintenance, au bord de la ruelle qui le longe.
    put(world, maintenance, x + 24, y + 11);
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
            found.push((lesson, name, pak));
        }
    }
    found.sort();
    found.into_iter().map(|(_, name, pak)| (name, pak)).collect()
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
        let load = |name: &str| Catalog::load(&data.join("data").join(name));
        let mut graphics = Graphics {
            renderer,
            terrain: load("zeus_terrain.sg3")?,
            poseidon: load("poseidon_loaded.sg3")?,
            general: load("zeus_general.sg3")?,
            fonts: load("zeus_fonts.sg3")?,
            figures: load("sprmain.sg3")?,
            destruction: load("destruction.sg3")?,
            interface: load("zeus_interface.sg3")?,
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
    let (map, settings) = load_map(map_path)?;
    let read = |file: &str| fs::read(data.join(file)).map_err(|e| format!("{file} : {e}"));
    let text = Text::parse(&read("zeus_text.eng")?)?;
    let model = Model::parse(&read("model/zeus_model_normal.txt")?)?;
    let general = Catalog::load(&data.join("data/zeus_general.sg3"))?;
    let figures = Catalog::load(&data.join("data/sprmain.sg3"))?;
    let road_cost = model.buildings.get(TYPE_ROAD).map_or(0, |b| b.cost);
    let road_name = text.get(TEXT_BUILDINGS, TYPE_ROAD);
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
        kinds.push(entry);
    }
    for (kind, crop) in FARMS {
        let mut parts = vec![Part { dx: 0, dy: 0, size: size_of(FARM_HOUSE_IMAGE)?, image: FARM_HOUSE_IMAGE, crop: false, bay: None }];
        parts.extend(FARM_FIELDS.map(|(dx, dy)| Part { dx, dy, size: 1, image: crop, crop: true, bay: None }));
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
        });
    }
    // Entrepôt : une cabane au coin du fond et huit baies.
    let bays = (0..3).flat_map(|dy| (0..3).map(move |dx| (dx, dy))).filter(|&at| at != (0, 0));
    let mut storehouse_parts = vec![Part { dx: 0, dy: 0, size: 1, image: STOREHOUSE_HUT_IMAGE, crop: false, bay: None }];
    storehouse_parts.extend(bays.enumerate().map(|(bay, (dx, dy))| Part { dx, dy, size: 1, image: EMPTY_BAY_IMAGE, crop: false, bay: Some(bay as u8) }));
    kinds.push(Kind {
        name: text.get(TEXT_BUILDINGS, TYPE_STOREHOUSE),
        cost: rules(TYPE_STOREHOUSE)?.cost,
        size: 3,
        depth: 3,
        parts: storehouse_parts,
        function: Function::Storehouse,
        walker: 0,
        stock_overlay: None,
        employees: rules(TYPE_STOREHOUSE)?.employees,
        appeal: appeal_of(TYPE_STOREHOUSE)?,
        fire_risk: rules(TYPE_STOREHOUSE)?.fire_risk,
        damage_risk: rules(TYPE_STOREHOUSE)?.damage_risk,
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
        parts: quarters.map(|(quarter, (dx, dy))| Part { dx, dy, size: 2, image: ELITE_LOT_IMAGE, crop: false, bay: Some(quarter as u8) }).collect(),
        function: Function::House,
        walker: 0,
        stock_overlay: None,
        employees: 0,
        appeal: appeal_of(TYPE_ELITE_HOUSE)?,
        fire_risk: rules(TYPE_ELITE_HOUSE)?.fire_risk,
        damage_risk: rules(TYPE_ELITE_HOUSE)?.damage_risk,
    });
    // Palais : la disposition est celle de `model/palace.txt` — deux moitiés 4 × 4
    // l'une derrière l'autre, bordées de dalles sur trois côtés et de deux dalles
    // d'angle (6 × 9 cases).
    let mut palace_parts = vec![
        Part { dx: 1, dy: 1, size: 4, image: PALACE_BACK_IMAGE, crop: false, bay: None },
        Part { dx: 1, dy: 5, size: 4, image: PALACE_FRONT_IMAGE, crop: false, bay: None },
    ];
    let border = (0..6).map(|dx| (dx, 0)).chain((1..9).flat_map(|dy| [(0, dy), (5, dy)]));
    palace_parts.extend(border.map(|(dx, dy)| {
        let image = if dy == 0 && (dx == 0 || dx == 5) { PALACE_CORNER_IMAGE } else { PALACE_TILE_IMAGE };
        Part { dx, dy, size: 1, image, crop: false, bay: None }
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
    let walkers = WalkerImages { immigrant: group(GROUP_IMMIGRANT), elite_immigrant: group(GROUP_ELITE_IMMIGRANT), carrier: group(GROUP_CARRIER), buyer: group(GROUP_BUYER) };
    // Les niveaux de logement se suivent dans les noms de bâtiments : modestes, puis luxueux.
    let level_names: Vec<String> = (0..13).map(|level| text.get(TEXT_BUILDINGS, TYPE_HOUSE + level)).collect();
    let good_names = GOOD_TEXTS.map(|index| text.get(23, index));
    let mut world = World::new(map, kinds, model.houses.clone(), walkers, settings.as_ref().map_or(STARTING_MONEY, |s| s.funds));
    let start_year = settings.as_ref().map_or(0, |s| s.start_year);
    // L'aventure dit ce que la cité peut produire : les exploitations et ateliers des
    // autres ressources ne sont pas proposés. La démonstration, elle, a droit à tout.
    // Tant que le moteur ne sait nourrir une cité que par ses exploitations et ses
    // laiteries, ses pavillons de chasse et ses pêcheries, la restriction ne s'applique qu'aux aventures qui en
    // permettent au moins une : les autres resteraient sans vivres.
    let farmable = |r: &Vec<u8>| FARM_RESOURCES.iter().any(|crop| r.contains(crop)) || [RESOURCE_FISH, RESOURCE_MEAT, RESOURCE_CHEESE].iter().any(|food| r.contains(food));
    let resources = settings.as_ref().map(|s| s.resources.clone()).filter(|r| farmable(r) && !demo);
    if let Some(resources) = resources {
        let has = |resource: u8| resources.contains(&resource);
        world.locked = world
            .kinds
            .iter()
            .map(|kind| match kind.function {
                Function::Farm => !kind.parts.get(1).is_some_and(|field| FARMS.iter().zip(FARM_RESOURCES).any(|(farm, resource)| farm.1 == field.image && has(resource))),
                Function::Producer(Good::Fleece) => !has(RESOURCE_FLEECE),
                Function::Fishery => !has(RESOURCE_FISH),
                // Laiterie et pavillon de chasse se distinguent par leur image.
                Function::Producer(Good::Food) => {
                    let hunting = kind.parts.first().is_some_and(|part| part.image == HUNTING_LODGE_IMAGE);
                    !has(if hunting { RESOURCE_MEAT } else { RESOURCE_CHEESE })
                }
                Function::Supplier(Raw::Crops) => !has(RESOURCE_OLIVES) && !has(RESOURCE_GRAPES),
                Function::Workshop(Good::Oil) => !has(RESOURCE_OLIVES),
                Function::Workshop(Good::Wine) => !has(RESOURCE_GRAPES),
                Function::Supplier(Raw::Bronze) => !has(RESOURCE_BRONZE),
                _ => false,
            })
            .collect();
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
        titled.unwrap_or_else(|| letters.next().map(|first| first.to_uppercase().chain(letters).collect()).unwrap_or_default())
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
            destruction: Catalog::load(&data.join("data/destruction.sg3"))?,
            interface: Catalog::load(&data.join("data/zeus_interface.sg3"))?,
            ink: (255, 255, 255),
            ground: None,
            sprites: HashMap::new(),
        };

        // Départ au centre de la zone jouable, ou sur la scène de démonstration.
        let (start, mut demo_plan) = match demo.then(|| demo_scene(&mut world, road_cost)).flatten() {
            Some((start, plan)) => (start, plan),
            // La vue s'ouvre sur le terrain constructible le plus proche du centre : le
            // centre lui-même tombe parfois sur l'eau ou sur des falaises.
            None => {
                let centre = GRID as i32 / 2;
                let mut sites: Vec<(i32, i32)> = (0..GRID as i32).flat_map(|y| (0..GRID as i32).map(move |x| (x, y))).collect();
                sites.sort_by_key(|&(x, y)| (x - centre).abs() + (y - centre).abs());
                let open = sites.into_iter().find(|&(x, y)| world.can_build(x - START_CLEARING / 2, y - START_CLEARING / 2, START_CLEARING as usize));
                (open.unwrap_or((centre, centre)), DemoPlan::new())
            }
        };
        let (start_x, start_y) = tile_origin(start.0, start.1);
        let mut view = View { cx: start_x, cy: start_y, zoom: 1.0 };
        // Échelle de l'interface : 1 jusqu'à 1440 lignes, 2 en 4K, etc. Fixée à la
        // première image, avec le zoom de départ.
        let mut ui = 0.0f32;
        let mut fullscreen = false;
        let mut pointer = (0.0f32, 0.0f32);
        let mut hovered = (-1, -1);
        let mut event = sdl::Event::new();
        let mut running = true;
        // (profondeur, couche, coin haut-gauche en pixels de monde, catalogue, image)
        let mut draw_list: Vec<(i32, i32, f32, f32, Source, usize)> = Vec::new();
        // Emplacement de la liste des outils : (x, y de la première ligne, largeur, hauteur de ligne).
        // Action de chaque ligne du menu, telle qu'affichée à la dernière image.
        let mut menu_rows: Vec<((f32, f32, f32, f32), MenuRow)> = Vec::new();
        let mut open_category: Option<usize> = None;
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
        // Images dessinées, et instant de la première, pour `--mesure`.
        let (mut frames, mut measure_start, mut slowest, mut previous_frame) = (0u32, 0u64, 0u64, 0u64);
        // Récit d'ouverture, affiché jusqu'au premier clic ou à la première touche. Pas
        // en capture, sauf si le script commence par « intro ».
        let mut intro = briefing(data, map_path, settings.clone()).filter(|_| capture.is_none() || script.split_whitespace().next() == Some("intro"));
        let mut clock = sdl::SDL_GetTicks();
        let mut paused = false;
        let mut speed = NORMAL_SPEED;
        // Sol nu le plus courant de la carte, pour remplacer les tuiles sans image propre.
        let plain_ground = {
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
        };
        graphics.ground = {
            let catalog = graphics.catalog(plain_ground.0);
            let fits = catalog.images.get(plain_ground.1).is_some_and(|i| (i.width, i.height) == (TILE_WIDTH, TILE_HEIGHT));
            fits.then(|| catalog.decode(plain_ground.1).ok()).flatten()
        };
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
            let houses: Vec<_> = world.buildings.iter().flatten().filter_map(|b| b.house.as_ref()).collect();
            let served: Vec<String> =
                SERVICES.iter().map(|&s| format!("{s:?} {}", houses.iter().filter(|h| h.has(s, world.ticks)).count())).collect();
            let mut by_level = [0; 13];
            houses.iter().for_each(|h| by_level[h.level.min(12)] += 1);
            let levels = [format!("par niveau {by_level:?}, réserves totales {:?}", GOODS.map(|g| houses.iter().map(|h| h.goods[g as usize]).sum::<i32>()))];
            let stocks: Vec<String> = world.buildings.iter().flatten().filter(|b| b.house.is_none()).filter(|b| world.kinds[b.kind].function != Function::None).map(|b| format!("{}={}{}{}", world.kinds[b.kind].name, b.stock, if b.goods.iter().any(|&g| g > 0) { format!("{:?}", b.goods) } else { String::new() }, if b.staffed { "" } else { " (arrêt)" })).collect();
            let appeals: Vec<i32> = world.buildings.iter().flatten().filter(|b| b.house.is_some()).map(|b| world.appeal(b.x as i32, b.y as i32)).collect();
            println!("maisons : {} | attrait {appeals:?} | stocks : {}", levels.join(", "), stocks.join(", "));
            println!("bâtiments debout : {} | sauvegarde : {} cases en ruine, {} en feu", world.buildings.iter().flatten().count(), saved.matches("\nruine ").count(), saved.matches("\nfeu ").count());
            println!("drachmes {} | dernier mois : impôts {}, salaires {} | employés {} demandés, {} disponibles", world.money, world.last_month.0, world.last_month.1, world.workers_needed(), world.workers_available());
            println!(
                "après {warmup} pas : population {}, {} maisons desservies [{}], {} personnages {nearest}, scène en {start:?}",
                world.population(),
                houses.len(),
                served.join(", "),
                world.figures.len()
            );
        }

        let mut ambience = match capture.is_none() || !script.is_empty() {
            true => Ambience::open(&data.join("audio")).inspect_err(|e| eprintln!("pas d'ambiance : {e}")).ok(),
            false => None,
        };
        let (mut ambience_at, mut ambience_seed) = (sdl::SDL_GetTicks(), sdl::SDL_GetTicks() as u32 | 1);
        while running {
            if let Some(music) = music.as_mut() {
                music.update();
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
                    // Chiffre : la touche à cet emplacement du clavier, quelle que soit la disposition.
                    ("k", &[digit]) if (1.0..=9.0).contains(&digit) => Some(sdl::Event::key_down(sdl::SCANCODE_1 + digit as u32 - 1, 0)),
                    ("k", _) => match argument {
                        "x" => Some(sdl::Event::key_down(0, sdl::KEY_X)),
                        "esc" => Some(sdl::Event::key_down(0, sdl::KEY_ESCAPE)),
                        "espace" => Some(sdl::Event::key_down(0, sdl::KEY_SPACE)),
                        "f5" => Some(sdl::Event::key_down(0, sdl::KEY_F5)),
                        "f9" => Some(sdl::Event::key_down(0, sdl::KEY_F9)),
                        "h" => Some(sdl::Event::key_down(0, sdl::KEY_H)),
                        _ => None,
                    },
                    // Pause en temps réel, pour laisser vivre ce qui en dépend (son).
                    ("p", &[ms]) => {
                        sdl::SDL_Delay(ms as u32);
                        None
                    }
                    ("intro", _) => None,
                    ("t", &[ticks]) => {
                        for _ in 0..ticks as u32 {
                            world.tick();
                            advance_demo(&mut world, &mut demo_plan);
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
            if let Some(disaster) = world.disasters.pop() {
                let what = if disaster.fire { "Incendie" } else { "Effondrement" };
                notice = (format!("{what} : {}", world.kinds[disaster.kind].name), now);
                world.disasters.clear();
            }

            while sdl::SDL_PollEvent(&mut event) {
                // Sur un écran mis à l'échelle, la souris est en points et le rendu en pixels.
                sdl::SDL_ConvertEventToRenderCoordinates(renderer, &mut event);
                // Le récit d'ouverture se referme au premier clic ou à la première touche.
                if intro.is_some() && matches!(event.kind(), sdl::EVENT_KEY_DOWN | sdl::EVENT_MOUSE_BUTTON_DOWN) {
                    intro = None;
                    continue;
                }
                match event.kind() {
                    sdl::EVENT_QUIT => running = false,
                    sdl::EVENT_KEY_DOWN => match event.key() {
                        sdl::KEY_ESCAPE if tool != Tool::None => {
                            tool = Tool::None;
                            hovered = (-1, -1);
                        }
                        sdl::KEY_ESCAPE if open_category.is_some() => open_category = None,
                        sdl::KEY_ESCAPE | sdl::KEY_Q => running = false,
                        sdl::KEY_SPACE => paused = !paused,
                        sdl::KEY_F5 => {
                            notice = match fs::write(&save_file, world.save()) {
                                Ok(()) => (format!("Partie enregistrée : {}", save_file.display()), now),
                                Err(e) => (format!("Enregistrement impossible : {e}"), now),
                            };
                        }
                        sdl::KEY_F9 => {
                            let loaded = fs::read_to_string(&save_file).map_err(|e| e.to_string()).and_then(|t| world.load(&t));
                            notice = match loaded {
                                Ok(()) => ("Partie chargée".to_string(), now),
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
                                None if slot < CATEGORY_NAMES.len() => open_category = Some(slot),
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
                        }
                        // Glisser avec le bouton gauche : parcourir la mini-carte, ou tracer une
                        // route, ou démolir en continu.
                        if let (true, Some(target)) = (buttons & sdl::BUTTON_LEFT_MASK != 0, minimap_at.target(pointer.0 / ui, pointer.1 / ui)) {
                            (view.cx, view.cy) = target;
                        } else if buttons & sdl::BUTTON_LEFT_MASK != 0 {
                            let (wx, wy) = view.to_world(pointer.0, pointer.1, w, h);
                            let (x, y) = tile_at(wx, wy);
                            match tool {
                                Tool::Road => drop(world.place_road(x, y, road_cost)),
                                Tool::Demolish => drop(world.demolish(x, y)),
                                _ => {}
                            }
                            hovered = (-1, -1);
                        }
                    }
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
                                continue;
                            }
                            Some(MenuRow::Pick(picked)) => {
                                tool = picked;
                                hovered = (-1, -1);
                                continue;
                            }
                            None => {}
                        }
                        // Le bandeau et le panneau latéral ne laissent pas passer le clic.
                        if iy < TOP_BAR_HEIGHT || ix >= w / ui - SIDEBAR_WIDTH {
                            continue;
                        }
                        let (wx, wy) = view.to_world(mx, my, w, h);
                        let (x, y) = tile_at(wx, wy);
                        match tool {
                            Tool::None => {}
                            Tool::Road => drop(world.place_road(x, y, road_cost)),
                            Tool::Demolish => drop(world.demolish(x, y)),
                            Tool::Build(kind) => drop(world.place_building(kind, x, y)),
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
            let under = tile_at(wx, wy);
            if under != hovered {
                hovered = under;
                let held = match tool {
                    Tool::None => String::new(),
                    Tool::Road => format!(" — {road_name} ({road_cost})"),
                    Tool::Demolish => " — Démolir".to_string(),
                    Tool::Build(kind) => format!(" — {} ({})", world.kinds[kind].name, world.kinds[kind].cost),
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
            for sum in first_sum.max(0)..=last_sum.min(2 * GRID as i32 - 2) {
                for diff in first_diff..=last_diff {
                    if (sum + diff) % 2 != 0 {
                        continue;
                    }
                    let tile = ((sum + diff) / 2, (sum - diff) / 2);
                    if !in_grid(tile) {
                        continue;
                    }
                    if let Some(draw) = world.draw(tile.0 as usize, tile.1 as usize) {
                        // Un objet de n × n cases se dessine au rang de son coin le plus proche.
                        let n = graphics.size(draw.source, draw.image);
                        let height = graphics.catalog(draw.source).images.get(draw.image).map_or(0, |i| i.height) as f32;
                        // L'image part du bord gauche de sa case ; son bas est celui de l'emprise.
                        let (ox, oy) = tile_origin(tile.0, tile.1);
                        let top = oy + (n as f32 + 1.0) * HALF_HEIGHT - height;
                        if let Some((source, image)) = draw.ground {
                            let ground_height = graphics.catalog(source).images.get(image).map_or(0, |i| i.height) as f32;
                            draw_list.push((sum, tile.0 * 3, ox, oy + 2.0 * HALF_HEIGHT - ground_height, source, image));
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
                        draw_list.push((sum + n - 1, tile.0 * 3 + 1, ox, top, draw.source, image));
                        // Incendie : une flamme animée, le pied au centre de la case, décalée
                        // d'une case à l'autre pour ne pas battre à l'unisson.
                        if draw.burning {
                            let phase = (tile.0 * 7 + tile.1 * 13) as usize;
                            let frame = FIRE_FIRST_IMAGE + ((world.ticks / FIRE_FRAME_TICKS) as usize + phase) % FIRE_FRAMES;
                            if let Some(flame) = graphics.destruction.images.get(frame) {
                                let (fx, fy) = (ox + HALF_WIDTH - flame.width as f32 / 2.0, oy + 1.5 * HALF_HEIGHT - flame.height as f32);
                                draw_list.push((sum, tile.0 * 3 + 2, fx, fy, Source::Destruction, frame));
                            }
                        }
                        // Bâtiment animé : l'image d'animation se pose sur la base, à l'ancrage indiqué.
                        let base = &graphics.catalog(draw.source).images[draw.image];
                        if let Some(overlay) = draw.overlay {
                            let (ax, ay) = (ox + base.offset_x as f32, top + base.offset_y as f32);
                            draw_list.push((sum + n - 1, tile.0 * 3 + 2, ax, ay, draw.source, overlay));
                        } else if draw.source == Source::General && base.animation_frames > 0 {
                            let frame = (world.ticks / ANIMATION_TICKS) as usize % base.animation_frames;
                            let (ax, ay) = (ox + base.offset_x as f32, top + base.offset_y as f32);
                            draw_list.push((sum + n - 1, tile.0 * 3 + 2, ax, ay, draw.source, draw.image + 1 + frame));
                        }
                    }
                }
            }
            // Personnages : ancrés au centre de leur case, par-dessus le sol de même rang.
            for figure in &world.figures {
                let image = figure.image();
                let Some(info) = graphics.figures.images.get(image) else { continue };
                let wx = (figure.x - figure.y) * HALF_WIDTH + HALF_WIDTH - 1.0 - info.offset_x as f32;
                let wy = (figure.x + figure.y) * HALF_HEIGHT + HALF_HEIGHT - info.offset_y as f32;
                if wx < right && wx + info.width as f32 > left && wy < bottom && wy + info.height as f32 > top {
                    draw_list.push(((figure.x + figure.y).ceil() as i32, i32::MAX, wx, wy, Source::Figures, image));
                }
            }
            draw_list.sort_unstable_by_key(|&(depth, layer, ..)| (depth, layer));

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
            for &(_, _, wx, wy, source, image) in &draw_list {
                let Some(sprite) = graphics.sprite(source, image) else { continue };
                let (sx, sy) = view.to_screen(wx, wy, w, h);
                let target = sdl::FRect {
                    x: sx.floor(),
                    y: sy.floor(),
                    w: (sprite.width * view.zoom).ceil(),
                    h: (sprite.height * view.zoom).ceil(),
                };
                sdl::SDL_SetTextureScaleMode(sprite.texture, scale_mode);
                sdl::SDL_RenderTexture(renderer, sprite.texture, null(), &target);
            }

            // Aperçu de ce que l'outil poserait : translucide, rougi si c'est impossible.
            if let (Tool::Build(kind), true) = (tool, in_grid(under)) {
                let allowed = world.can_place(kind, under.0, under.1) && world.money >= world.kinds[kind].cost;
                for part in world.kinds[kind].parts.clone() {
                    let Some(sprite) = graphics.sprite(Source::General, part.image) else { continue };
                    let (px, py) = (under.0 + part.dx as i32, under.1 + part.dy as i32);
                    let (ox, oy) = tile_origin(px, py + part.size as i32 - 1);
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
            } else if in_grid(under) {
                outline_tile(renderer, &view, under, w, h);
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
            // Le titre ne s'affiche que s'il tient à gauche des chiffres : dans une
            // fenêtre étroite, ce sont eux qui comptent.
            if 12.0 + graphics.text_width(FONT_CHROME, &display_name) + 24.0 <= side - status_width - 16.0 {
                graphics.draw_text(FONT_CHROME, &display_name, 12.0, (TOP_BAR_HEIGHT - line) / 2.0);
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
            // « Dégager » occupe l'emplacement d'onglet suivant.
            let clear_slot = (side + TAB_LEFT, TAB_TOP + TAB_STEP * CATEGORY_TABS.len() as f32, TAB_WIDTH, TAB_HEIGHT);
            if tool == Tool::Demolish || inside(clear_slot) {
                sdl::SDL_SetRenderDrawColor(renderer, 200, 160, 40, 140);
                sdl::SDL_RenderFillRect(renderer, &sdl::FRect { x: clear_slot.0, y: clear_slot.1, w: clear_slot.2, h: clear_slot.3 });
            }
            graphics.draw_text(FONT_CHROME, "X", clear_slot.0 + 14.0, clear_slot.1 + (TAB_HEIGHT - line) / 2.0);
            menu_rows.push((clear_slot, MenuRow::Pick(Tool::Demolish)));
            // Nom de la catégorie ouverte ou survolée, dans le haut du panneau.
            let named = open_category.or_else(|| menu_rows.iter().find_map(|&(slot, action)| match action {
                MenuRow::Category(index) if inside(slot) => Some(index),
                _ => None,
            }));
            if named.is_none() && (tool == Tool::Demolish || inside(clear_slot)) {
                graphics.draw_text(FONT_CHROME, &demolish_name, side + PANEL_LEFT, TAB_TOP + 6.0);
            }
            if let Some(index) = named {
                let mut top = TAB_TOP + 6.0;
                for word in category_names[index].split(' ') {
                    graphics.draw_text(FONT_CHROME, word, side + PANEL_LEFT, top);
                    top += row;
                }
            }
            graphics.ink = (255, 255, 255);
            if let Some(index) = open_category {
                let tools = category_tools(&world, index);
                let labels: Vec<String> = tools
                    .iter()
                    .enumerate()
                    .map(|(slot, picked)| {
                        let key = if slot < 9 { (slot + 1).to_string() } else { "-".to_string() };
                        match *picked {
                            Tool::Build(kind) => format!("{key}  {}  ({})", world.kinds[kind].name, world.kinds[kind].cost),
                            _ => format!("{key}  {road_name}  ({road_cost})"),
                        }
                    })
                    .collect();
                let width = labels.iter().map(|label| graphics.text_width(FONT_TEXT, label)).fold(0.0, f32::max) + 32.0;
                let height = row * labels.len() as f32 + 10.0;
                // À hauteur de l'onglet, sans sortir de l'écran par le bas.
                let top = (TAB_TOP + TAB_STEP * index as f32).min(h - height - 8.0).max(TOP_BAR_HEIGHT + 4.0);
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
                let (mw, mh) = minimap_at.size();
                (minimap_at.x, minimap_at.y) = (12.0, h - mh - 12.0);
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

            // Panneau d'information sur le bâtiment survolé.
            if let Some(building) = world.building_at(under.0, under.1) {
                let mut lines = vec![world.kinds[building.kind].name.clone()];
                let employees = world.kinds[building.kind].employees;
                if employees > 0 {
                    lines.push(format!("{} : {}", text.get(8, 13), if building.staffed { employees } else { 0 }));
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
                    for good in GOODS {
                        lines.push(format!("{} : {}", good_names[good as usize], house.goods[good as usize]));
                    }
                    for service in SERVICES {
                        let label = match service {
                            Service::Water => text.get(TEXT_BUILDINGS, 74),
                            Service::Maintenance => text.get(TEXT_BUILDINGS, 121),
                            Service::Security => text.get(TEXT_BUILDINGS, 124),
                            Service::Health => text.get(TEXT_BUILDINGS, 76),
                            Service::Tax => text.get(TEXT_BUILDINGS, 122),
                            Service::Philosophy => text.get(TEXT_BUILDINGS, 81),
                            Service::Athletics => text.get(TEXT_BUILDINGS, 79),
                            Service::Drama => text.get(TEXT_BUILDINGS, 82),
                        };
                        lines.push(format!("{label} : {}", text.get(18, if house.has(service, world.ticks) { 1 } else { 0 })));
                    }
                }
                let width = lines.iter().map(|l| graphics.text_width(FONT_TEXT, l)).fold(0.0, f32::max) + 32.0;
                let height = row * lines.len() as f32 + 12.0;
                let (px, py) = (12.0, TOP_BAR_HEIGHT + 12.0);
                graphics.panel(px, py, width, height);
                for (i, l) in lines.iter().enumerate() {
                    graphics.draw_text(FONT_TEXT, l, px + 16.0, py + 8.0 + row * i as f32);
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
        drop(music);
        drop(ambience);
        sdl::SDL_Quit();
    }
    Ok(())
}

fn in_grid((x, y): (i32, i32)) -> bool {
    (0..GRID as i32).contains(&x) && (0..GRID as i32).contains(&y)
}

/// Trace le losange d'une case.
unsafe fn outline_tile(renderer: sdl::Renderer, view: &View, (x, y): (i32, i32), w: f32, h: f32) {
    let (ox, oy) = tile_origin(x, y);
    let corner = |dx: f32, dy: f32| view.to_screen(ox + dx, oy + dy, w, h);
    let points = [
        corner(HALF_WIDTH, 0.0),
        corner(2.0 * HALF_WIDTH, HALF_HEIGHT),
        corner(HALF_WIDTH, 2.0 * HALF_HEIGHT),
        corner(0.0, HALF_HEIGHT),
    ];
    unsafe {
        sdl::SDL_SetRenderDrawColor(renderer, 255, 255, 0, 255);
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
