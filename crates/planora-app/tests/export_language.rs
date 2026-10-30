//! Sprache der Zeichnung beim Export (Sprachplan, Task 17): Bei Zeichnungsformaten lässt sich im
//! Fenster „Exportieren“ eine eigene Sprache für die Zeichnung wählen. Das Blatt (Beschriftungen,
//! Dezimaltrenner) folgt dieser Sprache, das Fenster und seine Meldungen bleiben in der Sprache
//! des Programms. Kein Test ruft `set_language` oder `set_global` (Ruling 4): Das Programm ist
//! hier Deutsch (Vorgabe), Englisch kommt über die Wahl im Fenster bzw. `with_lang`.

use planora_app::export_dialog::*;
use planora_app::fonts;
use planora_app::state::*;
use planora_app::testing::collect_texts;
use planora_drawing::dimension::{
    DimId, DimKind, DimSpec, GeoRef, GeoRefData, Tolerance as DimTolerance,
};
use planora_drawing::sheet::SheetModel;
use planora_drawing::{
    DrawBody, DrawError, DrawingFormat, DrawingId, Paper, Placement, Projection, Scale, Sheet,
    ViewKind, ViewLines,
};
use planora_exchange::{Format, Progress};
use planora_feature::geo_ref_for_edge;
use planora_i18n::{english, with_lang};
use planora_math::{DVec3, Tolerance};
use std::fs;
use std::path::{Path, PathBuf};

const SVG: ExportFormat = ExportFormat::Drawing(DrawingFormat::Svg);
const DXF: ExportFormat = ExportFormat::Drawing(DrawingFormat::Dxf);
const PDF: ExportFormat = ExportFormat::Drawing(DrawingFormat::Pdf);

const A3_QUER: Sheet = Sheet {
    paper: Paper::A3,
    landscape: true,
};

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-export-lang-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// Fenster in Full-HD-Größe (1920 × 1080).
fn wide() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1920.0, 1080.0),
        )),
        ..Default::default()
    }
}

