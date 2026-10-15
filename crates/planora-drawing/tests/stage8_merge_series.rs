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
    // Großzügige Grenze (vorher 17,7 s debug / 2,1 s release, jetzt ~0,4 s / 0,1 s).
    assert!(t.elapsed().as_secs_f64() < 3.0, "{:?}", t.elapsed());
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

/// Kennzahlen einer Teilreihe.
#[derive(Default, Debug)]
struct SeriesStats {
    ok: usize,
    named: usize,
    undecided: usize,
    sampled: usize,
    /// (e)-Abweichungen der Netzreferenz, die der exakte Richter am Formelmodell zugunsten der
    /// Bibliothek entschied (Material dünner als die Netzgenauigkeit).
    exact_agrees: usize,
    /// (e)-Abweichungen, bei denen auch der exakte Richter unentschieden ist (zählen als
    /// unentschieden).
    exact_undecided: usize,
    /// Merkmalskanten des Grundnetzes, die das feinere Netz nicht bestätigt.
    refined_features: usize,
    /// (f): verdeckt unter sichtbar, sichtbar auf sichtbar (müssen 0 sein).
    hidden_under_visible: usize,
    visible_on_visible: usize,
    /// Teilstellen, die `compute_view` als entartet verwarf (`ViewLines::dropped_splits`).
    dropped_splits: usize,
}

/// Prüfreihe (Aufgabe 8): Zufallskörper `seeds` (tests/common/model.rs, `random_body_csg`), je
/// alle 7 Ansichten. Erlaubt: Ok – dann alle Prüfungen von `check_view` ohne Fehler und je
/// Ansicht höchstens 30 % unentschiedene Referenzstellen – oder eine benannte Meldung
/// (`AmbiguousVisibility`, `TooComplex`, `Unsupported`). Weicht die Netzreferenz in (e) von der
/// Bibliothek ab, prüft der exakte Richter am Formelmodell (`CsgJudge`) nach: Gibt er der
/// Bibliothek recht, war das Netz zu grob (gezählt); urteilt er anders, ist es ein Fehler.
fn series(seeds: std::ops::RangeInclusive<u64>) -> SeriesStats {
    use planora_drawing::DrawError;
    let mut st = SeriesStats::default();
    let mut failures: Vec<String> = Vec::new();
    let t = Instant::now();
    for seed in seeds.clone() {
        let (s, csg) = random_body_csg(seed);
        let judge = CsgJudge::new(csg, &s);
        for k in ViewKind::ALL {
            match compute_view(
                &[DrawBody {
                    name: "R".into(),
                    solid: &s,
                }],
                k,
                &Progress::new(),
            ) {
                Ok(v) => {
                    st.dropped_splits += v.dropped_splits;
                    let r = check_lines(&[&s], k, &v);
                    let basis = planora_drawing::basis::Basis::of(k);
                    let mut resolved = vec![false; r.errors.len()];
                    let mut extra_undecided = 0;
                    for &(i, x, lib) in &r.vis_diffs {
                        match judge.visible(x, &basis) {
                            Some(v) if v == lib => {
                                resolved[i] = true;
                                st.exact_agrees += 1;
                            }
                            Some(_) => {}
                            None => {
                                resolved[i] = true;
                                extra_undecided += 1;
                            }
                        }
                    }
                    for (e, done) in r.errors.iter().zip(&resolved) {
                        if !done {
                            failures.push(format!("seed {seed} {k:?}: {e}"));
                        }
                    }
                    let undecided = r.undecided + extra_undecided;
                    if undecided * 10 > r.sampled * 3 {
                        failures.push(format!(
                            "seed {seed} {k:?}: Referenz unentschieden an {undecided} von {} Stellen",
                            r.sampled
                        ));
                    }
                    st.exact_undecided += extra_undecided;
                    st.undecided += undecided;
                    st.sampled += r.sampled;
                    st.refined_features += r.refined_features;
                    st.hidden_under_visible += r.hidden_under_visible;
                    st.visible_on_visible += r.visible_on_visible;
                    st.ok += 1;
                }
                Err(
                    e @ (DrawError::AmbiguousVisibility(_)
                    | DrawError::TooComplex(_)
                    | DrawError::Unsupported(_)),
                ) => {
                    eprintln!("seed {seed} {k:?}: benannt: {e}");
                    st.named += 1;
                }
                Err(e) => failures.push(format!("seed {seed} {k:?}: {e}")),
            }
        }
    }
    eprintln!(
        "Reihe {seeds:?}: Ok {}, benannt {}, unentschieden {}/{} ({:.2} %), {:?}",
        st.ok,
        st.named,
        st.undecided,
        st.sampled,
        100.0 * st.undecided as f64 / st.sampled.max(1) as f64,
        t.elapsed()
    );
    eprintln!(
        "Reihe {seeds:?}: exakt bestätigt {}, exakt unentschieden (verdeckt, nicht bestätigt) {}, \
         grobe Netzkanten {}, (f) verdeckt unter sichtbar {}, sichtbar auf sichtbar {},          verworfene Teilstellen {}",
        st.exact_agrees,
        st.exact_undecided,
        st.refined_features,
        st.hidden_under_visible,
        st.visible_on_visible,
        st.dropped_splits
    );
    for f in &failures {
        eprintln!("FEHLER {f}");
    }
    assert!(
        failures.is_empty(),
        "{} Fehler: {failures:#?}",
        failures.len()
    );
    st
}

