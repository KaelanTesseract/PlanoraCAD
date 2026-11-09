//! Skizzen 6, Task 4: Projektionen mit Herkunft. Projizierte Kurven folgen dem Körper; geht der
//! Bezug verloren, bleibt die Kurve stehen und die Skizze bekommt eine Warnung statt eines
//! Fehlers.

use planora_feature::planes::{AxisRef, PlaneDef, PlaneFeature, StdAxis};
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

/// Skizzen 6, Task 9: Ein schräg projizierter Kreisbogen (Ellipsenbogen) folgt seiner Kante
/// ohne Warnung. Neue Projektionen einer Kante entstehen über `add_projected_curve`; das
/// Nachführen muss dieselbe Projektion benutzen (nicht die ältere Kantenkopie, die schräge
/// Bögen ablehnt).
#[test]
fn oblique_arc_projection_follows_without_warning() {
    let mut d = Document::new();
    let b = add_box(&mut d, "Quader", DVec3::splat(10.0), DVec3::ZERO);
    let solid = body(&d, b);
    // Eine senkrechte Kante runden: oben und unten entstehen Viertelbögen in waagerechten Ebenen.
    let vertical = solid
        .edges
        .iter()
        .find(|(_, e)| (e.curve.point(e.t0).z - e.curve.point(e.t1).z).abs() > 5.0)
        .unwrap()
        .0;
    let r = EdgeRef::from_edge(&solid, vertical, &tol()).unwrap();
    let f = d
        .add_feature(
            "Rundung",
            FeatureKind::Verrunden {
                body: BodyRef {
                    feature: b,
                    index: 0,
                },
                edges: vec![r],
                art: BlendArt::Rundung { radius: 2.0 },
                tangentenkette: true,
            },
        )
        .unwrap();
    assert_eq!(d.feature(f).unwrap().status, FeatureStatus::Ok);
    // Skizze auf einer um 30° um X gedrehten Ebene: die Bögen liegen schräg zu ihr.
    let p = d
        .add_feature(
            "Ebene",
            FeatureKind::Plane(PlaneFeature {
                def: PlaneDef::Angle {
                    axis: AxisRef::Standard(StdAxis::X),
                    angle: 30.0,
                },
            }),
        )
        .unwrap();
    let frame = d.plane_frame(p).unwrap();
    let solid = body(&d, f);
    let mut sk = Sketch::new(frame, "Skizze", tol());
    let (curve, edge) = solid
        .edges
        .iter()
        .find_map(|(id, e)| {
            let partial = matches!(e.curve, planora_kernel::Curve::Circle { .. })
                && (e.t1 - e.t0).abs() < 6.0;
            if !partial {
                return None;
            }
            let c = sk.add_projected_curve(&e.curve, e.t0, e.t1, LINKED).ok()?;
            Some((c, EdgeRef::from_edge(&solid, id, &tol()).ok()?))
        })
        .expect("ein Viertelbogen");
    assert!(matches!(
        sk.curve(curve).unwrap().kind,
        CurveKind::EllipticArc { .. }
    ));
    let s = d
        .add_feature(
            "Skizze",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::OnPlane(p),
                copies: vec![edge_copy(curve, edge)],
                formulas: Default::default(),
            }),
        )
        .unwrap();
    let feature = d.feature(s).unwrap();
    assert_eq!(feature.status, FeatureStatus::Ok);
    assert!(feature.warning().is_none(), "{:?}", feature.warning());
}

// ---- Skizzen 7, Task 10: feste Splines ---------------------------------------------------------

