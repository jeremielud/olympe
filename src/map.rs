//! Cartes (`.map`) : une suite de blocs décrits par une table rangée dans le fichier
//! lui-même. Chaque bloc correspond à une variable du jeu d'origine, dont la table
//! conserve l'adresse mémoire.

use crate::pkware;

/// Côté de toutes les grilles ; la zone jouable est un losange inscrit dedans.
pub const GRID: usize = 228;

const TABLE_OFFSET: usize = 12;
const ENTRY_SIZE: usize = 20;
/// Longueur compressée spéciale : bloc incompressible, stocké tel quel.
const STORED: u32 = 0x8000_0000;

#[derive(Clone)]
pub struct Block {
    /// Adresse de la variable dans `Zeus.exe` au moment de l'enregistrement.
    pub address: u32,
    pub element_size: usize,
    pub count: usize,
    pub data: Vec<u8>,
}

#[derive(Clone)]
pub struct Map {
    pub version: u32,
    pub blocks: Vec<Block>,
}

/// Rangs des grilles identifiées parmi les blocs.
const IMAGES: usize = 4;
const EDGES: usize = 5;
const TERRAIN: usize = 6;
/// Réglages du scénario ; à l'octet 908, deux points (x, y en 16 bits) : entrée et
/// sortie des voyageurs.
const SCENARIO: usize = 14;
const ENTRY_POINTS_OFFSET: usize = 908;
/// Points de catastrophe de l'éditeur : huit abscisses (16 bits) en 916, huit
/// ordonnées en 932 (lus par Zeus.exe en 0x51eff0, rangés en 0x902cbc et 0x902ccc).
const DISASTER_X_OFFSET: usize = 916;
const DISASTER_Y_OFFSET: usize = 932;
const DISASTER_POINTS: usize = 8;
/// Points de poissons de l'éditeur (« Sea Animal Points ») : huit abscisses (16 bits)
/// en 668, huit ordonnées en 684, -1 pour un point vide. Les bancs de poissons du jeu
/// s'y tiennent.
const FISH_X_OFFSET: usize = 668;
const FISH_Y_OFFSET: usize = 684;
const FISH_POINTS: usize = 8;
const INVASION_X_OFFSET: usize = 744;
const INVASION_Y_OFFSET: usize = 776;
const LAND_INVASION_POINTS: usize = 8;

