//! STEP-Körper im Zustand der App (Teilprojekt STEP-Import, Aufgabe 13): ein Verlaufsschritt
//! „STEP importieren“, Namen „STEP-Körper n“, Auswahl und Kamera nach dem Import, Weiterverwenden
//! wie jeder exakte Körper, Verlaufsgröße ohne Formtext, Warnung bei grober Genauigkeit, Art,
//! Symbol und kein Bearbeiten-Fenster.

mod common;

use planora_app::drawing_state::ViewResult;
use planora_app::feature_dialog::FeatureDialog;
use planora_app::history::snapshot_bytes;
use planora_app::icons::{self, FeatureIcon, Icon, IconSource};
use planora_app::state::*;
use planora_app::undo_labels::{describe_change, fallback};
use planora_app::workspace::Workspace;
use planora_app::{fonts, properties, theme};
use planora_drawing::{Paper, Placement, Projection, Scale, Sheet, ViewKind};
use planora_exchange::Progress;
use planora_feature::{
    BlendArt, BodyRef, FeatureId, FeatureKind, FeatureStatus, SketchPlane, StandardPlane,
    StepShape, limit_step_text_sum_for_tests,
};
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::boolean::BoolOp;
use planora_kernel::{AnalyticKernel, Edge, Face, Id, Kernel, Solid};
use planora_math::{DMat4, DVec2, DVec3, PI};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_render::{FaceRef, Pick};
use planora_step::{ImportOptions, ReadLimits, StepFile, import_bodies, normalize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

// ---- Hilfen ---------------------------------------------------------------------------------

const COARSE_HINT: &str = "Rechnen mit diesem Körper kann scheitern.";

fn kernel() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: Solid, by: DVec3) -> Solid {
    kernel()
        .transform(&s, &DMat4::from_translation(by))
        .unwrap()
}

/// Kernel-Quader 10 × 20 × 30 bei `at`.
fn box_at(at: DVec3) -> Solid {
    moved(kernel().make_box(DVec3::new(10.0, 20.0, 30.0)).unwrap(), at)
}

/// Kernel-Kugel r 10 um `at`.
fn sphere_at(at: DVec3) -> Solid {
    moved(kernel().make_sphere(10.0).unwrap(), at)
}

/// Form wie aus dem Import: normalisierter Text samt gebautem Körper, Genauigkeit 1e-6.
fn shape(s: &Solid) -> Arc<StepShape> {
    let n = normalize(s, "Teil").unwrap();
    Arc::new(StepShape::with_solid(n.text, 1e-6, n.solid))
}

/// Text mit gültigem Kopf und Ende, aber ohne Körper.
fn broken() -> Arc<StepShape> {
    let text = normalize(&box_at(DVec3::ZERO), "Q").unwrap().text;
    let head = &text[..text.find("DATA;").unwrap()];
    Arc::new(StepShape::new(format!("{head}DATA;\nENDSEC;\nEND-ISO-10303-21;\n"), 1e-6).unwrap())
}

fn three() -> Vec<(String, Arc<StepShape>)> {
    vec![
        ("".into(), shape(&box_at(DVec3::ZERO))),
        ("Teil".into(), shape(&sphere_at(DVec3::new(50.0, 0.0, 0.0)))),
        ("".into(), shape(&box_at(DVec3::new(0.0, 40.0, 0.0)))),
    ]
}

fn names(s: &AppState) -> Vec<String> {
    s.document
        .features()
        .iter()
        .map(|f| f.name.clone())
        .collect()
}

/// Rollen aller Flächen je sichtbarem Körper, mit dem Namen des Körpers.
fn marks(s: &AppState) -> Vec<(String, Vec<String>)> {
    s.bodies
        .iter()
        .map(|b| {
            let mut roles: Vec<String> = b
                .solid
                .faces
                .iter()
                .map(|(id, _)| face_name(&b.solid, id))
                .collect();
            roles.sort();
            (b.name.clone(), roles)
        })
        .collect()
}

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id)
                .iter()
                .map(|u| face_name(s, u.face))
                .collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap_or_else(|| panic!("keine Kante zwischen {a} und {b}"))
}

/// Deckfläche (ebene Fläche mit Normale +z ganz oben) des Körpers `i`.
fn top_face(s: &AppState, i: usize) -> Id<Face> {
    let solid = &s.bodies[i].solid;
    let top_z = s.bodies[i].bounds.1.z;
    solid
        .faces
        .iter()
        .find(|(id, _)| {
            planora_sketch::plane_of_face(solid, *id)
                .is_some_and(|f| f.z.z > 0.5 && (f.origin.z - top_z).abs() < 1e-9)
        })
        .map(|(id, _)| id)
        .expect("Deckfläche")
}

