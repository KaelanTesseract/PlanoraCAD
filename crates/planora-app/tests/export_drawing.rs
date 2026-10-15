//! Fenster „Exportieren“ mit Zeichnungen (Teilprojekt 11, Aufgabe 17): sechstes und siebtes
//! Format, Karte „Zeichnung“ statt Qualität und Umfang, Ansichten im eigenen Arbeitsthread,
//! Schreiben über eine Hilfsdatei, veraltete Ergebnisse, Fehler und Abbrechen, volle Breite und
//! gleich hohe Karten.
//!
//! Die Dateien werden echt geschrieben und gelesen; verglichen wird mit einem Blatt, das der Test
//! selbst aus den Körpern berechnet (`compute_view`, `build_sheet`), nicht mit dem Ergebnis des
//! Fensters.

use planora_app::export_dialog::*;
use planora_app::fonts;
use planora_app::state::*;
use planora_app::testing::collect_texts;
use planora_drawing::sheet::{SheetModel, build_sheet};
use planora_drawing::{
    DrawBody, DrawError, DrawingFormat, DrawingId, Paper, Placement, Projection, Scale, Sheet,
    ViewId, ViewKind, ViewLines, Visibility, compute_view, dxf_string, svg_string,
};
use planora_exchange::{ExchangeMesh, Format, Progress};
use planora_math::DVec3;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const SVG: ExportFormat = ExportFormat::Drawing(DrawingFormat::Svg);
const DXF: ExportFormat = ExportFormat::Drawing(DrawingFormat::Dxf);

const A3_QUER: Sheet = Sheet {
    paper: Paper::A3,
    landscape: true,
};

/// Einheitswürfel als Netz, Dreiecke von außen gegen den Uhrzeigersinn.
fn cube() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7,
            3, 1, 2, 6, 1, 6, 5,
        ],
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-drawing-ui-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// Zahl der Hilfsdateien („….planora-tmp“) im Ordner.
fn temp_files(dir: &Path) -> usize {
    fs::read_dir(dir)
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with("planora-tmp")
        })
        .count()
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

fn wide() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200.0, 800.0),
        )),
        ..Default::default()
    }
}

