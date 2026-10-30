//! Körper und Modellieren auf Englisch (Sprachplan, Task 7): Erzeugen-Fenster, Bearbeiten,
//! Verrunden und Fase, Boolesche Operation, Parameter, Eigenschaften, Netz- und STEP-Import und
//! die Meldungen des Modells, unter `with_lang(&english(), …)` gezeichnet, ohne Fenster. Kein
//! Test ruft `set_language` oder `set_global` (Ruling 4): die Sprache gilt nur für den Thread
//! des Tests.
//!
//! Texte aus den Bibliotheken (`planora-feature`, `planora-kernel`, `planora-step`,
//! `planora-exchange`, `planora-sketch`) bleiben bis Task 11–13 deutsch; sie stehen in
//! [`LIBRARY_TEXTS`] und werden nur von der Deutsch-Prüfung ausgenommen, nicht von den
//! Erwartungen.

mod common;

use common::{Seen, assert_has, join, looks_like_key};
use egui::{Pos2, Rect, vec2};
use planora_app::blend_dialog::{ArtChoice, BlendDialog, no_edge, too_many_points};
use planora_app::boolean_dialog::BooleanDialog;
use planora_app::create_dialog::CreateDialog;
use planora_app::feature_dialog::{FeatureDialog, stale_message};
use planora_app::import_dialog::{ImportDialog, Unit, stale};
use planora_app::parameter_dialog::ParameterDialog;
use planora_app::places::Places;
use planora_app::state::*;
use planora_app::step_import_dialog::StepImportDialog;
use planora_app::{fonts, properties, theme};
use planora_exchange::{ExchangeMesh, ExportBody, Format, export_file};
use planora_feature::{
    BlendArt, FeatureId, FeatureKind, FieldKey, ParamEdit, ParamUnit, SketchPlane, StandardPlane,
};
use planora_i18n::{english, german, with_lang};
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::boolean::BoolOp;
use planora_kernel::{Edge, Id, Solid};
use planora_math::{DVec2, DVec3};
use planora_render::{EdgeRef, Pick};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Texte aus den Bibliotheken, die noch deutsch sind (Task 12/13: Lesefehler und Name
/// unbenannter STEP-Körper) und einen deutschen Buchstaben enthalten (nur solche Zeilen fielen
/// sonst durch). Die Texte von planora-feature, -sketch und -math (Feldnamen, Einheiten,
/// Formelfehler) sind seit Task 11 englisch und werden hier geprüft.
const LIBRARY_TEXTS: [&str; 3] = [
    "STEP-Körper",
    "Zeile 2: Die Fläche verweist auf einen Punkt",
    "Die Datei ist unvollständig",
];

/// Wie `common::assert_english`; die Zeilen mit einer der Wendungen `later` werden übergangen
/// (keine kurzen Bruchstücke, hinter denen sich Texte der App verstecken könnten).
fn assert_english(what: &str, seen: &[Seen], _skip: &[Rect], later: &[&str]) {
    for s in seen {
        for line in s.text.lines() {
            if later.iter().any(|l| line.contains(l)) {
                continue;
            }
            assert!(
                !line.chars().any(|c| "äöüßÄÖÜ„".contains(c)),
                "{what}: deutscher Text „{line}“ in {}",
                join(seen)
            );
            assert!(
                !looks_like_key(line),
                "{what}: Schlüssel-Id „{line}“ sichtbar in {}",
                join(seen)
            );
        }
    }
}

fn draw_ctx() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    theme::apply(&ctx, true);
    ctx
}

/// Ein Fenster ohne Bildschirm mit eigener Zeit (0,05 s je Bild).
struct Draw {
    ctx: egui::Context,
    time: f64,
}

impl Draw {
    fn new() -> Draw {
        Draw {
            ctx: draw_ctx(),
            time: 0.0,
        }
    }

    fn settle(&mut self, n: usize, f: &mut dyn FnMut(&mut egui::Ui)) -> Vec<Seen> {
        let mut seen = Vec::new();
        for _ in 0..n {
            self.time += 0.05;
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1080.0))),
                time: Some(self.time),
                ..Default::default()
            };
            let out = self.ctx.run_ui(raw, |ui| f(ui));
            seen = common::collect_seen(&out.shapes);
            out.drop_without_applying_deltas();
        }
        seen
    }

    /// Vier Bilder; `f` zeichnet mit dem Kontext.
    fn show(&mut self, f: &mut dyn FnMut(&egui::Context)) -> Vec<Seen> {
        self.settle(4, &mut |ui| f(ui.ctx()))
    }
}

/// Prüft ein Bild: kein deutscher Buchstabe und keine Schlüssel-Id (Bibliothekstexte
/// ausgenommen) und alle erwarteten Texte ganz da.
fn check(what: &str, seen: &[Seen], wanted: &[&str]) {
    assert_english(what, seen, &[], &LIBRARY_TEXTS);
    assert_has(what, seen, wanted);
}

/// Wie [`check`] für einen einzelnen Text (Meldung): englisch, ohne Schlüssel-Id.
fn plain(what: &str, text: &str) {
    for line in text.lines() {
        if LIBRARY_TEXTS.iter().any(|l| line.contains(l)) {
            continue;
        }
        assert!(
            !line.chars().any(|c| "äöüßÄÖÜ„".contains(c)),
            "{what}: deutscher Text „{line}“"
        );
        assert!(!looks_like_key(line), "{what}: Schlüssel-Id „{line}“");
    }
}

fn has_start(seen: &[Seen], start: &str) -> bool {
    seen.iter().any(|s| s.text.starts_with(start))
}

fn assert_starts(what: &str, seen: &[Seen], start: &str) {
    assert!(
        has_start(seen, start),
        "{what}: „{start} …“ fehlt in {}",
        join(seen)
    );
}

// ---- Modelle ----------------------------------------------------------------------------------

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id)
                .iter()
                .map(|u| face_name(s, u.face))
                .collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap()
}

/// Skizze mit Rechteck und Achse, geschlossen.
fn closed_sketch(s: &mut AppState) -> FeatureId {
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    {
        let sketch = s.session_sketch_mut().unwrap().1;
        sketch
            .add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(5.0, 10.0))
            .unwrap();
        let a = sketch.add_point(0.0, -2.0).unwrap();
        let b = sketch.add_point(0.0, 12.0).unwrap();
        let axis = sketch.add_line(a, b).unwrap();
        sketch.set_construction(axis, true).unwrap();
    }
    s.close_sketch();
    sk
}

fn box_with_fillet() -> (AppState, FeatureId, Id<Edge>) {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s.bodies[0].solid, "Deckel", "Wand 1");
    let id = s
        .add_blend(0, &[e], BlendArt::Rundung { radius: 2.0 }, true)
        .unwrap();
    (s, id, e)
}

/// Einheitswürfel, Dreiecke von außen gegen den Uhrzeigersinn.
fn cube() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7,
            3, 1, 2, 6, 1, 6, 5,
        ],
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let d = common::temp_root(&format!("planora-modelling-{name}"));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn write_stl(dir: &Path, name: &str, mesh: ExchangeMesh) -> PathBuf {
    let path = dir.join(name);
    export_file(
        &path,
        Format::Stl,
        &[ExportBody {
            name: "x".into(),
            mesh,
        }],
    )
    .unwrap();
    path
}

// ---- Erzeugen ---------------------------------------------------------------------------------

