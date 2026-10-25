//! Pavage du terrain comme `Zeus.exe` le recalcule (0x521520) : chaque case de terrain
//! nu reçoit son image d'après ses bits de terrain, ses voisines, sa hauteur et la grille
//! d'aléa de la carte, pour l'une des quatre orientations de la vue. Les cartes stockent
//! le résultat pour l'orientation 0 ; la rotation de la vue demande de le refaire.
//!
//! Les fonctions portent l'adresse de leur modèle dans le jeu. Les écarts connus avec
//! les cartes : les zones de catastrophe (bits 0x2000000 et 0x10) que le pinceau de
//! l'éditeur a peintes autrement, et les routes, qu'Olympe dessine lui-même.

use crate::map::{GRID, Map};
use crate::tiling_tables::*;

/// Entrée d'une table de contexte (20 octets dans le jeu) : pour chacune des huit
/// voisines (N, NE, E, SE, S, SO, O, NO), 1 = drapeau levé, 0 = baissé, 2 = indifférent ;
/// décalage d'image par orientation ; nombre de variantes ; compteur de rotation des
/// variantes (état initial du jeu).
#[derive(Clone, Copy)]
pub struct Context {
    pub pattern: [u8; 8],
    pub offsets: [u8; 4],
    pub variants: u8,
    pub counter: u8,
}

const W: usize = GRID;
const NB: [isize; 8] = [-228, -227, 1, 229, 228, 227, -1, -229];
/// Blocs de la carte : images, bords, terrain, tailles, aléa, et grilles d'un octet.
const IMAGES: usize = 4;
const EDGES: usize = 5;
const TERRAIN: usize = 6;
const SIZES: usize = 7;
const RANDOM: usize = 8;
const SCENARIO: usize = 14;
const GROWTH_OLD: usize = 15;
const FERTILITY: usize = 19;
const GROWTH: usize = 22;
const HEIGHTS: usize = 23;
/// Bits du terrain.
const TREE: u32 = 1;
const ROCK: u32 = 0x2;
const WATER_BIT: u32 = 4;
const ROAD: u32 = 0x40;
const MEADOW: u32 = 0x80;
const ELEVATED: u32 = 0x200;
const SCRUB: u32 = 0x800;
const BEACH: u32 = 0x10000;
const QUARRY: u32 = 0x20000;
const MARSH: u32 = 0x40000;
const RAMP: u32 = 0x800000;
const DEEP: u32 = 0x4000000;
const BUILT: u32 = 0x8c008;
const ROCKS: u32 = 0x300002;
const BLOCKING: u32 = 0x2dfed66f;
/// Codes de relief qui forment des rampes de 2 × 2 cases, posées à la seconde passe.
const SPECIAL: [u32; 12] = [0xa92, 0xa93, 0xa94, 0xa95, 0xa53, 0xa54, 0xa55, 0xa56, 0xa8a, 0xa8c, 0xa8e, 0xa90];

/// Images et bords d'une carte pavée pour une orientation.
pub struct Tiling {
    pub images: Vec<u32>,
    pub edges: Vec<u8>,
}

/// Pave le terrain de la carte pour l'orientation `orientation` (0, 2, 4 ou 6 comme
/// dans le jeu). `terrain` et `poseidon` sont les premières images de chaque groupe
/// de `zeus_terrain.sg3` et `poseidon_loaded.sg3`.
pub fn tile(map: &Map, terrain: &[usize], poseidon: &[usize], orientation: u8) -> Tiling {
    let mut work = Work::new(map, terrain, poseidon, orientation);
    work.run();
    // Les cartes de Zeus numérotent les images du terrain à partir de 0xC000.
    if map.version < 318 {
        for id in &mut work.img {
            if (0x14000..0x14000 + 10_000).contains(id) {
                *id -= 0x8000;
            }
        }
    }
    Tiling { images: work.img, edges: work.edge }
}

struct Work<'a> {
    terrain_groups: &'a [usize],
    poseidon_groups: &'a [usize],
    o: usize,
    img: Vec<u32>,
    edge: Vec<u8>,
    ter: Vec<u32>,
    sizes: Vec<u8>,
    rnd: Vec<u8>,
    fertility: Vec<u8>,
    growth: Vec<u8>,
    old_growth: Vec<u8>,
    heights: Vec<u8>,
    tropical: bool,
    w: i32,
    h: i32,
    corner: i32,
    /// Lignes de la zone jouable : (rang, x de début, x de fin, index de la première case).
    rows: Vec<(i32, i32, i32, usize)>,
    /// Résultat de la première passe du relief : (code, direction, bits).
    relief: Vec<(u32, i32, u32)>,
    /// Voisines du motif courant.
    f: [bool; 8],
    off: u32,
    var: u32,
    counters: Vec<Vec<u8>>,
}

