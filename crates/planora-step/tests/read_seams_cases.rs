//! STEP-Leser, Nähte (STEP-Import, Aufgabe 6, Nacharbeit): nahtlose Zylinder, wie Exporteure
//! sie schreiben, mit Fenstern im Mantel, Laschen am Ende, schrägem Deckel, Ecken an beliebigen
//! Winkeln und Schleifen, die an beliebiger Ecke beginnen. Die Texte erzeugt der Test; jeder
//! Körper hat das Volumen π·25·10 (Fenster und Laschen sind mit Flächen derselben Zylinderfläche
//! gefüllt, der schräge Deckel hat die mittlere Höhe 10).

mod common;

use common::Lcg;
use planora_exchange::Progress;
use planora_kernel::{Solid, validate};
use planora_math::{DVec3, PI, TAU};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use planora_step::read::geom::Skip;
use planora_step::read::topo::{BuildOptions, build_solid};
use planora_step::read::units::context_units;
use planora_step::{ReadLimits, StepFile};

const R: f64 = 5.0;
const H: f64 = 10.0;

/// Kleiner Schreiber für STEP-Einträge (Kennungen fortlaufend).
struct Step {
    lines: Vec<String>,
    next: u64,
}

impl Step {
    fn new() -> Step {
        Step {
            lines: Vec::new(),
            next: 1000,
        }
    }

    fn push(&mut self, body: String) -> u64 {
        let id = self.next;
        self.next += 1;
        self.lines.push(format!("#{id}={body};"));
        id
    }

    fn point(&mut self, p: DVec3) -> u64 {
        self.push(format!(
            "CARTESIAN_POINT('',({:.17e},{:.17e},{:.17e}))",
            p.x, p.y, p.z
        ))
    }

    fn dir(&mut self, d: DVec3) -> u64 {
        self.push(format!(
            "DIRECTION('',({:.17e},{:.17e},{:.17e}))",
            d.x, d.y, d.z
        ))
    }

    fn axis2(&mut self, o: DVec3, z: DVec3, x: DVec3) -> u64 {
        let (o, z, x) = (self.point(o), self.dir(z), self.dir(x));
        self.push(format!("AXIS2_PLACEMENT_3D('',#{o},#{z},#{x})"))
    }

    fn vertex(&mut self, p: DVec3) -> u64 {
        let p = self.point(p);
        self.push(format!("VERTEX_POINT('',#{p})"))
    }

    fn line(&mut self, a: DVec3, b: DVec3) -> u64 {
        let o = self.point(a);
        let d = self.dir((b - a).normalize());
        let v = self.push(format!("VECTOR('',#{d},1.)"));
        self.push(format!("LINE('',#{o},#{v})"))
    }

    fn edge(&mut self, a: u64, b: u64, curve: u64, sense: bool) -> u64 {
        self.push(format!("EDGE_CURVE('',#{a},#{b},#{curve},{})", flag(sense)))
    }

    /// Schleife aus (Kante, vorwärts) als Rand; `outer`: `FACE_OUTER_BOUND`.
    fn bound(&mut self, hes: &[(u64, bool)], outer: bool) -> u64 {
        let oes: Vec<String> = hes
            .iter()
            .map(|(e, f)| {
                let id = self.push(format!("ORIENTED_EDGE('',*,*,#{e},{})", flag(*f)));
                format!("#{id}")
            })
            .collect();
        let lp = self.push(format!("EDGE_LOOP('',({}))", oes.join(",")));
        let kind = if outer {
            "FACE_OUTER_BOUND"
        } else {
            "FACE_BOUND"
        };
        self.push(format!("{kind}('',#{lp},.T.)"))
    }

    fn face(&mut self, bounds: &[u64], surface: u64, sense: bool) -> u64 {
        let b: Vec<String> = bounds.iter().map(|b| format!("#{b}")).collect();
        self.push(format!(
            "ADVANCED_FACE('',({}),#{surface},{})",
            b.join(","),
            flag(sense)
        ))
    }

