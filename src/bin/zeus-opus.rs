//! Décode un fichier Ogg Opus en WAV 16 bits, pour vérifier le décodeur du jeu.

use zeus::opus::{SAMPLE_RATE, Stream};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage : zeus-opus <entrée.opus> <sortie.wav>");
        std::process::exit(2);
    }
    if let Err(e) = run(&args[1], &args[2]) {
        eprintln!("erreur : {e}");
        std::process::exit(1);
    }
}

fn run(input: &str, output: &str) -> Result<(), String> {
    let mut stream = Stream::open(std::fs::read(input).map_err(|e| e.to_string())?)?;
    let channels = stream.channels;
    let mut samples = Vec::new();
    while let Some(pcm) = stream.decode() {
        samples.extend(pcm);
    }
    let bytes = samples.len() as u32 * 2;
    let mut wav = Vec::with_capacity(bytes as usize + 44);
    wav.extend(b"RIFF");
    wav.extend((36 + bytes).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend((channels as u16).to_le_bytes());
    wav.extend((SAMPLE_RATE as u32).to_le_bytes());
    wav.extend((SAMPLE_RATE as u32 * channels as u32 * 2).to_le_bytes());
    wav.extend((channels as u16 * 2).to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(bytes.to_le_bytes());
    wav.extend(samples.iter().flat_map(|s| s.to_le_bytes()));
    std::fs::write(output, wav).map_err(|e| e.to_string())?;
    println!("{input} : {channels} canaux, {:.2} s", samples.len() as f64 / channels as f64 / SAMPLE_RATE as f64);
    Ok(())
}
