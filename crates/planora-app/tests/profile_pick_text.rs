//! Skizzen 8, Task 14 (Spec D2): Ganzer Text als Profil. Ein einfacher Klick auf ein Profil eines
//! Textes schaltet alle Profile dieses Textes gemeinsam um, Strg+Klick ein einzelnes; die Liste
//! hat je Text eine Zeile mit Zwischenzustand. Gilt in „Skizze extrudieren“, „Skizze drehen“ und
//! „Bearbeiten“. Ohne Fenster bei 1920 × 1080, Text aus einer eigenen Form (keine Schrift).

use egui::epaint::{ClippedShape, Shape};
use planora_app::create_dialog::CreateDialog;
use planora_app::feature_dialog::FeatureDialog;
use planora_app::profile_pick::{self, CheckState, ProfileGroup};
use planora_app::state::{AppState, StandardPlane};
use planora_app::{fonts, sketch_view, viewport};
use planora_feature::{FeatureId, SketchPlane};
use planora_math::DVec2;
use planora_render::ViewFace;
use planora_sketch::model::Sketch;
use planora_sketch::text::{HAlign, TextId, TextParams, VAlign};
use planora_sketch::text_shape::{Contour, Glyph, Seg, ShapedLine, TextShape, TextStyle};
use std::sync::Arc;

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const SCREEN: egui::Rect = egui::Rect {
    min: egui::pos2(0.0, 0.0),
    max: egui::pos2(W, H),
};
const NONE: egui::Modifiers = egui::Modifiers::NONE;
const CTRL: egui::Modifiers = egui::Modifiers {
    alt: false,
    ctrl: true,
    shift: false,
    mac_cmd: false,
    command: true,
};

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

/// Eingabe eines Bilds; die Tasten `modifiers` gelten ab Anfang des Bilds.
fn raw(events: Vec<egui::Event>, modifiers: egui::Modifiers) -> egui::RawInput {
    let mut all = vec![egui::Event::ModifiersChanged(modifiers)];
    all.extend(events);
    egui::RawInput {
        screen_rect: Some(SCREEN),
        events: all,
        ..Default::default()
    }
}

fn button(p: egui::Pos2, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    }
}

/// Rechteckige Kontur (x0, y0)–(x1, y1) in Einheiten der Höhe; `ccw` gegen den Uhrzeigersinn.
fn square(x0: f64, y0: f64, x1: f64, y1: f64, ccw: bool) -> Contour {
    let mut pts = vec![v(x1, y0), v(x1, y1), v(x0, y1), v(x0, y0)];
    if !ccw {
        pts = vec![v(x0, y1), v(x1, y1), v(x1, y0), v(x0, y0)];
    }
    Contour {
        start: v(x0, y0),
        segs: pts.into_iter().map(Seg::Line).collect(),
    }
}

/// Form eines Textes aus `glyphs` Zeichen nebeneinander (Vorschub 0,8): „o“ ist ein Quadrat
/// 0,6 × 0,7 mit Innenkontur 0,2 × 0,3 (Loch), jedes andere Zeichen ein volles Quadrat.
fn shape(content: &str) -> Arc<TextShape> {
    let glyphs: Vec<Glyph> = content
        .chars()
        .enumerate()
        .map(|(i, c)| {
            let mut contours = vec![square(0.0, 0.0, 0.6, 0.7, true)];
            if c == 'o' {
                contours.push(square(0.2, 0.2, 0.4, 0.5, false));
            }
            Glyph {
                origin: v(0.8 * i as f64, 0.0),
                advance: 0.8,
                contours,
            }
        })
        .collect();
    let width = 0.8 * glyphs.len() as f64;
    Arc::new(TextShape {
        lines: vec![ShapedLine { glyphs, width }],
        ascender: 0.9,
        descender: -0.2,
        line_height: 1.2,
        substituted: None,
        synthetic: false,
        missing_chars: false,
    })
}

