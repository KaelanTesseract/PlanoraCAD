use planora_drawing::dimension::*;
use planora_drawing::limits::MAX_DIMENSIONS;
use planora_drawing::model::*;

fn drawing() -> Drawing {
    Drawing::new(
        DrawingId(1),
        "Test".into(),
        Sheet {
            paper: Paper::A3,
            landscape: true,
        },
        Projection::First,
    )
}

fn spec(view: ViewId) -> DimSpec {
    DimSpec {
        id: DimId(0),
        view,
        kind: DimKind::Horizontal,
        refs: vec![],
        offset: 10.0,
        angle: 45.0,
        text_shift: (0.0, 0.0),
        tolerance: Tolerance::None,
        decimals: 1,
        text_override: None,
        chain: None,
        suggested: false,
    }
}

fn add(d: &mut Drawing, kind: ViewKind, parent: Option<ViewId>) -> ViewId {
    let placement = match parent {
        Some(_) => Placement::Aligned { distance: 50.0 },
        None => Placement::Free { x: 100.0, y: 100.0 },
    };
    d.add_view(kind, Scale { num: 1, den: 1 }, placement, parent)
}

#[test]
fn add_view_sets_centerlines_by_kind() {
    for kind in ViewKind::ALL {
        let mut d = drawing();
        let id = add(&mut d, kind, None);
        let expected = kind != ViewKind::Isometric;
        assert_eq!(d.view(id).unwrap().centerlines, expected, "{kind:?}");
    }
}

#[test]
fn add_and_remove_dimension_assigns_increasing_ids() {
    let mut d = drawing();
    let v = add(&mut d, ViewKind::Front, None);
    assert_eq!(d.next_dim, 1);
    let a = d.add_dimension(spec(v));
    let b = d.add_dimension(spec(v));
    let c = d.add_dimension(spec(v));
    assert_eq!((a, b, c), (DimId(1), DimId(2), DimId(3)));
    assert_eq!(d.dimension(b).unwrap().id, b);
    assert!(d.remove_dimension(b));
    assert!(d.dimension(b).is_none());
    assert!(!d.remove_dimension(b));
    assert!(!d.remove_dimension(DimId(99)));
    let e = d.add_dimension(spec(v));
    assert_eq!(e, DimId(4));
    assert_eq!(d.dimensions.len(), 3);
    let limit = MAX_DIMENSIONS;
    assert_eq!(limit, 2000);
}

#[test]
fn removing_a_view_removes_its_dimensions() {
    let mut d = drawing();
    let front = add(&mut d, ViewKind::Front, None);
    let top = add(&mut d, ViewKind::Top, None);
    let derived = add(&mut d, ViewKind::Right, Some(front));
    d.add_dimension(spec(front));
    d.add_dimension(spec(front));
    let third = d.add_dimension(spec(top));
    d.add_dimension(spec(derived));
    assert_eq!(d.dimensions.len(), 4);
    let removed = d.remove_view(front);
    assert_eq!(removed, vec![front, derived]);
    assert_eq!(d.dimensions.len(), 1);
    assert_eq!(d.dimensions[0].id, third);
    assert!(d.dimensions.iter().all(|m| m.view == top));
}

#[test]
fn ref_counts_match_the_kinds() {
    assert_eq!(ref_count(DimKind::Horizontal), (2, 2));
    assert_eq!(ref_count(DimKind::Vertical), (2, 2));
    assert_eq!(ref_count(DimKind::Aligned), (2, 2));
    assert_eq!(ref_count(DimKind::Diameter), (1, 1));
    assert_eq!(ref_count(DimKind::Radius), (1, 1));
    assert_eq!(ref_count(DimKind::Angle), (2, 2));
}

#[test]
fn dim_error_texts_are_german_and_plain() {
    let all = [
        DimError::NotAcross,
        DimError::NotVisible,
        DimError::Ambiguous,
        DimError::Lost("Fläche 3".into()),
        DimError::NotAllowedInIsometric,
        DimError::TooMany,
        DimError::Invalid("Wert".into()),
    ];
    assert_eq!(all[0].to_string(), "Die Kante steht nicht quer zum Maß.");
    assert_eq!(
        all[3].to_string(),
        "Der Bezug wurde nicht gefunden: Fläche 3"
    );
    for e in all {
        let t = e.to_string();
        let lower = t.to_lowercase();
        for w in ["view", "sheet", "error", "dimension"] {
            assert!(!lower.contains(w), "{t}");
        }
        let body = match &e {
            DimError::Lost(_) | DimError::Invalid(_) => t.split(':').next().unwrap().to_string(),
            _ => t.clone(),
        };
        if !matches!(e, DimError::Lost(_) | DimError::Invalid(_)) {
            assert!(t.ends_with('.'), "{t}");
        }
        assert!(!body.is_empty());
    }
}

#[test]
fn drawing_equality_includes_dimensions() {
    let mut a = drawing();
    let v = add(&mut a, ViewKind::Front, None);
    let mut b = a.clone();
    assert_eq!(a, b);
    a.add_dimension(spec(v));
    assert_ne!(a, b);
    b.add_dimension(spec(v));
    assert_eq!(a, b);
    b.dimensions[0].offset = 17.0;
    assert_ne!(a, b);
    let mut c = a.clone();
    c.views[0].centerlines = !c.views[0].centerlines;
    assert_ne!(a, c);
}
