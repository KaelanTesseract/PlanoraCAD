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

#[test]
fn plate_with_a_thousand_holes_from_the_top_is_cheap() {
    // Aufgabe 3, (4): Ober- und Unterkante jeder Bohrung fallen von oben zusammen. Früher kostete
    // jedes solche Kreispaar ~12 000 Zerlegungsschritte; jetzt erkennt `isect` die gleiche
    // Bildellipse vorab. Gemessen wird der Paarzähler der Teilstellensuche.
    use planora_drawing::stroke::*;
    use planora_drawing::{basis::Basis, isect::split_params_counted, limits::*, silhouette::*};
    let t = Instant::now();
    let s = plate_with_holes(40, 25, 2.0, 0.5, 3.0);
    eprintln!("Platte gebaut in {:?}", t.elapsed());
    let b = vec![DrawBody {
        name: "P".into(),
        solid: &s,
    }];
    let basis = Basis::of(ViewKind::Top);
    let e = extent_of(&b);
    let mut st = edge_strokes(&b, &basis, e).unwrap();
    st.extend(silhouette_strokes(&b, &basis, e, &Progress::new()).unwrap());
    st.retain(|x| !x.is_degenerate_2d(&basis, 16.0 * geom_tol(e)));
    let circles = st
        .iter()
        .filter(|x| matches!(x.curve, planora_kernel::Curve::Circle { .. }))
        .count();
    assert_eq!(circles, 2000);
    let pairs = std::cell::Cell::new(0);
    let t = Instant::now();
    split_params_counted(&st, &basis, e, &pairs, &Progress::new()).unwrap();
    eprintln!(
        "{} Striche, {} Paartests ({:.3} % von MAX_PAIR_TESTS) in {:?}",
        st.len(),
        pairs.get(),
        100.0 * pairs.get() as f64 / MAX_PAIR_TESTS as f64,
        t.elapsed()
    );
    assert!(pairs.get() * 100 < MAX_PAIR_TESTS, "{}", pairs.get());
    let t = Instant::now();
    let v = view(&s, ViewKind::Top);
    eprintln!("Draufsicht: {} Stücke in {:?}", v.pieces.len(), t.elapsed());
    // Rechteck 80 × 50 und 1000 Kreise, je einmal sichtbar (eigene Längenmessung).
    let want = 2.0 * (80.0 + 50.0) + 1000.0 * std::f64::consts::TAU * 0.5;
    let got = piece_length_sum(&v, Visibility::Visible);
    assert!(got >= want - 1e-6, "{got} gegen {want}");
}

#[test]
fn front_view_of_box_is_exactly_four_visible_pieces() {
    let v = view(&box_solid(10.0, 20.0, 30.0), ViewKind::Front);
    assert_eq!(v.pieces.len(), 4);
    assert!(v.pieces.iter().all(|p| p.vis == Visibility::Visible));
    assert!((piece_length_sum(&v, Visibility::Visible) - 80.0).abs() < 1e-6);
}

#[test]
fn top_view_of_cylinder_is_one_circle() {
    let v = view(&cylinder_solid(5.0, 12.0), ViewKind::Top);
    assert_eq!(v.pieces.len(), 1);
    assert!(
        (piece_length_sum(&v, Visibility::Visible) - 2.0 * std::f64::consts::PI * 5.0).abs() < 1e-6
    );
}

#[test]
fn front_view_of_cylinder_is_rectangle_outline() {
    // Mantellinien (2·12) + obere und untere Kreiskante als Strecken der Länge 10 (zwei Hälften
    // je Seite fallen zusammen)
    let v = view(&cylinder_solid(5.0, 12.0), ViewKind::Front);
    assert!(
        (piece_length_sum(&v, Visibility::Visible) - (2.0 * 12.0 + 2.0 * 10.0)).abs() < 1e-6,
        "{}",
        piece_length_sum(&v, Visibility::Visible)
    );
    assert_eq!(piece_length_sum(&v, Visibility::Hidden), 0.0);
}

#[test]
fn visible_wins_over_hidden_when_coincident() {
    // Quader mit Bohrung längs Y: Lochkreis vorne (sichtbar) und hinten (verdeckt, aber
    // deckungsgleich) → ein sichtbarer Kreis
    let s = box_with_hole(40.0, 20.0, 30.0, 5.0, planora_math::DVec3::Y);
    let v = view(&s, ViewKind::Front);
    assert_eq!(piece_length_sum(&v, Visibility::Hidden), 0.0);
    let circle: f64 = 2.0 * std::f64::consts::PI * 5.0;
    assert!(
        (piece_length_sum(&v, Visibility::Visible) - (2.0 * (40.0 + 30.0) + circle)).abs() < 1e-6,
        "{}",
        piece_length_sum(&v, Visibility::Visible)
    );
}

