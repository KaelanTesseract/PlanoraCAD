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
        let shell = self.shell(faces);
        let body = self.push(format!("MANIFOLD_SOLID_BREP('Teil',#{shell})"));
        self.finish(&[body])
    }

    fn shell(&mut self, faces: &[u64]) -> u64 {
        let f: Vec<String> = faces.iter().map(|f| format!("#{f}")).collect();
        self.push(format!("CLOSED_SHELL('',({}))", f.join(",")))
    }

    /// Ganze Datei; `items` sind die Volumen-Einträge der einen Darstellung.
    fn finish(self, items: &[u64]) -> String {
        let body: Vec<String> = items.iter().map(|f| format!("#{f}")).collect();
        let body = body.join(",");
        format!(
            "ISO-10303-21;\n/* Selbst geschrieben für die Tests von PlanoraCAD (GPL-3.0-only). */\n\
             HEADER;\nFILE_DESCRIPTION((''),'2;1');\n\
             FILE_NAME('x.step','2026-10-21T00:00:00',(''),(''),'','','');\n\
             FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\nENDSEC;\nDATA;\n\
             #9=ADVANCED_BREP_SHAPE_REPRESENTATION('',({body}),#105);\n\
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
    let items: Vec<u64> = e
        .arg("ADVANCED_BREP_SHAPE_REPRESENTATION", 1)
        .expect("Liste")
        .refs()
        .expect("Verweise");
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
fn pointed_cone(shift: f64, apex_first: bool) -> String {
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
    // Die Naht beginnt an der Spitze (`apex_first`) oder am Grundkreis.
    let seam = if apex_first {
        s.edge(va, vb, generator, false)
    } else {
        s.edge(vb, va, generator, true)
    };
    let up = !apex_first;
    let plane = s.push(format!("PLANE('',#{a0})"));
    let b = s.bound(&[(circle, false)], true);
    let bottom = s.face(&[b], plane, false);
    let cone = s.push(format!(
        "CONICAL_SURFACE('',#{down},{R:.1},{:.17e})",
        (R / H).atan()
    ));
    let b = s.bound(&[(circle, true), (seam, up), (seam, !up)], true);
    let mantle = s.face(&[b], cone, true);
    s.file(&[bottom, mantle])
        .replace("LENGTH_MEASURE(1.E-06)", "LENGTH_MEASURE(1.E-3)")
}

#[test]
fn apex_corner_along_the_seam_is_pulled_onto_the_apex() {
    // Die Ecke liegt auf der Naht-Geraden 5e-4 vor der Spitze: Nach dem Ziehen auf die Spitze
    // muss auch der Parameter der Kante dort neu sein (sonst endet die Kante neben dem Pol).
    for (shift, apex_first) in [(0.0, false), (5e-4, false), (0.0, true), (5e-4, true)] {
        let s = read(&pointed_cone(shift, apex_first))
            .unwrap_or_else(|Skip(m)| panic!("{shift} {apex_first}: {m}"));
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

// --- Nacharbeit 2: schräge Kugelkappen, runde Löcher an der Naht, Mehrkörper ---

fn unit(rng: &mut Lcg) -> DVec3 {
    let z = rng.range(-1.0, 1.0);
    let phi = rng.range(0.0, TAU);
    let r = (1.0 - z * z).sqrt();
    DVec3::new(r * phi.cos(), r * phi.sin(), z)
}

/// Die Kappe liegt ganz auf der Seite `n · p ≥ h` (die Gegenkappe nicht) und hat das Volumen.
fn assert_cap(s: &Solid, n: DVec3, h: f64, want: f64, what: &str) {
    let mesh = tessellate(
        s,
        &TessellationOptions {
            chord_tolerance: 0.002,
        },
    )
    .expect("Netz");
    let v = mass_properties(&mesh).volume;
    assert!(
        ((v - want) / want).abs() <= 5e-3,
        "{what}: {v} statt {want}"
    );
    let low = mesh
        .positions
        .iter()
        .map(|p| p.dot(n))
        .fold(f64::INFINITY, f64::min);
    assert!(
        low >= h - 1e-6,
        "{what}: Punkt bei {low} unter der Schnittebene {h}"
    );
}

fn mesh_volume(s: &Solid, chord: f64) -> f64 {
    let mesh = tessellate(
        s,
        &TessellationOptions {
            chord_tolerance: chord,
        },
    )
    .expect("Netz");
    mass_properties(&mesh).volume
}

/// Kugel r 6 um den Ursprung mit Achse `axis`, geschnitten von der Ebene `n · p = h`; der Körper
/// ist der Teil auf der Seite von `n` (eben der Deckel, gekrümmt die Kappe ohne Naht und ohne
/// Pol). Liefert Text und Volumen.
fn skew_cap(axis: DVec3, n: DVec3, h: f64, theta: f64) -> (String, f64) {
    let r = 6.0;
    let rho = (r * r - h * h).sqrt();
    let c = n * h;
    let cx = n.any_orthonormal_vector();
    let cy = n.cross(cx);
    let mut s = Step::new();
    let frame = s.axis2(c, n, cx);
    let circle = s.push(format!("CIRCLE('',#{frame},{rho:.17e})"));
    let v = s.vertex(c + (cx * theta.cos() + cy * theta.sin()) * rho);
    let e = s.edge(v, v, circle, true);
    let plane = s.push(format!("PLANE('',#{frame})"));
    let b = s.bound(&[(e, false)], true);
    let lid = s.face(&[b], plane, false);
    let sf = s.axis2(DVec3::ZERO, axis, axis.any_orthonormal_vector());
    let sphere = s.push(format!("SPHERICAL_SURFACE('',#{sf},{r:.1})"));
    let b = s.bound(&[(e, true)], false);
    let cap = s.face(&[b], sphere, true);
    let height = r - h;
    (
        s.file(&[lid, cap]),
        PI * height * height * (3.0 * r - height) / 3.0,
    )
}

/// Jede einzelne Richtungsangabe (`.T.`/`.F.` am Zeilenende von Fläche, Rand, Halbkante, Kante)
/// gekippt.
fn single_flips(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for (k, l) in lines.iter().enumerate() {
        let kinds = [
            "ADVANCED_FACE(",
            "FACE_BOUND(",
            "FACE_OUTER_BOUND(",
            "ORIENTED_EDGE(",
            "EDGE_CURVE(",
        ];
        if !kinds.iter().any(|t| l.contains(t)) {
            continue;
        }
        let flipped = if let Some(head) = l.strip_suffix(".T.);") {
            format!("{head}.F.);")
        } else if let Some(head) = l.strip_suffix(".F.);") {
            format!("{head}.T.);")
        } else {
            continue;
        };
        let mut t: Vec<String> = lines.iter().map(|x| x.to_string()).collect();
        t[k] = flipped;
        out.push(t.join("\n"));
    }
    out
}

#[test]
fn skew_caps_are_never_the_other_cap() {
    // Kugelachse schräg zur Kappe (die Kappe enthält einen, beide oder keinen Parameterpol):
    // richtig geschrieben gelesen; jede einzelne gekippte Richtung abgelehnt oder derselbe
    // Körper, nie die Gegenkappe.
    let mut rng = Lcg(0x5eed_6007);
    for case in 0..40 {
        let axis = unit(&mut rng);
        let n = unit(&mut rng);
        let h = rng.range(-0.8, 0.8) * 6.0;
        let theta = rng.range(0.0, TAU);
        let (text, want) = skew_cap(axis, n, h, theta);
        let s = read(&text).unwrap_or_else(|Skip(m)| panic!("Fall {case}: {m}"));
        validate(&s).expect("gültig");
        assert_cap(&s, n, h, want, &format!("Fall {case}"));
        for (k, t) in single_flips(&text).iter().enumerate() {
            if let Ok(s) = read(t) {
                assert_cap(&s, n, h, want, &format!("Fall {case}, Kippung {k}"));
            }
        }
    }
}

/// Kugelband r 6 zwischen z = −3 und z = 3 mit einer runden Bohrung (Kreis auf der Kugel um die
/// Richtung bei der Länge `phi`, Winkelradius 0,4), mit einer Kugelfläche gefüllt; die Ecken des
/// unteren und oberen Kreises liegen bei den Längen `ub` und `ut`. Volumen 198π.
fn bored_band(phi: f64, ub: f64, ut: f64, uncertainty: &str) -> String {
    let r = 6.0;
    let mut s = Step::new();
    let ring = |z: f64, u: f64| {
        let q = (r * r - z * z).sqrt();
        DVec3::new(q * u.cos(), q * u.sin(), z)
    };
    let q = (r * r - 9.0f64).sqrt();
    let fb = s.axis2(DVec3::new(0.0, 0.0, -3.0), DVec3::Z, DVec3::X);
    let ft = s.axis2(DVec3::new(0.0, 0.0, 3.0), DVec3::Z, DVec3::X);
    let cb = s.push(format!("CIRCLE('',#{fb},{q:.17e})"));
    let ct = s.push(format!("CIRCLE('',#{ft},{q:.17e})"));
    let vb = s.vertex(ring(-3.0, ub));
    let vt = s.vertex(ring(3.0, ut));
    let eb = s.edge(vb, vb, cb, true);
    let et = s.edge(vt, vt, ct, true);
    let alpha: f64 = 0.4;
    let d = DVec3::new(phi.cos(), phi.sin(), 0.0);
    let fh = s.axis2(d * (r * alpha.cos()), d, DVec3::Z);
    let ch = s.push(format!("CIRCLE('',#{fh},{:.17e})", r * alpha.sin()));
    // Ecke des Lochkreises schräg (nicht an einem Extrem von u, damit Abtastpunkte es verfehlen).
    let beta: f64 = 0.123;
    let side = d.cross(DVec3::Z);
    let vh = s.vertex(
        d * (r * alpha.cos()) + (DVec3::Z * beta.cos() + side * beta.sin()) * (r * alpha.sin()),
    );
    let eh = s.edge(vh, vh, ch, true);
    let pb = s.push(format!("PLANE('',#{fb})"));
    let pt = s.push(format!("PLANE('',#{ft})"));
    let sf = s.axis2(DVec3::ZERO, DVec3::Z, DVec3::X);
    let sphere = s.push(format!("SPHERICAL_SURFACE('',#{sf},{r:.1})"));
    let b = s.bound(&[(eb, false)], true);
    let bottom = s.face(&[b], pb, false);
    let b = s.bound(&[(et, true)], true);
    let top = s.face(&[b], pt, true);
    let b1 = s.bound(&[(eb, true)], false);
    let b2 = s.bound(&[(et, false)], false);
    let b3 = s.bound(&[(eh, false)], false);
    let band = s.face(&[b1, b2, b3], sphere, true);
    let b = s.bound(&[(eh, true)], false);
    let plug = s.face(&[b], sphere, true);
    s.file(&[bottom, top, band, plug]).replace(
        "LENGTH_MEASURE(1.E-06)",
        &format!("LENGTH_MEASURE({uncertainty})"),
    )
}

#[test]
fn round_hole_near_a_corner_angle_keeps_off_the_seam() {
    // Die Ecken der Kreise liegen knapp innerhalb des Längenbereichs der Bohrung (φ ± 0,4); die
    // Naht muss trotzdem neben dem Loch liegen. Fein vernetzbar (Sehne 0,001), Volumen 198π.
    let want = 198.0 * PI;
    for phi in [0.3, 2.0] {
        for delta in [1e-5, 1e-4, 1e-3, 3e-3, 6e-3] {
            for side in [1.0, -1.0] {
                let inside = phi + side * (0.4 - delta);
                let away = phi + PI;
                for (ub, ut) in [(inside, away), (away, inside), (inside, inside)] {
                    let what = format!("φ {phi}, δ {delta}, Seite {side}, Ecken {ub}/{ut}");
                    let s = read(&bored_band(phi, ub, ut, "1.E-06"))
                        .unwrap_or_else(|Skip(m)| panic!("{what}: {m}"));
                    validate(&s).unwrap_or_else(|e| panic!("{what}: {e}"));
                    let v = mesh_volume(&s, 0.001);
                    assert!(((v - want) / want).abs() <= 1e-3, "{what}: {v}");
                }
            }
        }
    }
    // Grobe Datei (1e-3): Die Naht hält mindestens 10 Genauigkeiten Abstand vom Loch (als
    // Winkel am kleinsten Abstand des Lochs zur Achse, 6 · cos 0,4), auch wenn eine Ecke knapp
    // außerhalb des Lochbereichs liegt.
    for side in [1.0, -1.0] {
        let phi = 0.3;
        let corner = phi + side * (0.4 + 1e-4);
        let s = read(&bored_band(phi, corner, corner, "1.E-3"))
            .unwrap_or_else(|Skip(m)| panic!("grob {side}: {m}"));
        let band = s
            .faces
            .iter()
            .map(|(_, f)| f)
            .find(|f| f.inner.len() == 1)
            .expect("Band mit Loch");
        let seam = band
            .outer
            .half_edges
            .iter()
            .find(|a| {
                band.outer
                    .half_edges
                    .iter()
                    .filter(|b| b.edge == a.edge)
                    .count()
                    == 2
            })
            .map(|he| &s.edges[he.edge])
            .expect("Naht");
        let (u, _) = band
            .surface
            .project(seam.curve.point(0.5 * (seam.t0 + seam.t1)));
        let off = |x: f64| {
            let d = (x - u).rem_euclid(TAU);
            d.min(TAU - d)
        };
        let gap = if ((u - phi + PI).rem_euclid(TAU) - PI).abs() <= 0.4 {
            0.0
        } else {
            off(phi - 0.4).min(off(phi + 0.4))
        };
        let need = 10.0 * 1e-3 / (6.0 * 0.4f64.cos());
        assert!(
            gap >= need * (1.0 - 1e-9),
            "grob {side}: Abstand {gap} < {need}"
        );
    }
}

/// Fläche eines Quaders: Kanten (Index, vorwärts), Außennormale, Ecke für den Rahmen.
type BoxFace = ([(usize, bool); 4], DVec3, usize);

/// Flächen des Quaders `lo`…`hi` (nach außen); `holes`: zusätzliche Ränder (Löcher) für Boden
/// und Deckel.
fn box_faces(s: &mut Step, lo: DVec3, hi: DVec3, holes: Option<(u64, u64)>) -> Vec<u64> {
    let corner = |i: usize| {
        DVec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        )
    };
    let v: Vec<u64> = (0..8).map(|i| s.vertex(corner(i))).collect();
    // Kanten: x (j, k), y (i, k), z (i, j) wie in `void_box.step`.
    let mut e = Vec::new();
    for idx in 0..12 {
        let (a, b) = match idx {
            0..=3 => {
                let (j, k) = (idx % 2, idx / 2);
                (2 * j + 4 * k, 1 + 2 * j + 4 * k)
            }
            4..=7 => {
                let t = idx - 4;
                let (i, k) = (t % 2, t / 2);
                (i + 4 * k, i + 2 + 4 * k)
            }
            _ => {
                let t = idx - 8;
                let (i, j) = (t % 2, t / 2);
                (i + 2 * j, i + 2 * j + 4)
            }
        };
        let l = s.line(corner(a), corner(b));
        e.push(s.edge(v[a], v[b], l, true));
    }
    let faces: [BoxFace; 6] = [
        ([(4, true), (1, true), (5, false), (0, false)], -DVec3::Z, 0),
        ([(2, true), (7, true), (3, false), (6, false)], DVec3::Z, 7),
        ([(0, true), (9, true), (2, false), (8, false)], -DVec3::Y, 0),
        (
            [(10, true), (3, true), (11, false), (1, false)],
            DVec3::Y,
            7,
        ),
        (
            [(8, true), (6, true), (10, false), (4, false)],
            -DVec3::X,
            0,
        ),
        ([(5, true), (11, true), (7, false), (9, false)], DVec3::X, 7),
    ];
    let mut ids = Vec::new();
    for (k, (hes, normal, at)) in faces.into_iter().enumerate() {
        let hes: Vec<(u64, bool)> = hes.iter().map(|(k, f)| (e[*k], *f)).collect();
        let mut bounds = vec![s.bound(&hes, true)];
        match (k, holes) {
            (0, Some((b, _))) | (1, Some((_, b))) => bounds.push(b),
            _ => {}
        }
        let a = s.axis2(corner(at), normal, normal.any_orthonormal_vector());
        let p = s.push(format!("PLANE('',#{a})"));
        ids.push(s.face(&bounds, p, true));
    }
    ids
}

/// Quader `lo`…`hi` als Schale (Flächen nach außen).
fn box_shell(s: &mut Step, lo: DVec3, hi: DVec3) -> u64 {
    let faces = box_faces(s, lo, hi, None);
    s.shell(&faces)
}

#[test]
fn piece_in_a_cavity_is_legitimate() {
    // Quader 20 mit Quaderhohlraum 5 … 15 und darin ein Quader 8 … 12 als zweites Stück:
    // 8000 − 1000 + 64.
    let mut s = Step::new();
    let outer = box_shell(&mut s, DVec3::ZERO, DVec3::splat(20.0));
    let cavity = box_shell(&mut s, DVec3::splat(5.0), DVec3::splat(15.0));
    let void = s.push(format!("ORIENTED_CLOSED_SHELL('',*,#{cavity},.F.)"));
    let a = s.push(format!("BREP_WITH_VOIDS('Hohl',#{outer},(#{void}))"));
    let inner = box_shell(&mut s, DVec3::splat(8.0), DVec3::splat(12.0));
    let b = s.push(format!("MANIFOLD_SOLID_BREP('Kern',#{inner})"));
    let text = s.finish(&[a, b]);
    let solid = read(&text).unwrap_or_else(|Skip(m)| panic!("{m}"));
    assert_eq!(solid.shells.len(), 3);
    assert!((mesh_volume(&solid, 0.05) - 7064.0).abs() <= 1e-9);
}

/// Platte 20 × 20 × 10 mit Bohrung r 5 und darin ein Stift r 5, h 10 (zwei Stücke, die sich
/// am Mantel berühren); Ecken der Bohrungs- und Stiftkreise bei den Winkeln `ub` und `up`.
fn pin_in_bore(ub: f64, up: f64) -> String {
    let mut s = Step::new();
    let fb = s.axis2(DVec3::ZERO, DVec3::Z, DVec3::X);
    let ft = s.axis2(DVec3::new(0.0, 0.0, H), DVec3::Z, DVec3::X);
    let circles = |s: &mut Step, u: f64| {
        let cb = s.push(format!("CIRCLE('',#{fb},{R:.1})"));
        let ct = s.push(format!("CIRCLE('',#{ft},{R:.1})"));
        let vb = s.vertex(on_cyl(u, 0.0));
        let vt = s.vertex(on_cyl(u, H));
        (s.edge(vb, vb, cb, true), s.edge(vt, vt, ct, true))
    };
    let (bb, bt) = circles(&mut s, ub);
    let (pb, pt) = circles(&mut s, up);
    let cyl = s.push(format!("CYLINDRICAL_SURFACE('',#{fb},{R:.1})"));
    // Platte: Löcher in Boden und Deckel, Bohrungsmantel nach innen (Material außen).
    let holes = (
        s.bound(&[(bb, true)], false),
        s.bound(&[(bt, false)], false),
    );
    let mut faces = box_faces(
        &mut s,
        DVec3::new(-10.0, -10.0, 0.0),
        DVec3::new(10.0, 10.0, H),
        Some(holes),
    );
    let b1 = s.bound(&[(bb, false)], false);
    let b2 = s.bound(&[(bt, true)], false);
    faces.push(s.face(&[b1, b2], cyl, false));
    let plate = s.shell(&faces);
    let a = s.push(format!("MANIFOLD_SOLID_BREP('Platte',#{plate})"));
    // Stift.
    let pbp = s.push(format!("PLANE('',#{fb})"));
    let ptp = s.push(format!("PLANE('',#{ft})"));
    let b = s.bound(&[(pb, false)], true);
    let f1 = s.face(&[b], pbp, false);
    let b = s.bound(&[(pt, true)], true);
    let f2 = s.face(&[b], ptp, true);
    let b1 = s.bound(&[(pb, true)], false);
    let b2 = s.bound(&[(pt, false)], false);
    let f3 = s.face(&[b1, b2], cyl, true);
    let pin = s.shell(&[f1, f2, f3]);
    let b = s.push(format!("MANIFOLD_SOLID_BREP('Stift',#{pin})"));
    s.finish(&[a, b])
}

#[test]
fn pin_in_bore_touches_but_does_not_overlap() {
    for (ub, up) in [(0.0, 0.0), (0.0, 1.0), (2.0, 0.5), (PI, 0.0)] {
        let solid = read(&pin_in_bore(ub, up)).unwrap_or_else(|Skip(m)| panic!("{ub}/{up}: {m}"));
        assert_eq!(solid.shells.len(), 2);
        let v = mesh_volume(&solid, 0.002);
        assert!(((v - 4000.0) / 4000.0).abs() <= 1e-3, "{ub}/{up}: {v}");
    }
}

#[test]
fn many_pieces_are_checked_quickly() {
    // 60 Kugeln r 100 (je eine Fläche mit Naht) in einer Darstellung: Die Prüfung der Schalen
    // filtert über die Hüllquader und bleibt schnell.
    let mut s = Step::new();
    let mut items = Vec::new();
    for k in 0..60 {
        let c = DVec3::new(250.0 * (k % 8) as f64, 250.0 * (k / 8) as f64, 0.0);
        let r = 100.0;
        let south = s.vertex(c - DVec3::Z * r);
        let north = s.vertex(c + DVec3::Z * r);
        let mf = s.axis2(c, -DVec3::Y, DVec3::X);
        let m = s.push(format!("CIRCLE('',#{mf},{r:.1})"));
        let e = s.edge(south, north, m, true);
        let sf = s.axis2(c, DVec3::Z, DVec3::X);
        let sphere = s.push(format!("SPHERICAL_SURFACE('',#{sf},{r:.1})"));
        let b = s.bound(&[(e, true), (e, false)], true);
        let f = s.face(&[b], sphere, true);
        let sh = s.shell(&[f]);
        items.push(s.push(format!("MANIFOLD_SOLID_BREP('Kugel',#{sh})")));
    }
    let text = s.finish(&items);
    let t = std::time::Instant::now();
    let solid = read(&text).unwrap_or_else(|Skip(m)| panic!("{m}"));
    let took = t.elapsed();
    assert_eq!(solid.shells.len(), 60);
    assert!(took.as_secs_f64() < 5.0, "{took:?}");
}
