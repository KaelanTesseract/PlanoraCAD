//! STEP-Leser, Nähte (STEP-Import, Aufgabe 6, Nacharbeit): nahtlose Zylinder, wie Exporteure
//! sie schreiben, mit Fenstern im Mantel, Laschen am Ende, schrägem Deckel, Ecken an beliebigen
//! Winkeln und Schleifen, die an beliebiger Ecke beginnen. Die Texte erzeugt der Test; jeder
//! Körper hat das Volumen π·25·10 (Fenster und Laschen sind mit Flächen derselben Zylinderfläche
//! gefüllt, der schräge Deckel hat die mittlere Höhe 10).

mod common;

use common::{Lcg, export};
use planora_exchange::Progress;
use planora_kernel::boolean::BoolOp;
use planora_kernel::{AnalyticKernel, Kernel, Solid, validate};
use planora_math::{DMat4, DVec3, PI, TAU};
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

    /// Pol als `VERTEX_LOOP` in einem `FACE_BOUND`.
    fn pole_bound(&mut self, p: DVec3) -> u64 {
        let v = self.vertex(p);
        let lp = self.push(format!("VERTEX_LOOP('',#{v})"));
        self.push(format!("FACE_BOUND('',#{lp},.T.)"))
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
        .expect("Verweise")
        .into_iter()
        .filter(|id| {
            let it = file.entity(*id).expect("Eintrag");
            it.is("MANIFOLD_SOLID_BREP") || it.is("BREP_WITH_VOIDS")
        })
        .collect();
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

// --- Nacharbeit 3: Kugelflächen mit Polen im Rand, Bänder, Laufzeit der Schalenprüfung ---

/// Umlaufzahl des Netzes um `q` (innen 1, außen 0).
fn winding_at(s: &Solid, q: DVec3) -> f64 {
    let m = tessellate(
        s,
        &TessellationOptions {
            chord_tolerance: 0.01,
        },
    )
    .expect("Netz");
    let mut total = 0.0;
    for t in m.indices.chunks(3) {
        let [a, b, c] = [t[0], t[1], t[2]].map(|i| m.positions[i as usize] - q);
        let (la, lb, lc) = (a.length(), b.length(), c.length());
        let num = a.dot(b.cross(c));
        let den = la * lb * lc + a.dot(b) * lc + b.dot(c) * la + c.dot(a) * lb;
        total += 2.0 * num.atan2(den);
    }
    total / (4.0 * PI)
}

/// Richtig gelesen (Volumen, `inside` innen) und jede einzelne Kippung abgelehnt oder derselbe
/// Körper (nie der Gegenkörper).
fn assert_never_opposite(text: &str, want: f64, inside: DVec3, what: &str) {
    let check = |s: &Solid, how: &str| {
        let v = mesh_volume(s, 0.002);
        assert!(
            ((v - want) / want).abs() <= 5e-3,
            "{what} {how}: {v} statt {want}"
        );
        let w = winding_at(s, inside);
        assert!(w > 0.5, "{what} {how}: Probepunkt außen ({w})");
    };
    let s = read(text).unwrap_or_else(|Skip(m)| panic!("{what}: {m}"));
    validate(&s).unwrap_or_else(|e| panic!("{what}: {e}"));
    check(&s, "richtig");
    for (k, t) in single_flips(text).iter().enumerate() {
        if let Ok(s) = read(t) {
            check(&s, &format!("Kippung {k}"));
        }
    }
}

const SR: f64 = 6.0;

/// Meridian der Kugel r 6 um den Ursprung (Achse z) bei der Länge `u`, als Kante vom Süd- zum
/// Nordpol.
fn meridian_edge(s: &mut Step, south: u64, north: u64, u: f64) -> u64 {
    let radial = DVec3::new(u.cos(), u.sin(), 0.0);
    let f = s.axis2(DVec3::ZERO, radial.cross(DVec3::Z), radial);
    let c = s.push(format!("CIRCLE('',#{f},{SR:.1})"));
    s.edge(south, north, c, true)
}

fn sphere_surface(s: &mut Step) -> u64 {
    let f = s.axis2(DVec3::ZERO, DVec3::Z, DVec3::X);
    s.push(format!("SPHERICAL_SURFACE('',#{f},{SR:.1})"))
}

/// Kugelkeil zwischen den Längen 0 und `alpha` (Rand durch beide Pole); bei `alpha = π` mit
/// einer ebenen Halbscheibe statt zweier. Volumen 144 · α.
fn lune(alpha: f64) -> String {
    let mut s = Step::new();
    let south = s.vertex(DVec3::new(0.0, 0.0, -SR));
    let north = s.vertex(DVec3::new(0.0, 0.0, SR));
    let m0 = meridian_edge(&mut s, south, north, 0.0);
    let ma = meridian_edge(&mut s, south, north, alpha);
    let sphere = sphere_surface(&mut s);
    let b = s.bound(&[(ma, true), (m0, false)], false);
    let lune = s.face(&[b], sphere, true);
    let mut faces = vec![lune];
    if (alpha - PI).abs() < 1e-12 {
        let f = s.axis2(DVec3::ZERO, -DVec3::Y, DVec3::X);
        let p = s.push(format!("PLANE('',#{f})"));
        let b = s.bound(&[(m0, true), (ma, false)], true);
        faces.push(s.face(&[b], p, true));
    } else {
        let axis = s.line(DVec3::new(0.0, 0.0, -SR), DVec3::new(0.0, 0.0, SR));
        let z = s.edge(south, north, axis, true);
        let f = s.axis2(DVec3::ZERO, -DVec3::Y, DVec3::X);
        let p = s.push(format!("PLANE('',#{f})"));
        let b = s.bound(&[(m0, true), (z, false)], true);
        faces.push(s.face(&[b], p, true));
        let n = DVec3::new(-alpha.sin(), alpha.cos(), 0.0);
        let f = s.axis2(DVec3::ZERO, n, DVec3::new(alpha.cos(), alpha.sin(), 0.0));
        let p = s.push(format!("PLANE('',#{f})"));
        let b = s.bound(&[(ma, false), (z, true)], true);
        faces.push(s.face(&[b], p, true));
    }
    s.file(&faces)
}

#[test]
fn lunes_through_both_poles_are_never_the_other_lune() {
    for alpha in [0.7, 2.0, PI, 4.0, 5.5] {
        let inside = DVec3::new((0.5 * alpha).cos(), (0.5 * alpha).sin(), 0.0) * 3.0;
        assert_never_opposite(
            &lune(alpha),
            144.0 * alpha,
            inside,
            &format!("Keil {alpha}"),
        );
    }
}

/// Halbkugel y ≥ 0 (Kugelachse z liegt in der flachen Seite) mit dem Rand als eine geschlossene
/// Großkreiskante; ihre Ecke liegt beim Winkel `t` (π/2: am Nordpol, 0: am Äquator).
fn great_circle_hemisphere(t: f64) -> String {
    let mut s = Step::new();
    let faces = great_circle_hemisphere_at(&mut s, DVec3::ZERO, t, 0.0);
    s.file(&faces)
}

/// Flächen der Halbkugel wie [`great_circle_hemisphere`] um `c`; `tilt` kippt die Kreisebene um
/// die x-Achse (dann läuft der Großkreis `SR · sin(tilt)` neben beiden Polen vorbei).
fn great_circle_hemisphere_at(s: &mut Step, c: DVec3, t: f64, tilt: f64) -> Vec<u64> {
    let n = DVec3::new(0.0, -tilt.cos(), tilt.sin());
    let f = s.axis2(c, n, DVec3::X);
    let circle = s.push(format!("CIRCLE('',#{f},{SR:.1})"));
    let y = n.cross(DVec3::X);
    let v = s.vertex(c + (DVec3::X * t.cos() + y * t.sin()) * SR);
    let e = s.edge(v, v, circle, true);
    let p = s.push(format!("PLANE('',#{f})"));
    let b = s.bound(&[(e, true)], true);
    let disk = s.face(&[b], p, true);
    let sf = s.axis2(c, DVec3::Z, DVec3::X);
    let sphere = s.push(format!("SPHERICAL_SURFACE('',#{sf},{SR:.1})"));
    let b = s.bound(&[(e, false)], false);
    let half = s.face(&[b], sphere, true);
    vec![disk, half]
}

#[test]
fn great_circle_through_the_poles_is_read_and_never_the_other_half() {
    for t in [PI / 2.0, -PI / 2.0, 0.0, 1.0, 2.5] {
        assert_never_opposite(
            &great_circle_hemisphere(t),
            144.0 * PI,
            DVec3::new(0.0, 3.0, 0.0),
            &format!("Großkreis, Ecke bei {t}"),
        );
    }
}

/// Achtelkugel x, y, z ≥ 0 (der Rand berührt nur den Nordpol). Volumen 36π.
fn octant() -> String {
    let mut s = Step::new();
    let o = s.vertex(DVec3::ZERO);
    let a = s.vertex(DVec3::new(SR, 0.0, 0.0));
    let b = s.vertex(DVec3::new(0.0, SR, 0.0));
    let n = s.vertex(DVec3::new(0.0, 0.0, SR));
    let fz = s.axis2(DVec3::ZERO, DVec3::Z, DVec3::X);
    let ce = s.push(format!("CIRCLE('',#{fz},{SR:.1})"));
    let e = s.edge(a, b, ce, true);
    let f0 = s.axis2(DVec3::ZERO, -DVec3::Y, DVec3::X);
    let c0 = s.push(format!("CIRCLE('',#{f0},{SR:.1})"));
    let m0 = s.edge(a, n, c0, true);
    let f9 = s.axis2(DVec3::ZERO, DVec3::X, DVec3::Y);
    let c9 = s.push(format!("CIRCLE('',#{f9},{SR:.1})"));
    let m9 = s.edge(b, n, c9, true);
    let lx = s.line(DVec3::ZERO, DVec3::new(SR, 0.0, 0.0));
    let x = s.edge(o, a, lx, true);
    let ly = s.line(DVec3::ZERO, DVec3::new(0.0, SR, 0.0));
    let y = s.edge(o, b, ly, true);
    let lz = s.line(DVec3::ZERO, DVec3::new(0.0, 0.0, SR));
    let z = s.edge(o, n, lz, true);
    let plane = |s: &mut Step, normal: DVec3| {
        let f = s.axis2(DVec3::ZERO, normal, normal.any_orthonormal_vector());
        s.push(format!("PLANE('',#{f})"))
    };
    let pz = plane(&mut s, -DVec3::Z);
    let b1 = s.bound(&[(x, false), (y, true), (e, false)], true);
    let f1 = s.face(&[b1], pz, true);
    let py = plane(&mut s, -DVec3::Y);
    let b2 = s.bound(&[(m0, true), (z, false), (x, true)], true);
    let f2 = s.face(&[b2], py, true);
    let px = plane(&mut s, -DVec3::X);
    let b3 = s.bound(&[(y, false), (z, true), (m9, false)], true);
    let f3 = s.face(&[b3], px, true);
    let sphere = sphere_surface(&mut s);
    let b4 = s.bound(&[(e, true), (m9, true), (m0, false)], false);
    let f4 = s.face(&[b4], sphere, true);
    s.file(&[f1, f2, f3, f4])
}

#[test]
fn octant_touching_one_pole_stays_fine() {
    assert_never_opposite(&octant(), 36.0 * PI, DVec3::splat(2.0), "Achtel");
}

/// Kugelband zwischen den Ebenen x = 2 und x = 4 (beide Pole liegen in derselben Kappe, das Band
/// ist im Parameterraum ein Ring); `outer`: welcher Rand `FACE_OUTER_BOUND` ist (0: x = 2,
/// 1: x = 4, sonst keiner). Volumen 160π/3.
fn band_without_poles(outer: usize) -> String {
    let mut s = Step::new();
    let circle = |s: &mut Step, c: f64| {
        let r = (SR * SR - c * c).sqrt();
        let f = s.axis2(DVec3::new(c, 0.0, 0.0), DVec3::X, DVec3::Y);
        let k = s.push(format!("CIRCLE('',#{f},{r:.17e})"));
        let v = s.vertex(DVec3::new(c, r, 0.0));
        (s.edge(v, v, k, true), f)
    };
    let (c2, f2) = circle(&mut s, 2.0);
    let (c4, f4) = circle(&mut s, 4.0);
    let p2 = s.push(format!("PLANE('',#{f2})"));
    let p4 = s.push(format!("PLANE('',#{f4})"));
    let b = s.bound(&[(c2, false)], true);
    let d2 = s.face(&[b], p2, false);
    let b = s.bound(&[(c4, true)], true);
    let d4 = s.face(&[b], p4, true);
    let sphere = sphere_surface(&mut s);
    let b2 = s.bound(&[(c2, true)], outer == 0);
    let b4 = s.bound(&[(c4, false)], outer == 1);
    let band = s.face(&[b2, b4], sphere, true);
    s.file(&[d2, d4, band])
}

#[test]
fn band_without_poles_reads_whichever_loop_is_marked_outer() {
    for outer in [0, 1, 2] {
        assert_never_opposite(
            &band_without_poles(outer),
            160.0 * PI / 3.0,
            DVec3::new(3.0, 0.0, 0.0),
            &format!("Band, außen {outer}"),
        );
    }
}

/// Kugelfläche (eine Fläche mit Naht im Meridian bei u = 0) um `c` mit Radius `r`, als Schale.
fn sphere_shell(s: &mut Step, c: DVec3, r: f64, outward: bool) -> u64 {
    let south = s.vertex(c - DVec3::Z * r);
    let north = s.vertex(c + DVec3::Z * r);
    let mf = s.axis2(c, -DVec3::Y, DVec3::X);
    let m = s.push(format!("CIRCLE('',#{mf},{r:.1})"));
    let e = s.edge(south, north, m, true);
    let sf = s.axis2(c, DVec3::Z, DVec3::X);
    let sphere = s.push(format!("SPHERICAL_SURFACE('',#{sf},{r:.1})"));
    let b = s.bound(&[(e, true), (e, false)], true);
    let f = s.face(&[b], sphere, outward);
    s.shell(&[f])
}

#[test]
fn overlapping_balls_are_named() {
    // Zwei Kugeln r 5, 0,3 mm ineinander: abgelehnt; 0,3 mm auseinander: gelesen.
    for (gap, ok) in [(-0.3, false), (0.3, true)] {
        let mut s = Step::new();
        let a = sphere_shell(&mut s, DVec3::ZERO, 5.0, true);
        let b = sphere_shell(&mut s, DVec3::new(10.0 + gap, 0.0, 0.0), 5.0, true);
        let a = s.push(format!("MANIFOLD_SOLID_BREP('A',#{a})"));
        let b = s.push(format!("MANIFOLD_SOLID_BREP('B',#{b})"));
        match read(&s.finish(&[a, b])) {
            Ok(_) => assert!(ok, "Überlappung {gap} angenommen"),
            Err(Skip(m)) => {
                assert!(!ok, "{gap}: {m}");
                assert_eq!(m, "Zwei Stücke des Körpers überschneiden sich.");
            }
        }
    }
}

#[test]
fn pieces_in_a_curved_housing_are_checked_quickly() {
    // Hohlkugel r 210 mit Hohlraum r 200 und 50 Kugeln r 10 darin.
    let mut s = Step::new();
    let outer = sphere_shell(&mut s, DVec3::ZERO, 210.0, true);
    let cavity = sphere_shell(&mut s, DVec3::ZERO, 200.0, true);
    let void = s.push(format!("ORIENTED_CLOSED_SHELL('',*,#{cavity},.F.)"));
    let mut items = vec![s.push(format!("BREP_WITH_VOIDS('Gehäuse',#{outer},(#{void}))"))];
    for k in 0..50 {
        let c = DVec3::new(
            -100.0 + 40.0 * (k % 5) as f64,
            -100.0 + 40.0 * ((k / 5) % 5) as f64,
            -40.0 + 40.0 * (k / 25) as f64,
        );
        let b = sphere_shell(&mut s, c, 10.0, true);
        items.push(s.push(format!("MANIFOLD_SOLID_BREP('Kugel',#{b})")));
    }
    let text = s.finish(&items);
    let t = std::time::Instant::now();
    let solid = read(&text).unwrap_or_else(|Skip(m)| panic!("{m}"));
    let took = t.elapsed();
    assert_eq!(solid.shells.len(), 52);
    assert!(took.as_secs_f64() < 5.0, "{took:?}");
}

#[test]
fn many_separate_balls_are_checked_quickly() {
    let mut s = Step::new();
    let mut items = Vec::new();
    for k in 0..300 {
        let c = DVec3::new(30.0 * (k % 20) as f64, 30.0 * (k / 20) as f64, 0.0);
        let b = sphere_shell(&mut s, c, 10.0, true);
        items.push(s.push(format!("MANIFOLD_SOLID_BREP('Kugel',#{b})")));
    }
    let text = s.finish(&items);
    let t = std::time::Instant::now();
    let solid = read(&text).unwrap_or_else(|Skip(m)| panic!("{m}"));
    let took = t.elapsed();
    assert_eq!(solid.shells.len(), 300);
    assert!(took.as_secs_f64() < 3.0, "{took:?}");
}

#[test]
fn rim_close_to_a_pole_gets_its_seam() {
    // Kappe mit schräger Kugelachse, deren Rand 0,02 mm neben einem Pol vorbeiläuft (N4).
    let cases = [1.0, -2.0, 3.5, 0.2, -4.5, 5.0]
        .iter()
        .enumerate()
        .flat_map(|(k, h)| [(2 * k, *h, 0.02), (2 * k + 1, *h, 0.02)]);
    let mut read_ok = 0;
    for (k, h, gap) in cases {
        // Achse so gekippt, dass der Pol 0,02 mm neben dem Kreis liegt.
        let n = DVec3::Z;
        let rho = (36.0f64 - h * h).sqrt();
        // Pol auf der Kugel im Abstand 0,02 vom Kreis: Polarwinkel des Kreises ± 0,02 / 6.
        let polar = (h / SR).acos() + gap / SR * if k % 2 == 0 { 1.0 } else { -1.0 };
        let axis = DVec3::new(polar.sin(), 0.0, polar.cos());
        let (text, want) = skew_cap(axis, n, h, 0.3 + 0.7 * k as f64);
        match read(&text) {
            Ok(s) => {
                read_ok += 1;
                assert_cap(&s, n, h, want, &format!("Pol nahe am Rand {k}"));
            }
            // Der Vernetzer des Kernels tastet Ränder alle 5° ab; 0,02 mm neben dem Pol reicht
            // das nicht immer (Grenze des Kernels, nicht des Lesers).
            Err(Skip(m)) => assert!(
                m.ends_with("lässt sich nicht in Dreiecke zerlegen."),
                "{k}: {m}"
            ),
        }
        let _ = rho;
    }
    assert!(read_ok >= 11, "nur {read_ok} von 12 gelesen");
}

#[test]
fn shallow_box_overlap_is_named() {
    // Zwei Quader 10, 0,15 mm ineinander: abgelehnt; berührend: gelesen.
    for (shift, ok) in [(9.85, false), (10.0, true)] {
        let mut s = Step::new();
        let a = box_shell(&mut s, DVec3::ZERO, DVec3::splat(10.0));
        let b = box_shell(
            &mut s,
            DVec3::new(shift, 0.0, 0.0),
            DVec3::new(shift + 10.0, 10.0, 10.0),
        );
        let a = s.push(format!("MANIFOLD_SOLID_BREP('A',#{a})"));
        let b = s.push(format!("MANIFOLD_SOLID_BREP('B',#{b})"));
        match read(&s.finish(&[a, b])) {
            Ok(_) => assert!(ok, "Überlappung bei {shift} angenommen"),
            Err(Skip(m)) => {
                assert!(!ok, "{shift}: {m}");
                assert_eq!(m, "Zwei Stücke des Körpers überschneiden sich.");
            }
        }
    }
}

// --- Nacharbeit 4: Kugelflächen mit einem Pol im Rand, Schalen ohne Volumen, Halbkreise,
// --- viele Teilungen, Raster neben spitzen Kanten.

/// Kugel r 6 ohne das Achtel x, y, z ≥ 0: Die Kugelfläche ist das große Stück, ihr Rand berührt
/// nur den Nordpol, der Südpol liegt in ihrem Inneren (`mark`: `VERTEX_LOOP` am Südpol).
/// Volumen 252π.
fn ball_minus_octant(mark: bool) -> String {
    let mut s = Step::new();
    let o = s.vertex(DVec3::ZERO);
    let a = s.vertex(DVec3::new(SR, 0.0, 0.0));
    let b = s.vertex(DVec3::new(0.0, SR, 0.0));
    let n = s.vertex(DVec3::new(0.0, 0.0, SR));
    let fz = s.axis2(DVec3::ZERO, DVec3::Z, DVec3::X);
    let ce = s.push(format!("CIRCLE('',#{fz},{SR:.1})"));
    let e = s.edge(a, b, ce, true);
    let f0 = s.axis2(DVec3::ZERO, -DVec3::Y, DVec3::X);
    let c0 = s.push(format!("CIRCLE('',#{f0},{SR:.1})"));
    let m0 = s.edge(a, n, c0, true);
    let f9 = s.axis2(DVec3::ZERO, DVec3::X, DVec3::Y);
    let c9 = s.push(format!("CIRCLE('',#{f9},{SR:.1})"));
    let m9 = s.edge(b, n, c9, true);
    let lx = s.line(DVec3::ZERO, DVec3::new(SR, 0.0, 0.0));
    let x = s.edge(o, a, lx, true);
    let ly = s.line(DVec3::ZERO, DVec3::new(0.0, SR, 0.0));
    let y = s.edge(o, b, ly, true);
    let lz = s.line(DVec3::ZERO, DVec3::new(0.0, 0.0, SR));
    let z = s.edge(o, n, lz, true);
    let plane = |s: &mut Step, normal: DVec3| {
        let f = s.axis2(DVec3::ZERO, normal, normal.any_orthonormal_vector());
        s.push(format!("PLANE('',#{f})"))
    };
    let pz = plane(&mut s, DVec3::Z);
    let b1 = s.bound(&[(x, true), (e, true), (y, false)], true);
    let f1 = s.face(&[b1], pz, true);
    let py = plane(&mut s, DVec3::Y);
    let b2 = s.bound(&[(z, true), (m0, false), (x, false)], true);
    let f2 = s.face(&[b2], py, true);
    let px = plane(&mut s, DVec3::X);
    let b3 = s.bound(&[(y, true), (m9, true), (z, false)], true);
    let f3 = s.face(&[b3], px, true);
    let sphere = sphere_surface(&mut s);
    let b4 = s.bound(&[(m0, true), (m9, false), (e, false)], false);
    let mut bounds = vec![b4];
    if mark {
        bounds.push(s.pole_bound(DVec3::new(0.0, 0.0, -SR)));
    }
    let f4 = s.face(&bounds, sphere, true);
    s.file(&[f1, f2, f3, f4])
}

#[test]
fn ball_minus_octant_is_read_and_never_the_octant() {
    // R3-1: Das große Stück (Rand berührt genau einen Pol) wird gelesen, mit und ohne Marke am
    // inneren Pol; jede Kippung ist abgelehnt oder derselbe Körper, nie das Achtel.
    for mark in [false, true] {
        assert_never_opposite(
            &ball_minus_octant(mark),
            252.0 * PI,
            DVec3::splat(-2.0),
            &format!("Kugel ohne Achtel, Marke {mark}"),
        );
    }
}

/// Achtel wie [`octant`], die Kugelfläche zusätzlich mit einer Marke am Punkt `p`.
fn octant_with_mark(p: DVec3) -> String {
    let text = octant();
    // Die Kugelfläche ist die letzte `ADVANCED_FACE`; die Marke bekommt freie Kennungen.
    let face = text
        .lines()
        .rfind(|l| l.contains("ADVANCED_FACE("))
        .expect("Kugelfläche")
        .to_string();
    let with_mark = face.replacen("),#", ",#9003),#", 1);
    assert_ne!(with_mark, face);
    let mark = format!(
        "#9000=CARTESIAN_POINT('',({:.17e},{:.17e},{:.17e}));\n#9001=VERTEX_POINT('',#9000);\n\
         #9002=VERTEX_LOOP('',#9001);\n#9003=FACE_BOUND('',#9002,.T.);\nENDSEC;\nEND-ISO-10303-21;",
        p.x, p.y, p.z
    );
    let out = text
        .replace(&face, &with_mark)
        .replace("ENDSEC;\nEND-ISO-10303-21;", &mark);
    assert!(out.contains("#9003=FACE_BOUND"), "Dateiende nicht gefunden");
    out
}

#[test]
fn pole_mark_outside_a_piece_touching_one_pole_is_named() {
    // V02: Ein Achtel (Rand berührt den Nordpol) mit einer Marke am Südpol, der nicht in ihm
    // liegt, ist verdreht; eine Marke am berührten Pol stört nicht.
    match read(&octant_with_mark(DVec3::new(0.0, 0.0, -SR))) {
        Ok(_) => panic!("Achtel mit Marke am falschen Pol angenommen"),
        Err(Skip(m)) => assert_eq!(
            m,
            "Die Fläche „STEP-Fläche 4“ ist verdreht: Rand und Außenseite passen nicht zusammen."
        ),
    }
    let s = read(&octant_with_mark(DVec3::new(0.0, 0.0, SR)))
        .unwrap_or_else(|Skip(m)| panic!("Marke am berührten Pol: {m}"));
    let v = mesh_volume(&s, 0.002);
    assert!(((v - 36.0 * PI) / (36.0 * PI)).abs() <= 5e-3, "{v}");
}

/// Kugel r 6 aus `n` Keilen (nur Kugelflächen, Meridiane bei 2πk/n). Volumen 288π.
fn sphere_of_lunes(n: usize) -> String {
    let mut s = Step::new();
    let south = s.vertex(DVec3::new(0.0, 0.0, -SR));
    let north = s.vertex(DVec3::new(0.0, 0.0, SR));
    let m: Vec<u64> = (0..n)
        .map(|k| meridian_edge(&mut s, south, north, TAU * k as f64 / n as f64))
        .collect();
    let sphere = sphere_surface(&mut s);
    let faces: Vec<u64> = (0..n)
        .map(|k| {
            let b = s.bound(&[(m[(k + 1) % n], true), (m[k], false)], false);
            s.face(&[b], sphere, true)
        })
        .collect();
    s.file(&faces)
}

/// Kugel r 6 aus zwei Halbkugeln an einer Großkreiskante (Ebene y = 0). Volumen 288π.
fn two_hemispheres() -> String {
    let mut s = Step::new();
    let f = s.axis2(DVec3::ZERO, -DVec3::Y, DVec3::X);
    let c = s.push(format!("CIRCLE('',#{f},{SR:.1})"));
    let v = s.vertex(DVec3::new(SR, 0.0, 0.0));
    let e = s.edge(v, v, c, true);
    let sphere = sphere_surface(&mut s);
    let b = s.bound(&[(e, false)], false);
    let front = s.face(&[b], sphere, true);
    let b = s.bound(&[(e, true)], false);
    let back = s.face(&[b], sphere, true);
    s.file(&[front, back])
}

#[test]
fn spheres_of_lunes_and_hemispheres_are_never_doubled_or_empty() {
    // R3-2/R3-3: Kippt man ein Flächenbit, hat die Schale das Volumen null; kippt man den Sinn
    // einer Halbkreiskante, läuft sie über die andere Hälfte und die Kugel ist doppelt belegt.
    // Beides darf nie still angenommen werden.
    for n in [2, 3, 4] {
        assert_never_opposite(
            &sphere_of_lunes(n),
            288.0 * PI,
            DVec3::new(1.0, 0.5, 0.2),
            &format!("Kugel aus {n} Keilen"),
        );
    }
    assert_never_opposite(
        &two_hemispheres(),
        288.0 * PI,
        DVec3::new(1.0, 0.5, 0.2),
        "zwei Halbkugeln",
    );
}

fn kernel() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, v: DVec3) -> Solid {
    kernel()
        .transform(s, &DMat4::from_translation(v))
        .expect("verschoben")
}

