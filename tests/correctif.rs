//! Le correctif 2.1, quand les fichiers de référence sont là (`reference/correctif_2.1`).

use std::fs;
use std::path::Path;
use zeus::wise::Delta;

#[test]
fn deltas_rebuild_the_patched_files() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("reference/correctif_2.1");
    if !folder.exists() {
        return;
    }
    for name in ["Zeus_MM.eng", "sierra.inf", "Zeus event message categories.txt"] {
        let delta = fs::read(folder.join("differentiels").join(format!("{name}.delta"))).unwrap();
        let old = fs::read(folder.join("avant").join(name)).unwrap();
        let new = fs::read(folder.join("apres").join(name)).unwrap();
        let rebuilt = Delta::parse(&delta).unwrap().apply(&old).unwrap_or_else(|e| panic!("{name} : {e}"));
        assert!(rebuilt == new, "{name}");
    }
}
