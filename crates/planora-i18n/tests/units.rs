//! Tests für die Anzeige-Maßeinheit. Diese Datei ruft nie `set_global_unit` (Millimeter bleibt Grundwert).

use planora_i18n::{
    DisplayUnit, current_unit, english, fmt_area, fmt_len, fmt_len_num, fmt_volume, with_lang,
    with_unit,
};

use DisplayUnit::{Millimeter, Zoll};

#[test]
fn ids_and_factors() {
    assert_eq!(DisplayUnit::default(), Millimeter);
    assert_eq!(Millimeter.id(), "mm");
    assert_eq!(Zoll.id(), "in");
    assert_eq!(DisplayUnit::from_id("in"), Some(Zoll));
    assert_eq!(DisplayUnit::from_id("mm"), Some(Millimeter));
    assert_eq!(DisplayUnit::from_id("x"), None);
    assert_eq!(Millimeter.mm_per_unit(), 1.0);
    assert_eq!(Zoll.mm_per_unit(), 25.4);
    assert_eq!(Zoll.symbol(), "in");
    assert_eq!(Zoll.area_symbol(), "in²");
    assert_eq!(Millimeter.volume_symbol(), "mm³");
    assert_eq!(Millimeter.decimals(), 2);
    assert_eq!(Zoll.decimals(), 3);
    assert!((Zoll.to_display(25.4) - 1.0).abs() < 1e-12);
    assert!((Zoll.from_display(1.0) - 25.4).abs() < 1e-12);
}

#[test]
fn millimeter_is_default_and_formats_by_language() {
    assert_eq!(current_unit(), Millimeter);
    assert_eq!(fmt_len(12.5), "12,50 mm");
    with_lang(&english(), || assert_eq!(fmt_len(12.5), "12.50 mm"));
}

#[test]
fn inch_formatting() {
    with_unit(Zoll, || {
        assert_eq!(fmt_len(25.4), "1,000 in");
        assert_eq!(fmt_len(12.5), "0,492 in");
        assert_eq!(fmt_len_num(25.4), "1,000");
        assert_eq!(fmt_area(645.16), "1,000 in²");
        assert_eq!(fmt_volume(16387.064), "1,000 in³");
        assert_eq!(fmt_len(-0.0001), "0,000 in");
        with_lang(&english(), || assert_eq!(fmt_len(25.4), "1.000 in"));
    });
}

#[test]
fn with_unit_nests_and_restores_after_panic() {
    with_unit(Zoll, || {
        with_unit(Millimeter, || assert_eq!(current_unit(), Millimeter));
        assert_eq!(current_unit(), Zoll);
    });
    let r = std::panic::catch_unwind(|| with_unit(Zoll, || panic!("absichtlich")));
    assert!(r.is_err());
    assert_eq!(current_unit(), Millimeter);
}

#[test]
fn from_id_ignores_case_and_surrounding_space() {
    assert_eq!(DisplayUnit::from_id("IN"), Some(Zoll));
    assert_eq!(DisplayUnit::from_id(" mm "), Some(Millimeter));
    assert_eq!(DisplayUnit::from_id("Mm"), Some(Millimeter));
    assert_eq!(DisplayUnit::from_id("cm"), None);
    assert_eq!(DisplayUnit::from_id(""), None);
}