    /// Ganze Datei mit einem Körper aus den Flächen `faces`.
    fn file(mut self, faces: &[u64]) -> String {
        let f: Vec<String> = faces.iter().map(|f| format!("#{f}")).collect();
        let shell = self.push(format!("CLOSED_SHELL('',({}))", f.join(",")));
        let body = self.push(format!("MANIFOLD_SOLID_BREP('Teil',#{shell})"));
        format!(
            "ISO-10303-21;\n/* Selbst geschrieben für die Tests von PlanoraCAD (GPL-3.0-only). */\n\
             HEADER;\nFILE_DESCRIPTION((''),'2;1');\n\
             FILE_NAME('x.step','2026-10-21T00:00:00',(''),(''),'','','');\n\
             FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\nENDSEC;\nDATA;\n\
             #9=ADVANCED_BREP_SHAPE_REPRESENTATION('',(#{body}),#105);\n\
             #100=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));\n\
             #101=(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.));\n\
             #103=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-06),#100,'distance_accuracy_value','');\n\
             #105=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#103))\
             GLOBAL_UNIT_ASSIGNED_CONTEXT((#100,#101))REPRESENTATION_CONTEXT('',''));\n\
             {}\nENDSEC;\nEND-ISO-10303-21;\n",
            self.lines.join("\n")
        )
    }
}

fn flag(b: bool) -> &'static str {
    if b { ".T." } else { ".F." }
}

fn on_cyl(u: f64, z: f64) -> DVec3 {
    DVec3::new(R * u.cos(), R * u.sin(), z)
}

/// Bauplan eines nahtlosen Zylinders R 5, h 10 um z.
#[derive(Clone, Debug)]
struct Case {
    /// Winkel der Ecken des unteren Kreises (aufsteigend in [0, 2π)).
    bottom: Vec<f64>,
    /// Winkel der Ecken des oberen Kreises bzw. Parameter auf der Ellipse.
    top: Vec<f64>,
    /// Schräger Deckel: z = 10 + 0,2 · x (oberer Rand eine Ellipse).
    oblique: bool,
    /// Fenster im Mantel zwischen den Winkeln (u0, u1), z 4…7, mit einer Fläche gefüllt.
    window: Option<(f64, f64)>,
    /// Lasche am unteren Ende zwischen den Winkeln (a, b), z 0…3, als eigene Fläche; a und b
    /// müssen Ecken des unteren Kreises sein.
    tab: Option<(f64, f64)>,
    /// Drehung des Anfangs der unteren bzw. oberen Mantelschleife.
    rot: (usize, usize),
    /// Reihenfolge der Ränder des Mantels: oberer Rand zuerst.
    top_first: bool,
}

/// Punkt des oberen Rands beim Winkel u (schräg: auf der Ebene z = 10 + 0,2 x).
fn top_point(case: &Case, u: f64) -> DVec3 {
    let p = on_cyl(u, H);
    if case.oblique {
        DVec3::new(p.x, p.y, H + 0.2 * p.x)
    } else {
        p
    }
}

fn rotate<T: Clone>(v: &[T], k: usize) -> Vec<T> {
    let k = k % v.len();
    v[k..].iter().chain(&v[..k]).cloned().collect()
}

