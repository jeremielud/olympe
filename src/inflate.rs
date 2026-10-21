//! Décompression « deflate » brute (RFC 1951), sans en-tête zlib ni somme de contrôle :
//! c'est sous cette forme que les archives InstallShield rangent leurs fichiers.

/// Longueurs de base et bits supplémentaires des codes de longueur 257 à 285.
const LENGTH_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LENGTH_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
/// Distances de base et bits supplémentaires des trente codes de distance.
const DISTANCE_BASE: [u16; 30] =
    [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DISTANCE_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];
/// Ordre dans lequel sont transmises les longueurs des codes de longueur.
const CODE_LENGTH_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
const MAX_BITS: usize = 15;

/// Code de Huffman canonique : nombre de codes par longueur, et symboles rangés par
/// longueur puis par valeur.
struct Huffman {
    counts: [u16; MAX_BITS + 1],
    symbols: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Huffman {
        let mut counts = [0u16; MAX_BITS + 1];
        for &length in lengths {
            counts[length as usize] += 1;
        }
        counts[0] = 0;
        let mut offsets = [0u16; MAX_BITS + 2];
        for bits in 1..=MAX_BITS {
            offsets[bits + 1] = offsets[bits] + counts[bits];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (symbol, &length) in lengths.iter().enumerate() {
            if length != 0 {
                symbols[offsets[length as usize] as usize] = symbol as u16;
                offsets[length as usize] += 1;
            }
        }
        Huffman { counts, symbols }
    }
}

struct Bits<'a> {
    data: &'a [u8],
    position: usize,
    buffer: u32,
    count: u32,
}

impl Bits<'_> {
    fn bits(&mut self, need: u32) -> Result<u32, String> {
        while self.count < need {
            let byte = *self.data.get(self.position).ok_or("données compressées tronquées")?;
            self.position += 1;
            self.buffer |= (byte as u32) << self.count;
            self.count += 8;
        }
        let value = self.buffer & ((1u32 << need) - 1);
        self.buffer >>= need;
        self.count -= need;
        Ok(value)
    }

    fn symbol(&mut self, code: &Huffman) -> Result<usize, String> {
        let (mut value, mut first, mut index) = (0i32, 0i32, 0i32);
        for bits in 1..=MAX_BITS {
            value |= self.bits(1)? as i32;
            let count = code.counts[bits] as i32;
            if value - count < first {
                return Ok(code.symbols[(index + value - first) as usize] as usize);
            }
            index += count;
            first = (first + count) << 1;
            value <<= 1;
        }
        Err("code de Huffman invalide".into())
    }
}

/// Décompresse un flux deflate et ajoute le résultat à `out`.
pub fn inflate(data: &[u8], out: &mut Vec<u8>) -> Result<(), String> {
    inflate_prefix(data, out).map(|_| ())
}