/// Grenzen je Teilreihe: benannte Meldungen höchstens 10 % der Läufe, unentschiedene
/// Referenzstellen insgesamt höchstens 5 % (wie Aufgabe 6/7).
fn assert_series(seeds: std::ops::RangeInclusive<u64>) {
    let st = series(seeds);
    assert!(
        st.named * 10 <= st.ok + st.named,
        "zu viele benannte Fehler: {} von {}",
        st.named,
        st.ok + st.named
    );
    assert!(
        st.undecided * 20 <= st.sampled,
        "Referenz unentschieden an {} von {} Stellen",
        st.undecided,
        st.sampled
    );
}

// 200 Körper in 8 Teilreihen zu 25 Startwerten (parallel, Fehler genau zuzuordnen).
#[test]
fn series_seeds_001_025() {
    assert_series(1..=25);
}
#[test]
fn series_seeds_026_050() {
    assert_series(26..=50);
}
#[test]
fn series_seeds_051_075() {
    assert_series(51..=75);
}
#[test]
fn series_seeds_076_100() {
    assert_series(76..=100);
}
#[test]
fn series_seeds_101_125() {
    assert_series(101..=125);
}
#[test]
fn series_seeds_126_150() {
    assert_series(126..=150);
}
#[test]
fn series_seeds_151_175() {
    assert_series(151..=175);
}
#[test]
fn series_seeds_176_200() {
    assert_series(176..=200);
}

#[test]
fn the_exact_judge_agrees_with_the_mesh_reference() {
    // Gegenprobe des exakten Richters (CsgJudge), der in der Reihe Abweichungen des Netzes
    // entscheidet: An den Mitten aller Stücke der Körper 1–20 in allen Ansichten müssen beide
    // übereinstimmen, wo beide entschieden sind; Abweichungen nur dort, wo das Netz das Material
    // nicht auflöst (gezählt, höchstens 1 %).
    use common::mesh_ref::{RefScene, fine_mesh};
    let (mut both, mut differ) = (0, 0);
    for seed in 1..=20u64 {
        let (s, csg) = random_body_csg(seed);
        let judge = CsgJudge::new(csg, &s);
        let extent = judge.extent;
        let mesh = fine_mesh(&s, extent);
        for k in ViewKind::ALL {
            let basis = planora_drawing::basis::Basis::of(k);
            let scene = RefScene::new(std::slice::from_ref(&mesh), basis, extent);
            let v = view(&s, k);
            for p in &v.pieces {
                let x = v.strokes[p.stroke].point(0.5 * (p.t0 + p.t1));
                if let (Some(a), Some(b)) = (scene.ref_visibility(x), judge.visible(x, &basis)) {
                    both += 1;
                    if a != b {
                        differ += 1;
                        eprintln!("seed {seed} {k:?} {x:?}: Netz {a}, exakt {b}");
                    }
                }
            }
        }
    }
    eprintln!("beide entschieden: {both}, verschieden: {differ}");
    assert!(both > 500, "{both}");
    assert!(differ * 100 <= both, "{differ} von {both}");
}

/// `2 · 50 · rows` Quader 1 × 1 × 1 mm im Raster (Abstand 2 mm) in der XZ-Ebene, je ein eigener
/// Körper (neu konstruiert und verschoben), zwei Lagen hintereinander (y = 0 und y = 3), die
/// zweite um 0,5 mm in X versetzt (teils verdeckt).
fn box_grid(rows: usize) -> Vec<planora_kernel::Solid> {
    let mut out = Vec::with_capacity(100 * rows);
    for layer in 0..2 {
        for i in 0..50 {
            for j in 0..rows {
                let lo = (
                    2.0 * i as f64 + 0.5 * layer as f64,
                    3.0 * layer as f64,
                    2.0 * j as f64,
                );
                out.push(box_at(lo, (lo.0 + 1.0, lo.1 + 1.0, lo.2 + 1.0)));
            }
        }
    }
    out
}

