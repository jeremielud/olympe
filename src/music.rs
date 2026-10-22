//! Musique de fond : enchaîne les morceaux de la liste du jeu (`audio/music.txt`).

use crate::opus::{SAMPLE_RATE, Stream};
use crate::sdl;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::ptr::null;

/// Section de `music.txt` jouée pendant la construction de la cité.
const GENERAL_SECTION: &str = "GENERAL_MUSIC";
/// Avance de son gardée dans la file de SDL, en millisecondes.
const QUEUE_AHEAD_MS: i32 = 500;
const CHANNELS: usize = 2;
const VOLUME: f32 = 0.6;

pub struct Music {
    tracks: Vec<PathBuf>,
    /// Rang du prochain morceau.
    next: usize,
    current: Option<Stream>,
    output: sdl::AudioStream,
    playing: bool,
}

impl Music {
    /// Ouvre la sortie son et lit la liste des morceaux. `first` choisit le morceau
    /// de départ, pour ne pas toujours commencer par le même.
    pub fn open(audio: &Path, first: usize) -> Result<Music, String> {
        let list = std::fs::read(audio.join("music.txt")).map_err(|e| format!("music.txt : {e}"))?;
        let list: String = list.iter().map(|&b| b as char).collect();
        let mut tracks = Vec::new();
        let mut in_section = false;
        for line in list.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with(';')) {
            match line.rsplit_once('.') {
                // Les morceaux sont listés en `.mp3` ; nos données les ont en Opus, en minuscules.
                Some((stem, _)) if in_section => tracks.push(audio.join("music").join(format!("{}.opus", stem.to_lowercase()))),
                Some(_) => {}
                None => in_section = line == GENERAL_SECTION,
            }
        }
        tracks.retain(|track| track.exists());
        if tracks.is_empty() {
            return Err("aucun morceau de musique".into());
        }
        let spec = sdl::AudioSpec { format: sdl::AUDIO_S16LE, channels: CHANNELS as i32, freq: SAMPLE_RATE };
        // SAFETY : appels directs à SDL3 ; le pointeur rendu est vérifié.
        let output = unsafe {
            if !sdl::SDL_InitSubSystem(sdl::INIT_AUDIO) {
                return Err(sdl::error("initialisation du son"));
            }
            let output = sdl::SDL_OpenAudioDeviceStream(sdl::AUDIO_DEVICE_DEFAULT_PLAYBACK, &spec, null(), std::ptr::null_mut());
            if output.is_null() {
                return Err(sdl::error("ouverture de la sortie son"));
            }
            sdl::SDL_SetAudioStreamGain(output, crate::sdl::volume(VOLUME));
            sdl::SDL_ResumeAudioStreamDevice(output);
            output
        };
        let next = first % tracks.len();
        Ok(Music { tracks, next, current: None, output, playing: true })
    }

    /// Nom du morceau en cours.
    pub fn title(&self) -> String {
        let playing = (self.next + self.tracks.len() - 1) % self.tracks.len();
        self.tracks[playing].file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    }

    /// Coupe ou rétablit la musique ; rend le nouvel état.
    pub fn toggle(&mut self) -> bool {
        self.playing = !self.playing;
        // SAFETY : `output` est un flux ouvert par `open`.
        unsafe {
            if self.playing {
                sdl::SDL_ResumeAudioStreamDevice(self.output);
            } else {
                sdl::SDL_PauseAudioStreamDevice(self.output);
            }
        }
        self.playing
    }

    /// À appeler à chaque image : décode de quoi garder la file de SDL remplie.
    pub fn update(&mut self) {
        if !self.playing {
            return;
        }
        let wanted = SAMPLE_RATE * (CHANNELS as i32) * 2 * QUEUE_AHEAD_MS / 1000;
        // Garde-fou : jamais plus d'un tour complet de la liste par appel.
        let mut failures = 0;
        // SAFETY : `output` est un flux ouvert par `open` ; les tampons passés vivent le temps de l'appel.
        while unsafe { sdl::SDL_GetAudioStreamQueued(self.output) } < wanted && failures < self.tracks.len() {
            let Some(stream) = self.current.as_mut() else {
                let track = &self.tracks[self.next];
                self.next = (self.next + 1) % self.tracks.len();
                match std::fs::read(track).map_err(|e| e.to_string()).and_then(Stream::open) {
                    Ok(stream) => self.current = Some(stream),
                    Err(e) => {
                        eprintln!("{} : {e}", track.display());
                        failures += 1;
                    }
                }
                continue;
            };
            let Some(pcm) = stream.decode() else {
                self.current = None;
                continue;
            };
            // La sortie est stéréo : un morceau mono est dédoublé.
            let pcm: Vec<i16> = if stream.channels == 1 { pcm.iter().flat_map(|&s| [s, s]).collect() } else { pcm };
            unsafe { sdl::SDL_PutAudioStreamData(self.output, pcm.as_ptr() as *const c_void, (pcm.len() * 2) as i32) };
        }
    }
}

