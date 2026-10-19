//! Maße und Positionsnummern in Schnittansichten (Teilprojekt 3d, Aufgabe 10; Spec §9.3).
//!
//! Prüfkörper ist ein Quader 40 × 20 × 10 (x, y, z) mit einer durchgehenden Bohrung r 5 um die
//! Achse (20, 13, z), in A3 quer. Die Draufsicht „Oben“ liegt frei, der Schnitt „A–A“ (Art
//! Vorne, Ebene y = 10) hängt an ihr. Von Hand: Die Ebene trifft die Bohrung 3 mm neben der
//! Achse, halbe Sehne √(5² − 3²) = 4. Die Schnittfläche besteht aus x 0…16 und x 24…40
//! (z 0…10); die beiden Kanten an der Bohrung (x = 16 und x = 24) liegen 8 mm auseinander. Keine
//! Ecke liegt auf der Ebene (die Naht der Bohrung liegt bei y = 13): keine Verschiebung.

#[path = "../../planora-drawing/tests/common/svg_read.rs"]
mod svg_read;

use egui::{Pos2, Rect, vec2};
use planora_app::balloon_tool;
use planora_app::dimension_state::{SuggestOutcome, notes_in, resolve_in, suggest_for};
use planora_app::dimension_tool::{DimensionTool, Hit, LinearDir, ToolMode, ToolOutcome};
use planora_app::drawing_state::ComputeFn;
use planora_app::drawing_view;
use planora_app::export_dialog::*;
use planora_app::state::*;
use planora_app::testing::collect_texts;
use planora_app::{fonts, theme};
use planora_drawing::dimension::{DimId, GeoRef, GeoRefData};
use planora_drawing::out2d::Out2d;
use planora_drawing::section::SECTION_ORIGIN_MIN;
use planora_drawing::sheet::build_sheet;
use planora_drawing::stroke::StrokeOrigin;
use planora_drawing::{
    DrawBody, DrawError, DrawingFormat, DrawingId, Paper, Placement, Projection, Scale, Sheet,
    ViewId, ViewKind, ViewLines, compute_view,
};
use planora_exchange::Progress;
use planora_feature::{FeatureId, FeatureKind};
use planora_kernel::boolean::BoolOp;
use planora_kernel::{Curve, Solid};
use planora_math::{DVec2, DVec3};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const A3_QUER: Sheet = Sheet {
    paper: Paper::A3,
    landscape: true,
};
const NAME: &str = "Schnitt A\u{2013}A";
const EPS: f64 = 1e-9;
const LOST_ONE: &str = "1 Maß hat seinen Bezug verloren.";

fn one() -> Scale {
    Scale { num: 1, den: 1 }
}

// --- Aufbau ---------------------------------------------------------------------------------

fn add(s: &mut AppState, name: &str, kind: FeatureKind) -> FeatureId {
    s.document.add_feature(name, kind).unwrap()
}

/// Der Quader mit Bohrung; liefert die Boolesche Operation.
fn block_state() -> (AppState, FeatureId) {
    let mut s = AppState::new();
    add(
        &mut s,
        "Block",
        FeatureKind::Box {
            size: DVec3::new(40.0, 20.0, 10.0),
            origin: DVec3::ZERO,
        },
    );
    add(
        &mut s,
        "Bohrung",
        FeatureKind::Cylinder {
            radius: 5.0,
            height: 20.0,
            origin: DVec3::new(20.0, 13.0, -5.0),
        },
    );
    s.refresh();
    let cut = s.add_boolean(0, 1, BoolOp::Difference).unwrap();
    s.refresh();
    assert_eq!(s.bodies.len(), 1);
    (s, cut)
}

fn add_section(s: &mut AppState, d: DrawingId, parent: ViewId) -> ViewId {
    let mut made = None;
    s.document
        .update_drawing(d, |dr| {
            made = Some(dr.add_section(parent, ViewKind::Front, 10.0, 60.0));
        })
        .unwrap();
    made.unwrap().unwrap()
}

