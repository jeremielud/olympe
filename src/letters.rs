//! Lettres d'événements du jeu (`model/zeus eventmsg.txt`) : un titre et un texte par
//! événement, avec des champs à remplir (`[city_name]`, `[amount]`…).

use std::collections::HashMap;

pub struct Letters(HashMap<String, String>);

impl Letters {
    /// Lit les lignes `PHRASE_clé "texte"` du fichier, en Windows-1252.
    pub fn parse(file: &[u8]) -> Letters {
        let text: String = file.iter().map(|&b| crate::text::cp1252(b)).collect();
        let mut phrases = HashMap::new();
        for line in text.lines() {
            let line = line.trim_start();
            let Some(rest) = line.strip_prefix("PHRASE_") else { continue };
            let Some(space) = rest.find(char::is_whitespace) else { continue };
            let value = rest[space..].trim();
            if let Some(value) = value.strip_prefix('"').and_then(|v| v.rsplit_once('"')).map(|(v, _)| v) {
                phrases.insert(rest[..space].to_string(), value.to_string());
            }
        }
        Letters(phrases)
    }

    /// Titre et texte d'un événement (clés sans `PHRASE_`), champs remplis ; rien si
    /// le texte manque ou ne dit rien (« N/A »).
    pub fn letter(&self, title: &str, body: &str, fields: &[(&str, &str)]) -> Option<(String, String)> {
        let title = self.0.get(title)?;
        let text = self.0.get(body)?;
        if text.is_empty() || text.eq_ignore_ascii_case("n/a") {
            return None;
        }
        Some((fill(title, fields), fill(text, fields)))
    }
}

/// Remplace les champs `[nom]` ; un champ inconnu disparaît avec l'espace qui le suit.
pub fn fill(text: &str, fields: &[(&str, &str)]) -> String {
    let mut out = text.to_string();
    for (name, value) in fields {
        out = out.replace(&format!("[{name}]"), value);
    }
    while let Some(start) = out.find('[') {
        let Some(end) = out[start..].find(']') else { break };
        let mut end = start + end + 1;
        if out[end..].starts_with(' ') {
            end += 1;
        }
        out.replace_range(start..end, "");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_fill_their_fields() {
        let letters = Letters::parse(b"PHRASE_gift_title \t\"Don de [city_name]\"\nPHRASE_gift_granted \"Voici [amount] [item] [unknown] pour vous.\"\n");
        let (title, text) = letters.letter("gift_title", "gift_granted", &[("city_name", "Sparte"), ("amount", "4"), ("item", "vin")]).unwrap();
        assert_eq!(title, "Don de Sparte");
        assert_eq!(text, "Voici 4 vin pour vous.");
    }
}
