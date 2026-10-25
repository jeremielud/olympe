//! Essais de la simulation sur une vraie carte, quand `donnees/` est présent.

use std::fs;
use std::path::{Path, PathBuf};
use zeus::goals::{Goal, Kind as GoalKind, Status};
use zeus::map::{GRID, Map};
use zeus::model::Model;
use zeus::world::{Function, Good, Kind, Service, TICKS_PER_DAY, TICKS_PER_MONTH, WalkerImages, World};

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

/// Comme `world`, mais le coin libre est assez près des arbres et d'une carrière
/// de marbre pour qu'une scierie posée en (x + 10, y + 3) et une maçonnerie en
/// (x + 4, y + 5) aient leur ressource à portée.
fn world_with_resources(data: &Path) -> (World, i32, i32) {
    let (world, ..) = world(data);
    let reach = |x: i32, y: i32, bits: u32| {
        (y - 8..y + 10).any(|ty| (x - 8..x + 10).any(|tx| (0..GRID as i32).contains(&tx) && (0..GRID as i32).contains(&ty) && world.map.terrain(tx as usize, ty as usize) & bits != 0))
    };
    use zeus::world::{TERRAIN_QUARRY, TERRAIN_TREE};
    let (x, y) = (0..GRID as i32 * GRID as i32)
        .map(|i| (i % GRID as i32, i / GRID as i32))
        .find(|&(x, y)| world.can_build(x, y, 12) && reach(x + 10, y + 3, TERRAIN_TREE) && reach(x + 4, y + 5, TERRAIN_QUARRY))
        .unwrap();
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

    // Sans maintenance mais avec le sanctuaire d'Héphaïstos : aucun incendie (les huttes
    // ne risquent pas de s'effondrer).
    let (mut world, x, y) = self::world(&data);
    street(&mut world, x, y);
    world.kinds.push(Kind::simple("sanctuaire d'Héphaïstos".into(), 0, 3, 1460, Function::Sanctuary { god: 9, needs: [0, 0, 0] }, 0));
    assert!(world.place_building(world.kinds.len() - 1, x, y + 5));
    (0..3 * 9600).for_each(|_| world.tick());
    assert!(world.disasters.is_empty());
}

