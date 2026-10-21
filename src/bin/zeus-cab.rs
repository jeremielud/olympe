//! Liste ou extrait une archive InstallShield (outil de vérification).

use std::fs;
use std::path::Path;
use zeus::cabinet::Cabinet;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage : zeus-cab <dossier contenant data1.cab> [dossier de sortie]");
        std::process::exit(2);
    }
    if let Err(e) = run(Path::new(&args[1]), args.get(2).map(Path::new)) {
        eprintln!("erreur : {e}");
        std::process::exit(1);
    }
}

fn run(folder: &Path, out: Option<&Path>) -> Result<(), String> {
    let read = |name: &str| fs::read(folder.join(name)).map_err(|e| format!("{name} : {e}"));
    let header = read("data1.hdr").or_else(|_| read("data1.cab"))?;
    let cabinet = Cabinet::parse(&header)?;
    let mut volumes: Vec<Option<Vec<u8>>> = Vec::new();
    let mut total = 0;
    for member in &cabinet.members {
        total += member.size;
        let Some(out) = out else {
            println!("{:>10} {}", member.size, member.path);
            continue;
        };
        let volume = member.volume as usize;
        if volumes.len() <= volume {
            volumes.resize(volume + 1, None);
        }
        if volumes[volume].is_none() {
            volumes[volume] = Some(read(&format!("data{volume}.cab"))?);
        }
        let data = member.extract(volumes[volume].as_deref().unwrap())?;
        let target = out.join(&member.path);
        fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::write(&target, data).map_err(|e| e.to_string())?;
    }
    eprintln!("{} fichiers, {} Mo", cabinet.members.len(), total / 1_000_000);
    Ok(())
}