/// Zeichnung A3 quer mit Oben (frei) und Schnitt A–A (Vorne, y = 10) darunter; geöffnet.
fn block_drawing(s: &mut AppState) -> (DrawingId, ViewId, ViewId) {
    let d = s
        .document
        .add_drawing("Blatt 1", A3_QUER, Projection::First)
        .unwrap();
    let top = s
        .document
        .add_view(
            d,
            ViewKind::Top,
            one(),
            Placement::Free { x: 150.0, y: 200.0 },
            None,
        )
        .unwrap();
    let sec = add_section(s, d, top);
    s.drawing_ui.open(d);
    (d, top, sec)
}

/// Rechnet alle Ansichten der geöffneten Zeichnung fertig.
fn compute(s: &mut AppState) {
    s.drawing_ui.tick(&s.bodies, &s.document, true);
}

fn model(s: &AppState) -> u64 {
    s.document.model_edit_count()
}

fn view_spec(s: &AppState, d: DrawingId, v: ViewId) -> planora_drawing::ViewSpec {
    s.document.drawing(d).unwrap().view(v).unwrap().clone()
}

/// Die Linien der Ansicht `v` auf dem Blatt (wie im Zeichnungsbereich).
fn sheet_lines(s: &AppState, d: DrawingId, v: ViewId) -> Vec<planora_drawing::sheet::SheetLine> {
    let dr = s.document.drawing(d).unwrap();
    let owned = s.drawing_ui.results_for(dr, model(s));
    let refs: BTreeMap<ViewId, &ViewLines> = owned.iter().map(|(k, l)| (*k, l.as_ref())).collect();
    build_sheet(dr, &refs)
        .views
        .into_iter()
        .find(|x| x.id == v)
        .expect("Ansicht auf dem Blatt")
        .lines
}

fn mid(l: &planora_drawing::sheet::SheetLine) -> DVec2 {
    match l.curve {
        Out2d::Line { a, b } => (a + b) * 0.5,
        ref c => panic!("Gerade erwartet: {c:?}"),
    }
}

/// Die Blattlinie der Schnittansicht, deren Kante im geschnittenen Körper die Gerade bei
/// x = `x`, y = 10 von z 0 bis 10 ist (Kante an der Bohrung).
fn bore_line(s: &AppState, d: DrawingId, sec: ViewId, x: f64) -> planora_drawing::sheet::SheetLine {
    let data = s
        .drawing_ui
        .view_solids(&view_spec(s, d, sec), model(s))
        .expect("Schnitt fertig");
    let is_bore = |o: StrokeOrigin| {
        let StrokeOrigin::Edge { body, edge } = o else {
            return false;
        };
        let solid: &Solid = &data.solids[body];
        let Some(e) = solid.edges.get(planora_kernel::Id::from_index(edge)) else {
            return false;
        };
        let (p, q) = (solid.vertices[e.start].point, solid.vertices[e.end].point);
        let on = |p: DVec3| (p.x - x).abs() < 1e-6 && (p.y - 10.0).abs() < 1e-6;
        matches!(e.curve, Curve::Line { .. }) && on(p) && on(q) && (p.z - q.z).abs() > 9.0
    };
    sheet_lines(s, d, sec)
        .into_iter()
        .find(|l| is_bore(l.origin.clone()))
        .unwrap_or_else(|| panic!("keine Bohrungskante bei x = {x}"))
}

fn hit(l: &planora_drawing::sheet::SheetLine) -> Hit {
    Hit {
        origin: l.origin.clone(),
        at: mid(l),
        px_per_mm: 4.0,
    }
}