#[test]
fn create_dialogs_speak_english() {
    with_lang(&english(), || {
        let mut s = AppState::new();
        let mut d = Draw::new();
        for (mut dialog, wanted) in [
            (
                CreateDialog::Box {
                    x: 10.5,
                    y: 20.0,
                    z: 30.0,
                },
                vec![
                    "Create box",
                    "Width (x)",
                    "Depth (y)",
                    "Height (z)",
                    "Create",
                    "Cancel",
                    "10.5",
                ],
            ),
            (
                CreateDialog::Cylinder {
                    radius: 2.0,
                    height: 5.0,
                },
                vec!["Create cylinder", "Radius", "Height"],
            ),
            (
                CreateDialog::Sphere { radius: 2.0 },
                vec!["Create sphere", "Radius"],
            ),
            (
                CreateDialog::Cone {
                    radius: 2.0,
                    height: 5.0,
                },
                vec!["Create cone", "Radius", "Height"],
            ),
        ] {
            let seen = d.show(&mut |ctx| {
                assert!(dialog.show(ctx, &mut s));
            });
            check("Erzeugen", &seen, &wanted);
        }

        // Mit Parametern: Namen-Hinweis; kaputte Formel: Meldung unter dem Raster.
        let id = s.add_parameter().unwrap();
        s.edit_parameter(
            id,
            ParamEdit {
                name: "Breite".into(),
                formula: "40".into(),
                unit: ParamUnit::Length,
                comment: String::new(),
            },
        )
        .unwrap();
        let mut dialog = CreateDialog::Box {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        };
        let seen = d.show(&mut |ctx| {
            dialog.show(ctx, &mut s);
        });
        assert_starts("Namen", &seen, "Names: ");
        dialog.set_text(&d.ctx, &s, FieldKey::Breite, "Breit/2");
        let seen = d.show(&mut |ctx| {
            dialog.show(ctx, &mut s);
        });
        assert_english("Formel", &seen, &[], &LIBRARY_TEXTS);
        assert_starts("Formelfehler", &seen, "Width (x): ");

        // Skizzen: Extrudieren und Drehen.
        let mut s = AppState::new();
        let sk = closed_sketch(&mut s);
        let mut extrude = CreateDialog::extrude_sketch(sk);
        let seen = d.show(&mut |ctx| {
            extrude.show(ctx, &mut s);
        });
        check(
            "Extrudieren",
            &seen,
            &[
                "Extrude sketch",
                "Sketch",
                "Profile",
                "Profile 1: 50.00 mm²",
                "Height",
            ],
        );
        let mut revolve = CreateDialog::revolve_sketch(&s, sk);
        let seen = d.show(&mut |ctx| {
            revolve.show(ctx, &mut s);
        });
        check(
            "Drehen",
            &seen,
            &[
                "Revolve sketch",
                "Axis of revolution",
                "Construction line 1 (14.00 mm)",
            ],
        );
        // Ohne Skizze und ohne Profil.
        let mut gone = CreateDialog::extrude_sketch(FeatureId(999));
        let seen = d.show(&mut |ctx| {
            gone.show(ctx, &mut s);
        });
        check(
            "Skizze fehlt",
            &seen,
            &["(none)", "Sketch not found or not computed."],
        );
        let mut empty = AppState::new();
        let sk = empty
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        let mut dialog = CreateDialog::extrude_sketch(sk);
        let seen = d.show(&mut |ctx| {
            dialog.show(ctx, &mut empty);
        });
        check("Kein Profil", &seen, &["The sketch has no closed profile."]);
        let mut dialog = CreateDialog::revolve_sketch(&empty, sk);
        let seen = d.show(&mut |ctx| {
            dialog.show(ctx, &mut empty);
        });
        check(
            "Keine Achse",
            &seen,
            &["The sketch has no line that can serve as the axis of revolution."],
        );

        // Meldungen beim Erzeugen.
        let mut s = AppState::new();
        let sk = closed_sketch(&mut s);
        let no_axis = CreateDialog::RevolveSketch {
            sketch: sk,
            profile: 0,
            axis: None,
        };
        assert!(!no_axis.create(&mut s));
        let m = s.message.clone().unwrap();
        assert_eq!(
            m,
            "Please choose an axis of revolution (a line of the sketch)."
        );
        let bad = CreateDialog::Box {
            x: 0.0,
            y: 1.0,
            z: 1.0,
        };
        assert!(!bad.create(&mut s));
        assert_eq!(
            s.message.as_deref(),
            Some("Creating failed: The sizes must be greater than zero")
        );
    });
}

#[test]
fn the_create_fields_show_the_decimal_separator_of_the_language() {
    // Vorbelegte Zahlen folgen der Sprache (Ruling 5); getippte Formeln bleiben, wie sie sind.
    for (lang, expected) in [(german(), "2,5"), (english(), "2.5")] {
        with_lang(&lang, || {
            let s = AppState::new();
            let d = CreateDialog::Cylinder {
                radius: 2.5,
                height: 10.0,
            };
            let ctx = draw_ctx();
            let texts = d.texts(&ctx, &s);
            assert_eq!(texts[&FieldKey::Radius], expected);
            assert_eq!(texts[&FieldKey::Hoehe], "10");
            d.set_text(&ctx, &s, FieldKey::Radius, "Breite / 2,5");
            assert_eq!(d.texts(&ctx, &s)[&FieldKey::Radius], "Breite / 2,5");
        });
    }
}

// ---- Bearbeiten -------------------------------------------------------------------------------

impl Draw {
    /// Linksklick auf die Mitte des Textes `text` im Bild `seen`, dann einige Bilder.
    fn click_text(
        &mut self,
        seen: &[Seen],
        text: &str,
        f: &mut dyn FnMut(&egui::Context),
    ) -> Vec<Seen> {
        let at = seen
            .iter()
            .find(|s| s.text == text && s.clip.contains(s.rect.center()))
            .unwrap_or_else(|| panic!("„{text}“ nicht sichtbar in {}", join(seen)))
            .rect
            .center();
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        for events in [
            vec![egui::Event::PointerMoved(at)],
            vec![button(true)],
            vec![button(false)],
        ] {
            self.time += 0.05;
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1080.0))),
                time: Some(self.time),
                events,
                ..Default::default()
            };
            let out = self.ctx.run_ui(raw, |ui| f(ui.ctx()));
            out.drop_without_applying_deltas();
        }
        self.show(f)
    }
}

