//! Sprites : un catalogue `.sg3` décrit les images, dont les pixels (15 bits, 5-5-5)
//! sont stockés dans le fichier `.555` de même nom.

use std::collections::HashMap;
use std::path::Path;

const HEADER_SIZE: usize = 680;
const BITMAP_RECORD_SIZE: usize = 200;
/// Le catalogue réserve toujours la place de 200 planches.
const MAX_BITMAPS: usize = 200;
/// Couleur réservée à la transparence dans les images non compressées.
const TRANSPARENT: u16 = 0xF81F;
/// Rouge pur : dans les sprites, il marque l'ombre portée.
const SHADOW: u16 = 0x7C00;
const SHADOW_RGBA: [u8; 4] = [0, 0, 0, 90];
/// Nombre d'entrées de la table des groupes, dans l'en-tête.
const GROUPS: usize = 300;

/// Tuile de terrain en losange : 58 pixels de large, 30 de haut.
const TILE_WIDTH: usize = 58;
const TILE_HEIGHT: usize = 30;
const TILE_BYTES: usize = 1800;

/// Planche d'origine : un groupe d'images consécutives.
pub struct Bitmap {
    pub name: String,
    pub comment: String,
    pub first: usize,
    pub count: usize,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    /// Pixels bruts, ligne par ligne.
    Plain,
    /// Base en losange non compressée, surmontée d'une partie en plages.
    Isometric,
    /// Plages de pixels opaques et de sauts transparents.
    Sprite,
    Unknown(u16),
}

pub struct Image {
    pub width: usize,
    pub height: usize,
    pub kind: Kind,
    /// Numéro de la planche d'appartenance.
    pub bitmap: usize,
    /// Point d'ancrage : position, dans l'image, du point que le jeu place sur la carte.
    pub offset_x: i32,
    pub offset_y: i32,
    /// Nombre d'images d'animation qui suivent celle-ci (bâtiments animés).
    pub animation_frames: usize,
    /// Image sans pixels propres : c'est le reflet horizontal de celle-ci.
    pub mirror_of: Option<usize>,
    /// Les pixels sont dans le `.555` qui porte le nom de la planche, pas dans le fichier principal.
    external: bool,
    offset: usize,
    length: usize,
    /// Pour une image isométrique : taille de la base non compressée.
    base_length: usize,
}

pub struct Catalog {
    /// Première image de chaque groupe (0 = groupe absent).
    pub groups: Vec<usize>,
    pub bitmaps: Vec<Bitmap>,
    /// Les images sont numérotées à partir de 1 ; l'entrée 0 est vide.
    pub images: Vec<Image>,
    pixels: Vec<u8>,
    /// Fichiers `.555` externes, par numéro de planche.
    externals: HashMap<usize, Vec<u8>>,
}