/// Cités du monde : un bloc de 22 enregistrements, de 476 octets dans les cartes de
/// Zeus et de 572 dans celles de Poséidon (la forme qu'ils ont en mémoire). Les
/// cartes de colonie n'en ont pas : elles partagent le monde de la cité mère.
const CITIES: usize = 22;
const CITY_SIZES: [usize; 2] = [476, 572];
/// Dans un enregistrement : active (octet 0), genre (octet 2 : 0 la cité du joueur,
/// 1 une colonie à fonder, 2 une cité étrangère), nom (octet 3, groupe de textes 21),
/// statut (entier en 212 : 0 alliée, 1 rivale, 2 vassale, 3 colonie, comme le groupe
/// de textes 253) et force militaire (octet 220, de 1 à 6 boucliers).
/// Prix des marchandises : un entier par ressource.
const PRICES: usize = 36;
const CITY_KIND: usize = 2;
const CITY_NAME: usize = 3;
const CITY_STATUS: usize = 212;
/// Chef de la cité : rang dans le groupe de textes 139 (0 pour la cité du joueur).
const CITY_LEADER: usize = 208;
const CITY_TITLE: usize = 505;
const CITY_LEADER_TITLE: usize = 537;
const CITY_MILITARY: usize = 220;
/// Richesse (entier en 216, de 1 à 5) : `Zeus.exe` en tire les drachmes d'un raid.
const CITY_WEALTH: usize = 216;
/// Nation de l'armée (octet 204 : Troie 1, Sardes 2, Cymé 4…).
const CITY_ARMY: usize = 204;
const CITY_TRIBUTE_RECEIVED: usize = 228;
const CITY_TRIBUTE_PAID: usize = 232;
/// Attitude envers le joueur (entier en 340 : de 50 à 85 dans les aventures, 80
/// pour les colonies à fonder) ; Olympe en fait la faveur de la cité.
const CITY_FAVOR: usize = 340;
/// Commerce : les marchandises vendues (octets à partir de 188) et achetées (à
/// partir de 196), quatre au plus en tout, et la quantité annuelle de chacune (octet
/// 96 + numéro de la marchandise). Les cartes de Zeus gardent l'ancienne
/// numérotation des marchandises.
const CITY_SELLS: usize = 188;
const CITY_BUYS: usize = 196;
const CITY_QUANTITIES: usize = 96;
const TRADE_SLOTS: usize = 4;
pub const FOREIGN: u8 = 2;
pub const ALLY: i32 = 0;
pub const RIVAL: i32 = 1;
pub const VASSAL: i32 = 2;
pub const COLONY: i32 = 3;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct City {
    pub active: bool,
    pub kind: u8,
    pub name: u8,
    pub status: i32,
    pub military: u8,
    /// Route commerciale ouverte par le joueur.
    pub trade: bool,
    /// Faveur de la cité envers le joueur, de 0 à 100.
    pub favor: i32,
    /// Ce que la cité vend et achète : (ressource, quantité par an).
    pub sells: Vec<(u8, i32)>,
    pub buys: Vec<(u8, i32)>,
    /// Drachmes gagnées et dépensées avec elle depuis le début de la partie.
    pub sold: i32,
    pub bought: i32,
    /// Nation de son armée, dans l'ordre de l'éditeur : 0 grecque, 1 troyenne,
    /// 2 perse, 3 centaures, 4 amazones, 5 égyptienne, 6 maya, 7 phénicienne,
    /// 8 océanides, 9 atlante.
    pub army: u8,
    /// Tributs réglés dans l'éditeur : ce que la cité recevrait du joueur (« Rec »,
    /// entier en 228) et ce qu'elle lui paie (« Pay », entier en 232), en drachmes.
    pub tribute_received: i32,
    pub tribute_paid: i32,
    /// Richesse, de 1 à 5.
    pub wealth: u8,
    /// Force militaire du moment (entier en 0x1e8 de la cité dans `Zeus.exe`) :
    /// négative tant que la partie ne l'a pas fixée.
    pub strength: i32,
}

/// Route du monde entre deux cités (bloc de 232 enregistrements de 324 octets, en
/// 0x9034f8 dans `Zeus.exe`) : rangs des deux cités, longueur et allure.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Route {
    pub from: i16,
    pub to: i16,
    pub length: i32,
    pub pace: u8,
}

const ROUTE_SIZE: usize = 324;
const ROUTES: usize = 232;
const ROUTE_LENGTH: usize = 0x134;
const ROUTE_CITIES: usize = 0x13c;
const ROUTE_ACTIVE: usize = 0x142;

/// Catalogue d'où provient une image de terrain.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Source {
    /// `zeus_terrain.sg3`
    Terrain,
    /// `poseidon_loaded.sg3`
    Poseidon,
    /// `zeus_general.sg3` (bâtiments) ; jamais référencé par une carte vierge.
    General,
    /// `zeus_fonts.sg3` (polices de l'interface).
    Fonts,
    /// `sprmain.sg3` (personnages).
    Figures,
    /// `destruction.sg3` (flammes, poussière, fumée).
    Destruction,
    /// `zeus_interface.sg3` (bandeau, panneau latéral, boutons).
    Interface,
    /// `zeus_sanctuary1.sg3` à `zeus_sanctuary3.sg3` : temples des sanctuaires, en
    /// chantier (0 et 1) puis achevés (2).
    Sanctuary(u8),
    /// `zeus_godNN.sg3` : grande statue d'un dieu, par rang du dieu dans le jeu.
    God(u8),
    /// Catalogue d'un monstre (numéro du groupe de textes 87).
    Monster(u8),
    /// Catalogue d'une armée ennemie (`zeus_greek.sg3`…).
    Army(u8),
    /// Catalogue d'un héros (numéro du groupe de textes 163).
    Hero(u8),
    /// Catalogue d'un dieu en personne (numéro du groupe de textes 157).
    Deity(u8),
}