fn has_section_mark(r: &GeoRef) -> bool {
    let data = match r {
        GeoRef::Edge(d) | GeoRef::EdgeEnd { edge: d, .. } | GeoRef::Center(d) | GeoRef::Face(d) => {
            d
        }
    };
    match data {
        GeoRefData::Face(t) => t.origin >= SECTION_ORIGIN_MIN,
        GeoRefData::Edge { a, b, .. } => {
            a.origin >= SECTION_ORIGIN_MIN || b.origin >= SECTION_ORIGIN_MIN
        }
    }
}

/// Setzt mit dem Werkzeug „Maß“ (waagerecht) das Maß zwischen den beiden Bohrungskanten der
/// Schnittansicht; prüft, dass der erste Bezug die Schnittmarke trägt.
fn bore_dimension(s: &mut AppState, d: DrawingId, sec: ViewId) -> DimId {
    let left = bore_line(s, d, sec, 16.0);
    let right = bore_line(s, d, sec, 24.0);
    let mut tool = DimensionTool::default();
    tool.set_mode(ToolMode::Linear(LinearDir::Horizontal));
    let out = tool.click(s, d, sec, &hit(&left));
    assert!(matches!(out, ToolOutcome::NeedMore(_)), "{out:?}");
    assert_eq!(tool.pending().len(), 1);
    assert!(
        matches!(tool.pending()[0], GeoRef::Edge(_)),
        "{:?}",
        tool.pending()[0]
    );
    assert!(
        has_section_mark(&tool.pending()[0]),
        "Bezug ohne Schnittmarke: {:?}",
        tool.pending()[0]
    );
    let out = tool.click(s, d, sec, &hit(&right));
    assert_eq!(out, ToolOutcome::Ready);
    let pointer = (mid(&left) + mid(&right)) * 0.5 + DVec2::new(0.0, 25.0);
    let ids = tool.finish(s, d, pointer).unwrap();
    assert_eq!(ids.len(), 1);
    ids[0]
}

fn value(s: &mut AppState, d: DrawingId, view: ViewId, id: DimId) -> Option<f64> {
    let dims = resolve_in(s, d);
    dims.by_view
        .get(&view)?
        .iter()
        .find(|(spec, _)| spec.id == id)
        .and_then(|(_, r)| r.as_ref().ok().map(|m| m.value))
}

// --- (b) ------------------------------------------------------------------------------------

#[test]
fn tool_in_section_view() {
    let (mut st, _) = block_state();
    let (d, top, sec) = block_drawing(&mut st);
    compute(&mut st);
    let id = bore_dimension(&mut st, d, sec);
    let spec = st
        .document
        .drawing(d)
        .unwrap()
        .dimension(id)
        .unwrap()
        .clone();
    assert_eq!(spec.view, sec);
    assert!(spec.refs.iter().all(has_section_mark));
    let x = value(&mut st, d, sec, id).expect("aufgelöst");
    assert!((x - 8.0).abs() <= EPS, "{x} statt 8");
    // Die Bezüge gibt es nur im Schnitt: in der Draufsicht wäre das Maß verloren.
    let dims = resolve_in(&mut st, d);
    assert!(!dims.by_view.contains_key(&top));
    // Eine Kante der Draufsicht hat keine Schnittmarke.
    let any_top = sheet_lines(&st, d, top)
        .into_iter()
        .find(|l| {
            matches!(l.origin, StrokeOrigin::Edge { .. }) && matches!(l.curve, Out2d::Line { .. })
        })
        .unwrap();
    let mut tool = DimensionTool::default();
    tool.set_mode(ToolMode::Linear(LinearDir::Horizontal));
    tool.click(&st, d, top, &hit(&any_top));
    assert_eq!(tool.pending().len(), 1);
    assert!(!has_section_mark(&tool.pending()[0]));
}

// --- (c) ------------------------------------------------------------------------------------

