//! Messages du jeu (`zeus_mm.eng`) : récits des épisodes, conseils, annonces. Même
//! format que dans Caesar III : un en-tête, mille entrées de 80 octets, puis les textes.

use crate::text::cp1252;

const HEADER_SIZE: usize = 24;
const ENTRIES: usize = 1000;
const ENTRY_SIZE: usize = 80;
/// Rang, dans une entrée, des positions du titre, du sous-titre et du contenu.
const TITLE: usize = 17;
const SUBTITLE: usize = 18;
const CONTENT: usize = 19;

pub struct Messages {
    file: Vec<u8>,
}

pub struct Message {
    pub title: String,
    pub subtitle: String,
    /// Paragraphes du contenu, débarrassés des balises de mise en page.
    pub paragraphs: Vec<String>,
}

impl Messages {
    pub fn parse(file: Vec<u8>) -> Result<Messages, String> {
        if !file.starts_with(b"Zeus MM file.") || file.len() < HEADER_SIZE + ENTRIES * ENTRY_SIZE {
            return Err("fichier de messages invalide".into());
        }
        Ok(Messages { file })
    }

    /// Message numéro `id`, s'il a un titre ou un contenu.
    pub fn get(&self, id: usize) -> Option<Message> {
        let entry = self.file.get(HEADER_SIZE + id * ENTRY_SIZE..HEADER_SIZE + (id + 1) * ENTRY_SIZE).filter(|_| id < ENTRIES)?;
        let text_at = |field: usize| -> String {
            let offset = u32::from_le_bytes([entry[field * 4], entry[field * 4 + 1], entry[field * 4 + 2], entry[field * 4 + 3]]) as usize;
            let start = HEADER_SIZE + ENTRIES * ENTRY_SIZE + offset;
            // Une position nulle signifie « pas de texte ».
            let bytes = self.file.get(start..).filter(|_| offset != 0).unwrap_or(&[]);
            bytes.iter().take_while(|&&b| b != 0).map(|&b| cp1252(b)).collect()
        };
        let message = Message { title: text_at(TITLE), subtitle: text_at(SUBTITLE), paragraphs: paragraphs(&text_at(CONTENT)) };
        (!message.title.is_empty() || !message.paragraphs.is_empty()).then_some(message)
    }
}

/// Découpe un texte du jeu en paragraphes : `@P` et `@L` séparent, `@G` suivi d'un
/// nombre désigne une image et `@` suivi d'un chiffre une mise en forme, ignorées ici.
pub fn paragraphs(text: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '@' {
            out.last_mut().unwrap().push(c);
            continue;
        }
        match chars.next() {
            Some('P' | 'L') => out.push(String::new()),
            Some('G') => while chars.next_if(|d| d.is_ascii_digit()).is_some() {},
            Some(d) if d.is_ascii_digit() => {}
            Some(other) => out.last_mut().unwrap().extend(['@', other]),
            None => {}
        }
    }
    out.into_iter().map(|p| p.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|p| !p.is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::paragraphs;

    #[test]
    fn splits_on_paragraph_and_line_marks() {
        assert_eq!(paragraphs("@PUn. @PDeux.@LTrois"), ["Un.", "Deux.", "Trois"]);
    }

    #[test]
    fn drops_image_and_format_marks() {
        assert_eq!(paragraphs("Voici @G12l'image et @3du texte"), ["Voici l'image et du texte"]);
    }

    #[test]
    fn collapses_spaces_and_skips_empty_paragraphs() {
        assert_eq!(paragraphs("@P@P  a   b  @P"), ["a b"]);
    }
}
