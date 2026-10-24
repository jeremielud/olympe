//! Langues d'Olympe : français (langue des sources), anglais et espéranto.
//!
//! Les textes propres à Olympe sont écrits en français dans le code ; `tr` les traduit
//! d'après les catalogues `lang/en.tsv` et `lang/eo.tsv` (une ligne par texte : le
//! français, une tabulation, la traduction). Un texte absent du catalogue reste en
//! français. Les gabarits gardent leurs trous `{}` (ou `{0}`, `{1}`… pour changer
//! l'ordre dans la traduction) ; `trf!` les remplit.
//!
//! Les textes du jeu viennent de ses fichiers de données, dans la langue de
//! l'exemplaire installé. Une langue peut avoir son propre dossier de données
//! (`<données>/en`, `<données>/eo`) ; l'espéranto, que le jeu n'a jamais eu, reçoit en
//! outre les traductions d'Olympe pour les groupes de textes les plus visibles
//! (`lang/eo-jeu.tsv` : groupe, numéro, texte).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Fr,
    En,
    Eo,
}

impl Lang {
    pub const ALL: [Lang; 3] = [Lang::Fr, Lang::En, Lang::Eo];

    /// Code ISO de la langue (aussi le nom de son dossier de données).
    pub fn code(self) -> &'static str {
        match self {
            Lang::Fr => "fr",
            Lang::En => "en",
            Lang::Eo => "eo",
        }
    }

    /// Nom de la langue dans cette langue.
    pub fn name(self) -> &'static str {
        match self {
            Lang::Fr => "Français",
            Lang::En => "English",
            Lang::Eo => "Esperanto",
        }
    }

    /// Langue désignée par un code ou un nom (« en », « english », « eo_XX.UTF-8 »…).
    pub fn parse(value: &str) -> Option<Lang> {
        let value = value.trim().to_lowercase();
        let code = value.split(['_', '.', '-', '@']).next().unwrap_or("");
        match code {
            "fr" | "français" | "francais" | "french" | "franca" => Some(Lang::Fr),
            "en" | "english" | "anglais" | "angla" => Some(Lang::En),
            "eo" | "esperanto" | "espéranto" => Some(Lang::Eo),
            _ => None,
        }
    }

    pub fn next(self) -> Lang {
        match self {
            Lang::Fr => Lang::En,
            Lang::En => Lang::Eo,
            Lang::Eo => Lang::Fr,
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn current() -> Lang {
    Lang::ALL[CURRENT.load(Ordering::Relaxed) as usize % Lang::ALL.len()]
}

pub fn set(lang: Lang) {
    CURRENT.store(Lang::ALL.iter().position(|&l| l == lang).unwrap_or(0) as u8, Ordering::Relaxed);
}

/// Langue au démarrage : celle retenue dans `<données>/langue`, sinon celle du système
/// (`LANGUAGE`, `LC_ALL`, `LC_MESSAGES`, `LANG`), sinon le français.
pub fn initial(data: &Path) -> Lang {
    if let Some(chosen) = std::fs::read_to_string(data.join(SETTING)).ok().and_then(|v| Lang::parse(&v)) {
        return chosen;
    }
    ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .flat_map(|value| value.split(':').map(str::to_string).collect::<Vec<_>>())
        .find_map(|value| Lang::parse(&value))
        .unwrap_or(Lang::Fr)
}

/// Fichier, dans le dossier des données, qui retient la langue choisie.
const SETTING: &str = "langue";

/// Retient la langue pour les lancements suivants.
pub fn remember(data: &Path, lang: Lang) {
    let _ = std::fs::write(data.join(SETTING), lang.code());
}

fn catalog(lang: Lang) -> Option<&'static HashMap<&'static str, &'static str>> {
    static EN: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    static EO: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    let parse = |file: &'static str| {
        file.lines()
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| line.split_once('\t'))
            .collect::<HashMap<_, _>>()
    };
    match lang {
        Lang::Fr => None,
        Lang::En => Some(EN.get_or_init(|| parse(include_str!("../lang/en.tsv")))),
        Lang::Eo => Some(EO.get_or_init(|| parse(include_str!("../lang/eo.tsv")))),
    }
}

/// Le texte dans la langue courante (les sauts de ligne s'écrivent `\n` au catalogue).
pub fn tr(french: &str) -> String {
    let key = french.replace('\n', "\\n");
    match catalog(current()).and_then(|c| c.get(key.as_str())) {
        Some(translated) => translated.replace("\\n", "\n"),
        None => french.to_string(),
    }
}

/// Remplit un gabarit : `{}` prend l'argument suivant, `{n}` l'argument de rang n.
pub fn fill(template: &str, args: &[String]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut next = 0;
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) if after[..close].chars().all(|c| c.is_ascii_digit()) => {
                let index = if close == 0 {
                    next += 1;
                    next - 1
                } else {
                    after[..close].parse().unwrap_or(usize::MAX)
                };
                out.push_str(args.get(index).map_or("", String::as_str));
                rest = &after[close + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `trf!("Sanctuaire de {}", nom)` : traduit le gabarit, puis le remplit.
#[macro_export]
macro_rules! trf {
    ($template:expr $(, $arg:expr)* $(,)?) => {
        $crate::lang::fill(&$crate::lang::tr($template), &[$(($arg).to_string()),*])
    };
}

/// Remplacements de textes du jeu pour la langue courante : (groupe, numéro) → texte.
pub fn game_text_overrides() -> Option<&'static HashMap<(usize, usize), &'static str>> {
    static EO: OnceLock<HashMap<(usize, usize), &'static str>> = OnceLock::new();
    match current() {
        Lang::Eo => Some(EO.get_or_init(|| {
            include_str!("../lang/eo-jeu.tsv")
                .lines()
                .filter(|line| !line.starts_with('#'))
                .filter_map(|line| {
                    let mut parts = line.splitn(3, '\t');
                    Some(((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?), parts.next()?))
                })
                .collect()
        })),
        _ => None,
    }
}

/// Fichier de données du jeu dans la langue courante : celui du dossier de la langue
/// s'il existe (le nom sans tenir compte de la casse), sinon celui de l'installation.
pub fn data_file(data: &Path, name: &str) -> PathBuf {
    let folder = data.join(current().code());
    if let Ok(entries) = std::fs::read_dir(&folder) {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().eq_ignore_ascii_case(name) {
                return entry.path();
            }
        }
    }
    data.join(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_fill_in_order_or_by_rank() {
        assert_eq!(fill("{} de {}", &["a".into(), "b".into()]), "a de b");
        assert_eq!(fill("{1} of {0}", &["a".into(), "b".into()]), "b of a");
        assert_eq!(fill("pas de trou {x}", &[]), "pas de trou {x}");
    }

    #[test]
    fn languages_are_recognised_by_code_or_name() {
        assert_eq!(Lang::parse("eo_XX.UTF-8"), Some(Lang::Eo));
        assert_eq!(Lang::parse("en_GB:fr"), Some(Lang::En));
        assert_eq!(Lang::parse("Français"), Some(Lang::Fr));
        assert_eq!(Lang::parse("de_DE"), None);
    }

    #[test]
    fn catalogues_keep_their_template_holes() {
        for lang in [Lang::En, Lang::Eo] {
            for (french, translated) in catalog(lang).unwrap() {
                let holes = |s: &str| s.matches("{}").count() + (0..9).filter(|i| s.contains(&format!("{{{i}}}"))).count();
                assert_eq!(holes(french), holes(translated), "{lang:?} : {french}");
            }
        }
    }
}
