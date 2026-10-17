//! Dialog-Rahmen im Graphit-Stil (Etappe D, Aufgabe 1): Größenklassen, mittige Lage, Rahmen,
//! Kopfzeile mit Schließen-Symbol, Vordergrund, fertig schon im ersten Bild, scrollen und im
//! Fenster bleiben, Verschieben, kein Fokus für den Kopf.

use egui::epaint::Shape;
use egui::{Color32, Context, Event, Id, Modifiers, PointerButton, Pos2, Rect, vec2};
use planora_app::dialog::{CLOSE_LABEL, Dialog, DialogSize, limits};
use planora_app::icons::{self, Icon};
use planora_app::shell::SHELL_INSET;
use planora_app::{fonts, theme};

const TITLE: &str = "Probedialog";

/// Was im Dialog steht und was er zurückmeldet.
struct Content {
    /// Höhe des Platzhalters zwischen Text und Knopf.
    gap: f32,
    clicked: bool,
    focus_field: bool,
    field: String,
    /// Kennung des Textfelds (gesetzt beim Zeichnen).
    field_id: Option<egui::Id>,
    /// Vom Inhalt belegte Fläche (gesetzt beim Zeichnen).
    inner: Option<Rect>,
}

impl Content {
    fn new(gap: f32) -> Self {
        Content {
            gap,
            clicked: false,
            focus_field: false,
            field: String::new(),
            field_id: None,
            inner: None,
        }
    }
}

struct Harness {
    ctx: Context,
    size: egui::Vec2,
    time: f64,
    id: Id,
    /// Dialog offen (für `.open`), Dialogart, fest verankert, mit Schließen-Symbol.
    open: bool,
    kind: DialogSize,
    fixed: bool,
    closable: bool,
    first: bool,
    /// Zeit je Bild; Klicks und Ziehen brauchen kurze Abstände.
    step: f64,
}

struct Frame {
    texts: Vec<(String, Pos2, egui::FontId, Color32)>,
    fills: Vec<(Rect, Color32, egui::Stroke, egui::CornerRadius)>,
    events: Vec<egui::output::OutputEvent>,
}

fn walk(shape: &Shape, frame: &mut Frame) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|s| walk(s, frame)),
        Shape::Text(t) => {
            let section = t.galley.job.sections.first();
            let font = section.map_or(egui::FontId::default(), |s| s.format.font_id.clone());
            let mut color = section.map_or(t.fallback_color, |s| s.format.color);
            if color == Color32::PLACEHOLDER {
                color = t.fallback_color;
            }
            frame.texts.push((
                t.galley.text().to_string(),
                t.pos + t.galley.size() / 2.0,
                font,
                color,
            ));
        }
        Shape::Rect(r) => frame
            .fills
            .push((r.rect, r.fill, r.stroke, r.corner_radius)),
        _ => {}
    }
}

impl Harness {
    fn new(dark: bool, w: f32, h: f32, kind: DialogSize) -> Self {
        let ctx = Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, dark);
        Harness {
            ctx,
            size: vec2(w, h),
            time: 0.0,
            id: Id::new("dialog_frame_test"),
            open: true,
            kind,
            fixed: false,
            closable: false,
            first: true,
            step: 1.0,
        }
    }

    fn content_rect(&self) -> Rect {
        Rect::from_min_size(Pos2::ZERO, self.size)
    }

    /// Ein Bild (mit eventuell verworfenen Durchgängen).
    fn run(&mut self, events: Vec<Event>, c: &mut Content) -> Frame {
        self.time += self.step;
        let input = egui::RawInput {
            screen_rect: Some(self.content_rect()),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let (id, kind, fixed, closable) = (self.id, self.kind, self.fixed, self.closable);
        let mut open = self.open;
        let first = self.first;
        let out = self.ctx.run_ui(input, |ui| {
            let mut d = Dialog::new(TITLE, kind).id(id);
            if fixed {
                d = d.fixed();
            }
            if closable {
                d = d.open(&mut open);
            }
            d.show(ui.ctx(), |ui| {
                ui.label("Inhalt");
                ui.allocate_space(vec2(1.0, c.gap));
                if ui.button("Ende").clicked() {
                    c.clicked = true;
                }
                if c.focus_field {
                    let r = theme::text_field(ui, egui::TextEdit::singleline(&mut c.field));
                    c.field_id = Some(r.id);
                    if first {
                        r.request_focus();
                    }
                }
                c.inner = Some(ui.min_rect());
            });
        });
        self.open = open;
        self.first = false;
        let mut frame = Frame {
            texts: Vec::new(),
            fills: Vec::new(),
            events: out.platform_output.events.clone(),
        };
        for s in &out.shapes {
            walk(&s.shape, &mut frame);
        }
        out.drop_without_applying_deltas();
        frame
    }

    fn area(&self) -> Option<Rect> {
        self.ctx.memory(|m| m.area_rect(self.id))
    }

    fn click(&mut self, p: Pos2, c: &mut Content) -> Frame {
        let button = |pressed| Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.step = 0.05;
        self.run(vec![Event::PointerMoved(p)], c);
        self.run(vec![button(true)], c);
        let f = self.run(vec![button(false)], c);
        self.step = 1.0;
        f
    }
}

