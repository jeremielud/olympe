//! Essais de la simulation sur une vraie carte, quand `donnees/` est présent.

use std::fs;
use std::path::{Path, PathBuf};
use zeus::goals::{Goal, Kind as GoalKind, Status};
use zeus::map::{GRID, Map};
use zeus::model::Model;
use zeus::world::{Function, Good, Kind, Service, TICKS_PER_MONTH, WalkerImages, World};

const HOUSE: usize = 0;
const FOUNTAIN: usize = 1;
const MAINTENANCE: usize = 2;

fn data() -> Option<PathBuf> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("donnees");
    folder.join("zeus_text.eng").exists().then_some(folder)
}

/// Monde vierge sur la carte des essais, avec trois types de bâtiments, et un coin de
/// terrain libre de 12 × 12 cases proche du centre.
fn world(data: &Path) -> (World, i32, i32) {
    let map = Map::parse(&fs::read(data.join("adventures/les plus jeunes jumeaux/les plus jeunes jumeauxp.map")).unwrap()).unwrap();
    let model = Model::parse(&fs::read(data.join("model/zeus_model_normal.txt")).unwrap()).unwrap();
    let mut kinds = vec![
        Kind::simple("maison".into(), 20, 2, 1006, Function::House, 0),
        Kind::simple("fontaine".into(), 35, 2, 1055, Function::Service(Service::Water), 0),
        Kind::simple("maintenance".into(), 20, 2, 1075, Function::Service(Service::Maintenance), 0),
    ];
    kinds[HOUSE].fire_risk = model.buildings[2].fire_risk;
    let world = World::new(map, kinds, model.houses.clone(), WalkerImages::default(), 10_000);
    let centre = GRID as i32 / 2;
    let mut sites: Vec<(i32, i32)> = (0..GRID as i32).flat_map(|y| (0..GRID as i32).map(move |x| (x, y))).collect();
    sites.sort_by_key(|&(x, y)| (x - centre).abs() + (y - centre).abs());
    let (x, y) = sites.into_iter().find(|&(x, y)| world.can_build(x, y, 12)).unwrap();
    (world, x, y)
}

/// Une rue de dix cases bordée de quatre maisons.
fn street(world: &mut World, x: i32, y: i32) {
    for i in 0..10 {
        assert!(world.place_road(x + i, y + 4, 3));
    }
    for i in 0..4 {
        assert!(world.place_building(HOUSE, x + 2 * i, y + 2));
    }
}

#[test]
fn immigrants_fill_the_houses() {
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    street(&mut world, x, y);
    assert_eq!(world.money, 10_000 - 10 * 3 - 4 * 20);
    // Quatre mois : assez pour arriver à pied, trop peu pour qu'une maison sans
    // maintenance ait déjà brûlé.
    (0..3200).for_each(|_| world.tick());
    // Quatre huttes de huit habitants ; plus aucun immigrant en chemin.
    assert_eq!(world.population(), 32);
    assert!(world.figures.is_empty());
}

#[test]
fn a_save_restores_the_same_state() {
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    street(&mut world, x, y);
    assert!(world.place_building(FOUNTAIN, x + 8, y + 2));
    // On s'arrête en pleine activité : immigrants et porteur d'eau en chemin.
    (0..700).for_each(|_| world.tick());
    assert!(!world.figures.is_empty());
    let saved = world.save();
    let (mut other, ..) = self::world(&data);
    other.load(&saved).unwrap();
    assert_eq!(other.save(), saved);
    // Les deux mondes évoluent ensuite de la même façon.
    (0..2000).for_each(|_| {
        world.tick();
        other.tick();
    });
    assert_eq!(other.save(), world.save());
}

#[test]
fn houses_burn_without_maintenance_and_stand_with_it() {
    let Some(data) = data() else { return };
    // Trois ans sans maintenance : les maisons habitées finissent en décombres.
    let (mut world, x, y) = world(&data);
    street(&mut world, x, y);
    (0..3 * 9600).for_each(|_| world.tick());
    assert!(world.buildings.iter().flatten().count() < 4, "aucune maison n'a brûlé");
    assert!(!world.disasters.is_empty());

    // Avec une maintenance sur la rue, elles sont toujours là.
    let (mut world, x, y) = self::world(&data);
    street(&mut world, x, y);
    assert!(world.place_building(MAINTENANCE, x + 8, y + 2));
    (0..3 * 9600).for_each(|_| world.tick());
    assert_eq!(world.buildings.iter().flatten().count(), 5);
    assert!(world.disasters.is_empty());
}

