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
    // Erst warten, bis die Hilfsdatei da ist, oder bis das Schreiben schon fertig ist (die Datei
    // ist dann nicht mehr „alt“); beides ist ein gültiger Ausgangspunkt, nur das Ausbleiben nicht.
    let start = std::time::Instant::now();
    let finished = |t: &std::io::Result<String>| t.as_ref().is_ok_and(|t| t != "alt");
    while temp_files(&dir) == 0
        && !finished(&fs::read_to_string(&target))
        && start.elapsed().as_secs() < 10
    {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        temp_files(&dir) > 0 || finished(&fs::read_to_string(&target)),
        "weder Hilfsdatei noch fertige Datei gesehen"
    );
    assert!(!w.request_close(0));
    w.discard_and_continue();
    assert!(w.tabs[0].export_dialog.is_none());
    let start = std::time::Instant::now();
    while temp_files(&dir) > 0 && start.elapsed().as_secs() < 10 {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(temp_files(&dir), 0);
    // Die Zieldatei ist alt oder fertig; beim Ersetzen kann ein Lesen kurz scheitern.
    let mut text = fs::read_to_string(&target);
    for _ in 0..100 {
        if text.is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
        text = fs::read_to_string(&target);
    }
    let text = text.unwrap();
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
        "Das Zeichen U+1F600 im Feld „Titel“ lässt sich als DXF nicht ausgeben. Bitte ändern."
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
        parts: Vec::new(),
    };
    t.fields.insert(TitleField::Owner, "Eigentümer GmbH".into());
    assert_eq!(
        dxf_sheet_texts_ok(&t),
        Err(
            "Das Zeichen U+1F600 im Feld „Hinweis 2“ lässt sich als DXF nicht ausgeben. Bitte ändern."
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
    assert!((2.1..3.5).contains(&h), "{h}");
    let width = planora_drawing::sheet_marks::sheet_text_width(owner, h);
    assert!(width * 1.45 <= 78.0 + 1e-6, "{width}");
    let _ = fs::remove_dir_all(&dir);
}

// Stückliste und Positionsnummern (Teilprojekt 3c-2, Aufgabe 8) -------------------------------

const A4_HOCH: Sheet = Sheet {
    paper: Paper::A4,
    landscape: false,
};

fn stamp() -> Stamp {
    Stamp {
        date: "2026-10-18".into(),
        user: "dennis".into(),
    }
}

/// `n` Quader mit den Namen „Teil 1“ … „Teil n“ (jeder eine eigene Zeile der Stückliste).
fn named_boxes(n: usize) -> (AppState, Vec<planora_feature::FeatureId>) {
    let mut s = AppState::new();
    let ids: Vec<_> = (0..n)
        .map(|i| {
            let f = s
                .create_box(DVec3::new(40.0, 30.0, 20.0 + i as f64))
                .unwrap();
            s.rename_feature(f, &format!("Teil {}", i + 1)).unwrap();
            f
        })
        .collect();
    s.selected_body = None;
    (s, ids)
}

fn body_key(f: planora_feature::FeatureId) -> planora_drawing::BodyKey {
    planora_drawing::BodyKey {
        feature: f.0,
        index: 0,
    }
}

fn set_parts_switch(s: &mut AppState, id: DrawingId, on: bool) {
    let mut info = s
        .document
        .drawing(id)
        .unwrap()
        .info
        .clone()
        .unwrap_or_else(|| SheetInfo::new(&stamp()));
    info.parts_list = on;
    s.document.set_sheet_info(id, info).unwrap();
}

fn front_of(s: &AppState, id: DrawingId) -> ViewId {
    s.document.drawing(id).unwrap().views[0].id
}

fn set_name_override(s: &mut AppState, f: planora_feature::FeatureId, name: &str) {
    s.document
        .set_parts_override(
            body_key(f),
            planora_drawing::RowOverride {
                name: FieldValue::Text(name.to_string()),
                ..Default::default()
            },
        )
        .unwrap();
}

/// Der Inhalt der SVG-Gruppe `id` (leer, wenn es sie nicht gibt).
fn svg_group<'a>(svg: &'a str, id: &str) -> &'a str {
    let open = format!("<g id=\"{id}\">");
    match svg.find(&open) {
        Some(at) => {
            let rest = &svg[at + open.len()..];
            &rest[..rest.find("</g>").unwrap_or(rest.len())]
        }
        None => "",
    }
}

