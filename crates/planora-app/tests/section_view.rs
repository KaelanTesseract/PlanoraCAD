//! Schnittansichten in der App (Teilprojekt 3d, Aufgabe 8): im Hintergrund rechnen, auf dem Blatt
//! zeigen, in SVG, DXF und PDF ausgeben.
//!
//! Prüfkörper ist ein Rohr (Außenradius 20, Innenradius 15, Höhe 40, Achse z) in A3 quer; die
//! Vorderansicht liegt frei, der Schnitt „A–A“ (Art Oben, Versatz z = 20) darunter. Die Zahlen
//! sind von Hand gerechnet: Schnittfläche π · (20² − 15²) = 549,8 mm², bei 3 mm Abstand also
//! rund 183 mm Schraffur in Summe.

use egui::{Pos2, Rect, vec2};
use planora_app::drawing_state::{ComputeFn, DrawingUi, MAX_CACHED, ViewResult};
use planora_app::drawing_view::{self, DrawnState};
use planora_app::export_dialog::*;
use planora_app::state::*;
use planora_app::testing::collect_texts;
use planora_app::{fonts, theme};
use planora_drawing::annotate::{Mark, MarkKind, SectionPen};
use planora_drawing::hatch::MAX_HATCH_LINES;
use planora_drawing::section_marks::{add_section_marks, section_marks};
use planora_drawing::sheet::{SheetModel, build_sheet};
use planora_drawing::{
    DrawBody, DrawError, DrawingFormat, DrawingId, MAX_SECTIONS, Paper, Placement, Projection,
    Scale, SectionData, Sheet, ViewId, ViewKind, ViewLines, compute_section, compute_view,
    dxf_string, svg_string,
};
use planora_exchange::Progress;
use planora_feature::FeatureKind;
use planora_kernel::boolean::BoolOp;
use planora_math::{DVec2, DVec3};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

#[path = "../../planora-drawing/tests/common/pdf_read.rs"]
mod pdf_read;

const A3_QUER: Sheet = Sheet {
    paper: Paper::A3,
    landscape: true,
};
const SVG: ExportFormat = ExportFormat::Drawing(DrawingFormat::Svg);
const DXF: ExportFormat = ExportFormat::Drawing(DrawingFormat::Dxf);
const PDF: ExportFormat = ExportFormat::Drawing(DrawingFormat::Pdf);

const MISSES: &str = "Die Schnittebene trifft den Körper nicht.";
const NAME: &str = "Schnitt A\u{2013}A";

fn one() -> Scale {
    Scale { num: 1, den: 1 }
}

// --- Aufbau ---------------------------------------------------------------------------------