/// Profil aus drei Linien und einem Spline durch `(5, top)`.
fn spline_base(top: f64) -> Sketch {
    let mut sk = Sketch::new(Frame::WORLD, "Basis", tol());
    let p: Vec<_> = [(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (5.0, top), (0.0, 5.0)]
        .iter()
        .map(|&(x, y)| sk.add_point(x, y).unwrap())
        .collect();
    sk.add_line(p[0], p[1]).unwrap();
    sk.add_line(p[1], p[2]).unwrap();
    sk.add_spline(&p[2..5]).unwrap();
    sk.add_line(p[4], p[0]).unwrap();
    sk
}

/// Körper aus [`spline_base`], darauf eine Skizze mit der projizierten oberen Freiformkante und einem
/// Punkt auf ihr. Skizze 2 steht auf XY.
fn body_with_fixed_spline() -> (Document, FeatureId, FeatureId, CurveId, PointId) {
    let mut d = Document::new();
    let base = d
        .add_feature(
            "Basis",
            sketch_kind(spline_base(7.0), StandardPlane::XY, vec![]),
        )
        .unwrap();
    d.add_feature(
        "Körper",
        FeatureKind::Extrude {
            sketch: base,
            profiles: ProfileSel::Index(0),
            height: 4.0,
        },
    )
    .unwrap();
    let solid = d.visible_bodies()[0].solid.clone();
    let (id, e) = solid
        .edges
        .iter()
        .find(|(_, e)| matches!(e.curve, planora_kernel::Curve::Nurbs(_)))
        .expect("Freiformkante");
    let edge = EdgeRef::from_edge(&solid, id, &tol()).unwrap();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", tol());
    let c = sk
        .add_projected_curve(&e.curve, e.t0, e.t1, LINKED)
        .unwrap();
    let q = sk.add_point(5.0, 7.5).unwrap();
    sk.add_constraint(K::PointOnCurve(q, c)).unwrap();
    assert_eq!(
        sk.solve().status,
        planora_sketch::solver::SolveStatus::Solved
    );
    let s = d
        .add_feature(
            "Skizze",
            sketch_kind(sk, StandardPlane::XY, vec![edge_copy(c, edge)]),
        )
        .unwrap();
    assert!(d.feature(s).unwrap().warning().is_none());
    (d, base, s, c, q)
}

fn set_base(d: &mut Document, base: FeatureId, sketch: Sketch) {
    d.update(base, |k| {
        if let FeatureKind::Sketch(sf) = k {
            sf.sketch = sketch;
        }
    })
    .unwrap();
}

#[test]
fn reproject_keeps_id_and_constraints() {
    let (mut d, base, s, c, q) = body_with_fixed_spline();
    let before = match &d.sketch_of(s).unwrap().curve(c).unwrap().kind {
        CurveKind::FixedSpline { curve, .. } => curve.clone(),
        k => panic!("kein fester Spline: {k:?}"),
    };
    set_base(&mut d, base, spline_base(9.0));
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    assert!(f.warning().is_none(), "der Bezug hält: {:?}", f.warning());
    let sk = d.sketch_of(s).unwrap();
    let CurveKind::FixedSpline { curve, start, end } = &sk.curve(c).unwrap().kind else {
        panic!("Art und Id bleiben");
    };
    assert_ne!(*curve, before, "die Kurve folgt dem Körper");
    let top = DVec2::new(5.0, 9.0);
    assert!(sk.nearest_on_curve(c, top).unwrap().distance(top) < 1e-6);
    // Die Fest-Werte folgen, der Punkt bleibt auf der Kurve.
    for p in [*start, *end] {
        let xy = sk.point_xy(p).unwrap();
        assert!(
            sk.constraints().any(|(_, d)| matches!(d.kind,
                K::Fixed(o, x, y) if o == p && x == xy.x && y == xy.y)),
            "{xy}"
        );
    }
    assert!(sk.constraints().any(|(_, d)| matches!(d.kind,
        K::PointOnCurve(o, k) if o == q && k == c)));
    let on = sk.point_xy(q).unwrap();
    assert!(sk.nearest_on_curve(c, on).unwrap().distance(on) < 1e-6);
    d.check_invariants().unwrap();
}

#[test]
fn degree_change_on_reproject_is_fine() {
    // Mehr Stützpunkte, also mehr Knoten; aus einem Grad-3-Stück wird keine andere Art.
    let (mut d, base, s, c, _) = body_with_fixed_spline();
    let knots = |d: &Document| match &d.sketch_of(s).unwrap().curve(c).unwrap().kind {
        CurveKind::FixedSpline { curve, .. } => curve.knots().knots().len(),
        k => panic!("kein fester Spline: {k:?}"),
    };
    let before = knots(&d);
    let mut sk = Sketch::new(Frame::WORLD, "Basis", tol());
    let p: Vec<_> = [
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, 5.0),
        (8.0, 7.0),
        (5.0, 8.0),
        (2.0, 7.0),
        (0.0, 5.0),
    ]
    .iter()
    .map(|&(x, y)| sk.add_point(x, y).unwrap())
    .collect();
    sk.add_line(p[0], p[1]).unwrap();
    sk.add_line(p[1], p[2]).unwrap();
    sk.add_spline(&p[2..7]).unwrap();
    sk.add_line(p[6], p[0]).unwrap();
    set_base(&mut d, base, sk);
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    assert!(
        f.warning().is_none(),
        "keine Artänderung: {:?}",
        f.warning()
    );
    assert_ne!(knots(&d), before);
    let top = DVec2::new(5.0, 8.0);
    let sk = d.sketch_of(s).unwrap();
    assert!(sk.nearest_on_curve(c, top).unwrap().distance(top) < 1e-6);
}