/// Text `content` (Höhe 5) links unten im Rahmen ab `min`: Zeichen k liegt bei
/// x = min.x + 4k … min.x + 4k + 3, y = min.y + 1 … min.y + 4,5.
fn add_text(sk: &mut Sketch, min: DVec2, content: &str) -> TextId {
    let params = TextParams {
        content: content.into(),
        style: TextStyle {
            font: "Test".into(),
            bold: false,
            italic: false,
            spacing: 0.0,
        },
        height: 5.0,
        flip_h: false,
        flip_v: false,
    };
    sk.add_text_box(
        min,
        min + v(30.0, 8.0),
        params,
        HAlign::Left,
        VAlign::Bottom,
        shape(content),
    )
    .unwrap()
}

/// Mitte des Rings von „o“ bzw. eines vollen Zeichens an Stelle `k` eines Textes ab `min`.
fn ring_of(min: DVec2, k: usize) -> DVec2 {
    min + v(4.0 * k as f64 + 0.5, 2.75)
}
fn glyph_of(min: DVec2, k: usize) -> DVec2 {
    min + v(4.0 * k as f64 + 1.5, 2.75)
}

const T1: DVec2 = DVec2::new(-75.0, 0.0);
const T2: DVec2 = DVec2::new(-75.0, 12.0);
/// Mitte des einfachen Rechtecks (-40, 0)–(-30, 10).
const RECT: DVec2 = DVec2::new(-35.0, 5.0);

/// Skizze auf XY: Text „ol“ ab T1, Text „x“ ab T2, ein Rechteck; danach beendet. Mit
/// `axis` eine Hilfslinie als Drehachse unterhalb aller Profile.
fn scene(axis: bool) -> (AppState, FeatureId, Option<planora_sketch::model::CurveId>) {
    let mut s = AppState::new();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    let ax = {
        let sk = s.session_sketch_mut().unwrap().1;
        add_text(sk, T1, "ol");
        add_text(sk, T2, "x");
        sk.add_rectangle(v(-40.0, 0.0), v(-30.0, 10.0)).unwrap();
        let ax = axis.then(|| {
            let a = sk.add_point(-90.0, -5.0).unwrap();
            let b = sk.add_point(-20.0, -5.0).unwrap();
            let l = sk.add_line(a, b).unwrap();
            sk.set_construction(l, true).unwrap();
            l
        });
        sk.solve();
        ax
    };
    s.observe_sketch(false);
    s.close_sketch();
    (s, id, ax)
}

/// Profilnummern unter den Punkten der Skizze.
struct Idx {
    o: usize,
    l: usize,
    x: usize,
    rect: usize,
}

fn indices(s: &AppState, id: FeatureId) -> Idx {
    let ex = s.profile_cache.get(s.document.sketch_of(id).unwrap());
    let at = |p: DVec2| {
        ex.profile_at(p)
            .unwrap_or_else(|| panic!("Profil bei {p:?}"))
    };
    let idx = Idx {
        o: at(ring_of(T1, 0)),
        l: at(glyph_of(T1, 1)),
        x: at(glyph_of(T2, 0)),
        rect: at(RECT),
    };
    assert_eq!(ex.profiles.len(), 4, "o, l, x und das Rechteck");
    assert_eq!(ex.profiles[idx.o].holes.len(), 1, "„o“ hat ein Loch");
    assert_eq!(
        ex.profile_at(T1 + v(1.5, 2.75)),
        None,
        "das Loch ist kein Profil"
    );
    idx
}

fn sorted(mut v: Vec<usize>) -> Vec<usize> {
    v.sort_unstable();
    v
}

fn aim(s: &mut AppState) {
    s.camera.set_view(ViewFace::Top);
    s.camera.target = glam::Vec3::new(30.0, 5.0, 0.0);
    s.camera.distance = 160.0;
}