/// Rohr (Außenradius 20, Innenradius 15, Höhe 40, Achse z) mit Oben (frei), Vorne (frei) und
/// Schnitt A–A (Vorne, Ebene y = 3). Von Hand: Vorne schlägt Höhe 40 und Ø 40 vor; der Schnitt
/// zeigt die Schnittflächen bei |x| ≥ √(15² − 3²) bis |x| ≤ √(20² − 3²) und schlägt die Breite
/// zwischen ihren Außenkanten vor: 2 · √391 = 39,547… (kein Schlüssel aus Vorne).
#[test]
fn suggest_in_section_view() {
    let mut st = AppState::new();
    add(
        &mut st,
        "Außen",
        FeatureKind::Cylinder {
            radius: 20.0,
            height: 40.0,
            origin: DVec3::ZERO,
        },
    );
    add(
        &mut st,
        "Bohrung",
        FeatureKind::Cylinder {
            radius: 15.0,
            height: 60.0,
            origin: DVec3::new(0.0, 0.0, -10.0),
        },
    );
    st.refresh();
    st.add_boolean(0, 1, BoolOp::Difference).unwrap();
    st.refresh();
    let d = st
        .document
        .add_drawing("Blatt 1", A3_QUER, Projection::First)
        .unwrap();
    let top = st
        .document
        .add_view(
            d,
            ViewKind::Top,
            one(),
            Placement::Free { x: 120.0, y: 200.0 },
            None,
        )
        .unwrap();
    let front = st
        .document
        .add_view(
            d,
            ViewKind::Front,
            one(),
            Placement::Free { x: 300.0, y: 200.0 },
            None,
        )
        .unwrap();
    let mut made = None;
    st.document
        .update_drawing(d, |dr| {
            made = Some(dr.add_section(top, ViewKind::Front, 3.0, 60.0));
        })
        .unwrap();
    let sec = made.unwrap().unwrap();
    st.drawing_ui.open(d);
    compute(&mut st);
    assert!(
        !st.drawing_ui
            .view_solids(&view_spec(&st, d, sec), model(&st))
            .unwrap()
            .nudged
    );
    assert_eq!(
        suggest_for(&mut st, d, Some(front)),
        SuggestOutcome::Added(2)
    );
    let front_dims: Vec<DimId> = st
        .document
        .drawing(d)
        .unwrap()
        .dimensions
        .iter()
        .map(|m| m.id)
        .collect();
    let front_keys: Vec<String> = front_dims
        .iter()
        .map(|id| st.dimension_ui.last_key(d, *id).unwrap().to_string())
        .collect();

    assert_eq!(suggest_for(&mut st, d, Some(sec)), SuggestOutcome::Added(1));
    let dr = st.document.drawing(d).unwrap().clone();
    assert_eq!(dr.dimensions.len(), front_dims.len() + 1);
    let new: Vec<_> = dr
        .dimensions
        .iter()
        .filter(|m| !front_dims.contains(&m.id))
        .collect();
    assert!(
        new.iter().all(|m| m.view == sec),
        "nur in der Schnittansicht"
    );
    assert!(
        new.iter().all(|m| m.refs.iter().all(has_section_mark)),
        "Bezüge an der Schnittfläche"
    );
    // Keine Kopie der Maße von Vorne: kein neuer Schlüssel gleicht einem aus Vorne.
    for m in &new {
        let k = st
            .dimension_ui
            .last_key(d, m.id)
            .expect("Schlüssel")
            .to_string();
        assert!(!front_keys.contains(&k), "{k} kopiert");
    }
    // Alle lösen sich gegen die geschnittenen Körper auf.
    let dims = resolve_in(&mut st, d);
    let list = &dims.by_view[&sec];
    assert_eq!(list.len(), 1);
    let x = list[0].1.as_ref().unwrap().value;
    let want = 2.0 * 391f64.sqrt();
    assert!((x - want).abs() <= EPS, "{x} statt {want}");
    assert!(dims.by_view[&front].iter().all(|(_, r)| r.is_ok()));
    // Noch einmal: nichts Neues. Alle Ansichten zusammen: die Draufsicht bekommt ihre Maße, der
    // Schnitt keines mehr.
    assert_eq!(
        suggest_for(&mut st, d, Some(sec)),
        SuggestOutcome::NothingNew
    );
    assert!(matches!(
        suggest_for(&mut st, d, None),
        SuggestOutcome::Added(_)
    ));
    let dr = st.document.drawing(d).unwrap();
    assert_eq!(dr.dimensions.iter().filter(|m| m.view == sec).count(), 1);
    assert_eq!(dr.dimensions.iter().filter(|m| m.view == front).count(), 2);
    assert!(dr.dimensions.iter().any(|m| m.view == top));
    let dims = resolve_in(&mut st, d);
    assert!(dims.by_view.values().flatten().all(|(_, r)| r.is_ok()));
}