fn pos_of(frame: &Frame, text: &str) -> Pos2 {
    frame
        .texts
        .iter()
        .find(|t| t.0 == text)
        .unwrap_or_else(|| panic!("„{text}“ fehlt in {:?}", names(frame)))
        .1
}

fn names(frame: &Frame) -> Vec<&str> {
    frame.texts.iter().map(|t| t.0.as_str()).collect()
}

fn has(frame: &Frame, text: &str) -> bool {
    frame.texts.iter().any(|t| t.0 == text)
}

fn close_glyph() -> String {
    icons::glyph(Icon::Close).to_string()
}

fn max_abs(v: egui::Vec2) -> f32 {
    v.x.abs().max(v.y.abs())
}

#[test]
fn size_classes() {
    assert_eq!(DialogSize::Prompt.width(), 440.0);
    assert_eq!(DialogSize::Normal.width(), 480.0);
    assert_eq!(DialogSize::Wide.width(), 720.0);
    let small = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
    let big = Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1080.0));
    for (size, w) in [
        (DialogSize::Prompt, 440.0),
        (DialogSize::Normal, 480.0),
        (DialogSize::Wide, 720.0),
    ] {
        assert_eq!(limits(small, size), (w, 568.0), "{size:?} bei 800 × 600");
        assert_eq!(limits(big, size), (w, 1048.0), "{size:?} bei 1920 × 1080");
    }
    // Ein schmales Fenster begrenzt auch die Breite.
    let narrow = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 600.0));
    assert_eq!(
        limits(narrow, DialogSize::Wide).0,
        400.0 - 2.0 * SHELL_INSET
    );
}

#[test]
fn dialog_is_centred_with_the_class_width() {
    for dark in [false, true] {
        for (w, h) in [(800.0, 600.0), (1400.0, 900.0)] {
            for kind in [DialogSize::Prompt, DialogSize::Normal, DialogSize::Wide] {
                let mut hn = Harness::new(dark, w, h, kind);
                let mut c = Content::new(40.0);
                hn.run(vec![], &mut c);
                hn.run(vec![], &mut c);
                let a = hn.area().expect("Dialog offen");
                let want = kind.width().min(w - 2.0 * SHELL_INSET);
                assert!(
                    (a.width() - want).abs() <= 1.0,
                    "{kind:?} {w}x{h}: Breite {} statt {want}",
                    a.width()
                );
                let centre = hn.content_rect().center();
                assert!(
                    max_abs(a.center() - centre) <= 1.0,
                    "{kind:?} {w}x{h}: Mitte {:?} statt {centre:?}",
                    a.center()
                );
                assert!(
                    hn.content_rect().shrink(SHELL_INSET).contains_rect(a),
                    "{a:?} liegt nicht in der Fläche"
                );
            }
        }
    }
}