/// Ebene und Text jedes `TEXT` einer DXF-Datei.
fn dxf_entity_texts(dxf: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = dxf.lines().map(str::trim).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < lines.len() {
        if lines[i] == "0" && lines[i + 1] == "TEXT" {
            let (mut layer, mut text) = (None, None);
            let mut j = i + 2;
            while j + 1 < lines.len() && lines[j] != "0" {
                match lines[j] {
                    "8" => layer = Some(lines[j + 1].to_string()),
                    "1" => text = Some(lines[j + 1].to_string()),
                    _ => {}
                }
                j += 2;
            }
            if let (Some(l), Some(t)) = (layer, text) {
                out.push((l, t));
            }
            i = j;
        } else {
            i += 2;
        }
    }
    out
}

fn dxf_has_text(dxf: &str, layer: &str, text: &str) -> bool {
    dxf_entity_texts(dxf)
        .iter()
        .any(|(l, t)| l == layer && t == text)
}

/// Die Texte des Blatts in der Datei, gleich welcher Ebene.
fn dxf_all_texts(dxf: &str) -> Vec<String> {
    dxf_entity_texts(dxf).into_iter().map(|(_, t)| t).collect()
}

fn dialog_with(s: &AppState, format: ExportFormat, frame: bool, id: DrawingId) -> ExportDialog {
    let mut d = ExportDialog::new(s);
    d.with_frame = frame;
    d.format = format;
    d.drawing = Some(id);
    d.tick(s, true);
    d
}

/// Ob der Text `t` als eigener Text im Ergebnis steht (SVG: `<text>`, DXF: `TEXT`).
fn has_text(format: ExportFormat, out: &str, t: &str) -> bool {
    match format {
        ExportFormat::Drawing(DrawingFormat::Svg) => out.contains(&format!(">{t}</text>")),
        _ => dxf_all_texts(out).iter().any(|x| x == t),
    }
}

