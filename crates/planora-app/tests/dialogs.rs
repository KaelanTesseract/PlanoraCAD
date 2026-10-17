//! Alle Dialoge und Rückfragen im neuen Dialog-Rahmen (Etappe D, Aufgabe 3): Größenklasse, mittige
//! Lage im Fenster, Graphit-Rahmen und Überschrift, Schließen-Symbol genau dort, wo das Fenster
//! früher ein Kreuz hatte, einfache Texte, Tastatur wie vorher, Vordergrund in allen Modi und
//! Verschieben des Rundungsdialogs. Die ganze App ohne Fenster: Rohereignisse je Bild gegen einen
//! egui-Kontext.

mod common;

use eframe::App as _;
use egui::epaint::Shape;
use egui::{Color32, Event, Id, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::blend_dialog::BlendDialog;
use planora_app::boolean_dialog::BooleanDialog;
use planora_app::browser::delete_plan;
use planora_app::create_dialog::CreateDialog;
use planora_app::dialog::DialogSize;
use planora_app::dimension_browser::{CHAIN_ASK, DimDeletePlan};
use planora_app::drawing_browser::request_delete_drawing;
use planora_app::feature_dialog::FeatureDialog;
use planora_app::icons::{self, Icon};
use planora_app::shell::RightTab;
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::{After, Prompt, Workspace};
use planora_app::{drawing_browser, fonts, projects_panel, theme, versions_panel};
use planora_drawing::dimension::{
    DimId, DimKind, DimSpec, GeoRef, GeoRefData, Tolerance as DimTol,
};
use planora_drawing::{DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind};
use planora_feature::{FeatureId, SketchPlane, geo_ref_for_edge};
use planora_math::{DVec3, Tolerance};
use planora_project::config::Config;
use planora_project::library::{Library, NodePath};

const SMALL: Vec2 = vec2(800.0, 600.0);
const BIG: Vec2 = vec2(1400.0, 900.0);

// ---- Bild und Hilfen -------------------------------------------------------------------------

struct Seen {
    text: String,
    rect: Rect,
    font: egui::FontId,
}

#[derive(Default)]
struct Shot {
    texts: Vec<Seen>,
    fills: Vec<(Rect, Color32)>,
}

impl Shot {
    fn of(out: &egui::FullOutput) -> Shot {
        fn walk(shape: &Shape, s: &mut Shot) {
            match shape {
                Shape::Vec(v) => v.iter().for_each(|x| walk(x, s)),
                Shape::Text(t) => s.texts.push(Seen {
                    text: t.galley.text().to_string(),
                    rect: Rect::from_min_size(t.pos, t.galley.size()),
                    font: t
                        .galley
                        .job
                        .sections
                        .first()
                        .map_or(egui::FontId::default(), |x| x.format.font_id.clone()),
                }),
                Shape::Rect(r) => s.fills.push((r.rect, r.fill)),
                _ => {}
            }
        }
        let mut s = Shot::default();
        for c in &out.shapes {
            walk(&c.shape, &mut s);
        }
        s
    }

    fn find(&self, text: &str) -> Option<&Seen> {
        self.texts.iter().find(|t| t.text == text)
    }

    fn names(&self) -> Vec<&str> {
        self.texts.iter().map(|t| t.text.as_str()).collect()
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn edge_where(st: &AppState, body: usize, pred: impl Fn(DVec3) -> bool) -> GeoRefData {
    let solid = &st.bodies[body].solid;
    let (id, _) = solid
        .edges
        .iter()
        .find(|(_, e)| {
            let a = solid.vertices.get(e.start).unwrap().point;
            let b = solid.vertices.get(e.end).unwrap().point;
            e.start != e.end && pred(a) && pred(b)
        })
        .expect("Kante nicht gefunden");
    geo_ref_for_edge(solid, id, &Tolerance::default()).unwrap()
}

/// Die ganze App mit einem gespeicherten Dokument „P / Teil“ (zwei Versionen, zwei Quader, eine
/// Skizze, eine Zeichnung mit Maß).
struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    size: Vec2,
    time: f64,
    fid: FeatureId,
    sketch: FeatureId,
    drawing: DrawingId,
    dim: DimId,
    project: NodePath,
}

impl Rig {
    fn new(size: Vec2, dark: bool) -> Rig {
        let root = common::temp_root("planora-dialogs");
        let mut ws = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        let project = ws.library.as_ref().unwrap().create_project("P").unwrap();
        let fid = ws
            .active_tab_mut()
            .state
            .create_box(DVec3::new(100.0, 60.0, 40.0))
            .unwrap();
        ws.save_as(&project, "Teil", "Anfang").unwrap();
        let st = &mut ws.active_tab_mut().state;
        st.create_box(DVec3::new(30.0, 30.0, 30.0)).unwrap();
        let sketch = st
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        let drawing = st
            .document
            .add_drawing(
                "Blatt 1",
                Sheet {
                    paper: Paper::A4,
                    landscape: true,
                },
                Projection::First,
            )
            .unwrap();
        let front: ViewId = st
            .document
            .add_view(
                drawing,
                ViewKind::Front,
                Scale { num: 1, den: 1 },
                Placement::Free { x: 100.0, y: 100.0 },
                None,
            )
            .unwrap();
        let (lo, hi) = st.bodies[0].bounds;
        let refs = vec![
            GeoRef::Edge(edge_where(st, 0, |p| near(p.x, lo.x) && near(p.y, lo.y))),
            GeoRef::Edge(edge_where(st, 0, |p| near(p.x, hi.x) && near(p.y, lo.y))),
        ];
        let dim = st
            .document
            .add_dimension(
                drawing,
                DimSpec {
                    id: DimId(0),
                    view: front,
                    kind: DimKind::Horizontal,
                    refs,
                    offset: 10.0,
                    angle: 45.0,
                    text_shift: (0.0, 0.0),
                    tolerance: DimTol::None,
                    decimals: 1,
                    text_override: None,
                    chain: None,
                    suggested: false,
                },
            )
            .unwrap();
        st.dark = dark;
        ws.save_active("Stand 2", None).unwrap();
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        let mut rig = Rig {
            ctx,
            app: PlanoraApp::for_tests(ws),
            size,
            time: 0.0,
            fid,
            sketch,
            drawing,
            dim,
            project,
        };
        rig.app.shell_mut().show_projects();
        rig.app.shell_mut().right_tab = RightTab::Versions;
        rig
    }

    fn content(&self) -> Rect {
        Rect::from_min_size(Pos2::ZERO, self.size)
    }

    fn state(&mut self) -> &mut AppState {
        &mut self.app.ws_mut().active_tab_mut().state
    }

    fn frame(&mut self, events: Vec<Event>) -> Shot {
        self.time += 0.05;
        let raw = egui::RawInput {
            screen_rect: Some(self.content()),
            events,
            time: Some(self.time),
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        let shot = Shot::of(&out);
        out.drop_without_applying_deltas();
        shot
    }

    fn settle(&mut self, n: usize) -> Shot {
        let mut shot = Shot::default();
        for _ in 0..n {
            shot = self.frame(vec![]);
        }
        shot
    }

    fn click(&mut self, p: Pos2) -> Shot {
        let button = |pressed| Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![button(true)]);
        self.frame(vec![button(false)]);
        self.settle(2)
    }

    fn press(&mut self, key: Key) -> Shot {
        let ev = |pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![ev(true)]);
        self.frame(vec![ev(false)]);
        self.settle(2)
    }

    fn area(&self, id: Id) -> Option<Rect> {
        self.ctx.memory(|m| m.area_rect(id))
    }

    /// Zieht von `start` um `delta`.
    fn drag(&mut self, start: Pos2, delta: Vec2) {
        let btn = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![Event::PointerMoved(start)]);
        self.frame(vec![btn(start, true)]);
        let mut p = start;
        for _ in 0..6 {
            p += delta / 6.0;
            self.frame(vec![Event::PointerMoved(p)]);
        }
        self.frame(vec![btn(p, false)]);
        self.settle(2);
    }
}