#[test]
fn a_dairy_feeds_the_city_through_granary_and_vendor() {
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    // Types ajoutés pour cet essai : laiterie, grenier, agora, emplacement, épicière.
    let first = world.kinds.len();
    world.kinds.push(Kind::simple("laiterie".into(), 35, 2, 548, Function::Producer(zeus::world::Good::Food), 0));
    world.kinds.push(Kind::simple("grenier".into(), 100, 4, 473, Function::Granary, 0));
    world.kinds.push(Kind::simple("agora".into(), 50, 2, 460, Function::Agora { grand: false }, 0));
    world.kinds.push(Kind::simple("emplacement".into(), 0, 2, 460, Function::Plot, 0));
    world.kinds.push(Kind::simple("épicière".into(), 20, 2, 445, Function::Vendor(zeus::world::Good::Food), 0));
    street(&mut world, x, y);
    assert!(world.place_building(MAINTENANCE, x + 8, y + 2));
    assert!(world.place_building(first, x + 10, y + 3), "laiterie");
    assert!(world.place_building(first + 1, x + 6, y + 5), "grenier");
    assert!(world.place_building(first + 2, x, y + 5), "agora");
    assert!(world.place_building(first + 4, x, y + 5), "épicière");
    (0..3 * 9600).for_each(|_| world.tick());
    // Nourries, les quatre huttes sont devenues des masures.
    let levels: Vec<usize> = world.buildings.iter().flatten().filter_map(|b| b.house.as_ref()).map(|h| h.level).collect();
    assert_eq!(levels, [1, 1, 1, 1]);
}

#[test]
fn a_fishery_must_touch_water() {
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    let fishery = world.kinds.len();
    world.kinds.push(Kind::simple("pêcherie".into(), 65, 3, 719, Function::Fishery, 0));
    // En pleine terre : refusée.
    assert!(!world.can_place(fishery, x + 4, y + 4));
    // Quelque part sur la carte, un emplacement au bord de l'eau l'accepte.
    let shore = (0..GRID as i32).flat_map(|ty| (0..GRID as i32).map(move |tx| (tx, ty))).find(|&(tx, ty)| world.can_place(fishery, tx, ty));
    let (sx, sy) = shore.expect("aucun rivage constructible");
    assert!(world.place_building(fishery, sx, sy));
}

#[test]
fn goals_follow_the_city() {
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    street(&mut world, x, y);
    assert!(world.place_building(MAINTENANCE, x + 8, y + 2));
    let goal = |kind, values| Goal { kind, values };
    let population = goal(GoalKind::Population, [32, 0, 0]);
    let treasury = goal(GoalKind::Treasury, [20_000, 0, 0]);
    // Huit personnes en masure ou mieux : les huttes, sans vivres, ne comptent pas.
    let housing = goal(GoalKind::Housing, [1, 8, 0]);
    let profit = goal(GoalKind::Profit, [0, 0, 0]);
    assert_eq!(population.status(&world), Status::Pending(Some(0)));
    // Aucune année n'est encore close.
    assert_eq!(profit.status(&world), Status::Pending(None));
    (0..9600).for_each(|_| world.tick());
    assert_eq!(population.status(&world), Status::Met);
    assert_eq!(treasury.status(&world), Status::Pending(Some(world.money)));
    assert_eq!(housing.status(&world), Status::Pending(Some(0)));
    // Ni impôts ni salaires dans cette rue : une année à l'équilibre.
    assert_eq!(world.best_year_profit, Some(0));
    assert_eq!(profit.status(&world), Status::Met);
    assert_eq!(goal(GoalKind::Rule, [0, 0, 0]).status(&world), Status::Unsupported);
    // Le bilan annuel passe par la sauvegarde.
    let (mut other, ..) = self::world(&data);
    other.load(&world.save()).unwrap();
    assert_eq!(other.best_year_profit, Some(0));
}