#[test]
fn balloons_always_written() {
    let (mut s, f) = named_boxes(2);
    let id = add_drawing(&mut s, "Blatt 1");
    let front = front_of(&s, id);
    s.document
        .add_balloon(id, front, body_key(f[0]), (0.5, 0.5), (-15.0, 10.0))
        .unwrap();
    s.document
        .add_balloon(id, front, body_key(f[1]), (0.2, 0.8), (-25.0, 5.0))
        .unwrap();
    let dir = temp_dir("kreise");
    for frame in [false, true] {
        let mut d = dialog_with(&s, SVG, frame, id);
        let svg = written(&mut d, &s, &dir.join(format!("k{frame}.svg")));
        let group = svg_group(&svg, "Vorne Positionen");
        assert!(
            group.contains(">1</text>") && group.contains(">2</text>"),
            "{frame}: {group}"
        );
        assert!(group.contains("<path"), "Kreis fehlt, Rahmen {frame}");
        assert_eq!(svg.contains("id=\"Blattrahmen\""), frame);
        let mut d = dialog_with(&s, DXF, frame, id);
        let dxf = written(&mut d, &s, &dir.join(format!("k{frame}.dxf")));
        assert!(dxf_has_text(&dxf, "POSITIONEN", "1"), "{frame}");
        assert!(dxf_has_text(&dxf, "POSITIONEN", "2"), "{frame}");
        assert!(dxf.contains("CIRCLE"), "{frame}");
        assert_eq!(dxf.contains("BLATTRAHMEN"), frame);
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn table_follows_frame_switch() {
    let (mut s, _) = named_boxes(2);
    let id = add_drawing(&mut s, "Blatt 1");
    set_parts_switch(&mut s, id, true);
    let dir = temp_dir("tabelle");
    // Rahmen an und Schalter im Blatt an: Tabelle in beiden Formaten.
    let mut d = dialog_with(&s, SVG, true, id);
    let svg = written(&mut d, &s, &dir.join("an.svg"));
    for t in ["Benennung", "Teil 1", "Teil 2", "Menge"] {
        assert!(svg.contains(&format!(">{t}</text>")), "SVG ohne {t}");
    }
    let mut d = dialog_with(&s, DXF, true, id);
    let dxf = written(&mut d, &s, &dir.join("an.dxf"));
    for t in ["Benennung", "Teil 1", "Teil 2"] {
        assert!(dxf_has_text(&dxf, "BLATT", t), "DXF ohne {t}");
    }
    // Rahmen aus: keine Tabelle (nur der Rahmen schaltet sie), das Blatt selbst bleibt.
    let mut d = dialog_with(&s, SVG, false, id);
    let svg = written(&mut d, &s, &dir.join("aus.svg"));
    assert!(!svg.contains("Benennung") && !svg.contains("Teil 2"));
    assert!(svg.contains("id=\"Vorne\""));
    let mut d = dialog_with(&s, DXF, false, id);
    let dxf = written(&mut d, &s, &dir.join("aus.dxf"));
    assert!(!dxf.contains("Benennung") && !dxf.contains("BLATTRAHMEN"));
    // Umschalten bei offenem Fenster rechnet neu.
    d.with_frame = true;
    assert!(matches!(d.status(&s), ExportStatus::Computing(_)));
    let dxf = written(&mut d, &s, &dir.join("wieder.dxf"));
    assert!(dxf.contains("Benennung"));
    // Schalter im Blatt aus: nie eine Tabelle, auch mit Rahmen.
    set_parts_switch(&mut s, id, false);
    for format in [SVG, DXF] {
        let mut d = dialog_with(&s, format, true, id);
        let ext = format.extension();
        let out = written(&mut d, &s, &dir.join(format!("nie.{ext}")));
        assert!(!out.contains("Benennung"), "{format:?}");
        assert!(
            out.contains("Fertigungszeichnung"),
            "Rahmen ist da: {format:?}"
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn parts_sheet_exports_its_table() {
    let (mut s, _) = named_boxes(45);
    let first = add_drawing(&mut s, "Blatt 1");
    s.drawing_ui.open(first);
    let p1 = planora_app::parts_section::create_or_open(&mut s, A4_HOCH).unwrap();
    let sheets = s.document.parts_sheets();
    assert_eq!(sheets.len(), 2);
    let p2 = sheets[1];
    assert_eq!(sheets[0], p1);
    let dir = temp_dir("stueckliste");
    for frame in [false, true] {
        for format in [SVG, DXF] {
            let ext = format.extension();
            for (n, page) in [(1, p1), (2, p2)] {
                let mut d = dialog_with(&s, format, frame, page);
                let ExportStatus::DrawingReady { views, .. } = d.status(&s) else {
                    panic!("{:?}", d.status(&s));
                };
                assert_eq!(views, 0);
                let out = written(&mut d, &s, &dir.join(format!("s{frame}{n}.{ext}")));
                let at = format!("{format:?} Rahmen {frame} Blatt {n}");
                assert!(out.contains("Benennung"), "{at}");
                assert!(!out.contains("Fertigungszeichnung"), "{at}");
                // Blatt 1: Pos. 1 bis 44; Blatt 2: Pos. 45.
                assert_eq!(has_text(format, &out, "Teil 1"), n == 1, "{at}");
                assert_eq!(has_text(format, &out, "Teil 44"), n == 1, "{at}");
                assert_eq!(has_text(format, &out, "Teil 45"), n == 2, "{at}");
                assert_eq!(has_text(format, &out, "45"), n == 2, "Pos. 45: {at}");
                // Der Rahmen gehört dem Schalter.
                match format.drawing_format().unwrap() {
                    DrawingFormat::Svg => {
                        assert_eq!(out.contains(">Stückliste</text>"), frame, "{at}");
                    }
                    DrawingFormat::Dxf => {
                        assert_eq!(out.contains("BLATTRAHMEN"), frame, "{at}");
                        assert!(out.contains("\r\nBLATT\r\n"), "Ebene BLATT {at}");
                    }
                }
            }
        }
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn lost_balloon_blocks_export() {
    let lost = "Die Positionsnummer in der Ansicht „Vorne“ gehört zu einem Teil, das es nicht mehr gibt. Bitte die Positionsnummer löschen und neu setzen.".to_string();
    assert_eq!(lost_balloon_text("Vorne"), lost);
    let dir = temp_dir("verloren");
    for format in [SVG, DXF] {
        for frame in [false, true] {
            let (mut s, f) = named_boxes(2);
            let id = add_drawing(&mut s, "Blatt 1");
            let front = front_of(&s, id);
            let b0 = s
                .document
                .add_balloon(id, front, body_key(f[0]), (0.5, 0.5), (-15.0, 10.0))
                .unwrap();
            let b1 = s
                .document
                .add_balloon(id, front, body_key(f[1]), (0.5, 0.5), (-25.0, 10.0))
                .unwrap();
            let at = format!("{format:?} Rahmen {frame}");
            let mut d = dialog_with(&s, format, frame, id);
            assert!(matches!(d.status(&s), ExportStatus::DrawingReady { .. }));
            // Der zweite Körper verschwindet: Kreis 2 ist verloren.
            s.delete_body(1).unwrap();
            let target = dir.join(format!("v.{}", format.extension()));
            d.tick(&s, true);
            assert_eq!(d.status(&s), ExportStatus::Failed(lost.clone()), "{at}");
            let ui = draw(&mut d, &mut s);
            assert!(ui.contains(&lost), "Meldung im Fenster: {ui}");
            d.write_to(target.clone(), &s);
            assert_eq!(d.write_state(), WriteState::Failed(lost.clone()), "{at}");
            assert!(!target.exists(), "keine Datei: {at}");
            // Kreis 2 löschen: der Export gelingt (ohne ihn).
            s.document
                .update_drawing(id, |dr| {
                    dr.remove_balloon(b1);
                })
                .unwrap();
            let out = written(&mut d, &s, &target);
            if format == SVG {
                let g = svg_group(&out, "Vorne Positionen");
                assert!(g.contains(">1</text>") && !g.contains(">2</text>"), "{g}");
            } else {
                assert!(dxf_has_text(&out, "POSITIONEN", "1"), "{at}");
                assert!(!dxf_has_text(&out, "POSITIONEN", "2"), "{at}");
            }
            let _ = (b0, fs::remove_file(&target));
        }
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn dxf_rejects_bad_cell_text() {
    let (mut s, f) = named_boxes(2);
    let id = add_drawing(&mut s, "Blatt 1");
    set_parts_switch(&mut s, id, true);
    set_name_override(&mut s, f[0], "😀");
    let dir = temp_dir("zelle");
    // Zeichnungsblatt mit Rahmen: DXF nennt Zeile und Spalte, SVG wird geschrieben.
    let d = dialog_with(&s, DXF, true, id);
    let named = "Das Zeichen U+1F600 im Feld „Stückliste, Pos. 1, Benennung“ lässt sich als DXF nicht ausgeben. Bitte ändern.";
    assert_eq!(d.status(&s), ExportStatus::Failed(named.into()));
    let mut d = dialog_with(&s, SVG, true, id);
    assert!(written(&mut d, &s, &dir.join("e.svg")).contains("😀"));
    // Ohne Rahmen steht die Tabelle nicht in der Datei, also gibt es keine Meldung.
    let d = dialog_with(&s, DXF, false, id);
    assert!(matches!(d.status(&s), ExportStatus::DrawingReady { .. }));
    // Stücklistenblatt: auch ohne Rahmen, mit dem Namen der Zeile und der Spalte.
    s.drawing_ui.open(id);
    let p1 = planora_app::parts_section::create_or_open(&mut s, A4_HOCH).unwrap();
    for frame in [false, true] {
        let d = dialog_with(&s, DXF, frame, p1);
        assert_eq!(d.status(&s), ExportStatus::Failed(named.into()), "{frame}");
        let mut d = dialog_with(&s, SVG, frame, p1);
        assert!(written(&mut d, &s, &dir.join("l.svg")).contains("😀"));
    }
    // Zu lange Texte je Spalte: Zeile 2, Spalte Bemerkung bzw. Werkstoff.
    set_name_override(&mut s, f[0], "Welle");
    let long = "Ä".repeat(60);
    assert!(!dxf_text_fits(&long), "Testvoraussetzung");
    s.document
        .set_parts_override(
            body_key(f[1]),
            planora_drawing::RowOverride {
                note: FieldValue::Text(long.clone()),
                ..Default::default()
            },
        )
        .unwrap();
    let d = dialog_with(&s, DXF, false, p1);
    assert_eq!(
        d.status(&s),
        ExportStatus::Failed(
            "Der Text im Feld „Stückliste, Pos. 2, Bemerkung“ ist für DXF zu lang. Bitte kürzen."
                .into()
        )
    );
    s.document
        .set_parts_override(
            body_key(f[1]),
            planora_drawing::RowOverride {
                material: FieldValue::Text(long),
                ..Default::default()
            },
        )
        .unwrap();
    let d = dialog_with(&s, DXF, true, p1);
    assert_eq!(
        d.status(&s),
        ExportStatus::Failed(
            "Der Text im Feld „Stückliste, Pos. 2, Werkstoff“ ist für DXF zu lang. Bitte kürzen."
                .into()
        )
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn special_chars_in_cells() {
    let (mut s, f) = named_boxes(2);
    let id = add_drawing(&mut s, "Blatt 1");
    set_parts_switch(&mut s, id, true);
    set_name_override(&mut s, f[0], "ä Ø <&> \"x\"");
    let dir = temp_dir("sonder");
    s.drawing_ui.open(id);
    let p1 = planora_app::parts_section::create_or_open(&mut s, A4_HOCH).unwrap();
    for (page, frame) in [(id, true), (p1, false), (p1, true)] {
        let mut d = dialog_with(&s, SVG, frame, page);
        let svg = written(&mut d, &s, &dir.join("s.svg"));
        assert!(
            svg.contains(">ä Ø &lt;&amp;&gt; &quot;x&quot;</text>"),
            "maskiert ({frame})"
        );
        assert!(!svg.contains("<&>"));
        // Jedes kaufmännische Und gehört zu einer Maskierung.
        for (at, _) in svg.match_indices('&') {
            let rest = &svg[at..];
            assert!(
                ["&amp;", "&lt;", "&gt;", "&quot;", "&apos;", "&#"]
                    .iter()
                    .any(|e| rest.starts_with(e)),
                "{}",
                &rest[..rest.len().min(20)]
            );
        }
        assert_eq!(svg.matches("<g ").count(), svg.matches("</g>").count());
        let mut d = dialog_with(&s, DXF, frame, page);
        let dxf = written(&mut d, &s, &dir.join("s.dxf"));
        assert!(
            dxf_all_texts(&dxf)
                .iter()
                .any(|t| t == "\\U+00E4 %%c <&> \"x\""),
            "DXF-Schreibweise ({frame}): {:?}",
            dxf_all_texts(&dxf)
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn override_change_restarts() {
    let (mut s, f) = named_boxes(2);
    let id = add_drawing(&mut s, "Blatt 1");
    set_parts_switch(&mut s, id, true);
    let dir = temp_dir("ueberschreiben");
    for format in [SVG, DXF] {
        let mut d = dialog_with(&s, format, true, id);
        assert!(matches!(d.status(&s), ExportStatus::DrawingReady { .. }));
        let text = format!("Neu {}", format.extension());
        set_name_override(&mut s, f[0], &text);
        assert!(
            matches!(d.status(&s), ExportStatus::Computing(_)),
            "eine Überschreibung startet die Berechnung neu"
        );
        d.write_to(dir.join("alt"), &s);
        assert_eq!(d.write_state(), WriteState::Failed(STALE.into()));
        let out = written(&mut d, &s, &dir.join(format!("n.{}", format.extension())));
        assert!(has_text(format, &out, &text), "{format:?}");
        assert!(!has_text(format, &out, "Teil 1"), "{format:?}");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn every_change_to_the_output_restarts_the_computation() {
    let (mut s, f) = named_boxes(2);
    let id = add_drawing(&mut s, "Blatt 1");
    set_parts_switch(&mut s, id, true);
    let front = front_of(&s, id);
    let b = s
        .document
        .add_balloon(id, front, body_key(f[0]), (0.5, 0.5), (-15.0, 10.0))
        .unwrap();
    let dir = temp_dir("schluessel");
    let mut d = dialog_with(&s, SVG, true, id);
    let before = written(&mut d, &s, &dir.join("0.svg"));
    // Kreis verschieben.
    s.document
        .update_drawing(id, |dr| {
            dr.balloons.iter_mut().find(|x| x.id == b).unwrap().offset = (-40.0, 20.0);
        })
        .unwrap();
    d.write_to(dir.join("1.svg"), &s);
    assert_eq!(
        d.write_state(),
        WriteState::Failed(STALE.into()),
        "Kreis verschoben"
    );
    let moved = written(&mut d, &s, &dir.join("1.svg"));
    assert_ne!(
        svg_group(&before, "Vorne Positionen"),
        svg_group(&moved, "Vorne Positionen")
    );
    // Körper umbenennen.
    s.rename_feature(f[1], "Platte").unwrap();
    d.write_to(dir.join("2.svg"), &s);
    assert_eq!(
        d.write_state(),
        WriteState::Failed(STALE.into()),
        "umbenannt"
    );
    let renamed = written(&mut d, &s, &dir.join("2.svg"));
    assert!(renamed.contains(">Platte</text>") && !renamed.contains(">Teil 2</text>"));
    // Körper hinzufügen.
    let g = s.create_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
    s.rename_feature(g, "Neu").unwrap();
    s.selected_body = None;
    d.write_to(dir.join("3.svg"), &s);
    assert_eq!(
        d.write_state(),
        WriteState::Failed(STALE.into()),
        "hinzugefügt"
    );
    let added = written(&mut d, &s, &dir.join("3.svg"));
    assert!(added.contains(">Neu</text>"));
    // Schalter im Blatt aus.
    set_parts_switch(&mut s, id, false);
    d.write_to(dir.join("4.svg"), &s);
    assert_eq!(
        d.write_state(),
        WriteState::Failed(STALE.into()),
        "Schalter im Blatt"
    );
    let off = written(&mut d, &s, &dir.join("4.svg"));
    assert!(!off.contains("Benennung"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn dxf_handles_stay_unique_with_table_and_balloons() {
    let (mut s, f) = named_boxes(3);
    let id = add_drawing(&mut s, "Blatt 1");
    set_parts_switch(&mut s, id, true);
    let front = front_of(&s, id);
    for (k, off) in f.iter().zip([-15.0, -25.0, -35.0]) {
        s.document
            .add_balloon(id, front, body_key(*k), (0.5, 0.5), (off, 10.0))
            .unwrap();
    }
    s.drawing_ui.open(id);
    let p1 = planora_app::parts_section::create_or_open(&mut s, A4_HOCH).unwrap();
    let dir = temp_dir("handles");
    for (page, frame) in [(id, true), (p1, false), (p1, true)] {
        let mut d = dialog_with(&s, DXF, frame, page);
        let dxf = written(&mut d, &s, &dir.join("h.dxf"));
        let lines: Vec<&str> = dxf.lines().map(str::trim).collect();
        let mut handles = Vec::new();
        let mut i = 0;
        while i + 1 < lines.len() {
            if lines[i] == "5" && !(i >= 2 && lines[i - 1] == "$HANDSEED") {
                handles.push(lines[i + 1].to_string());
            }
            i += 2;
        }
        assert!(handles.len() > 30, "{}", handles.len());
        let mut sorted = handles.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), handles.len(), "Griffe doppelt ({frame})");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn dxf_table_texts_get_room_for_cad_fonts_with_and_without_frame() {
    let (mut s, f) = named_boxes(2);
    let id = add_drawing(&mut s, "Blatt 1");
    s.drawing_ui.open(id);
    let name = "W".repeat(36);
    set_name_override(&mut s, f[0], &name);
    let p1 = planora_app::parts_section::create_or_open(&mut s, A4_HOCH).unwrap();
    let dir = temp_dir("dxf-breite");
    let mut heights = Vec::new();
    for frame in [false, true] {
        let mut d = dialog_with(&s, DXF, frame, p1);
        let dxf = written(&mut d, &s, &dir.join("b.dxf"));
        let h = dxf_text_heights(&dxf, &name);
        assert_eq!(h.len(), 1, "vollständig ({frame})");
        heights.push(h[0]);
    }
    let mut d = dialog_with(&s, SVG, false, p1);
    let svg = written(&mut d, &s, &dir.join("b.svg"));
    let at = svg.find(&format!(">{name}</text>")).unwrap();
    let element = &svg[svg[..at].rfind("<text").unwrap()..at];
    let svg_h: f64 = element
        .split("font-size=\"")
        .nth(1)
        .and_then(|r| r.split('"').next())
        .and_then(|v| v.parse().ok())
        .unwrap();
    // In DXF kleiner als in SVG (Zuschlag für breitere CAD-Schriften), mit und ohne Rahmen gleich.
    assert!(heights[0] < svg_h - 0.1, "{heights:?} {svg_h}");
    assert!((heights[0] - heights[1]).abs() < 1e-9, "{heights:?}");
    let _ = fs::remove_dir_all(&dir);
}

// Nachbesserung nach Review ------------------------------------------------------------------

#[test]
fn first_lost_balloon_in_drawing_order_names_its_view() {
    let (mut s, f) = named_boxes(2);
    let id = add_drawing(&mut s, "Blatt 1");
    let views = s.document.drawing(id).unwrap().views.clone();
    let front = views[0].id;
    let top = views[1].id;
    let names = planora_app::drawing_view::view_names(s.document.drawing(id).unwrap());
    assert_eq!(names[&front], "Vorne");
    assert_ne!(names[&top], "Vorne");
    // Reihenfolge: heil (Vorne), verloren (Oben), verloren (Vorne).
    for (view, k) in [(front, 0), (top, 1), (front, 1)] {
        s.document
            .add_balloon(id, view, body_key(f[k]), (0.5, 0.5), (-15.0, 10.0))
            .unwrap();
    }
    s.delete_body(1).unwrap();
    for format in [SVG, DXF] {
        let d = dialog_with(&s, format, false, id);
        let want = format!(
            "Die Positionsnummer in der Ansicht „{}“ gehört zu einem Teil, das es nicht mehr gibt. Bitte die Positionsnummer löschen und neu setzen.",
            names[&top]
        );
        assert_eq!(d.status(&s), ExportStatus::Failed(want), "{format:?}");
    }
}

#[test]
fn dxf_cell_message_uses_the_pos_text_of_page_two() {
    let (mut s, f) = named_boxes(45);
    let first = add_drawing(&mut s, "Blatt 1");
    s.drawing_ui.open(first);
    let p1 = planora_app::parts_section::create_or_open(&mut s, A4_HOCH).unwrap();
    let p2 = s.document.parts_sheets()[1];
    set_name_override(&mut s, f[44], "😀");
    for frame in [false, true] {
        let d = dialog_with(&s, DXF, frame, p2);
        assert_eq!(
            d.status(&s),
            ExportStatus::Failed(
                "Das Zeichen U+1F600 im Feld „Stückliste, Pos. 45, Benennung“ lässt sich als DXF nicht ausgeben. Bitte ändern."
                    .into()
            ),
            "{frame}"
        );
        // Blatt 1 enthält die Zeile nicht und geht durch.
        let d = dialog_with(&s, DXF, frame, p1);
        assert!(matches!(d.status(&s), ExportStatus::DrawingReady { .. }));
    }
}

#[test]
fn dxf_cell_check_reports_characters_before_length() {
    let (mut s, f) = named_boxes(2);
    let id = add_drawing(&mut s, "Blatt 1");
    s.drawing_ui.open(id);
    let p1 = planora_app::parts_section::create_or_open(&mut s, A4_HOCH).unwrap();
    let both = format!("😀{}", "Ä".repeat(55));
    assert!(!dxf_text_fits(&both), "Testvoraussetzung");
    set_name_override(&mut s, f[0], &both);
    let d = dialog_with(&s, DXF, false, p1);
    assert_eq!(
        d.status(&s),
        ExportStatus::Failed(
            "Das Zeichen U+1F600 im Feld „Stückliste, Pos. 1, Benennung“ lässt sich als DXF nicht ausgeben. Bitte ändern."
                .into()
        )
    );
}

#[test]
fn parts_sheet_texts_in_the_export_window() {
    let (mut s, _) = named_boxes(2);
    s.import_mesh("Würfel", Arc::new(cube()), 1.0).unwrap();
    s.selected_body = None;
    let first = add_drawing(&mut s, "Blatt 1");
    s.drawing_ui.open(first);
    let p1 = planora_app::parts_section::create_or_open(&mut s, A4_HOCH).unwrap();
    // Fertig: eigener Satz, nur die Blattangabe, keine Meldung zu Netz-Körpern.
    let mut d = dialog_with(&s, SVG, true, p1);
    let ui = draw(&mut d, &mut s);
    assert!(ui.contains("Die Stückliste ist berechnet."), "{ui}");
    assert!(!ui.contains("Alle Ansichten sind berechnet."), "{ui}");
    assert!(ui.contains("Blatt: A4 hoch"), "{ui}");
    for gone in [
        "Ansichten:",
        "Linien:",
        "Maße:",
        "Mittellinien:",
        "Netz-Körper",
    ] {
        assert!(!ui.contains(gone), "{gone}: {ui}");
    }
    // Das Zeichnungsblatt zeigt dagegen alles.
    let mut d = dialog_with(&s, SVG, true, first);
    let ui = draw(&mut d, &mut s);
    assert!(ui.contains("Alle Ansichten sind berechnet."), "{ui}");
    for shown in [
        "Ansichten:",
        "Linien:",
        "Maße:",
        "Mittellinien:",
        "Netz-Körper",
    ] {
        assert!(ui.contains(shown), "{shown}: {ui}");
    }
    // Während der Berechnung (der Text hängt nur von Format und Blattart ab).
    assert_eq!(computing_text(SVG, true), "Die Stückliste wird berechnet …");
    assert_eq!(computing_text(DXF, true), "Die Stückliste wird berechnet …");
    assert_eq!(
        computing_text(SVG, false),
        "Die Ansichten werden berechnet …"
    );
    assert_eq!(
        computing_text(ExportFormat::Step, false),
        "Die Körper werden geprüft …"
    );
    assert_eq!(
        computing_text(ExportFormat::Mesh(Format::Stl), false),
        "Das Netz wird berechnet …"
    );
}

#[test]
fn export_table_geometry_equals_the_screen() {
    use planora_app::drawing_view::{layout_of, sheet_context, sheet_texts};
    use planora_drawing::sheet_marks::sheet_frame;
    for sheet in [A4_HOCH, A3_QUER] {
        let (mut s, _) = named_boxes(2);
        let id = s
            .document
            .add_drawing("Blatt 1", sheet, Projection::First)
            .unwrap();
        s.document
            .add_view(
                id,
                ViewKind::Front,
                one(1, 2),
                Placement::Free { x: 60.0, y: 150.0 },
                None,
            )
            .unwrap();
        set_parts_switch(&mut s, id, true);
        let drawing = s.document.drawing(id).unwrap().clone();
        let texts = sheet_texts(&s, &drawing);
        assert!(!texts.notes.is_empty(), "Hinweisfeld ist da");
        assert_eq!(texts.parts.len(), 2);
        let screen = sheet_frame(&layout_of(&s, &drawing), &texts, drawing.projection);
        let frame = FrameInput {
            project: s.document.project_info().cloned(),
            ctx: sheet_context(&s, id).unwrap(),
            dxf: false,
        };
        let solids: Vec<(String, planora_kernel::Solid)> = s
            .bodies
            .iter()
            .map(|b| (b.name.clone(), b.solid.clone()))
            .collect();
        let out = prepare_drawing_parts(
            &drawing,
            &solids,
            compute_view,
            resolve_dimensions_in_worker,
            Some(&frame),
            &parts_input(&s, &drawing),
            &Progress::new(),
        )
        .unwrap();
        let exported = out.model.frame.as_ref().unwrap();
        assert_eq!(exported.marks.len(), screen.marks.len(), "{sheet:?}");
        assert!(exported.marks == screen.marks, "{sheet:?}");
        assert_eq!(exported.scale, screen.scale);
    }
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "prepare_drawing_parts")]
fn plain_prepare_refuses_drawings_with_balloons() {
    let (mut s, f) = named_boxes(1);
    let id = add_drawing(&mut s, "Blatt 1");
    let front = front_of(&s, id);
    s.document
        .add_balloon(id, front, body_key(f[0]), (0.5, 0.5), (-15.0, 10.0))
        .unwrap();
    let drawing = s.document.drawing(id).unwrap().clone();
    let solids = vec![("Teil 1".to_string(), s.bodies[0].solid.clone())];
    let _ = prepare_drawing(&drawing, &solids, compute_view, &Progress::new());
}
