//! Lecture d'une image de CD-ROM (ISO 9660, avec noms longs Joliet quand ils existent).
//! Accepte les images à secteurs de 2048 octets et les images brutes à 2352 octets.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Octets de données par secteur.
const SECTOR: usize = 2048;
/// Taille d'un secteur d'image brute, et début de ses données selon le mode.
const RAW_SECTOR: u64 = 2352;
const RAW_SYNC: [u8; 12] = [0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0];
const RAW_DATA_MODE1: u64 = 16;
const RAW_DATA_MODE2: u64 = 24;
/// Premier secteur des descripteurs de volume.
const FIRST_DESCRIPTOR: u32 = 16;
const DIRECTORY_FLAG: u8 = 2;

/// Fichier ou dossier de l'image.
#[derive(Clone, Debug)]
pub struct Entry {
    /// Chemin depuis la racine, séparé par des `/`.
    pub path: String,
    pub directory: bool,
    pub size: u64,
    sector: u32,
}

pub struct Image {
    file: File,
    /// Taille d'un secteur dans le fichier, et position des données dans ce secteur.
    sector_size: u64,
    data_offset: u64,
    /// Secteur et taille du dossier racine, et codage des noms (Joliet ou non).
    root: (u32, u64),
    joliet: bool,
}

impl Image {
    pub fn open(path: &Path) -> Result<Image, String> {
        let mut file = File::open(path).map_err(|e| format!("{} : {e}", path.display()))?;
        let mut head = [0u8; 16];
        file.read_exact(&mut head).map_err(|e| format!("{} : {e}", path.display()))?;
        // Une image brute commence par le motif de synchronisation ; l'octet 15 donne le mode.
        let (sector_size, data_offset) = match (head[..12] == RAW_SYNC, head[15]) {
            (true, 2) => (RAW_SECTOR, RAW_DATA_MODE2),
            (true, _) => (RAW_SECTOR, RAW_DATA_MODE1),
            (false, _) => (SECTOR as u64, 0),
        };
        let mut image = Image { file, sector_size, data_offset, root: (0, 0), joliet: false };
        // Descripteurs de volume : le principal (type 1), puis d'éventuels
        // supplémentaires (type 2) dont Joliet, jusqu'au terminateur (type 255).
        for sector in FIRST_DESCRIPTOR..FIRST_DESCRIPTOR + 32 {
            let descriptor = image.sector(sector)?;
            if &descriptor[1..6] != b"CD001" {
                break;
            }
            let root = (le32(&descriptor, 158), le32(&descriptor, 166) as u64);
            match descriptor[0] {
                1 if !image.joliet => image.root = root,
                // Joliet s'annonce par une séquence d'échappement « %/@ », « %/C » ou « %/E ».
                2 if descriptor[88..90] == *b"%/" && matches!(descriptor[90], b'@' | b'C' | b'E') => {
                    image.root = root;
                    image.joliet = true;
                }
                255 => break,
                _ => {}
            }
        }
        if image.root.0 == 0 {
            return Err(format!("{} : ce n'est pas une image ISO 9660", path.display()));
        }
        Ok(image)
    }

    fn sector(&mut self, sector: u32) -> Result<[u8; SECTOR], String> {
        let mut data = [0u8; SECTOR];
        let position = sector as u64 * self.sector_size + self.data_offset;
        let read = self.file.seek(SeekFrom::Start(position)).and_then(|_| self.file.read_exact(&mut data));
        read.map(|_| data).map_err(|e| format!("secteur {sector} illisible : {e}"))
    }

    /// Tous les fichiers et dossiers de l'image, les dossiers avant leur contenu.
    pub fn entries(&mut self) -> Result<Vec<Entry>, String> {
        let mut found = Vec::new();
        let mut pending = vec![(String::new(), self.root.0, self.root.1)];
        while let Some((folder, sector, size)) = pending.pop() {
            let mut data = Vec::with_capacity(size as usize);
            for index in 0..size.div_ceil(SECTOR as u64) as u32 {
                data.extend_from_slice(&self.sector(sector + index)?);
            }
            let mut position = 0;
            while position + 33 < data.len() {
                let length = data[position] as usize;
                // Un enregistrement ne chevauche pas deux secteurs : zéro = passer au suivant.
                if length == 0 {
                    position = (position / SECTOR + 1) * SECTOR;
                    continue;
                }
                let record = data.get(position..position + length).ok_or("dossier d'image tronqué")?;
                position += length;
                let name = record.get(33..33 + record[32] as usize).ok_or("nom de fichier tronqué")?;
                // Les deux premières entrées désignent le dossier lui-même et son parent.
                if name == [0] || name == [1] {
                    continue;
                }
                let directory = record[25] & DIRECTORY_FLAG != 0;
                let entry = Entry {
                    path: format!("{folder}{}", self.name(name)),
                    directory,
                    size: le32(record, 10) as u64,
                    sector: le32(record, 2),
                };
                if directory {
                    pending.push((format!("{}/", entry.path), entry.sector, entry.size));
                }
                found.push(entry);
            }
        }
        found.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(found)
    }

    /// Nom lisible d'une entrée : UCS-2 gros-boutiste en Joliet, sinon ASCII ; le
    /// numéro de version (« ;1 ») est retiré.
    fn name(&self, raw: &[u8]) -> String {
        let name: String = match self.joliet {
            true => char::decode_utf16(raw.chunks_exact(2).map(|pair| u16::from_be_bytes([pair[0], pair[1]]))).map(|c| c.unwrap_or('_')).collect(),
            false => raw.iter().map(|&byte| byte as char).collect(),
        };
        let name = name.split(';').next().unwrap_or_default();
        name.strip_suffix('.').unwrap_or(name).to_string()
    }

    /// Copie le contenu d'un fichier de l'image vers `out`.
    pub fn extract(&mut self, entry: &Entry, out: &mut dyn Write) -> Result<(), String> {
        let mut left = entry.size;
        let mut sector = entry.sector;
        while left > 0 {
            let data = self.sector(sector)?;
            let take = left.min(SECTOR as u64) as usize;
            out.write_all(&data[..take]).map_err(|e| e.to_string())?;
            left -= take as u64;
            sector += 1;
        }
        Ok(())
    }

    /// Contenu d'un fichier de l'image.
    pub fn read(&mut self, entry: &Entry) -> Result<Vec<u8>, String> {
        let mut data = Vec::with_capacity(entry.size as usize);
        self.extract(entry, &mut data)?;
        Ok(data)
    }
}

fn le32(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
}
