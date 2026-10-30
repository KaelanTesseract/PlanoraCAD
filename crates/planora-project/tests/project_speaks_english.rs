//! Task 13: Die Meldungen von `planora-project` (Fehlertyp, Namensprüfung, Bibliothek,
//! Versionsspeicher, Dateiformat) erscheinen unter `with_lang(&english())` auf Englisch; unter
//! Deutsch bleiben sie wie bisher.

use planora_feature::{Document, FeatureKind, Snapshot};
use planora_i18n::{english, german, with_lang};
use planora_math::DVec3;
use planora_project::docstore::*;
use planora_project::fsutil::{new_id, validate_name};
use planora_project::library::Library;
use planora_project::{ProjectError, base64, decode_version, encode_version};
use std::path::PathBuf;

fn assert_english(what: &str, text: &str) {
    assert!(!text.is_empty(), "{what}: leer");
    assert!(
        !text.chars().any(|c| "äöüßÄÖÜ„“".contains(c)),
        "{what}: {text}"
    );
    let looks_like_key = !text.contains(' ')
        && text.contains('-')
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    assert!(!looks_like_key, "{what}: Schlüssel statt Text: {text}");
}

fn temp(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("planora-en-{tag}-{}", new_id()))
}

fn snap(n: usize) -> Snapshot {
    let mut d = Document::new();
    for i in 0..n {
        d.add_feature(
            format!("Q{i}"),
            FeatureKind::Box {
                size: DVec3::splat(1.0 + i as f64),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    }
    d.snapshot()
}

#[test]
fn error_type_speaks_english() {
    let errors = vec![
        ProjectError::Io {
            path: "a.json".into(),
            msg: "x".into(),
        },
        ProjectError::Corrupt {
            path: "a.json".into(),
            msg: "x".into(),
        },
        ProjectError::TooNew {
            path: "a.json".into(),
            found: 99,
        },
        ProjectError::OutsideRoot,
        ProjectError::Exists("Name".into()),
        ProjectError::NotFound("Name".into()),
        ProjectError::Locked {
            since: "2026-01-01".into(),
        },
        ProjectError::LockLost,
        ProjectError::NeedsBranch,
    ];
    with_lang(&english(), || {
        for e in &errors {
            assert_english(&format!("{e:?}"), &e.to_string());
        }
        assert_eq!(
            ProjectError::Exists("Name".into()).to_string(),
            "\"Name\" already exists."
        );
        assert_eq!(
            ProjectError::TooNew {
                path: "a.json".into(),
                found: 99
            }
            .to_string(),
            "The file \"a.json\" was created with a newer program version (format 99)."
        );
        assert_english("base64", &base64::Base64Error.to_string());
    });
    with_lang(&german(), || {
        assert_eq!(
            ProjectError::Exists("Name".into()).to_string(),
            "„Name“ gibt es schon."
        );
    });
}

#[test]
fn names_and_library_speak_english() {
    let bad_names = [
        "",
        &"x".repeat(81),
        ".hidden",
        "a/b",
        "CON",
        " lead.pcad-ish.",
    ];
    let lib_root = temp("lib");
    let lib = Library::open(&lib_root).unwrap();
    with_lang(&english(), || {
        for n in bad_names {
            if let Err(e) = validate_name(n) {
                assert_english(n, &e.to_string());
            }
        }
        assert_eq!(
            validate_name("").unwrap_err().to_string(),
            "The name must not be empty."
        );
        assert_eq!(
            validate_name(&"x".repeat(81)).unwrap_err().to_string(),
            "The name is too long (at most 80 characters)."
        );
        let mut seen = 0;
        let mut check = |what: &str, e: ProjectError| {
            assert_english(what, &e.to_string());
            seen += 1;
        };
        check("project suffix", lib.create_project("a.pcad").unwrap_err());
        let p = lib.create_project("P").unwrap();
        check(
            "folder suffix",
            lib.create_folder(&p, "f.pcad").unwrap_err(),
        );
        check("exists", lib.create_project("P").unwrap_err());
        check("move project", lib.move_node(&p, &p).unwrap_err());
        check("nothing selected", lib.delete(&Vec::new()).unwrap_err());
        let f = lib.create_folder(&p, "F").unwrap();
        check("into itself", lib.move_node(&f, &f).unwrap_err());
        check(
            "not a node",
            lib.rename(&vec!["nope".into(), "x".into()], "y")
                .unwrap_err(),
        );
        assert_eq!(seen, 7);
    });
    let _ = std::fs::remove_dir_all(lib_root);
}

#[test]
fn version_store_speaks_english() {
    let dir = temp("doc");
    let dir = dir.with_extension("pcad");
    with_lang(&english(), || {
        let mut st = DocStore::create(&dir, &snap(1), "").unwrap();
        st.save(None, &snap(2), "", None).unwrap();
        let mut errors = Vec::new();
        errors.push(st.delete_version(1).unwrap_err());
        errors.push(st.delete_version(9).unwrap_err());
        errors.push(st.delete_branch("Haupt").unwrap_err());
        st.save(Some(1), &snap(3), "", Some("Zweig")).unwrap();
        st.set_active_branch("Zweig").unwrap();
        errors.push(st.delete_branch("Haupt").unwrap_err());
        errors.push(st.delete_branch("nope").unwrap_err());
        errors.push(st.save(Some(1), &snap(3), "", None).unwrap_err());
        errors.push(st.save(None, &snap(3), "", Some("")).unwrap_err());
        errors.push(st.save(None, &snap(3), &"c".repeat(501), None).unwrap_err());
        errors.push(st.restore(9, "Haupt").unwrap_err());
        errors.push(st.restore(1, "nope").unwrap_err());
        for e in &errors {
            assert_english(&format!("{e:?}"), &e.to_string());
        }
        assert_eq!(
            errors[0].to_string(),
            "Other versions are built on this one."
        );
        assert_eq!(errors[1].to_string(), "\"Version 9\" was not found.");
        // Der Kommentar der Wiederherstellung entsteht in der Sprache des Aufrufs.
        let n = st.restore(1, "Haupt").unwrap();
        assert_eq!(st.info().version(n).unwrap().comment, "Restored from v1");
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn version_tree_check_speaks_english() {
    let v = |number: u32, parent: Option<u32>| VersionInfo {
        number,
        parent,
        branch: "Haupt".into(),
        time: "2026-01-01T00:00:00Z".into(),
        comment: String::new(),
        program: "x".into(),
    };
    let info = |versions: Vec<VersionInfo>, tip: u32, active: &str, max: u32| DocumentInfo {
        id: "id".into(),
        versions,
        branches: [("Haupt".to_string(), tip)].into_iter().collect(),
        active_branch: active.into(),
        max_number: max,
    };
    let cases = vec![
        info(vec![], 1, "Haupt", 1),
        info(vec![v(2, None), v(1, Some(2))], 1, "Haupt", 2),
        info(vec![v(1, None), v(2, None)], 2, "Haupt", 2),
        info(vec![v(1, None), v(2, Some(7))], 2, "Haupt", 2),
        info(vec![v(1, None)], 1, "Haupt", 0),
        info(vec![v(1, None)], 5, "Haupt", 1),
        info(vec![v(1, None)], 1, "Andere", 1),
    ];
    with_lang(&english(), || {
        for c in &cases {
            let e = c.check().unwrap_err();
            assert_english(&e, &e);
        }
    });
    with_lang(&german(), || {
        assert_eq!(
            cases[0].check().unwrap_err(),
            "Das Dokument hat keine Version"
        );
    });
}

#[test]
fn file_format_errors_speak_english() {
    let base = encode_version(&snap(1)).unwrap();
    let base: serde_json::Value = serde_json::from_str(&base).unwrap();
    let with = |f: &dyn Fn(&mut serde_json::Value)| {
        let mut v = base.clone();
        v["format"] = 11.into();
        f(&mut v);
        v.to_string()
    };
    let cases: Vec<(&str, String, &str)> = vec![
        (
            "format 0",
            with(&|v| v["format"] = 0.into()),
            "Invalid format number 0",
        ),
        (
            "too new",
            with(&|v| v["format"] = 99.into()),
            "newer program version",
        ),
        (
            "drawings need 4",
            with(&|v| {
                v["format"] = 1.into();
                v["naechste_zeichnung"] = 5.into();
            }),
            "Drawings need file format 4 or newer",
        ),
        (
            "parameters need 9",
            with(&|v| {
                v["format"] = 1.into();
                v["naechster_parameter"] = 5.into();
            }),
            "Parameters and formulas need file format 9 or newer",
        ),
        (
            "rollbacks need 11",
            with(&|v| {
                v["format"] = 1.into();
                v["rueckrollstellen"] = serde_json::json!([1]);
            }),
            "Rollback points need file format 11 or newer",
        ),
        (
            "unknown rollback",
            with(&|v| v["rueckrollstellen"] = serde_json::json!([777])),
            "A rollback point refers to an unknown step (id 777)",
        ),
        (
            "number too large",
            with(&|v| v["features"][0]["art"]["groesse"][0] = 1e300.into()),
            "\"Q0\" size: number invalid or too large",
        ),
        (
            "unknown formula field",
            with(&|v| v["features"][0]["formeln"] = serde_json::json!({ "zzz": "1" })),
            "unknown formula field \"zzz\"",
        ),
        (
            "field without formula",
            with(&|v| v["features"][0]["formeln"] = serde_json::json!({ "radius": "1" })),
            "The field \"radius\" cannot have a formula",
        ),
        (
            "formula too long",
            with(&|v| {
                v["features"][0]["formeln"] = serde_json::json!({ "breite": "1".repeat(300) })
            }),
            "The formula is too long (at most 200 characters)",
        ),
        (
            "parameter kind",
            with(&|v| {
                v["parameter"] = serde_json::json!([
                    {"id": 1, "name": "a", "formel": "1", "art": "foo", "kommentar": ""}
                ]);
            }),
            "unknown kind \"foo\"",
        ),
        (
            "parameter twice",
            with(&|v| {
                v["parameter"] = serde_json::json!([
                    {"id": 1, "name": "a", "formel": "1", "art": "zahl", "kommentar": ""},
                    {"id": 1, "name": "b", "formel": "1", "art": "zahl", "kommentar": ""}
                ]);
            }),
            "The id occurs twice",
        ),
        (
            "mesh id",
            with(&|v| v["netze"] = serde_json::json!([{"id": 0, "daten": ""}])),
            "Mesh 0: The id is invalid",
        ),
        (
            "mesh data",
            with(&|v| v["netze"] = serde_json::json!([{"id": 1, "daten": "!!!!"}])),
            "Mesh 1: The mesh data is not a valid Base64 string",
        ),
    ];
    with_lang(&english(), || {
        for (what, text, expected) in &cases {
            let e = decode_version(text, "doc.json")
                .map(|_| ())
                .expect_err(what);
            let msg = e.to_string();
            assert_english(what, &msg);
            assert!(msg.contains(expected), "{what}: {msg}");
        }
        let e = decode_version("not json", "doc.json")
            .map(|_| ())
            .unwrap_err();
        assert_english("not json", &e.to_string());
    });
    with_lang(&german(), || {
        let e = decode_version(&cases[0].1, "doc.json")
            .map(|_| ())
            .unwrap_err();
        assert_eq!(
            e.to_string(),
            "Die Datei „doc.json“ ist beschädigt: Ungültige Formatnummer 0"
        );
    });
}
