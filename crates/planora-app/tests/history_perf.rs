//! Messung des Zurückladens für Rückgängig und Wiederholen (Aufgabe 2, Haltepunkt).
//!
//! Zweck: Rückgängig lädt ein Zustandsbild zurück (`Document::from_snapshot` und danach
//! `AppState::refresh`). Das soll synchron im Bild geschehen. Dieser Test baut ein mittleres und
//! ein schweres Modell und misst je Modell (Median aus 5 Läufen) `snapshot()`, `snapshot_bytes`,
//! `Document::from_snapshot`, `AppState::refresh` danach (mit und ohne Netzprüfungs-Speicher)
//! und die reine Netzprüfung (`ExchangeMesh::validate`).
//!
//! Grenzen der Entscheidung: Summe `from_snapshot + refresh` des schweren Modells <= 1000 ms und
//! `snapshot()` <= 50 ms, sonst Planergänzung (Fortschrittsanzeige/Hintergrund, Spec §6a.6).
//!
//! Ergebnis (Release, Ryzen 9 9950X, Median in ms, schweres Modell): `snapshot()` unter 1,
//! `from_snapshot` 250, `refresh` 39, Summe 289; mittleres Modell Summe 235; Netzprüfung 0,2.
//! Im Debug-Build liegt das schwere Modell bei 1209 ms. Entscheidung: synchron wie geplant.
//! Tabelle: Abschnitt „Messergebnis Aufgabe 2“ im Plan.

use planora_app::section_tool;
use planora_app::state::{AppState, BodyKind};
use planora_drawing::dimension::{DimId, DimKind, DimSpec, GeoRef, Tolerance as DimTol};
use planora_drawing::{DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind};
use planora_exchange::ExchangeMesh;
use planora_feature::{
    BlendArt, BodyRef, Document, FeatureId, FeatureKind, FeatureStatus, SketchFeature, SketchPlane,
    StandardPlane, geo_ref_for_edge,
};
use planora_kernel::{BoolOp, Edge, Id, SurfaceKind};
use planora_math::{DVec3, Frame, Tolerance};
use planora_sketch::model::Sketch;
use std::sync::Arc;
use std::time::Instant;

const A3_QUER: Sheet = Sheet {
    paper: Paper::A3,
    landscape: true,
};

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn ok(st: &AppState) {
    assert!(st.message.is_none(), "{:?}", st.message);
}

/// Index des sichtbaren Körpers von Feature `f`.
fn body_of(st: &AppState, f: FeatureId) -> usize {
    st.body_index(BodyRef {
        feature: f,
        index: 0,
    })
    .expect("Körper sichtbar")
}

/// Senkrechte Kante des Körpers `body` an der Ecke (`x`, `y`).
fn vertical_edge_at(st: &AppState, body: usize, x: f64, y: f64) -> Id<Edge> {
    let solid = &st.bodies[body].solid;
    solid
        .edges
        .iter()
        .find(|(_, e)| {
            let a = solid.vertices.get(e.start).unwrap().point;
            let b = solid.vertices.get(e.end).unwrap().point;
            e.start != e.end && near(a.x, x) && near(a.y, y) && near(b.x, x) && near(b.y, y)
        })
        .map(|(id, _)| id)
        .expect("Kante nicht gefunden")
}

/// Kante des Körpers `body`, deren Endpunkte beide `pred` erfüllen, als Maßbezug.
fn edge_ref(st: &AppState, body: usize, pred: impl Fn(DVec3) -> bool) -> GeoRef {
    let solid = &st.bodies[body].solid;
    let (id, _) = solid
        .edges
        .iter()
        .find(|(_, e)| {
            let a = solid.vertices.get(e.start).unwrap().point;
            let b = solid.vertices.get(e.end).unwrap().point;
            e.start != e.end && pred(a) && pred(b)
        })
        .expect("Kante nicht gefunden");
    GeoRef::Edge(geo_ref_for_edge(solid, id, &Tolerance::default()).unwrap())
}

fn add_boolean(st: &mut AppState, a: FeatureId, b: FeatureId, op: BoolOp) -> FeatureId {
    st.add_feature(
        None,
        FeatureKind::Boolean {
            target: BodyRef {
                feature: a,
                index: 0,
            },
            tool: BodyRef {
                feature: b,
                index: 0,
            },
            op,
        },
    )
    .unwrap()
}

