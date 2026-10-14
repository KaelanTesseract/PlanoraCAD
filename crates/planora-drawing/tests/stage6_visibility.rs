mod common;
use common::{check::*, model::*};
use planora_drawing::{DrawBody, Visibility, compute_view, model::ViewKind};
use planora_exchange::Progress;

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
fn len(v: &planora_drawing::ViewLines, vis: Visibility) -> f64 {
    piece_length_sum(v, vis)
} // eigene Längensumme, tests/common/check.rs

#[test]
fn isometric_box_has_nine_visible_and_three_hidden_edges() {
    let (a, b, c) = (10.0, 20.0, 30.0);
    let v = view(&box_solid(a, b, c), ViewKind::Isometric);
    let k = (2.0_f64 / 3.0).sqrt(); // projizierte Länge je Achseneinheit
    assert!(
        (len(&v, Visibility::Visible) - 3.0 * (a + b + c) * k).abs() < 1e-6,
        "{}",
        len(&v, Visibility::Visible)
    );
    assert!(
        (len(&v, Visibility::Hidden) - (a + b + c) * k).abs() < 1e-6,
        "{}",
        len(&v, Visibility::Hidden)
    );
}

#[test]
fn front_view_of_box_has_no_hidden_pieces() {
    let v = view(&box_solid(10.0, 20.0, 30.0), ViewKind::Front);
    assert_eq!(len(&v, Visibility::Hidden), 0.0);
    assert!(len(&v, Visibility::Visible) >= 2.0 * (10.0 + 30.0) - 1e-9); // Rechteck (hintere Kopien dürfen dazukommen, Aufgabe 8 vereint sie)
}

#[test]
fn through_hole_along_x_shows_hidden_lines_from_the_front() {
    let s = box_with_hole(40.0, 20.0, 30.0, 5.0, planora_math::DVec3::X); // Bohrung längs X auf Höhe z = 15, y = 10
    let v = view(&s, ViewKind::Front);
    // verdeckt: zwei Mantellinien des Lochs bei z = 15 ± 5, je Länge 40
    assert!(
        (len(&v, Visibility::Hidden) - 80.0).abs() < 1e-6,
        "{}",
        len(&v, Visibility::Hidden)
    );
}

#[test]
fn through_hole_along_y_is_all_visible_from_the_front() {
    let s = box_with_hole(40.0, 20.0, 30.0, 5.0, planora_math::DVec3::Y);
    let v = view(&s, ViewKind::Front);
    // Lochkreis vorn sichtbar, hinterer Kreis fällt darauf (Aufgabe 8 vereint); Mantellinien sind von vorn Punkte
    assert!(
        (len(&v, Visibility::Hidden)).abs() < 1e-6,
        "{}",
        len(&v, Visibility::Hidden)
    );
}

#[test]
fn mesh_reference_agrees_for_all_shapes_and_views() {
    let shapes: Vec<(&str, planora_kernel::Solid)> = vec![
        ("box", box_solid(10.0, 20.0, 30.0)),
        ("cylinder", cylinder_solid(5.0, 12.0)),
        ("sphere", sphere_solid(7.0)),
        ("cone", cone_solid(6.0, 2.0, 10.0)),
        (
            "hole_x",
            box_with_hole(40.0, 20.0, 30.0, 5.0, planora_math::DVec3::X),
        ),
        (
            "hole_y",
            box_with_hole(40.0, 20.0, 30.0, 5.0, planora_math::DVec3::Y),
        ),
        ("l_bracket", l_bracket(40.0, 30.0, 20.0, 10.0)),
        ("pocket", box_with_pocket(40.0, 30.0, 20.0, 10.0, 8.0, 5.0)),
        ("cross_hole", cylinder_cross_hole(8.0, 30.0, 3.0)),
        ("sphere_on_box", sphere_on_box(20.0, 7.0)),
        ("rounded_box", rounded_box(20.0, 20.0, 20.0, 3.0)),
    ];
    let cases: Vec<(&str, Vec<&planora_kernel::Solid>)> =
        shapes.iter().map(|(n, s)| (*n, vec![s])).collect();
    check_all_views(&cases);
}