/// Volumen über ein feines Netz (Sehne 0,002 mm).
fn fine_volume(s: &Solid) -> f64 {
    let mesh = tessellate(
        s,
        &TessellationOptions {
            chord_tolerance: 0.002,
        },
    )
    .unwrap();
    mass_properties(&mesh).volume
}

fn first(id: FeatureId) -> BodyRef {
    BodyRef {
        feature: id,
        index: 0,
    }
}

/// Zieht einen Zylinder r 2, Höhe 40 senkrecht durch die Mitte des Körpers `target` (Hülle
/// 10 × 20 × 30 ab dem Ursprung) ab. Liefert das Ergebnis von `add_boolean`.
fn subtract_cylinder(s: &mut AppState, target: BodyRef) -> Result<FeatureId, String> {
    let cyl = s.create_cylinder(2.0, 40.0).unwrap();
    let c = s.body_index(first(cyl)).unwrap();
    let base = s.bodies[c].bounds.0 + DVec3::new(2.0, 2.0, 0.0);
    s.move_body(c, DVec3::new(5.0, 10.0, -5.0) - base).unwrap();
    let c = s
        .bodies
        .iter()
        .position(|b| b.source != target)
        .expect("Zylinder");
    let t = s.body_index(target).expect("Ziel");
    s.add_boolean(t, c, BoolOp::Difference)
}

// ---- grober Quader (wie Aufgabe 7 (c)) -------------------------------------------------------

fn data(name: &str) -> String {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "planora-step",
        "tests",
        "data",
        name,
    ]
    .iter()
    .collect();
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn base(f: usize) -> usize {
    1000 + 100 * f
}

fn coords(line: &str) -> DVec3 {
    let inner = line
        .split_once("((")
        .or_else(|| line.split_once(",("))
        .expect("Klammer")
        .1;
    let inner = inner.split_once(')').expect("Klammer zu").0;
    let c: Vec<f64> = inner
        .split(',')
        .map(|x| x.trim().parse().expect("Zahl"))
        .collect();
    DVec3::new(c[0], c[1], c[2])
}

fn corners(text: &str, f: usize) -> [DVec3; 4] {
    std::array::from_fn(|k| {
        let head = format!("#{}=CARTESIAN_POINT(", base(f) + k);
        coords(
            text.lines()
                .find(|l| l.starts_with(&head))
                .expect("Eckpunkt"),
        )
    })
}

fn corner_index(p: DVec3) -> usize {
    usize::from(p.x > 5.0) + 2 * usize::from(p.y > 10.0) + 4 * usize::from(p.z > 15.0)
}

fn corner_dir(c: usize) -> DVec3 {
    let x = c as f64;
    DVec3::new(
        (1.7 * x + 0.3).sin(),
        (2.3 * x + 1.1).cos(),
        (0.7 * x + 2.9).sin(),
    )
    .normalize()
}

fn coarse_shift(f: usize, p: DVec3) -> DVec3 {
    const SCALE: [f64; 6] = [1.0, 0.8, 0.6, 0.9, 0.7, 0.95];
    let wobble = DVec3::new(
        (1.3 * f as f64).sin(),
        (0.9 * f as f64 + 0.4).cos(),
        (2.1 * f as f64).sin(),
    );
    let dir = (corner_dir(corner_index(p)) + 0.2 * wobble).normalize();
    9e-4 * SCALE[f] * dir
}

