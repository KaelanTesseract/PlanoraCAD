//! Zeichnungsblatt in der Zeichnungsansicht (Teilprojekt 3c-1, Aufgabe 5): Rahmen und Schriftfeld
//! auf weißem Papier, Stempel beim Anlegen, automatische Werte je Blatt und neue Ansichten im
//! freien Bereich des Blatts – mit echten egui-Bildern wie `tests/drawing_ui.rs`.
//!
//! Der freie Bereich folgt `sheet_layout`: A4 quer mit 3 Hinweiszeilen x 20…287, y 88…193
//! (267 × 105); A4 hoch 180 × 192; mit 6 Änderungen und 6 Hinweiszeilen A4 hoch 180 × 132.

use egui::epaint::Shape;
use egui::{Color32, Event, Pos2, Rect, vec2};
use planora_app::drawing_dialog::NewDrawingDialog;
use planora_app::drawing_state::{
    WorkArea, block_origin, estimate_view_size, keep_in_free, suggest_scale_in,
};
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

    /// Gibt es einen Text `exact`, dessen Mitte höchstens `tol` Pixel von `p` entfernt liegt?
    fn text_near(&self, exact: &str, p: Pos2, tol: f32) -> bool {
        self.flat.iter().any(|s| match s {
            Shape::Text(t) => {
                t.galley.text() == exact
                    && (Rect::from_min_size(t.pos, t.galley.size()).center() - p).length() <= tol
            }
            _ => false,
        })
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
                for want in ["Fertigungszeichnung", "1 von 1", label] {
                    assert!(shot.has_line(want), "„{want}“ fehlt in {info}");
                }
                // Rasterbeschriftung (1, 2, … und A, B, …) an ihren Stellen am Rand.
                let layout = drawing_view::layout_of(&st, st.document.drawing(d).unwrap());
                assert!(layout.grid_labels.iter().any(|(_, t)| t == "1"));
                assert!(layout.grid_labels.iter().any(|(_, t)| t == "A"));
                let tol = (4.0 * hn.k() + 2.0) as f32;
                for (c, text) in &layout.grid_labels {
                    assert!(
                        shot.text_near(text, hn.screen(*c), tol),
                        "Rasterzeichen „{text}“ bei {c:?} fehlt in {info}"
                    );
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
    // Der Block aus drei Ansichten liegt mittig im freien Bereich (20…287 × 88…193, Mitte
    // 153,5 | 140,5); 100 × 60 × 40 gibt 1:2, die Grundansicht sitzt um (20 + 60·0,5)/2 = 25 mm
    // links und oberhalb der Mitte (Erstwinkel).
    assert_eq!(dr.sheet, A4_QUER);
    assert_eq!(dr.views.len(), 1);
    assert_eq!(dr.views[0].scale, s(1, 2));
    assert_eq!(
        dr.views[0].placement,
        Placement::Free { x: 128.5, y: 165.5 }
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
        // danach eine Normansicht ohne Grundansicht (links oben). Ein Teil von 40 × 30 × 20; das
        // Verhalten bei einem Teil, das das Blatt fast füllt, prüft der Test unten.
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
            let (ew, eh) = estimate_view_size(&st.bodies, spec.kind);
            let f = spec.scale.factor();
            let free = ((lo.x, lo.y), (hi.x, hi.y));
            let want = if spec.kind == ViewKind::Front {
                // Block aus drei Ansichten mittig.
                block_origin(&st.bodies, free, Projection::First, spec.kind, spec.scale)
            } else {
                keep_in_free(
                    (lo.x + fx * (hi.x - lo.x), lo.y + fy * (hi.y - lo.y)),
                    (ew * f, eh * f),
                    free,
                )
            };
            assert!(
                (x - want.0).abs() < 1e-9 && (y - want.1).abs() < 1e-9,
                "{:?} auf {sheet:?}: ({x}, {y}) statt {want:?}",
                spec.kind
            );
            // Mit der ganzen geschätzten Ausdehnung im freien Bereich.
            let (hw, hh) = (ew * f / 2.0, eh * f / 2.0);
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

// ---------------------------------------------------------------------------------------------
// (d2) Block aus drei Ansichten und Isometrie im freien Bereich

type Span = ((f64, f64), (f64, f64));

/// Linke untere und rechte obere Ecke einer Ansicht der Art `kind` bei Mitte `c`.
fn span(st: &AppState, kind: ViewKind, scale: Scale, c: (f64, f64)) -> Span {
    let (w, h) = estimate_view_size(&st.bodies, kind);
    let f = scale.factor();
    (
        (c.0 - w * f / 2.0, c.1 - h * f / 2.0),
        (c.0 + w * f / 2.0, c.1 + h * f / 2.0),
    )
}

fn inside(a: Span, lo: DVec2, hi: DVec2) -> bool {
    let e = 1e-9;
    a.0.0 >= lo.x - e && a.0.1 >= lo.y - e && a.1.0 <= hi.x + e && a.1.1 <= hi.y + e
}

/// Legt eine Zeichnung über das Fenster an, fügt Seiten- und Draufsicht sowie die Isometrie
/// hinzu und gibt Block (Vorne, Links, Oben), Isometrie und freien Bereich zurück.
fn arrange(
    body: DVec3,
    paper: Paper,
    landscape: bool,
    projection: Projection,
) -> (Vec<Span>, Span, (DVec2, DVec2)) {
    let mut st = AppState::new();
    st.create_box(body).unwrap();
    let mut dialog = NewDrawingDialog::new(&st);
    dialog.paper = paper;
    dialog.landscape = landscape;
    dialog.projection = projection;
    dialog.scale = dialog.suggestion(&st);
    let id = dialog.create(&mut st).unwrap();
    let base = st.document.drawing(id).unwrap().views[0].clone();
    let Placement::Free { x, y } = base.placement else {
        panic!("frei");
    };
    st.drawing_ui.selected_view = Some(base.id);
    let mut block = vec![span(&st, ViewKind::Front, base.scale, (x, y))];
    for kind in [ViewKind::Left, ViewKind::Top] {
        let v = drawing_view::add_view(&mut st, kind).unwrap();
        let spec = st.document.drawing(id).unwrap().view(v).unwrap().clone();
        let Placement::Aligned { distance } = spec.placement else {
            panic!("abgeleitet");
        };
        let (dx, dy) = planora_drawing::align_dir(projection, ViewKind::Front, kind).unwrap();
        block.push(span(
            &st,
            kind,
            spec.scale,
            (x + dx * distance, y + dy * distance),
        ));
    }
    let iso = drawing_view::add_view(&mut st, ViewKind::Isometric).unwrap();
    let spec = st.document.drawing(id).unwrap().view(iso).unwrap().clone();
    let Placement::Free { x, y } = spec.placement else {
        panic!("frei");
    };
    let free = drawing_view::layout_of(&st, st.document.drawing(id).unwrap()).free;
    (
        block,
        span(&st, ViewKind::Isometric, spec.scale, (x, y)),
        free,
    )
}

#[test]
fn three_view_block_and_isometric_stay_in_the_free_area() {
    let probe = DVec3::new(100.0, 60.0, 40.0);
    let cases = [
        (probe, Paper::A4, true, Projection::First),
        (probe, Paper::A4, true, Projection::Third),
        (probe, Paper::A4, false, Projection::First),
        (probe, Paper::A0, true, Projection::First),
        (
            DVec3::new(40.0, 30.0, 20.0),
            Paper::A3,
            true,
            Projection::Third,
        ),
    ];
    for (body, paper, landscape, projection) in cases {
        let (block, iso, (lo, hi)) = arrange(body, paper, landscape, projection);
        for b in &block {
            assert!(
                inside(*b, lo, hi),
                "{paper:?} {landscape} {projection:?}: Block {b:?} ragt aus {lo:?}…{hi:?}"
            );
        }
        assert!(
            inside(iso, lo, hi),
            "{paper:?} {landscape} {projection:?}: Isometrie {iso:?} ragt aus {lo:?}…{hi:?}"
        );
    }

    // Ein Teil, das das Blatt fast ausfüllt: der Block passt (Maßstabsvorschlag), die Isometrie
    // nur nach bestem Wissen – was nicht hineinpasst, sitzt bündig an der rechten bzw. oberen
    // Kante des freien Bereichs.
    let (block, iso, (lo, hi)) = arrange(
        DVec3::new(150.0, 100.0, 80.0),
        Paper::A4,
        true,
        Projection::First,
    );
    for b in &block {
        assert!(inside(*b, lo, hi), "Block {b:?} ragt aus {lo:?}…{hi:?}");
    }
    let x_ok = iso.0.0 >= lo.x - 1e-9 && iso.1.0 <= hi.x + 1e-9;
    let y_ok = iso.0.1 >= lo.y - 1e-9 && iso.1.1 <= hi.y + 1e-9;
    assert!(x_ok || (iso.1.0 - hi.x).abs() < 1e-9, "{iso:?}");
    assert!(y_ok || (iso.1.1 - hi.y).abs() < 1e-9, "{iso:?}");
}

// ---------------------------------------------------------------------------------------------
// Anlegen scheitert; Maßstabsvorschlag mit den Hinweiszeilen der Projektangaben

#[test]
fn failed_creation_rolls_back_the_project_info_it_created() {
    let mut st = box_state();
    let mut dialog = NewDrawingDialog::new(&st);
    dialog.scale = Scale { num: 0, den: 1 };
    assert!(dialog.create(&mut st).is_err());
    assert!(st.document.drawings().is_empty());
    assert!(
        st.document.project_info().is_none(),
        "Projektangaben blieben"
    );

    // Gab es die Projektangaben schon, bleiben sie.
    let id = stamped(&mut st, "Blatt 1", A4_QUER);
    st.document.delete_drawing(id).unwrap();
    let before = st.document.project_info().cloned();
    assert!(before.is_some());
    let mut dialog = NewDrawingDialog::new(&st);
    dialog.scale = Scale { num: 0, den: 1 };
    assert!(dialog.create(&mut st).is_err());
    assert_eq!(st.document.project_info().cloned(), before);
    assert!(st.document.drawings().is_empty());
}

#[test]
fn suggestion_uses_the_note_lines_of_the_project() {
    // Quader 100 × 100 × 50: Höhe 150·f + 20 ≤ 105 (drei Zeilen) → 1:2; ≤ 87 (sechs Zeilen,
    // freier Bereich 267 × 87) → 1:5.
    let mut st = AppState::new();
    st.create_box(DVec3::new(100.0, 100.0, 50.0)).unwrap();
    let dialog = NewDrawingDialog::new(&st);
    assert_eq!(dialog.suggestion(&st), s(1, 2));
    let mut project = planora_drawing::sheet_info::ProjectInfo::new(&stamp());
    project.notes = planora_drawing::sheet_info::NotesValue::Lines(
        (1..=6).map(|i| format!("Hinweis {i}")).collect(),
    );
    st.document.set_project_info(project).unwrap();
    let (lo, hi) = sheet_layout(A4_QUER, 0, 6).free;
    let want = suggest_scale_in(&st.bodies, (hi.x - lo.x, hi.y - lo.y), ViewKind::Front);
    assert_eq!(dialog.suggestion(&st), want);
    assert_eq!(want, s(1, 5));
}
