//! Liste le contenu d'une image de CD-ROM, ou en extrait un fichier.

use std::path::Path;
use zeus::iso::Image;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage : zeus-iso <image> [chemin dans l'image] [fichier de sortie]");
        std::process::exit(2);
    }
    if let Err(e) = run(&args) {
        eprintln!("erreur : {e}");
        std::process::exit(1);
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let mut image = Image::open(Path::new(&args[1]))?;
    let entries = image.entries()?;
    match (args.get(2), args.get(3)) {
        (Some(path), Some(output)) => {
            let entry = entries.iter().find(|e| e.path.eq_ignore_ascii_case(path)).ok_or("fichier absent de l'image")?;
            std::fs::write(output, image.read(entry)?).map_err(|e| e.to_string())
        }
        _ => {
            for entry in &entries {
                println!("{:>11} {}{}", entry.size, entry.path, if entry.directory { "/" } else { "" });
            }
            Ok(())
        }
    }
}