#[test]
fn kernel_written_pocket_and_dome_with_axis_in_the_top_face_never_double() {
    // R3-3: Der Kernel schreibt den Rand als zwei Halbkreise zwischen den Polen; ein gekippter
    // Kantensinn darf nie eine doppelt gezählte Mulde oder Kuppel ergeben. Die Kugelnaht liegt
    // einmal in der Deckfläche (Drehung um x) und einmal senkrecht dazu (Drehung um y: die
    // Mulde trägt die Naht). Das Kippen von Flächenbit oder Rand der Kugelfläche allein ergibt
    // den jeweils anderen gültigen Körper (Mulde ↔ Kuppel; die Datei ist dann dieser Körper);
    // Kanten- und Halbkantenkippungen dürfen nur abgelehnt werden oder denselben Körper ergeben.
    let k = kernel();
    let cube = k.make_box(DVec3::splat(10.0)).expect("Quader");
    let pocket_v = 1000.0 - 18.0 * PI;
    let dome_v = 1000.0 + 18.0 * PI;
    for (name, rot) in [
        ("um x", DMat4::from_rotation_x(PI / 2.0)),
        ("um y", DMat4::from_rotation_y(PI / 2.0)),
    ] {
        let lying = k
            .transform(&k.make_sphere(3.0).expect("Kugel"), &rot)
            .expect("gedreht");
        let ball = moved(&lying, DVec3::new(5.0, 5.0, 10.0));
        let pocket = k.boolean(&cube, &ball, BoolOp::Difference).expect("Mulde");
        let dome = k.boolean(&cube, &ball, BoolOp::Union).expect("Kuppel");
        for (what, body, want, other) in [
            ("Mulde", &pocket, pocket_v, dome_v),
            ("Kuppel", &dome, dome_v, pocket_v),
        ] {
            let (text, _) = export(&[(what, body)]);
            let s = read(&text).unwrap_or_else(|Skip(m)| panic!("{what} {name}: {m}"));
            let v = mesh_volume(&s, 0.002);
            assert!(((v - want) / want).abs() <= 5e-3, "{what} {name}: {v}");
            let flipped_lines: Vec<&str> = text
                .lines()
                .filter(|l| l.ends_with(".T.);") || l.ends_with(".F.);"))
                .filter(|l| {
                    [
                        "ADVANCED_FACE(",
                        "FACE_BOUND(",
                        "FACE_OUTER_BOUND(",
                        "ORIENTED_EDGE(",
                        "EDGE_CURVE(",
                    ]
                    .iter()
                    .any(|t| l.contains(t))
                })
                .collect();
            for (k, t) in single_flips(&text).iter().enumerate() {
                let Ok(s) = read(t) else { continue };
                let v = mesh_volume(&s, 0.002);
                let same = ((v - want) / want).abs() <= 5e-3;
                let complement = ((v - other) / other).abs() <= 5e-3;
                let edge_flip = flipped_lines[k].contains("EDGE_CURVE(")
                    || flipped_lines[k].contains("ORIENTED_EDGE(");
                assert!(
                    same || (complement && !edge_flip),
                    "{what} {name}, Kippung {k} ({}): {v} statt {want}",
                    flipped_lines[k]
                );
            }
        }
    }
}

