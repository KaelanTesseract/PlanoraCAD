use planora_drawing::{basis::Basis, layout::*, model::*};
use planora_math::DVec3;

#[test]
fn bases_are_orthonormal_right_handed() {
    for k in ViewKind::ALL {
        let b = Basis::of(k);
        for (a, c) in [(b.d, b.ex), (b.d, b.ey), (b.ex, b.ey)] {
            assert!(a.dot(c).abs() < 1e-14, "{k:?} nicht senkrecht");
        }
        for v in [b.d, b.ex, b.ey] {
            assert!((v.length() - 1.0).abs() < 1e-14);
        }
        assert!(
            (b.ex.cross(b.ey) + b.d).length() < 1e-14,
            "{k:?} kein Rechtssystem"
        );
    }
}

#[test]
fn front_view_projects_x_right_z_up() {
    let b = Basis::of(ViewKind::Front);
    let p = b.project(DVec3::new(3.0, 5.0, 7.0));
    assert_eq!((p.x, p.y), (3.0, 7.0));
    assert_eq!(b.depth(DVec3::new(3.0, 5.0, 7.0)), 5.0);
}

#[test]
fn first_angle_places_top_below_and_left_right_of_front() {
    let top = align_dir(Projection::First, ViewKind::Front, ViewKind::Top).unwrap();
    let left = align_dir(Projection::First, ViewKind::Front, ViewKind::Left).unwrap();
    assert_eq!(top, (0.0, -1.0));
    assert_eq!(left, (1.0, 0.0));
}

#[test]
fn third_angle_mirrors_first_angle() {
    let top = align_dir(Projection::Third, ViewKind::Front, ViewKind::Top).unwrap();
    let right = align_dir(Projection::Third, ViewKind::Front, ViewKind::Right).unwrap();
    assert_eq!(top, (0.0, 1.0));
    assert_eq!(right, (1.0, 0.0));
}

#[test]
fn back_view_sits_right_of_base_in_both_projections() {
    assert_eq!(
        align_dir(Projection::First, ViewKind::Front, ViewKind::Back),
        Some((1.0, 0.0))
    );
    assert_eq!(
        align_dir(Projection::Third, ViewKind::Front, ViewKind::Back),
        Some((1.0, 0.0))
    );
}

#[test]
fn isometric_cannot_be_derived() {
    assert_eq!(
        align_dir(Projection::First, ViewKind::Front, ViewKind::Isometric),
        None
    );
}

#[test]
fn paper_sizes_and_orientation() {
    assert_eq!(Paper::A4.size_mm(), (210.0, 297.0));
    let s = Sheet {
        paper: Paper::A3,
        landscape: true,
    };
    assert_eq!(s.size_mm(), (420.0, 297.0));
}

#[test]
fn scale_series_and_labels() {
    assert_eq!(Scale::SERIES.len(), 10);
    assert_eq!(Scale { num: 1, den: 2 }.label(), "1:2");
    assert_eq!(Scale { num: 5, den: 1 }.label(), "5:1");
    assert!((Scale { num: 1, den: 4 }.factor() - 0.25).abs() < 1e-15);
    assert!(!Scale { num: 0, den: 1 }.is_valid());
}

#[test]
fn removing_a_view_removes_its_children() {
    let mut d = Drawing::new(
        DrawingId(1),
        "Z".into(),
        Sheet {
            paper: Paper::A4,
            landscape: true,
        },
        Projection::First,
    );
    let f = d.add_view(
        ViewKind::Front,
        Scale { num: 1, den: 1 },
        Placement::Free { x: 100.0, y: 100.0 },
        None,
    );
    let t = d.add_view(
        ViewKind::Top,
        Scale { num: 1, den: 1 },
        Placement::Aligned { distance: 80.0 },
        Some(f),
    );
    let iso = d.add_view(
        ViewKind::Isometric,
        Scale { num: 1, den: 1 },
        Placement::Free { x: 200.0, y: 50.0 },
        None,
    );
    let gone = d.remove_view(f);
    assert_eq!(gone, vec![f, t]);
    assert!(d.view(iso).is_some() && d.view(f).is_none() && d.view(t).is_none());
}

// ---------------------------------------------------------------------------------------------
// Nachzüge (Aufgabe 18): Sollwerte von Hand aus Spec und Plan gerechnet, nicht aus dem Code.
// ---------------------------------------------------------------------------------------------

fn sc(num: u32, den: u32) -> Scale {
    Scale { num, den }
}

fn drawing() -> Drawing {
    Drawing::new(
        DrawingId(1),
        "Z".into(),
        Sheet {
            paper: Paper::A4,
            landscape: true,
        },
        Projection::First,
    )
}

