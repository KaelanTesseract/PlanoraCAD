//! Zeitmessung: Leistungspunkte aus Skizzen 11b im Dokument (Skizzen 12, Task 12).
//!
//! Nur von Hand im Release-Build:
//! `cargo test --release -p planora-feature --test app_perf_timing -- --ignored --nocapture`.
//!
//! Dokument: 50 Skizzen zu je 5 Texten `{Wert}` im Rahmen (jeder Text wächst über seinen Rahmen,
//! wenn `Wert` lang wird), 100 weitere Parameter. Gemessen wird je Fall in 5 Läufen der Mittelwert
//! je Schritt; berichtet wird der Median der 5 Läufe.
//! - (a) Parameteränderung: `Wert` kurz → lang, danach `reshape_texts` (wie die App), samt
//!   Wachsen aller 250 Rahmen (`grow_frame_to_fit`).
//! - (b) Rückgängig: Bild vor der Änderung zurückladen, danach `reshape_texts`.
//! - (c) Neusetzen ohne Änderung (`reshape_texts` auf einem gesetzten Dokument, wie nach einem
//!   Einheitenwechsel ohne neue Form): kostet vor allem den Klon je Skizze.
//! - (d) Neuberechnung von 200 Skizzen ohne Text (Zurückladen eines Bildes mit anderem
//!   Parameterwert, voller Weg): kostet je Skizze den `ParamScope`.
//! - (e) Rahmen wächst in einer Skizze mit 2000 Kurven (`Sketch::reshape_texts` mit einem Text,
//!   der über den Rahmen wächst): kostet den Klon der ganzen Skizze in `grow_frame_to_fit`.

use planora_feature::*;
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::model::Sketch;
use planora_sketch::text::{HAlign, TextId, TextParams, VAlign};
use planora_sketch::text_shape::{TextShaper, TextStyle};
use planora_text::FontLibrary;
use std::hint::black_box;
use std::sync::Arc;
use std::time::Instant;

const SHORT: &str = "1";
const LONG: &str = "1234567890123";

fn style() -> TextStyle {
    TextStyle {
        font: "Montserrat".into(),
        bold: false,
        italic: false,
        spacing: 0.0,
    }
}

/// Text `content` im Rahmen `a`–`b`, Höhe 5, gesetzt wie getippt.
fn add_text(sk: &mut Sketch, content: &str, a: DVec2, b: DVec2, lib: &dyn TextShaper) -> TextId {
    let shape = lib.shape(content, &style()).unwrap();
    sk.add_text_box(
        a,
        b,
        TextParams {
            content: content.into(),
            style: style(),
            height: 5.0,
            flip_h: false,
            flip_v: false,
        },
        HAlign::Left,
        VAlign::Bottom,
        Arc::new(shape),
    )
    .unwrap()
}

fn sketch_kind(sketch: Sketch) -> FeatureKind {
    FeatureKind::Sketch(SketchFeature {
        sketch,
        plane: SketchPlane::Standard(StandardPlane::XY),
        copies: vec![],
        formulas: Default::default(),
        text_formulas: Default::default(),
    })
}

fn length(d: &mut Document, name: &str, formula: &str) -> ParamId {
    d.add_parameter(name, formula, ParamUnit::Length, "")
        .unwrap()
}

fn set_wert(d: &mut Document, p: ParamId, formula: &str) {
    d.set_parameter(
        p,
        ParamEdit {
            name: "Wert".into(),
            formula: formula.into(),
            unit: ParamUnit::Length,
            comment: String::new(),
        },
    )
    .unwrap();
}

/// 100 Parameter `P0` … `P99`.
fn many_params(d: &mut Document) {
    for i in 0..100 {
        length(d, &format!("P{i}"), &format!("{}", i + 1));
    }
}

/// Das Dokument aus dem Kopf: `Wert` = [`SHORT`], gesetzt.
fn text_doc(lib: &FontLibrary) -> (Document, ParamId) {
    let mut d = Document::new();
    let p = length(&mut d, "Wert", SHORT);
    many_params(&mut d);
    for k in 0..50 {
        let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
        for i in 0..5 {
            let y = 15.0 * i as f64;
            add_text(
                &mut sk,
                "{Wert}",
                DVec2::new(0.0, y),
                DVec2::new(30.0, y + 10.0),
                lib,
            );
        }
        d.add_feature(format!("Skizze {k}"), sketch_kind(sk))
            .unwrap();
    }
    d.reshape_texts(lib);
    (d, p)
}

/// 200 Skizzen ohne Text (je ein Rechteck) und 100 Parameter.
fn plain_doc() -> (Document, ParamId) {
    let mut d = Document::new();
    let p = length(&mut d, "Wert", SHORT);
    many_params(&mut d);
    for k in 0..200 {
        let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
        let q: Vec<_> = [(0.0, 0.0), (20.0, 0.0), (20.0, 10.0), (0.0, 10.0)]
            .iter()
            .map(|&(x, y)| sk.add_point(x, y).unwrap())
            .collect();
        for i in 0..4 {
            sk.add_line(q[i], q[(i + 1) % 4]).unwrap();
        }
        d.add_feature(format!("Skizze {k}"), sketch_kind(sk))
            .unwrap();
    }
    (d, p)
}

