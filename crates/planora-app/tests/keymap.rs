//! Tastenbelegung (Keymap): Voreinstellung wie Fusion für die vorhandenen Aktionen,
//! Doppelbelegung je Bereich, Ändern/Tauschen/Zurücksetzen, Texte zum Anzeigen und Speichern,
//! Laden der Abweichungen aus der Konfiguration (mit Hinweisen) und Speichern über den
//! Arbeitsbereich (nur im Temp-Ordner).

mod common;

use egui::Key;
use planora_app::keymap::text::{
    binding_text, chord_text, gesture_text, is_modifier_key, parse_binding,
};
use planora_app::keymap::{
    Action, Area, Binding, Gesture, KeyChord, Keymap, MouseButton, SetResult,
};
use planora_app::workspace::Workspace;
use planora_project::config::Config;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

fn chord(key: Key, ctrl: bool, shift: bool, alt: bool) -> KeyChord {
    KeyChord {
        key,
        ctrl,
        shift,
        alt,
    }
}

fn key(k: Key) -> Binding {
    Binding::Key(chord(k, false, false, false))
}

fn ctrl(k: Key) -> Binding {
    Binding::Key(chord(k, true, false, false))
}

fn ctrl_shift(k: Key) -> Binding {
    Binding::Key(chord(k, true, true, false))
}

fn drag(button: MouseButton, shift: bool) -> Binding {
    Binding::Mouse(Gesture::Drag {
        button,
        ctrl: false,
        shift,
        alt: false,
    })
}

fn wheel() -> Binding {
    Binding::Mouse(Gesture::Wheel {
        ctrl: false,
        shift: false,
        alt: false,
    })
}

fn ctrl_wheel() -> Binding {
    Binding::Mouse(Gesture::Wheel {
        ctrl: true,
        shift: false,
        alt: false,
    })
}

fn overrides(entries: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
    entries
        .iter()
        .map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect()))
        .collect()
}

#[test]
fn defaults_match_the_spec_table_for_existing_actions() {
    let km = Keymap::defaults();
    let want: [(Action, Vec<Binding>); 11] = [
        (Action::New, vec![ctrl(Key::N)]),
        (Action::Projects, vec![ctrl(Key::O)]),
        (Action::Save, vec![ctrl(Key::S)]),
        (Action::Close, vec![ctrl(Key::W)]),
        (Action::Export, vec![ctrl_shift(Key::E)]),
        (Action::FeedbackOpen, vec![ctrl_shift(Key::R)]),
        (Action::Undo, vec![ctrl(Key::Z)]),
        (Action::Redo, vec![ctrl(Key::Y), ctrl_shift(Key::Z)]),
        (
            Action::ViewPan,
            vec![
                drag(MouseButton::Middle, false),
                drag(MouseButton::Left, true),
            ],
        ),
        (
            Action::ViewOrbit,
            vec![
                drag(MouseButton::Middle, true),
                drag(MouseButton::Left, false),
            ],
        ),
        (Action::ViewZoom, vec![wheel(), ctrl_wheel()]),
    ];
    // Die Aktionen für überall; die Modellaktionen prüft `keymap_model.rs`.
    let all = Action::ALL.iter().filter(|a| a.area() == Area::All).count();
    assert_eq!(all, want.len());
    for (a, b) in &want {
        assert_eq!(km.bindings(*a), b.as_slice(), "{a:?}");
        assert_eq!(a.area(), Area::All, "{a:?}");
        assert!(km.is_default(*a));
    }
    // Ids und Anzeigenamen sind eindeutig und nicht leer.
    let mut ids: Vec<_> = Action::ALL.iter().map(|a| a.id()).collect();
    let mut labels: Vec<_> = Action::ALL.iter().map(|a| a.label()).collect();
    ids.sort_unstable();
    ids.dedup();
    labels.sort_unstable();
    labels.dedup();
    assert_eq!(ids.len(), Action::ALL.len());
    assert_eq!(labels.len(), Action::ALL.len());
    assert!(ids.iter().all(|i| !i.is_empty() && i.is_ascii()));
    assert_eq!(Action::Save.id(), "datei.speichern");
    // Die Voreinstellung selbst hat keine Doppelbelegung.
    for &a in Action::ALL {
        for b in km.bindings(a) {
            assert_eq!(km.conflict_for(a, b), None, "{a:?} {b:?}");
        }
    }
    assert_eq!(Area::All.label(), "Überall");
    assert_eq!(Area::Model.label(), "Modell");
    assert_eq!(Area::Sketch.label(), "Skizze");
    assert_eq!(Area::Drawing.label(), "Zeichnung");
    assert_eq!(km.text(Action::Redo), "Strg+Y / Strg+Umschalt+Z");
    assert_eq!(km.text(Action::Export), "Strg+Umschalt+E");
    assert_eq!(km.text(Action::FeedbackOpen), "Strg+Umschalt+R");
}

