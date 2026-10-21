//! Archives InstallShield (`data1.hdr`, `data1.cab`, `data2.cab`…) des CD du jeu.
//! Versions 5 (Zeus : l'en-tête est au début de l'unique `data1.cab`) et 6 (Poséidon :
//! en-tête dans `data1.hdr`, données réparties sur plusieurs volumes).

use crate::inflate::inflate;

const SIGNATURE: u32 = 0x2863_5349;
/// Drapeaux d'un fichier.
const FLAG_SPLIT: u16 = 1;
const FLAG_OBFUSCATED: u16 = 2;
const FLAG_COMPRESSED: u16 = 4;
const FLAG_INVALID: u16 = 8;
/// Le fichier partage ses données avec celui que désigne `link_previous`.
const LINK_PREVIOUS: u8 = 1;
/// Taille d'un descripteur de fichier en version 6.
const DESCRIPTOR_V6: usize = 0x57;
/// Entrées de la table des groupes de fichiers.
const FILE_GROUP_SLOTS: usize = 71;

/// Fichier d'une archive.
#[derive(Clone, Debug)]
pub struct Member {
    /// Chemin dans l'archive, séparé par des `/`.
    pub path: String,
    pub size: u64,
    /// Numéro du volume (`data<N>.cab`) qui porte les données.
    pub volume: u16,
    compressed: bool,
    packed_size: u64,
    offset: u64,
}

/// Table des fichiers d'une archive.
pub struct Cabinet {
    pub members: Vec<Member>,
}

struct Raw {
    name: usize,
    directory: usize,
    flags: u16,
    size: u64,
    packed_size: u64,
    offset: u64,
    volume: u16,
    link_previous: Option<usize>,
}