// --- (d) ------------------------------------------------------------------------------------

#[test]
fn balloon_in_section_view() {
    let mut st = AppState::new();
    // Körper 0 ganz vor der Ebene y = 10 (y 0…5), Körper 1 geschnitten.
    let klotz = add(
        &mut st,
        "Klotz",
        FeatureKind::Box {
            size: DVec3::new(10.0, 5.0, 10.0),
            origin: DVec3::new(60.0, 0.0, 0.0),
        },
    );
    let block = add(
        &mut st,
        "Block",
        FeatureKind::Box {
            size: DVec3::new(40.0, 20.0, 10.0),
            origin: DVec3::ZERO,
        },
    );
    st.refresh();
    assert_eq!(st.bodies.len(), 2);
    let (d, top, sec) = block_drawing(&mut st);
    compute(&mut st);
    let data = st
        .drawing_ui
        .view_solids(&view_spec(&st, d, sec), model(&st))
        .unwrap();
    assert_eq!(data.source, vec![1], "Körper 0 fällt weg");
    let key = |f: FeatureId| {
        st.document.body_key(planora_feature::BodyRef {
            feature: f,
            index: 0,
        })
    };
    let (k0, k1) = (key(klotz), key(block));
    let line = sheet_lines(&st, d, sec)
        .into_iter()
        .find(|l| matches!(l.curve, Out2d::Line { .. }))
        .unwrap();
    let body = match line.origin {
        StrokeOrigin::Edge { body, .. } | StrokeOrigin::Silhouette { body, .. } => body,
    };
    assert_eq!(body, 0, "Index unter den geschnittenen Körpern");
    let b = balloon_tool::place(&mut st, d, sec, body, mid(&line)).unwrap();
    let balloon = st.document.drawing(d).unwrap().balloon(b).unwrap().clone();
    assert_eq!(balloon.body, k1, "Kreis gehört zum ursprünglichen Körper 1");
    assert_eq!(balloon.view, sec);
    let numbers = st.document.balloon_numbers(d);
    assert_eq!(numbers[&b], st.document.row_of(k1));
    assert_ne!(st.document.row_of(k0), st.document.row_of(k1));
    // In der Draufsicht zählt der Index unter allen exakten Körpern.
    let b0 = balloon_tool::place(&mut st, d, top, 0, DVec2::new(150.0, 200.0)).unwrap();
    assert_eq!(
        st.document.drawing(d).unwrap().balloon(b0).unwrap().body,
        k0
    );
    // Ein Index außerhalb der geschnittenen Körper wird abgelehnt.
    assert_eq!(
        balloon_tool::place(&mut st, d, sec, 1, mid(&line)),
        Err(balloon_tool::MISSED.to_string())
    );
}

// --- (e) ------------------------------------------------------------------------------------

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "planora-section-dims-{}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn svg_dialog(s: &AppState) -> ExportDialog {
    let mut d = ExportDialog::new(s);
    d.with_frame = false;
    d.format = ExportFormat::Drawing(DrawingFormat::Svg);
    d.tick(s, true);
    d
}

