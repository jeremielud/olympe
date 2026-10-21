//! Décodeur JPEG réduit à ce qu'utilisent les fonds d'écran du jeu : codage de base
//! (Huffman, 8 bits), trois composantes sans sous-échantillonnage, intervalles de reprise.

/// Ordre en zigzag des coefficients d'un bloc.
const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21, 28, 35, 42,
    49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

pub struct Picture {
    pub width: usize,
    pub height: usize,
    /// Pixels RVBA, opaques.
    pub rgba: Vec<u8>,
}

/// Table de Huffman : pour chaque longueur de code, le premier code, le nombre de
/// codes et le rang du premier symbole.
#[derive(Clone, Default)]
struct Huffman {
    counts: [u16; 17],
    symbols: Vec<u8>,
}

#[derive(Clone, Copy, Default)]
struct Component {
    quantization: usize,
    dc_table: usize,
    ac_table: usize,
    /// Dernière valeur continue décodée : chaque bloc code l'écart avec le précédent.
    previous_dc: i32,
}

/// Lecteur de bits des données compressées, qui retire les octets de bourrage.
struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    buffer: u32,
    count: u32,
}

impl Bits<'_> {
    fn bit(&mut self) -> u32 {
        if self.count == 0 {
            let byte = self.data.get(self.pos).copied().unwrap_or(0);
            self.pos += 1;
            // Un octet FF est suivi d'un 00 de bourrage ; devant un marqueur, on s'arrête.
            if byte == 0xFF {
                match self.data.get(self.pos) {
                    Some(0) => self.pos += 1,
                    _ => self.pos -= 1,
                }
            }
            self.buffer = byte as u32;
            self.count = 8;
        }
        self.count -= 1;
        (self.buffer >> self.count) & 1
    }

    fn bits(&mut self, count: u32) -> i32 {
        (0..count).fold(0, |value, _| (value << 1) | self.bit() as i32)
    }

    /// Valeur signée codée sur `size` bits, à la manière de JPEG.
    fn signed(&mut self, size: u32) -> i32 {
        if size == 0 {
            return 0;
        }
        let value = self.bits(size);
        if value < 1 << (size - 1) { value - (1 << size) + 1 } else { value }
    }

    fn symbol(&mut self, table: &Huffman) -> Result<u8, String> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0usize);
        for length in 1..=16 {
            code |= self.bit() as i32;
            let count = table.counts[length] as i32;
            if code - first < count {
                return table.symbols.get(index + (code - first) as usize).copied().ok_or_else(|| "code de Huffman hors table".into());
            }
            index += count as usize;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err("code de Huffman invalide".into())
    }

    /// Passe le marqueur de reprise attendu et repart sur un octet entier.
    fn restart(&mut self) {
        self.count = 0;
        if self.data.get(self.pos) == Some(&0xFF) && matches!(self.data.get(self.pos + 1), Some(0xD0..=0xD7)) {
            self.pos += 2;
        }
    }
}

