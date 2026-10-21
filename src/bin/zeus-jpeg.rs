//! Décode un JPEG du jeu en PNG, pour vérifier le décodeur.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage : zeus-jpeg <entrée.jpg> <sortie.png>");
        std::process::exit(2);
    }
    let picture = std::fs::read(&args[1]).map_err(|e| e.to_string()).and_then(|file| zeus::jpeg::decode(&file));
    match picture {
        Ok(picture) => {
            let png = zeus::png::encode_rgba(picture.width, picture.height, &picture.rgba);
            if let Err(e) = std::fs::write(&args[2], png) {
                eprintln!("erreur : {e}");
                std::process::exit(1);
            }
            println!("{} : {} × {}", args[1], picture.width, picture.height);
        }
        Err(e) => {
            eprintln!("erreur : {e}");
            std::process::exit(1);
        }
    }
}