#[test]
fn conflict_within_area_and_with_all() {
    let km = Keymap::defaults();
    // Eine Taste, die eine Aktion von „Überall“ schon hat, ist in jedem Bereich belegt.
    for area in [Area::All, Area::Model, Area::Sketch, Area::Drawing] {
        assert!(Keymap::areas_overlap(area, Area::All), "{area:?}");
        assert!(Keymap::areas_overlap(Area::All, area), "{area:?}");
        assert!(Keymap::areas_overlap(area, area), "{area:?}");
    }
    // Verschiedene Bereiche (ohne „Überall“) überschneiden sich nicht: gleiche Taste erlaubt.
    assert!(!Keymap::areas_overlap(Area::Sketch, Area::Model));
    assert!(!Keymap::areas_overlap(Area::Sketch, Area::Drawing));
    assert!(!Keymap::areas_overlap(Area::Model, Area::Drawing));
    // Mit den vorhandenen Aktionen (alle „Überall“): Strg+S ist Speichern.
    assert_eq!(
        km.conflict_for(Action::Undo, &ctrl(Key::S)),
        Some(Action::Save)
    );
    assert_eq!(km.conflict_for(Action::Undo, &ctrl(Key::Q)), None);
    // Die eigene Taste ist kein Konflikt.
    assert_eq!(km.conflict_for(Action::Save, &ctrl(Key::S)), None);
    // Umschalttasten zählen genau: Strg+Umschalt+S ist frei.
    assert_eq!(km.conflict_for(Action::Undo, &ctrl_shift(Key::S)), None);
    // Suche der Aktion zu einer Taste: Bereich zuerst, dann „Überall“.
    assert_eq!(
        km.action_for_key(Area::Sketch, &chord(Key::S, true, false, false)),
        Some(Action::Save)
    );
    assert_eq!(
        km.action_for_key(Area::Model, &chord(Key::Z, true, true, false)),
        Some(Action::Redo)
    );
    assert_eq!(
        km.action_for_key(Area::Model, &chord(Key::Z, true, false, false)),
        Some(Action::Undo)
    );
    assert_eq!(
        km.action_for_key(Area::Model, &chord(Key::Z, false, false, true)),
        None
    );
    assert!(km.matches_key(Action::Redo, &chord(Key::Y, true, false, false)));
    assert!(km.matches_key(Action::Redo, &chord(Key::Z, true, true, false)));
    assert!(!km.matches_key(Action::Undo, &chord(Key::Z, true, true, false)));
}

