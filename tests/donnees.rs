//! Vérifications sur les données du jeu, quand le dossier `donnees/` est présent
//! (il n'est pas distribué avec le code : sans lui, ces essais ne font rien).

use std::fs;
use std::path::{Path, PathBuf};
use zeus::goals::{Goal, Kind};
use zeus::map::Map;
use zeus::messages::Messages;
use zeus::model::Model;
use zeus::pak::{Campaign, Settings, Step};
use zeus::sg::Catalog;
use zeus::text::Text;

fn data() -> Option<PathBuf> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("donnees");
    folder.join("zeus_text.eng").exists().then_some(folder)
}

/// Fichiers d'un dossier (et de ses sous-dossiers directs) portant cette extension.
fn files(folder: &Path, extension: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(folder).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(fs::read_dir(&path).into_iter().flatten().flatten().map(|e| e.path()));
        } else {
            found.push(path);
        }
    }
    found.retain(|p| p.extension().is_some_and(|e| e == extension));
    found.sort();
    found
}

#[test]
fn every_adventure_parses() {
    let Some(data) = data() else { return };
    let adventures = data.join("adventures");
    let maps = files(&adventures, "map");
    assert!(!maps.is_empty());
    for file in maps {
        let map = Map::parse(&fs::read(&file).unwrap()).unwrap_or_else(|e| panic!("{} : {e}", file.display()));
        // Toute image de la grille doit se résoudre dans un catalogue.
        let unresolved = (0..zeus::map::GRID * zeus::map::GRID)
            .map(|i| map.image(i % zeus::map::GRID, i / zeus::map::GRID))
            .filter(|&id| id != 0 && map.resolve(id).is_none())
            .count();
        assert_eq!(unresolved, 0, "{}", file.display());
    }
    for file in files(&adventures, "pak") {
        let campaign = Campaign::parse(&fs::read(&file).unwrap()).unwrap_or_else(|e| panic!("{} : {e}", file.display()));
        let settings = campaign.settings.clone().unwrap_or_else(|| panic!("{} : réglages absents", file.display()));
        assert!((100..=100_000).contains(&settings.funds), "{} : trésor {}", file.display(), settings.funds);
        assert!((-4000..=0).contains(&settings.start_year), "{} : année {}", file.display(), settings.start_year);
        // Chaque aventure a des épisodes, et une carte pour chaque épisode de colonie
        // que l'on joue.
        assert!(!settings.parent.is_empty(), "{}", file.display());
        let colonies = settings.order().iter().filter(|step| matches!(step, Step::Colony(_))).count();
        assert!(colonies <= campaign.colonies().len(), "{} : {colonies} épisodes de colonie", file.display());
    }
    for file in files(&adventures, "set") {
        let settings = Settings::parse(&fs::read(&file).unwrap()).unwrap();
        assert!(!settings.parent.is_empty(), "{}", file.display());
    }
}

#[test]
fn episode_goals_and_resources() {
    let Some(data) = data() else { return };
    let settings = |name: &str| Campaign::parse(&fs::read(data.join("adventures").join(name)).unwrap()).unwrap().settings.unwrap();
    let goal = |kind, values| Goal { kind, values };
    let jason = settings("[les aventures de jason.pak");
    // Premier épisode : la quête, le monstre, 3000 drachmes.
    assert_eq!(jason.parent[0].goals, [goal(Kind::Quest, [0, 0, 0]), goal(Kind::Slay, [12, 0, 0]), goal(Kind::Treasury, [3000, 0, 0])]);
    assert_eq!(jason.parent[3].goals[4], goal(Kind::Housing, [11, 72, 0]));
    // Mise de côté dans une aventure de Zeus : la laine, ancien numéro 13, devient 14 ;
    // de 4881 (0x1311), le jeu ne lit que l'octet bas, 17, l'huile d'olive, devenue 20.
    assert_eq!(jason.parent[2].goals[3], goal(Kind::SetAside, [14, 24, 0]));
    assert_eq!(jason.parent[2].goals[2], goal(Kind::SetAside, [20, 24, 0]));
    // Ressources renumérotées comme le fait le jeu : bois et laine, pas oranges et olives.
    assert_eq!(jason.parent[0].resources, [2, 4, 5, 7, 9, 14]);
    assert_eq!(settings("firstcity.pak").parent[0].goals.len(), 1);
    assert!(settings("}jeu ouvert - bac à sable.pak").parent[0].goals.is_empty());
    // Poséidon (version 26) : les marchandises gardent leur numéro.
    let atlantis = settings("#la naissance de l'atlantide.pak");
    assert_eq!(atlantis.parent[1].goals[3], goal(Kind::Pyramid, [1, 100, 0]));
    assert_eq!(atlantis.parent[6].goals[3], goal(Kind::Hippodrome, [40, 0, 0]));
}

