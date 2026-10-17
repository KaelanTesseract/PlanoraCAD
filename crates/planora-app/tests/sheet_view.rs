//! Zeichnungsblatt in der Zeichnungsansicht (Teilprojekt 3c-1, Aufgabe 5): Rahmen und Schriftfeld
//! auf weißem Papier, Stempel beim Anlegen, automatische Werte je Blatt und neue Ansichten im
//! freien Bereich des Blatts – mit echten egui-Bildern wie `tests/drawing_ui.rs`.
//!
//! Der freie Bereich folgt `sheet_layout`: A4 quer mit 3 Hinweiszeilen x 20…287, y 88…193
//! (267 × 105); A4 hoch 180 × 192; mit 6 Änderungen und 6 Hinweiszeilen A4 hoch 180 × 132.

use egui::epaint::Shape;
use egui::{Color32, Event, Pos2, Rect, vec2};
use planora_app::drawing_dialog::NewDrawingDialog;
use planora_app::drawing_state::{WorkArea, estimate_view_size, suggest_scale_in};
use planora_app::drawing_view;
use planora_app::state::*;
use planora_app::testing::collect_texts;
use planora_app::{fonts, theme};
use planora_drawing::sheet_frame::sheet_layout;
use planora_drawing::sheet_info::{FieldValue, Stamp};
use planora_drawing::{DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewKind};
use planora_math::{DVec2, DVec3};

const A4_QUER: Sheet = Sheet {
    paper: Paper::A4,
    landscape: true,
};

fn s(num: u32, den: u32) -> Scale {
    Scale { num, den }
}

struct Shot {
    text: String,
    flat: Vec<Shape>,
}

impl Shot {
    fn has_line(&self, exact: &str) -> bool {
        self.text.lines().any(|l| l == exact)
    }

    fn segments(&self) -> Vec<([Pos2; 2], Color32, f32)> {
        self.flat
            .iter()
            .filter_map(|s| match s {
                Shape::LineSegment { points, stroke } => {
                    Some((*points, stroke.color, stroke.width))
                }
                _ => None,
            })
            .collect()
    }

    fn rect_fills(&self) -> Vec<(Rect, Color32)> {
        self.flat
            .iter()
            .filter_map(|s| match s {
                Shape::Rect(r) => Some((r.rect, r.fill)),
                _ => None,
            })
            .collect()
    }
}

fn flatten(shape: &Shape, flat: &mut Vec<Shape>) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|s| flatten(s, flat)),
        s => flat.push(s.clone()),
    }
}

struct Harness {
    ctx: egui::Context,
    size: egui::Vec2,
    time: f64,
    dark: Option<bool>,
}

impl Harness {
    fn new(w: f32, h: f32) -> Harness {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        Harness {
            ctx,
            size: vec2(w, h),
            time: 0.0,
            dark: None,
        }
    }

    fn idle(&mut self, st: &mut AppState) -> Shot {
        if self.dark != Some(st.dark) {
            theme::apply(&self.ctx, st.dark);
            self.dark = Some(st.dark);
        }
        self.time += 0.05;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            events: vec![Event::ModifiersChanged(egui::Modifiers::NONE)],
            time: Some(self.time),
            ..Default::default()
        };
        let out = self.ctx.run_ui(raw, |ui| drawing_view::show(ui, st));
        let mut flat = Vec::new();
        for c in &out.shapes {
            flatten(&c.shape, &mut flat);
        }
        let shot = Shot {
            text: collect_texts(&out),
            flat,
        };
        out.drop_without_applying_deltas();
        shot
    }

    fn screen(&self, p: DVec2) -> Pos2 {
        let l = drawing_view::last_layout(&self.ctx).expect("kein Zeichnungsbereich");
        drawing_view::sheet_to_screen(l.canvas, l.sheet_mm, l.zoom, l.pan, p)
    }

    fn k(&self) -> f64 {
        f64::from(
            drawing_view::last_layout(&self.ctx)
                .expect("kein Zeichnungsbereich")
                .px_per_mm,
        )
    }
}

fn box_state() -> AppState {
    let mut st = AppState::new();
    st.create_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
    st
}

fn stamp() -> Stamp {
    Stamp {
        date: "2026-10-17".into(),
        user: "dennis".into(),
    }
}

fn stamped(st: &mut AppState, name: &str, sheet: Sheet) -> DrawingId {
    st.document
        .add_drawing_stamped(name, sheet, Projection::First, &stamp())
        .unwrap()
}

fn close(a: Pos2, b: Pos2) -> bool {
    (a - b).length() < 0.01
}

// ---------------------------------------------------------------------------------------------
// (a)

