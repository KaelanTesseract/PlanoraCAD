//! Stufe 7: Umrisse von Torus und Freiformflächen als angepasste Splines.

mod common;
use common::{check::*, model::*};
use planora_drawing::{
    DrawBody, basis::Basis, compute_view, model::ViewKind, silhouette::*, stroke::*,
};
use planora_exchange::Progress;

fn strokes_of(s: &planora_kernel::Solid, k: ViewKind) -> Vec<Stroke> {
    let b = vec![DrawBody {
        name: "K".into(),
        solid: s,
    }];
    silhouette_strokes(&b, &Basis::of(k), extent_of(&b), &Progress::new()).unwrap()
}

#[test]
fn torus_silhouette_matches_independent_formula_in_all_views() {
    let s = torus_solid(10.0, 3.0);
    for k in ViewKind::ALL {
        let basis = Basis::of(k);
        let st = strokes_of(&s, k);
        // Abstand Punkt gegen Streckenzug: Die Sehnen der Striche müssen feiner sein als die
        // Schranke (400 Stellen ergäben am Kreis r = 13 schon 4e-4 Sehnenabweichung).
        let pts2: Vec<_> = st.iter().map(|x| sample2(x, &basis, 8000)).collect();
        // eigene Formel (tests/common/model.rs), projiziert
        let want: Vec<Vec<_>> = torus_silhouette_points(10.0, 3.0, basis.d, 4000)
            .iter()
            .map(|b| b.iter().map(|p| basis.project(*p)).collect())
            .collect();
        let h = hausdorff(&pts2, &want);
        eprintln!("{k:?}: Hausdorff {h:e}");
        assert!(h <= 1.05e-4, "{k:?}: {h}");
    }
}

#[test]
fn a_large_torus_keeps_the_absolute_outline_accuracy() {
    // Ring ×100 (R = 1000, r = 300 mm): SILHOUETTE_TOL ist absolut (1e-4 mm), der Umriss ist
    // 8 m lang – mehr, als eine Anpassung mit höchstens 512 Kontrollpunkten auf 1e-4 mm halten
    // kann; er wird in Teilen angepasst. Eigene Prüfung je Probepunkt p: Abstand zum Punkt des
    // näheren Zweigs der eigenen Formel beim selben u (obere Schranke des Abstands zum Umriss),
    // dazu die Abdeckung: Die Striche jedes Zweigs überdecken zusammen den ganzen Umlauf in u.
    let (big_r, small_r) = (1000.0, 300.0);
    let s = transformed(&torus_solid(10.0, 3.0), 100.0, 0.0);
    for k in [ViewKind::Top, ViewKind::Isometric] {
        let d = Basis::of(k).d;
        let branch = |u: f64, sign: f64| {
            let a = d.x * u.cos() + d.y * u.sin();
            let h = a.hypot(d.z);
            let (c, sn) = (sign * d.z / h, -sign * a / h);
            let rho = big_r + small_r * c;
            planora_math::DVec3::new(rho * u.cos(), rho * u.sin(), small_r * sn)
        };
        let st = strokes_of(&s, k);
        let mut span = [0.0_f64; 2];
        let mut worst: f64 = 0.0;
        for x in &st {
            let pts = sample3(x, 4000);
            let (mut sum_u, mut prev_u, mut which) = (0.0, None::<f64>, 0usize);
            for p in &pts {
                let u = p.y.atan2(p.x);
                let (d0, d1) = (p.distance(branch(u, 1.0)), p.distance(branch(u, -1.0)));
                which = if d0 <= d1 { 0 } else { 1 };
                worst = worst.max(d0.min(d1));
                if let Some(q) = prev_u {
                    let mut du = u - q;
                    du -= (du / std::f64::consts::TAU).round() * std::f64::consts::TAU;
                    sum_u += du.abs();
                }
                prev_u = Some(u);
            }
            span[which] += sum_u;
        }
        eprintln!(
            "{k:?}: {} Striche, größte Abweichung {worst:e}, Umlauf {span:?}",
            st.len()
        );
        assert!(worst <= 1.05e-4, "{k:?}: {worst}");
        for sp in span {
            assert!(
                (sp - std::f64::consts::TAU).abs() < 1e-6,
                "{k:?}: Umlauf {sp}"
            );
        }
    }
}

