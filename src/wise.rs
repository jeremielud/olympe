//! Correctifs du jeu au format Wise : un installateur Windows qui renferme, pour chaque
//! fichier à corriger, un différentiel à appliquer sur la version déjà installée.
//!
//! Un différentiel commence par un en-tête (somme de contrôle et taille du fichier
//! avant et après), suivi d'une section de contrôle et d'une section de littéraux. Le
//! contrôle est une suite d'enregistrements « n littéraux, puis copie depuis l'ancien
//! fichier » ; le format a été retrouvé en confrontant les différentiels du correctif
//! 2.1 aux fichiers avant et après. Il est vérifié sur les trois fichiers de données du
//! correctif ; celui de `Zeus.exe`, dont le moteur n'a pas l'usage, emploie une forme
//! de copie qui reste à comprendre, et son application échoue proprement.

use crate::inflate::inflate_prefix;

/// Taille de l'en-tête d'un différentiel, et valeur fixe de son cinquième champ.
const HEADER: usize = 29;
const MARKER: u32 = 0x19;
/// Une copie vaut au moins trois octets : sa longueur est codée moins trois.
const MIN_COPY: usize = 3;

/// Différentiel reconnu : ce qu'il attend et ce qu'il produit.
pub struct Delta<'a> {
    pub old_crc: u32,
    pub old_size: usize,
    pub new_crc: u32,
    pub new_size: usize,
    control: &'a [u8],
    literals: &'a [u8],
}

impl<'a> Delta<'a> {
    /// Lit l'en-tête ; `None` si ces données ne sont pas un différentiel.
    pub fn parse(data: &'a [u8]) -> Option<Delta<'a>> {
        let int = |offset: usize| data.get(offset..offset + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        if *data.first()? != 1 || int(13)? != MARKER {
            return None;
        }
        let control_end = HEADER.checked_add(int(25)? as usize).filter(|&end| end <= data.len())?;
        Some(Delta {
            new_crc: int(5)?,
            new_size: int(9)? as usize,
            old_crc: int(17)?,
            old_size: int(21)? as usize,
            control: &data[HEADER..control_end],
            literals: &data[control_end..],
        })
    }

    /// Applique le différentiel à l'ancien fichier ; le résultat est vérifié par sa
    /// taille et sa somme de contrôle.
    pub fn apply(&self, old: &[u8]) -> Result<Vec<u8>, String> {
        if old.len() != self.old_size || crc32(old) != self.old_crc {
            return Err("le fichier n'est pas la version attendue par le correctif".into());
        }
        let mut out = Vec::with_capacity(self.new_size);
        let mut control = Reader { data: self.control, position: 0 };
        let mut literal = 0;
        // Curseur dans l'ancien fichier : copies et littéraux l'avancent tous deux.
        let mut cursor = 0usize;
        while out.len() < self.new_size {
            let Some(first) = control.byte() else {
                // Fin du contrôle : ce qui reste de littéraux termine le fichier.
                out.extend_from_slice(self.literals.get(literal..).unwrap_or(&[]));
                break;
            };
            // Forme courte (bit 7 nul) : quelques littéraux et une petite copie à la
            // suite. Forme longue : le nombre de littéraux, puis la copie à part.
            let (literals, copy) = match first & 0x80 {
                0 => ((first >> 4) as usize, Some(((first & 0x0F) as usize + MIN_COPY, true))),
                _ => {
                    let literals = control.number(first & 0x3F, first & 0x40 != 0)?;
                    let copy = match control.byte() {
                        Some(head) => {
                            let length = control.number(head & 0x3F, head & 0x80 != 0)? + MIN_COPY;
                            Some((length, head & 0x40 != 0))
                        }
                        None => None,
                    };
                    (literals, copy)
                }
            };
            let run = self.literals.get(literal..literal + literals).ok_or("littéraux du correctif épuisés")?;
            out.extend_from_slice(run);
            literal += literals;
            cursor += literals;
            if let Some((length, sequential)) = copy {
                // Une copie non séquentielle donne son déplacement par rapport au curseur.
                if !sequential {
                    let moved = cursor as i64 + control.signed()?;
                    cursor = usize::try_from(moved).map_err(|_| "copie avant le début de l'ancien fichier")?;
                }
                out.extend_from_slice(old.get(cursor..cursor + length).ok_or("copie hors de l'ancien fichier")?);
                cursor += length;
            }
        }
        out.truncate(self.new_size);
        if out.len() != self.new_size || crc32(&out) != self.new_crc {
            return Err("le résultat du correctif ne correspond pas à sa somme de contrôle".into());
        }
        Ok(out)
    }
}

struct Reader<'a> {
    data: &'a [u8],
    position: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Option<u8> {
        let byte = *self.data.get(self.position)?;
        self.position += 1;
        Some(byte)
    }

    /// Nombre commencé par `head` : s'il se poursuit, chaque octet suivant apporte sept
    /// bits, le bit 7 annonçant encore une suite.
    fn number(&mut self, head: u8, mut more: bool) -> Result<usize, String> {
        let mut value = head as usize;
        while more {
            let byte = self.byte().ok_or("contrôle du correctif tronqué")?;
            value = value << 7 | (byte & 0x7F) as usize;
            more = byte & 0x80 != 0;
        }
        Ok(value)
    }

    /// Déplacement signé : groupes de sept bits, poids fort d'abord, signe étendu.
    fn signed(&mut self) -> Result<i64, String> {
        let (mut value, mut bits) = (0i64, 0);
        loop {
            let byte = self.byte().ok_or("contrôle du correctif tronqué")?;
            value = value << 7 | (byte & 0x7F) as i64;
            bits += 7;
            if byte & 0x80 == 0 {
                break;
            }
        }
        Ok(if value >> (bits - 1) & 1 == 1 { value - (1 << bits) } else { value })
    }
}

/// Fichiers renfermés dans un installateur Wise : une suite de flux deflate, chacun
/// suivi de sa somme de contrôle, placée après le programme lui-même.
pub fn streams(installer: &[u8]) -> Vec<Vec<u8>> {
    let int = |offset: usize| installer.get(offset..offset + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let short = |offset: usize| installer.get(offset..offset + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
    // Fin de la dernière section du programme Windows.
    let end = (|| {
        let pe = int(0x3C)?;
        let (sections, optional) = (short(pe + 6)?, short(pe + 20)?);
        (0..sections).map(|i| Some(int(pe + 24 + optional + i * 40 + 16)? + int(pe + 24 + optional + i * 40 + 20)?)).collect::<Option<Vec<_>>>()?.into_iter().max()
    })();
    let Some(end) = end else { return Vec::new() };
    let valid = |start: usize| -> Option<(Vec<u8>, usize)> {
        let mut data = Vec::new();
        let used = inflate_prefix(installer.get(start..)?, &mut data).ok()?;
        (data.len() > 16 && int(start + used)? as u32 == crc32(&data)).then_some((data, used))
    };
    // Le premier flux suit de près la fin du programme.
    let Some(mut position) = (end..end + 0x400).find(|&start| valid(start).is_some()) else { return Vec::new() };
    let mut found = Vec::new();
    while let Some((data, used)) = valid(position) {
        found.push(data);
        position += used + 4;
    }
    found
}

/// Somme de contrôle CRC-32 (celle de zlib).
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { crc >> 1 ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}