#[test]
fn panel_frame_heading_and_close_icon() {
    for dark in [false, true] {
        let p = theme::palette(dark);
        let mut hn = Harness::new(dark, 1400.0, 900.0, DialogSize::Normal);
        hn.closable = true;
        let mut c = Content::new(40.0);
        hn.run(vec![], &mut c);
        let f = hn.run(vec![], &mut c);
        let a = hn.area().unwrap();
        // Fläche, Rand und Rundung des Graphit-Rahmens
        assert!(
            f.fills.iter().any(|(r, fill, stroke, radius)| {
                *fill == p.bg_panel
                    && stroke.width == 1.0
                    && stroke.color == p.border_subtle
                    && *radius == egui::CornerRadius::same(theme::RADIUS_LG)
                    && max_abs(r.size() - a.size()) <= 2.0
            }),
            "Rahmen fehlt (dark={dark}): {:#?}",
            f.fills
        );
        // Titel
        let title = f.texts.iter().find(|t| t.0 == TITLE).expect("Titel");
        assert_eq!(title.2, theme::heading_font());
        assert_eq!(title.3, p.text_primary);
        assert!(title.1.x < a.center().x, "Titel links");
        // Schließen-Symbol rechts oben
        let g = pos_of(&f, &close_glyph());
        assert!(g.x > a.right() - 40.0 && g.x < a.right(), "{g:?} / {a:?}");
        assert!(g.y > a.top() && g.y < a.top() + 40.0, "{g:?} / {a:?}");
        // Klick: Meldung „Schließen“, open = false, im nächsten Bild weg
        let f = hn.click(g, &mut c);
        let hit = f.events.iter().any(|e| {
            matches!(e, egui::output::OutputEvent::Clicked(i)
                if i.label.as_deref() == Some(CLOSE_LABEL))
        });
        assert!(hit, "kein Klick „{CLOSE_LABEL}“ gemeldet: {:?}", f.events);
        assert!(!hn.open);
        let f = hn.run(vec![], &mut c);
        assert!(!has(&f, TITLE) && !has(&f, "Inhalt"), "Dialog noch da");
    }
    // Ohne `.open` kein Schließen-Symbol.
    let mut hn = Harness::new(false, 1400.0, 900.0, DialogSize::Normal);
    let mut c = Content::new(40.0);
    hn.run(vec![], &mut c);
    let f = hn.run(vec![], &mut c);
    assert!(has(&f, TITLE));
    assert!(
        !has(&f, &close_glyph()),
        "Symbol ohne .open: {:?}",
        names(&f)
    );
}

#[test]
fn order_is_foreground() {
    let mut hn = Harness::new(false, 1000.0, 700.0, DialogSize::Normal);
    let mut c = Content::new(40.0);
    hn.run(vec![], &mut c);
    hn.run(vec![], &mut c);
    let layer = egui::LayerId::new(egui::Order::Foreground, hn.id);
    let found = hn.ctx.memory(|m| m.layer_ids().any(|l| l == layer));
    assert!(found, "Dialog nicht in Order::Foreground");
}

#[test]
fn first_frame_is_final() {
    for fixed in [false, true] {
        let mut hn = Harness::new(true, 1000.0, 700.0, DialogSize::Normal);
        hn.fixed = fixed;
        let mut c = Content::new(40.0);
        let f = hn.run(vec![], &mut c);
        assert!(
            has(&f, "Inhalt") && has(&f, TITLE),
            "fixed={fixed}: nicht gemalt"
        );
        let first = hn.area().expect("Bereich");
        let centre = hn.content_rect().center();
        assert!(
            max_abs(first.center() - centre) <= 1.0,
            "fixed={fixed}: im ersten Bild nicht mittig: {first:?}"
        );
        hn.run(vec![], &mut c);
        let second = hn.area().unwrap();
        assert_eq!(
            first, second,
            "fixed={fixed}: Lage ändert sich im zweiten Bild"
        );
    }
}

/// Abschluss-Nachbesserung 1: ein kurzer Dialog ist so hoch wie sein Inhalt (Kopfzeile, Inhalt,
/// Rand), nicht fest 420 px hoch.
#[test]
fn short_content_hugs_the_dialog() {
    let edge = theme::SPACE_3 + 1.0; // Innenrand und Strich
    for (w, h) in [(800.0, 600.0), (1400.0, 900.0)] {
        for kind in [DialogSize::Prompt, DialogSize::Normal, DialogSize::Wide] {
            for fixed in [false, true] {
                let what = format!("{kind:?} {w}x{h} fixed={fixed}");
                let mut hn = Harness::new(false, w, h, kind);
                hn.fixed = fixed;
                hn.closable = true;
                let mut c = Content::new(40.0);
                hn.run(vec![], &mut c);
                let f = hn.run(vec![], &mut c);
                let a = hn.area().unwrap();
                let inner = c.inner.unwrap();
                assert!(
                    (a.bottom() - (inner.bottom() + edge)).abs() <= 2.0,
                    "{what}: Unterkante {} statt Inhalt {} + Rand",
                    a.bottom(),
                    inner.bottom()
                );
                // Über dem Inhalt nur Rand und Kopfzeile.
                let title = pos_of(&f, TITLE);
                assert!(
                    a.top() < title.y && title.y < inner.top(),
                    "{what}: Titel {title:?} nicht über dem Inhalt {inner:?}"
                );
                assert!(
                    inner.top() - a.top() < 70.0,
                    "{what}: Kopf zu hoch: {a:?} / {inner:?}"
                );
                assert!(a.height() < 200.0, "{what}: zu hoch: {a:?}");
                let centre = hn.content_rect().center();
                assert!(
                    max_abs(a.center() - centre) <= 1.0,
                    "{what}: nicht mittig: {a:?}"
                );
                // 100 px mehr Inhalt: 100 px höher.
                let before = a.height();
                c.gap = 140.0;
                hn.run(vec![], &mut c);
                hn.run(vec![], &mut c);
                let a = hn.area().unwrap();
                assert!(
                    (a.height() - before - 100.0).abs() <= 2.0,
                    "{what}: {before} -> {}",
                    a.height()
                );
                assert!(
                    max_abs(a.center() - centre) <= 1.0,
                    "{what}: nach dem Wachsen nicht mittig: {a:?}"
                );
            }
        }
    }
}