fn grid(map: &Map, block: usize) -> Vec<u8> {
    map.blocks.get(block).filter(|b| b.element_size == 1 && b.count == W * W).map_or(vec![0; W * W], |b| b.data.clone())
}

fn grid32(map: &Map, block: usize) -> Vec<u32> {
    map.blocks.get(block).filter(|b| b.element_size == 4 && b.count == W * W).map_or(vec![0; W * W], |b| {
        b.data.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
    })
}

impl<'a> Work<'a> {
    fn new(map: &Map, terrain_groups: &'a [usize], poseidon_groups: &'a [usize], orientation: u8) -> Work<'a> {
        let scenario = &map.blocks[SCENARIO].data;
        let int = |at: usize| scenario.get(at..at + 4).map_or(0, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let terrain_base = if map.version >= 318 { 0x14000 } else { 0xC000 };
        // Les cartes de Zeus numérotent les images du terrain à partir de 0xC000.
        let img = grid32(map, IMAGES).into_iter().map(|v| if (0xC000..0xC000 + 10_000).contains(&v) && terrain_base == 0xC000 { v - 0xC000 + 0x14000 } else { v }).collect();
        let mut work = Work {
            terrain_groups,
            poseidon_groups,
            o: orientation as usize & 6,
            img,
            edge: grid(map, EDGES),
            ter: grid32(map, TERRAIN),
            sizes: grid(map, SIZES),
            rnd: grid(map, RANDOM),
            fertility: grid(map, FERTILITY),
            growth: grid(map, GROWTH),
            old_growth: grid(map, GROWTH_OLD),
            heights: grid(map, HEIGHTS),
            tropical: scenario.get(0x3e4) == Some(&1),
            w: int(44),
            h: int(48),
            corner: int(52),
            rows: Vec::new(),
            relief: vec![(0, 0, 0); W * W],
            f: [false; 8],
            off: 0,
            var: 0,
            counters: [&WATER[..], &DEEP_WATER[..], &BLACK_ROCK[..], &QUARRY_EDGE[..], &QUARRY_DEPTH[..]].iter().map(|t| t.iter().map(|c| c.counter).collect()).collect(),
        };
        work.make_rows();
        work
    }

    /// Lignes du losange jouable (0x495e90).
    fn make_rows(&mut self) {
        let (w, h) = (self.w, self.h);
        let (x0, y0) = ((228 - w) / 2, (228 - h) / 2);
        let (xr, yb) = (x0 - 1 + w, y0 - 1 + h);
        let (xm, ym) = ((xr + x0) / 2, (yb + y0) / 2);
        let mut v = (ym - y0) + x0;
        for yy in y0..=yb {
            let a = if yy <= ym { v } else { (x0 - ym) - 1 + yy };
            let (start, end) = (a - x0, (xm * 2 - a) + 2 - x0);
            self.rows.push((yy - y0, start, end, (a + yy * 228) as usize));
            v -= 1;
        }
    }

    /// Identifiant de la première image du groupe `k` du catalogue `cat` (5 terrain,
    /// 52 Poséidon), comme le jeu les range (rang k → groupe k + 1 du catalogue).
    fn first(&self, cat: u32, k: usize) -> u32 {
        let groups = if cat == 52 { self.poseidon_groups } else { self.terrain_groups };
        cat * 0x4000 + groups.get(k + 1).copied().unwrap_or(201) as u32 - 201
    }

    fn t5(&self, k: usize) -> u32 {
        self.first(5, k)
    }

    /// Code d'image du jeu : catalogue << 9 | (groupe + 1).
    fn code(&self, c: u32) -> u32 {
        if c & 0x1ff == 0 { 0 } else { self.first(c >> 9, (c & 0x1ff) as usize - 1) }
    }

    fn t(&self, i: isize) -> u32 {
        if (0..(W * W) as isize).contains(&i) { self.ter[i as usize] } else { 0 }
    }

    fn at(v: &[u8], i: isize) -> u8 {
        if (0..(W * W) as isize).contains(&i) { v[i as usize] } else { 0 }
    }

    fn neighbours(&mut self, i: usize, bit: u32) {
        for (k, &o) in NB.iter().enumerate() {
            self.f[k] = self.t(i as isize + o) & bit != 0;
        }
    }

    fn invert(&mut self) {
        self.f = self.f.map(|v| !v);
    }

