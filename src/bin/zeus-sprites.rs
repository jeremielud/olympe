//! Exporte les images d'un catalogue `.sg3` en PNG, rangées par planche.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use zeus::png;
use zeus::sg::Catalog;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage : zeus-sprites <catalogue.sg3> <dossier de sortie>");
        std::process::exit(2);
    }
    if let Err(e) = run(Path::new(&args[1]), Path::new(&args[2])) {
        eprintln!("erreur : {e}");
        std::process::exit(1);
    }
}

fn run(sg3: &Path, out: &Path) -> Result<(), String> {
    let catalog = Catalog::load(sg3)?;
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let (mut written, mut failed) = (0, 0);
    for (id, image) in catalog.images.iter().enumerate().skip(1) {
        *kinds.entry(format!("{:?}", image.kind)).or_default() += 1;
        if catalog.is_empty(id) {
            continue;
        }
        let bitmap = catalog.bitmaps.get(image.bitmap).map_or("sans_planche", |b| b.name.as_str());
        let dir = out.join(bitmap.rsplit_once('.').map_or(bitmap, |(stem, _)| stem).to_lowercase());
        match catalog.decode(id) {
            Ok(rgba) => {
                fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let file = dir.join(format!("{id:05}.png"));
                fs::write(&file, png::encode_rgba(image.width, image.height, &rgba)).map_err(|e| e.to_string())?;
                written += 1;
            }
            Err(e) => {
                eprintln!("image {id} : {e}");
                failed += 1;
            }
        }
    }
    println!(
        "{} : {} planches, {written} images écrites, {failed} échecs, types {kinds:?}",
        sg3.display(),
        catalog.bitmaps.len()
    );
    Ok(())
}