fn frame(
    ctx: &egui::Context,
    s: &mut AppState,
    dialog: &mut CreateDialog,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
) -> Vec<ClippedShape> {
    let out = ctx.run_ui(raw(events, modifiers), |ui| {
        viewport::show_in(ui, s, None);
        dialog.show(ui.ctx(), s);
    });
    let shapes = out.shapes.clone();
    out.drop_without_applying_deltas();
    shapes
}

fn screen(s: &AppState, id: FeatureId, p: DVec2) -> egui::Pos2 {
    let frame = *s.document.sketch_of(id).unwrap().frame();
    let v = sketch_view::sketch_to_screen(&s.camera, &frame, glam::Vec2::new(W, H), p).unwrap();
    egui::pos2(v.x, v.y)
}

/// Die Bilderfolge eines Klicks an `p` mit `m` gedrückt.
fn click_events(p: egui::Pos2, m: egui::Modifiers) -> [(Vec<egui::Event>, egui::Modifiers); 4] {
    [
        (vec![egui::Event::PointerMoved(p)], NONE),
        (vec![button(p, true, m)], m),
        (vec![button(p, false, m)], m),
        (vec![], NONE),
    ]
}

/// Klick (mit `m`) auf den Punkt `at` der Skizze; prüft, dass er frei vom Dialog liegt.
fn click_at(
    ctx: &egui::Context,
    s: &mut AppState,
    id: FeatureId,
    dialog: &mut CreateDialog,
    at: DVec2,
    m: egui::Modifiers,
) {
    let p = screen(s, id, at);
    assert!(SCREEN.contains(p), "{at:?} → {p:?}");
    let key = if matches!(dialog, CreateDialog::RevolveSketch { .. }) {
        "revolve"
    } else {
        "extrude"
    };
    if let Some(r) = ctx.memory(|mem| mem.area_rect(egui::Id::unique(("create_dialog", key)))) {
        assert!(!r.contains(p), "{at:?} liegt unter dem Dialog");
    }
    for (events, mods) in click_events(p, m) {
        frame(ctx, s, dialog, events, mods);
    }
}

fn picked(dialog: &CreateDialog) -> Vec<usize> {
    dialog.profile_choice().unwrap().1.to_vec()
}

/// Bereit: Szene, Dialog „Skizze extrudieren“ und zwei Bilder.
fn extrude_ready() -> (egui::Context, AppState, FeatureId, CreateDialog, Idx) {
    let ctx = context();
    let (mut s, id, _) = scene(false);
    aim(&mut s);
    let idx = indices(&s, id);
    let mut dialog = CreateDialog::extrude_sketch(&s, id);
    for _ in 0..2 {
        frame(&ctx, &mut s, &mut dialog, vec![], NONE);
    }
    (ctx, s, id, dialog, idx)
}

#[test]
fn click_text_profile_selects_all_of_text() {
    let (ctx, mut s, id, mut d, i) = extrude_ready();
    assert_eq!(picked(&d), Vec::<usize>::new());
    click_at(&ctx, &mut s, id, &mut d, ring_of(T1, 0), NONE);
    assert_eq!(picked(&d), sorted(vec![i.o, i.l]), "ganzer Text „ol“");
    assert_eq!(
        s.profile_pick.as_ref().map(|p| p.selected.clone()),
        Some(sorted(vec![i.o, i.l])),
        "die Ansicht zeigt beide als gewählt"
    );
}

#[test]
fn click_again_deselects_all() {
    let (ctx, mut s, id, mut d, i) = extrude_ready();
    click_at(&ctx, &mut s, id, &mut d, ring_of(T1, 0), NONE);
    assert_eq!(picked(&d), sorted(vec![i.o, i.l]));
    // Ein anderes Zeichen desselben Textes: alle gewählt, also alle ab.
    click_at(&ctx, &mut s, id, &mut d, glyph_of(T1, 1), NONE);
    assert_eq!(picked(&d), Vec::<usize>::new());
}

