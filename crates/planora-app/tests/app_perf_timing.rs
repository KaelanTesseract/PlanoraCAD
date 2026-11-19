//! Zeitmessung: Leistungspunkte aus Skizzen 11b in der App (Skizzen 12, Task 12). Der Teil im
//! Dokument liegt in `crates/planora-feature/tests/app_perf_timing.rs`.
//!
//! Nur von Hand im Release-Build:
//! `cargo test --release -p planora-app --test app_perf_timing -- --ignored --nocapture`.
//!
//! Je Fall 5 Läufe zu je 100 Bildern; berichtet wird der Median der 5 Läufe (je Bild).
//! - (f) `is_feature_hidden` für alle Features, 100 Bilder: 50 Skizzen zu je 5 Texten in der
//!   innersten von 20 verschachtelten Gruppen (keine ausgeblendet, also der ganze Weg nach oben).
//! - (g) Zeichnen von 100 Bildern desselben Dokuments (fertige Skizzen: GPU-Linien und Formen).
//! - (h) Geöffnete Skizze mit einem ganz gewählten Text von 200 Zeichen und vier weiteren Texten,
//!   Werkzeug Auswahl: 100 Durchläufe von `show_overlay` (mit Drehgriff am Text).
//! - (i) Nur Messung: Hover-Wahl `pick_with` über einer fertigen Skizze mit 2000 Kurven, 100
//!   Zeigerstellungen.

use planora_app::sketch_view;
use planora_app::state::{AppState, PickFilter};
use planora_app::viewport::stroke_view;
use planora_feature::{FeatureKind, GroupItem, SketchFeature, SketchPlane, StandardPlane};
use planora_math::{DVec2, Frame, Tolerance};
use planora_render::StrokeVertex;
use planora_sketch::model::Sketch;
use planora_sketch::text::{HAlign, TextId, TextParams, VAlign};
use planora_sketch::text_shape::{TextShaper, TextStyle};
use planora_text::FontLibrary;
use std::hint::black_box;
use std::sync::Arc;
use std::time::{Duration, Instant};

const SCREEN: egui::Rect =
    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1920.0, 1080.0));
/// Pixelfaktor eines üblichen Bildschirms mit 150 % Skalierung.
const PPP: f32 = 1.5;
const FRAMES: usize = 100;

fn style() -> TextStyle {
    TextStyle {
        font: "Montserrat".into(),
        bold: false,
        italic: false,
        spacing: 0.0,
    }
}

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

fn add_sketch(s: &mut AppState, sk: Sketch) -> planora_feature::FeatureId {
    s.add_feature(
        Some("Skizze"),
        FeatureKind::Sketch(SketchFeature {
            sketch: sk,
            plane: SketchPlane::Standard(StandardPlane::XY),
            copies: vec![],
            formulas: Default::default(),
            text_formulas: Default::default(),
        }),
    )
    .unwrap()
}

/// 50 Skizzen zu je 5 Texten in der innersten von 20 verschachtelten Gruppen.
fn grouped_state(lib: &FontLibrary) -> AppState {
    let mut s = AppState::new();
    let mut ids = Vec::new();
    for k in 0..50 {
        let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
        for i in 0..5 {
            let (x, y) = (40.0 * i as f64, 20.0 * k as f64);
            add_text(
                &mut sk,
                "Text 12",
                DVec2::new(x, y),
                DVec2::new(x + 30.0, y + 10.0),
                lib,
            );
        }
        ids.push(add_sketch(&mut s, sk));
    }
    let items: Vec<GroupItem> = ids.iter().map(|&f| GroupItem::Sketch(f)).collect();
    for _ in 0..20 {
        s.document.create_sketch_group(&items).unwrap();
    }
    assert_eq!(s.document.sketch_groups().groups.len(), 20);
    assert!(ids.iter().all(|&f| !s.is_feature_hidden(f)));
    s.camera.set_viewport(SCREEN.width(), SCREEN.height());
    s.camera.set_iso_view();
    s
}

/// 200 Zeichen in fünf Zeilen.
fn two_hundred() -> String {
    let lines = [
        "Profile aus Glyphen: Hallo Welt, wir bau",
        "en einen Text mit vielen Kurven fuer den",
        "Messtest. Jede Zeile hat vierzig Zeichen",
        "und alle zusammen ergeben zweihundert Ze",
        "ichen mit Umbruechen. Das ist genug.",
    ];
    lines.join("\n")
}

/// Fertige Skizze mit `n` Kurven im Raster (Linien, Kreise, Bögen).
fn grid_sketch(n: usize) -> Sketch {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let cols = (n as f64).sqrt().ceil() as usize;
    for i in 0..n {
        let (x, y) = (6.0 * (i % cols) as f64, 6.0 * (i / cols) as f64);
        match i % 3 {
            0 => {
                let a = sk.add_point(x, y).unwrap();
                let b = sk.add_point(x + 4.0, y + 3.0).unwrap();
                sk.add_line(a, b).unwrap();
            }
            1 => {
                let c = sk.add_point(x + 2.0, y + 2.0).unwrap();
                sk.add_circle(c, 2.0).unwrap();
            }
            _ => {
                let c = sk.add_point(x + 2.0, y).unwrap();
                let a = sk.add_point(x + 4.0, y).unwrap();
                let b = sk.add_point(x, y).unwrap();
                sk.add_arc(c, a, b).unwrap();
            }
        }
    }
    sk
}

