//! Aufwand von Vorschlag und Werkzeug „Maß“ an Platten mit vielen Bohrungen (Nachbesserung
//! Teilprojekt 12).
//!
//! Der Prüfpunkt einer großen Fläche mit vielen Bohrungen kostet Zehntelsekunden. Früher wurde
//! er für jeden Bezug neu berechnet (je Kante beide Nachbarflächen), der Vorschlag stand bei
//! 100 Bohrungen viele Sekunden. Geprüft wird über die Zahl der Prüfpunkt-Berechnungen
//! (`face_probe_calls`, Zähler je Thread) statt über die Laufzeit; dazu ein großzügiger
//! Zeitrahmen als Plausibilitätsprüfung.

use planora_app::dimension_state::{SuggestOutcome, suggest_for};
use planora_app::dimension_tool::{Hit, LinearDir, ToolMode, ToolOutcome};
use planora_app::state::AppState;
use planora_drawing::dimension::DimKind;
use planora_drawing::stroke::StrokeOrigin;
use planora_drawing::{DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind};
use planora_feature::{FeatureKind, SketchFeature, SketchPlane, StandardPlane, face_probe_calls};
use planora_kernel::Surface;
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::model::Sketch;
use std::time::{Duration, Instant};

const A3_QUER: Sheet = Sheet {
    paper: Paper::A3,
    landscape: true,
};

/// Platte 210 × 110 × 5 mit `rows × cols` Bohrungen r = 2 im Raster 10 mm (eine Extrusion).
fn plate(rows: usize, cols: usize) -> AppState {
    let mut st = AppState::new();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let p: Vec<_> = [(0.0, 0.0), (210.0, 0.0), (210.0, 110.0), (0.0, 110.0)]
        .iter()
        .map(|&(x, y)| sk.add_point(x, y).unwrap())
        .collect();
    for i in 0..4 {
        sk.add_line(p[i], p[(i + 1) % 4]).unwrap();
    }
    for r in 0..rows {
        for c in 0..cols {
            let pt = sk
                .add_point(10.0 + 10.0 * c as f64, 10.0 + 10.0 * r as f64)
                .unwrap();
            sk.add_circle(pt, 2.0).unwrap();
        }
    }
    let s = st
        .add_feature(
            Some("Skizze"),
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: vec![],
            }),
        )
        .unwrap();
    st.add_feature(
        None,
        FeatureKind::Extrude {
            sketch: s,
            profile: 0,
            height: 5.0,
        },
    )
    .unwrap();
    assert!(st.message.is_none(), "{:?}", st.message);
    assert_eq!(st.bodies.len(), 1);
    st
}

fn faces(st: &AppState) -> u64 {
    st.bodies[0].solid.faces.iter().count() as u64
}

/// Zeichnung mit einer Ansicht `kind` in (210, 150), geöffnet und berechnet.
fn view(st: &mut AppState, kind: ViewKind) -> (DrawingId, ViewId) {
    let d = st
        .document
        .add_drawing("Blatt 1", A3_QUER, Projection::First)
        .unwrap();
    let v = st
        .document
        .add_view(
            d,
            kind,
            Scale { num: 1, den: 1 },
            Placement::Free { x: 210.0, y: 150.0 },
            None,
        )
        .unwrap();
    st.drawing_ui.open(d);
    st.drawing_ui.tick(&st.bodies, &st.document, true);
    (d, v)
}

#[test]
fn suggest_probes_each_face_at_most_once() {
    let mut st = plate(4, 5);
    let (d, _) = view(&mut st, ViewKind::Top);
    let before = face_probe_calls();
    let out = suggest_for(&mut st, d, None);
    let used = face_probe_calls() - before;
    assert!(
        matches!(out, SuggestOutcome::Added(n) if n > 20),
        "{}",
        out.message()
    );
    // 20 Bohrungen: je ein Ø-Maß an der Kreiskante (Nachbarn Deckfläche und Bohrungswand).
    // Ohne Speicher wäre die Deckfläche für jedes davon neu berechnet worden.
    let dims = &st.document.drawing(d).unwrap().dimensions;
    assert!(
        dims.iter().filter(|m| m.kind == DimKind::Diameter).count() >= 20,
        "{dims:?}"
    );
    assert!(
        used <= faces(&st),
        "{used} Prüfpunkte für {} Flächen",
        faces(&st)
    );
}

#[test]
fn suggest_on_a_plate_with_100_holes_stays_quick() {
    let mut st = plate(10, 10);
    let (d, _) = view(&mut st, ViewKind::Top);
    let before = face_probe_calls();
    let t = Instant::now();
    let out = suggest_for(&mut st, d, None);
    let took = t.elapsed();
    let used = face_probe_calls() - before;
    assert!(
        matches!(out, SuggestOutcome::Added(n) if n >= 100),
        "{}",
        out.message()
    );
    assert!(used <= faces(&st), "{used} Prüfpunkte");
    // Großzügig (ungeprüfte Testübersetzung, langsame Rechner); ohne Speicher dauerte es hier
    // ein Vielfaches.
    assert!(
        took < Duration::from_secs(60),
        "Vorschlag brauchte {took:?}"
    );
}

#[test]
fn the_tool_probes_each_face_once_per_click() {
    let mut st = plate(1, 3);
    // Vorderansicht: die Bohrungswände erscheinen als (verdeckte) Umrisse.
    let (d, v) = view(&mut st, ViewKind::Front);
    let solid = &st.bodies[0].solid;
    let walls: Vec<usize> = solid
        .faces
        .iter()
        .filter(|(_, f)| matches!(f.surface, Surface::Cylinder { .. }))
        .map(|(i, _)| i.index())
        .collect();
    let mut tool = std::mem::take(&mut st.dimension_tool);
    tool.set_mode(ToolMode::Linear(LinearDir::Horizontal));
    // Zwei Bohrungswände (Flächenbezüge): je Klick wird jede beteiligte Fläche einmal
    // berechnet, auch wenn sie mehrmals aufgelöst wird (Bezug bilden, prüfen, messen).
    let hit = |face: usize| Hit {
        origin: StrokeOrigin::Silhouette { body: 0, face },
        at: DVec2::new(0.0, 0.0),
        px_per_mm: 4.0,
    };
    let before = face_probe_calls();
    let first = tool.click(&st, d, v, &hit(walls[0]));
    assert!(matches!(first, ToolOutcome::NeedMore(_)), "{first:?}");
    assert_eq!(face_probe_calls() - before, 1, "eine Fläche, ein Prüfpunkt");
    let before = face_probe_calls();
    let second = tool.click(&st, d, v, &hit(walls[1]));
    assert_eq!(second, ToolOutcome::Ready);
    assert_eq!(
        face_probe_calls() - before,
        2,
        "zwei Flächen (die erste wird für die Messung neu aufgelöst)"
    );
    st.dimension_tool = tool;
}