/// Prüft alle Ansichten aller Fälle gegen die Netzreferenz: keine Fehler, je Ansicht höchstens
/// 30 % und insgesamt höchstens 5 % unentschiedene Referenzstellen.
fn check_all_views(cases: &[(&str, Vec<&planora_kernel::Solid>)]) {
    let (mut tolerated, mut sampled, mut warnings, mut undecided) = (0, 0, 0, 0);
    for (name, solids) in cases {
        for k in planora_drawing::model::ViewKind::ALL {
            let r = check_view(solids, k);
            assert!(r.errors.is_empty(), "{name} {k:?}: {:?}", r.errors);
            assert!(
                r.undecided * 10 <= r.sampled * 3,
                "{name} {k:?}: Referenz unentschieden an {} von {} Stellen",
                r.undecided,
                r.sampled
            );
            if r.undecided > 0 {
                eprintln!("{name} {k:?}: unentschieden {}/{}", r.undecided, r.sampled);
            }
            tolerated += r.tolerated;
            sampled += r.sampled;
            warnings += r.warnings.len();
            undecided += r.undecided;
        }
    }
    eprintln!(
        "Stellen: {sampled}, zulässig abweichend: {tolerated}, Referenz unentschieden: {undecided}, Warnungen (b): {warnings}"
    );
    assert!(
        undecided * 20 <= sampled,
        "Referenz unentschieden an {undecided} von {sampled} Stellen"
    );
}

#[test]
fn bodies_passing_into_each_other_agree_with_the_mesh_reference() {
    // Getrennte Körper, die ineinander stecken: Die Sichtbarkeit wechselt dort, wo eine Kante
    // die Fläche des anderen Körpers durchstößt – ohne Kreuzung im Bild.
    use planora_math::DVec3;
    let a = box_solid(10.0, 10.0, 10.0);
    let peg = box_at((3.0, 3.0, 9.5), (6.0, 6.0, 11.5));
    let column = box_at((2.0, 6.0, 2.0), (5.0, 9.0, 32.0));
    let rod = axis_cylinder(3.0, 10.0, DVec3::Z, DVec3::new(5.0, 5.0, 5.0));
    let bar = axis_cylinder(2.0, 10.0, DVec3::X, DVec3::new(5.0, 5.0, 4.0));
    let cases: Vec<(&str, Vec<&planora_kernel::Solid>)> = vec![
        ("box+peg", vec![&a, &peg]),
        ("box+column", vec![&a, &column]),
        ("box+rod", vec![&a, &rod]),
        ("box+bar", vec![&a, &bar]),
    ];
    check_all_views(&cases);
}

#[test]
fn edge_entering_another_body_changes_visibility_exactly_there() {
    // Isometrisch: Die senkrechte Kante (3, 3, z) des Zapfens [3,6]×[3,6]×[9,5; 11,5] steckt
    // bis z = 10 im Würfel [0,10]³ (verdeckt) und ragt darüber hinaus (sichtbar); Wechsel genau
    // bei z = 10, dem Durchstoßpunkt durch die Würfeloberseite (eigene Formel).
    let a = box_solid(10.0, 10.0, 10.0);
    let peg = box_at((3.0, 3.0, 9.5), (6.0, 6.0, 11.5));
    let v = compute_view(
        &[
            DrawBody {
                name: "A".into(),
                solid: &a,
            },
            DrawBody {
                name: "B".into(),
                solid: &peg,
            },
        ],
        ViewKind::Isometric,
        &Progress::new(),
    )
    .unwrap();
    let on_edge = |p: planora_math::DVec3| (p.x - 3.0).abs() < 1e-9 && (p.y - 3.0).abs() < 1e-9;
    let mut seen = 0;
    for p in &v.pieces {
        let s = &v.strokes[p.stroke];
        let (pa, pb) = (s.point(p.t0), s.point(p.t1));
        if !(on_edge(pa) && on_edge(pb)) {
            continue;
        }
        seen += 1;
        let (lo, hi) = (pa.z.min(pb.z), pa.z.max(pb.z));
        if hi <= 10.0 + 1e-9 {
            assert_eq!(p.vis, Visibility::Hidden, "{lo}..{hi}");
        } else {
            assert!(lo >= 10.0 - 1e-9, "Stück {lo}..{hi} überspannt z = 10");
        }
    }
    assert!(seen >= 2, "Kante (3, 3, z) nicht in zwei Stücken: {seen}");
}