// ---- Die Dialoge -----------------------------------------------------------------------------

type Open = Box<dyn Fn(&mut Rig)>;
type IsOpen = Box<dyn Fn(&mut Rig) -> bool>;

/// Ein Dialog mit seiner Öffnungsart.
struct Case {
    name: &'static str,
    /// Leer beim Feature-Dialog: sein Titel hängt am Namen des Features.
    title: &'static str,
    /// `None` beim Feature-Dialog (Id hängt am Feature).
    id: Option<Id>,
    class: DialogSize,
    /// Früher mit `.open(..)`, jetzt mit Schließen-Symbol.
    closable: bool,
    open: Open,
    is_open: IsOpen,
}

impl Case {
    fn title(&self, rig: &mut Rig) -> String {
        if self.title.is_empty() {
            let fid = rig.fid;
            let name = rig.state().document.feature(fid).unwrap().name.clone();
            format!("„{name}“ bearbeiten")
        } else {
            self.title.to_string()
        }
    }

    fn id(&self, rig: &Rig) -> Id {
        self.id
            .unwrap_or_else(|| Id::new(("feature_dialog", rig.fid)))
    }
}

fn tab_id(rig: &mut Rig) -> u64 {
    rig.app.ws_mut().active_tab().id
}

fn model_case(
    name: &'static str,
    title: &'static str,
    open: impl Fn(&mut Rig) + 'static,
    is_open: impl Fn(&mut Rig) -> bool + 'static,
) -> Case {
    Case {
        name,
        title,
        id: (!title.is_empty()).then(|| Id::new(title)),
        class: DialogSize::Normal,
        closable: true,
        open: Box::new(open),
        is_open: Box::new(is_open),
    }
}

