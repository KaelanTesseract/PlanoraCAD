//! STEP-Import, Aufgabe 10: Round-Trip aller eigenen Körper (Export → Import), Einheiten,
//! Handproben mit ihren Werten und Körper nach Rundungen und Booleschen Operationen.

mod common;

use common::bodies::{
    nurbs_sphere, poses, random_body, random_pose, spline_prism, torus_ring, vase, weighted_prism,
};
use common::{Lcg, export};
use planora_exchange::Progress;
use planora_kernel::boolean::BoolOp;
use planora_kernel::{
    AnalyticKernel, BlendEdge, BlendKind, BlendRequest, Curve, Edge, Face, Id, Kernel,
    NURBS_EDGE_TOL, Solid, Surface, UvRect, validate,
};
use planora_math::nurbs::{NurbsCurve, NurbsSurface};
use planora_math::{DMat4, DVec3, Frame, PI, TAU};
use planora_mesh::{
    Mesh, TessellationOptions, boolean_volume_plausible, mass_properties, tessellate,
};
use planora_step::read::topo::derive_patch_sensed;
use planora_step::{
    ImportOptions, ImportedBody, LengthUnit, NormalBody, ReadLimits, StepFile, StepImport,
    import_bodies,
};
use std::f64::consts::FRAC_PI_2;
use std::path::PathBuf;

/// Sehnentoleranz der Netze für Volumen, Oberfläche und Hülle.
const CHORD: f64 = 0.01;
/// Relative Schranke für Volumen und Oberfläche gegen das Original (beide über `tessellate`).
const MESH_REL: f64 = 1e-6;
/// Volumenfehler eines Netzes höchstens `SAG_FACTOR · CHORD · Oberfläche` (wie `stage2.rs`).
const SAG_FACTOR: f64 = 2.0;
/// Netzpunkte je Fläche für den Geometrievergleich.
const SAMPLES: usize = 60;
/// Mindestwert des Skalarprodukts zweier Außennormalen derselben Stelle.
const NORMAL_DOT: f64 = 0.99;
/// Ohne Normalenvergleich so nahe an einer Ecke (Spitze, Pol), in mm.
const NEAR_VERTEX: f64 = 1e-3;
/// Teilung der Kanten für den Vergleich (Punkte innen).
const EDGE_SAMPLES: usize = 4;
/// Schranke der Hülle (mm) bei rein analytischen Flächen.
const HULL_TOL: f64 = 1e-9;
/// Relative Schranke des Volumens der Handproben gegen die Rechnung.
const SAMPLE_REL: f64 = 1e-4;
/// Sehnentoleranz für das Volumen der Handproben.
const SAMPLE_CHORD: f64 = 0.001;
/// Spielraum der Volumenprobe nach dem Abziehen (relativ, wie im Machbarkeitstor).
const PLAUSIBLE_REL: f64 = 1e-3;
/// Startwert der Körper aus Booleschen Operationen (wie `stage3.rs`).
const SERIES_SEED: u64 = 20_261_014;
/// Körper aus Zufallsoperationen in Test (d).
const RANDOM_BODIES: usize = 6;
/// Kegel der kurzen Zufallsreihe in `cones_in_any_pose_come_back`.
const CONE_SERIES: usize = 12;
/// Punkte je Seite des Rands in `notch_at_pole`.
const NOTCH_STEPS: usize = 200;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, m: DMat4) -> Solid {
    k().transform(s, &m).unwrap()
}

fn data(name: &str) -> String {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "data", name]
        .iter()
        .collect();
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn import_with(text: &str, opts: &ImportOptions) -> StepImport {
    let file = StepFile::parse(
        text.as_bytes().to_vec(),
        &ReadLimits::DEFAULT,
        &Progress::new(),
    )
    .expect("Datei lädt");
    import_bodies(&file, opts, &Progress::new()).expect("Import")
}

fn import(text: &str) -> StepImport {
    import_with(text, &ImportOptions::default())
}

fn ok<'a>(b: &'a ImportedBody, what: &str) -> &'a NormalBody {
    b.outcome
        .as_ref()
        .unwrap_or_else(|e| panic!("{what}: „{}“ ausgelassen: {e}", b.name))
}

/// Netzkennzahlen: Volumen, Oberfläche, Hülle.
struct Measured {
    volume: f64,
    area: f64,
    lo: DVec3,
    hi: DVec3,
}

fn measured(s: &Solid, chord: f64) -> Result<(Measured, Mesh), String> {
    let mesh = tessellate(
        s,
        &TessellationOptions {
            chord_tolerance: chord,
        },
    )
    .map_err(|e| format!("Netz: {e}"))?;
    let p = mass_properties(&mesh);
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for q in &mesh.positions {
        lo = lo.min(*q);
        hi = hi.max(*q);
    }
    Ok((
        Measured {
            volume: p.volume,
            area: p.surface_area,
            lo,
            hi,
        },
        mesh,
    ))
}

/// Kante, deren beide Endpunkte `test` erfüllen (die erste, nach Id).
fn edge_where(s: &Solid, test: impl Fn(DVec3) -> bool) -> Id<Edge> {
    s.edges
        .iter()
        .find(|(_, e)| test(s.vertices[e.start].point) && test(s.vertices[e.end].point))
        .map(|(id, _)| id)
        .expect("Kante gefunden")
}

fn blend(s: &Solid, edges: &[Id<Edge>], kind: BlendKind) -> Result<Solid, String> {
    k().blend(
        s,
        &BlendRequest {
            edges: edges
                .iter()
                .map(|&edge| BlendEdge {
                    edge,
                    first_face: None,
                })
                .collect(),
            kind,
            tangent_chain: false,
        },
    )
    .map(|b| b.solid)
    .map_err(|e| format!("Rundung: {e}"))
}

/// Obere vordere Kante (y = 0, z = 10) eines Würfels 10³.
fn top_front(s: &Solid) -> Id<Edge> {
    edge_where(s, |p| p.y.abs() < 1e-9 && (p.z - 10.0).abs() < 1e-9)
}

/// Zylinder r = 3, h = 20 ab z = −10, dessen Mantel eine NURBS-Drehfläche ist (wie in
/// `stage3.rs`).
fn nurbs_cylinder() -> Solid {
    let (r, h) = (3.0, 20.0);
    let mut solid = k().make_cylinder(r, h).unwrap();
    let foot = DVec3::new(r, 0.0, 0.0);
    let line = NurbsCurve::new(
        1,
        vec![0.0, 0.0, 1.0, 1.0],
        vec![foot, foot + DVec3::Z * h],
        vec![1.0; 2],
    )
    .unwrap();
    let surface = Surface::nurbs(
        NurbsSurface::revolve(&line, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap(),
        true,
        false,
    )
    .unwrap();
    let id = solid
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Cylinder { .. }))
        .map(|(id, _)| id)
        .unwrap();
    solid.faces[id].surface = surface;
    solid.faces[id].patch = Some(UvRect {
        u0: 0.0,
        u1: TAU,
        v0: 0.0,
        v1: 1.0,
    });
    moved(&solid, DMat4::from_translation(DVec3::new(0.0, 0.0, -10.0)))
}

fn cube() -> Solid {
    k().make_box(DVec3::splat(10.0)).unwrap()
}