#[test]
fn boxes_sharing_a_face_are_drawn_without_ambiguity() {
    // Zwei Würfel mit gemeinsamer Fläche x = 10 (deckungsgleiche Kanten): keine Meldung,
    // Netzreferenz stimmt in allen Ansichten.
    let a = box_solid(10.0, 10.0, 10.0);
    let b = box_at((10.0, 0.0, 0.0), (20.0, 10.0, 10.0));
    let cases: Vec<(&str, Vec<&planora_kernel::Solid>)> = vec![("shared_face", vec![&a, &b])];
    check_all_views(&cases);
}

#[test]
fn torus_body_is_reported_unsupported_for_now() {
    // VORLÄUFIG: Bis Aufgabe 7 den Torus-Umriss liefert, meldet compute_view einen Körper mit
    // Torusfläche als nicht unterstützt. Aufgabe 7 ersetzt diesen Test.
    let t = torus_solid(10.0, 3.0);
    let e = compute_view(
        &[DrawBody {
            name: "Ring".into(),
            solid: &t,
        }],
        ViewKind::Front,
        &Progress::new(),
    )
    .unwrap_err();
    assert!(
        matches!(e, planora_drawing::DrawError::Unsupported(_)),
        "{e:?}"
    );
}
#[test]
fn scaled_and_shifted_bodies_give_the_same_classification() {
    // Ausdehnung 1e5 mm, weit vom Ursprung (1e6 mm) und 0,1 mm: gleiche Anzahl sichtbarer/verdeckter Stücke je Ansicht
    let base = box_with_hole(40.0, 20.0, 30.0, 5.0, planora_math::DVec3::X);
    let want: Vec<(usize, usize)> = planora_drawing::model::ViewKind::ALL
        .iter()
        .map(|k| counts(&view(&base, *k)))
        .collect();
    for (f, shift) in [(2500.0, 0.0), (0.0025, 0.0), (1.0, 1.0e6)] {
        let s = transformed(&base, f, shift); // tests/common/model.rs: uniform skaliert und verschoben
        for (i, k) in planora_drawing::model::ViewKind::ALL.iter().enumerate() {
            assert_eq!(
                counts(&view(&s, *k)),
                want[i],
                "factor {f} shift {shift} {k:?}"
            );
        }
    }
}