/// Zeichnet das Fenster dreimal (Layout setzt sich) und liefert die Texte des letzten Bilds.
fn draw(d: &mut ExportDialog, s: &mut AppState) -> String {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    let mut dir = None;
    let mut text = String::new();
    for _ in 0..3 {
        let out = ctx.run_ui(wide(), |ui| {
            assert!(d.show_in(
                ui.ctx(),
                s,
                &mut dir,
                &mut planora_app::places::Places::default()
            ));
        });
        text = collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    text
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// Verweisdaten der geraden Kante des ersten Körpers, deren beide Eckpunkte `pred` erfüllen.
fn edge_where(s: &AppState, pred: impl Fn(DVec3) -> bool) -> GeoRefData {
    let solid = &s.bodies[0].solid;
    let (id, _) = solid
        .edges
        .iter()
        .find(|(_, e)| {
            let a = solid.vertices.get(e.start).unwrap().point;
            let b = solid.vertices.get(e.end).unwrap().point;
            e.start != e.end && pred(a) && pred(b)
        })
        .expect("Kante nicht gefunden");
    geo_ref_for_edge(solid, id, &Tolerance::default()).unwrap()
}

/// Quader 100 × 60 × 40 mm mit der Zeichnung „Blatt 1“ (Vorne, Oben abgeleitet) und einem Maß der
/// Breite mit einer Nachkommastelle (Text „100,0“ bzw. „100.0“).
fn fixture() -> (AppState, DrawingId) {
    let mut s = AppState::new();
    s.create_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
    s.selected_body = None;
    let one = Scale { num: 1, den: 1 };
    let d = s
        .document
        .add_drawing("Blatt 1", A3_QUER, Projection::First)
        .unwrap();
    let front = s
        .document
        .add_view(
            d,
            ViewKind::Front,
            one,
            Placement::Free { x: 120.0, y: 200.0 },
            None,
        )
        .unwrap();
    s.document
        .add_view(
            d,
            ViewKind::Top,
            one,
            Placement::Aligned { distance: 70.0 },
            Some(front),
        )
        .unwrap();
    let (lo, hi) = s.bodies[0].bounds;
    let spec = DimSpec {
        id: DimId(0),
        view: front,
        kind: DimKind::Horizontal,
        refs: vec![
            GeoRef::Edge(edge_where(&s, |p| near(p.x, lo.x) && near(p.y, lo.y))),
            GeoRef::Edge(edge_where(&s, |p| near(p.x, hi.x) && near(p.y, lo.y))),
        ],
        offset: 10.0,
        angle: 45.0,
        text_shift: (0.0, 0.0),
        tolerance: DimTolerance::None,
        decimals: 1,
        text_override: None,
        chain: None,
        suggested: false,
    };
    s.document.add_dimension(d, spec).unwrap();
    (s, d)
}

fn dialog(s: &AppState, format: ExportFormat, lang: Option<&str>) -> ExportDialog {
    let mut d = ExportDialog::new(s);
    d.format = format;
    d.export_lang = lang.map(str::to_string);
    d.tick(s, true);
    d
}

fn ready(d: &ExportDialog, s: &AppState) -> bool {
    matches!(d.status(s), ExportStatus::DrawingReady { .. })
}

/// Inhalte der `<text>`-Elemente einer SVG-Datei.
fn svg_texts(svg: &str) -> Vec<String> {
    svg.split("<text")
        .skip(1)
        .map(|part| {
            let body = part.split_once('>').map(|x| x.1).unwrap_or("");
            body.split("</text>")
                .next()
                .unwrap_or("")
                .replace("&amp;", "&")
        })
        .collect()
}

/// Kennungen der Gruppen einer SVG-Datei.
fn svg_groups(svg: &str) -> Vec<String> {
    svg.split("<g id=\"")
        .skip(1)
        .map(|p| p.split('"').next().unwrap_or("").to_string())
        .collect()
}

fn write_svg(d: &mut ExportDialog, s: &AppState, path: &Path) -> String {
    d.write_to(path.to_path_buf(), s);
    d.tick(s, true);
    assert!(
        matches!(d.write_state(), WriteState::Done(..)),
        "{:?}",
        d.write_state()
    );
    fs::read_to_string(path).unwrap()
}

#[test]
fn english_drawing_from_german_program() {
    let (mut s, _) = fixture();
    let dir = temp_dir("svg");

    // Vorgabe „Wie das Programm“: deutsch, Komma.
    let mut d = dialog(&s, SVG, None);
    assert!(ready(&d, &s), "{:?}", d.status(&s));
    let de = write_svg(&mut d, &s, &dir.join("de.svg"));
    let texts = svg_texts(&de);
    for must in ["100,0", "Zeichnungsnummer", "Maßstab", "de"] {
        assert!(texts.iter().any(|t| t == must), "„{must}“ fehlt: {texts:?}");
    }
    assert!(svg_groups(&de).iter().any(|g| g == "Vorne Maße"));

    // Englisch gewählt: englische Beschriftungen und Punkt als Trenner.
    let mut d = dialog(&s, SVG, Some("en"));
    assert!(ready(&d, &s), "{:?}", d.status(&s));
    let en = write_svg(&mut d, &s, &dir.join("en.svg"));
    let texts = svg_texts(&en);
    for must in ["100.0", "Drawing number", "Scale", "Sheet", "en", "1 of 1"] {
        assert!(texts.iter().any(|t| t == must), "„{must}“ fehlt: {texts:?}");
    }
    for never in ["100,0", "Zeichnungsnummer", "Maßstab", "1 von 1"] {
        assert!(
            !texts.iter().any(|t| t == never),
            "„{never}“ in der englischen Zeichnung: {texts:?}"
        );
    }
    let groups = svg_groups(&en);
    assert!(groups.iter().any(|g| g == "Front Dimensions"), "{groups:?}");
    assert!(groups.iter().any(|g| g == "Sheet frame"), "{groups:?}");

    // Das Fenster bleibt deutsch, auch die Meldung „Fertig“.
    let window = draw(&mut d, &mut s);
    for must in [
        "Sprache der Zeichnung",
        "English",
        "Blattrahmen mit ausgeben",
        "Fertig: „en.svg“ geschrieben",
        "Maße: 1",
    ] {
        assert!(window.contains(must), "„{must}“ fehlt: {window}");
    }
    for never in [
        "Drawing language",
        "Done:",
        "Include sheet frame",
        "Dimensions:",
    ] {
        assert!(!window.contains(never), "„{never}“ im Fenster: {window}");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn switching_the_drawing_language_discards_the_result() {
    let (s, _) = fixture();
    let mut d = dialog(&s, SVG, None);
    assert!(ready(&d, &s));
    // Ausdrücklich Deutsch ist dieselbe Sprache wie das Programm: kein neues Rechnen.
    d.export_lang = Some("de".into());
    assert!(ready(&d, &s), "{:?}", d.status(&s));
    // Englisch verwirft das Ergebnis; das nächste `tick` rechnet neu.
    d.export_lang = Some("en".into());
    assert!(
        matches!(d.status(&s), ExportStatus::Computing(_)),
        "{:?}",
        d.status(&s)
    );
    let dir = temp_dir("stale");
    d.write_to(dir.join("x.svg"), &s);
    assert!(
        matches!(d.write_state(), WriteState::Failed(_)),
        "{:?}",
        d.write_state()
    );
    assert!(!dir.join("x.svg").exists());
    d.tick(&s, true);
    assert!(ready(&d, &s), "{:?}", d.status(&s));
    // Zurück auf „Wie das Programm“: wieder neu (das Ergebnis gehört zur Sprache).
    d.export_lang = None;
    assert!(matches!(d.status(&s), ExportStatus::Computing(_)));
    d.tick(&s, true);
    assert!(ready(&d, &s));
    // Eine unbekannte Sprache gilt als „Wie das Programm“.
    d.export_lang = Some("xx-unbekannt".into());
    assert!(ready(&d, &s));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn only_drawing_formats_offer_the_choice() {
    let (mut s, _) = fixture();
    for format in [
        ExportFormat::Mesh(Format::Stl),
        ExportFormat::Mesh(Format::Obj),
        ExportFormat::Step,
    ] {
        let mut d = dialog(&s, format, None);
        let text = draw(&mut d, &mut s);
        assert!(
            !text.contains("Sprache der Zeichnung") && !text.contains("Wie das Programm"),
            "{format:?}: {text}"
        );
    }
    for format in [SVG, DXF, PDF] {
        let mut d = dialog(&s, format, None);
        let text = draw(&mut d, &mut s);
        assert!(text.contains("Sprache der Zeichnung"), "{format:?}: {text}");
        assert!(
            text.contains("Wie das Programm (Deutsch)"),
            "{format:?}: {text}"
        );
        // Unter Englisch heißt der erste Eintrag nach der Programmsprache.
        let text = with_lang(&english(), || draw(&mut d, &mut s));
        assert!(text.contains("Drawing language"), "{format:?}: {text}");
        assert!(
            text.contains("Same as program (English)"),
            "{format:?}: {text}"
        );
    }
}

/// Rechnung einer Ansicht, die mit einem Grund der Bibliothek scheitert (Text in der Sprache des
/// rechnenden Threads).
fn failing_compute(_: &[DrawBody], _: ViewKind, _: &Progress) -> Result<ViewLines, DrawError> {
    Err(DrawError::Unsupported(planora_i18n::t(
        "drawing-err-section-normal",
    )))
}

/// Schreiben, das mit einem Dateifehler scheitert.
fn failing_writer(
    _: &Path,
    _: &SheetModel,
    _: DrawingFormat,
    _: &Progress,
) -> Result<u64, DrawError> {
    Err(DrawError::Io(std::io::Error::other("kaputt")))
}

#[test]
fn messages_stay_in_the_program_language() {
    let (mut s, _) = fixture();
    let de_reason = planora_i18n::t("drawing-err-section-normal");
    let en_reason = with_lang(&english(), || planora_i18n::t("drawing-err-section-normal"));
    assert_ne!(de_reason, en_reason, "Testvoraussetzung");

    // Fehler beim Berechnen: Grund und Meldung deutsch.
    let mut d = ExportDialog::new(&s);
    d.format = SVG;
    d.export_lang = Some("en".into());
    d.set_compute(failing_compute);
    d.tick(&s, true);
    match d.status(&s) {
        ExportStatus::Failed(m) => {
            assert!(m.contains(&de_reason), "{m}");
            assert!(m.contains("Vorne"), "{m}");
            assert!(!m.contains(&en_reason), "{m}");
        }
        other => panic!("Fehler erwartet: {other:?}"),
    }
    let window = draw(&mut d, &mut s);
    assert!(window.contains(&de_reason), "{window}");

    // Fehler beim Schreiben: deutsch.
    let mut d = dialog(&s, SVG, Some("en"));
    d.set_writer(failing_writer);
    assert!(ready(&d, &s));
    let dir = temp_dir("fail");
    d.write_to(dir.join("x.svg"), &s);
    d.tick(&s, true);
    match d.write_state() {
        WriteState::Failed(m) => {
            assert!(m.starts_with("Die Datei lässt sich nicht schreiben"), "{m}");
            assert!(m.contains("kaputt"), "{m}");
        }
        other => panic!("Fehler erwartet: {other:?}"),
    }
    let window = draw(&mut d, &mut s);
    assert!(window.contains("Nicht geschrieben: Die Datei"), "{window}");
    let _ = fs::remove_dir_all(&dir);
}

/// „Wie das Programm“ folgt auch einem Wechsel der Programmsprache bei offenem Fenster: Das
/// Ergebnis gehörte zur alten Sprache und wird neu berechnet; danach steht die Meldung in der
/// neuen Sprache.
#[test]
fn same_as_program_follows_a_program_switch() {
    let (mut s, _) = fixture();
    let mut d = ExportDialog::new(&s);
    d.format = SVG;
    d.set_compute(failing_compute);
    d.tick(&s, true);
    assert!(matches!(d.status(&s), ExportStatus::Failed(_)));
    let en_reason = with_lang(&english(), || planora_i18n::t("drawing-err-section-normal"));
    with_lang(&english(), || {
        assert!(
            matches!(d.status(&s), ExportStatus::Computing(_)),
            "{:?}",
            d.status(&s)
        );
        d.tick(&s, true);
        match d.status(&s) {
            ExportStatus::Failed(m) => assert!(m.contains(&en_reason), "{m}"),
            other => panic!("Fehler erwartet: {other:?}"),
        }
        let window = draw(&mut d, &mut s);
        assert!(window.contains(&en_reason), "{window}");
        assert!(window.contains("Same as program (English)"), "{window}");
    });
}