    /// Cherche la première entrée de la table qui convient aux voisines (0x4a6920) ;
    /// `rnd` négatif fait tourner le compteur de l'entrée.
    fn matches(&mut self, table: usize, rnd: i32, reverse: bool) -> bool {
        let entries: &[Context] = [&WATER[..], &DEEP_WATER[..], &BLACK_ROCK[..], &QUARRY_EDGE[..], &QUARRY_DEPTH[..]][table];
        let fits = |c: &Context| c.pattern.iter().zip(&self.f).all(|(&p, &f)| p == 2 || (p == 1) == f);
        let found = if reverse { (0..entries.len()).rev().find(|&k| fits(&entries[k])) } else { (0..entries.len()).find(|&k| fits(&entries[k])) };
        let Some(k) = found else {
            (self.off, self.var) = (0, 0);
            return false;
        };
        let e = entries[k];
        self.off = e.offsets[self.o / 2] as u32;
        self.var = if rnd >= 0 && e.variants > 0 {
            rnd as u32 % e.variants as u32
        } else {
            let counter = &mut self.counters[table][k];
            *counter = (*counter + 1) % e.variants.max(1);
            *counter as u32
        };
        true
    }

    fn set(&mut self, i: usize, v: u32) {
        self.img[i] = v;
        self.sizes[i] &= 0xf0;
        self.edge[i] |= 0x40;
    }

    fn index(&self, x: i32, y: i32) -> usize {
        (self.corner + y * 228 + x) as usize
    }

    /// Une image sur un bloc de cases (0x4a04b0) : la case porteuse dépend de l'orientation.
    fn place(&mut self, x: i32, y: i32, wd: i32, hg: i32, image: u32, bits: u32) {
        if x < 0 || y < 0 || x - 1 + wd >= self.w || y - 1 + hg >= self.h {
            return;
        }
        let carrier = match self.o {
            0 => (0, hg - 1),
            2 => (0, 0),
            4 => (wd - 1, 0),
            _ => (wd - 1, hg - 1),
        };
        let size = match wd {
            2 => 1,
            3 => 2,
            4 => 3,
            5 => 4,
            6 => 5,
            _ => 0,
        };
        for r in 0..hg {
            for c in 0..wd {
                let i = self.index(x + c, y + r);
                self.ter[i] = (self.ter[i] & 0x93872f90) | bits;
                self.sizes[i] = (self.sizes[i] & 0xe0) | size;
                self.img[i] = image;
                self.edge[i] = (c | r << 3) as u8 | if (c, r) == carrier { 0x40 } else { 0 };
            }
        }
    }

    /// Images consécutives sur un bloc de cases (0x4a0680).
    fn place_each(&mut self, x: i32, y: i32, wd: i32, hg: i32, image: u32) {
        if x < 0 || y < 0 || x - 1 + wd >= self.w || y - 1 + hg >= self.h {
            return;
        }
        let mut k = 0;
        for r in 0..hg {
            for c in 0..wd {
                let i = self.index(x + c, y + r);
                self.ter[i] &= 0x93872f90;
                self.sizes[i] &= 0xf0;
                self.edge[i] |= 0x40;
                self.img[i] = image + k;
                k += 1;
            }
        }
    }

    /// Le bloc a le terrain voulu et pas encore d'image (0x4a0fd0).
    fn fits(&self, x: i32, y: i32, wd: i32, hg: i32, mask: u32) -> bool {
        if x < 0 || y < 0 || x - 1 + wd >= self.w || y - 1 + hg >= self.h {
            return false;
        }
        (0..hg).all(|r| (0..wd).all(|c| {
            let i = self.index(x + c, y + r);
            self.ter[i] & BLOCKING == mask && self.img[i] == 0
        }))
    }

    /// Case dans le losange jouable (0x496010), en coordonnées de la grille.
    fn inside(&self, x: i32, y: i32) -> bool {
        let (x0, y0) = ((228 - self.w) / 2, (228 - self.h) / 2);
        let (xr, yb) = (x0 - 1 + self.w, y0 - 1 + self.h);
        let (xm, ym) = ((xr + x0) / 2, (yb + y0) / 2);
        if xm < x {
            if y <= ym { x - y <= xr - ym } else { x + y <= xr + 1 + ym }
        } else if y <= ym {
            x0 + ym <= x + y
        } else {
            xm - yb <= x - y
        }
    }

    /// Case à moins de `m` cases du bord du losange (0x496080).
    fn near_border(&self, x: i32, y: i32, m: i32) -> bool {
        if !self.inside(x, y) {
            return false;
        }
        let (x0, y0) = ((228 - self.w) / 2, (228 - self.h) / 2);
        let (xr, yb) = (x0 - 1 + self.w, y0 - 1 + self.h);
        let (xm, ym) = ((xr + x0) / 2, (yb + y0) / 2);
        if x <= xm {
            if y <= ym { (x - m) + y < x0 + ym } else { (x - y) - m < xm - yb }
        } else if y <= ym {
            xr - ym < (x - y) + m
        } else {
            xr + 1 + ym < x + y + m
        }
    }

