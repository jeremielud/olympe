//! Campagnes (`.pak`) : un en-tête décrivant les épisodes, puis les cartes de la
//! cité mère et des colonies, chacune au format `.map`.

use crate::goals::{Goal, Goals, modern_resource};
use crate::map::Map;

/// Version de l'en-tête (32 bits) : 5 pour Zeus, 26 pour Poséidon.
const VERSION_OFFSET: usize = 4;
/// Nombres d'épisodes de la cité mère et des colonies (32 bits).
const PARENT_EPISODES_OFFSET: usize = 8;
const COLONY_EPISODES_OFFSET: usize = 16;
/// À partir de cette version, les marchandises sont numérotées comme dans Poséidon.
const MODERN_RESOURCES_VERSION: i32 = 22;
/// Fiches courtes des épisodes (récits, suite), dix pour la cité mère puis dix pour
/// les colonies.
const PARENT_HEADERS: usize = 20;
const COLONY_HEADERS: usize = 3580;
const HEADER_SIZE: usize = 356;
/// Dans une fiche courte : récit d'ouverture et récit de victoire (numéros de
/// `zeus_mm.eng`), et ce qui suit l'épisode.
const INTRO_MESSAGE: usize = 8;
const VICTORY_MESSAGE: usize = 20;
const NEXT: usize = 84;
/// Fiches complètes des épisodes, de la cité mère puis des colonies.
const PARENT_RECORDS: usize = 7140;
const COLONY_RECORDS: usize = 27460;
const RECORD_SIZE: usize = 2032;
/// Dans une fiche complète : année de départ (16 bits signés), trésor, puis la liste
/// des ressources (octets, terminée par un zéro).
const START_YEAR: usize = 0;
const FUNDS: usize = 32;
const RESOURCES: usize = 1960;
const MAX_RESOURCES: usize = 16;
/// Mythologie de chaque épisode (dix de la cité mère, puis quatre de colonie), une
/// table par épisode : 224 octets dans Zeus, 300 dans Poséidon. On y lit les six dieux
/// favorables proches (entiers), si leur sanctuaire est permis (octets), et le nombre
/// de sanctuaires permis à la fois.
const MYTHOLOGY: usize = 35736;
const MYTHOLOGY_SIZE: usize = 224;
const MYTHOLOGY_SIZE_POSEIDON: usize = 300;
const PROPONENT_GODS: usize = 48;
const SANCTUARY_ALLOWED: usize = 208;
const MAX_SANCTUARIES: usize = 220;
const PROXIMATE_PROPONENTS: usize = 6;

/// Ce qui suit un épisode de la cité mère. Valeurs écrites par l'éditeur du jeu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Next {
    /// Un autre épisode dans la cité mère (2).
    Parent,
    /// Le prochain épisode de colonie (1), après quoi l'on revient à la cité mère.
    Colony,
    /// La fin de l'aventure (3).
    End,
}

/// Un épisode, dans la cité mère ou dans une colonie.
#[derive(Clone, Debug)]
pub struct Episode {
    /// Numéros, dans `zeus_mm.eng`, des récits d'ouverture et de victoire ; négatifs
    /// pour une aventure qui apporte ses textes dans un fichier à part.
    pub intro_message: i32,
    pub victory_message: i32,
    pub next: Next,
    /// Ressources que la cité peut produire : numéros du groupe de textes 23
    /// (2 poisson, 5 carottes, 7 blé, 13 olives…). Vide quand l'épisode n'en déclare
    /// aucune.
    pub resources: Vec<u8>,
    pub goals: Vec<Goal>,
    /// Dieux (numéros du jeu) à qui l'on peut bâtir un sanctuaire, et combien de
    /// sanctuaires la cité peut avoir à la fois.
    pub sanctuaries: Vec<u8>,
    pub max_sanctuaries: usize,
}

/// Épisode à jouer, désigné par son rang dans la cité mère ou parmi les colonies.
/// L'épisode de colonie k se joue sur la k-ième carte de colonie.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Parent(usize),
    Colony(usize),
}

