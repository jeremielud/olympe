//! Liaison minimale avec SDL3 : seulement les fonctions dont le jeu a besoin.
#![allow(non_snake_case)]

use std::ffi::{CStr, c_char, c_int, c_void};

pub const INIT_AUDIO: u32 = 0x10;
pub const INIT_VIDEO: u32 = 0x20;
/// Sortie son par défaut du système.
pub const AUDIO_DEVICE_DEFAULT_PLAYBACK: u32 = 0xFFFF_FFFF;
/// Échantillons 16 bits signés, petit-boutistes.
pub const AUDIO_S16LE: u32 = 0x8010;
pub const WINDOW_RESIZABLE: u64 = 0x20;
pub const WINDOW_HIGH_PIXEL_DENSITY: u64 = 0x2000;
/// Octets R, V, B, A dans cet ordre en mémoire (machines petit-boutistes).
pub const PIXELFORMAT_ABGR8888: u32 = 0x1676_2004;
pub const TEXTUREACCESS_STATIC: c_int = 0;
/// Texture dans laquelle on peut dessiner.
pub const TEXTUREACCESS_TARGET: c_int = 2;
pub const SCALEMODE_NEAREST: c_int = 0;
pub const SCALEMODE_LINEAR: c_int = 1;
pub const BLENDMODE_BLEND: u32 = 1;

pub const EVENT_QUIT: u32 = 0x100;
pub const EVENT_KEY_DOWN: u32 = 0x300;
pub const EVENT_MOUSE_MOTION: u32 = 0x400;
pub const EVENT_MOUSE_BUTTON_DOWN: u32 = 0x401;
pub const EVENT_MOUSE_BUTTON_UP: u32 = 0x402;
pub const EVENT_MOUSE_WHEEL: u32 = 0x403;
pub const BUTTON_LEFT: u8 = 1;
pub const BUTTON_MIDDLE: u8 = 2;
pub const BUTTON_RIGHT: u8 = 3;
pub const BUTTON_LEFT_MASK: u32 = 1;
pub const BUTTON_MIDDLE_MASK: u32 = 2;
pub const BUTTON_RIGHT_MASK: u32 = 4;

pub const KEY_ESCAPE: u32 = 0x1B;
pub const KEY_SPACE: u32 = 0x20;
/// Touches « 1 » à « 9 » de la rangée du haut, par emplacement (AZERTY comme QWERTY).
pub const SCANCODE_1: u32 = 30;
pub const SCANCODE_9: u32 = 38;
pub const KEY_B: u32 = 0x62;
pub const KEY_C: u32 = 0x63;
pub const KEY_F: u32 = 0x66;
pub const KEY_H: u32 = 0x68;
pub const KEY_M: u32 = 0x6D;
pub const KEY_O: u32 = 0x6F;
pub const KEY_X: u32 = 0x78;
pub const KEY_Q: u32 = 0x71;
pub const KEY_R: u32 = 0x72;
pub const KEY_F5: u32 = 0x4000_003E;
pub const KEY_F9: u32 = 0x4000_0042;
pub const KEY_PAGEUP: u32 = 0x4000_004B;
pub const KEY_PAGEDOWN: u32 = 0x4000_004E;
pub const KEY_RIGHT: u32 = 0x4000_004F;
pub const KEY_LEFT: u32 = 0x4000_0050;
pub const KEY_DOWN: u32 = 0x4000_0051;
pub const KEY_UP: u32 = 0x4000_0052;

pub type Window = *mut c_void;
pub type Renderer = *mut c_void;
pub type Texture = *mut c_void;

/// `SDL_Event` est une union de 128 octets ; on y lit les champs par décalage.
#[repr(C, align(8))]
pub struct Event(pub [u8; 128]);

impl Event {
    pub fn new() -> Event {
        Event([0; 128])
    }

    pub fn kind(&self) -> u32 {
        self.u32_at(0)
    }

    pub fn u32_at(&self, offset: usize) -> u32 {
        u32::from_ne_bytes(self.0[offset..offset + 4].try_into().unwrap())
    }

    pub fn f32_at(&self, offset: usize) -> f32 {
        f32::from_ne_bytes(self.0[offset..offset + 4].try_into().unwrap())
    }

    fn set_u32(&mut self, offset: usize, value: u32) {
        self.0[offset..offset + 4].copy_from_slice(&value.to_ne_bytes());
    }