#[test]
fn set_refuses_on_conflict_and_changes_nothing() {
    let mut km = Keymap::defaults();
    let before = km.clone();
    assert_eq!(
        km.set(Action::Undo, 0, ctrl(Key::S)),
        SetResult::Conflict(Action::Save)
    );
    assert_eq!(km, before);
    // Gelingt: nur die geänderte Belegung, die zweite von Wiederholen bleibt.
    assert_eq!(km.set(Action::Redo, 0, key(Key::F5)), SetResult::Ok);
    assert_eq!(
        km.bindings(Action::Redo),
        [key(Key::F5), ctrl_shift(Key::Z)]
    );
    assert!(!km.is_default(Action::Redo));
    // Strg+Y ist danach frei.
    assert_eq!(km.conflict_for(Action::Undo, &ctrl(Key::Y)), None);
    // Eine Stelle hinter dem Ende hängt eine Belegung an.
    assert_eq!(km.set(Action::Save, 5, ctrl(Key::K)), SetResult::Ok);
    assert_eq!(km.bindings(Action::Save), [ctrl(Key::S), ctrl(Key::K)]);
    // Die eigene andere Belegung erneut setzen: kein Konflikt, keine doppelte Zeile.
    assert_eq!(km.set(Action::Redo, 0, ctrl_shift(Key::Z)), SetResult::Ok);
    assert_eq!(km.bindings(Action::Redo), [ctrl_shift(Key::Z)]);
    // Auf die Voreinstellung zurück ändern zählt wieder als Voreinstellung.
    assert_eq!(km.set(Action::Redo, 1, ctrl(Key::Y)), SetResult::Ok);
    assert_eq!(
        km.bindings(Action::Redo),
        [ctrl_shift(Key::Z), ctrl(Key::Y)]
    );
    assert_eq!(km.set(Action::Redo, 0, ctrl(Key::Y)), SetResult::Ok);
    assert_eq!(km.set(Action::Redo, 1, ctrl_shift(Key::Z)), SetResult::Ok);
    assert!(km.is_default(Action::Redo));
    // Gesten ebenso.
    assert_eq!(
        km.set(Action::ViewZoom, 0, drag(MouseButton::Middle, false)),
        SetResult::Conflict(Action::ViewPan)
    );
}

#[test]
fn swap_gives_the_key_to_the_new_action_and_frees_the_old_one() {
    let mut km = Keymap::defaults();
    km.swap(Action::Undo, 0, ctrl(Key::S));
    assert_eq!(km.bindings(Action::Undo), [ctrl(Key::S)]);
    // Speichern verliert die Taste ersatzlos (bleibt ohne Belegung in der Liste).
    assert!(km.bindings(Action::Save).is_empty());
    assert_eq!(km.text(Action::Save), "–");
    // Strg+Z ist jetzt frei, keine Taste wurde verschluckt.
    assert_eq!(km.conflict_for(Action::Save, &ctrl(Key::Z)), None);
    assert_eq!(
        km.action_for_key(Area::Model, &chord(Key::S, true, false, false)),
        Some(Action::Undo)
    );
    // Eine von zwei Belegungen wegnehmen: die andere bleibt.
    km.swap(Action::Close, 0, ctrl(Key::Y));
    assert_eq!(km.bindings(Action::Redo), [ctrl_shift(Key::Z)]);
    assert_eq!(km.bindings(Action::Close), [ctrl(Key::Y)]);
    // Ohne Konflikt verhält sich Tauschen wie Setzen.
    km.swap(Action::Save, 0, key(Key::F2));
    assert_eq!(km.bindings(Action::Save), [key(Key::F2)]);
}

#[test]
fn tastes_and_gestures_never_conflict_with_each_other() {
    let mut km = Keymap::defaults();
    // Eine Taste neben vorhandenen Gesten und eine Geste neben vorhandenen Tasten.
    assert_eq!(km.set(Action::ViewZoom, 2, ctrl(Key::Q)), SetResult::Ok);
    let g = Binding::Mouse(Gesture::Wheel {
        ctrl: false,
        shift: true,
        alt: false,
    });
    assert_eq!(km.set(Action::Save, 1, g), SetResult::Ok);
    for &a in Action::ALL {
        for b in km.bindings(a) {
            assert_eq!(km.conflict_for(a, b), None, "{a:?} {b:?}");
        }
    }
    // Gleiche Taste, andere Geste: Rad mit Umschalt ist nicht Rad ohne Umschalt.
    assert_eq!(km.conflict_for(Action::Undo, &g), Some(Action::Save));
    assert_eq!(
        km.conflict_for(Action::Undo, &wheel()),
        Some(Action::ViewZoom)
    );
    // Eine Mausgeste ist nie eine Taste.
    assert!(!km.matches_key(Action::ViewZoom, &chord(Key::Q, false, false, false)));
    assert!(km.matches_key(Action::ViewZoom, &chord(Key::Q, true, false, false)));
}