fn build(case: &Case) -> String {
    let mut s = Step::new();
    let zaxis = DVec3::Z;
    // Unterer Kreis.
    let a0 = s.axis2(DVec3::ZERO, zaxis, DVec3::X);
    let c0 = s.push(format!("CIRCLE('',#{a0},{R:.1})"));
    let bv: Vec<u64> = case
        .bottom
        .iter()
        .map(|u| s.vertex(on_cyl(*u, 0.0)))
        .collect();
    let n = bv.len();
    let barcs: Vec<u64> = (0..n)
        .map(|i| s.edge(bv[i], bv[(i + 1) % n], c0, true))
        .collect();
    // Oberer Rand: Kreis oder Ellipse.
    let top_curve = if case.oblique {
        let n = DVec3::new(-0.2, 0.0, 1.0).normalize();
        let x = DVec3::new(1.0, 0.0, 0.2).normalize();
        let a = s.axis2(DVec3::new(0.0, 0.0, H), n, x);
        s.push(format!(
            "ELLIPSE('',#{a},{:.17e},{R:.1})",
            R * (1.04f64).sqrt()
        ))
    } else {
        let a = s.axis2(DVec3::new(0.0, 0.0, H), zaxis, DVec3::X);
        s.push(format!("CIRCLE('',#{a},{R:.1})"))
    };
    // Ellipse: Parameter t gehört zum Winkel u = t (x = R cos t, y = R sin t).
    let tv: Vec<u64> = case
        .top
        .iter()
        .map(|u| s.vertex(top_point(case, *u)))
        .collect();
    let m = tv.len();
    let tarcs: Vec<u64> = (0..m)
        .map(|i| s.edge(tv[i], tv[(i + 1) % m], top_curve, true))
        .collect();
    let mut faces = Vec::new();
    // Boden (Normale −z) und Deckel.
    let bottom_plane = s.push(format!("PLANE('',#{a0})"));
    let hes: Vec<(u64, bool)> = barcs.iter().rev().map(|e| (*e, false)).collect();
    let b = s.bound(&hes, true);
    faces.push(s.face(&[b], bottom_plane, false));
    let lid_axis = if case.oblique {
        s.axis2(
            DVec3::new(0.0, 0.0, H),
            DVec3::new(-0.2, 0.0, 1.0).normalize(),
            DVec3::new(1.0, 0.0, 0.2).normalize(),
        )
    } else {
        s.axis2(DVec3::new(0.0, 0.0, H), zaxis, DVec3::X)
    };
    let lid_plane = s.push(format!("PLANE('',#{lid_axis})"));
    let hes: Vec<(u64, bool)> = tarcs.iter().map(|e| (*e, true)).collect();
    let b = s.bound(&hes, true);
    faces.push(s.face(&[b], lid_plane, true));
    let cyl = s.push(format!("CYLINDRICAL_SURFACE('',#{a0},{R:.1})"));
    // Untere Mantelschleife (läuft +u): Kreisbögen, an der Lasche über ihre Oberkante.
    let mut lower: Vec<(u64, bool)> = Vec::new();
    if let Some((ta, tb)) = case.tab {
        let ia = case.bottom.iter().position(|u| *u == ta).expect("Ecke a");
        let ib = case.bottom.iter().position(|u| *u == tb).expect("Ecke b");
        let up_a = s.vertex(on_cyl(ta, 3.0));
        let up_b = s.vertex(on_cyl(tb, 3.0));
        let la = s.line(on_cyl(ta, 0.0), on_cyl(ta, 3.0));
        let lb = s.line(on_cyl(tb, 0.0), on_cyl(tb, 3.0));
        let ga = s.edge(bv[ia], up_a, la, true);
        let gb = s.edge(bv[ib], up_b, lb, true);
        let a3 = s.axis2(DVec3::new(0.0, 0.0, 3.0), zaxis, DVec3::X);
        let c3 = s.push(format!("CIRCLE('',#{a3},{R:.1})"));
        let top_arc = s.edge(up_a, up_b, c3, true);
        // Bögen des Bodens zwischen a und b gehören zur Lasche.
        let mut tab_arcs = Vec::new();
        let mut i = ia;
        while i != ib {
            tab_arcs.push((barcs[i], true));
            i = (i + 1) % n;
        }
        let mut tab_loop = tab_arcs;
        tab_loop.extend([(gb, true), (top_arc, false), (ga, false)]);
        let b = s.bound(&tab_loop, true);
        faces.push(s.face(&[b], cyl, true));
        let mut i = ib;
        while i != ia {
            lower.push((barcs[i], true));
            i = (i + 1) % n;
        }
        lower.extend([(ga, true), (top_arc, true), (gb, false)]);
    } else {
        lower = barcs.iter().map(|e| (*e, true)).collect();
    }
    let lower = rotate(&lower, case.rot.0);
    let upper: Vec<(u64, bool)> = tarcs.iter().rev().map(|e| (*e, false)).collect();
    let upper = rotate(&upper, case.rot.1);
    let mut bounds = Vec::new();
    let lb = s.bound(&lower, false);
    let ub = s.bound(&upper, false);
    if case.top_first {
        bounds.extend([ub, lb]);
    } else {
        bounds.extend([lb, ub]);
    }
    // Fenster: Loch im Mantel, mit einem Stück derselben Fläche gefüllt.
    if let Some((u0, u1)) = case.window {
        let w00 = s.vertex(on_cyl(u0, 4.0));
        let w10 = s.vertex(on_cyl(u1, 4.0));
        let w01 = s.vertex(on_cyl(u0, 7.0));
        let w11 = s.vertex(on_cyl(u1, 7.0));
        let a3 = s.axis2(DVec3::new(0.0, 0.0, 4.0), zaxis, DVec3::X);
        let c3 = s.push(format!("CIRCLE('',#{a3},{R:.1})"));
        let a7 = s.axis2(DVec3::new(0.0, 0.0, 7.0), zaxis, DVec3::X);
        let c7 = s.push(format!("CIRCLE('',#{a7},{R:.1})"));
        let lo = s.edge(w00, w10, c3, true);
        let hi = s.edge(w01, w11, c7, true);
        let l0 = s.line(on_cyl(u0, 4.0), on_cyl(u0, 7.0));
        let l1 = s.line(on_cyl(u1, 4.0), on_cyl(u1, 7.0));
        let g0 = s.edge(w00, w01, l0, true);
        let g1 = s.edge(w10, w11, l1, true);
        let fill = s.bound(&[(lo, true), (g1, true), (hi, false), (g0, false)], true);
        faces.push(s.face(&[fill], cyl, true));
        bounds.push(s.bound(&[(g0, true), (hi, true), (g1, false), (lo, false)], false));
    }
    faces.push(s.face(&bounds, cyl, true));
    s.file(&faces)
}