#[test]
fn second_text_adds() {
    let (ctx, mut s, id, mut d, i) = extrude_ready();
    click_at(&ctx, &mut s, id, &mut d, ring_of(T1, 0), NONE);
    click_at(&ctx, &mut s, id, &mut d, glyph_of(T2, 0), NONE);
    assert_eq!(picked(&d), sorted(vec![i.o, i.l, i.x]));
    // Das einfache Rechteck schaltet weiter einzeln.
    click_at(&ctx, &mut s, id, &mut d, RECT, NONE);
    assert_eq!(picked(&d), sorted(vec![i.o, i.l, i.x, i.rect]));
}

#[test]
fn ctrl_click_toggles_single() {
    let (ctx, mut s, id, mut d, i) = extrude_ready();
    click_at(&ctx, &mut s, id, &mut d, ring_of(T1, 0), CTRL);
    assert_eq!(picked(&d), vec![i.o], "Strg: nur „o“");
    // Ein Teil gewählt: ein einfacher Klick wählt alle.
    click_at(&ctx, &mut s, id, &mut d, ring_of(T1, 0), NONE);
    assert_eq!(picked(&d), sorted(vec![i.o, i.l]));
    click_at(&ctx, &mut s, id, &mut d, glyph_of(T1, 1), CTRL);
    assert_eq!(picked(&d), vec![i.o], "Strg: nur „l“ ab");
    click_at(&ctx, &mut s, id, &mut d, ring_of(T1, 0), CTRL);
    assert_eq!(picked(&d), Vec::<usize>::new(), "Strg: „o“ ab");
}

#[test]
fn ctrl_click_on_plain_profile_same_as_click() {
    let (ctx, mut s, id, mut d, i) = extrude_ready();
    click_at(&ctx, &mut s, id, &mut d, RECT, CTRL);
    assert_eq!(picked(&d), vec![i.rect]);
    click_at(&ctx, &mut s, id, &mut d, RECT, CTRL);
    assert_eq!(picked(&d), Vec::<usize>::new());
    // Umschalt hat keine eigene Bedeutung: wie ein einfacher Klick.
    let shift = egui::Modifiers::SHIFT;
    click_at(&ctx, &mut s, id, &mut d, ring_of(T1, 0), shift);
    assert_eq!(picked(&d), sorted(vec![i.o, i.l]));
}

