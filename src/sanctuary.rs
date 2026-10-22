//! Sanctuaires : leur disposition, case par case, telle que la décrivent les fichiers
//! `model/sanctuary_<dieu>.txt` du jeu (temple, autel, statues, dallage).

/// Dieux, dans l'ordre du jeu (groupe de textes 157, objectifs, réglages des
/// aventures) : 0 Zeus … 11 Hadès, puis Héra et Atlas, venus avec Poséidon.
pub const GODS: usize = 14;
/// Fichier de disposition du sanctuaire de chaque dieu.
pub const LAYOUTS: [&str; GODS] = [
    "zeus",
    "poseidon",
    "demeter",
    "apollo",
    "artemis",
    "ares",
    "aphrodite",
    "hermes",
    "athena",
    "hephaistos",
    "dionysus",
    "hades",
    "hera",
    "atlas",
];
/// Type de bâtiment (règles et noms) du sanctuaire du dieu 0 ; les suivants se suivent.
pub const FIRST_TYPE: usize = 84;
/// Nom des dieux dans `zeus_text.eng`.
pub const TEXT_GODS: usize = 157;

/// Rang, dans les dispositions, de la statue nord d'un dieu : les quatre orientations
/// (N, E, S, O) se suivent, et les dieux dans l'ordre de leurs rangs dans le jeu,
/// qui n'est pas celui de `GODS`.
const SCULPTURE_ORDER: [usize; 12] = [0, 1, 11, 2, 8, 4, 3, 5, 9, 6, 7, 10];

/// Rang d'un dieu dans le jeu (Zeus, Poséidon, Hadès, Déméter…), qui ordonne ses
/// statues dans `zeus_general.sg3` et ses catalogues `zeus_godNN.sg3` ; Héra et Atlas
/// n'en ont pas.
pub fn rank(god: usize) -> Option<usize> {
    SCULPTURE_ORDER.iter().position(|&g| g == god)
}

/// Élément d'un sanctuaire, à sa place relative au coin du temple.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Piece {
    /// Temple d'un seul tenant (4 × 4).
    Temple,
    /// Moitiés arrière et avant d'un grand temple (4 × 4 chacune).
    TempleBack,
    TempleFront,
    /// Autel (2 × 2).
    Altar,
    /// Grande statue d'un dieu (2 × 2), et son orientation (0 à 3 : N, E, S, O).
    Sculpture { god: usize, facing: usize },
    /// Petite statue d'un dieu, sur une case.
    Statue { god: usize, facing: usize },
    /// Dalle décorée (six motifs).
    Tile(usize),
    /// Dallage uni, et tout ce qu'Olympe ne sait pas encore dessiner (arbres, mâts…).
    Plain,
}

impl Piece {
    pub fn size(self) -> usize {
        match self {
            Piece::Temple | Piece::TempleBack | Piece::TempleFront => 4,
            Piece::Altar | Piece::Sculpture { .. } => 2,
            _ => 1,
        }
    }

    pub fn from_type(kind: i32) -> Piece {
        let god_of = |rank: usize| SCULPTURE_ORDER[rank];
        match kind {
            0 => Piece::TempleFront,
            1 => Piece::TempleBack,
            2 => Piece::Altar,
            // « SCULPTURE_* » : petites statues ; « STATUE_OF_* » : grandes statues.
            7..=54 => Piece::Statue { god: god_of((kind as usize - 7) / 4), facing: (kind as usize - 7) % 4 },
            55..=60 => Piece::Tile(kind as usize - 55),
            61..=108 => Piece::Sculpture { god: god_of((kind as usize - 61) / 4), facing: (kind as usize - 61) % 4 },
            110 => Piece::Temple,
            _ => Piece::Plain,
        }
    }
}

/// Disposition d'un sanctuaire : ses éléments, avec leur coin relatif à celui de
/// l'emprise, et la taille de l'emprise.
#[derive(Clone, Debug)]
pub struct Layout {
    pub pieces: Vec<(usize, usize, Piece)>,
    pub width: usize,
    pub depth: usize,
}

