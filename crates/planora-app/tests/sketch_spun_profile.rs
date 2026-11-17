//! Skizzen 11b, Task 14: „Drehprofil“ in der App (Spec 5, wie Fusion „Spun Profile“): Dialog am
//! Knopf „Projizieren/Einbeziehen“, Auswahl und Achse per Klick, OK legt die Kurven als ein
//! Schritt an, Abbrechen hinterlässt nichts, verlorener Bezug bietet „Verknüpfung aufheben“ statt
//! „Neu verknüpfen“. Bei 1920 × 1080.

mod common;

use common::Screen;
use egui::{Event, Modifiers};
use planora_app::detach::DIALOG_KEYS;
use planora_app::keymap::Action;
use planora_app::sketch_kinds::ToolKind;
use planora_app::sketch_state::Tool;
use planora_app::spun_profile_dialog::{SpunField, SpunFilter, SpunProfileDialog};
use planora_app::state::{AppState, IncludePick, StandardPlane};
use planora_app::surface_project_dialog::{self as spd, DialogOutcome, SurfaceDialog};
use planora_app::workspace::Workspace;
use planora_feature::{
    AxisId, BodyRef, CopySource, FeatureId, FeatureKind, IncludeRef, SketchFeature, SketchPlane,
};
use planora_kernel::BoolOp;
use planora_math::DVec3;
use planora_project::config::Config;
use planora_sketch::constraint::ConstraintKind;
use planora_sketch::model::Sketch;

const W: f32 = 1920.0;
const H: f32 = 1080.0;

// ---- Hilfen ---------------------------------------------------------------------------------------

fn cylinder(s: &mut AppState, r: f64, h: f64, z0: f64) -> FeatureId {
    s.document
        .add_feature(
            "Zylinder",
            FeatureKind::Cylinder {
                radius: r,
                height: h,
                origin: DVec3::new(0.0, 0.0, z0),
            },
        )
        .unwrap()
}

/// Welle aus zwei Zylindern um Z (R 10 von z 0 bis 30, R 5 von z 25 bis 50), vereinigt; liefert
/// das Vereinigungs-Feature.
fn shaft(s: &mut AppState) -> FeatureId {
    let a = cylinder(s, 10.0, 30.0, 0.0);
    let b = cylinder(s, 5.0, 25.0, 25.0);
    let u = s
        .document
        .add_feature(
            "Vereinigung",
            FeatureKind::Boolean {
                target: BodyRef {
                    feature: a,
                    index: 0,
                },
                tool: BodyRef {
                    feature: b,
                    index: 0,
                },
                op: BoolOp::Union,
            },
        )
        .unwrap();
    s.refresh();
    u
}

fn body_index(s: &AppState, feature: FeatureId) -> usize {
    s.bodies
        .iter()
        .position(|b| b.source.feature == feature)
        .expect("Körper sichtbar")
}

/// Eine beliebige Fläche des Körpers `body` (für den Filter „Körper“).
fn some_face(s: &AppState, body: usize) -> IncludePick {
    let (id, _) = s.bodies[body].solid.faces.iter().next().expect("Fläche");
    IncludePick::Face(body, id)
}

/// Neue Skizze auf `plane`, geöffnet und mit leerem Verlauf.
fn open_new(s: &mut AppState, plane: StandardPlane) -> FeatureId {
    let id = s.add_sketch(SketchPlane::Standard(plane)).unwrap();
    s.open_sketch(id).unwrap();
    s.observe_sketch(false);
    id
}

/// Welle, offene Skizze auf XZ und gewählter Dialog „Drehprofil“; liefert Zustand und Körper.
fn shaft_rig() -> (AppState, usize) {
    let mut s = AppState::new();
    let u = shaft(&mut s);
    open_new(&mut s, StandardPlane::XZ);
    let bi = body_index(&s, u);
    choose(&mut s);
    (s, bi)
}

fn draft(s: &AppState) -> &SketchFeature {
    s.sketch_draft.as_ref().unwrap()
}

fn sk(s: &AppState) -> &Sketch {
    s.session_sketch().unwrap()
}

fn undo_len(s: &AppState) -> usize {
    s.sketch_history().unwrap().undo_len()
}

fn curve_count(s: &AppState) -> usize {
    sk(s).curves().count()
}

/// Wählt die Art „Drehprofil“ am Knopf „Projizieren/Einbeziehen“; der Dialog öffnet.
fn choose(s: &mut AppState) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.choose_kind(sk, ToolKind::SpunProfile);
    spd::sync(s);
}