#[test]
fn more_than_eight_pole_splits_are_made() {
    // R3-4: neun getrennte Halbkugeln mit Großkreiskante (Ecke am Nordpol, der Südpol mitten
    // auf der Kante) brauchen neun Teilungen.
    let mut s = Step::new();
    let mut items = Vec::new();
    for k in 0..9 {
        let c = DVec3::new(20.0 * k as f64, 0.0, 0.0);
        let faces = great_circle_hemisphere_at(&mut s, c, PI / 2.0, 0.0);
        let shell = s.shell(&faces);
        items.push(s.push(format!("MANIFOLD_SOLID_BREP('Halb',#{shell})")));
    }
    let text = s.finish(&items);
    let solid = read(&text).unwrap_or_else(|Skip(m)| panic!("{m}"));
    assert_eq!(solid.shells.len(), 9);
    let v = mesh_volume(&solid, 0.002);
    let want = 9.0 * 144.0 * PI;
    assert!(((v - want) / want).abs() <= 5e-3, "{v} statt {want}");
    // Das Kugelbit der letzten Halbkugel gekippt: abgelehnt oder derselbe Körper.
    let last_sphere = text
        .lines()
        .rfind(|l| l.contains("ADVANCED_FACE("))
        .expect("Fläche")
        .to_string();
    let flipped = text.replace(&last_sphere, &last_sphere.replace(".T.);", ".F.);"));
    assert_ne!(flipped, text);
    if let Ok(s) = read(&flipped) {
        let v = mesh_volume(&s, 0.002);
        assert!(
            ((v - want) / want).abs() <= 5e-3,
            "gekippt: {v} statt {want}"
        );
        assert!(
            winding_at(&s, DVec3::new(160.0, 3.0, 0.0)) > 0.5,
            "gekippt: die letzte Halbkugel liegt auf der anderen Seite"
        );
    }
    // Platte mit fünf Kuppeln, deren Kugelachse in der Deckfläche liegt (zwei Teilungen je
    // Kuppel), vom Kernel geschrieben.
    let k = kernel();
    let mut plate = k.make_box(DVec3::new(60.0, 12.0, 4.0)).expect("Platte");
    let lying = k
        .transform(
            &k.make_sphere(3.0).expect("Kugel"),
            &DMat4::from_rotation_x(PI / 2.0),
        )
        .expect("gedreht");
    for i in 0..5 {
        let ball = moved(&lying, DVec3::new(6.0 + 12.0 * i as f64, 6.0, 4.0));
        plate = k.boolean(&plate, &ball, BoolOp::Union).expect("Kuppel");
    }
    let (text, _) = export(&[("Platte", &plate)]);
    let solid = read(&text).unwrap_or_else(|Skip(m)| panic!("Platte: {m}"));
    let v = mesh_volume(&solid, 0.002);
    let want = 60.0 * 12.0 * 4.0 + 5.0 * 18.0 * PI;
    assert!(
        ((v - want) / want).abs() <= 5e-3,
        "Platte: {v} statt {want}"
    );
}