#[test]
fn keys_without_letters_have_german_names_without_plus() {
    for &k in Key::ALL {
        if is_modifier_key(k) {
            continue;
        }
        let t = chord_text(&chord(k, false, false, false));
        assert!(!t.contains('+'), "{k:?}: {t}");
    }
    for (k, name) in [
        (Key::Comma, "Komma"),
        (Key::Minus, "Minus"),
        (Key::Plus, "Plus"),
        (Key::Period, "Punkt"),
        (Key::Questionmark, "Fragezeichen"),
        (Key::OpenBracket, "Eckige Klammer auf"),
        (Key::Colon, "Doppelpunkt"),
        (Key::Slash, "Schrägstrich"),
    ] {
        assert_eq!(chord_text(&chord(k, false, false, false)), name);
    }
    assert_eq!(
        chord_text(&chord(Key::Plus, true, false, false)),
        "Strg+Plus"
    );
    // Kein englischer egui-Name mehr für Satzzeichen und Sondertasten in der Oberfläche.
    for &k in Key::ALL {
        let t = chord_text(&chord(k, false, false, false));
        let english = [
            "Comma",
            "Period",
            "Questionmark",
            "Exclamationmark",
            "Bracket",
            "Colon",
            "Semicolon",
            "Slash",
            "Backtick",
            "Quote",
            "Equals",
            "Pipe",
            "Copy",
            "Paste",
            "Cut",
            "Intl",
        ];
        if !is_modifier_key(k) && k != Key::Backslash {
            assert!(!english.iter().any(|e| t.contains(e)), "{k:?}: {t}");
        }
    }
}

#[test]
fn text_roundtrip_for_every_default_binding() {
    let km = Keymap::defaults();
    for &a in Action::ALL {
        for b in km.bindings(a) {
            let t = binding_text(b);
            assert_eq!(parse_binding(&t), Some(*b), "{a:?}: {t}");
        }
    }
    // Jede Taste mit jeder Kombination der Umschalttasten; eine Umschalttaste allein ist keine
    // Belegung.
    for &k in Key::ALL {
        if is_modifier_key(k) {
            let t = chord_text(&chord(k, false, false, false));
            assert_eq!(parse_binding(&t), None, "{t}");
            assert_eq!(parse_binding(k.name()), None, "{k:?}");
            continue;
        }
        for bits in 0..8u8 {
            let c = chord(k, bits & 1 != 0, bits & 2 != 0, bits & 4 != 0);
            let t = chord_text(&c);
            assert_eq!(parse_binding(&t), Some(Binding::Key(c)), "{t}");
        }
    }
    // Jede Geste.
    for bits in 0..8u8 {
        let (ctrl, shift, alt) = (bits & 1 != 0, bits & 2 != 0, bits & 4 != 0);
        let mut all = vec![Gesture::Wheel { ctrl, shift, alt }];
        for button in [MouseButton::Left, MouseButton::Middle, MouseButton::Right] {
            all.push(Gesture::Drag {
                button,
                ctrl,
                shift,
                alt,
            });
        }
        for g in all {
            let t = gesture_text(&g);
            assert_eq!(parse_binding(&t), Some(Binding::Mouse(g)), "{t}");
        }
    }
    // Feste Texte wie in der Oberfläche.
    assert_eq!(
        chord_text(&chord(Key::E, true, true, false)),
        "Strg+Umschalt+E"
    );
    assert_eq!(chord_text(&chord(Key::Delete, false, false, false)), "Entf");
    assert_eq!(
        chord_text(&chord(Key::Space, false, false, false)),
        "Leertaste"
    );
    assert_eq!(chord_text(&chord(Key::E, false, false, false)), "E");
    assert_eq!(chord_text(&chord(Key::Z, false, false, true)), "Alt+Z");
    assert_eq!(
        gesture_text(&Gesture::Drag {
            button: MouseButton::Middle,
            ctrl: false,
            shift: false,
            alt: false
        }),
        "Mittlere Taste ziehen"
    );
    assert_eq!(
        gesture_text(&Gesture::Drag {
            button: MouseButton::Middle,
            ctrl: false,
            shift: true,
            alt: false
        }),
        "Umschalt + Mittlere Taste ziehen"
    );
    assert_eq!(
        gesture_text(&Gesture::Wheel {
            ctrl: false,
            shift: false,
            alt: false
        }),
        "Mausrad"
    );
    assert_eq!(binding_text(&ctrl_wheel()), "Strg + Mausrad");
    assert_eq!(parse_binding("Strg + Mausrad"), Some(ctrl_wheel()));
    // Ungültiges wird abgelehnt.
    for bad in [
        "",
        "Strg+",
        "+S",
        "Strg+Strg+S",
        "Hyper+S",
        "Quatsch",
        "Strg",
        "S+Strg",
        "Mausrad+Strg",
    ] {
        assert_eq!(parse_binding(bad), None, "„{bad}“");
    }
    // Leerzeichen um die Pluszeichen sind egal.
    assert_eq!(parse_binding(" Strg + S "), Some(ctrl(Key::S)));
}

