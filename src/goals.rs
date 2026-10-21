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
use crate::world::World;

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
    /// (dieu ou -1, nombre, « autres » si non nul)…
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
            Kind::Housing => against(world.people_housed_from(first.max(0) as usize), second),
            // Une fois atteint, l'objectif le reste : on compare la meilleure année.
            Kind::Profit => match world.best_year_profit {
                Some(best) => against(best, first),
                None => Status::Pending(None),
            },
            // Meilleure année, l'année en cours comprise.
            Kind::Production if world.produces(first) => against(world.best_production(first as usize), second),
            // Le jeu tient pour atteint un objectif sans marchandise.
            Kind::SetAside if first == 0 => Status::Met,
            Kind::SetAside if world.can_store(first) => against(world.set_aside[first as usize], second),
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
                let level = usize::try_from(first).ok().and_then(|l| levels.get(l)).cloned().unwrap_or_default();
                amount(line(33), second).replace("[house_level]", &level.to_lowercase())
            }
            Kind::Partners => amount(line(34), first),
            Kind::SetAside => amount(line(if in_colony { 35 } else { 36 }), second).replace("[item]", &resource(first)),
            Kind::Sanctuary if first == -1 => match (second >= 2, third != 0) {
                (false, false) => line(20),
                (false, true) => line(21),
                (true, false) => amount(line(22), second),
                (true, true) => amount(line(23), second),
            },
            Kind::Quest => line(26),
            Kind::Slay => line(27),
            Kind::Pyramid if first == -1 => line(39),
            Kind::Hippodrome => amount(line(if first == 1 { 42 } else { 41 }), first),
            // Le reste nomme un dieu, une unité ou une cité : on s'en tient au type.
            kind => line(kind as usize),
        }
    }
}