#[test]
fn episodes_follow_each_other() {
    let Some(data) = data() else { return };
    let order = |name: &str| Campaign::parse(&fs::read(data.join("adventures").join(name)).unwrap()).unwrap().settings.unwrap().order();
    use Step::{Colony, Parent};
    // Un épisode marqué « colonie » mène au prochain épisode de colonie, après lequel
    // on revient à la cité mère ; le dernier épisode clôt l'aventure.
    assert_eq!(order("{la guerre de troie.pak"), [Parent(0), Parent(1), Colony(0), Parent(2), Colony(1), Parent(3)]);
    assert_eq!(order("@l'histoire d'athènes.pak"), [Parent(0), Parent(1), Colony(0), Parent(2), Colony(1), Parent(3), Parent(4), Parent(5)]);
    assert_eq!(order("#la naissance de l'atlantide.pak").len(), 9);
    // Jason n'annonce qu'une colonie : son second épisode de colonie reste hors jeu.
    assert_eq!(order("[les aventures de jason.pak"), [Parent(0), Parent(1), Parent(2), Colony(0), Parent(3)]);
    assert_eq!(order("firstcity.pak"), [Parent(0)]);
    // Récits : ouverture et victoire de chaque épisode.
    let troy = Campaign::parse(&fs::read(data.join("adventures/{la guerre de troie.pak")).unwrap()).unwrap();
    let settings = troy.settings.as_ref().unwrap();
    assert_eq!((settings.colonies[1].intro_message, settings.colonies[1].victory_message), (385, 399));
    assert_eq!(troy.colonies().len(), 2);
}

#[test]
fn rules_texts_and_messages_load() {
    let Some(data) = data() else { return };
    let text = Text::parse(&fs::read(data.join("zeus_text.eng")).unwrap()).unwrap();
    assert_eq!(text.get(28, 28), "Route");
    let model = Model::parse(&fs::read(data.join("model/zeus_model_normal.txt")).unwrap()).unwrap();
    assert_eq!(model.buildings[74].name, "BUILD_FOUNTAIN");
    // Sept niveaux modestes puis six niveaux luxueux ; l'appartement loge 48 habitants.
    assert_eq!(model.houses.len(), 13);
    assert_eq!(model.houses[5].capacity, 48);
    let messages = Messages::parse(fs::read(data.join("zeus_mm.eng")).unwrap()).unwrap();
    assert_eq!(messages.get(318).unwrap().title, "Sauver un héros");
}

#[test]
fn sprites_decode() {
    let Some(data) = data() else { return };
    for name in ["zeus_general", "zeus_terrain", "sprmain", "zeus_fonts", "destruction", "poseidon_loaded"] {
        let catalog = Catalog::load(&data.join("data").join(format!("{name}.sg3"))).unwrap();
        let failures = (1..catalog.images.len()).filter(|&id| !catalog.is_empty(id) && catalog.decode(id).is_err()).count();
        assert_eq!(failures, 0, "{name}");
    }
}

