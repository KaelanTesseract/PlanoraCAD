//! Skizzen 6, Task 4: Projektionen mit Herkunft. Projizierte Kurven folgen dem Körper; geht der
//! Bezug verloren, bleibt die Kurve stehen und die Skizze bekommt eine Warnung statt eines
//! Fehlers.

use planora_feature::*;
use planora_kernel::{BoolOp, Face, Id, Solid, section_by_plane, silhouette};
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::helpers::ProjectStyle;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};

const LINKED: ProjectStyle = ProjectStyle {
    construction: false,
};

fn tol() -> Tolerance {
    Tolerance::default()
}

fn body(d: &Document, feature: FeatureId) -> Solid {
    d.body(BodyRef { feature, index: 0 }).unwrap().clone()
}

fn add_box(d: &mut Document, name: &str, size: DVec3, origin: DVec3) -> FeatureId {
    d.add_feature(name, FeatureKind::Box { size, origin })
        .unwrap()
}

fn sketch_kind(sketch: Sketch, plane: StandardPlane, copies: Vec<EdgeCopy>) -> FeatureKind {
    FeatureKind::Sketch(SketchFeature {
        sketch,
        plane: SketchPlane::Standard(plane),
        copies,
        formulas: Default::default(),
    })
}

fn line_points(sk: &Sketch, c: CurveId) -> (PointId, PointId) {
    match sk.curve(c).unwrap().kind {
        CurveKind::Line { a, b } => (a, b),
        ref k => panic!("keine Linie: {k:?}"),
    }
}

fn line_ends(sk: &Sketch, c: CurveId) -> (DVec2, DVec2) {
    let (a, b) = line_points(sk, c);
    (sk.point_xy(a).unwrap(), sk.point_xy(b).unwrap())
}

fn close(a: DVec2, b: DVec2) -> bool {
    (a - b).length() < 1e-7
}

/// Erste Linienkante von `solid`, für die `pick` gilt, verknüpft projiziert in `sk`.
fn project_line_edge(
    sk: &mut Sketch,
    solid: &Solid,
    pick: impl Fn(&EdgeRef, &Sketch, CurveId) -> bool,
) -> (CurveId, EdgeRef) {
    for (id, e) in solid.edges.iter() {
        let Ok(r) = EdgeRef::from_edge(solid, id, &tol()) else {
            continue;
        };
        let mut trial = sk.clone();
        let Ok(c) = trial.add_projected_curve(&e.curve, e.t0, e.t1, LINKED) else {
            continue;
        };
        if matches!(trial.curve(c).unwrap().kind, CurveKind::Line { .. }) && pick(&r, &trial, c) {
            *sk = trial;
            return (c, r);
        }
    }
    panic!("keine passende Kante");
}

fn edge_copy(curve: CurveId, edge: EdgeRef) -> EdgeCopy {
    EdgeCopy {
        curve,
        source: CopySource::Edge(edge),
    }
}

fn sketch_def(d: &Document, s: FeatureId) -> &SketchFeature {
    match &d.feature(s).unwrap().kind {
        FeatureKind::Sketch(sf) => sf,
        k => panic!("keine Skizze: {k:?}"),
    }
}

/// Quader (Länge 10 in x) mit einer verknüpft projizierten Kante der Länge 10 auf XY.
fn box_with_projected_edge() -> (Document, FeatureId, FeatureId, CurveId) {
    let mut d = Document::new();
    let b = add_box(&mut d, "Quader", DVec3::new(10.0, 20.0, 5.0), DVec3::ZERO);
    let solid = body(&d, b);
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", tol());
    let (curve, edge) = project_line_edge(&mut sk, &solid, |_, s, c| {
        (s.line_length(c).unwrap() - 10.0).abs() < 1e-9
    });
    // Eigenes Profil abseits des Quaders für eine Extrusion.
    sk.add_rectangle(DVec2::new(50.0, 50.0), DVec2::new(55.0, 55.0))
        .unwrap();
    let s = d
        .add_feature(
            "Skizze",
            sketch_kind(sk, StandardPlane::XY, vec![edge_copy(curve, edge)]),
        )
        .unwrap();
    assert_eq!(d.feature(s).unwrap().status, FeatureStatus::Ok);
    assert!(d.feature(s).unwrap().warning().is_none());
    (d, b, s, curve)
}

fn set_box_x(d: &mut Document, b: FeatureId, x: f64) {
    d.update(b, |k| {
        if let FeatureKind::Box { size, .. } = k {
            size.x = x;
        }
    })
    .unwrap();
}

