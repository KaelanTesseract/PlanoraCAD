//! Fenster „Exportieren“ mit Maßen und Mittellinien (Teilprojekt 12, Aufgabe 14): Die Maße
//! werden im Arbeitsthread des Fensters gegen Kopien der exakten Körper aufgelöst, das Blatt
//! entsteht mit `build_sheet_with`; ein Maß mit verlorenem Bezug sperrt das Schreiben (P13), Maße
//! außerhalb des Blatts werden geschrieben und gezählt (P12).
//!
//! Sollwerte aus den Eingabemaßen: Quader 100 × 60 × 40 mm (x, y, z) → Vorderansicht Breite 100,
//! Höhe 40; Draufsicht Tiefe 60. Zylinder r = 5, h = 12 → senkrechte Mittellinie der
//! Vorderansicht durch die Mitte der Ansicht, Länge 12 + 2 · 3 (Überstand). Die Dateien werden
//! echt geschrieben und mit den eigenen Lesern der Zeichnungstests gelesen; verglichen wird mit
//! einem Blatt, das der Test selbst berechnet (`compute_view`, `Document::resolve_dimensions`,
//! `build_sheet_with`).

#[path = "../../planora-drawing/tests/common/dxf_read.rs"]
mod dxf_read;
#[path = "../../planora-drawing/tests/common/svg_read.rs"]
mod svg_read;

use planora_app::dimension_state::{SuggestOutcome, resolve_in, suggest_for};
use planora_app::export_dialog::*;
use planora_app::fonts;
use planora_app::state::*;
use planora_app::testing::collect_texts;
use planora_drawing::annotate::ResolvedDims;
use planora_drawing::dimension::{
    DimId, DimKind, DimSpec, GeoRef, GeoRefData, Tolerance as DimTolerance,
};
use planora_drawing::sheet::{SheetModel, build_sheet, build_sheet_with};
use planora_drawing::{
    DrawBody, DrawError, Drawing, DrawingFormat, DrawingId, Paper, Placement, Projection, Scale,
    Sheet, ViewId, ViewKind, ViewLines, compute_view, dxf_string, svg_string,
};
use planora_exchange::{ExchangeMesh, Progress};
use planora_feature::{FeatureKind, geo_ref_for_edge};
use planora_kernel::Solid;
use planora_math::{DVec3, Tolerance};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const SVG: ExportFormat = ExportFormat::Drawing(DrawingFormat::Svg);
const DXF: ExportFormat = ExportFormat::Drawing(DrawingFormat::Dxf);

const A3_QUER: Sheet = Sheet {
    paper: Paper::A3,
    landscape: true,
};
/// Höhe des Blatts A3 quer in mm.
const A3_H: f64 = 297.0;
const BOX: DVec3 = DVec3::new(100.0, 60.0, 40.0);
/// Lage der Vorderansicht (Mitte) auf dem Blatt.
const FRONT: (f64, f64) = (120.0, 200.0);

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-dims-ui-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

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

fn one(num: u32, den: u32) -> Scale {
    Scale { num, den }
}

/// Zeichnung (A3 quer, Erstwinkel): Vorne frei in `FRONT`, Oben und Links abgeleitet.
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
            Placement::Free {
                x: FRONT.0,
                y: FRONT.1,
            },
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
    d
}

/// Berechnet die Ansichten im Zeichnungsbereich und schlägt die Grundmaße vor.
fn suggest(s: &mut AppState, d: DrawingId) -> usize {
    s.drawing_ui.open(d);
    s.drawing_ui.tick(&s.bodies, &s.document, true);
    match suggest_for(s, d, None) {
        SuggestOutcome::Added(n) => n,
        other => panic!("{other:?}"),
    }
}

/// Quader 100 × 60 × 40 mit Zeichnung und vorgeschlagenen Gesamtmaßen.
fn box_with_dims() -> (AppState, DrawingId) {
    let mut s = AppState::new();
    s.create_box(BOX).unwrap();
    s.selected_body = None;
    let d = add_drawing(&mut s, "Blatt 1");
    assert!(suggest(&mut s, d) >= 3);
    (s, d)
}

fn dialog(s: &AppState, format: ExportFormat) -> ExportDialog {
    let mut d = ExportDialog::new(s);
    d.format = format;
    d.tick(s, true);
    d
}

fn exact(s: &AppState) -> Vec<&Body> {
    s.bodies
        .iter()
        .filter(|b| b.kind != BodyKind::Mesh)
        .collect()
}

/// Ansichten, die der Test selbst aus den exakten Körpern berechnet.
fn own_lines(s: &AppState, drawing: &Drawing) -> Vec<(ViewId, ViewLines)> {
    let bodies: Vec<DrawBody> = exact(s)
        .into_iter()
        .map(|b| DrawBody {
            name: b.name.clone(),
            solid: &b.solid,
        })
        .collect();
    drawing
        .views
        .iter()
        .map(|v| {
            (
                v.id,
                compute_view(&bodies, v.kind, &Progress::new()).unwrap(),
            )
        })
        .collect()
}