/// Dans tous les catalogues, les 200 premières images sont communes (« système ») ;
/// la numérotation du jeu commence donc à la 201e.
const FIRST_OWN_IMAGE: u32 = 201;
/// Base des identifiants du terrain : elle a changé avec Poséidon (cartes ≥ 318).
const TERRAIN_BASE_ZEUS: u32 = 0xC000;
const TERRAIN_BASE_POSEIDON: u32 = 0x14000;
const POSEIDON_BASE: u32 = 0xD0000;
const POSEIDON_MAP_VERSION: u32 = 318;
/// Taille maximale d'un catalogue.
const CATALOG_SPAN: u32 = 10_000;

/// Bit de la grille des bords : la case porte l'image de son bâtiment.
pub const EDGE_DRAW_TILE: u8 = 0x40;

impl Map {
    pub fn parse(file: &[u8]) -> Result<Map, String> {
        Self::parse_prefix(file).map(|(map, _)| map)
    }

    /// Lit une carte au début de `file` et rend aussi le nombre d'octets qu'elle occupe.
    pub fn parse_prefix(file: &[u8]) -> Result<(Map, usize), String> {
        if file.len() < TABLE_OFFSET || &file[..4] != b"MAPS" {
            return Err("en-tête MAPS absent".into());
        }
        let (version, count) = (le32(file, 4), le32(file, 8) as usize);
        let mut blocks = Vec::new();
        let mut pos = 0;
        for i in 0..count {
            let entry = file.get(TABLE_OFFSET + i * ENTRY_SIZE..).filter(|e| e.len() >= ENTRY_SIZE).ok_or("table des blocs tronquée")?;
            let (compressed, address) = (le32(entry, 0) != 0, le32(entry, 4));
            let (element_size, count) = (le32(entry, 8) as usize, le32(entry, 12) as usize);
            let size = element_size * count;
            let slice = |from: usize, len: usize| file.get(from..from + len).ok_or(format!("bloc {i} hors du fichier"));
            let data = if !compressed {
                pos += size;
                slice(pos - size, size)?.to_vec()
            } else {
                let packed = le32(slice(pos, 4)?, 0);
                pos += 4;
                if packed == STORED {
                    pos += size;
                    slice(pos - size, size)?.to_vec()
                } else {
                    pos += packed as usize;
                    pkware::explode(slice(pos - packed as usize, packed as usize)?).map_err(|e| format!("bloc {i} : {e}"))?
                }
            };
            if data.len() != size {
                return Err(format!("bloc {i} : {} octets au lieu de {size}", data.len()));
            }
            blocks.push(Block { address, element_size, count, data });
        }
        if blocks.len() <= TERRAIN || blocks[IMAGES].data.len() != GRID * GRID * 4 {
            return Err("grilles de carte absentes".into());
        }
        Ok((Map { version, blocks }, pos))
    }

    /// Identifiant d'image de la case, 0 hors de la carte.
    pub fn image(&self, x: usize, y: usize) -> u32 {
        le32(&self.blocks[IMAGES].data, (y * GRID + x) * 4)
    }

    /// Traduit un identifiant d'image de la grille en (catalogue, numéro d'image).
    pub fn resolve(&self, id: u32) -> Option<(Source, usize)> {
        let terrain = if self.version >= POSEIDON_MAP_VERSION { TERRAIN_BASE_POSEIDON } else { TERRAIN_BASE_ZEUS };
        let local = |base: u32| (base..base + CATALOG_SPAN).contains(&id).then(|| (id - base + FIRST_OWN_IMAGE) as usize);
        local(terrain).map(|i| (Source::Terrain, i)).or_else(|| local(POSEIDON_BASE).map(|i| (Source::Poseidon, i)))
    }

    pub fn edge(&self, x: usize, y: usize) -> u8 {
        self.blocks[EDGES].data[y * GRID + x]
    }

    /// Points d'entrée puis de sortie des voyageurs, quand ils tombent dans la grille.
    /// Sur certaines cartes ils sont hors de la zone jouable : à l'appelant de vérifier.
    pub fn entry_points(&self) -> Vec<(usize, usize)> {
        let Some(data) = self.blocks.get(SCENARIO).and_then(|b| b.data.get(ENTRY_POINTS_OFFSET..ENTRY_POINTS_OFFSET + 8)) else {
            return Vec::new();
        };
        let value = |i: usize| i16::from_le_bytes([data[i * 2], data[i * 2 + 1]]);
        let in_grid = |v: i16| (0..GRID as i16).contains(&v);
        [(value(0), value(1)), (value(2), value(3))]
            .into_iter()
            .filter(|&(x, y)| in_grid(x) && in_grid(y))
            .map(|(x, y)| (x as usize, y as usize))
            .collect()
    }

