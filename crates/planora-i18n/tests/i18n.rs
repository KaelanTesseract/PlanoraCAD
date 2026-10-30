//! Tests für planora-i18n. Diese Datei ruft nie `set_global` (Deutsch bleibt Grundsprache).

use std::path::PathBuf;

use planora_i18n::{
    Arg, available, current, english, find, fmt_num, german, load_dir, localize_decimal, parse_num,
    pick, t, t_args, tr, with_lang,
};

/// Leerer Ordner nur für einen Test (unter dem Temp-Ordner des Systems).
fn fresh_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("planora-i18n-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn german_is_the_default_before_set_global() {
    let text = std::thread::spawn(|| t("language-name")).join().unwrap();
    assert_eq!(text, "Deutsch");
    let lang = std::thread::spawn(current).join().unwrap();
    assert_eq!(lang.id(), "de");
    assert_eq!(lang.decimal(), ',');
}

#[test]
fn builtin_languages_have_name_and_separator() {
    assert_eq!(german().id(), "de");
    assert_eq!(german().name(), "Deutsch");
    assert_eq!(german().decimal(), ',');
    assert_eq!(english().id(), "en");
    assert_eq!(english().name(), "English");
    assert_eq!(english().decimal(), '.');
    let ids: Vec<String> = available().iter().map(|l| l.id().to_string()).collect();
    assert!(ids.contains(&"de".to_string()));
    assert!(ids.contains(&"en".to_string()));
    assert_eq!(find("en").unwrap().name(), "English");
    assert!(find("zz").is_none());
}

#[test]
fn with_lang_switches_only_this_thread_and_restores() {
    let (inside_tx, inside_rx) = std::sync::mpsc::channel::<()>();
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    let other = std::thread::spawn(move || {
        inside_rx.recv().unwrap();
        let text = t("language-name");
        done_tx.send(()).unwrap();
        text
    });
    let inner = with_lang(&english(), || {
        let text = t("language-name");
        inside_tx.send(()).unwrap();
        done_rx.recv().unwrap();
        text
    });
    assert_eq!(inner, "English");
    assert_eq!(t("language-name"), "Deutsch");
    assert_eq!(other.join().unwrap(), "Deutsch");
}

#[test]
fn with_lang_nests_and_restores_after_panic() {
    let text = with_lang(&english(), || {
        let nested = with_lang(&german(), || t("test-hello"));
        assert_eq!(nested, "Hallo");
        t("test-hello")
    });
    assert_eq!(text, "Hello");

    let result = std::panic::catch_unwind(|| {
        with_lang(&english(), || -> () { panic!("Absicht") });
    });
    assert!(result.is_err());
    assert_eq!(current().id(), "de");
    assert_eq!(t("test-hello"), "Hallo");
}

#[test]
fn arguments_and_plurals() {
    assert_eq!(tr!("test-args", name = "Anna"), "Hallo Anna");
    let name = String::from("Bob");
    assert_eq!(tr!("test-args", name = name), "Hallo Bob");
    assert_eq!(tr!("test-count", count = 1), "1 Datei");
    assert_eq!(tr!("test-count", count = 3usize), "3 Dateien");
    with_lang(&english(), || {
        assert_eq!(tr!("test-count", count = 1u32), "1 file");
        assert_eq!(tr!("test-count", count = 2i64), "2 files");
        assert_eq!(tr!("test-hello"), "Hello");
    });
    assert_eq!(
        t_args("test-args", &[("name", Arg::from("Carla"))]),
        "Hallo Carla"
    );
}

#[test]
fn missing_key_falls_back_to_english_then_to_id() {
    let dir = fresh_dir("fallback");
    std::fs::write(
        dir.join("xx.ftl"),
        "language-name = Testisch\ndecimal-separator = ,\ntest-hello = Hej\n",
    )
    .unwrap();
    let problems = load_dir(&dir);
    assert!(problems.is_empty(), "{problems:?}");
    let xx = find("xx").expect("xx geladen");
    assert_eq!(xx.name(), "Testisch");
    assert_eq!(xx.decimal(), ',');
    assert!(available().iter().any(|l| l.id() == "xx"));
    with_lang(&xx, || {
        assert_eq!(t("test-hello"), "Hej");
        assert_eq!(t("test-only-en"), "Only in English");
        assert_eq!(t("no-such-key-anywhere"), "no-such-key-anywhere");
    });
    assert_eq!(t("no-such-key-anywhere"), "no-such-key-anywhere");
}