fn dialog(s: &mut AppState) -> &mut SpunProfileDialog {
    match s.surface_dialog.as_mut() {
        Some(SurfaceDialog::Spun(d)) => d,
        other => panic!("Dialog „Drehprofil“ erwartet: {other:?}"),
    }
}

/// Wählt den Körper `body` und die Ursprungsachse Z über Klicks in den Feldern.
fn fill_shaft(s: &mut AppState, body: usize) {
    assert!(spd::deliver(s, some_face(s, body)));
    dialog(s).focus = SpunField::Axis;
    assert!(spd::deliver(s, IncludePick::Axis(AxisId::Z)));
}

fn key(k: egui::Key) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

/// Ein Bild mit dem offenen Dialog und den Ereignissen; liefert das letzte Ergebnis.
fn frame(screen: &mut Screen, s: &mut AppState, events: Vec<Event>) -> Option<DialogOutcome> {
    let mut out = None;
    screen.frame(events, &mut |ctx| {
        if let Some(o) = spd::show_open(ctx, s) {
            out = Some(o);
        }
    });
    out
}

/// Alle Weltpunkte der Kurven der Skizze (Anfang, Mitte, Ende).
fn world_points(s: &AppState) -> Vec<DVec3> {
    let mut pts = Vec::new();
    for (c, _) in sk(s).curves() {
        let (k, t0, t1) = sk(s).curve_world(c).unwrap();
        for i in 0..=2 {
            pts.push(k.point(t0 + (t1 - t0) * f64::from(i) / 2.0));
        }
    }
    pts
}

// ---- Tests ----------------------------------------------------------------------------------------

#[test]
fn shaft_profile_one_step() {
    let (mut s, bi) = shaft_rig();
    fill_shaft(&mut s, bi);
    {
        let d = dialog(&mut s);
        assert_eq!(d.filter, SpunFilter::Bodies);
        assert!(d.linked && !d.project_axis && !d.flip);
        assert!((d.tolerance - 0.01).abs() < 1e-12);
        assert!(d.ok_enabled(), "{:?}", d.error());
    }
    let n = undo_len(&s);
    let mut screen = Screen::new(W, H);
    let seen = screen.settle(2, &mut |ctx| {
        spd::show_open(ctx, &mut s);
    });
    for label in [
        "Drehprofil",
        "Körper",
        "Bestimmte Elemente",
        "Achse",
        "Achse projizieren",
        "Toleranz",
        "Umkehren",
        "Projektionsverknüpfung",
    ] {
        assert!(common::find(&seen, label).is_some(), "„{label}“ fehlt");
    }
    let out = frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]);
    assert_eq!(out, Some(DialogOutcome::Applied));
    assert!(s.surface_dialog.is_none(), "der Dialog ist zu");
    assert_eq!(s.sketch_session.as_ref().unwrap().tool, Tool::Select);
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), n + 1, "ein OK, ein Schritt");
    assert_eq!(s.sketch_undo_label().as_deref(), Some("Drehprofil"));
    let copies = &draft(&s).copies;
    assert!(!copies.is_empty());
    assert!(
        copies
            .iter()
            .all(|c| matches!(c.source, CopySource::SpunProfile(_)))
    );
    let of = u32::try_from(copies.len()).unwrap();
    assert!(copies.iter().all(|c| c.of == of), "eine Gruppe");
    assert_eq!(copies.len(), curve_count(&s), "jede Kurve ist verknüpft");
    // Ohne „Umkehren“ liegt das Profil auf der Seite x < 0 (XZ mit Achse Z).
    let pts = world_points(&s);
    assert!(
        pts.iter().all(|p| p.x <= 1e-6 && p.y.abs() < 1e-6),
        "{pts:?}"
    );
    assert!(pts.iter().any(|p| p.x < -4.0));
    // Rückgängig nimmt alles.
    assert!(s.undo_sketch());
    assert_eq!(curve_count(&s), 0);
    assert!(draft(&s).copies.is_empty());
}