#[test]
fn a_tiny_undecidable_piece_at_a_corner_takes_its_neighbours_verdict() {
    // Kuppel mit Überhang von hinten: Ein Umriss endet auf der Schnittkante; dicht daneben
    // (Stück kürzer als OWN_EPS, entstanden aus einer Kreuzung knapp neben dem Ende) streift jeder
    // Prüfstrahl den Rand der Schnittfläche. Das Stück übernimmt das Urteil des Nachbarstücks –
    // des vorigen, wenn es eines gibt, sonst des nächsten –, statt die ganze Ansicht mit „nicht
    // sicher“ abzubrechen. Geprüft vor dem Zusammenfassen (danach ist es mit dem Nachbarn vereint).
    let (s, face) = nurbs_overhang_dome_solid();
    let b = vec![DrawBody {
        name: "U".into(),
        solid: &s,
    }];
    let v = planora_drawing::compute_view_unmerged(&b, ViewKind::Back, &Progress::new()).unwrap();
    let own_eps = planora_drawing::limits::own_eps(extent_of(&b));
    let mut tiny = 0;
    for (k, p) in v.pieces.iter().enumerate() {
        let st = &v.strokes[p.stroke];
        if !matches!(st.origin, StrokeOrigin::Silhouette { face: f, .. } if f == face)
            || st.point(p.t0).distance(st.point(p.t1)) > own_eps
        {
            continue;
        }
        tiny += 1;
        let next_to = |before: bool| {
            v.pieces.iter().enumerate().find(|(j, q)| {
                *j != k && q.stroke == p.stroke && if before { q.t1 == p.t0 } else { q.t0 == p.t1 }
            })
        };
        let (_, q) = next_to(true).or(next_to(false)).expect("Nachbarstück");
        assert_eq!(p.vis, q.vis, "kurzes Stück {p:?} gegen Nachbar {q:?}");
    }
    assert!(tiny > 0, "kein kurzes Stück am Eck");
    // Nach dem Zusammenfassen: dieselbe Ansicht ohne Meldung.
    compute_view(&b, ViewKind::Back, &Progress::new()).unwrap();
}
#[test]
fn the_fit_keeps_its_end_points() {
    // Grundlage für Umrissenden auf der Randkante: NurbsCurve::fit hält ersten und letzten Punkt
    // fest (Ausgleich mit festen Endpunkten bzw. Interpolation). Punkte einer Schraubenlinie,
    // ungleichmäßig, mit verschiedenen Schranken.
    use planora_math::{DVec3, nurbs::NurbsCurve};
    let pts: Vec<DVec3> = (0..=300)
        .map(|i| {
            let t = (i as f64 / 300.0).powf(1.3) * 7.0;
            DVec3::new(
                5.0 * t.cos(),
                5.0 * t.sin(),
                0.7 * t + 0.01 * (3.0 * t).sin(),
            )
        })
        .collect();
    for tol in [1e-2, 1e-4, 5e-5, 1e-6] {
        let c = NurbsCurve::fit(&pts, tol).unwrap();
        let (a, b) = c.domain();
        assert_eq!(c.point(a), pts[0], "Anfang bei {tol}");
        assert_eq!(c.point(b), pts[300], "Ende bei {tol}");
    }
}