    /// Place libre pour une plaque d'herbe (0x4a0dc0) : terre ou plage d'un seul tenant,
    /// loin du bord, sans image.
    fn free(&self, x: i32, y: i32, wd: i32, hg: i32) -> bool {
        if x < 3 || y < 3 || self.w - 3 <= x - 1 + wd || self.h - 3 <= y - 1 + hg {
            return false;
        }
        let (mut beach, mut land) = (false, false);
        for r in 0..hg {
            for c in 0..wd {
                let i = self.index(x + c, y + r);
                if self.near_border((i % 228) as i32, (i / 228) as i32, 3) {
                    return false;
                }
                let u = self.ter[i];
                if u & BEACH == 0 {
                    if beach {
                        return false;
                    }
                    land = true;
                } else {
                    if land {
                        return false;
                    }
                    beach = true;
                }
                if u & 0x3dffdeef != 0 || self.img[i] != 0 {
                    return false;
                }
            }
        }
        true
    }

    // ---- Relief ----

    /// Variante d'un code de relief selon l'aléa de la case (0x45de10).
    fn variant(&self, c: u32, i: usize) -> u32 {
        let n = match c {
            0xa1f | 0xa57 | 0xa58 | 0xa5a | 0xa5c | 0xa5d | 0xa6a | 0xa6b | 0xa6c | 0xa6e | 0xa70 | 0xa71 | 0xa8a | 0xa8c | 0xa8e | 0xa90 => 2,
            0xa59 | 0xa5b | 0xa6d | 0xa6f => 4,
            0xa9a..=0xa9d => 3,
            _ => 1,
        };
        if n == 1 {
            return c;
        }
        let v = self.rnd[i] as u32 % n;
        let four = |a: u32| if v == 0 { c } else { a + v - 1 };
        match c {
            0xa59 => four(0xa2d),
            0xa5b => four(0xa30),
            0xa6d => four(0xa3d),
            0xa6f => four(0xa40),
            _ if v == 0 => c,
            0xa1f => 0xa39,
            0xa57 => 0xa34,
            0xa58 => 0xa3a,
            0xa5a => 0xa3b,
            0xa5c => 0xa3c,
            0xa5d => 0xa33,
            0xa6a => 0xa49,
            0xa6b => 0xa48,
            0xa6c => 0xa4a,
            0xa6e => 0xa4b,
            0xa70 => 0xa4c,
            0xa71 => 0xa47,
            0xa8a => 0xa53,
            0xa8c => 0xa54,
            0xa8e => 0xa55,
            0xa90 => 0xa56,
            _ => c,
        }
    }

    fn height(&self, i: isize) -> i32 {
        Self::at(&self.heights, i) as i32
    }

    /// Côtés d'une rampe qui touchent d'autres rampes (0x45e080).
    fn side(&self, i: usize, dir: i32) -> u32 {
        let (a, b) = [(-1, 1), (-228, 228), (1, -1), (228, -228)][(dir / 2).clamp(0, 3) as usize];
        let u = self.t(i as isize + b) & RAMP != 0;
        let v = self.t(i as isize + a) & RAMP != 0;
        match (u, v) {
            (false, true) => 1,
            (true, false) => 2,
            (true, true) => 3,
            _ => 0,
        }
    }

    fn ramp_code(&self, c: u32, i: usize, dir: i32) -> u32 {
        let sided = |base: u32, [one, two, three]: [u32; 3]| match self.side(i, dir) {
            1 => one,
            2 => two,
            3 => three,
            _ => base,
        };
        match c {
            0xa6a => 0xa8a,
            0xa6c => 0xa8c,
            0xa6e => 0xa8e,
            0xa70 => 0xa90,
            0xa72..=0xa75 => c + 0x20,
            0xa76..=0xa79 => c + 0x28,
            0xa6b => sided(0xac9, [0xaab, 0xaaa, 0xa9a]),
            0xa6d => sided(0xac7, [0xaad, 0xaac, 0xa9b]),
            0xa6f => sided(0xac4, [0xaae, 0xaaf, 0xa9c]),
            0xa71 => sided(0xa03, [0xab1, 0xab0, 0xa9d]),
            _ => c,
        }
    }

