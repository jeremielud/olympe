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

    pub fn terrain(&self, x: usize, y: usize) -> u32 {
        le32(&self.blocks[TERRAIN].data, (y * GRID + x) * 4)
    }
}

fn le32(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
}