#[test]
fn kind_change_is_lost_reference_warning() {
    let (mut d, base, s, c, _) = body_with_fixed_spline();
    let before = match &d.sketch_of(s).unwrap().curve(c).unwrap().kind {
        CurveKind::FixedSpline { curve, .. } => curve.clone(),
        k => panic!("kein fester Spline: {k:?}"),
    };
    // Aus dem Spline wird eine Linie: Die Kante ist keine Freiformkante mehr.
    let mut sk = Sketch::new(Frame::WORLD, "Basis", tol());
    let p: Vec<_> = [(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (0.0, 5.0)]
        .iter()
        .map(|&(x, y)| sk.add_point(x, y).unwrap())
        .collect();
    for i in 0..4 {
        sk.add_line(p[i], p[(i + 1) % 4]).unwrap();
    }
    set_base(&mut d, base, sk);
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    assert!(f.warning().is_some(), "Warnung statt Fehler");
    let CurveKind::FixedSpline { curve, .. } = &d.sketch_of(s).unwrap().curve(c).unwrap().kind
    else {
        panic!("die Kurve bleibt stehen");
    };
    assert_eq!(*curve, before, "an der alten Lage");
    assert_eq!(sketch_def(&d, s).copies.len(), 1, "der Verweis bleibt");
    d.check_invariants().unwrap();
}

/// Review Focus 3: Die geschlossene Kante eines Ellipsenzylinders wird ein fester Spline mit einem
/// Punkt, bildet allein ein Profil und lässt sich mit der richtigen Größe extrudieren.
#[test]
fn closed_projection_makes_a_profile_and_extrudes() {
    let mut d = Document::new();
    let mut base = Sketch::new(Frame::WORLD, "Basis", tol());
    let m = base.add_point(0.0, 0.0).unwrap();
    base.add_ellipse(m, DVec2::new(6.0, 0.0), 3.0).unwrap();
    let base = d
        .add_feature("Basis", sketch_kind(base, StandardPlane::XY, vec![]))
        .unwrap();
    d.add_feature(
        "Körper",
        FeatureKind::Extrude {
            sketch: base,
            profiles: ProfileSel::Index(0),
            height: 4.0,
        },
    )
    .unwrap();
    let solid = d.visible_bodies()[0].solid.clone();
    let (_, e) = solid
        .edges
        .iter()
        .find(|(_, e)| {
            matches!(&e.curve, planora_kernel::Curve::Nurbs(n) if n.is_closed(tol().linear))
        })
        .expect("geschlossene Freiformkante");
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", tol());
    let c = sk
        .add_projected_curve(&e.curve, e.t0, e.t1, LINKED)
        .unwrap();
    let CurveKind::FixedSpline { start, end, .. } = sk.curve(c).unwrap().kind.clone() else {
        panic!("kein fester Spline");
    };
    assert_eq!(start, end);
    assert_eq!(sk.points().count(), 1);
    let found = sk.profiles();
    assert_eq!(found.profiles.len(), 1, "{:?}", found.rejected);
    let s = d
        .add_feature("Skizze", sketch_kind(sk, StandardPlane::XY, vec![]))
        .unwrap();
    let before = d.visible_bodies().len();
    let x = d
        .add_feature(
            "Extrusion",
            FeatureKind::Extrude {
                sketch: s,
                profiles: ProfileSel::Index(0),
                height: 2.0,
            },
        )
        .unwrap();
    let f = d.feature(x).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    let bodies = d.visible_bodies();
    assert_eq!(bodies.len(), before + 1);
    let v = planora_mesh::mass_properties(
        &planora_mesh::tessellate(
            bodies[before].solid,
            &planora_mesh::TessellationOptions {
                chord_tolerance: 1e-3,
            },
        )
        .unwrap(),
    )
    .volume;
    let want = std::f64::consts::PI * 6.0 * 3.0 * 2.0;
    assert!((v - want).abs() < 0.05, "{v} {want}");
    d.check_invariants().unwrap();
}

/// „Verknüpfung aufheben“ nimmt nur den Bezug: Die Kurve bleibt fest, mit ihren Fest-Werten.
#[test]
fn break_link_keeps_a_fixed_spline_fixed() {
    let (mut d, base, s, c, _) = body_with_fixed_spline();
    d.update(s, |k| {
        if let FeatureKind::Sketch(sf) = k {
            assert!(sf.break_link(c));
        }
    })
    .unwrap();
    let curve_of = |d: &Document| match &d.sketch_of(s).unwrap().curve(c).unwrap().kind {
        CurveKind::FixedSpline { curve, start, end } => (curve.clone(), *start, *end),
        k => panic!("kein fester Spline: {k:?}"),
    };
    let (before, start, end) = curve_of(&d);
    set_base(&mut d, base, spline_base(9.0));
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    assert!(f.warning().is_none(), "kein Bezug, also auch keine Warnung");
    let (after, ..) = curve_of(&d);
    assert_eq!(after, before, "folgt dem Körper nicht mehr");
    let sk = d.sketch_of(s).unwrap();
    for p in [start, end] {
        assert!(
            sk.constraints()
                .any(|(_, x)| matches!(x.kind, K::Fixed(o, ..) if o == p)),
            "der Fest-Wert bleibt"
        );
    }
    // Auch ohne Bezug bleibt er geschützt, und die Datei lädt.
    let mut copy = sk.clone();
    let row = copy
        .constraints()
        .find(|(_, x)| matches!(x.kind, K::Fixed(..)))
        .unwrap()
        .0;
    assert!(copy.remove_constraint(row).is_err());
    assert!(Sketch::from_data(copy.to_data()).is_ok());
}

// ---- Skizzen 7, Task 11: Schneiden am schrägen Kegel -------------------------------------------

/// Skizzen-Feature mit dem Schnitt der Kegelfläche als einziger verknüpfter Kurve.
fn cone_section_sketch(
    d: &mut Document,
    cone: FeatureId,
    frame: Frame,
    plane: SketchPlane,
) -> (FeatureId, CurveId) {
    let solid = body(d, cone);
    let face = find_face(&solid, |f| {
        matches!(solid.faces[f].surface, planora_kernel::Surface::Cone { .. })
    });
    let pieces = section_by_plane(&solid, &frame, Some(&[face]), &tol()).unwrap();
    assert_eq!(pieces.len(), 1, "{pieces:?}");
    let mut sk = Sketch::new(frame, "Skizze", tol());
    let p = &pieces[0];
    let curve = sk
        .add_projected_curve(&p.curve, p.t0, p.t1, LINKED)
        .unwrap();
    let face = FaceRef::from_face(&solid, face, &tol()).unwrap();
    let s = d
        .add_feature(
            "Skizze",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane,
                copies: vec![EdgeCopy {
                    curve,
                    source: CopySource::Section { face, index: 0 },
                }],
                formulas: Default::default(),
            }),
        )
        .unwrap();
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    assert!(f.warning().is_none(), "{:?}", f.warning());
    (s, curve)
}

