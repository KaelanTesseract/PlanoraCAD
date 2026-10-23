//! Dateifenster (Exportieren, Netz importieren, STEP importieren) in verschiedenen
//! Fenstergrößen: alle Zeilen nutzen dieselbe Breite (auch bei sehr langen Namen), das Fenster
//! steht beim Öffnen mittig und wächst in großen Fenstern mit, die Leiste der Schnellzugriffe
//! verschiebt sich beim Überfahren nicht.

mod common;

use common::{Screen, Seen, find};
use egui::{Rect, pos2, vec2};
use planora_app::export_dialog::ExportDialog;
use planora_app::file_chooser::FileChooser;
use planora_app::import_dialog::ImportDialog;
use planora_app::places::Places;
use planora_app::state::AppState;
use planora_app::step_import_dialog::StepImportDialog;
use planora_math::DVec3;
use std::fs;
use std::path::{Path, PathBuf};

const LONG: &str = "Ein sehr langer Ordnername mit vielen Wörtern, der nicht in eine Zeile passt und immer weiter geht bis zum Ende";

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-groesse-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    for i in 0..30 {
        fs::create_dir_all(d.join(format!("Ordner {i:02}"))).unwrap();
    }
    fs::create_dir_all(d.join(LONG)).unwrap();
    for f in ["a.stl", "b.obj", "c.step"] {
        fs::write(d.join(f), b"x").unwrap();
    }
    fs::write(d.join(format!("{LONG}.stl")), b"x").unwrap();
    fs::write(d.join(format!("{LONG}.step")), b"x").unwrap();
    d
}

fn shared(dir: &Path) -> Places {
    Places {
        drives: Some(
            ["C:\\", "D:\\", "E:\\", "F:\\", "G:\\", "H:\\"]
                .into_iter()
                .map(PathBuf::from)
                .collect(),
        ),
        favorites: vec![dir.join(LONG), dir.join("Ordner 01")],
        ..Places::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Export,
    Mesh,
    Step,
}

const KINDS: [Kind; 3] = [Kind::Export, Kind::Mesh, Kind::Step];
const SIZES: [(f32, f32); 3] = [(800.0, 600.0), (1400.0, 900.0), (2000.0, 1100.0)];

/// Ein offenes Dateifenster mit Zustand.
struct Open {
    kind: Kind,
    export: Option<ExportDialog>,
    mesh: Option<ImportDialog>,
    step: Option<StepImportDialog>,
    state: AppState,
    last: Option<PathBuf>,
    places: Places,
}

impl Open {
    fn new(kind: Kind, dir: &Path) -> Open {
        let mut state = AppState::new();
        let (mut export, mut mesh, mut step) = (None, None, None);
        match kind {
            Kind::Export => {
                state.create_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
                let mut d = ExportDialog::new(&state);
                d.tick(&state, true);
                d.begin_save(dir.to_path_buf(), &state);
                export = Some(d);
            }
            Kind::Mesh => mesh = Some(ImportDialog::new(dir.to_path_buf())),
            Kind::Step => step = Some(StepImportDialog::new(dir.to_path_buf())),
        }
        Open {
            kind,
            export,
            mesh,
            step,
            state,
            last: None,
            places: shared(dir),
        }
    }

    fn show(&mut self, ctx: &egui::Context) {
        let (s, l, p) = (&mut self.state, &mut self.last, &mut self.places);
        let open = match self.kind {
            Kind::Export => self.export.as_mut().unwrap().show_in(ctx, s, l, p),
            Kind::Mesh => self.mesh.as_mut().unwrap().show_in(ctx, s, l, p),
            Kind::Step => self.step.as_mut().unwrap().show_in(ctx, s, l, p),
        };
        assert!(open);
    }

    fn rect(&self) -> Rect {
        match self.kind {
            Kind::Export => self.export.as_ref().unwrap().last_rect,
            Kind::Mesh => self.mesh.as_ref().unwrap().last_rect,
            Kind::Step => self.step.as_ref().unwrap().last_rect,
        }
        .expect("gezeichnet")
    }

    fn title(&self) -> &'static str {
        match self.kind {
            Kind::Export => "Exportieren",
            Kind::Mesh => "Netz importieren",
            Kind::Step => "STEP importieren",
        }
    }

    fn button(&self) -> &'static str {
        match self.kind {
            Kind::Export => "Speichern",
            _ => "Öffnen",
        }
    }
}

