//! Handprüfung Haltepunkt 2 (Graphit, Etappe B): Die Seitenfelder bleiben genau
//! `theme::PANEL_WIDTH` breit, auch wenn ihr Inhalt breit ist (gewählter Körper mit langen
//! Zahlen, gewähltes Feature, Fläche, Kante, Netz-Körper, langer Name). Kein Text ragt über den
//! rechten Rand seines Felds, und kein Feld deckt den Navigationswürfel ab. Geprüft bei
//! 1400 × 900, 1100 × 700 und 800 × 600, dunkel und hell.

mod common;

use std::sync::Arc;

use egui::epaint::{ClippedShape, Shape};
use egui::{Pos2, Rect, Vec2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::workspace::Workspace;
use planora_app::{fonts, shell_panels, theme, viewport};
use planora_exchange::ExchangeMesh;
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_render::{EdgeRef, FaceRef, Pick};

fn context(dark: bool) -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    theme::apply(&c, dark);
    c
}

thread_local! {
    static TIME: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

fn input(size: Vec2) -> egui::RawInput {
    let time = TIME.with(|t| {
        t.set(t.get() + 1.0 / 60.0);
        t.get()
    });
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
        time: Some(time),
        ..Default::default()
    }
}

/// Ein gezeichneter Text mit seinem Ausschnitt.
#[derive(Debug, Clone)]
struct Seen {
    text: String,
    rect: Rect,
    clip: Rect,
}

fn walk(shape: &Shape, clip: Rect, texts: &mut Vec<Seen>) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|s| walk(s, clip, texts)),
        Shape::Text(t) => texts.push(Seen {
            text: t.galley.text().to_string(),
            // Lage der Glyphen: bei rechtsbündigem Text liegt `pos` am rechten Rand.
            rect: t.galley.rect.translate(t.pos.to_vec2()),
            clip,
        }),
        _ => {}
    }
}

fn texts_of(shapes: &[ClippedShape]) -> Vec<Seen> {
    let mut texts = Vec::new();
    for c in shapes {
        walk(&c.shape, c.clip_rect, &mut texts);
    }
    texts
}

/// Einige Bilder ohne Eingaben; liefert die Texte des letzten.
fn settle(c: &egui::Context, app: &mut PlanoraApp, size: Vec2) -> Vec<Seen> {
    let mut texts = Vec::new();
    for _ in 0..4 {
        let out = c.run_ui(input(size), |ui| {
            let free = shell_panels::show_fields(ui.ctx(), app);
            viewport::show_in(ui, &mut app.ws_mut().active_tab_mut().state, Some(free));
        });
        texts = texts_of(&out.shapes);
        out.drop_without_applying_deltas();
    }
    texts
}