#[test]
fn a_dairy_feeds_the_city_through_granary_and_vendor() {
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    // Types ajoutés pour cet essai : laiterie, grenier, agora, emplacement, épicière.
    let first = world.kinds.len();
    world.kinds.push(Kind::simple("laiterie".into(), 35, 2, 565, Function::Producer(zeus::world::Good::Food), 0));
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
    let housing = goal(GoalKind::Housing, [3, 8, 0]);
    let profit = goal(GoalKind::Profit, [0, 0, 0]);
    assert_eq!(population.status(&world), Status::Pending(Some(0)));
    // Aucune année n'est encore close.
    assert_eq!(profit.status(&world), Status::Pending(None));
    // Quatre mois : les huttes sont pleines (plus tard, la maladie les éclaircit).
    (0..3200).for_each(|_| world.tick());
    assert_eq!(population.status(&world), Status::Met, "population {} humeur {}", world.population(), world.sentiment);
    (0..6400).for_each(|_| world.tick());
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
    let mut dairy = Kind::simple("laiterie".into(), 35, 2, 565, Function::Producer(zeus::world::Good::Food), 0);
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
    let (mut world, x, y) = world_with_resources(&data);
    // Le sujet est la chaîne du bronze : pas de rochers de cuivre à chercher.
    world.waived.push(10);
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
    let (mut world, x, y) = world_with_resources(&data);
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
    let (mut world, x, y) = world_with_resources(&data);
    // Le sujet est la chaîne du bronze : pas de rochers de cuivre à chercher.
    world.waived.push(10);
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
    // Une case de route entre l'entrepôt et le chantier : les porteurs y passent.
    assert!(world.place_road(x + 3, y + 1, 0));
    let hermes = Goal { kind: GoalKind::Sanctuary, values: [7, 1, 0] };
    let any_two = Goal { kind: GoalKind::Sanctuary, values: [-1, 2, 0] };
    // Un seul sanctuaire par dieu.
    assert!(!world.can_place(first, x + 8, y));
    (0..TICKS_PER_MONTH).for_each(|_| world.tick());
    assert_eq!(hermes.status(&world), Status::Pending(None));
    // Les matériaux arrivent à l'entrepôt ; des porteurs les apportent au chantier.
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
fn a_wharf_builds_a_trireme_from_wood() {
    let Some(data) = data() else { return };
    let campaign = zeus::pak::Campaign::parse(&fs::read(data.join("adventures/&persée et méduse.pak")).unwrap()).unwrap();
    let (base, ..) = world(&data);
    let mut world = World::new(campaign.parent().clone(), base.kinds.clone(), Vec::new(), WalkerImages::default(), 10_000);
    let (wharf, storehouse) = (world.kinds.len(), world.kinds.len() + 1);
    world.kinds.push(Kind::simple("quai des trières".into(), 150, 3, 721, Function::Wharf, 0));
    world.kinds.push(Kind::simple("entrepôt".into(), 70, 3, 934, Function::Storehouse, 0));
    // Au bord de l'eau seulement : on cherche une place sur la côte du raz-de-marée.
    let (wx, wy) = world.map.disaster_point(3).unwrap();
    let shore = (-12..12).flat_map(|dy| (-12..12).map(move |dx| (wx as i32 + dx, wy as i32 + dy))).find(|&(x, y)| world.can_place(wharf, x, y)).unwrap();
    assert!(world.place_building(wharf, shore.0, shore.1));
    // Une case de route le long du quai, et l'entrepôt contre elle.
    let around = |(x, y): (i32, i32), size: i32| (0..size).flat_map(move |d| [(x + d, y - 1), (x + d, y + size), (x - 1, y + d), (x + size, y + d)]);
    let (road, site) = around(shore, 3)
        .filter(|&(rx, ry)| world.can_build(rx, ry, 1))
        .find_map(|road| (-3..=1).flat_map(|dy| (-3..=1).map(move |dx| (road.0 + dx, road.1 + dy))).find(|&s| world.can_place(storehouse, s.0, s.1) && around(s, 3).any(|t| t == road) && !(s.0..s.0 + 3).contains(&road.0) | !(s.1..s.1 + 3).contains(&road.1)).map(|s| (road, s)))
        .unwrap();
    assert!(world.place_building(storehouse, site.0, site.1));
    assert!(world.place_road(road.0, road.1, 0));
    world.buildings.iter_mut().flatten().find(|b| b.kind == storehouse).unwrap().goods[Good::Wood as usize] = 6;
    let goal = Goal { kind: GoalKind::Support, values: [57, 1, 0] };
    assert_eq!(goal.status(&world), Status::Pending(Some(0)));
    // Sans armement, le chantier ne commence pas.
    (0..5 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert_eq!(world.triremes(), 0);
    // Deux charges d'armement : le chantier dure 32 jours (deux mois), puis la trière
    // part ; deux des six charges de bois ont servi.
    world.buildings.iter_mut().flatten().find(|b| b.kind == storehouse).unwrap().goods[Good::Arms as usize] = 2;
    (0..TICKS_PER_MONTH).for_each(|_| world.tick());
    assert_eq!(world.triremes(), 0);
    (0..2 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert_eq!((world.triremes(), world.stored(9)), (1, 4));
    assert_eq!(goal.status(&world), Status::Met);
    // La trière part seule soumettre une cité sans défense : son quai l'attend sans
    // en bâtir d'autre, puis elle rentre.
    world.cities = vec![zeus::map::City { active: true, kind: 2, name: 68, status: zeus::map::RIVAL, military: 1, trade: false, favor: 50, sells: vec![], buys: vec![], sold: 0, bought: 0, army: 0, tribute_received: 0, tribute_paid: 0, wealth: 3, strength: 0 }];
    assert_eq!(world.army(), 6);
    assert_eq!(world.conquer(0, false), Ok(2));
    assert_eq!((world.army(), world.expedition.unwrap().triremes), (0, 1));
    (0..4 * TICKS_PER_MONTH + TICKS_PER_DAY).for_each(|_| world.tick());
    assert!(world.expedition.is_none());
    assert_eq!((world.triremes(), world.cities[0].status), (1, zeus::map::VASSAL));
}

#[test]
fn a_closed_racetrack_makes_a_hippodrome() {
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    let track = world.kinds.len();
    world.kinds.push(Kind::simple("hippodrome".into(), 60, 4, 426, Function::Racetrack, 0));
    let goal = Goal { kind: GoalKind::Hippodrome, values: [8, 0, 0] };
    // Une boucle de trois sections sur trois, vide au milieu : huit stades.
    let ring = [(0, 0), (4, 0), (8, 0), (8, 4), (8, 8), (4, 8), (0, 8), (0, 4)];
    for (i, &(dx, dy)) in ring.iter().enumerate() {
        assert!(world.place_building(track, x + dx, y + dy), "section {i}");
        if i < ring.len() - 1 {
            assert_eq!(world.hippodrome().1, false);
        }
    }
    // Une section détachée ne se pose pas.
    assert!(!world.can_place(track, x + 13, y + 9));
    assert_eq!(world.hippodrome(), (8, true));
    assert_eq!(goal.status(&world), Status::Met);
    // Les coins sont des virages, les côtés des droites.
    let image = |dx: i32, dy: i32| world.draw((x + dx) as usize, (y + dy + 3) as usize).map(|d| d.image);
    assert_eq!((image(0, 0), image(8, 8)), (Some(429), Some(425)));
    assert!(matches!(image(4, 0), Some(422 | 426)) && matches!(image(0, 4), Some(424 | 428)));
}

#[test]
fn extractors_need_their_deposit() {
    let Some(data) = data() else { return };
    // Persée et Méduse : une carrière de marbre sur la carte de la cité mère, pas de
    // cuivre.
    let campaign = zeus::pak::Campaign::parse(&fs::read(data.join("adventures/&persée et méduse.pak")).unwrap()).unwrap();
    let (base, ..) = world(&data);
    let mut world = World::new(campaign.parent().clone(), base.kinds.clone(), Vec::new(), WalkerImages::default(), 100_000);
    let (quarry, foundry) = (world.kinds.len(), world.kinds.len() + 1);
    let mut mason = Kind::simple("maçonnerie".into(), 40, 2, 609, Function::Producer(Good::Marble), 0);
    mason.product = 11;
    world.kinds.push(mason);
    let mut smelter = Kind::simple("fonderie".into(), 40, 2, 618, Function::Supplier(zeus::world::Raw::Bronze), 0);
    smelter.product = 10;
    world.kinds.push(smelter);
    let anywhere = |w: &World, kind: usize| (0..GRID as i32 * GRID as i32).any(|t| w.can_place(kind, t % GRID as i32, t / GRID as i32));
    assert!(anywhere(&world, quarry));
    assert!(!anywhere(&world, foundry));
    // Un objectif qui exige du bronze lève la règle sur cette carte.
    assert!(world.waive_missing_deposit(10));
    assert!(anywhere(&world, foundry));
}

#[test]
fn a_pyramid_rises_with_its_materials() {
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    let layout = zeus::monument::Layout::parse(&String::from_utf8_lossy(&fs::read(data.join("model/poseidon_pyramid_02.txt")).unwrap())).unwrap();
    let needs = layout.materials();
    let first = world.kinds.len();
    let mut pyramid = Kind::simple("pyramide".into(), 0, layout.width, 663, Function::Monument { model: 1, needs }, 0);
    pyramid.depth = layout.depth;
    world.kinds.push(pyramid);
    world.kinds.push(Kind::simple("entrepôt".into(), 70, 3, 934, Function::Storehouse, 0));
    world.monument_layouts = vec![layout.clone(), layout];
    assert!(world.place_building(first + 1, x, y), "entrepôt");
    assert!(world.place_building(first, x + 4, y), "pyramide");
    assert!(world.place_road(x + 3, y + 1, 0));
    // Une pyramide traditionnelle (101), comme dans « La naissance de l'Atlantide ».
    let goal = Goal { kind: GoalKind::Pyramid, values: [1, 101, 0] };
    let any = Goal { kind: GoalKind::Pyramid, values: [2, -1, 0] };
    assert_eq!(goal.status(&world), Status::Pending(Some(0)));
    // Le sommet n'est d'abord que le sol du chantier ; la moitié du marbre élève la
    // moitié des étapes.
    let top = (x as usize + 4 + 3, y as usize + 2);
    assert!(world.draw(top.0, top.1).is_some_and(|d| d.above.is_none()));
    let storehouse = world.buildings.iter_mut().flatten().find(|b| b.kind == first + 1).unwrap();
    storehouse.goods[Good::Marble as usize] = needs[0] as i32;
    storehouse.goods[Good::Wood as usize] = needs[1] as i32;
    (0..3 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert_eq!(goal.status(&world), Status::Met);
    assert_eq!(any.status(&world), Status::Pending(Some(1)));
    // Achevée, elle porte son sommet quatre niveaux plus haut.
    assert!(world.draw(top.0, top.1).is_some_and(|d| d.above.is_some_and(|(_, _, lift)| lift == 120)));
}

#[test]
fn heroes_fight_monsters_then_leave_on_quests() {
    use zeus::world::{Monster, Quest, QuestState};
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    // Ulysse (3) contre le Cyclope (3), puis le Palladium de Poséidon.
    world.monsters.push(Monster { event: 1, monster: 3, fight_until: None, slain: false, arrives_at: 0 });
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
    // Popularité excellente : l'humeur de la cité, sur 100.
    assert_eq!(checks[0], zeus::heroes::Check::Missing { current: world.sentiment, wanted: 89 }, "popularité");
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
    // Pas de quai, pas de trière.
    assert_eq!(Goal { kind: GoalKind::Support, values: [57, 1, 0] }.status(&world), Status::Pending(Some(0)));
}

#[test]
fn a_strong_enough_army_makes_a_city_vassal() {
    use zeus::map::{City, RIVAL, VASSAL};
    use zeus::world::{CampaignNews, Conquest};
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    street(&mut world, x, y);
    world.cities = vec![City { active: true, kind: 2, name: 68, status: RIVAL, military: 1, trade: false, favor: 50, sells: vec![], buys: vec![], sold: 0, bought: 0, army: 0, tribute_received: 0, tribute_paid: 0, wealth: 3, strength: -1 }];
    let rule = Goal { kind: GoalKind::Rule, values: [0, 0, 0] };
    assert_eq!(rule.status(&world), Status::Pending(None));
    assert_eq!(world.conquer(0, false), Err(Conquest::NoArmy));
    // Quatre maisons de ville pleines : 60 hommes de piétaille.
    for building in world.buildings.iter_mut().flatten() {
        if let Some(house) = building.house.as_mut() {
            (house.level, house.population) = (6, 1000);
        }
    }
    assert_eq!(world.army(), 60);
    // Quatre ans plus tard, la cité (un bouclier, en héros) se défend avec
    // √4 × 4,5 = 9 hommes.
    world.ticks = 4 * 12 * TICKS_PER_MONTH;
    assert_eq!(world.defence(0), 9);
    // Sans route connue, deux mois de marche ; l'armée quitte la cité.
    assert_eq!(world.conquer(0, false), Ok(2));
    assert_eq!(world.army(), 0);
    assert_eq!(world.conquer(0, true), Err(Conquest::Away));
    // La campagne passe par la sauvegarde.
    let saved = world.save();
    let (mut other, ..) = self::world(&data);
    other.cities = world.cities.clone();
    other.load(&saved).unwrap();
    assert_eq!(other.expedition, world.expedition);
    (0..2 * TICKS_PER_MONTH + TICKS_PER_DAY).for_each(|_| world.tick());
    // Victoire : écart de 85 % (au-delà de 74), aucune perte ; la cité perd la moitié
    // de sa défense (9 − 4), en reprend un point le même jour (début de mois) et
    // devient vassale.
    assert_eq!(world.campaign_news, vec![CampaignNews::Conquered(0)]);
    assert_eq!(world.cities[0].status, VASSAL);
    assert_eq!(world.cities[0].strength, 6);
    assert_eq!(rule.status(&world), Status::Met);
    assert_eq!(world.conquer(0, false), Err(Conquest::AlreadyRuled));
    world.campaign_news.clear();
    (0..2 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert_eq!(world.campaign_news, vec![CampaignNews::Returned { city: 0, won: true }]);
    assert!(world.expedition.is_none());
    // Une cité qui n'est plus rivale devient partenaire commerciale dès que le
    // commerce s'ouvre (drapeau de la carte, ou événement).
    let partners = Goal { kind: GoalKind::Partners, values: [1, 0, 0] };
    assert_eq!(partners.status(&world), Status::Pending(Some(0)));
    world.cities[0].trade = true;
    assert_eq!(partners.status(&world), Status::Met);
    // Le statut passe par la sauvegarde.
    let (mut other, ..) = self::world(&data);
    other.cities = vec![City { active: true, kind: 2, name: 68, status: RIVAL, military: 1, trade: false, favor: 50, sells: vec![], buys: vec![], sold: 0, bought: 0, army: 0, tribute_received: 0, tribute_paid: 0, wealth: 3, strength: -1 }];
    other.load(&world.save()).unwrap();
    assert_eq!((other.cities[0].status, other.cities[0].trade), (VASSAL, true));
}

#[test]
fn a_raid_brings_back_plunder_or_dies_trying() {
    use zeus::map::{City, RIVAL};
    use zeus::world::CampaignNews;
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    street(&mut world, x, y);
    // Une cité riche (richesse 3) qui n'échange rien : le butin est en drachmes,
    // (3 + 1) × 3 × 200 en héros.
    world.cities = vec![City { active: true, kind: 2, name: 68, status: RIVAL, military: 1, trade: false, favor: 50, sells: vec![], buys: vec![], sold: 0, bought: 0, army: 0, tribute_received: 0, tribute_paid: 0, wealth: 3, strength: 9 }];
    for building in world.buildings.iter_mut().flatten() {
        if let Some(house) = building.house.as_mut() {
            (house.level, house.population) = (6, 1000);
        }
    }
    let money = world.money;
    assert_eq!(world.conquer(0, true), Ok(2));
    (0..4 * TICKS_PER_MONTH + TICKS_PER_DAY).for_each(|_| world.tick());
    assert_eq!(world.campaign_news, vec![CampaignNews::Plunder { city: 0, resource: zeus::world::DRACHMAS, amount: 2400 }]);
    assert!(world.money >= money + 2400 - 500, "{} puis {}", money, world.money);
    // Contre une cité bien plus forte, le raid perd tout son monde.
    world.campaign_news.clear();
    world.cities[0].strength = 1000;
    let population: i32 = world.buildings.iter().flatten().filter_map(|b| b.house.as_ref()).map(|h| h.population).sum();
    assert_eq!(world.conquer(0, true), Ok(2));
    (0..2 * TICKS_PER_MONTH + TICKS_PER_DAY).for_each(|_| world.tick());
    assert_eq!(world.campaign_news, vec![CampaignNews::WipedOut(0)]);
    assert!(world.expedition.is_none());
    let after: i32 = world.buildings.iter().flatten().filter_map(|b| b.house.as_ref()).map(|h| h.population).sum();
    assert!(after < population, "{population} puis {after}");
}

#[test]
fn world_changes_happen_on_their_date() {
    use zeus::map::{ALLY, City, RIVAL};
    let Some(data) = data() else { return };
    let (mut world, ..) = world(&data);
    world.cities = vec![City { active: true, kind: 2, name: 68, status: ALLY, military: 3, trade: false, favor: 50, sells: vec![], buys: vec![], sold: 0, bought: 0, army: 0, tribute_received: 0, tribute_paid: 0, wealth: 3, strength: -1 }];
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

#[test]
fn cities_ask_for_goods_and_drachmas() {
    use zeus::map::{ALLY, City};
    use zeus::world::{DRACHMAS, REQUEST};
    let Some(data) = data() else { return };
    let (mut world, ..) = world(&data);
    world.cities = vec![City { active: true, kind: 2, name: 57, status: ALLY, military: 3, trade: false, favor: 50, sells: vec![], buys: vec![], sold: 0, bought: 0, army: 0, tribute_received: 0, tribute_paid: 0, wealth: 3, strength: -1 }; 2];
    world.schedule(REQUEST + DRACHMAS, (0, 0), (1500, 1500), 1, (0, 0));
    world.schedule(REQUEST + 14, (1, 1), (8, 8), 1, (0, 0));
    (0..2 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert_eq!(world.requests.len(), 2);
    // Pas de laine en réserve : la demande reste ; les drachmes, elles, sont là.
    let money = world.money;
    assert!(!world.fulfil(1));
    assert!(world.fulfil(0));
    assert_eq!(world.money, money - 1500);
    // Sans réponse, la demande finit par expirer.
    (0..12 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert!(world.requests.is_empty());
}

#[test]
fn neglected_requests_turn_allies_into_rivals() {
    use zeus::map::{ALLY, City, RIVAL};
    use zeus::world::{DRACHMAS, REQUEST};
    let Some(data) = data() else { return };
    let (mut world, ..) = world(&data);
    world.cities = vec![City { active: true, kind: 2, name: 57, status: ALLY, military: 3, trade: false, favor: 35, sells: vec![], buys: vec![], sold: 0, bought: 0, army: 0, tribute_received: 0, tribute_paid: 0, wealth: 3, strength: -1 }];
    world.schedule(REQUEST + DRACHMAS, (0, 0), (100, 100), 1, (0, 0));
    world.schedule(REQUEST + DRACHMAS, (0, 0), (100, 100), 3, (0, 0));
    (0..3 * TICKS_PER_MONTH).for_each(|_| world.tick());
    // Servie une fois, elle est plus amicale ; ignorée ensuite, elle se fâche.
    assert!(world.fulfil(0));
    assert_eq!(world.cities[0].favor, 50);
    (0..14 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert_eq!((world.cities[0].favor, world.cities[0].status), (30, ALLY));
    world.cities[0].favor = 25;
    world.schedule(REQUEST + DRACHMAS, (0, 0), (100, 100), 0, (1, 1));
    (0..26 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert_eq!(world.cities[0].status, RIVAL);
}

#[test]
fn a_living_monster_wrecks_the_city_until_slain() {
    use zeus::map::Source;
    use zeus::world::Monster;
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    street(&mut world, x, y);
    let standing = |w: &World| w.buildings.iter().flatten().count();
    let before = standing(&world);
    world.monsters.push(Monster { event: 0, monster: 3, fight_until: None, slain: false, arrives_at: 0 });
    (0..TICKS_PER_MONTH + 1).for_each(|_| world.tick());
    assert_eq!(world.figures.iter().filter(|f| f.source == Source::Monster(3)).count(), 1);
    (0..12 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert!(standing(&world) < before, "le Cyclope n'a rien détruit");
    // La sauvegarde garde le monstre en marche.
    let (mut other, ..) = self::world(&data);
    other.load(&world.save()).unwrap();
    assert_eq!(other.figures.iter().filter(|f| f.source == Source::Monster(3)).count(), 1);
    world.monsters[0].slain = true;
    (0..TICKS_PER_MONTH).for_each(|_| world.tick());
    assert!(world.figures.iter().all(|f| f.source != Source::Monster(3)));
}

#[test]
fn an_invading_monster_waits_for_its_date() {
    use zeus::map::Source;
    use zeus::world::{HeroNews, Monster};
    let Some(data) = data() else { return };
    let (mut world, ..) = world(&data);
    let arrives_at = world.date_in(5, (1, 1));
    world.monsters.push(Monster { event: 4, monster: 0, fight_until: None, slain: false, arrives_at });
    (0..12 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert!(world.figures.iter().all(|f| f.source != Source::Monster(0)), "l'Hydre est venue trop tôt");
    (0..7 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert!(world.figures.iter().any(|f| f.source == Source::Monster(0)));
    assert!(world.hero_news.contains(&HeroNews::Invaded(4)));
}

#[test]
fn a_hero_walks_to_his_monster_then_leaves_on_his_quest() {
    use zeus::map::Source;
    use zeus::world::{Monster, Quest, QuestState};
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    street(&mut world, x, y);
    let hall = world.kinds.len();
    world.kinds.push(Kind::simple("temple d'Hercule".into(), 0, 4, 1460, Function::HeroHall(1), 0));
    assert!(world.place_building(hall, x, y + 5));
    world.heroes[1] = true;
    let hero = |w: &World| w.figures.iter().find(|f| f.source == Source::Hero(1)).map(|f| (f.x, f.y));
    (0..TICKS_PER_MONTH + 1).for_each(|_| world.tick());
    let home = hero(&world).expect("Hercule n'est pas à son temple");
    // L'Hydre arrive : Hercule marche à sa rencontre.
    world.monsters.push(Monster { event: 0, monster: 0, fight_until: None, slain: false, arrives_at: 0 });
    (0..TICKS_PER_MONTH + 200).for_each(|_| world.tick());
    assert_ne!(hero(&world), Some(home), "Hercule n'a pas bougé");
    // Une fois en quête, il n'est plus dans la cité.
    world.monsters[0].slain = true;
    world.quests.push(Quest { event: 1, god: 3, quest: 0, hero: 1, state: QuestState::Away { until: world.ticks + 100 * TICKS_PER_MONTH } });
    (0..TICKS_PER_MONTH).for_each(|_| world.tick());
    assert_eq!(hero(&world), None);
}

#[test]
fn a_hostile_god_ravages_then_leaves() {
    use zeus::map::Source;
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    street(&mut world, x, y);
    let before = world.buildings.iter().flatten().count();
    world.schedule_god(5, 3, (0, 0));
    // La sauvegarde garde la venue programmée.
    let (mut other, ..) = self::world(&data);
    other.load(&world.save()).unwrap();
    assert_eq!(other.invasions.len(), 1);
    (0..4 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert!(world.figures.iter().any(|f| f.source == Source::Deity(5)), "Arès n'est pas venu");
    assert_eq!(world.god_news, [(5, true)]);
    (0..7 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert!(world.figures.iter().all(|f| f.source != Source::Deity(5)), "Arès n'est pas reparti");
    assert!(world.buildings.iter().flatten().count() < before);
}

#[test]
fn a_friendly_god_drives_a_hostile_god_away() {
    use zeus::map::Source;
    let Some(data) = data() else { return };
    // Arès est-il encore là trois mois après l'arrivée prévue, selon le sanctuaire bâti ?
    let ares_stays = |god: u8| {
        let (mut world, x, y) = world(&data);
        let first = world.kinds.len();
        world.kinds.push(Kind::simple("sanctuaire".into(), 0, 3, 1460, Function::Sanctuary { god, needs: [0, 0, 0] }, 0));
        assert!(world.place_building(first, x, y));
        assert_eq!(world.sanctuaries(), [god]);
        world.schedule_god(5, 1, (0, 0));
        (0..2 * TICKS_PER_MONTH + 10).for_each(|_| world.tick());
        assert!(world.figures.iter().any(|f| f.source == Source::Deity(5)));
        (0..TICKS_PER_MONTH + 10).for_each(|_| world.tick());
        world.figures.iter().any(|f| f.source == Source::Deity(5))
    };
    // Aphrodite le chasse ; Hermès ne défend pas la cité contre lui.
    assert!(!ares_stays(6), "Arès est resté malgré Aphrodite");
    assert!(ares_stays(7), "Hermès a chassé Arès");
}

#[test]
fn trade_partners_buy_surplus_and_sell_what_is_missing() {
    use zeus::map::{ALLY, City};
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    let (first, docks) = (world.kinds.len(), world.kinds.len() + 1);
    world.kinds.push(Kind::simple("poste de commerce".into(), 70, 3, 934, Function::TradePost { sea: false }, 0));
    world.kinds.push(Kind::simple("docks".into(), 70, 3, 934, Function::TradePost { sea: true }, 0));
    // Une cité achète de la laine (24 par an) et vend du vin (12 par an) ; sa route
    // est de terre (pas de route connue) : des docks n'y font rien.
    world.cities = vec![City { active: true, kind: 2, name: 40, status: ALLY, military: 3, trade: true, favor: 50, sells: vec![(21, 12)], buys: vec![(14, 24)], sold: 0, bought: 0, army: 0, tribute_received: 0, tribute_paid: 0, wealth: 3, strength: -1 }];
    assert!(!world.can_place(docks, x, y), "les docks se posent au bord de l'eau");
    assert!(world.trade_posts().is_empty());
    assert!(world.place_building(first, x, y));
    assert_eq!(world.trade_posts(), vec![0]);
    // Le poste de commerce range la laine comme un entrepôt.
    world.buildings.iter_mut().flatten().find(|b| b.kind == first).unwrap().goods[Good::Fleece as usize] = 12;
    assert_eq!(world.stored(14), 12);
    let money = world.money;
    (0..TICKS_PER_MONTH + 1).for_each(|_| world.tick());
    // Laine au-delà de la réserve vendue (2 par mois), vin acheté (1 par mois).
    assert_eq!((world.stored(14), world.stored(21)), (10, 1));
    // Aux prix de la carte : la laine 60 drachmes, le vin 160.
    assert_eq!((world.price(14), world.price(21)), (60, 160));
    assert_eq!((world.cities[0].sold, world.cities[0].bought), (120, 160));
    assert_eq!(world.money - money, 120 - 160 - world.last_month.1 + world.last_month.0);
    // Le joueur garde sa laine et n'achète plus de vin : plus rien ne change.
    use zeus::world::TradePolicy;
    world.trade_policy[14] = TradePolicy::BuyOnly;
    world.trade_policy[21] = TradePolicy::SellOnly;
    (0..TICKS_PER_MONTH).for_each(|_| world.tick());
    assert_eq!((world.stored(14), world.stored(21)), (10, 1));
    // Le réglage passe par la sauvegarde.
    let (mut other, ..) = self::world(&data);
    other.kinds = world.kinds.clone();
    other.load(&world.save()).unwrap();
    assert_eq!(other.trade_policy[14], TradePolicy::BuyOnly);
}

#[test]
fn vassals_pay_a_yearly_tribute() {
    use zeus::map::{City, VASSAL};
    let Some(data) = data() else { return };
    let (mut world, ..) = world(&data);
    world.cities = vec![City { active: true, kind: 2, name: 68, status: VASSAL, military: 2, trade: false, favor: 50, sells: vec![], buys: vec![], sold: 0, bought: 0, army: 0, tribute_received: 0, tribute_paid: 0, wealth: 3, strength: -1 }];
    (0..12 * TICKS_PER_MONTH + 1).for_each(|_| world.tick());
    assert_eq!(world.tributes, [(0, 350)]);
}

#[test]
fn friendly_gods_visit_once_a_year() {
    use zeus::map::Source;
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    let first = world.kinds.len();
    world.kinds.push(Kind::simple("refuge d'Hermès".into(), 0, 3, 1460, Function::Sanctuary { god: 7, needs: [0, 0, 0] }, 0));
    assert!(world.place_building(first, x, y));
    (0..12 * TICKS_PER_MONTH + 1).for_each(|_| world.tick());
    assert_eq!(world.blessings, [(7, 23, 300)]);
    assert!(world.figures.iter().any(|f| f.source == Source::Deity(7)), "Hermès n'est pas venu");
    (0..TICKS_PER_MONTH + 20).for_each(|_| world.tick());
    assert!(world.figures.iter().all(|f| f.source != Source::Deity(7)), "Hermès n'est pas reparti");
}

#[test]
fn an_army_lands_from_the_sea() {
    use zeus::map::{City, RIVAL};
    let Some(data) = data() else { return };
    let campaign = zeus::pak::Campaign::parse(&fs::read(data.join("adventures/@la vie en méditerranée.pak")).unwrap()).unwrap();
    let (base, ..) = world(&data);
    let mut world = World::new(campaign.parent().clone(), base.kinds.clone(), Vec::new(), WalkerImages { ship: 6465, ..WalkerImages::default() }, 0);
    let point = (9..=16).find(|&n| world.map.invasion_point(n).is_some()).expect("un point d'invasion maritime");
    world.cities = vec![City { active: true, kind: 2, name: 40, status: RIVAL, military: 3, trade: false, favor: 50, sells: vec![], buys: vec![], sold: 0, bought: 0, army: 0, tribute_received: 0, tribute_paid: 0, wealth: 3, strength: -1 }];
    // Une maison à piller, pour que l'armée reste.
    let site = (0..GRID as i32 * GRID as i32).map(|t| (t % GRID as i32, t / GRID as i32)).find(|&(x, y)| world.can_place(HOUSE, x, y)).unwrap();
    world.money = 1000;
    assert!(world.place_building(HOUSE, site.0, site.1));
    world.fighters = zeus::model::Fighters::parse(&fs::read(data.join("model/figure_model_normal.txt")).unwrap()).unwrap();
    world.schedule_invasion((0, 0), (10, 10), (point as i16, point as i16), 1, (0, 0));
    (0..TICKS_PER_MONTH + 1).for_each(|_| world.tick());
    // Un navire au point maritime, dix soldats débarqués sur la terre ferme.
    let (sx, sy) = world.map.invasion_point(point).unwrap();
    assert!(world.figures.iter().any(|f| f.soldier().is_none() && (f.x as usize, f.y as usize) == (sx, sy)));
    let soldiers: Vec<_> = world.figures.iter().filter(|f| f.soldier() == Some(true)).collect();
    assert_eq!(soldiers.len(), 10);
    // Grecs : six hoplites, deux cavaliers, deux frondeurs (fréquences 60, 20, 20).
    let mut kinds: Vec<usize> = soldiers.iter().map(|f| f.image()).collect();
    kinds.sort_unstable();
    kinds.dedup();
    assert!(kinds.len() >= 2, "{kinds:?}");
    assert!(soldiers.iter().all(|f| world.map.terrain(f.x.round() as usize, f.y.round() as usize) & zeus::world::TERRAIN_WATER == 0));
}

#[test]
fn rival_armies_sack_a_defenceless_city() {
    use zeus::map::{ALLY, City, RIVAL};
    use zeus::world::INVASION;
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    street(&mut world, x, y);
    let city = |status| City { active: true, kind: 2, name: 68, status, military: 3, trade: false, favor: 50, sells: vec![], buys: vec![], sold: 0, bought: 0, army: 0, tribute_received: 0, tribute_paid: 0, wealth: 3, strength: -1 };
    world.cities = vec![city(RIVAL), city(ALLY)];
    world.schedule(INVASION, (0, 0), (32, 32), 1, (0, 0));
    world.schedule(INVASION, (1, 1), (32, 32), 1, (0, 0));
    let (money, standing) = (world.money, world.buildings.iter().flatten().count());
    // L'armée entre par un point d'invasion de la carte et marche jusqu'à la cité.
    (0..2 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert!(world.raid.is_some() && world.figures.iter().filter(|f| f.soldier() == Some(true)).count() == 32);
    (0..4 * TICKS_PER_MONTH).for_each(|_| world.tick());
    // Seule la rivale attaque ; sans armée, la cité est pillée.
    assert_eq!(world.battles.len(), 1);
    let battle = world.battles[0];
    assert!(!battle.repelled && battle.buildings >= 2 && battle.drachmas > 0);
    // Deux bâtiments brûlés par l'armée (une maison sans entretien peut aussi brûler
    // pendant ces six mois).
    assert!(world.money < money && world.buildings.iter().flatten().count() <= standing - 2);
    // Avec une armée assez forte, elle est repoussée : on rebâtit les maisons perdues.
    world.battles.clear();
    for i in 0..4 {
        world.place_building(HOUSE, x + 2 * i, y + 2);
    }
    for building in world.buildings.iter_mut().flatten() {
        if let Some(house) = building.house.as_mut() {
            (house.level, house.population) = (6, 1000);
        }
    }
    assert!(world.army() >= 30);
    world.schedule(INVASION, (0, 0), (5, 5), 7, (0, 0));
    (0..8 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert!(world.battles.last().is_some_and(|b| b.repelled));
}

#[test]
fn gifts_bring_the_goods_set_aside_by_the_previous_city() {
    use zeus::map::{City, COLONY};
    use zeus::world::GIFT;
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    let first = world.kinds.len();
    world.kinds.push(Kind::simple("entrepôt".into(), 70, 3, 934, Function::Storehouse, 0));
    assert!(world.place_building(first, x, y));
    world.cities = vec![City { active: true, kind: 0, name: 5, status: COLONY, military: 0, trade: false, favor: 50, sells: vec![], buys: vec![], sold: 0, bought: 0, army: 0, tribute_received: 0, tribute_paid: 0, wealth: 3, strength: -1 }];
    world.carried[14] = 12;
    // Quantité « −1 » : la laine mise de côté ; puis 500 drachmes offertes.
    world.schedule(GIFT + 14, (0, 0), (-1, -1), 1, (0, 0));
    world.schedule(GIFT + 23, (0, 0), (500, 500), 1, (0, 0));
    let money = world.money;
    (0..2 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert_eq!(world.stored(14), 12);
    assert_eq!(world.gifts, [(0, 14, 12), (0, 23, 500)]);
    assert!(world.money >= money + 500 - 100);
}

#[test]
fn disasters_strike_the_zones_painted_on_the_map() {
    use zeus::world::{EARTHQUAKE, TIDAL_WAVE};
    let Some(data) = data() else { return };
    let campaign = zeus::pak::Campaign::parse(&fs::read(data.join("adventures/&persée et méduse.pak")).unwrap()).unwrap();
    let (base, ..) = world(&data);
    let mut world = World::new(campaign.parent().clone(), base.kinds.clone(), Model::parse(&fs::read(data.join("model/zeus_model_normal.txt")).unwrap()).unwrap().houses, WalkerImages::default(), 10_000);
    // Le point 2 de Persée est sur la fissure, le point 3 dans la zone du raz-de-marée.
    let (fx, fy) = world.map.disaster_point(2).unwrap();
    assert_ne!(world.map.terrain(fx, fy) & zeus::world::TERRAIN_FISSURE, 0);
    let (wx, wy) = world.map.disaster_point(3).unwrap();
    assert!(world.can_build(fx as i32, fy as i32, 1) && world.can_build(wx as i32, wy as i32, 1));
    assert!(world.place_building(HOUSE, fx as i32, fy as i32));
    assert_eq!(world.strike(EARTHQUAKE, 2), 1);
    assert!(world.building_at(fx as i32, fy as i32).is_none() && !world.can_build(fx as i32, fy as i32, 1));
    world.strike(TIDAL_WAVE, 3);
    assert!(!world.can_build(wx as i32, wy as i32, 1));
    // La sauvegarde garde la fissure et l'eau ; l'eau se retire au bout de six mois.
    let saved = world.save();
    let mut other = World::new(campaign.parent().clone(), world.kinds.clone(), Vec::new(), WalkerImages::default(), 0);
    other.load(&saved).unwrap();
    assert_eq!(other.save(), saved);
    (0..6 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert!(world.can_build(wx as i32, wy as i32, 1) && !world.can_build(fx as i32, fy as i32, 1));
}

#[test]
fn a_landslide_buries_the_foot_of_the_slope() {
    use zeus::world::{LANDSLIDE, TERRAIN_ELEVATION};
    let Some(data) = data() else { return };
    let campaign = zeus::pak::Campaign::parse(&fs::read(data.join("adventures/^deux mondes s'affrontent.pak")).unwrap()).unwrap();
    let (base, ..) = world(&data);
    let mut world = World::new(campaign.parent().clone(), base.kinds.clone(), Vec::new(), WalkerImages::default(), 0);
    let (px, py) = world.map.disaster_point(1).unwrap();
    assert_ne!(world.map.terrain(px, py) & TERRAIN_ELEVATION, 0);
    let free = |w: &World| (-4..=4).flat_map(|dy| (-4..=4).map(move |dx| (px as i32 + dx, py as i32 + dy))).filter(|&(x, y)| w.can_build(x, y, 1)).count();
    let before = free(&world);
    world.strike(LANDSLIDE, 1);
    assert!(free(&world) < before, "{} puis {}", before, free(&world));
}

#[test]
fn wages_and_rival_armies_follow_events() {
    use zeus::map::{City, RIVAL};
    use zeus::world::{ARMY_AWAY, WAGE_FALL, WAGE_RISE};
    let Some(data) = data() else { return };
    let (mut world, ..) = world(&data);
    world.cities = vec![City { active: true, kind: 2, name: 40, status: RIVAL, military: 4, trade: false, favor: 50, sells: vec![], buys: vec![], sold: 0, bought: 0, army: 0, tribute_received: 0, tribute_paid: 0, wealth: 3, strength: -1 }];
    // Les salaires montent de 6 puis baissent de 3 ; l'armée rivale part trois mois.
    world.schedule(WAGE_RISE, (0, 0), (6, 6), 1, (0, 0));
    world.schedule(WAGE_FALL, (0, 0), (3, 3), 2, (0, 0));
    world.schedule(ARMY_AWAY, (0, 0), (3, 3), 1, (0, 0));
    (0..2 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert!(world.army_away(0));
    assert_eq!(world.defence(0), world.cities[0].strength / 2);
    // La sauvegarde garde l'écart de salaire et l'absence.
    let (mut other, ..) = self::world(&data);
    other.cities = world.cities.clone();
    other.load(&world.save()).unwrap();
    assert!(other.army_away(0));
    (0..TICKS_PER_MONTH).for_each(|_| world.tick());
    assert_eq!(world.wage_shift, 3);
    (0..3 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert_eq!(world.defence(0), world.cities[0].strength);
}

#[test]
fn trade_changes_move_prices_and_city_demand() {
    use zeus::map::{ALLY, City};
    let Some(data) = data() else { return };
    let (mut world, ..) = world(&data);
    world.cities = vec![City { active: true, kind: 2, name: 40, status: ALLY, military: 3, trade: true, favor: 50, sells: vec![], buys: vec![(14, 12)], sold: 0, bought: 0, army: 0, tribute_received: 0, tribute_paid: 0, wealth: 3, strength: -1 }];
    let wine = world.price(21);
    // Le vin monte de 10 drachmes ; la cité achète un niveau de laine de plus, puis
    // se met à vendre de l'huile.
    world.schedule_trade(4, 21, (0, 0), (10, 10), 1, (0, 0));
    world.schedule_trade(0, 14, (0, 0), (-1, -1), 1, (0, 0));
    world.schedule_trade(2, 20, (0, 0), (-1, -1), 1, (0, 0));
    (0..2 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert_eq!(world.price(21), wine + 10);
    assert_eq!((world.cities[0].buys.clone(), world.cities[0].sells.clone()), (vec![(14, 24)], vec![(20, 12)]));
    // La sauvegarde garde l'écart de prix.
    let (mut other, ..) = self::world(&data);
    other.cities = world.cities.clone();
    other.load(&world.save()).unwrap();
    assert_eq!(other.price(21), wine + 10);
}

#[test]
fn heavy_taxes_sour_the_city_and_stop_immigration() {
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    world.taxes = zeus::model::Taxes::parse(&fs::read(data.join("model/tax_sentiment_model.txt")).unwrap()).unwrap();
    assert_eq!(world.taxes.levels.len(), 7);
    assert_eq!(world.taxes.levels[3], (9, [2, 0, 0, 0, 0]));
    world.kinds.push(Kind::simple("percepteur".into(), 20, 2, 1075, Function::Service(Service::Tax), 0));
    let collector = world.kinds.len() - 1;
    street(&mut world, x, y);
    assert!(world.place_building(MAINTENANCE, x + 8, y + 5));
    assert!(world.place_building(collector, x, y + 5));
    // Les percepteurs dépendent du palais.
    world.kinds.push(Kind::simple("palais".into(), 20, 2, 1075, Function::Palace, 0));
    assert!(world.place_building(world.kinds.len() - 1, x + 4, y + 5));
    (0..3 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert!(world.population() > 0);
    // Taux scandaleux en très difficile : l'humeur s'effondre et plus personne n'arrive
    // (une cité qui a déjà dépassé 349 habitants ; avant, le jeu ne compte pas les
    // mécontentements). Sous 100 habitants, personne ne part non plus.
    world.grown = true;
    world.difficulty = 4;
    world.set_tax_level(6);
    world.set_tax_level(9);
    assert_eq!(world.tax_level, 6);
    (0..6 * TICKS_PER_MONTH).for_each(|_| world.tick());
    let before = world.population();
    (0..6 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert!(world.sentiment < 15, "humeur {}", world.sentiment);
    assert!(world.population() <= before, "population {} puis {}", before, world.population());
    let low = world.sentiment;
    // Plus d'impôts : en très difficile, cela ne compense pas le chômage et la faim
    // (les huttes ne mangent pas) ; en très facile, l'humeur remonte, lentement.
    world.set_tax_level(0);
    world.difficulty = 0;
    (0..24 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert!(world.sentiment > low, "humeur {} puis {}", low, world.sentiment);
    let saved = world.save();
    let (mut other, ..) = self::world(&data);
    other.taxes = world.taxes.clone();
    other.kinds = world.kinds.clone();
    other.load(&saved).unwrap();
    assert_eq!((other.tax_level, other.sentiment), (0, world.sentiment));
}

#[test]
fn guards_and_dionysus_keep_thieves_away() {
    let Some(data) = data() else { return };
    // Rien, des gardes (et une infirmerie), ou le sanctuaire de Dionysos.
    let run = |protection: u8| {
        let (mut world, x, y) = world(&data);
        street(&mut world, x, y);
        assert!(world.place_building(MAINTENANCE, x + 8, y + 5));
        assert!(world.place_building(FOUNTAIN, x + 8, y + 2));
        if protection == 2 {
            world.kinds.push(Kind::simple("sanctuaire de Dionysos".into(), 0, 3, 1460, Function::Sanctuary { god: 10, needs: [0, 0, 0] }, 0));
            assert!(world.place_building(world.kinds.len() - 1, x, y + 5));
        }
        if protection == 1 {
            world.kinds.push(Kind::simple("infirmerie".into(), 20, 2, 1075, Function::Service(Service::Health), 0));
            world.kinds.push(Kind::simple("garde".into(), 20, 2, 1075, Function::Service(Service::Security), 0));
            assert!(world.place_building(world.kinds.len() - 2, x, y + 5));
            assert!(world.place_building(world.kinds.len() - 1, x + 4, y + 5));
        }
        // La criminalité ne monte qu'une fois la cité grande.
        world.grown = true;
        let mut incidents = Vec::new();
        for _ in 0..24 * TICKS_PER_MONTH {
            world.tick();
            incidents.append(&mut world.incidents);
        }
        incidents
    };
    let theft = |i: &zeus::world::Incident| matches!(i, zeus::world::Incident::Theft { .. });
    let exposed = run(0);
    assert!(exposed.iter().any(theft), "{exposed:?}");
    // Une petite cité reste en bonne santé : pas d'épidémie, protégée ou non.
    assert!(exposed.iter().all(theft), "{exposed:?}");
    for protection in [1, 2] {
        let protected = run(protection);
        assert!(protected.is_empty(), "{protected:?}");
    }
}

#[test]
fn a_sickly_city_dwindles_at_the_turn_of_the_year() {
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    street(&mut world, x, y);
    assert!(world.place_building(MAINTENANCE, x + 8, y + 5));
    let year = 12 * TICKS_PER_MONTH;
    (0..year - 1).for_each(|_| world.tick());
    assert_eq!(world.population(), 32);
    // Santé de 5 et santé visée sous 30 : 5 % des habitants meurent dans l'année.
    (world.health, world.health_target) = (5, 5);
    world.tick();
    assert_eq!(world.population(), 31);
}

#[test]
fn a_sickly_city_catches_the_plague() {
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    street(&mut world, x, y);
    assert!(world.place_building(MAINTENANCE, x + 8, y + 5));
    assert!(world.place_building(FOUNTAIN, x + 8, y + 2));
    (0..3 * TICKS_PER_MONTH).for_each(|_| world.tick());
    // Une petite cité tend vers une santé de 80 : aucune chance d'épidémie.
    assert_eq!(world.health, 80);
    // Santé tenue au plus bas pendant deux ans : 3 chances sur 100 par jour, au plus
    // une épidémie par mois et six par an.
    let mut plagues = 0;
    for _ in 0..24 * TICKS_PER_MONTH {
        world.health = 0;
        world.tick();
        plagues += world.incidents.drain(..).filter(|i| matches!(i, zeus::world::Incident::Plague { .. })).count();
    }
    assert!((1..=12).contains(&plagues), "{plagues} épidémies");
    let saved = world.save();
    let (mut other, ..) = self::world(&data);
    other.load(&saved).unwrap();
    assert_eq!(other.save(), saved);
}

/// Un grenier au bout d'un embranchement, qui s'effondrerait en deux mois sans
/// entretien : le surintendant posté sur la rue doit aller jusqu'à lui.
#[test]
fn maintenance_reaches_the_end_of_a_side_road() {
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    street(&mut world, x, y);
    assert!(world.place_building(MAINTENANCE, x + 8, y + 2));
    let granary = world.kinds.len();
    world.kinds.push(Kind::simple("grenier".into(), 100, 4, 473, Function::Granary, 0));
    world.kinds[granary].damage_risk = 16;
    for dy in 5..9 {
        assert!(world.place_road(x + 4, y + dy, 0));
    }
    assert!(world.place_building(granary, x + 5, y + 6));
    (0..24 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert!(world.buildings.iter().flatten().any(|b| b.kind == granary), "le grenier s'est effondré");
}

#[test]
fn a_house_tells_what_it_lacks_and_a_tax_office_waits_for_a_palace() {
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    street(&mut world, x, y);
    (0..3200).for_each(|_| world.tick());
    // Une hutte habitée, sans vivres : il lui faut de la nourriture pour devenir baraque.
    let hut = world.buildings.iter().flatten().find(|b| b.house.as_ref().is_some_and(|h| h.population > 0)).unwrap();
    assert!(world.house_needs(hut).contains(&zeus::world::Need::Food), "{:?}", world.house_needs(hut));
    // Un hôtel des impôts sans palais attend le palais.
    let tax = world.kinds.len();
    world.kinds.push(Kind::simple("impôts".into(), 50, 2, 1075, Function::Service(Service::Tax), 0));
    assert!(world.place_building(tax, x + 8, y + 5));
    let office = world.buildings.iter().flatten().find(|b| b.kind == tax).unwrap();
    assert_eq!(world.service_waits_for(office), Some(Function::Palace));
}

#[test]
fn a_monster_can_wreck_buildings_whose_walkers_are_out() {
    use zeus::world::Monster;
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    // Des bâtiments dont les marcheurs sont dehors quand le monstre les abat : leur
    // chute ne doit pas déranger la marche des autres personnages.
    street(&mut world, x, y);
    assert!(world.place_building(FOUNTAIN, x + 8, y + 2));
    assert!(world.place_building(MAINTENANCE, x, y + 5));
    for i in 1..5 {
        assert!(world.place_building(if i % 2 == 0 { FOUNTAIN } else { MAINTENANCE }, x + 2 * i, y + 5));
    }
    world.monsters.push(Monster { event: 0, monster: 3, fight_until: None, slain: false, arrives_at: 0 });
    (0..10 * 12 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert!(world.buildings.iter().flatten().count() < 10);
}

#[test]
fn walls_hold_invaders_back_until_breached() {
    use zeus::map::{City, RIVAL};
    use zeus::world::INVASION;
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    // Quatre maisons dans une enceinte, au milieu du terrain libre.
    for i in 0..4 {
        assert!(world.place_building(HOUSE, x + 2 + 2 * i, y + 2));
    }
    for i in 0..=11 {
        for (wx, wy) in [(x + i, y), (x + i, y + 6), (x, y + i.min(6)), (x + 11, y + i.min(6))] {
            if !world.is_wall(wx, wy) {
                assert!(world.place_wall(wx, wy, 0), "mur refusé en {},{}", wx - x, wy - y);
            }
        }
    }
    let walls = |w: &World| (0..GRID as i32).flat_map(|ty| (0..GRID as i32).map(move |tx| (tx, ty))).filter(|&(tx, ty)| w.is_wall(tx, ty)).count();
    let built = walls(&world);
    assert!(built >= 30, "{built} murs");
    // L'intérieur est protégé, pas l'extérieur.
    assert!(world.walled_in((x + 4) as usize, (y + 3) as usize));
    assert!(!world.walled_in((x + 14) as usize, (y + 3) as usize));
    world.cities = vec![City { active: true, kind: 2, name: 68, status: RIVAL, military: 3, trade: false, favor: 50, sells: vec![], buys: vec![], sold: 0, bought: 0, army: 0, tribute_received: 0, tribute_paid: 0, wealth: 3, strength: -1 }];
    world.schedule(INVASION, (0, 0), (8, 8), 1, (0, 0));
    let standing = world.buildings.iter().flatten().count();
    // L'armée arrive et sape le mur au lieu de brûler les maisons, qui tiennent.
    let mut ticks = 0;
    while !world.figures.iter().any(|f| f.soldier() == Some(true) && world.is_wall(f.x.round() as i32, f.y.round() as i32)) {
        world.tick();
        ticks += 1;
        assert!(ticks < 6 * TICKS_PER_MONTH, "l'armée n'atteint pas le mur");
    }
    (0..TICKS_PER_MONTH / 2).for_each(|_| world.tick());
    assert_eq!(world.buildings.iter().flatten().count(), standing, "les maisons ont brûlé derrière le mur : {:?}", world.disasters);
    // Elle finit par faire une brèche.
    (0..6 * TICKS_PER_MONTH).for_each(|_| world.tick());
    assert!(walls(&world) < built, "aucune brèche");
    // Les murs passent par la sauvegarde.
    let (mut other, ..) = self::world(&data);
    other.load(&world.save()).unwrap();
    assert_eq!(walls(&other), walls(&world));
}

#[test]
fn a_staffed_tower_shoots_invaders_down() {
    use zeus::map::{City, RIVAL};
    use zeus::world::INVASION;
    let Some(data) = data() else { return };
    let raid = |with_tower: bool| {
        let (mut world, x, y) = world(&data);
        world.fighters = zeus::model::Fighters::parse(&fs::read(data.join("model/figure_model_normal.txt")).unwrap()).unwrap();
        street(&mut world, x, y);
        // Des huttes bien remplies : des ouvriers pour la tour, mais pas de soldats.
        for building in world.buildings.iter_mut().flatten() {
            if let Some(house) = building.house.as_mut() {
                house.population = 60;
            }
        }
        if with_tower {
            let tower = world.kinds.len();
            let mut kind = Kind::simple("tour".into(), 100, 2, 774, Function::Tower, 0);
            kind.employees = 15;
            world.kinds.push(kind);
            assert!(world.place_building(tower, x + 8, y + 2));
        }
        world.cities = vec![City { active: true, kind: 2, name: 68, status: RIVAL, military: 3, trade: false, favor: 50, sells: vec![], buys: vec![], sold: 0, bought: 0, army: 0, tribute_received: 0, tribute_paid: 0, wealth: 3, strength: -1 }];
        world.schedule(INVASION, (0, 0), (3, 3), 1, (0, 0));
        (0..8 * TICKS_PER_MONTH).for_each(|_| world.tick());
        world.battles.last().copied()
    };
    // Sans tour ni armée, la cité est pillée ; une tour en activité repousse l'invasion.
    assert!(raid(false).is_some_and(|b| !b.repelled));
    assert!(raid(true).is_some_and(|b| b.repelled), "{:?}", raid(true));
}

#[test]
fn citizens_cross_walls_only_through_a_gatehouse() {
    let Some(data) = data() else { return };
    let settle = |with_gate: bool| {
        let (mut world, x, y) = world(&data);
        // Quatre maisons dans une enceinte ; une route sort par le côté est, où se pose le
        // corps de garde, ou bien le mur reste continu.
        for i in 0..4 {
            assert!(world.place_building(HOUSE, x + 2 + 2 * i, y + 2));
        }
        for i in 1..=11 {
            assert!(world.place_road(x + i, y + 4, 0));
        }
        let gate = world.kinds.len();
        world.kinds.push(Kind::simple("corps de garde".into(), 80, 2, 768, Function::Gatehouse, 0));
        if with_gate {
            assert!(world.place_building(gate, x + 10, y + 3));
        } else {
            world.demolish(x + 11, y + 4);
        }
        for i in 0..=11 {
            for (wx, wy) in [(x + i, y), (x + i, y + 6), (x, y + i.min(6)), (x + 11, y + i.min(6))] {
                if !world.is_wall(wx, wy) && !world.is_road(wx, wy) {
                    assert!(world.place_wall(wx, wy, 0), "mur refusé en {},{}", wx - x, wy - y);
                }
            }
        }
        (0..4 * TICKS_PER_MONTH).for_each(|_| world.tick());
        world.population()
    };
    // Sans corps de garde, personne n'entre ; avec, les maisons se remplissent.
    let (closed, open) = (settle(false), settle(true));
    assert!(open > closed, "fermée {closed}, ouverte {open}");
}

#[test]
fn a_roadblock_turns_back_the_rounds() {
    let Some(data) = data() else { return };
    // Une maintenance au début d'une rue, deux maisons au bout ; le barrage, entre les
    // deux, arrête le préposé : les maisons ne sont plus visitées.
    let fire_risk = |blocked: bool| {
        let (mut world, x, y) = world(&data);
        for i in 0..12 {
            assert!(world.place_road(x + i, y + 4, 0));
        }
        assert!(world.place_building(MAINTENANCE, x, y + 5));
        for i in 0..2 {
            assert!(world.place_building(HOUSE, x + 8 + 2 * i, y + 2));
        }
        let roadblock = world.kinds.len();
        world.kinds.push(Kind::simple("barrage routier".into(), 4, 1, 1150, Function::Roadblock, 0));
        if blocked {
            assert!(world.place_building(roadblock, x + 4, y + 4));
            assert!(!world.place_building(roadblock, x + 4, y + 6), "un barrage hors de la route");
        }
        (0..TICKS_PER_MONTH * 6).for_each(|_| world.tick());
        let fire = world.buildings.iter().flatten().filter(|b| b.house.is_some()).map(|b| b.fire).max().unwrap_or(0);
        // Démoli, le barrage rend sa route.
        if blocked {
            assert!(world.demolish(x + 4, y + 4));
            assert!(world.is_road(x + 4, y + 4));
        }
        fire
    };
    let (open, blocked) = (fire_risk(false), fire_risk(true));
    assert!(blocked > open, "ouverte {open}, barrée {blocked}");
}

#[test]
fn a_dragged_road_goes_around_obstacles() {
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    // Une maison au milieu du chemin direct : le tracé la contourne, avec peu de virages.
    assert!(world.place_building(HOUSE, x + 4, y + 2));
    let line = world.plan_line((x, y + 2), (x + 10, y + 2), false);
    assert!(!line.is_empty());
    assert_eq!((line[0], *line.last().unwrap()), ((x, y + 2), (x + 10, y + 2)));
    assert!(line.windows(2).all(|w| (w[0].0 - w[1].0).abs() + (w[0].1 - w[1].1).abs() == 1));
    assert!(!line.iter().any(|&(lx, ly)| (x + 4..x + 6).contains(&lx) && (y + 2..y + 4).contains(&ly)));
    // En terrain libre, un seul virage.
    let line = world.plan_line((x, y + 6), (x + 8, y + 10), false);
    let turns = line.windows(3).filter(|w| (w[1].0 - w[0].0, w[1].1 - w[0].1) != (w[2].0 - w[1].0, w[2].1 - w[1].1)).count();
    assert_eq!((line.len(), turns), (13, 1));
}

#[test]
fn a_commemorative_monument_is_built_once_earned() {
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    let first = world.kinds.len();
    for model in 0..zeus::world::COMMEMORATIVES as u8 {
        world.kinds.push(Kind::simple("monument commémoratif".into(), 0, 3, 225 + model as usize, Function::Commemorative(model), 0));
    }
    let victory = first + zeus::world::COMMEMORATIVE_VICTORY as usize;
    assert!(!world.place_building(victory, x, y), "un monument qui n'a pas été gagné");
    // Une victoire gagnée, gardée par la sauvegarde.
    let saved = world.save().replace("commemoratifs 0 \n", "commemoratifs 0 1\n");
    world.load(&saved).unwrap();
    assert_eq!(world.commemoratives, vec![zeus::world::COMMEMORATIVE_VICTORY]);
    assert!(!world.place_building(first, x, y), "un autre modèle");
    assert!(world.place_building(victory, x, y));
    assert!(world.commemoratives.is_empty());
    assert!(!world.place_building(victory, x + 4, y), "un seul par victoire");
}

#[test]
fn the_isthmian_games_need_philosophers() {
    use zeus::world::{GAMES_ISTHMIAN, GamesNews};
    let Some(data) = data() else { return };
    let games = |podium: bool| {
        let (mut world, x, y) = world(&data);
        // L'an 1 : année des jeux Isthmiques ((1 + 0) & 3 = 1).
        (world.games_from, world.start_year) = (Some(0), 1);
        if podium {
            world.kinds.push(Kind::simple("podium".into(), 0, 2, 1055, Function::Service(Service::Philosophy), 0));
            assert!(world.place_building(world.kinds.len() - 1, x, y));
        }
        let mut news = Vec::new();
        for month in 0..8 {
            (0..TICKS_PER_MONTH).for_each(|_| world.tick());
            news.extend(world.games_news.drain(..));
            // Les jeux en cours survivent à une sauvegarde.
            if podium && month == 5 {
                let saved = world.save();
                assert!(saved.contains("\njeux 0 1 0 "), "{}", saved.lines().find(|l| l.starts_with("jeux")).unwrap_or(""));
                world.load(&saved).unwrap();
            }
        }
        news
    };
    assert_eq!(games(false), vec![GamesNews::CantParticipate(GAMES_ISTHMIAN)]);
    let news = games(true);
    assert_eq!(news[0], GamesNews::Begin { kind: GAMES_ISTHMIAN, home: false });
    assert!(matches!(news[1], GamesNews::Concluded { kind: GAMES_ISTHMIAN, home: false, .. }), "{news:?}");
}

#[test]
fn an_atlantean_city_raises_atlantean_soldiers() {
    use zeus::map::{City, RIVAL, Source};
    use zeus::world::INVASION;
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    street(&mut world, x, y);
    (world.atlantean, world.walkers.atlantean) = (true, [11, 22, 33]);
    world.cities = vec![City { active: true, kind: 2, name: 68, status: RIVAL, military: 3, trade: false, favor: 50, sells: vec![], buys: vec![], sold: 0, bought: 0, army: 0, tribute_received: 0, tribute_paid: 0, wealth: 3, strength: -1 }];
    for building in world.buildings.iter_mut().flatten() {
        if let Some(house) = building.house.as_mut() {
            (house.level, house.population) = (6, 1000);
        }
    }
    world.schedule(INVASION, (0, 0), (5, 5), 1, (0, 0));
    for _ in 0..2 * TICKS_PER_MONTH {
        world.tick();
        if world.raid.is_some() {
            break;
        }
    }
    // Piétaille des maisons de ville : des archers atlantes.
    let defenders: Vec<_> = world.figures.iter().filter(|f| f.soldier() == Some(false)).collect();
    assert!(!defenders.is_empty());
    assert!(defenders.iter().all(|f| f.source == Source::Imps && (11..11 + 104 + 8 * 15).contains(&f.image())), "archers attendus");
}

#[test]
fn a_chariot_factory_turns_wood_and_horses_into_chariots() {
    let Some(data) = data() else { return };
    let (mut world, x, y) = world(&data);
    for i in 0..10 {
        assert!(world.place_road(x + i, y + 4, 3));
    }
    let (storehouse, factory, ranch) = (world.kinds.len(), world.kinds.len() + 1, world.kinds.len() + 2);
    world.kinds.push(Kind::simple("entrepôt".into(), 70, 3, 934, Function::Storehouse, 0));
    world.kinds.push(Kind::simple("fabrique de chars".into(), 225, 2, 679, Function::Workshop(Good::Chariot), 0));
    world.kinds.push(Kind::simple("haras".into(), 200, 2, 723, Function::Producer(Good::Horses), 0));
    world.kinds[factory].product = 22;
    assert!(world.place_building(storehouse, x, y + 5));
    assert!(world.place_building(factory, x + 4, y + 5));
    assert!(world.place_building(ranch, x + 7, y + 5));
    world.buildings.iter_mut().flatten().find(|b| b.kind == storehouse).unwrap().goods[Good::Wood as usize] = 4;
    // Les porteurs apportent le bois de l'entrepôt et les chevaux du haras ; deux mois
    // d'assemblage par char, qui rejoint l'entrepôt.
    (0..6 * TICKS_PER_MONTH).for_each(|_| world.tick());
    let state: Vec<_> = world.buildings.iter().flatten().map(|b| (b.kind, b.stock, b.goods)).collect();
    assert!(world.produced[22] >= 1, "{state:?}");
    assert!(world.stored(22) >= 1);
    // Une cité atlante exige des chars pour ses logements luxueux ; la sauvegarde les
    // garde.
    let saved = world.save();
    let (mut other, ..) = self::world(&data);
    other.kinds = world.kinds.clone();
    other.load(&saved).unwrap();
    assert_eq!(other.stored(22), world.stored(22));
}
