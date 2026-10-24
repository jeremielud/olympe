//! Objectifs d'épisode : jusqu'à six par épisode. Le jeu les range dans l'en-tête des
//! aventures (`.pak`, `.set`), en une table qui précède immédiatement les cartes des
//! cités : nombres d'objectifs puis objectifs des quatre épisodes de colonie, puis de
//! même pour les dix épisodes de la cité mère.
//!
//! Lecture établie sur `Zeus.exe` : la table est lue et écrite d'un bloc (fonctions
//! 0x52df50 et 0x52def0), décrite par le panneau des objectifs (0x5272c0), et les
//! fichiers d'avant la version 22 voient leurs numéros de marchandises renumérotés au
//! chargement (0x526500).

use crate::text::Text;
use crate::world::{Soldier, World};

pub const MAX_GOALS: usize = 6;
pub const PARENT_EPISODES: usize = 10;
pub const COLONY_EPISODES: usize = 4;
/// Un objectif occupe 76 octets : son type, puis dix-huit valeurs dont les trois
/// premières seules servent.
const GOAL_SIZE: usize = 76;
const EPISODE_SIZE: usize = GOAL_SIZE * MAX_GOALS;
/// Taille de la table entière.
pub const TABLE_SIZE: usize = (COLONY_EPISODES + PARENT_EPISODES) * (4 + EPISODE_SIZE);
/// Les cartes d'une aventure commencent par une copie de travail rangée dans une zone
/// de taille fixe ; la table suit cette zone et l'en-tête qui la prolonge.
const WORKING_MAP_AREA: usize = 500_000;

/// Groupe de `zeus_text.eng` qui décrit les objectifs.
const TEXT_GROUP: usize = 194;
/// Noms des types de bâtiments (les monuments de 100 à 114).
const TEXT_BUILDINGS: usize = 28;
/// Nom des marchandises, numérotées comme les ressources des aventures.
const TEXT_RESOURCES: usize = 23;

/// Types d'objectifs, dans l'ordre du jeu (et des noms du groupe de textes 194).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Population,
    Treasury,
    Sanctuary,
    /// Entretenir des soldats ou des navires de guerre.
    Support,
    Quest,
    Slay,
    /// Produire une quantité d'une marchandise en une année civile.
    Production,
    /// Gouverner une cité (vassale ou colonie).
    Rule,
    /// Bénéfice d'une année civile.
    Profit,
    /// Loger un nombre d'habitants à un niveau de logement donné ou mieux.
    Housing,
    Partners,
    Unused,
    Deadline,
    Survival,
    /// Mettre de côté des marchandises pour la cité suivante.
    SetAside,
    Pyramid,
    Hippodrome,
}