/// Maße, die der Test selbst über das Dokument auflöst.
fn own_dims(s: &AppState, id: DrawingId) -> ResolvedDims {
    let solids: Vec<&Solid> = exact(s).into_iter().map(|b| &b.solid).collect();
    s.document.resolve_dimensions(id, &solids).unwrap()
}

/// Blatt mit Maßen, das der Test selbst berechnet.
fn own_sheet(s: &AppState, id: DrawingId) -> SheetModel {
    let drawing = s.document.drawing(id).unwrap();
    let lines = own_lines(s, drawing);
    let map: BTreeMap<ViewId, &ViewLines> = lines.iter().map(|(i, l)| (*i, l)).collect();
    build_sheet_with(drawing, &map, &own_dims(s, id))
}

fn done(d: &ExportDialog) -> (PathBuf, u64) {
    match d.write_state() {
        WriteState::Done(p, n) => (p, n),
        other => panic!("{other:?}"),
    }
}

/// Schreibt mit dem Fenster nach `path` und liefert den Dateiinhalt.
fn write(d: &mut ExportDialog, s: &AppState, path: &Path) -> String {
    d.write_to(path.to_path_buf(), s);
    d.tick(s, true);
    let (p, n) = done(d);
    assert_eq!(p, path);
    let text = fs::read_to_string(path).unwrap();
    assert_eq!(n, text.len() as u64);
    text
}