fn prompt_case(
    name: &'static str,
    title: &'static str,
    key: &'static str,
    open: impl Fn(&mut Rig) + 'static,
) -> Case {
    Case {
        name,
        title,
        id: Some(Id::new(("planora_prompt", key))),
        class: DialogSize::Prompt,
        closable: false,
        open: Box::new(open),
        is_open: Box::new(|r| r.app.ws_mut().prompt.is_some()),
    }
}

fn project_case(
    name: &'static str,
    title: &'static str,
    key: &'static str,
    open: impl Fn(&mut Rig) + 'static,
) -> Case {
    Case {
        name,
        title,
        id: Some(Id::new(("planora_projects_dialog", key.to_string()))),
        class: DialogSize::Prompt,
        closable: false,
        open: Box::new(open),
        is_open: Box::new(|r| projects_panel::dialog_open(r.app.panels_mut().1)),
    }
}

fn prompt_set(rig: &mut Rig, p: Prompt) {
    rig.app.ws_mut().prompt = Some(p);
}

/// Alle Dialoge; die Reihenfolge benutzen `keyboard_like_before` und `case_named`.
fn cases() -> Vec<Case> {
    vec![
        model_case(
            "Quader",
            "Quader erzeugen",
            |r| {
                r.app.ws_mut().active_tab_mut().dialog = Some(CreateDialog::Box {
                    x: 10.0,
                    y: 20.0,
                    z: 30.0,
                })
            },
            |r| r.app.ws_mut().active_tab().dialog.is_some(),
        ),
        model_case(
            "Zylinder",
            "Zylinder erzeugen",
            |r| {
                r.app.ws_mut().active_tab_mut().dialog = Some(CreateDialog::Cylinder {
                    radius: 5.0,
                    height: 12.0,
                })
            },
            |r| r.app.ws_mut().active_tab().dialog.is_some(),
        ),
        model_case(
            "Extrudieren",
            "Skizze extrudieren",
            |r| {
                let sk = r.sketch;
                r.app.ws_mut().active_tab_mut().dialog = Some(CreateDialog::extrude_sketch(sk))
            },
            |r| r.app.ws_mut().active_tab().dialog.is_some(),
        ),
        model_case(
            "Drehen",
            "Skizze drehen",
            |r| {
                let sk = r.sketch;
                let tab = r.app.ws_mut().active_tab_mut();
                tab.dialog = Some(CreateDialog::revolve_sketch(&tab.state, sk))
            },
            |r| r.app.ws_mut().active_tab().dialog.is_some(),
        ),
        model_case(
            "Boolesche Operation",
            "Boolesche Operation",
            |r| {
                let tab = r.app.ws_mut().active_tab_mut();
                tab.boolean_dialog = Some(BooleanDialog::new(&tab.state))
            },
            |r| r.app.ws_mut().active_tab().boolean_dialog.is_some(),
        ),
        model_case(
            "Feature bearbeiten",
            "",
            |r| {
                let fid = r.fid;
                let tab = r.app.ws_mut().active_tab_mut();
                tab.feature_dialog = Some(FeatureDialog::new(&tab.state, fid).expect("Dialog"))
            },
            |r| r.app.ws_mut().active_tab().feature_dialog.is_some(),
        ),
        model_case(
            "Rundung",
            "Verrunden",
            |r| r.app.ws_mut().active_tab_mut().blend_dialog = Some(BlendDialog::fillet()),
            |r| r.app.ws_mut().active_tab().blend_dialog.is_some(),
        ),
        model_case(
            "Fase",
            "Fase",
            |r| r.app.ws_mut().active_tab_mut().blend_dialog = Some(BlendDialog::chamfer()),
            |r| r.app.ws_mut().active_tab().blend_dialog.is_some(),
        ),
        // Baum: Umbenennen und Rückfrage.
        Case {
            name: "Baum umbenennen",
            title: "Umbenennen",
            id: Some(Id::new("Umbenennen")),
            class: DialogSize::Prompt,
            closable: true,
            open: Box::new(|r| {
                let fid = r.fid;
                r.state().rename_draft = Some((fid, "Neuer Name".into()));
            }),
            is_open: Box::new(|r| r.state().rename_draft.is_some()),
        },
        Case {
            name: "Baum löschen",
            title: "Rückfrage",
            id: Some(Id::new("Rückfrage")),
            class: DialogSize::Prompt,
            closable: true,
            open: Box::new(|r| {
                let fid = r.fid;
                let st = r.state();
                st.pending_delete = Some(delete_plan(st, fid));
            }),
            is_open: Box::new(|r| r.state().pending_delete.is_some()),
        },
        Case {
            name: "Zeichnung umbenennen",
            title: "Umbenennen",
            id: Some(Id::new("planora-rename-drawing")),
            class: DialogSize::Prompt,
            closable: true,
            open: Box::new(|r| {
                let d = r.drawing;
                drawing_browser::request_rename(r.state(), d);
            }),
            is_open: Box::new(|r| r.state().rename_drawing_draft.is_some()),
        },
        Case {
            name: "Zeichnung löschen",
            title: "Rückfrage",
            id: Some(Id::new("planora-delete-drawing")),
            class: DialogSize::Prompt,
            closable: true,
            open: Box::new(|r| {
                let d = r.drawing;
                request_delete_drawing(r.state(), d);
            }),
            is_open: Box::new(|r| r.state().pending_delete_drawing.is_some()),
        },
        Case {
            name: "Maß löschen",
            title: "Rückfrage",
            id: Some(Id::new("planora-delete-dimension")),
            class: DialogSize::Prompt,
            closable: true,
            open: Box::new(|r| {
                let (drawing, dim) = (r.drawing, r.dim);
                r.state().pending_delete_dimension = Some(DimDeletePlan {
                    drawing,
                    dim,
                    text: CHAIN_ASK.into(),
                });
            }),
            is_open: Box::new(|r| r.state().pending_delete_dimension.is_some()),
        },
        // Projekte (fest, ohne Schließen-Symbol).
        project_case("Neuer Ordner", "Neuer Ordner", "folder", |r| {
            let p = r.project.clone();
            projects_panel::request_new_folder(r.app.panels_mut().1, p, "Ordner 1");
        }),
        project_case("Projekt umbenennen", "Umbenennen", "rename", |r| {
            let p = r.project.clone();
            projects_panel::request_rename(r.app.panels_mut().1, p, "Q");
        }),
        project_case("Projekt verschieben", "Verschieben", "move", |r| {
            let mut p = r.project.clone();
            p.push("Teil".into());
            projects_panel::request_move(r.app.panels_mut().1, p);
        }),
        project_case("Projekt löschen", "Löschen", "delete", |r| {
            let p = r.project.clone();
            projects_panel::request_delete(r.app.panels_mut().1, p);
        }),
        // Versionen (fest, ohne Schließen-Symbol).
        Case {
            name: "Kommentar ändern",
            title: "Kommentar ändern",
            id: Some(Id::new("Kommentar ändern")),
            class: DialogSize::Prompt,
            closable: false,
            open: Box::new(|r| {
                let (ws, _, v) = r.app.panels_mut();
                versions_panel::request_comment(ws, v, 2, "Stand 2");
            }),
            is_open: Box::new(|r| versions_panel::has_pending(r.app.panels_mut().2)),
        },
        Case {
            name: "Version löschen",
            title: "Löschen",
            id: Some(Id::new("Löschen")),
            class: DialogSize::Prompt,
            closable: false,
            open: Box::new(|r| {
                let (ws, _, v) = r.app.panels_mut();
                versions_panel::request_delete_version(ws, v, 2);
            }),
            is_open: Box::new(|r| versions_panel::has_pending(r.app.panels_mut().2)),
        },
        // Rückfragen des Arbeitsbereichs.
        prompt_case(
            "Änderungen speichern",
            "Änderungen speichern?",
            "save_changes",
            |r| {
                let tab = tab_id(r);
                prompt_set(
                    r,
                    Prompt::SaveChanges {
                        tab,
                        then: After::CloseTab,
                    },
                );
            },
        ),
        prompt_case(
            "Version speichern",
            "Version speichern",
            "save_comment",
            |r| {
                let tab = tab_id(r);
                prompt_set(
                    r,
                    Prompt::SaveComment {
                        tab,
                        then: None,
                        comment: "Kommentar".into(),
                        branch: Some(String::new()),
                    },
                );
            },
        ),
        prompt_case("Speichern unter", "Speichern unter", "save_as", |r| {
            let tab = tab_id(r);
            let parent = r.project.clone();
            prompt_set(
                r,
                Prompt::SaveAs {
                    tab,
                    then: None,
                    parent,
                    name: "Teil 2".into(),
                    comment: "Anfang".into(),
                },
            );
        }),
        prompt_case("Dokument gesperrt", "Dokument gesperrt", "take_over", |r| {
            let mut path = r.project.clone();
            path.push("Teil".into());
            prompt_set(
                r,
                Prompt::TakeOver {
                    path,
                    version: None,
                },
            );
        }),
        prompt_case(
            "Autosicherung",
            "Autosicherung gefunden",
            "autosave_offer",
            |r| {
                let tab = tab_id(r);
                prompt_set(r, Prompt::AutosaveOffer { tab });
            },
        ),
        prompt_case("Hinweis", "Hinweis", "notice", |r| {
            prompt_set(r, Prompt::Notice("Etwas ist schiefgegangen.".into()));
        }),
    ]
}