#[test]
fn edit_dialogs_speak_english() {
    with_lang(&english(), || {
        let mut d = Draw::new();
        let mut s = AppState::new();
        let boxed = s.create_box(DVec3::splat(10.0)).unwrap();
        let mut dialog = FeatureDialog::new(&s, boxed).unwrap();
        let seen = d.show(&mut |ctx| {
            assert!(dialog.show(ctx, &mut s));
        });
        let title = format!("Edit \"{}\"", s.document.feature(boxed).unwrap().name);
        check("Quader", &seen, &["Apply", "Cancel", &title]);

        // Extrusion und Rotation.
        let mut s = AppState::new();
        let sk = closed_sketch(&mut s);
        let e = s.extrude_sketch(sk, 0, 4.0).unwrap();
        let mut dialog = FeatureDialog::new(&s, e).unwrap();
        let seen = d.show(&mut |ctx| {
            dialog.show(ctx, &mut s);
        });
        check(
            "Extrusion",
            &seen,
            &["Sketch", "Profile", "Profile 1: 50.00 mm²", "Apply"],
        );
        let axis = s
            .document
            .sketch_of(sk)
            .and_then(|k| {
                planora_app::create_dialog::axis_candidates(k)
                    .first()
                    .map(|a| a.0)
            })
            .unwrap();
        let r = s.revolve_sketch(sk, 0, axis).unwrap();
        let mut dialog = FeatureDialog::new(&s, r).unwrap();
        let seen = d.show(&mut |ctx| {
            dialog.show(ctx, &mut s);
        });
        check(
            "Rotation",
            &seen,
            &["Axis of revolution", "Construction line 1 (14.00 mm)"],
        );

        // Boolesche Operation und Verschieben.
        let mut s = AppState::new();
        s.create_box(DVec3::splat(10.0)).unwrap();
        s.create_box(DVec3::splat(5.0)).unwrap();
        let bo = s.add_boolean(0, 1, BoolOp::Union).unwrap();
        let mut dialog = FeatureDialog::new(&s, bo).unwrap();
        let seen = d.show(&mut |ctx| {
            dialog.show(ctx, &mut s);
        });
        check(
            "Boolesch",
            &seen,
            &[
                "Target",
                "Tool",
                "Union",
                "Subtract (target − tool)",
                "Intersect",
            ],
        );
        s.move_body(0, DVec3::X).unwrap();
        let mv = s.document.features().last().unwrap().id;
        let mut dialog = FeatureDialog::new(&s, mv).unwrap();
        let seen = d.show(&mut |ctx| {
            dialog.show(ctx, &mut s);
        });
        assert_english("Verschieben", &seen, &[], &LIBRARY_TEXTS);
        assert_starts("Verschieben", &seen, "Body: ");

        // Rundung, variable Rundung, Fase.
        let (mut s, id, _) = box_with_fillet();
        let mut dialog = FeatureDialog::new(&s, id).unwrap();
        let seen = d.show(&mut |ctx| {
            dialog.show(ctx, &mut s);
        });
        check(
            "Rundung",
            &seen,
            &[
                "Fillet",
                "Fillet with variable radius",
                "Edges: 1",
                "Tangent chain (include smoothly connected edges)",
            ],
        );
        assert_starts("Rundung", &seen, "Smallest size: ");
        dialog.set_blend_art(ArtChoice::RundungVariabel);
        dialog.add_point();
        let seen = d.show(&mut |ctx| {
            dialog.show(ctx, &mut s);
        });
        check(
            "Variabel",
            &seen,
            &[
                "Position 1",
                "Radius 1",
                "Position 3",
                "Add point",
                "Remove point",
            ],
        );
        dialog.set_blend_art(ArtChoice::FaseZwei);
        let seen = d.show(&mut |ctx| {
            dialog.show(ctx, &mut s);
        });
        check(
            "Fase",
            &seen,
            &[
                "Chamfer: equal distance",
                "Chamfer: two distances",
                "Chamfer: distance and angle",
                "Face 1",
            ],
        );
    });
}

#[test]
fn edit_dialog_errors_speak_english() {
    with_lang(&english(), || {
        let (mut s, id, _) = box_with_fillet();
        let mut a = FeatureDialog::new(&s, id).unwrap();
        let b = FeatureDialog::new(&s, id).unwrap();
        a.set_tangent_chain(false);
        a.apply(&mut s).unwrap();
        let e = b.apply(&mut s).unwrap_err();
        assert_eq!(e, stale_message().text());
        assert_eq!(
            e,
            "The feature has been changed in the meantime; the window now shows the current \
             state. Please check the values and apply again."
        );
        plain("Veraltet", &e);
        assert_eq!(s.message.as_deref(), Some(e.as_str()));

        // Eine Formel in einem Punktfeld der variablen Rundung.
        let mut v = FeatureDialog::new(&s, id).unwrap();
        v.set_blend_art(ArtChoice::RundungVariabel);
        v.set_text("Radius 1", "Breite");
        let e = v.apply(&mut s).unwrap_err();
        assert_eq!(e, "Radius 1: Only numbers are possible here.");
        v.set_text("Radius 1", "");
        let e = v.apply(&mut s).unwrap_err();
        assert_eq!(e, "Invalid number for \"Radius 1\": \"\"");
        v.set_text("Radius 1", "1e12");
        let e = v.apply(&mut s).unwrap_err();
        plain("Zu groß", &e);
        assert!(
            e.starts_with("The value for \"Radius 1\" is too large"),
            "{e}"
        );

        // Zu kleines Maß.
        let mut small = FeatureDialog::new(&s, id).unwrap();
        small.set_text("Radius", "0,00001");
        let e = small.apply(&mut s).unwrap_err();
        plain("Zu klein", &e);
        assert!(
            e.starts_with("\"Radius\" is too small; the smallest possible size is "),
            "{e}"
        );
        assert!(e.ends_with(" mm"), "{e}");
    });
}

// ---- Verrunden und Fase -----------------------------------------------------------------------

#[test]
fn blend_dialog_speaks_english() {
    with_lang(&english(), || {
        let mut d = Draw::new();
        let (mut s, _, e) = box_with_fillet();
        s.set_selection(Some(Pick::Edge(EdgeRef { body: 0, edge: e })));
        let mut fillet = BlendDialog::fillet();
        let seen = d.show(&mut |ctx| {
            assert!(fillet.show(ctx, &mut s));
        });
        check(
            "Verrunden",
            &seen,
            &[
                "Fillet",
                "Chosen edges: 1",
                "Fillet with variable radius",
                "Radius",
                "Tangent chain (include smoothly connected edges)",
                "Create",
                "Cancel",
            ],
        );
        assert_starts("Verrunden", &seen, "Smallest size: ");

        let mut variable = BlendDialog::variable();
        let seen = d.show(&mut |ctx| {
            variable.show(ctx, &mut s);
        });
        check(
            "Variabel",
            &seen,
            &["Point 1", "Point 2", "%", "Remove", "Add point"],
        );

        let mut chamfer = BlendDialog::chamfer();
        let seen = d.show(&mut |ctx| {
            chamfer.show(ctx, &mut s);
        });
        check(
            "Fase",
            &seen,
            &[
                "Chamfer",
                "Chamfer: equal distance",
                "Chamfer: two distances",
                "Chamfer: distance and angle",
                "Distance",
            ],
        );
        chamfer.set_art(ArtChoice::FaseWinkel);
        let seen = d.show(&mut |ctx| {
            chamfer.show(ctx, &mut s);
        });
        check("Winkel", &seen, &["Distance", "Angle", "Face 1"]);
        assert_eq!(
            ArtChoice::FaseZwei.fields(),
            ("Distance 1".to_string(), Some("Distance 2".to_string()))
        );

        // Ohne Kante.
        let mut empty = AppState::new();
        empty.create_box(DVec3::splat(10.0)).unwrap();
        let mut none = BlendDialog::fillet();
        let seen = d.show(&mut |ctx| {
            none.show(ctx, &mut empty);
        });
        check(
            "Ohne Kante",
            &seen,
            &["Click an edge in the view (Shift: several)."],
        );
        assert!(none.execute(&mut empty));
        assert_eq!(
            none.error().as_deref(),
            Some("Please click an edge in the view first.")
        );
        let seen = d.show(&mut |ctx| {
            none.show(ctx, &mut empty);
        });
        check(
            "Meldung",
            &seen,
            &["Please click an edge in the view first."],
        );
    });
}

