//! Der Hauptzweig heißt in der Datei immer „Haupt“ (`MAIN_BRANCH`, Kennung), angezeigt wird er in
//! der Sprache der Oberfläche („Main“ auf Englisch, wie in Fusion). Ein neuer Zweig darf weder
//! „Haupt“ noch den angezeigten Namen tragen, sonst gäbe es zwei gleich aussehende Einträge.

use planora_feature::{Document, FeatureKind, Snapshot};
use planora_i18n::{english, german, with_lang};
use planora_math::DVec3;
use planora_project::docstore::*;
use planora_project::fsutil::new_id;
use std::path::PathBuf;

fn dir() -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-main-label-{}", new_id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join("Teil.pcad")
}

fn snap() -> Snapshot {
    let mut d = Document::new();
    d.add_feature(
        "Q".to_string(),
        FeatureKind::Box {
            size: DVec3::splat(1.0),
            origin: DVec3::ZERO,
        },
    )
    .unwrap();
    d.snapshot()
}

#[test]
fn the_main_branch_is_shown_in_the_ui_language() {
    assert_eq!(MAIN_BRANCH, "Haupt", "Kennung in der Datei bleibt");
    assert_eq!(with_lang(&german(), || branch_label(MAIN_BRANCH)), "Haupt");
    assert_eq!(with_lang(&english(), || branch_label(MAIN_BRANCH)), "Main");
    // Andere Zweige bleiben, wie sie heißen.
    assert_eq!(
        with_lang(&english(), || branch_label("Variante A")),
        "Variante A"
    );
}

#[test]
fn a_new_branch_may_not_look_like_the_main_branch() {
    let p = dir();
    let mut st = DocStore::create(&p, &snap(), "").unwrap();
    st.save(Some(1), &snap(), "", None).unwrap();
    for bad in ["Main", "main", "Haupt", "haupt"] {
        let r = with_lang(&english(), || st.save(Some(1), &snap(), "", Some(bad)));
        assert!(r.is_err(), "{bad:?}");
    }
    assert!(st.save(Some(1), &snap(), "", Some("Mainline")).is_ok());
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}