fn read(text: &str) -> Result<Solid, Skip> {
    let file = StepFile::parse(
        text.as_bytes().to_vec(),
        &ReadLimits::DEFAULT,
        &Progress::new(),
    )
    .expect("Datei lädt");
    let rep = file.ids_of("ADVANCED_BREP_SHAPE_REPRESENTATION")[0];
    let e = file.entity(rep).expect("Darstellung");
    let ctx = e
        .arg("ADVANCED_BREP_SHAPE_REPRESENTATION", 2)
        .expect("Kontext")
        .reference()
        .expect("Verweis");
    let units = context_units(&file, ctx).expect("Einheiten");
    let items = file.ids_of("MANIFOLD_SOLID_BREP").to_vec();
    build_solid(
        &file,
        units,
        &items,
        &BuildOptions {
            weld_tol_mm: units.weld_tol_mm(),
        },
        &Progress::new(),
    )
    .map(|b| b.solid)
}

/// Liest den Fall und prüft `validate` und das Volumen π·25·10 (Netz, Sehne 0,002: auf etwa
/// 5e-4 genau; eine Naht auf vorhandenem Rand verschiebt es um 0,5 % und mehr).
fn check(case: &Case) {
    let text = build(case);
    let s = read(&text).unwrap_or_else(|Skip(m)| panic!("{case:?} ausgelassen: {m}"));
    validate(&s).unwrap_or_else(|e| panic!("{case:?} ungültig: {e}"));
    let mesh = tessellate(
        &s,
        &TessellationOptions {
            chord_tolerance: 0.002,
        },
    )
    .expect("Netz");
    let v = mass_properties(&mesh).volume;
    let want = PI * R * R * H;
    assert!(
        ((v - want) / want).abs() <= 1.5e-3,
        "{case:?}: Volumen {v} statt {want}"
    );
}

