//! Lecture des fichiers Ogg Opus : découpage en paquets par `ogg`, décodage par la
//! bibliothèque `libopus` du système.

use crate::ogg::Packets;
use std::ffi::{c_int, c_void};

/// Opus décode toujours à cette fréquence, quelle que soit celle de la source.
pub const SAMPLE_RATE: i32 = 48_000;
/// Plus longue trame possible : 120 ms.
const MAX_FRAME: usize = 5760;

#[link(name = "opus")]
unsafe extern "C" {
    fn opus_decoder_create(sample_rate: i32, channels: c_int, error: *mut c_int) -> *mut c_void;
    fn opus_decode(decoder: *mut c_void, data: *const u8, length: i32, pcm: *mut i16, frame_size: c_int, fec: c_int) -> c_int;
    fn opus_decoder_destroy(decoder: *mut c_void);
}

/// Morceau en cours de décodage.
pub struct Stream {
    packets: Packets,
    decoder: *mut c_void,
    pub channels: usize,
    /// Échantillons de mise en route à jeter au début du morceau.
    skip: usize,
}

impl Stream {
    pub fn open(data: Vec<u8>) -> Result<Stream, String> {
        let mut packets = Packets::new(data);
        let head = packets.next().filter(|p| p.len() >= 19 && p.starts_with(b"OpusHead")).ok_or("en-tête Opus absent")?;
        let channels = head[9] as usize;
        // Famille 0 : mono ou stéréo, sans table de canaux.
        if !(1..=2).contains(&channels) || head[18] != 0 {
            return Err("disposition de canaux non gérée".into());
        }
        let skip = u16::from_le_bytes([head[10], head[11]]) as usize;
        // Le deuxième paquet porte les étiquettes (titre, auteur) : inutile ici.
        packets.next().filter(|p| p.starts_with(b"OpusTags")).ok_or("étiquettes Opus absentes")?;
        let mut error = 0;
        // SAFETY : arguments valides ; le pointeur rendu est vérifié.
        let decoder = unsafe { opus_decoder_create(SAMPLE_RATE, channels as c_int, &mut error) };
        if decoder.is_null() || error != 0 {
            return Err(format!("décodeur Opus refusé (code {error})"));
        }
        Ok(Stream { packets, decoder, channels, skip })
    }

    /// Décode le paquet suivant : échantillons 16 bits entrelacés, ou `None` à la fin.
    pub fn decode(&mut self) -> Option<Vec<i16>> {
        loop {
            let packet = self.packets.next()?;
            let mut pcm = vec![0i16; MAX_FRAME * self.channels];
            // SAFETY : `pcm` a la place de MAX_FRAME échantillons par canal.
            let frames = unsafe { opus_decode(self.decoder, packet.as_ptr(), packet.len() as i32, pcm.as_mut_ptr(), MAX_FRAME as c_int, 0) };
            // Un paquet abîmé est sauté.
            if frames <= 0 {
                continue;
            }
            let skipped = self.skip.min(frames as usize);
            self.skip -= skipped;
            pcm.truncate(frames as usize * self.channels);
            pcm.drain(..skipped * self.channels);
            if !pcm.is_empty() {
                return Some(pcm);
            }
        }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        // SAFETY : le décodeur vient de `opus_decoder_create` et n'est détruit qu'ici.
        unsafe { opus_decoder_destroy(self.decoder) }
    }
}