/// Alle Körper, die unser Schreiber ausgeben kann, mit „hat Freiformflächen“.
fn exportable() -> Vec<(&'static str, Solid, bool)> {
    let c = cube();
    let bore = k()
        .boolean(
            &c,
            &moved(
                &k().make_cylinder(2.0, 20.0).unwrap(),
                DMat4::from_translation(DVec3::new(5.0, 5.0, -5.0)),
            ),
            BoolOp::Difference,
        )
        .unwrap();
    let fillet = blend(&c, &[top_front(&c)], BlendKind::Fillet { radius: 2.0 }).unwrap();
    let chamfer = blend(
        &c,
        &[top_front(&c)],
        BlendKind::ChamferEqual { distance: 1.0 },
    )
    .unwrap();
    let crossing = k()
        .boolean(
            &nurbs_cylinder(),
            &moved(
                &k().make_cylinder(1.5, 20.0).unwrap(),
                DMat4::from_rotation_y(PI / 2.0)
                    * DMat4::from_translation(DVec3::new(0.0, 0.0, -10.0)),
            ),
            BoolOp::Union,
        )
        .unwrap();
    // Zylinder, schräg abgeschnitten: Ellipse als Schnittkante (wie in `curved.rs`).
    let slanted = k()
        .boolean(
            &k().make_cylinder(5.0, 40.0).unwrap(),
            &moved(
                &k().make_box(DVec3::splat(200.0)).unwrap(),
                DMat4::from_translation(DVec3::new(0.0, 0.0, 25.0))
                    * DMat4::from_rotation_x(0.5)
                    * DMat4::from_translation(DVec3::new(-100.0, -100.0, 0.0)),
            ),
            BoolOp::Difference,
        )
        .unwrap();
    let two = k()
        .boolean(
            &c,
            &moved(&c, DMat4::from_translation(DVec3::new(20.0, 0.0, 0.0))),
            BoolOp::Union,
        )
        .unwrap();
    vec![
        (
            "Quader",
            k().make_box(DVec3::new(10.0, 20.0, 30.0)).unwrap(),
            false,
        ),
        ("Zylinder", k().make_cylinder(4.0, 9.0).unwrap(), false),
        ("Kegel", k().make_cone(6.0, 9.0).unwrap(), false),
        ("Bohrung", bore, false),
        ("Rundung", fillet, false),
        ("Fase", chamfer, false),
        ("Kugel", k().make_sphere(6.0).unwrap(), false),
        ("Torus", torus_ring(), false),
        ("Vase", vase(), true),
        ("Spline-Prisma", spline_prism(), true),
        ("Gewichte-Prisma", weighted_prism(), true),
        ("NURBS-Kugel", nurbs_sphere(), true),
        ("Zylinderkreuz", crossing, true),
        ("Zwei Stücke", two, false),
        ("Schräg", slanted, false),
    ]
}

/// Punkte des Netzes je Fläche (Reihenfolge der Flächen im Körper), höchstens `SAMPLES` je
/// Fläche, gleichmäßig über die Dreiecksecken verteilt.
fn face_samples(s: &Solid, mesh: &Mesh) -> Vec<Vec<DVec3>> {
    let ids: Vec<_> = s.faces.iter().map(|(id, _)| id).collect();
    ids.iter()
        .map(|id| {
            let pts: Vec<DVec3> = mesh
                .face_ranges
                .iter()
                .filter(|r| r.face == *id)
                .flat_map(|r| {
                    let a = r.first_index as usize;
                    mesh.indices[a..a + r.index_count as usize]
                        .iter()
                        .map(|&i| mesh.positions[i as usize])
                })
                .collect();
            let step = pts.len().div_ceil(SAMPLES).max(1);
            pts.into_iter().step_by(step).collect()
        })
        .collect()
}

/// Ecken, die eine Kante benutzt (der Kernel lässt nach dem Bauen unbenutzte stehen).
fn used_vertices(s: &Solid) -> Vec<DVec3> {
    let mut ids: Vec<_> = s.edges.iter().flat_map(|(_, e)| [e.start, e.end]).collect();
    ids.sort_by_key(|id| id.index());
    ids.dedup();
    ids.into_iter().map(|id| s.vertices[id].point).collect()
}

/// Kanten, die eine Fläche benutzt, ohne Nähte.
fn used_edges(s: &Solid) -> Vec<&Edge> {
    let mut ids: Vec<_> = Vec::new();
    for (_, f) in s.faces.iter() {
        let mut own: Vec<_> = std::iter::once(&f.outer)
            .chain(&f.inner)
            .flat_map(|l| l.half_edges.iter().map(|h| h.edge))
            .collect();
        own.sort_by_key(|id| id.index());
        // Nähte (zweimal in derselben Fläche) sind keine Kanten der Gestalt: Ihre Lage hängt nur
        // von der Parametrisierung ab.
        let seams: Vec<_> = own
            .windows(2)
            .filter(|w| w[0] == w[1])
            .map(|w| w[0])
            .collect();
        ids.extend(own.into_iter().filter(|id| !seams.contains(id)));
    }
    ids.sort_by_key(|id| id.index());
    ids.dedup();
    ids.into_iter().map(|id| &s.edges[id]).collect()
}

/// Liegt `p` höchstens `tol` neben der Kante `e` (auf ihrer Kurve und im Bereich `t0`–`t1`)?
// Die negierten Vergleiche lehnen absichtlich auch NaN ab.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn on_edge(e: &Edge, p: DVec3, tol: f64) -> bool {
    let t = e.curve.project(p);
    if !(e.curve.point(t).distance(p) <= tol) {
        return false;
    }
    let (lo, hi) = (e.t0.min(e.t1), e.t0.max(e.t1));
    let t = match e.curve.period() {
        Some(per) => lo + (t - lo).rem_euclid(per),
        None => t,
    };
    let slack = 1e-9 * (1.0 + hi - lo);
    t >= lo - slack && t <= hi + slack
}

/// Außennormale der Fläche `f` am Fußpunkt von `p` und der Abstand von `p` zur Fläche.
fn foot(f: &Face, p: DVec3) -> (DVec3, f64) {
    let (u, v) = f.surface.project(p);
    let n = f.surface.normal(u, v) * if f.same_sense { 1.0 } else { -1.0 };
    (n, f.surface.point(u, v).distance(p))
}

/// Gleiche Geometrie unabhängig von der Parametrisierung: Die Netzpunkte jeder Fläche liegen
/// auf einer Fläche des anderen Körpers (höchstens `tol`), deren Außennormale dorthin zeigt,
/// wohin auch die eigene zeigt; die Kanten liegen auf Kanten, die Ecken stimmen (höchstens
/// `tol`) überein. Ob eine gekrümmte Fläche das richtige *Gebiet* ihrer Trägerfläche bedeckt,
/// prüft das nicht (Punkte liegen auf der unbegrenzten Fläche): Das deckt der Vergleich der
/// Netzvolumina in [`same_body`] (ein falsches Gebiet bei gleichen Kanten ändert das Volumen
/// grob).
fn same_geometry(a: &Solid, b: &Solid, mesh_a: &Mesh, tol: f64) -> Result<(), String> {
    if a.faces.len() != b.faces.len() {
        return Err(format!("{} Flächen statt {}", b.faces.len(), a.faces.len()));
    }
    same_faces(a, b, mesh_a, tol)?;
    same_edges_and_vertices(a, b, tol)
}

