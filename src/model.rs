//! Règles chiffrées du jeu (`model/zeus_model_*.txt`) : un tableau par bâtiment.

/// Paramètres d'un type de bâtiment, dans l'ordre du fichier.
#[derive(Clone, Debug, Default)]
pub struct BuildingModel {
    /// Nom interne, par exemple `BUILD_FOUNTAIN`.
    pub name: String,
    pub cost: i32,
    pub desirability: i32,
    pub desirability_step: i32,
    pub desirability_step_size: i32,
    pub desirability_range: i32,
    pub employees: i32,
    pub fire_risk: i32,
    pub damage_risk: i32,
}

/// Paramètres d'un niveau de logement.
#[derive(Clone, Debug, Default)]
pub struct HouseModel {
    pub name: String,
    /// Attrait en dessous duquel la maison régresse, et à partir duquel elle évolue.
    pub devolve_desirability: i32,
    pub evolve_desirability: i32,
    pub culture: i32,
    pub water: i32,
    pub food_types: i32,
    pub fleece: i32,
    pub olive_oil: i32,
    pub wine: i32,
    /// Exigences des logements luxueux qui ne sont pas simulées : stade, chevaux, armes.
    pub stadium: i32,
    pub horses: i32,
    pub arms: i32,
    pub capacity: i32,
    pub tax_multiplier: i32,
}

pub struct Model {
    /// Indexé par numéro de type de bâtiment.
    pub buildings: Vec<BuildingModel>,
    /// Niveaux de logement modeste (hutte → maison de ville), puis logements luxueux.
    pub houses: Vec<HouseModel>,
}

impl Model {
    pub fn parse(file: &[u8]) -> Result<Model, String> {
        let text: String = file.iter().map(|&b| b as char).collect();
        let mut buildings = Vec::new();
        let mut houses = Vec::new();
        for line in text.lines() {
            // Logements : « 3: Hovel,{,-2,10,15,1,0,20,0,0,1,0,0,0,0,0,10,0,15,24,1,0,15,,… »
            if let (Some(colon), Some(open)) = (line.find(':'), line.find(",{")) {
                let is_house = colon < open && (line[..colon].trim().parse::<u32>().is_ok() || line.starts_with("Elite"));
                if is_house {
                    let values: Vec<i32> = line[open + 2..].split(',').skip(1).map_while(|v| v.trim().parse().ok()).collect();
                    if values.len() >= 19 {
                        houses.push(HouseModel {
                            name: line[colon + 1..open].trim().to_string(),
                            devolve_desirability: values[0],
                            evolve_desirability: values[1],
                            culture: values[2],
                            water: values[3],
                            food_types: values[8],
                            fleece: values[9],
                            olive_oil: values[10],
                            wine: values[11],
                            stadium: values[4],
                            horses: values[7],
                            arms: values[12],
                            capacity: values[17],
                            tax_multiplier: values[18],
                        });
                    }
                    continue;
                }
            }
            // Forme : « 74,BUILD_FOUNTAIN,{,35,4,2,-2,4,4,0,8,0,0,}, »
            let (Some(open), Some(close)) = (line.find('{'), line.find('}')) else { continue };
            let mut head = line[..open].split(',');
            let (Some(number), Some(name)) = (head.next(), head.next()) else { continue };
            // Quelques noms sont entre guillemets, avec une virgule dedans.
            let name = name.trim_start_matches('"');
            let Ok(number) = number.trim().parse::<usize>() else { continue };
            if !(name.starts_with("BUILD_") || name.starts_with("UNUSED")) {
                continue;
            }
            let values: Vec<i32> = line[open + 1..close].split(',').filter_map(|v| v.trim().parse().ok()).collect();
            if values.len() < 8 {
                continue;
            }
            // Les règles sont rangées par numéro de type ; les lignes sautées (noms
            // illisibles) laissent un trou. Un numéro en arrière est une erreur du
            // fichier (la fonderie, numérotée 10 au rang 50) : on garde son rang.
            while buildings.len() < number {
                buildings.push(BuildingModel::default());
            }
            buildings.push(BuildingModel {
                name: name.trim().to_string(),
                cost: values[0],
                desirability: values[1],
                desirability_step: values[2],
                desirability_step_size: values[3],
                desirability_range: values[4],
                employees: values[5],
                fire_risk: values[6],
                damage_risk: values[7],
            });
        }
        if buildings.len() < 100 {
            return Err(format!("fichier de règles incomplet ({} bâtiments)", buildings.len()));
        }
        if houses.len() < 7 {
            return Err(format!("fichier de règles incomplet ({} logements)", houses.len()));
        }
        Ok(Model { buildings, houses })
    }

    pub fn building(&self, name: &str) -> Option<&BuildingModel> {
        self.buildings.iter().find(|b| b.name == name)
    }
}

/// Caractéristiques de combat d'un personnage (`model/figure_model_*.txt`) : points
/// de vie, attaque et armure au corps à corps, armure contre les projectiles, attaque,
/// portée et cadence de tir, vitesse, et fréquence (part de l'unité dans son armée,
/// en pour cent).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fighter {
    pub health: i32,
    pub attack: i32,
    pub armor: i32,
    pub missile_armor: i32,
    pub missile_attack: i32,
    pub missile_range: i32,
    pub rate: i32,
    pub speed: i32,
    pub frequency: i32,
}

/// Combattants du jeu : les soldats de la cité (piétaille et archers, hoplites,
/// cavaliers ; personnages 11 à 13) et les unités ennemies, six par nation
/// (infanterie, tireurs, cavalerie, navire de guerre, transport, réserve).
#[derive(Clone, Debug, Default)]
pub struct Fighters {
    pub citizens: [Fighter; 3],
    pub enemies: Vec<Fighter>,
}

/// Rangs des soldats de la cité parmi les personnages.
const CITIZEN_FIGHTERS: [usize; 3] = [11, 12, 13];

impl Fighters {
    pub fn parse(file: &[u8]) -> Result<Fighters, String> {
        let text: String = file.iter().map(|&b| b as char).collect();
        let mut fighters = Fighters::default();
        let mut enemies = false;
        for line in text.lines() {
            if line.starts_with("ALL ENEMIES") {
                enemies = true;
                continue;
            }
            let (Some(open), Some(close)) = (line.find('{'), line.find('}')) else { continue };
            let Ok(number) = line.split(',').next().unwrap_or("").trim().parse::<usize>() else { continue };
            let v: Vec<i32> = line[open + 1..close].split(',').filter_map(|v| v.trim().parse().ok()).collect();
            if v.len() < 10 {
                continue;
            }
            let fighter = Fighter {
                health: v[1],
                attack: v[2],
                armor: v[3],
                missile_armor: v[4],
                missile_attack: v[5],
                missile_range: v[6],
                rate: v[7],
                speed: v[8],
                frequency: v[9],
            };
            if enemies {
                while fighters.enemies.len() < number {
                    fighters.enemies.push(Fighter::default());
                }
                fighters.enemies.push(fighter);
            } else if let Some(rank) = CITIZEN_FIGHTERS.iter().position(|&n| n == number) {
                fighters.citizens[rank] = fighter;
            }
        }
        if fighters.enemies.is_empty() {
            return Err("fichier des personnages sans unités ennemies".into());
        }
        Ok(fighters)
    }
}