/// `split_vertices.step` mit Ecken je Fläche um bis zu 9e-4 mm versetzt, Genauigkeit 1e-3 mm.
fn coarse_box_text() -> String {
    let text = data("split_vertices.step");
    let mut new: HashMap<String, String> = HashMap::new();
    for f in 0..6 {
        let old = corners(&text, f);
        let c: [DVec3; 4] = std::array::from_fn(|k| old[k] + coarse_shift(f, old[k]));
        let b = base(f);
        for (k, p) in c.iter().enumerate() {
            new.insert(
                format!("#{}", b + k),
                format!(
                    "#{}=CARTESIAN_POINT('',({:.12},{:.12},{:.12}));",
                    b + k,
                    p.x,
                    p.y,
                    p.z
                ),
            );
        }
        for j in 0..4 {
            let d = c[(j + 1) % 4] - c[j];
            let u = d.normalize();
            new.insert(
                format!("#{}", b + 10 + j),
                format!(
                    "#{}=DIRECTION('',({:.15},{:.15},{:.15}));",
                    b + 10 + j,
                    u.x,
                    u.y,
                    u.z
                ),
            );
            new.insert(
                format!("#{}", b + 20 + j),
                format!(
                    "#{}=VECTOR('',#{},{:.12});",
                    b + 20 + j,
                    b + 10 + j,
                    d.length()
                ),
            );
        }
    }
    text.lines()
        .map(|l| {
            let id = l.split_once('=').map(|(id, _)| id.to_string());
            match id.and_then(|id| new.get(&id)) {
                Some(n) => n.clone(),
                None => l.replace("LENGTH_MEASURE(1.E-07)", "LENGTH_MEASURE(1.E-03)"),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Der grobe Quader über den echten Import (Lesen, Bauen, Normalisieren).
fn coarse_box() -> Arc<StepShape> {
    let file = StepFile::parse(
        coarse_box_text().into_bytes(),
        &ReadLimits::DEFAULT,
        &Progress::new(),
    )
    .unwrap();
    let import = import_bodies(&file, &ImportOptions::default(), &Progress::new()).unwrap();
    assert_eq!(import.bodies.len(), 1);
    let b = &import.bodies[0];
    assert_eq!(b.tolerance_mm, 1e-3);
    let n = b.outcome.clone().expect("grober Quader übernommen");
    let shape = StepShape::with_solid(n.text, b.tolerance_mm, n.solid);
    assert!(shape.is_coarse());
    Arc::new(shape)
}

// ---- Eigenschaftenfeld im Bild ---------------------------------------------------------------

/// Ein Text im Bild mit seiner Farbe.
struct Colored {
    text: String,
    color: egui::Color32,
}

fn draw_properties(s: &mut AppState, dark: bool) -> Vec<Colored> {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx.set_visuals(if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    });
    let mut seen = Vec::new();
    for _ in 0..2 {
        let out = ctx.run_ui(egui::RawInput::default(), |ui| properties::show(ui, s));
        seen.clear();
        fn walk(shape: &egui::epaint::Shape, out: &mut Vec<Colored>) {
            match shape {
                egui::epaint::Shape::Text(t) => out.push(Colored {
                    text: t.galley.text().to_string(),
                    color: t
                        .galley
                        .job
                        .sections
                        .first()
                        .map_or(t.fallback_color, |s| s.format.color),
                }),
                egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        for c in &out.shapes {
            walk(&c.shape, &mut seen);
        }
        out.drop_without_applying_deltas();
    }
    seen
}

fn all_text(seen: &[Colored]) -> String {
    seen.iter()
        .map(|c| c.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

// ---- (a) ein Schritt „STEP importieren“ ------------------------------------------------------

#[test]
fn import_is_one_step_named() {
    let root = common::temp_root("planora-step-state");
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    w.observe_history(false);
    let ids = w.active_tab_mut().state.import_step(three()).unwrap();
    assert_eq!(ids.len(), 3);
    w.observe_history(false);
    assert_eq!(w.active_tab().history().undo_len(), 1, "genau ein Schritt");
    assert_eq!(
        w.active_tab().undo_label(),
        Some("STEP importieren".to_string())
    );
    let after = marks(&w.active_tab().state);
    assert_eq!(after.len(), 3);
    assert_eq!(after[0].1.len(), 6);
    assert!(after[0].1.iter().all(|r| r.starts_with("STEP-Fläche ")));

    w.active_tab_mut().undo().unwrap();
    w.observe_history(false);
    assert!(w.active_tab().state.bodies.is_empty(), "alle drei zurück");
    assert!(w.active_tab().state.document.features().is_empty());
    assert_eq!(
        w.active_tab().redo_label(),
        Some("STEP importieren".to_string())
    );

    w.active_tab_mut().redo().unwrap();
    w.observe_history(false);
    assert_eq!(
        marks(&w.active_tab().state),
        after,
        "gleiche Namen und Marken"
    );
    assert_eq!(
        names(&w.active_tab().state),
        ["STEP-Körper 1", "Teil", "STEP-Körper 2"]
    );
    assert_eq!(w.active_tab().history().undo_len(), 1);
}

#[test]
fn step_name_only_for_pure_imports() {
    let mut s = AppState::new();
    let empty = s.snapshot();
    s.import_step(vec![("A".into(), shape(&box_at(DVec3::ZERO)))])
        .unwrap();
    assert_eq!(describe_change(&empty, &s.snapshot()), "STEP importieren");
    let one = s.snapshot();
    s.import_step(three()).unwrap();
    assert_eq!(describe_change(&one, &s.snapshot()), "STEP importieren");
    // Ein STEP-Körper zusammen mit einem anderen neuen Feature ist kein Import.
    let before = s.snapshot();
    s.create_box(DVec3::splat(3.0)).unwrap();
    s.import_step(vec![("B".into(), shape(&box_at(DVec3::ZERO)))])
        .unwrap();
    assert_eq!(describe_change(&before, &s.snapshot()), fallback());
}

#[test]
fn import_is_all_or_nothing() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(5.0)).unwrap();
    let before = s.snapshot();
    let edits = s.document.edit_count();
    let mut list = three();
    list.push(("Kaputt".into(), broken()));
    let e = s.import_step(list).unwrap_err();
    assert!(e.starts_with("„Kaputt“:"), "{e}");
    assert_eq!(s.message.as_deref(), Some(e.as_str()), "Meldung über fail");
    assert!(s.snapshot().same_content(&before), "Dokument unverändert");
    assert_eq!(s.document.edit_count(), edits);
    assert_eq!(s.bodies.len(), 1);
    // Die Nummern der leeren Namen sind nicht verbraucht.
    s.import_step(vec![("".into(), shape(&box_at(DVec3::ZERO)))])
        .unwrap();
    assert_eq!(names(&s)[1], "STEP-Körper 1");
    // Leere Liste: nichts geschieht.
    let edits = s.document.edit_count();
    let face = s.bodies[0].solid.faces.iter().next().unwrap().0;
    let pick = Pick::Face(FaceRef { body: 0, face });
    s.set_selection(Some(pick));
    s.selected_feature = Some(s.document.features()[0].id);
    assert_eq!(s.import_step(Vec::new()), Ok(Vec::new()));
    assert_eq!(s.document.edit_count(), edits);
    assert_eq!(s.selection, Some(pick), "Auswahl bleibt");
    assert_eq!(s.selected_body, Some(0));
    assert_eq!(s.selected_feature, Some(s.document.features()[0].id));
}

#[test]
fn the_text_sum_limit_is_surfaced() {
    let mut s = AppState::new();
    let one = shape(&box_at(DVec3::ZERO));
    let _limit = limit_step_text_sum_for_tests(one.text().len() + 10);
    s.import_step(vec![("A".into(), one)]).unwrap();
    let before = s.snapshot();
    let e = s
        .import_step(vec![("B".into(), shape(&box_at(DVec3::X)))])
        .unwrap_err();
    assert!(e.contains("zusammen zu groß"), "{e}");
    assert!(!e.contains("Ungültige Eingabe"), "{e}");
    assert_eq!(s.message.as_deref(), Some(e.as_str()));
    assert!(s.snapshot().same_content(&before));
}

// ---- (b) Namen, Auswahl, Kamera --------------------------------------------------------------

#[test]
fn names_selection_camera() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.move_body(0, DVec3::new(1000.0, 0.0, 0.0)).unwrap();
    s.selected_feature = s.document.features().first().map(|f| f.id);
    let ids = s.import_step(three()).unwrap();
    assert_eq!(
        names(&s)[2..],
        ["STEP-Körper 1", "Teil", "STEP-Körper 2"],
        "{:?}",
        names(&s)
    );
    let new: Vec<usize> = ids
        .iter()
        .map(|id| s.body_index(first(*id)).unwrap())
        .collect();
    assert_eq!(s.selected_body, Some(new[0]), "der erste ist gewählt");
    assert_eq!(s.selection, None);
    assert_eq!(s.selected_feature, None);
    let (lo, hi) = new.iter().map(|i| s.bodies[*i].bounds).fold(
        (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)),
        |(lo, hi), (a, b)| (lo.min(a), hi.max(b)),
    );
    let t = s.camera.target;
    let t = DVec3::new(t.x as f64, t.y as f64, t.z as f64);
    assert!(
        t.cmpge(lo - DVec3::splat(1e-3)).all() && t.cmple(hi + DVec3::splat(1e-3)).all(),
        "Kamera-Ziel {t} außerhalb {lo}–{hi}"
    );
    let mut fit = s.camera;
    fit.frame_bounds(
        glam::Vec3::new(lo.x as f32, lo.y as f32, lo.z as f32),
        glam::Vec3::new(hi.x as f32, hi.y as f32, hi.z as f32),
    );
    assert!((s.camera.distance - fit.distance).abs() < 1e-3 * fit.distance);
    // Der Zähler zählt weiter.
    s.import_step(vec![("  ".into(), shape(&box_at(DVec3::ZERO)))])
        .unwrap();
    assert_eq!(names(&s).last().unwrap(), "STEP-Körper 3");
    // Nach dem Laden setzt er fort.
    let mut back = AppState::from_snapshot(s.snapshot()).unwrap();
    back.import_step(vec![("".into(), shape(&box_at(DVec3::ZERO)))])
        .unwrap();
    assert_eq!(names(&back).last().unwrap(), "STEP-Körper 4");
}

#[test]
fn frame_bodies_frames_only_those() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.move_body(1, DVec3::new(500.0, 0.0, 0.0)).unwrap();
    s.frame_bodies(&[1]);
    let (lo, hi) = s.bodies[1].bounds;
    let mid = (lo + hi) * 0.5;
    assert!((s.camera.target.x as f64 - mid.x).abs() < 1e-3);
    let before = s.camera;
    s.frame_bodies(&[]);
    s.frame_bodies(&[7]);
    assert_eq!(
        s.camera.target, before.target,
        "ohne Körper bleibt die Kamera"
    );
    assert_eq!(s.camera.distance, before.distance);
}

// ---- (c) Weiterverwenden ---------------------------------------------------------------------

#[test]
fn downstream_operations() {
    let mut s = AppState::new();
    let id = s
        .import_step(vec![("Quader".into(), shape(&box_at(DVec3::ZERO)))])
        .unwrap()[0];
    // Abziehen mit Zylinder
    let diff = subtract_cylinder(&mut s, first(id)).expect("Abziehen");
    let i = s.body_index(first(diff)).unwrap();
    let v = fine_volume(&s.bodies[i].solid);
    let expected = 6000.0 - PI * 4.0 * 30.0;
    assert!((v - expected).abs() < 1e-3 * expected, "{v} / {expected}");
    // Verrunden einer Außenkante
    let edge = edge_between(&s.bodies[i].solid, "STEP-Fläche 1", "STEP-Fläche 2");
    let blend = s
        .add_blend(i, &[edge], BlendArt::Rundung { radius: 1.0 }, false)
        .expect("Verrunden");
    let i = s.body_index(first(blend)).unwrap();
    // Skizze auf der Deckfläche, Rechteck, Extrusion
    let sk = s.new_sketch_on_face(i, top_face(&s, i)).unwrap();
    let frame = *s.document.sketch_of(sk).unwrap().frame();
    let local = |p: DVec3| frame.to_local(p).truncate();
    let (a, b) = (
        local(DVec3::new(6.0, 14.0, 30.0)),
        local(DVec3::new(8.0, 17.0, 30.0)),
    );
    s.open_sketch(sk).unwrap();
    s.session_sketch_mut()
        .unwrap()
        .1
        .add_rectangle(a.min(b), a.max(b))
        .unwrap();
    s.close_sketch();
    s.extrude_sketch(sk, 0, 5.0).expect("Extrusion");
    assert!(
        s.document
            .features()
            .iter()
            .all(|f| f.status == FeatureStatus::Ok),
        "{:?}",
        s.document
            .features()
            .iter()
            .map(|f| (&f.name, &f.status))
            .collect::<Vec<_>>()
    );
    // Positionsfeld: ein Verschieben entsteht
    let mut t = AppState::new();
    t.import_step(vec![("Q".into(), shape(&box_at(DVec3::ZERO)))])
        .unwrap();
    t.move_body(0, DVec3::new(3.0, 0.0, 0.0)).unwrap();
    assert_eq!(t.document.features().len(), 2);
    assert!(matches!(
        t.document.features()[1].kind,
        FeatureKind::Move { .. }
    ));
    assert!((t.bodies[0].bounds.0.x - 3.0).abs() < 1e-9);
    assert_eq!(t.bodies[0].kind, BodyKind::Step, "verschoben bleibt STEP");
}

#[test]
fn drawing_section_and_parts_list_show_it() {
    let mut s = AppState::new();
    s.import_step(vec![("Quader".into(), shape(&box_at(DVec3::ZERO)))])
        .unwrap();
    let d = s
        .document
        .add_drawing(
            "Blatt 1",
            Sheet {
                paper: Paper::A3,
                landscape: true,
            },
            Projection::First,
        )
        .unwrap();
    let front = s
        .document
        .add_view(
            d,
            ViewKind::Front,
            Scale { num: 1, den: 1 },
            Placement::Free { x: 150.0, y: 230.0 },
            None,
        )
        .unwrap();
    let mut made = None;
    s.document
        .update_drawing(d, |dr| {
            made = Some(dr.add_section(front, ViewKind::Top, 15.0, 70.0));
        })
        .unwrap();
    let sec = made.unwrap().unwrap();
    s.drawing_ui.open(d);
    s.drawing_ui.tick(&s.bodies, &s.document, true);
    let model = s.document.model_edit_count();
    match s.drawing_ui.result(ViewKind::Front, model) {
        ViewResult::Ready(l) => assert!(!l.strokes.is_empty()),
        other => panic!("Vorderansicht: {other:?}"),
    }
    let spec = s.document.drawing(d).unwrap().view(sec).unwrap().clone();
    let data = s
        .drawing_ui
        .section_of(&spec, model)
        .expect("Schnitt berechnet");
    assert_eq!(data.regions.len(), 1);
    let rows = s.document.parts_rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "Quader");
}

// ---- (d) Verlaufsgröße -----------------------------------------------------------------------

#[test]
fn history_size_ignores_shapes() {
    let n = normalize(&box_at(DVec3::ZERO), "Q").unwrap();
    let (head, rest) = n.text.split_at(n.text.find("DATA;").unwrap());
    let text = format!("{head}/* {} */\n{rest}", "x".repeat(1024 * 1024));
    assert!(text.len() > 1024 * 1024);
    let big = Arc::new(StepShape::with_solid(text, 1e-6, n.solid));
    let mut s = AppState::new();
    let empty = snapshot_bytes(&s.snapshot());
    s.import_step(vec![("Groß".into(), big)]).unwrap();
    let with = snapshot_bytes(&s.snapshot());
    assert!(with > empty);
    assert!(with - empty < 4096, "{with} − {empty}");
}

// ---- (e) grobe Körper ------------------------------------------------------------------------

#[test]
fn coarse_body_never_silently_wrong() {
    let mut s = AppState::new();
    let id = s.import_step(vec![("Grob".into(), coarse_box())]).unwrap()[0];
    for dark in [true, false] {
        let seen = draw_properties(&mut s, dark);
        let text = all_text(&seen);
        for word in [
            "STEP-Körper",
            "Flächen",
            "6",
            "Genauigkeit der Datei",
            "0,001 mm",
        ] {
            assert!(text.contains(word), "„{word}“ fehlt:\n{text}");
        }
        let hint = seen
            .iter()
            .find(|c| c.text == COARSE_HINT)
            .unwrap_or_else(|| panic!("Warnung fehlt:\n{text}"));
        assert_eq!(hint.color, theme::palette(dark).warning, "Warnfarbe");
    }
    // Feature gewählt: dieselben Angaben
    s.selected_body = None;
    s.selected_feature = Some(id);
    let seen = draw_properties(&mut s, true);
    let text = all_text(&seen);
    assert!(text.contains("Genauigkeit der Datei"), "{text}");
    let hint = seen.iter().find(|c| c.text == COARSE_HINT).expect(&text);
    assert_eq!(hint.color, theme::palette(true).warning);
    let kind = s.document.feature(id).unwrap().kind.clone();
    let rows = properties::parameter_rows(&s, &kind);
    assert_eq!(
        rows,
        [
            ("Flächen", "6".to_string()),
            ("Genauigkeit der Datei", "0,001 mm".to_string())
        ]
    );

    // Abziehen: Ok mit dem berechneten Volumen oder eine klare Meldung, nie still falsch.
    let before = fine_volume(&s.bodies[0].solid);
    assert!((before - 6000.0).abs() < 1e-3 * 6000.0, "{before}");
    let expected = before - PI * 4.0 * 30.0;
    match subtract_cylinder(&mut s, first(id)) {
        Ok(diff) => {
            assert_eq!(s.document.feature(diff).unwrap().status, FeatureStatus::Ok);
            let i = s.body_index(first(diff)).unwrap();
            let v = fine_volume(&s.bodies[i].solid);
            assert!((v - expected).abs() < 1e-3 * expected, "{v} / {expected}");
        }
        Err(e) => {
            assert!(!e.trim().is_empty());
            assert_eq!(s.message.as_deref(), Some(e.as_str()));
        }
    }
}

#[test]
fn exact_body_has_no_warning() {
    let mut s = AppState::new();
    s.import_step(vec![("Genau".into(), shape(&box_at(DVec3::ZERO)))])
        .unwrap();
    let seen = draw_properties(&mut s, true);
    let text = all_text(&seen);
    assert!(text.contains("0,000001 mm"), "{text}");
    assert!(!text.contains(COARSE_HINT), "{text}");
}

// ---- (f) Art, Symbol, kein Bearbeiten-Fenster, Skizze wird beendet ---------------------------

#[test]
fn kind_icon_and_dialog() {
    let mut s = AppState::new();
    let sketch = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sketch).unwrap();
    s.session_sketch_mut()
        .unwrap()
        .1
        .add_rectangle(DVec2::ZERO, DVec2::new(4.0, 3.0))
        .unwrap();
    assert!(s.sketch_session.is_some());
    let id = s
        .import_step(vec![("S".into(), shape(&box_at(DVec3::ZERO)))])
        .unwrap()[0];
    assert!(s.sketch_session.is_none(), "Skizze beendet");
    assert_eq!(
        s.document.sketch_of(sketch).unwrap().curves().count(),
        4,
        "Entwurf übernommen"
    );
    let i = s.body_index(first(id)).unwrap();
    assert_eq!(s.bodies[i].kind, BodyKind::Step);
    let kind = s.document.feature(id).unwrap().kind.clone();
    assert_eq!(FeatureIcon::of(&kind), FeatureIcon::Body(BodyKind::Step));
    assert_eq!(
        icons::body_icon(BodyKind::Step),
        IconSource::Font(Icon::StepBody)
    );
    assert_eq!(icons::phosphor_name(Icon::StepBody), "file-arrow-down");
    assert_eq!(icons::glyph(Icon::StepBody), '\u{e232}');
    // Im Feature-Baum zeichnet das Körpersymbol das Zeichen aus der Symbolschrift.
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    let out = ctx.run_ui(egui::RawInput::default(), |ui| {
        let rect = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(16.0, 16.0));
        icons::paint_feature_icon(
            ui.painter(),
            rect,
            FeatureIcon::of(&kind),
            egui::Color32::WHITE,
        );
    });
    assert_eq!(
        planora_app::testing::collect_texts(&out),
        "\u{e232}\n",
        "Symbol im Baum"
    );
    out.drop_without_applying_deltas();
    assert!(
        FeatureDialog::new(&s, id).is_none(),
        "kein Bearbeiten-Fenster"
    );
    assert_eq!(kind.label(), "STEP-Körper");
}