#[test]
fn production_and_set_aside_goals() {
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    // Une laiterie livre son fromage (ressource 4) à un grenier.
    let first = world.kinds.len();
    let mut dairy = Kind::simple("laiterie".into(), 35, 2, 548, Function::Producer(zeus::world::Good::Food), 0);
    dairy.product = 4;
    world.kinds.push(dairy);
    world.kinds.push(Kind::simple("grenier".into(), 100, 4, 473, Function::Granary, 0));
    street(&mut world, x, y);
    assert!(world.place_building(MAINTENANCE, x + 8, y + 2));
    assert!(world.place_building(first, x + 10, y + 3), "laiterie");
    assert!(world.place_building(first + 1, x + 6, y + 5), "grenier");
    let goal = |kind, values| Goal { kind, values };
    let production = goal(GoalKind::Production, [4, 6, 0]);
    let set_aside = goal(GoalKind::SetAside, [4, 4, 0]);
    // Rien que la cité ne sache produire ou ranger : objectifs hors d'atteinte.
    assert_eq!(goal(GoalKind::Production, [10, 6, 0]).status(&world), Status::Unsupported);
    assert_eq!(goal(GoalKind::SetAside, [7, 4, 0]).status(&world), Status::Unsupported);
    assert_eq!(set_aside.status(&world), Status::Pending(Some(0)));
    (0..9600).for_each(|_| world.tick());
    // L'année est close : sa production reste acquise.
    assert!(world.best_produced[4] >= 6, "{}", world.best_produced[4]);
    assert_eq!(production.status(&world), Status::Met);
    // Au grenier, le fromage est rangé comme tel.
    let stored = world.stored(4);
    assert!(stored >= 4, "{stored}");
    let granary = world.buildings.iter().flatten().find(|b| b.kind == first + 1).unwrap();
    assert_eq!(granary.foods[4], granary.stock);
    // Tout ou rien : trop demander ne prend rien.
    assert!(!world.set_aside(4, stored + 1));
    assert!(world.set_aside(4, 4));
    assert_eq!(world.stored(4), stored - 4);
    assert_eq!(set_aside.status(&world), Status::Met);
    // Les compteurs passent par la sauvegarde, et un nouvel épisode les remet à zéro.
    let (mut other, ..) = self::world(&data);
    other.kinds = world.kinds.clone();
    other.load(&world.save()).unwrap();
    assert_eq!((other.set_aside[4], other.best_produced[4], other.stored(4)), (4, world.best_produced[4], stored - 4));
    other.start_episode();
    assert_eq!(set_aside.status(&other), Status::Pending(Some(0)));
}

#[test]
fn bronze_without_armory_waits_at_the_storehouse() {
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    let first = world.kinds.len();
    let mut foundry = Kind::simple("fonderie".into(), 40, 2, 618, Function::Supplier(zeus::world::Raw::Bronze), 0);
    foundry.product = 10;
    world.kinds.push(foundry);
    world.kinds.push(Kind::simple("entrepôt".into(), 70, 3, 934, Function::Storehouse, 0));
    street(&mut world, x, y);
    assert!(world.place_building(MAINTENANCE, x + 8, y + 2));
    assert!(world.place_building(first, x + 10, y + 3), "fonderie");
    assert!(world.place_building(first + 1, x + 6, y + 5), "entrepôt");
    let set_aside = Goal { kind: GoalKind::SetAside, values: [10, 8, 0] };
    assert_eq!(set_aside.status(&world), Status::Pending(Some(0)));
    (0..9600).for_each(|_| world.tick());
    assert!(world.stored(10) >= 8, "{}", world.stored(10));
    assert!(world.set_aside(10, 8));
    assert_eq!(set_aside.status(&world), Status::Met);
}

#[test]
fn wood_and_marble_reach_the_storehouse() {
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    let first = world.kinds.len();
    for (name, good, product) in [("scierie", Good::Wood, 9), ("maçonnerie", Good::Marble, 11)] {
        let mut kind = Kind::simple(name.into(), 40, 2, 598, Function::Producer(good), 0);
        kind.product = product;
        world.kinds.push(kind);
    }
    world.kinds.push(Kind::simple("entrepôt".into(), 70, 3, 934, Function::Storehouse, 0));
    street(&mut world, x, y);
    assert!(world.place_building(MAINTENANCE, x + 8, y + 2));
    assert!(world.place_building(first, x + 10, y + 3), "scierie");
    assert!(world.place_building(first + 1, x + 4, y + 5), "maçonnerie");
    assert!(world.place_building(first + 2, x + 6, y + 5), "entrepôt");
    (0..4800).for_each(|_| world.tick());
    assert!(world.stored(9) > 0 && world.stored(11) > 0, "{} {}", world.stored(9), world.stored(11));
    assert!(world.best_production(9) > 0);
}