/// Rechtecke der Bedienelemente im Fenster `title` (letztes Bild).
fn widgets(screen: &Screen, title: &str) -> Vec<Rect> {
    let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new(title));
    screen.ctx.viewport(|v| {
        v.prev_pass
            .widgets
            .get_layer(layer)
            .map(|w| w.rect)
            .collect()
    })
}

fn settle(screen: &mut Screen, o: &mut Open) -> Vec<Seen> {
    screen.settle(8, &mut |ctx| o.show(ctx))
}

/// Erwartete Außenbreite eines Dateifensters im Fenster der Breite `w` (Rand 16 je Seite).
fn expected_width(w: f32) -> f32 {
    (0.55 * w).clamp(720.0, 1200.0).min(w - 32.0)
}

#[test]
fn all_rows_share_the_dialog_width_even_with_long_names() {
    for (w, h) in SIZES {
        for kind in KINDS {
            let dir = temp_dir(&format!("zeilen-{kind:?}-{w}"));
            let mut o = Open::new(kind, &dir);
            let mut screen = Screen::new(w, h);
            let seen = settle(&mut screen, &mut o);
            let what = format!("{kind:?} bei {w} × {h}");
            let rect = o.rect();
            assert!(
                (rect.width() - expected_width(w)).abs() <= 1.0,
                "{what}: Breite {rect:?}"
            );
            // Innenrand des Rahmens: 12 + 1 Strich.
            let inner_right = rect.right() - 13.0;
            let rects = widgets(&screen, o.title());
            let inside: Vec<&Rect> = rects
                .iter()
                .filter(|r| r.width() < rect.width() - 4.0)
                .collect();
            for r in &inside {
                assert!(
                    r.right() <= inner_right + 1.0 && r.left() >= rect.left(),
                    "{what}: Element ragt hinaus: {r:?} (Inhalt bis {inner_right})"
                );
            }
            // Kopfzeile (Schließen-Symbol), Ordnerfeld und Dateiname enden am rechten Rand.
            let close = planora_app::icons::glyph(planora_app::icons::Icon::Close).to_string();
            let close = find(&seen, &close).expect("Schließen-Symbol").rect;
            assert!(
                (close.right() - inner_right).abs() <= 8.0,
                "{what}: Schließen-Symbol bei {close:?}, Inhalt bis {inner_right}"
            );
            let reaching = inside
                .iter()
                .filter(|r| (r.right() - inner_right).abs() <= 2.0)
                .count();
            assert!(reaching >= 3, "{what}: zu wenige Zeilen bis zum Rand");
            // Jede Listenzeile geht über die ganze Breite (bis zur Rollleiste der Liste).
            for name in ["Ordner 00", "Ordner 02"] {
                let text = find(&seen, name).unwrap_or_else(|| panic!("{what}: {name}"));
                let row = inside
                    .iter()
                    .filter(|r| r.contains(text.rect.center()) && r.height() < 30.0)
                    .max_by(|a, b| a.width().total_cmp(&b.width()))
                    .unwrap_or_else(|| panic!("{what}: Zeile von {name}"));
                assert!(
                    row.right() >= inner_right - 20.0,
                    "{what}: Zeile {name} endet bei {row:?}, Inhalt bis {inner_right}"
                );
            }
            // Lange Namen werden gekürzt statt das Fenster zu verbreitern.
            for s in &seen {
                if s.clip.intersects(s.rect) && s.text.contains("Ein sehr langer") {
                    assert!(s.rect.right() <= inner_right + 1.0, "{what}: {:?}", s.rect);
                }
            }
            let _ = fs::remove_dir_all(&dir);
        }
    }
}