// ---- Nachbesserung 1 -------------------------------------------------------------------------

/// Text, der im Bild genau so steht (eine eigene Beschriftung).
fn has_exact(seen: &[Colored], text: &str) -> bool {
    seen.iter().any(|c| c.text == text)
}

#[test]
fn body_panel_names_the_face_count_of_the_file() {
    let mut s = AppState::new();
    s.import_step(vec![("Q".into(), shape(&box_at(DVec3::ZERO)))])
        .unwrap();
    let edge = edge_between(&s.bodies[0].solid, "STEP-Fläche 1", "STEP-Fläche 2");
    s.add_blend(0, &[edge], BlendArt::Rundung { radius: 1.0 }, false)
        .unwrap();
    assert_eq!(s.bodies[0].kind, BodyKind::Step, "gerundet bleibt STEP");
    let seen = draw_properties(&mut s, true);
    let text = all_text(&seen);
    assert!(has_exact(&seen, "Flächen in der Datei"), "{text}");
    assert!(
        !has_exact(&seen, "Flächen"),
        "kein zweites „Flächen“:
{text}"
    );
    assert!(has_exact(&seen, "Flächen / Kanten"), "{text}");
    // Feature-Ansicht: die Form aus der Datei
    s.selected_body = None;
    s.selected_feature = Some(s.document.features()[0].id);
    let seen = draw_properties(&mut s, true);
    assert!(has_exact(&seen, "Flächen"), "{}", all_text(&seen));
}