#[test]
fn flip_puts_profile_on_other_side() {
    let (mut s, bi) = shaft_rig();
    fill_shaft(&mut s, bi);
    {
        let d = dialog(&mut s);
        d.flip = true;
    }
    let SurfaceDialog::Spun(mut d) = s.surface_dialog.take().unwrap() else {
        panic!("Dialog");
    };
    d.refresh(&s);
    assert!(d.ok_enabled());
    s.surface_dialog = Some(SurfaceDialog::Spun(d));
    let mut screen = Screen::new(W, H);
    assert_eq!(
        frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]),
        Some(DialogOutcome::Applied)
    );
    let pts = world_points(&s);
    assert!(pts.iter().all(|p| p.x >= -1e-6), "{pts:?}");
    assert!(pts.iter().any(|p| p.x > 4.0));
}

#[test]
fn unlinked_gives_free_curves() {
    let (mut s, bi) = shaft_rig();
    fill_shaft(&mut s, bi);
    dialog(&mut s).linked = false;
    let mut screen = Screen::new(W, H);
    assert_eq!(
        frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]),
        Some(DialogOutcome::Applied)
    );
    s.observe_sketch(false);
    assert!(curve_count(&s) > 0);
    assert!(draft(&s).copies.is_empty(), "keine Herkunft");
    assert!(
        !sk(&s)
            .constraints()
            .any(|(_, c)| matches!(c.kind, ConstraintKind::Fixed(..))),
        "frei, ohne Fixierung"
    );
    // Linked dagegen fixiert.
    let (mut t, bi) = shaft_rig();
    fill_shaft(&mut t, bi);
    let mut screen = Screen::new(W, H);
    frame(&mut screen, &mut t, vec![key(egui::Key::Enter)]);
    assert!(
        sk(&t)
            .constraints()
            .any(|(_, c)| matches!(c.kind, ConstraintKind::Fixed(..)))
    );
}

#[test]
fn project_axis_adds_axis_line() {
    let (mut s, bi) = shaft_rig();
    fill_shaft(&mut s, bi);
    dialog(&mut s).project_axis = true;
    let mut screen = Screen::new(W, H);
    assert_eq!(
        frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]),
        Some(DialogOutcome::Applied)
    );
    s.observe_sketch(false);
    let copies = &draft(&s).copies;
    let spun = copies
        .iter()
        .filter(|c| matches!(c.source, CopySource::SpunProfile(_)))
        .count();
    let axis: Vec<_> = copies
        .iter()
        .filter(|c| matches!(c.source, CopySource::Include(IncludeRef::Axis(AxisId::Z))))
        .collect();
    assert!(spun > 0);
    assert_eq!(axis.len(), 1, "die Ursprungsachse kommt als Einbeziehen");
    assert_eq!(copies.len(), spun + 1);
    assert_eq!(copies.len(), curve_count(&s));
    // Ein Schritt für beides.
    assert_eq!(s.sketch_undo_label().as_deref(), Some("Drehprofil"));
}

#[test]
fn own_line_axis_gives_same_profile() {
    let (mut s, bi) = shaft_rig();
    // Eigene Linie auf der Achse Z (Skizzen-y ist bei XZ die Z-Richtung).
    let line = {
        let (_, sk) = s.session_sketch_mut().unwrap();
        let a = sk.add_point(0.0, -10.0).unwrap();
        let b = sk.add_point(0.0, 60.0).unwrap();
        sk.add_line(a, b).unwrap()
    };
    s.observe_sketch(false);
    let before = curve_count(&s);
    let id = s.open_sketch_id().unwrap();
    let f = some_face(&s, bi);
    assert!(spd::deliver(&mut s, f));
    dialog(&mut s).focus = SpunField::Axis;
    assert!(spd::deliver(
        &mut s,
        IncludePick::SketchCurve {
            sketch: id,
            curve: line
        }
    ));
    assert!(dialog(&mut s).ok_enabled());
    // Mit „Achse projizieren“ kommt für die eigene Linie nichts dazu (liegt schon da).
    dialog(&mut s).project_axis = true;
    let mut screen = Screen::new(W, H);
    assert_eq!(
        frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]),
        Some(DialogOutcome::Applied)
    );
    s.observe_sketch(false);
    let copies = &draft(&s).copies;
    assert!(
        copies
            .iter()
            .all(|c| matches!(c.source, CopySource::SpunProfile(_)))
    );
    assert_eq!(curve_count(&s), before + copies.len());
}

#[test]
fn cancel_leaves_nothing() {
    let (mut s, bi) = shaft_rig();
    fill_shaft(&mut s, bi);
    let (steps, curves) = (undo_len(&s), curve_count(&s));
    let mut screen = Screen::new(W, H);
    let out = frame(&mut screen, &mut s, vec![key(egui::Key::Escape)]);
    assert_eq!(out, Some(DialogOutcome::Cancelled));
    assert!(s.surface_dialog.is_none());
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), steps);
    assert_eq!(curve_count(&s), curves);
    assert!(draft(&s).copies.is_empty());
    assert!(s.dialog_marks.is_empty(), "Markierungen weg");
}