pub fn decode(file: &[u8]) -> Result<Picture, String> {
    if !file.starts_with(&[0xFF, 0xD8]) {
        return Err("ce n'est pas un JPEG".into());
    }
    let mut quantization = [[0u16; 64]; 4];
    let mut tables = vec![Huffman::default(); 8];
    let mut components = [Component::default(); 3];
    let (mut width, mut height, mut restart_interval) = (0usize, 0usize, 0usize);
    let mut pos = 2;
    loop {
        let header = file.get(pos..pos + 4).ok_or("JPEG tronqué")?;
        if header[0] != 0xFF {
            return Err("marqueur JPEG attendu".into());
        }
        let (marker, length) = (header[1], u16::from_be_bytes([header[2], header[3]]) as usize);
        let segment = file.get(pos + 4..pos + 2 + length).ok_or("segment JPEG tronqué")?;
        pos += 2 + length;
        match marker {
            // Tables de quantification (8 bits), rangées en zigzag.
            0xDB => {
                for table in segment.chunks_exact(65) {
                    if table[0] > 3 {
                        return Err("table de quantification 16 bits non gérée".into());
                    }
                    for (i, &value) in table[1..].iter().enumerate() {
                        quantization[table[0] as usize][ZIGZAG[i]] = value as u16;
                    }
                }
            }
            0xC0 => {
                if segment.len() < 15 || segment[0] != 8 || segment[5] != 3 {
                    return Err("JPEG autre que 8 bits à trois composantes".into());
                }
                height = u16::from_be_bytes([segment[1], segment[2]]) as usize;
                width = u16::from_be_bytes([segment[3], segment[4]]) as usize;
                for (i, component) in components.iter_mut().enumerate() {
                    let description = &segment[6 + 3 * i..9 + 3 * i];
                    if description[1] != 0x11 {
                        return Err("JPEG sous-échantillonné non géré".into());
                    }
                    component.quantization = (description[2] & 3) as usize;
                }
            }
            0xC1..=0xCF if marker != 0xC4 && marker != 0xC8 && marker != 0xCC => {
                return Err("JPEG progressif ou étendu non géré".into());
            }
            0xC4 => {
                let mut rest = segment;
                while rest.len() >= 17 {
                    let mut table = Huffman::default();
                    for length in 1..=16 {
                        table.counts[length] = rest[length] as u16;
                    }
                    let total: usize = table.counts.iter().map(|&c| c as usize).sum();
                    table.symbols = rest.get(17..17 + total).ok_or("table de Huffman tronquée")?.to_vec();
                    // Classe (0 = continue, 1 = alternative) et numéro de table.
                    tables[((rest[0] >> 4) as usize & 1) * 4 + (rest[0] as usize & 3)] = table;
                    rest = &rest[17 + total..];
                }
            }
            0xDD => restart_interval = u16::from_be_bytes([segment[0], segment[1]]) as usize,
            0xDA => {
                if segment.len() < 7 || segment[0] != 3 {
                    return Err("balayage JPEG partiel non géré".into());
                }
                for (i, component) in components.iter_mut().enumerate() {
                    let choice = segment[2 + 2 * i];
                    component.dc_table = (choice >> 4) as usize & 3;
                    component.ac_table = 4 + (choice as usize & 3);
                }
                break;
            }
            _ => {}
        }
    }
    if width == 0 || height == 0 {
        return Err("dimensions JPEG absentes".into());
    }

    let mut bits = Bits { data: &file[pos..], pos: 0, buffer: 0, count: 0 };
    let (columns, rows) = (width.div_ceil(8), height.div_ceil(8));
    let mut rgba = vec![255u8; width * height * 4];
    let cosines: [[f32; 8]; 8] = std::array::from_fn(|x| {
        std::array::from_fn(|u| {
            let scale = if u == 0 { (0.5f32).sqrt() } else { 1.0 };
            scale * ((2 * x + 1) as f32 * u as f32 * std::f32::consts::PI / 16.0).cos() / 2.0
        })
    });
    let mut planes = [[0f32; 64]; 3];
    for block in 0..columns * rows {
        if restart_interval > 0 && block > 0 && block % restart_interval == 0 {
            bits.restart();
            components.iter_mut().for_each(|c| c.previous_dc = 0);
        }
        for (component, plane) in components.iter_mut().zip(planes.iter_mut()) {
            let mut coefficients = [0f32; 64];
            let table = &quantization[component.quantization];
            let size = bits.symbol(&tables[component.dc_table])? as u32;
            component.previous_dc += bits.signed(size);
            coefficients[0] = (component.previous_dc * table[0] as i32) as f32;
            let mut index = 1;
            while index < 64 {
                let symbol = bits.symbol(&tables[component.ac_table])?;
                let (run, size) = ((symbol >> 4) as usize, (symbol & 15) as u32);
                if size == 0 {
                    // 0x00 : fin du bloc ; 0xF0 : seize zéros.
                    if run != 15 {
                        break;
                    }
                    index += 16;
                    continue;
                }
                index += run;
                if index >= 64 {
                    return Err("coefficients JPEG hors bloc".into());
                }
                coefficients[ZIGZAG[index]] = (bits.signed(size) * table[ZIGZAG[index]] as i32) as f32;
                index += 1;
            }
            // Transformée en cosinus inverse, lignes puis colonnes.
            let mut half = [0f32; 64];
            for v in 0..8 {
                for x in 0..8 {
                    half[v * 8 + x] = (0..8).map(|u| coefficients[v * 8 + u] * cosines[x][u]).sum();
                }
            }
            for y in 0..8 {
                for x in 0..8 {
                    plane[y * 8 + x] = (0..8).map(|v| half[v * 8 + x] * cosines[y][v]).sum::<f32>() + 128.0;
                }
            }
        }
        let (left, top) = (block % columns * 8, block / columns * 8);
        for y in 0..8.min(height - top) {
            for x in 0..8.min(width - left) {
                let (luma, blue, red) = (planes[0][y * 8 + x], planes[1][y * 8 + x] - 128.0, planes[2][y * 8 + x] - 128.0);
                let pixel = &mut rgba[((top + y) * width + left + x) * 4..][..3];
                pixel[0] = (luma + 1.402 * red).round().clamp(0.0, 255.0) as u8;
                pixel[1] = (luma - 0.344_136 * blue - 0.714_136 * red).round().clamp(0.0, 255.0) as u8;
                pixel[2] = (luma + 1.772 * blue).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    Ok(Picture { width, height, rgba })
}