fn set_cone_radius(d: &mut Document, cone: FeatureId, r: f64) {
    d.update(cone, |k| {
        if let FeatureKind::Cone { radius, .. } = k {
            *radius = r;
        }
    })
    .unwrap();
}

/// Die Skizzenkurve deckt sich mit dem frisch gerechneten Schnitt des Körpers.
fn assert_curve_matches_section(d: &Document, cone: FeatureId, s: FeatureId, c: CurveId) {
    let solid = body(d, cone);
    let sk = d.sketch_of(s).unwrap();
    let frame = *sk.frame();
    let face = find_face(&solid, |f| {
        matches!(solid.faces[f].surface, planora_kernel::Surface::Cone { .. })
    });
    let pieces = section_by_plane(&solid, &frame, Some(&[face]), &tol()).unwrap();
    let p = &pieces[0];
    for i in 0..=32 {
        let t = p.t0 + (p.t1 - p.t0) * i as f64 / 32.0;
        let q = frame.to_local(p.curve.point(t));
        assert!(q.z.abs() < 1e-7, "{q}");
        let q = q.truncate();
        let on = sk.nearest_on_curve(c, q).unwrap();
        assert!(on.distance(q) < 1e-5, "{q} gegen {on}");
    }
}

#[test]
fn oblique_cone_section_in_sketch_follows_the_body() {
    // Ellipse: Spitze bei z = 3, die Ebene geht durch die X-Achse und ist 30° gedreht.
    let mut d = Document::new();
    let cone = d
        .add_feature(
            "Kegel",
            FeatureKind::Cone {
                radius: 3.0,
                height: 10.0,
                origin: DVec3::new(0.0, 0.0, -7.0),
            },
        )
        .unwrap();
    let p = d
        .add_feature(
            "Ebene",
            FeatureKind::Plane(PlaneFeature {
                def: PlaneDef::Angle {
                    axis: AxisRef::Standard(StdAxis::X),
                    angle: 30.0,
                },
            }),
        )
        .unwrap();
    let frame = d.plane_frame(p).unwrap();
    let (s, c) = cone_section_sketch(&mut d, cone, frame, SketchPlane::OnPlane(p));
    assert!(matches!(
        d.sketch_of(s).unwrap().curve(c).unwrap().kind,
        CurveKind::Ellipse { .. }
    ));
    set_cone_radius(&mut d, cone, 4.0);
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    assert!(f.warning().is_none(), "{:?}", f.warning());
    assert_curve_matches_section(&d, cone, s, c);
    // Aus der Ellipse wird eine Hyperbel: Art geändert, das ist ein verlorener Bezug.
    set_cone_radius(&mut d, cone, 30.0);
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    assert!(f.warning().is_some(), "Warnung statt Fehler");
    assert!(
        matches!(
            d.sketch_of(s).unwrap().curve(c).unwrap().kind,
            CurveKind::Ellipse { .. }
        ),
        "die Kurve bleibt stehen"
    );
    assert_eq!(sketch_def(&d, s).copies.len(), 1, "der Verweis bleibt");
    d.check_invariants().unwrap();

    // Hyperbel: fester Spline, folgt dem Körper ohne Warnung.
    let mut d = Document::new();
    let cone = d
        .add_feature(
            "Kegel",
            FeatureKind::Cone {
                radius: 5.0,
                height: 10.0,
                origin: DVec3::new(0.0, 2.0, 0.0),
            },
        )
        .unwrap();
    let frame = StandardPlane::XZ.frame();
    let (s, c) = cone_section_sketch(
        &mut d,
        cone,
        frame,
        SketchPlane::Standard(StandardPlane::XZ),
    );
    let spline = |d: &Document| match &d.sketch_of(s).unwrap().curve(c).unwrap().kind {
        CurveKind::FixedSpline { curve, .. } => curve.clone(),
        k => panic!("kein fester Spline: {k:?}"),
    };
    let before = spline(&d);
    assert!(before.weights().iter().any(|w| *w != 1.0), "rational");
    assert_curve_matches_section(&d, cone, s, c);
    set_cone_radius(&mut d, cone, 7.0);
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    assert!(f.warning().is_none(), "{:?}", f.warning());
    assert_ne!(spline(&d), before, "die Kurve folgt");
    assert_curve_matches_section(&d, cone, s, c);
    d.check_invariants().unwrap();
}