/// Réglages d'une aventure, lus au début d'un `.pak` ou d'un `.set` (même en-tête).
#[derive(Clone, Debug)]
pub struct Settings {
    /// Année de départ ; négative avant notre ère.
    pub start_year: i32,
    /// Trésor de départ, en drachmes.
    pub funds: i32,
    pub parent: Vec<Episode>,
    pub colonies: Vec<Episode>,
}

impl Settings {
    pub fn parse(file: &[u8]) -> Option<Settings> {
        let int = |offset: usize| file.get(offset..offset + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let version = int(VERSION_OFFSET)?;
        // Zeus.exe renumérote au chargement les ressources des anciennes aventures.
        let resource = |r: u8| if version < MODERN_RESOURCES_VERSION { modern_resource(r) } else { r };
        let goals = Goals::parse(file, version).unwrap_or_default();
        let table_size = if version < MODERN_RESOURCES_VERSION { MYTHOLOGY_SIZE } else { MYTHOLOGY_SIZE_POSEIDON };
        let episodes = |count: i32, headers: usize, records: usize, goals: &[Vec<Goal>], first_table: usize| -> Option<Vec<Episode>> {
            (0..count.clamp(0, 10) as usize)
                .map(|i| {
                    let (header, record) = (headers + i * HEADER_SIZE, records + i * RECORD_SIZE);
                    let resources = file.get(record + RESOURCES..record + RESOURCES + MAX_RESOURCES)?;
                    let table = MYTHOLOGY + (first_table + i) * table_size;
                    let sanctuaries = (0..PROXIMATE_PROPONENTS)
                        .filter(|&slot| file.get(table + SANCTUARY_ALLOWED + slot).is_some_and(|&allowed| allowed != 0))
                        .filter_map(|slot| u8::try_from(int(table + PROPONENT_GODS + 4 * slot)?).ok())
                        .collect();
                    Some(Episode {
                        intro_message: int(header + INTRO_MESSAGE)?,
                        victory_message: int(header + VICTORY_MESSAGE)?,
                        next: match int(header + NEXT)? {
                            1 => Next::Colony,
                            3 => Next::End,
                            _ => Next::Parent,
                        },
                        resources: resources.iter().copied().take_while(|&r| r != 0).map(resource).collect(),
                        goals: goals.get(i).cloned().unwrap_or_default(),
                        sanctuaries,
                        max_sanctuaries: int(table + MAX_SANCTUARIES).map_or(0, |n| n.clamp(0, 4) as usize),
                    })
                })
                .collect()
        };
        let year = file.get(PARENT_RECORDS + START_YEAR..PARENT_RECORDS + START_YEAR + 2)?;
        Some(Settings {
            start_year: i16::from_le_bytes([year[0], year[1]]) as i32,
            funds: int(PARENT_RECORDS + FUNDS)?,
            parent: episodes(int(PARENT_EPISODES_OFFSET)?, PARENT_HEADERS, PARENT_RECORDS, &goals.parent, 0)?,
            // Quatre fiches complètes de colonie seulement.
            colonies: episodes(int(COLONY_EPISODES_OFFSET)?.min(4), COLONY_HEADERS, COLONY_RECORDS, &goals.colonies, 10)?,
        })
    }

    /// Ordre de jeu des épisodes : ceux de la cité mère, chacun suivi au besoin du
    /// prochain épisode de colonie, jusqu'au dernier.
    pub fn order(&self) -> Vec<Step> {
        let mut colonies = 0..self.colonies.len();
        let mut steps = Vec::new();
        for (i, episode) in self.parent.iter().enumerate() {
            steps.push(Step::Parent(i));
            match episode.next {
                Next::End => break,
                Next::Colony => steps.extend(colonies.next().map(Step::Colony)),
                Next::Parent => {}
            }
        }
        steps
    }

    pub fn episode(&self, step: Step) -> Option<&Episode> {
        match step {
            Step::Parent(i) => self.parent.get(i),
            Step::Colony(i) => self.colonies.get(i),
        }
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

    /// Cartes des colonies, dans l'ordre de leurs épisodes.
    pub fn colonies(&self) -> &[Map] {
        self.maps.get(2..).unwrap_or(&[])
    }
}
