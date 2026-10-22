//! Installation des données du jeu d'origine : à partir d'un jeu déjà installé, du
//! contenu d'un CD copié dans un dossier, ou d'une image de CD. Les fichiers utiles au
//! moteur sont copiés, en minuscules, dans le dossier de données ; la musique et les
//! voix (MP3) sont converties en Opus si `ffmpeg` est disponible.

use crate::cabinet::Cabinet;
use crate::iso::{Entry, Image};
use crate::wise::{self, Delta};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Dossiers du jeu que le moteur utilise.
const FOLDERS: [&str; 4] = ["data/", "model/", "adventures/", "audio/"];
/// Extensions gardées : images, cartes, aventures, règles, textes, sons.
const EXTENSIONS: [&str; 13] = ["sg3", "555", "jpg", "map", "pak", "set", "sav", "txt", "eng", "wav", "mp3", "inf", "pdf"];

/// Ce qu'a fait une installation.
#[derive(Default, Debug)]
pub struct Report {
    pub copied: usize,
    pub bytes: u64,
    /// Fichiers audio convertis en Opus, et ceux laissés en MP3 faute de `ffmpeg`.
    pub converted: usize,
    pub unconverted: usize,
    /// Fichiers mis à jour par un correctif.
    pub patched: usize,
}

/// Origine de fichiers : un dossier, ou une image de CD.
enum Source {
    Folder(PathBuf),
    Disc(Box<Image>, Vec<Entry>),
}

impl Source {
    fn open(path: &Path) -> Result<Source, String> {
        if path.is_dir() {
            return Ok(Source::Folder(path.to_path_buf()));
        }
        let mut image = Image::open(path)?;
        let entries = image.entries()?;
        Ok(Source::Disc(Box::new(image), entries))
    }

    /// Chemins relatifs de tous les fichiers, séparés par des `/`.
    fn files(&self) -> Vec<String> {
        match self {
            Source::Folder(root) => {
                let mut found = Vec::new();
                let mut pending = vec![root.clone()];
                while let Some(folder) = pending.pop() {
                    for entry in fs::read_dir(&folder).into_iter().flatten().flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            pending.push(path);
                        } else if let Ok(relative) = path.strip_prefix(root) {
                            found.push(relative.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"));
                        }
                    }
                }
                found
            }
            Source::Disc(_, entries) => entries.iter().filter(|e| !e.directory).map(|e| e.path.clone()).collect(),
        }
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>, String> {
        match self {
            Source::Folder(root) => fs::read(root.join(path)).map_err(|e| format!("{path} : {e}")),
            Source::Disc(image, entries) => {
                let entry = entries.iter().find(|e| e.path == path).ok_or_else(|| format!("{path} : absent de l'image"))?;
                image.read(entry)
            }
        }
    }
}

/// Chemin d'installation d'un fichier du jeu (en minuscules), ou `None` s'il ne sert
/// pas au moteur.
fn destination(path: &str) -> Option<String> {
    let lower = path.to_lowercase();
    let extension = lower.rsplit_once('.').map(|(_, extension)| extension)?;
    let wanted = match lower.contains('/') {
        true => FOLDERS.iter().any(|folder| lower.starts_with(folder)),
        // À la racine : les textes du jeu et sa fiche d'identité.
        false => matches!(extension, "eng" | "inf"),
    };
    (wanted && EXTENSIONS.contains(&extension)).then_some(lower)
}