fn plain(bottom: &[f64], top: &[f64]) -> Case {
    Case {
        bottom: bottom.to_vec(),
        top: top.to_vec(),
        oblique: false,
        window: None,
        tab: None,
        rot: (0, 0),
        top_first: false,
    }
}

#[test]
fn both_circles_with_a_vertex_at_the_same_angle_keep_their_vertices() {
    // Der Normalfall vieler Exporteure: beide Kreise mit einer Ecke bei u = 0. Die Naht verbindet
    // die beiden Ecken, kein Kreis wird geteilt: 2 Kreise + Naht.
    let case = plain(&[0.0], &[0.0]);
    let s = read(&build(&case)).unwrap_or_else(|Skip(m)| panic!("{m}"));
    assert_eq!(s.edges.len(), 3);
    assert_eq!(s.vertices.len(), 2);
    check(&case);
}

#[test]
fn window_at_the_first_vertex_does_not_take_the_seam() {
    // Fenster, dessen Seite genau beim Winkel der ersten Ecke liegt (C1a), und eins über diesen
    // Winkel hinweg (I1), nah und fern der ersten Ecke.
    for (bottom, window) in [
        (vec![0.0], (0.0, 1.0)),
        (vec![0.0], (-1.0, 0.0)),
        (vec![1.0], (1.0, 2.5)),
        (vec![0.0], (-0.3, 0.3)),
        (vec![2.0, 4.0], (1.7, 2.3)),
        (vec![0.0], (0.2, 6.0)),
        (vec![0.0], (1.0, 2.0)),
    ] {
        for top in [vec![0.0], vec![1.0], vec![3.0, 5.0]] {
            for top_first in [false, true] {
                check(&Case {
                    window: Some(window),
                    top_first,
                    ..plain(&bottom, &top)
                });
            }
        }
    }
}

#[test]
fn tab_at_the_end_does_not_take_the_seam() {
    // Lasche am unteren Ende (C1b, I2): die untere Schleife hat Ecken an den Mantellinien der
    // Lasche und beginnt an jeder ihrer Kanten einmal; der obere Rand kreisförmig oder schräg.
    for (bottom, tab) in [
        (vec![0.0, 1.0], (0.0, 1.0)),
        (vec![0.5, 2.0, 4.0], (2.0, 4.0)),
        (vec![1.0, 5.5], (5.5, 1.0)),
    ] {
        for oblique in [false, true] {
            for top in [vec![0.0], vec![2.0]] {
                let lower_len = bottom.len() + 3 - {
                    let (ta, tb) = tab;
                    let ia = bottom.iter().position(|u| *u == ta).unwrap();
                    let ib = bottom.iter().position(|u| *u == tb).unwrap();
                    (ib + bottom.len() - ia) % bottom.len()
                };
                for r in 0..lower_len {
                    for top_first in [false, true] {
                        check(&Case {
                            tab: Some(tab),
                            oblique,
                            rot: (r, 0),
                            top_first,
                            ..plain(&bottom, &top)
                        });
                    }
                }
            }
        }
    }
}

#[test]
fn oblique_lid_is_cut_by_the_seam() {
    // Schräger Deckel: der obere Rand ist eine Ellipse; die Naht schneidet sie bei u_s.
    for top in [vec![0.0], vec![1.0], vec![2.0, 4.0]] {
        for bottom in [vec![0.0], vec![3.0]] {
            check(&Case {
                oblique: true,
                ..plain(&bottom, &top)
            });
        }
    }
}

