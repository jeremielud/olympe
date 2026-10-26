//! Héros : ce qu'ils exigent avant de venir dans la cité, et les monstres qu'ils
//! savent combattre. Les exigences sont la table de `Zeus.exe` (0x63b3d0, lue par
//! 0x4faa40 et affichée par 0x4faa70) : cinq par héros, chacune un type et cinq
//! paramètres.

use crate::world::{ELITE_FIRST_LEVEL, Function, Good, Service, World};

/// Achille, Hercule, Jason, Ulysse, Persée, Thésée, puis Atalante et Bellérophon
/// (Poséidon) ; noms dans le groupe de textes 163.
pub const HEROES: usize = 8;
pub const TEXT_HEROES: usize = 163;
/// Monstres avec article (« l'Hydre »), et quêtes (le dieu `g` en a deux : `g` et
/// `g + 14`).
pub const TEXT_MONSTERS: usize = 87;
pub const TEXT_QUESTS: usize = 52;
/// Nom du temple de chaque héros (« Temple des héros pour Achille »…), à partir du 8.
pub const TEXT_HALLS: usize = 185;
pub const FIRST_HALL_TEXT: usize = 8;

/// Héros qu'il faut contre chaque monstre (groupe 87 : l'Hydre, le Kraken, Cerbère,
/// le Cyclope, le Minotaure, Méduse, le Sanglier, le Dragon, Talos, Hector, Scylla,
/// la Ménade, le Sphinx, la Chimère, la Harpie, Echidna).
const MONSTER_HERO: [u8; 16] = [1, 4, 1, 3, 5, 4, 5, 2, 2, 0, 3, 0, 6, 7, 6, 7];

pub fn hero_against(monster: u8) -> Option<u8> {
    MONSTER_HERO.get(monster as usize).copied()
}

/// Exigences de chaque héros, telles que le jeu les range.
const REQUIREMENTS: [[[i32; 6]; 5]; HEROES] = [
    [[0, 3200, 18, 0, 0, 0], [2, 3, 1, 12, 2, 13], [3, 1, 9, 8, -1, -1], [4, 70, 0, 0, 0, 0], [0, 1600, 21, 0, 0, 0]],
    [[5, 89, -1, 0, 0, 0], [6, 0, 0, 0, 0, 0], [5, 89, 0, 0, 0, 0], [7, 1500, 0, 0, 0, 0], [0, 3200, 21, 0, 0, 0]],
    [[1, 3, 57, 0, 0, 0], [2, 2, 2, 13, 0, 0], [0, 6400, 25, 0, 0, 0], [1, 8, 43, 1, 0, 0], [0, 1600, 21, 0, 0, 0]],
    [[8, 89, 0, 0, 0, 0], [9, 80, 0, 0, 0, 0], [10, 8, 16, 0, 0, 0], [0, 3200, 20, 0, 0, 0], [0, 1600, 21, 0, 0, 0]],
    [[3, 1, 8, 7, 11, 0], [3, 2, 8, 7, 11, 0], [11, 3000, 1, 0, 0, 0], [0, 1600, 14, 0, 0, 0], [0, 2400, 19, 0, 0, 0]],
    [[12, 4, 0, 0, 0, 0], [13, 60, 0, 0, 0, 0], [14, 0, 0, 0, 0, 0], [0, 3200, 11, 0, 0, 0], [0, 1600, 21, 0, 0, 0]],
    [[3, 1, 4, -1, -1, -1], [18, 1, 207, 80, 0, 0], [0, 3200, 3, 0, 0, 0], [0, 3200, 9, 0, 0, 0], [2, 8, 1, 12, 2, 13]],
    [[1, 15, 43, 0, 0, 0], [17, 89, 0, 0, 0, 0], [11, 10000, 0, 0, 0, 0], [0, 2400, 17, 10, 0, 0], [0, 2400, 21, 0, 0, 0]],
];

/// Ressource « vivres » des exigences : n'importe lesquels.
const ANY_FOOD: i32 = 25;
/// Les sculptures se comptent par quatre unités de stockage, le reste par cent.
const SCULPTURE: i32 = 19;
/// Paramètre des exigences de comptage : chevaux (43) ou trières (57).
const HORSES: i32 = 43;
const TRIREMES: i32 = 57;
/// Niveaux luxueux : résidence, puis hôtel particulier.
const RESIDENCE: usize = ELITE_FIRST_LEVEL + 2;
const MANSION: usize = ELITE_FIRST_LEVEL + 3;

/// Où en est une exigence.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Check {
    /// Remplie ; ou pas encore, avec la valeur actuelle et la valeur demandée.
    Met,
    Missing { current: i32, wanted: i32 },
    /// Olympe ne simule pas ce qu'elle demande : on la tient pour remplie.
    Unsimulated,
}

impl Check {
    pub fn passes(self) -> bool {
        !matches!(self, Check::Missing { .. })
    }
}

/// Une exigence de héros, et de quoi l'afficher.
#[derive(Clone, Copy, Debug)]
pub struct Requirement {
    pub kind: i32,
    pub values: [i32; 5],
}

pub fn requirements(hero: u8) -> impl Iterator<Item = Requirement> {
    REQUIREMENTS.get(hero as usize).into_iter().flatten().map(|r| Requirement { kind: r[0], values: [r[1], r[2], r[3], r[4], r[5]] })
}

