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

/// Fichier qui retient la langue choisie. Il s'écrit dans le dossier de configuration
/// de l'utilisateur ; le fichier homonyme des données du jeu, hérité des versions où la
/// préférence y vivait, reste lu par compatibilité.
const SETTING: &str = "langue";

/// Langue au démarrage : celle retenue dans `<config>/langue`, puis, par compatibilité,
/// celle des données du jeu, sinon celle du système (`LANGUAGE`, `LC_ALL`,
/// `LC_MESSAGES`, `LANG`), sinon le français.
pub fn initial(config: &Path, data: &Path) -> Lang {
    for file in [config.join(SETTING), data.join(SETTING)] {
        if let Some(chosen) = std::fs::read_to_string(file).ok().and_then(|v| Lang::parse(&v)) {
            return chosen;
        }
    }
    ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .flat_map(|value| value.split(':').map(str::to_string).collect::<Vec<_>>())
        .find_map(|value| Lang::parse(&value))
        .unwrap_or(Lang::Fr)
}

/// Retient la langue pour les lancements suivants, dans le dossier de configuration :
/// les données du jeu ne sont jamais écrites ici (CD, dossier partagé, lecture seule).
pub fn remember(config: &Path, lang: Lang) {
    if std::fs::create_dir_all(config).is_ok() {
        let _ = std::fs::write(config.join(SETTING), lang.code());
    }
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

    /// Les textes propres à Olympe s'écrivent en français dans le code : chaque chaîne
    /// passée à `tr` ou `trf!` doit donc avoir son entrée dans les deux catalogues,
    /// sinon elle resterait en français chez l'anglais ou l'espérantophone.
    #[test]
    fn every_translated_key_is_in_the_catalogues() {
        let keys = translated_keys(include_str!("bin/olympe.rs"));
        assert!(!keys.is_empty(), "le scanner ne trouve plus aucune chaîne");
        for file in ["lang/en.tsv", "lang/eo.tsv"] {
            let catalog: std::collections::HashSet<String> = catalog_keys(file).into_iter().collect();
            for key in &keys {
                assert!(catalog.contains(key), "{file} : « {} » n'est pas traduit", key.replace('\n', "\\n"));
            }
        }
    }

    /// Un doublon passe inaperçu (la table d'index garde la dernière ligne) mais égare
    /// celui qui lit ou corrige le catalogue.
    #[test]
    fn catalogues_have_no_duplicate_entries() {
        for file in ["lang/en.tsv", "lang/eo.tsv"] {
            let mut seen = std::collections::HashSet::new();
            for key in catalog_keys(file) {
                assert!(seen.insert(key.clone()), "{file} : « {key} » en double");
            }
        }
    }

    /// Les premières colonnes d'un catalogue, telles qu'elles s'écrivent dans le fichier.
    fn catalog_keys(file: &str) -> Vec<String> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(file);
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .filter_map(|line| line.split('\t').next())
            .map(str::to_string)
            .collect()
    }

    /// Les chaînes passées à `tr` et `trf!` dans `source`, telles que le catalogue les
    /// attend : `\"` et `\\` se résolvent, les autres échappements (dont `\n`) restent
    /// écrits en deux caractères, comme dans les fichiers.
    fn translated_keys(source: &str) -> Vec<String> {
        let mut keys = Vec::new();
        for marker in ["tr(", "trf!("] {
            let mut search = 0;
            while let Some(found) = source[search..].find(marker) {
                let start = search + found;
                search = start + marker.len();
                // « contr( », « entree( »… ne sont pas nos macros.
                if start > 0 && (source.as_bytes()[start - 1] as char).is_alphanumeric() {
                    continue;
                }
                // la chaîne peut commencer à la ligne suivante : « trf!(\n    "…" »
                let mut open = search;
                while source[open..].starts_with(char::is_whitespace) {
                    open += 1;
                }
                let Some(body) = source[open..].strip_prefix('"') else { continue };
                let mut key = String::new();
                let mut chars = body.char_indices();
                while let Some((offset, c)) = chars.next() {
                    match c {
                        '"' => {
                            search = open + 1 + offset;
                            keys.push(std::mem::take(&mut key));
                            break;
                        }
                        '\\' => match chars.next() {
                            Some((_, next @ ('"' | '\\'))) => key.push(next),
                            Some((_, next)) => {
                                key.push('\\');
                                key.push(next);
                            }
                            None => break,
                        },
                        other => key.push(other),
                    }
                }
            }
        }
        keys.sort();
        keys.dedup();
        keys
    }
}