fn case_named(name: &str) -> Case {
    cases()
        .into_iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("Dialog {name} fehlt"))
}

/// Rig mit geöffnetem Dialog, eingeschwungen. Liefert das letzte Bild und die Fläche des Dialogs.
fn opened(case: &Case, size: Vec2, dark: bool) -> (Rig, Shot, Rect) {
    let mut rig = Rig::new(size, dark);
    // Erst ein Bild ohne Dialog (Felder, Theme), dann öffnen.
    rig.settle(3);
    (case.open)(&mut rig);
    let shot = rig.settle(6);
    let id = case.id(&rig);
    let rect = rig.area(id).unwrap_or_else(|| {
        panic!(
            "{}: Dialog nicht gezeichnet ({:?})",
            case.name,
            shot.names()
        )
    });
    (rig, shot, rect)
}

fn for_all(mut check: impl FnMut(&Case, &mut Rig, &Shot, Rect, bool)) {
    for case in cases() {
        for dark in [true, false] {
            for size in [SMALL, BIG] {
                let (mut rig, shot, rect) = opened(&case, size, dark);
                check(&case, &mut rig, &shot, rect, dark);
            }
        }
    }
}

// ---- Tests -----------------------------------------------------------------------------------

#[test]
fn every_dialog_has_its_size_class() {
    for_all(|case, _, _, rect, _| {
        assert!(
            (rect.width() - case.class.width()).abs() <= 1.0,
            "{}: Breite {} statt {}",
            case.name,
            rect.width(),
            case.class.width()
        );
    });
}