#[test]
fn box_shows_message_and_ok_disabled() {
    let mut s = AppState::new();
    s.create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    open_new(&mut s, StandardPlane::XZ);
    choose(&mut s);
    fill_shaft(&mut s, 0);
    {
        let d = dialog(&mut s);
        assert!(!d.ok_enabled());
        assert_eq!(
            d.error().as_deref(),
            Some("Drehprofil geht in dieser Fassung nur für Drehkörper um die gewählte Achse.")
        );
    }
    let steps = undo_len(&s);
    let mut screen = Screen::new(W, H);
    let seen = screen.settle(2, &mut |ctx| {
        spd::show_open(ctx, &mut s);
    });
    assert!(
        common::find(
            &seen,
            "Drehprofil geht in dieser Fassung nur für Drehkörper um die gewählte Achse."
        )
        .is_some(),
        "die Meldung steht im Dialog"
    );
    // Enter tut nichts, der Dialog bleibt offen.
    let out = frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]);
    assert_eq!(out, Some(DialogOutcome::Open));
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), steps);
    assert_eq!(curve_count(&s), 0);
}

#[test]
fn axis_off_plane_shows_message() {
    // Achse Y steht senkrecht auf der Skizzenebene XZ.
    let (mut s, bi) = shaft_rig();
    let f = some_face(&s, bi);
    assert!(spd::deliver(&mut s, f));
    dialog(&mut s).focus = SpunField::Axis;
    assert!(spd::deliver(&mut s, IncludePick::Axis(AxisId::Y)));
    let d = dialog(&mut s);
    assert!(!d.ok_enabled());
    assert_eq!(
        d.error().as_deref(),
        Some("Die Achse muss in der Skizzenebene liegen oder parallel zu ihr sein.")
    );
}

#[test]
fn wrong_pick_keeps_fields_and_says_why() {
    let (mut s, bi) = shaft_rig();
    // Als Achse ist eine Fläche nicht wählbar.
    dialog(&mut s).focus = SpunField::Axis;
    let f = some_face(&s, bi);
    assert!(!spd::deliver(&mut s, f));
    let d = dialog(&mut s);
    assert!(d.axis.is_none());
    assert!(d.error().is_some());
}

#[test]
fn lost_body_offers_unlink_not_relink() {
    use planora_app::lost_projections_dialog::{LostAction, LostProjectionsDialog};
    let mut w = Workspace::new(None, Config::default());
    let (sketch, u) = {
        let s = &mut w.active_tab_mut().state;
        let u = shaft(s);
        let sketch = open_new(s, StandardPlane::XZ);
        let bi = body_index(s, u);
        choose(s);
        fill_shaft(s, bi);
        let mut screen = Screen::new(W, H);
        assert_eq!(
            frame(&mut screen, s, vec![key(egui::Key::Enter)]),
            Some(DialogOutcome::Applied)
        );
        s.observe_sketch(false);
        s.close_sketch();
        (sketch, u)
    };
    w.observe_history(false);
    // Die Welle verschwindet: alle Kurven des Drehprofils verlieren ihren Bezug.
    let s = &mut w.active_tab_mut().state;
    s.delete_feature(u).unwrap();
    let lost = s.document.feature(sketch).unwrap().lost_projections().len();
    assert!(lost > 0, "verlorene Projektionen erwartet");
    assert!(s.open_lost_projections(sketch));
    let d: LostProjectionsDialog = s.lost_dialog.clone().unwrap();
    let rows = d.rows(s);
    assert_eq!(rows.len(), lost);
    assert!(rows.iter().all(|r| !r.relinkable), "kein Neu verknüpfen");
    // „Neu verknüpfen“ ist auch direkt gesperrt.
    assert!(s.start_relink(sketch, rows[0].target).is_err());
    // Ein Klick auf „Neu verknüpfen“ (der Knopf ist gesperrt) startet nichts.
    let mut dlg = d.clone();
    dlg.act(s, LostAction::Relink(rows[0].target));
    assert!(dlg.relink.is_none());
    assert!(
        s.sketch_session
            .as_ref()
            .is_none_or(|se| se.relink.is_none())
    );
    // Die Zeilen bieten „Verknüpfung aufheben“ und „Geometrie löschen“.
    let c = egui::Context::default();
    c.set_fonts(planora_app::fonts::definitions());
    planora_app::theme::apply(&c, true);
    let mut text = String::new();
    for _ in 0..2 {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(W, H),
            )),
            ..Default::default()
        };
        let out = c.run_ui(raw, |ui| {
            planora_app::lost_projections_dialog::show_open(ui.ctx(), s);
        });
        text = planora_app::testing::collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    for want in ["Verknüpfung aufheben", "Geometrie löschen"] {
        assert!(text.contains(want), "{want} fehlt:\n{text}");
    }
}