#[test]
fn export_resolves_section_dims() {
    let (mut st, cut) = block_state();
    let (d, _, sec) = block_drawing(&mut st);
    compute(&mut st);
    bore_dimension(&mut st, d, sec);
    let dir = temp_dir("export");
    let path = dir.join("Blatt.svg");
    let mut dlg = svg_dialog(&st);
    assert!(
        matches!(dlg.status(&st), ExportStatus::DrawingReady { .. }),
        "{:?}",
        dlg.status(&st)
    );
    dlg.write_to(path.clone(), &st);
    dlg.tick(&st, true);
    assert!(
        matches!(dlg.write_state(), WriteState::Done(..)),
        "{:?}",
        dlg.write_state()
    );
    let svg = svg_read::read(&fs::read_to_string(&path).unwrap());
    let texts: Vec<String> = svg
        .texts_of(&format!("{NAME} Maße"))
        .map(|t| t.text.clone())
        .collect();
    assert_eq!(texts, vec!["8,0".to_string()]);
    fs::remove_file(&path).unwrap();

    // Bohrung weg (die Boolesche Operation gelöscht): das Maß ist verloren, die Ausgabe gesperrt.
    st.delete_feature(cut).unwrap();
    compute(&mut st);
    let mut dlg = svg_dialog(&st);
    let ExportStatus::Failed(msg) = dlg.status(&st) else {
        panic!("{:?}", dlg.status(&st));
    };
    assert!(
        msg.starts_with(&format!(
            "Das Maß „Waagerecht“ in der Ansicht „{NAME}“ hat seinen Bezug verloren:"
        )),
        "{msg}"
    );
    assert!(msg.ends_with(LOST_ADVICE), "{msg}");
    dlg.write_to(path.clone(), &st);
    dlg.tick(&st, true);
    assert!(matches!(dlg.write_state(), WriteState::Failed(_)));
    assert!(!path.exists(), "keine Datei");
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
    let _ = fs::remove_dir_all(&dir);
}

/// Die Auflösung im Arbeitsthread bekommt die geschnittenen Körper der Schnittansicht.
#[test]
fn worker_resolution_uses_cut_solids() {
    let (mut st, _) = block_state();
    let (d, _, sec) = block_drawing(&mut st);
    compute(&mut st);
    let id = bore_dimension(&mut st, d, sec);
    let dr = st.document.drawing(d).unwrap().clone();
    let solids: Vec<(String, Solid)> = st
        .bodies
        .iter()
        .map(|b| (b.name.clone(), b.solid.clone()))
        .collect();
    let prepared = prepare_drawing(&dr, &solids, compute_view, &Progress::new()).expect("Blatt");
    assert_eq!(prepared.dimensions, 1);
    // Ohne Schnittkörper fehlt das Maß in der Auflösung (nie gegen die Originalkörper).
    let plain: Vec<&Solid> = solids.iter().map(|(_, s)| s).collect();
    let none =
        resolve_dimensions_in_worker(&dr, &plain, &ViewSolids::new(), &Progress::new()).unwrap();
    assert!(!none.by_view.contains_key(&sec));
    let data = st
        .drawing_ui
        .view_solids(&view_spec(&st, d, sec), model(&st))
        .unwrap();
    let cut: ViewSolids = BTreeMap::from([(sec, data.solids.iter().collect())]);
    let with = resolve_dimensions_in_worker(&dr, &plain, &cut, &Progress::new()).unwrap();
    let (spec, r) = &with.by_view[&sec][0];
    assert_eq!(spec.id, id);
    assert!((r.as_ref().unwrap().value - 8.0).abs() <= EPS);
}

// --- (f) ------------------------------------------------------------------------------------

/// Hält die Rechnung der Art „Vorne“ (also nur die Schnittansicht) an, bis `GATE` offen ist.
static GATE: AtomicBool = AtomicBool::new(false);