#[test]
fn lost_edge_keeps_geometry_and_warns() {
    // Quader mit einem Loch aus einem zweiten Quader (Boolesche Differenz). Projiziert wird eine
    // Kante am Lochrand: zwischen einer Fläche des Quaders und einer Wand des Werkzeugs. Wird die
    // Differenz gelöscht, gibt es diese Kante nicht mehr.
    // Skizzen 6: Abweichung vom Plan – statt „Verschieben“ eine Boolesche Differenz, weil
    // Verschieben die Flächenmarken behält und die Kante nach dem Löschen wiedergefunden würde.
    let mut d = Document::new();
    let a = add_box(&mut d, "A", DVec3::new(20.0, 20.0, 10.0), DVec3::ZERO);
    let t = add_box(
        &mut d,
        "B",
        DVec3::new(10.0, 10.0, 20.0),
        DVec3::new(5.0, 5.0, -5.0),
    );
    let cut = d
        .add_feature(
            "Differenz",
            FeatureKind::Boolean {
                target: BodyRef {
                    feature: a,
                    index: 0,
                },
                tool: BodyRef {
                    feature: t,
                    index: 0,
                },
                op: BoolOp::Difference,
            },
        )
        .unwrap();
    assert_eq!(d.feature(cut).unwrap().status, FeatureStatus::Ok);
    let solid = body(&d, cut);
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", tol());
    let (curve, edge) = project_line_edge(&mut sk, &solid, |r, _, _| r.a.origin != r.b.origin);
    // Dreieck aus der Projektion und zwei Nutzerlinien, angebunden über `Coincident`.
    let (pa, pb) = line_points(&sk, curve);
    let (ea, eb) = line_ends(&sk, curve);
    let along = (eb - ea).normalize();
    let apex = (ea + eb) * 0.5 + DVec2::new(-along.y, along.x) * 4.0;
    let u1 = sk.add_point(ea.x + 0.5, ea.y + 0.5).unwrap();
    let u2 = sk.add_point(eb.x - 0.5, eb.y + 0.5).unwrap();
    let q1 = sk.add_point(apex.x, apex.y).unwrap();
    let q2 = sk.add_point(apex.x, apex.y).unwrap();
    sk.add_line(u1, q1).unwrap();
    sk.add_line(q2, u2).unwrap();
    sk.add_constraint(K::Coincident(u1, pa)).unwrap();
    sk.add_constraint(K::Coincident(u2, pb)).unwrap();
    sk.add_constraint(K::Coincident(q1, q2)).unwrap();
    let s = d
        .add_feature(
            "Skizze",
            sketch_kind(sk, StandardPlane::XY, vec![edge_copy(curve, edge)]),
        )
        .unwrap();
    let e = d
        .add_feature(
            "Extrusion",
            FeatureKind::Extrude {
                sketch: s,
                profiles: ProfileSel::Index(0),
                height: 3.0,
            },
        )
        .unwrap();
    assert_eq!(d.feature(s).unwrap().status, FeatureStatus::Ok);
    assert!(d.feature(s).unwrap().warning().is_none(), "Bezug noch da");
    assert_eq!(
        d.feature(e).unwrap().status,
        FeatureStatus::Ok,
        "{:?}",
        d.feature(e).unwrap().status
    );
    let before = line_ends(d.sketch_of(s).unwrap(), curve);

    d.delete(cut).unwrap();
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    let warning = f
        .warning()
        .map(str::to_string)
        .expect("Warnung über den verlorenen Bezug");
    assert_eq!(
        warning,
        planora_i18n::tr!("feature-sketch-projection-lost", count = 1)
    );
    let sk = d.sketch_of(s).unwrap();
    let after = line_ends(sk, curve);
    assert!(
        close(before.0, after.0) && close(before.1, after.1),
        "Kurve an alter Stelle: {before:?} → {after:?}"
    );
    // Die Nutzerlinien hängen weiter an der stehengebliebenen Kurve.
    assert!(close(sk.point_xy(u1).unwrap(), after.0));
    assert!(close(sk.point_xy(u2).unwrap(), after.1));
    assert_eq!(
        d.feature(e).unwrap().status,
        FeatureStatus::Ok,
        "{:?}",
        d.feature(e).unwrap().status
    );
    // Der Verweis bleibt gespeichert (ein späterer Körper könnte die Kante wiederbringen).
    assert_eq!(sketch_def(&d, s).copies.len(), 1);
    d.check_invariants().unwrap();
}