fn draw_bodies(solids: &[planora_kernel::Solid]) -> Vec<DrawBody<'_>> {
    solids
        .iter()
        .enumerate()
        .map(|(i, s)| DrawBody {
            name: format!("Q{i}"),
            solid: s,
        })
        .collect()
}

/// Ergebnis oder `TooComplex`, in weniger als 60 s.
fn ok_or_too_complex_in_time(bodies: &[DrawBody], k: ViewKind) {
    use planora_drawing::DrawError;
    let t = Instant::now();
    let r = compute_view(bodies, k, &Progress::new());
    let dt = t.elapsed();
    match &r {
        Ok(v) => eprintln!(
            "{} Körper {k:?}: Ergebnis mit {} Stücken in {dt:?}",
            bodies.len(),
            v.pieces.len()
        ),
        Err(e) => eprintln!("{} Körper {k:?}: {e} nach {dt:?}", bodies.len()),
    }
    assert!(
        matches!(r, Ok(_) | Err(DrawError::TooComplex(_))),
        "{k:?}: {:?}",
        r.err()
    );
    assert!(dt.as_secs_f64() < 60.0, "{k:?}: {dt:?}");
}

#[test]
fn many_bodies_cost_limit_is_reported_not_exceeded() {
    // 2 000 kleine Quader in einem Raster: entweder Ergebnis oder TooComplex, innerhalb von 60 s;
    // Abbruch über Progress wirkt binnen 2 s. Vorderansicht: 16 000 Striche (die Kanten längs der
    // Blickrichtung entfallen) – unter MAX_STROKES, wird gerechnet; isometrisch 24 000 Striche →
    // TooComplex.
    use planora_drawing::DrawError;
    let solids = box_grid(20);
    assert_eq!(solids.len(), 2000);
    let bodies = draw_bodies(&solids);
    for k in [ViewKind::Front, ViewKind::Isometric] {
        ok_or_too_complex_in_time(&bodies, k);
    }
    // Abbruch nach 500 ms (wie im Brief). Ist die Rechnung schneller fertig, gilt das Ergebnis.
    // Das Warten auf den Fortschritt hat eine Frist (60 s) und endet auch, wenn die Rechnung
    // schon fertig ist: Der Test scheitert dann, statt zu hängen.
    use std::sync::atomic::{AtomicBool, Ordering};
    let cancel_after =
        |progress: std::sync::Arc<Progress>, finished: std::sync::Arc<AtomicBool>, wait: bool| {
            std::thread::spawn(move || {
                if wait {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                } else {
                    // Sobald die Sichtprüfung läuft (Fortschritt > 0).
                    let deadline = Instant::now() + std::time::Duration::from_secs(60);
                    while progress.fraction() == 0.0
                        && !finished.load(Ordering::SeqCst)
                        && Instant::now() < deadline
                    {
                        std::thread::yield_now();
                    }
                }
                progress.cancel();
                Instant::now()
            })
        };
    for wait in [true, false] {
        let progress = std::sync::Arc::new(Progress::new());
        let finished = std::sync::Arc::new(AtomicBool::new(false));
        let canceller = cancel_after(progress.clone(), finished.clone(), wait);
        let t = Instant::now();
        let r = compute_view(&bodies, ViewKind::Front, &progress);
        finished.store(true, Ordering::SeqCst);
        let done = Instant::now();
        let cancelled_at = canceller.join().unwrap();
        let lag = done.saturating_duration_since(cancelled_at);
        eprintln!(
            "Abbruch ({}): Ende nach {:?}, {lag:?} nach dem Abbruchwunsch: {:?}",
            if wait {
                "nach 500 ms"
            } else {
                "während der Sichtprüfung"
            },
            t.elapsed(),
            r.as_ref().map(|v| v.pieces.len())
        );
        match r {
            Err(DrawError::Cancelled) => assert!(lag.as_secs_f64() < 2.0, "{lag:?}"),
            // Nur beim festen Zeitpunkt darf die Rechnung schon fertig sein.
            Ok(_) if wait && done <= cancelled_at => {}
            other => panic!("{:?}", other.map(|v| v.pieces.len())),
        }
    }
}