#[test]
fn freeform_outline_ends_lie_on_the_edges_and_split_them() {
    // Ruling zu Aufgabe 3: Die Enden eines Umriss-Splines liegen höchstens 16·geom_tol(E) neben
    // der Randkante (eigene Messung: Abstand zum nächsten Punkt der Kantenkurve, Kernel
    // `Curve::project`), und die Kante ist dort geteilt (ein Stückende der Kante liegt dort).
    let (s, face) = nurbs_dome_solid();
    let b = vec![DrawBody {
        name: "D".into(),
        solid: &s,
    }];
    let e = extent_of(&b);
    let near = 16.0 * planora_drawing::limits::geom_tol(e);
    let mut ends = 0;
    for k in [
        ViewKind::Front,
        ViewKind::Left,
        ViewKind::Isometric,
        ViewKind::Back,
    ] {
        let basis = Basis::of(k);
        // Teilung vor dem Zusammenfassen (danach sind gleich sichtbare Teile wieder vereint).
        let v = planora_drawing::compute_view_unmerged(&b, k, &Progress::new()).unwrap();
        for st in v
            .strokes
            .iter()
            .filter(|x| matches!(x.origin, StrokeOrigin::Silhouette { face: f, .. } if f == face))
        {
            if st.point(st.t0) == st.point(st.t1) {
                continue;
            }
            for q in [st.point(st.t0), st.point(st.t1)] {
                // Nächste Kante im Raum.
                let (dist, _) = s
                    .edges
                    .iter()
                    .map(|(_, ed)| {
                        let t = ed.curve.project(q);
                        let t = if ed.curve.period().is_some() {
                            t
                        } else {
                            t.clamp(ed.t0.min(ed.t1), ed.t0.max(ed.t1))
                        };
                        (ed.curve.point(t).distance(q), t)
                    })
                    .fold((f64::INFINITY, 0.0), |a, x| if x.0 < a.0 { x } else { a });
                assert!(dist <= near, "{k:?}: Ende {q:?} {dist:e} neben der Kante");
                ends += 1;
                // Teilung: ein Stückende eines Kantenstrichs liegt im Bild bei q.
                let q2 = basis.project(q);
                let split = v.pieces.iter().any(|p| {
                    let es = &v.strokes[p.stroke];
                    matches!(es.origin, StrokeOrigin::Edge { .. })
                        && [p.t0, p.t1]
                            .iter()
                            .any(|&t| basis.project(es.point(t)).distance(q2) <= 2.0 * near)
                });
                assert!(split, "{k:?}: Kante bei {q:?} nicht geteilt");
            }
        }
    }
    assert!(ends >= 4, "zu wenige Umrissenden geprüft: {ends}");
}

#[test]
fn torus_outline_far_from_the_origin_stays_visible() {
    // F4/OWN_EPS geometrisch: Der Ring liegt 1e6 mm vom Ursprung entfernt. Dort runden die
    // Koordinaten auf etwa 1e-10 mm; ein Umrisspunkt liegt dann bis dahin in seiner Fläche.
    // Jeder Prüfstrahl (auch der nach außen gekippte) verlässt die eigene Fläche erst nach
    // s ≈ Tiefe / Kippwinkel bzw. nach √(2·r·Tiefe), also zwischen SELF_EPS (2,6e-6 mm) und
    // OWN_EPS (2,6e-4 mm). Ohne die OWN_EPS-Regel verdeckte der Umriss sich selbst.
    let s = translated(
        &torus_solid(10.0, 3.0),
        planora_math::DVec3::new(1e6, -1e6, 1e6),
    );
    let b = vec![DrawBody {
        name: "R".into(),
        solid: &s,
    }];
    let e = extent_of(&b);
    let scene = planora_drawing::raycast::Scene::new(&b, e, &Progress::new()).unwrap();
    let surface = s.faces.iter().next().unwrap().1.surface.clone();
    for k in [ViewKind::Top, ViewKind::Bottom] {
        // Einzelurteile an Punkten des wahren Umrisses (wie die Sichtprüfung sie nimmt).
        let basis = Basis::of(k);
        let mut checked = 0;
        for st in strokes_of(&s, k) {
            for i in 0..100 {
                let t = st.t0 + (st.t1 - st.t0) * (i as f64 + 0.5) / 100.0;
                let p = outline_point(&surface, basis.d, st.point(t));
                let vis =
                    planora_drawing::visibility::sample_visibility(&scene, &basis, p, &[0], 0, e);
                assert_eq!(vis, Some(true), "{k:?}: {p:?}");
                checked += 1;
            }
        }
        assert!(checked >= 200, "{k:?}");
        let v = compute_view(
            &[DrawBody {
                name: "R".into(),
                solid: &s,
            }],
            k,
            &Progress::new(),
        )
        .unwrap();
        let outline: Vec<_> = v
            .pieces
            .iter()
            .filter(|p| matches!(v.strokes[p.stroke].origin, StrokeOrigin::Silhouette { .. }))
            .collect();
        assert!(!outline.is_empty(), "{k:?}");
        // Von oben/unten sind Außen- und Innenrand des Rings ganz sichtbar (eigene Anschauung:
        // nichts liegt davor), zusammen 2π·(13 + 7) lang.
        for p in &outline {
            assert_eq!(p.vis, planora_drawing::Visibility::Visible, "{k:?}: {p:?}");
        }
        let len = piece_length_sum(&v, planora_drawing::Visibility::Visible);
        assert!(
            (len - std::f64::consts::TAU * 20.0).abs() < 1e-3,
            "{k:?}: {len}"
        );
    }
}

