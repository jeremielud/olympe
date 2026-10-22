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
/// Événements : 150 de 124 octets par épisode (dix de la cité mère, quatre de
/// colonie), juste après les tables de mythologie ; leur nombre réel par épisode est
/// rangé après la copie de travail de la carte (lecture de 0x425d00).
const EVENT_SIZE: usize = 124;
const EVENTS_PER_EPISODE: usize = 150;
const EVENT_COUNTS_GAP: usize = 1 + 4 + 20 + 500_000;
/// Dans un événement : son type (octet 2), son mois (octet 3) ; pour une quête, le
/// héros (octet 54 : celui du premier monstre, du deuxième ou du monstre
/// indépendant), le dieu (octet 96) et le rang de la quête parmi les deux du dieu
/// (octet 122) ; pour une invasion de monstre, le monstre (octet 6 : 0 et 1 ceux des
/// deux premiers dieux hostiles proches, 2 l'indépendant, 3 à 8 ceux des dieux
/// favorables proches, comme 0x586b40). Rangs établis sur les récits des épisodes.
const EVENT_TYPE: usize = 2;
const EVENT_HERO: usize = 54;
const EVENT_MONSTER: usize = 6;
const EVENT_GOD: usize = 96;
const EVENT_QUEST: usize = 122;
/// Dans la table de mythologie : dieux hostiles proches (entiers 0 à 3) et monstre
/// indépendant (entier 24).
const OPPONENT_GODS: usize = 0;
const INDEPENDENT_MONSTER: usize = 96;
/// Monstre de chaque dieu (0x586c90), et de ceux des dieux favorables qui en prêtent
/// un (0x586d50).
const GOD_MONSTER: [i32; 14] = [3, 1, 5, 10, 6, 7, 9, 4, 0, 8, 11, 2, 12, 13];
const ALLY_MONSTER: [(i32, i32); 4] = [(1, 1), (5, 7), (9, 8), (11, 2)];
const EVENT_QUEST_TYPE: u8 = 4;
const EVENT_MONSTER_TYPE: u8 = 26;
/// Changement dans le monde (type 19) : sa nature (octet 96), la cité (entier court
/// en 30, ou à défaut un intervalle en 32 et 34), une quantité tirée entre les
/// entiers courts en 16 et 18 ; quand : le mois (octet 3) et l'année depuis le début
/// de l'épisode (entier court en 22, ou à défaut un intervalle en 24 et 26).
const EVENT_WORLD_TYPE: u8 = 19;
/// Demande d'une cité (type 1) : la marchandise (octet 6, numérotée comme les
/// ressources, avec 23 drachmes, 24 troupes et 25 vivres), la quantité, la cité et
/// la date comme pour un changement du monde.
const EVENT_REQUEST_TYPE: u8 = 1;
/// Invasion d'un dieu hostile (type 27) : le dieu (octet 6), la date comme pour un
/// changement du monde.
const EVENT_GOD_TYPE: u8 = 27;
/// Invasion d'une armée rivale (type 2) : nombre de soldats (entre les entiers courts
/// en 16 et 18), la cité qui attaque (intervalle en 32 et 34), la date comme pour
/// un changement du monde.
const EVENT_INVASION_TYPE: u8 = 2;
/// Point d'invasion d'une invasion : seul en 90, intervalle en 92 et 94.
const EVENT_POINT: usize = 90;
const EVENT_POINTS: usize = 92;
/// Cadeau d'une cité (type 23) : la marchandise (octet 6), la quantité (−1 : ce qui
/// a été mis de côté dans l'épisode précédent), la cité (0 : la cité mère), la date.
/// Établi sur Troie, où la colonie reçoit ainsi les marchandises réservées.
const EVENT_GIFT_TYPE: u8 = 23;
/// Changements du commerce : demande d'une cité en hausse ou en baisse (13, 14), offre
/// (21, 22), prix d'une marchandise pour tous (15, 16). Les noms des types sont dans
/// `zeus_editor_text.eng`, rangés par numéro (« Free Event », « Request »,
/// « Invasion », « Earthquake »…), et le jeu les regroupe en 0x468250.
const EVENT_TRADE_TYPES: [u8; 6] = [13, 14, 21, 22, 15, 16];
/// Hausse et baisse des salaires (8, 9) : de tant de drachmes par an et par dix
/// ouvriers (quantité en 16 et 18), à la date dite.
const EVENT_WAGE_TYPES: [u8; 2] = [8, 9];
/// Armée rivale partie en campagne (20) : la cité est affaiblie pendant tant de mois
/// (quantité ; Olympe y lit une durée).
const EVENT_ARMY_AWAY_TYPE: u8 = 20;
/// Catastrophes : tremblement de terre (3), glissement de terrain (5), coulée de lave
/// (24), raz-de-marée (25), terres englouties (28). Le point de départ est un point de
/// catastrophe de la carte (comme une cité : seul en 30, intervalle en 32 et 34) ; un
/// raz-de-marée permanent (octet 121) est une inondation.
const EVENT_DISASTER_TYPES: [u8; 5] = [3, 5, 24, 25, 28];
const EVENT_PERMANENT: usize = 121;
const EVENT_GOOD: usize = 6;
const EVENT_MONTH: usize = 3;
const EVENT_AMOUNT: usize = 16;
const EVENT_YEAR: usize = 22;
const EVENT_YEARS: usize = 24;
const EVENT_CITY: usize = 30;
const EVENT_CITIES: usize = 32;

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
    /// Événements, par rang (les objectifs de quête et de monstre y renvoient).
    pub events: Vec<Event>,
}