/// Kegelstumpf (unten Radius `rb` bei z = 0, oben `rt` bei z = `h`, Mantel ohne Naht) und
/// daneben eine kleine Kugel r 0,5 um `c` als zweites Stück. Volumen
/// πh(rb² + rb·rt + rt²)/3 + π/6.
fn frustum_and_ball(rb: f64, rt: f64, h: f64, c: DVec3) -> String {
    let mut s = Step::new();
    let fb = s.axis2(DVec3::ZERO, DVec3::Z, DVec3::X);
    let ft = s.axis2(DVec3::new(0.0, 0.0, h), DVec3::Z, DVec3::X);
    let cb = s.push(format!("CIRCLE('',#{fb},{rb:.17e})"));
    let ct = s.push(format!("CIRCLE('',#{ft},{rt:.17e})"));
    let vb = s.vertex(DVec3::new(rb, 0.0, 0.0));
    let vt = s.vertex(DVec3::new(0.0, rt, h));
    let eb = s.edge(vb, vb, cb, true);
    let et = s.edge(vt, vt, ct, true);
    let pb = s.push(format!("PLANE('',#{fb})"));
    let pt = s.push(format!("PLANE('',#{ft})"));
    // Achse nach unten: Der Radius wächst entlang der Achse (`rt` oben, `rb` unten).
    let fc = s.axis2(DVec3::new(0.0, 0.0, h), -DVec3::Z, DVec3::X);
    let cone = s.push(format!(
        "CONICAL_SURFACE('',#{fc},{rt:.17e},{:.17e})",
        ((rb - rt) / h).atan()
    ));
    let b = s.bound(&[(eb, false)], true);
    let bottom = s.face(&[b], pb, false);
    let b = s.bound(&[(et, true)], true);
    let top = s.face(&[b], pt, true);
    let b1 = s.bound(&[(eb, true)], false);
    let b2 = s.bound(&[(et, false)], false);
    let side = s.face(&[b1, b2], cone, true);
    let frustum = s.shell(&[bottom, top, side]);
    let ball = sphere_shell(&mut s, c, 0.5, true);
    let a = s.push(format!("MANIFOLD_SOLID_BREP('Stumpf',#{frustum})"));
    let b = s.push(format!("MANIFOLD_SOLID_BREP('Kugel',#{ball})"));
    s.finish(&[a, b])
}

