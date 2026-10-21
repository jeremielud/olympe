//! Campagnes (`.pak`) : un en-tête décrivant les épisodes, puis les cartes de la
//! cité mère et des colonies, chacune au format `.map`.

use crate::goals::{Goals, modern_resource};
use crate::map::Map;

/// Version de l'en-tête (32 bits) : 5 pour Zeus, 26 pour Poséidon.
const VERSION_OFFSET: usize = 4;
/// À partir de cette version, les marchandises sont numérotées comme dans Poséidon.
const MODERN_RESOURCES_VERSION: i32 = 22;
/// Position, dans l'en-tête, de l'année de départ (16 bits signés) et du trésor.
const START_YEAR_OFFSET: usize = 7140;
const FUNDS_OFFSET: usize = 7172;
/// Numéros, dans `zeus_mm.eng`, du récit d'ouverture et du récit de victoire du
/// premier épisode.
const FIRST_MESSAGE_OFFSET: usize = 28;
const VICTORY_MESSAGE_OFFSET: usize = 40;
/// Liste des ressources du premier épisode (octets, terminée par un zéro).
const RESOURCES_OFFSET: usize = 9100;
const MAX_RESOURCES: usize = 16;

/// Réglages d'une aventure, lus au début d'un `.pak` ou d'un `.set` (même en-tête).
#[derive(Clone, Debug)]
pub struct Settings {
    pub episodes: usize,
    /// Année de départ ; négative avant notre ère.
    pub start_year: i32,
    /// Trésor de départ, en drachmes.
    pub funds: i32,
    /// Numéro, dans `zeus_mm.eng`, du récit du premier épisode ; négatif pour une
    /// aventure qui apporte ses textes dans un fichier à part.
    pub first_message: i32,
    /// Récit de victoire du premier épisode, même numérotation.
    pub victory_message: i32,
    /// Ressources que la cité mère peut produire au premier épisode : numéros du
    /// groupe de textes 23 (2 poisson, 5 carottes, 7 blé, 13 olives…). Vide quand
    /// l'aventure n'en déclare aucune.
    pub resources: Vec<u8>,
    /// Objectifs de chaque épisode ; vides si la table n'a pas pu être lue.
    pub goals: Goals,
}

impl Settings {
    pub fn parse(file: &[u8]) -> Option<Settings> {
        let int = |offset: usize| file.get(offset..offset + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let year = file.get(START_YEAR_OFFSET..START_YEAR_OFFSET + 2)?;
        let version = int(VERSION_OFFSET)?;
        // Zeus.exe renumérote au chargement les ressources des anciennes aventures.
        let resource = |r: u8| if version < MODERN_RESOURCES_VERSION { modern_resource(r) } else { r };
        Some(Settings {
            episodes: int(8)?.max(0) as usize,
            start_year: i16::from_le_bytes([year[0], year[1]]) as i32,
            funds: int(FUNDS_OFFSET)?,
            first_message: int(FIRST_MESSAGE_OFFSET)?,
            victory_message: int(VICTORY_MESSAGE_OFFSET)?,
            resources: file.get(RESOURCES_OFFSET..RESOURCES_OFFSET + MAX_RESOURCES).unwrap_or(&[]).iter().copied().take_while(|&r| r != 0).map(resource).collect(),
            goals: Goals::parse(file, version).unwrap_or_default(),
        })
    }
}

pub struct Campaign {
    pub settings: Option<Settings>,
    /// Cartes dans l'ordre du fichier. La première est une copie de travail ;
    /// la cité mère est la deuxième, les colonies suivent.
    pub maps: Vec<Map>,
}

impl Campaign {
    pub fn parse(file: &[u8]) -> Result<Campaign, String> {
        // L'en-tête n'est décodé qu'en partie : on repère les cartes par leur signature,
        // en vérifiant que chacune se lit jusqu'au bout.
        let mut maps = Vec::new();
        let mut pos = 0;
        while let Some(found) = file[pos..].windows(4).position(|w| w == b"MAPS") {
            let start = pos + found;
            match Map::parse_prefix(&file[start..]) {
                Ok((map, length)) => {
                    maps.push(map);
                    pos = start + length;
                }
                Err(_) => pos = start + 4,
            }
        }
        if maps.is_empty() {
            return Err("aucune carte dans la campagne".into());
        }
        Ok(Campaign { settings: Settings::parse(file).filter(|_| !file.starts_with(b"MAPS")), maps })
    }

    /// Carte de la cité mère.
    pub fn parent(&self) -> &Map {
        self.maps.get(1).unwrap_or(&self.maps[0])
    }
}