#[test]
fn backdrops_and_music_decode() {
    let Some(data) = data() else { return };
    let picture = zeus::jpeg::decode(&fs::read(data.join("data/zeus_fe_choosegame.jpg")).unwrap()).unwrap();
    assert_eq!((picture.width, picture.height), (1024, 768));
    let mut stream = zeus::opus::Stream::open(fs::read(data.join("audio/music/proi.opus")).unwrap()).unwrap();
    let first = stream.decode().unwrap();
    assert_eq!(stream.channels, 2);
    assert!(!first.is_empty());
}

#[test]
fn sanctuary_goals_name_allowed_gods() {
    let Some(data) = data() else { return };
    let adventures = data.join("adventures");
    let mut checked = 0;
    for file in files(&adventures, "pak") {
        let Some(settings) = Campaign::parse(&fs::read(&file).unwrap()).unwrap().settings else { continue };
        for episode in settings.parent.iter().chain(&settings.colonies) {
            for goal in episode.goals.iter().filter(|g| g.kind == Kind::Sanctuary) {
                let [god, count, _] = goal.values;
                if god >= 0 {
                    assert!(episode.sanctuaries.contains(&(god as u8)), "{} : dieu {god} absent de {:?}", file.display(), episode.sanctuaries);
                    checked += 1;
                } else {
                    assert!((1..=4).contains(&count), "{} : {count} sanctuaires", file.display());
                }
            }
        }
    }
    assert!(checked > 10, "{checked}");
    // L'Odyssée : Hermès d'abord, puis Athéna, Hermès et Zeus, trois à la fois.
    let odyssey = Settings::parse(&fs::read(adventures.join("l'odyssée/l'odyssée.set")).unwrap()).unwrap();
    assert_eq!((odyssey.parent[1].sanctuaries.clone(), odyssey.parent[1].max_sanctuaries), (vec![7], 1));
    assert_eq!((odyssey.parent[3].sanctuaries.clone(), odyssey.parent[3].max_sanctuaries), (vec![8, 7, 0], 3));
}