/// Rohr der Höhe `h` (Außenzylinder minus durchgehende Bohrung); liefert den Außenzylinder.
fn tube_state(h: f64) -> (AppState, planora_feature::FeatureId) {
    let mut s = AppState::new();
    let outer = s
        .document
        .add_feature(
            "Außen",
            FeatureKind::Cylinder {
                radius: 20.0,
                height: h,
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    s.document
        .add_feature(
            "Bohrung",
            FeatureKind::Cylinder {
                radius: 15.0,
                height: 60.0,
                origin: DVec3::new(0.0, 0.0, -10.0),
            },
        )
        .unwrap();
    s.refresh();
    s.add_boolean(0, 1, BoolOp::Difference).unwrap();
    s.refresh();
    assert_eq!(s.bodies.len(), 1);
    (s, outer)
}

fn set_height(s: &mut AppState, outer: planora_feature::FeatureId, h: f64) {
    s.document
        .update(outer, |k| {
            if let FeatureKind::Cylinder { height, .. } = k {
                *height = h;
            }
        })
        .unwrap();
    s.refresh();
}

/// Zeichnung A3 quer mit Vorne (frei) und Schnitt A–A (Oben, Versatz `offset`) darunter.
fn tube_drawing(s: &mut AppState, offset: f64) -> (DrawingId, ViewId, ViewId) {
    let d = s
        .document
        .add_drawing("Blatt 1", A3_QUER, Projection::First)
        .unwrap();
    let front = s
        .document
        .add_view(
            d,
            ViewKind::Front,
            one(),
            Placement::Free { x: 150.0, y: 230.0 },
            None,
        )
        .unwrap();
    let sec = add_section(s, d, front, ViewKind::Top, offset, 70.0);
    (d, front, sec)
}

fn add_section(
    s: &mut AppState,
    d: DrawingId,
    parent: ViewId,
    kind: ViewKind,
    offset: f64,
    distance: f64,
) -> ViewId {
    let mut made = None;
    s.document
        .update_drawing(d, |dr| {
            made = Some(dr.add_section(parent, kind, offset, distance));
        })
        .unwrap();
    made.unwrap().unwrap()
}

fn edit_section(
    s: &mut AppState,
    d: DrawingId,
    sec: ViewId,
    f: impl FnOnce(&mut planora_drawing::SectionSpec),
) {
    s.document
        .update_drawing(d, |dr| {
            f(dr.views
                .iter_mut()
                .find(|v| v.id == sec)
                .unwrap()
                .section
                .as_mut()
                .unwrap());
        })
        .unwrap();
}

fn spec(s: &AppState, d: DrawingId, v: ViewId) -> planora_drawing::ViewSpec {
    s.document.drawing(d).unwrap().view(v).unwrap().clone()
}

fn tick(ui: &mut DrawingUi, s: &AppState) {
    ui.tick(&s.bodies, &s.document, true);
}

fn model(s: &AppState) -> u64 {
    s.document.model_edit_count()
}

// --- Bild -----------------------------------------------------------------------------------

struct Screen {
    ctx: egui::Context,
    time: f64,
    dark: bool,
}

impl Screen {
    fn new() -> Screen {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, false);
        Screen {
            ctx,
            time: 0.0,
            dark: false,
        }
    }

    /// Ein Bild des Zeichnungsbereichs; liefert alle gezeichneten Texte.
    fn frame(&mut self, s: &mut AppState) -> String {
        if self.dark != s.dark {
            theme::apply(&self.ctx, s.dark);
            self.dark = s.dark;
        }
        self.time += 0.05;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0))),
            time: Some(self.time),
            ..Default::default()
        };
        let out = self.ctx.run_ui(raw, |ui| drawing_view::show(ui, s));
        let text = collect_texts(&out);
        out.drop_without_applying_deltas();
        text
    }

    /// Bilder, bis keine Ansicht mehr rechnet (mindestens zwei); liefert die Texte des letzten.
    fn settle(&mut self, s: &mut AppState) -> String {
        let mut text = String::new();
        for i in 0..1500 {
            text = self.frame(s);
            let pending = drawing_view::last_layout(&self.ctx).is_some_and(|l| {
                l.views
                    .iter()
                    .any(|v| matches!(v.state, DrawnState::Pending(_)))
            });
            if i >= 1 && !pending {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        text
    }

    fn sheet(&self, s: &AppState, d: DrawingId) -> Arc<SheetModel> {
        drawing_view::last_sheet_for(&self.ctx, s, d).expect("gezeichnetes Blatt")
    }

    fn state_of(&self, v: ViewId) -> DrawnState {
        drawing_view::last_layout(&self.ctx)
            .unwrap()
            .views
            .iter()
            .find(|x| x.id == v)
            .unwrap_or_else(|| panic!("Ansicht {v:?} nicht gezeichnet"))
            .state
            .clone()
    }
}

fn open(s: &mut AppState, d: DrawingId) {
    s.drawing_ui.open(d);
}

fn marks_named<'a>(sheet: &'a SheetModel, name: &str) -> &'a [Mark] {
    &sheet
        .views
        .iter()
        .find(|v| v.name == name)
        .unwrap_or_else(|| panic!("keine Ansicht „{name}“"))
        .marks
}

fn kind_of(m: &Mark) -> MarkKind {
    match m {
        Mark::Line { kind, .. }
        | Mark::Arrow { kind, .. }
        | Mark::Arc { kind, .. }
        | Mark::Text { kind, .. } => *kind,
    }
}

fn hatch_lines(marks: &[Mark]) -> Vec<(DVec2, DVec2)> {
    marks
        .iter()
        .filter_map(|m| match m {
            Mark::Line {
                a,
                b,
                kind: MarkKind::Hatch,
            } => Some((*a, *b)),
            _ => None,
        })
        .collect()
}

