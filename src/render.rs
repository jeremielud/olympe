//! Rendu isométrique du terrain d'une carte dans une image RVBA.

use crate::map::{EDGE_DRAW_TILE, GRID, Map, Source};
use crate::sg::Catalog;
use std::collections::HashMap;
use std::path::Path;

/// Demi-largeur et demi-hauteur d'une case à l'écran.
const HALF_WIDTH: i64 = 30;
const HALF_HEIGHT: i64 = 15;

pub struct Picture {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    /// Nombre de cases dessinées.
    pub drawn: usize,
    /// Cases dont l'identifiant d'image n'a pas pu être traduit ou décodé.
    pub missing: usize,
}

/// Dessine tout le terrain de `map` ; `data` est le dossier des données du jeu.
pub fn terrain(map: &Map, data: &Path) -> Result<Picture, String> {
    let terrain = Catalog::load(&data.join("data/zeus_terrain.sg3"))?;
    let poseidon = Catalog::load(&data.join("data/poseidon_loaded.sg3"))?;
    let catalog = |source: Source| if source == Source::Poseidon { &poseidon } else { &terrain };

    let mut tiles = Vec::new();
    let mut missing = 0;
    for y in 0..GRID {
        for x in 0..GRID {
            let id = map.image(x, y);
            if id == 0 || map.edge(x, y) & EDGE_DRAW_TILE == 0 {
                continue;
            }
            match map.resolve(id) {
                Some(image) => tiles.push((x as i64, y as i64, image)),
                None => missing += 1,
            }
        }
    }
    if tiles.is_empty() {
        return Err("carte vide".into());
    }
    // Un objet de n × n cases est porté par sa case la plus à gauche à l'écran ;
    // son coin le plus proche du spectateur est n − 1 cases plus bas.
    let size = |source: Source, id: usize| {
        catalog(source).images.get(id).map_or(1, |image| ((image.width as i64 + 2) / (2 * HALF_WIDTH)).max(1))
    };
    // De l'arrière vers l'avant.
    tiles.sort_by_key(|&(x, y, (source, id))| x + y + size(source, id) - 1);

    let screen = |x: i64, y: i64| ((x - y) * HALF_WIDTH, (x + y) * HALF_HEIGHT);
    let left = tiles.iter().map(|&(x, y, _)| screen(x, y).0).min().unwrap();
    let right = tiles.iter().map(|&(x, y, _)| screen(x, y).0).max().unwrap() + 2 * HALF_WIDTH;
    let top = tiles.iter().map(|&(x, y, _)| screen(x, y).1).min().unwrap() - 4 * HALF_HEIGHT;
    let bottom = tiles.iter().map(|&(x, y, _)| screen(x, y).1).max().unwrap() + 2 * HALF_HEIGHT;
    let (width, height) = ((right - left) as usize, (bottom - top) as usize);

    let mut rgba = vec![0u8; width * height * 4];
    for pixel in rgba.chunks_exact_mut(4) {
        pixel[3] = 255;
    }
    let mut cache: HashMap<(Source, usize), Option<Vec<u8>>> = HashMap::new();
    let mut drawn = 0;
    for &(x, y, (source, id)) in &tiles {
        let pixels = cache.entry((source, id)).or_insert_with(|| catalog(source).decode(id).ok());
        let (Some(pixels), Some(image)) = (pixels, catalog(source).images.get(id)) else {
            missing += 1;
            continue;
        };
        // L'image part du bord gauche de la case qui la porte, et son bas est
        // celui de l'emprise entière.
        let (sx, sy) = screen(x, y);
        let x0 = sx - left;
        let y0 = sy - top + (size(source, id) + 1) * HALF_HEIGHT - image.height as i64;
        for row in 0..image.height {
            let cy = y0 + row as i64;
            if cy < 0 || cy >= height as i64 {
                continue;
            }
            for column in 0..image.width {
                let cx = x0 + column as i64;
                let pixel = &pixels[(row * image.width + column) * 4..][..4];
                if pixel[3] != 0 && cx >= 0 && cx < width as i64 {
                    rgba[(cy as usize * width + cx as usize) * 4..][..4].copy_from_slice(pixel);
                }
            }
        }
        drawn += 1;
    }
    Ok(Picture { width, height, rgba, drawn, missing })
}