#[test]
fn generated_names_never_repeat_existing_ones() {
    let mut s = AppState::new();
    // Name aus der Datei gleich einem erzeugten Namen, im selben Import
    let ids = s
        .import_step(vec![
            ("STEP-Körper 1".into(), shape(&box_at(DVec3::ZERO))),
            ("".into(), shape(&box_at(DVec3::new(100.0, 0.0, 0.0)))),
        ])
        .unwrap();
    assert_eq!(names(&s), ["STEP-Körper 1", "STEP-Körper 2"]);
    assert_eq!(ids.len(), 2);
    let rows = s.document.parts_rows();
    assert_eq!(rows.len(), 2, "zwei Zeilen in der Stückliste");
    assert!(rows.iter().all(|r| r.qty == 1));
    // Ein umbenannter Quader und ein Netz-Name im Dokument
    let mut t = AppState::new();
    let b = t.create_box(DVec3::splat(4.0)).unwrap();
    t.rename_feature(b, "STEP-Körper 3").unwrap();
    t.import_step(vec![("".into(), shape(&box_at(DVec3::ZERO)))])
        .unwrap();
    assert_eq!(names(&t), ["STEP-Körper 3", "STEP-Körper 4"]);
    // Der Helfer liefert dieselben Namen wie der Import (für die Lage-Meldung im Fenster).
    let given: Vec<String> = vec!["".into(), "Teil".into(), " ".into(), "STEP-Körper 9".into()];
    let planned = t.step_body_names(&given);
    assert_eq!(
        planned,
        ["STEP-Körper 10", "Teil", "STEP-Körper 11", "STEP-Körper 9"]
    );
    let list: Vec<(String, Arc<StepShape>)> = given
        .iter()
        .enumerate()
        .map(|(i, n)| {
            (
                n.clone(),
                shape(&box_at(DVec3::new(50.0 * i as f64, 0.0, 0.0))),
            )
        })
        .collect();
    t.import_step(list).unwrap();
    assert_eq!(names(&t)[2..], planned[..]);
    // Der Zähler zählt nur hoch: Nach dem Umbenennen kommt die Nummer nicht wieder.
    let mut u = AppState::new();
    let id = u
        .import_step(vec![("".into(), shape(&box_at(DVec3::ZERO)))])
        .unwrap()[0];
    u.rename_feature(id, "Gehäuse").unwrap();
    u.import_step(vec![("".into(), shape(&box_at(DVec3::ZERO)))])
        .unwrap();
    assert_eq!(names(&u), ["Gehäuse", "STEP-Körper 2"]);
    let all = names(&t);
    let mut unique = all.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), all.len(), "{all:?}");
}