/// Installe dans `target` les données trouvées dans `sources`, dans l'ordre : une
/// source plus tardive (l'extension Poséidon après Zeus) remplace les fichiers de
/// même nom. `progress` reçoit une ligne par étape.
pub fn install(sources: &[PathBuf], target: &Path, progress: &mut dyn FnMut(&str)) -> Result<Report, String> {
    let mut report = Report::default();
    for path in sources {
        progress(&format!("Lecture de {}", path.display()));
        // Un programme Windows parmi les sources est un correctif à appliquer à ce qui
        // est déjà installé.
        if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")) {
            report.patched += patch(path, target, progress)?;
            continue;
        }
        let mut source = Source::open(path)?;
        let files = source.files();
        let find = |name: &str| files.iter().find(|file| file.eq_ignore_ascii_case(name)).cloned();
        let before = report.copied;

        // Archives InstallShield d'un CD : l'en-tête est dans data1.hdr, ou au début de
        // data1.cab ; les fichiers sont répartis par volume.
        if let Some(first) = find("data1.cab") {
            let header = match find("data1.hdr") {
                Some(header) => source.read(&header)?,
                None => source.read(&first)?,
            };
            let cabinet = Cabinet::parse(&header)?;
            let mut by_volume: BTreeMap<u16, Vec<(&crate::cabinet::Member, String)>> = BTreeMap::new();
            for member in &cabinet.members {
                // La vidéo d'introduction est mise de côté pour être convertie.
                let intro = is_intro(&member.path).then(|| INTRO_BINK.to_string());
                if let Some(to) = destination(&member.path).or(intro) {
                    by_volume.entry(member.volume).or_default().push((member, to));
                }
            }
            for (number, members) in by_volume {
                let name = find(&format!("data{number}.cab")).ok_or_else(|| format!("data{number}.cab absent de {}", path.display()))?;
                progress(&format!("  archive {name} : {} fichiers", members.len()));
                let volume = source.read(&name)?;
                for (member, to) in members {
                    write(target, &to, &member.extract(&volume)?, &mut report)?;
                }
            }
        }
        // Fichiers présents tels quels : jeu installé, ou sons laissés sur le CD.
        let loose: Vec<(String, String)> =
            files.iter().filter_map(|file| Some((file.clone(), destination(file).or_else(|| is_intro(file).then(|| INTRO_BINK.to_string()))?))).collect();
        if !loose.is_empty() {
            progress(&format!("  {} fichiers à copier", loose.len()));
        }
        for (from, to) in loose {
            write(target, &to, &source.read(&from)?, &mut report)?;
        }
        if report.copied == before {
            return Err(format!("{} : aucun fichier du jeu trouvé", path.display()));
        }
    }
    if !target.join("zeus_text.eng").exists() || !target.join("data").join("zeus_general.sg3").exists() {
        return Err("installation incomplète : les fichiers de base de Zeus manquent (textes ou images)".into());
    }
    convert_audio(target, &mut report, progress);
    convert_intro(target, progress);
    Ok(report)
}

/// Vidéo d'introduction du jeu (Bink), et ce qu'Olympe en fait : des images JPEG
/// (15 par seconde) et une piste Opus, qu'il sait lire lui-même.
const INTRO_BINK: &str = "binks/intro.bik";
pub const INTRO_FRAMES: &str = "binks/intro";
pub const INTRO_RATE: u64 = 15;

/// La version haute qualité de l'introduction, ou à défaut l'autre.
fn is_intro(path: &str) -> bool {
    let name = path.replace('\\', "/").to_lowercase();
    name.ends_with("binks/zeus_intro_hiq.bik") || (name.ends_with("binks/zeus_intro.bik"))
}