#[test]
fn quest_and_monster_goals_name_their_events() {
    use zeus::pak::Event;
    let Some(data) = data() else { return };
    let mut checked = 0;
    for file in files(&data.join("adventures"), "pak") {
        let Some(settings) = Campaign::parse(&fs::read(&file).unwrap()).unwrap().settings else { continue };
        for episode in settings.parent.iter().chain(&settings.colonies) {
            // Les venues de dieux sont celles de dieux qui existent.
            for event in &episode.events {
                if let Event::GodInvasion { god, .. } = event {
                    assert!((*god as usize) < zeus::sanctuary::GODS, "{} : dieu {god}", file.display());
                }
            }
            for goal in &episode.goals {
                let event = episode.events.get(goal.values[0] as usize).copied();
                match goal.kind {
                    Kind::Quest => assert!(matches!(event, Some(Event::Quest { .. })), "{} : {event:?}", file.display()),
                    Kind::Slay => assert!(matches!(event, Some(Event::Monster { .. })), "{} : {event:?}", file.display()),
                    _ => continue,
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 40, "{checked}");
    // Troie : Ulysse doit rapporter le Palladium de Poséidon.
    let troy = Campaign::parse(&fs::read(data.join("adventures/{la guerre de troie.pak")).unwrap()).unwrap().settings.unwrap();
    assert_eq!(troy.parent[2].events[0], Event::Quest { god: 1, quest: 0, hero: 3 });
    // Jason, d'après les récits : Hercule sauve Thésée et affronte Cerbère, puis
    // Thésée sert Dionysos contre le sanglier d'Artémis, puis Jason rapporte la
    // Toison d'or.
    let jason = Campaign::parse(&fs::read(data.join("adventures/[les aventures de jason.pak")).unwrap()).unwrap().settings.unwrap();
    assert_eq!((jason.parent[0].events[0], jason.parent[0].events[12]), (Event::Quest { god: 3, quest: 0, hero: 1 }, Event::Monster { monster: 2, arrives: None }));
    assert_eq!((jason.parent[1].events[0], jason.parent[1].events[1]), (Event::Quest { god: 10, quest: 0, hero: 5 }, Event::Monster { monster: 6, arrives: None }));
    assert_eq!(jason.parent[2].events[0], Event::Quest { god: 0, quest: 0, hero: 2 });
    // Troie devient rivale entre la première et la deuxième année de la guerre.
    assert_eq!(troy.parent[0].events[20], Event::World { change: 10, cities: (6, 6), amount: (5, 10), month: 0, years: (1, 2) });
    // Après le vol du Palladium, l'armée de Troie s'affaiblit (15) ; Ithaque, qu'Ulysse
    // a quittée, se retire du monde (19).
    assert!(matches!(troy.parent[3].events[0], Event::World { change: 15, cities: (6, 6), .. }));
    assert!(matches!(troy.parent[2].events[1], Event::World { change: 19, cities: (13, 13), .. }));
    // Péloponnèse : l'armée de cités rivales part en campagne ; Jason : les salaires
    // montent de 3 à 6 drachmes.
    let all = |s: &zeus::pak::Settings| s.parent.iter().chain(&s.colonies).flat_map(|e| e.events.clone()).collect::<Vec<_>>();
    let war = Campaign::parse(&fs::read(data.join("adventures/^guerre du péloponnèse.pak")).unwrap()).unwrap().settings.unwrap();
    assert_eq!(all(&war).iter().filter(|e| matches!(e, Event::ArmyAway { months: (5, 10), .. })).count(), 4);
    assert!(all(&war).contains(&Event::ArmyAway { cities: (8, 8), months: (5, 10), month: 3, years: (4, 4) }));
    assert!(all(&jason).iter().any(|e| matches!(e, Event::Wages { rise: true, amount: (3, 6), .. })));
    // Persée : un tremblement de terre au point 2 et des raz-de-marée aux points 3 à 5.
    let perseus = Campaign::parse(&fs::read(data.join("adventures/&persée et méduse.pak")).unwrap()).unwrap().settings.unwrap();
    let disasters: Vec<(u8, (i16, i16))> = all(&perseus).into_iter().filter_map(|e| match e { Event::Disaster { kind, points, .. } => Some((kind, points)), _ => None }).collect();
    assert_eq!(disasters, [(3, (1, 2)), (25, (3, 5))]);
    // Caractéristiques de combat : hoplites grecs, archères amazones, soldats de la cité.
    let fighters = zeus::model::Fighters::parse(&fs::read(data.join("model/figure_model_normal.txt")).unwrap()).unwrap();
    assert_eq!((fighters.enemies[0].health, fighters.enemies[0].attack, fighters.enemies[0].frequency), (150, 15, 60));
    assert_eq!((fighters.enemies[25].missile_attack, fighters.enemies[25].frequency), (8, 75));
    assert_eq!(fighters.citizens.map(|f| f.health), [80, 140, 240]);
    // Prix de l'éditeur : ancienne numérotation chez Zeus, nouvelle chez Poséidon.
    let troy_prices = Campaign::parse(&fs::read(data.join("adventures/{la guerre de troie.pak")).unwrap()).unwrap().parent().prices();
    let atlantis_prices = Campaign::parse(&fs::read(data.join("adventures/#la naissance de l'atlantide.pak")).unwrap()).unwrap().parent().prices();
    for prices in [&troy_prices, &atlantis_prices] {
        assert!(prices.contains(&(7, 30)) && prices.contains(&(14, 60)) && prices.contains(&(21, 160)) && prices.contains(&(19, 640)));
    }
    assert!(atlantis_prices.contains(&(17, 140)) && !troy_prices.iter().any(|&(r, _)| r == 17));
    // Nation des armées : Troie envoie des Troyens, Sardes des Perses.
    let troy_cities = Campaign::parse(&fs::read(data.join("adventures/{la guerre de troie.pak")).unwrap()).unwrap().parent().cities();
    let war_cities = Campaign::parse(&fs::read(data.join("adventures/^guerre du péloponnèse.pak")).unwrap()).unwrap().parent().cities();
    assert_eq!((troy_cities[6].army, troy_cities[3].army, war_cities[7].army), (1, 0, 2));
    // Tributs : Ténédos, colonie, paie 1000 ; Mycènes recevrait 400 et paierait 500.
    assert_eq!((troy_cities[1].tribute_paid, troy_cities[3].tribute_received, troy_cities[3].tribute_paid), (1000, 400, 500));
    // La colonie de Ténédos reçoit de la cité mère ce qu'elle a mis de côté.
    assert!(matches!(troy.colonies[0].events[1], Event::Gift { resource: 14, cities: (0, 0), amount: (-1, -1), .. }));
    // Une cité demande entre 1000 et 2000 drachmes (19 dans la numérotation de Zeus).
    assert!(matches!(troy.parent[2].events[4], Event::Request { resource: 23, amount: (1000, 2000), .. }));
}

#[test]
fn rule_goals_name_foreign_cities() {
    let Some(data) = data() else { return };
    let mut checked = 0;
    for file in files(&data.join("adventures"), "pak") {
        let campaign = Campaign::parse(&fs::read(&file).unwrap()).unwrap();
        let Some(settings) = &campaign.settings else { continue };
        let cities = campaign.parent().cities();
        for goal in settings.parent.iter().chain(&settings.colonies).flat_map(|e| &e.goals).filter(|g| g.kind == Kind::Rule) {
            let city = cities.get(goal.values[0] as usize).unwrap_or_else(|| panic!("{} : cité {}", file.display(), goal.values[0]));
            assert!(city.active && city.kind == zeus::map::FOREIGN, "{} : {city:?}", file.display());
            checked += 1;
        }
    }
    assert!(checked > 50, "{checked}");
    // Troie : la cité du joueur est Aulis ; Troie elle-même est encore alliée.
    let troy = Campaign::parse(&fs::read(data.join("adventures/{la guerre de troie.pak")).unwrap()).unwrap();
    let cities = troy.parent().cities();
    assert_eq!((cities[0].name, cities[6].name, cities[6].status, cities[6].military), (5, 68, zeus::map::ALLY, 6));
    // Commerce, renuméroté comme les ressources : Ténédos vend blé, raisin, olives et
    // laine ; Mycènes vend de la laine et achète raisin et vin.
    assert_eq!(cities[1].sells, [(7, 36), (12, 24), (13, 24), (14, 24)]);
    assert_eq!((cities[3].sells.clone(), cities[3].buys.clone()), (vec![(14, 12)], vec![(12, 12), (21, 12)]));
}

/// Points de poissons de l'éditeur : ceux de l'Odyssée sont tous dans l'eau.
#[test]
fn fish_points_lie_in_the_water() {
    let Some(data) = data() else { return };
    let map = Map::parse(&fs::read(data.join("adventures/l'odyssée/l'odysséep.map")).unwrap()).unwrap();
    let fish = map.fish_points();
    assert_eq!(fish.len(), 8);
    assert_eq!(fish[0], (99, 69));
    assert!(fish.iter().all(|&(x, y)| map.terrain(x, y) & 0x4 != 0), "{fish:?}");
}

#[test]
fn episodes_list_their_allowed_buildings() {
    let Some(data) = data() else { return };
    let allowed = |pak: &str| Settings::parse(&fs::read(data.join("adventures").join(pak)).unwrap()).unwrap().parent[0].allowed.clone();
    // Rangs de la liste de l'éditeur : 12 théâtre, 16 podium, 18 fontaine, 21 maintenance.
    let first = allowed("firstcity.pak");
    assert!(first[18] && first[21] && first[16], "le premier tutoriel permet fontaine, maintenance et podium");
    assert!(!first[12], "mais pas le théâtre");
    assert!(allowed("}jeu ouvert - bac à sable.pak")[1..58].iter().all(|&a| a));
}

#[test]
fn atlantean_adventures_are_flagged() {
    let Some(data) = data() else { return };
    let atlantean = |pak: &str| Settings::parse(&fs::read(data.join("adventures").join(pak)).unwrap()).unwrap().parent.iter().all(|e| e.atlantean);
    for pak in ["#la naissance de l'atlantide.pak", "[la renaissance de l'atlantide.pak", "&la conquête de l'ouest.pak", "@la vie en méditerranée.pak"] {
        assert!(atlantean(pak), "{pak}");
    }
    for pak in ["]proétos et bellérophon.pak", "^deux mondes s'affrontent.pak", "#zeus et europe.pak", "firstcity.pak"] {
        assert!(!atlantean(pak), "{pak}");
    }
}

#[test]
fn some_episodes_hold_panhellenic_games() {
    let Some(data) = data() else { return };
    let mut with_games = Vec::new();
    for file in files(&data.join("adventures"), "pak") {
        let settings = Settings::parse(&fs::read(&file).unwrap()).unwrap();
        for (rank, episode) in settings.parent.iter().enumerate() {
            if let Some(years) = episode.games {
                // Dans les aventures livrées, le délai vaut toujours 0 : jeux dès le départ.
                assert_eq!(years, 0, "{} épisode {rank}", file.display());
                with_games.push((file.clone(), rank));
            }
        }
    }
    assert_eq!(with_games.len(), 47);
}

#[test]
fn terrain_tiling_matches_the_maps() {
    let Some(data) = data() else { return };
    let groups = |file: &str| zeus::sg::Catalog::load(&data.join("data").join(file)).unwrap().groups;
    let (terrain, poseidon) = (groups("zeus_terrain.sg3"), groups("poseidon_loaded.sg3"));
    // Cartes isolées : comparaison stricte. Celles des aventures ont parfois une
    // grille d'aléa qui ne correspond plus aux variantes stockées, et les cartes de Zeus
    // 1.0 d'autres variantes de couleur : on ne fait qu'en rendre compte.
    let mut strict: Vec<Map> = files(&data.join("adventures"), "map").iter().map(|f| Map::parse(&fs::read(f).unwrap()).unwrap()).collect();
    let mut others = Vec::new();
    for file in files(&data.join("adventures"), "pak") {
        let campaign = Campaign::parse(&fs::read(&file).unwrap()).unwrap();
        others.push(campaign.parent().clone());
        others.extend(campaign.colonies().iter().cloned());
    }
    let count = |maps: &mut Vec<Map>| {
        let (mut same, mut total) = (0, 0);
        for map in maps.drain(..) {
            let tiling = zeus::tiling::tile(&map, &terrain, &poseidon, 0);
            for i in 0..zeus::map::GRID * zeus::map::GRID {
                let (x, y) = (i % zeus::map::GRID, i / zeus::map::GRID);
                // Terrain nu, hors routes et zones de catastrophe peintes par l'éditeur.
                if map.terrain(x, y) & (0x8c008 | 0x40 | 0x2000010) != 0 || tiling.images[i] == 0 {
                    continue;
                }
                let stored = map.image(x, y);
                total += 1;
                same += (stored == tiling.images[i]) as usize;
            }
        }
        (same, total)
    };
    let (same, total) = count(&mut strict);
    let (all_same, all_total) = count(&mut others);
    eprintln!("pavage : {same}/{total} cases justes (cartes isolées), {all_same}/{all_total} (aventures)");
    assert!(total > 250_000, "{total} cases");
    assert!(same * 1000 >= total * 998, "{same} justes sur {total}");
}