const KINDS: [Kind; 17] = [
    Kind::Population,
    Kind::Treasury,
    Kind::Sanctuary,
    Kind::Support,
    Kind::Quest,
    Kind::Slay,
    Kind::Production,
    Kind::Rule,
    Kind::Profit,
    Kind::Housing,
    Kind::Partners,
    Kind::Unused,
    Kind::Deadline,
    Kind::Survival,
    Kind::SetAside,
    Kind::Pyramid,
    Kind::Hippodrome,
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Goal {
    pub kind: Kind,
    /// Sens selon le type : population : (nombre) ; production et mise de côté :
    /// (marchandise, quantité) ; logement : (niveau, habitants) ; sanctuaire :
    /// (dieu ou -1, nombre, « autres » si non nul) — tirés des valeurs 0, 2 et 4 du
    /// jeu, dont la 1 et la 3 sont des compteurs de partie…
    pub values: [i32; 3],
}

/// Objectifs de chaque épisode, cité mère et colonies séparément.
#[derive(Clone, Debug, Default)]
pub struct Goals {
    pub parent: Vec<Vec<Goal>>,
    pub colonies: Vec<Vec<Goal>>,
}

/// Renumérotation des marchandises au passage de Zeus à Poséidon (fichiers d'avant la
/// version 22) : les oranges s'intercalent en 8, d'autres plus loin.
pub fn modern_resource(old: u8) -> u8 {
    match old {
        1..=7 => old,
        8..=14 => old + 1,
        15..=18 => old + 3,
        19..=23 => old + 4,
        _ => 0,
    }
}

/// Un objectif de logement désigne son niveau par le type de bâtiment du jeu (7 pour
/// les appartements, 11 pour les résidences…) : les logements commencent au type 2,
/// la hutte, niveau 0 d'Olympe.
const FIRST_HOUSE_TYPE: i32 = 2;

pub fn housing_level(building_type: i32) -> usize {
    (building_type - FIRST_HOUSE_TYPE).max(0) as usize
}

impl Goals {
    /// Lit la table des objectifs d'un `.pak` ou d'un `.set` ; `version` est celle de
    /// l'en-tête. Rien quand la table n'est pas là où on l'attend.
    pub fn parse(file: &[u8], version: i32) -> Option<Goals> {
        let first_map = find_maps(file, 0)?;
        // Dans un `.pak`, la table s'arrête à la carte de la cité mère ; un `.set`
        // s'arrête avec elle.
        let end = find_maps(file, first_map + WORKING_MAP_AREA).unwrap_or(file.len());
        let table = file.get(end.checked_sub(TABLE_SIZE).filter(|&start| start >= first_map + WORKING_MAP_AREA)?..end)?;
        let int = |offset: usize| i32::from_le_bytes([table[offset], table[offset + 1], table[offset + 2], table[offset + 3]]);
        let modern = version >= 22;
        let block = |start: usize, episodes: usize| -> Option<Vec<Vec<Goal>>> {
            let goals_start = start + 4 * episodes;
            (0..episodes)
                .map(|episode| {
                    let count = usize::try_from(int(start + 4 * episode)).ok().filter(|&n| n <= MAX_GOALS)?;
                    (0..count)
                        .map(|i| {
                            let at = goals_start + episode * EPISODE_SIZE + i * GOAL_SIZE;
                            let kind = *KINDS.get(usize::try_from(int(at)).ok()?)?;
                            let mut values = [int(at + 4), int(at + 8), int(at + 12)];
                            // Sanctuaire : nombre en troisième valeur, « autres » en cinquième ;
                            // un dieu désigné ne demande qu'un sanctuaire (0x52b7d0).
                            if kind == Kind::Sanctuary {
                                values = [values[0], if values[0] == -1 { values[2] } else { 1 }, int(at + 20)];
                            }
                            // Le jeu ne lit que l'octet bas de la marchandise mise de côté.
                            if kind == Kind::SetAside {
                                values[0] &= 0xFF;
                            }
                            if matches!(kind, Kind::SetAside | Kind::Production) && !modern {
                                values[0] = u8::try_from(values[0]).map_or(0, modern_resource) as i32;
                            }
                            Some(Goal { kind, values })
                        })
                        .collect()
                })
                .collect()
        };
        let colonies = block(0, COLONY_EPISODES)?;
        let parent = block(COLONY_EPISODES * (4 + EPISODE_SIZE), PARENT_EPISODES)?;
        Some(Goals { parent, colonies })
    }
}

fn find_maps(file: &[u8], from: usize) -> Option<usize> {
    file.get(from..)?.windows(4).position(|w| w == b"MAPS").map(|at| from + at)
}

/// Où en est un objectif.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Met,
    /// Pas encore atteint ; la valeur actuelle quand elle se mesure.
    Pending(Option<i32>),
    /// Olympe ne simule pas encore ce que l'objectif demande.
    Unsupported,
}

impl Goal {
    pub fn status(&self, world: &World) -> Status {
        let [first, second, _] = self.values;
        let against = |current: i32, target: i32| if current >= target { Status::Met } else { Status::Pending(Some(current)) };
        match self.kind {
            Kind::Population => against(world.population(), first),
            Kind::Treasury => against(world.money, first),
            Kind::Housing => against(world.people_housed_from(housing_level(first)), second),
            // Une fois atteint, l'objectif le reste : on compare la meilleure année.
            Kind::Profit => match world.best_year_profit {
                Some(best) => against(best, first),
                None => Status::Pending(None),
            },
            // Soldats d'un rang ou mieux (11 piétaille, 12 hoplites, 13 cavaliers, comme
            // 0x4de170), ou trières (57).
            Kind::Support => match first {
                11 => against(world.soldiers(Soldier::Rabble), second),
                12 => against(world.soldiers(Soldier::Hoplite), second),
                13 => against(world.soldiers(Soldier::Horseman), second),
                57 => against(world.triremes(), second),
                _ => Status::Unsupported,
            },
            Kind::Partners => against(world.partners(), first),
            // Régner sur une cité : la rendre vassale (0x526ed0).
            Kind::Rule => match world.cities.get(first.max(0) as usize) {
                Some(city) if city.status == crate::map::VASSAL => Status::Met,
                Some(_) => Status::Pending(None),
                None => Status::Unsupported,
            },
            // Objectifs désignés par un événement de l'épisode.
            Kind::Quest => if world.quest_done(first.max(0) as usize) { Status::Met } else { Status::Pending(None) },
            Kind::Slay => if world.monster_slain(first.max(0) as usize) { Status::Met } else { Status::Pending(None) },
            Kind::Sanctuary => {
                let built = world.sanctuaries();
                match first {
                    // « Autres sanctuaires » : Olympe compte aussi ceux qu'un autre
                    // objectif désigne.
                    -1 => against(built.len() as i32, second),
                    god => {
                        if built.iter().any(|&g| g as i32 == god) {
                            Status::Met
                        } else {
                            Status::Pending(None)
                        }
                    }
                }
            }
            // Meilleure année, l'année en cours comprise.
            Kind::Production if world.produces(first) => against(world.best_production(first as usize), second),
            // Le jeu tient pour atteint un objectif sans marchandise.
            Kind::SetAside if first == 0 => Status::Met,
            Kind::SetAside if world.can_store(first) => against(world.set_aside[first as usize], second),
            // Hippodrome d'au moins `first` stades, piste fermée.
            Kind::Hippodrome => match world.hippodrome() {
                (sections, true) => against(sections, first),
                (sections, false) => Status::Pending(Some(sections.min(first - 1))),
            },
            // Monuments achevés : `first` d'entre eux, du type `second` (100 à 114),
            // ou de n'importe quel type (−1).
            Kind::Pyramid => {
                let model = (second >= 0).then(|| (second - crate::monument::FIRST_TYPE as i32) as u8);
                if model.is_some_and(|m| m as usize >= crate::monument::MONUMENTS) {
                    return Status::Unsupported;
                }
                against(world.monuments_built(model), first.max(1))
            }
            _ => Status::Unsupported,
        }
    }