fn frame(s: &AppState, strokes: &mut Vec<StrokeVertex>) -> usize {
    strokes.clear();
    let view = stroke_view(s, SCREEN, PPP);
    sketch_view::finished_sketch_strokes(s, &view, strokes);
    let shapes = sketch_view::finished_sketch_shapes(s, SCREEN);
    shapes.len() + strokes.len()
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(planora_app::fonts::definitions());
    ctx
}

fn overlay(ctx: &egui::Context, s: &mut AppState) -> usize {
    let raw = egui::RawInput {
        screen_rect: Some(SCREEN),
        ..Default::default()
    };
    let out = ctx.run_ui(raw, |ui| {
        sketch_view::show_overlay(ui, SCREEN, s);
    });
    let n = out.shapes.len();
    out.drop_without_applying_deltas();
    n
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

/// 5 Läufe zu je [`FRAMES`] Aufrufen von `step`; druckt den Median je Bild.
fn measure(what: &str, mut step: impl FnMut(usize)) {
    for k in 0..3 {
        step(k);
    }
    let runs: Vec<Duration> = (0..5)
        .map(|_| {
            let t = Instant::now();
            for k in 0..FRAMES {
                step(k);
            }
            t.elapsed() / FRAMES as u32
        })
        .collect();
    let ms: Vec<f64> = runs.iter().map(|d| d.as_secs_f64() * 1e3).collect();
    println!(
        "{what}: Median {:.4} ms je Bild (Läufe {ms:.4?})",
        median(runs).as_secs_f64() * 1e3
    );
}

#[test]
#[ignore = "Zeitmessung, nur im Release-Build von Hand"]
fn app_perf_timing() {
    let lib = FontLibrary::bundled_only();

    // (f), (g): 50 Skizzen in 20 verschachtelten Gruppen.
    let s = grouped_state(&lib);
    let ids: Vec<_> = s.document.features().iter().map(|f| f.id).collect();
    measure("(f) is_feature_hidden, alle Features", |_| {
        let n = ids.iter().filter(|&&f| s.is_feature_hidden(f)).count();
        black_box(n);
    });
    let mut strokes = Vec::new();
    measure("(g) Zeichnen, 50 Skizzen x 5 Texte, 20 Gruppen", |_| {
        black_box(frame(&s, &mut strokes));
    });

    // (h) Geöffnete Skizze, ganz gewählter Text, Werkzeug Auswahl.
    let mut s = AppState::new();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let t = add_text(
        &mut sk,
        &two_hundred(),
        DVec2::new(0.0, 0.0),
        DVec2::new(150.0, 40.0),
        &lib,
    );
    for i in 0..4 {
        let y = 50.0 + 15.0 * i as f64;
        add_text(
            &mut sk,
            "Text 12",
            DVec2::new(0.0, y),
            DVec2::new(30.0, y + 10.0),
            &lib,
        );
    }
    let id = add_sketch(&mut s, sk);
    s.open_sketch(id).unwrap();
    s.camera.set_viewport(SCREEN.width(), SCREEN.height());
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.select_text(sk, t, false);
        assert!(!se.text_rotate_grips(sk, 0.1).is_empty(), "Drehgriff da");
    }
    let ctx = context();
    measure(
        "(h) Geöffnete Skizze, Text mit 200 Zeichen gewählt",
        |_| {
            black_box(overlay(&ctx, &mut s));
        },
    );

    // (i) Hover-Wahl über einer fertigen Skizze mit 2000 Kurven (nur Messung).
    let mut s = AppState::new();
    add_sketch(&mut s, grid_sketch(2000));
    s.camera.set_viewport(SCREEN.width(), SCREEN.height());
    s.camera.set_iso_view();
    let viewport = glam::Vec2::new(SCREEN.width(), SCREEN.height());
    let hits = (0..FRAMES)
        .filter(|&k| {
            let c = glam::Vec2::new(
                400.0 + 11.0 * (k % 10) as f32 * 10.0,
                300.0 + 5.0 * (k / 10) as f32 * 10.0,
            );
            s.pick_with(c, viewport, PickFilter::SELECT).is_some()
        })
        .count();
    println!("(i) Treffer: {hits} von {FRAMES} Zeigerstellungen");
    measure("(i) pick_with, fertige Skizze mit 2000 Kurven", |k| {
        let c = glam::Vec2::new(
            400.0 + 11.0 * (k % 10) as f32 * 10.0,
            300.0 + 5.0 * (k / 10) as f32 * 10.0,
        );
        black_box(s.pick_with(c, viewport, PickFilter::SELECT));
    });
}