#[test]
fn random_seamless_cylinders() {
    let mut rng = Lcg(0x5eed_6006);
    for _ in 0..24 {
        let angles = |rng: &mut Lcg, k: usize| -> Vec<f64> {
            let mut v: Vec<f64> = (0..k).map(|_| rng.range(0.0, TAU)).collect();
            v.sort_by(f64::total_cmp);
            v
        };
        let nb = 1 + (rng.uniform() * 3.0) as usize;
        let nt = 1 + (rng.uniform() * 3.0) as usize;
        let mut bottom = angles(&mut rng, nb.max(2));
        let top = angles(&mut rng, nt);
        let tab = (rng.uniform() < 0.5).then(|| (bottom[0], bottom[1]));
        if tab.is_none() {
            bottom.truncate(nb);
        }
        let window = (rng.uniform() < 0.5).then(|| {
            let u0 = if rng.uniform() < 0.5 {
                bottom[0]
            } else {
                rng.range(-1.0, 6.0)
            };
            (u0, u0 + rng.range(0.3, 2.0))
        });
        let case = Case {
            oblique: rng.uniform() < 0.3,
            rot: (
                (rng.uniform() * 4.0) as usize,
                (rng.uniform() * 3.0) as usize,
            ),
            top_first: rng.uniform() < 0.5,
            window,
            tab,
            ..plain(&bottom, &top)
        };
        check(&case);
    }
}

/// Spitzer Kegel R 5, h 10 (Spitze oben) mit Naht vom Grundkreis zur Spitze; die Ecke der
/// Spitze liegt `shift` entlang der Naht neben ihr, Genauigkeit 1e-3.
fn pointed_cone(shift: f64) -> String {
    let mut s = Step::new();
    let apex = DVec3::new(0.0, 0.0, H);
    let base = on_cyl(0.0, 0.0);
    let a0 = s.axis2(DVec3::ZERO, DVec3::Z, DVec3::X);
    let down = s.axis2(DVec3::ZERO, -DVec3::Z, DVec3::X);
    let c0 = s.push(format!("CIRCLE('',#{a0},{R:.1})"));
    let vb = s.vertex(base);
    let va = s.vertex(apex + (base - apex).normalize() * shift);
    let circle = s.edge(vb, vb, c0, true);
    let generator = s.line(base, apex);
    let seam = s.edge(vb, va, generator, true);
    let plane = s.push(format!("PLANE('',#{a0})"));
    let b = s.bound(&[(circle, false)], true);
    let bottom = s.face(&[b], plane, false);
    let cone = s.push(format!(
        "CONICAL_SURFACE('',#{down},{R:.1},{:.17e})",
        (R / H).atan()
    ));
    let b = s.bound(&[(circle, true), (seam, true), (seam, false)], true);
    let mantle = s.face(&[b], cone, true);
    s.file(&[bottom, mantle])
        .replace("LENGTH_MEASURE(1.E-06)", "LENGTH_MEASURE(1.E-3)")
}

#[test]
fn apex_corner_along_the_seam_is_pulled_onto_the_apex() {
    // Die Ecke liegt auf der Naht-Geraden 5e-4 vor der Spitze: Nach dem Ziehen auf die Spitze
    // muss auch der Parameter der Kante dort neu sein (sonst endet die Kante neben dem Pol).
    for shift in [0.0, 5e-4] {
        let s = read(&pointed_cone(shift)).unwrap_or_else(|Skip(m)| panic!("{shift}: {m}"));
        validate(&s).expect("gültig");
        let mesh = tessellate(
            &s,
            &TessellationOptions {
                chord_tolerance: 0.002,
            },
        )
        .expect("Netz");
        let want = PI * R * R * H / 3.0;
        let v = mass_properties(&mesh).volume;
        assert!(
            ((v - want) / want).abs() <= 1.5e-3,
            "{shift}: {v} statt {want}"
        );
    }
}
