//! Gewählte Texte (Skizzen 8, Spec B9, C9): `selected_texts` läuft in jedem Bild (Skizzenfeld,
//! Ansicht, Menü). Ein ganz gewählter Text mit 200 Zeichen darf dabei nicht quadratisch kosten:
//! höchstens 0,5 ms (die frühere Suche je gewähltem Eintrag brauchte im Release 0,97 ms, die
//! jetzige etwa 0,1 ms).
//!
//! Der Messtest läuft nur im Release-Build:
//! `cargo test --release -p planora-app --test sketch_text_selection_perf -- --ignored`

use planora_app::sketch_state::{Item, SketchSession};
use planora_feature::FeatureId;
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::model::Sketch;
use planora_sketch::text::{HAlign, TextId, TextParams, VAlign};
use planora_sketch::text_shape::{TextShaper, TextStyle};
use planora_text::FontLibrary;
use std::sync::Arc;
use std::time::Instant;

/// Läufe je Messung; gemeldet und geprüft wird der Median.
const RUNS: usize = 11;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn params(content: &str) -> TextParams {
    TextParams {
        content: content.into(),
        style: TextStyle {
            font: "Montserrat".into(),
            bold: false,
            italic: false,
            spacing: 0.0,
        },
        height: 5.0,
        flip_h: false,
        flip_v: false,
    }
}

/// Text in Montserrat (mitgeliefert, nie Systemschriften) im Rahmen `a`–`b`.
fn text(sk: &mut Sketch, lib: &FontLibrary, content: &str, a: DVec2, b: DVec2) -> TextId {
    let p = params(content);
    let shape = Arc::new(lib.shape(&p.content, &p.style).unwrap());
    sk.add_text_box(a, b, p, HAlign::Left, VAlign::Bottom, shape)
        .unwrap()
}

/// 200 Zeichen in fünf Zeilen (Zeilenumbrüche zählen mit).
fn two_hundred() -> String {
    let lines = [
        "Profile aus Glyphen: Hallo Welt, wir bau",
        "en einen Text mit vielen Kurven fuer den",
        "Messtest. Jede Zeile hat vierzig Zeichen",
        "und alle zusammen ergeben zweihundert Ze",
        "ichen mit Umbruechen. Das ist genug.",
    ];
    let s = lines.join("\n");
    assert_eq!(s.chars().count(), 200);
    s
}

fn median(mut ms: Vec<f64>) -> f64 {
    ms.sort_by(f64::total_cmp);
    ms[ms.len() / 2]
}

#[test]
fn selected_texts_keep_selection_order() {
    let lib = FontLibrary::bundled_only();
    let mut sk = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let a = text(&mut sk, &lib, "Ab", v(0.0, 0.0), v(30.0, 10.0));
    let b = text(&mut sk, &lib, "Cd", v(0.0, 20.0), v(30.0, 30.0));
    let (da, db) = (sk.text(a).unwrap().clone(), sk.text(b).unwrap().clone());
    let mut se = SketchSession::new(FeatureId(0));
    // Ein Punkt von b, dann eine Kurve von a, dann noch eine Kurve von b: b zuerst, je einmal.
    se.selection = vec![
        Item::Point(db.points[3]),
        Item::Curve(da.curves[1]),
        Item::Curve(db.curves[0]),
    ];
    assert_eq!(se.selected_texts(&sk), vec![b, a]);
    // Rahmenlinien allein zählen nicht.
    let planora_sketch::text::TextPlacement::Box { lines, corners, .. } = da.placement else {
        panic!("kein Rahmen");
    };
    se.selection = vec![Item::Curve(lines[0]), Item::Point(corners[1])];
    assert!(se.selected_texts(&sk).is_empty());
    se.selection.clear();
    assert!(se.selected_texts(&sk).is_empty());
}

#[test]
#[ignore = "Messtest, nur im Release-Build sinnvoll"]
fn selected_texts_of_a_fully_selected_200_char_text_under_half_a_ms() {
    let lib = FontLibrary::bundled_only();
    let mut sk = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let t = text(&mut sk, &lib, &two_hundred(), v(0.0, 0.0), v(150.0, 40.0));
    let d = sk.text(t).unwrap();
    let mut se = SketchSession::new(FeatureId(0));
    se.select_text(&sk, t, false);
    assert_eq!(
        se.selection.len(),
        d.curves.len(),
        "der ganze Text ist gewählt"
    );
    let ms: Vec<f64> = (0..RUNS)
        .map(|_| {
            let t0 = Instant::now();
            let found = std::hint::black_box(se.selected_texts(&sk));
            let dt = t0.elapsed().as_secs_f64() * 1e3;
            assert_eq!(found, vec![t]);
            dt
        })
        .collect();
    let med = median(ms.clone());
    println!(
        "selected_texts, 200 Zeichen ganz gewählt ({} Kurven, {} Punkte): Median {med:.3} ms ({ms:.3?})",
        d.curves.len(),
        d.points.len()
    );
    assert!(med < 0.5, "Median {med:.3} ms");
}