impl Catalog {
    /// Charge un catalogue `.sg3` et le `.555` voisin.
    pub fn load(sg3: &Path) -> Result<Catalog, String> {
        let read = |path: &Path| std::fs::read(path).map_err(|e| format!("{} : {e}", path.display()));
        let index = read(sg3)?;
        // Certains catalogues n'ont que des images externes, donc pas de `.555` principal.
        let pixels = std::fs::read(sg3.with_extension("555")).unwrap_or_default();
        if index.len() < HEADER_SIZE {
            return Err("catalogue tronqué".into());
        }
        let version = le32(&index, 4);
        let image_count = le32(&index, 16) as usize;
        let bitmap_count = (le32(&index, 20) as usize).min(MAX_BITMAPS);
        // À partir de la version 0xD6, chaque image porte 8 octets de plus (masque alpha).
        let record_size = if version >= 0xD6 { 72 } else { 64 };

        let bitmap_at = |i: usize| index.get(HEADER_SIZE + i * BITMAP_RECORD_SIZE..).filter(|r| r.len() >= BITMAP_RECORD_SIZE);
        let mut bitmaps = Vec::new();
        for i in 0..bitmap_count {
            let record = bitmap_at(i).ok_or("table des planches tronquée")?;
            bitmaps.push(Bitmap {
                name: cstr(&record[..65]),
                comment: cstr(&record[65..116]),
                count: le32(record, 124) as usize,
                first: le32(record, 128) as usize,
            });
        }

        let start = HEADER_SIZE + MAX_BITMAPS * BITMAP_RECORD_SIZE;
        let mut images = Vec::new();
        for i in 0..=image_count {
            let record = index.get(start + i * record_size..).filter(|r| r.len() >= 64).ok_or("table des images tronquée")?;
            let kind = match le16(record, 50) {
                0 | 1 | 10 | 12 | 13 => Kind::Plain,
                30 => Kind::Isometric,
                256 | 257 | 276 => Kind::Sprite,
                other => Kind::Unknown(other),
            };
            images.push(Image {
                offset: le32(record, 0) as usize,
                length: le32(record, 4) as usize,
                base_length: le32(record, 8) as usize,
                width: le16(record, 20) as usize,
                height: le16(record, 22) as usize,
                kind,
                bitmap: record[56] as usize,
                external: record[52] != 0,
                animation_frames: le16(record, 30) as usize,
                offset_x: le16(record, 34) as i16 as i32,
                offset_y: le16(record, 36) as i16 as i32,
                mirror_of: match le32(record, 16) as i32 {
                    0 => None,
                    delta => i.checked_add_signed(delta as isize),
                },
            });
        }

        let mut externals = HashMap::new();
        for image in images.iter().filter(|i| i.external) {
            if let (false, Some(bitmap)) = (externals.contains_key(&image.bitmap), bitmaps.get(image.bitmap)) {
                let stem = bitmap.name.rsplit_once('.').map_or(bitmap.name.as_str(), |(stem, _)| stem);
                let file = sg3.with_file_name(format!("{}.555", stem.to_lowercase()));
                externals.insert(image.bitmap, read(&file)?);
            }
        }
        let groups = (0..GROUPS).map(|i| le16(&index, 80 + i * 2) as usize).collect();
        Ok(Catalog { groups, bitmaps, images, pixels, externals })
    }

    /// Certaines entrées du catalogue sont des emplacements vides.
    pub fn is_empty(&self, id: usize) -> bool {
        self.images.get(id).is_none_or(|i| i.width == 0 || i.height == 0 || (i.length == 0 && i.mirror_of.is_none()))
    }

    /// Décode une image en RVBA (4 octets par pixel).
    pub fn decode(&self, id: usize) -> Result<Vec<u8>, String> {
        let image = self.images.get(id).ok_or("image inexistante")?;
        if let (0, Some(original)) = (image.length, image.mirror_of) {
            if self.images.get(original).is_none_or(|o| o.length == 0 || (o.width, o.height) != (image.width, image.height)) {
                return Err("image miroir sans original valide".into());
            }
            let mut rgba = self.decode(original)?;
            for row in rgba.chunks_exact_mut(image.width * 4) {
                let pixels: Vec<[u8; 4]> = row.chunks_exact(4).rev().map(|p| [p[0], p[1], p[2], p[3]]).collect();
                row.copy_from_slice(pixels.as_flattened());
            }
            return Ok(rgba);
        }
        // Dans un fichier externe, les décalages sont comptés à partir de 1.
        let (source, offset) = match image.external {
            true => (self.externals.get(&image.bitmap).ok_or("fichier .555 externe absent")?, image.offset.saturating_sub(1)),
            false => (&self.pixels, image.offset),
        };
        let data = source.get(offset..offset + image.length).ok_or("données hors du fichier .555")?;
        let mut canvas =
            Canvas { width: image.width, height: image.height, rgba: vec![0; image.width * image.height * 4], shadows: false };
        match image.kind {
            Kind::Plain => {
                for (i, pixel) in data.chunks_exact(2).take(image.width * image.height).enumerate() {
                    canvas.put(i % image.width, i / image.width, u16::from_le_bytes([pixel[0], pixel[1]]));
                }
            }
            Kind::Sprite => {
                canvas.shadows = true;
                canvas.runs(data);
            }
            Kind::Isometric => {
                let base = data.get(..image.base_length).ok_or("base isométrique tronquée")?;
                canvas.diamond(base)?;
                canvas.runs(&data[image.base_length..]);
            }
            Kind::Unknown(kind) => return Err(format!("type d'image {kind} inconnu")),
        }
        Ok(canvas.rgba)
    }
}