fn sketch(st: &mut AppState, sk: Sketch) -> FeatureId {
    st.add_feature(
        Some("Skizze"),
        FeatureKind::Sketch(SketchFeature {
            sketch: sk,
            plane: SketchPlane::Standard(StandardPlane::XY),
            copies: vec![],
            formulas: Default::default(),
        }),
    )
    .unwrap()
}

/// Platte 210 × 110 × 5 mit `rows × cols` Bohrungen r = 2 (Raster 10 mm), eine Extrusion.
fn plate(st: &mut AppState, rows: usize, cols: usize) -> FeatureId {
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
    let s = sketch(st, sk);
    st.add_feature(
        None,
        FeatureKind::Extrude {
            sketch: s,
            profile: 0,
            height: 5.0,
        },
    )
    .unwrap()
}

/// Rechteck mit Spline-Kuppe (wie `nurbs_app.rs`), extrudiert zur Freiformfläche.
fn spline_body(st: &mut AppState) -> FeatureId {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let p: Vec<_> = [(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (5.0, 7.0), (0.0, 5.0)]
        .iter()
        .map(|&(x, y)| sk.add_point(x, y).unwrap())
        .collect();
    sk.add_line(p[0], p[1]).unwrap();
    sk.add_line(p[1], p[2]).unwrap();
    sk.add_spline(&p[2..5]).unwrap();
    sk.add_line(p[4], p[0]).unwrap();
    let s = sketch(st, sk);
    st.add_feature(
        None,
        FeatureKind::Extrude {
            sketch: s,
            profile: 0,
            height: 2.0,
        },
    )
    .unwrap()
}

/// Kugelnetz mit `2 * segments * (rings - 1)` Dreiecken, nach außen gewendet.
fn sphere_mesh(segments: usize, rings: usize, radius: f32) -> ExchangeMesh {
    let mut positions = vec![[0.0, 0.0, radius]];
    for r in 1..rings {
        let theta = std::f32::consts::PI * r as f32 / rings as f32;
        for s in 0..segments {
            let phi = std::f32::consts::TAU * s as f32 / segments as f32;
            positions.push([
                radius * theta.sin() * phi.cos(),
                radius * theta.sin() * phi.sin(),
                radius * theta.cos(),
            ]);
        }
    }
    positions.push([0.0, 0.0, -radius]);
    let south = (positions.len() - 1) as u32;
    let ring = |r: usize, s: usize| (1 + (r - 1) * segments + s % segments) as u32;
    let mut indices = Vec::new();
    for s in 0..segments {
        indices.extend([0, ring(1, s), ring(1, s + 1)]);
    }
    for r in 1..rings - 1 {
        for s in 0..segments {
            let (a, b) = (ring(r, s), ring(r, s + 1));
            let (c, d) = (ring(r + 1, s), ring(r + 1, s + 1));
            indices.extend([a, c, b, b, c, d]);
        }
    }
    for s in 0..segments {
        indices.extend([south, ring(rings - 1, s + 1), ring(rings - 1, s)]);
    }
    ExchangeMesh { positions, indices }
}

/// 20 Grundkörper, 10 Boolesche Operationen, 5 Verrundungen, 1 Skizze mit Extrusion (37
/// Features) und eine Zeichnung mit 4 Ansichten.
fn medium_model() -> AppState {
    let mut st = AppState::new();
    let cube = DVec3::splat(20.0);
    for k in 0..10usize {
        let ox = 300.0 + 60.0 * (k % 5) as f64;
        let oy = 100.0 + 60.0 * (k / 5) as f64;
        let target = st
            .add_feature(
                None,
                FeatureKind::Box {
                    size: cube,
                    origin: DVec3::new(ox, oy, 0.0),
                },
            )
            .unwrap();
        let (tool, op) = match k {
            0..=4 => (
                FeatureKind::Box {
                    size: cube,
                    origin: DVec3::new(ox + 10.0, oy + 10.0, 0.0),
                },
                BoolOp::Union,
            ),
            5..=7 => (
                FeatureKind::Cylinder {
                    radius: 5.0,
                    height: 30.0,
                    origin: DVec3::new(ox + 10.0, oy + 10.0, -5.0),
                },
                BoolOp::Difference,
            ),
            _ => (
                FeatureKind::Sphere {
                    radius: 12.0,
                    origin: DVec3::new(ox + 10.0, oy + 10.0, 10.0),
                },
                BoolOp::Intersection,
            ),
        };
        let tool = st.add_feature(None, tool).unwrap();
        let result = add_boolean(&mut st, target, tool, op);
        if k < 5 {
            let body = body_of(&st, result);
            let edge = vertical_edge_at(&st, body, ox, oy);
            st.add_blend(body, &[edge], BlendArt::Rundung { radius: 1.0 }, true)
                .unwrap();
        }
    }
    plate(&mut st, 1, 1);
    ok(&st);
    drawing(&mut st);
    st
}

fn drawing(st: &mut AppState) -> (DrawingId, ViewId) {
    let d = st
        .document
        .add_drawing("Blatt 1", A3_QUER, Projection::First)
        .unwrap();
    let mut front = None;
    for (kind, x, y) in [
        (ViewKind::Front, 100.0, 200.0),
        (ViewKind::Top, 100.0, 80.0),
        (ViewKind::Right, 250.0, 200.0),
        (ViewKind::Isometric, 300.0, 80.0),
    ] {
        let v = st
            .document
            .add_view(
                d,
                kind,
                Scale { num: 1, den: 10 },
                Placement::Free { x, y },
                None,
            )
            .unwrap();
        front.get_or_insert(v);
    }
    (d, front.unwrap())
}

/// Mittleres Modell plus Platte 10 × 10 Bohrungen, Spline-Extrusion, Netz-Körper mit
/// 200 000 Dreiecken, 2 Schnitte und 30 Maße.
fn heavy_model() -> AppState {
    let mut st = medium_model();
    let plate = plate(&mut st, 10, 10);
    spline_body(&mut st);
    st.import_mesh("Kugel", Arc::new(sphere_mesh(400, 251, 40.0)), 1.0)
        .unwrap();
    let d = st.document.drawings()[0].id;
    let front = st.document.drawing(d).unwrap().views[0].id;
    for y in [5.0, 10.0] {
        section_tool::create(&mut st, d, front, true, y).expect("Schnitt");
    }
    let body = body_of(&st, plate);
    let (lo, hi) = st.bodies[body].bounds;
    let refs = vec![
        edge_ref(&st, body, |p| near(p.x, lo.x) && near(p.y, lo.y)),
        edge_ref(&st, body, |p| near(p.x, hi.x) && near(p.y, lo.y)),
    ];
    for i in 0..30 {
        st.document
            .add_dimension(
                d,
                DimSpec {
                    id: DimId(0),
                    view: front,
                    kind: DimKind::Horizontal,
                    refs: refs.clone(),
                    offset: 10.0 + i as f64,
                    angle: 45.0,
                    text_shift: (0.0, 0.0),
                    tolerance: DimTol::None,
                    decimals: 1,
                    text_override: None,
                    chain: None,
                    suggested: false,
                },
            )
            .unwrap();
    }
    ok(&st);
    st
}

#[test]
fn models_have_the_promised_shape() {
    let medium = medium_model();
    let feats = medium.document.features();
    assert_eq!(feats.len(), 37);
    assert_eq!(medium.document.marker(), 37, "Marker am Ende");
    assert!(feats.iter().all(|f| f.status == FeatureStatus::Ok));
    let booleans = feats
        .iter()
        .filter(|f| matches!(f.kind, FeatureKind::Boolean { .. }))
        .count();
    let blends = feats
        .iter()
        .filter(|f| matches!(f.kind, FeatureKind::Verrunden { .. }))
        .count();
    let sketches = feats
        .iter()
        .filter(|f| matches!(f.kind, FeatureKind::Sketch(_)))
        .count();
    assert_eq!((booleans, blends, sketches), (10, 5, 1));
    assert_eq!(medium.document.drawings().len(), 1);
    assert_eq!(medium.document.drawings()[0].views.len(), 4);

    let heavy = heavy_model();
    assert!(
        heavy
            .document
            .features()
            .iter()
            .all(|f| f.status == FeatureStatus::Ok)
    );
    assert!(heavy.bodies.iter().any(|b| {
        b.solid
            .faces
            .iter()
            .any(|(_, f)| f.surface.kind() == SurfaceKind::Nurbs)
    }));
    // Platte mit 10 x 10 Bohrungen: mindestens 100 Zylinderwände in einem Körper.
    assert!(heavy.bodies.iter().any(|b| {
        b.solid
            .faces
            .iter()
            .filter(|(_, f)| f.surface.kind() == SurfaceKind::Cylinder)
            .count()
            >= 100
    }));
    let mesh = heavy
        .document
        .features()
        .iter()
        .find_map(|f| heavy.document.mesh_body(f.id))
        .expect("Netz-Körper");
    assert_eq!(mesh.0.triangle_count(), 200_000);
    assert!(
        heavy.bodies.iter().any(|b| b.kind == BodyKind::Mesh),
        "Netz-Körper sichtbar"
    );
    assert_eq!(heavy.document.marker(), heavy.document.features().len());
    let dr = &heavy.document.drawings()[0];
    assert_eq!(dr.views.len(), 6);
    assert_eq!(dr.views.iter().filter(|v| v.section.is_some()).count(), 2);
    assert_eq!(dr.dimensions.len(), 30);
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn ms<T>(f: impl FnOnce() -> T) -> (f64, T) {
    let t = Instant::now();
    let r = f();
    (t.elapsed().as_secs_f64() * 1000.0, r)
}

struct Row {
    snapshot: f64,
    bytes: f64,
    from_snapshot: f64,
    refresh_warm: f64,
    refresh_cold: f64,
    validate: f64,
}

fn measure(st: &mut AppState) -> Row {
    const RUNS: usize = 5;
    let (mut snap, mut bytes, mut from, mut warm, mut cold, mut val) =
        (vec![], vec![], vec![], vec![], vec![], vec![]);
    let tol = st.chord_tolerance;
    for _ in 0..RUNS {
        let (t, s) = ms(|| st.snapshot());
        snap.push(t);
        let (t, _) = ms(|| planora_app::history::snapshot_bytes(&s));
        bytes.push(t);
        let meshes: Vec<_> = s.meshes.values().cloned().collect();
        let (t, _) = ms(|| meshes.iter().for_each(|m| m.validate().unwrap()));
        val.push(t);
        let (t, doc) = ms(|| Document::from_snapshot(s.clone(), tol).unwrap());
        from.push(t);
        let mut fresh = AppState::new();
        fresh.document = doc;
        let (t, _) = ms(|| fresh.refresh());
        cold.push(t);
        // Wie beim echten Zurückladen: der Zustand behält seinen Netzprüfungs-Speicher.
        st.document = Document::from_snapshot(s, tol).unwrap();
        let (t, _) = ms(|| st.refresh());
        warm.push(t);
    }
    Row {
        snapshot: median(snap),
        bytes: median(bytes),
        from_snapshot: median(from),
        refresh_warm: median(warm),
        refresh_cold: median(cold),
        validate: median(val),
    }
}

#[test]
#[ignore = "Messung, im Release-Build mit --ignored --nocapture"]
fn measure_restore() {
    println!(
        "{:<8} {:>10} {:>12} {:>14} {:>13} {:>13} {:>14} {:>14}",
        "Modell",
        "snapshot",
        "snap_bytes",
        "from_snapshot",
        "refresh warm",
        "refresh kalt",
        "davon Netzpr.",
        "Summe (warm)"
    );
    let mut results = Vec::new();
    for (name, mut st) in [("mittel", medium_model()), ("schwer", heavy_model())] {
        let r = measure(&mut st);
        println!(
            "{:<8} {:>8.1}ms {:>10.2}ms {:>12.1}ms {:>11.1}ms {:>11.1}ms {:>12.1}ms {:>12.1}ms",
            name,
            r.snapshot,
            r.bytes,
            r.from_snapshot,
            r.refresh_warm,
            r.refresh_cold,
            r.validate,
            r.from_snapshot + r.refresh_warm
        );
        results.push(r);
    }
    let heavy = &results[1];
    println!(
        "Grenzen: Summe schwer <= 1000 ms: {}; snapshot schwer <= 50 ms: {}",
        heavy.from_snapshot + heavy.refresh_warm <= 1000.0,
        heavy.snapshot <= 50.0
    );
}