#[test]
fn dialog_has_dock_key() {
    assert!(DIALOG_KEYS.contains(&"sketch_spun_profile"));
    let d = SpunProfileDialog::default();
    assert_eq!(d.filter, SpunFilter::Bodies);
    assert!(!d.project_axis && !d.flip && d.linked);
    assert!((d.tolerance - 0.01).abs() < 1e-12);
    assert!(d.picks.is_empty() && d.axis.is_none());
    // Taste und Name wie Fusion; ohne Vorbelegung, belegbar.
    assert_eq!(ToolKind::SpunProfile.action(), Action::SketchSpunProfile);
    assert!(Action::SketchSpunProfile.default_bindings().is_empty());
    assert_eq!(ToolKind::SpunProfile.label(), "Drehprofil");
    assert_eq!(ToolKind::SpunProfile.tool(), Tool::ProjectEdge);
    assert!(ToolKind::of(Tool::ProjectEdge).contains(&ToolKind::SpunProfile));
    let en = planora_i18n::with_lang(&planora_i18n::english(), || ToolKind::SpunProfile.label());
    assert_eq!(en, "Spun Profile");
}

#[test]
fn unlinked_outline_stays_joined_when_a_corner_is_dragged() {
    let (mut s, bi) = shaft_rig();
    fill_shaft(&mut s, bi);
    dialog(&mut s).linked = false;
    let mut screen = Screen::new(W, H);
    assert_eq!(
        frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]),
        Some(DialogOutcome::Applied)
    );
    s.observe_sketch(false);
    // Weder Fixed noch Fixed3, dafür sind die Ecken des Umrisses verbunden.
    let sk0 = sk(&s);
    assert!(
        !sk0.constraints()
            .any(|(_, c)| c.kind.fixed_point().is_some()),
        "keine Fixierung"
    );
    let joints: Vec<_> = sk0
        .constraints()
        .filter_map(|(_, c)| match c.kind {
            ConstraintKind::Coincident(p, q) => Some((p, q)),
            _ => None,
        })
        .collect();
    assert!(joints.len() >= 3, "Ecken verbunden: {}", joints.len());
    // Eine Ecke ziehen (auf eine neue Lage festhalten): beide Enden folgen, nichts reißt auf.
    let (p, q) = joints[0];
    let target = (-7.0, 3.5);
    {
        let (_, sk) = s.session_sketch_mut().unwrap();
        sk.add_constraint(ConstraintKind::Fixed(p, target.0, target.1))
            .unwrap();
        sk.solve();
    }
    let sk1 = sk(&s);
    for e in [p, q] {
        let at = sk1.point_xy(e).unwrap();
        assert!(
            (at.x - target.0).abs() < 1e-6 && (at.y - target.1).abs() < 1e-6,
            "{at:?}"
        );
    }
}

