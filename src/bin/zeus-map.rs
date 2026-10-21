//! Dessine le terrain d'une carte `.map` dans un fichier PNG.

use std::fs;
use std::path::Path;
use zeus::map::Map;
use zeus::{png, render};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("usage : zeus-map <dossier donnees> <carte.map> <sortie.png>");
        std::process::exit(2);
    }
    if let Err(e) = run(Path::new(&args[1]), Path::new(&args[2]), Path::new(&args[3])) {
        eprintln!("erreur : {e}");
        std::process::exit(1);
    }
}

fn run(data: &Path, map: &Path, out: &Path) -> Result<(), String> {
    let map = Map::parse(&fs::read(map).map_err(|e| e.to_string())?)?;
    let picture = render::terrain(&map, data)?;
    fs::write(out, png::encode_rgba(picture.width, picture.height, &picture.rgba)).map_err(|e| e.to_string())?;
    println!(
        "carte version {} : {} cases dessinées, {} manquantes, {}×{} → {}",
        map.version,
        picture.drawn,
        picture.missing,
        picture.width,
        picture.height,
        out.display()
    );
    Ok(())
}