/// Korrektur 1: Die Radiusänderung lässt die Hyperbel über die Naht wachsen. Die Kurve folgt dem
/// ganzen Schnitt und nicht nur einem Stück davon.
#[test]
fn cone_section_follows_across_the_seam() {
    let mut d = Document::new();
    let cone = d
        .add_feature(
            "Kegel",
            FeatureKind::Cone {
                radius: 2.9,
                height: 10.0,
                origin: DVec3::new(-3.0, 0.0, 0.0),
            },
        )
        .unwrap();
    // Ebene durch die z-Achse mit der Normalen (0,894; 0,447; 0).
    let p = d
        .add_feature(
            "Ebene",
            FeatureKind::Plane(PlaneFeature {
                def: PlaneDef::Angle {
                    axis: AxisRef::Standard(StdAxis::Z),
                    angle: -(2f64.atan().to_degrees()),
                },
            }),
        )
        .unwrap();
    let frame = d.plane_frame(p).unwrap();
    let wanted = DVec3::new(2.0, 1.0, 0.0).normalize();
    assert!(frame.z.cross(wanted).length() < 1e-9, "{}", frame.z);
    let (s, c) = cone_section_sketch(&mut d, cone, frame, SketchPlane::OnPlane(p));
    assert!(matches!(
        d.sketch_of(s).unwrap().curve(c).unwrap().kind,
        CurveKind::FixedSpline { .. }
    ));
    set_cone_radius(&mut d, cone, 5.0);
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    assert!(f.warning().is_none(), "{:?}", f.warning());
    let solid = body(&d, cone);
    let face = find_face(&solid, |f| {
        matches!(solid.faces[f].surface, planora_kernel::Surface::Cone { .. })
    });
    let pieces = section_by_plane(&solid, &frame, Some(&[face]), &tol()).unwrap();
    assert_eq!(pieces.len(), 1, "ein Stück über die Naht: {pieces:?}");
    assert_curve_matches_section(&d, cone, s, c);
    d.check_invariants().unwrap();
}