    fn set_f32(&mut self, offset: usize, value: f32) {
        self.0[offset..offset + 4].copy_from_slice(&value.to_ne_bytes());
    }

    /// Appui sur une touche, pour les essais scriptés.
    pub fn key_down(scancode: u32, key: u32) -> Event {
        let mut event = Event::new();
        event.set_u32(0, EVENT_KEY_DOWN);
        event.set_u32(24, scancode);
        event.set_u32(28, key);
        event.0[36] = 1;
        event
    }

    /// Déplacement du pointeur vers `to`, boutons `buttons` enfoncés.
    pub fn mouse_motion(from: (f32, f32), to: (f32, f32), buttons: u32) -> Event {
        let mut event = Event::new();
        event.set_u32(0, EVENT_MOUSE_MOTION);
        event.set_u32(24, buttons);
        event.set_f32(28, to.0);
        event.set_f32(32, to.1);
        event.set_f32(36, to.0 - from.0);
        event.set_f32(40, to.1 - from.1);
        event
    }

    /// Appui ou relâchement d'un bouton de souris.
    pub fn mouse_button(down: bool, button: u8, at: (f32, f32)) -> Event {
        let mut event = Event::new();
        event.set_u32(0, if down { EVENT_MOUSE_BUTTON_DOWN } else { EVENT_MOUSE_BUTTON_UP });
        event.0[24] = button;
        event.0[25] = down as u8;
        event.set_f32(28, at.0);
        event.set_f32(32, at.1);
        event
    }

    /// Cran de molette au-dessus du point `at`.
    pub fn mouse_wheel(steps: f32, at: (f32, f32)) -> Event {
        let mut event = Event::new();
        event.set_u32(0, EVENT_MOUSE_WHEEL);
        event.set_f32(28, steps);
        event.set_f32(36, at.0);
        event.set_f32(40, at.1);
        event
    }

    /// Touche d'un événement clavier.
    pub fn key(&self) -> u32 {
        self.u32_at(28)
    }

    /// Emplacement physique de la touche, indépendant de la disposition du clavier.
    pub fn scancode(&self) -> u32 {
        self.u32_at(24)
    }

    /// Bouton d'un événement de clic.
    pub fn button(&self) -> u8 {
        self.0[24]
    }

    /// Position du pointeur pour un clic ou un déplacement.
    pub fn position(&self) -> (f32, f32) {
        (self.f32_at(28), self.f32_at(32))
    }

    /// Déplacement relatif et boutons enfoncés d'un événement de mouvement.
    pub fn motion(&self) -> (f32, f32, u32) {
        (self.f32_at(36), self.f32_at(40), self.u32_at(24))
    }

    /// Cran de molette et position du pointeur.
    pub fn wheel(&self) -> (f32, f32, f32) {
        (self.f32_at(28), self.f32_at(36), self.f32_at(40))
    }
}

