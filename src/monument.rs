//! Monuments de Poséidon (pyramides, monuments au ciel, sanctuaires mineurs, autel et
//! temple de l'Olympe…) : leur disposition, case par case, telle que la décrivent les
//! fichiers `model/poseidon_pyramid_NN.txt` du jeu (pièce, élévation, ordre de
//! construction).

/// Type de bâtiment (règles et noms) du premier monument ; les quinze se suivent.
pub const FIRST_TYPE: usize = 100;
pub const MONUMENTS: usize = 15;

/// Pièces nommées dans les fichiers : pentes raides (unies, à marches, en V, à
/// triangle, à cercle), sommet de pyramide, aire d'accès, monte-charge, dalles.
pub const STEEP_FIRST: u16 = 117;
pub const STEEP_LAST: u16 = 121;
pub const TOPPER: u16 = 122;
pub const LIFT: u16 = 124;
pub const ACCESS_PAD: u16 = 1006;
pub const TILE_PLAIN: u16 = 160;
pub const TILE_ORICHALC: u16 = 161;
/// Statues et grandes statues au choix du joueur (141 à 148), observatoire, musée.
pub const CUSTOM_FIRST: u16 = 141;
pub const CUSTOM_LAST: u16 = 148;

/// Case d'un monument, relative au coin de l'emprise. `elevation` est la hauteur de
/// sa base, en niveaux ; une pente raide monte de deux niveaux. `order` est l'étape
/// de construction où elle s'élève (0 : dès le début).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Tile {
    pub dx: usize,
    pub dy: usize,
    pub piece: u16,
    pub elevation: u8,
    pub order: u16,
}

/// Côté d'une pièce, en cases : temple (110), autel (2) et grandes statues des
/// dispositions de sanctuaire (61 à 108) en couvrent plusieurs.
pub fn piece_size(piece: u16) -> usize {
    match piece {
        110 => 4,
        2 | 61..=108 => 2,
        _ => 1,
    }
}

/// Hauteur d'une pente raide, en niveaux.
pub const STEEP_RISE: u8 = 2;

impl Tile {
    /// Hauteur du sommet de la case, en niveaux.
    pub fn top(&self) -> u8 {
        match self.piece {
            STEEP_FIRST..=STEEP_LAST | LIFT => self.elevation + STEEP_RISE,
            _ => self.elevation,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Layout {
    pub tiles: Vec<Tile>,
    pub width: usize,
    pub depth: usize,
}

impl Layout {
    /// Lit un fichier de disposition : une ligne `{x, y, pièce, élévation, ordre},`
    /// par case ; les cases de l'emprise que rien ne couvre restent unies (pièce 0).
    pub fn parse(text: &str) -> Option<Layout> {
        let mut cells = Vec::new();
        for line in text.lines().map(str::trim).filter(|l| l.starts_with('{')) {
            let numbers: Vec<i32> = line.split(|c: char| !(c.is_ascii_digit() || c == '-')).filter_map(|n| n.parse().ok()).collect();
            if let [x, y, piece, elevation, order, ..] = numbers[..] {
                cells.push((x, y, piece.max(0) as u16, elevation.clamp(0, 255) as u8, order.max(0) as u16));
            }
        }
        let (min_x, min_y) = (cells.iter().map(|c| c.0).min()?, cells.iter().map(|c| c.1).min()?);
        let width = (cells.iter().map(|c| c.0).max()? - min_x + 1) as usize;
        let depth = (cells.iter().map(|c| c.1).max()? - min_y + 1) as usize;
        let mut tiles: Vec<Tile> = cells
            .into_iter()
            .map(|(x, y, piece, elevation, order)| Tile { dx: (x - min_x) as usize, dy: (y - min_y) as usize, piece, elevation, order })
            .collect();
        // Les cases que rien ne désigne sont sous une grande pièce (temple de quatre
        // cases de côté, autel ou statue de deux) : elles prennent sa hauteur.
        for dy in 0..depth {
            for dx in 0..width {
                if tiles.iter().any(|t| (t.dx, t.dy) == (dx, dy)) {
                    continue;
                }
                let under = tiles.iter().find(|t| {
                    let size = piece_size(t.piece);
                    (t.dx..t.dx + size).contains(&dx) && (t.dy..t.dy + size).contains(&dy)
                });
                let (elevation, order) = under.map_or((0, 0), |t| (t.elevation, t.order));
                tiles.push(Tile { dx, dy, piece: 0, elevation, order });
            }
        }
        Some(Layout { tiles, width, depth })
    }

    /// Nombre d'étapes de construction.
    pub fn stages(&self) -> u16 {
        self.tiles.iter().map(|t| t.order).max().unwrap_or(0)
    }

    /// Matériaux à livrer : marbre, bois, sculptures. Quantités d'Olympe
    /// (provisoires) : une charge de marbre par niveau de pierre posé, du bois pour
    /// l'échafaudage (une charge par étape), une sculpture par statue.
    pub fn materials(&self) -> [u16; 3] {
        let marble = self.tiles.iter().map(|t| if t.piece == 0 || t.piece == ACCESS_PAD { 0 } else { t.top().max(1) as u16 }).sum();
        let statues = self.tiles.iter().filter(|t| (CUSTOM_FIRST..=CUSTOM_LAST).contains(&t.piece) || (7..=108).contains(&t.piece) && t.piece != 55).count();
        [marble, self.stages(), statues as u16]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pyramid_rises_in_tiers() {
        let text = "{ -1, -1, 117, 0, 1,},\n{ 0, -1, 120, 0, 1,},\n{ -2, 0, 1006, 0, 0,},\n{ 0, 0, 122, 2, 2,},\n{ 0, 1, 117, 0, 3,},\n";
        let layout = Layout::parse(text).unwrap();
        assert_eq!((layout.width, layout.depth, layout.stages()), (3, 3, 3));
        assert_eq!(layout.tiles.len(), 9);
        assert!(layout.tiles.contains(&Tile { dx: 2, dy: 1, piece: TOPPER, elevation: 2, order: 2 }));
        // Trois pentes de deux niveaux et un sommet posé à deux niveaux.
        assert_eq!(layout.materials(), [8, 3, 0]);
    }
}