#[test]
fn the_sculpture_studio_casts_the_foundry_bronze() {
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    let first = world.kinds.len();
    world.kinds.push(Kind::simple("fonderie".into(), 40, 2, 618, Function::Supplier(zeus::world::Raw::Bronze), 0));
    let mut studio = Kind::simple("atelier de sculpture".into(), 50, 2, 683, Function::Workshop(Good::Sculpture), 0);
    studio.product = 19;
    world.kinds.push(studio);
    world.kinds.push(Kind::simple("entrepôt".into(), 70, 3, 934, Function::Storehouse, 0));
    street(&mut world, x, y);
    assert!(world.place_building(MAINTENANCE, x + 8, y + 2));
    assert!(world.place_building(first, x + 10, y + 3), "fonderie");
    assert!(world.place_building(first + 1, x + 4, y + 5), "atelier");
    assert!(world.place_building(first + 2, x + 6, y + 5), "entrepôt");
    (0..9600).for_each(|_| world.tick());
    assert!(world.stored(19) > 0, "{}", world.stored(19));
    assert!(world.best_production(19) >= world.stored(19));
}

#[test]
fn a_sanctuary_is_built_from_storehouse_materials() {
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    let first = world.kinds.len();
    world.kinds.push(Kind::simple("refuge d'Hermès".into(), 640, 3, 1460, Function::Sanctuary { god: 7, needs: [4, 2, 1] }, 0));
    world.kinds.push(Kind::simple("entrepôt".into(), 70, 3, 934, Function::Storehouse, 0));
    assert!(world.place_building(first + 1, x, y), "entrepôt");
    assert!(world.place_building(first, x + 4, y), "sanctuaire");
    let hermes = Goal { kind: GoalKind::Sanctuary, values: [7, 1, 0] };
    let any_two = Goal { kind: GoalKind::Sanctuary, values: [-1, 2, 0] };
    // Un seul sanctuaire par dieu.
    assert!(!world.can_place(first, x + 8, y));
    (0..TICKS_PER_MONTH).for_each(|_| world.tick());
    assert_eq!(hermes.status(&world), Status::Pending(None));
    // Les matériaux arrivent à l'entrepôt ; le chantier les prend le mois suivant.
    let storehouse = world.buildings.iter_mut().flatten().find(|b| b.kind == first + 1).unwrap();
    storehouse.goods[Good::Marble as usize] = 4;
    storehouse.goods[Good::Wood as usize] = 4;
    storehouse.goods[Good::Sculpture as usize] = 1;
    (0..TICKS_PER_MONTH).for_each(|_| world.tick());
    assert_eq!(world.sanctuaries(), [7]);
    assert_eq!(hermes.status(&world), Status::Met);
    assert_eq!(any_two.status(&world), Status::Pending(Some(1)));
    assert_eq!((world.stored(11), world.stored(9), world.stored(19)), (0, 2, 0));
}