/// Convertit la vidéo d'introduction avec `ffmpeg` ; sans lui, elle reste de côté.
fn convert_intro(target: &Path, progress: &mut dyn FnMut(&str)) {
    let bink = target.join(INTRO_BINK);
    if !bink.exists() {
        return;
    }
    let frames = target.join(INTRO_FRAMES);
    let _ = fs::create_dir_all(&frames);
    progress("Conversion de la vidéo d'introduction");
    let run = |args: &[&str], output: &Path| {
        Command::new("ffmpeg")
            .args(["-nostdin", "-loglevel", "error", "-y", "-i"])
            .arg(&bink)
            .args(args)
            .arg(output)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    let rate = format!("fps={INTRO_RATE}");
    let images = run(&["-vf", &rate, "-pix_fmt", "yuvj420p", "-q:v", "4"], &frames.join("%04d.jpg"));
    let sound = run(&["-vn", "-c:a", "libopus", "-b:a", "96k"], &frames.join("son.opus"));
    if images && sound {
        let _ = fs::remove_file(&bink);
    } else {
        progress("  ffmpeg introuvable ou en échec : pas de vidéo d'introduction");
    }
}

/// Applique un correctif Wise : chacun de ses différentiels désigne le fichier qu'il
/// corrige par sa taille et sa somme de contrôle, qu'on cherche parmi les fichiers
/// installés. Rend le nombre de fichiers mis à jour.
fn patch(installer: &Path, target: &Path, progress: &mut dyn FnMut(&str)) -> Result<usize, String> {
    let data = fs::read(installer).map_err(|e| format!("{} : {e}", installer.display()))?;
    let streams = wise::streams(&data);
    let deltas: Vec<Delta> = streams.iter().filter_map(|stream| Delta::parse(stream)).collect();
    if deltas.is_empty() {
        return Err(format!("{} : aucun correctif reconnu dans ce programme", installer.display()));
    }
    // Fichiers installés, rangés par taille : seuls ceux de la bonne taille sont lus.
    let mut installed: Vec<(u64, PathBuf)> = Vec::new();
    let mut pending = vec![target.to_path_buf()];
    while let Some(folder) = pending.pop() {
        for entry in fs::read_dir(&folder).into_iter().flatten().flatten() {
            match entry.metadata() {
                Ok(meta) if meta.is_dir() => pending.push(entry.path()),
                Ok(meta) => installed.push((meta.len(), entry.path())),
                Err(_) => {}
            }
        }
    }
    let mut patched = 0;
    for delta in &deltas {
        for (_, file) in installed.iter().filter(|(size, _)| *size == delta.old_size as u64) {
            let Ok(old) = fs::read(file) else { continue };
            if wise::crc32(&old) != delta.old_crc {
                continue;
            }
            let name = file.strip_prefix(target).unwrap_or(file).display().to_string();
            match delta.apply(&old) {
                Ok(new) => {
                    fs::write(file, new).map_err(|e| format!("{} : {e}", file.display()))?;
                    progress(&format!("  corrigé : {name}"));
                    patched += 1;
                }
                Err(e) => progress(&format!("  non corrigé : {name} ({e})")),
            }
        }
    }
    if patched == 0 {
        progress("  rien à corriger : les fichiers visés sont absents ou déjà à jour");
    }
    Ok(patched)
}

fn write(target: &Path, relative: &str, data: &[u8], report: &mut Report) -> Result<(), String> {
    let file = target.join(relative);
    if let Some(folder) = file.parent() {
        fs::create_dir_all(folder).map_err(|e| format!("{} : {e}", folder.display()))?;
    }
    fs::write(&file, data).map_err(|e| format!("{} : {e}", file.display()))?;
    report.copied += 1;
    report.bytes += data.len() as u64;
    Ok(())
}

/// Convertit les MP3 installés en Opus avec `ffmpeg`, sur tous les cœurs. Sans
/// `ffmpeg`, ils restent en place et le jeu se passera de musique.
fn convert_audio(target: &Path, report: &mut Report, progress: &mut dyn FnMut(&str)) {
    // Partout : certaines aventures apportent leurs propres voix.
    let mut pending = vec![target.to_path_buf()];
    let mut tracks = Vec::new();
    while let Some(folder) = pending.pop() {
        for entry in fs::read_dir(&folder).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "mp3") {
                tracks.push(path);
            }
        }
    }
    if tracks.is_empty() {
        return;
    }
    let available = Command::new("ffmpeg").arg("-version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
    if !available {
        report.unconverted = tracks.len();
        progress("  ffmpeg introuvable : musique et voix restent en MP3, le jeu sera muet de ce côté");
        return;
    }
    progress(&format!("Conversion de {} fichiers audio en Opus", tracks.len()));
    let (next, done) = (AtomicUsize::new(0), AtomicUsize::new(0));
    let workers = std::thread::available_parallelism().map_or(2, |n| n.get());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                while let Some(track) = tracks.get(next.fetch_add(1, Ordering::Relaxed)) {
                    // La musique mérite plus de débit que les voix.
                    let music = track.parent().is_some_and(|folder| folder.ends_with("music"));
                    let output = track.with_extension("opus");
                    let status = Command::new("ffmpeg")
                        .args(["-nostdin", "-loglevel", "error", "-y", "-i"])
                        .arg(track)
                        .args(["-vn", "-c:a", "libopus", "-b:a", if music { "128k" } else { "48k" }])
                        .arg(&output)
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status();
                    // Un fichier vide ou illisible (il y en a un sur les CD) est laissé de côté.
                    if status.is_ok_and(|s| s.success()) {
                        let _ = fs::remove_file(track);
                        done.fetch_add(1, Ordering::Relaxed);
                    } else {
                        let _ = fs::remove_file(&output);
                    }
                }
            });
        }
    });
    report.converted = done.load(Ordering::Relaxed);
    report.unconverted = tracks.len() - report.converted;
}

#[cfg(test)]
mod tests {
    use super::destination;

    #[test]
    fn keeps_what_the_engine_reads() {
        assert_eq!(destination("DATA/Zeus_General.SG3").as_deref(), Some("data/zeus_general.sg3"));
        assert_eq!(destination("Zeus_Text.eng").as_deref(), Some("zeus_text.eng"));
        assert_eq!(destination("Adventures/L'odyssée/L'odysséeP.map").as_deref(), Some("adventures/l'odyssée/l'odysséep.map"));
        assert_eq!(destination("Audio/Music/Proi.mp3").as_deref(), Some("audio/music/proi.mp3"));
    }

    #[test]
    fn leaves_out_the_rest() {
        assert_eq!(destination("Zeus.exe"), None);
        assert_eq!(destination("mss32.dll"), None);
        assert_eq!(destination("Setup.bmp"), None);
        assert_eq!(destination("Binks/Zeus_Intro.bik"), None);
        assert_eq!(destination("Demos/Pharaoh/data1.cab"), None);
        assert_eq!(destination("DATA/sansextension"), None);
    }
}