/// Abschluss-Nachbesserung 7: ein wieder geöffneter Dialog mit anderer Höhe steht schon im
/// ersten Bild mittig.
#[test]
fn reopened_dialog_is_centred_in_the_first_frame() {
    for fixed in [false, true] {
        let mut hn = Harness::new(false, 1000.0, 800.0, DialogSize::Prompt);
        hn.fixed = fixed;
        hn.closable = true;
        let mut c = Content::new(40.0);
        hn.run(vec![], &mut c);
        hn.run(vec![], &mut c);
        let small = hn.area().unwrap();
        hn.open = false;
        hn.run(vec![], &mut c);
        hn.run(vec![], &mut c);
        c.gap = 400.0;
        hn.open = true;
        let f = hn.run(vec![], &mut c);
        assert!(has(&f, TITLE), "fixed={fixed}: nicht gemalt");
        let a = hn.area().unwrap();
        assert!(
            a.height() > small.height() + 300.0,
            "fixed={fixed}: Höhe {} (vorher {})",
            a.height(),
            small.height()
        );
        let centre = hn.content_rect().center();
        assert!(
            max_abs(a.center() - centre) <= 1.0,
            "fixed={fixed}: im ersten Bild nicht mittig: {a:?}"
        );
        // Auch gemalt mittig: der Titel steht im Rahmen oben.
        let title = pos_of(&f, TITLE);
        assert!(
            title.y > a.top() && title.y < a.top() + 40.0,
            "fixed={fixed}: Titel {title:?} / {a:?}"
        );
    }
}

/// Nachbesserung (Re-Review 1): ändert sich die Höhe des Inhalts bei offenem Dialog, steht er
/// schon im ersten Bild danach mittig und im Fenster (kein Sprung über ein Bild), wachsend und
/// schrumpfend, fest und verschiebbar (unverschoben).
#[test]
fn content_height_change_is_centred_in_the_first_frame() {
    for fixed in [false, true] {
        for (from, to) in [(40.0, 300.0), (300.0, 40.0), (40.0, 1500.0), (1500.0, 40.0)] {
            let what = format!("fixed={fixed} {from}->{to}");
            let mut hn = Harness::new(false, 1400.0, 900.0, DialogSize::Normal);
            hn.fixed = fixed;
            hn.closable = true;
            let mut c = Content::new(from);
            hn.run(vec![], &mut c);
            hn.run(vec![], &mut c);
            c.gap = to;
            let first = hn.run(vec![], &mut c);
            let a = hn.area().unwrap();
            let settled = hn.run(vec![], &mut c);
            // Gemalt: Titel und Schließen-Symbol stehen schon im ersten Bild an ihrer Stelle.
            assert_eq!(
                pos_of(&first, TITLE),
                pos_of(&settled, TITLE),
                "{what}: Titel springt"
            );
            assert_eq!(
                pos_of(&first, &close_glyph()),
                pos_of(&settled, &close_glyph()),
                "{what}: Schließen-Symbol springt"
            );
            let title = pos_of(&first, TITLE);
            assert!(
                title.y > a.top() && title.y < a.top() + 40.0,
                "{what}: Titel {title:?} nicht oben in {a:?}"
            );
            let centre = hn.content_rect().center();
            assert!(
                max_abs(a.center() - centre) <= 1.0,
                "{what}: nicht mittig: {a:?}"
            );
            // Der gemalte Rahmen liegt im Fenster (Inhalt schneidet der Rollbereich ab).
            let panel = theme::palette(false).bg_panel;
            let frames: Vec<_> = first.fills.iter().filter(|f| f.1 == panel).collect();
            assert!(!frames.is_empty(), "{what}: kein Rahmen gemalt");
            for (r, ..) in frames {
                assert!(
                    hn.content_rect().expand(1.0).contains_rect(*r),
                    "{what}: Fläche {r:?} außerhalb des Fensters"
                );
            }
        }
    }
}

