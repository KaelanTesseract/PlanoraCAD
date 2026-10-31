//! Anzeigen und Meldungen in der Anzeigeeinheit (Teilprojekt Zoll, Aufgabe 5): Eigenschaften,
//! Skizzenmaße, Meldungen mit Grenzen, STEP-Import und englische Zahlen. Die Einheit gilt je
//! Test über `with_unit` (nur dieser Thread). In Millimeter bleibt jeder Text wie bisher.

mod common;

use planora_app::blend_dialog::{check_min_sizes, min_text};
use planora_app::sketch_view::{format_length, overlay_text};
use planora_app::state::*;
use planora_app::step_import_dialog::{size_hint, size_text};
use planora_app::{fonts, properties};
use planora_feature::BlendArt;
use planora_i18n::{DisplayUnit, english, tr, with_lang, with_unit};
use planora_math::{DVec3, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind;
use planora_sketch::model::Sketch;

const ZOLL: DisplayUnit = DisplayUnit::Zoll;
const MM: DisplayUnit = DisplayUnit::Millimeter;

/// Ein Quader von 1 in Kantenlänge (25,4 mm), ausgewählt.
fn inch_box() -> AppState {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(25.4)).unwrap();
    s.selected_body = Some(0);
    s
}

/// Alle Texte der Eigenschaften des gewählten Körpers.
fn property_texts(s: &mut AppState) -> Vec<String> {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    let mut all = Vec::new();
    for _ in 0..2 {
        let out = ctx.run_ui(egui::RawInput::default(), |ui| properties::show(ui, s));
        all = common::collect_seen(&out.shapes)
            .into_iter()
            .map(|x| x.text)
            .collect();
        out.drop_without_applying_deltas();
    }
    all
}

fn has(texts: &[String], wanted: &str) -> bool {
    texts.iter().any(|t| t == wanted)
}

#[test]
fn the_properties_of_a_box_show_size_volume_and_area_in_inches() {
    with_unit(ZOLL, || {
        let mut s = inch_box();
        let t = property_texts(&mut s);
        assert!(has(&t, "1,000 × 1,000 × 1,000 in"), "{t:?}");
        assert!(has(&t, "≈ 1,000 in³"), "{t:?}");
        assert!(has(&t, "≈ 6,000 in²"), "{t:?}");
        assert!(!t.iter().any(|x| x.contains("mm")), "{t:?}");
        assert!(has(&t, "Position x / y / z (in)"), "{t:?}");
    });
}

#[test]
fn the_properties_of_a_box_stay_in_millimetres_by_default() {
    with_unit(MM, || {
        let mut s = inch_box();
        let t = property_texts(&mut s);
        assert!(has(&t, "25,40 × 25,40 × 25,40 mm"), "{t:?}");
        assert!(has(&t, "≈ 16387,06 mm³"), "{t:?}");
        assert!(has(&t, "≈ 3870,96 mm²"), "{t:?}");
        assert!(has(&t, "Position x / y / z in mm"), "{t:?}");
    });
}

#[test]
fn the_feature_rows_of_a_box_use_the_unit() {
    let mut s = AppState::new();
    let id = s.create_box(DVec3::new(25.4, 50.8, 76.2)).unwrap();
    let kind = s.document.feature(id).unwrap().kind.clone();
    let size = |u| {
        with_unit(u, || properties::parameter_rows(&s, &kind))
            .into_iter()
            .find(|(l, _)| l == "Größe")
            .unwrap()
            .1
    };
    assert_eq!(size(ZOLL), "1,000 × 2,000 × 3,000 in");
    assert_eq!(size(MM), "25,40 × 50,80 × 76,20 mm");
}

#[test]
fn a_sketch_dimension_label_uses_the_unit() {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let center = sk.add_point(0.0, 0.0).unwrap();
    let circle = sk.add_circle(center, 12.7).unwrap();
    let kind = ConstraintKind::Radius(circle, 12.7);
    with_unit(ZOLL, || {
        assert_eq!(overlay_text(&ctx, &sk, &kind, false), "R 0,500 in");
        assert_eq!(overlay_text(&ctx, &sk, &kind, true), "(R 0,500 in)");
        assert_eq!(format_length(25.4), "1,000 in");
    });
    with_unit(MM, || {
        assert_eq!(overlay_text(&ctx, &sk, &kind, false), "R 12,70 mm");
        assert_eq!(format_length(25.4), "25,40 mm");
    });
}