fn gated(bodies: &[DrawBody], kind: ViewKind, p: &Progress) -> Result<ViewLines, DrawError> {
    if kind == ViewKind::Front {
        while !GATE.load(Ordering::SeqCst) {
            p.check()?;
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    compute_view(bodies, kind, p)
}

struct Screen {
    ctx: egui::Context,
    time: f64,
}

impl Screen {
    fn new() -> Screen {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, false);
        Screen { ctx, time: 0.0 }
    }

    fn frame(&mut self, s: &mut AppState) -> String {
        self.time += 0.05;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0))),
            time: Some(self.time),
            ..Default::default()
        };
        let out = self.ctx.run_ui(raw, |ui| drawing_view::show(ui, s));
        let text = collect_texts(&out);
        out.drop_without_applying_deltas();
        text
    }
}

#[test]
fn pending_section_no_lost_flash() {
    let (mut st, _) = block_state();
    let (d, _, sec) = block_drawing(&mut st);
    compute(&mut st);
    let id = bore_dimension(&mut st, d, sec);
    let mut screen = Screen::new();
    let text = screen.frame(&mut st);
    assert!(!text.contains(LOST_ONE), "{text}");

    // Die Ebene wird verschoben: der Schnitt rechnet neu und hält an.
    GATE.store(false, Ordering::SeqCst);
    st.drawing_ui.set_compute(gated as ComputeFn);
    st.document
        .update_drawing(d, |dr| {
            let v = dr.views.iter_mut().find(|v| v.id == sec).unwrap();
            v.section.as_mut().unwrap().offset = 11.0;
        })
        .unwrap();
    for _ in 0..5 {
        let text = screen.frame(&mut st);
        assert!(
            !text.contains(LOST_ONE),
            "kein verlorenes Maß, solange es rechnet: {text}"
        );
        let dims = resolve_in(&mut st, d);
        assert!(!dims.by_view.contains_key(&sec), "noch nicht aufgelöst");
        assert_eq!(notes_in(&mut st, d, &dims).lost, 0);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        st.drawing_ui
            .view_solids(&view_spec(&st, d, sec), model(&st))
            .is_none()
    );

    // Fertig: bei y = 11 trifft die Ebene die Bohrung 2 mm neben der Achse, halbe Sehne √21.
    GATE.store(true, Ordering::SeqCst);
    let mut text = String::new();
    for _ in 0..1500 {
        text = screen.frame(&mut st);
        if st
            .drawing_ui
            .view_solids(&view_spec(&st, d, sec), model(&st))
            .is_some()
        {
            text = screen.frame(&mut st);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    st.drawing_ui.set_compute(compute_view);
    let want = 2.0 * 21f64.sqrt();
    let x = value(&mut st, d, sec, id).expect("nach dem Schnitt aufgelöst");
    assert!((x - want).abs() <= 1e-9, "{x} statt {want}");
    assert!(!text.contains(LOST_ONE), "{text}");
}

/// Die Ebene wird verschoben und der Schnitt gleich fertig gerechnet (ohne Bild dazwischen):
/// das Maß wird an den neuen Schnittkörpern gemessen, nicht aus der alten Auflösung übernommen.
#[test]
fn moved_plane_measures_again() {
    let (mut st, _) = block_state();
    let (d, _, sec) = block_drawing(&mut st);
    compute(&mut st);
    let id = bore_dimension(&mut st, d, sec);
    assert!((value(&mut st, d, sec, id).unwrap() - 8.0).abs() <= EPS);
    st.document
        .update_drawing(d, |dr| {
            let v = dr.views.iter_mut().find(|v| v.id == sec).unwrap();
            v.section.as_mut().unwrap().offset = 11.0;
        })
        .unwrap();
    compute(&mut st);
    let want = 2.0 * 21f64.sqrt();
    let x = value(&mut st, d, sec, id).unwrap();
    assert!((x - want).abs() <= EPS, "{x} statt {want}");
}
