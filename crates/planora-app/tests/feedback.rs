//! Rückmeldung ohne Oberfläche: Bereinigung, Adresse mit Kürzung, Rückmeldungsordner.

use planora_app::build_info::{BuildInfo, GpuInfo};
use planora_app::feedback::*;
use planora_project::fsutil::new_id;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

fn temp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("planora-feedback-{tag}-{}", new_id()));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn steps(n: usize) -> String {
    (0..n)
        .map(|i| {
            format!(
                "14:{:02}:{:02} Schritt {i}: Skizze extrudieren",
                i / 60,
                i % 60
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn log_lines(n: usize) -> String {
    (0..n)
        .map(|i| format!("2026-10-26T14:03:22Z INFO Zeile {i} mit etwas Text"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn report(kind: Kind, description: &str) -> Report {
    Report {
        kind,
        area: None,
        project_file: None,
        description: description.to_string(),
        parts: vec![
            (Part::Version, "0.2.0-dev.1 (Dev)".to_string()),
            (Part::System, "System: Windows 11 (Build 26100)".to_string()),
            (Part::Grafik, "Testkarte · Treiber 1.2 · Vulkan".to_string()),
            (Part::Schritte, steps(3)),
        ],
    }
}

/// Wert eines Parameters der Adresse (noch kodiert).
fn param<'a>(url: &'a str, name: &str) -> Option<&'a str> {
    let query = url.split_once('?')?.1;
    query
        .split('&')
        .find_map(|kv| kv.strip_prefix(&format!("{name}=")))
}

fn no_broken_escape(url: &str) {
    let b = url.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            assert!(i + 2 < b.len(), "halbe Folge am Ende");
            assert!(b[i + 1].is_ascii_hexdigit() && b[i + 2].is_ascii_hexdigit());
            i += 3;
        } else {
            i += 1;
        }
    }
}

#[test]
fn encode_examples() {
    assert_eq!(
        percent_encode("Größe\nä &=#%+"),
        "Gr%C3%B6%C3%9Fe%0A%C3%A4%20%26%3D%23%25%2B"
    );
    assert_eq!(percent_encode("a\r\nb"), "a%0Ab");
    assert_eq!(percent_encode("👍"), "%F0%9F%91%8D");
    assert_eq!(percent_encode("Az09-._~"), "Az09-._~");
}

#[test]
fn short_report_fits_with_all_parts() {
    let b = issue_url(
        &report(Kind::Fehler, "Beim Speichern friert alles ein."),
        false,
    );
    assert!(
        b.url.starts_with(
            "https://github.com/KaelanTesseract/PlanoraCAD/issues/new?template=fehler.yml"
        ),
        "{}",
        b.url
    );
    assert!(b.url.contains("&passiert="));
    assert!(b.url.contains("&version="));
    assert!(b.url.contains("&schritte="));
    assert!(b.url.contains("&system="));
    assert!(b.url.contains("&grafik="));
    assert!(b.dropped.is_empty());
    assert!(!b.steps_cut);
    assert!(!b.description_cut);
    assert_eq!(
        param(&b.url, "title"),
        Some(percent_encode("Fehler: Beim Speichern friert alles ein.").as_str())
    );
}

#[test]
fn wish_and_question_use_the_wish_form_with_their_prefix() {
    for (kind, prefix, label) in [
        (Kind::Wunsch, "Wunsch: ", "Wunsch"),
        (Kind::Frage, "Frage: ", "Frage"),
    ] {
        assert_eq!(kind.template(), "wunsch.yml");
        assert_eq!(kind.title_prefix(), prefix);
        assert_eq!(kind.label(), label);
        assert_eq!(kind.description_field(), FIELD_WUNSCH);
        let b = issue_url(&report(kind, "Mehr Farben im Baum\nzweite Zeile"), false);
        assert!(b.url.contains("issues/new?template=wunsch.yml&"));
        assert_eq!(
            param(&b.url, "title"),
            Some(percent_encode(&format!("{prefix}Mehr Farben im Baum")).as_str())
        );
        assert!(b.url.contains("&wunsch="));
        // alle Angaben gesammelt in `angaben`, keine Fehler-Felder
        let angaben = param(&b.url, FIELD_ANGABEN).unwrap();
        assert!(angaben.contains(&percent_encode("0.2.0-dev.1")));
        assert!(angaben.contains(&percent_encode("Schritt 2")));
        for f in [
            FIELD_VERSION,
            FIELD_SYSTEM,
            FIELD_GRAFIK,
            FIELD_SCHRITTE,
            FIELD_PASSIERT,
        ] {
            assert!(param(&b.url, f).is_none(), "{f}");
        }
    }
    assert_eq!(Kind::Fehler.template(), "fehler.yml");
    assert_eq!(Kind::Fehler.title_prefix(), "Fehler: ");
    assert_eq!(Kind::Fehler.description_field(), FIELD_PASSIERT);
}

#[test]
fn area_goes_to_the_start_of_the_text_not_into_a_parameter() {
    let mut r = report(Kind::Fehler, "Linie lässt sich nicht ziehen");
    r.area = Some("Skizze".to_string());
    let b = issue_url(&r, false);
    assert!(param(&b.url, "bereich").is_none());
    assert!(
        param(&b.url, FIELD_PASSIERT)
            .unwrap()
            .starts_with(&percent_encode("Bereich: Skizze\n\nLinie"))
    );
    let mut r = report(Kind::Wunsch, "Mehr Farben");
    r.area = Some("3D".to_string());
    let b = issue_url(&r, false);
    assert!(param(&b.url, "bereich").is_none());
    assert!(
        param(&b.url, FIELD_ANGABEN)
            .unwrap()
            .starts_with(&percent_encode("Bereich: 3D\n"))
    );
    assert!(full_text(&r).contains("Bereich: 3D"));
}

#[test]
fn attach_hint_is_added_to_the_description_field() {
    let b = issue_url(&report(Kind::Fehler, "Kurz und klar beschrieben"), true);
    assert!(
        param(&b.url, FIELD_PASSIERT)
            .unwrap()
            .ends_with(&percent_encode(ATTACH_HINT))
    );
    let b = issue_url(&report(Kind::Fehler, "Kurz und klar beschrieben"), false);
    assert!(!b.url.contains(&percent_encode(ATTACH_HINT)));
}

#[test]
fn long_log_is_dropped_first_then_oldest_steps() {
    let mut r = report(Kind::Fehler, "Absturz beim Export");
    r.parts.retain(|(p, _)| *p != Part::Schritte);
    r.parts.push((Part::Schritte, steps(50)));
    r.parts.push((Part::Protokoll, log_lines(500)));
    let b = issue_url(&r, true);
    assert_eq!(b.dropped, [Part::Protokoll]);
    assert!(b.steps_cut);
    assert!(!b.description_cut);
    assert!(b.url.len() < 2000, "{}", b.url.len());
    assert!(param(&b.url, FIELD_PROTOKOLL).is_none());
    let schritte = param(&b.url, FIELD_SCHRITTE).unwrap();
    assert!(schritte.contains(&percent_encode("Schritt 49")));
    assert!(!schritte.contains(&percent_encode("Schritt 0:")));
    // die Datei hat alles
    let full = full_text(&r);
    assert!(full.contains("Schritt 0:"));
    assert!(full.contains("Zeile 499"));
}

#[test]
fn description_is_never_cut_while_it_fits_alone() {
    let desc = "x".repeat(1500);
    let mut r = report(Kind::Fehler, &desc);
    r.parts.retain(|(p, _)| *p != Part::Schritte);
    r.parts.push((Part::Schritte, steps(50)));
    r.parts.push((Part::Protokoll, log_lines(50)));
    let b = issue_url(&r, true);
    assert!(b.url.len() < 2000);
    assert!(!b.description_cut);
    assert!(param(&b.url, FIELD_PASSIERT).unwrap().contains(&desc));
}

#[test]
fn oversized_description_is_cut_at_a_char_boundary_with_hint() {
    let desc = format!("{}👍", "ü".repeat(5000));
    let b = issue_url(&report(Kind::Fehler, &desc), false);
    assert!(b.url.len() < 2000, "{}", b.url.len());
    assert!(b.description_cut);
    // gekürzt → Hinweis und Datei-Hinweis kommen von selbst
    assert!(
        b.url.ends_with(&percent_encode(&format!(
            "{KUERZUNG_HINWEIS}\n\n{ATTACH_HINT}"
        ))),
        "{}",
        b.url
    );
    assert!(b.needs_file);
    assert_eq!(
        b.dropped,
        [Part::Schritte, Part::Grafik, Part::System, Part::Version]
    );
    no_broken_escape(&b.url);
    let full = full_text(&report(Kind::Fehler, &desc));
    assert_eq!(full.matches('ü').count(), 5000);
    assert!(full.contains('👍'));
}

#[test]
fn url_never_exceeds_2000_for_any_mix() {
    for len in [0usize, 10, 1999, 3000] {
        for c in ["a", "ä", "&", "\n", "👍"] {
            for kind in [Kind::Fehler, Kind::Wunsch, Kind::Frage] {
                for with_file in [false, true] {
                    let mut r = report(kind, &c.repeat(len));
                    r.area = Some("Tastenkürzel".to_string());
                    r.parts.push((Part::Protokoll, log_lines(500)));
                    r.parts
                        .push((Part::Absturzbericht, "Panik in main.rs:12".to_string()));
                    let b = issue_url(&r, with_file);
                    assert!(
                        b.url.len() < 2000,
                        "{len} × {c:?} {kind:?}: {}",
                        b.url.len()
                    );
                    no_broken_escape(&b.url);
                }
            }
        }
    }
}

#[test]
fn huge_automatic_parts_still_fit() {
    let mut r = report(Kind::Fehler, "Kurz");
    r.parts.push((Part::Absturzbericht, "ö".repeat(4000)));
    r.parts.push((Part::Grafik, "g".repeat(3000)));
    let b = issue_url(&r, true);
    assert!(b.url.len() < 2000);
    assert!(param(&b.url, FIELD_PASSIERT).unwrap().contains("Kurz"));
}

fn who() -> Personal {
    Personal {
        user_names: vec!["Max Müller".to_string()],
        home_dirs: vec![r"C:\Users\Max Müller".to_string()],
        project_paths: vec![],
    }
}

#[test]
fn sanitize_user_name_variants() {
    let who = who();
    for text in [
        r"Fehler in C:\Users\Max Müller\AppData\x.log",
        "c:/users/max müller/AppData/x.log",
        r"C:\USERS\MAX MÜLLER\AppData\x.log",
        "%USERPROFILE% ist C:\\Users\\Max Müller",
    ] {
        let out = sanitize(text, &who);
        assert!(out.contains("<Benutzer>"), "{out}");
        assert!(!out.to_lowercase().contains("müller"), "{out}");
        assert!(!out.to_lowercase().contains("max"), "{out}");
    }
    let out = sanitize("/home/max/.config/planora/x.log", &Personal::default());
    assert_eq!(out, "/home/<Benutzer>/.config/planora/x.log");
    // unbekannter Name, aber unter Users
    let out = sanitize(r"C:\Users\Erika\x.log und weiter", &Personal::default());
    assert_eq!(out, r"C:\Users\<Benutzer>\x.log und weiter");
    // Wörter, die den Namen nur enthalten, bleiben
    let w = Personal {
        user_names: vec!["max".to_string()],
        ..Personal::default()
    };
    assert_eq!(sanitize("Maximale Länge", &w), "Maximale Länge");
}

#[test]
fn sanitize_project_paths_to_file_name() {
    let none = Personal::default();
    let out = sanitize(r"lade D:\Kunden\Geheim\Halter.pcad\dokument.json", &none);
    assert_eq!(out, r"lade Halter.pcad\dokument.json");
    let out = sanitize(r"gespeichert: D:\Kunden\Geheim\Halter.pcad", &none);
    assert_eq!(out, "gespeichert: Halter.pcad");
    let out = sanitize("/srv/Kunden/Geheim Projekt/Halter.PCAD fertig", &none);
    assert_eq!(out, "Halter.PCAD fertig");
    let with_project = Personal {
        project_paths: vec![PathBuf::from(r"E:\Geheim\Ablage")],
        ..Personal::default()
    };
    let out = sanitize(r"öffne E:\Geheim\Ablage\teil.json", &with_project);
    assert!(!out.contains("Geheim"), "{out}");
    assert!(out.contains("Ablage"), "{out}");
}

#[test]
fn sanitize_mail_addresses() {
    let none = Personal::default();
    assert_eq!(
        sanitize("Absender max.mueller@example.org, bitte", &none),
        "Absender <Mailadresse>, bitte"
    );
    assert_eq!(sanitize("<a@b.de>", &none), "<<Mailadresse>>");
    assert_eq!(sanitize("3 @ 4", &none), "3 @ 4");
    assert_eq!(sanitize("x@localhost", &none), "x@localhost");
    // Versionsangaben sind keine Mailadressen
    assert_eq!(
        sanitize("wgpu@0.19.3 geladen", &none),
        "wgpu@0.19.3 geladen"
    );
    assert_eq!(sanitize("a@b.c", &none), "a@b.c");
}

#[test]
fn sanitize_name_as_word_in_a_log_line() {
    // nur die Namensregel kann das treffen (kein Benutzerordner, kein Pfad)
    let w = Personal {
        user_names: vec!["Max Müller".to_string()],
        ..Personal::default()
    };
    assert_eq!(
        sanitize("Fehler bei Max Müller beim Laden", &w),
        "Fehler bei <Benutzer> beim Laden"
    );
    assert_eq!(
        sanitize("Fehler bei MAX MÜLLER beim Laden", &w),
        "Fehler bei <Benutzer> beim Laden"
    );
}

#[test]
fn sanitize_home_dir_outside_users() {
    // Name zu kurz für die Namensregel, Ordner nicht unter Users/home
    let w = Personal {
        home_dirs: vec![r"D:\Profile\mm".to_string()],
        user_names: vec!["mm".to_string()],
        ..Personal::default()
    };
    assert_eq!(
        sanitize(r"lese d:/profile/MM/planora/x.log", &w),
        r"lese <Benutzer>/planora/x.log"
    );
}

#[test]
fn sanitize_onedrive_company_name() {
    let out = sanitize(
        r"C:\Users\Erika\OneDrive - Geheim GmbH\Teile\x.log",
        &Personal::default(),
    );
    assert_eq!(out, r"C:\Users\<Benutzer>\OneDrive\Teile\x.log");
    let out = sanitize("/mnt/c/onedrive - Firma AG/x", &Personal::default());
    assert_eq!(out, "/mnt/c/OneDrive/x");
}

#[test]
fn automatic_parts_go_before_the_description_is_cut() {
    let backtrace = (0..15)
        .map(|i| format!("  {i}: planora_app::commands::run_action at src\\commands.rs:{i}23"))
        .collect::<Vec<_>>()
        .join("\n");
    let desc = "x".repeat(900);
    let r = Report {
        kind: Kind::Fehler,
        area: None,
        project_file: None,
        description: desc.clone(),
        parts: vec![
            (Part::Version, "0.2.0-dev.1 (Dev)".to_string()),
            (Part::Absturzbericht, backtrace),
        ],
    };
    let b = issue_url(&r, false);
    assert!(b.url.len() < 2000, "{}", b.url.len());
    assert!(!b.description_cut);
    assert!(b.url.contains(&desc));
    assert_eq!(b.dropped, [Part::Absturzbericht]);
    assert!(b.needs_file);
    assert!(b.url.contains("&version="));
}

#[test]
fn url_stays_below_the_limit_near_the_boundary() {
    assert_eq!(MAX_URL_LEN, 2000);
    let mut seen_1999 = false;
    for n in 1700..2000 {
        let r = Report {
            kind: Kind::Fehler,
            area: None,
            project_file: None,
            description: "y".repeat(n),
            parts: vec![],
        };
        for with_file in [false, true] {
            let b = issue_url(&r, with_file);
            assert!(b.url.len() < 2000, "{n}: {}", b.url.len());
            seen_1999 |= b.url.len() == 1999;
        }
    }
    assert!(seen_1999, "die Grenze wird ausgenutzt");
}

#[test]
fn anything_cut_adds_the_file_hint_by_itself() {
    let b = issue_url(&report(Kind::Fehler, &"y".repeat(3000)), false);
    assert!(b.needs_file);
    assert!(b.description_cut);
    assert!(b.url.contains(&percent_encode(ATTACH_HINT)));
    let b = issue_url(&report(Kind::Fehler, "kurz und knapp"), false);
    assert!(!b.needs_file);
    assert!(!b.url.contains(&percent_encode(ATTACH_HINT)));
    let b = issue_url(&report(Kind::Fehler, "kurz und knapp"), true);
    assert!(b.needs_file);
}

#[test]
fn lone_carriage_return_is_a_line_break() {
    assert_eq!(percent_encode("a\rb\r\nc\n\rd"), "a%0Ab%0Ac%0A%0Ad");
    let r = Report {
        kind: Kind::Wunsch,
        area: None,
        project_file: None,
        description: "a\r\nb\rc".to_string(),
        parts: vec![(Part::Schritte, "s1\r\ns2".to_string())],
    };
    let full = full_text(&r);
    assert!(!full.contains('\r'));
    assert!(full.contains("a\nb\nc"));
    assert!(full.contains("s1\ns2"));
}

#[test]
fn gpu_driver_is_sanitized_and_cut() {
    let info = BuildInfo {
        version: planora_app::running_version(),
        os: "Windows 11".to_string(),
        gpu: Some(GpuInfo {
            name: "Testkarte".to_string(),
            driver: format!(r"C:\Users\Max Müller\treiber {}", "z".repeat(300)),
            backend: "Vulkan".to_string(),
        }),
    };
    let line = gpu_line(&info, &who());
    assert!(!line.contains("Müller"), "{line}");
    assert!(line.contains("<Benutzer>"));
    let driver = line
        .split(" · ")
        .nth(1)
        .unwrap()
        .trim_start_matches("Treiber ");
    assert!(
        driver.chars().count() <= MAX_DRIVER_CHARS,
        "{}",
        driver.chars().count()
    );
}

#[test]
fn personal_from_env_takes_the_last_part_of_the_home_dir() {
    let p = Personal::from_env(vec![PathBuf::from("x")]);
    assert_eq!(p.project_paths, [PathBuf::from("x")]);
    for h in &p.home_dirs {
        let last = h.rsplit(['\\', '/']).find(|s| !s.is_empty()).unwrap_or("");
        if last.chars().count() >= 3 {
            assert!(p.user_names.iter().any(|u| u == last), "{last}");
        }
    }
}

#[test]
fn parts_defaults_and_labels() {
    use Part::*;
    for p in [Version, System, Grafik, Schritte] {
        assert!(p.default_on(), "{p:?}");
    }
    // Absturzbericht: nur aus der Absturz-Rückfrage vorausgewählt (tests/feedback_dialog.rs).
    for p in [Protokoll, Bild, Projektdatei, Absturzbericht] {
        assert!(!p.default_on(), "{p:?}");
    }
    for p in [
        Version,
        System,
        Grafik,
        Schritte,
        Protokoll,
        Bild,
        Projektdatei,
        Absturzbericht,
    ] {
        assert!(!p.label().is_empty());
    }
}

#[test]
fn bundle_dir_names_and_collision() {
    let data = temp("dir");
    let now = chrono::NaiveDate::from_ymd_opt(2026, 10, 26)
        .unwrap()
        .and_hms_opt(14, 3, 22)
        .unwrap();
    let d1 = bundle_dir(&data, now);
    assert_eq!(d1, data.join(FEEDBACK_DIR).join("2026-10-26_14-03-22"));
    std::fs::create_dir_all(&d1).unwrap();
    let d2 = bundle_dir(&data, now);
    assert_eq!(d2, data.join(FEEDBACK_DIR).join("2026-10-26_14-03-22_2"));
    std::fs::create_dir_all(&d2).unwrap();
    assert_eq!(
        bundle_dir(&data, now),
        data.join(FEEDBACK_DIR).join("2026-10-26_14-03-22_3")
    );
}

#[test]
fn write_bundle_writes_full_text() {
    let dir = temp("write").join("a").join("b");
    let mut r = report(Kind::Fehler, &"ü".repeat(5000));
    r.parts.push((Part::Protokoll, log_lines(500)));
    let path = write_bundle(&dir, &r).unwrap();
    assert_eq!(path, dir.join(FEEDBACK_FILE));
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text, full_text(&r));
    assert_eq!(text.matches('ü').count(), 5000);
    assert!(text.contains("Zeile 499"));
    for p in [Part::Version, Part::Protokoll, Part::Schritte] {
        assert!(text.contains(p.label()), "{p:?}");
    }
}

#[test]
fn write_bundle_into_unwritable_dir_is_an_error_not_a_panic() {
    let base = temp("blocked");
    let file = base.join("eine-datei");
    std::fs::write(&file, b"x").unwrap();
    assert!(write_bundle(&file.join("drunter"), &report(Kind::Fehler, "abc")).is_err());
}

struct FakeOpen(RefCell<Vec<PathBuf>>);

impl SystemOpen for FakeOpen {
    fn open_folder(&self, dir: &Path) -> bool {
        self.0.borrow_mut().push(dir.to_path_buf());
        true
    }
}

#[test]
fn system_open_can_be_replaced_in_tests() {
    let fake = FakeOpen(RefCell::new(Vec::new()));
    let opener: &dyn SystemOpen = &fake;
    assert!(opener.open_folder(Path::new("ordner")));
    assert_eq!(fake.0.borrow().as_slice(), [PathBuf::from("ordner")]);
    let _real: &dyn SystemOpen = &ExplorerOpen;
}

// ---- Webformular ----

fn web_report(kind: Kind, desc: &str) -> Report {
    let mut r = report(kind, desc);
    r.area = Some("Tastenkürzel".to_string());
    r
}

#[test]
fn webform_constants() {
    assert_eq!(
        WEBFORM_URL,
        "https://dennis-guse.de/planoracad/rueckmeldung.php"
    );
    assert_eq!(
        WEBFORM_URL_EN,
        "https://dennis-guse.de/en/planoracad/feedback.php"
    );
}

#[test]
fn webform_fields_art_bereich_and_names() {
    let b = webform_url(&web_report(Kind::Fehler, "Es hängt"), false);
    assert!(
        b.url.starts_with(&format!("{WEBFORM_URL}?art=fehler&")),
        "{}",
        b.url
    );
    assert_eq!(param(&b.url, "bereich"), Some("Tastenk%C3%BCrzel"));
    assert_eq!(param(&b.url, "passiert"), Some("Es%20h%C3%A4ngt"));
    for f in ["version", "system", "grafik", "schritte"] {
        assert!(param(&b.url, f).is_some(), "{f}: {}", b.url);
    }
    for f in ["ziel", "erwartet", "template", "title", "wunsch", "angaben"] {
        assert!(param(&b.url, f).is_none(), "{f}: {}", b.url);
    }
    assert!(!b.url.contains("Bereich%3A"));
    assert!(!b.needs_file);
    for (kind, art) in [(Kind::Wunsch, "wunsch"), (Kind::Frage, "frage")] {
        let b = webform_url(&web_report(kind, "Mehr Platz"), false);
        assert_eq!(param(&b.url, "art"), Some(art));
        assert_eq!(param(&b.url, "wunsch"), Some("Mehr%20Platz"));
        assert!(param(&b.url, "passiert").is_none());
        assert!(param(&b.url, "angaben").is_none());
    }
}

#[test]
fn webform_no_area_means_no_parameter() {
    let b = webform_url(&report(Kind::Fehler, "Es hängt"), false);
    assert!(param(&b.url, "bereich").is_none(), "{}", b.url);
}

#[test]
fn webform_encoding() {
    let b = webform_url(
        &web_report(Kind::Wunsch, "Größe & Form #1 a+b\nZeile 2"),
        false,
    );
    assert_eq!(
        param(&b.url, "wunsch"),
        Some("Gr%C3%B6%C3%9Fe%20%26%20Form%20%231%20a%2Bb%0AZeile%202")
    );
    no_broken_escape(&b.url);
}

#[test]
fn webform_with_file_adds_hint() {
    let b = webform_url(&web_report(Kind::Fehler, "Es hängt"), true);
    assert!(b.needs_file);
    assert!(
        param(&b.url, "passiert")
            .unwrap()
            .ends_with(&percent_encode(&format!("\n\n{ATTACH_HINT}")))
    );
}

#[test]
fn webform_drops_steps_and_log_before_the_description() {
    let desc = "x".repeat(1200);
    let mut r = web_report(Kind::Fehler, &desc);
    r.parts.retain(|(p, _)| *p != Part::Schritte);
    r.parts.push((Part::Protokoll, log_lines(200)));
    r.parts.push((Part::Schritte, steps(200)));
    let b = webform_url(&r, false);
    assert!(b.url.len() < 2000, "{}", b.url.len());
    assert!(b.needs_file);
    assert!(!b.description_cut);
    assert!(b.dropped.contains(&Part::Protokoll));
    assert!(param(&b.url, "passiert").unwrap().starts_with(&desc));
    assert!(param(&b.url, "protokoll").is_none());
    // Version, System, Grafik bleiben.
    for f in ["version", "system", "grafik"] {
        assert!(param(&b.url, f).is_some(), "{f}");
    }
    assert!(
        param(&b.url, "passiert")
            .unwrap()
            .contains("rueckmeldung.txt")
    );
}

#[test]
fn webform_oversized_description_is_cut_with_file() {
    let desc = format!("{}👍", "ü".repeat(5000));
    let b = webform_url(&web_report(Kind::Fehler, &desc), false);
    assert!(b.url.len() < 2000);
    assert!(b.description_cut);
    assert!(b.needs_file);
    assert!(b.url.ends_with(&percent_encode(&format!(
        "{KUERZUNG_HINWEIS}\n\n{ATTACH_HINT}"
    ))));
    for f in ["version", "system", "grafik", "bereich"] {
        assert!(param(&b.url, f).is_some(), "{f}");
    }
    no_broken_escape(&b.url);
}

#[test]
fn webform_url_never_exceeds_2000_for_any_mix() {
    for len in [0usize, 10, 1999, 3000] {
        for c in ["a", "ä", "&", "\n", "👍"] {
            for kind in [Kind::Fehler, Kind::Wunsch, Kind::Frage] {
                for with_file in [false, true] {
                    let mut r = web_report(kind, &c.repeat(len));
                    r.parts.push((Part::Protokoll, log_lines(500)));
                    r.parts
                        .push((Part::Absturzbericht, "Panik in main.rs:12".to_string()));
                    let b = webform_url(&r, with_file);
                    assert!(b.url.len() < 2000, "{len} × {c:?} {kind:?}");
                    no_broken_escape(&b.url);
                }
            }
        }
    }
    let mut r = web_report(Kind::Fehler, "Kurz");
    r.parts.push((Part::Grafik, "g".repeat(3000)));
    let b = webform_url(&r, true);
    assert!(b.url.len() < 2000);
}

fn with_picture(mut r: Report) -> Report {
    r.parts.push((Part::Bild, "Bild".to_string()));
    r
}

#[test]
fn attach_hint_names_the_picture_only_when_one_goes_along() {
    assert!(
        ATTACH_HINT_PICTURE.contains("rueckmeldung.txt")
            && ATTACH_HINT_PICTURE.contains("bild.png")
    );
    for web in [false, true] {
        let build = |r: &Report, with_file: bool| {
            if web {
                webform_url(r, with_file)
            } else {
                issue_url(r, with_file)
            }
        };
        let plain = build(&web_report(Kind::Fehler, "Es hängt"), true);
        assert!(plain.url.contains(&percent_encode(ATTACH_HINT)), "{web}");
        assert!(!plain.url.contains("bild.png"), "{web}");
        let pic = build(&with_picture(web_report(Kind::Fehler, "Es hängt")), true);
        assert!(
            pic.url.contains(&percent_encode(ATTACH_HINT_PICTURE)),
            "{web}"
        );
        assert!(!pic.url.contains(&percent_encode(ATTACH_HINT)), "{web}");
        // Auch wenn nur gekürzt wurde und `with_file` nicht gesetzt war.
        let long = build(
            &with_picture(web_report(Kind::Fehler, &"x".repeat(5000))),
            false,
        );
        assert!(long.needs_file && long.description_cut, "{web}");
        assert!(
            long.url.contains(&percent_encode(ATTACH_HINT_PICTURE)),
            "{web}"
        );
    }
}

#[test]
fn webform_system_field_is_a_single_line_but_github_keeps_lines() {
    let nl = char::from(10);
    let system = format!("Windows 11 (Build 26100){nl}Bildschirm: 1920 × 1080");
    let mut r = web_report(Kind::Fehler, "Es hängt");
    r.parts.retain(|(p, _)| *p != Part::System);
    r.parts.push((Part::System, system.clone()));
    let web = webform_url(&r, false);
    assert_eq!(
        param(&web.url, "system"),
        Some(percent_encode("Windows 11 (Build 26100), Bildschirm: 1920 × 1080").as_str())
    );
    let gh = issue_url(&r, false);
    assert_eq!(
        param(&gh.url, "system"),
        Some(percent_encode(&system).as_str())
    );
}

fn web_report_with(parts: &[(Part, usize)]) -> Report {
    let mut r = web_report(Kind::Fehler, "Kurz");
    for (p, n) in parts {
        r.parts.retain(|(q, _)| q != p);
        r.parts.push((*p, "g".repeat(*n)));
    }
    r
}

#[test]
fn webform_last_resort_drops_graphics_then_system_then_version() {
    // Nur die Grafikkarte ist riesig: nur sie fällt weg, die Beschreibung bleibt ganz.
    let b = webform_url(&web_report_with(&[(Part::Grafik, 1900)]), false);
    assert!(b.url.len() < 2000);
    assert!(b.dropped.contains(&Part::Grafik));
    assert!(!b.dropped.contains(&Part::System) && !b.dropped.contains(&Part::Version));
    assert!(!b.description_cut, "die Beschreibung passt ganz");
    assert!(param(&b.url, "system").is_some() && param(&b.url, "version").is_some());
    assert!(param(&b.url, "grafik").is_none());

    // Grafikkarte und System riesig: beide fallen, die Version bleibt.
    let b = webform_url(
        &web_report_with(&[(Part::Grafik, 1900), (Part::System, 1900)]),
        false,
    );
    assert!(b.url.len() < 2000);
    assert!(b.dropped.contains(&Part::Grafik) && b.dropped.contains(&Part::System));
    assert!(!b.dropped.contains(&Part::Version));
    assert!(param(&b.url, "version").is_some());
    assert!(!b.description_cut);

    // Alle drei riesig: Reihenfolge Grafikkarte, System, Version.
    let b = webform_url(
        &web_report_with(&[
            (Part::Grafik, 1900),
            (Part::System, 1900),
            (Part::Version, 1900),
        ]),
        true,
    );
    assert!(b.url.len() < 2000);
    let order: Vec<Part> = b
        .dropped
        .iter()
        .copied()
        .filter(|p| matches!(p, Part::Grafik | Part::System | Part::Version))
        .collect();
    assert_eq!(order, vec![Part::Grafik, Part::System, Part::Version]);
    assert!(param(&b.url, "passiert").unwrap().starts_with("Kurz"));
    no_broken_escape(&b.url);
}

#[test]
fn webform_address_boundary_is_below_2000() {
    assert_eq!(MAX_URL_LEN, 2000);
    let build = |n: usize| webform_url(&web_report(Kind::Fehler, &"a".repeat(n)), false);
    let base = build(0).url.len();
    // Jedes Zeichen der Beschreibung kostet genau ein Zeichen der Adresse.
    let fits = build(1999 - base);
    assert_eq!(fits.url.len(), 1999);
    assert!(!fits.needs_file && !fits.description_cut && fits.dropped.is_empty());
    let over = build(2000 - base);
    assert!(over.url.len() < 2000);
    assert!(over.needs_file, "2000 Zeichen passen nicht mehr");
}

#[test]
fn attach_hint_names_the_project_zip_when_it_goes_along() {
    assert_eq!(attach_hint_for(&["rueckmeldung.txt"]), ATTACH_HINT);
    assert_eq!(
        attach_hint_for(&["rueckmeldung.txt", "bild.png"]),
        ATTACH_HINT_PICTURE
    );
    assert_eq!(
        attach_hint_for(&["rueckmeldung.txt", "bild.png", "Halter.pcad.zip"]),
        "Dateien hier anhängen: rueckmeldung.txt, bild.png und Halter.pcad.zip"
    );
    for web in [false, true] {
        let mut r = web_report(Kind::Fehler, "Es hängt");
        r.project_file = Some("Halter.pcad.zip".to_string());
        let b = if web {
            webform_url(&r, true)
        } else {
            issue_url(&r, true)
        };
        assert!(
            b.url.contains(&percent_encode(
                "Dateien hier anhängen: rueckmeldung.txt und Halter.pcad.zip"
            )),
            "{web}: {}",
            b.url
        );
    }
}