/// Comme `inflate`, pour un flux suivi d'autres données : rend le nombre d'octets
/// qu'occupait le flux.
pub fn inflate_prefix(data: &[u8], out: &mut Vec<u8>) -> Result<usize, String> {
    let mut bits = Bits { data, position: 0, buffer: 0, count: 0 };
    loop {
        let last = bits.bits(1)? == 1;
        match bits.bits(2)? {
            // Bloc recopié tel quel : on repart sur un octet entier.
            0 => {
                bits.buffer = 0;
                bits.count = 0;
                let header = data.get(bits.position..bits.position + 4).ok_or("bloc non compressé tronqué")?;
                let length = u16::from_le_bytes([header[0], header[1]]) as usize;
                if length != !u16::from_le_bytes([header[2], header[3]]) as usize {
                    return Err("bloc non compressé incohérent".into());
                }
                let start = bits.position + 4;
                out.extend_from_slice(data.get(start..start + length).ok_or("bloc non compressé tronqué")?);
                bits.position = start + length;
            }
            // Codes fixes.
            1 => {
                let mut lengths = [8u8; 288];
                lengths[144..256].fill(9);
                lengths[256..280].fill(7);
                block(&mut bits, out, &Huffman::new(&lengths), &Huffman::new(&[5u8; 30]))?;
            }
            // Codes transmis avec le bloc.
            2 => {
                let literals = bits.bits(5)? as usize + 257;
                let distances = bits.bits(5)? as usize + 1;
                let code_lengths = bits.bits(4)? as usize + 4;
                let mut lengths = [0u8; 19];
                for &slot in &CODE_LENGTH_ORDER[..code_lengths] {
                    lengths[slot] = bits.bits(3)? as u8;
                }
                let length_code = Huffman::new(&lengths);
                let mut all = vec![0u8; literals + distances];
                let mut index = 0;
                while index < all.len() {
                    let symbol = bits.symbol(&length_code)?;
                    // 16 répète la longueur précédente, 17 et 18 une suite de zéros.
                    let (value, repeat) = match symbol {
                        0..=15 => (symbol as u8, 1),
                        16 => (*all.get(index.wrapping_sub(1)).ok_or("répétition sans longueur précédente")?, 3 + bits.bits(2)? as usize),
                        17 => (0, 3 + bits.bits(3)? as usize),
                        _ => (0, 11 + bits.bits(7)? as usize),
                    };
                    all.get_mut(index..index + repeat).ok_or("trop de longueurs de code")?.fill(value);
                    index += repeat;
                }
                block(&mut bits, out, &Huffman::new(&all[..literals]), &Huffman::new(&all[literals..]))?;
            }
            _ => return Err("type de bloc deflate inconnu".into()),
        }
        if last {
            // Les bits lus d'avance mais non consommés appartiennent à la suite.
            return Ok(bits.position - (bits.count / 8) as usize);
        }
    }
}

/// Décode un bloc compressé : littéraux, ou recopie d'une séquence déjà sortie.
fn block(bits: &mut Bits, out: &mut Vec<u8>, literal_code: &Huffman, distance_code: &Huffman) -> Result<(), String> {
    loop {
        let symbol = bits.symbol(literal_code)?;
        match symbol {
            0..=255 => out.push(symbol as u8),
            256 => return Ok(()),
            _ => {
                let slot = symbol - 257;
                let base = *LENGTH_BASE.get(slot).ok_or("code de longueur invalide")?;
                let length = base as usize + bits.bits(LENGTH_EXTRA[slot] as u32)? as usize;
                let slot = bits.symbol(distance_code)?;
                let base = *DISTANCE_BASE.get(slot).ok_or("code de distance invalide")?;
                let distance = base as usize + bits.bits(DISTANCE_EXTRA[slot] as u32)? as usize;
                let start = out.len().checked_sub(distance).ok_or("recopie avant le début des données")?;
                // Octet par octet : la source peut chevaucher ce qu'on écrit.
                for index in start..start + length {
                    out.push(out[index]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::inflate;

    fn unpack(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        inflate(data, &mut out).unwrap();
        out
    }

    #[test]
    fn stored_block() {
        assert_eq!(unpack(&[1, 3, 0, 0xFC, 0xFF, b'a', b'b', b'c']), b"abc");
    }

    #[test]
    fn fixed_codes_with_a_repeat() {
        // « aaaaaaaaaa » compressé par zlib (deflate brut, codes fixes).
        assert_eq!(unpack(&[0x4B, 0x4C, 0x84, 0x01, 0x00]), b"aaaaaaaaaa");
    }

    #[test]
    fn overlapping_copy() {
        // Trois cents « ab » puis « c », compressés par zlib : la recopie chevauche
        // ce qu'elle écrit.
        let packed = [75, 76, 74, 28, 133, 163, 144, 234, 48, 25, 0];
        let mut expected = b"ab".repeat(300);
        expected.push(b'c');
        assert_eq!(unpack(&packed), expected);
    }

    #[test]
    fn truncated_data_is_an_error() {
        assert!(inflate(&[0x4B, 0x4C], &mut Vec::new()).is_err());
    }
}