/// Zeichnet das Fenster dreimal und liefert die Texte des letzten Bildes.
fn draw(d: &mut ExportDialog, s: &mut AppState) -> String {
    let ctx = context();
    let mut dir = None;
    let mut text = String::new();
    for _ in 0..3 {
        let out = ctx.run_ui(wide(), |ui| {
            assert!(d.show(ui.ctx(), s, &mut dir));
        });
        text = collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    text
}

/// Quader 100 × 60 × 40 mm.
fn box_state() -> AppState {
    let mut s = AppState::new();
    s.create_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
    s.selected_body = None;
    s
}

fn one(num: u32, den: u32) -> Scale {
    Scale { num, den }
}

/// Zeichnung `name` (A3 quer, Erstwinkel): Vorne (frei), Oben und Links (abgeleitet) und eine
/// isometrische Ansicht mit verdeckten Kanten.
fn add_drawing(s: &mut AppState, name: &str) -> DrawingId {
    let d = s
        .document
        .add_drawing(name, A3_QUER, Projection::First)
        .unwrap();
    let front = s
        .document
        .add_view(
            d,
            ViewKind::Front,
            one(1, 1),
            Placement::Free { x: 120.0, y: 200.0 },
            None,
        )
        .unwrap();
    s.document
        .add_view(
            d,
            ViewKind::Top,
            one(1, 1),
            Placement::Aligned { distance: 70.0 },
            Some(front),
        )
        .unwrap();
    s.document
        .add_view(
            d,
            ViewKind::Left,
            one(1, 1),
            Placement::Aligned { distance: 100.0 },
            Some(front),
        )
        .unwrap();
    let iso = s
        .document
        .add_view(
            d,
            ViewKind::Isometric,
            one(1, 2),
            Placement::Free { x: 330.0, y: 90.0 },
            None,
        )
        .unwrap();
    s.document
        .update_drawing(d, |dr| {
            dr.views
                .iter_mut()
                .find(|v| v.id == iso)
                .unwrap()
                .show_hidden = true;
        })
        .unwrap();
    d
}

fn dialog(s: &AppState, format: ExportFormat) -> ExportDialog {
    let mut d = ExportDialog::new(s);
    d.format = format;
    d.tick(s, true);
    d
}

/// Blatt, das der Test selbst aus den exakten Körpern und der Zeichnung berechnet.
fn own_sheet(s: &AppState, id: DrawingId) -> SheetModel {
    let bodies: Vec<DrawBody> = s
        .bodies
        .iter()
        .filter(|b| b.kind != BodyKind::Mesh)
        .map(|b| DrawBody {
            name: b.name.clone(),
            solid: &b.solid,
        })
        .collect();
    let drawing = s.document.drawing(id).unwrap();
    let lines: Vec<(ViewId, ViewLines)> = drawing
        .views
        .iter()
        .map(|v| {
            (
                v.id,
                compute_view(&bodies, v.kind, &Progress::new()).unwrap(),
            )
        })
        .collect();
    let map: BTreeMap<ViewId, &ViewLines> = lines.iter().map(|(i, l)| (*i, l)).collect();
    build_sheet(drawing, &map)
}

fn count(m: &SheetModel, vis: Visibility) -> usize {
    m.views
        .iter()
        .flat_map(|v| &v.lines)
        .filter(|l| l.vis == vis)
        .count()
}

fn done(d: &ExportDialog) -> (PathBuf, u64) {
    match d.write_state() {
        WriteState::Done(p, n) => (p, n),
        other => panic!("{other:?}"),
    }
}

// (j) ------------------------------------------------------------------------------------------

#[test]
fn format_list_has_seven_entries_in_order() {
    assert_eq!(
        ExportFormat::ALL,
        [
            ExportFormat::Mesh(Format::Stl),
            ExportFormat::Mesh(Format::Obj),
            ExportFormat::Mesh(Format::ThreeMf),
            ExportFormat::Mesh(Format::Glb),
            ExportFormat::Step,
            SVG,
            DXF,
        ]
    );
    assert_eq!(SVG.extension(), "svg");
    assert_eq!(DXF.extension(), "dxf");
    assert_eq!(SVG.label(), "Zeichnung als SVG");
    assert_eq!(DXF.label(), "Zeichnung als DXF");
    assert!(!SVG.is_step() && SVG.is_drawing() && !ExportFormat::Step.is_drawing());
}

// (a) ------------------------------------------------------------------------------------------

#[test]
fn svg_and_dxf_files_are_written_and_readable() {
    let mut s = box_state();
    let id = add_drawing(&mut s, "Blatt 1");
    let own = own_sheet(&s, id);
    let (visible, hidden) = (
        count(&own, Visibility::Visible),
        count(&own, Visibility::Hidden),
    );
    // Von Hand: vier Rechtecke (je 4 Kanten), die isometrische Ansicht 9 sichtbare und 3
    // verdeckte Kanten.
    assert_eq!((visible, hidden), (21, 3));
    let dir = temp_dir("schreiben");
    for (format, ext, want) in [
        (SVG, "svg", svg_string(&own).unwrap()),
        (DXF, "dxf", dxf_string(&own).unwrap()),
    ] {
        let mut d = dialog(&s, format);
        assert_eq!(
            d.status(&s),
            ExportStatus::DrawingReady {
                views: 4,
                sheet: "A3 quer".into(),
                visible,
                hidden,
                skipped_meshes: vec![],
            }
        );
        d.begin_save(dir.clone(), &s);
        let path = dir.join(format!("Blatt 1.{ext}"));
        assert_eq!(d.chooser().unwrap().target(), Some(path.clone()));
        d.accept_chosen(path.clone(), &s);
        d.tick(&s, true);
        let (p, n) = done(&d);
        assert_eq!(p, path);
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(n, text.len() as u64);
        assert_eq!(text, want, "Datei = eigenes Blatt");
        if format == SVG {
            assert!(text.starts_with("<?xml") && text.contains("<svg"));
        } else {
            assert!(text.contains("SECTION") && text.trim_end().ends_with("EOF"));
        }
        let ui = draw(&mut d, &mut s);
        let line = format!("Fertig: „Blatt 1.{ext}“ geschrieben ({}).", mb_text(n));
        assert!(ui.contains(&line), "„{line}“ fehlt:\n{ui}");
        assert!(ui.contains("unter 0,1 MB"), "{ui}");
        assert_eq!(temp_files(&dir), 0);
    }
    let _ = fs::remove_dir_all(&dir);
}

// (b) ------------------------------------------------------------------------------------------

#[test]
fn no_drawing_gives_a_clear_hint() {
    assert_eq!(
        NO_DRAWING,
        "Es gibt noch keine Zeichnung. Über „Zeichnung“ im Menü lässt sich eine anlegen."
    );
    let mut s = box_state();
    let mut d = dialog(&s, SVG);
    assert_eq!(d.status(&s), ExportStatus::Unavailable(NO_DRAWING.into()));
    assert_eq!(disabled_hint(&d.status(&s)), NO_DRAWING);
    let text = draw(&mut d, &mut s);
    assert!(text.contains(NO_DRAWING), "{text}");
    assert!(!text.contains("Neu berechnen"), "{text}");
    let dir = temp_dir("keine");
    d.write_to(dir.join("x.svg"), &s);
    assert!(matches!(d.write_state(), WriteState::Failed(_)));
    assert!(!dir.join("x.svg").exists());
    // Zeichnung ohne Ansicht.
    s.document
        .add_drawing("Leer", A3_QUER, Projection::First)
        .unwrap();
    d.tick(&s, true);
    assert_eq!(
        d.status(&s),
        ExportStatus::Unavailable("Die Zeichnung „Leer“ hat noch keine Ansicht.".into())
    );
    // Nur Netz-Körper und gar keine Körper: dieselben Hinweise wie im Zeichnungsbereich.
    let mut m = AppState::new();
    m.import_mesh("Würfel", Arc::new(cube()), 1.0).unwrap();
    add_drawing(&mut m, "Z");
    let d = dialog(&m, DXF);
    assert_eq!(
        d.status(&m),
        ExportStatus::Unavailable(
            "Alle Körper sind Netze; eine Zeichnung braucht exakte Körper.".into()
        )
    );
    let mut e = AppState::new();
    add_drawing(&mut e, "Z");
    let d = dialog(&e, DXF);
    assert_eq!(
        d.status(&e),
        ExportStatus::Unavailable("Es gibt keinen Körper, der sich zeichnen lässt.".into())
    );
    let _ = fs::remove_dir_all(&dir);
}

// (c) ------------------------------------------------------------------------------------------

fn fail_top(b: &[DrawBody], k: ViewKind, p: &Progress) -> Result<ViewLines, DrawError> {
    if k == ViewKind::Top {
        Err(DrawError::TooComplex(123))
    } else {
        compute_view(b, k, p)
    }
}

#[test]
fn failed_view_blocks_writing_with_its_text() {
    let mut s = box_state();
    add_drawing(&mut s, "Blatt 1");
    let mut d = ExportDialog::new(&s);
    d.set_compute(fail_top);
    d.format = SVG;
    d.tick(&s, true);
    let msg = "Die Ansicht „Oben“ lässt sich nicht berechnen: Der Körper ist für diese Ansicht zu aufwendig (123 Teilstücke).";
    assert_eq!(d.status(&s), ExportStatus::Failed(msg.into()));
    let text = draw(&mut d, &mut s);
    assert!(text.contains(msg), "{text}");
    let dir = temp_dir("fehler-ansicht");
    let path = dir.join("z.svg");
    d.write_to(path.clone(), &s);
    d.tick(&s, true);
    assert!(matches!(d.write_state(), WriteState::Failed(_)));
    assert!(!path.exists(), "keine Zeichnung mit fehlender Ansicht");
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_repeated_kind_is_named_like_in_the_browser() {
    let mut s = box_state();
    let id = add_drawing(&mut s, "Blatt 1");
    s.document
        .add_view(
            id,
            ViewKind::Top,
            one(1, 2),
            Placement::Free { x: 300.0, y: 250.0 },
            None,
        )
        .unwrap();
    let prepared = prepare_drawing(
        s.document.drawing(id).unwrap(),
        &[("K".into(), s.bodies[0].solid.clone())],
        fail_top,
        &Progress::new(),
    )
    .unwrap_err();
    assert!(
        prepared.starts_with("Die Ansicht „Oben“ lässt sich nicht berechnen"),
        "{prepared}"
    );
}

// (d) ------------------------------------------------------------------------------------------

#[test]
fn stale_result_is_not_written() {
    let mut s = box_state();
    let a = add_drawing(&mut s, "Blatt A");
    let dir = temp_dir("veraltet");
    let mut d = dialog(&s, SVG);
    assert_eq!(d.drawing, Some(a));
    // Modell geändert zwischen Prüfung und Schreiben.
    s.create_box(DVec3::splat(20.0)).unwrap();
    s.selected_body = None;
    let path = dir.join("a.svg");
    d.write_to(path.clone(), &s);
    assert_eq!(d.write_state(), WriteState::Failed(STALE.into()));
    assert!(!path.exists());
    // Neue Prüfung, dann passt die Datei zum neuen Modell (zwei Quader).
    d.tick(&s, true);
    d.write_to(path.clone(), &s);
    d.tick(&s, true);
    done(&d);
    let own = own_sheet(&s, a);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        svg_string(&own).unwrap()
    );
    // Andere Zeichnung gewählt, ohne neue Prüfung: nichts wird geschrieben.
    let b = s
        .document
        .add_drawing("Blatt B", A3_QUER, Projection::First)
        .unwrap();
    s.document
        .add_view(
            b,
            ViewKind::Right,
            one(1, 1),
            Placement::Free { x: 200.0, y: 150.0 },
            None,
        )
        .unwrap();
    let a = s.document.drawing(a).unwrap().id;
    d.tick(&s, true);
    assert!(matches!(d.status(&s), ExportStatus::DrawingReady { .. }));
    d.drawing = Some(b);
    let other = dir.join("b.svg");
    d.write_to(other.clone(), &s);
    assert_eq!(d.write_state(), WriteState::Failed(STALE.into()));
    assert!(!other.exists());
    d.tick(&s, true);
    d.write_to(other.clone(), &s);
    d.tick(&s, true);
    done(&d);
    assert_eq!(
        fs::read_to_string(&other).unwrap(),
        svg_string(&own_sheet(&s, b)).unwrap()
    );
    assert_ne!(
        fs::read_to_string(&other).unwrap(),
        svg_string(&own_sheet(&s, a)).unwrap()
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_drawing_edit_prepares_again() {
    let mut s = box_state();
    let a = add_drawing(&mut s, "Blatt A");
    let dir = temp_dir("massstab");
    let mut d = dialog(&s, DXF);
    s.document
        .update_drawing(a, |dr| dr.views[0].scale = one(1, 2))
        .unwrap();
    d.write_to(dir.join("a.dxf"), &s);
    assert_eq!(d.write_state(), WriteState::Failed(STALE.into()));
    d.tick(&s, true);
    d.write_to(dir.join("a.dxf"), &s);
    d.tick(&s, true);
    done(&d);
    assert_eq!(
        fs::read_to_string(dir.join("a.dxf")).unwrap(),
        dxf_string(&own_sheet(&s, a)).unwrap()
    );
    let _ = fs::remove_dir_all(&dir);
}

// (e) ------------------------------------------------------------------------------------------

fn slow(_: &[DrawBody], _: ViewKind, p: &Progress) -> Result<ViewLines, DrawError> {
    loop {
        p.check()?;
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn existing_file_stays_on_error_and_cancel() {
    let mut s = box_state();
    add_drawing(&mut s, "Blatt 1");
    let dir = temp_dir("fehler");
    // Ordner fehlt: verständliche Meldung, nichts angelegt.
    let mut d = dialog(&s, SVG);
    let missing = dir.join("fehlt").join("z.svg");
    d.write_to(missing.clone(), &s);
    d.tick(&s, true);
    match d.write_state() {
        WriteState::Failed(m) => {
            assert!(
                m.starts_with("Die Datei lässt sich nicht schreiben: "),
                "{m}"
            )
        }
        other => panic!("{other:?}"),
    }
    let text = draw(&mut d, &mut s);
    assert!(
        text.contains("Nicht geschrieben: Die Datei lässt sich nicht schreiben"),
        "{text}"
    );
    assert!(!dir.join("fehlt").exists());
    // Schreibgeschützte Zieldatei (Windows ersetzt sie nicht): bleibt, keine Hilfsdatei.
    if cfg!(windows) {
        let path = dir.join("fest.svg");
        fs::write(&path, "alt").unwrap();
        let mut perm = fs::metadata(&path).unwrap().permissions();
        perm.set_readonly(true);
        fs::set_permissions(&path, perm.clone()).unwrap();
        d.write_to(path.clone(), &s);
        d.tick(&s, true);
        let state = d.write_state();
        let text = fs::read_to_string(&path).unwrap();
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(false);
        fs::set_permissions(&path, perm).unwrap();
        assert!(matches!(state, WriteState::Failed(_)), "{state:?}");
        assert_eq!(text, "alt");
        assert_eq!(temp_files(&dir), 0);
    }
    // Abbrechen während der Berechnung: keine Datei, die alte bleibt.
    let old = dir.join("alt.svg");
    fs::write(&old, "alt").unwrap();
    let mut d = ExportDialog::new(&s);
    d.set_compute(slow);
    d.format = SVG;
    d.tick(&s, false);
    assert!(matches!(d.status(&s), ExportStatus::Computing(_)));
    d.cancel();
    assert_eq!(d.status(&s), ExportStatus::Failed("Abgebrochen".into()));
    d.write_to(old.clone(), &s);
    d.tick(&s, true);
    assert!(matches!(d.write_state(), WriteState::Failed(_)));
    assert_eq!(fs::read_to_string(&old).unwrap(), "alt");
    assert_eq!(temp_files(&dir), 0);
    // Abbrechen des Arbeitsthreads selbst (etwa beim Schließen des Tabs).
    let p = Progress::new();
    p.cancel();
    let err = prepare_drawing(
        s.document.drawings().first().unwrap(),
        &[("K".into(), s.bodies[0].solid.clone())],
        compute_view,
        &p,
    )
    .unwrap_err();
    assert_eq!(err, "Abgebrochen");
    let _ = fs::remove_dir_all(&dir);
}

// (f) ------------------------------------------------------------------------------------------

#[test]
fn extension_is_normalized() {
    let p = |s: &str| PathBuf::from(s);
    assert_eq!(normalize_target(SVG, p("x.SVG")), p("x.svg"));
    assert_eq!(normalize_target(SVG, p("plan")), p("plan.svg"));
    assert_eq!(normalize_target(SVG, p("plan.dxf")), p("plan.dxf.svg"));
    assert_eq!(normalize_target(DXF, p("x.Dxf")), p("x.dxf"));
    assert_eq!(normalize_target(DXF, p("x.txt")), p("x.txt.dxf"));
    assert_eq!(normalize_target(DXF, p("x.dxf")), p("x.dxf"));
    // Bisherige Formate unverändert.
    assert_eq!(
        normalize_target(ExportFormat::Step, p("x.stp")),
        p("x.step")
    );
    assert_eq!(
        normalize_target(ExportFormat::Mesh(Format::Stl), p("x.txt")),
        p("x.txt")
    );
    // Geschrieben wird unter dem richtigen Namen; der Name der Zeichnung ist dateitauglich.
    let mut s = box_state();
    add_drawing(&mut s, "Teil: A/B*?");
    let dir = temp_dir("endung");
    let mut d = dialog(&s, SVG);
    d.begin_save(dir.clone(), &s);
    assert_eq!(
        d.chooser().unwrap().target(),
        Some(dir.join("Teil_ A_B__.svg"))
    );
    d.write_to(dir.join("x.SVG"), &s);
    d.tick(&s, true);
    assert_eq!(done(&d).0, dir.join("x.svg"));
    let names: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["x.svg".to_string()]);
    // Die Dateiauswahl zeigt nur passende Dateien.
    fs::write(dir.join("y.dxf"), "x").unwrap();
    fs::write(dir.join("y.step"), "x").unwrap();
    let mut d = dialog(&s, DXF);
    d.begin_save(dir.clone(), &s);
    let shown: Vec<String> = d
        .chooser()
        .unwrap()
        .entries()
        .iter()
        .map(|e| e.name.clone())
        .collect();
    assert_eq!(shown, vec!["y.dxf".to_string()]);
    let _ = fs::remove_dir_all(&dir);
}

// (g) ------------------------------------------------------------------------------------------

#[test]
fn mesh_bodies_are_named_and_skipped() {
    let mut s = box_state();
    s.import_mesh("Würfel", Arc::new(cube()), 1.0).unwrap();
    s.selected_body = None;
    let id = add_drawing(&mut s, "Blatt 1");
    let mut d = dialog(&s, DXF);
    assert!(
        matches!(&d.status(&s), ExportStatus::DrawingReady { skipped_meshes, .. } if *skipped_meshes == vec!["Würfel".to_string()]),
        "{:?}",
        d.status(&s)
    );
    let text = draw(&mut d, &mut s);
    assert!(
        text.contains("Netz-Körper sind in der Zeichnung nicht enthalten: „Würfel“."),
        "{text}"
    );
    let dir = temp_dir("netz");
    d.write_to(dir.join("z.dxf"), &s);
    d.tick(&s, true);
    done(&d);
    assert_eq!(
        fs::read_to_string(dir.join("z.dxf")).unwrap(),
        dxf_string(&own_sheet(&s, id)).unwrap()
    );
    let _ = fs::remove_dir_all(&dir);
}

// (h) ------------------------------------------------------------------------------------------

#[test]
fn cards_use_full_width_and_equal_heights() {
    let mut s = box_state();
    let a = add_drawing(&mut s, "Blatt A");
    let b = add_drawing(&mut s, "Blatt B");
    // Vorgabe: die geöffnete Zeichnung, sonst die erste.
    assert_eq!(ExportDialog::new(&s).drawing, Some(a));
    s.drawing_ui.open(b);
    let mut d = dialog(&s, SVG);
    assert_eq!(d.drawing, Some(b));
    let own = own_sheet(&s, b);
    let text = draw(&mut d, &mut s);
    let lines = format!(
        "Linien: {} sichtbar, {} verdeckt",
        count(&own, Visibility::Visible),
        count(&own, Visibility::Hidden)
    );
    for word in [
        "Format",
        "Zeichnung als SVG",
        "Zeichnung als DXF",
        "STEP – für andere CAD-Programme (exakt)",
        "Zeichnung",
        "Blatt B",
        "Ansichten: 4",
        "Blatt: A3 quer",
        lines.as_str(),
        "Exportieren …",
        "Schließen",
    ] {
        assert!(text.contains(word), "„{word}“ fehlt:\n{text}");
    }
    for word in ["Qualität", "Umfang", "Dreiecke", "Grob", "Gewählter Körper"] {
        assert!(
            !text.contains(word),
            "„{word}“ gehört nicht zur Zeichnung:\n{text}"
        );
    }
    let rect = d.last_rect.unwrap();
    assert!(
        rect.width() >= 1200.0 - 2.0 * DIALOG_MARGIN - 1.0,
        "{rect:?}"
    );
    assert_eq!(d.last_cards.len(), 2);
    assert!(
        d.last_cards
            .windows(2)
            .all(|w| (w[0].height() - w[1].height()).abs() < 0.5),
        "{:?}",
        d.last_cards
    );
    assert!(
        (d.last_cards[0].width() - d.last_cards[1].width()).abs() < 1.0,
        "{:?}",
        d.last_cards
    );
}

// (i) ------------------------------------------------------------------------------------------

fn slow_writer(_: &Path, _: &SheetModel, _: DrawingFormat, p: &Progress) -> Result<u64, DrawError> {
    loop {
        p.check()?;
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn window_stays_open_while_writing() {
    let mut s = box_state();
    add_drawing(&mut s, "Blatt 1");
    let dir = temp_dir("schliessen");
    let path = dir.join("z.svg");
    let mut d = dialog(&s, SVG);
    d.set_writer(slow_writer);
    d.write_to(path.clone(), &s);
    assert!(d.is_writing_file());
    assert!(!d.is_writing_step(), "STEP-Name bleibt bei STEP");
    assert!(!d.request_close(), "Schließen muss abgewiesen werden");
    assert_eq!(d.write_state(), WriteState::Writing);
    let ctx = context();
    let mut last = None;
    let mut text = String::new();
    for _ in 0..2 {
        let out = ctx.run_ui(wide(), |ui| {
            assert!(d.show(ui.ctx(), &mut s, &mut last), "Fenster bleibt offen");
        });
        text = collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    assert!(d.is_writing_file(), "das Schreiben muss noch laufen");
    assert!(text.contains(WRITING_HINT), "{text}");
    assert!(text.contains("Die Datei wird geschrieben"), "{text}");
    assert!(text.contains("Abbrechen"), "{text}");
    d.cancel_write();
    d.tick(&s, true);
    assert_eq!(d.write_state(), WriteState::Failed("Abgebrochen".into()));
    assert!(!path.exists());
    assert!(d.request_close());
    // Mit dem echten Schreiber: danach fertig und schließbar.
    let mut d = dialog(&s, SVG);
    d.write_to(path.clone(), &s);
    assert!(!d.request_close());
    d.tick(&s, true);
    done(&d);
    assert!(d.request_close());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn closing_a_tab_while_a_drawing_is_written_leaves_no_helper_file() {
    use planora_app::workspace::Workspace;
    use planora_project::config::Config;
    let mut w = Workspace::new(None, Config::default());
    let dir = temp_dir("tab");
    let target = dir.join("alt.svg");
    fs::write(&target, "alt").unwrap();
    let tab = w.active_tab_mut();
    tab.state.create_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
    add_drawing(&mut tab.state, "Blatt 1");
    let mut d = dialog(&tab.state, SVG);
    d.write_to(target.clone(), &tab.state);
    tab.export_dialog = Some(d);
    assert!(!w.request_close(0));
    w.discard_and_continue();
    assert!(w.tabs[0].export_dialog.is_none());
    let start = std::time::Instant::now();
    while temp_files(&dir) > 0 && start.elapsed().as_secs() < 10 {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(temp_files(&dir), 0);
    let text = fs::read_to_string(&target).unwrap();
    assert!(text == "alt" || text.starts_with("<?xml"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn switching_formats_keeps_the_other_results() {
    let mut s = box_state();
    add_drawing(&mut s, "Blatt 1");
    let mut d = dialog(&s, ExportFormat::Mesh(Format::Stl));
    assert!(matches!(d.status(&s), ExportStatus::Ready { .. }));
    d.format = SVG;
    d.tick(&s, true);
    assert!(matches!(d.status(&s), ExportStatus::DrawingReady { .. }));
    d.format = ExportFormat::Mesh(Format::Stl);
    assert!(matches!(d.status(&s), ExportStatus::Ready { .. }));
    d.format = ExportFormat::Step;
    d.tick(&s, true);
    assert!(matches!(
        d.status(&s),
        ExportStatus::StepReady { bodies: 1, .. }
    ));
    let text = draw(&mut d, &mut s);
    assert!(!text.contains("Ansichten:"), "{text}");
}