/// Korrektur 2: Ab einer großen Halbachse von 1e7 kommt die Ellipse als rationales Bézier-Stück.
/// Der Wechsel gilt als Artänderung: Warnung, Kurve bleibt, und zurück ist die Warnung wieder weg.
#[test]
fn form_switch_of_a_huge_ellipse_is_a_lost_reference() {
    let mut d = Document::new();
    let cone = d
        .add_feature(
            "Kegel",
            FeatureKind::Cone {
                radius: 4.9999999,
                height: 10.0,
                origin: DVec3::new(0.0, 0.0, -7.0),
            },
        )
        .unwrap();
    // Ebene durch die X-Achse, um atan 2 gedreht: fast parallel zu einer Mantellinie.
    let p = d
        .add_feature(
            "Ebene",
            FeatureKind::Plane(PlaneFeature {
                def: PlaneDef::Angle {
                    axis: AxisRef::Standard(StdAxis::X),
                    angle: 2f64.atan().to_degrees(),
                },
            }),
        )
        .unwrap();
    let frame = d.plane_frame(p).unwrap();
    let (s, c) = cone_section_sketch(&mut d, cone, frame, SketchPlane::OnPlane(p));
    let is_spline = |d: &Document| {
        matches!(
            d.sketch_of(s).unwrap().curve(c).unwrap().kind,
            CurveKind::FixedSpline { .. }
        )
    };
    assert!(is_spline(&d));
    assert_curve_matches_section(&d, cone, s, c);
    // Knapp unter der Grenze: `Curve::Ellipse`, die Kurve bleibt stehen.
    set_cone_radius(&mut d, cone, 4.999999);
    let solid = body(&d, cone);
    let face = find_face(&solid, |f| {
        matches!(solid.faces[f].surface, planora_kernel::Surface::Cone { .. })
    });
    let pieces = section_by_plane(&solid, &frame, Some(&[face]), &tol()).unwrap();
    assert!(
        matches!(pieces[0].curve, planora_kernel::Curve::Ellipse { .. }),
        "{pieces:?}"
    );
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    assert!(f.warning().is_some(), "Bezug verloren");
    assert!(is_spline(&d), "die Kurve bleibt stehen");
    // Zurück: Die Warnung ist weg, die Kurve folgt wieder.
    set_cone_radius(&mut d, cone, 4.9999999);
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    assert!(f.warning().is_none(), "{:?}", f.warning());
    assert!(is_spline(&d));
    assert_curve_matches_section(&d, cone, s, c);
    d.check_invariants().unwrap();
}

