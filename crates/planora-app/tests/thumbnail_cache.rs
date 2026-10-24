//! Miniatur aus einem Dokument und ihre Zusatzdateien `vorschau.png`/`vorschau.json` im
//! Dokumentordner (Aufgabe 5, Plan P6, P8): Aufbau aus der gespeicherten Version, Gültigkeit des
//! Zwischenspeichers, Grenzen, beschädigte Dokumente und schreibgeschützte Ordner.

mod common;

use planora_app::state::AppState;
use planora_app::thumbnail::{Image, encode_png, thumb_colors};
use planora_app::thumbnail_cache::*;
use planora_app::workspace::Workspace;
use planora_exchange::ExchangeMesh;
use planora_feature::{FeatureKind, SketchPlane, StandardPlane, StepShape};
use planora_kernel::{AnalyticKernel, Kernel};
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::docstore::{DocStore, LockMode, MAIN_BRANCH, VERSIONS_DIR};
use planora_project::library::Library;
use planora_step::normalize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

// ---- Hilfen ---------------------------------------------------------------------------------

/// Neue Bibliothek in Temp mit dem Projekt „P“.
fn library() -> Library {
    let root = common::temp_root("planora-thumb");
    let lib = Library::open(&root).unwrap();
    lib.create_project("P").unwrap();
    lib
}

/// Dokument „P/<name>.pcad“ mit dem Stand von `state` als Version 1 (ohne offene Sperre).
fn doc_in(lib: &Library, name: &str, state: &AppState) -> PathBuf {
    let dir = lib.root().path().join("P").join(format!("{name}.pcad"));
    drop(DocStore::create(&dir, &state.snapshot(), "").unwrap());
    dir
}

fn doc(state: &AppState) -> PathBuf {
    doc_in(&library(), "Teil", state)
}

/// Schlüssel ohne Dokumentkennung (genügt für `build` und `load_cached`).
fn key1() -> ThumbKey {
    kv(1, MAIN_BRANCH, true)
}

fn kv(version: u32, branch: &str, dark: bool) -> ThumbKey {
    ThumbKey::new("", version, branch, dark)
}

/// Schlüssel der Spitze des Dokuments in `dir` (Dunkel), mit Kennung.
fn key_of(dir: &Path) -> ThumbKey {
    ThumbKey::current(dir, true).unwrap()
}

fn boxed() -> AppState {
    let mut s = AppState::new();
    s.create_box(DVec3::new(10.0, 20.0, 30.0)).unwrap();
    s
}

fn image(t: &Thumb) -> &Image {
    match t {
        Thumb::Image(i) => i,
        other => panic!("Bild erwartet, war {other:?}"),
    }
}

/// Anteil der Pixel, die nicht Hintergrund sind.
fn body_share(img: &Image, dark: bool) -> f64 {
    let bg = thumb_colors(dark).background;
    let body = img.rgba.chunks(4).filter(|p| *p != bg).count();
    body as f64 / (img.rgba.len() / 4) as f64
}

/// Namen der Einträge im Ordner, die mit „vorschau“ beginnen (sortiert).
fn previews(dir: &Path) -> Vec<String> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new(); // Ordner fehlt
    };
    let mut v: Vec<String> = rd
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("vorschau"))
        .collect();
    v.sort();
    v
}