#[test]
fn every_dialog_is_centred_and_inside_the_window() {
    for_all(|case, rig, _, rect, _| {
        let content = rig.content();
        let c = content.center();
        assert!(
            (rect.center() - c).abs().max_elem() <= 1.0,
            "{}: Mitte {:?} statt {c:?} ({:?})",
            case.name,
            rect.center(),
            rig.size
        );
        assert!(
            content.shrink(16.0).expand(0.5).contains_rect(rect),
            "{}: {rect:?} nicht in {:?}",
            case.name,
            content.shrink(16.0)
        );
    });
}

#[test]
fn every_dialog_uses_the_graphit_frame_and_heading() {
    for_all(|case, rig, shot, rect, dark| {
        let p = theme::palette(dark);
        assert!(
            shot.fills.iter().any(|(r, fill)| *fill == p.bg_panel
                && r.expand(2.0).contains_rect(rect)
                && rect.expand(2.0).contains_rect(*r)),
            "{}: keine Fläche bg_panel um {rect:?}",
            case.name
        );
        let title = case.title(rig);
        assert!(
            shot.texts
                .iter()
                .any(|t| t.text == title && t.font == theme::heading_font()),
            "{}: Titel „{title}“ nicht in der Überschrift: {:?}",
            case.name,
            shot.names()
        );
    });
}