#[test]
fn blend_dialog_messages_speak_english() {
    with_lang(&english(), || {
        let (mut s, _, e) = box_with_fillet();
        s.set_selection(Some(Pick::Edge(EdgeRef { body: 0, edge: e })));
        let mut d = BlendDialog::fillet();
        d.first = "abc".into();
        assert!(d.execute(&mut s));
        assert_eq!(
            d.error().as_deref(),
            Some("\"Radius\" must be a number greater than zero (entered: \"abc\")")
        );
        d.first = "0,00001".into();
        assert!(d.execute(&mut s));
        let m = d.error().unwrap();
        plain("Zu klein", &m);
        assert!(
            m.starts_with("\"Radius\" is too small; the smallest possible size is 0.0"),
            "{m}"
        );
        let mut c = BlendDialog::chamfer();
        c.set_art(ArtChoice::FaseWinkel);
        c.second = "500".into();
        assert!(c.execute(&mut s));
        assert_eq!(
            c.error().as_deref(),
            Some("\"Angle\" must be between 1° and 179° (entered: \"500\")")
        );
        let mut v = BlendDialog::variable();
        v.points[1].0 = "150".into();
        assert!(v.execute(&mut s));
        assert_eq!(
            v.error().as_deref(),
            Some("\"Position 2\" must be between 0 and 100 %")
        );
        // Mehr als 64 Punkte.
        v.points = (0..64).map(|i| (i.to_string(), "1".to_string())).collect();
        v.add_point();
        assert_eq!(
            v.error().as_deref(),
            Some("A variable radius can have at most 64 points")
        );
        assert_eq!(too_many_points().text(), v.error().unwrap());
        assert_eq!(no_edge().text(), "Please click an edge in the view first.");
        // Kanten zweier Körper.
        let mut two = AppState::new();
        two.create_box(DVec3::splat(10.0)).unwrap();
        two.create_box(DVec3::splat(10.0)).unwrap();
        let e0 = edge_between(&two.bodies[0].solid, "Deckel", "Wand 1");
        let e1 = edge_between(&two.bodies[1].solid, "Deckel", "Wand 1");
        two.marked_edges = vec![EdgeRef { body: 0, edge: e0 }, EdgeRef { body: 1, edge: e1 }];
        let mut m = BlendDialog::fillet();
        assert!(m.execute(&mut two));
        let text = m.error().unwrap();
        plain("Zwei Körper", &text);
        assert!(
            text.starts_with("The edges belong to different bodies ("),
            "{text}"
        );
    });
}

// ---- Boolesche Operation ----------------------------------------------------------------------

#[test]
fn boolean_dialog_speaks_english() {
    with_lang(&english(), || {
        let mut d = Draw::new();
        let mut s = AppState::new();
        s.create_box(DVec3::splat(10.0)).unwrap();
        s.create_cylinder(2.0, 14.0).unwrap();
        let mut dialog = BooleanDialog::new(&s);
        let seen = d.show(&mut |ctx| {
            assert!(dialog.show(ctx, &mut s));
        });
        check(
            "Boolesch",
            &seen,
            &[
                "Boolean operation",
                "Body A",
                "Body B",
                "Union",
                "Subtract (A − B)",
                "Intersect",
                "Run",
                "Cancel",
            ],
        );
        // Zwei gleiche Körper: Meldung im Fenster.
        dialog.b = dialog.a;
        assert!(dialog.execute(&mut s));
        assert_eq!(
            dialog.error().as_deref(),
            Some("Please choose two different bodies")
        );
        let seen = d.show(&mut |ctx| {
            dialog.show(ctx, &mut s);
        });
        check("Meldung", &seen, &["Please choose two different bodies"]);
        // Gesperrte Einträge: Gründe.
        assert_eq!(
            AppState::new().boolean_blocked().as_deref(),
            Some("This needs at least two bodies.")
        );
        assert_eq!(
            AppState::new().blend_blocked().as_deref(),
            Some("This needs at least one body.")
        );
        assert_eq!(
            AppState::new().sketch_on_face_blocked().as_deref(),
            Some("First click a flat face of a body.")
        );
    });
}

// ---- Parameter --------------------------------------------------------------------------------

#[test]
fn parameter_dialog_speaks_english() {
    with_lang(&english(), || {
        let mut d = Draw::new();
        let mut s = AppState::new();
        let mut dialog = ParameterDialog::new(&s);
        let seen = d.show(&mut |ctx| {
            assert!(dialog.show(ctx, &mut s, false));
        });
        check(
            "Leer",
            &seen,
            &["Parameters", "No parameters yet.", "Add row"],
        );

        let a = s.add_parameter().unwrap();
        s.edit_parameter(
            a,
            ParamEdit {
                name: "P".into(),
                formula: "20".into(),
                unit: ParamUnit::Length,
                comment: "x".into(),
            },
        )
        .unwrap();
        let b = s.add_parameter().unwrap();
        let mut dialog = ParameterDialog::new(&s);
        let seen = d.show(&mut |ctx| {
            dialog.show(ctx, &mut s, false);
        });
        check(
            "Zeilen",
            &seen,
            &["Name", "Formula", "Value", "Type", "Comment", "Add row"],
        );
        // Zwei kaputte Entwürfe: erste Meldung mit Zähler.
        dialog.set_text(a, planora_app::parameter_dialog::Column::Formula, "1/");
        dialog.set_text(b, planora_app::parameter_dialog::Column::Formula, "2*");
        let seen = d.show(&mut |ctx| {
            dialog.show(ctx, &mut s, false);
        });
        assert!(
            seen.iter().any(|t| t.text.ends_with("(+ 1 more error)")),
            "{}",
            join(&seen)
        );
        // Ein benutzter Parameter: Rückfrage.
        let mut s = AppState::new();
        let p = s.add_parameter().unwrap();
        s.edit_parameter(
            p,
            ParamEdit {
                name: "Br".into(),
                formula: "20".into(),
                unit: ParamUnit::Length,
                comment: String::new(),
            },
        )
        .unwrap();
        let mut formulas = planora_feature::Formulas::new();
        formulas.insert(FieldKey::Breite, "Br".to_string());
        s.create_box_with(DVec3::splat(20.0), formulas).unwrap();
        let mut dialog = ParameterDialog::new(&s);
        dialog.request_delete(p, &mut s).unwrap();
        assert!(dialog.confirm_text().is_some());
        let seen = d.show(&mut |ctx| {
            dialog.show(ctx, &mut s, false);
        });
        check("Rückfrage", &seen, &["Replace with value", "Cancel"]);

        // Gesperrt: alte Version, offene Skizze.
        let seen = d.show(&mut |ctx| {
            dialog.show(ctx, &mut s, true);
        });
        check(
            "Alte Version",
            &seen,
            &["Nothing can be changed in an old version."],
        );
        let sk = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(sk).unwrap();
        let seen = d.show(&mut |ctx| {
            dialog.show(ctx, &mut s, false);
        });
        check("Skizze offen", &seen, &["Please finish the sketch first."]);
        assert_eq!(
            s.add_parameter().unwrap_err(),
            "Please finish the sketch first."
        );
        assert_eq!(
            planora_app::parameter_dialog::dialog_closed(),
            "The window was closed because parameters changed."
        );
    });
}

// ---- Eigenschaften ----------------------------------------------------------------------------

/// Zeichnet die Eigenschaften in einem Seitenfeld.
fn props(d: &mut Draw, s: &mut AppState) -> Vec<Seen> {
    d.settle(4, &mut |ui| {
        egui::CentralPanel::default().show(ui, |ui| properties::show(ui, s));
    })
}

fn coarse_box() -> Arc<planora_feature::StepShape> {
    let k = planora_kernel::AnalyticKernel::default();
    let solid = planora_kernel::Kernel::make_box(&k, DVec3::new(10.0, 20.0, 30.0)).unwrap();
    let n = planora_step::normalize(&solid, "Teil").unwrap();
    Arc::new(planora_feature::StepShape::with_solid(
        n.text, 1e-3, n.solid,
    ))
}