#[test]
fn bodies_follow_the_input_order() {
    let mut s = AppState::new();
    let at = [0.0, 100.0, 200.0];
    let list: Vec<(String, Arc<StepShape>)> = ["A", "B", "C"]
        .iter()
        .zip(at)
        .map(|(n, x)| (n.to_string(), shape(&box_at(DVec3::new(x, 0.0, 0.0)))))
        .collect();
    let ids = s.import_step(list).unwrap();
    assert_eq!(names(&s), ["A", "B", "C"], "Baum in Dateireihenfolge");
    let order: Vec<FeatureId> = s.document.features().iter().map(|f| f.id).collect();
    assert_eq!(order, ids);
    for (id, x) in ids.iter().zip(at) {
        let i = s.body_index(first(*id)).unwrap();
        assert!((s.bodies[i].bounds.0.x - x).abs() < 1e-9, "{id:?}");
    }
    let sel = s.selected_body.unwrap();
    assert_eq!(s.bodies[sel].name, "A");
    assert!(s.bodies[sel].bounds.0.x.abs() < 1e-9);
}

#[test]
fn import_clears_picks_and_hover() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    let face = s.bodies[0].solid.faces.iter().next().unwrap().0;
    let pick = Pick::Face(FaceRef { body: 0, face });
    s.set_selection(Some(pick));
    s.set_hover(Some(pick));
    s.import_step(vec![(
        "Q".into(),
        shape(&box_at(DVec3::new(50.0, 0.0, 0.0))),
    )])
    .unwrap();
    assert_eq!(s.selection, None);
    assert_eq!(s.hover, None);
}

