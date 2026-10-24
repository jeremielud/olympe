//! Polices du jeu : `zeus_fonts.sg3` contient 18 polices de 134 glyphes, rangés
//! dans le même ordre que dans Caesar III.

/// Nombre de glyphes par police.
pub const GLYPHS: usize = 134;

/// Polices, désignées par leur première image dans le catalogue.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Font(pub usize);

impl Font {
    /// Toutes les polices du catalogue, dans l'ordre des images.
    pub const ALL: [Font; 17] = [
        Font(201),
        Font(335),
        Font(469),
        Font(603),
        Font(737),
        Font(871),
        Font(1005),
        Font(1139),
        Font(1273),
        Font(1541),
        Font(1675),
        Font(1809),
        Font(1943),
        Font(2077),
        Font(2211),
        Font(2345),
        Font(2479),
    ];

    /// Image du caractère dans le catalogue, s'il a un glyphe.
    pub fn image(self, c: char) -> Option<usize> {
        glyph(c).map(|index| self.0 + index)
    }
}

/// Rang du glyphe d'un caractère (0 = « A »).
fn glyph(c: char) -> Option<usize> {
    const PUNCTUATION: &str = "!\"%*()-+=:;'?\\/,._";
    // Chaque voyelle minuscule : tréma, aigu, circonflexe, grave (relevé sur les glyphes).
    const ACCENTS: &str = "äáâàëéêèïíîìöóôòüúûùçñæßÄÉÜÑÆŒœÁÂÀÊÈÍÎÌÓÔÒÖÚÛÙ¡¿^¨ÅØåø";
    let index = match c {
        'A'..='Z' => c as usize - 'A' as usize,
        'a'..='z' => 26 + c as usize - 'a' as usize,
        '1'..='9' => 52 + c as usize - '1' as usize,
        '0' => 61,
        // Apostrophe typographique et majuscules sans glyphe propre.
        '’' => return glyph('\''),
        // Guillemets français : les polices n'ont que le guillemet droit.
        '«' | '»' => return glyph('"'),
        'Ç' => return glyph('ç'),
        'Ë' => return glyph('ë'),
        'Ï' => return glyph('ï'),
        _ => match PUNCTUATION.chars().position(|p| p == c) {
            Some(i) => 62 + i,
            None => 80 + ACCENTS.chars().position(|a| a == c)?,
        },
    };
    Some(index)
}

#[cfg(test)]
mod tests {
    use super::Font;

    #[test]
    fn letters_digits_and_accents_have_glyphs() {
        let font = Font(201);
        assert_eq!(font.image('A'), Some(201));
        assert_eq!(font.image('a'), Some(227));
        assert_eq!(font.image('1'), Some(253));
        assert_eq!(font.image('0'), Some(262));
        assert!(font.image('é').is_some());
        // Dans la police, le circonflexe précède le grave.
        assert_eq!(font.image('â'), Some(201 + 82));
        assert_eq!(font.image('à'), Some(201 + 83));
        assert_eq!(font.image('è'), Some(201 + 87));
        assert_eq!(font.image(' '), None);
    }

    #[test]
    fn typographic_marks_fall_back_on_plain_ones() {
        let font = Font(201);
        assert_eq!(font.image('’'), font.image('\''));
        assert_eq!(font.image('«'), font.image('"'));
    }
}