#[test]
fn properties_speak_english() {
    with_lang(&english(), || {
        let mut d = Draw::new();
        let mut s = AppState::new();
        let seen = props(&mut d, &mut s);
        check("Leer", &seen, &["Properties", "Nothing selected."]);

        let id = s.create_box(DVec3::new(10.0, 20.0, 30.0)).unwrap();
        s.selected_body = Some(0);
        let seen = props(&mut d, &mut s);
        check(
            "Körper",
            &seen,
            &[
                "Properties",
                "Name",
                "Volume",
                "Surface area",
                "Center of mass",
                "Dimensions",
                "Faces / edges",
                "Position x / y / z in mm",
                "Click a face or an edge in the viewport.",
            ],
        );
        let face = s.bodies[0].solid.faces.iter().next().unwrap().0;
        s.set_selection(Some(Pick::Face(planora_render::FaceRef { body: 0, face })));
        let seen = props(&mut d, &mut s);
        assert_english("Fläche", &seen, &[], &LIBRARY_TEXTS);
        assert_starts("Fläche", &seen, "Selected face: ");
        let edge = s.bodies[0].solid.edges.iter().next().unwrap().0;
        s.set_selection(Some(Pick::Edge(EdgeRef { body: 0, edge })));
        let seen = props(&mut d, &mut s);
        assert_english("Kante", &seen, &[], &LIBRARY_TEXTS);
        assert_starts("Kante", &seen, "Selected edge: ");

        // Ein Feature: Status und Schaltfläche.
        s.selected_body = None;
        s.selected_feature = Some(id);
        let seen = props(&mut d, &mut s);
        check(
            "Feature",
            &seen,
            &[
                "Name", "Type", "Status", "computed", "Size", "Position", "Edit …",
            ],
        );
        let sk = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.selected_feature = Some(sk);
        let seen = props(&mut d, &mut s);
        check(
            "Skizze",
            &seen,
            &["Plane", "XY plane", "Curves", "Edge copies", "Open sketch"],
        );

        // Verrunden, Fase und Boolesche Operation: Zeilen.
        let (s2, fid, _) = box_with_fillet();
        let kind = s2.document.feature(fid).unwrap().kind.clone();
        let rows = properties::parameter_rows(&s2, &kind);
        let labels: Vec<&str> = rows.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(labels, ["Body", "Edges", "Type", "Radius", "Tangent chain"]);
        assert_eq!(rows[4].1, "on");
        let chamfer = FeatureKind::Verrunden {
            body: planora_feature::BodyRef {
                feature: FeatureId(1),
                index: 0,
            },
            edges: Vec::new(),
            art: BlendArt::FaseWinkel {
                abstand: 1.0,
                winkel: 30.0,
            },
            tangentenkette: false,
        };
        let rows = properties::parameter_rows(&s2, &chamfer);
        assert_eq!(rows[3].0, "Distance");
        assert_eq!(rows[4], ("Angle".to_string(), "30.00 °".to_string()));
        assert_eq!(rows[5].1, "off");
        let rows = properties::parameter_rows(
            &s2,
            &FeatureKind::Boolean {
                target: planora_feature::BodyRef {
                    feature: FeatureId(77),
                    index: 0,
                },
                tool: planora_feature::BodyRef {
                    feature: FeatureId(78),
                    index: 0,
                },
                op: BoolOp::Difference,
            },
        );
        assert_eq!(rows[0], ("Target".to_string(), "Feature 77".to_string()));
        assert_eq!(rows[2], ("Operation".to_string(), "Subtract".to_string()));

        // STEP-Körper, grob.
        let mut st = AppState::new();
        st.import_step(vec![("Grob".into(), coarse_box())]).unwrap();
        st.selected_body = Some(0);
        let seen = props(&mut d, &mut st);
        assert_english("STEP", &seen, &[], &LIBRARY_TEXTS);
        check(
            "STEP",
            &seen,
            &[
                "Faces in the file",
                "Accuracy of the file",
                "0.001 mm",
                "Calculating with this body may fail.",
            ],
        );
        let kind = st
            .document
            .feature(st.document.features()[0].id)
            .unwrap()
            .kind
            .clone();
        let rows = properties::parameter_rows(&st, &kind);
        assert_eq!(rows[0].0, "Faces");
    });
}

#[test]
fn suppressed_and_error_states_speak_english() {
    with_lang(&english(), || {
        let mut d = Draw::new();
        let mut s = AppState::new();
        let id = s.create_box(DVec3::splat(10.0)).unwrap();
        s.document.set_suppressed(id, true).unwrap();
        s.refresh();
        s.selected_feature = Some(id);
        let seen = props(&mut d, &mut s);
        check("Unterdrückt", &seen, &["suppressed"]);
    });
}

#[test]
fn mesh_body_properties_speak_english() {
    with_lang(&english(), || {
        let mut d = Draw::new();
        let mut s = AppState::new();
        let mut open = cube();
        open.indices.truncate(33);
        s.import_mesh("Open", Arc::new(open), 1.0).unwrap();
        s.selected_body = Some(0);
        let seen = props(&mut d, &mut s);
        check(
            "Netz",
            &seen,
            &[
                "Mesh body",
                "Volume",
                "Dimensions",
                "Triangles",
                "Closed at import",
                "no",
                "Position x",
                "Position y",
                "Position z",
                "Scale",
                "Rotation x / y / z in degrees",
                "Mesh bodies can be moved, rotated, scaled, measured and exported, but not \
                 combined with other bodies, filleted or used as a face for a sketch.",
            ],
        );
        assert_starts(
            "Warnung",
            &seen,
            "At import: The mesh is not fully closed: 3 open edges",
        );

        let mut closed = AppState::new();
        closed.import_mesh("Closed", Arc::new(cube()), 1.0).unwrap();
        closed.selected_body = Some(0);
        let seen = props(&mut d, &mut closed);
        check("Geschlossen", &seen, &["yes"]);
        // Feature eines Netz-Körpers.
        let fid = closed.document.features()[0].id;
        let kind = closed.document.feature(fid).unwrap().kind.clone();
        let rows = properties::parameter_rows(&closed, &kind);
        let labels: Vec<&str> = rows.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(labels, ["Mesh", "Position", "Rotation", "Scale"]);
        assert_eq!(rows[0].1, "12 triangles");
        // Sperren und Namen.
        assert_eq!(
            closed.boolean_blocked().as_deref(),
            Some("Mesh bodies cannot be edited.")
        );
        assert_eq!(
            closed.blend_blocked().as_deref(),
            Some("Mesh bodies cannot be edited.")
        );
        let mut named = AppState::new();
        named.import_mesh("", Arc::new(cube()), 1.0).unwrap();
        assert_eq!(named.bodies[0].name, "Mesh body 1");
        // Vorschau.
        let mut big = AppState::new();
        big.display_triangles = 5;
        big.import_mesh("Big", Arc::new(cube()), 1.0).unwrap();
        let note = big.mesh_note().unwrap();
        plain("Vorschau", &note);
        assert_eq!(
            note,
            "\"Big\": Note: The mesh is very large; the view shows only some of the triangles. \
             It is saved and exported in full."
        );
        assert_eq!(
            planora_app::mesh_body::report_text(&closed.bodies[0].mesh_report.unwrap()),
            "The mesh is closed."
        );
        let r = planora_exchange::MeshReport {
            open_edges: 1,
            flipped_edges: 2,
            non_manifold_edges: 1,
            degenerate_triangles: 1,
            ..planora_exchange::MeshReport::default()
        };
        assert_eq!(
            planora_app::mesh_body::report_text(&r),
            "The mesh is not fully closed: 1 open edge, 2 edges with flipped triangles, \
             1 edge shared by more than two triangles. 1 triangle has no area."
        );
    });
}