fn close_glyph() -> String {
    icons::glyph(Icon::Close).to_string()
}

/// Schließen-Symbole im Dialog (andere Felder haben auch welche).
fn close_icons_in(shot: &Shot, rect: Rect) -> Vec<Rect> {
    let g = close_glyph();
    shot.texts
        .iter()
        .filter(|t| t.text == g && rect.expand(2.0).contains(t.rect.center()))
        .map(|t| t.rect)
        .collect()
}

#[test]
fn close_icon_where_the_window_had_one() {
    for case in cases() {
        let (mut rig, shot, rect) = opened(&case, BIG, true);
        let icons = close_icons_in(&shot, rect);
        if !case.closable {
            assert!(icons.is_empty(), "{}: Schließen-Symbol da", case.name);
            continue;
        }
        assert_eq!(icons.len(), 1, "{}: {:?}", case.name, shot.names());
        assert!(
            (case.is_open)(&mut rig),
            "{}: vor dem Klick offen",
            case.name
        );
        rig.click(icons[0].center());
        assert!(!(case.is_open)(&mut rig), "{}: Klick schließt", case.name);
    }
}

#[test]
fn dialog_texts_are_plain() {
    const FORBIDDEN: [&str; 12] = [
        "Spec",
        "egui",
        "Kernel",
        "Autodesk",
        "Fusion",
        "Graphit",
        "Phosphor",
        "Montserrat",
        "Roboto",
        "TODO",
        "Debug",
        "Datei stammt",
    ];
    for_all(|case, _, shot, rect, _| {
        for t in shot
            .texts
            .iter()
            .filter(|t| rect.expand(2.0).contains(t.rect.center()))
        {
            for w in FORBIDDEN {
                assert!(
                    !t.text.contains(w),
                    "{}: „{}“ enthält „{w}“",
                    case.name,
                    t.text
                );
            }
        }
    });
}

/// Mitte des letzten Textes `text` im Bild (das Textfeld steht nach dem Beschriftungstext).
fn last_text_pos(shot: &Shot, text: &str) -> Pos2 {
    shot.texts
        .iter()
        .filter(|t| t.text == text)
        .map(|t| t.rect.center())
        .next_back()
        .unwrap_or_else(|| panic!("„{text}“ fehlt: {:?}", shot.names()))
}

#[test]
fn keyboard_like_before() {
    // Umbenennen im Baum: Enter im Textfeld übernimmt.
    let case = case_named("Baum umbenennen");
    let (mut rig, shot, _) = opened(&case, BIG, true);
    rig.click(last_text_pos(&shot, "Neuer Name"));
    rig.press(Key::Enter);
    assert!(rig.state().rename_draft.is_none());
    let fid = rig.fid;
    assert_eq!(
        rig.state().document.feature(fid).unwrap().name,
        "Neuer Name"
    );

    // Umbenennen in Zeichnungen.
    let case = case_named("Zeichnung umbenennen");
    let (mut rig, shot, _) = opened(&case, BIG, true);
    let name = rig.state().rename_drawing_draft.clone().unwrap().1;
    rig.click(last_text_pos(&shot, &name));
    rig.press(Key::Enter);
    assert!(rig.state().rename_drawing_draft.is_none());

    // Neuer Ordner (Projekte).
    let case = case_named("Neuer Ordner");
    let (mut rig, shot, _) = opened(&case, BIG, true);
    rig.click(last_text_pos(&shot, "Ordner 1"));
    rig.press(Key::Enter);
    assert!(!(case.is_open)(&mut rig), "Neuer Ordner: Enter übernimmt");

    // Kommentar ändern (Versionen).
    let case = case_named("Kommentar ändern");
    let (mut rig, shot, _) = opened(&case, BIG, true);
    rig.click(last_text_pos(&shot, "Stand 2"));
    rig.press(Key::Enter);
    assert!(
        !(case.is_open)(&mut rig),
        "Kommentar ändern: Enter übernimmt"
    );

    // Rückfrage „Änderungen speichern?“: weder Schließen-Symbol noch Escape schließen sie.
    let case = case_named("Änderungen speichern");
    let (mut rig, shot, rect) = opened(&case, BIG, true);
    assert!(close_icons_in(&shot, rect).is_empty());
    rig.press(Key::Escape);
    assert!(
        (case.is_open)(&mut rig),
        "Escape schließt die Rückfrage nicht"
    );
}