/// Alle Dateien unter `dir` außer `vorschau.*` mit ihrem Inhalt.
fn files_except_previews(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for e in fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            let rel = p.strip_prefix(base).unwrap().to_path_buf();
            if rel.to_string_lossy().starts_with("vorschau") {
                continue;
            }
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                out.insert(rel, fs::read(&p).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

fn kernel() -> AnalyticKernel {
    AnalyticKernel::default()
}

/// Würfel als Netz (8 Punkte, 12 Dreiecke), Kantenlänge `a`.
fn cube_mesh(a: f32) -> ExchangeMesh {
    let p = |x: f32, y: f32, z: f32| [x * a, y * a, z * a];
    ExchangeMesh {
        positions: vec![
            p(0., 0., 0.),
            p(1., 0., 0.),
            p(1., 1., 0.),
            p(0., 1., 0.),
            p(0., 0., 1.),
            p(1., 0., 1.),
            p(1., 1., 1.),
            p(0., 1., 1.),
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, // unten
            4, 5, 6, 4, 6, 7, // oben
            0, 1, 5, 0, 5, 4, // vorn
            1, 2, 6, 1, 6, 5, // rechts
            2, 3, 7, 2, 7, 6, // hinten
            3, 0, 4, 3, 4, 7, // links
        ],
    }
}

fn mesh_doc(mesh: ExchangeMesh) -> PathBuf {
    let mut s = AppState::new();
    s.import_mesh("Netz", Arc::new(mesh), 1.0).unwrap();
    doc(&s)
}

/// Gültige Zusatzdatei von Hand (Felder wie P6).
fn json_for(key: &ThumbKey, w: u32, h: u32, art: &str) -> String {
    format!(
        "{{\"v\":1,\"version\":{},\"branch\":\"{}\",\"program\":\"{}\",\"dark\":{},\"w\":{w},\"h\":{h},\"art\":\"{art}\"}}",
        key.version, key.branch, key.program, key.dark
    )
}

// ---- Tests ----------------------------------------------------------------------------------

#[test]
fn constants_are_as_planned() {
    assert_eq!(PREVIEW_PNG, "vorschau.png");
    assert_eq!(PREVIEW_JSON, "vorschau.json");
    assert_eq!(PREVIEW_FORMAT, 1);
    assert_eq!(PROGRAM, env!("CARGO_PKG_VERSION"));
    assert_eq!(MAX_TRIANGLES, 200_000);
    assert_eq!(MAX_BUILD, Duration::from_secs(3));
    assert_eq!(THUMB_CHORD_MM, 0.1);
    assert_eq!(
        BuildLimits::default(),
        BuildLimits {
            max_triangles: MAX_TRIANGLES,
            max_time: MAX_BUILD
        }
    );
    let k = ThumbKey::new("abc", 7, "Zweig", false);
    assert_eq!(
        k,
        ThumbKey {
            doc_id: "abc".into(),
            version: 7,
            branch: "Zweig".into(),
            program: PROGRAM.into(),
            dark: false
        }
    );
}

#[test]
fn a_box_gives_an_image_that_is_stored_and_reused() {
    let dir = doc(&boxed());
    let key = key_of(&dir);
    let out = thumbnail_for(&dir, &key, &BuildLimits::default());
    assert!(out.stored, "gespeichert");
    assert!(!out.from_cache, "neu gebaut");
    let img = image(&out.thumb).clone();
    assert_eq!((img.width, img.height), (496, 280));
    let share = body_share(&img, true);
    assert!(share > 0.05 && share < 0.95, "Körperanteil {share}");
    let json = fs::read_to_string(dir.join(PREVIEW_JSON)).unwrap();
    assert_eq!(
        json,
        format!(
            "{{\"v\":1,\"version\":1,\"branch\":\"Haupt\",\"program\":\"{PROGRAM}\",\"dark\":true,\"w\":496,\"h\":280,\"art\":\"bild\"}}"
        )
    );
    assert!(dir.join(PREVIEW_PNG).is_file());
    assert_eq!(previews(&dir), ["vorschau.json", "vorschau.png"]);

    let again = thumbnail_for(&dir, &key, &BuildLimits::default());
    assert!(again.from_cache, "aus der Datei");
    assert!(again.stored, "bei einem Treffer liegt die Datei vor");
    assert_eq!(image(&again.thumb).rgba, img.rgba, "gleiche Pixel");
    let cached = load_cached(&dir, &key).expect("gültig");
    assert_eq!(image(&cached).rgba, img.rgba);

    // Hell: andere Hintergrundfarbe, eigenes Bild.
    let light = ThumbKey {
        dark: false,
        ..key.clone()
    };
    let l = thumbnail_for(&dir, &light, &BuildLimits::default());
    assert!(!l.from_cache && l.stored);
    assert!(body_share(image(&l.thumb), false) > 0.05);
    assert_ne!(image(&l.thumb).rgba, img.rgba);
    // Die helle Datei wird als hell zurückgelesen, nicht als dunkel.
    let json = fs::read_to_string(dir.join(PREVIEW_JSON)).unwrap();
    assert!(json.contains("\"dark\":false"), "{json}");
    let back = load_cached(&dir, &light).expect("hell gültig");
    assert_eq!(image(&back).rgba, image(&l.thumb).rgba);
    assert!(load_cached(&dir, &key).is_none(), "nicht als Dunkel");
}

#[test]
fn the_key_invalidates_on_version_branch_program_and_theme() {
    let dir = doc(&boxed());
    let key = key_of(&dir);
    assert!(thumbnail_for(&dir, &key, &BuildLimits::default()).stored);
    assert!(load_cached(&dir, &key).is_some(), "gleicher Schlüssel");
    assert!(load_cached(&dir, &kv(2, MAIN_BRANCH, true)).is_none());
    assert!(load_cached(&dir, &kv(1, "Andere", true)).is_none());
    let mut old = key.clone();
    old.program = "0.0.0".into();
    assert!(load_cached(&dir, &old).is_none(), "Programmversion");
    assert!(load_cached(&dir, &kv(1, MAIN_BRANCH, false)).is_none());
    // Ein Fremdstand auf demselben Schlüssel ist für einen anderen Schlüssel ungültig, auch als
    // „leer“ oder „zu aufwendig“.
    for art in ["leer", "zu_aufwendig"] {
        fs::write(dir.join(PREVIEW_JSON), json_for(&key, 496, 280, art)).unwrap();
        assert!(load_cached(&dir, &key).is_some(), "{art}");
        assert!(load_cached(&dir, &kv(1, "Andere", true)).is_none());
        assert!(load_cached(&dir, &old).is_none());
        assert!(load_cached(&dir, &kv(1, MAIN_BRANCH, false)).is_none());
        assert!(load_cached(&dir, &kv(9, MAIN_BRANCH, true)).is_none());
    }
}

#[test]
fn a_damaged_or_missing_preview_is_rebuilt() {
    let dir = doc(&boxed());
    let key = key_of(&dir);
    let limits = BuildLimits::default();
    let first = thumbnail_for(&dir, &key, &limits);
    let good_png = fs::read(dir.join(PREVIEW_PNG)).unwrap();
    let good_json = fs::read(dir.join(PREVIEW_JSON)).unwrap();
    let small = encode_png(&Image {
        width: 10,
        height: 10,
        rgba: vec![7; 400],
    })
    .unwrap();
    type Damage = Box<dyn Fn(&Path)>;
    let damages: Vec<(&str, Damage)> = vec![
        (
            "PNG abgeschnitten",
            Box::new(|d: &Path| {
                let b = fs::read(d.join(PREVIEW_PNG)).unwrap();
                fs::write(d.join(PREVIEW_PNG), &b[..b.len() / 2]).unwrap();
            }),
        ),
        (
            "JSON {",
            Box::new(|d: &Path| fs::write(d.join(PREVIEW_JSON), "{").unwrap()),
        ),
        (
            "JSON ohne PNG",
            Box::new(|d: &Path| fs::remove_file(d.join(PREVIEW_PNG)).unwrap()),
        ),
        (
            "PNG 10 × 10",
            Box::new(move |d: &Path| fs::write(d.join(PREVIEW_PNG), &small).unwrap()),
        ),
        (
            "JSON nennt 10 × 10",
            Box::new(|d: &Path| {
                fs::write(d.join(PREVIEW_JSON), json_for(&key1(), 10, 10, "bild")).unwrap()
            }),
        ),
        (
            "JSON ohne Bildgröße 496 × 280",
            Box::new(|d: &Path| {
                fs::write(d.join(PREVIEW_JSON), json_for(&key1(), 280, 496, "bild")).unwrap()
            }),
        ),
        // Nur eine Seite falsch, auch ohne Bild („leer“): Breite und Höhe zählen je für sich.
        (
            "JSON nur Breite falsch",
            Box::new(|d: &Path| {
                fs::write(d.join(PREVIEW_JSON), json_for(&key1(), 495, 280, "bild")).unwrap()
            }),
        ),
        (
            "JSON nur Höhe falsch",
            Box::new(|d: &Path| {
                fs::write(d.join(PREVIEW_JSON), json_for(&key1(), 496, 279, "bild")).unwrap()
            }),
        ),
        (
            "leer mit falscher Breite",
            Box::new(|d: &Path| {
                fs::write(d.join(PREVIEW_JSON), json_for(&key1(), 10, 280, "leer")).unwrap()
            }),
        ),
        (
            "leer mit falscher Höhe",
            Box::new(|d: &Path| {
                fs::write(d.join(PREVIEW_JSON), json_for(&key1(), 496, 10, "leer")).unwrap()
            }),
        ),
        (
            "JSON v 2",
            Box::new(|d: &Path| {
                let t = json_for(&key1(), 496, 280, "bild").replace("\"v\":1", "\"v\":2");
                fs::write(d.join(PREVIEW_JSON), t).unwrap()
            }),
        ),
        (
            "unbekannte Art",
            Box::new(|d: &Path| {
                fs::write(d.join(PREVIEW_JSON), json_for(&key1(), 496, 280, "kaputt")).unwrap()
            }),
        ),
        (
            "JSON fehlt",
            Box::new(|d: &Path| fs::remove_file(d.join(PREVIEW_JSON)).unwrap()),
        ),
    ];
    for (what, damage) in &damages {
        fs::write(dir.join(PREVIEW_PNG), &good_png).unwrap();
        fs::write(dir.join(PREVIEW_JSON), &good_json).unwrap();
        assert!(load_cached(&dir, &key).is_some(), "vorher gültig ({what})");
        damage(&dir);
        assert!(load_cached(&dir, &key).is_none(), "{what}: ungültig");
        let out = thumbnail_for(&dir, &key, &limits);
        assert!(!out.from_cache, "{what}: neu gebaut");
        assert!(out.stored, "{what}: gespeichert");
        assert_eq!(image(&out.thumb).rgba, image(&first.thumb).rgba, "{what}");
        assert!(load_cached(&dir, &key).is_some(), "{what}: wieder gültig");
    }
}

#[test]
fn the_reader_is_strict() {
    let dir = doc(&AppState::new());
    let key = key1();
    let good = json_for(&key, 496, 280, "leer");
    let pretty = format!(
        "{{\n  \"art\" : \"leer\",\n  \"h\": 280, \"w\": 496,\n  \"dark\": true,\n  \"program\": \"{}\",\n  \"branch\": \"Haupt\", \"version\": 1, \"v\": 1\n}}\n",
        key.program
    );
    for (what, text, ok) in [
        ("wie geschrieben", good.clone(), true),
        ("andere Reihenfolge, Leerraum", pretty, true),
        ("Zusatzschlüssel", good.replace("}", ",\"x\":1}"), false),
        ("Schlüssel fehlt", good.replace(",\"h\":280", ""), false),
        ("doppelt", good.replace("}", ",\"h\":280}"), false),
        (
            "Komma",
            good.replace("\"version\":1", "\"version\":1.0"),
            false,
        ),
        (
            "Exponent",
            good.replace("\"version\":1", "\"version\":1e0"),
            false,
        ),
        (
            "Vorzeichen",
            good.replace("\"version\":1", "\"version\":-1"),
            false,
        ),
        (
            "Plus",
            good.replace("\"version\":1", "\"version\":+1"),
            false,
        ),
        (
            "führende Null",
            good.replace("\"version\":1", "\"version\":01"),
            false,
        ),
        (
            "Zahl als Text",
            good.replace("\"version\":1", "\"version\":\"1\""),
            false,
        ),
        (
            "Überlauf",
            good.replace("\"version\":1", "\"version\":4294967297"),
            false,
        ),
        ("dark als Zahl", good.replace("true", "1"), false),
        ("dark als Text", good.replace("true", "\"true\""), false),
        ("Rest danach", format!("{good}x"), false),
        ("zwei Objekte", format!("{good}{good}"), false),
        ("Liste", format!("[{good}]"), false),
        ("abgeschnitten", good[..good.len() - 1].to_string(), false),
        ("null", good.replace("true", "null"), false),
        ("zu groß", format!("{good}{}", " ".repeat(5000)), false),
        ("leer", String::new(), false),
    ] {
        fs::write(dir.join(PREVIEW_JSON), &text).unwrap();
        assert_eq!(load_cached(&dir, &key).is_some(), ok, "{what}: {text}");
    }
    // Maskierte Zeichen im Zweignamen gelten, wenn sie denselben Namen ergeben.
    fs::write(
        dir.join(PREVIEW_JSON),
        good.replace("Haupt", "\\u0048au\\u0070t"),
    )
    .unwrap();
    assert!(load_cached(&dir, &key).is_some(), "\\u-Maskierung");
    // Steuerzeichen nur maskiert, nie roh.
    let tab = kv(1, "Ha\tupt", true);
    fs::write(dir.join(PREVIEW_JSON), good.replace("Haupt", "Ha\\tupt")).unwrap();
    assert!(load_cached(&dir, &tab).is_some(), "\\t maskiert");
    fs::write(dir.join(PREVIEW_JSON), good.replace("Haupt", "Ha\tupt")).unwrap();
    assert!(load_cached(&dir, &tab).is_none(), "Tabulator roh");
    // Kein UTF-8
    let mut bytes = good.into_bytes();
    bytes.insert(10, 0xff);
    fs::write(dir.join(PREVIEW_JSON), bytes).unwrap();
    assert!(load_cached(&dir, &key).is_none(), "kein UTF-8");
}

#[test]
fn branch_names_with_quotes_and_control_characters_round_trip() {
    let dir = doc(&AppState::new());
    for branch in [
        "Zweig \"A\"",
        "C:\\Teile\\",
        "Zeile\nzwei",
        "Ton\u{7}",
        "Ünïcödé 😀",
        "/",
    ] {
        let key = kv(3, branch, true);
        let text = record_json(&key, "leer");
        assert!(!text.contains('\n'), "eine Zeile: {text:?}");
        assert!(text.chars().all(|c| !c.is_control()), "{text:?}");
        fs::write(dir.join(PREVIEW_JSON), &text).unwrap();
        assert!(
            matches!(load_cached(&dir, &key), Some(Thumb::NoBodies)),
            "{branch:?}: {text}"
        );
        assert!(load_cached(&dir, &kv(3, "Zweig", true)).is_none());
    }
    let key = kv(3, "a\"b\\c\u{1}", true);
    let text = record_json(&key, "zu_aufwendig");
    assert!(text.contains(r#""branch":"a\"b\\c\u0001""#), "{text}");
    // Über `store` mit einem echten Zweig, dessen Name Anführungszeichen enthält.
    let name = "Zweig \"B\" \\ x";
    {
        let (mut st, _) = DocStore::open(&dir, LockMode::Acquire).unwrap();
        st.save(None, &AppState::new().snapshot(), "", Some(name))
            .unwrap();
    }
    let key = ThumbKey::new(&key_of(&dir).doc_id, 2, name, true);
    store(&dir, &key, &Thumb::NoBodies).unwrap();
    assert!(matches!(load_cached(&dir, &key), Some(Thumb::NoBodies)));
}

#[test]
fn an_empty_document_stores_no_bodies() {
    let dir = doc(&AppState::new());
    let key = key_of(&dir);
    let out = thumbnail_for(&dir, &key, &BuildLimits::default());
    assert!(matches!(out.thumb, Thumb::NoBodies), "{:?}", out.thumb);
    assert!(out.stored && !out.from_cache);
    assert_eq!(
        fs::read_to_string(dir.join(PREVIEW_JSON)).unwrap(),
        json_for(&key, 496, 280, "leer")
    );
    assert!(!dir.join(PREVIEW_PNG).exists(), "kein Bild nötig");
    let again = thumbnail_for(&dir, &key, &BuildLimits::default());
    assert!(again.from_cache && matches!(again.thumb, Thumb::NoBodies));
}

#[test]
fn suppressed_features_and_sketches_do_not_appear() {
    let mut s = boxed();
    let id = s.document.features()[0].id;
    s.document.set_suppressed(id, true).unwrap();
    s.refresh();
    s.add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    let dir = doc(&s);
    let t = build(&dir, &key1(), &BuildLimits::default());
    assert!(matches!(t, Thumb::NoBodies), "{t:?}");

    // Ein verschobener Körper erscheint einmal (am neuen Ort), nicht auch der Ausgangskörper.
    let lib = library();
    let alone = build(
        &doc_in(&lib, "Allein", &boxed()),
        &key1(),
        &BuildLimits::default(),
    );
    let mut moved = boxed();
    moved.move_body(0, DVec3::new(200.0, 0.0, 0.0)).unwrap();
    assert_eq!(moved.document.visible_bodies().len(), 1);
    let m = build(
        &doc_in(&lib, "Verschoben", &moved),
        &key1(),
        &BuildLimits::default(),
    );
    let (a, b) = (body_share(image(&alone), true), body_share(image(&m), true));
    assert!((a - b).abs() < 0.002, "{a} gegen {b}");
}

#[test]
fn a_failed_feature_leaves_the_other_bodies() {
    let lib = library();
    let mut s = boxed();
    let bad = s
        .add_feature(
            None,
            FeatureKind::Cylinder {
                radius: -1.0,
                height: 2.0,
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    assert!(matches!(
        s.document.feature(bad).unwrap().status,
        planora_feature::FeatureStatus::Error(_)
    ));
    let t = build(&doc_in(&lib, "Teil", &s), &key1(), &BuildLimits::default());
    let alone = build(
        &doc_in(&lib, "Allein", &boxed()),
        &key1(),
        &BuildLimits::default(),
    );
    assert_eq!(image(&t).rgba, image(&alone).rgba, "nur der Quader");
}

#[test]
fn mesh_and_step_bodies_are_drawn() {
    let t = build(&mesh_doc(cube_mesh(10.0)), &key1(), &BuildLimits::default());
    let share = body_share(image(&t), true);
    assert!(share > 0.05, "Netz-Körper {share}");

    let solid = kernel().make_box(DVec3::new(10.0, 20.0, 30.0)).unwrap();
    let n = normalize(&solid, "Teil").unwrap();
    let shape = Arc::new(StepShape::with_solid(n.text, 1e-6, n.solid));
    let mut s = AppState::new();
    s.import_step(vec![("".into(), shape)]).unwrap();
    let t = build(&doc(&s), &key1(), &BuildLimits::default());
    let share = body_share(image(&t), true);
    assert!(share > 0.05, "STEP-Körper {share}");
}

#[test]
fn too_many_triangles_store_too_complex_once() {
    let dir = doc(&boxed());
    let key = key_of(&dir);
    let limits = BuildLimits {
        max_triangles: 10,
        ..BuildLimits::default()
    };
    let out = thumbnail_for(&dir, &key, &limits);
    assert!(matches!(out.thumb, Thumb::TooComplex), "{:?}", out.thumb);
    assert!(out.stored && !out.from_cache);
    assert_eq!(
        fs::read_to_string(dir.join(PREVIEW_JSON)).unwrap(),
        json_for(&key, 496, 280, "zu_aufwendig")
    );
    assert!(!dir.join(PREVIEW_PNG).exists());
    // Zweiter Aufruf: aus der Datei, ohne Aufbau (auch mit großzügigen Grenzen).
    let again = thumbnail_for(&dir, &key, &BuildLimits::default());
    assert!(again.from_cache && matches!(again.thumb, Thumb::TooComplex));

    // Netz-Körper: 12 Dreiecke; die Grenze gilt für die Summe, genau auf der Grenze ist erlaubt.
    let dir = mesh_doc(cube_mesh(10.0));
    let at = |n: usize| {
        build(
            &dir,
            &key,
            &BuildLimits {
                max_triangles: n,
                ..BuildLimits::default()
            },
        )
    };
    assert!(matches!(at(12), Thumb::Image(_)), "{:?}", at(12));
    assert!(matches!(at(11), Thumb::TooComplex), "{:?}", at(11));

    // Zwei Körper, jeder für sich unter der Grenze, zusammen darüber.
    let mut s = AppState::new();
    s.import_mesh("A", Arc::new(cube_mesh(10.0)), 1.0).unwrap();
    s.import_mesh("B", Arc::new(cube_mesh(5.0)), 1.0).unwrap();
    let two = doc(&s);
    let limits = |n: usize| BuildLimits {
        max_triangles: n,
        ..BuildLimits::default()
    };
    assert!(matches!(build(&two, &key, &limits(24)), Thumb::Image(_)));
    assert!(matches!(build(&two, &key, &limits(20)), Thumb::TooComplex));
}

#[test]
fn an_expensive_drawing_is_too_complex() {
    // 2000 gestapelte Quadrate (4000 Dreiecke): wenige Dreiecke, aber jedes bedeckt einen großen
    // Teil des Bildes; das Zeichnen wäre zu teuer.
    let mut m = ExchangeMesh {
        positions: Vec::new(),
        indices: Vec::new(),
    };
    for i in 0..2000u32 {
        let z = i as f32 * 0.01;
        m.positions.extend([
            [0.0, 0.0, z],
            [100.0, 0.0, z],
            [100.0, 100.0, z],
            [0.0, 100.0, z],
        ]);
        let b = i * 4;
        m.indices.extend([b, b + 1, b + 2, b, b + 2, b + 3]);
    }
    let started = std::time::Instant::now();
    let t = build(&mesh_doc(m), &key1(), &BuildLimits::default());
    assert!(matches!(t, Thumb::TooComplex), "{t:?}");
    assert!(started.elapsed() < Duration::from_secs(20));
}

#[test]
fn a_slow_build_is_too_complex() {
    let dir = doc(&boxed());
    let key = key1();
    let zero = BuildLimits {
        max_time: Duration::ZERO,
        ..BuildLimits::default()
    };
    assert!(matches!(build(&dir, &key, &zero), Thumb::TooComplex));
    // Ein Schritt, der länger dauert als die Grenze: zu aufwendig; mit Zeit genug ein Bild.
    let short = BuildLimits {
        max_time: Duration::from_millis(300),
        ..BuildLimits::default()
    };
    let mut steps = 0;
    let t = build_with(&dir, &key, &short, &mut || {
        steps += 1;
        if steps == 2 {
            std::thread::sleep(Duration::from_millis(400));
        }
    });
    assert!(matches!(t, Thumb::TooComplex), "{t:?}");
    assert_eq!(steps, 2, "nach dem Körper geprüft");
    let mut steps = 0;
    let t = build_with(&dir, &key, &BuildLimits::default(), &mut || steps += 1);
    assert!(matches!(t, Thumb::Image(_)), "{t:?}");
    assert_eq!(steps, 2, "nach from_snapshot und nach dem einen Körper");
}

#[test]
fn damaged_documents_give_failed_without_panic_and_write_nothing() {
    let lib = library();
    let limits = BuildLimits::default();
    let check = |dir: &Path, key: &ThumbKey, what: &str| {
        let out = thumbnail_for(dir, key, &limits);
        assert!(
            matches!(out.thumb, Thumb::Failed(_)),
            "{what}: {:?}",
            out.thumb
        );
        assert!(!out.stored && !out.from_cache, "{what}");
        assert!(previews(dir).is_empty(), "{what}: {:?}", previews(dir));
    };

    let cut = doc_in(&lib, "Abgeschnitten", &boxed());
    let key = key_of(&cut);
    let v1 = cut.join(VERSIONS_DIR).join("0001.json");
    let text = fs::read(&v1).unwrap();
    fs::write(&v1, &text[..text.len() / 2]).unwrap();
    check(&cut, &key, "abgeschnitten");

    let tip = doc_in(&lib, "Spitze", &boxed());
    {
        let (mut st, _) = DocStore::open(&tip, LockMode::Acquire).unwrap();
        let mut s = boxed();
        s.create_sphere(4.0).unwrap();
        assert_eq!(st.save(None, &s.snapshot(), "", None).unwrap(), 2);
    }
    fs::remove_file(tip.join(VERSIONS_DIR).join("0002.json")).unwrap();
    let out = thumbnail_for(&tip, &key_of(&tip), &limits);
    assert!(matches!(out.thumb, Thumb::Failed(_)), "{:?}", out.thumb);
    assert!(previews(&tip).is_empty());

    let mut s = AppState::new();
    s.create_box(DVec3::new(12.5, 20.0, 30.0)).unwrap();
    let nan = doc_in(&lib, "NaN", &s);
    let key = key_of(&nan);
    let v1 = nan.join(VERSIONS_DIR).join("0001.json");
    let text = fs::read_to_string(&v1).unwrap();
    let broken = text.replacen("12.5", "NaN", 1);
    assert_ne!(broken, text);
    fs::write(&v1, broken).unwrap();
    check(&nan, &key, "NaN");

    let gone = doc_in(&lib, "Ohne", &boxed());
    let key = key_of(&gone);
    fs::remove_file(gone.join("dokument.json")).unwrap();
    check(&gone, &key, "ohne dokument.json");

    check(
        &lib.root().path().join("P").join("Fehlt.pcad"),
        &key,
        "Ordner fehlt",
    );
    assert!(!lib.root().path().join("P").join("Fehlt.pcad").exists());

    // Eine Panik im Aufbau kommt nie nach außen.
    let ok = doc_in(&lib, "Gut", &boxed());
    let key = key_of(&ok);
    let t = build_with(&ok, &key, &limits, &mut || panic!("Prüfpanik"));
    assert!(matches!(t, Thumb::Failed(_)), "{t:?}");
    assert!(
        store(&ok, &key, &t).is_err(),
        "Fehlschlag wird nie gespeichert"
    );
    assert!(previews(&ok).is_empty());
    // Über `thumbnail_for`: die Marke „baut“ wird nach dem Fehlschlag wieder entfernt.
    let out = thumbnail_for_with(&ok, &key, &limits, &mut || panic!("Prüfpanik"));
    assert!(matches!(out.thumb, Thumb::Failed(_)), "{:?}", out.thumb);
    assert!(!out.stored);
    assert!(previews(&ok).is_empty(), "{:?}", previews(&ok));
}

#[test]
fn a_read_only_folder_keeps_the_thumbnail_in_memory_only() {
    let lib = library();
    // vorschau.json ist ein Ordner: Schreiben scheitert.
    let dir = doc_in(&lib, "A", &boxed());
    let key = key_of(&dir);
    fs::create_dir(dir.join(PREVIEW_JSON)).unwrap();
    let out = thumbnail_for(&dir, &key, &BuildLimits::default());
    assert!(matches!(out.thumb, Thumb::Image(_)), "{:?}", out.thumb);
    assert!(!out.stored && !out.from_cache);
    assert!(load_cached(&dir, &key).is_none());
    assert!(dir.join(PREVIEW_JSON).is_dir(), "nichts ersetzt");

    // vorschau.png ist ein Ordner: dann entsteht auch keine JSON (die JSON ist das Fertig-Zeichen).
    let dir = doc_in(&lib, "B", &boxed());
    let key = key_of(&dir);
    fs::create_dir(dir.join(PREVIEW_PNG)).unwrap();
    let out = thumbnail_for(&dir, &key, &BuildLimits::default());
    assert!(matches!(out.thumb, Thumb::Image(_)));
    assert!(!out.stored);
    assert!(!dir.join(PREVIEW_JSON).exists(), "keine JSON ohne Bild");
    for d in [doc_in(&lib, "C", &AppState::new()), dir] {
        let left: Vec<String> = previews(&d)
            .into_iter()
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(left.is_empty(), "{left:?}");
    }
}

#[test]
fn a_leftover_fixed_temporary_name_does_not_block_storing() {
    // Reste unter den festen Zwischennamen (z. B. als Ordner) stören nicht: jeder Schreibvorgang
    // hat seine eigene Zwischendatei.
    let dir = doc(&boxed());
    fs::create_dir(dir.join("vorschau.png.tmp")).unwrap();
    fs::create_dir(dir.join("vorschau.json.tmp")).unwrap();
    let out = thumbnail_for(&dir, &key_of(&dir), &BuildLimits::default());
    assert!(out.stored, "gespeichert");
    assert!(load_cached(&dir, &key1()).is_some());
}

#[test]
fn an_image_of_the_wrong_size_is_not_stored() {
    let dir = doc(&AppState::new());
    let small = Thumb::Image(Arc::new(Image {
        width: 10,
        height: 10,
        rgba: vec![0; 400],
    }));
    let key = key_of(&dir);
    assert!(store(&dir, &key, &small).is_err());
    assert!(previews(&dir).is_empty());
    assert!(store(&dir, &key, &Thumb::Failed("x".into())).is_err());
    assert!(store(&dir, &key, &Thumb::Busy).is_err());
    assert!(previews(&dir).is_empty());
}

#[test]
fn an_open_modified_document_shows_the_saved_version() {
    let lib = library();
    let root = lib.root().path().to_path_buf();
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    w.config_path = root.join("config.json");
    w.active_tab_mut()
        .state
        .create_box(DVec3::new(10.0, 20.0, 30.0))
        .unwrap();
    let node = w.save_as(&vec!["P".into()], "Offen", "").unwrap();
    let dir = w.library.as_ref().unwrap().document_dir(&node).unwrap();
    w.active_tab_mut().state.create_sphere(8.0).unwrap();
    assert!(w.active_tab().state.document.visible_bodies().len() == 2);

    let t = build(&dir, &key1(), &BuildLimits::default());
    let alone = build(
        &doc_in(&lib, "Allein", &boxed()),
        &key1(),
        &BuildLimits::default(),
    );
    let (a, b) = (body_share(image(&t), true), body_share(image(&alone), true));
    assert!((a - b).abs() < 1e-9, "{a} gegen {b}");
    assert!(dir.join(".sperre").is_file(), "der Tab hält die Sperre");
    let t2 = thumbnail_for(&dir, &key_of(&dir), &BuildLimits::default());
    assert!(t2.stored, "Miniatur trotz offenem Tab gespeichert");
    assert!(dir.join(".sperre").is_file(), "Sperre unberührt");
    drop(w);
}

#[test]
fn an_older_version_is_drawn_as_asked() {
    let lib = library();
    let dir = doc_in(&lib, "Teil", &boxed());
    {
        let (mut st, _) = DocStore::open(&dir, LockMode::Acquire).unwrap();
        let mut s = boxed();
        s.create_sphere(8.0).unwrap();
        assert_eq!(st.save(None, &s.snapshot(), "", None).unwrap(), 2);
    }
    let alone = build(
        &doc_in(&lib, "Allein", &boxed()),
        &key1(),
        &BuildLimits::default(),
    );
    let v1 = build(&dir, &key1(), &BuildLimits::default());
    let v2 = build(&dir, &kv(2, MAIN_BRANCH, true), &BuildLimits::default());
    assert_eq!(
        image(&v1).rgba,
        image(&alone).rgba,
        "Version 1 = nur Quader"
    );
    assert_ne!(image(&v2).rgba, image(&v1).rgba, "Version 2 mit Kugel");
}

#[test]
fn building_never_changes_other_files_in_the_document_folder() {
    let lib = library();
    let dir = doc_in(&lib, "Teil", &boxed());
    // Offener Tab: Sperre und Sicherung liegen im Ordner.
    let (st, _) = DocStore::open(&dir, LockMode::Acquire).unwrap();
    st.write_autosave(Some(1), &boxed().snapshot()).unwrap();
    fs::write(dir.join("notiz.txt"), "fremd").unwrap();
    let before = files_except_previews(&dir);
    assert!(before.len() >= 4, "{:?}", before.keys());
    assert!(
        before.contains_key(Path::new(".sperre")),
        "{:?}",
        before.keys()
    );
    let key = key_of(&dir);
    let light = ThumbKey {
        dark: false,
        ..key.clone()
    };
    let other = ThumbKey {
        branch: "Andere".into(),
        ..key.clone()
    };
    let five = ThumbKey {
        version: 5,
        ..key.clone()
    };
    for (key, limits) in [
        (key.clone(), BuildLimits::default()),
        (light, BuildLimits::default()),
        (
            other,
            BuildLimits {
                max_triangles: 1,
                ..BuildLimits::default()
            },
        ),
        (five, BuildLimits::default()),
    ] {
        let _ = thumbnail_for(&dir, &key, &limits);
        assert_eq!(files_except_previews(&dir), before, "{key:?}");
    }
    // Nur vorschau.png und vorschau.json, keine Zwischendateien.
    assert_eq!(previews(&dir), ["vorschau.json", "vorschau.png"]);
    drop(st);
}

// ---- Nachbesserung 1: alte JSON, Kennung, Zwischendateien, Absturzschutz ---------------------

/// Setzt oder entfernt den Schreibschutz einer Datei.
fn set_read_only(p: &Path, on: bool) {
    let mut perm = fs::metadata(p).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perm.set_readonly(on);
    fs::set_permissions(p, perm).unwrap();
}

/// Hält eine Datei offen, sodass sie sich weder löschen noch ersetzen lässt (wie ein
/// Virenscanner oder ein anderes Programm); Lesen bleibt möglich.
#[cfg(windows)]
fn lock(p: &Path) -> fs::File {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_READ: u32 = 1;
    fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(p)
        .unwrap()
}

/// Ohne Windows: der Ordner wird schreibgeschützt (Löschen und Anlegen scheitern).
#[cfg(not(windows))]
fn lock(p: &Path) -> impl Drop {
    struct Guard(PathBuf);
    impl Drop for Guard {
        fn drop(&mut self) {
            set_read_only(&self.0, false);
        }
    }
    let dir = p.parent().unwrap().to_path_buf();
    set_read_only(&dir, true);
    Guard(dir)
}

#[test]
fn a_locked_old_json_never_ends_up_next_to_a_new_png() {
    let dir = doc(&boxed());
    let dark = key_of(&dir);
    let first = thumbnail_for(&dir, &dark, &BuildLimits::default());
    assert!(first.stored);
    let old_png = fs::read(dir.join(PREVIEW_PNG)).unwrap();
    let old_json = fs::read(dir.join(PREVIEW_JSON)).unwrap();
    let guard = lock(&dir.join(PREVIEW_JSON));
    let light = ThumbKey {
        dark: false,
        ..dark.clone()
    };
    let out = thumbnail_for(&dir, &light, &BuildLimits::default());
    assert!(matches!(out.thumb, Thumb::Image(_)), "{:?}", out.thumb);
    assert!(!out.stored, "nur im Speicher");
    // Auch `store` direkt: kein PNG, solange die alte JSON bleibt.
    assert!(store(&dir, &light, &out.thumb).is_err());
    assert_eq!(
        fs::read(dir.join(PREVIEW_PNG)).unwrap(),
        old_png,
        "altes Bild"
    );
    assert_eq!(
        fs::read(dir.join(PREVIEW_JSON)).unwrap(),
        old_json,
        "alte JSON"
    );
    // Weiter ein stimmiges Paar: Dunkel zeigt das dunkle Bild, Hell ist ungültig.
    let back = load_cached(&dir, &dark).expect("dunkel gültig");
    assert_eq!(image(&back).rgba, image(&first.thumb).rgba);
    assert!(load_cached(&dir, &light).is_none());
    drop(guard);
    let out = thumbnail_for(&dir, &light, &BuildLimits::default());
    assert!(out.stored);
    assert!(
        load_cached(&dir, &dark).is_none(),
        "alte dunkle JSON ersetzt"
    );
}

#[test]
fn a_read_only_old_json_never_ends_up_next_to_a_new_png() {
    // Schreibgeschützte alte JSON: entweder wird sie ersetzt (dann passt alles zum neuen Bild)
    // oder sie bleibt samt altem Bild; nie die alte JSON neben einem neuen Bild.
    let dir = doc(&boxed());
    let dark = key_of(&dir);
    let first = thumbnail_for(&dir, &dark, &BuildLimits::default());
    let old_png = fs::read(dir.join(PREVIEW_PNG)).unwrap();
    set_read_only(&dir.join(PREVIEW_JSON), true);
    let light = ThumbKey {
        dark: false,
        ..dark.clone()
    };
    let out = thumbnail_for(&dir, &light, &BuildLimits::default());
    if out.stored {
        assert!(load_cached(&dir, &dark).is_none());
        let l = load_cached(&dir, &light).expect("hell gültig");
        assert_eq!(image(&l).rgba, image(&out.thumb).rgba);
    } else {
        assert_eq!(fs::read(dir.join(PREVIEW_PNG)).unwrap(), old_png);
        let d = load_cached(&dir, &dark).expect("altes Paar");
        assert_eq!(image(&d).rgba, image(&first.thumb).rgba);
    }
    if dir.join(PREVIEW_JSON).exists() {
        set_read_only(&dir.join(PREVIEW_JSON), false);
    }
}

#[test]
fn a_folder_replaced_by_another_document_is_not_written() {
    let lib = library();
    let dir = doc_in(&lib, "Teil", &boxed());
    let key = key_of(&dir);
    let t = build(&dir, &key, &BuildLimits::default());
    assert!(matches!(t, Thumb::Image(_)));
    // Zwischen Aufbau und Speichern: Ordner weg, ein anderes Dokument am selben Ort.
    fs::rename(&dir, lib.root().path().join("P").join("Alt.pcad")).unwrap();
    drop(DocStore::create(&dir, &AppState::new().snapshot(), "").unwrap());
    assert_ne!(key_of(&dir).doc_id, key.doc_id);
    assert!(store(&dir, &key, &t).is_err());
    assert!(previews(&dir).is_empty(), "{:?}", previews(&dir));
    let out = thumbnail_for(&dir, &key, &BuildLimits::default());
    assert!(matches!(out.thumb, Thumb::Failed(_)), "{:?}", out.thumb);
    assert!(previews(&dir).is_empty(), "nichts für das fremde Dokument");

    // Neue Version gespeichert: das Ergebnis für die alte Spitze wird nicht abgelegt.
    let key = key_of(&dir);
    let t = build(&dir, &key, &BuildLimits::default());
    {
        let (mut st, _) = DocStore::open(&dir, LockMode::Acquire).unwrap();
        st.save(None, &boxed().snapshot(), "", None).unwrap();
    }
    assert!(store(&dir, &key, &t).is_err());
    assert!(previews(&dir).is_empty());
    assert!(store(&dir, &key_of(&dir), &t).is_ok());
}

#[test]
fn old_own_temporary_files_are_removed_when_storing() {
    let dir = doc(&AppState::new());
    let two_hours_ago = std::time::SystemTime::now() - Duration::from_secs(2 * 3600);
    let old = ["vorschau.png.123-4.tmp", "vorschau.json.99-0.tmp"];
    let foreign = ["vorschau.png.abc.tmp", "notiz.tmp"];
    for n in old.iter().chain(&foreign) {
        let f = fs::File::create(dir.join(n)).unwrap();
        f.set_modified(two_hours_ago).unwrap();
    }
    fs::write(dir.join("vorschau.png.555-1.tmp"), "frisch").unwrap();
    // Ein alter Ordner mit passendem Namen bleibt (nur Dateien werden entfernt).
    fs::create_dir(dir.join("vorschau.json.7-7.tmp")).unwrap();
    store(&dir, &key_of(&dir), &Thumb::NoBodies).unwrap();
    for n in old {
        assert!(!dir.join(n).exists(), "{n} entfernt");
    }
    for n in foreign.iter().chain(&["vorschau.png.555-1.tmp"]) {
        assert!(dir.join(n).exists(), "{n} bleibt");
    }
    assert!(dir.join("vorschau.json.7-7.tmp").is_dir());
    assert_eq!(STALE_TMP, Duration::from_secs(3600));
}

#[test]
fn the_program_field_comes_from_the_key() {
    let dir = doc(&AppState::new());
    let key = ThumbKey {
        program: "9.9.9".into(),
        ..key_of(&dir)
    };
    store(&dir, &key, &Thumb::NoBodies).unwrap();
    let text = fs::read_to_string(dir.join(PREVIEW_JSON)).unwrap();
    assert!(text.contains("\"program\":\"9.9.9\""), "{text}");
    assert!(load_cached(&dir, &key).is_some());
    assert!(load_cached(&dir, &key_of(&dir)).is_none());
}

#[test]
fn the_kernel_body_triangle_limit_is_exact() {
    let dir = doc(&boxed());
    let solid = kernel().make_box(DVec3::new(10.0, 20.0, 30.0)).unwrap();
    let options = planora_mesh::TessellationOptions {
        chord_tolerance: THUMB_CHORD_MM,
    };
    let n = planora_mesh::tessellate(&solid, &options)
        .unwrap()
        .indices
        .len()
        / 3;
    let at = |max: usize| {
        build(
            &dir,
            &key1(),
            &BuildLimits {
                max_triangles: max,
                ..BuildLimits::default()
            },
        )
    };
    assert!(matches!(at(n), Thumb::Image(_)), "{n}: {:?}", at(n));
    assert!(matches!(at(n - 1), Thumb::TooComplex), "{:?}", at(n - 1));
}

/// Marke „baut“ von Hand (z. B. eines fremden Prozesses), erster Versuch.
fn marker(dir: &Path, key: &ThumbKey, pid: u32, since: &str) {
    marker_n(dir, key, pid, since, 1);
}

/// Marke „baut“ von Hand mit Versuchszahl.
fn marker_n(dir: &Path, key: &ThumbKey, pid: u32, since: &str, versuche: u32) {
    fs::write(
        dir.join(PREVIEW_JSON),
        marker_json(key, pid, since, versuche),
    )
    .unwrap();
}

/// Zeitpunkt als „JJJJ-MM-TTThh:mm:ssZ“ (UTC; Rechnung nach H. Hinnant).
fn iso(t: std::time::SystemTime) -> String {
    let secs = t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Prozessnummer, die nicht dieser Prozess ist.
const FOREIGN_PID: u32 = 4_000_000_001;

#[test]
fn the_helper_formats_times_like_the_program() {
    let now = iso(std::time::SystemTime::now());
    let real = planora_project::fsutil::now_iso();
    assert_eq!(now[..15], real[..15], "{now} / {real}");
    assert_eq!(iso(std::time::UNIX_EPOCH), "1970-01-01T00:00:00Z");
}

#[test]
fn the_marker_is_written_before_the_build_and_visible_during_it() {
    let dir = doc(&boxed());
    let key = key_of(&dir);
    // Ein altes helles Bild liegt da; vor dem Aufbau verschwinden JSON und PNG.
    let light = ThumbKey {
        dark: false,
        ..key.clone()
    };
    assert!(thumbnail_for(&dir, &light, &BuildLimits::default()).stored);
    let mut seen = Vec::new();
    let out = thumbnail_for_with(&dir, &key, &BuildLimits::default(), &mut || {
        seen.push((
            fs::read_to_string(dir.join(PREVIEW_JSON)).unwrap_or_default(),
            dir.join(PREVIEW_PNG).exists(),
            load_cached(&dir, &key).is_none(),
        ));
    });
    assert!(out.stored && matches!(out.thumb, Thumb::Image(_)));
    assert_eq!(seen.len(), 2, "{seen:?}");
    for (json, png, not_cached) in &seen {
        assert!(json.contains("\"art\":\"baut\""), "{json}");
        let pid = format!("\"pid\":{}", std::process::id());
        assert!(json.contains(&pid), "{json}");
        assert!(json.contains("\"since\":\""), "{json}");
        assert!(json.contains("\"versuche\":1}"), "erster Versuch: {json}");
        assert!(json.contains("\"dark\":true"), "{json}");
        assert!(!png, "altes Bild vor dem Aufbau entfernt");
        assert!(not_cached, "die Marke ist keine Miniatur");
    }
    assert!(
        load_cached(&dir, &key).is_some(),
        "Ergebnis ersetzt die Marke"
    );
}

#[test]
fn a_fresh_foreign_marker_means_busy_and_nothing_is_built() {
    let dir = doc(&boxed());
    let key = key_of(&dir);
    marker(&dir, &key, FOREIGN_PID, &iso(std::time::SystemTime::now()));
    let before = fs::read(dir.join(PREVIEW_JSON)).unwrap();
    let mut steps = 0;
    let out = thumbnail_for_with(&dir, &key, &BuildLimits::default(), &mut || steps += 1);
    assert!(matches!(out.thumb, Thumb::Busy), "{:?}", out.thumb);
    assert!(!out.stored && !out.from_cache);
    assert_eq!(steps, 0, "nicht gebaut");
    assert_eq!(
        fs::read(dir.join(PREVIEW_JSON)).unwrap(),
        before,
        "Marke bleibt"
    );
    assert!(!dir.join(PREVIEW_PNG).exists());
}

#[test]
fn a_stale_first_attempt_is_built_again_as_attempt_two() {
    // Ein Lauf endete während des Aufbaus (Beenden oder Absturz): einmal neu versuchen.
    let dir = doc(&boxed());
    let key = key_of(&dir);
    let hour_ago = iso(std::time::SystemTime::now() - Duration::from_secs(3600));
    marker_n(&dir, &key, FOREIGN_PID, &hour_ago, 1);
    let mut seen = Vec::new();
    let out = thumbnail_for_with(&dir, &key, &BuildLimits::default(), &mut || {
        seen.push(fs::read_to_string(dir.join(PREVIEW_JSON)).unwrap_or_default());
    });
    assert!(matches!(out.thumb, Thumb::Image(_)), "{:?}", out.thumb);
    assert!(out.stored && !out.from_cache);
    assert!(!seen.is_empty(), "neu gebaut");
    for json in &seen {
        let pid = format!("\"pid\":{}", std::process::id());
        assert!(json.contains(&pid), "eigene Marke: {json}");
        assert!(json.contains("\"versuche\":2}"), "zweiter Versuch: {json}");
    }
    assert!(
        load_cached(&dir, &key).is_some(),
        "Ergebnis ersetzt die Marke"
    );
    // Endet auch der zweite Versuch nie, gilt die Miniatur danach als nicht möglich.
    let dir = doc(&boxed());
    let key = key_of(&dir);
    marker_n(&dir, &key, FOREIGN_PID, &hour_ago, 1);
    let mut second = String::new();
    thumbnail_for_with(&dir, &key, &BuildLimits::default(), &mut || {
        if second.is_empty() {
            second = fs::read_to_string(dir.join(PREVIEW_JSON)).unwrap();
        }
    });
    let crashed = second
        .replace(
            &format!("\"pid\":{}", std::process::id()),
            &format!("\"pid\":{FOREIGN_PID}"),
        )
        .replace(&now_iso_like(&second), &hour_ago);
    fs::write(dir.join(PREVIEW_JSON), &crashed).unwrap();
    fs::remove_file(dir.join(PREVIEW_PNG)).unwrap();
    let mut steps = 0;
    let out = thumbnail_for_with(&dir, &key, &BuildLimits::default(), &mut || steps += 1);
    assert!(matches!(out.thumb, Thumb::Impossible), "{:?}", out.thumb);
    assert_eq!(steps, 0, "kein dritter Versuch");
    assert_eq!(MAX_ATTEMPTS, 2);
}

/// Der Wert von `since` in einer Marke.
fn now_iso_like(json: &str) -> String {
    let start = json.find("\"since\":\"").unwrap() + 9;
    json[start..start + 20].to_string()
}

#[test]
fn a_fresh_foreign_second_attempt_is_still_busy() {
    let dir = doc(&boxed());
    let key = key_of(&dir);
    marker_n(
        &dir,
        &key,
        FOREIGN_PID,
        &iso(std::time::SystemTime::now()),
        2,
    );
    let mut steps = 0;
    let out = thumbnail_for_with(&dir, &key, &BuildLimits::default(), &mut || steps += 1);
    assert!(matches!(out.thumb, Thumb::Busy), "{:?}", out.thumb);
    assert_eq!(steps, 0);
}

#[test]
fn an_own_marker_starts_again_at_attempt_one() {
    // Eigene Prozessnummer: Rest dieses lebenden Laufs, nie ein Absturz, zählt nicht weiter.
    let dir = doc(&boxed());
    let key = key_of(&dir);
    let hour_ago = iso(std::time::SystemTime::now() - Duration::from_secs(3600));
    marker_n(&dir, &key, std::process::id(), &hour_ago, 2);
    let mut seen = Vec::new();
    let out = thumbnail_for_with(&dir, &key, &BuildLimits::default(), &mut || {
        seen.push(fs::read_to_string(dir.join(PREVIEW_JSON)).unwrap_or_default());
    });
    assert!(matches!(out.thumb, Thumb::Image(_)), "{:?}", out.thumb);
    assert!(
        seen.iter().all(|j| j.contains("\"versuche\":1}")),
        "{seen:?}"
    );
}

#[test]
fn a_stale_foreign_marker_stores_impossible_and_never_builds() {
    let dir = doc(&boxed());
    let key = key_of(&dir);
    let hour_ago = iso(std::time::SystemTime::now() - Duration::from_secs(3600));
    marker_n(&dir, &key, FOREIGN_PID, &hour_ago, 2);
    let mut steps = 0;
    let out = thumbnail_for_with(&dir, &key, &BuildLimits::default(), &mut || steps += 1);
    assert!(matches!(out.thumb, Thumb::Impossible), "{:?}", out.thumb);
    assert!(out.stored);
    assert_eq!(steps, 0, "nicht erneut gebaut");
    assert_eq!(
        fs::read_to_string(dir.join(PREVIEW_JSON)).unwrap(),
        json_for(&key, 496, 280, "fehlgeschlagen")
    );
    assert!(!dir.join(PREVIEW_PNG).exists());
    // Danach immer aus der Datei, ohne Aufbau.
    let again = thumbnail_for_with(&dir, &key, &BuildLimits::default(), &mut || steps += 1);
    assert!(again.from_cache && matches!(again.thumb, Thumb::Impossible));
    assert_eq!(steps, 0);
    assert_eq!(IMPOSSIBLE_TEXT, "Miniatur nicht möglich");
    // Eine Marke weit in der Zukunft (verstellte Uhr) gilt ebenfalls als alt.
    let dir = doc(&boxed());
    let key = key_of(&dir);
    let later = iso(std::time::SystemTime::now() + Duration::from_secs(3600));
    marker_n(&dir, &key, FOREIGN_PID, &later, 2);
    let out = thumbnail_for(&dir, &key, &BuildLimits::default());
    assert!(matches!(out.thumb, Thumb::Impossible), "{:?}", out.thumb);
    // Kurz vor der Grenze von 15 Minuten: noch frisch.
    let dir = doc(&boxed());
    let key = key_of(&dir);
    let recent = iso(std::time::SystemTime::now() - Duration::from_secs(14 * 60));
    marker_n(&dir, &key, FOREIGN_PID, &recent, 2);
    let out = thumbnail_for(&dir, &key, &BuildLimits::default());
    assert!(matches!(out.thumb, Thumb::Busy), "{:?}", out.thumb);
    // Kurz nach der Grenze: alt.
    let dir = doc(&boxed());
    let key = key_of(&dir);
    let past = iso(std::time::SystemTime::now() - Duration::from_secs(16 * 60));
    marker_n(&dir, &key, FOREIGN_PID, &past, 2);
    let out = thumbnail_for(&dir, &key, &BuildLimits::default());
    assert!(matches!(out.thumb, Thumb::Impossible), "{:?}", out.thumb);
    assert_eq!(MARKER_FRESH, Duration::from_secs(15 * 60));
}

#[test]
fn a_marker_for_another_key_or_from_this_process_is_replaced() {
    let lib = library();
    let hour_ago = iso(std::time::SystemTime::now() - Duration::from_secs(3600));
    let now = iso(std::time::SystemTime::now());
    // Marke für Version 1, inzwischen ist Version 2 die Spitze.
    let dir = doc_in(&lib, "Neu", &boxed());
    let v1 = key_of(&dir);
    {
        let (mut st, _) = DocStore::open(&dir, LockMode::Acquire).unwrap();
        st.save(None, &boxed().snapshot(), "", None).unwrap();
    }
    let v2 = key_of(&dir);
    assert_eq!(v2.version, 2);
    let others = [
        ("alte Version", v1.clone()),
        (
            "anderes Thema",
            ThumbKey {
                dark: false,
                ..v2.clone()
            },
        ),
        (
            "anderes Programm",
            ThumbKey {
                program: "0.0.1".into(),
                ..v2.clone()
            },
        ),
        (
            "anderer Zweig",
            ThumbKey {
                branch: "Z".into(),
                ..v2.clone()
            },
        ),
    ];
    for (what, other) in others {
        for since in [&hour_ago, &now] {
            marker(&dir, &other, FOREIGN_PID, since);
            let out = thumbnail_for(&dir, &v2, &BuildLimits::default());
            assert!(
                matches!(out.thumb, Thumb::Image(_)),
                "{what}: {:?}",
                out.thumb
            );
            assert!(out.stored, "{what}");
            fs::remove_file(dir.join(PREVIEW_JSON)).unwrap();
        }
    }
    // Eigene Marke dieses (lebenden) Prozesses: Rest ohne Absturz, wird ersetzt.
    for since in [&hour_ago, &now] {
        marker(&dir, &v2, std::process::id(), since);
        let out = thumbnail_for(&dir, &v2, &BuildLimits::default());
        assert!(matches!(out.thumb, Thumb::Image(_)), "{:?}", out.thumb);
        fs::remove_file(dir.join(PREVIEW_JSON)).unwrap();
    }
}

#[test]
fn the_reader_handles_the_new_art_values_strictly() {
    let dir = doc(&AppState::new());
    let key = key_of(&dir);
    let now = iso(std::time::SystemTime::now());
    let good_marker = marker_json(&key, FOREIGN_PID, &now, 1);
    assert_eq!(
        good_marker,
        json_for(&key, 496, 280, "baut").replace(
            '}',
            &format!(",\"pid\":{FOREIGN_PID},\"since\":\"{now}\",\"versuche\":1}}")
        )
    );
    assert!(marker_json(&key, FOREIGN_PID, &now, 7).ends_with(",\"versuche\":7}"));
    // Gültige Marke: keine Miniatur, aber eine Marke (frisch → Busy).
    fs::write(dir.join(PREVIEW_JSON), &good_marker).unwrap();
    assert!(load_cached(&dir, &key).is_none(), "baut ist keine Miniatur");
    assert!(matches!(
        thumbnail_for(&dir, &key, &BuildLimits::default()).thumb,
        Thumb::Busy
    ));
    // Ungültige Marken gelten als keine Marke: es wird gebaut.
    let base = json_for(&key, 496, 280, "baut");
    let since = format!(",\"since\":\"{now}\"");
    let pid = format!(",\"pid\":{FOREIGN_PID}");
    let tries = ",\"versuche\":1";
    for (what, text) in [
        (
            "ohne pid und since",
            base.replace('}', &format!("{tries}}}")),
        ),
        ("ohne since", base.replace('}', &format!("{pid}{tries}}}"))),
        ("ohne pid", base.replace('}', &format!("{since}{tries}}}"))),
        (
            "ohne versuche",
            base.replace('}', &format!("{pid}{since}}}")),
        ),
        ("versuche 0", good_marker.replace(tries, ",\"versuche\":0")),
        (
            "versuche als Text",
            good_marker.replace(tries, ",\"versuche\":\"1\""),
        ),
        (
            "versuche doppelt",
            good_marker.replace(tries, &format!("{tries}{tries}")),
        ),
        (
            "since falsch",
            good_marker.replace(&now, "2026-10-24 12:00:00"),
        ),
        ("pid als Text", good_marker.replace(&pid, ",\"pid\":\"1\"")),
        ("pid negativ", good_marker.replace(&pid, ",\"pid\":-1")),
        (
            "pid doppelt",
            good_marker.replace(&pid, &format!("{pid}{pid}")),
        ),
        ("Zusatzschlüssel", good_marker.replace('}', ",\"x\":1}")),
    ] {
        fs::write(dir.join(PREVIEW_JSON), &text).unwrap();
        assert!(load_cached(&dir, &key).is_none(), "{what}");
        let out = thumbnail_for(&dir, &key, &BuildLimits::default());
        assert!(
            matches!(out.thumb, Thumb::NoBodies),
            "{what}: {:?}",
            out.thumb
        );
        assert!(out.stored, "{what}");
    }
    // `pid`/`since`/`versuche` gehören nur zu „baut“.
    for art in ["bild", "leer", "zu_aufwendig", "fehlgeschlagen"] {
        let text = json_for(&key, 496, 280, art).replace('}', &format!("{pid}{since}}}"));
        fs::write(dir.join(PREVIEW_JSON), &text).unwrap();
        assert!(load_cached(&dir, &key).is_none(), "{art} mit pid");
        let text = json_for(&key, 496, 280, art).replace('}', &format!("{tries}}}"));
        fs::write(dir.join(PREVIEW_JSON), &text).unwrap();
        assert!(load_cached(&dir, &key).is_none(), "{art} mit versuche");
    }
    // `fehlgeschlagen` und `baut` sind nie ein Bild, auch mit einem gültigen PNG daneben.
    let png = encode_png(&Image {
        width: 496,
        height: 280,
        rgba: vec![9; 496 * 280 * 4],
    })
    .unwrap();
    fs::write(dir.join(PREVIEW_PNG), png).unwrap();
    fs::write(
        dir.join(PREVIEW_JSON),
        json_for(&key, 496, 280, "fehlgeschlagen"),
    )
    .unwrap();
    assert!(matches!(load_cached(&dir, &key), Some(Thumb::Impossible)));
    fs::write(dir.join(PREVIEW_JSON), &good_marker).unwrap();
    assert!(load_cached(&dir, &key).is_none(), "baut mit PNG daneben");
    // Andere Schreibweise der Art: unbekannt.
    for art in ["Baut", "fehlgeschlagen ", "fehler"] {
        fs::write(dir.join(PREVIEW_JSON), json_for(&key, 496, 280, art)).unwrap();
        assert!(load_cached(&dir, &key).is_none(), "{art:?}");
    }
}