#[test]
fn unlinked_offset_axis_edge_leaves_no_fixed3() {
    let mut s = AppState::new();
    s.create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    open_new(&mut s, StandardPlane::XZ);
    choose(&mut s);
    // Gerade Kante parallel zu Z bei x = 0, y = 20: parallel zur Ebene XZ, aber nicht in ihr.
    let edge = {
        let solid = &s.bodies[0].solid;
        solid
            .edges
            .iter()
            .find(|(_, e)| {
                let on = |p: DVec3| p.x.abs() < 1e-9 && (p.y - 20.0).abs() < 1e-9;
                on(solid.vertices[e.start].point) && on(solid.vertices[e.end].point)
            })
            .map(|(id, _)| id)
            .expect("Kante")
    };
    let vertex = {
        let solid = &s.bodies[0].solid;
        solid
            .edges
            .iter()
            .find(|(_, e)| {
                solid.vertices[e.start]
                    .point
                    .distance(DVec3::new(10.0, 0.0, 0.0))
                    < 1e-9
            })
            .map(|(id, _)| id)
            .expect("Ecke")
    };
    dialog(&mut s).set_filter(SpunFilter::Elements);
    assert!(spd::deliver(
        &mut s,
        IncludePick::Vertex {
            body: 0,
            edge: vertex,
            end: false
        }
    ));
    dialog(&mut s).focus = SpunField::Axis;
    assert!(spd::deliver(&mut s, IncludePick::Edge(0, edge)));
    {
        let d = dialog(&mut s);
        assert!(d.ok_enabled(), "{:?}", d.error());
        d.project_axis = true;
        d.linked = false;
    }
    let mut screen = Screen::new(W, H);
    assert_eq!(
        frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]),
        Some(DialogOutcome::Applied)
    );
    s.observe_sketch(false);
    assert!(curve_count(&s) >= 1, "die Achse ist da");
    assert!(
        !sk(&s)
            .constraints()
            .any(|(_, c)| c.kind.fixed_point().is_some()),
        "weder Fixed noch Fixed3"
    );
    assert!(draft(&s).copies.is_empty());
}

#[test]
fn body_filter_skips_axes_planes_and_origin() {
    use planora_render::{FaceRef, Pick};
    let (mut s, bi) = shaft_rig();
    assert!(planora_app::spun_profile_dialog::wants_body_only(&s));
    // Nur Körper zählen: Ebene und alles andere fallen weg, die Fläche bleibt.
    assert_eq!(
        planora_app::spun_profile_dialog::body_pick(Some(Pick::Plane(FeatureId(1)))),
        None
    );
    assert_eq!(planora_app::spun_profile_dialog::body_pick(None), None);
    let IncludePick::Face(b, f) = some_face(&s, bi) else {
        panic!("Fläche");
    };
    assert_eq!(
        planora_app::spun_profile_dialog::body_pick(Some(Pick::Face(FaceRef { body: b, face: f }))),
        Some(IncludePick::Face(b, f))
    );
    // Im Feld „Achse“ oder mit dem Filter „Elemente“ gilt das nicht.
    dialog(&mut s).focus = SpunField::Axis;
    assert!(!planora_app::spun_profile_dialog::wants_body_only(&s));
    dialog(&mut s).focus = SpunField::Selection;
    dialog(&mut s).set_filter(SpunFilter::Elements);
    assert!(!planora_app::spun_profile_dialog::wants_body_only(&s));
}

#[test]
fn rejected_click_does_not_block_ok_and_message_goes_with_next_good_click() {
    let (mut s, bi) = shaft_rig();
    fill_shaft(&mut s, bi);
    // Ein abgelehnter Klick (Fläche als Achse) lässt gültige Felder gelten: OK bleibt frei.
    dialog(&mut s).focus = SpunField::Axis;
    let f = some_face(&s, bi);
    assert!(!spd::deliver(&mut s, f));
    {
        let d = dialog(&mut s);
        assert!(d.error().is_some());
        assert!(d.ok_enabled());
    }
    // Der nächste passende Klick nimmt die Meldung weg.
    assert!(spd::deliver(&mut s, IncludePick::Axis(AxisId::Z)));
    assert!(dialog(&mut s).error().is_none());
}

#[test]
fn elements_pick_names_the_exact_reason() {
    let (mut s, _) = shaft_rig();
    let own = {
        let (_, sk) = s.session_sketch_mut().unwrap();
        let a = sk.add_point(0.0, 0.0).unwrap();
        let b = sk.add_point(0.0, 10.0).unwrap();
        sk.add_line(a, b).unwrap()
    };
    s.observe_sketch(false);
    let id = s.open_sketch_id().unwrap();
    dialog(&mut s).set_filter(SpunFilter::Elements);
    assert!(!spd::deliver(
        &mut s,
        IncludePick::SketchCurve {
            sketch: id,
            curve: own
        }
    ));
    assert_eq!(
        dialog(&mut s).error().as_deref(),
        Some("Kurven der aktiven Skizze sind nicht wählbar; bitte eine neue Skizze beginnen.")
    );
}

#[test]
fn hint_does_not_promise_a_link() {
    let de = ToolKind::SpunProfile.hint();
    assert!(!de.contains("verknüpft"), "{de}");
    let en = planora_i18n::with_lang(&planora_i18n::english(), || ToolKind::SpunProfile.hint());
    assert!(!en.contains("linked"), "{en}");
}