    /// Motif d'écarts de hauteur de la case (0x45e110) : (code, direction, bits).
    fn relief_of(&self, i: usize) -> Option<(u32, i32, u32)> {
        let h = self.height(i as isize);
        let t = self.ter[i];
        let diff: Vec<i32> = NB.iter().map(|&o| self.height(i as isize + o) - h).collect();
        if !diff.iter().any(|&d| d > 0) {
            return None;
        }
        for e in &ELEVATION {
            if !(e[15] == 0 || t & 0x400 != 0) {
                continue;
            }
            let fits = (0..8).all(|k| match e[k] {
                3 => true,
                4 => diff[k].abs() <= 1,
                p => p == diff[k],
            });
            if !fits {
                continue;
            }
            let mut c = e[8 + self.o / 2] as u32;
            let (bits, dir) = (e[12] as u32, e[13]);
            if c == 1 || c == 2 {
                return Some((c, dir, bits));
            }
            let road = |c: u32| if t & ROAD != 0 { c + 4 } else { c };
            if t & RAMP != 0 {
                c = self.ramp_code(c, i, dir);
                if t & 0x400 != 0 {
                    return Some((self.variant(road(c), i), dir, bits));
                }
            }
            if e[15] == 0 {
                return Some((self.variant(c, i), dir, bits));
            }
            if e[14] as u32 & t == 0x400 {
                return Some((self.variant(road(c), i), dir, bits));
            }
        }
        None
    }

    /// Première passe du relief (0x51fec0).
    fn relief_first(&mut self, i: usize) {
        self.ter[i] &= 0xdfbffdff;
        let (c, dir, bits) = self.relief_of(i).unwrap_or((0, 0, 0));
        self.relief[i] = (c, dir, bits);
        if c == 1 || c == 2 || SPECIAL.contains(&c) {
            return;
        }
        if c == 0 {
            // Au pied d'une hauteur.
            if NB.iter().any(|&o| self.height(i as isize) < self.height(i as isize + o)) {
                self.img[i] = self.t5(0);
                self.ter[i] |= ELEVATED;
            }
            return;
        }
        self.edge[i] = 0x40;
        self.sizes[i] &= 0xf0;
        self.img[i] = self.code(c);
        if dir >= 0 && self.ter[i] & RAMP != 0 {
            let j = (i as isize + NB[dir as usize & 7]) as usize;
            self.sizes[j] &= 0xf0;
            self.edge[j] = 0x41;
            self.img[j] = self.code(c) + 1;
            self.ter[j] |= ELEVATED;
        }
        self.ter[i] |= bits | ELEVATED;
    }

    fn ramps_around(&self, i: usize, a: usize, b: usize) -> bool {
        let ids: Vec<u32> = RAMP_CODES.iter().map(|&c| self.code(c)).collect();
        let at = |d: usize| self.img.get((i as isize + NB[d]) as usize).copied().unwrap_or(0);
        ids.contains(&at(a)) && ids.contains(&at(b))
    }

    /// Seconde passe du relief (0x5200b0) : rampes de 2 × 2 cases.
    fn relief_second(&mut self, i: usize, mut x: i32, mut y: i32) -> bool {
        let (c, dir, bits) = self.relief[i];
        if c == 0 {
            return false;
        }
        let k = (dir / 2).clamp(0, 3) as usize;
        let near = |w: &Work, d: isize| w.img.get((i as isize + d) as usize).copied().unwrap_or(0);
        let (a, b) = [(-1isize, 1isize), (-228, 228), (1, -1), (228, -228)][k];
        if SPECIAL.contains(&c) {
            let pairs = if (0xa92..=0xa95).contains(&c) { [(4, 2), (6, 4), (0, 6), (2, 0)] } else { [(0, 6), (2, 0), (4, 2), (6, 4)] };
            let (pa, pb) = pairs[k];
            if self.ramps_around(i, pa, pb) {
                self.edge[i] = 0x40;
                match dir {
                    2 => x -= 1,
                    4 => {
                        x -= 1;
                        y -= 1
                    }
                    6 => y -= 1,
                    _ => {}
                }
                self.place(x, y, 2, 2, self.code(c), ELEVATED);
                self.ter[i] |= bits | ELEVATED;
            }
        } else if (0xaaa..=0xab1).contains(&c) || (0xa9a..=0xa9d).contains(&c) {
            let result = if (0xaaa..=0xab1).contains(&c) {
                let (ia, ib, r) = match c {
                    0xaaa => (self.t5(153), self.t5(169), 0xa82),
                    0xaab => (self.t5(170), self.t5(153), 0xa86),
                    0xaac => (self.t5(154), self.t5(171), 0xa83),
                    0xaad => (self.t5(172), self.t5(154), 0xa87),
                    0xaaf => (self.t5(155), self.code(0xaaf), 0xa88),
                    0xaae => (self.code(0xaae), self.code(0xa9c), 0xa84),
                    0xab0 => (self.code(0xa9d), self.code(0xab0), 0xa85),
                    _ => (self.code(0xab1), self.code(0xa9d), 0xa89),
                };
                if near(self, 0) == ia && near(self, a) == ib { r } else { 0 }
            } else {
                let key = [0xa8b, 0xa8d, 0xa8f, 0xa91][(c - 0xa9a) as usize];
                let image = self.code(c);
                let pick = self.rnd[i] & 3;
                if near(self, 0) == image && near(self, a) == image {
                    if pick == 0 { key } else { [0xad2, 0xad5, 0xad8, 0xadb][(c - 0xa9a) as usize] + pick as u32 - 1 }
                } else {
                    0
                }
            };
            let _ = b;
            if result != 0 {
                self.edge[i] = 0x40;
                match dir {
                    0 => {
                        x -= 1;
                        y -= 1
                    }
                    2 => y -= 1,
                    6 => x -= 1,
                    _ => {}
                }
                self.place(x, y, 2, 2, self.code(result), ELEVATED);
                self.ter[i] |= bits | ELEVATED;
            }
        }
        if self.ter[i] & RAMP != 0 {
            let j = i as isize + NB[dir as usize & 7];
            if (0..(W * W) as isize).contains(&j) {
                let j = j as usize;
                if (0xaa2..=0xaa9).contains(&c) {
                    self.ter[j] |= ROAD;
                }
                if (0xaa2..=0xaa9).contains(&c) || (0xa9e..=0xaa1).contains(&c) {
                    self.ter[j] |= 0x800400;
                }
                if (0xaa2..=0xaa9).contains(&c) || (0xa9e..=0xaa1).contains(&c) || matches!(c, 0xa03 | 0xac4 | 0xac7 | 0xac9) || (0xa9a..=0xa9d).contains(&c) || (0xaaa..=0xab1).contains(&c) {
                    self.ter[j] = (self.ter[j] & 0xffffff7f) | ELEVATED;
                }
            }
        }
        true
    }