#[test]
fn broken_file_is_skipped_with_a_problem() {
    let dir = fresh_dir("broken");
    std::fs::write(
        dir.join("yy.ftl"),
        "language-name = Kaputt\n= ohne Schlüssel {\n",
    )
    .unwrap();
    std::fs::write(dir.join("yz.ftl"), "decimal-separator = .\nfoo = bar\n").unwrap();
    std::fs::write(dir.join("de.ftl"), "language-name = Falsch-Deutsch\n").unwrap();
    std::fs::write(dir.join("readme.txt"), "keine Sprachdatei").unwrap();
    let problems = load_dir(&dir);
    assert_eq!(problems.len(), 3, "{problems:?}");
    for name in ["yy.ftl", "yz.ftl", "de.ftl"] {
        let hits = problems
            .iter()
            .filter(|p| p.file.file_name().unwrap() == name)
            .count();
        assert_eq!(hits, 1, "{name}: {problems:?}");
    }
    assert!(problems.iter().all(|p| !p.message.is_empty()));
    let ids: Vec<String> = available().iter().map(|l| l.id().to_string()).collect();
    assert!(!ids.contains(&"yy".to_string()));
    assert!(!ids.contains(&"yz".to_string()));
    assert_eq!(german().name(), "Deutsch");
    assert_eq!(find("de").unwrap().name(), "Deutsch");
}

/// Wie in `docs/uebersetzen.md` beschrieben: doppelter Schlüssel oder falscher Dezimaltrenner
/// machen die ganze Datei ungültig; ohne `decimal-separator` gilt der Punkt.
#[test]
fn duplicate_keys_and_bad_separator_skip_the_file_missing_separator_is_a_dot() {
    let dir = fresh_dir("rules");
    std::fs::write(
        dir.join("xd.ftl"),
        "language-name = Doppelt\nfoo = eins\nfoo = zwei\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("xs.ftl"),
        "language-name = Trenner\ndecimal-separator = ;\n",
    )
    .unwrap();
    std::fs::write(dir.join("xp.ftl"), "language-name = Punkt\n").unwrap();
    let problems = load_dir(&dir);
    let mut skipped: Vec<String> = problems
        .iter()
        .map(|p| p.file.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    skipped.sort();
    assert_eq!(skipped, ["xd.ftl", "xs.ftl"], "{problems:?}");
    assert!(find("xd").is_none());
    assert!(find("xs").is_none());
    assert_eq!(find("xp").unwrap().decimal(), '.');
}

#[test]
fn missing_dir_gives_no_problems() {
    let dir = std::env::temp_dir().join("planora-i18n-test-gibt-es-nicht-4711");
    assert!(load_dir(&dir).is_empty());
}

#[test]
fn pick_prefers_saved_then_system_then_english() {
    assert_eq!(pick(Some("de"), &[]).id(), "de");
    assert_eq!(pick(Some("zz"), &["de-AT".to_string()]).id(), "de");
    assert_eq!(
        pick(None, &["fr-FR".to_string(), "en-US".to_string()]).id(),
        "en"
    );
    assert_eq!(pick(None, &[]).id(), "en");
    assert_eq!(pick(Some("en"), &["de-DE".to_string()]).id(), "en");
}

#[test]
fn numbers_follow_the_language() {
    assert_eq!(fmt_num(38.8, 1), "38,8");
    assert_eq!(fmt_num(-0.001, 2), "0,00");
    assert_eq!(fmt_num(-1.5, 2), "-1,50");
    assert_eq!(localize_decimal("0.5 mm"), "0,5 mm");
    with_lang(&english(), || {
        assert_eq!(fmt_num(38.8, 1), "38.8");
        assert_eq!(fmt_num(-0.001, 2), "0.00");
        assert_eq!(localize_decimal("0.5 mm"), "0.5 mm");
    });
    assert_eq!(parse_num("1,5"), Some(1.5));
    assert_eq!(parse_num("1.5"), Some(1.5));
    assert_eq!(parse_num(" 2 "), Some(2.0));
    assert_eq!(parse_num("inf"), None);
    assert_eq!(parse_num("NaN"), None);
    assert_eq!(parse_num("abc"), None);
}

#[test]
fn no_isolation_marks() {
    let text = tr!("test-args", name = "A");
    assert!(!text.contains('\u{2068}'));
    assert!(!text.contains('\u{2069}'));
    assert_eq!(text, "Hallo A");
}

#[test]
fn lang_is_debug_and_clone() {
    let lang = english();
    let copy = lang.clone();
    assert_eq!(copy.id(), "en");
    assert!(format!("{lang:?}").contains("en"));
}
