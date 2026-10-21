//! Conteneur Ogg : découpe un fichier en paquets, sans vérifier les sommes de contrôle.

const PAGE_HEADER: usize = 27;

/// Paquets successifs d'un fichier Ogg à un seul flux.
pub struct Packets {
    data: Vec<u8>,
    /// Début de la prochaine page à lire.
    next_page: usize,
    /// Longueurs des segments de la page courante qui restent à lire.
    segments: Vec<u8>,
    segment: usize,
    /// Position, dans le fichier, du prochain segment.
    body: usize,
}

impl Packets {
    pub fn new(data: Vec<u8>) -> Packets {
        Packets { data, next_page: 0, segments: Vec::new(), segment: 0, body: 0 }
    }

    /// Passe à la page suivante ; `false` à la fin du fichier ou sur une page abîmée.
    fn open_page(&mut self) -> bool {
        let Some(header) = self.data.get(self.next_page..self.next_page + PAGE_HEADER) else { return false };
        if !header.starts_with(b"OggS") {
            return false;
        }
        let count = header[26] as usize;
        let table = self.next_page + PAGE_HEADER;
        let Some(segments) = self.data.get(table..table + count) else { return false };
        self.segments = segments.to_vec();
        self.segment = 0;
        self.body = table + count;
        self.next_page = self.body + self.segments.iter().map(|&s| s as usize).sum::<usize>();
        true
    }
}

impl Iterator for Packets {
    type Item = Vec<u8>;

    /// Un paquet est une suite de segments ; un segment de moins de 255 octets le
    /// termine. Il peut se poursuivre sur la page suivante.
    fn next(&mut self) -> Option<Vec<u8>> {
        let mut packet = Vec::new();
        loop {
            if self.segment >= self.segments.len() && !self.open_page() {
                return None;
            }
            while self.segment < self.segments.len() {
                let length = self.segments[self.segment] as usize;
                packet.extend_from_slice(self.data.get(self.body..self.body + length)?);
                self.body += length;
                self.segment += 1;
                if length < 255 {
                    return Some(packet);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Packets;

    /// Page Ogg minimale portant les segments donnés.
    fn page(segments: &[&[u8]]) -> Vec<u8> {
        let mut page = b"OggS".to_vec();
        page.extend([0; 22]);
        page.push(segments.len() as u8);
        page.extend(segments.iter().map(|s| s.len() as u8));
        segments.iter().for_each(|s| page.extend_from_slice(s));
        page
    }

    #[test]
    fn a_short_segment_ends_a_packet() {
        let file = page(&[b"abc", b"de"]);
        assert_eq!(Packets::new(file).collect::<Vec<_>>(), [b"abc".to_vec(), b"de".to_vec()]);
    }

    #[test]
    fn a_full_segment_continues_on_the_next_page() {
        let full = [7u8; 255];
        let mut file = page(&[&full]);
        file.extend(page(&[b"fin"]));
        let packets: Vec<Vec<u8>> = Packets::new(file).collect();
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].len(), 258);
        assert!(packets[0].ends_with(b"fin"));
    }

    #[test]
    fn a_damaged_file_yields_nothing() {
        assert_eq!(Packets::new(b"pas un fichier ogg".to_vec()).count(), 0);
    }
}