/// Événement d'un épisode, quand Olympe sait le jouer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    /// Quête qu'un dieu confie à un héros ; `quest` vaut 0 ou 1 (chaque dieu en a deux).
    Quest { god: u8, quest: u8, hero: u8 },
    /// Monstre qui s'en prend à la cité (numéro du groupe de textes 87) : présent dès
    /// le début de l'épisode, ou arrivant le mois et entre les années donnés (monstre
    /// lâché par un dieu, ou envahisseur : sous-type 1 ou 2, octet 96).
    Monster { monster: u8, arrives: Option<(u8, (i16, i16))> },
    /// Changement dans le monde : sa nature (sous-type du jeu), les cités entre
    /// lesquelles il tombe, la quantité, le mois et les années (depuis le début de
    /// l'épisode) entre lesquelles il a lieu.
    World { change: u8, cities: (i16, i16), amount: (i16, i16), month: u8, years: (i16, i16) },
    /// Demande d'une cité : la ressource voulue et sa quantité.
    Request { resource: u8, cities: (i16, i16), amount: (i16, i16), month: u8, years: (i16, i16) },
    /// Un dieu hostile vient en personne dans la cité.
    GodInvasion { god: u8, month: u8, years: (i16, i16) },
    /// Une cité offre une marchandise ; une quantité négative désigne ce qui a été
    /// mis de côté pour la cité.
    Gift { resource: u8, cities: (i16, i16), amount: (i16, i16), month: u8, years: (i16, i16) },
    /// Changement du commerce : 0 et 1 la demande d'une cité monte ou baisse, 2 et 3
    /// son offre, 4 et 5 le prix d'une marchandise.
    TradeChange { change: u8, resource: u8, cities: (i16, i16), amount: (i16, i16), month: u8, years: (i16, i16) },
    /// Une cité rivale envoie une armée de tant de soldats.
    /// `points` : points d'invasion de la carte (1 à 8 par terre, 9 à 16 par mer).
    Invasion { cities: (i16, i16), soldiers: (i16, i16), points: (i16, i16), month: u8, years: (i16, i16) },
    /// Les salaires montent ou baissent de tant de drachmes par an et par dix ouvriers.
    Wages { rise: bool, amount: (i16, i16), month: u8, years: (i16, i16) },
    /// Catastrophe naturelle du type de l'éditeur (3, 5, 24, 25, 28), partant d'un
    /// point de catastrophe de la carte.
    Disaster { kind: u8, points: (i16, i16), permanent: bool, month: u8, years: (i16, i16) },
    /// L'armée d'une cité rivale part pour tant de mois.
    ArmyAway { cities: (i16, i16), months: (i16, i16), month: u8, years: (i16, i16) },
    Other,
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
        let events_start = MYTHOLOGY + 14 * table_size;
        let event_counts = events_start + 14 * EVENTS_PER_EPISODE * EVENT_SIZE + EVENT_COUNTS_GAP;
        let events_of = |table: usize| -> Vec<Event> {
            let count = int(event_counts + 4 * table).unwrap_or(0).clamp(0, EVENTS_PER_EPISODE as i32) as usize;
            // Monstre désigné par un choix de l'éditeur : 0 et 1 les monstres des dieux
            // hostiles, 2 l'indépendant, 3 à 8 ceux des dieux favorables.
            let mythology = MYTHOLOGY + table * table_size;
            let monster_of = |choice: u8| -> Option<u8> {
                let god = |at: usize| int(mythology + at);
                let monster = match choice {
                    0 | 1 => GOD_MONSTER.get(usize::try_from(god(OPPONENT_GODS + 4 * choice as usize)?).ok()?).copied()?,
                    2 => god(INDEPENDENT_MONSTER)?,
                    3..=8 => {
                        let ally = god(PROPONENT_GODS + 4 * (choice as usize - 3))?;
                        ALLY_MONSTER.iter().find(|&&(g, _)| g == ally)?.1
                    }
                    _ => return None,
                };
                u8::try_from(monster).ok()
            };
            (0..count)
                .map(|i| {
                    let at = events_start + (table * EVENTS_PER_EPISODE + i) * EVENT_SIZE;
                    let Some(event) = file.get(at..at + EVENT_SIZE) else { return Event::Other };
                    match event[EVENT_TYPE] {
                        EVENT_QUEST_TYPE => match monster_of(event[EVENT_HERO]).and_then(crate::heroes::hero_against) {
                            Some(hero) => Event::Quest { god: event[EVENT_GOD], quest: event[EVENT_QUEST], hero },
                            None => Event::Other,
                        },
                        EVENT_MONSTER_TYPE => match monster_of(event[EVENT_MONSTER]) {
                            Some(monster) => {
                                let short = |at: usize| i16::from_le_bytes([event[at], event[at + 1]]);
                                let years = match short(EVENT_YEAR) {
                                    n if n >= 0 => (n, n),
                                    _ => (short(EVENT_YEARS), short(EVENT_YEARS + 2)),
                                };
                                let arrives = (event[EVENT_GOD] != 0).then_some((event[EVENT_MONTH] % 12, years));
                                Event::Monster { monster, arrives }
                            }
                            None => Event::Other,
                        },
                        EVENT_GOD_TYPE => {
                            let short = |at: usize| i16::from_le_bytes([event[at], event[at + 1]]);
                            let years = match short(EVENT_YEAR) {
                                n if n >= 0 => (n, n),
                                _ => (short(EVENT_YEARS), short(EVENT_YEARS + 2)),
                            };
                            Event::GodInvasion { god: event[EVENT_GOOD], month: event[EVENT_MONTH] % 12, years }
                        }
                        EVENT_INVASION_TYPE => {
                            let short = |at: usize| i16::from_le_bytes([event[at], event[at + 1]]);
                            let either = |one: usize, range: usize| match short(one) {
                                n if n >= 0 => (n, n),
                                _ => (short(range), short(range + 2)),
                            };
                            Event::Invasion {
                                cities: either(EVENT_CITY, EVENT_CITIES),
                                soldiers: (short(EVENT_AMOUNT), short(EVENT_AMOUNT + 2)),
                                points: either(EVENT_POINT, EVENT_POINTS),
                                month: event[EVENT_MONTH] % 12,
                                years: either(EVENT_YEAR, EVENT_YEARS),
                            }
                        }
                        kind if EVENT_DISASTER_TYPES.contains(&kind) => {
                            let short = |at: usize| i16::from_le_bytes([event[at], event[at + 1]]);
                            let either = |one: usize, range: usize| match short(one) {
                                n if n >= 0 => (n, n),
                                _ => (short(range), short(range + 2)),
                            };
                            Event::Disaster {
                                kind,
                                points: either(EVENT_CITY, EVENT_CITIES),
                                permanent: event[EVENT_PERMANENT] != 0,
                                month: event[EVENT_MONTH] % 12,
                                years: either(EVENT_YEAR, EVENT_YEARS),
                            }
                        }
                        kind @ (EVENT_ARMY_AWAY_TYPE | 8 | 9) => {
                            let short = |at: usize| i16::from_le_bytes([event[at], event[at + 1]]);
                            let either = |one: usize, range: usize| match short(one) {
                                n if n >= 0 => (n, n),
                                _ => (short(range), short(range + 2)),
                            };
                            let (amount, month, years) = ((short(EVENT_AMOUNT), short(EVENT_AMOUNT + 2)), event[EVENT_MONTH] % 12, either(EVENT_YEAR, EVENT_YEARS));
                            match kind {
                                EVENT_ARMY_AWAY_TYPE => Event::ArmyAway { cities: either(EVENT_CITY, EVENT_CITIES), months: amount, month, years },
                                _ => Event::Wages { rise: kind == EVENT_WAGE_TYPES[0], amount, month, years },
                            }
                        }
                        kind if EVENT_TRADE_TYPES.contains(&kind) => {
                            let short = |at: usize| i16::from_le_bytes([event[at], event[at + 1]]);
                            let either = |one: usize, range: usize| match short(one) {
                                n if n >= 0 => (n, n),
                                _ => (short(range), short(range + 2)),
                            };
                            Event::TradeChange {
                                change: EVENT_TRADE_TYPES.iter().position(|&t| t == kind).unwrap_or(0) as u8,
                                resource: resource(event[EVENT_GOOD]),
                                cities: either(EVENT_CITY, EVENT_CITIES),
                                amount: (short(EVENT_AMOUNT), short(EVENT_AMOUNT + 2)),
                                month: event[EVENT_MONTH] % 12,
                                years: either(EVENT_YEAR, EVENT_YEARS),
                            }
                        }
                        kind @ (EVENT_WORLD_TYPE | EVENT_REQUEST_TYPE | EVENT_GIFT_TYPE) => {
                            let short = |at: usize| i16::from_le_bytes([event[at], event[at + 1]]);
                            let either = |one: usize, range: usize| match short(one) {
                                n if n >= 0 => (n, n),
                                _ => (short(range), short(range + 2)),
                            };
                            if kind == EVENT_GIFT_TYPE {
                                return Event::Gift {
                                    resource: resource(event[EVENT_GOOD]),
                                    cities: either(EVENT_CITY, EVENT_CITIES),
                                    amount: (short(EVENT_AMOUNT), short(EVENT_AMOUNT + 2)),
                                    month: event[EVENT_MONTH] % 12,
                                    years: either(EVENT_YEAR, EVENT_YEARS),
                                };
                            }
                            if kind == EVENT_REQUEST_TYPE {
                                return Event::Request {
                                    resource: resource(event[EVENT_GOOD]),
                                    cities: either(EVENT_CITY, EVENT_CITIES),
                                    amount: (short(EVENT_AMOUNT), short(EVENT_AMOUNT + 2)),
                                    month: event[EVENT_MONTH] % 12,
                                    years: either(EVENT_YEAR, EVENT_YEARS),
                                };
                            }
                            Event::World {
                                change: event[EVENT_GOD],
                                cities: either(EVENT_CITY, EVENT_CITIES),
                                amount: (short(EVENT_AMOUNT), short(EVENT_AMOUNT + 2)),
                                month: event[EVENT_MONTH] % 12,
                                years: either(EVENT_YEAR, EVENT_YEARS),
                            }
                        }
                        _ => Event::Other,
                    }
                })
                .collect()
        };
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
                        events: events_of(first_table + i),
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
