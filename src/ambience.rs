//! Ambiance sonore : bruitages associés à ce qui est visible à l'écran, d'après la
//! table du jeu (`audio/sound_fxu.txt`, couche 2).

use crate::sdl;
use std::collections::HashMap;
use std::ffi::{CString, c_void};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};

/// Nombre de bruitages qui peuvent se superposer.
const VOICES: usize = 3;
const VOLUME: f32 = 0.45;
const LAYER_START: &str = "LAYER2_START";

/// Bruitages d'une catégorie (« FOUNTAIN », « COMMON_HOUSING »…).
struct Category {
    /// Chance sur cent que la catégorie se fasse entendre quand elle est tirée.
    chance: u32,
    sounds: Vec<PathBuf>,
}

pub struct Ambience {
    categories: HashMap<String, Category>,
    voices: Vec<sdl::AudioStream>,
    next_voice: usize,
    pub enabled: bool,
}

impl Ambience {
    pub fn open(audio: &Path) -> Result<Ambience, String> {
        let table = std::fs::read(audio.join("sound_fxu.txt")).map_err(|e| format!("sound_fxu.txt : {e}"))?;
        let table: String = table.iter().map(|&b| b as char).collect();
        let folder = audio.join("ambient").join("layer2");
        let mut categories: HashMap<String, Category> = HashMap::new();
        let mut current = None;
        let lines = table.lines().map(str::trim).skip_while(|line| *line != LAYER_START);
        for line in lines.filter(|l| !l.is_empty() && !l.starts_with(';')) {
            let mut words = line.split_whitespace();
            match (words.next(), words.next(), words.next()) {
                (Some("CATEGORY"), Some(name), Some(chance)) => {
                    current = Some(name.to_string());
                    categories.insert(name.to_string(), Category { chance: chance.parse().unwrap_or(50), sounds: Vec::new() });
                }
                (Some(file), None, None) if file.to_lowercase().ends_with(".wav") => {
                    let path = folder.join(file.to_lowercase());
                    if let (Some(category), true) = (current.as_ref().and_then(|name| categories.get_mut(name)), path.exists()) {
                        category.sounds.push(path);
                    }
                }
                _ => {}
            }
        }
        categories.retain(|_, category| !category.sounds.is_empty());
        if categories.is_empty() {
            return Err("aucun bruitage d'ambiance".into());
        }
        let mut voices = Vec::new();
        // SAFETY : appels directs à SDL3 ; chaque flux rendu est vérifié.
        unsafe {
            if !sdl::SDL_InitSubSystem(sdl::INIT_AUDIO) {
                return Err(sdl::error("initialisation du son"));
            }
            for _ in 0..VOICES {
                let voice = sdl::SDL_OpenAudioDeviceStream(sdl::AUDIO_DEVICE_DEFAULT_PLAYBACK, null(), null(), null_mut());
                if voice.is_null() {
                    return Err(sdl::error("ouverture de la sortie son"));
                }
                sdl::SDL_SetAudioStreamGain(voice, crate::sdl::volume(VOLUME));
                sdl::SDL_ResumeAudioStreamDevice(voice);
                voices.push(voice);
            }
        }
        Ok(Ambience { categories, voices, next_voice: 0, enabled: true })
    }

    /// Joue un bruitage de la catégorie, si elle existe et que le sort le veut.
    /// `random` fournit le hasard (deux tirages).
    pub fn play(&mut self, category: &str, mut random: impl FnMut() -> u32) {
        let Some(category) = self.categories.get(category).filter(|_| self.enabled) else { return };
        if random() % 100 >= category.chance {
            return;
        }
        let file = &category.sounds[random() as usize % category.sounds.len()];
        let Ok(path) = CString::new(file.to_string_lossy().as_bytes()) else { return };
        let voice = self.voices[self.next_voice];
        self.next_voice = (self.next_voice + 1) % self.voices.len();
        let mut spec = sdl::AudioSpec { format: 0, channels: 0, freq: 0 };
        let (mut data, mut length) = (null_mut::<u8>(), 0u32);
        // SAFETY : SDL remplit `spec`, `data` et `length` si le chargement réussit ; le
        // tampon est copié dans le flux puis rendu à SDL.
        unsafe {
            if !sdl::SDL_LoadWAV(path.as_ptr(), &mut spec, &mut data, &mut length) {
                return;
            }
            // La voix abandonne ce qu'elle jouait encore et prend le format du fichier.
            sdl::SDL_ClearAudioStream(voice);
            sdl::SDL_SetAudioStreamFormat(voice, &spec, null());
            sdl::SDL_PutAudioStreamData(voice, data as *const c_void, length as i32);
            sdl::SDL_free(data as *mut c_void);
        }
    }
}

impl Drop for Ambience {
    fn drop(&mut self) {
        for &voice in &self.voices {
            // SAFETY : chaque flux vient de `SDL_OpenAudioDeviceStream` et n'est détruit qu'ici.
            unsafe { sdl::SDL_DestroyAudioStream(voice) }
        }
    }
}
