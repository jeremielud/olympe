//! Décompression « implode » de PKWare (bibliothèque DCL), utilisée par les cartes
//! et les sauvegardes. Seul le mode binaire (littéraux non codés) est géré : c'est
//! le seul que le jeu produit.

/// Tables de Huffman canoniques, décrites par des couples (longueur, répétitions).
struct Huffman {
    count: [u16; 16],
    symbol: Vec<u16>,
}

impl Huffman {
    fn new(compact: &[u8]) -> Huffman {
        let lengths: Vec<usize> =
            compact.iter().flat_map(|&b| std::iter::repeat_n((b & 15) as usize, (b >> 4) as usize + 1)).collect();
        let mut count = [0u16; 16];
        for &length in &lengths {
            count[length] += 1;
        }
        let mut offsets = [0usize; 16];
        for i in 1..16 {
            offsets[i] = offsets[i - 1] + count[i - 1] as usize;
        }
        let mut symbol = vec![0; lengths.len()];
        for (value, &length) in lengths.iter().enumerate() {
            if length != 0 {
                symbol[offsets[length]] = value as u16;
                offsets[length] += 1;
            }
        }
        Huffman { count, symbol }
    }
}

const LENGTH_CODES: [u8; 6] = [2, 35, 36, 53, 38, 23];
const DISTANCE_CODES: [u8; 7] = [2, 20, 53, 230, 247, 151, 248];
const LENGTH_BASE: [usize; 16] = [3, 2, 4, 5, 6, 7, 8, 9, 10, 12, 16, 24, 40, 72, 136, 264];
const LENGTH_EXTRA: [u32; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8];
/// Longueur réservée qui marque la fin du flux.
const END: usize = 519;

struct Bits<'a> {
    src: &'a [u8],
    pos: usize,
    buffer: u32,
    count: u32,
}

impl Bits<'_> {
    fn take(&mut self, n: u32) -> Result<u32, String> {
        while self.count < n {
            let byte = *self.src.get(self.pos).ok_or("flux compressé tronqué")?;
            self.buffer |= (byte as u32) << self.count;
            self.pos += 1;
            self.count += 8;
        }
        let value = self.buffer & ((1 << n) - 1);
        self.buffer >>= n;
        self.count -= n;
        Ok(value)
    }

    /// Les codes sont stockés bits inversés, du plus court au plus long.
    fn decode(&mut self, huffman: &Huffman) -> Result<usize, String> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for length in 1..16 {
            code |= (self.take(1)? ^ 1) as i32;
            let count = huffman.count[length] as i32;
            if code < first + count {
                return Ok(huffman.symbol[(index + code - first) as usize] as usize);
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err("code de Huffman invalide".into())
    }
}

pub fn explode(src: &[u8]) -> Result<Vec<u8>, String> {
    let (&coded, &dictionary) = src.first().zip(src.get(1)).ok_or("flux compressé vide")?;
    if coded != 0 {
        return Err("mode à littéraux codés non géré".into());
    }
    if !(4..=6).contains(&dictionary) {
        return Err("taille de dictionnaire invalide".into());
    }
    let lengths = Huffman::new(&LENGTH_CODES);
    let distances = Huffman::new(&DISTANCE_CODES);
    let mut bits = Bits { src, pos: 2, buffer: 0, count: 0 };
    let mut out = Vec::new();
    loop {
        if bits.take(1)? == 0 {
            out.push(bits.take(8)? as u8);
            continue;
        }
        let symbol = bits.decode(&lengths)?;
        let length = LENGTH_BASE[symbol] + bits.take(LENGTH_EXTRA[symbol])? as usize;
        if length == END {
            return Ok(out);
        }
        let symbol = bits.decode(&distances)?;
        let low_bits = if length == 2 { 2 } else { dictionary as u32 };
        let distance = (symbol << low_bits | bits.take(low_bits)? as usize) + 1;
        let start = out.len().checked_sub(distance).ok_or("référence avant le début des données")?;
        for i in 0..length {
            out.push(out[start + i]);
        }
    }
}
