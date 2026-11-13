//! „Fest“ als Schalter (Skizzen 9, Task 7): Fixieren und Lösen wie in Fusion.

use planora_math::{Frame, Tolerance};
use planora_sketch::constraint::{ConstraintKind as K, FixToggle};
use planora_sketch::model::{PointId, Sketch};

fn fixed_count(s: &Sketch) -> usize {
    s.constraints()
        .filter(|(_, d)| matches!(d.kind, K::Fixed(..)))
        .count()
}

fn three_points() -> (Sketch, Vec<PointId>) {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let p: Vec<PointId> = (0..3)
        .map(|i| s.add_point(f64::from(i), 1.0).unwrap())
        .collect();
    (s, p)
}

#[test]
fn all_fixed_selection_is_released() {
    let (mut s, p) = three_points();
    assert_eq!(s.toggle_fixed(&p), FixToggle::Fixed(3));
    assert_eq!(fixed_count(&s), 3);
    assert_eq!(s.toggle_fixed(&p), FixToggle::Released(3));
    assert_eq!(fixed_count(&s), 0);
}

#[test]
fn mixed_selection_fixes_rest() {
    let (mut s, p) = three_points();
    assert_eq!(s.toggle_fixed(&p[..1]), FixToggle::Fixed(1));
    assert_eq!(s.toggle_fixed(&p), FixToggle::Fixed(2));
    assert_eq!(fixed_count(&s), 3);
}

#[test]
fn duplicates_count_once_and_empty_does_nothing() {
    let (mut s, p) = three_points();
    assert_eq!(s.toggle_fixed(&[p[0], p[0]]), FixToggle::Fixed(1));
    assert_eq!(s.toggle_fixed(&[]), FixToggle::Fixed(0));
    assert_eq!(fixed_count(&s), 1);
}
