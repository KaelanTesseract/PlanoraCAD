//! Stufe 8: deckungsgleiche Stücke, Zusammenfassen, Härtung und Prüfreihe.

mod common;
use common::{check::*, model::*};
use planora_drawing::{DrawBody, Visibility, compute_view, model::ViewKind};
use planora_exchange::Progress;
use std::time::Instant;

fn view(s: &planora_kernel::Solid, k: ViewKind) -> planora_drawing::ViewLines {
    compute_view(
        &[DrawBody {
            name: "K".into(),
            solid: s,
        }],
        k,
        &Progress::new(),
    )
    .unwrap()
}

#[test]
fn torus_front_view_timing() {
    // Leistung der Torus-Strahltreffer (Aufgabe 8, D): Zeit einer Vorderansicht des Vollrings.
    let s = torus_solid(10.0, 3.0);
    let t = Instant::now();
    let v = view(&s, ViewKind::Front);
    eprintln!(
        "Torus Vorderansicht: {:?}, {} Stücke, sichtbar {:.6}, verdeckt {:.6}",
        t.elapsed(),
        v.pieces.len(),
        piece_length_sum(&v, Visibility::Visible),
        piece_length_sum(&v, Visibility::Hidden)
    );
}

/// Teilparameter aller nicht entarteten Striche der Ansicht (Kanten und Umrisse, wie
/// `compute_view` sie bildet) und die Mindestlänge `16·geom_tol(E)` eines Teilstücks.
fn split_ranges(
    s: &planora_kernel::Solid,
    k: ViewKind,
) -> (Vec<planora_drawing::stroke::Stroke>, Vec<Vec<f64>>, f64) {
    use planora_drawing::{basis::Basis, isect::split_params, silhouette::*, stroke::*};
    let b = vec![DrawBody {
        name: "K".into(),
        solid: s,
    }];
    let basis = Basis::of(k);
    let e = extent_of(&b);
    let tiny = 16.0 * (64.0 * f64::EPSILON * e).max(1e-9);
    let mut st = edge_strokes(&b, &basis, e).unwrap();
    st.extend(silhouette_strokes(&b, &basis, e, &Progress::new()).unwrap());
    st.retain(|x| !x.is_degenerate_2d(&basis, tiny));
    let sp = split_params(&st, &basis, e, &Progress::new()).unwrap();
    (st, sp, tiny)
}

#[test]
fn side_on_circles_are_split_once_at_their_turnaround_points() {
    // Aufgabe 6, E4: An der Umkehrstelle eines von der Seite gesehenen Kreises enden die
    // Umrisslinien der Bohrung; die Fußpunkte ihrer Enden lagen auf dem Kreis bis ~1e-8 rad
    // auseinander (Abstand dort von 4. Ordnung flach) und ergaben ein zum Punkt entartetes Stück.
    // Erwartet: kein Teilstück kürzer als 16·geom_tol (eigene Längenmessung), und je Strich
    // dieselbe Zahl Teilstellen bei den Maßstäben 1, 2500 und 0,0025.
    let shapes = [
        box_with_hole(40.0, 20.0, 30.0, 5.0, planora_math::DVec3::X),
        cylinder_solid(5.0, 12.0),
    ];
    for base in &shapes {
        for k in ViewKind::ALL {
            let mut want: Option<Vec<usize>> = None;
            for f in [1.0, 2500.0, 0.0025] {
                let s = transformed(base, f, 0.0);
                let (st, sp, tiny) = split_ranges(&s, k);
                let basis = planora_drawing::basis::Basis::of(k);
                for (x, ts) in st.iter().zip(&sp) {
                    let (lo, hi) = (x.t0.min(x.t1), x.t0.max(x.t1));
                    let mut params = vec![lo];
                    params.extend(ts.iter().copied());
                    params.push(hi);
                    for w in params.windows(2) {
                        let piece = planora_drawing::stroke::Stroke {
                            t0: w[0],
                            t1: w[1],
                            ..x.clone()
                        };
                        let pts = sample2(&piece, &basis, 64);
                        let len: f64 = pts.windows(2).map(|q| q[0].distance(q[1])).sum();
                        assert!(
                            len >= tiny,
                            "Maßstab {f} {k:?} {:?}: Stück [{}, {}] nur {len:e} lang",
                            x.origin,
                            w[0],
                            w[1]
                        );
                    }
                }
                let n: Vec<usize> = sp.iter().map(|t| t.len()).collect();
                match &want {
                    None => want = Some(n),
                    Some(w) => assert_eq!(&n, w, "Maßstab {f} {k:?}"),
                }
            }
        }
    }
}