#[test]
fn a_truly_hidden_edge_under_a_visible_edge_is_dropped() {
    // Zusatz zum Mutationsbeweis (a): Bei der Bohrung oben ist der hintere Kreis nach F9 selbst
    // sichtbar (Kippstrahlen ins Loch sind frei). Hier liegt eine wirklich verdeckte Kante
    // deckungsgleich unter einer sichtbaren: Körper A (Grundriss (0,0) → (10,0) → (10,1) →
    // (20,1) → (20,5) → (0,5), Höhe 20, rechte Hälfte um 1 zurückgesetzt) hat bei x = 10 zwei
    // senkrechte Kanten (y = 0 und y = 1), beide sichtbar. Körper B = [10,15] × [10,15] × [5,15]
    // steht ganz dahinter; seine Kanten bei x = 10 fallen auf A's Stufenkante, alle seine Kanten
    // sind verdeckt. Erwartet (eigene Rechnung): sichtbar 2·(20 + 20) + 20 = 100, verdeckt nur
    // B's Kanten bei x = 15 (10), z = 5 und z = 15 (je 5) = 20 – die Kante x = 10 (10) nicht.
    use planora_kernel::{Profile, ProfileLoop, Segment, extrude};
    use planora_math::{DVec2, Frame};
    let pts = [
        (10.0, 0.0),
        (10.0, 1.0),
        (20.0, 1.0),
        (20.0, 5.0),
        (0.0, 5.0),
        (0.0, 0.0),
    ];
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: pts
                .iter()
                .map(|&(x, y)| Segment::Line {
                    to: DVec2::new(x, y),
                })
                .collect(),
        },
        holes: vec![],
    };
    let a = extrude(
        &profile,
        &Frame::WORLD,
        20.0,
        &planora_math::Tolerance::default(),
    )
    .unwrap();
    let b = box_at((10.0, 10.0, 5.0), (15.0, 15.0, 15.0));
    let v = compute_view(
        &[
            DrawBody {
                name: "A".into(),
                solid: &a,
            },
            DrawBody {
                name: "B".into(),
                solid: &b,
            },
        ],
        ViewKind::Front,
        &Progress::new(),
    )
    .unwrap();
    let (vis, hid) = (
        piece_length_sum(&v, Visibility::Visible),
        piece_length_sum(&v, Visibility::Hidden),
    );
    assert!((vis - 100.0).abs() < 1e-6, "sichtbar {vis}");
    assert!((hid - 20.0).abs() < 1e-6, "verdeckt {hid}");
}

#[test]
fn compute_view_is_deterministic() {
    let s = box_with_hole(40.0, 20.0, 30.0, 5.0, planora_math::DVec3::X);
    let a = view(&s, ViewKind::Isometric);
    let b = view(&s, ViewKind::Isometric);
    assert_eq!(a.pieces, b.pieces);
}

#[test]
fn isometric_box_edges_are_single_pieces() {
    // Zusatz zum Mutationsbeweis (c): Im Quader 10 × 20 × 30 isometrisch kreuzen die drei
    // verdeckten Kanten die drei inneren sichtbaren Kanten; beide werden dort geteilt und müssen
    // wieder zu je einem Stück vereint werden: 9 sichtbare und 3 verdeckte Stücke.
    let v = view(&box_solid(10.0, 20.0, 30.0), ViewKind::Isometric);
    let raw = planora_drawing::compute_view_unmerged(
        &[DrawBody {
            name: "K".into(),
            solid: &box_solid(10.0, 20.0, 30.0),
        }],
        ViewKind::Isometric,
        &Progress::new(),
    )
    .unwrap();
    assert!(raw.pieces.len() > 12, "ungeteilt: {}", raw.pieces.len());
    assert_eq!(counts(&v), (9, 3));
}

#[test]
fn a_circle_split_by_crossings_is_joined_across_its_seam() {
    // Kugel (r = 5) von oben, darüber ein Riegel [−1,1] × [−10,10] × [10,12] (eigener Körper):
    // Er verdeckt den Umrisskreis der Kugel um θ = π/2 und 3π/2 (x ≈ 0). Der Kreis beginnt bei
    // θ = 0 (Naht), mitten in einem sichtbaren Teil. Vor dem Zusammenfassen 5 Stücke (sichtbar,
    // verdeckt, sichtbar, verdeckt, sichtbar); danach 4: die beiden sichtbaren Teile an der Naht
    // sind ein Stück, das über die Naht reicht.
    let k = sphere_solid(5.0);
    let bar = box_at((-1.0, -10.0, 10.0), (1.0, 10.0, 12.0));
    let bodies = [
        DrawBody {
            name: "K".into(),
            solid: &k,
        },
        DrawBody {
            name: "R".into(),
            solid: &bar,
        },
    ];
    let outline = |v: &planora_drawing::ViewLines| -> Vec<planora_drawing::Piece> {
        v.pieces
            .iter()
            .filter(|p| {
                matches!(
                    v.strokes[p.stroke].origin,
                    planora_drawing::stroke::StrokeOrigin::Silhouette { body: 0, .. }
                )
            })
            .copied()
            .collect()
    };
    let raw =
        planora_drawing::compute_view_unmerged(&bodies, ViewKind::Top, &Progress::new()).unwrap();
    let r = outline(&raw);
    assert_eq!(r.len(), 5, "ungeteilt: {r:?}");
    let v = compute_view(&bodies, ViewKind::Top, &Progress::new()).unwrap();
    let o = outline(&v);
    assert_eq!(o.len(), 4, "{o:?}");
    let tau = std::f64::consts::TAU;
    let across: Vec<_> = o.iter().filter(|p| p.t1 > tau).collect();
    assert_eq!(across.len(), 1, "{o:?}");
    assert_eq!(across[0].vis, Visibility::Visible);
    // Das Stück über die Naht deckt die beiden früheren Randstücke ab.
    assert_eq!(across[0].t0, r[4].t0);
    assert_eq!(across[0].t1, r[0].t1 + tau);
    assert_eq!(o.iter().filter(|p| p.vis == Visibility::Hidden).count(), 2);
}
