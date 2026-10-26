//! Emplacements du jeu sur la machine, selon les usages de chaque système.

use std::path::PathBuf;

/// Nom du dossier du moteur dans les emplacements de l'utilisateur.
const APPLICATION: &str = "olympe";

/// Variable d'environnement non vide, en chemin.
fn env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|value| !value.is_empty()).map(PathBuf::from)
}

/// Dossier où l'installateur range les données du jeu et où le moteur les cherche :
/// `$OLYMPE_DONNEES` s'il est défini, sinon l'emplacement habituel du système
/// (`~/.local/share/olympe` sous Linux, `~/Library/Application Support/olympe` sous
/// macOS, `%APPDATA%\olympe` sous Windows).
pub fn data_dir() -> PathBuf {
    if let Some(chosen) = env("OLYMPE_DONNEES") {
        return chosen;
    }
    let base = if cfg!(windows) {
        env("APPDATA")
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|home| home.join("Library").join("Application Support"))
    } else {
        env("XDG_DATA_HOME").or_else(|| env("HOME").map(|home| home.join(".local").join("share")))
    };
    base.unwrap_or_else(|| PathBuf::from(".")).join(APPLICATION)
}

/// Dossier des choix de l'utilisateur (langue…) : jamais dans les données du jeu, qui
/// peuvent venir d'un CD, d'un dossier partagé ou d'une installation en lecture seule.
pub fn config_dir() -> PathBuf {
    let base = if cfg!(windows) {
        env("APPDATA")
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|home| home.join("Library").join("Application Support"))
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|home| home.join(".config")))
    };
    base.unwrap_or_else(|| PathBuf::from(".")).join(APPLICATION)
}

/// Dossier des parties enregistrées, à côté des données.
pub fn saves_dir(data: &std::path::Path) -> PathBuf {
    data.join("sauvegardes")
}