impl Drop for Music {
    fn drop(&mut self) {
        // SAFETY : le flux vient de `SDL_OpenAudioDeviceStream` et n'est détruit qu'ici.
        unsafe { sdl::SDL_DestroyAudioStream(self.output) }
    }
}

/// Narration des récits d'épisode : un fichier Opus lu une fois, sur sa propre sortie
/// son, coupé quand le récit se referme.
pub struct Voice {
    current: Option<Stream>,
    output: sdl::AudioStream,
}

/// Voix un peu plus forte que la musique.
const VOICE_VOLUME: f32 = 0.9;

impl Voice {
    pub fn open() -> Result<Voice, String> {
        let spec = sdl::AudioSpec { format: sdl::AUDIO_S16LE, channels: CHANNELS as i32, freq: SAMPLE_RATE };
        // SAFETY : appels directs à SDL3 ; le pointeur rendu est vérifié.
        let output = unsafe {
            if !sdl::SDL_InitSubSystem(sdl::INIT_AUDIO) {
                return Err(sdl::error("initialisation du son"));
            }
            let output = sdl::SDL_OpenAudioDeviceStream(sdl::AUDIO_DEVICE_DEFAULT_PLAYBACK, &spec, null(), std::ptr::null_mut());
            if output.is_null() {
                return Err(sdl::error("ouverture de la sortie son"));
            }
            sdl::SDL_SetAudioStreamGain(output, crate::sdl::volume(VOICE_VOLUME));
            sdl::SDL_ResumeAudioStreamDevice(output);
            output
        };
        Ok(Voice { current: None, output })
    }

    /// Commence la lecture d'un fichier, en coupant la précédente.
    pub fn play(&mut self, file: &Path) {
        self.stop();
        match std::fs::read(file).map_err(|e| e.to_string()).and_then(Stream::open) {
            Ok(stream) => self.current = Some(stream),
            Err(e) => eprintln!("{} : {e}", file.display()),
        }
    }

    pub fn stop(&mut self) {
        self.current = None;
        // SAFETY : `output` est un flux ouvert par `open`.
        unsafe { sdl::SDL_ClearAudioStream(self.output) };
    }

    /// À appeler à chaque image, comme `Music::update`.
    pub fn update(&mut self) {
        let wanted = SAMPLE_RATE * (CHANNELS as i32) * 2 * QUEUE_AHEAD_MS / 1000;
        // SAFETY : `output` est un flux ouvert par `open` ; les tampons vivent le temps de l'appel.
        while unsafe { sdl::SDL_GetAudioStreamQueued(self.output) } < wanted {
            let Some(stream) = self.current.as_mut() else { return };
            let Some(pcm) = stream.decode() else {
                self.current = None;
                return;
            };
            let pcm: Vec<i16> = if stream.channels == 1 { pcm.iter().flat_map(|&s| [s, s]).collect() } else { pcm };
            unsafe { sdl::SDL_PutAudioStreamData(self.output, pcm.as_ptr() as *const c_void, (pcm.len() * 2) as i32) };
        }
    }
}

impl Drop for Voice {
    fn drop(&mut self) {
        // SAFETY : le flux vient de `SDL_OpenAudioDeviceStream` et n'est détruit qu'ici.
        unsafe { sdl::SDL_DestroyAudioStream(self.output) }
    }
}