#[test]
fn from_overrides_ignores_unknown_action_invalid_binding_and_duplicates_and_reports_them() {
    // Ohne Abschnitt: Voreinstellung, keine Hinweise.
    let (km, notes) = Keymap::from_overrides(&BTreeMap::new());
    assert_eq!(km, Keymap::defaults());
    assert!(notes.is_empty());

    let (km, notes) = Keymap::from_overrides(&overrides(&[
        ("gibt.es.nicht", &["Strg+K"]),
        ("datei.neu", &["Strg+Umschalt+N", "Quatsch"]),
        // Doppelbelegung in der Datei: Neu (früher in der Tabelle) gewinnt.
        ("datei.projekte", &["Strg+Umschalt+N", "Strg+P"]),
        // Nur Ungültiges: die Voreinstellung bleibt.
        ("datei.speichern", &["Strg+Strg+S"]),
        // Leere Liste: ohne Belegung.
        ("datei.schliessen", &[]),
        // Eine Taste, die eine Voreinstellung trägt: die Datei gewinnt, Rückgängig verliert sie.
        ("bearbeiten.wiederholen", &["Strg+Z"]),
    ]));
    assert_eq!(km.bindings(Action::New), [ctrl_shift(Key::N)]);
    assert_eq!(km.bindings(Action::Projects), [ctrl(Key::P)]);
    assert_eq!(km.bindings(Action::Save), [ctrl(Key::S)]);
    assert!(km.bindings(Action::Close).is_empty());
    assert_eq!(km.bindings(Action::Redo), [ctrl(Key::Z)]);
    assert!(km.bindings(Action::Undo).is_empty());
    assert_eq!(km.bindings(Action::Export), [ctrl_shift(Key::E)]);
    // Keine Doppelbelegung nach dem Laden.
    for &a in Action::ALL {
        for b in km.bindings(a) {
            assert_eq!(km.conflict_for(a, b), None, "{a:?} {b:?}");
        }
    }
    let all = notes.join("\n");
    assert_eq!(notes.len(), 5, "{all}");
    assert!(all.contains("gibt.es.nicht"), "{all}");
    assert!(all.contains("„Quatsch“"), "{all}");
    assert!(all.contains("„Strg+Strg+S“"), "{all}");
    assert!(
        notes
            .iter()
            .any(|n| n.contains("Strg+Umschalt+N") && n.contains(&Action::Projects.label())),
        "{all}"
    );
    assert!(
        notes
            .iter()
            .any(|n| n.contains("Strg+Z") && n.contains(&Action::Undo.label())),
        "{all}"
    );
}

#[test]
fn to_overrides_writes_only_differences_and_roundtrips() {
    let km = Keymap::defaults();
    assert!(km.to_overrides().is_empty());
    let mut km = Keymap::defaults();
    km.swap(Action::Undo, 0, ctrl(Key::S));
    assert_eq!(
        km.set(Action::ViewOrbit, 1, drag(MouseButton::Right, false)),
        SetResult::Ok
    );
    let o = km.to_overrides();
    assert_eq!(
        o,
        overrides(&[
            (
                "ansicht.drehen",
                &["Umschalt + Mittlere Taste ziehen", "Rechte Taste ziehen"]
            ),
            ("bearbeiten.rueckgaengig", &["Strg+S"]),
            ("datei.speichern", &[]),
        ])
    );
    let (back, notes) = Keymap::from_overrides(&o);
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(back, km);
}