#[test]
fn frame_is_shown_at_800x600_and_large() {
    for dark in [false, true] {
        for (w, h) in [(800.0, 600.0), (1920.0, 1080.0)] {
            for (paper, label) in [(Paper::A4, "A4"), (Paper::A0, "A0")] {
                let sheet = Sheet {
                    paper,
                    landscape: true,
                };
                let mut st = box_state();
                st.dark = dark;
                let d = stamped(&mut st, "Blatt 1", sheet);
                st.document
                    .add_view(
                        d,
                        ViewKind::Front,
                        s(1, 20),
                        Placement::Free { x: 400.0, y: 300.0 },
                        None,
                    )
                    .unwrap();
                st.drawing_ui.open(d);
                st.drawing_ui.tick(&st.bodies, &st.document, true);
                let mut hn = Harness::new(w, h);
                hn.idle(&mut st);
                let shot = hn.idle(&mut st);
                let info = format!(
                    "{label} {w}x{h} dunkel {dark} k {}:
{}",
                    hn.k(),
                    shot.text
                );
                for want in ["Fertigungszeichnung", "1 von 1", label, "1", "A"] {
                    assert!(shot.has_line(want), "„{want}“ fehlt in {info}");
                }
                // Papier: weiß in beiden Themes, in der Größe des Blatts.
                let (sw, sh) = sheet.size_mm();
                let paper_rect =
                    Rect::from_two_pos(hn.screen(DVec2::ZERO), hn.screen(DVec2::new(sw, sh)));
                assert!(
                    shot.rect_fills().iter().any(|(r, c)| *c == theme::PAPER
                        && (r.min - paper_rect.min).length() < 0.5
                        && (r.max - paper_rect.max).length() < 0.5),
                    "Papier fehlt in {info}"
                );
                // Rahmenlinie links (Breite 0,7 mm, mindestens 1 px) in der Tintenfarbe.
                let a = hn.screen(DVec2::new(20.0, 10.0));
                let b = hn.screen(DVec2::new(20.0, sh - 10.0));
                let want = (0.7 * hn.k()).max(1.0) as f32;
                assert!(
                    shot.segments()
                        .iter()
                        .any(|(p, c, wd)| *c == theme::SHEET_INK
                            && (*wd - want).abs() < 1e-3
                            && ((close(p[0], a) && close(p[1], b))
                                || (close(p[0], b) && close(p[1], a)))),
                    "Rahmenlinie fehlt in {info}"
                );
                // Zoom ohne Absturz.
                for z in [0.1, 40.0] {
                    st.drawing_ui.zoom = z;
                    hn.idle(&mut st);
                    hn.idle(&mut st);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// (b)

#[test]
fn new_drawing_is_stamped_and_centred() {
    let mut st = box_state();
    let dialog = NewDrawingDialog::new(&st);
    let id = dialog.create(&mut st).unwrap();
    let today = Stamp::now();
    let dr = st.document.drawing(id).unwrap();
    let info = dr.info.as_ref().expect("Blattangaben");
    assert_eq!(info.created, today.date);
    let project = st.document.project_info().expect("Projektangaben");
    assert_eq!(project.login, today.user);
    assert_eq!(project.creator, FieldValue::Auto);
    // Grundansicht in der Mitte des freien Bereichs (20…287 × 88…193).
    assert_eq!(dr.sheet, A4_QUER);
    assert_eq!(dr.views.len(), 1);
    assert_eq!(
        dr.views[0].placement,
        Placement::Free { x: 153.5, y: 140.5 }
    );
    assert_eq!(st.drawing_ui.area, WorkArea::Drawing(id));
    // Mit Blattangaben braucht die Datei das Format 6.
    assert_eq!(planora_project::required_format(&st.document.snapshot()), 6);
}

// ---------------------------------------------------------------------------------------------
// (c)

#[test]
fn auto_values_follow_the_document() {
    let mut st = box_state();
    st.document_name = "Werkstück".into();
    let first = stamped(&mut st, "Blatt 1", A4_QUER);
    let second = stamped(&mut st, "Blatt 2", A4_QUER);
    let mut h = Harness::new(1600.0, 1000.0);
    st.drawing_ui.open(first);
    h.idle(&mut st);
    let shot = h.idle(&mut st);
    assert!(shot.has_line("1 von 2"), "{}", shot.text);
    assert!(shot.has_line("Werkstück-01"), "{}", shot.text);
    st.drawing_ui.open(second);
    h.idle(&mut st);
    let shot = h.idle(&mut st);
    assert!(shot.has_line("2 von 2"), "{}", shot.text);
    assert!(shot.has_line("Werkstück-02"), "{}", shot.text);

    // Die erste löschen: die übrige ist Blatt 1 von 1.
    st.document.delete_drawing(first).unwrap();
    h.idle(&mut st);
    let shot = h.idle(&mut st);
    assert!(shot.has_line("1 von 1"), "{}", shot.text);
    assert!(shot.has_line("Werkstück-01"), "{}", shot.text);
    assert!(!shot.has_line("2 von 2") && !shot.has_line("Werkstück-02"));

    // Anderer Name des Dokuments: ohne weitere Änderung am Blatt.
    st.document_name = "Neu".into();
    h.idle(&mut st);
    let shot = h.idle(&mut st);
    assert!(shot.has_line("Neu-01"), "{}", shot.text);
    assert!(!shot.has_line("Werkstück-01"));
}

// ---------------------------------------------------------------------------------------------
// (d)

#[test]
fn views_are_added_in_the_free_area() {
    let hoch = Sheet {
        paper: Paper::A4,
        landscape: false,
    };
    let a3 = Sheet {
        paper: Paper::A3,
        landscape: true,
    };
    for sheet in [hoch, A4_QUER, a3] {
        // Zwei Zeichnungen je Blatt: Grundansicht allein (Mitte); Isometrie (rechts oben) und
        // danach eine Normansicht ohne Grundansicht (links oben). Ein Teil von 40 × 30 × 20:
        // füllt ein Teil das Blatt fast ganz, ragt die Isometrie bei 0,75 über den Rand (der
        // Maßstab richtet sich nach der Grundansicht).
        let mut st = AppState::new();
        st.create_box(DVec3::new(40.0, 30.0, 20.0)).unwrap();
        let one = stamped(&mut st, "Blatt 1", sheet);
        let two = stamped(&mut st, "Blatt 2", sheet);
        let (lo, hi) = {
            let d = st.document.drawing(one).unwrap();
            drawing_view::layout_of(&st, d).free
        };
        st.drawing_ui.open(one);
        let front = drawing_view::add_view(&mut st, ViewKind::Front).unwrap();
        st.drawing_ui.open(two);
        let iso = drawing_view::add_view(&mut st, ViewKind::Isometric).unwrap();
        let top = drawing_view::add_view(&mut st, ViewKind::Top).unwrap();
        for (drawing, v, fx, fy) in [
            (one, front, 0.5, 0.5),
            (two, iso, 0.75, 0.75),
            (two, top, 0.25, 0.75),
        ] {
            let spec = st.document.drawing(drawing).unwrap().view(v).unwrap();
            let Placement::Free { x, y } = spec.placement else {
                panic!("{:?} sollte frei liegen", spec.kind);
            };
            let want = (lo.x + fx * (hi.x - lo.x), lo.y + fy * (hi.y - lo.y));
            assert!(
                (x - want.0).abs() < 1e-9 && (y - want.1).abs() < 1e-9,
                "{:?} auf {sheet:?}: ({x}, {y}) statt {want:?}",
                spec.kind
            );
            // Mit der geschätzten Ausdehnung im freien Bereich. Die Isometrie sitzt fest bei
            // 0,75 und kann bei einem Teil, das den freien Bereich ausfüllt, über den Rand
            // ragen; ihre Mitte liegt aber immer darin.
            let (ew, eh) = estimate_view_size(&st.bodies, spec.kind);
            let f = spec.scale.factor();
            let (hw, hh) = if spec.kind == ViewKind::Isometric {
                (0.0, 0.0)
            } else {
                (ew * f / 2.0, eh * f / 2.0)
            };
            assert!(
                x - hw >= lo.x - 1e-9
                    && x + hw <= hi.x + 1e-9
                    && y - hh >= lo.y - 1e-9
                    && y + hh <= hi.y + 1e-9,
                "{:?} auf {sheet:?} ragt aus dem freien Bereich: ({x}, {y}) ± ({hw}, {hh}) in {lo:?}…{hi:?}",
                spec.kind
            );
        }
    }

    // Maßstab im freien Bereich: A4 quer, Quader 100 × 60 × 40: 1:2 (freier Bereich 267 × 105).
    let st = box_state();
    let (lo, hi) = sheet_layout(A4_QUER, 0, 3).free;
    assert_eq!((hi.x - lo.x, hi.y - lo.y), (267.0, 105.0));
    assert_eq!(
        suggest_scale_in(&st.bodies, (267.0, 105.0), ViewKind::Front),
        s(1, 2)
    );
    // Sechs Änderungen und sechs Hinweiszeilen auf A4 hoch: weiter eine Stufe der Reihe.
    let (lo, hi) = sheet_layout(hoch, 6, 6).free;
    let scale = suggest_scale_in(&st.bodies, (hi.x - lo.x, hi.y - lo.y), ViewKind::Front);
    assert!(Scale::SERIES.contains(&scale), "{scale:?}");
}

// ---------------------------------------------------------------------------------------------
// (e)

#[test]
fn old_drawing_shows_defaults_without_changes() {
    let mut st = box_state();
    let id = st
        .document
        .add_drawing("Blatt 1", A4_QUER, Projection::First)
        .unwrap();
    st.drawing_ui.open(id);
    let edits = st.document.edit_count();
    let mut h = Harness::new(1600.0, 1000.0);
    let mut shot = h.idle(&mut st);
    for _ in 0..10 {
        shot = h.idle(&mut st);
    }
    assert!(shot.has_line("–"), "{}", shot.text);
    assert!(shot.has_line("1 von 1"), "{}", shot.text);
    assert_eq!(st.document.edit_count(), edits);
    assert!(st.document.drawing(id).unwrap().info.is_none());
    assert!(st.document.project_info().is_none());
}