#[test]
fn prefilled_numbers_follow_the_language_in_the_properties() {
    // Ruling 5: Zahlen, die aus einem Wert vorbelegt werden, zeigen den Trenner der Sprache;
    // winzige negative Werte ergeben „0“, nie „-0“.
    let build = || {
        let mut s = AppState::new();
        s.import_mesh("M", Arc::new(cube()), 1.0).unwrap();
        let t = s.mesh_transform(0).unwrap();
        s.set_mesh_transform(
            0,
            planora_exchange::MeshTransform {
                offset: DVec3::new(2.5, -0.000_012_345_678_9, 1_234.567_890_12),
                ..t
            },
        )
        .unwrap();
        s.selected_body = Some(0);
        s
    };
    for (lang, expected) in [
        (german(), ["2,5", "0", "1234,5679"]),
        (english(), ["2.5", "0", "1234.5679"]),
    ] {
        with_lang(&lang, || {
            let mut d = Draw::new();
            let mut s = build();
            let seen = props(&mut d, &mut s);
            for want in expected {
                assert!(
                    seen.iter().any(|t| t.text == want),
                    "{} fehlt in {}",
                    want,
                    join(&seen)
                );
            }
            assert!(
                !seen.iter().any(|t| t.text == "-0"
                    || t.text.starts_with("-0,0")
                    || t.text.starts_with("-0.0")),
                "{}",
                join(&seen)
            );
        });
    }
}

// ---- Netz importieren -------------------------------------------------------------------------

fn import_frame(d: &mut Draw, dialog: &mut ImportDialog, s: &mut AppState) -> Vec<Seen> {
    let mut dir = None;
    d.show(&mut |ctx| {
        dialog.show_in(ctx, s, &mut dir, &mut Places::default());
    })
}

#[test]
fn mesh_import_speaks_english() {
    with_lang(&english(), || {
        let dir = temp_dir("mesh");
        let mut d = Draw::new();
        let mut s = AppState::new();
        let mut dialog = ImportDialog::new(dir.clone());
        let seen = import_frame(&mut d, &mut dialog, &mut s);
        check("Wahl", &seen, &["Import mesh", "Choose file (STL or OBJ)"]);

        // Gelesen, geschlossen.
        let path = write_stl(&dir, "cube.stl", cube());
        dialog.load(path.clone());
        assert!(dialog.is_loading());
        dialog.tick(true);
        let seen = import_frame(&mut d, &mut dialog, &mut s);
        check(
            "Geprüft",
            &seen,
            &[
                "Check",
                "File: cube.stl",
                "12 triangles, 8 points",
                "The mesh is closed.",
                "Name",
                "Unit of the file",
                "Millimeters",
                "Centimeters",
                "Meters",
                "Inches",
                "Import",
            ],
        );
        // Offenes Netz.
        let mut open = cube();
        open.indices.truncate(33);
        let path2 = write_stl(&dir, "open.stl", open);
        dialog.load(path2);
        dialog.tick(true);
        let seen = import_frame(&mut d, &mut dialog, &mut s);
        check(
            "Offen",
            &seen,
            &[
                "You can still create and show the mesh body; volume and surface area are then only approximate.",
            ],
        );
        assert_eq!(Unit::Zoll.label(), "Inches");
        // Abbrechen.
        dialog.load(path);
        dialog.cancel();
        assert_eq!(dialog.error_text().as_deref(), Some("Cancelled"));
        let seen = import_frame(&mut d, &mut dialog, &mut s);
        check("Abgebrochen", &seen, &["Cancelled"]);
        // Lesefehler: Text vom Leser (Bibliothek) bleibt deutsch, die Hülle ist englisch.
        let bad = dir.join("bad.obj");
        fs::write(&bad, "v 0 0 0\nf 1 2 3\n").unwrap();
        dialog.load(bad);
        dialog.tick(true);
        let seen = import_frame(&mut d, &mut dialog, &mut s);
        assert_english("Lesefehler", &seen, &[], &LIBRARY_TEXTS);
        // Veraltet.
        let mut dialog = ImportDialog::new(dir.clone());
        dialog.load_at(dir.join("cube.stl"), s.document.edit_count());
        dialog.tick(true);
        s.create_box(DVec3::splat(1.0)).unwrap();
        let seen = import_frame(&mut d, &mut dialog, &mut s);
        check(
            "Veraltet",
            &seen,
            &["The document has changed since the file was read. Please choose the file again."],
        );
        assert_eq!(dialog.error, Some(stale()));
    });
}

// ---- STEP importieren -------------------------------------------------------------------------

fn step_data(name: &str) -> PathBuf {
    [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "planora-step",
        "tests",
        "data",
        name,
    ]
    .iter()
    .collect()
}

fn step_frame(d: &mut Draw, dialog: &mut StepImportDialog, s: &mut AppState) -> Vec<Seen> {
    let mut dir = None;
    d.show(&mut |ctx| {
        dialog.show_in(ctx, s, &mut dir, &mut Places::default());
    })
}

#[test]
fn step_import_speaks_english() {
    with_lang(&english(), || {
        let dir = temp_dir("step");
        let mut d = Draw::new();
        let mut s = AppState::new();
        let mut dialog = StepImportDialog::new(dir.clone());
        let seen = step_frame(&mut d, &mut dialog, &mut s);
        check("Wahl", &seen, &["Import STEP", "Choose file (STEP)"]);

        let file = dir.join("box.step");
        fs::copy(step_data("box_ap214.step"), &file).unwrap();
        dialog.load(file.clone());
        assert_eq!(dialog.phase().as_deref(), Some("Reading the file …"));
        dialog.tick(true);
        let mut files = LIBRARY_TEXTS.to_vec();
        files.extend(["W\u{fc}rfel", "Deckel", "Wand"]);
        let seen = step_frame(&mut d, &mut dialog, &mut s);
        assert_english("Liste", &seen, &[], &files);
        assert_has(
            "Liste",
            &seen,
            &[
                "File",
                "Unit of the file",
                "Millimeters",
                "Inches",
                "Name",
                "Faces",
                "Size",
                "Unit",
                "Status",
                "will be imported",
                "Import",
                "Choose another file …",
                "Close",
                "1 of 1 body can be imported.",
            ],
        );

        // Andere Einheit: Hinweis.
        dialog.set_unit(planora_step::LengthUnit::Zoll);
        dialog.tick(true);
        let seen = step_frame(&mut d, &mut dialog, &mut s);
        assert_starts("Einheit", &seen, "The file names ");
        assert_english("Einheit", &seen, &[], &files);

        // Importieren: Zusammenfassung in der Statuszeile.
        dialog.import(&mut s).unwrap();
        let info = s.info.clone().unwrap();
        assert_eq!(info, "1 body imported");

        // Veraltet, abgebrochen, Ablehnung.
        let mut dialog = StepImportDialog::new(dir.clone());
        dialog.load_at(file.clone(), s.document.edit_count());
        dialog.tick(true);
        s.create_box(DVec3::splat(1.0)).unwrap();
        let seen = step_frame(&mut d, &mut dialog, &mut s);
        check(
            "Veraltet",
            &seen,
            &["The document has changed since the file was read. Please choose the file again."],
        );
        let mut dialog = StepImportDialog::new(dir);
        dialog.load(file);
        dialog.cancel();
        assert_eq!(dialog.error_text().as_deref(), Some("Cancelled"));
        let mut fresh = AppState::new();
        assert_eq!(
            dialog.import(&mut fresh).unwrap_err(),
            "Please choose a file first."
        );
    });
}