    // ---- Chaîne des terrains ----

    /// Eau (0x520ca0).
    fn water(&mut self, i: usize) -> bool {
        let t = self.ter[i];
        if t & WATER_BIT == 0 {
            return false;
        }
        let r = self.rnd[i] as i32;
        self.neighbours(i, WATER_BIT);
        self.invert();
        self.matches(0, r, false);
        let fallback = |w: &mut Work| -> u32 {
            if w.ter[i] & DEEP == 0 {
                return if w.off == 0 { w.t5(34) + w.var } else { w.t5(4) + w.var + w.off };
            }
            w.neighbours(i, DEEP);
            w.invert();
            w.matches(1, r, true);
            w.t5(19) + w.var * 15 + w.off
        };
        let v = if t & BEACH != 0 {
            if self.off < 8 { fallback(self) } else { self.t5(34) + self.var + self.off }
        } else {
            let (off, var) = (self.off, self.var);
            let shore = NB.iter().any(|&o| self.t(i as isize + o) & BEACH != 0);
            (self.off, self.var) = (off, var);
            if shore { self.t5(34) + self.var + self.off } else { fallback(self) }
        };
        self.set(i, v);
        true
    }

    /// Profondeur de forêt : l'anneau de rayon `r` n'est fait que d'arbres ou de
    /// rochers, ou sort de la carte (0x4a4840).
    fn ring(&self, x: i32, y: i32, r: i32) -> bool {
        if r <= 0 {
            return true;
        }
        let base = self.index(x, y) as isize;
        for dy in -r..=r {
            for dx in -r..=r {
                if dx.abs().max(dy.abs()) != r || x + dx < -1 || y + dy < -1 || x + dx > self.w || y + dy > self.h {
                    continue;
                }
                let t = self.t(base + (dy * 228 + dx) as isize);
                if t & TREE == 0 && t & ROCKS == 0 {
                    return false;
                }
            }
        }
        true
    }

