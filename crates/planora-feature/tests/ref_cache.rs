//! Zwischenspeicher der Verweise (Nachbesserung Teilprojekt 12): gleiche Ergebnisse wie ohne
//! Speicher, und der teure Prüfpunkt einer Fläche wird je Durchgang nur einmal berechnet.
//!
//! Gezählt wird über `face_probe_calls` (Zähler je Thread), nicht über die Laufzeit.

use planora_drawing::dimension::{EndSel, GeoRef};
use planora_feature::*;
use planora_kernel::{Curve, Solid, Surface};
use planora_math::{Frame, Tolerance};
use planora_sketch::model::Sketch;

const HOLES: usize = 24;

fn tol() -> Tolerance {
    Tolerance::default()
}

/// Platte 130 × 60 × 4 mit `HOLES` Bohrungen r = 2 im Raster 10 mm (eine Extrusion).
fn plate() -> Document {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", tol());
    let p: Vec<_> = [(0.0, 0.0), (130.0, 0.0), (130.0, 60.0), (0.0, 60.0)]
        .iter()
        .map(|&(x, y)| sk.add_point(x, y).unwrap())
        .collect();
    for i in 0..4 {
        sk.add_line(p[i], p[(i + 1) % 4]).unwrap();
    }
    for i in 0..HOLES {
        let (x, y) = (10.0 + 10.0 * (i % 12) as f64, 20.0 + 20.0 * (i / 12) as f64);
        let c = sk.add_point(x, y).unwrap();
        sk.add_circle(c, 2.0).unwrap();
    }
    let mut d = Document::new();
    let s = d
        .add_feature(
            "Skizze 1",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: vec![],
            }),
        )
        .unwrap();
    d.add_feature(
        "Platte",
        FeatureKind::Extrude {
            sketch: s,
            profile: 0,
            height: 4.0,
        },
    )
    .unwrap();
    d
}

fn solid(d: &Document) -> &Solid {
    let bodies = d.visible_bodies();
    assert_eq!(bodies.len(), 1);
    bodies[0].solid
}

#[test]
fn cached_references_equal_uncached_ones() {
    let d = plate();
    let s = solid(&d);
    let cylinders = s
        .faces
        .iter()
        .filter(|(_, f)| matches!(f.surface, Surface::Cylinder { .. }))
        .count();
    assert_eq!(cylinders, HOLES, "eine Wand je Bohrung");
    let solids = [s];
    let cache = RefCache::new();
    let mut refs = Vec::new();
    for (e, edge) in s.edges.iter() {
        let plain = geo_ref_for_edge(s, e, &tol());
        let cached = geo_ref_for_edge_cached(s, e, &tol(), &cache);
        assert_eq!(plain.as_ref().ok(), cached.as_ref().ok(), "Kante {e:?}");
        assert_eq!(plain.is_err(), cached.is_err());
        if let Ok(r) = plain {
            refs.push(GeoRef::EdgeEnd {
                edge: r.clone(),
                end: EndSel::End,
            });
            if matches!(edge.curve, Curve::Circle { .. }) {
                refs.push(GeoRef::Center(r.clone()));
            }
            refs.push(GeoRef::Edge(r));
        }
    }
    for (f, _) in s.faces.iter() {
        let plain = geo_ref_for_face(s, f, &tol());
        let cached = geo_ref_for_face_cached(s, f, &tol(), &cache);
        assert_eq!(plain.as_ref().ok(), cached.as_ref().ok(), "Fläche {f:?}");
        if let Ok(r) = plain {
            refs.push(GeoRef::Face(r));
        }
    }
    assert!(refs.len() > 4 * HOLES);
    // Auflösen: gleiches Ergebnis mit und ohne Speicher (auch Fehler gleich).
    for r in &refs {
        assert_eq!(
            resolve_geo(&solids, r, &tol()),
            resolve_geo_cached(&solids, r, &tol(), &cache),
            "{r:?}"
        );
    }
}

#[test]
fn each_face_is_probed_once_per_cache() {
    let d = plate();
    let s = solid(&d);
    let faces = s.faces.iter().count() as u64;
    // Deck- und Bodenfläche grenzen an alle Bohrungskreise: ohne Speicher würde ihr Prüfpunkt
    // für jede dieser Kanten neu berechnet.
    assert_eq!(
        s.faces
            .iter()
            .filter(|(_, f)| f.inner.len() == HOLES)
            .count(),
        2
    );
    let edges: Vec<_> = s.edges.iter().map(|(e, _)| e).collect();
    let cache = RefCache::new();
    let before = face_probe_calls();
    let mut n = 0;
    for &e in &edges {
        if geo_ref_for_edge_cached(s, e, &tol(), &cache).is_ok() {
            n += 1;
        }
    }
    let cached = face_probe_calls() - before;
    assert!(n >= 3 * HOLES, "{n} Bezüge");
    assert!(
        cached <= faces,
        "jede Fläche höchstens einmal: {cached} Prüfpunkte für {faces} Flächen"
    );
    // Ohne gemeinsamen Speicher: je Kante die Prüfpunkte beider Nachbarflächen (bei einer Naht
    // nur einer), Deck- und Bodenfläche also für jede ihrer Kanten neu.
    let before = face_probe_calls();
    for &e in &edges {
        let _ = geo_ref_for_edge(s, e, &tol());
    }
    let plain = face_probe_calls() - before;
    assert!(plain >= n as u64 + 2 * HOLES as u64, "Zählung: {plain}");
    assert!(plain > 4 * cached, "{plain} ohne, {cached} mit Speicher");
    // Zylinderflächen: Prüfpunkt und Achsbereich einmal je Durchgang, egal wie oft aufgelöst.
    let solids = [s];
    let wall = s
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Cylinder { .. }))
        .map(|(i, _)| i)
        .unwrap();
    let r = GeoRef::Face(geo_ref_for_face(s, wall, &tol()).unwrap());
    let cache = RefCache::new();
    let before = face_probe_calls();
    let first = resolve_geo_cached(&solids, &r, &tol(), &cache).unwrap();
    for _ in 0..10 {
        assert_eq!(
            resolve_geo_cached(&solids, &r, &tol(), &cache).unwrap(),
            first
        );
    }
    let used = face_probe_calls() - before;
    assert!(used <= 1, "{used} Prüfpunkte für eine Fläche");
}