struct Canvas {
    width: usize,
    height: usize,
    rgba: Vec<u8>,
    /// Traduire le rouge pur en ombre translucide.
    shadows: bool,
}

impl Canvas {
    fn put(&mut self, x: usize, y: usize, color: u16) {
        if color == TRANSPARENT || x >= self.width || y >= self.height {
            return;
        }
        // Passage de 5 à 8 bits en recopiant les bits de poids fort.
        let expand = |c: u16| ((c << 3) | (c >> 2)) as u8;
        let pixel = &mut self.rgba[(y * self.width + x) * 4..][..4];
        if self.shadows && color == SHADOW {
            pixel.copy_from_slice(&SHADOW_RGBA);
            return;
        }
        pixel.copy_from_slice(&[expand(color >> 10 & 31), expand(color >> 5 & 31), expand(color & 31), 255]);
    }

    /// Plages : un octet 255 suivi d'un nombre de pixels transparents à sauter,
    /// ou un nombre de pixels opaques suivi de ces pixels.
    fn runs(&mut self, data: &[u8]) {
        let (mut pos, mut i) = (0, 0);
        while pos < data.len() {
            let count = data[pos] as usize;
            pos += 1;
            if count == 255 {
                i += data.get(pos).copied().unwrap_or(0) as usize;
                pos += 1;
            } else {
                for pixel in data[pos..].chunks_exact(2).take(count) {
                    self.put(i % self.width, i / self.width, u16::from_le_bytes([pixel[0], pixel[1]]));
                    i += 1;
                }
                pos += count * 2;
            }
        }
    }

    /// Base d'un bâtiment de `size` × `size` cases : des tuiles en losange
    /// rangées elles-mêmes en losange, calées en bas de l'image.
    fn diamond(&mut self, data: &[u8]) -> Result<(), String> {
        let size = (self.width + 2) / (TILE_WIDTH + 2);
        if size == 0 || data.len() < size * size * TILE_BYTES {
            return Err("base isométrique incohérente".into());
        }
        let top = self.height.checked_sub(size * TILE_HEIGHT).ok_or("image plus basse que sa base")?;
        let mut tiles = data.chunks_exact(TILE_BYTES);
        for row in 0..2 * size - 1 {
            let count = if row < size { row + 1 } else { 2 * size - 1 - row };
            let left = (size - count) * (TILE_WIDTH + 2) / 2;
            for column in 0..count {
                let tile = tiles.next().unwrap();
                self.tile(tile, left + column * (TILE_WIDTH + 2), top + row * TILE_HEIGHT / 2);
            }
        }
        Ok(())
    }

    /// Une tuile : lignes de 2, 6, 10… 58 pixels, puis l'inverse.
    fn tile(&mut self, data: &[u8], x0: usize, y0: usize) {
        let mut pixels = data.chunks_exact(2).map(|p| u16::from_le_bytes([p[0], p[1]]));
        for y in 0..TILE_HEIGHT {
            let half = if y < TILE_HEIGHT / 2 { y } else { TILE_HEIGHT - 1 - y };
            let span = 2 + 4 * half;
            let inset = (TILE_WIDTH - span) / 2;
            for x in 0..span {
                let color = pixels.next().unwrap_or(TRANSPARENT);
                self.put(x0 + inset + x, y0 + y, color);
            }
        }
    }
}

fn le32(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
}

fn le16(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

/// Chaîne terminée par un zéro, en Latin-1.
fn cstr(bytes: &[u8]) -> String {
    bytes.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect()
}