impl Requirement {
    /// Évalue l'exigence pour le temple du héros posé en `hall` (coin et côté).
    pub fn check(&self, world: &World, hall: (usize, usize, usize)) -> Check {
        let [a, b, c, d, e] = self.values;
        let against = |current: i32, wanted: i32| if current >= wanted { Check::Met } else { Check::Missing { current, wanted } };
        let houses = || world.buildings.iter().flatten().filter_map(|b| b.house.as_ref()).filter(|h| h.population > 0);
        let coverage = |service: Service| {
            let (all, served) = houses().fold((0, 0), |(all, served), h| (all + 1, served + h.has(service, world.ticks) as i32));
            if all == 0 { 0 } else { served * 100 / all }
        };
        match self.kind {
            // Marchandises en réserve : `a` centièmes d'unité de la ressource `b`, ou
            // à défaut de la ressource `c` (Bellérophon : orichalque ou bronze).
            0 => {
                let unit = if b == SCULPTURE { 400 } else { 100 };
                let stored = |resource: i32| match resource {
                    ANY_FOOD => (1..8).map(|r| world.stored(r)).sum(),
                    r if r > 0 => world.stored(r as usize),
                    _ => 0,
                };
                against(stored(b).max(stored(c)), a / unit)
            }
            // Chevaux dans les écuries, ou trières à quai.
            1 if b == HORSES => {
                let horses = world.buildings.iter().flatten().filter(|x| world.kinds[x.kind].function == Function::Producer(Good::Horses)).map(|x| x.stock).sum();
                against(horses, a)
            }
            1 if b == TRIREMES => against(world.triremes(), a),
            // Hôtels particuliers ou mieux (types 12 et 13 des règles), et logements
            // luxueux habités (résidences ou mieux).
            2 => against(houses().filter(|h| h.level >= MANSION).count() as i32, a),
            10 => against(houses().filter(|h| h.level >= RESIDENCE).count() as i32, a),
            // Sanctuaires dédiés à `a` des dieux nommés ensuite.
            3 => {
                let built = world.sanctuaries();
                against([b, c, d, e].iter().filter(|&&god| god >= 0 && built.contains(&(god as u8))).count() as i32, a)
            }
            // Culture au temple (b = -1) ou gymnase pour tous : part des maisons desservies.
            5 if b == -1 => {
                let culture = [Service::Philosophy, Service::Athletics, Service::Drama].map(coverage);
                against(culture.iter().sum::<i32>() / 3, a)
            }
            5 => against(coverage(Service::Athletics), a),
            7 => against(world.population(), a),
            // Aucun trouble : le jeu appelle « trouble » la criminalité des maisons (textes
            // 65:61 à 66). Lecture d'Olympe : aucune maison n'atteint le seuil `a`.
            4 => {
                let worst = houses().map(|h| h.crime).max().unwrap_or(0);
                if worst < a { Check::Met } else { Check::Missing { current: worst, wanted: a } }
            }
            // Popularité et santé générale : humeur et santé de la cité, sur 100.
            8 => against(world.sentiment, a),
            9 => against(world.health, a),
            11 => against(world.money, a),
            17 => against(coverage(Service::Tax), a),
            // Temple à moins de `a` cases du palais.
            12 => {
                let (x, y, size) = hall;
                let near = world.buildings.iter().flatten().filter(|p| world.kinds[p.kind].function == Function::Palace).any(|p| {
                    let k = &world.kinds[p.kind];
                    let gap = |lo: usize, len: usize, other: usize, other_len: usize| (lo as i32 - (other + other_len) as i32).max(other as i32 - (lo + len) as i32).max(0);
                    gap(x, size, p.x, k.size).max(gap(y, size, p.y, k.depth)) <= a
                });
                if near { Check::Met } else { Check::Missing { current: 0, wanted: 1 } }
            }
            // Temple protégé par des murs : lecture d'Olympe, il est dans une enceinte
            // qu'aucun envahisseur ne peut franchir sans brèche.
            14 => if world.walled_in(hall.0, hall.1) { Check::Met } else { Check::Missing { current: 0, wanted: 1 } },
            // Attrait autour du temple.
            13 => against(world.appeal(hall.0 as i32, hall.1 as i32), a),
            // Exigence 6 : dans une cité atlante, un hippodrome en activité (textes
            // 117 à 119 du groupe 185) ; dans une cité grecque, une victoire aux jeux
            // panhelléniques (texte 50), qu'Olympe ne compte pas encore.
            6 if world.atlantean => {
                let (sections, closed) = world.hippodrome();
                if sections > 0 && closed { Check::Met } else { Check::Missing { current: sections.min(1), wanted: 1 } }
            }
            6 => Check::Unsimulated,
            // Stade actif (textes 127 à 130) : un stade, ou un musée dans une cité
            // atlante, avec son personnel. Le jeu ne demande qu'un bâtiment actif ;
            // le paramètre `a` n'est pas lu.
            18 => if world.stadium_open() { Check::Met } else { Check::Missing { current: 0, wanted: 1 } },
            // Jeux panhelléniques…
            _ => Check::Unsimulated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_monster_has_its_hero() {
        // Le Cyclope et Scylla pour Ulysse, l'Hydre et Cerbère pour Hercule.
        assert_eq!([3, 10].map(hero_against), [Some(3), Some(3)]);
        assert_eq!([0, 2].map(hero_against), [Some(1), Some(1)]);
        assert_eq!(hero_against(16), None);
    }
}