    /// Point d'invasion `n` (de 1 à 16 : 1 à 8 par terre, 9 à 16 par mer), s'il tombe
    /// dans la grille.
    pub fn invasion_point(&self, n: usize) -> Option<(usize, usize)> {
        let data = &self.blocks.get(SCENARIO)?.data;
        let i = n.checked_sub(1).filter(|&i| i < 2 * LAND_INVASION_POINTS)?;
        let value = |at: usize| data.get(at..at + 2).map(|b| i16::from_le_bytes([b[0], b[1]]));
        let (x, y) = (value(INVASION_X_OFFSET + 2 * i)?, value(INVASION_Y_OFFSET + 2 * i)?);
        let in_grid = |v: i16| (0..GRID as i16).contains(&v);
        (in_grid(x) && in_grid(y)).then_some((x as usize, y as usize))
    }

    /// Points d'invasion terrestres (les huit premiers des seize de l'éditeur : x en
    /// 744, y en 776 du bloc du scénario, rangés par Zeus.exe en 0x902c10 et 0x902c30)
    /// qui tombent dans la grille.
    pub fn invasion_points(&self) -> Vec<(usize, usize)> {
        let Some(data) = self.blocks.get(SCENARIO).map(|b| &b.data) else { return Vec::new() };
        let value = |at: usize| data.get(at..at + 2).map_or(-1, |b| i16::from_le_bytes([b[0], b[1]]));
        let in_grid = |v: i16| (0..GRID as i16).contains(&v);
        (0..LAND_INVASION_POINTS)
            .map(|i| (value(INVASION_X_OFFSET + 2 * i), value(INVASION_Y_OFFSET + 2 * i)))
            .filter(|&(x, y)| in_grid(x) && in_grid(y))
            .map(|(x, y)| (x as usize, y as usize))
            .collect()
    }

    /// Point de catastrophe `n` (de 1 à 8), s'il tombe dans la grille.
    pub fn disaster_point(&self, n: usize) -> Option<(usize, usize)> {
        let data = &self.blocks.get(SCENARIO)?.data;
        let i = n.checked_sub(1).filter(|&i| i < DISASTER_POINTS)?;
        let value = |at: usize| data.get(at..at + 2).map(|b| i16::from_le_bytes([b[0], b[1]]));
        let (x, y) = (value(DISASTER_X_OFFSET + 2 * i)?, value(DISASTER_Y_OFFSET + 2 * i)?);
        let in_grid = |v: i16| (0..GRID as i16).contains(&v);
        (in_grid(x) && in_grid(y)).then_some((x as usize, y as usize))
    }

    /// Points de poissons placés dans l'éditeur.
    pub fn fish_points(&self) -> Vec<(usize, usize)> {
        let Some(data) = self.blocks.get(SCENARIO).map(|b| &b.data) else { return Vec::new() };
        let value = |at: usize| data.get(at..at + 2).map(|b| i16::from_le_bytes([b[0], b[1]]));
        let in_grid = |v: i16| (0..GRID as i16).contains(&v);
        (0..FISH_POINTS)
            .filter_map(|i| Some((value(FISH_X_OFFSET + 2 * i)?, value(FISH_Y_OFFSET + 2 * i)?)))
            .filter(|&(x, y)| in_grid(x) && in_grid(y))
            .map(|(x, y)| (x as usize, y as usize))
            .collect()
    }

    /// Prix des marchandises réglés dans l'éditeur (« World Properties ») : un bloc de
    /// 36 entiers, un par ressource. Les cartes de Zeus les rangent dans l'ancienne
    /// numérotation des ressources. Rend (ressource, prix) pour les prix non nuls.
    pub fn prices(&self) -> Vec<(u8, i32)> {
        let Some(block) = self.blocks.iter().find(|b| b.count == PRICES && b.element_size == 4) else { return Vec::new() };
        let zeus = self.version < POSEIDON_MAP_VERSION;
        block
            .data
            .chunks_exact(4)
            .enumerate()
            .filter_map(|(r, b)| {
                let price = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                let resource = if zeus { crate::goals::modern_resource(r as u8) } else { r as u8 };
                (price > 0 && resource != 0).then_some((resource, price))
            })
            .collect()
    }