#[test]
fn dialogs_in_all_modes_above_fields() {
    for mode in ["Modell", "Zeichnung", "Skizze"] {
        let mut rig = Rig::new(SMALL, false);
        match mode {
            "Zeichnung" => {
                let d = rig.drawing;
                rig.state().drawing_ui.open(d);
            }
            "Skizze" => {
                let sk = rig.sketch;
                rig.state().open_sketch(sk).unwrap();
            }
            _ => {}
        }
        rig.settle(4);
        // Ein Hinweis ist in jedem Modus möglich, der Quader-Dialog nur außerhalb der Skizze.
        let mut ids = vec![Id::new(("planora_prompt", "notice"))];
        rig.app.ws_mut().prompt = Some(Prompt::Notice("Hinweis".into()));
        if mode != "Skizze" {
            rig.app.ws_mut().active_tab_mut().dialog = Some(CreateDialog::Box {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            });
            ids.push(Id::new("Quader erzeugen"));
        }
        rig.settle(6);
        for id in ids {
            let rect = rig
                .area(id)
                .unwrap_or_else(|| panic!("{mode}: Dialog {id:?} fehlt"));
            let r = rect.shrink(6.0);
            for p in [
                r.center(),
                r.left_top(),
                r.right_top(),
                r.left_bottom(),
                r.right_bottom(),
            ] {
                // Mit Rückfrage liegt der Hinweis über dem Quader-Dialog: es zählt jede
                // Vordergrund-Ebene, nie ein Feld (Middle).
                let top = rig.ctx.layer_id_at(p);
                assert!(
                    top.is_some_and(|l| l.order == egui::Order::Foreground),
                    "{mode}: an {p:?} liegt {top:?}"
                );
            }
        }
    }
}

#[test]
fn blend_dialog_can_be_moved_and_edges_stay_clickable() {
    let mut rig = Rig::new(BIG, true);
    rig.settle(4);
    rig.app.ws_mut().active_tab_mut().blend_dialog = Some(BlendDialog::fillet());
    rig.settle(6);
    let id = Id::new(BlendDialog::fillet().title());
    let rect = rig.area(id).expect("Dialog");

    // Eine Kante des Quaders, deren Mitte unter dem Dialog liegt und die man sonst anklicken könnte.
    let viewport = glam::Vec2::new(rig.size.x, rig.size.y);
    let candidates: Vec<Pos2> = {
        let st = rig.state();
        let cam = st.camera;
        let solid = &st.bodies[0].solid;
        solid
            .edges
            .iter()
            .filter_map(|(eid, e)| {
                let a = solid.vertices.get(e.start)?.point;
                let b = solid.vertices.get(e.end)?.point;
                let mid = ((a + b) * 0.5).as_vec3();
                let s = cam.project(mid, viewport)?;
                match st.pick(s, viewport) {
                    Some(planora_render::Pick::Edge(r)) if r.edge == eid && r.body == 0 => {
                        Some(pos2(s.x, s.y))
                    }
                    _ => None,
                }
            })
            .collect()
    };
    let target = *candidates
        .iter()
        .find(|p| rect.contains(**p))
        .unwrap_or_else(|| panic!("keine anklickbare Kante unter {rect:?}: {candidates:?}"));

    // Vor dem Verschieben fängt der Dialog den Klick ab.
    rig.click(target);
    let shot = rig.settle(2);
    assert!(
        shot.find("Gewählte Kanten: 1").is_none(),
        "Klick durch den Dialog hindurch"
    );

    // An der Dialogfläche (Kopfzeile, rechts vom Titel) nach links oben ziehen.
    let grab = pos2(rect.left() + rect.width() * 0.6, rect.top() + 14.0);
    rig.drag(grab, vec2(-2000.0, -2000.0));
    let moved = rig.area(id).expect("Dialog");
    assert!(moved.center().x < rect.center().x - 100.0, "{moved:?}");
    assert!(!moved.contains(target), "Kante noch verdeckt: {moved:?}");

    rig.click(target);
    let shot = rig.settle(2);
    assert!(
        shot.find("Gewählte Kanten: 1").is_some(),
        "Kante nicht gewählt: {:?}",
        shot.names()
    );
}