#[test]
fn reset_one_and_all() {
    let mut km = Keymap::defaults();
    km.swap(Action::Undo, 0, ctrl(Key::S));
    assert_eq!(km.set(Action::Redo, 0, key(Key::F4)), SetResult::Ok);
    km.reset(Action::Redo);
    assert!(km.is_default(Action::Redo));
    assert!(!km.is_default(Action::Undo));
    assert!(!km.is_default(Action::Save));
    km.reset_all();
    assert_eq!(km, Keymap::defaults());
    assert_eq!(Keymap::default(), Keymap::defaults());
}

/// Arbeitsbereich ohne Bibliothek, Konfiguration in einem eigenen Temp-Ordner (nie `%APPDATA%`).
fn ws_in_temp(prefix: &str) -> (Workspace, PathBuf, PathBuf) {
    let tmp = common::temp_root(prefix);
    fs::create_dir_all(&tmp).unwrap();
    let mut ws = Workspace::new(None, Config::default());
    let cfg = tmp.join("config.json");
    ws.config_path = cfg.clone();
    (ws, tmp, cfg)
}

#[test]
fn workspace_loads_the_keymap_from_the_config_with_notes() {
    let ws = Workspace::new(
        None,
        Config {
            keys: overrides(&[("datei.neu", &["Strg+Umschalt+N"]), ("weg", &["Strg+K"])]),
            ..Config::default()
        },
    );
    assert_eq!(ws.keymap.bindings(Action::New), [ctrl_shift(Key::N)]);
    assert_eq!(ws.key_notes.len(), 1, "{:?}", ws.key_notes);
    let ws = Workspace::new(None, Config::default());
    assert_eq!(ws.keymap, Keymap::defaults());
    assert!(ws.key_notes.is_empty());
}

#[test]
fn set_keymap_writes_only_keys_in_a_temp_config() {
    let (mut ws, tmp, cfg) = ws_in_temp("planora-tasten-speichern");
    assert_ne!(cfg, Config::config_path(), "nie die echte Datei");
    Config {
        root: Some(tmp.join("Projekte")),
        favorites: vec![tmp.join("Fav")],
        panel_transparency: 20,
        ..Config::default()
    }
    .save_to(&cfg)
    .unwrap();
    let mut km = ws.keymap.clone();
    km.swap(Action::Undo, 0, ctrl(Key::S));
    assert!(ws.set_keymap(km.clone()));
    assert_eq!(ws.keymap, km);
    let disk = Config::read_from(&cfg).unwrap();
    assert_eq!(disk.keys, km.to_overrides());
    assert_eq!(disk.root, Some(tmp.join("Projekte")), "Wurzel bleibt");
    assert_eq!(disk.favorites, [tmp.join("Fav")], "Favoriten bleiben");
    assert_eq!(disk.panel_transparency, 20, "Durchsichtigkeit bleibt");
    // Zurück auf die Voreinstellung entfernt den Abschnitt.
    assert!(ws.set_keymap(Keymap::defaults()));
    let text = fs::read_to_string(&cfg).unwrap();
    assert!(!text.contains("\"keys\""), "{text}");

    // Ohne Schreibrecht: gilt für die Sitzung, die Datei bleibt, wie sie ist.
    let blocker = tmp.join("blockiert");
    fs::write(&blocker, b"x").unwrap();
    ws.config_path = blocker.join("config.json");
    assert!(!ws.set_keymap(km.clone()));
    assert_eq!(ws.keymap, km);
    assert!(ws.keys_pending);
    assert_eq!(fs::read(&blocker).unwrap(), b"x");
    // Das nächste gelungene Schreiben (hier: Favoriten) nimmt die Tasten mit.
    ws.config_path = cfg.clone();
    let mut places = ws.places();
    places.add_favorite(&tmp.join("Teile")).unwrap();
    assert!(ws.save_places(places));
    let disk = Config::read_from(&cfg).unwrap();
    assert_eq!(disk.keys, km.to_overrides());
    assert!(!ws.keys_pending);
    // Ohne Vormerkung bleiben die Tasten auf der Platte (eine andere Instanz darf sie ändern).
    Config::update_at(&cfg, |c| c.keys.clear()).unwrap();
    assert!(ws.try_persist_config());
    assert!(Config::read_from(&cfg).unwrap().keys.is_empty());
    // Vorgemerkt und dann über die Wurzel geschrieben.
    ws.config_path = blocker.join("config.json");
    assert!(!ws.set_keymap(km.clone()));
    ws.config_path = cfg.clone();
    assert!(ws.try_persist_config());
    assert_eq!(Config::read_from(&cfg).unwrap().keys, km.to_overrides());
    // Und über die Durchsichtigkeit.
    ws.config_path = blocker.join("config.json");
    assert!(!ws.set_keymap(Keymap::defaults()));
    ws.config_path = cfg.clone();
    assert!(ws.set_panel_transparency(33));
    let disk = Config::read_from(&cfg).unwrap();
    assert!(disk.keys.is_empty());
    assert_eq!(disk.panel_transparency, 33);

    // Eine unlesbare Datei wird nicht ersetzt.
    fs::write(&cfg, b"{kaputt").unwrap();
    assert!(!ws.set_keymap(km.clone()));
    assert_eq!(fs::read(&cfg).unwrap(), b"{kaputt");
}