#[test]
fn scale_series_has_exactly_the_ten_steps_in_order() {
    // Spec §3: 1:1, 1:2, 1:5, 1:10, 1:20, 1:50, 1:100, 2:1, 5:1, 10:1.
    let want = [
        (1, 1),
        (1, 2),
        (1, 5),
        (1, 10),
        (1, 20),
        (1, 50),
        (1, 100),
        (2, 1),
        (5, 1),
        (10, 1),
    ];
    let got: Vec<(u32, u32)> = Scale::SERIES.iter().map(|s| (s.num, s.den)).collect();
    assert_eq!(got, want);
    assert!(Scale::SERIES.iter().all(|s| s.is_valid()));
    assert_eq!(Scale::SERIES[6].label(), "1:100");
    assert_eq!(Scale::SERIES[9].label(), "10:1");
    let factors = [1.0, 0.5, 0.2, 0.1, 0.05, 0.02, 0.01, 2.0, 5.0, 10.0];
    for (s, f) in Scale::SERIES.iter().zip(factors) {
        assert!((s.factor() - f).abs() < 1e-15, "{}", s.label());
    }
}

#[test]
fn scale_needs_both_parts_positive() {
    assert!(sc(1, 1).is_valid());
    assert!(!sc(1, 0).is_valid());
    assert!(!sc(0, 5).is_valid());
}

#[test]
fn paper_sizes_follow_iso_216() {
    // Hochformat: A4 210×297, A3 297×420, A2 420×594, A1 594×841, A0 841×1189.
    let table = [
        (Paper::A4, (210.0, 297.0)),
        (Paper::A3, (297.0, 420.0)),
        (Paper::A2, (420.0, 594.0)),
        (Paper::A1, (594.0, 841.0)),
        (Paper::A0, (841.0, 1189.0)),
    ];
    for (paper, (w, h)) in table {
        assert_eq!(paper.size_mm(), (w, h), "{paper:?}");
        let portrait = Sheet {
            paper,
            landscape: false,
        };
        let landscape = Sheet {
            paper,
            landscape: true,
        };
        assert_eq!(portrait.size_mm(), (w, h), "{paper:?} hoch");
        assert_eq!(landscape.size_mm(), (h, w), "{paper:?} quer");
    }
}

#[test]
fn view_kind_labels_order_and_normal_views() {
    let labels: Vec<&str> = ViewKind::ALL.iter().map(|k| k.label()).collect();
    assert_eq!(
        labels,
        [
            "Vorne",
            "Hinten",
            "Links",
            "Rechts",
            "Oben",
            "Unten",
            "Isometrisch"
        ]
    );
    for k in ViewKind::ALL {
        assert_eq!(k.is_normal(), k != ViewKind::Isometric, "{k:?}");
    }
}

#[test]
fn geom_tol_has_a_floor_and_grows_linearly() {
    use planora_drawing::limits::{geom_tol, own_eps, self_eps};
    // 64·ε = 64 · 2⁻⁵² = 1,4210854715202004e-14.
    let unit = 64.0 * 2f64.powi(-52);
    // Kleine Ausdehnung: die Untergrenze 1e-9 gilt (auch für 0 und 1e-9).
    for e in [0.0, 1e-9, 1.0, 1e3, 1e4] {
        assert_eq!(geom_tol(e), 1e-9, "E = {e}");
    }
    // Ab E ≈ 7,04e4 (= 1e-9 / unit) gilt 64·ε·E; Prüfung bei 1e6 und 1e8.
    for e in [1e6, 1e8] {
        let want = unit * e;
        assert!((geom_tol(e) - want).abs() <= 1e-12 * want, "E = {e}");
    }
    // Verdopplung der Ausdehnung verdoppelt die Toleranz (oberhalb der Untergrenze).
    assert!((geom_tol(2e6) - 2.0 * geom_tol(1e6)).abs() <= 1e-12 * geom_tol(2e6));
    // SELF_EPS = 1e-7·E, OWN_EPS = 1e-5·E (Global Constraints), mit E ≥ 1e-9.
    assert!((self_eps(100.0) - 1e-5).abs() < 1e-17);
    assert!((own_eps(100.0) - 1e-3).abs() < 1e-15);
    assert!((self_eps(0.0) - 1e-16).abs() < 1e-28);
    assert!((own_eps(0.0) - 1e-14).abs() < 1e-26);
}

#[test]
fn view_extent_scales_width_and_height() {
    // Hüllbox (−5, 2) … (15, 12): 20 × 10.
    let bbox = ((-5.0, 2.0), (15.0, 12.0));
    assert_eq!(view_extent(bbox, sc(1, 1)), (20.0, 10.0));
    assert_eq!(view_extent(bbox, sc(1, 2)), (10.0, 5.0));
    assert_eq!(view_extent(bbox, sc(5, 1)), (100.0, 50.0));
    let (w, h) = view_extent(bbox, sc(1, 100));
    assert!((w - 0.2).abs() < 1e-15 && (h - 0.1).abs() < 1e-15);
    // Zu einem Punkt entartete Hüllbox.
    assert_eq!(view_extent(((3.0, 3.0), (3.0, 3.0)), sc(10, 1)), (0.0, 0.0));
}