/// Die Netzpunkte jeder Fläche von `a` liegen auf einer Fläche von `b` mit gleicher Außenseite.
// Die negierten Vergleiche lehnen absichtlich auch NaN ab.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn same_faces(a: &Solid, b: &Solid, mesh_a: &Mesh, tol: f64) -> Result<(), String> {
    let fa: Vec<&Face> = a.faces.iter().map(|(_, f)| f).collect();
    let fb: Vec<&Face> = b.faces.iter().map(|(_, f)| f).collect();
    let corners = used_vertices(a);
    for (i, pts) in face_samples(a, mesh_a).iter().enumerate() {
        if pts.is_empty() {
            return Err(format!("Fläche {} ohne Netz", i + 1));
        }
        // Die Flächen müssen nicht in derselben Reihenfolge stehen (der Schreiber ordnet nach
        // Schalen): Jeder Punkt liegt auf irgendeiner Fläche des anderen Körpers, die ihn zur
        // selben Seite hin begrenzt.
        for &p in pts {
            let (na, _) = foot(fa[i], p);
            // An Spitzen und Polen (Ecken) gibt es keine Normale.
            let at_vertex = corners.iter().any(|v| v.distance(p) < NEAR_VERTEX);
            let mut nearest = f64::INFINITY;
            let found = fb.iter().any(|g| {
                let (nb, d) = foot(g, p);
                nearest = nearest.min(d);
                d <= tol && (at_vertex || na.dot(nb) > NORMAL_DOT)
            });
            if !found {
                return Err(format!(
                    "Fläche {}: Punkt {p} auf keiner Fläche mit derselben Außenseite \
                     (nächste {nearest:e} entfernt)",
                    i + 1
                ));
            }
        }
    }
    Ok(())
}

/// Kantenpunkte (ohne Nähte) liegen auf Kanten, die Ecken stimmen überein.
// Die negierten Vergleiche lehnen absichtlich auch NaN ab.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn same_edges_and_vertices(a: &Solid, b: &Solid, tol: f64) -> Result<(), String> {
    let edges_b = used_edges(b);
    for e in used_edges(a) {
        for k in 1..EDGE_SAMPLES {
            let t = e.t0 + (e.t1 - e.t0) * k as f64 / EDGE_SAMPLES as f64;
            let p = e.curve.point(t);
            if !edges_b.iter().any(|f| on_edge(f, p, tol)) {
                return Err(format!("Kantenpunkt {p} liegt auf keiner Kante"));
            }
        }
    }
    let (va, vb) = (used_vertices(a), used_vertices(b));
    if va.len() != vb.len() {
        return Err(format!("{} Ecken statt {}", vb.len(), va.len()));
    }
    for p in &va {
        let d = vb
            .iter()
            .map(|q| q.distance(*p))
            .fold(f64::INFINITY, f64::min);
        if !(d <= tol) {
            return Err(format!("Ecke {p}: nächste {d:e} entfernt"));
        }
    }
    Ok(())
}

/// Export → Import eines Körpers und Vergleich mit dem Original; `Err` mit Befund.
fn round_trip(name: &str, s: &Solid, freeform: bool) -> Result<(), String> {
    let (text, _) = export(&[(name, s)]);
    same_body(&text, s, freeform)
}

/// Liest `text` (ein Körper) und vergleicht ihn mit `s`; `Err` mit Befund.
fn same_body(text: &str, s: &Solid, freeform: bool) -> Result<(), String> {
    let imp = import(text);
    if imp.bodies.len() != 1 {
        return Err(format!("{} Körper statt 1", imp.bodies.len()));
    }
    let n = imp.bodies[0]
        .outcome
        .as_ref()
        .map_err(|e| format!("ausgelassen: {e}"))?;
    let want = validate(s).map_err(|e| format!("Original ungültig: {e}"))?;
    let got = validate(&n.solid).map_err(|e| format!("Import ungültig: {e}"))?;
    if (got.faces, got.edges, got.vertices) != (want.faces, want.edges, want.vertices) {
        return Err(format!("Zahlen {got:?} statt {want:?}"));
    }
    let tol = if freeform { NURBS_EDGE_TOL } else { HULL_TOL };
    let (ma, ka) = measured(s, CHORD)?;
    let (mb, kb) = measured(&n.solid, CHORD)?;
    same_geometry(s, &n.solid, &ka, tol)?;
    same_geometry(&n.solid, s, &kb, tol)?;
    // Netze hängen von der Parametrisierung ab (Lage der Naht, Bereich), die STEP nicht ganz
    // trägt: gleich, wo sie gleich ist; sonst höchstens ihr eigener Sehnenfehler.
    let rel = |x: f64, y: f64| ((x - y) / y).abs();
    let bound = (SAG_FACTOR * CHORD * ma.area / ma.volume).max(MESH_REL);
    if rel(mb.volume, ma.volume) > bound {
        return Err(format!("Volumen {} statt {}", mb.volume, ma.volume));
    }
    if rel(mb.area, ma.area) > bound {
        return Err(format!("Oberfläche {} statt {}", mb.area, ma.area));
    }
    let d = (mb.lo - ma.lo)
        .abs()
        .max((mb.hi - ma.hi).abs())
        .max_element();
    if d > SAG_FACTOR * CHORD {
        return Err(format!(
            "Hülle {}–{} statt {}–{} (Abstand {d:e})",
            mb.lo, mb.hi, ma.lo, ma.hi
        ));
    }
    Ok(())
}