/// Korrektur 2: Schlanke Ellipsen gehen genau in die Skizze, und die Skizze bleibt lösbar. Bis zum
/// Verhältnis 1e4 als Ellipsenbogen, darüber als fester Spline: Der Löser meldet für einen
/// Ellipsenbogen ab etwa 1e5 `NotConverged`.
#[test]
fn slim_ellipses_keep_the_sketch_solvable() {
    let tilted = |r: f64, height: f64, h: f64, tilt: f64| {
        let phi = 1.1f64;
        let radial = DVec3::new(phi.cos(), phi.sin(), 0.0);
        let side = DVec3::Z.cross(radial);
        let along = (radial * (-r / height) + DVec3::Z).cross(side).normalize();
        let origin = DVec3::Z * height - along * (h * along.dot(radial).signum());
        Frame::from_origin_normal(origin, (along + DVec3::Z * tilt).normalize())
    };
    for (r, height, h, tilt) in [
        (5.0, 10.0, 1e-3, -1e-4),
        (5.0, 10.0, 1e-3, -1e-7),
        (5.0, 10.0, 1e-3, -1e-9),
        (5.0, 10.0, 1e-3, -1e-10),
        (100.0, 200.0, 1e-3, -1e-10),
        (1000.0, 2000.0, 1e-3, -1e-7),
        (1000.0, 2000.0, 1e-3, -1e-10),
        (1000.0, 2000.0, 1e-5, -1e-9),
        (1000.0, 2000.0, 1e-5, -1e-10),
        (1000.0, 2000.0, 1e-5, -1e-12),
    ] {
        let what = format!("r {r}, h {h}, Kippung {tilt}");
        let solid = planora_kernel::primitives::make_cone(r, height, &tol()).unwrap();
        let face = find_face(&solid, |f| {
            matches!(solid.faces[f].surface, planora_kernel::Surface::Cone { .. })
        });
        let frame = tilted(r, height, h, tilt);
        let pieces = section_by_plane(&solid, &frame, Some(&[face]), &tol()).unwrap();
        assert_eq!(pieces.len(), 1, "{what}: {pieces:?}");
        let p = &pieces[0];
        let mut sk = Sketch::new(frame, "Skizze", tol());
        let c = sk
            .add_projected_curve(&p.curve, p.t0, p.t1, LINKED)
            .unwrap();
        assert_eq!(
            sk.solve().status,
            planora_sketch::SolveStatus::Solved,
            "{what}"
        );
        for i in 0..=32 {
            let t = p.t0 + (p.t1 - p.t0) * i as f64 / 32.0;
            let q = frame.to_local(p.curve.point(t)).truncate();
            match p.curve {
                // Beim selben Parameter gleich. `nearest_on_curve` rechnet über `atan2` und wäre
                // an so schlanken Ellipsen zu grob.
                planora_kernel::Curve::Ellipse { major, minor, .. } => {
                    assert!(major / minor <= 1e4, "{what}: {major} / {minor}");
                    let on = sk.ellipse_point(c, t).unwrap();
                    assert!(on.distance(q) < 1e-7, "{what}: {q} gegen {on}");
                }
                _ => {
                    assert!(matches!(
                        sk.curve(c).unwrap().kind,
                        CurveKind::FixedSpline { .. }
                    ));
                    let on = sk.nearest_on_curve(c, q).unwrap();
                    assert!(on.distance(q) < 1e-5, "{what}: {q} gegen {on}");
                }
            }
        }
    }
}