#[test]
fn align_dir_for_every_parent_and_child_by_hand() {
    // Von Hand: ex = d × ey, Auge e = −d des Kindes, (a, b) = (e·ex, e·ey) des Elternbildes;
    // Erstwinkel (−a, −b), Dritter Winkel (a, b), dominante Achse; Gegenansicht → (1, 0).
    //   Vorne:  d=(0,1,0)  ey=Z  ex=(1,0,0)    Hinten: d=(0,−1,0) ex=(−1,0,0)
    //   Links:  d=(1,0,0)  ey=Z  ex=(0,−1,0)   Rechts: d=(−1,0,0) ex=(0,1,0)
    //   Oben:   d=(0,0,−1) ey=Y  ex=(1,0,0)    Unten:  d=(0,0,1)  ey=−Y ex=(1,0,0)
    use ViewKind::*;
    type Dir = (f64, f64);
    // (Eltern, Kind, Erstwinkel, Dritter Winkel)
    let table: [(ViewKind, ViewKind, Dir, Dir); 17] = [
        (Front, Top, (0.0, -1.0), (0.0, 1.0)),
        (Front, Bottom, (0.0, 1.0), (0.0, -1.0)),
        (Front, Left, (1.0, 0.0), (-1.0, 0.0)),
        (Front, Right, (-1.0, 0.0), (1.0, 0.0)),
        (Front, Back, (1.0, 0.0), (1.0, 0.0)),
        (Front, Front, (1.0, 0.0), (1.0, 0.0)),
        (Left, Top, (0.0, -1.0), (0.0, 1.0)),
        (Left, Bottom, (0.0, 1.0), (0.0, -1.0)),
        (Left, Front, (-1.0, 0.0), (1.0, 0.0)),
        (Left, Back, (1.0, 0.0), (-1.0, 0.0)),
        (Left, Right, (1.0, 0.0), (1.0, 0.0)),
        (Top, Front, (0.0, 1.0), (0.0, -1.0)),
        (Top, Back, (0.0, -1.0), (0.0, 1.0)),
        (Top, Left, (1.0, 0.0), (-1.0, 0.0)),
        (Top, Right, (-1.0, 0.0), (1.0, 0.0)),
        (Top, Bottom, (1.0, 0.0), (1.0, 0.0)),
        (Right, Left, (1.0, 0.0), (1.0, 0.0)),
    ];
    for (parent, child, first, third) in table {
        assert_eq!(
            align_dir(Projection::First, parent, child),
            Some(first),
            "Erstwinkel {parent:?} -> {child:?}"
        );
        assert_eq!(
            align_dir(Projection::Third, parent, child),
            Some(third),
            "Dritter Winkel {parent:?} -> {child:?}"
        );
    }
    // Isometrisch ist nie abgeleitet, auch nicht von anderen Elternbildern.
    for parent in ViewKind::ALL {
        assert_eq!(align_dir(Projection::First, parent, Isometric), None);
        assert_eq!(align_dir(Projection::Third, parent, Isometric), None);
    }
}

#[test]
fn removing_a_view_removes_grandchildren_depth_first() {
    let mut d = drawing();
    let one = sc(1, 1);
    let f = d.add_view(
        ViewKind::Front,
        one,
        Placement::Free { x: 100.0, y: 100.0 },
        None,
    );
    let t = d.add_view(
        ViewKind::Top,
        one,
        Placement::Aligned { distance: 80.0 },
        Some(f),
    );
    // Enkel: Kind der Draufsicht.
    let g = d.add_view(
        ViewKind::Left,
        one,
        Placement::Aligned { distance: 60.0 },
        Some(t),
    );
    // Zweites Kind der Vorderansicht, in der Liste nach dem Enkel.
    let s = d.add_view(
        ViewKind::Right,
        one,
        Placement::Aligned { distance: 70.0 },
        Some(f),
    );
    assert_eq!([f.0, t.0, g.0, s.0], [1, 2, 3, 4]);

    // Mittleres Glied: die Draufsicht samt Enkel, Vorderansicht und Schwester bleiben.
    let mut mid = d.clone();
    assert_eq!(mid.remove_view(t), vec![t, g]);
    let left: Vec<ViewId> = mid.views.iter().map(|v| v.id).collect();
    assert_eq!(left, vec![f, s]);

    // Wurzel: erst die Ansicht, dann Kinder in Listenreihenfolge, je Kind erst dessen Enkel.
    assert_eq!(d.remove_view(f), vec![f, t, g, s]);
    assert!(d.views.is_empty());
}

#[test]
fn removing_an_unknown_or_a_removed_view_changes_nothing() {
    let mut d = drawing();
    let f = d.add_view(
        ViewKind::Front,
        sc(1, 1),
        Placement::Free { x: 1.0, y: 2.0 },
        None,
    );
    let before = d.clone();
    assert!(d.remove_view(ViewId(99)).is_empty());
    assert_eq!(d, before);
    assert_eq!(d.remove_view(f), vec![f]);
    assert!(d.remove_view(f).is_empty());
    // Kennungen werden nicht wiederverwendet: die nächste ist 2.
    let g = d.add_view(
        ViewKind::Back,
        sc(1, 1),
        Placement::Free { x: 0.0, y: 0.0 },
        None,
    );
    assert_eq!(g, ViewId(2));
}
