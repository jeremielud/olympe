//! Vérifications sur les données du jeu, quand le dossier `donnees/` est présent
//! (il n'est pas distribué avec le code : sans lui, ces essais ne font rien).

use std::fs;
use std::path::{Path, PathBuf};
use zeus::goals::{Goal, Kind};
use zeus::map::Map;
use zeus::messages::Messages;
use zeus::model::Model;
use zeus::pak::{Campaign, Settings};
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
        let settings = campaign.settings.unwrap_or_else(|| panic!("{} : réglages absents", file.display()));
        assert!((100..=100_000).contains(&settings.funds), "{} : trésor {}", file.display(), settings.funds);
        assert!((-4000..=0).contains(&settings.start_year), "{} : année {}", file.display(), settings.start_year);
        // La table des objectifs se lit pour chaque aventure, cité mère et colonies.
        assert_eq!(settings.goals.parent.len(), 10, "{}", file.display());
        assert_eq!(settings.goals.colonies.len(), 4, "{}", file.display());
    }
    for file in files(&adventures, "set") {
        let settings = Settings::parse(&fs::read(&file).unwrap()).unwrap();
        assert_eq!(settings.goals.parent.len(), 10, "{}", file.display());
    }
}

#[test]
fn episode_goals_and_resources() {
    let Some(data) = data() else { return };
    let settings = |name: &str| Campaign::parse(&fs::read(data.join("adventures").join(name)).unwrap()).unwrap().settings.unwrap();
    let goal = |kind, values| Goal { kind, values };
    let jason = settings("[les aventures de jason.pak");
    // Premier épisode : la quête, le monstre, 3000 drachmes.
    assert_eq!(jason.goals.parent[0], [goal(Kind::Quest, [0, 0, 0]), goal(Kind::Slay, [12, 0, 0]), goal(Kind::Treasury, [3000, 0, 0])]);
    assert_eq!(jason.goals.parent[3][4], goal(Kind::Housing, [11, 72, 0]));
    // Mise de côté dans une aventure de Zeus : la laine, ancien numéro 13, devient 14 ;
    // de 4881 (0x1311), le jeu ne lit que l'octet bas, 17, l'huile d'olive, devenue 20.
    assert_eq!(jason.goals.parent[2][3], goal(Kind::SetAside, [14, 24, 0]));
    assert_eq!(jason.goals.parent[2][2], goal(Kind::SetAside, [20, 24, 0]));
    // Ressources renumérotées comme le fait le jeu : bois et laine, pas oranges et olives.
    assert_eq!(jason.resources, [2, 4, 5, 7, 9, 14]);
    assert_eq!(settings("firstcity.pak").goals.parent[0].len(), 1);
    assert!(settings("}jeu ouvert - bac à sable.pak").goals.parent[0].is_empty());
    // Poséidon (version 26) : les marchandises gardent leur numéro.
    let atlantis = settings("#la naissance de l'atlantide.pak");
    assert_eq!(atlantis.goals.parent[1][3], goal(Kind::Pyramid, [1, 100, 0]));
    assert_eq!(atlantis.goals.parent[6][3], goal(Kind::Hippodrome, [40, 0, 0]));
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