fn library_ws() -> Workspace {
    let root = common::temp_root("planora-width");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

/// Einheitswürfel, Dreiecke von außen gegen den Uhrzeigersinn.
fn cube_mesh() -> ExchangeMesh {
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

const LONG_NAME: &str =
    "Grundplatte mit Bohrungen für die Halterung links und rechts, zweite Fassung";

/// Die geprüften Zustände: Name und eine App in diesem Zustand.
fn cases(dark: bool) -> Vec<(&'static str, PlanoraApp)> {
    let with_box = |size: f64| {
        let mut app = PlanoraApp::for_tests(library_ws());
        app.ws_mut().set_dark(dark);
        app.ws_mut()
            .active_tab_mut()
            .state
            .create_box(DVec3::splat(size))
            .unwrap();
        app
    };
    let mut out = Vec::new();

    let mut a = with_box(40.0);
    a.ws_mut().active_tab_mut().state.selected_body = Some(0);
    out.push(("Körper gewählt", a));

    // Große Maße: lange Zahlen bei Volumen, Oberfläche, Schwerpunkt und Position.
    let mut a = with_box(12_345.678);
    {
        let s = &mut a.ws_mut().active_tab_mut().state;
        s.move_body(
            0,
            DVec3::new(-234_567_890.25, 345_678_901.5, -456_789_012.75),
        )
        .unwrap();
        s.selected_body = Some(0);
    }
    out.push(("großer Körper gewählt", a));

    let mut a = with_box(40.0);
    {
        let s = &mut a.ws_mut().active_tab_mut().state;
        s.selected_body = None;
        s.selected_feature = Some(s.document.features()[0].id);
    }
    out.push(("Feature gewählt", a));

    let mut a = with_box(40.0);
    {
        let s = &mut a.ws_mut().active_tab_mut().state;
        let face = s.bodies[0].solid.faces.iter().map(|(id, _)| id).next();
        s.set_selection(Some(Pick::Face(FaceRef {
            body: 0,
            face: face.unwrap(),
        })));
    }
    out.push(("Fläche gewählt", a));

    let mut a = with_box(40.0);
    {
        let s = &mut a.ws_mut().active_tab_mut().state;
        let edge = s.bodies[0].solid.edges.iter().map(|(id, _)| id).next();
        s.set_selection(Some(Pick::Edge(EdgeRef {
            body: 0,
            edge: edge.unwrap(),
        })));
    }
    out.push(("Kante gewählt", a));

    let mut a = PlanoraApp::for_tests(library_ws());
    a.ws_mut().set_dark(dark);
    {
        let s = &mut a.ws_mut().active_tab_mut().state;
        let mut open = cube_mesh();
        open.indices.truncate(33);
        s.import_mesh("Netz", Arc::new(open), 1.0).unwrap();
    }
    out.push(("Netz-Körper gewählt", a));

    let mut a = with_box(40.0);
    {
        let s = &mut a.ws_mut().active_tab_mut().state;
        let id = s.document.features()[0].id;
        s.rename_feature(id, LONG_NAME).unwrap();
        s.selected_body = None;
        s.selected_feature = Some(id);
    }
    out.push(("langer Feature-Name", a));

    let mut a = with_box(40.0);
    {
        let s = &mut a.ws_mut().active_tab_mut().state;
        let id = s.document.features()[0].id;
        s.rename_feature(id, LONG_NAME).unwrap();
        s.selected_body = Some(0);
    }
    out.push(("Körper mit langem Namen", a));
    out
}

const SIZES: [Vec2; 3] = [vec2(1400.0, 900.0), vec2(1100.0, 700.0), vec2(800.0, 600.0)];

fn overlaps(a: Rect, b: Rect) -> bool {
    let i = a.intersect(b);
    i.width() > 0.5 && i.height() > 0.5
}

#[test]
fn side_fields_keep_their_width_and_no_text_leaves_them() {
    for dark in [true, false] {
        for size in SIZES {
            for (what, mut app) in cases(dark) {
                let c = context(dark);
                let texts = settle(&c, &mut app, size);
                let at = format!("{what}, {size:?}, dunkel {dark}");
                let window = Rect::from_min_size(Pos2::ZERO, size);
                let layout = shell_panels::last_layout(&c).expect("Geometrie");
                let fields = shell_panels::field_rects(&c);
                let cube = viewport::cube_rect(layout.free);
                for (name, planned) in [("links", layout.left), ("rechts", layout.right)] {
                    assert!(
                        planned.width() <= theme::PANEL_WIDTH + 0.5,
                        "{at}: geplantes Feld {name} {planned:?}"
                    );
                    let real = fields
                        .iter()
                        .find(|(n, _)| *n == name)
                        .map(|(_, r)| *r)
                        .unwrap_or_else(|| panic!("{at}: Feld {name} fehlt in {fields:?}"));
                    assert!(
                        real.width() <= theme::PANEL_WIDTH + 0.5,
                        "{at}: Feld {name} ist {} breit: {real:?}",
                        real.width()
                    );
                    assert!(
                        window.expand(0.5).contains_rect(real),
                        "{at}: Feld {name} {real:?} ragt aus dem Fenster"
                    );
                    // Texte dieses Felds (sichtbar, links im Feld beginnend) enden vor seinem
                    // rechten Rand: nichts ist abgeschnitten. Ausgenommen sind Texte, die ein
                    // Element im Feld selbst bewusst kürzt (Textfeld, Name vor der Statusspalte
                    // im Baum): deren Ausschnitt endet deutlich vor dem Rand.
                    let inside: Vec<&Seen> = texts
                        .iter()
                        .filter(|t| {
                            t.clip.intersects(t.rect)
                                && t.rect.left() >= real.left() - 0.5
                                && t.rect.left() < real.right()
                                && t.rect.center().y > real.top()
                                && t.rect.center().y < real.bottom()
                        })
                        .collect();
                    // Keine zwei Texte dieses Felds (Ausschnitt im Feld) liegen übereinander, z. B.
                    // ein langer Wert über seiner Bezeichnung.
                    let own: Vec<&&Seen> = inside
                        .iter()
                        .filter(|t| {
                            t.clip.left() >= real.left() - 1.0
                                && t.clip.right() <= real.right() + 1.0
                        })
                        .collect();
                    for (i, a) in own.iter().enumerate() {
                        for b in &own[i + 1..] {
                            let va = a.rect.intersect(a.clip);
                            let vb = b.rect.intersect(b.clip);
                            assert!(
                                !overlaps(va, vb),
                                "{at}: „{}“ {:?} liegt über „{}“ {:?}",
                                a.text,
                                a.rect,
                                b.text,
                                b.rect
                            );
                        }
                    }
                    for t in inside {
                        let inner_clip = t.clip.right() <= real.right() - theme::SPACE_2;
                        assert!(
                            t.rect.right() <= real.right() + 0.5 || inner_clip,
                            "{at}: „{}“ ragt über den rechten Rand von {name} ({:?}, Feld {real:?})",
                            t.text,
                            t.rect
                        );
                    }
                }
                for (i, (name, r)) in fields.iter().enumerate() {
                    assert!(
                        !overlaps(*r, cube),
                        "{at}: Feld {name} {r:?} deckt den Würfel {cube:?} ab"
                    );
                    // Auch das obere Feld bleibt bei schmalen Fenstern neben den Spalten.
                    for (other, s) in &fields[i + 1..] {
                        assert!(
                            !overlaps(*r, *s),
                            "{at}: Feld {name} {r:?} überlappt {other} {s:?}"
                        );
                    }
                }
            }
        }
    }
}

/// Die Zahlen des gewählten Körpers stehen ganz im Feld (nicht gekürzt, nicht abgeschnitten).
#[test]
fn body_values_are_shown_in_full() {
    let c = context(true);
    let (_, mut app) = cases(true).swap_remove(1);
    let texts = settle(&c, &mut app, vec2(1100.0, 700.0));
    let right = shell_panels::last_layout(&c).unwrap().right;
    let real = shell_panels::field_rects(&c)
        .into_iter()
        .find(|(n, _)| *n == "rechts")
        .unwrap()
        .1;
    assert!(
        (real.left() - right.left()).abs() < 0.5 && (real.width() - right.width()).abs() < 0.5,
        "das Feld steht an seiner Stelle: {real:?} statt {right:?}"
    );
    let joined: String = texts
        .iter()
        .filter(|t| right.contains(t.rect.center()))
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    for word in [
        "Volumen",
        "Oberfläche",
        "Schwerpunkt",
        "Abmessungen",
        "Position",
    ] {
        assert!(joined.contains(word), "„{word}“ fehlt: {joined}");
    }
    assert!(!joined.contains('…'), "nichts gekürzt: {joined}");
    for t in texts.iter().filter(|t| right.contains(t.rect.center())) {
        assert!(
            t.clip.expand(0.5).contains_rect(t.rect),
            "„{}“ abgeschnitten: {:?} außerhalb {:?}",
            t.text,
            t.rect,
            t.clip
        );
    }
}