#[test]
fn torus_views_agree_with_the_mesh_reference() {
    // Alle 7 Ansichten ohne Fehler, Unentschiedenes je Ansicht ≤ 30 %, insgesamt ≤ 5 %.
    let s = torus_solid(10.0, 3.0);
    check_all_views(&[("torus", vec![&s])]);
}

#[test]
fn freeform_silhouette_stays_on_the_surface_and_satisfies_the_condition() {
    let (s, face) = nurbs_dome_solid();
    for k in [ViewKind::Front, ViewKind::Left, ViewKind::Isometric] {
        let basis = Basis::of(k);
        let st = strokes_of(&s, k);
        assert!(!st.is_empty(), "{k:?}");
        let own: Vec<_> = st
            .iter()
            .filter(|x| matches!(x.origin, StrokeOrigin::Silhouette { face: f, .. } if f == face))
            .collect();
        assert!(!own.is_empty(), "{k:?}: kein Umriss der Freiformfläche");
        let (mut dmax, mut gmax) = (0.0_f64, 0.0_f64);
        for stroke in own {
            for p in sample3(stroke, 200) {
                let (dist, g) = surface_residual(&s, face, p, basis.d);
                assert!(dist <= 1.2e-4, "{k:?}: Abstand {dist}");
                assert!(g.abs() <= 2e-3, "{k:?}: n·d = {g}");
                dmax = dmax.max(dist);
                gmax = gmax.max(g.abs());
            }
        }
        eprintln!("{k:?}: größter Abstand {dmax:e}, größtes |n·d| {gmax:e}");
    }
}

#[test]
fn freeform_views_agree_with_the_mesh_reference() {
    // Alle 7 Ansichten ohne Fehler, Unentschiedenes je Ansicht ≤ 30 %, insgesamt ≤ 5 %.
    let (s, _) = nurbs_dome_solid();
    check_all_views(&[("dome", vec![&s])]);
}

#[test]
fn pole_of_a_freeform_face_is_named_not_guessed() {
    let s = nurbs_pole_solid();
    let r = compute_view(
        &[DrawBody {
            name: "P".into(),
            solid: &s,
        }],
        ViewKind::Front,
        &Progress::new(),
    );
    assert!(matches!(
        r,
        Ok(_) | Err(planora_drawing::DrawError::Unsupported(_))
    ));
    // Genauer: Von vorne läuft der Umriss (Meridian) durch den Pol → benannte Meldung, auch in
    // der Gesamtrechnung.
    assert!(
        matches!(r, Err(planora_drawing::DrawError::Unsupported(ref m)) if *m == pole_message()),
        "{:?}",
        r.as_ref().map(|v| v.pieces.len())
    );
}

#[test]
fn pole_away_from_the_outline_is_fine_and_the_outline_closes_across_the_seam() {
    // Von oben liegt der Pol nicht auf dem Umriss: Umriss ist der Kreis größten Radius der
    // Kuppel, eine geschlossene Kette über die Naht (bei 20°) hinweg.
    let s = nurbs_pole_solid();
    let face = s
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, planora_kernel::Surface::Nurbs(_)))
        .map(|(id, _)| id.index())
        .unwrap();
    let st: Vec<_> = strokes_of(&s, ViewKind::Top)
        .into_iter()
        .filter(|x| matches!(x.origin, StrokeOrigin::Silhouette { face: f, .. } if f == face))
        .collect();
    assert_eq!(st.len(), 1, "ein geschlossener Umriss");
    let c = &st[0];
    assert_eq!(c.point(c.t0), c.point(c.t1), "Kette geschlossen");
    // Eigene Rechnung: größter Radius des Profils r(t) = 8(1−t)³ + 30(1−t)²t + 18(1−t)t²,
    // z(t) = 4(1−t)³ + 18(1−t)²t + 30(1−t)t² + 10t³ (Bernstein, fein abgetastet + Goldener Schnitt).
    let r =
        |t: f64| 8.0 * (1.0 - t).powi(3) + 30.0 * (1.0 - t).powi(2) * t + 18.0 * (1.0 - t) * t * t;
    let z = |t: f64| {
        4.0 * (1.0 - t).powi(3)
            + 18.0 * (1.0 - t).powi(2) * t
            + 30.0 * (1.0 - t) * t * t
            + 10.0 * t.powi(3)
    };
    let (mut a, mut b) = (0.0_f64, 1.0_f64);
    for _ in 0..200 {
        let (m1, m2) = (a + (b - a) / 3.0, b - (b - a) / 3.0);
        if r(m1) < r(m2) {
            a = m1;
        } else {
            b = m2;
        }
    }
    let ts = 0.5 * (a + b);
    let (rmax, zmax) = (r(ts), z(ts));
    for p in sample3(c, 400) {
        assert!(
            (p.x.hypot(p.y) - rmax).abs() <= 1.2e-4,
            "{p:?} gegen r = {rmax}"
        );
        assert!((p.z - zmax).abs() <= 1.2e-4, "{p:?} gegen z = {zmax}");
    }
    let r = compute_view(
        &[DrawBody {
            name: "P".into(),
            solid: &s,
        }],
        ViewKind::Top,
        &Progress::new(),
    );
    assert!(r.is_ok(), "{:?}", r.err());
}