#[test]
fn horn_and_spindle_tori_give_a_result_or_a_named_error() {
    // Hinweis aus der Prüfung von Aufgabe 8: Drehkörper können Horn- (R = r) und Spindeltori
    // (R < r) bilden; die Berührungserkennung nahe der Selbstberührung ist sonst ungeprüft.
    // Erwartet je Ansicht: Ergebnis oder benannte Meldung, nie ein Absturz.
    // Gebaut wie der Vollring der Tests (eine Torusfläche, zwei Nähte), mit R = r (Horn),
    // R < r (Spindel) und knapp darüber.
    use planora_drawing::DrawError;
    let mut built = 0;
    for (center, r) in [(3.0, 3.0), (2.0, 3.0), (3.0, 2.999)] {
        let Some(s) = try_torus_solid(center, r) else {
            eprintln!("R = {center}, r = {r}: Kernel lehnt ab");
            continue;
        };
        built += 1;
        for k in ViewKind::ALL {
            let r2 = compute_view(
                &[DrawBody {
                    name: "T".into(),
                    solid: &s,
                }],
                k,
                &Progress::new(),
            );
            eprintln!(
                "R = {center}, r = {r} {k:?}: {:?}",
                r2.as_ref().map(|v| v.pieces.len())
            );
            assert!(
                matches!(
                    r2,
                    Ok(_)
                        | Err(DrawError::AmbiguousVisibility(_)
                            | DrawError::TooComplex(_)
                            | DrawError::Unsupported(_)
                            | DrawError::InvalidBody(_))
                ),
                "{r2:?}"
            );
        }
    }
    eprintln!("{built} Tori gebaut");
    assert!(built >= 2, "{built}");
}

#[test]
fn a_wave_touching_a_line_at_its_quarters_is_not_dropped_as_covered() {
    // N1: Eine Welle mit zwei Perioden (vier Bögen als kubische Bézierstücke, an den Vierteln
    // genau auf z = 0, dort C1) liegt mit beiden Enden auf einer Geraden und kreuzt sie bei ¼, ½
    // und ¾ ihres Parameters. Von vorne gesehen bedeckt die Gerade die Welle nicht; die Welle
    // muss ganz erhalten bleiben (früher entschieden nur die Stellen ¼, ½, ¾ – alle auf der
    // Geraden).
    use planora_drawing::{Piece, basis::Basis, stroke::*, visibility::merge_pieces};
    use planora_kernel::Curve;
    use planora_math::DVec3;
    use planora_math::nurbs::NurbsCurve;
    let (len, h) = (10.0, 1.0);
    let mut ctrl = Vec::new();
    for k in 0..4 {
        let x0 = 0.25 * len * k as f64;
        let s = if k % 2 == 0 { h } else { -h };
        let step = 0.25 * len / 3.0;
        if k == 0 {
            ctrl.push(DVec3::new(x0, 0.0, 0.0));
        }
        ctrl.push(DVec3::new(x0 + step, 0.0, s));
        ctrl.push(DVec3::new(x0 + 2.0 * step, 0.0, s));
        ctrl.push(DVec3::new(x0 + 3.0 * step, 0.0, 0.0));
    }
    let knots = vec![
        0.0, 0.0, 0.0, 0.0, 0.25, 0.25, 0.25, 0.5, 0.5, 0.5, 0.75, 0.75, 0.75, 1.0, 1.0, 1.0, 1.0,
    ];
    let wave = NurbsCurve::new(3, knots, ctrl, vec![1.0; 13]).unwrap();
    // Eigene Prüfung der Vorlage: Enden und Viertel auf der Geraden, dazwischen nicht.
    for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
        assert!(wave.point(t).z.abs() < 1e-12, "t = {t}");
    }
    assert!(wave.point(0.381_966).z.abs() > 0.1 && wave.point(0.125).z.abs() > 0.1);
    let strokes = vec![
        line_stroke((0.0, 0.0, 0.0), (1.0, 0.0, 0.0), len),
        Stroke {
            curve: Curve::nurbs(wave),
            t0: 0.0,
            t1: 1.0,
            origin: StrokeOrigin::Edge { body: 0, edge: 1 },
        },
    ];
    let pieces = vec![
        Piece {
            stroke: 0,
            t0: 0.0,
            t1: len,
            vis: Visibility::Visible,
        },
        Piece {
            stroke: 1,
            t0: 0.0,
            t1: 1.0,
            vis: Visibility::Visible,
        },
    ];
    let basis = Basis::of(ViewKind::Front);
    let out = merge_pieces(&strokes, pieces, &basis, len, &Progress::new()).unwrap();
    let wave_len: f64 = out
        .iter()
        .filter(|p| p.stroke == 1)
        .map(|p| p.t1 - p.t0)
        .sum();
    assert_eq!(wave_len, 1.0, "{out:?}");
    assert!(out.iter().any(|p| p.stroke == 0 && p.t1 - p.t0 == len));
}