#[test]
fn tall_content_scrolls_and_stays_inside() {
    let mut hn = Harness::new(false, 800.0, 600.0, DialogSize::Normal);
    hn.closable = true;
    let mut c = Content::new(1500.0);
    hn.run(vec![], &mut c);
    let f = hn.run(vec![], &mut c);
    let a = hn.area().unwrap();
    // So hoch wie möglich (568 bei 800 × 600), nicht höher.
    assert!((a.height() - 568.0).abs() <= 1.0, "Höhe: {a:?}");
    let title_before = pos_of(&f, TITLE);
    let close_before = pos_of(&f, &close_glyph());
    assert!(
        hn.content_rect().contains_rect(a),
        "nicht im Fenster: {a:?}"
    );
    // Der Knopf am Ende ist erst nach dem Scrollen im Dialog.
    assert!(
        !has(&f, "Ende") || !a.contains(pos_of(&f, "Ende")),
        "Knopf schon sichtbar"
    );
    hn.run(vec![Event::PointerMoved(a.center())], &mut c);
    for _ in 0..4 {
        hn.run(
            vec![Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, -600.0),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::NONE,
            }],
            &mut c,
        );
    }
    let f = hn.run(vec![], &mut c);
    let p = pos_of(&f, "Ende");
    assert!(a.contains(p), "Knopf nicht im Dialog: {p:?} / {a:?}");
    // Die Kopfzeile scrollt nicht mit.
    assert_eq!(pos_of(&f, TITLE), title_before, "Titel weggescrollt");
    let g = pos_of(&f, &close_glyph());
    assert_eq!(g, close_before, "Schließen-Symbol weggescrollt");
    hn.click(p, &mut c);
    assert!(c.clicked, "Knopf am Ende nicht klickbar");
    // Das Schließen-Symbol bleibt nach dem Scrollen bedienbar.
    hn.click(g, &mut c);
    assert!(!hn.open, "Schließen nach dem Scrollen wirkungslos");

    // Wächst der Inhalt, bleibt die Mitte stehen.
    let mut hn = Harness::new(false, 1000.0, 800.0, DialogSize::Normal);
    let mut c = Content::new(100.0);
    hn.run(vec![], &mut c);
    hn.run(vec![], &mut c);
    let before = hn.area().unwrap().center();
    c.gap = 300.0;
    hn.run(vec![], &mut c);
    hn.run(vec![], &mut c);
    let after = hn.area().unwrap();
    assert!(
        max_abs(after.center() - before) <= 1.0,
        "Mitte wandert: {before:?} -> {:?}",
        after.center()
    );
}

/// Zieht an einer freien Stelle des Dialogs (linker Innenrand) um `delta`; liefert die Fläche danach.
fn drag(hn: &mut Harness, c: &mut Content, delta: egui::Vec2) -> Rect {
    let a = hn.area().unwrap();
    drag_from(hn, c, Pos2::new(a.left() + 6.0, a.center().y), delta)
}

/// Zieht ab `start` um `delta`; liefert die Fläche danach.
fn drag_from(hn: &mut Harness, c: &mut Content, start: Pos2, delta: egui::Vec2) -> Rect {
    let btn = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    hn.step = 0.05;
    hn.run(vec![Event::PointerMoved(start)], c);
    hn.run(vec![btn(start, true)], c);
    let mut p = start;
    for _ in 0..4 {
        p += delta / 4.0;
        hn.run(vec![Event::PointerMoved(p)], c);
    }
    hn.run(vec![btn(p, false)], c);
    hn.step = 1.0;
    hn.run(vec![], c);
    hn.area().unwrap()
}