#[test]
fn step_import_texts_speak_english() {
    use planora_app::step_import_dialog::*;
    with_lang(&english(), || {
        assert_eq!(summary(1, &[]), "1 body imported");
        assert_eq!(summary(3, &[]), "3 bodies imported");
        let skipped: Vec<(String, String)> = (1..=5)
            .map(|i| (format!("B{i}"), format!("why {i}")))
            .collect();
        assert_eq!(
            summary(2, &skipped),
            "2 bodies imported, 5 skipped: B1 (why 1); B2 (why 2); B3 (why 3); and 2 more"
        );
        assert_eq!(
            size_text(Some((DVec3::ZERO, DVec3::new(25.4, 50.8, 76.2)))),
            "25.4 × 50.8 × 76.2 mm"
        );
        for text in [
            checking(),
            reading(),
            no_unit_hint(),
            unclear_unit_hint(),
            unreadable_unit_hint(),
            mixed_unit_hint(),
            free_skipped_hint(),
            mixed_unit(),
            nothing_checked(),
            size_hint(),
            menu_label(),
            title(),
        ] {
            plain("STEP-Text", &text);
        }
        assert_eq!(
            size_hint(),
            "Size of the bounding box, rounded to 0.1 mm; approximate for curved surfaces."
        );
        assert_eq!(menu_label(), "Import STEP …");
    });
}

// ---- Meldungen des Modells --------------------------------------------------------------------

#[test]
fn model_messages_speak_english() {
    with_lang(&english(), || {
        let mut s = AppState::new();
        s.create_box(DVec3::splat(10.0)).unwrap();
        // Namen: Skizze und Extrusion in der Sprache des Anlegens.
        let sk = closed_sketch(&mut s);
        assert_eq!(s.document.feature(sk).unwrap().name, "Sketch 1");
        let e = s.extrude_sketch(sk, 0, 3.0).unwrap();
        assert_eq!(s.document.feature(e).unwrap().name, "Extrusion: Sketch 1");
        assert_eq!(
            s.extrude_sketch(sk, 0, 0.0).unwrap_err(),
            "The height must be greater than zero"
        );
        assert_eq!(
            s.extrude_sketch(sk, 5, 3.0).unwrap_err(),
            "Profile not found"
        );
        assert_eq!(
            s.extrude_sketch(FeatureId(999), 0, 3.0).unwrap_err(),
            "Sketch not found"
        );
        assert_eq!(
            s.open_sketch(FeatureId(999)).unwrap_err(),
            "Sketch not found"
        );
        // Weitere Fehler.
        assert_eq!(s.move_body(99, DVec3::X).unwrap_err(), "Body not found");
        assert_eq!(
            s.move_body(0, DVec3::splat(f64::NAN)).unwrap_err(),
            "Invalid displacement"
        );
        let e = s.move_body(0, DVec3::splat(1e30)).unwrap_err();
        plain("Verschieben", &e);
        assert!(
            e.starts_with("Move rejected: The body would lie outside ±"),
            "{e}"
        );
        assert_eq!(
            s.create_box(DVec3::ZERO).unwrap_err(),
            "Creating failed: The sizes must be greater than zero"
        );
        let e = s.create_box(DVec3::splat(1e30)).unwrap_err();
        assert!(
            e.starts_with("Creating failed: The sizes may be at most "),
            "{e}"
        );
        assert_eq!(
            s.add_boolean(0, 0, BoolOp::Union).unwrap_err(),
            "Please choose two different bodies"
        );
        assert_eq!(
            s.add_blend(0, &[], BlendArt::Rundung { radius: 1.0 }, true)
                .unwrap_err(),
            "Please click an edge first"
        );
        assert_eq!(s.delete_body(99).unwrap_err(), "Body not found");
        assert_eq!(
            s.new_sketch_on_face(99, Id::from_index(0)).unwrap_err(),
            "Body not found"
        );
        // Das Feature hat Abhängige.
        let e = s.delete_feature(sk).unwrap_err();
        plain("Löschen", &e);
        assert!(
            e.contains("cannot be deleted because these features depend on it: "),
            "{e}"
        );
        // Offene Skizze.
        let sk2 = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XZ))
            .unwrap();
        s.open_sketch(sk2).unwrap();
        assert_eq!(
            s.end_sketch_for(Some(sk2)).unwrap_err(),
            "Please finish the sketch first (button \"Finish sketch\" at the bottom), then try again"
        );
        let e = s
            .project_edge_into_sketch(99, Id::from_index(0))
            .unwrap_err();
        assert_eq!(e, "Body not found");
        s.close_sketch();
        assert_eq!(
            s.project_edge_into_sketch(0, Id::from_index(0))
                .unwrap_err(),
            "No sketch is open"
        );
        assert_eq!(undo_failed(), "Undo is not possible.");
        assert_eq!(redo_failed(), "Redo is not possible.");
        assert_eq!(
            mesh_note(),
            "Note: The mesh of this body is inexact in places; the view may show small gaps."
        );
        assert_eq!(
            planora_app::formula_field::empty(),
            "Please enter a number or a formula."
        );
        assert_eq!(
            s.body_ref_name(planora_feature::BodyRef {
                feature: FeatureId(4242),
                index: 0
            }),
            "Feature 4242"
        );
        assert_eq!(AppState::new().document_name, "Untitled");
        let mut m = AppState::new();
        assert_eq!(
            m.set_mesh_transform(0, planora_exchange::MeshTransform::default())
                .unwrap_err(),
            "This is not a mesh body"
        );
    });
}

// ---- Gespeicherte Meldungen -------------------------------------------------------------------