#[test]
fn oblique_view_nearly_parallel_to_a_face_is_classified_or_named() {
    // Quader um 0,01° gedreht gegenüber der Vorderansicht (Fläche fast parallel zu d): entweder korrekt (Mesh-Referenz) oder AmbiguousVisibility, nie falsch.
    let s = rotated(
        &box_solid(10.0, 20.0, 30.0),
        planora_math::DVec3::Z,
        0.01_f64.to_radians(),
    );
    match compute_view(
        &[DrawBody {
            name: "K".into(),
            solid: &s,
        }],
        ViewKind::Front,
        &Progress::new(),
    ) {
        Ok(_) => {
            let r = check_view(&[&s], ViewKind::Front);
            assert!(r.errors.is_empty(), "{:?}", r.errors);
        }
        Err(planora_drawing::DrawError::AmbiguousVisibility(_)) => {}
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn missing_split_is_found_by_bisection() {
    // classify_range über einen Strich, der hinter der Kante einer Platte hervortritt; Übergang bei bekannter Stelle t*
    let (stroke, scene, t_star) = stroke_behind_plate_edge(); // tests/common/model.rs; t* aus der Geometrie berechnet
    let parts = planora_drawing::visibility::classify_range(
        &scene,
        &stroke.basis,
        &stroke.stroke,
        &[],
        0,
        stroke.stroke.t0,
        stroke.stroke.t1,
        100.0,
        "T",
        &std::cell::Cell::new(0),
    )
    .unwrap();
    assert_eq!(parts.len(), 2, "{parts:?}");
    assert!((parts[0].1 - t_star).abs() < 1e-7, "{parts:?}");
    assert_ne!(parts[0].2, parts[1].2);
}

#[test]
fn all_rays_unsafe_names_the_stroke() {
    struct AlwaysUnsafe;
    impl planora_drawing::raycast::Caster for AlwaysUnsafe {
        fn cast(
            &self,
            _: planora_math::DVec3,
            _: planora_math::DVec3,
            _: f64,
        ) -> planora_drawing::raycast::Ray {
            planora_drawing::raycast::Ray::Unsafe("test")
        }
    }
    let st = line_stroke((0.0, 0.0, 0.0), (1.0, 0.0, 0.0), 10.0);
    let e = planora_drawing::visibility::classify_range(
        &AlwaysUnsafe,
        &planora_drawing::basis::Basis::of(ViewKind::Front),
        &st,
        &[],
        0,
        0.0,
        10.0,
        10.0,
        "Kante 3",
        &std::cell::Cell::new(0),
    )
    .unwrap_err();
    assert!(matches!(e, planora_drawing::DrawError::AmbiguousVisibility(n) if n == "Kante 3"));
}

#[test]
fn empty_input_and_progress_cancel() {
    assert!(matches!(
        compute_view(&[], ViewKind::Front, &Progress::new()),
        Err(planora_drawing::DrawError::NoBodies)
    ));
    let p = Progress::new();
    p.cancel();
    let s = box_solid(1.0, 1.0, 1.0);
    assert!(matches!(
        compute_view(
            &[DrawBody {
                name: "K".into(),
                solid: &s
            }],
            ViewKind::Front,
            &p
        ),
        Err(planora_drawing::DrawError::Cancelled)
    ));
}

/// Prüfstrahl-Attrappe: jeder Strahl liefert dieselben Treffer.
struct FixedHits(Vec<planora_drawing::raycast::Hit>);
impl planora_drawing::raycast::Caster for FixedHits {
    fn cast(
        &self,
        _: planora_math::DVec3,
        _: planora_math::DVec3,
        _: f64,
    ) -> planora_drawing::raycast::Ray {
        planora_drawing::raycast::Ray::Hits(self.0.clone())
    }
}

fn classify_with(
    caster: &FixedHits,
    own: &[usize],
) -> Result<Vec<(f64, f64, Visibility)>, planora_drawing::DrawError> {
    let st = line_stroke((0.0, 0.0, 0.0), (1.0, 0.0, 0.0), 10.0);
    planora_drawing::visibility::classify_range(
        caster,
        &planora_drawing::basis::Basis::of(ViewKind::Front),
        &st,
        own,
        0,
        0.0,
        10.0,
        10.0,
        "Kante 1",
        &std::cell::Cell::new(0),
    )
}

#[test]
fn own_face_hits_close_to_the_start_do_not_hide_the_line() {
    // Ausdehnung 10: SELF_EPS = 1e-6, OWN_EPS = 1e-4. Ein Treffer bei s = 1e-5 auf der eigenen
    // Fläche 4 (etwa ungenauer Wiedertreffer der Startfläche) verdeckt nicht; auf einer fremden
    // Fläche verdeckt derselbe Treffer.
    let hit = |face| planora_drawing::raycast::Hit {
        s: 1e-5,
        body: 0,
        face,
        class: planora_drawing::raycast::HitClass::Inside,
    };
    let own = classify_with(&FixedHits(vec![hit(4)]), &[4]).unwrap();
    assert_eq!(own, vec![(0.0, 10.0, Visibility::Visible)]);
    let foreign = classify_with(&FixedHits(vec![hit(5)]), &[4]).unwrap();
    assert_eq!(foreign, vec![(0.0, 10.0, Visibility::Hidden)]);
}

#[test]
fn boundary_hits_never_count_as_hiding() {
    // Jeder Strahl streift nur einen Flächenrand: weder frei noch sicher verdeckt → benannte
    // Meldung statt Raten.
    let edge = planora_drawing::raycast::Hit {
        s: 5.0,
        body: 0,
        face: 2,
        class: planora_drawing::raycast::HitClass::Boundary,
    };
    let e = classify_with(&FixedHits(vec![edge]), &[]).unwrap_err();
    assert!(matches!(e, planora_drawing::DrawError::AmbiguousVisibility(n) if n == "Kante 1"));
}

#[test]
fn short_entry_into_another_body_is_split_at_the_piercing_point() {
    // Isometrisch: Der Zapfen [3,6]×[3,6]×[9,99; 20] steckt nur 0,01 mm im Würfel [0,10]³. Seine
    // Kante (3, 3, z) kreuzt im Bild die Würfelkante (0, 6, 10) bei z = 13 (eigene Rechnung:
    // (3, 3−y, z−10) ∥ (−1, 1, −1) → y = 6, z = 13); im Stück z ∈ [9,99; 13] liegt der Wechsel
    // bei 0,3 % der Breite – keine Stützstelle (auch nicht die Endstellen bei 1 %) sieht ihn,
    // nur der Durchstoßpunkt z = 10 teilt dort.
    let a = box_solid(10.0, 10.0, 10.0);
    let peg = box_at((3.0, 3.0, 9.99), (6.0, 6.0, 20.0));
    let v = compute_view(
        &[
            DrawBody {
                name: "A".into(),
                solid: &a,
            },
            DrawBody {
                name: "B".into(),
                solid: &peg,
            },
        ],
        ViewKind::Isometric,
        &Progress::new(),
    )
    .unwrap();
    let on_edge = |p: planora_math::DVec3| (p.x - 3.0).abs() < 1e-9 && (p.y - 3.0).abs() < 1e-9;
    let inside: Vec<_> = v
        .pieces
        .iter()
        .filter(|p| {
            let s = &v.strokes[p.stroke];
            let (pa, pb) = (s.point(p.t0), s.point(p.t1));
            on_edge(pa) && on_edge(pb) && pa.z.min(pb.z) < 9.995
        })
        .collect();
    assert_eq!(inside.len(), 1, "{inside:?}");
    let s = &v.strokes[inside[0].stroke];
    let top = s.point(inside[0].t0).z.max(s.point(inside[0].t1).z);
    assert!(
        (top - 10.0).abs() < 1e-9,
        "Stück im Würfel endet bei z = {top}"
    );
    assert_eq!(inside[0].vis, Visibility::Hidden);
}

#[test]
fn transition_in_the_outer_quarter_is_found_by_the_end_probes() {
    // Wie `missing_split…`, aber der Strich beginnt bei x = −0,5: Wechsel bei t* = 0,5, also bei
    // 5 % der Breite – alle drei Stützstellen (25/50/75 %) sind verdeckt, nur die Endstelle bei
    // 1 % ist frei.
    let (plate, scene, _) = stroke_behind_plate_edge();
    let y = plate.stroke.point(0.0).y;
    let st = line_stroke((-0.5, y, 5.0), (1.0, 0.0, 0.0), 10.0);
    let parts = planora_drawing::visibility::classify_range(
        &scene,
        &plate.basis,
        &st,
        &[],
        0,
        0.0,
        10.0,
        100.0,
        "T",
        &std::cell::Cell::new(0),
    )
    .unwrap();
    assert_eq!(parts.len(), 2, "{parts:?}");
    assert!((parts[0].1 - 0.5).abs() < 1e-7, "{parts:?}");
    assert_eq!(parts[0].2, Visibility::Visible);
    assert_eq!(parts[1].2, Visibility::Hidden);
}