#[test]
fn changed_kind_keeps_geometry_and_warns() {
    // Die Kante ist eine Gerade, die gespeicherte Kurve ein Kreis: Die Art hat sich geändert.
    let mut d = Document::new();
    let b = add_box(&mut d, "Quader", DVec3::new(10.0, 20.0, 5.0), DVec3::ZERO);
    let solid = body(&d, b);
    let mut probe = Sketch::new(Frame::WORLD, "Probe", tol());
    let (_, edge) = project_line_edge(&mut probe, &solid, |_, _, _| true);
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", tol());
    let m = sk.add_point(30.0, 30.0).unwrap();
    let circle = sk.add_circle(m, 4.0).unwrap();
    let s = d
        .add_feature(
            "Skizze",
            sketch_kind(sk, StandardPlane::XY, vec![edge_copy(circle, edge)]),
        )
        .unwrap();
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    assert!(f.warning().is_some());
    let sk = d.sketch_of(s).unwrap();
    assert!(matches!(
        sk.curve(circle).unwrap().kind,
        CurveKind::Circle { .. }
    ));
    assert!((sk.radius(circle).unwrap() - 4.0).abs() < 1e-9);
    assert!(close(sk.point_xy(m).unwrap(), DVec2::new(30.0, 30.0)));
}

#[test]
fn deleted_curve_is_pruned_not_error() {
    let (mut d, _, s, curve) = box_with_projected_edge();
    d.update(s, |k| {
        if let FeatureKind::Sketch(sf) = k {
            sf.sketch.remove_curve(curve).unwrap();
        }
    })
    .unwrap();
    let f = d.feature(s).unwrap();
    assert_eq!(
        f.status,
        FeatureStatus::Ok,
        "tote Kurve ist kein Fehler: {:?}",
        f.status
    );
    assert!(f.warning().is_some(), "tote Kurve zählt als verloren");

    let mut sf = sketch_def(&d, s).clone();
    assert!(sf.prune_copies());
    assert!(sf.copies.is_empty());
    assert!(!sf.prune_copies(), "nichts mehr zu entfernen");
    d.update(s, |k| *k = FeatureKind::Sketch(sf)).unwrap();
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok);
    assert!(f.warning().is_none(), "nach dem Aufräumen keine Warnung");
}

#[test]
fn break_link_stops_following() {
    let (mut d, b, s, curve) = box_with_projected_edge();
    set_box_x(&mut d, b, 12.0);
    let len = d.sketch_of(s).unwrap().line_length(curve).unwrap();
    assert!((len - 12.0).abs() < 1e-7, "verknüpft folgt: {len}");

    // Wie die App beim Öffnen: Definition mit der berechneten (nachgeführten) Skizze.
    let mut sf = sketch_def(&d, s).clone();
    sf.sketch = d.sketch_of(s).unwrap().clone();
    assert!(sf.break_link(curve));
    assert!(!sf.break_link(curve), "nur einmal");
    d.update(s, |k| *k = FeatureKind::Sketch(sf)).unwrap();
    set_box_x(&mut d, b, 15.0);
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok);
    assert!(f.warning().is_none());
    let sk = d.sketch_of(s).unwrap();
    let len = sk.line_length(curve).unwrap();
    assert!((len - 12.0).abs() < 1e-7, "gelöst bleibt stehen: {len}");
    // Die `Fixed`-Werte der Projektion bleiben.
    let (pa, _) = line_points(sk, curve);
    assert!(
        sk.constraints()
            .any(|(_, c)| matches!(c.kind, K::Fixed(p, ..) if p == pa)),
        "Fixed bleibt"
    );
}

/// Fläche von `solid`, für die `pick` gilt.
fn find_face(solid: &Solid, pick: impl Fn(Id<Face>) -> bool) -> Id<Face> {
    solid
        .faces
        .iter()
        .map(|(f, _)| f)
        .find(|f| pick(*f))
        .expect("passende Fläche")
}