#[test]
fn load_notes_are_capped_at_twenty() {
    // 25 ungültige Belegungen bei einer Aktion: 20 Hinweise und „… und 5 weitere“.
    let texts: Vec<String> = (0..25).map(|i| format!("Quatsch{i}")).collect();
    let o = BTreeMap::from([("datei.speichern".to_string(), texts)]);
    let (km, notes) = Keymap::from_overrides(&o);
    assert_eq!(notes.len(), 21, "{notes:?}");
    assert_eq!(notes[20], "… und 5 weitere");
    assert!(notes[19].contains("Quatsch"));
    // Nur Ungültiges: die Voreinstellung gilt.
    assert!(km.is_default(Action::Save));
    // Genau 20: kein Zusatz.
    let texts: Vec<String> = (0..20).map(|i| format!("Quatsch{i}")).collect();
    let o = BTreeMap::from([("datei.speichern".to_string(), texts)]);
    assert_eq!(Keymap::from_overrides(&o).1.len(), 20);
}

#[test]
fn from_overrides_refuses_keys_the_views_need() {
    let (km, notes) = Keymap::from_overrides(&overrides(&[
        ("skizze.linie", &["Entf"]),
        ("skizze.kreis", &["Eingabe"]),
        ("zeichnung.mass", &["Entf"]),
        ("modell.verrunden", &["Tab", "Pfeil links"]),
        // Frei: Eingabe in der Zeichnung, Entf im Modell.
        ("zeichnung.position", &["Eingabe"]),
        ("modell.extrudieren", &["Entf"]),
    ]));
    // Nur Unbrauchbares: es gilt die Voreinstellung.
    assert_eq!(km.bindings(Action::SketchLine), [key(Key::L)]);
    assert_eq!(km.bindings(Action::DrawingDimension), [key(Key::D)]);
    assert_eq!(km.bindings(Action::Fillet), [key(Key::F)]);
    assert_eq!(km.bindings(Action::DrawingBalloon), [key(Key::Enter)]);
    let text = |n: &str| notes.iter().any(|x| x.contains(n));
    assert!(text("„Entf“ ist in der Skizze fest belegt."), "{notes:?}");
    assert!(
        text("„Entf“ ist in der Zeichnung fest belegt."),
        "{notes:?}"
    );
    assert!(text("„Tab“ ist für den Wechsel"), "{notes:?}");
    assert!(text("„Pfeil links“ ist an der Verlaufsleiste"), "{notes:?}");
}