#[test]
fn stored_messages_follow_the_language() {
    // Meldungen, die ein Fenster behält, sind unter Deutsch entstanden und erscheinen im
    // nächsten Bild englisch.
    let mut d = Draw::new();

    // Bearbeiten: veraltetes Fenster.
    let (mut s, id, e) = box_with_fillet();
    let mut a = FeatureDialog::new(&s, id).unwrap();
    let mut b = FeatureDialog::new(&s, id).unwrap();
    with_lang(&german(), || {
        a.set_tangent_chain(false);
        a.apply(&mut s).unwrap();
        let seen = d.show(&mut |ctx| {
            b.show(ctx, &mut s);
        });
        d.click_text(&seen, "Übernehmen", &mut |ctx| {
            b.show(ctx, &mut s);
        });
    });
    with_lang(&english(), || {
        let seen = d.show(&mut |ctx| {
            b.show(ctx, &mut s);
        });
        check(
            "Veraltet",
            &seen,
            &[
                "The feature has been changed in the meantime; the window now shows the current state. Please check the values and apply again.",
            ],
        );
    });

    // Verrunden: keine Kante, zu kleines Maß.
    let mut empty = AppState::new();
    empty.create_box(DVec3::splat(10.0)).unwrap();
    let mut none = BlendDialog::fillet();
    with_lang(&german(), || {
        assert!(none.execute(&mut empty));
        assert_eq!(
            none.error().as_deref(),
            Some("Bitte zuerst eine Kante im Fenster anklicken.")
        );
    });
    with_lang(&english(), || {
        let seen = d.show(&mut |ctx| {
            none.show(ctx, &mut empty);
        });
        check(
            "Verrunden",
            &seen,
            &["Please click an edge in the view first."],
        );
    });
    s.set_selection(Some(Pick::Edge(EdgeRef { body: 0, edge: e })));
    let mut small = BlendDialog::fillet();
    small.first = "0,00001".into();
    with_lang(&german(), || {
        assert!(small.execute(&mut s));
        assert!(small.error().unwrap().contains("zu klein"));
    });
    with_lang(&english(), || {
        let text = small.error().unwrap();
        plain("Zu klein", &text);
        assert!(text.contains("is too small"), "{text}");
        // Die Zahl folgt der Sprache.
        assert!(text.contains("0.0"), "{text}");
        let seen = d.show(&mut |ctx| {
            small.show(ctx, &mut s);
        });
        assert!(
            has_start(&seen, "\"Radius\" is too small"),
            "{}",
            join(&seen)
        );
    });

    // Fase: die Bezeichnung des Feldes in der Meldung wechselt mit (Abstand 2 / Distance 2).
    let mut chamfer = BlendDialog::chamfer();
    chamfer.set_art(ArtChoice::FaseZwei);
    chamfer.second = "0,00001".into();
    with_lang(&german(), || {
        assert!(chamfer.execute(&mut s));
        assert!(
            chamfer
                .error()
                .unwrap()
                .starts_with("„Abstand 2“ ist zu klein")
        );
    });
    with_lang(&english(), || {
        let text = chamfer.error().unwrap();
        assert!(text.starts_with("\"Distance 2\" is too small;"), "{text}");
    });
    let mut angle = BlendDialog::chamfer();
    angle.set_art(ArtChoice::FaseWinkel);
    angle.first = "abc".into();
    with_lang(&german(), || {
        assert!(angle.execute(&mut s));
        assert!(
            angle
                .error()
                .unwrap()
                .starts_with("„Abstand“ muss eine Zahl")
        );
    });
    with_lang(&english(), || {
        let text = angle.error().unwrap();
        assert!(text.starts_with("\"Distance\" must be a number"), "{text}");
    });
    // Kanten zweier Körper, einer davon unbekannt: der Ersatzname wechselt mit.
    let mut mixed = BlendDialog::fillet();
    let mut m = AppState::new();
    m.create_box(DVec3::splat(10.0)).unwrap();
    let me = edge_between(&m.bodies[0].solid, "Deckel", "Wand 1");
    m.marked_edges = vec![EdgeRef { body: 0, edge: me }, EdgeRef { body: 5, edge: me }];
    with_lang(&german(), || {
        assert!(mixed.execute(&mut m));
        assert!(mixed.error().unwrap().contains("Körper 6"));
    });
    with_lang(&english(), || {
        let text = mixed.error().unwrap();
        assert!(text.contains("Body 6"), "{text}");
        assert!(text.ends_with("only one body at a time"), "{text}");
    });
    // Variable Rundung im Bearbeiten-Fenster: Meldung mit Bezeichnung des Punktfeldes.
    let mut var = FeatureDialog::new(&s, id).unwrap();
    var.set_blend_art(ArtChoice::RundungVariabel);
    var.set_text("Radius 2", "");
    let mut d2 = Draw::new();
    with_lang(&german(), || {
        let seen = d2.show(&mut |ctx| {
            var.show(ctx, &mut s);
        });
        d2.click_text(&seen, "Übernehmen", &mut |ctx| {
            var.show(ctx, &mut s);
        });
        assert_eq!(
            var.error().as_deref(),
            Some("Ungültige Zahl bei „Radius 2“: „“")
        );
    });
    with_lang(&english(), || {
        assert_eq!(
            var.error().as_deref(),
            Some("Invalid number for \"Radius 2\": \"\"")
        );
    });

    // Boolesche Operation.
    let mut two = AppState::new();
    two.create_box(DVec3::splat(10.0)).unwrap();
    two.create_box(DVec3::splat(5.0)).unwrap();
    let mut bool_dialog = BooleanDialog::new(&two);
    bool_dialog.b = bool_dialog.a;
    with_lang(&german(), || {
        assert!(bool_dialog.execute(&mut two));
        assert_eq!(
            bool_dialog.error().as_deref(),
            Some("Bitte zwei verschiedene Körper wählen")
        );
    });
    with_lang(&english(), || {
        let seen = d.show(&mut |ctx| {
            bool_dialog.show(ctx, &mut two);
        });
        check("Boolesch", &seen, &["Please choose two different bodies"]);
    });

    // Parameter: Zeile weg.
    let mut ps = AppState::new();
    let p = ps.add_parameter().unwrap();
    let mut pd = ParameterDialog::new(&ps);
    ps.delete_parameter(p).unwrap();
    with_lang(&german(), || {
        assert_eq!(
            pd.commit(p, &mut ps).unwrap_err(),
            "Den Parameter gibt es nicht mehr."
        );
    });
    with_lang(&english(), || {
        let seen = d.show(&mut |ctx| {
            pd.show(ctx, &mut ps, false);
        });
        check("Parameter", &seen, &["The parameter no longer exists."]);
    });

    // Netz-Import: abgebrochen, veraltet.
    let dir = temp_dir("stored");
    let path = write_stl(&dir, "w.stl", cube());
    let mut s = AppState::new();
    let mut imp = ImportDialog::new(dir.clone());
    with_lang(&german(), || {
        imp.load(path.clone());
        imp.cancel();
        assert_eq!(imp.error_text().as_deref(), Some("Abgebrochen"));
    });
    with_lang(&english(), || {
        let seen = import_frame(&mut d, &mut imp, &mut s);
        check("Import", &seen, &["Cancelled"]);
    });
    let mut imp = ImportDialog::new(dir.clone());
    imp.load_at(path.clone(), s.document.edit_count());
    imp.tick(true);
    s.create_box(DVec3::splat(1.0)).unwrap();
    with_lang(&german(), || {
        let seen = import_frame(&mut d, &mut imp, &mut s);
        assert!(
            seen.iter()
                .any(|t| t.text.starts_with("Das Dokument hat sich"))
        );
    });
    with_lang(&english(), || {
        let seen = import_frame(&mut d, &mut imp, &mut s);
        check(
            "Veraltet",
            &seen,
            &["The document has changed since the file was read. Please choose the file again."],
        );
    });

    // STEP-Import: abgebrochen, veraltet.
    let file = dir.join("box.step");
    fs::copy(step_data("box_ap214.step"), &file).unwrap();
    let mut sd = StepImportDialog::new(dir.clone());
    with_lang(&german(), || {
        sd.load(file.clone());
        sd.cancel();
        assert_eq!(sd.error_text().as_deref(), Some("Abgebrochen"));
    });
    with_lang(&english(), || {
        let seen = step_frame(&mut d, &mut sd, &mut s);
        check("STEP", &seen, &["Cancelled"]);
    });
    let mut sd = StepImportDialog::new(dir);
    sd.load_at(file, s.document.edit_count());
    sd.tick(true);
    s.create_box(DVec3::splat(1.0)).unwrap();
    with_lang(&german(), || {
        let seen = step_frame(&mut d, &mut sd, &mut s);
        assert!(
            seen.iter()
                .any(|t| t.text.starts_with("Das Dokument hat sich"))
        );
    });
    with_lang(&english(), || {
        let seen = step_frame(&mut d, &mut sd, &mut s);
        check(
            "STEP veraltet",
            &seen,
            &["The document has changed since the file was read. Please choose the file again."],
        );
    });
}

#[test]
fn table_texts_of_the_step_import_follow_the_language_when_drawn() {
    // Die Größe und die Einheit je Zeile sind Werte; der Text entsteht beim Zeichnen.
    let dir = temp_dir("table");
    let file = dir.join("box.step");
    fs::copy(step_data("box_ap214.step"), &file).unwrap();
    let mut dialog = StepImportDialog::new(dir);
    dialog.load(file);
    dialog.tick(true);
    let l = dialog.loaded.as_ref().unwrap();
    let de = with_lang(&german(), || (l.sizes[0].text(), l.row_units[0].text()));
    let en = with_lang(&english(), || (l.sizes[0].text(), l.row_units[0].text()));
    assert_eq!(de.1, "mm");
    assert_eq!(en.1, "mm");
    assert_eq!(de.0.replace(',', "."), en.0);
}