#[test]
fn file_dialogs_open_centred_and_grow_with_the_window() {
    for kind in KINDS {
        let dir = temp_dir(&format!("mitte-{kind:?}"));
        let mut list_heights = Vec::new();
        // Ein Bildschirm, mehrere Fenstergrößen: zwischen dem Schließen und dem neuen Öffnen
        // ändert sich die Größe (die alte Lage darf nicht hängen bleiben).
        let mut screen = Screen::new(800.0, 600.0);
        for (w, h) in SIZES {
            screen.size = vec2(w, h);
            // Ein paar Bilder ohne Fenster (geschlossen).
            screen.settle(2, &mut |_| {});
            let mut o = Open::new(kind, &dir);
            let seen = settle(&mut screen, &mut o);
            let what = format!("{kind:?} bei {w} × {h}");
            let rect = o.rect();
            let centre = pos2(w / 2.0, h / 2.0);
            assert!(
                (rect.center().x - centre.x).abs() <= 3.0
                    && (rect.center().y - centre.y).abs() <= 3.0,
                "{what}: nicht mittig: {rect:?}"
            );
            assert!(
                Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h)).contains_rect(rect),
                "{what}: {rect:?}"
            );
            assert!(
                (rect.width() - expected_width(w)).abs() <= 1.0,
                "{what}: Breite {rect:?}"
            );
            let b = find(&seen, o.button()).unwrap_or_else(|| panic!("{what}: Knopf fehlt"));
            assert!(
                b.clip.contains_rect(b.rect) && rect.contains_rect(b.rect),
                "{what}: Knopf {:?} nicht sichtbar",
                b.rect
            );
            // Höhe der Liste: vom ersten bis zum letzten sichtbaren Ordner.
            let rows: Vec<&Seen> = seen
                .iter()
                .filter(|s| s.text.starts_with("Ordner ") && s.clip.contains_rect(s.rect))
                .collect();
            let top = rows.iter().map(|s| s.rect.top()).fold(f32::MAX, f32::min);
            let bottom = rows
                .iter()
                .map(|s| s.rect.bottom())
                .fold(f32::MIN, f32::max);
            list_heights.push(bottom - top);
            // Fenster schließen: nicht mehr zeigen.
            drop(o);
        }
        assert!(
            list_heights[1] > list_heights[0] + 40.0 && list_heights[2] > list_heights[1] + 40.0,
            "{kind:?}: Liste wächst mit dem Fenster: {list_heights:?}"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn file_dialog_width_is_a_share_of_the_window_between_720_and_1200() {
    use planora_app::dialog::{DialogSize, limits};
    let at = |w: f32| {
        limits(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(w, 900.0)),
            DialogSize::File,
        )
        .0
    };
    assert_eq!(at(800.0), 720.0);
    assert_eq!(at(1000.0), 720.0);
    assert!((at(1600.0) - 880.0).abs() < 0.5);
    assert_eq!(at(2400.0), 1200.0);
    assert_eq!(at(3840.0), 1200.0);
    // Schmaler als 720 nur, wenn das Fenster selbst schmaler ist.
    assert_eq!(at(700.0), 700.0 - 32.0);
    // Andere Dialoge bleiben wie sie sind.
    let wide = limits(
        Rect::from_min_size(pos2(0.0, 0.0), vec2(2400.0, 900.0)),
        DialogSize::Wide,
    )
    .0;
    assert_eq!(wide, 720.0);
}

#[test]
fn at_800x600_file_dialogs_stay_720_wide() {
    for kind in KINDS {
        let dir = temp_dir(&format!("klein-{kind:?}"));
        let mut o = Open::new(kind, &dir);
        let mut screen = Screen::new(800.0, 600.0);
        let seen = settle(&mut screen, &mut o);
        let b = find(&seen, o.button()).expect("Knopf");
        assert!(b.clip.contains_rect(b.rect), "{kind:?}: Fußzeile sichtbar");
        assert!(
            (o.rect().width() - 720.0).abs() <= 1.0,
            "{kind:?}: {:?}",
            o.rect()
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

// --- Leiste: Überfahren ändert nichts an der Lage -----------------------------------------------

/// Lage aller Beschriftungen der Leiste (Ordnerfeld bis Liste) nach einem Bild mit `events`.
fn bar_layout(
    ctx: &egui::Context,
    c: &mut FileChooser,
    shared: &mut Places,
    events: Vec<egui::Event>,
    time: f64,
) -> Vec<(String, Rect)> {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(520.0, 700.0))),
        events,
        time: Some(time),
        ..Default::default()
    };
    let out = ctx.run_ui(input, |ui| {
        c.show_with(ui, shared);
    });
    let texts = common_texts(&out);
    out.drop_without_applying_deltas();
    texts
        .into_iter()
        .filter(|(t, _)| {
            [
                "Dokumente",
                "Downloads",
                "Desktop",
                "Bilder",
                "C:",
                "D:",
                "E:",
                "F:",
                "G:",
                "H:",
            ]
            .contains(&t.as_str())
                || t == "Lieblingsordner"
        })
        .collect()
}