#[test]
fn heroes_fight_monsters_then_leave_on_quests() {
    use zeus::world::{Monster, Quest, QuestState};
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    // Ulysse (3) contre le Cyclope (3), puis le Palladium de Poséidon.
    world.monsters.push(Monster { event: 1, monster: 3, fight_until: None, slain: false });
    world.quests.push(Quest { event: 0, god: 1, quest: 0, hero: 3, state: QuestState::Waiting });
    let quest = Goal { kind: GoalKind::Quest, values: [0, 0, 0] };
    let slay = Goal { kind: GoalKind::Slay, values: [1, 0, 0] };
    // Sans temple, Ulysse ne vient pas.
    (0..2 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert_eq!(slay.status(&world), Status::Pending(None));
    let hall = world.kinds.len();
    world.kinds.push(Kind::simple("temple d'Ulysse".into(), 0, 4, 1460, Function::HeroHall(3), 0));
    assert!(world.place_building(hall, x, y));
    assert!(!world.can_place(hall, x + 5, y), "un seul temple par héros");
    // Ses exigences (huile, vin, logements luxueux…) ne sont pas remplies.
    (0..2 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert!(!world.heroes[3]);
    let checks: Vec<_> = zeus::heroes::requirements(3).map(|r| r.check(&world, (x as usize, y as usize, 4))).collect();
    assert_eq!(checks[0], zeus::heroes::Check::Unsimulated, "popularité");
    assert_eq!(checks[3], zeus::heroes::Check::Missing { current: 0, wanted: 32 }, "huile");
    // Une fois là, il affronte d'abord le monstre, puis part en quête.
    world.heroes[3] = true;
    (0..3 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert_eq!(slay.status(&world), Status::Met);
    assert!(matches!(world.quests[0].state, QuestState::Away { .. }));
    (0..7 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert_eq!(quest.status(&world), Status::Met);
    // L'état passe par la sauvegarde.
    let (mut other, ..) = self::world(&data);
    other.kinds = world.kinds.clone();
    other.load(&world.save()).unwrap();
    assert!(other.heroes[3] && other.quest_done(0) && other.monster_slain(1));
}

#[test]
fn elite_houses_support_soldiers() {
    use zeus::world::Soldier;
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    street(&mut world, x, y);
    let hoplites = Goal { kind: GoalKind::Support, values: [12, 2, 0] };
    assert_eq!(hoplites.status(&world), Status::Pending(Some(0)));
    // Une masure pleine fournit cinq hommes de piétaille, un hôtel particulier deux hoplites.
    let houses: Vec<usize> = (0..world.buildings.len()).filter(|&i| world.buildings[i].as_ref().is_some_and(|b| b.house.is_some())).collect();
    for (i, level) in [(houses[0], 2), (houses[1], 10)] {
        let house = world.buildings[i].as_mut().unwrap().house.as_mut().unwrap();
        // Au-delà de sa capacité, une maison ne fournit pas plus de soldats.
        (house.level, house.population) = (level, 1000);
    }
    assert_eq!(world.soldiers(Soldier::Rabble), 7);
    assert_eq!(world.soldiers(Soldier::Hoplite), 2);
    assert_eq!(hoplites.status(&world), Status::Met);
    assert_eq!(Goal { kind: GoalKind::Support, values: [57, 1, 0] }.status(&world), Status::Unsupported);
}

#[test]
fn a_strong_enough_army_makes_a_city_vassal() {
    use zeus::map::{City, RIVAL, VASSAL};
    use zeus::world::Conquest;
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    street(&mut world, x, y);
    world.cities = vec![City { active: true, kind: 2, name: 68, status: RIVAL, military: 1, trade: false }];
    let rule = Goal { kind: GoalKind::Rule, values: [0, 0, 0] };
    assert_eq!(rule.status(&world), Status::Pending(None));
    assert_eq!(world.conquer(0), Err(Conquest::TooWeak { army: 0, defence: 24 }));
    // Quatre maisons de ville pleines : 60 hommes de piétaille.
    for building in world.buildings.iter_mut().flatten() {
        if let Some(house) = building.house.as_mut() {
            (house.level, house.population) = (6, 1000);
        }
    }
    assert_eq!(world.army(), 60);
    assert_eq!(world.conquer(0), Ok(()));
    assert_eq!(world.cities[0].status, VASSAL);
    assert_eq!(rule.status(&world), Status::Met);
    assert_eq!(world.conquer(0), Err(Conquest::AlreadyRuled));
    // Une cité qui n'est plus rivale peut devenir partenaire commerciale.
    let partners = Goal { kind: GoalKind::Partners, values: [1, 0, 0] };
    assert_eq!(partners.status(&world), Status::Pending(Some(0)));
    assert!(world.open_trade(0));
    assert!(!world.open_trade(0), "déjà ouverte");
    assert_eq!(partners.status(&world), Status::Met);
    // Le statut passe par la sauvegarde.
    let (mut other, ..) = self::world(&data);
    other.cities = vec![City { active: true, kind: 2, name: 68, status: RIVAL, military: 1, trade: false }];
    other.load(&world.save()).unwrap();
    assert_eq!((other.cities[0].status, other.cities[0].trade), (VASSAL, true));
}

#[test]
fn world_changes_happen_on_their_date() {
    use zeus::map::{ALLY, City, RIVAL};
    let Some(data) = data() else { return };
    let (mut world, ..) = world(&data);
    world.cities = vec![City { active: true, kind: 2, name: 68, status: ALLY, military: 3, trade: false }];
    // Troie devient rivale en mars de l'année suivante, puis renforce son armée.
    world.schedule(zeus::world::BECOMES_RIVAL, (0, 0), (5, 10), 2, (1, 1));
    world.schedule(zeus::world::MILITARY_BUILDUP, (0, 0), (2, 2), 6, (1, 1));
    (0..12 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert_eq!(world.cities[0].status, ALLY);
    // La sauvegarde garde ce qui reste à venir.
    let (mut other, ..) = self::world(&data);
    other.cities = world.cities.clone();
    other.load(&world.save()).unwrap();
    assert_eq!(other.scheduled.len(), 2);
    (0..8 * TICKS_PER_MONTH).for_each(|_| other.tick());
    assert_eq!((other.cities[0].status, other.cities[0].military), (RIVAL, 5));
    assert!(other.scheduled.is_empty());
}