#[test]
fn section_copy_follows_body() {
    // Quader quer durch die XZ-Ebene; projiziert wird der Schnitt der Deckfläche (z = 10).
    let mut d = Document::new();
    let b = add_box(
        &mut d,
        "Quader",
        DVec3::new(20.0, 20.0, 10.0),
        DVec3::new(-10.0, -10.0, 0.0),
    );
    let solid = body(&d, b);
    let frame = StandardPlane::XZ.frame();
    let top = find_face(&solid, |f| {
        planora_sketch::plane_of_face(&solid, f).is_some_and(|p| p.z.z > 0.5 && p.origin.z > 9.0)
    });
    let pieces = section_by_plane(&solid, &frame, Some(&[top]), &tol()).unwrap();
    assert_eq!(pieces.len(), 1);
    let mut sk = Sketch::new(frame, "Skizze", tol());
    let p = &pieces[0];
    let curve = sk
        .add_projected_curve(&p.curve, p.t0, p.t1, LINKED)
        .unwrap();
    assert!((sk.line_length(curve).unwrap() - 20.0).abs() < 1e-7);
    let face = FaceRef::from_face(&solid, top, &tol()).unwrap();
    let s = d
        .add_feature(
            "Skizze",
            sketch_kind(
                sk,
                StandardPlane::XZ,
                vec![EdgeCopy {
                    curve,
                    source: CopySource::Section { face, index: 0 },
                }],
            ),
        )
        .unwrap();
    assert!(d.feature(s).unwrap().warning().is_none());
    d.update(b, |k| {
        if let FeatureKind::Box { size, .. } = k {
            size.x = 30.0;
            size.z = 12.0;
        }
    })
    .unwrap();
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    assert!(f.warning().is_none(), "{:?}", f.warning());
    let sk = d.sketch_of(s).unwrap();
    assert!((sk.line_length(curve).unwrap() - 30.0).abs() < 1e-7);
    let (a, b2) = line_ends(sk, curve);
    assert!(
        (a.y - 12.0).abs() < 1e-7 && (b2.y - 12.0).abs() < 1e-7,
        "{a:?} {b2:?}"
    );
}