#[test]
fn frustum_beside_a_small_ball_is_read() {
    // R3-5: Die Kugel liegt im Hüllquader des Stumpfs, aber mit Abstand außerhalb seines Mantels
    // (spitze Kante am Boden); das Raster darf den Außenraum nicht als innen einstufen.
    for (rb, rt, h) in [
        (5.0, 3.0, 4.0),
        (5.0, 2.0, 6.0),
        (4.0, 2.0, 2.0),
        (6.0, 3.0, 5.0),
    ] {
        let want = PI * h * (rb * rb + rb * rt + rt * rt) / 3.0 + PI / 6.0;
        for k in 0..8 {
            let phi = TAU * (k % 4) as f64 / 4.0 + 0.3;
            // Kugel oben außen neben dem Mantel, mit Abstand ≥ 0,2 und im Hüllquader.
            let z = if k < 4 { 0.8 * h } else { 0.95 * h };
            let rho = rb - 0.6;
            let c = DVec3::new(rho * phi.cos(), rho * phi.sin(), z);
            let text = frustum_and_ball(rb, rt, h, c);
            let solid =
                read(&text).unwrap_or_else(|Skip(m)| panic!("Stumpf {rb}/{rt}/{h}, Lage {k}: {m}"));
            assert_eq!(solid.shells.len(), 2);
            let v = mesh_volume(&solid, 0.002);
            assert!(
                ((v - want) / want).abs() <= 5e-3,
                "Stumpf {rb}/{rt}/{h}, Lage {k}: {v} statt {want}"
            );
        }
    }
}