fn common_texts(out: &egui::FullOutput) -> Vec<(String, Rect)> {
    fn walk(s: &egui::epaint::Shape, out: &mut Vec<(String, Rect)>) {
        match s {
            egui::epaint::Shape::Text(t) => out.push((
                t.galley.text().to_string(),
                Rect::from_min_size(t.pos, t.galley.size()),
            )),
            egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut v = Vec::new();
    for c in &out.shapes {
        walk(&c.shape, &mut v);
    }
    v
}

#[test]
fn hovering_or_pressing_a_place_button_does_not_move_the_bar() {
    // Mit dem Thema des Programms (hell, dunkel) und mit dem egui-Stil (dort wachsen Knöpfe
    // beim Überfahren um ihre Ausdehnung).
    for theme in [Some(false), Some(true), None] {
        bar_stays_put(theme);
    }
}

fn bar_stays_put(theme: Option<bool>) {
    let dir = temp_dir(&format!("leiste-{theme:?}"));
    let fav = dir.join("Ordner 01").join("Lieblingsordner");
    fs::create_dir_all(&fav).unwrap();
    let ctx = egui::Context::default();
    ctx.set_fonts(planora_app::fonts::definitions());
    if let Some(dark) = theme {
        planora_app::theme::apply(&ctx, dark);
    }
    let mut c = FileChooser::open(dir.clone(), vec!["stl"]);
    let mut shared = shared(&dir);
    shared.favorites = vec![fav.clone()];
    let mut time = 0.0;
    let mut frame = |c: &mut FileChooser, shared: &mut Places, ev: Vec<egui::Event>| {
        time += 0.05;
        bar_layout(&ctx, c, shared, ev, time)
    };
    frame(&mut c, &mut shared, vec![]);
    let rest = frame(&mut c, &mut shared, vec![]);
    assert!(rest.len() >= 6, "{rest:?}");
    // Jeden Knopf überfahren und drücken: die Lage aller Knöpfe bleibt.
    for (label, r) in rest.clone() {
        let at = r.center();
        frame(&mut c, &mut shared, vec![egui::Event::PointerMoved(at)]);
        let hovered = frame(&mut c, &mut shared, vec![]);
        assert_eq!(hovered, rest, "beim Überfahren von „{label}“");
        let pressed = frame(
            &mut c,
            &mut shared,
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(pressed, rest, "beim Drücken von „{label}“");
        frame(
            &mut c,
            &mut shared,
            vec![
                egui::Event::PointerMoved(pos2(510.0, 690.0)),
                egui::Event::PointerButton {
                    pos: pos2(510.0, 690.0),
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(&mut c, &mut shared, vec![]);
    }
    // Der Knopf des aktuellen Ordners (hervorgehoben) ist so groß wie sonst: der Favorit
    // als aktueller Ordner.
    c.enter("Ordner 01");
    c.enter("Lieblingsordner");
    assert_eq!(c.dir(), fav.as_path());
    frame(&mut c, &mut shared, vec![]);
    let current = frame(&mut c, &mut shared, vec![]);
    let size =
        |v: &[(String, Rect)], l: &str| v.iter().find(|(t, _)| t == l).map(|(_, r)| r.size());
    for (label, r) in &rest {
        if let Some(s) = size(&current, label) {
            assert_eq!(s, r.size(), "{label}");
        }
    }
    let pos = |v: &[(String, Rect)], l: &str| v.iter().find(|(t, _)| t == l).map(|(_, r)| r.min);
    for l in ["C:", "D:", "H:"] {
        assert_eq!(
            pos(&current, l),
            pos(&rest, l),
            "{l} bleibt, wenn ein Favorit aktuell ist"
        );
    }
    let _ = fs::remove_dir_all(&dir);
}