#[test]
fn silhouette_copy_follows_radius() {
    // Zylinder um z, Skizze auf XZ (Blick entlang −y): Umriss sind die Mantellinien bei x = ±r.
    let mut d = Document::new();
    let c = d
        .add_feature(
            "Zylinder",
            FeatureKind::Cylinder {
                radius: 5.0,
                height: 20.0,
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let solid = body(&d, c);
    let frame = StandardPlane::XZ.frame();
    let face = find_face(&solid, |f| {
        silhouette(&solid, f, frame.z, &tol()).is_ok_and(|p| !p.is_empty())
    });
    let pieces = silhouette(&solid, face, frame.z, &tol()).unwrap();
    let mut sk = Sketch::new(frame, "Skizze", tol());
    let p = &pieces[0];
    let curve = sk
        .add_projected_curve(&p.curve, p.t0, p.t1, LINKED)
        .unwrap();
    let x0 = line_ends(&sk, curve).0.x;
    assert!((x0.abs() - 5.0).abs() < 1e-7, "{x0}");
    let face_ref = FaceRef::from_face(&solid, face, &tol()).unwrap();
    let s = d
        .add_feature(
            "Skizze",
            sketch_kind(
                sk,
                StandardPlane::XZ,
                vec![EdgeCopy {
                    curve,
                    source: CopySource::Silhouette {
                        face: face_ref,
                        index: 0,
                    },
                }],
            ),
        )
        .unwrap();
    assert!(d.feature(s).unwrap().warning().is_none());
    d.update(c, |k| {
        if let FeatureKind::Cylinder { radius, .. } = k {
            *radius = 8.0;
        }
    })
    .unwrap();
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    assert!(f.warning().is_none(), "{:?}", f.warning());
    let (a, b) = line_ends(d.sketch_of(s).unwrap(), curve);
    assert!((a.x - 8.0 * x0.signum()).abs() < 1e-7, "{a:?}");
    assert!((b.x - 8.0 * x0.signum()).abs() < 1e-7, "{b:?}");
}

#[test]
fn hints_do_not_bump_edits() {
    // Wie `hints_are_written_back_after_a_resolution_without_a_new_version`: Ein falscher
    // Hinweispunkt der Herkunft wird zurückgeschrieben, ohne neue Version und ohne Änderung.
    let mut d = Document::new();
    let b = add_box(
        &mut d,
        "Quader",
        DVec3::new(20.0, 20.0, 10.0),
        DVec3::new(-10.0, -10.0, 0.0),
    );
    let solid = body(&d, b);
    let frame = StandardPlane::XZ.frame();
    let top = find_face(&solid, |f| {
        planora_sketch::plane_of_face(&solid, f).is_some_and(|p| p.z.z > 0.5 && p.origin.z > 9.0)
    });
    let pieces = section_by_plane(&solid, &frame, Some(&[top]), &tol()).unwrap();
    let mut sk = Sketch::new(frame, "Skizze", tol());
    let curve = sk
        .add_projected_curve(&pieces[0].curve, pieces[0].t0, pieces[0].t1, LINKED)
        .unwrap();
    let mut face = FaceRef::from_face(&solid, top, &tol()).unwrap();
    let probe = face.hint;
    face.hint += DVec3::new(3.0, -3.0, 0.0);
    let (edge_curve, mut edge) = project_line_edge(&mut sk, &solid, |_, _, _| true);
    let edge_probe = edge.hint;
    edge.hint += DVec3::new(0.5, 0.0, 0.0);
    let copies = vec![
        EdgeCopy {
            curve,
            source: CopySource::Section { face, index: 0 },
        },
        edge_copy(edge_curve, edge),
    ];
    let s = d
        .add_feature("Skizze", sketch_kind(sk, StandardPlane::XZ, copies))
        .unwrap();
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok);
    assert!(f.warning().is_none(), "{:?}", f.warning());
    let (version, edits) = (f.version(), d.edit_count());
    let sf = sketch_def(&d, s);
    match &sf.copies[0].source {
        CopySource::Section { face, .. } => assert_eq!(face.hint, probe, "Hinweis zurück"),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        sf.copies[1].edge().unwrap().hint,
        edge_probe,
        "Kantenhinweis zurück"
    );
    let again = d.recompute();
    assert!(again.computed.is_empty(), "{:?}", again.computed);
    assert_eq!(
        d.feature(s).unwrap().version(),
        version,
        "keine neue Version"
    );
    assert_eq!(d.edit_count(), edits, "keine Änderung gezählt");
}

/// Extrusion des Rechtecks der Skizze aus [`box_with_projected_edge`].
fn extrude(d: &mut Document, s: FeatureId) -> FeatureId {
    let e = d
        .add_feature(
            "Extrusion",
            FeatureKind::Extrude {
                sketch: s,
                profiles: ProfileSel::Index(0),
                height: 2.0,
            },
        )
        .unwrap();
    assert_eq!(d.feature(e).unwrap().status, FeatureStatus::Ok);
    e
}

fn assert_lost_but_ok(d: &Document, s: FeatureId, e: FeatureId) {
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    assert!(f.warning().is_some(), "Warnung statt Stillstand");
    let fe = d.feature(e).unwrap();
    assert_eq!(fe.status, FeatureStatus::Ok, "{:?}", fe.status);
    d.check_invariants().unwrap();
}

#[test]
fn suppressed_source_warns_and_dependents_compute() {
    let (mut d, b, s, curve) = box_with_projected_edge();
    let e = extrude(&mut d, s);
    d.set_suppressed(b, true).unwrap();
    assert_lost_but_ok(&d, s, e);
    let len = d.sketch_of(s).unwrap().line_length(curve).unwrap();
    assert!((len - 10.0).abs() < 1e-7, "{len}");
    // Zurückgeholt folgt die Kurve wieder, ohne Warnung.
    d.set_suppressed(b, false).unwrap();
    set_box_x(&mut d, b, 13.0);
    let f = d.feature(s).unwrap();
    assert!(f.warning().is_none(), "{:?}", f.warning());
    let len = d.sketch_of(s).unwrap().line_length(curve).unwrap();
    assert!((len - 13.0).abs() < 1e-7, "{len}");
}

#[test]
fn source_in_error_warns_and_dependents_compute() {
    let (mut d, b, s, _) = box_with_projected_edge();
    let e = extrude(&mut d, s);
    d.update(b, |k| {
        if let FeatureKind::Box { size, .. } = k {
            size.x = 0.0;
        }
    })
    .unwrap();
    assert!(
        matches!(d.feature(b).unwrap().status, FeatureStatus::Error(_)),
        "{:?}",
        d.feature(b).unwrap().status
    );
    assert_lost_but_ok(&d, s, e);
}

#[test]
fn lost_curve_stays_at_last_followed_position() {
    let (mut d, b, s, curve) = box_with_projected_edge();
    set_box_x(&mut d, b, 12.0);
    let followed = line_ends(d.sketch_of(s).unwrap(), curve);
    assert!((d.sketch_of(s).unwrap().line_length(curve).unwrap() - 12.0).abs() < 1e-7);
    d.set_suppressed(b, true).unwrap();
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok);
    assert!(f.warning().is_some());
    let sk = d.sketch_of(s).unwrap();
    let after = line_ends(sk, curve);
    assert!(
        close(followed.0, after.0) && close(followed.1, after.1),
        "zuletzt nachgeführte Lage, nicht die der Definition: {followed:?} → {after:?}"
    );
    // Die `Fixed`-Werte stimmen mit der Lage überein (die Kurve springt beim Lösen nicht).
    let (pa, _) = line_points(sk, curve);
    assert!(sk.constraints().any(|(_, c)| matches!(
        c.kind,
        K::Fixed(p, x, y) if p == pa && close(DVec2::new(x, y), after.0)
    )));
    // Die Definition bleibt unverändert (alte Dateien bytegleich).
    let def = &sketch_def(&d, s).sketch;
    assert!((def.line_length(curve).unwrap() - 10.0).abs() < 1e-7);
}