#[test]
fn a_limit_message_names_the_limit_in_inches() {
    let fail = |u| {
        with_unit(u, || {
            let mut s = AppState::new();
            s.create_box(DVec3::splat(2.0e9)).unwrap_err()
        })
    };
    assert_eq!(
        fail(ZOLL),
        "Erzeugen fehlgeschlagen: Die Maße dürfen höchstens 39370078,740 in betragen"
    );
    assert_eq!(
        fail(MM),
        "Erzeugen fehlgeschlagen: Die Maße dürfen höchstens 1e9 mm betragen"
    );
}

#[test]
fn a_move_limit_message_in_english_uses_the_dot() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    let text = with_lang(&english(), || {
        with_unit(ZOLL, || {
            s.move_body(0, DVec3::new(2.0e9, 0.0, 0.0)).unwrap_err()
        })
    });
    assert_eq!(
        text,
        "Move rejected: The body would lie outside ±39370078.740 in"
    );
}

#[test]
fn the_smallest_blend_size_is_shown_in_the_unit() {
    let art = BlendArt::Rundung { radius: 0.001 };
    let msg = |u| with_unit(u, || check_min_sizes(&art, 0.5).unwrap_err().text());
    let zoll = msg(ZOLL);
    assert!(zoll.contains("0,01969 in"), "{zoll}");
    assert!(!zoll.contains("mm"), "{zoll}");
    let mm = msg(MM);
    assert!(mm.contains("0,5 mm"), "{mm}");
    assert_eq!(with_unit(ZOLL, || min_text(0.5)), "0,01969");
    assert_eq!(with_unit(MM, || min_text(0.5)), "0,5");
}

#[test]
fn the_step_import_sizes_and_hint_are_in_the_unit() {
    let bounds = Some((DVec3::ZERO, DVec3::new(25.4, 50.8, 76.2)));
    with_unit(ZOLL, || {
        assert_eq!(size_text(bounds), "1 × 2 × 3 in");
        assert_eq!(
            size_text(Some((DVec3::ZERO, DVec3::new(12.7, 6.35, 25.4)))),
            "0,50 × 0,25 × 1 in"
        );
        assert_eq!(
            size_hint(),
            "Maße der Hülle, auf 0,01 in gerundet; bei gekrümmten Flächen ungefähr."
        );
    });
    with_unit(MM, || {
        assert_eq!(size_text(bounds), "25,4 × 50,8 × 76,2 mm");
        assert_eq!(
            size_hint(),
            "Maße der Hülle, auf 0,1 mm gerundet; bei gekrümmten Flächen ungefähr."
        );
    });
    with_lang(&english(), || {
        with_unit(ZOLL, || {
            assert_eq!(size_text(bounds), "1 × 2 × 3 in");
            assert!(
                size_hint().contains("rounded to 0.01 in"),
                "{}",
                size_hint()
            );
        });
    });
}

#[test]
fn the_profile_label_shows_the_area_in_the_unit() {
    let label = |u: DisplayUnit| {
        with_unit(u, || {
            tr!(
                "create-profile-label",
                n = 1,
                area = planora_i18n::fmt_area_num(645.16),
                unit = u.area_symbol()
            )
        })
    };
    assert_eq!(label(ZOLL), "Profil 1: 1,000 in²");
    assert_eq!(label(MM), "Profil 1: 645,16 mm²");
}

#[test]
fn english_properties_in_inches_use_the_dot() {
    with_lang(&english(), || {
        with_unit(ZOLL, || {
            let mut s = inch_box();
            let t = property_texts(&mut s);
            assert!(has(&t, "1.000 × 1.000 × 1.000 in"), "{t:?}");
            assert!(has(&t, "≈ 1.000 in³"), "{t:?}");
        });
    });
}