#[test]
fn great_circle_barely_beside_the_poles_is_split_onto_them() {
    // V04: Der Großkreis läuft 1e-7 neben beiden Polen vorbei (innerhalb der Genauigkeit des
    // Kernels); die Teilungsecken müssen genau auf die Pole gesetzt werden. Läuft die Kurve
    // selbst weiter neben dem Pol vorbei (5e-4 bei Genauigkeit 1e-3), wird abgelehnt: Der Kernel
    // sähe dort keinen Pol.
    let hemisphere = |gap: f64| {
        let mut s = Step::new();
        let faces = great_circle_hemisphere_at(&mut s, DVec3::ZERO, 0.0, gap / SR);
        s.file(&faces)
    };
    let solid = read(&hemisphere(1e-7)).unwrap_or_else(|Skip(m)| panic!("1e-7: {m}"));
    for pole in [DVec3::new(0.0, 0.0, SR), DVec3::new(0.0, 0.0, -SR)] {
        let d = solid
            .vertices
            .iter()
            .map(|(_, v)| v.point.distance(pole))
            .fold(f64::INFINITY, f64::min);
        assert!(d <= 1e-12, "Ecke {d} neben dem Pol {pole}");
    }
    let v = mesh_volume(&solid, 0.002);
    let want = 144.0 * PI;
    assert!(((v - want) / want).abs() <= 5e-3, "{v} statt {want}");
    let coarse = hemisphere(5e-4).replace("LENGTH_MEASURE(1.E-06)", "LENGTH_MEASURE(1.E-03)");
    assert!(coarse.contains("1.E-03"));
    match read(&coarse) {
        Ok(_) => panic!("Kurve 5e-4 neben dem Pol angenommen"),
        Err(Skip(m)) => assert_eq!(
            m,
            "Der Rand der Fläche „STEP-Fläche 2“ läuft nicht genau durch einen Pol."
        ),
    }
}