/// Abstand zur Fläche und `n·d` für alle Umrisspunkte der Freiformfläche (eigene Rechnung).
fn residuals_ok(
    s: &planora_kernel::Solid,
    face: usize,
    strokes: &[Stroke],
    basis: &Basis,
) -> usize {
    let mut n = 0;
    for st in strokes
        .iter()
        .filter(|x| matches!(x.origin, StrokeOrigin::Silhouette { face: f, .. } if f == face))
    {
        for p in sample3(st, 200) {
            let (dist, g) = surface_residual(s, face, p, basis.d);
            assert!(dist <= 1.2e-4, "{p:?}: Abstand {dist}");
            assert!(g.abs() <= 2e-3, "{p:?}: n·d = {g}");
            n += 1;
        }
    }
    n
}

#[test]
fn tangent_dome_is_drawn_correctly_in_all_views() {
    // Häufigster Freiformfall: Spline-Wand tangential auf einem Zylindersockel. Von oben laufen
    // die Prüfstrahlen der Sockelkante tangential an der Wand entlang (der Kernel verweigert
    // sie). Geprüft in allen 7 Ansichten: Rechnung gelingt, Umrisse liegen auf der Fläche mit
    // n·d ≈ 0 (eigene Fußpunktsuche), und die Sichtbarkeit stimmt mit einem eigenen Urteil gegen
    // das exakte Profil (`DomeJudge`) überein – die Netzreferenz kann tangentiale Übergänge nicht
    // beurteilen.
    let (s, face) = nurbs_tangent_dome_solid();
    let judge = DomeJudge::new(TANGENT_DOME_WALL);
    let b = vec![DrawBody {
        name: "D".into(),
        solid: &s,
    }];
    let (mut total, mut decided, mut residuals) = (0, 0, 0);
    let mut wrong = Vec::new();
    for k in ViewKind::ALL {
        let basis = Basis::of(k);
        let v = compute_view(&b, k, &Progress::new()).unwrap_or_else(|e| panic!("{k:?}: {e:?}"));
        residuals += residuals_ok(&s, face, &v.strokes, &basis);
        for p in &v.pieces {
            let st = &v.strokes[p.stroke];
            let (a, z) = (st.point(p.t0), st.point(p.t1));
            for f in [0.25, 0.5, 0.75] {
                let x = st.point(p.t0 + f * (p.t1 - p.t0));
                if x.distance(a) < 0.2 || x.distance(z) < 0.2 {
                    continue;
                }
                total += 1;
                if let Some(vis) = judge.visible(x, &basis) {
                    decided += 1;
                    if vis != (p.vis == planora_drawing::Visibility::Visible) {
                        wrong.push(format!("{k:?} {:?} {x:?}: {:?}", st.origin, p.vis));
                    }
                }
            }
        }
    }
    eprintln!("Kuppel tangential: {decided}/{total} Stellen entschieden, {residuals} Umrisspunkte");
    assert!(wrong.is_empty(), "{wrong:?}");
    assert!(
        decided * 10 >= total * 6,
        "nur {decided} von {total} entschieden"
    );
    assert!(residuals > 0);
}

