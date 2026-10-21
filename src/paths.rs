//! Emplacements du jeu sur la machine, selon les usages de chaque système.

use std::path::PathBuf;

/// Nom du dossier du moteur dans les emplacements de l'utilisateur.
const APPLICATION: &str = "olympe";

/// Dossier où l'installateur range les données du jeu et où le moteur les cherche :
/// `$OLYMPE_DONNEES` s'il est défini, sinon l'emplacement habituel du système
/// (`~/.local/share/olympe` sous Linux, `~/Library/Application Support/olympe` sous
/// macOS, `%APPDATA%\olympe` sous Windows).
pub fn data_dir() -> PathBuf {
    let var = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty()).map(PathBuf::from);
    if let Some(chosen) = var("OLYMPE_DONNEES") {
        return chosen;
    }
    let base = if cfg!(windows) {
        var("APPDATA")
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|home| home.join("Library").join("Application Support"))
    } else {
        var("XDG_DATA_HOME").or_else(|| var("HOME").map(|home| home.join(".local").join("share")))
    };
    base.unwrap_or_else(|| PathBuf::from(".")).join(APPLICATION)
}

/// Dossier des parties enregistrées, à côté des données.
pub fn saves_dir(data: &std::path::Path) -> PathBuf {
    data.join("sauvegardes")
}