/// Skizze mit 2000 Linien und einem Text im Rahmen (0, -20)–(30, -10).
fn big_sketch(lib: &FontLibrary) -> (Sketch, TextId) {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    for i in 0..2000 {
        let (x, y) = (6.0 * (i % 45) as f64, 6.0 * (i / 45) as f64);
        let a = sk.add_point(x, y).unwrap();
        let b = sk.add_point(x + 4.0, y + 3.0).unwrap();
        sk.add_line(a, b).unwrap();
    }
    let t = add_text(
        &mut sk,
        SHORT,
        DVec2::new(0.0, -20.0),
        DVec2::new(30.0, -10.0),
        lib,
    );
    (sk, t)
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// Druckt die Mittelwerte der Läufe in ms und deren Median.
fn report(what: &str, runs: Vec<f64>) {
    let ms: Vec<f64> = runs.iter().map(|s| s * 1e3).collect();
    println!(
        "{what}: Median {:.3} ms je Schritt (Läufe {:.3?})",
        median(ms.clone()),
        ms
    );
}

#[test]
#[ignore = "Zeitmessung, nur im Release-Build von Hand"]
fn app_perf_timing() {
    let lib = FontLibrary::bundled_only();

    // (a), (b): Parameteränderung und Rückgängig.
    let (mut d, p) = text_doc(&lib);
    let before = d.snapshot();
    {
        // Prüfung vorab: Mit dem langen Wert wachsen die Rahmen.
        let mut d2 = d.clone();
        set_wert(&mut d2, p, LONG);
        assert!(d2.reshape_texts(&lib));
        let f = d2.features().last().unwrap();
        let FeatureKind::Sketch(sf) = &f.kind else {
            panic!("keine Skizze");
        };
        let (_, td) = sf.sketch.texts().next().unwrap();
        let planora_sketch::text::TextPlacement::Box { corners, .. } = td.placement else {
            panic!("kein Rahmen");
        };
        assert!(
            sf.sketch.point_xy(corners[1]).unwrap().x > 30.5,
            "der Rahmen wächst"
        );
    }
    let (mut change, mut undo) = (Vec::new(), Vec::new());
    for _ in 0..5 {
        let (mut tc, mut tu) = (0.0, 0.0);
        const N: usize = 4;
        for _ in 0..N {
            let t = Instant::now();
            set_wert(&mut d, p, LONG);
            assert!(d.reshape_texts(&lib), "die Texte ändern sich");
            tc += t.elapsed().as_secs_f64();
            let snap = before.clone();
            let t = Instant::now();
            d.restore(snap).unwrap();
            black_box(d.reshape_texts(&lib));
            tu += t.elapsed().as_secs_f64();
        }
        change.push(tc / N as f64);
        undo.push(tu / N as f64);
    }
    report("(a) Parameteränderung, 50 Skizzen x 5 Texte", change);
    report("(b) Rückgängig, 50 Skizzen x 5 Texte", undo);

    // (c) Neusetzen ohne Änderung.
    let mut same = Vec::new();
    for _ in 0..5 {
        const N: usize = 20;
        let t = Instant::now();
        for _ in 0..N {
            assert!(!d.reshape_texts(&lib));
        }
        same.push(t.elapsed().as_secs_f64() / N as f64);
    }
    report("(c) Neusetzen ohne Änderung, 50 Skizzen x 5 Texte", same);

    // (d) Neuberechnung von 200 Skizzen ohne Text.
    let (mut d, p) = plain_doc();
    let a = d.snapshot();
    set_wert(&mut d, p, LONG);
    let b = d.snapshot();
    let mut rebuild = Vec::new();
    for _ in 0..5 {
        const N: usize = 10;
        let mut total = 0.0;
        for k in 0..N {
            let s = if k % 2 == 0 { a.clone() } else { b.clone() };
            let t = Instant::now();
            d.restore(s).unwrap();
            total += t.elapsed().as_secs_f64();
        }
        rebuild.push(total / N as f64);
    }
    report(
        "(d) Zurückladen, 200 Skizzen ohne Text, 100 Parameter",
        rebuild,
    );

    // (e) Rahmen wächst in einer Skizze mit 2000 Kurven.
    let (base, t) = big_sketch(&lib);
    let mut grow = Vec::new();
    for _ in 0..5 {
        const N: usize = 20;
        let mut total = 0.0;
        for _ in 0..N {
            let mut sk = base.clone();
            sk.set_text_content(t, LONG.into());
            let t0 = Instant::now();
            let r = sk.reshape_texts(&lib);
            total += t0.elapsed().as_secs_f64();
            assert!(r.changed);
            let Some(planora_sketch::text::TextPlacement::Box { corners, .. }) =
                sk.text(t).map(|d| d.placement)
            else {
                panic!("kein Rahmen");
            };
            assert!(
                sk.point_xy(corners[1]).unwrap().x > 30.5,
                "der Rahmen wächst"
            );
            black_box(sk);
        }
        grow.push(total / N as f64);
    }
    report("(e) Rahmen wächst, Skizze mit 2000 Kurven", grow);
    // Zum Vergleich: dasselbe Neusetzen mit einem Wert, der in den Rahmen passt.
    let mut fit = Vec::new();
    for _ in 0..5 {
        const N: usize = 20;
        let mut total = 0.0;
        for _ in 0..N {
            let mut sk = base.clone();
            sk.set_text_content(t, "2".into());
            let t0 = Instant::now();
            let r = sk.reshape_texts(&lib);
            total += t0.elapsed().as_secs_f64();
            assert!(r.changed);
            black_box(sk);
        }
        fit.push(total / N as f64);
    }
    report("(e0) Neusetzen ohne Wachsen, Skizze mit 2000 Kurven", fit);
}