    /// Annonce du jeu quand l'objectif vient d'être atteint.
    pub fn achieved(&self, text: &Text) -> String {
        let index = match self.kind {
            Kind::Unused | Kind::Deadline | Kind::Survival => return String::new(),
            Kind::SetAside => 95,
            Kind::Pyramid => 97,
            Kind::Hippodrome => 99,
            kind => 73 + 2 * kind as usize,
        };
        text.get(TEXT_GROUP, index)
    }

    /// Libellé du jeu, complété. `levels` donne le nom de chaque niveau de logement ;
    /// `in_colony` dit si l'épisode se joue dans une colonie, d'où les marchandises mises
    /// de côté partent pour la cité mère (et inversement).
    pub fn describe(&self, text: &Text, levels: &[String], in_colony: bool) -> String {
        let [first, second, third] = self.values;
        let line = |index: usize| text.get(TEXT_GROUP, index);
        let resource = |id: i32| usize::try_from(id).map_or(String::new(), |id| text.get(TEXT_RESOURCES, id).to_lowercase());
        let amount = |template: String, value: i32| template.replace("[amount]", &value.to_string());
        match self.kind {
            Kind::Population => amount(line(17), first),
            Kind::Treasury => amount(line(18), first),
            Kind::Profit => amount(line(32), first),
            Kind::Production => amount(line(30), second).replace("[item]", &resource(first)),
            Kind::Housing => {
                let level = levels.get(housing_level(first)).cloned().unwrap_or_default();
                amount(line(33), second).replace("[house_level]", &level.to_lowercase())
            }
            Kind::Partners => amount(line(34), first),
            Kind::Support => {
                let unit = match first {
                    11 => "soldats ou mieux",
                    12 => "hoplites ou mieux",
                    13 => "cavaliers",
                    _ => "trières",
                };
                amount(line(24), second).replace("[military_or_better]", unit)
            }
            Kind::SetAside => amount(line(if in_colony { 35 } else { 36 }), second).replace("[item]", &resource(first)),
            Kind::Sanctuary if first == -1 => match (second >= 2, third != 0) {
                (false, false) => line(20),
                (false, true) => line(21),
                (true, false) => amount(line(22), second),
                (true, true) => amount(line(23), second),
            },
            Kind::Sanctuary => line(19).replace("[god]", &text.get(crate::sanctuary::TEXT_GODS, first.max(0) as usize)),
            Kind::Quest => line(26),
            Kind::Slay => line(27),
            Kind::Pyramid if second == -1 && first > 1 => amount(line(40), first),
            Kind::Pyramid if second == -1 => line(39),
            Kind::Pyramid => {
                let name = text.get(TEXT_BUILDINGS, second.max(0) as usize);
                if first > 1 { format!("{first} × {name}") } else { name }
            }
            Kind::Hippodrome => amount(line(if first == 1 { 42 } else { 41 }), first),
            // Le reste nomme un dieu, une unité ou une cité : on s'en tient au type.
            kind => line(kind as usize),
        }
    }
}
