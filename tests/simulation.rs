//! Essais de la simulation sur une vraie carte, quand `donnees/` est présent.

use std::fs;
use std::path::{Path, PathBuf};
use zeus::goals::{Goal, Kind as GoalKind, Status};
use zeus::map::{GRID, Map};
use zeus::model::Model;
use zeus::world::{Function, Kind, Service, WalkerImages, World};

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
    assert_eq!(goal(GoalKind::Quest, [0, 0, 0]).status(&world), Status::Unsupported);
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
