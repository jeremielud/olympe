//! Convertit les médias des CD (MP3, WAV, Bink) vers des formats libres :
//! Opus pour le son, VP9 + Opus en WebM pour la vidéo. Nécessite `ffmpeg` et `ffprobe`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{fs, io, thread};

const USAGE: &str = "usage : zeus-convert <dossier de sortie> <source prioritaire> [autres sources…]
  Les sources sont des dossiers de CD ou d'installation ; à nom égal, la première l'emporte.";

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Music,
    Voice,
    Video,
}

struct Job {
    source: PathBuf,
    target: PathBuf,
    kind: Kind,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("{USAGE}");
        std::process::exit(2);
    }
    if let Err(e) = run(Path::new(&args[0]), &args[1..]) {
        eprintln!("erreur : {e}");
        std::process::exit(1);
    }
}

fn run(out: &Path, sources: &[String]) -> io::Result<()> {
    // Chemins relatifs en minuscules : le jeu d'origine ignore la casse, pas tous les systèmes.
    let mut jobs: BTreeMap<String, Job> = BTreeMap::new();
    for root in sources {
        let mut files = Vec::new();
        walk(Path::new(root), &mut files)?;
        for source in files {
            let relative = source.strip_prefix(root).unwrap().to_string_lossy().to_lowercase();
            let Some((stem, extension)) = relative.rsplit_once('.') else { continue };
            // Les CD contiennent des fichiers vides (voix jamais enregistrées).
            if fs::metadata(&source)?.len() == 0 {
                continue;
            }
            let kind = match extension {
                "mp3" | "wav" if relative.contains("music") => Kind::Music,
                "mp3" => Kind::Voice,
                // Les autres WAV sont des bruitages courts : ils restent sans perte.
                "bik" => Kind::Video,
                _ => continue,
            };
            let target = out.join(format!("{stem}.{}", if kind == Kind::Video { "webm" } else { "opus" }));
            jobs.entry(stem.to_string()).or_insert(Job { source, target, kind });
        }
    }

    let jobs: Vec<Job> = jobs.into_values().collect();
    let pending: Vec<&Job> = jobs.iter().filter(|j| !j.target.exists()).collect();
    println!("{} fichiers, dont {} déjà convertis", jobs.len(), jobs.len() - pending.len());

    let next = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::new());
    let workers = thread::available_parallelism().map_or(4, |n| n.get());
    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                while let Some(job) = pending.get(next.fetch_add(1, Ordering::Relaxed)) {
                    if let Err(e) = convert(job) {
                        failures.lock().unwrap().push(format!("{} : {e}", job.source.display()));
                    }
                }
            });
        }
    });

    let failures = failures.into_inner().unwrap();
    for failure in &failures {
        eprintln!("échec : {failure}");
    }
    let size = |paths: &mut dyn Iterator<Item = &PathBuf>| -> u64 {
        paths.filter_map(|p| fs::metadata(p).ok()).map(|m| m.len()).sum::<u64>() / 1_000_000
    };
    println!(
        "{} convertis, {} échecs ; {} Mo → {} Mo dans {}",
        pending.len() - failures.len(),
        failures.len(),
        size(&mut jobs.iter().map(|j| &j.source)),
        size(&mut jobs.iter().map(|j| &j.target)),
        out.display()
    );
    if failures.is_empty() { Ok(()) } else { Err(io::Error::other("certaines conversions ont échoué")) }
}

fn walk(dir: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            walk(&path, files)?;
        } else {
            files.push(path);
        }
    }
    Ok(())
}

fn convert(job: &Job) -> io::Result<()> {
    fs::create_dir_all(job.target.parent().unwrap())?;
    // On écrit à côté puis on renomme : une conversion interrompue ne laisse pas de fichier tronqué.
    let partial = job.target.with_extension(match job.kind {
        Kind::Video => "partiel.webm",
        _ => "partiel.opus",
    });
    let mut ffmpeg = Command::new("ffmpeg");
    ffmpeg.args(["-nostdin", "-loglevel", "error", "-y", "-i"]).arg(&job.source);
    match job.kind {
        Kind::Video => {
            // VP9 à qualité constante : lu partout où Opus l'est, et libre de droits.
            ffmpeg.args(["-c:v", "libvpx-vp9", "-crf", "24", "-b:v", "0", "-deadline", "good", "-cpu-used", "1"]);
            ffmpeg.args(["-row-mt", "1", "-pix_fmt", "yuv420p"]);
            ffmpeg.args(["-c:a", "libopus", "-b:a", "128k"]);
        }
        Kind::Music | Kind::Voice => {
            let stereo = channels(&job.source)? > 1;
            let bitrate = match (job.kind, stereo) {
                (Kind::Music, true) => "128k",
                (Kind::Music, false) | (_, true) => "72k",
                (_, false) => "40k",
            };
            ffmpeg.args(["-vn", "-c:a", "libopus", "-b:a", bitrate]);
        }
    }
    let output = ffmpeg.arg(&partial).stdout(Stdio::null()).output()?;
    if !output.status.success() {
        let _ = fs::remove_file(&partial);
        return Err(io::Error::other(String::from_utf8_lossy(&output.stderr).trim().to_string()));
    }
    fs::rename(&partial, &job.target)
}

fn channels(source: &Path) -> io::Result<u32> {
    let output = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "a:0", "-show_entries", "stream=channels", "-of", "csv=p=0"])
        .arg(source)
        .output()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.trim().parse().map_err(|_| io::Error::other("nombre de canaux illisible"))
}
