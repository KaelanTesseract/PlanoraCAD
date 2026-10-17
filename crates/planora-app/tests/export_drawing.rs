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
use planora_drawing::dxf::dxf_text_fits;
use planora_drawing::sheet::{SheetModel, build_sheet};
use planora_drawing::sheet_info::{FieldValue, SheetInfo, Stamp};
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
    // Diese Tests vergleichen mit dem Blatt ohne Rahmen; den Schalter prüft `frame_switch_*`.
    d.with_frame = false;
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
                dimensions: 0,
                outside: 0,
                centerlines: CenterlineUse::Yes,
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
    let msg = "Die Ansicht „Oben“ lässt sich nicht berechnen: Der Körper ist für diese Ansicht zu aufwendig (123 Arbeitsschritte).";
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
fn views_that_cannot_be_placed_stop_the_export_with_a_message() {
    // Eine beschädigte Zeichnung (Maßstab mit Nenner 0, am Dokument vorbei gesetzt): Das Blatt
    // ließe die Ansicht und ihre abgeleitete still weg. Das Export-Fenster schreibt dann nicht,
    // sondern nennt den Grund.
    let mut s = box_state();
    let id = add_drawing(&mut s, "Blatt 1");
    let mut drawing = s.document.drawing(id).unwrap().clone();
    let ok = prepare_drawing(
        &drawing,
        &[("K".into(), s.bodies[0].solid.clone())],
        compute_view,
        &Progress::new(),
    )
    .unwrap();
    assert_eq!(ok.views, drawing.views.len());
    drawing.views[0].scale = one(1, 0);
    let err = prepare_drawing(
        &drawing,
        &[("K".into(), s.bodies[0].solid.clone())],
        compute_view,
        &Progress::new(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        "Die Zeichnung enthält Ansichten, die sich nicht auf dem Blatt anordnen lassen."
    );
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
fn cards_stay_centred_720_with_equal_heights() {
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
        (rect.width() - 720.0).abs() <= 1.0 && (rect.center().x - 600.0).abs() <= 1.0,
        "720 breit und mittig: {rect:?}"
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

// Blattrahmen (Zeichnungsblatt, Aufgabe 6) ------------------------------------------------------

/// Setzt den Titel des Blatts; alle anderen Angaben bleiben automatisch.
fn set_title(s: &mut AppState, id: DrawingId, title: &str) {
    let mut info = SheetInfo::new(&Stamp {
        date: "2026-10-17".into(),
        user: "dennis".into(),
    });
    info.title = FieldValue::Text(title.to_string());
    s.document.set_sheet_info(id, info).unwrap();
}

/// Eine Änderung mit der Beschreibung `text`; alle anderen Angaben bleiben automatisch.
fn set_revision_text(s: &mut AppState, id: DrawingId, text: &str) {
    let mut info = SheetInfo::new(&Stamp {
        date: "2026-10-17".into(),
        user: "dennis".into(),
    });
    info.add_revision("2026-10-17", "dennis").unwrap();
    info.revisions[0].description = text.to_string();
    s.document.set_sheet_info(id, info).unwrap();
}

/// Schreibt mit dem Fenster nach `path` und liefert den Inhalt.
fn written(d: &mut ExportDialog, s: &AppState, path: &Path) -> String {
    d.tick(s, true);
    d.write_to(path.to_path_buf(), s);
    d.tick(s, true);
    done(d);
    fs::read_to_string(path).unwrap()
}

fn framed_dialog(s: &AppState, format: ExportFormat) -> ExportDialog {
    let mut d = ExportDialog::new(s);
    assert!(d.with_frame, "Vorgabe: Blattrahmen an");
    d.format = format;
    d.tick(s, true);
    d
}

#[test]
fn frame_switch_controls_the_output() {
    let mut s = box_state();
    let id = add_drawing(&mut s, "Blatt 1");
    let dir = temp_dir("rahmen");

    // Der Schalter steht nur in der Karte der Zeichnungsformate.
    let mut d = ExportDialog::new(&s);
    for format in [ExportFormat::Mesh(Format::Stl), ExportFormat::Step] {
        d.format = format;
        let ui = draw(&mut d, &mut s);
        assert!(!ui.contains(FRAME_SWITCH), "{format:?}: {ui}");
    }
    for format in [SVG, DXF] {
        d.format = format;
        let ui = draw(&mut d, &mut s);
        assert!(ui.contains(FRAME_SWITCH), "{format:?}: {ui}");
    }

    // An: Rahmen und Schriftfeld in beiden Formaten.
    let mut d = framed_dialog(&s, SVG);
    let svg = written(&mut d, &s, &dir.join("an.svg"));
    assert!(svg.contains("id=\"Blattrahmen\""), "SVG ohne Rahmen");
    assert!(svg.contains("1 von 1"), "SVG ohne Blattangabe");
    let mut d = framed_dialog(&s, DXF);
    let dxf = written(&mut d, &s, &dir.join("an.dxf"));
    assert!(dxf.contains("BLATTRAHMEN"), "DXF ohne Ebene");

    // Aus: weder noch; Umschalten rechnet neu (anderer Schlüssel).
    let mut d = framed_dialog(&s, SVG);
    assert!(matches!(d.status(&s), ExportStatus::DrawingReady { .. }));
    d.with_frame = false;
    assert!(
        matches!(d.status(&s), ExportStatus::Computing(_)),
        "Umschalten startet eine neue Berechnung"
    );
    let svg = written(&mut d, &s, &dir.join("aus.svg"));
    assert!(!svg.contains("id=\"Blattrahmen\"") && !svg.contains("1 von 1"));
    let mut d = framed_dialog(&s, DXF);
    d.with_frame = false;
    let dxf = written(&mut d, &s, &dir.join("aus.dxf"));
    assert!(!dxf.contains("BLATTRAHMEN"));
    // Wieder an: wieder neu und mit Rahmen.
    d.with_frame = true;
    assert!(matches!(d.status(&s), ExportStatus::Computing(_)));
    let dxf = written(&mut d, &s, &dir.join("wieder.dxf"));
    assert!(dxf.contains("BLATTRAHMEN"));

    // Ein Zeichen außerhalb der Grundebene: DXF meldet das Feld, SVG wird geschrieben.
    set_title(&mut s, id, "😀");
    let mut d = framed_dialog(&s, DXF);
    let ExportStatus::Failed(m) = d.status(&s) else {
        panic!("{:?}", d.status(&s));
    };
    assert_eq!(
        m,
        "Das Zeichen „😀“ im Feld „Titel“ lässt sich als DXF nicht ausgeben. Bitte ändern."
    );
    let ui = draw(&mut d, &mut s);
    assert!(ui.contains(&m), "Meldung im Fenster: {ui}");
    let mut d = framed_dialog(&s, SVG);
    let svg = written(&mut d, &s, &dir.join("emoji.svg"));
    assert!(svg.contains("😀"));
    // Ohne Rahmen gibt es das Feld nicht, also auch keine Meldung.
    let mut d = ExportDialog::new(&s);
    d.with_frame = false;
    d.format = DXF;
    d.tick(&s, true);
    assert!(matches!(d.status(&s), ExportStatus::DrawingReady { .. }));

    // Lange Texte: Ä steht in der Datei als sieben Zeichen („\U+00C4“).
    let long_title = "Ä".repeat(60);
    assert!(!dxf_text_fits(&long_title), "Testvoraussetzung");
    set_title(&mut s, id, &long_title);
    let d = framed_dialog(&s, DXF);
    assert_eq!(
        d.status(&s),
        ExportStatus::Failed("Der Text im Feld „Titel“ ist für DXF zu lang. Bitte kürzen.".into())
    );
    let long_text = "Ä".repeat(120);
    assert!(!dxf_text_fits(&long_text), "Testvoraussetzung");
    set_revision_text(&mut s, id, &long_text);
    let d = framed_dialog(&s, DXF);
    assert_eq!(
        d.status(&s),
        ExportStatus::Failed(
            "Der Text im Feld „Änderung A“ ist für DXF zu lang. Bitte kürzen.".into()
        )
    );
    // Ein Text, der passt (30 × Ä), geht durch.
    assert!(dxf_text_fits(&"Ä".repeat(30)), "Testvoraussetzung");
    set_revision_text(&mut s, id, &"Ä".repeat(30));
    let mut d = framed_dialog(&s, DXF);
    let dxf = written(&mut d, &s, &dir.join("passt.dxf"));
    assert!(dxf.contains("BLATTRAHMEN"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn dxf_sheet_texts_report_the_field() {
    use planora_drawing::TitleField;
    use planora_drawing::sheet_info::TitleTexts;
    let mut t = TitleTexts {
        fields: BTreeMap::new(),
        revisions: Vec::new(),
        notes: vec!["gut".into(), "x😀".into()],
    };
    t.fields.insert(TitleField::Owner, "Eigentümer GmbH".into());
    assert_eq!(
        dxf_sheet_texts_ok(&t),
        Err(
            "Das Zeichen „😀“ im Feld „Hinweis 2“ lässt sich als DXF nicht ausgeben. Bitte ändern."
                .into()
        )
    );
    t.notes = vec!["Ä".repeat(60)];
    assert_eq!(
        dxf_sheet_texts_ok(&t),
        Err("Der Text im Feld „Hinweis 1“ ist für DXF zu lang. Bitte kürzen.".into())
    );
    t.notes = vec!["Kanten gebrochen 0,3 mm".into()];
    t.revisions = vec![[
        "B".into(),
        "Ä".repeat(60),
        "2026-10-17".into(),
        "dennis".into(),
    ]];
    assert_eq!(
        dxf_sheet_texts_ok(&t),
        Err("Der Text im Feld „Änderung B“ ist für DXF zu lang. Bitte kürzen.".into())
    );
    t.revisions.clear();
    assert_eq!(dxf_sheet_texts_ok(&t), Ok(()));
}

#[test]
fn export_uses_current_auto_values() {
    let mut s = box_state();
    add_drawing(&mut s, "Blatt 1");
    let dir = temp_dir("automatisch");
    s.document_name = "Alt".into();
    let mut d = framed_dialog(&s, SVG);
    assert!(matches!(d.status(&s), ExportStatus::DrawingReady { .. }));
    // Der Name des Dokuments ändert sich bei offenem Fenster (Tab umbenannt).
    s.document_name = "Neu".into();
    assert!(
        matches!(d.status(&s), ExportStatus::Computing(_)),
        "der alte Stand gilt nicht mehr"
    );
    let svg = written(&mut d, &s, &dir.join("neu.svg"));
    assert!(svg.contains("Neu-01"), "neue Nummer fehlt");
    assert!(!svg.contains("Alt-01"), "alte Nummer steht noch da");
    // Ein Stand mit altem Namen wird nicht mehr geschrieben.
    s.document_name = "Dritter".into();
    d.write_to(dir.join("dritter.svg"), &s);
    assert_eq!(d.write_state(), WriteState::Failed(STALE.into()));
    let _ = fs::remove_dir_all(&dir);
}

/// Höhen (Gruppe 40) der TEXT-Objekte einer DXF-Datei mit dem Inhalt `text` (Gruppe 1).
fn dxf_text_heights(dxf: &str, text: &str) -> Vec<f64> {
    let lines: Vec<&str> = dxf.lines().map(str::trim).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < lines.len() {
        if lines[i] == "0" && lines[i + 1] == "TEXT" {
            let (mut h, mut t) = (None, None);
            let mut j = i + 2;
            while j + 1 < lines.len() && lines[j] != "0" {
                match lines[j] {
                    "40" => h = lines[j + 1].parse::<f64>().ok(),
                    "1" => t = Some(lines[j + 1]),
                    _ => {}
                }
                j += 2;
            }
            if t == Some(text) {
                out.extend(h);
            }
            i = j;
        } else {
            i += 2;
        }
    }
    out
}

#[test]
fn dxf_frame_texts_get_room_for_cad_fonts() {
    // 33 Zeichen Eigentümer auf A3: in SVG in voller Höhe 3,5 mm, in DXF wegen der breiteren
    // CAD-Schriften kleiner (mit Zuschlag 1,45 auf die geschätzte Breite), aber vollständig.
    let owner = "Mustermann Maschinenbau GmbH Nord";
    assert_eq!(owner.chars().count(), 33);
    let mut s = box_state();
    add_drawing(&mut s, "Blatt 1");
    let mut project = planora_drawing::sheet_info::ProjectInfo::new(&Stamp {
        date: "2026-10-17".into(),
        user: "dennis".into(),
    });
    project.owner = FieldValue::Text(owner.to_string());
    s.document.set_project_info(project).unwrap();
    let dir = temp_dir("cad-schrift");
    let mut d = framed_dialog(&s, SVG);
    let svg = written(&mut d, &s, &dir.join("e.svg"));
    let at = svg
        .find(&format!(">{owner}</text>"))
        .expect("Eigentümer im SVG");
    let element = &svg[svg[..at].rfind("<text").unwrap()..at];
    assert!(element.contains("font-size=\"3.5\""), "{element}");
    let mut d = framed_dialog(&s, DXF);
    let dxf = written(&mut d, &s, &dir.join("e.dxf"));
    let heights = dxf_text_heights(&dxf, owner);
    assert_eq!(heights.len(), 1, "Eigentümer vollständig in der DXF-Datei");
    let h = heights[0];
    assert!(h < 3.5 && h >= 2.1, "{h}");
    let width = planora_drawing::sheet_marks::sheet_text_width(owner, h);
    assert!(width * 1.45 <= 78.0 + 1e-6, "{width}");
    let _ = fs::remove_dir_all(&dir);
}