#[test]
fn accuracy_shows_four_digits() {
    let n = normalize(&box_at(DVec3::ZERO), "Q").unwrap();
    for (t, want) in [
        (2.54e-5, "0,0000254 mm"),
        (1.2346e-4, "0,0001235 mm"),
        (1e-3, "0,001 mm"),
        (1e-6, "0,000001 mm"),
    ] {
        let mut s = AppState::new();
        let sh = Arc::new(StepShape::with_solid(n.text.clone(), t, n.solid.clone()));
        let id = s.import_step(vec![("Q".into(), sh)]).unwrap()[0];
        let kind = s.document.feature(id).unwrap().kind.clone();
        let rows = properties::parameter_rows(&s, &kind);
        assert_eq!(rows[1], ("Genauigkeit der Datei", want.to_string()), "{t}");
    }
}

#[test]
fn undo_leaves_no_selection_and_import_makes_dirty() {
    let root = common::temp_root("planora-step-state-dirty");
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    w.active_tab_mut()
        .state
        .create_box(DVec3::splat(10.0))
        .unwrap();
    w.observe_history(false);
    w.active_tab_mut().history_mut().mark_saved();
    assert!(!w.active_tab().is_dirty());
    w.active_tab_mut().state.import_step(three()).unwrap();
    w.observe_history(false);
    assert!(w.active_tab().is_dirty(), "nach dem Import ungespeichert");
    assert!(w.active_tab().state.selected_body.is_some());
    w.active_tab_mut().undo().unwrap();
    w.observe_history(false);
    assert_eq!(w.active_tab().state.bodies.len(), 1, "der Quader bleibt");
    assert_eq!(w.active_tab().state.selected_body, None, "keine Auswahl");
}