#[test]
fn overhang_dome_outlines_stay_on_the_surface_in_all_views() {
    let (s, face) = nurbs_overhang_dome_solid();
    let mut n = 0;
    for k in ViewKind::ALL {
        n += residuals_ok(&s, face, &strokes_of(&s, k), &Basis::of(k));
    }
    assert!(n > 0);
}
#[test]
fn partial_torus_outline_matches_the_formula_on_the_face() {
    // Gerundete Zylinderkante (Teil-Torus R = 7, r = 3, v ∈ [0, π/2]): Der Umriss wird auf die
    // Fläche beschnitten. Eigene Formel nur auf der Fläche (Grenzpunkte genau auf v = 0 bzw.
    // π/2), Hausdorff ≤ 1,05e-4 in allen 7 Ansichten; die Gesamtrechnung gelingt überall.
    let (s, face) = rounded_cylinder_solid();
    let b = vec![DrawBody {
        name: "Z".into(),
        solid: &s,
    }];
    for k in ViewKind::ALL {
        let basis = Basis::of(k);
        let st: Vec<_> = strokes_of(&s, k)
            .into_iter()
            .filter(|x| matches!(x.origin, StrokeOrigin::Silhouette { face: f, .. } if f == face))
            .collect();
        let pts2: Vec<_> = st.iter().map(|x| sample2(x, &basis, 8000)).collect();
        let want: Vec<Vec<_>> = partial_torus_silhouette_points(
            7.0,
            3.0,
            7.0,
            basis.d,
            4000,
            (0.0, std::f64::consts::FRAC_PI_2),
        )
        .iter()
        .map(|r| r.iter().map(|p| basis.project(*p)).collect())
        .collect();
        if want.is_empty() {
            assert!(st.is_empty(), "{k:?}: {} Striche ohne Umriss", st.len());
        } else {
            let h = hausdorff(&pts2, &want);
            eprintln!("Teil-Torus {k:?}: {} Striche, Hausdorff {h:e}", st.len());
            assert!(h <= 1.05e-4, "{k:?}: {h}");
        }
        compute_view(&b, k, &Progress::new()).unwrap_or_else(|e| panic!("{k:?}: {e:?}"));
    }
}

#[test]
fn many_tori_share_no_work_budget() {
    // 40 Ringe: Der Aufwand der Umrissberechnung gilt je Fläche, nicht für alle zusammen.
    let rings: Vec<_> = (0..40)
        .map(|i| {
            translated(
                &torus_solid(10.0, 3.0),
                planora_math::DVec3::new(30.0 * i as f64, 0.0, 0.0),
            )
        })
        .collect();
    let b: Vec<_> = rings
        .iter()
        .map(|s| DrawBody {
            name: "R".into(),
            solid: s,
        })
        .collect();
    let st = silhouette_strokes(
        &b,
        &Basis::of(ViewKind::Isometric),
        extent_of(&b),
        &Progress::new(),
    )
    .unwrap();
    assert_eq!(st.len(), 80);
}

#[test]
fn a_view_just_off_the_torus_plane_is_drawn_or_named() {
    // Blick knapp neben der Ringebene (|d_z| = 1e-7): v(u) springt an den Meridianen beinahe;
    // erwartet ist ein genauer Umriss oder eine benannte Meldung, nie ein Absturz oder Raten.
    let s = rotated(&torus_solid(10.0, 3.0), planora_math::DVec3::X, 1e-7);
    let basis = Basis::of(ViewKind::Front);
    let b = vec![DrawBody {
        name: "R".into(),
        solid: &s,
    }];
    match silhouette_strokes(&b, &basis, extent_of(&b), &Progress::new()) {
        Ok(st) => {
            // Eigene Formel im gedrehten Rahmen: Blickrichtung in Ringkoordinaten.
            let r = planora_math::DQuat::from_axis_angle(planora_math::DVec3::X, 1e-7);
            let dl = r.inverse() * basis.d;
            let pts2: Vec<_> = st.iter().map(|x| sample2(x, &basis, 8000)).collect();
            let want: Vec<Vec<_>> = torus_silhouette_points(10.0, 3.0, dl, 40_000)
                .iter()
                .map(|b| b.iter().map(|p| basis.project(r * *p)).collect())
                .collect();
            let h = hausdorff(&pts2, &want);
            eprintln!(
                "knapp neben der Ringebene: {} Striche, Hausdorff {h:e}",
                st.len()
            );
            assert!(h <= 1.05e-4, "{h}");
        }
        Err(planora_drawing::DrawError::Unsupported(m)) => {
            eprintln!("knapp neben der Ringebene: {m}");
            assert!(
                m.starts_with("Umriss lässt sich nicht genau genug annähern"),
                "{m}"
            );
        }
        Err(planora_drawing::DrawError::TooComplex(n)) => eprintln!("zu aufwendig: {n}"),
        Err(e) => panic!("{e:?}"),
    }
}