    /// Cités du monde, vide pour une carte qui n'en décrit pas.
    pub fn cities(&self) -> Vec<City> {
        let Some(block) = self.blocks.iter().find(|b| b.count == CITIES && CITY_SIZES.contains(&b.element_size)) else { return Vec::new() };
        // Les cartes de Zeus (enregistrements de 476 octets) numérotent les
        // marchandises comme avant Poséidon.
        let zeus = block.element_size == CITY_SIZES[0];
        let goods = move |r: &[u8], at: usize| -> Vec<(u8, i32)> {
            r[at..at + TRADE_SLOTS]
                .iter()
                .filter(|&&g| g != 0)
                .map(|&g| {
                    let quantity = r.get(CITY_QUANTITIES + g as usize).copied().unwrap_or(0) as i32;
                    (if zeus { crate::goals::modern_resource(g) } else { g }, quantity)
                })
                .filter(|&(g, _)| g != 0)
                .collect()
        };
        block
            .data
            .chunks_exact(block.element_size)
            .map(|r| City {
                sells: goods(r, CITY_SELLS),
                buys: goods(r, CITY_BUYS),
                active: r[0] != 0,
                kind: r[CITY_KIND],
                name: r[CITY_NAME],
                status: le32(r, CITY_STATUS) as i32,
                military: r[CITY_MILITARY],
                trade: false,
                favor: (le32(r, CITY_FAVOR) as i32).clamp(0, 100),
                sold: 0,
                bought: 0,
                army: r[CITY_ARMY],
                tribute_received: le32(r, CITY_TRIBUTE_RECEIVED) as i32,
                tribute_paid: le32(r, CITY_TRIBUTE_PAID) as i32,
                wealth: r[CITY_WEALTH],
                strength: -1,
            })
            .collect()
    }

    /// Routes du monde en service.
    pub fn routes(&self) -> Vec<Route> {
        let Some(block) = self.blocks.iter().find(|b| b.count == ROUTES && b.element_size == ROUTE_SIZE) else { return Vec::new() };
        let short = |r: &[u8], at: usize| i16::from_le_bytes([r[at], r[at + 1]]);
        block
            .data
            .chunks_exact(ROUTE_SIZE)
            .filter(|r| r[ROUTE_ACTIVE] != 0)
            .map(|r| Route { from: short(r, ROUTE_CITIES), to: short(r, ROUTE_CITIES + 2), length: le32(r, ROUTE_LENGTH) as i32, pace: r[0] })
            .collect()
    }

    /// Chef de chaque cité du monde (octet 208 de son enregistrement : rang dans le
    /// groupe de textes 139), dans l'ordre de `cities`.
    pub fn city_leaders(&self) -> Vec<u8> {
        let Some(block) = self.blocks.iter().find(|b| b.count == CITIES && CITY_SIZES.contains(&b.element_size)) else { return Vec::new() };
        block.data.chunks_exact(block.element_size).map(|r| r[CITY_LEADER]).collect()
    }

    /// Noms écrits en toutes lettres dans les cartes de Poséidon (nom de la cité en 505,
    /// nom du chef en 537, chaînes terminées par un zéro), quand le numéro de nom vaut
    /// 255 : (cité, chef) pour chaque cité, dans l'ordre de `cities`.
    pub fn city_titles(&self) -> Vec<(Option<String>, Option<String>)> {
        let Some(block) = self.blocks.iter().find(|b| b.count == CITIES && b.element_size == CITY_SIZES[1]) else { return Vec::new() };
        let string = |r: &[u8], at: usize, end: usize| {
            let bytes: Vec<u8> = r[at..end].iter().copied().take_while(|&b| b != 0).collect();
            (!bytes.is_empty()).then(|| bytes.iter().map(|&b| crate::text::cp1252(b)).collect::<String>())
        };
        block.data.chunks_exact(block.element_size).map(|r| (string(r, CITY_TITLE, CITY_LEADER_TITLE), string(r, CITY_LEADER_TITLE, block.element_size))).collect()
    }

    pub fn terrain(&self, x: usize, y: usize) -> u32 {
        le32(&self.blocks[TERRAIN].data, (y * GRID + x) * 4)
    }
}

fn le32(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
}