    /// Arbres, marais boisés et broussailles (0x4a81f0).
    fn trees(&mut self, i: usize, x: i32, y: i32) {
        let t = self.ter[i];
        if t & 0xc064 != 0 {
            return;
        }
        let r = self.rnd[i] as u32;
        if t & TREE != 0 && t & 0x80000 == 0 {
            let u = self.growth[i].min(100) as u32 / 34;
            if self.fertility[i] != 0xff {
                self.img[i] = self.t5(11) + (r & 7) + u * 8;
                return;
            }
            let mut k = 3;
            while k > 0 && !self.ring(x, y, k) {
                k -= 1;
            }
            let (a, b) = if k < 2 { (u * 0x20, k as u32) } else { (u * 0x20 + 0x10, k as u32 - 2) };
            self.img[i] = self.code(if self.tropical { 0x6842 } else { 0xa04 }) + (r & 7) + b * 8 + a;
            return;
        }
        if t & MARSH != 0 && t & 0x80000 == 0 {
            if NB.iter().all(|&o| self.t(i as isize + o) & MARSH != 0) {
                self.img[i] = (r & 7) + self.t5(if self.fertility[i] != 0xff { 25 } else { 26 });
            }
            return;
        }
        if t & SCRUB != 0 && t & 0x80000 == 0 {
            let a = self.old_growth[i] as f64 - 1.0;
            let b = self.fertility[i] as f64;
            let mut e = (a * (1.0 / 99.0) * b * (1.0 / 85.0)) as i32;
            let v = self.growth[i].min(100) as f64 - 1.0;
            let mut u = (v / 96.0 * 12.0) as u32;
            let mut c = 0xa25;
            if e == 0 {
                if u == 0 {
                    u = r & 7;
                    c = 0xa1c;
                }
            } else if e == 2 {
                if u == 0 {
                    e = 0;
                    u = r & 7;
                    c = 0xa13;
                } else if u == 0xb {
                    e = 0;
                    u = r & 7;
                    c = 0xa09;
                }
            }
            self.img[i] = self.code(c) + u + e as u32 * 12;
        }
    }

    /// Rochers et minerais (0x520580).
    fn rock(&mut self, i: usize, x: i32, y: i32) -> bool {
        let t = self.ter[i];
        if t & ROCKS == 0 || t & QUARRY != 0 {
            return false;
        }
        let r = self.rnd[i] as u32;
        if self.sizes[i] & 0x80 != 0 {
            for (k, &o) in NB.iter().enumerate() {
                let j = i as isize + o;
                self.f[k] = self.t(j) & ROCK != 0 && Self::at(&self.sizes, j) & 0x80 != 0;
            }
            let base = self.t5(5);
            let v = if self.matches(2, r as i32, false) { self.var + self.off + base } else { base };
            self.set(i, v);
            return true;
        }
        let (kind, base) = match t & ROCKS {
            0x100002 => (0x100002, self.t5(39)),
            0x200002 => (0x200002, self.t5(31)),
            0x300002 => (0x300002, self.t5(21)),
            0x300000 => (0x300000, self.first(52, 28)),
            _ => (ROCK, self.t5(7)),
        };
        if self.fits(x, y, 3, 3, kind) {
            self.place(x, y, 3, 3, (r & 1) + 0xc + base, kind);
        } else if self.fits(x, y, 2, 2, kind) {
            self.place(x, y, 2, 2, (r & 3) + 8 + base, kind);
        } else {
            self.set(i, (r & 7) + base);
        }
        true
    }

    fn block(&self, centre: isize, radius: isize, mask: u32, value: u32) -> bool {
        (-radius..=radius).all(|dy| (-radius..=radius).all(|dx| self.t(centre + dy * 228 + dx) & mask == value))
    }

    /// Carrières de marbre et de marbre noir (0x520920, branche de l'éditeur : la
    /// profondeur de la carrière se lit dans les carrés de cases voisines).
    fn quarry(&mut self, i: usize) -> bool {
        let t = self.ter[i];
        if t & QUARRY == 0 {
            return false;
        }
        let black = t & 0x120000 == 0x120000;
        let want = if black { 0x120000 } else { QUARRY };
        let r = self.rnd[i] as i32;
        for (k, &o) in NB.iter().enumerate() {
            self.f[k] = self.t(i as isize + o) & 0x120000 == want;
        }
        self.invert();
        let shift = |plain: u32, dark: u32| if black { dark } else { plain };
        let mut c = shift(0xacd, 0x5d63 + 0xacd);
        self.matches(3, r, false);
        for (radius, plain, dark) in [(1, 0xacc, 0x5d65 + 0xacc), (2, 0xacb, 0x5d67 + 0xacb)] {
            if self.off != 0 {
                break;
            }
            c = shift(plain, dark);
            for (k, &o) in NB.iter().enumerate() {
                self.f[k] = self.block(i as isize + o, radius, 0x120000, want);
            }
            self.invert();
            self.matches(4, r, false);
        }
        let v = self.code(c) + self.var + self.off;
        self.set(i, v);
        true
    }

    /// Marais (0x521150).
    fn marsh(&mut self, i: usize, x: i32, y: i32) -> bool {
        if self.ter[i] & MARSH == 0 {
            return false;
        }
        self.neighbours(i, MARSH);
        self.invert();
        self.matches(0, self.rnd[i] as i32, false);
        let v = self.var + self.off + self.t5(25);
        self.set(i, v);
        self.trees(i, x, y);
        true
    }

    /// Plages (0x520b80) : le jeu attache à chaque case de plage un objet qui en tire
    /// la variante de l'aléa.
    fn beach(&mut self, i: usize) -> bool {
        let t = self.ter[i];
        if t & BEACH == 0 || t & 0x48 != 0 {
            return false;
        }
        let v = self.t5(17) + self.rnd[i] as u32 % 6;
        self.set(i, v);
        true
    }