#[test]
fn every_exportable_body_comes_back() {
    let mut failures = Vec::new();
    for (name, body, freeform) in exportable() {
        for (pose, m) in poses() {
            let s = moved(&body, m);
            if let Err(e) = round_trip(name, &s, freeform) {
                failures.push(format!("{name}, {pose}: {e}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Der dritte nicht leere Zufallskörper ab `Lcg(77)` (Prüfer-Befund P1 aus Aufgabe 9): Quader
/// minus Kugel; der Pol der Kugel liegt in der Taschenfläche, der Kernel führt eine Naht bis zum
/// Pol und zurück.
fn pocket_around_a_pole() -> Solid {
    let mut rng = Lcg(77);
    let mut found = Vec::new();
    while found.len() < 3 {
        found.extend(random_body(&mut rng));
    }
    found.pop().unwrap()
}

/// Kehrt den Sinn der Fläche auf der ersten Fläche vom Typ `kind` um (`.T.` ↔ `.F.`).
fn flip_face_on(text: &str, kind: &str) -> String {
    let id = text
        .lines()
        .find(|l| l.contains(&format!("={kind}(")))
        .and_then(|l| l.split('=').next())
        .expect("Fläche vorhanden")
        .to_string();
    text.lines()
        .map(|l| {
            if l.contains("=ADVANCED_FACE(") && l.contains(&format!(",{id},")) {
                if l.ends_with(".T.);") {
                    l.replace(&format!(",{id},.T.);"), &format!(",{id},.F.);"))
                } else {
                    l.replace(&format!(",{id},.F.);"), &format!(",{id},.T.);"))
                }
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// P1: Die Tasche kommt zurück; mit umgekehrter Kugelfläche wird der Körper weiter abgelehnt.
/// Das ist vorsichtig: Die umgekehrte Taschenfläche beschreibt gültig den vereinigten Körper
/// (Prüfer-Befund N1); der Leser kann ihn nur nicht sicher von einem Fehler unterscheiden.
#[test]
fn a_pocket_around_the_pole_of_its_sphere_comes_back() {
    let s = pocket_around_a_pole();
    assert!(
        s.faces
            .iter()
            .any(|(_, f)| matches!(f.surface, Surface::Sphere { .. })),
        "die Probe hat eine Kugelfläche"
    );
    round_trip("Tasche", &s, false).unwrap_or_else(|e| panic!("Tasche: {e}"));
    for (pose, m) in poses() {
        let p = moved(&s, m);
        round_trip("Tasche", &p, false).unwrap_or_else(|e| panic!("Tasche, {pose}: {e}"));
        let (text, _) = export(&[("Tasche", &p)]);
        let flipped = flip_face_on(&text, "SPHERICAL_SURFACE");
        assert_ne!(flipped, text);
        let imp = import(&flipped);
        assert!(
            imp.bodies[0].outcome.is_err(),
            "{pose}: umgekehrte Kugelfläche übernommen (vorsichtig abgelehnt erwartet; sie beschriebe gültig einen anderen Körper)"
        );
    }
}

/// Kegel in beliebiger Lage (Prüfer-Befund P2 aus Aufgabe 9; die ersten vier Lagen stammen aus
/// einer Zufallsreihe, in der sie abgelehnt wurden): An der Spitze entschied ein Rauschen von
/// 1e-9 in `u` über den Sprung am Pol. Dazu eine kurze Zufallsreihe; mit umgekehrter
/// Mantelfläche werden alle abgelehnt.
#[test]
fn cones_in_any_pose_come_back() {
    let found: [(f64, f64, [f64; 12]); 4] = [
        (
            5.01664767087986,
            6.198736003856614,
            [
                0.4270462129102846,
                -0.37202993886304025,
                -0.8241512340757287,
                0.5791336161669248,
                0.8125007595642435,
                -0.0666841085488028,
                0.6944319885088855,
                -0.44881648844164584,
                0.5624304161747523,
                3.869076353829559,
                -3.5551802475857404,
                7.232355337303794,
            ],
        ),
        (
            1.878378341889857,
            5.3943492723077195,
            [
                -0.14338340600673266,
                -0.8962463541507807,
                -0.41974238712970313,
                0.9221759425080679,
                0.032947625793607715,
                -0.3853647428267419,
                0.3592112608775858,
                -0.44233124084332714,
                0.8217726835522932,
                5.262421416522873,
                -1.4591027976574917,
                -8.349342534723782,
            ],
        ),
        (
            2.192239266725162,
            5.023850256147933,
            [
                -0.01051869411294538,
                0.9969323191037492,
                0.07755841798656442,
                0.06642525229540165,
                0.07808802589951244,
                -0.9947310923403416,
                -0.9977359585239226,
                -0.00531143460257432,
                -0.06704286487622714,
                6.098847936090962,
                9.2704867712429,
                -8.778286328322276,
            ],
        ),
        (
            3.1765906772339485,
            15.62494387327022,
            [
                0.15418113686702356,
                -0.09024234289269581,
                0.9839128500957944,
                0.47781304213697745,
                -0.8648230570534902,
                -0.15419395822295617,
                0.8648253429439954,
                0.4939001918787771,
                -0.09022043373819465,
                -1.7361873118568276,
                -6.751581149782211,
                -7.421278291175836,
            ],
        ),
    ];
    let mut cases: Vec<Solid> = found
        .iter()
        .map(|(r, h, m)| {
            let m = DMat4::from_cols_array(&[
                m[0], m[1], m[2], 0.0, m[3], m[4], m[5], 0.0, m[6], m[7], m[8], 0.0, m[9], m[10],
                m[11], 1.0,
            ]);
            moved(&k().make_cone(*r, *h).unwrap(), m)
        })
        .collect();
    let mut rng = Lcg(SERIES_SEED);
    for _ in 0..CONE_SERIES {
        let cone = k()
            .make_cone(rng.range(1.0, 10.0), rng.range(1.0, 20.0))
            .unwrap();
        cases.push(moved(&cone, random_pose(&mut rng)));
    }
    for (i, s) in cases.iter().enumerate() {
        round_trip("Kegel", s, false).unwrap_or_else(|e| panic!("Kegel {i}: {e}"));
        let (text, _) = export(&[("Kegel", s)]);
        let imp = import(&flip_face_on(&text, "CONICAL_SURFACE"));
        assert!(
            imp.bodies[0].outcome.is_err(),
            "Kegel {i}: umgekehrte Mantelfläche übernommen (vorsichtig abgelehnt erwartet)"
        );
    }
}

/// Zusatzeinträge für eine Zoll-Einheit (`#900` Maß, `#901` Dimension, `#902` Millimeter).
const INCH_ENTRIES: &str = "#900=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(25.4),#902);
#901=DIMENSIONAL_EXPONENTS(1.,0.,0.,0.,0.,0.,0.);
#902=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));
";

/// Quader 10 × 20 × 30 mit einer anderen Längeneinheit im Text.
fn box_in(unit: &str) -> String {
    let (text, _) = export(&[(
        "Quader",
        &k().make_box(DVec3::new(10.0, 20.0, 30.0)).unwrap(),
    )]);
    let mm = "(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));";
    assert_eq!(text.matches(mm).count(), 1, "eine Längeneinheit");
    let text = text.replacen(mm, unit, 1);
    let end = text.rfind("ENDSEC;").expect("Ende des Datenteils");
    format!("{}{INCH_ENTRIES}{}", &text[..end], &text[end..])
}

/// Hülle der Ecken des übernommenen Körpers.
fn corner_hull(b: &ImportedBody) -> (DVec3, DVec3) {
    let s = &ok(b, &b.name).solid;
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for p in used_vertices(s) {
        lo = lo.min(p);
        hi = hi.max(p);
    }
    (lo, hi)
}

#[test]
fn units_mm_m_inch() {
    let size = DVec3::new(10.0, 20.0, 30.0);
    let metre = box_in("(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT($,.METRE.));");
    let inch = box_in("(CONVERSION_BASED_UNIT('INCH',#900)LENGTH_UNIT()NAMED_UNIT(#901));");
    let mm = Some(LengthUnit::Millimeter);
    for (what, text, unit, want, factor) in [
        (
            "Millimeter",
            &box_in("(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));"),
            None,
            LengthUnit::Millimeter,
            1.0,
        ),
        ("Meter", &metre, None, LengthUnit::Meter, 1000.0),
        ("Zoll", &inch, None, LengthUnit::Zoll, 25.4),
        ("Meter → mm", &metre, mm, LengthUnit::Millimeter, 1.0),
        ("Zoll → mm", &inch, mm, LengthUnit::Millimeter, 1.0),
    ] {
        let imp = import_with(
            text,
            &ImportOptions {
                unit_override: unit,
            },
        );
        assert_eq!(imp.unit, want, "{what}");
        assert_eq!(imp.bodies.len(), 1, "{what}");
        let (lo, hi) = corner_hull(&imp.bodies[0]);
        assert!(lo.abs().max_element() <= 1e-9 * factor, "{what}: {lo}");
        let d = (hi - size * factor).abs().max_element();
        assert!(d <= 1e-9 * factor, "{what}: {hi} statt {}", size * factor);
    }
}

/// Volumen über das Netz, auf Sehne 0 hochgerechnet (der Netzfehler wächst linear mit der
/// Sehne: `2 · V(Sehne) − V(2 · Sehne)`).
fn sample_volume(s: &Solid) -> f64 {
    let v1 = measured(s, SAMPLE_CHORD).unwrap().0.volume;
    let v2 = measured(s, 2.0 * SAMPLE_CHORD).unwrap().0.volume;
    2.0 * v1 - v2
}

/// Simpson-Summe von `f` über `[a, b]` mit `n` (gerade) Schritten.
fn simpson(a: f64, b: f64, n: usize, f: impl Fn(f64) -> f64) -> f64 {
    let h = (b - a) / n as f64;
    let mut s = f(a) + f(b);
    for i in 1..n {
        s += if i % 2 == 1 { 4.0 } else { 2.0 } * f(a + h * i as f64);
    }
    s * h / 3.0
}

/// Fläche innerhalb der geschlossenen gleichförmigen kubischen Kurve des Flanschs
/// (`rational_bspline.step`, acht Kontrollpunkte aus der Datei, Knoten −3 … 11): acht Stücke
/// mit der Matrix der gleichförmigen kubischen B-Spline, Green: ∮ x dy.
fn flange_area() -> f64 {
    let ring = [
        (14.0, -14.0),
        (20.0, 0.0),
        (14.0, 14.0),
        (0.0, 20.0),
        (-14.0, 14.0),
        (-20.0, 0.0),
        (-14.0, -14.0),
        (0.0, -20.0),
    ];
    (0..8)
        .map(|j| {
            let p = |k: usize| ring[(j + k) % 8];
            // Gewichte der vier Punkte und ihre Ableitungen nach t.
            let w = |t: f64| {
                let s = 1.0 - t;
                [
                    s * s * s / 6.0,
                    (3.0 * t * t * t - 6.0 * t * t + 4.0) / 6.0,
                    (-3.0 * t * t * t + 3.0 * t * t + 3.0 * t + 1.0) / 6.0,
                    t * t * t / 6.0,
                ]
            };
            let dw = |t: f64| {
                let s = 1.0 - t;
                [
                    -s * s / 2.0,
                    (9.0 * t * t - 12.0 * t) / 6.0,
                    (-9.0 * t * t + 6.0 * t + 3.0) / 6.0,
                    t * t / 2.0,
                ]
            };
            simpson(0.0, 1.0, 1000, |t| {
                let (w, dw) = (w(t), dw(t));
                let x: f64 = (0..4).map(|k| w[k] * p(k).0).sum();
                let dy: f64 = (0..4).map(|k| dw[k] * p(k).1).sum();
                x * dy
            })
        })
        .sum()
}

/// Fläche zwischen der Kurve des Profils (`revolution_extrusion.step`) und x = 10.
fn profile_bulge_area() -> f64 {
    let points = [
        (10.0, 0.0),
        (13.0, 2.0),
        (11.0, 5.0),
        (14.0, 8.0),
        (10.0, 10.0),
    ]
    .map(|(x, y)| DVec3::new(x, y, 0.0));
    let c = NurbsCurve::new(
        3,
        vec![0.0, 0.0, 0.0, 0.0, 0.4, 1.0, 1.0, 1.0, 1.0],
        points.to_vec(),
        vec![1.0; 5],
    )
    .unwrap();
    let strip = |t: f64| {
        let d = c.derivs(t, 1);
        (d[0].x - 10.0) * d[1].y
    };
    simpson(0.0, 0.4, 4000, strip) + simpson(0.4, 1.0, 6000, strip)
}

/// Erwarteter Körper einer Handprobe: Name, Flächen, Volumen (mm³) oder Grund des Auslassens.
enum Want {
    Body(&'static str, usize, f64),
    Skipped(&'static str, &'static str),
}

/// Ein Kubikzoll in mm³.
const CUBIC_INCH: f64 = 25.4 * 25.4 * 25.4;

fn hand_samples() -> Vec<(&'static str, Vec<Want>)> {
    use Want::{Body, Skipped};
    let cap = 144.0 * PI;
    vec![
        ("box_ap214", vec![Body("Würfel", 6, 6000.0)]),
        ("bore_halves", vec![Body("Ring", 6, 11_250.0 * PI)]),
        ("bore_seam", vec![Body("Ring", 4, 11_250.0 * PI)]),
        (
            "rational_bspline",
            vec![
                Body("Viertelzylinder", 5, 25.0 * PI * 5.0),
                Body("Flansch", 3, flange_area() * 8.0),
            ],
        ),
        (
            "revolution_extrusion",
            vec![
                Body("Welle", 3, PI * 64.0 * 20.0),
                Body("Kegelstumpf", 3, PI * 10.0 / 3.0 * (100.0 + 50.0 + 25.0)),
                Body("Profil", 6, (100.0 + profile_bulge_area()) * 12.0),
            ],
        ),
        (
            "sphere_vertex_loop",
            vec![
                Body("Kugel", 1, 4.0 / 3.0 * PI * 1000.0),
                Body("Kuppel", 3, PI * 100.0 * 10.0 + 2.0 / 3.0 * PI * 1000.0),
            ],
        ),
        (
            "seamless",
            vec![
                Body("Zylinder", 3, PI * 25.0 * 10.0),
                Body("Kugel", 1, 4.0 / 3.0 * PI * 64.0),
                Skipped(
                    "Ring",
                    "Fläche vom Typ Torus ohne Naht wird noch nicht unterstützt.",
                ),
                Body("Kegel", 2, PI * 100.0 * 20.0 / 3.0),
                Body("Kappe", 2, cap),
                Body("Haube", 2, cap),
                Body("Schale", 2, cap),
            ],
        ),
        (
            "void_box",
            vec![
                Body("Hohlquader", 7, 8000.0 - 4.0 / 3.0 * PI * 125.0),
                Body("Zwei Quader", 12, 2000.0),
                Body("Ringhöhle", 7, 27_000.0 - 2.0 * PI * PI * 8.0 * 9.0),
            ],
        ),
        ("split_vertices", vec![Body("Quader", 6, 6000.0)]),
        (
            "inch_assembly",
            vec![
                Body("Block", 6, 6.0 * CUBIC_INCH),
                Body("Block (2)", 6, 6.0 * CUBIC_INCH),
                Body("Stift", 3, PI * 0.0625 * 2.0 * CUBIC_INCH),
            ],
        ),
        (
            "mapped_item",
            vec![Body("Teil", 6, 6000.0), Body("Teil (2)", 6, 6000.0)],
        ),
    ]
}

/// Meldung des Kernels, wenn er eine Ebene mit einer Fläche der Art `kind` nicht schneiden kann
/// (Kegel und Torus, außer in Sonderlagen).
fn plane_refusal(kind: &str) -> String {
    format!("Der Schnitt von {kind} mit Ebene wird noch nicht unterstützt")
}

/// Die Ablehnung `e` des Kernels ist „Ebene mit Kegel/Torus“, und `s` hat eine solche Fläche.
fn kernel_cannot_cut(s: &Solid, e: &str) -> bool {
    s.faces.iter().any(|(_, f)| match f.surface {
        Surface::Cone { .. } => e == plane_refusal("Kegel"),
        Surface::Torus { .. } => e == plane_refusal("Torus"),
        _ => false,
    })
}

/// Der Kernel lehnt das Abziehen des Stabs auch von seinem eigenen Kegelstumpf und Ring ab: Das
/// ist eine Grenze des Kernels, kein Befund des Lesers.
#[test]
fn the_kernel_refuses_plane_with_cone_or_torus_itself() {
    let frustum = k()
        .boolean(
            &k().make_cone(10.0, 20.0).unwrap(),
            &moved(
                &k().make_box(DVec3::splat(40.0)).unwrap(),
                DMat4::from_translation(DVec3::new(-20.0, -20.0, 10.0)),
            ),
            BoolOp::Difference,
        )
        .unwrap();
    let ring = moved(
        &torus_ring(),
        DMat4::from_translation(DVec3::new(30.0, 0.0, 0.0)) * DMat4::from_rotation_x(PI / 2.0),
    );
    for (s, kind) in [(&frustum, "Kegel"), (&ring, "Torus")] {
        let e = k()
            .boolean(s, &bar_through(s), BoolOp::Difference)
            .expect_err(kind);
        assert_eq!(e.to_string(), plane_refusal(kind));
    }
}

/// Stab 4 × 4 × 100 mm längs z durch die Mitte der Hülle der Ecken von `s`.
fn bar_through(s: &Solid) -> Solid {
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for p in used_vertices(s) {
        lo = lo.min(p);
        hi = hi.max(p);
    }
    let size = DVec3::new(4.0, 4.0, 100.0);
    moved(
        &k().make_box(size).unwrap(),
        DMat4::from_translation(0.5 * (lo + hi) - 0.5 * size),
    )
}

/// Lage der Handproben aus Baugruppen: Datei, Name, Hülle der Ecken (mm). Der Stift hat nur die
/// Anfangsecken seiner Kreise und fehlt darum hier.
const PLACED: [(&str, &str, [f64; 3], [f64; 3]); 4] = [
    (
        "inch_assembly",
        "Block",
        [0.0, 0.0, 0.0],
        [25.4, 50.8, 76.2],
    ),
    (
        "inch_assembly",
        "Block (2)",
        [50.8, 0.0, 0.0],
        [101.6, 25.4, 76.2],
    ),
    ("mapped_item", "Teil", [0.0, 0.0, 50.0], [10.0, 20.0, 80.0]),
    (
        "mapped_item",
        "Teil (2)",
        [0.0, 0.0, 0.0],
        [10.0, 20.0, 30.0],
    ),
];

#[test]
fn hand_samples_have_their_values() {
    for (file, name, lo, hi) in PLACED {
        let imp = import(&data(&format!("{file}.step")));
        let b = imp.bodies.iter().find(|b| b.name == name).expect(name);
        let (a, z) = corner_hull(b);
        let d = (a - DVec3::from(lo)).abs().max((z - DVec3::from(hi)).abs());
        assert!(d.max_element() <= 1e-9, "{file}: {name} liegt bei {a}–{z}");
    }
    for (file, want) in hand_samples() {
        let imp = import(&data(&format!("{file}.step")));
        assert_eq!(imp.bodies.len(), want.len(), "{file}: Körperzahl");
        for (b, w) in imp.bodies.iter().zip(&want) {
            match *w {
                Want::Skipped(name, why) => {
                    assert_eq!(b.name, name, "{file}");
                    assert_eq!(b.outcome.as_ref().err().map(String::as_str), Some(why));
                }
                Want::Body(name, faces, volume) => {
                    let what = format!("{file}: {name}");
                    assert_eq!(b.name, name, "{file}");
                    let s = &ok(b, &what).solid;
                    assert_eq!(b.faces, faces, "{what}: Flächen");
                    assert_eq!(s.faces.len(), faces, "{what}: Flächen im Körper");
                    let v = sample_volume(s);
                    assert!(
                        ((v - volume) / volume).abs() <= SAMPLE_REL,
                        "{what}: Volumen {v} statt {volume}"
                    );
                    let bar = bar_through(s);
                    let r = match k().boolean(s, &bar, BoolOp::Difference) {
                        Ok(r) => r,
                        // Ebene mit Kegel oder Torus rechnet der Kernel noch nicht (auch nicht
                        // bei seinen eigenen Körpern, siehe
                        // `the_kernel_refuses_plane_with_cone_or_torus_itself`).
                        Err(e) if kernel_cannot_cut(s, &e.to_string()) => continue,
                        Err(e) => panic!("{what}: Abziehen: {e}"),
                    };
                    validate(&r).unwrap_or_else(|e| panic!("{what}: Ergebnis ungültig: {e}"));
                    let vb = 4.0 * 4.0 * 100.0;
                    let vr = sample_volume(&r);
                    assert!(
                        boolean_volume_plausible(BoolOp::Difference, v, vb, vr, PLAUSIBLE_REL),
                        "{what}: {v} − {vb} → {vr}"
                    );
                }
            }
        }
    }
}

/// Obere vordere Kante und die beiden anderen Kanten an der Ecke (10, 0, 10) des Würfels 10³.
fn corner_edges(s: &Solid) -> Vec<Id<Edge>> {
    let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
    vec![
        edge_where(s, |p| near(p.y, 0.0) && near(p.z, 10.0)),
        edge_where(s, |p| near(p.x, 10.0) && near(p.z, 10.0)),
        edge_where(s, |p| near(p.x, 10.0) && near(p.y, 0.0)),
    ]
}

/// Körper wie in `stage3.rs`: Rundungen, Fase, Boolesche Operationen (auch mit Freiform-
/// Schnittkurve), dazu Zufallskörper und die Tasche um den Pol, je auch gespiegelt und gedreht.
fn combined_bodies() -> Vec<(String, Solid)> {
    let c = cube();
    let edges = corner_edges(&c);
    let centered = moved(
        &k().make_box(DVec3::new(20.0, 20.0, 10.0)).unwrap(),
        DMat4::from_translation(DVec3::new(-10.0, -10.0, -5.0)),
    );
    let cross = k()
        .boolean(
            &centered,
            &moved(&nurbs_cylinder(), DMat4::from_rotation_z(0.3)),
            BoolOp::Difference,
        )
        .unwrap();
    let dome = k()
        .boolean(
            &c,
            &moved(
                &k().make_sphere(4.0).unwrap(),
                DMat4::from_translation(DVec3::new(5.0, 5.0, 10.0)),
            ),
            BoolOp::Union,
        )
        .unwrap();
    let mut out: Vec<(String, Solid)> = vec![
        (
            "Rundung".into(),
            blend(&c, &edges[..1], BlendKind::Fillet { radius: 2.0 }).unwrap(),
        ),
        (
            "Ecke".into(),
            blend(&c, &edges, BlendKind::Fillet { radius: 2.0 }).unwrap(),
        ),
        (
            "Fase".into(),
            blend(&c, &edges[..1], BlendKind::ChamferEqual { distance: 1.5 }).unwrap(),
        ),
        (
            "Variabel".into(),
            blend(
                &c,
                &edges[..1],
                BlendKind::VariableFillet {
                    points: vec![(0.0, 1.0), (1.0, 2.5)],
                },
            )
            .unwrap(),
        ),
        ("Kreuz".into(), cross),
        ("Kuppel".into(), dome),
        ("Tasche".into(), pocket_around_a_pole()),
    ];
    let named = out.len();
    let mut rng = Lcg(SERIES_SEED);
    let mut i = 0;
    while out.len() < named + RANDOM_BODIES {
        if let Some(s) = random_body(&mut rng) {
            out.push((format!("Zufall {i}"), s));
        }
        i += 1;
    }
    let mirror = DMat4::from_rotation_z(0.6) * DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0));
    let mirrored: Vec<(String, Solid)> = out
        .iter()
        .map(|(n, s)| (format!("{n}, gespiegelt und gedreht"), moved(s, mirror)))
        .collect();
    out.extend(mirrored);
    out
}

/// Gerade Kanten zwischen zwei Ebenen, die längste zuerst.
fn straight_edges(s: &Solid) -> Vec<Id<Edge>> {
    let planes_at = |id: Id<Edge>| {
        s.faces
            .iter()
            .filter(|(_, f)| {
                std::iter::once(&f.outer)
                    .chain(&f.inner)
                    .any(|l| l.half_edges.iter().any(|h| h.edge == id))
            })
            .all(|(_, f)| matches!(f.surface, Surface::Plane { .. }))
    };
    let mut out: Vec<(Id<Edge>, f64)> = s
        .edges
        .iter()
        .filter(|(id, e)| matches!(e.curve, Curve::Line { .. }) && planes_at(*id))
        .map(|(id, e)| (id, (e.t1 - e.t0).abs()))
        .collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1));
    out.into_iter().map(|(id, _)| id).collect()
}

/// Die Kante von `s`, deren Mitte bei `p` liegt.
fn edge_at(s: &Solid, p: DVec3) -> Option<Id<Edge>> {
    s.edges
        .iter()
        .find(|(_, e)| e.curve.point(0.5 * (e.t0 + e.t1)).distance(p) < 1e-6)
        .map(|(id, _)| id)
}

/// Gelesen, dann an einer geraden Kante mit r 0,5 gerundet: Wo der Kernel die Kante am
/// Original rundet (die längste solche Kante), rundet er sie auch am gelesenen Körper.
#[test]
fn after_blends_and_booleans() {
    let fillet = BlendKind::Fillet { radius: 0.5 };
    let mut not_rounded = Vec::new();
    for (name, s) in combined_bodies() {
        let (text, _) = export(&[(&name, &s)]);
        let imp = import(&text);
        let n = &ok(&imp.bodies[0], &name).solid;
        let Some(edge) = straight_edges(n).into_iter().find(|&e| {
            let m = &n.edges[e];
            edge_at(&s, m.curve.point(0.5 * (m.t0 + m.t1)))
                .is_some_and(|o| blend(&s, &[o], fillet.clone()).is_ok())
        }) else {
            // Auch am Original rundet der Kernel keine gerade Kante (Grenze des Kernels).
            not_rounded.push(name);
            continue;
        };
        let r = blend(n, &[edge], fillet.clone()).unwrap_or_else(|e| panic!("{name}: {e}"));
        validate(&r).unwrap_or_else(|e| panic!("{name}: gerundet ungültig: {e}"));
    }
    assert!(
        not_rounded.iter().all(|n| n.starts_with("Zufall")) && not_rounded.len() <= RANDOM_BODIES,
        "nicht gerundet: {not_rounded:?}"
    );
}

/// Rand einer Kugelfläche durch den Nordpol mit einer Ausbuchtung dicht am Pol (Prüfer-Befund
/// I1): Keil u ∈ [0; 1] vom Äquator bis zum Pol, dazu am Pol bis zum Breitenkreis mit Polabstand
/// `d` ein Stück u ∈ [−2; 0]. Im Parameterraum (u, v):
/// (0, 0) → (0, lat) → (−2, lat) → (−2, Pol) → (1, Pol) → (1, 0) → (0, 0).
fn notch_at_pole(surface: &Surface, r: f64, d: f64) -> Vec<DVec3> {
    let lat = FRAC_PI_2 - d / r;
    let corners = [
        (0.0, 0.0),
        (0.0, lat),
        (-2.0, lat),
        (-2.0, FRAC_PI_2),
        (1.0, FRAC_PI_2),
        (1.0, 0.0),
    ];
    let mut out = Vec::new();
    for k in 0..corners.len() {
        let (a, b) = (corners[k], corners[(k + 1) % corners.len()]);
        for j in 0..NOTCH_STEPS {
            let t = j as f64 / NOTCH_STEPS as f64;
            out.push(surface.point(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
        }
    }
    out
}

/// Obergrenzen des Einrastens am Pol (`POLE_APPROACH_REL`, `POLE_APPROACH_SHARE`): Ein Stück des
/// Rands 1e-2 vom Pol einer Kugel r 5 (am Ursprung) bzw. 5 % der Schleife vom Pol einer Kugel r 1
/// (1 000 mm vom Ursprung, wo die relative Grenze 0,1 mm wäre) ist echte Form und bleibt im
/// Parameterbereich: u reicht über 3, nicht nur über 1.
#[test]
fn features_close_to_a_pole_keep_their_u_range() {
    for (r, c, d) in [
        (5.0, DVec3::ZERO, 1e-2),
        (1.0, DVec3::new(1000.0, 0.0, 0.0), 0.05),
    ] {
        let surface = Surface::Sphere {
            frame: Frame {
                origin: c,
                ..Frame::WORLD
            },
            radius: r,
        };
        let pts = notch_at_pole(&surface, r, d);
        let patch = derive_patch_sensed(&surface, false, &[pts], 1e-6)
            .unwrap_or_else(|e| panic!("r {r}, d {d}: {e:?}"));
        let span = patch.u1 - patch.u0;
        assert!((span - 3.0).abs() < 1e-9, "r {r}, d {d}: u-Bereich {span}");
    }
}

/// Der Parameterbereich geschlossener Flächen beginnt bei 0 (bzw. am Anfang des Bereichs der
/// Freiformfläche), auch wenn die Naht nur bis auf Rundung bei 0 liegt (Prüfer-Befund M1: eine
/// gedrehte Kugel kam mit u ab 2π − 4e-10 zurück).
#[test]
fn closed_faces_start_their_range_at_zero() {
    for (name, body) in [
        ("Kugel", k().make_sphere(6.0).unwrap()),
        ("NURBS-Kugel", nurbs_sphere()),
    ] {
        for (pose, m) in poses() {
            let (text, _) = export(&[(name, &moved(&body, m))]);
            let imp = import(&text);
            let n = ok(&imp.bodies[0], name);
            for (_, f) in n.solid.faces.iter() {
                if let (Some(per), Some(p)) = (f.surface.u_period(), f.patch) {
                    assert!(p.u0.abs() < 1e-8 * per, "{name}, {pose}: u ab {}", p.u0);
                }
            }
        }
    }
}

/// Ebene Kreise des Körpers (Bezug und Normale).
fn circle_planes(s: &Solid) -> Vec<(DVec3, DVec3)> {
    s.edges
        .iter()
        .filter_map(|(_, e)| match e.curve {
            Curve::Circle { frame, .. } => Some((frame.origin, frame.z)),
            _ => None,
        })
        .collect()
}

/// `b` beschnitten durch den Halbraum vor der Ebene (`origin`, `normal`).
fn clipped(b: &Solid, origin: DVec3, normal: DVec3) -> Option<Solid> {
    let x = normal.any_orthonormal_vector();
    let y = normal.cross(x);
    let m = DMat4::from_cols(
        x.extend(0.0),
        y.extend(0.0),
        normal.extend(0.0),
        origin.extend(1.0),
    ) * DMat4::from_translation(DVec3::new(-50.0, -50.0, 0.0));
    let half = moved(&k().make_box(DVec3::splat(100.0)).unwrap(), m);
    k().boolean(b, &half, BoolOp::Intersection).ok()
}

/// Volumen von `a op c` über Netze; lässt sich das Ergebnis nicht vernetzen, aus den Teilen
/// (Vereinigung: `V(a) + V(c) − V(a ∩ c)`, Differenz: `V(a) − V(a ∩ c)`).
fn op_volume(a: &Solid, c: &Solid, op: BoolOp, result: &Solid) -> Option<f64> {
    let v = |s: &Solid| measured(s, CHORD).ok().map(|m| m.0.volume);
    if let Some(x) = v(result) {
        return Some(x);
    }
    let both = k().boolean(a, c, BoolOp::Intersection).ok();
    let common = both.as_ref().and_then(v).unwrap_or(0.0);
    match op {
        BoolOp::Union => Some(v(a)? + v(c)? - common),
        BoolOp::Difference => Some(v(a)? - common),
        BoolOp::Intersection => None,
    }
}

/// `n` hat die Gestalt von `a op c`: Seine Flächenpunkte liegen auf Flächen des Ergebnisses mit
/// gleicher Außenseite, und die Volumina stimmen bis zum Netzfehler (die Zerlegung in Flächen
/// darf anders sein; Ergebnisse, die der Kernel nicht vernetzt, über ihre Teile).
fn has_shape_of(n: &Solid, a: &Solid, c: &Solid, op: BoolOp) -> bool {
    let Ok(result) = k().boolean(a, c, op) else {
        return false;
    };
    let Ok((m, mesh)) = measured(n, CHORD) else {
        return false;
    };
    same_faces(n, &result, &mesh, HULL_TOL).is_ok()
        && op_volume(a, c, op, &result)
            .is_some_and(|v| (v - m.volume).abs() <= SAG_FACTOR * CHORD * m.area)
}

/// Kehrt den Sinn der Kante `edge_line` (eine Zeile `#n=EDGE_CURVE(…)`) im Text um.
fn flip_edge_line(text: &str, line: &str) -> String {
    let flipped = if line.ends_with(".T.);") {
        line.replace(".T.);", ".F.);")
    } else {
        line.replace(".F.);", ".T.);")
    };
    text.replacen(line, &flipped, 1)
}

/// Jede Kante von `a op b` mit umgekehrtem Sinn (`EDGE_CURVE.same_sense`): Der Leser lehnt ab,
/// liest den Körper genau wie gebaut, oder die Datei beschreibt gültig einen anderen Körper,
/// nämlich `a op (b ∩ Halbraum)` an der Ebene eines Kreises (der Bogen läuft dann den langen Weg
/// um den Kreis; die ebene Fläche überdeckt die ganze Kreisscheibe, die Kugel verliert die Kappe
/// jenseits der Ebene – Prüfer-Befund I2, Aufgabe 10). Liefert die Zahl der Kanten und die der
/// gültigen anderen Körper; jeder andere Fall ist ein Fehler.
fn flipped_edges(a: &Solid, b: &Solid, op: BoolOp, what: &str) -> (usize, usize) {
    let s = k().boolean(a, b, op).unwrap();
    let (text, _) = export(&[("K", &s)]);
    let (mut edges, mut other) = (0, 0);
    for line in text.lines().filter(|l| l.contains("=EDGE_CURVE(")) {
        edges += 1;
        let t = flip_edge_line(&text, line);
        let imp = import(&t);
        let Ok(n) = &imp.bodies[0].outcome else {
            continue;
        };
        if same_body(&t, &s, false).is_ok() {
            continue;
        }
        let described = circle_planes(&n.solid).into_iter().any(|(o, z)| {
            [z, -z]
                .into_iter()
                .any(|z| clipped(b, o, z).is_some_and(|c| has_shape_of(&n.solid, a, &c, op)))
        });
        assert!(described, "{what}, {line}: anderer Körper übernommen");
        other += 1;
    }
    (edges, other)
}

/// Die Box-mit-Kugel-Reihe des Prüfers (`Lcg(777)`): Quader und Kugel in Zufallslage, verrechnet.
fn box_with_sphere(rng: &mut Lcg) -> (Solid, Solid, BoolOp) {
    let a = moved(
        &k().make_box(DVec3::new(
            rng.range(2.0, 8.0),
            rng.range(2.0, 8.0),
            rng.range(2.0, 8.0),
        ))
        .unwrap(),
        random_pose(rng),
    );
    let b = moved(
        &k().make_sphere(rng.range(1.0, 4.0)).unwrap(),
        random_pose(rng),
    );
    let op =
        [BoolOp::Union, BoolOp::Difference, BoolOp::Intersection][(rng.uniform() * 3.0) as usize];
    (a, b, op)
}

/// Die Fälle `cases` der Reihe (Index wie beim Prüfer, nur Paare, die der Kernel rechnet).
fn box_sphere_series(cases: &[usize], what: &str) -> (usize, usize) {
    let mut rng = Lcg(777);
    let (mut edges, mut other) = (0, 0);
    for i in 0..=*cases.iter().max().unwrap() {
        let (a, b, op) = box_with_sphere(&mut rng);
        if !cases.contains(&i) || k().boolean(&a, &b, op).is_err() {
            continue;
        }
        let (e, o) = flipped_edges(&a, &b, op, &format!("{what} {i}"));
        edges += e;
        other += o;
    }
    (edges, other)
}

/// Prüfer-Befund I2: In der Reihe sind 27 umgekehrte Kanten übernommen worden, die anders als
/// das Original sind. Jede beschreibt gültig `a op (b ∩ Halbraum)`; hier die Körper 2, 55 und
/// 219 („sph219“, Kante #56) der Reihe, die ganze Reihe nur im Release.
#[test]
fn flipped_circle_edges_describe_a_clipped_sphere() {
    let (edges, other) = box_sphere_series(&[2, 55, 219], "Reihe");
    assert!(edges > 0);
    assert_eq!(other, 4, "andere Körper in den Fällen 2, 55 (zwei) und 219");
}

#[test]
#[ignore = "lange Reihe, nur mit --release --ignored"]
fn flipped_circle_edges_in_the_whole_box_sphere_series() {
    let all: Vec<usize> = (0..250).collect();
    let (edges, other) = box_sphere_series(&all, "Reihe");
    println!("{edges} Kanten umgekehrt, {other} beschreiben einen beschnittenen Ball");
    assert_eq!(other, 27);
}

/// Von Hand: Würfel 10³ vereinigt mit einer Kugel r 3 um die Mitte der oberen rechten Kante;
/// auf Deckel und rechter Seite je ein Halbkreis. Jede Kante umgekehrt: abgelehnt, gleich oder
/// der Würfel mit der an einer Kreisebene beschnittenen Kugel.
#[test]
fn a_box_with_a_sphere_and_one_circle_edge_flipped() {
    let cube = cube();
    let ball = moved(
        &k().make_sphere(3.0).unwrap(),
        DMat4::from_translation(DVec3::new(10.0, 5.0, 10.0)),
    );
    let (edges, _) = flipped_edges(&cube, &ball, BoolOp::Union, "Würfel mit Kugel");
    assert!(edges >= 12, "{edges} Kanten");
}

/// Große Körper (Schlussprüfung M4: die Sehne der Seitenprobe wächst ab 100 mm Größe mit):
/// Die Tasche um den Pol und der Kegel ×100 und ×1000 kommen zurück, mit umgekehrter Kugel- bzw.
/// Mantelfläche werden sie weiter abgelehnt.
#[test]
fn large_bodies_keep_the_safety_net() {
    for scale in [100.0, 1000.0] {
        let grow = DMat4::from_scale(DVec3::splat(scale));
        for (name, s, kind) in [
            (
                "Tasche",
                moved(&pocket_around_a_pole(), grow),
                "SPHERICAL_SURFACE",
            ),
            (
                "Kegel",
                moved(&k().make_cone(6.0, 9.0).unwrap(), grow),
                "CONICAL_SURFACE",
            ),
        ] {
            round_trip(name, &s, false).unwrap_or_else(|e| panic!("{name} ×{scale}: {e}"));
            let (text, _) = export(&[(name, &s)]);
            let imp = import(&flip_face_on(&text, kind));
            assert!(
                imp.bodies[0].outcome.is_err(),
                "{name} ×{scale}: umgekehrte Fläche übernommen"
            );
        }
    }
}

/// Schlussprüfung M6: Gründe zu kaputten Einheiten in einfachen Worten (der Typname höchstens in
/// Klammern dahinter).
#[test]
fn broken_units_are_explained_in_plain_words() {
    for (unit, want) in [
        (
            "(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.));",
            "Eine Einheit ist unvollständig angegeben (SI_UNIT).",
        ),
        (
            "(CONVERSION_BASED_UNIT()LENGTH_UNIT()NAMED_UNIT(#901));",
            "Bei einer umgerechneten Einheit fehlt der Umrechnungswert (CONVERSION_BASED_UNIT).",
        ),
    ] {
        let imp = import(&box_in(unit));
        let e = imp.bodies[0].outcome.as_ref().expect_err(unit);
        assert!(e.ends_with(want), "{unit}: {e}");
    }
}