#[test]
fn the_step_label_has_one_source() {
    let mut s = AppState::new();
    let id = s
        .import_step(vec![("".into(), shape(&box_at(DVec3::ZERO)))])
        .unwrap()[0];
    let label = s.document.feature(id).unwrap().kind.label();
    assert_eq!(label, planora_step::UNNAMED_BODY);
    assert_eq!(names(&s), [format!("{label} 1")]);
    let seen = draw_properties(&mut s, true);
    assert!(has_exact(&seen, label), "Art im Eigenschaftenfeld");
}

/// Schlussprüfung M1: Nennt die Datei keine Genauigkeit, zeigt die Eigenschaft das als
/// „unbekannt“ mit dem angenommenen Wert, nicht als Angabe der Datei; die Warnung steht.
#[test]
fn unknown_accuracy_is_shown_as_unknown() {
    let n = normalize(&box_at(DVec3::ZERO), "Q").unwrap();
    let sh = Arc::new(StepShape::with_solid(n.text, 1e-3, n.solid).accuracy_unknown());
    let mut s = AppState::new();
    let id = s.import_step(vec![("Q".into(), sh)]).unwrap()[0];
    let kind = s.document.feature(id).unwrap().kind.clone();
    let rows = properties::parameter_rows(&s, &kind);
    assert_eq!(
        rows[1],
        (
            "Genauigkeit der Datei",
            "unbekannt (angenommen 0,001 mm)".to_string()
        )
    );
    s.selected_body = Some(0);
    let seen = draw_properties(&mut s, true);
    assert!(
        seen.iter().any(|c| c.text == COARSE_HINT),
        "Warnung fehlt:\n{}",
        all_text(&seen)
    );
}
