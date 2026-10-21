//! Textes du jeu (`zeus_text.eng`) : des groupes de chaînes terminées par un zéro.

const HEADER_SIZE: usize = 28;
const MAX_GROUPS: usize = 1000;

pub struct Text {
    /// Décalage du début de chaque groupe dans `data`, s'il est utilisé.
    groups: Vec<Option<usize>>,
    data: Vec<u8>,
}

impl Text {
    pub fn parse(file: &[u8]) -> Result<Text, String> {
        let data_start = HEADER_SIZE + MAX_GROUPS * 8;
        if file.len() < data_start || !file.starts_with(b"Zeus textfile.") {
            return Err("fichier de textes invalide".into());
        }
        let entry = |i: usize| &file[HEADER_SIZE + i * 8..][..8];
        let le32 = |b: &[u8]| u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        let groups = (0..MAX_GROUPS).map(|i| (le32(&entry(i)[4..]) != 0).then(|| le32(entry(i)) as usize)).collect();
        Ok(Text { groups, data: file[data_start..].to_vec() })
    }

    /// Chaîne numéro `index` du groupe `group`, ou une chaîne vide.
    pub fn get(&self, group: usize, index: usize) -> String {
        let Some(&Some(start)) = self.groups.get(group) else { return String::new() };
        let Some(data) = self.data.get(start..) else { return String::new() };
        let bytes = data.split(|&b| b == 0).nth(index).unwrap_or(&[]);
        bytes.iter().map(|&b| cp1252(b)).collect()
    }
}

/// Windows-1252 vers Unicode (les octets 0x80–0x9F diffèrent de Latin-1).
pub fn cp1252(byte: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘', '’',
        '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
    ];
    match byte {
        0x80..=0x9F => HIGH[(byte - 0x80) as usize],
        _ => byte as char,
    }
}