/// Alle Texte der Formen mit der Mitte ihres Rechtecks.
fn text_positions(shapes: &[ClippedShape]) -> Vec<(String, egui::Rect)> {
    fn walk(shape: &Shape, out: &mut Vec<(String, egui::Rect)>) {
        match shape {
            Shape::Text(t) => out.push((t.galley.text().to_owned(), t.visual_bounding_rect())),
            Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    shapes.iter().for_each(|c| walk(&c.shape, &mut out));
    out
}

#[derive(Debug, PartialEq)]
enum Mark {
    Empty,
    Check,
    Dash,
}

/// Zeichen im Häkchen links neben der Zeile `row`: Haken (Linienzug), Strich (Zwischenzustand)
/// oder leer.
fn mark_left_of(shapes: &[ClippedShape], row: egui::Rect) -> Mark {
    fn walk(shape: &Shape, row: egui::Rect, out: &mut Vec<Mark>) {
        let near = |r: egui::Rect| {
            (r.center().y - row.center().y).abs() < 8.0
                && r.max.x <= row.min.x + 1.0
                && r.min.x > row.min.x - 30.0
        };
        match shape {
            Shape::Vec(v) => v.iter().for_each(|s| walk(s, row, out)),
            Shape::LineSegment { points, .. }
                if near(egui::Rect::from_two_pos(points[0], points[1])) =>
            {
                out.push(Mark::Dash)
            }
            Shape::Path(p) if p.points.len() == 3 && near(egui::Rect::from_points(&p.points)) => {
                out.push(Mark::Check)
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    shapes.iter().for_each(|c| walk(&c.shape, row, &mut out));
    assert!(out.len() <= 1, "{out:?}");
    out.pop().unwrap_or(Mark::Empty)
}

#[test]
fn list_has_one_row_per_text_with_partial_state() {
    let (ctx, mut s, id, mut d, i) = extrude_ready();
    // Gruppen der berechneten Skizze: je Text eine, das Rechteck einzeln.
    let sk = s.document.sketch_of(id).unwrap();
    let groups = s.profile_cache.groups(sk);
    assert_eq!(groups.len(), 3, "{groups:?}");
    let text_members = |g: &ProfileGroup| match g {
        ProfileGroup::Text { members, .. } => Some(members.clone()),
        ProfileGroup::Single(_) => None,
    };
    let mut member_sets: Vec<Vec<usize>> = groups.iter().filter_map(text_members).collect();
    member_sets.sort();
    let mut want = vec![sorted(vec![i.o, i.l]), vec![i.x]];
    want.sort();
    assert_eq!(member_sets, want);
    assert!(groups.contains(&ProfileGroup::Single(i.rect)));
    // Aus dem Speicher: dieselbe Liste ohne Neuberechnung.
    assert!(Arc::ptr_eq(&groups, &s.profile_cache.groups(sk)));

    // Strg+Klick auf „o“: Zeile „Text „ol““ im Zwischenzustand.
    click_at(&ctx, &mut s, id, &mut d, ring_of(T1, 0), CTRL);
    let shapes = frame(&ctx, &mut s, &mut d, vec![], NONE);
    let texts = text_positions(&shapes);
    let rows = |label: &str| texts.iter().filter(|(t, _)| t == label).count();
    assert_eq!(rows("Text „ol“"), 1, "{texts:?}");
    assert_eq!(rows("Text „x“"), 1, "{texts:?}");
    let profile_rows: Vec<&String> = texts
        .iter()
        .map(|(t, _)| t)
        .filter(|t| t.starts_with("Profil "))
        .collect();
    assert_eq!(
        profile_rows,
        vec![&format!("Profil {}: 100,00 mm²", i.rect + 1)],
        "Textprofile haben keine eigene Zeile"
    );
    let row = |label: &str| texts.iter().find(|(t, _)| t == label).unwrap().1;
    assert_eq!(mark_left_of(&shapes, row("Text „ol“")), Mark::Dash);
    assert_eq!(mark_left_of(&shapes, row("Text „x“")), Mark::Empty);
    assert_eq!(
        profile_pick::check_state(&[i.o, i.l], &picked(&d)),
        CheckState::Partial
    );

    // Klick auf die Zeile im Zwischenzustand: alle gewählt, Haken.
    let at = row("Text „ol“").center();
    for (events, mods) in click_events(at, NONE) {
        frame(&ctx, &mut s, &mut d, events, mods);
    }
    assert_eq!(picked(&d), sorted(vec![i.o, i.l]));
    let shapes = frame(&ctx, &mut s, &mut d, vec![], NONE);
    let texts = text_positions(&shapes);
    let row = texts.iter().find(|(t, _)| t == "Text „ol“").unwrap().1;
    assert_eq!(mark_left_of(&shapes, row), Mark::Check);
    // Noch einmal: alle ab.
    for (events, mods) in click_events(row.center(), NONE) {
        frame(&ctx, &mut s, &mut d, events, mods);
    }
    assert_eq!(picked(&d), Vec::<usize>::new());
}

#[test]
fn extrude_text_with_holes_gives_body_with_holes() {
    let (ctx, mut s, id, mut d, i) = extrude_ready();
    click_at(&ctx, &mut s, id, &mut d, glyph_of(T1, 1), NONE);
    assert_eq!(picked(&d), sorted(vec![i.o, i.l]));
    assert!(d.create(&mut s), "{:?}", s.message);
    assert_eq!(s.bodies.len(), 2, "„o“ und „l“ getrennt");
    let height = match &d {
        CreateDialog::ExtrudeSketch { height, .. } => *height,
        _ => unreachable!(),
    };
    let volume: f64 = s.bodies.iter().map(|b| b.mass.volume).sum();
    // Je Zeichen 3 × 3,5 mm²; das Loch von „o“ 1 × 1,5 mm².
    let without_holes = 2.0 * 3.0 * 3.5 * height;
    let with_holes = without_holes - 1.0 * 1.5 * height;
    assert!(
        (volume - with_holes).abs() < 1e-6 * without_holes,
        "{volume} statt {with_holes}"
    );
    assert!(volume < without_holes);
}

#[test]
fn revolve_dialog_uses_groups_too() {
    let ctx = context();
    let (mut s, id, axis) = scene(true);
    aim(&mut s);
    let i = indices(&s, id);
    let mut d = CreateDialog::revolve_sketch(&s, id);
    if let CreateDialog::RevolveSketch { axis: a, .. } = &mut d {
        *a = axis;
    }
    for _ in 0..2 {
        frame(&ctx, &mut s, &mut d, vec![], NONE);
    }
    click_at(&ctx, &mut s, id, &mut d, ring_of(T1, 0), NONE);
    assert_eq!(picked(&d), sorted(vec![i.o, i.l]));
    click_at(&ctx, &mut s, id, &mut d, glyph_of(T1, 1), CTRL);
    assert_eq!(picked(&d), vec![i.o]);
    let shapes = frame(&ctx, &mut s, &mut d, vec![], NONE);
    assert!(
        text_positions(&shapes)
            .iter()
            .any(|(t, _)| t == "Text „ol“"),
        "Liste mit Textzeile"
    );
}

#[test]
fn edit_dialog_uses_groups_too() {
    let ctx = context();
    let (mut s, id, _) = scene(false);
    let i = indices(&s, id);
    let e = s.extrude_sketch(id, i.rect, 2.0).unwrap();
    aim(&mut s);
    let mut d = FeatureDialog::new(&s, e).unwrap();
    let mut run = |s: &mut AppState, events: Vec<egui::Event>, m: egui::Modifiers| {
        ctx.run_ui(raw(events, m), |ui| {
            viewport::show_in(ui, s, None);
            d.show(ui.ctx(), s);
        })
        .drop_without_applying_deltas();
    };
    run(&mut s, vec![], NONE);
    run(&mut s, vec![], NONE);
    let p = screen(&s, id, glyph_of(T2, 0));
    assert!(SCREEN.contains(p), "{p:?}");
    for (events, m) in click_events(p, NONE) {
        run(&mut s, events, m);
    }
    let p = screen(&s, id, ring_of(T1, 0));
    for (events, m) in click_events(p, NONE) {
        run(&mut s, events, m);
    }
    assert_eq!(d.picked_profiles(), sorted(vec![i.o, i.l, i.x, i.rect]));
}

// ---- Korrekturrunde 1 ---------------------------------------------------------------------------

#[test]
fn hover_over_text_highlights_all_of_text_and_with_ctrl_only_one() {
    let (ctx, mut s, id, mut d, i) = extrude_ready();
    let p = screen(&s, id, ring_of(T1, 0));
    let hover_fill = |s: &AppState| {
        let col = planora_app::theme::sketch_colors(s.dark);
        planora_app::testing::fill_triangles(
            &planora_app::testing::finished_strokes(s, SCREEN),
            col.profile_fill_hover,
        )
    };
    frame(
        &ctx,
        &mut s,
        &mut d,
        vec![egui::Event::PointerMoved(p)],
        NONE,
    );
    assert_eq!(
        s.profile_pick.as_ref().map(|p| p.hover.clone()),
        Some(sorted(vec![i.o, i.l])),
        "ohne Strg: der ganze Text"
    );
    let all = hover_fill(&s);
    frame(&ctx, &mut s, &mut d, vec![], CTRL);
    assert_eq!(
        s.profile_pick.as_ref().map(|p| p.hover.clone()),
        Some(vec![i.o]),
        "mit Strg: nur „o“"
    );
    let one = hover_fill(&s);
    assert!(0 < one && one < all, "{one} < {all}");
    frame(&ctx, &mut s, &mut d, vec![], NONE);
    assert_eq!(
        s.profile_pick.as_ref().map(|p| p.hover.clone()),
        Some(sorted(vec![i.o, i.l])),
        "Strg los: wieder der ganze Text"
    );
    // Ein einfaches Profil ist allein hervorgehoben.
    let r = screen(&s, id, RECT);
    frame(
        &ctx,
        &mut s,
        &mut d,
        vec![egui::Event::PointerMoved(r)],
        NONE,
    );
    assert_eq!(
        s.profile_pick.as_ref().map(|p| p.hover.clone()),
        Some(vec![i.rect])
    );
}

#[test]
fn command_without_ctrl_toggles_single() {
    let (ctx, mut s, id, mut d, i) = extrude_ready();
    let command = egui::Modifiers {
        command: true,
        ..egui::Modifiers::NONE
    };
    click_at(&ctx, &mut s, id, &mut d, ring_of(T1, 0), command);
    assert_eq!(picked(&d), vec![i.o], "nur Befehl");
    let mac = egui::Modifiers {
        mac_cmd: true,
        command: true,
        ..egui::Modifiers::NONE
    };
    click_at(&ctx, &mut s, id, &mut d, glyph_of(T1, 1), mac);
    assert_eq!(picked(&d), sorted(vec![i.o, i.l]), "Befehl unter macOS");
    let mac_only = egui::Modifiers {
        mac_cmd: true,
        ..egui::Modifiers::NONE
    };
    click_at(&ctx, &mut s, id, &mut d, ring_of(T1, 0), mac_only);
    assert_eq!(picked(&d), vec![i.l], "nur mac_cmd");
}

#[test]
fn long_text_row_is_one_shortened_line_with_full_tooltip() {
    let content = "Größenänderung über dreißig Zeichen hinaus\nzweite Zeile";
    let mut s = AppState::new();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    {
        let sk = s.session_sketch_mut().unwrap().1;
        let params = TextParams {
            content: content.into(),
            style: TextStyle {
                font: "Test".into(),
                bold: false,
                italic: false,
                spacing: 0.0,
            },
            height: 5.0,
            flip_h: false,
            flip_v: false,
        };
        sk.add_text_box(
            T1,
            T1 + v(30.0, 8.0),
            params,
            HAlign::Left,
            VAlign::Bottom,
            shape("ab"),
        )
        .unwrap();
        sk.add_rectangle(v(-40.0, 0.0), v(-30.0, 10.0)).unwrap();
        sk.solve();
    }
    s.observe_sketch(false);
    s.close_sketch();
    aim(&mut s);
    let ctx = context();
    ctx.all_styles_mut(|st| st.interaction.tooltip_delay = 0.0);
    let mut d = CreateDialog::extrude_sketch(&s, id);
    frame(&ctx, &mut s, &mut d, vec![], NONE);
    let shapes = frame(&ctx, &mut s, &mut d, vec![], NONE);

    let one_line = content.replace('\n', " ");
    let short: String = one_line.chars().take(30).collect();
    let want = format!("Text „{short}…“");
    let texts = text_positions(&shapes);
    let row = texts
        .iter()
        .find(|(t, _)| t.starts_with("Text „"))
        .unwrap_or_else(|| panic!("{texts:?}"));
    assert_eq!(row.0, want);
    assert!(!row.0.contains('\n'));
    let inner = row.0.trim_start_matches("Text „").trim_end_matches('“');
    assert_eq!(inner.chars().count(), 31, "30 Zeichen und „…“");

    // Darüberfahren zeigt den ganzen Inhalt in einer Zeile.
    let at = row.1.center();
    let mut tooltip = false;
    for _ in 0..4 {
        let shapes = frame(
            &ctx,
            &mut s,
            &mut d,
            vec![egui::Event::PointerMoved(at)],
            NONE,
        );
        tooltip |= text_positions(&shapes).iter().any(|(t, _)| *t == one_line);
    }
    assert!(tooltip, "Tooltip mit „{one_line}“");
}