    /// Prés (0x51f860).
    fn meadow(&mut self, i: usize) -> bool {
        let t = self.ter[i];
        if t & 0x10801 != 0 || t & MEADOW == 0 {
            return false;
        }
        let shore = NB.iter().any(|&o| self.t(i as isize + o) & BEACH != 0);
        let (c, u);
        if shore {
            self.neighbours(i, BEACH);
            self.matches(1, -1, false);
            (c, u) = (0xa12, self.off + 6);
        } else {
            let v = self.growth[i] as u32;
            let mut g = if v <= 100 { (v as f64 * (12.0 / 96.0)) as u32 } else { v };
            if v > 100 {
                g = if g & 8 == 0 { (g.wrapping_sub(self.o as u32)) & 7 } else { ((g.wrapping_sub(self.o as u32 / 2)) & 3) + (g & 8) };
                g |= 0x80;
            }
            if g < 11 {
                (c, u) = (0xa24, g);
            } else {
                let busy = |w: &Work, j: isize| NB.iter().any(|&o| w.t(j + o) & BLOCKING != 0);
                let code = if t & 0x10000000 == 0 && busy(self, i as isize) {
                    0xa10
                } else if NB.iter().all(|&o| {
                    let j = i as isize + o;
                    let gj = (if self.t(j) & MEADOW != 0 { Self::at(&self.growth, j) } else { 0 } as f64 * (12.0 / 96.0)) as u32;
                    gj == 0xb && self.t(j) & 0x10801 == 0 && !(self.t(j) & 0x10000000 == 0 && busy(self, j))
                }) {
                    0xa0f
                } else {
                    0xa26
                };
                (c, u) = (code, 0);
            }
        }
        self.edge[i] |= 0x40;
        self.sizes[i] &= 0xf0;
        self.img[i] = self.code(c) + u;
        true
    }

    /// Herbe (0x51fba0) : plaques de 4 × 4, 3 × 3 ou 2 × 2 cases quand la place est
    /// libre, sinon une case.
    fn grass(&mut self, i: usize, x: i32, y: i32) -> bool {
        if self.ter[i] & 0x2cfed66f != 0 {
            return false;
        }
        let r = self.rnd[i] as u32;
        if r & 0xf0 != 0 {
            let k = if self.sizes[i] & 0x20 != 0 { 6 } else { 9 };
            let v = self.t5(k) + (r & 7);
            self.place_each(x, y, 1, 1, v);
            return true;
        }
        let base = self.code(if self.sizes[i] & 0x20 != 0 { 0xa07 } else { 0xa0a });
        if self.free(x, y, 4, 4) {
            self.place_each(x, y, 4, 4, base + 0x2a);
        } else if self.free(x, y, 3, 3) {
            self.place_each(x, y, 3, 3, (r & 1) * 9 + 0x18 + base);
        } else if self.free(x, y, 2, 2) {
            self.place_each(x, y, 2, 2, base + 8 + (r & 3) * 4);
        } else {
            self.place_each(x, y, 1, 1, (r & 7) + base);
        }
        true
    }

    fn chain(&mut self, i: usize, x: i32, y: i32) {
        if self.relief_second(i, x, y) || self.img[i] != 0 {
            return;
        }
        if self.water(i) {
            return;
        }
        let t = self.ter[i];
        if t & TREE != 0 {
            self.sizes[i] &= 0xf0;
            self.edge[i] |= 0x40;
            self.trees(i, x, y);
            return;
        }
        if self.rock(i, x, y) || self.quarry(i) || t & ROAD != 0 || self.beach(i) || self.marsh(i, x, y) {
            return;
        }
        if t & SCRUB != 0 {
            self.trees(i, x, y);
            self.sizes[i] &= 0xf0;
            self.edge[i] |= 0x40;
            return;
        }
        if !self.meadow(i) {
            self.grass(i, x, y);
        }
    }

    fn run(&mut self) {
        let rows = self.rows.clone();
        for &(_, x0, x1, idx) in &rows {
            for (k, _) in (x0..x1).enumerate() {
                let i = idx + k;
                if self.ter[i] & BUILT == 0 {
                    if self.ter[i] & ELEVATED == 0 {
                        self.img[i] = 0;
                    }
                    self.relief_first(i);
                }
            }
        }
        for &(y, x0, x1, idx) in &rows {
            for (k, x) in (x0..x1).enumerate() {
                let i = idx + k;
                if self.ter[i] & BUILT == 0 {
                    self.chain(i, x, y);
                }
            }
        }
    }
}
