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
    let mut tolerated = 0;
    let mut sampled = 0;
    let mut warnings = 0;
    let mut undecided = 0;
    for (name, s) in &shapes {
        for k in planora_drawing::model::ViewKind::ALL {
            let r = check_view(&[s], k);
            assert!(r.errors.is_empty(), "{name} {k:?}: {:?}", r.errors);
            tolerated += r.tolerated;
            sampled += r.sampled;
            warnings += r.warnings.len();
            undecided += r.undecided;
        }
    }
    eprintln!(
        "Stellen: {sampled}, zulässig abweichend: {tolerated}, Referenz unentschieden: {undecided}, Warnungen (b): {warnings}"
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