impl Cabinet {
    /// Lit la table des fichiers dans l'en-tête de l'archive : le contenu de
    /// `data1.hdr`, ou le début de `data1.cab` quand il n'y a pas de `.hdr`.
    pub fn parse(header: &[u8]) -> Result<Cabinet, String> {
        let int = |offset: usize| header.get(offset..offset + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or("en-tête d'archive tronqué");
        let short = |offset: usize| header.get(offset..offset + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or("en-tête d'archive tronqué");
        let long = |offset: usize| Ok::<u64, &str>(int(offset)? as u64 | (int(offset + 4)? as u64) << 32);
        if int(0)? != SIGNATURE {
            return Err("ce n'est pas une archive InstallShield".into());
        }
        // Le numéro majeur est rangé de deux façons selon l'époque ; zéro vaut 5.
        let version = int(4)?;
        let major = match version >> 24 {
            1 => (version >> 12) & 0xF,
            2 | 4 => (version & 0xFFFF) / 100,
            _ => 0,
        };
        let major = if major == 0 { 5 } else { major };
        if major > 6 {
            return Err(format!("archive InstallShield {major} non gérée"));
        }
        let descriptor = int(12)? as usize;
        let table = descriptor + int(descriptor + 0x0C)? as usize;
        let directories = int(descriptor + 0x1C)? as usize;
        let count = int(descriptor + 0x28)? as usize;
        let table2 = int(descriptor + 0x2C)? as usize;
        let text = |offset: usize| -> Result<String, String> {
            let bytes = header.get(table + offset..).ok_or("nom hors de l'en-tête d'archive")?;
            Ok(bytes.iter().take_while(|&&b| b != 0).map(|&b| crate::text::cp1252(b)).collect())
        };

        let mut raw = Vec::with_capacity(count);
        for index in 0..count {
            raw.push(match major {
                5 => {
                    let at = table + int(table + 4 * (directories + index))? as usize;
                    Raw {
                        name: int(at)? as usize,
                        directory: int(at + 4)? as usize,
                        flags: short(at + 8)?,
                        size: int(at + 0x0A)? as u64,
                        packed_size: int(at + 0x0E)? as u64,
                        offset: int(at + 0x26)? as u64,
                        volume: 1,
                        link_previous: None,
                    }
                }
                _ => {
                    let at = table + table2 + index * DESCRIPTOR_V6;
                    let linked = header.get(at + 0x54).is_some_and(|flags| flags & LINK_PREVIOUS != 0);
                    Raw {
                        flags: short(at)?,
                        size: long(at + 0x02)?,
                        packed_size: long(at + 0x0A)?,
                        offset: long(at + 0x12)?,
                        name: int(at + 0x3A)? as usize,
                        directory: short(at + 0x3E)? as usize,
                        link_previous: linked.then_some(int(at + 0x4C)? as usize),
                        volume: short(at + 0x55)?,
                    }
                }
            });
        }

        // Seuls comptent les fichiers rattachés à un groupe : la table garde aussi des
        // descripteurs périmés. Les groupes sont en listes chaînées, à partir d'une
        // table de 71 entrées du descripteur d'archive.
        let mut grouped = vec![false; count];
        for slot in 0..FILE_GROUP_SLOTS {
            let mut link = int(descriptor + 0x3E + 4 * slot)? as usize;
            while link != 0 {
                // Maillon : nom du groupe, descripteur du groupe, maillon suivant.
                let group = descriptor + int(descriptor + link + 4)? as usize;
                link = int(descriptor + link + 8)? as usize;
                let range = group + 4 + if major == 5 { 0x48 } else { 0x12 };
                let (first, last) = (int(range)? as usize, int(range + 4)? as usize);
                grouped.iter_mut().take(last.saturating_add(1)).skip(first).for_each(|member| *member = true);
            }
        }

        let mut members = Vec::new();
        for (file, _) in raw.iter().zip(&grouped).filter(|&(_, &grouped)| grouped) {
            if file.flags & FLAG_INVALID != 0 || file.name == 0 || file.size == 0 {
                continue;
            }
            // Sans position dans l'archive : le fichier est resté tel quel sur le CD
            // (musique, voix, vidéos) et l'archive ne fait que le recenser.
            if file.offset == 0 && file.link_previous.is_none() {
                continue;
            }
            if file.flags & (FLAG_SPLIT | FLAG_OBFUSCATED) != 0 {
                return Err("archive InstallShield à fichiers scindés ou brouillés : non gérée".into());
            }
            // Un fichier lié prend ses données chez le premier de sa chaîne.
            let mut data = file;
            for _ in 0..raw.len() {
                match data.link_previous.and_then(|previous| raw.get(previous)) {
                    Some(previous) => data = previous,
                    None => break,
                }
            }
            let folder = match file.directory < directories {
                true => text(int(table + 4 * file.directory)? as usize)?.replace('\\', "/"),
                false => String::new(),
            };
            let name = text(file.name)?;
            members.push(Member {
                path: if folder.is_empty() { name } else { format!("{folder}/{name}") },
                size: file.size,
                volume: data.volume.max(1),
                compressed: data.flags & FLAG_COMPRESSED != 0,
                packed_size: data.packed_size,
                offset: data.offset,
            });
        }
        Ok(Cabinet { members })
    }
}

impl Member {
    /// Contenu du fichier, lu dans son volume (le `data<N>.cab` entier).
    pub fn extract(&self, volume: &[u8]) -> Result<Vec<u8>, String> {
        let end = self.offset.checked_add(self.packed_size).filter(|&end| end <= volume.len() as u64);
        let packed = &volume[self.offset as usize..end.ok_or_else(|| format!("{} : données hors du volume", self.path))? as usize];
        if !self.compressed {
            return Ok(packed.to_vec());
        }
        // Suite de blocs « longueur sur 16 bits + flux deflate indépendant ».
        let mut out = Vec::with_capacity(self.size as usize);
        let mut position = 0;
        while position + 2 <= packed.len() && (out.len() as u64) < self.size {
            let length = u16::from_le_bytes([packed[position], packed[position + 1]]) as usize;
            position += 2;
            let block = packed.get(position..position + length).ok_or_else(|| format!("{} : bloc compressé tronqué", self.path))?;
            inflate(block, &mut out).map_err(|e| format!("{} : {e}", self.path))?;
            position += length;
        }
        if out.len() as u64 != self.size {
            return Err(format!("{} : {} octets obtenus au lieu de {}", self.path, out.len(), self.size));
        }
        Ok(out)
    }
}