impl Default for Event {
    fn default() -> Event {
        Event::new()
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[repr(C)]
pub struct Surface {
    pub flags: u32,
    pub format: u32,
    pub w: c_int,
    pub h: c_int,
    pub pitch: c_int,
    pub pixels: *mut c_void,
}

pub type AudioStream = *mut c_void;

#[repr(C)]
pub struct AudioSpec {
    pub format: u32,
    pub channels: c_int,
    pub freq: c_int,
}

#[link(name = "SDL3")]
unsafe extern "C" {
    pub fn SDL_Init(flags: u32) -> bool;
    pub fn SDL_InitSubSystem(flags: u32) -> bool;
    pub fn SDL_OpenAudioDeviceStream(device: u32, spec: *const AudioSpec, callback: *const c_void, userdata: *mut c_void) -> AudioStream;
    pub fn SDL_ResumeAudioStreamDevice(stream: AudioStream) -> bool;
    pub fn SDL_PauseAudioStreamDevice(stream: AudioStream) -> bool;
    pub fn SDL_PutAudioStreamData(stream: AudioStream, data: *const c_void, length: c_int) -> bool;
    pub fn SDL_GetAudioStreamQueued(stream: AudioStream) -> c_int;
    pub fn SDL_ClearAudioStream(stream: AudioStream) -> bool;
    pub fn SDL_SetAudioStreamGain(stream: AudioStream, gain: f32) -> bool;
    pub fn SDL_DestroyAudioStream(stream: AudioStream);
    pub fn SDL_SetAudioStreamFormat(stream: AudioStream, source: *const AudioSpec, destination: *const AudioSpec) -> bool;
    pub fn SDL_LoadWAV(path: *const c_char, spec: *mut AudioSpec, data: *mut *mut u8, length: *mut u32) -> bool;
    pub fn SDL_free(memory: *mut c_void);
    pub fn SDL_Quit();
    pub fn SDL_GetError() -> *const c_char;
    pub fn SDL_GetTicks() -> u64;
    pub fn SDL_Delay(ms: u32);
    pub fn SDL_CreateWindow(title: *const c_char, w: c_int, h: c_int, flags: u64) -> Window;
    pub fn SDL_SetWindowTitle(window: Window, title: *const c_char) -> bool;
    pub fn SDL_SetWindowFullscreen(window: Window, fullscreen: bool) -> bool;
    pub fn SDL_CreateRenderer(window: Window, name: *const c_char) -> Renderer;
    pub fn SDL_SetRenderVSync(renderer: Renderer, vsync: c_int) -> bool;
    pub fn SDL_GetRendererName(renderer: Renderer) -> *const c_char;
    pub fn SDL_GetCurrentVideoDriver() -> *const c_char;
    pub fn SDL_GetRenderOutputSize(renderer: Renderer, w: *mut c_int, h: *mut c_int) -> bool;
    pub fn SDL_CreateTexture(renderer: Renderer, format: u32, access: c_int, w: c_int, h: c_int) -> Texture;
    pub fn SDL_DestroyTexture(texture: Texture);
    pub fn SDL_SetRenderTarget(renderer: Renderer, texture: Texture) -> bool;
    pub fn SDL_UpdateTexture(texture: Texture, rect: *const c_void, pixels: *const c_void, pitch: c_int) -> bool;
    pub fn SDL_SetTextureScaleMode(texture: Texture, mode: c_int) -> bool;
    pub fn SDL_SetTextureBlendMode(texture: Texture, mode: u32) -> bool;
    pub fn SDL_SetTextureColorMod(texture: Texture, r: u8, g: u8, b: u8) -> bool;
    pub fn SDL_SetTextureAlphaMod(texture: Texture, alpha: u8) -> bool;
    pub fn SDL_SetRenderDrawColor(renderer: Renderer, r: u8, g: u8, b: u8, a: u8) -> bool;
    pub fn SDL_SetRenderDrawBlendMode(renderer: Renderer, mode: u32) -> bool;
    pub fn SDL_RenderClear(renderer: Renderer) -> bool;
    pub fn SDL_RenderTexture(renderer: Renderer, texture: Texture, src: *const FRect, dst: *const FRect) -> bool;
    pub fn SDL_RenderFillRect(renderer: Renderer, rect: *const FRect) -> bool;
    pub fn SDL_RenderLine(renderer: Renderer, x1: f32, y1: f32, x2: f32, y2: f32) -> bool;
    pub fn SDL_RenderPresent(renderer: Renderer) -> bool;
    pub fn SDL_RenderReadPixels(renderer: Renderer, rect: *const c_void) -> *mut Surface;
    pub fn SDL_ConvertSurface(surface: *mut Surface, format: u32) -> *mut Surface;
    pub fn SDL_DestroySurface(surface: *mut Surface);
    pub fn SDL_PollEvent(event: *mut Event) -> bool;
    pub fn SDL_PushEvent(event: *mut Event) -> bool;
    pub fn SDL_ConvertEventToRenderCoordinates(renderer: Renderer, event: *mut Event) -> bool;
    pub fn SDL_SetRenderScale(renderer: Renderer, x: f32, y: f32) -> bool;
}

/// Volume à appliquer à une sortie son : nul quand `OLYMPE_MUET` est défini (essais sur
/// une vraie sortie son sans rien faire entendre).
pub fn volume(normal: f32) -> f32 {
    if std::env::var_os("OLYMPE_MUET").is_some() { 0.0 } else { normal }
}

/// Dernière erreur SDL, précédée de ce qu'on tentait de faire.
pub fn error(what: &str) -> String {
    // SAFETY : SDL_GetError rend toujours une chaîne C valide.
    let message = unsafe { CStr::from_ptr(SDL_GetError()) };
    format!("{what} : {}", message.to_string_lossy())
}