fn texts_of(marks: &[Mark]) -> Vec<String> {
    marks
        .iter()
        .filter_map(|m| match m {
            Mark::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn total_marks(sheet: &SheetModel) -> usize {
    sheet.views.iter().map(|v| v.marks.len()).sum()
}

// --- Ersatzrechnungen -----------------------------------------------------------------------

fn fail_top(b: &[DrawBody], k: ViewKind, p: &Progress) -> Result<ViewLines, DrawError> {
    if k == ViewKind::Top {
        Err(DrawError::TooComplex(5))
    } else {
        compute_view(b, k, p)
    }
}

// --- (a) ------------------------------------------------------------------------------------

#[test]
fn section_computes_in_background() {
    let (mut st, _) = tube_state(40.0);
    let (d, front, sec) = tube_drawing(&mut st, 20.0);
    let mut ui = DrawingUi::default();
    ui.open(d);
    tick(&mut ui, &st);
    assert_eq!(ui.spawned(), 2, "Vorne und der Schnitt je einmal");
    let m = model(&st);
    let (f_spec, s_spec) = (spec(&st, d, front), spec(&st, d, sec));
    let ViewResult::Ready(front_lines) = ui.result_of(&f_spec, m) else {
        panic!("Vorne: {:?}", ui.result_of(&f_spec, m))
    };
    let ViewResult::Ready(sec_lines) = ui.result_of(&s_spec, m) else {
        panic!("Schnitt: {:?}", ui.result_of(&s_spec, m))
    };
    assert_eq!(sec_lines.kind, ViewKind::Top);
    let data = ui.section_of(&s_spec, m).expect("Schnittdaten");
    assert_eq!(data.regions.len(), 1);
    assert_eq!(data.regions[0].loops.len(), 2, "Außen- und Innenrand");
    assert!(
        ui.section_of(&f_spec, m).is_none(),
        "Vorne ist kein Schnitt"
    );
    // Die Ansicht „Oben“ ohne Schnitt wäre ein anderer Schlüssel.
    assert_eq!(
        ui.result(ViewKind::Top, m),
        ViewResult::Pending { fraction: 0.0 }
    );

    // Nur der Schraffurabstand (und Schalter, Buchstabe): nichts rechnet neu.
    edit_section(&mut st, d, sec, |s| s.hatch.spacing = 5.0);
    tick(&mut ui, &st);
    edit_section(&mut st, d, sec, |s| s.hatch.on = false);
    tick(&mut ui, &st);
    assert_eq!(ui.spawned(), 2, "Abstand und Schalter rechnen nichts neu");
    assert_eq!(
        ui.result_of(&s_spec, m),
        ViewResult::Ready(sec_lines.clone())
    );

    // Ein anderer Versatz: genau ein neuer Auftrag, Vorne bleibt.
    edit_section(&mut st, d, sec, |s| s.offset = 25.0);
    tick(&mut ui, &st);
    assert_eq!(ui.spawned(), 3);
    let moved = spec(&st, d, sec);
    let ViewResult::Ready(sec2) = ui.result_of(&moved, m) else {
        panic!("{:?}", ui.result_of(&moved, m))
    };
    assert!(!Arc::ptr_eq(&sec_lines, &sec2), "neue Linien");
    assert_eq!(ui.result_of(&f_spec, m), ViewResult::Ready(front_lines));
    assert_eq!(ui.section_of(&moved, m).unwrap().regions.len(), 1);
}

#[test]
fn dragging_the_plane_keeps_the_cache_bounded() {
    let (mut st, _) = tube_state(40.0);
    let (d, _, sec) = tube_drawing(&mut st, 1.0);
    let mut ui = DrawingUi::default();
    ui.open(d);
    tick(&mut ui, &st);
    // Gebrauchte Einträge bleiben, ältere der Schnittebene fallen heraus.
    for i in 2..=(MAX_CACHED as i32 + 6) {
        edit_section(&mut st, d, sec, |s| s.offset = f64::from(i));
        tick(&mut ui, &st);
        assert!(ui.cached() <= MAX_CACHED, "{} Einträge", ui.cached());
    }
    assert_eq!(ui.spawned(), 1 + MAX_CACHED as u64 + 6);
    assert_eq!(ui.cached(), MAX_CACHED);
}

#[test]
fn twelve_sections_stay_cached_when_switching_sheets() {
    let (mut st, _) = tube_state(40.0);
    let d = st
        .document
        .add_drawing("Viele", A3_QUER, Projection::First)
        .unwrap();
    let front = st
        .document
        .add_view(
            d,
            ViewKind::Front,
            Scale { num: 1, den: 5 },
            Placement::Free { x: 100.0, y: 200.0 },
            None,
        )
        .unwrap();
    for i in 0..MAX_SECTIONS {
        add_section(&mut st, d, front, ViewKind::Top, 2.0 + 3.0 * i as f64, 60.0);
    }
    let other = st
        .document
        .add_drawing("Andere", A3_QUER, Projection::First)
        .unwrap();
    st.document
        .add_view(
            other,
            ViewKind::Front,
            one(),
            Placement::Free { x: 100.0, y: 200.0 },
            None,
        )
        .unwrap();
    let mut ui = DrawingUi::default();
    ui.open(d);
    tick(&mut ui, &st);
    let spawned = ui.spawned();
    assert_eq!(spawned, 1 + MAX_SECTIONS as u64);
    ui.open(other);
    tick(&mut ui, &st);
    assert_eq!(ui.spawned(), spawned, "Vorne 1:1 ist dieselbe Art");
    ui.open(d);
    tick(&mut ui, &st);
    assert_eq!(ui.spawned(), spawned, "alle zwölf Schnitte sind noch da");
    assert!(ui.cached() <= MAX_CACHED);
}

// --- (b) ------------------------------------------------------------------------------------

#[test]
fn sheet_shows_hatch_line_title() {
    let (mut st, _) = tube_state(40.0);
    let (d, front, sec) = tube_drawing(&mut st, 20.0);
    open(&mut st, d);
    let mut screen = Screen::new();
    screen.settle(&mut st);
    assert_eq!(screen.state_of(front), DrawnState::Lines);
    assert_eq!(screen.state_of(sec), DrawnState::Lines);
    let sheet = screen.sheet(&st, d);

    // Schraffur und Titel in „Schnitt A–A“.
    let own = marks_named(&sheet, NAME);
    let hatch = hatch_lines(own);
    assert!(hatch.len() >= 20, "{} Schraffurlinien", hatch.len());
    let len: f64 = hatch.iter().map(|(a, b)| (*b - *a).length()).sum();
    // Die Linien liegen im Abstand 3 mm · k von der Rohrmitte (Phase im Blattnullpunkt der
    // Ansicht): je Linie Sehne des Außenkreises minus Sehne des Innenkreises.
    let chord = |r: f64, t: f64| {
        if t.abs() < r {
            2.0 * (r * r - t * t).sqrt()
        } else {
            0.0
        }
    };
    let want: f64 = (-6..=6)
        .map(|k| f64::from(k) * 3.0)
        .map(|t| chord(20.0, t) - chord(15.0, t))
        .sum();
    assert!(
        (len - want).abs() < 0.05,
        "Schraffurlänge {len:.2} statt {want:.2}"
    );
    assert!(
        texts_of(own).contains(&"A\u{2013}A (1:1)".to_string()),
        "{:?}",
        texts_of(own)
    );
    // Alle Linien liegen auf 45° und halten 3 mm Abstand (gleiche Normale).
    let mut offsets: Vec<f64> = hatch
        .iter()
        .map(|(a, b)| {
            let dir = (*b - *a).normalize();
            assert!(
                (dir.x.abs() - dir.y.abs()).abs() < 1e-6,
                "keine 45°: {dir:?}"
            );
            a.x - a.y * dir.x / dir.y
        })
        .collect();
    offsets.sort_by(f64::total_cmp);
    offsets.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    for w in offsets.windows(2) {
        let gap = (w[1] - w[0]) / std::f64::consts::SQRT_2;
        assert!(
            (gap / 3.0).round() >= 1.0 && (gap - 3.0 * (gap / 3.0).round()).abs() < 1e-6,
            "Abstand {gap}"
        );
    }

    // Schnittlinie mit Buchstaben in „Vorne“.
    let src = marks_named(&sheet, "Vorne");
    let chain = src
        .iter()
        .filter(|m| matches!(kind_of(m), MarkKind::SectionLine(SectionPen::Chain)))
        .count();
    let ends = src
        .iter()
        .filter(|m| matches!(kind_of(m), MarkKind::SectionLine(SectionPen::End)))
        .count();
    assert!(
        chain >= 3 && ends == 2,
        "{chain} Kettenstücke, {ends} Endstücke"
    );
    assert_eq!(
        texts_of(src).iter().filter(|t| *t == "A").count(),
        2,
        "zwei Buchstaben"
    );
    assert_eq!(
        src.iter()
            .filter(|m| matches!(m, Mark::Arrow { .. }))
            .count(),
        2
    );

    // Alle Marken werden gemalt.
    let layout = drawing_view::last_layout(&screen.ctx).unwrap();
    assert_eq!(layout.marks_drawn, total_marks(&sheet));
    assert!(layout.marks_drawn > hatch.len() + chain + ends);

    // Erstwinkel: der Schnitt liegt in Pfeilrichtung (nach unten) unter Vorne.
    let r = |v: ViewId| layout.views.iter().find(|x| x.id == v).unwrap().rect;
    assert!(
        r(sec).center().y > r(front).bottom(),
        "{:?} {:?}",
        r(sec),
        r(front)
    );
    assert!((r(sec).center().x - r(front).center().x).abs() < 1.0);
}

// --- (c) ------------------------------------------------------------------------------------

#[test]
fn failure_shows_message() {
    let (mut st, _) = tube_state(40.0);
    let (d, front, sec) = tube_drawing(&mut st, 20.0);
    open(&mut st, d);
    st.drawing_ui.set_compute(fail_top as ComputeFn);
    let mut screen = Screen::new();
    let text = screen.settle(&mut st);
    let msg = DrawError::TooComplex(5).to_string();
    assert_eq!(screen.state_of(sec), DrawnState::Failed(msg.clone()));
    assert_eq!(screen.state_of(front), DrawnState::Lines);
    assert!(text.contains(&msg), "{text}");
    let sheet = screen.sheet(&st, d);
    assert!(
        sheet.views.iter().all(|v| v.name != NAME),
        "kein Blatt für die gescheiterte Ansicht"
    );
    assert!(hatch_lines(marks_named(&sheet, "Vorne")).is_empty());

    // Ebene außerhalb des Körpers.
    st.drawing_ui.set_compute(compute_view);
    edit_section(&mut st, d, sec, |s| s.offset = 100.0);
    let text = screen.settle(&mut st);
    assert_eq!(screen.state_of(sec), DrawnState::Failed(MISSES.into()));
    assert!(text.contains(MISSES), "{text}");
}

// --- (d) ------------------------------------------------------------------------------------

fn expected_missing() -> String {
    format!("Die Ansicht „{NAME}“ lässt sich nicht berechnen: {MISSES}")
}

#[test]
fn model_change_follows() {
    let (mut st, outer) = tube_state(40.0);
    let (d, _, sec) = tube_drawing(&mut st, 20.0);
    open(&mut st, d);
    let mut screen = Screen::new();
    screen.settle(&mut st);
    assert!(!hatch_lines(marks_named(&screen.sheet(&st, d), NAME)).is_empty());
    let s_spec = spec(&st, d, sec);
    let before = match st.drawing_ui.result_of(&s_spec, model(&st)) {
        ViewResult::Ready(l) => l,
        other => panic!("{other:?}"),
    };

    // Rohr auf 5 mm gekürzt: Ebene z = 20 liegt außerhalb.
    set_height(&mut st, outer, 5.0);
    let text = screen.settle(&mut st);
    assert_eq!(screen.state_of(sec), DrawnState::Failed(MISSES.into()));
    assert!(text.contains(MISSES), "{text}");
    assert!(
        screen.sheet(&st, d).views.iter().all(|v| v.name != NAME),
        "keine alte Schraffur"
    );
    assert!(st.drawing_ui.section_of(&s_spec, model(&st)).is_none());
    let mut dialog = ExportDialog::new(&st);
    dialog.with_frame = false;
    dialog.format = SVG;
    dialog.tick(&st, true);
    assert_eq!(dialog.status(&st), ExportStatus::Failed(expected_missing()));
    let dir = temp_dir("modell-aenderung");
    let path = dir.join("Blatt.svg");
    dialog.write_to(path.clone(), &st);
    dialog.tick(&st, true);
    assert!(matches!(dialog.write_state(), WriteState::Failed(_)));
    assert!(!path.exists(), "keine Datei");
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
    let _ = fs::remove_dir_all(&dir);

    // Wieder 40 mm: die Schraffur kommt zurück, mit neuen Linien.
    set_height(&mut st, outer, 40.0);
    screen.settle(&mut st);
    assert_eq!(screen.state_of(sec), DrawnState::Lines);
    assert!(!hatch_lines(marks_named(&screen.sheet(&st, d), NAME)).is_empty());
    let after = match st.drawing_ui.result_of(&s_spec, model(&st)) {
        ViewResult::Ready(l) => l,
        other => panic!("{other:?}"),
    };
    assert!(!Arc::ptr_eq(&before, &after), "kein altes Ergebnis");
    assert_eq!(
        st.drawing_ui
            .section_of(&s_spec, model(&st))
            .unwrap()
            .regions
            .len(),
        1
    );
}

// --- (e) ------------------------------------------------------------------------------------

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "planora-section-view-{}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// Das Blatt, das der Test selbst aus Körpern und Zeichnung rechnet (mit Schnittmarken).
fn own_sheet(s: &AppState, id: DrawingId) -> SheetModel {
    let bodies: Vec<DrawBody> = s
        .bodies
        .iter()
        .map(|b| DrawBody {
            name: b.name.clone(),
            solid: &b.solid,
        })
        .collect();
    let drawing = s.document.drawing(id).unwrap();
    let mut lines: Vec<(ViewId, ViewLines)> = Vec::new();
    let mut data: Vec<(ViewId, SectionData)> = Vec::new();
    for v in &drawing.views {
        match &v.section {
            None => lines.push((
                v.id,
                compute_view(&bodies, v.kind, &Progress::new()).unwrap(),
            )),
            Some(sec) => {
                let r =
                    compute_section(&bodies, &sec.plane(v.kind), compute_view, &Progress::new())
                        .unwrap();
                lines.push((v.id, r.lines));
                data.push((v.id, r.data));
            }
        }
    }
    let map: BTreeMap<ViewId, &ViewLines> = lines.iter().map(|(i, l)| (*i, l)).collect();
    let dmap: BTreeMap<ViewId, &SectionData> = data.iter().map(|(i, d)| (*i, d)).collect();
    let mut model = build_sheet(drawing, &map);
    let (marks, errors) = section_marks(drawing, &map, &dmap);
    assert!(errors.is_empty(), "{errors:?}");
    add_section_marks(&mut model, &marks);
    model
}

fn dialog(s: &AppState, format: ExportFormat) -> ExportDialog {
    let mut d = ExportDialog::new(s);
    d.with_frame = false;
    d.format = format;
    d.tick(s, true);
    d
}

fn write(d: &mut ExportDialog, s: &AppState, path: &Path) -> Vec<u8> {
    d.write_to(path.to_path_buf(), s);
    d.tick(s, true);
    match d.write_state() {
        WriteState::Done(p, n) => {
            let bytes = fs::read(&p).unwrap();
            assert_eq!(n, bytes.len() as u64);
            bytes
        }
        other => panic!("{other:?}"),
    }
}

fn pdf_texts(pdf: &pdf_read::Pdf, page: usize) -> Vec<String> {
    pdf.pages[page]
        .items
        .iter()
        .filter_map(|i| match i {
            pdf_read::Item::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn export_svg_dxf_pdf() {
    let (mut st, _) = tube_state(40.0);
    let (d, _, _) = tube_drawing(&mut st, 20.0);
    let own = own_sheet(&st, d);
    let hatch = hatch_lines(marks_named(&own, NAME)).len();
    assert!(hatch >= 20);
    let dir = temp_dir("ausgabe");

    // SVG: genau das eigene Blatt, mit den Gruppen der Schnittmarken.
    let mut dlg = dialog(&st, SVG);
    let bytes = write(&mut dlg, &st, &dir.join("a.svg"));
    let svg = String::from_utf8(bytes).unwrap();
    assert_eq!(svg, svg_string(&own).unwrap(), "Datei = eigenes Blatt");
    for id in [
        "Schnitt A\u{2013}A Schraffur",
        "Schnitt A\u{2013}A Schnitt",
        "Vorne Schnitt",
    ] {
        assert!(svg.contains(&format!("<g id=\"{id}\"")), "Gruppe „{id}“");
    }

    // DXF: ebenso, mit den Ebenen.
    let mut dlg = dialog(&st, DXF);
    let bytes = write(&mut dlg, &st, &dir.join("a.dxf"));
    let dxf = String::from_utf8(bytes).unwrap();
    assert_eq!(dxf, dxf_string(&own).unwrap(), "Datei = eigenes Blatt");
    assert!(dxf.contains("SCHRAFFUR") && dxf.contains("SCHNITTLINIE"));

    // PDF: gültig, mit dem Titel; „Alle Blätter“ mit einem zweiten, gewöhnlichen Blatt.
    let other = st
        .document
        .add_drawing("Blatt 2", A3_QUER, Projection::First)
        .unwrap();
    st.document
        .add_view(
            other,
            ViewKind::Front,
            one(),
            Placement::Free { x: 150.0, y: 150.0 },
            None,
        )
        .unwrap();
    for all in [false, true] {
        let mut dlg = ExportDialog::new(&st);
        dlg.with_frame = false;
        dlg.format = PDF;
        dlg.drawing = Some(d);
        if all {
            dlg.pdf_pages = PdfPages::All;
        }
        dlg.tick(&st, true);
        let bytes = write(
            &mut dlg,
            &st,
            &dir.join(if all { "alle.pdf" } else { "ein.pdf" }),
        );
        let pdf = pdf_read::read(&bytes).unwrap_or_else(|e| panic!("kein gültiges PDF: {e}"));
        assert_eq!(pdf.pages.len(), if all { 2 } else { 1 });
        let texts = pdf_texts(&pdf, 0);
        assert!(texts.contains(&"A\u{2013}A (1:1)".to_string()), "{texts:?}");
        assert_eq!(texts.iter().filter(|t| *t == "A").count(), 2);
    }
    let _ = fs::remove_dir_all(&dir);
}

// --- (f) ------------------------------------------------------------------------------------

#[test]
fn too_fine_blocks_export() {
    // Eine Platte von 8 m × 8 m, geschnitten bei 0,5 mm Abstand auf dem Blatt: mehr
    // Schraffurstücke als erlaubt.
    let mut st = AppState::new();
    st.document
        .add_feature(
            "Platte",
            FeatureKind::Box {
                size: DVec3::new(8000.0, 8000.0, 40.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    st.refresh();
    let d = st
        .document
        .add_drawing("Gross", A3_QUER, Projection::First)
        .unwrap();
    let front = st
        .document
        .add_view(
            d,
            ViewKind::Front,
            one(),
            Placement::Free { x: 150.0, y: 230.0 },
            None,
        )
        .unwrap();
    let sec = add_section(&mut st, d, front, ViewKind::Top, 20.0, 70.0);
    edit_section(&mut st, d, sec, |s| s.hatch.spacing = 0.5);
    const { assert!(MAX_HATCH_LINES < 22_000) };
    let msg = format!("Die Schraffur von „{NAME}“ ist zu fein.");
    assert!(
        !msg.chars().any(|c| c.is_ascii_digit()),
        "keine Zahlen in der Meldung: {msg}"
    );

    open(&mut st, d);
    let mut screen = Screen::new();
    let text = screen.settle(&mut st);
    assert!(text.contains(&msg), "Hinweiszeile:\n{text}");
    assert!(
        hatch_lines(marks_named(&screen.sheet(&st, d), NAME)).is_empty(),
        "keine halbe Schraffur"
    );
    assert_eq!(
        drawing_view::section_errors(&screen.ctx, &st, d),
        vec![msg.clone()]
    );

    let mut dlg = dialog(&st, SVG);
    assert_eq!(dlg.status(&st), ExportStatus::Failed(msg.clone()));
    let dir = temp_dir("zu-fein");
    let path = dir.join("g.svg");
    dlg.write_to(path.clone(), &st);
    dlg.tick(&st, true);
    assert!(matches!(dlg.write_state(), WriteState::Failed(_)));
    assert!(!path.exists());
    let _ = fs::remove_dir_all(&dir);

    // Mit gröberem Abstand geht es wieder.
    edit_section(&mut st, d, sec, |s| s.hatch.spacing = 50.0);
    let text = screen.settle(&mut st);
    assert!(!text.contains(&msg), "{text}");
    assert!(!hatch_lines(marks_named(&screen.sheet(&st, d), NAME)).is_empty());
    assert!(drawing_view::section_errors(&screen.ctx, &st, d).is_empty());
    let dlg = dialog(&st, SVG);
    assert!(
        matches!(dlg.status(&st), ExportStatus::DrawingReady { .. }),
        "{:?}",
        dlg.status(&st)
    );
}

// --- (g) ------------------------------------------------------------------------------------

#[test]
fn hidden_bodies_not_cut() {
    // Zwei sichtbare Körper, beide von der Ebene getroffen.
    let mut st = AppState::new();
    for (name, r, h, z) in [("Außen", 20.0, 40.0, 0.0), ("Bohrung", 15.0, 60.0, -10.0)] {
        st.document
            .add_feature(
                name,
                FeatureKind::Cylinder {
                    radius: r,
                    height: h,
                    origin: DVec3::new(0.0, 0.0, z),
                },
            )
            .unwrap();
    }
    st.refresh();
    let (d, _, sec) = tube_drawing(&mut st, 20.0);
    let mut ui = DrawingUi::default();
    ui.open(d);
    tick(&mut ui, &st);
    let s_spec = spec(&st, d, sec);
    let both = ui.section_of(&s_spec, model(&st)).unwrap();
    assert_eq!(both.source, vec![0, 1]);
    assert_eq!(both.regions.len(), 2);

    // Die Boolesche Operation blendet die beiden Eingangskörper aus: nur das Rohr bleibt.
    st.add_boolean(0, 1, BoolOp::Difference).unwrap();
    st.refresh();
    assert_eq!(st.bodies.len(), 1);
    tick(&mut ui, &st);
    let one_body = ui.section_of(&s_spec, model(&st)).unwrap();
    assert_eq!(one_body.source, vec![0], "nur der sichtbare Körper");
    assert_eq!(one_body.regions.len(), 1);
    assert_eq!(one_body.regions[0].loops.len(), 2, "mit Bohrung");
}

// --- Schnitt in einer anderen Quellansicht --------------------------------------------------

#[test]
fn vertical_line_in_the_side_view_gives_a_front_section() {
    // Schnitt aus „Links“: senkrechte Linie, Blickrichtung nach rechts, also Art „Hinten“ bzw.
    // „Vorne“ je nach Seite; hier genügt: es rechnet, hat eine Fläche und eine Schraffur.
    let (mut st, _) = tube_state(40.0);
    let d = st
        .document
        .add_drawing("Seite", A3_QUER, Projection::First)
        .unwrap();
    let left = st
        .document
        .add_view(
            d,
            ViewKind::Left,
            one(),
            Placement::Free { x: 150.0, y: 200.0 },
            None,
        )
        .unwrap();
    let kind = planora_drawing::section_kind(ViewKind::Left, false, false).unwrap();
    let sec = add_section(&mut st, d, left, kind, 3.0, 80.0);
    open(&mut st, d);
    let mut screen = Screen::new();
    screen.settle(&mut st);
    assert_eq!(screen.state_of(sec), DrawnState::Lines, "{kind:?}");
    let sheet = screen.sheet(&st, d);
    assert!(!hatch_lines(marks_named(&sheet, NAME)).is_empty());
}