fn ready(d: &ExportDialog, s: &AppState) -> (usize, usize, CenterlineUse) {
    match d.status(s) {
        ExportStatus::DrawingReady {
            dimensions,
            outside,
            centerlines,
            ..
        } => (dimensions, outside, centerlines),
        other => panic!("{other:?}"),
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// Texte der Gruppe `group` in der SVG-Datei.
fn svg_texts(svg: &svg_read::ReadSvg, group: &str) -> Vec<String> {
    svg.texts_of(group).map(|t| t.text.clone()).collect()
}

/// Entschlüsselte Texte im Block `block` auf der Ebene `layer`.
fn dxf_texts(d: &dxf_read::Dxf, block: &str, layer: &str) -> Vec<(String, String)> {
    d.block(block)
        .unwrap_or_else(|| panic!("Block {block} fehlt"))
        .entities
        .iter()
        .filter(|e| e.kind == "TEXT" && e.layer == layer)
        .map(|e| match e.geom().unwrap() {
            dxf_read::Geom::Text { text, raw, .. } => (text, raw),
            other => panic!("{other:?}"),
        })
        .collect()
}

fn dxf_count(d: &dxf_read::Dxf, block: &str, layer: &str) -> usize {
    d.block(block)
        .unwrap_or_else(|| panic!("Block {block} fehlt"))
        .entities
        .iter()
        .filter(|e| e.layer == layer)
        .count()
}

fn layer_names(d: &dxf_read::Dxf) -> Vec<String> {
    d.table("LAYER")
        .unwrap()
        .records
        .iter()
        .map(|r| r.name())
        .collect()
}

/// Kennung des ersten Maßes der Ansicht `kind` mit der Art `dim`.
fn dim_of(s: &AppState, id: DrawingId, kind: ViewKind, dim: DimKind) -> DimId {
    let d = s.document.drawing(id).unwrap();
    let view = d.views.iter().find(|v| v.kind == kind).unwrap().id;
    d.dimensions
        .iter()
        .find(|m| m.view == view && m.kind == dim)
        .unwrap_or_else(|| panic!("kein Maß {dim:?} in {kind:?}"))
        .id
}

/// Waagerechte Strecke der Länge `len` auf der Höhe `y` in der Gruppe `group`?
fn has_horizontal(svg: &svg_read::ReadSvg, group: &str, y: f64, len: f64) -> bool {
    svg.paths_of(group).any(|p| {
        p.segments.iter().any(|seg| match seg {
            svg_read::Seg::Line { a, b } => {
                near(a.1, y) && near(b.1, y) && near((a.0 - b.0).abs(), len)
            }
            _ => false,
        })
    })
}

// (a) ------------------------------------------------------------------------------------------

#[test]
fn svg_and_dxf_contain_dimensions_and_centerlines() {
    // Quader mit Gesamtmaßen.
    let (s, id) = box_with_dims();
    let own = own_sheet(&s, id);
    let n = s.document.drawing(id).unwrap().dimensions.len();
    let dir = temp_dir("marken");
    let mut d = dialog(&s, SVG);
    assert_eq!(ready(&d, &s), (n, 0, CenterlineUse::Yes));
    let text = write(&mut d, &s, &dir.join("q.svg"));
    assert_eq!(text, svg_string(&own).unwrap(), "Datei = eigenes Blatt");
    let svg = svg_read::read(&text);
    let mut front = svg_texts(&svg, "Vorne Maße");
    front.sort();
    assert_eq!(front, vec!["100,0".to_string(), "40,0".to_string()]);
    assert!(
        svg_texts(&svg, "Oben Maße").contains(&"60,0".to_string()),
        "{:?}",
        svg_texts(&svg, "Oben Maße")
    );
    // Jedes Maß zeigt eine der drei Kantenlängen.
    for t in svg.texts.iter().filter(|t| t.group.ends_with(" Maße")) {
        assert!(
            ["100,0", "60,0", "40,0"].contains(&t.text.as_str()),
            "{}",
            t.text
        );
        assert!(t.upright, "{t:?}");
    }
    // Maßlinie der Breite 10 mm über der Oberkante (Ansicht 40 hoch um y 200 → y 230).
    assert!(has_horizontal(
        &svg,
        "Vorne Maße",
        FRONT.1 + 20.0 + 10.0,
        100.0
    ));
    let mut d = dialog(&s, DXF);
    let text = write(&mut d, &s, &dir.join("q.dxf"));
    assert_eq!(text, dxf_string(&own).unwrap());
    let dxf = dxf_read::read(&text).unwrap_or_else(|e| panic!("Aufbau: {e}"));
    let mut front: Vec<String> = dxf_texts(&dxf, "Vorne", "MASSE")
        .into_iter()
        .map(|t| t.0)
        .collect();
    front.sort();
    assert_eq!(front, vec!["100,0".to_string(), "40,0".to_string()]);
    assert!(layer_names(&dxf).contains(&"MASSE".to_string()));

    // Zylinder mit Mittellinien.
    let mut c = AppState::new();
    c.create_cylinder(5.0, 12.0).unwrap();
    c.selected_body = None;
    let cid = add_drawing(&mut c, "Zylinder");
    let own = own_sheet(&c, cid);
    let mut d = dialog(&c, SVG);
    assert_eq!(ready(&d, &c), (0, 0, CenterlineUse::Yes));
    let text = write(&mut d, &c, &dir.join("z.svg"));
    assert_eq!(text, svg_string(&own).unwrap());
    let svg = svg_read::read(&text);
    let center: Vec<&svg_read::ReadPath> = svg.paths_of("Vorne Mittellinien").collect();
    assert!(!center.is_empty(), "{:?}", svg.group_ids);
    assert!(center.iter().all(|p| p.dashed), "strichpunktiert");
    // Senkrechte Achse durch die Mitte der Vorderansicht, 12 mm plus je 3 mm Überstand.
    assert!(
        center
            .iter()
            .any(|p| p.segments.iter().any(|seg| matches!(seg,
            svg_read::Seg::Line { a, b } if near(a.0, FRONT.0) && near(b.0, FRONT.0)
                && near((a.1 - b.1).abs(), 18.0)
                && near(0.5 * (a.1 + b.1), FRONT.1)))),
        "{center:?}"
    );
    assert!(svg.paths_of("Oben Mittellinien").count() > 0);
    let mut d = dialog(&c, DXF);
    let text = write(&mut d, &c, &dir.join("z.dxf"));
    assert_eq!(text, dxf_string(&own).unwrap());
    let dxf = dxf_read::read(&text).unwrap_or_else(|e| panic!("Aufbau: {e}"));
    assert!(layer_names(&dxf).contains(&"MITTEL".to_string()));
    assert!(dxf_count(&dxf, "Vorne", "MITTEL") > 0);
    assert!(dxf_count(&dxf, "Oben", "MITTEL") > 0);
    assert!(!layer_names(&dxf).contains(&"MASSE".to_string()));
    assert_eq!(temp_files(&dir), 0);
    let _ = fs::remove_dir_all(&dir);
}

// (b) ------------------------------------------------------------------------------------------

#[test]
fn old_drawings_export_byte_for_byte_like_before() {
    let mut s = AppState::new();
    s.create_cylinder(5.0, 12.0).unwrap();
    s.selected_body = None;
    let id = add_drawing(&mut s, "Alt");
    s.document
        .update_drawing(id, |dr| {
            for v in &mut dr.views {
                v.centerlines = false;
            }
        })
        .unwrap();
    let drawing = s.document.drawing(id).unwrap();
    assert!(drawing.dimensions.is_empty());
    let lines = own_lines(&s, drawing);
    let map: BTreeMap<ViewId, &ViewLines> = lines.iter().map(|(i, l)| (*i, l)).collect();
    let old = build_sheet(drawing, &map);
    let dir = temp_dir("alt");
    let mut d = dialog(&s, SVG);
    assert_eq!(ready(&d, &s), (0, 0, CenterlineUse::No));
    let svg = write(&mut d, &s, &dir.join("a.svg"));
    assert_eq!(svg, svg_string(&old).unwrap());
    assert!(!svg.contains("Mittellinien") && !svg.contains("Maße"));
    let mut d = dialog(&s, DXF);
    let dxf = write(&mut d, &s, &dir.join("a.dxf"));
    assert_eq!(dxf, dxf_string(&old).unwrap());
    for layer in ["MITTEL", "MASSE", "MASSE_FEHLT", "CENTER"] {
        assert!(!dxf.contains(layer), "{layer}");
    }
    let text = draw(&mut d, &mut s);
    assert!(text.contains("Maße: 0"), "{text}");
    assert!(text.contains("Mittellinien: nein"), "{text}");
    let _ = fs::remove_dir_all(&dir);
}

// (c) ------------------------------------------------------------------------------------------

/// Verweisdaten der geraden Kante des Körpers `body`, deren beide Eckpunkte `pred` erfüllen.
fn edge_where(s: &AppState, body: usize, pred: impl Fn(DVec3) -> bool) -> GeoRefData {
    let solid = &s.bodies[body].solid;
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

/// Breite des Körpers `body` in der Vorderansicht (linke und rechte senkrechte Vorderkante).
fn width_spec(s: &AppState, body: usize, view: ViewId) -> DimSpec {
    let (lo, hi) = s.bodies[body].bounds;
    DimSpec {
        id: DimId(0),
        view,
        kind: DimKind::Horizontal,
        refs: vec![
            GeoRef::Edge(edge_where(s, body, |p| near(p.x, lo.x) && near(p.y, lo.y))),
            GeoRef::Edge(edge_where(s, body, |p| near(p.x, hi.x) && near(p.y, lo.y))),
        ],
        offset: 10.0,
        angle: 45.0,
        text_shift: (0.0, 0.0),
        tolerance: DimTolerance::None,
        decimals: 1,
        text_override: None,
        chain: None,
        suggested: false,
    }
}

#[test]
fn lost_dimension_blocks_writing_with_its_text() {
    let mut s = AppState::new();
    s.create_box(BOX).unwrap();
    let second = s.create_box(DVec3::new(30.0, 30.0, 30.0)).unwrap();
    s.selected_body = None;
    let id = add_drawing(&mut s, "Blatt 1");
    let front = s.document.drawing(id).unwrap().views[0].id;
    let spec = width_spec(&s, 1, front);
    let dim = s.document.add_dimension(id, spec).unwrap();
    // Vorher lässt sich schreiben.
    let mut d = dialog(&s, SVG);
    assert_eq!(ready(&d, &s).0, 1);
    s.delete_feature(second).unwrap();
    s.selected_body = None;
    // Der Grund kommt aus der eigenen Auflösung des Tests.
    let reason = match &own_dims(&s, id).by_view[&front][0].1 {
        Err(e) => e.to_string(),
        Ok(m) => panic!("verloren erwartet: {m:?}"),
    };
    assert!(reason.contains("nicht mehr vorhanden"), "{reason}");
    let reason = reason.trim_end_matches('.');
    let msg = format!(
        "Das Maß „Waagerecht“ in der Ansicht „Vorne“ hat seinen Bezug verloren: {reason}. Bitte ändern oder löschen."
    );
    let dir = temp_dir("verloren");
    for format in [SVG, DXF] {
        d.format = format;
        d.tick(&s, true);
        assert_eq!(
            d.status(&s),
            ExportStatus::Failed(msg.clone()),
            "{format:?}"
        );
        let path = dir.join(format!("z.{}", format.extension()));
        d.write_to(path.clone(), &s);
        d.tick(&s, true);
        assert_eq!(d.write_state(), WriteState::Failed(msg.clone()));
        assert!(!path.exists());
        let text = draw(&mut d, &mut s);
        assert!(text.contains(&msg), "{text}");
        assert!(
            text.contains(&format!("Nicht geschrieben: {msg}")),
            "{text}"
        );
    }
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 0, "nichts geschrieben");
    // Mit eigenem Text heißt das Maß wie sein Text (Platzhalter des Werts als „?“).
    s.document
        .update_dimension(id, dim, |m| m.text_override = Some("Breite <> mm".into()))
        .unwrap();
    let d = dialog(&s, SVG);
    assert_eq!(
        d.status(&s),
        ExportStatus::Failed(format!(
            "Das Maß „Breite ? mm“ in der Ansicht „Vorne“ hat seinen Bezug verloren: {reason}. Bitte ändern oder löschen."
        ))
    );
    // Gelöscht → wieder schreibbar.
    s.document.delete_dimension(id, dim).unwrap();
    let mut d = dialog(&s, SVG);
    assert_eq!(ready(&d, &s).0, 0);
    write(&mut d, &s, &dir.join("ok.svg"));
    let _ = fs::remove_dir_all(&dir);
}

// (d) ------------------------------------------------------------------------------------------

#[test]
fn outside_dimension_is_written_and_counted() {
    let (mut s, id) = box_with_dims();
    let width = dim_of(&s, id, ViewKind::Front, DimKind::Horizontal);
    s.document
        .update_dimension(id, width, |m| m.offset = 5000.0)
        .unwrap();
    let n = s.document.drawing(id).unwrap().dimensions.len();
    let dir = temp_dir("aussen");
    let mut d = dialog(&s, SVG);
    assert_eq!(ready(&d, &s), (n, 1, CenterlineUse::Yes));
    let ui = draw(&mut d, &mut s);
    assert!(ui.contains("Maße außerhalb des Blatts: 1"), "{ui}");
    assert!(
        ui.contains("1 Maß liegt außerhalb des Blatts und wird trotzdem geschrieben."),
        "{ui}"
    );
    let text = write(&mut d, &s, &dir.join("a.svg"));
    assert_eq!(text, svg_string(&own_sheet(&s, id)).unwrap());
    let svg = svg_read::read(&text);
    // Die Breite steht 5000 mm über der Oberkante, weit oberhalb des Blatts.
    let far: Vec<&svg_read::ReadText> = svg
        .texts_of("Vorne Maße")
        .filter(|t| t.text == "100,0")
        .collect();
    assert_eq!(far.len(), 1);
    assert!(far[0].pos.1 > A3_H + 4000.0, "{:?}", far[0]);
    assert!(has_horizontal(
        &svg,
        "Vorne Maße",
        FRONT.1 + 20.0 + 5000.0,
        100.0
    ));
    // DXF ebenso.
    let mut d = dialog(&s, DXF);
    assert_eq!(ready(&d, &s).1, 1);
    let text = write(&mut d, &s, &dir.join("a.dxf"));
    assert_eq!(text, dxf_string(&own_sheet(&s, id)).unwrap());
    // Zwei Maße außerhalb: Mehrzahl.
    let height = dim_of(&s, id, ViewKind::Front, DimKind::Vertical);
    s.document
        .update_dimension(id, height, |m| m.offset = -5000.0)
        .unwrap();
    let mut d = dialog(&s, SVG);
    assert_eq!(ready(&d, &s).1, 2);
    let ui = draw(&mut d, &mut s);
    assert!(ui.contains("Maße außerhalb des Blatts: 2"), "{ui}");
    assert!(
        ui.contains("2 Maße liegen außerhalb des Blatts und werden trotzdem geschrieben."),
        "{ui}"
    );
    let _ = fs::remove_dir_all(&dir);
}

// (e) ------------------------------------------------------------------------------------------

fn assert_full_width_equal_heights(d: &ExportDialog) {
    let rect = d.last_rect.unwrap();
    assert!(
        rect.width() >= 1200.0 - 2.0 * DIALOG_MARGIN - 1.0,
        "{rect:?}"
    );
    assert_eq!(d.last_cards.len(), 2);
    assert!(
        (d.last_cards[0].height() - d.last_cards[1].height()).abs() < 0.5,
        "{:?}",
        d.last_cards
    );
    assert!(
        (d.last_cards[0].width() - d.last_cards[1].width()).abs() < 1.0,
        "{:?}",
        d.last_cards
    );
}

#[test]
fn card_shows_counts_and_stays_full_width_equal_height() {
    let (mut s, id) = box_with_dims();
    let n = s.document.drawing(id).unwrap().dimensions.len();
    let mut d = dialog(&s, SVG);
    let text = draw(&mut d, &mut s);
    for word in [
        "Ansichten: 3",
        "Blatt: A3 quer",
        &format!("Maße: {n}"),
        "Mittellinien: ja",
    ] {
        assert!(text.contains(word), "„{word}“ fehlt:\n{text}");
    }
    assert!(!text.contains("außerhalb"), "{text}");
    assert_full_width_equal_heights(&d);
    // Eine Normansicht ohne Mittellinien → „teilweise“; alle aus → „nein“.
    s.document
        .update_drawing(id, |dr| dr.views[1].centerlines = false)
        .unwrap();
    let text = draw(&mut d, &mut s);
    assert!(text.contains("Mittellinien: teilweise"), "{text}");
    assert_eq!(ready(&d, &s).2, CenterlineUse::Partly);
    assert_full_width_equal_heights(&d);
    s.document
        .update_drawing(id, |dr| {
            for v in &mut dr.views {
                v.centerlines = false;
            }
        })
        .unwrap();
    let text = draw(&mut d, &mut s);
    assert!(text.contains("Mittellinien: nein"), "{text}");
    // Eine isometrische Ansicht ändert daran nichts (sie hat nie Mittellinien).
    s.document
        .update_drawing(id, |dr| {
            for v in &mut dr.views {
                v.centerlines = true;
            }
        })
        .unwrap();
    s.document
        .add_view(
            id,
            ViewKind::Isometric,
            one(1, 2),
            Placement::Free { x: 330.0, y: 90.0 },
            None,
        )
        .unwrap();
    let text = draw(&mut d, &mut s);
    assert!(text.contains("Mittellinien: ja"), "{text}");
    assert!(text.contains("Ansichten: 4"), "{text}");
    assert_full_width_equal_heights(&d);
    // Viele Maße: Tausendertrenner wie bei den Linien.
    assert_eq!(group_digits(1234), "1 234");
}

// (f) ------------------------------------------------------------------------------------------

#[test]
fn stale_dimension_edit_prepares_again() {
    let (s0, id) = box_with_dims();
    let mut s = s0;
    let dir = temp_dir("verschoben");
    let mut d = dialog(&s, SVG);
    let old = write(&mut d, &s, &dir.join("alt.svg"));
    assert!(has_horizontal(
        &svg_read::read(&old),
        "Vorne Maße",
        FRONT.1 + 30.0,
        100.0
    ));
    let width = dim_of(&s, id, ViewKind::Front, DimKind::Horizontal);
    s.document
        .update_dimension(id, width, |m| m.offset = 30.0)
        .unwrap();
    // Kein altes Ergebnis: weder angezeigt noch geschrieben.
    assert!(matches!(d.status(&s), ExportStatus::Computing(_)));
    let path = dir.join("neu.svg");
    d.write_to(path.clone(), &s);
    assert_eq!(d.write_state(), WriteState::Failed(STALE.into()));
    assert!(!path.exists());
    d.tick(&s, true);
    let new = write(&mut d, &s, &path);
    assert_ne!(new, old);
    assert_eq!(new, svg_string(&own_sheet(&s, id)).unwrap());
    let svg = svg_read::read(&new);
    assert!(has_horizontal(&svg, "Vorne Maße", FRONT.1 + 50.0, 100.0));
    assert!(!has_horizontal(&svg, "Vorne Maße", FRONT.1 + 30.0, 100.0));
    // Ebenso bei DXF und einem geänderten Text.
    let mut d = dialog(&s, DXF);
    s.document
        .update_dimension(id, width, |m| m.text_override = Some("<> breit".into()))
        .unwrap();
    d.write_to(dir.join("t.dxf"), &s);
    assert_eq!(d.write_state(), WriteState::Failed(STALE.into()));
    d.tick(&s, true);
    let text = write(&mut d, &s, &dir.join("t.dxf"));
    let dxf = dxf_read::read(&text).unwrap();
    assert!(
        dxf_texts(&dxf, "Vorne", "MASSE")
            .iter()
            .any(|t| t.0 == "100,0 breit"),
        "{:?}",
        dxf_texts(&dxf, "Vorne", "MASSE")
    );
    let _ = fs::remove_dir_all(&dir);
}

// (g) ------------------------------------------------------------------------------------------

#[test]
fn hostile_text_is_escaped_in_both_formats() {
    let (mut s, id) = box_with_dims();
    let width = dim_of(&s, id, ViewKind::Front, DimKind::Horizontal);
    let hostile = r#"a<&"'>b 50%%c \U+00D8 ^J"#;
    s.document
        .update_dimension(id, width, |m| m.text_override = Some(hostile.into()))
        .unwrap();
    let dir = temp_dir("feindlich");
    let mut d = dialog(&s, SVG);
    let text = write(&mut d, &s, &dir.join("f.svg"));
    assert_eq!(text, svg_string(&own_sheet(&s, id)).unwrap());
    assert!(text.contains("a&lt;&amp;&quot;&apos;&gt;b"), "maskiert");
    let svg = svg_read::read(&text);
    assert!(
        svg_texts(&svg, "Vorne Maße").contains(&hostile.to_string()),
        "{:?}",
        svg_texts(&svg, "Vorne Maße")
    );
    let mut d = dialog(&s, DXF);
    let text = write(&mut d, &s, &dir.join("f.dxf"));
    assert_eq!(text, dxf_string(&own_sheet(&s, id)).unwrap());
    assert!(text.is_ascii());
    let dxf = dxf_read::read(&text).unwrap_or_else(|e| panic!("Aufbau: {e}"));
    let texts = dxf_texts(&dxf, "Vorne", "MASSE");
    let (decoded, raw) = texts
        .iter()
        .find(|t| t.0.starts_with("a<&"))
        .unwrap_or_else(|| panic!("{texts:?}"));
    // Kein Kürzel aus dem eigenen Text: „%%c“ bleibt „%%c“, „\U+00D8“ bleibt die Zeichenfolge.
    assert_eq!(decoded, hostile);
    assert_eq!(raw, r#"a<&"'>b 50\U+0025\U+0025c \U+005CU+00D8 ^ J"#);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn too_long_dxf_text_is_named_before_writing() {
    let (mut s, id) = box_with_dims();
    let width = dim_of(&s, id, ViewKind::Front, DimKind::Horizontal);
    // 200 × „ä“ sind in der Datei 200 · 7 = 1400 Zeichen (höchstens 255).
    let long = "ä".repeat(200);
    s.document
        .update_dimension(id, width, |m| m.text_override = Some(long.clone()))
        .unwrap();
    let dir = temp_dir("lang");
    let mut d = dialog(&s, DXF);
    let shown: String = long.chars().take(40).collect();
    let msg = format!(
        "Ein Maßtext in der Ansicht „Vorne“ ist für DXF zu lang: „{shown}…“. Bitte den eigenen Text kürzen."
    );
    assert_eq!(d.status(&s), ExportStatus::Failed(msg.clone()));
    d.write_to(dir.join("l.dxf"), &s);
    d.tick(&s, true);
    assert_eq!(d.write_state(), WriteState::Failed(msg));
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
    // SVG hat keine solche Grenze.
    let mut d = dialog(&s, SVG);
    let text = write(&mut d, &s, &dir.join("l.svg"));
    assert!(text.contains(&long));
    let _ = fs::remove_dir_all(&dir);
}

// (h) ------------------------------------------------------------------------------------------

static COUNTED: AtomicUsize = AtomicUsize::new(0);
static COUNTED_SOLIDS: AtomicUsize = AtomicUsize::new(0);
static COUNTED_THREAD: Mutex<Option<std::thread::ThreadId>> = Mutex::new(None);

fn counting(d: &Drawing, solids: &[&Solid], p: &Progress) -> Result<ResolvedDims, DrawError> {
    COUNTED.fetch_add(1, Ordering::SeqCst);
    COUNTED_SOLIDS.store(solids.len(), Ordering::SeqCst);
    *COUNTED_THREAD.lock().unwrap() = Some(std::thread::current().id());
    resolve_dimensions_in_worker(d, solids, p)
}

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

#[test]
fn dimensions_resolved_in_the_worker_not_from_the_drawing_cache() {
    let mut s = AppState::new();
    let f = s.create_box(BOX).unwrap();
    s.import_mesh("Würfel", Arc::new(cube()), 1.0).unwrap();
    s.selected_body = None;
    let id = add_drawing(&mut s, "Blatt 1");
    suggest(&mut s, id);
    // Der Zwischenspeicher der Oberfläche hält einen Stand, der gleich veraltet.
    let ui = resolve_in(&mut s, id);
    assert!(!ui.by_view.is_empty());
    let before = s.dimension_ui.resolutions();
    s.document
        .update(f, |k| {
            if let FeatureKind::Box { size, .. } = k {
                *size = DVec3::new(80.0, 60.0, 40.0);
            }
        })
        .unwrap();
    s.refresh();
    s.selected_body = None;
    let mut d = ExportDialog::new(&s);
    d.set_resolve(counting);
    d.format = SVG;
    d.tick(&s, true);
    assert_eq!(COUNTED.load(Ordering::SeqCst), 1, "einmal im Arbeitsthread");
    assert_eq!(
        COUNTED_SOLIDS.load(Ordering::SeqCst),
        1,
        "nur der exakte Körper, nicht das Netz"
    );
    assert_ne!(
        *COUNTED_THREAD.lock().unwrap(),
        Some(std::thread::current().id()),
        "nicht im Thread der Oberfläche"
    );
    assert_eq!(
        s.dimension_ui.resolutions(),
        before,
        "Zwischenspeicher unberührt"
    );
    // Das Ergebnis passt zum neuen Modell (Breite 80), nicht zum Zwischenspeicher.
    let dir = temp_dir("arbeitsthread");
    let text = write(&mut d, &s, &dir.join("w.svg"));
    assert_eq!(text, svg_string(&own_sheet(&s, id)).unwrap());
    let svg = svg_read::read(&text);
    let front = svg_texts(&svg, "Vorne Maße");
    assert!(front.contains(&"80,0".to_string()), "{front:?}");
    assert!(!front.contains(&"100,0".to_string()), "{front:?}");
    // Noch einmal zeichnen rechnet nicht neu (gleicher Stand).
    draw(&mut d, &mut s);
    assert_eq!(COUNTED.load(Ordering::SeqCst), 1);
    let _ = fs::remove_dir_all(&dir);
}

// (i) ------------------------------------------------------------------------------------------

static SLOW_ENTERED: AtomicBool = AtomicBool::new(false);
static SLOW_LEFT: AtomicBool = AtomicBool::new(false);

fn slow_resolve(_: &Drawing, _: &[&Solid], p: &Progress) -> Result<ResolvedDims, DrawError> {
    SLOW_ENTERED.store(true, Ordering::SeqCst);
    let r = loop {
        if p.check().is_err() {
            break Err(DrawError::Cancelled);
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    SLOW_LEFT.store(true, Ordering::SeqCst);
    r
}

fn wait_for(flag: &AtomicBool) {
    let start = Instant::now();
    while !flag.load(Ordering::SeqCst) {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "Zeitüberschreitung"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn cancel_during_dimension_resolution_leaves_no_file() {
    let (s, _) = box_with_dims();
    let dir = temp_dir("abbrechen");
    let old = dir.join("alt.svg");
    fs::write(&old, "alt").unwrap();
    let mut d = ExportDialog::new(&s);
    d.set_resolve(slow_resolve);
    d.format = SVG;
    d.tick(&s, false);
    wait_for(&SLOW_ENTERED);
    assert!(matches!(d.status(&s), ExportStatus::Computing(_)));
    d.cancel();
    assert_eq!(d.status(&s), ExportStatus::Failed("Abgebrochen".into()));
    wait_for(&SLOW_LEFT);
    d.write_to(old.clone(), &s);
    d.tick(&s, true);
    assert!(matches!(d.write_state(), WriteState::Failed(_)));
    assert_eq!(fs::read_to_string(&old).unwrap(), "alt");
    assert_eq!(temp_files(&dir), 0);
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
    // Der Arbeitsthread selbst: Abbruch beim Auflösen ergibt „Abgebrochen“.
    let drawing = s.document.drawings().first().unwrap().clone();
    let solids = vec![("K".to_string(), s.bodies[0].solid.clone())];
    let p = Progress::new();
    let err = std::thread::scope(|scope| {
        let h =
            scope.spawn(|| prepare_drawing_with(&drawing, &solids, compute_view, cancel_now, &p));
        h.join().unwrap()
    })
    .unwrap_err();
    assert_eq!(err, "Abgebrochen");
    let _ = fs::remove_dir_all(&dir);
}

/// Bricht beim ersten Maß ab (wie ein Abbruch von außen während des Auflösens).
fn cancel_now(d: &Drawing, solids: &[&Solid], p: &Progress) -> Result<ResolvedDims, DrawError> {
    p.cancel();
    resolve_dimensions_in_worker(d, solids, p)
}

// (j) ------------------------------------------------------------------------------------------

#[test]
fn every_view_kind_exports_without_marks_on_isometric() {
    let mut s = AppState::new();
    s.create_cylinder(5.0, 12.0).unwrap();
    s.selected_body = None;
    let id = s
        .document
        .add_drawing("Alle", A3_QUER, Projection::First)
        .unwrap();
    for (i, kind) in ViewKind::ALL.iter().enumerate() {
        s.document
            .add_view(
                id,
                *kind,
                one(2, 1),
                Placement::Free {
                    x: 50.0 + 50.0 * i as f64,
                    y: 150.0,
                },
                None,
            )
            .unwrap();
    }
    // Auch eingeschaltet zeigt die Isometrie keine Mittellinien.
    s.document
        .update_drawing(id, |dr| {
            for v in &mut dr.views {
                v.centerlines = true;
            }
        })
        .unwrap();
    assert!(suggest(&mut s, id) > 0);
    let drawing = s.document.drawing(id).unwrap();
    let iso = drawing
        .views
        .iter()
        .find(|v| v.kind == ViewKind::Isometric)
        .unwrap()
        .id;
    assert!(drawing.dimensions.iter().all(|m| m.view != iso));
    let n = drawing.dimensions.len();
    let dir = temp_dir("alle");
    let mut d = dialog(&s, SVG);
    assert_eq!(ready(&d, &s), (n, 0, CenterlineUse::Yes));
    let text = write(&mut d, &s, &dir.join("a.svg"));
    assert_eq!(text, svg_string(&own_sheet(&s, id)).unwrap());
    let svg = svg_read::read(&text);
    for kind in ViewKind::ALL {
        let name = kind.label();
        let centers = svg.paths_of(&format!("{name} Mittellinien")).count();
        if kind.is_normal() {
            assert!(centers > 0, "{name}: {:?}", svg.group_ids);
        } else {
            assert_eq!(centers, 0, "{name}");
            assert!(
                !svg.group_ids
                    .iter()
                    .any(|g| g.starts_with(&format!("{name} "))),
                "{:?}",
                svg.group_ids
            );
            assert!(!svg.paths_of(name).collect::<Vec<_>>().is_empty());
        }
    }
    let mut d = dialog(&s, DXF);
    let text = write(&mut d, &s, &dir.join("a.dxf"));
    assert_eq!(text, dxf_string(&own_sheet(&s, id)).unwrap());
    let dxf = dxf_read::read(&text).unwrap_or_else(|e| panic!("Aufbau: {e}"));
    for kind in ViewKind::ALL {
        let name = kind.label();
        let marks: usize = ["MASSE", "MITTEL", "MASSE_FEHLT"]
            .iter()
            .map(|l| dxf_count(&dxf, name, l))
            .sum();
        if kind.is_normal() {
            assert!(dxf_count(&dxf, name, "MITTEL") > 0, "{name}");
        } else {
            assert_eq!(marks, 0, "{name}");
            assert!(dxf_count(&dxf, name, "SICHTBAR") > 0, "{name}");
        }
    }
    let _ = fs::remove_dir_all(&dir);
}