#[test]
fn normal_dialog_moves_fixed_one_does_not() {
    let mut hn = Harness::new(false, 1000.0, 800.0, DialogSize::Normal);
    let mut c = Content::new(40.0);
    hn.run(vec![], &mut c);
    hn.run(vec![], &mut c);
    let start = hn.area().unwrap();
    let moved = drag(&mut hn, &mut c, vec2(60.0, 40.0));
    assert!(
        max_abs(moved.center() - start.center() - vec2(60.0, 40.0)) <= 2.0,
        "verschoben von {start:?} nach {moved:?}"
    );
    // Über den Rand hinaus: bleibt im Fenster.
    let moved = drag(&mut hn, &mut c, vec2(5000.0, 5000.0));
    assert!(
        hn.content_rect().expand(1.0).contains_rect(moved),
        "verlässt das Fenster: {moved:?}"
    );
    let moved = drag(&mut hn, &mut c, vec2(-9000.0, -9000.0));
    assert!(
        hn.content_rect().expand(1.0).contains_rect(moved),
        "verlässt das Fenster: {moved:?}"
    );

    let mut hn = Harness::new(false, 1000.0, 800.0, DialogSize::Prompt);
    hn.fixed = true;
    let mut c = Content::new(40.0);
    hn.run(vec![], &mut c);
    hn.run(vec![], &mut c);
    let start = hn.area().unwrap();
    let after = drag(&mut hn, &mut c, vec2(60.0, 40.0));
    assert_eq!(start, after, "fester Dialog bewegt sich");
}

/// Abschluss-Nachbesserung 3: Ziehen am Titeltext verschiebt einen normalen Dialog (der Titel
/// ist nicht markierbar); ein fester bleibt stehen.
#[test]
fn dragging_the_title_moves_a_normal_dialog() {
    for fixed in [false, true] {
        let mut hn = Harness::new(false, 1000.0, 800.0, DialogSize::Normal);
        hn.fixed = fixed;
        hn.closable = true;
        let mut c = Content::new(40.0);
        hn.run(vec![], &mut c);
        let f = hn.run(vec![], &mut c);
        let start = hn.area().unwrap();
        let title = pos_of(&f, TITLE);
        let moved = drag_from(&mut hn, &mut c, title, vec2(60.0, 40.0));
        let want = if fixed {
            egui::Vec2::ZERO
        } else {
            vec2(60.0, 40.0)
        };
        assert!(
            max_abs(moved.center() - start.center() - want) <= 2.0,
            "fixed={fixed}: von {start:?} nach {moved:?}"
        );
    }
}

#[test]
fn close_icon_does_not_take_focus_on_open() {
    let mut hn = Harness::new(false, 1000.0, 700.0, DialogSize::Normal);
    hn.closable = true;
    let mut c = Content::new(40.0);
    hn.run(vec![], &mut c);
    hn.run(vec![], &mut c);
    assert_eq!(hn.ctx.memory(|m| m.focused()), None, "Kopf hat den Fokus");

    let mut hn = Harness::new(false, 1000.0, 700.0, DialogSize::Normal);
    hn.closable = true;
    let mut c = Content::new(40.0);
    c.focus_field = true;
    hn.run(vec![], &mut c);
    hn.run(vec![], &mut c);
    let focused = hn.ctx.memory(|m| m.focused());
    assert!(focused.is_some(), "Textfeld hat den Fokus nicht");
    assert_eq!(
        focused, c.field_id,
        "den Fokus hat das Textfeld, nicht der Kopf"
    );
    // Weitere Bilder ändern daran nichts.
    hn.run(vec![], &mut c);
    assert_eq!(hn.ctx.memory(|m| m.focused()), focused);
}

/// Aufgabe 2: die alte Fensterhilfe für volle Breite ist ganz entfernt.
#[test]
fn wide_window_is_gone() {
    fn walk(dir: &std::path::Path, hits: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, hits);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let source = std::fs::read_to_string(&path).unwrap();
                for word in ["wide_window", "DIALOG_TOP", "DIALOG_MARGIN", "MIN_WIDTH"] {
                    // Als ganzes Wort (MIN_WIDTH_PX u. ä. zählt nicht).
                    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
                    let whole = source.match_indices(word).any(|(i, _)| {
                        !source[..i].chars().next_back().is_some_and(is_ident)
                            && !source[i + word.len()..]
                                .chars()
                                .next()
                                .is_some_and(is_ident)
                    });
                    if whole {
                        hits.push(format!("{word} in {}", path.display()));
                    }
                }
            }
        }
    }
    let mut hits = Vec::new();
    walk(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut hits,
    );
    assert!(hits.is_empty(), "{hits:?}");
}