impl Layout {
    /// Lit un fichier de disposition : une ligne `{x, y, type, élévation},` par case
    /// occupée, l'origine étant le coin du temple. L'élévation n'est pas reprise.
    pub fn parse(text: &str) -> Option<Layout> {
        let mut cells = Vec::new();
        for line in text.lines().map(str::trim).filter(|l| l.starts_with('{')) {
            let numbers: Vec<i32> = line.split(|c: char| !(c.is_ascii_digit() || c == '-')).filter_map(|n| n.parse().ok()).collect();
            if let [x, y, kind, ..] = numbers[..] {
                cells.push((x, y, Piece::from_type(kind)));
            }
        }
        let (min_x, min_y) = (cells.iter().map(|c| c.0).min()?, cells.iter().map(|c| c.1).min()?);
        let width = cells.iter().map(|&(x, _, piece)| x + piece.size() as i32).max()? - min_x;
        let depth = cells.iter().map(|&(_, y, piece)| y + piece.size() as i32).max()? - min_y;
        let (width, depth) = (width as usize, depth as usize);
        let mut pieces: Vec<(usize, usize, Piece)> = cells.into_iter().map(|(x, y, piece)| ((x - min_x) as usize, (y - min_y) as usize, piece)).collect();
        // Les cases que rien ne couvre (éléments de Poséidon encore inconnus) reçoivent
        // du dallage.
        let covered = |x: usize, y: usize, pieces: &[(usize, usize, Piece)]| {
            pieces.iter().any(|&(px, py, piece)| (px..px + piece.size()).contains(&x) && (py..py + piece.size()).contains(&y))
        };
        for y in 0..depth {
            for x in 0..width {
                if !covered(x, y, &pieces) {
                    pieces.push((x, y, Piece::Plain));
                }
            }
        }
        Some(Layout { pieces, width, depth })
    }

    /// Matériaux qu'il faut livrer pour bâtir le sanctuaire : marbre, bois, sculptures.
    /// Le jeu demande ces trois marchandises ; les quantités sont celles d'Olympe
    /// (provisoires) : une charge de marbre par dalle ou statue et huit par moitié de
    /// temple, du bois pour l'échafaudage, une sculpture par grande statue.
    pub fn materials(&self) -> [i32; 3] {
        let (mut marble, mut sculptures, mut temples) = (0, 0, 0);
        for &(_, _, piece) in &self.pieces {
            match piece {
                Piece::Temple => temples += 2,
                Piece::TempleBack | Piece::TempleFront => temples += 1,
                Piece::Tile(_) | Piece::Statue { .. } | Piece::Altar => marble += 1,
                Piece::Sculpture { .. } => sculptures += 1,
                Piece::Plain => {}
            }
        }
        [marble / 2 + 8 * temples, 8 * temples, sculptures]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_layout_is_shifted_to_its_corner() {
        let text = "header\n{-1,-1, 1000, 0,},\n{0,0, 110, 1,},\n{4,1, 58, 0,},\n{-1,4, 63, 0,},\n";
        let layout = Layout::parse(text).unwrap();
        assert_eq!((layout.width, layout.depth), (6, 7));
        assert_eq!(layout.pieces.iter().map(|&(_, _, piece)| piece.size().pow(2)).sum::<usize>(), 6 * 7);
        assert!(layout.pieces.contains(&(1, 1, Piece::Temple)));
        assert!(layout.pieces.contains(&(5, 2, Piece::Tile(3))));
        // 63 : grande statue de Zeus (deux cases de côté) tournée vers le sud.
        assert!(layout.pieces.contains(&(0, 5, Piece::Sculpture { god: 0, facing: 2 })));
    }

    /// Les dispositions du jeu, quand elles sont là, couvrent leur emprise case par
    /// case, sans chevauchement.
    #[test]
    fn game_layouts_tile_their_footprint() {
        for name in LAYOUTS {
            let Ok(text) = std::fs::read(format!("donnees/model/sanctuary_{name}.txt")) else { continue };
            let layout = Layout::parse(&String::from_utf8_lossy(&text)).unwrap();
            let area: usize = layout.pieces.iter().map(|&(_, _, piece)| piece.size().pow(2)).sum();
            assert_eq!(area, layout.width * layout.depth, "{name}");
        }
    }
}
