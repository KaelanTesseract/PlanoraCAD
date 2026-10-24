use planora_app::state::AppState;
use planora_app::{browser, fonts};
use planora_feature::FeatureStatus;
use planora_kernel::BoolOp;
use planora_math::DVec3;

fn ctx() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    c
}

fn texts(out: &egui::FullOutput) -> String {
    // sammelt alle Texte der gezeichneten Formen (rekursiv, wie in sketch_dialogs.rs)
    planora_app::testing::collect_texts(out)
}

#[test]
fn the_tree_lists_bodies_and_no_marker_line() {
    // Seit der Verlaufsleiste (Aufgabe 6) zeigt der Baum Körper statt Schritte: Nach dem
    // Vereinigen gibt es eine Zeile für das Ergebnis, die Eingangskörper erscheinen nicht mehr
    // einzeln. Der globale Marker hat keine eigene Linie mehr im Baum.
    let mut s = AppState::new();
    s.create_box(DVec3::splat(4.0)).unwrap();
    s.create_cylinder(1.0, 4.0).unwrap();
    s.add_boolean(0, 1, BoolOp::Union).unwrap();
    let c = ctx();
    let mut text = String::new();
    for _ in 0..3 {
        let out = c.run_ui(egui::RawInput::default(), |ui| browser::show(ui, &mut s));
        text = texts(&out);
        out.drop_without_applying_deltas();
    }
    let union = s.bodies[0].name.clone();
    assert!(text.contains(&union), "„{union}“ fehlt:\n{text}");
    assert!(!text.contains("Zylinder 1"), "{text}");
    s.document.set_marker(2).unwrap();
    s.refresh();
    let out = c.run_ui(egui::RawInput::default(), |ui| browser::show(ui, &mut s));
    let text = texts(&out);
    out.drop_without_applying_deltas();
    // Das Vereinigen ist zurückgestellt: beide Körper sind wieder einzeln da.
    for must in ["Quader 1", "Zylinder 1"] {
        assert!(text.contains(must), "„{must}“ fehlt:\n{text}");
    }
    assert!(!text.contains("Berechnung endet hier"), "{text}");
}

#[test]
fn an_error_row_carries_its_reason_and_dependents_are_marked() {
    let mut s = AppState::new();
    let bad = s
        .add_feature(
            None,
            planora_feature::FeatureKind::Cylinder {
                radius: -1.0,
                height: 2.0,
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    assert!(matches!(
        s.document.feature(bad).unwrap().status,
        FeatureStatus::Error(_)
    ));
    assert!(s.message.is_some(), "Fehler steht in der Statuszeile");
}

#[test]
fn delete_with_consequences_asks_first_and_then_removes_everything() {
    let mut s = AppState::new();
    let a = s.create_box(DVec3::splat(4.0)).unwrap();
    s.create_cylinder(1.0, 4.0).unwrap();
    s.add_boolean(0, 1, BoolOp::Union).unwrap();
    let plan = browser::delete_plan(&s, a);
    assert_eq!(plan.dependents.len(), 1);
    assert!(plan.text().contains("Quader 1") && plan.text().contains("Boolesche Operation"));
    browser::apply_delete(&mut s, &plan).unwrap();
    assert!(s.document.feature(a).is_none());
    assert!(
        s.document
            .features()
            .iter()
            .all(|f| f.name != "Boolesche Operation 1")
    );
}

#[test]
fn context_actions_work_on_the_state_level() {
    let mut s = AppState::new();
    let a = s.create_box(DVec3::splat(4.0)).unwrap();
    s.create_box(DVec3::splat(2.0)).unwrap();
    browser::toggle_suppress(&mut s, a).unwrap();
    assert!(s.document.feature(a).unwrap().suppressed);
    assert_eq!(s.bodies.len(), 1);
    browser::toggle_suppress(&mut s, a).unwrap();
    assert_eq!(s.bodies.len(), 2);
    browser::roll_back_to(&mut s, a).unwrap();
    assert_eq!(s.document.marker(), 1);
    assert_eq!(s.bodies.len(), 1);
    browser::roll_to_end(&mut s).unwrap();
    assert_eq!(s.document.marker(), 2);
}

// ---- Zusätzliche Tests (über den Brief hinaus) ----------------------------------------------

use planora_feature::{SketchPlane, StandardPlane};
use planora_math::DVec2;

const SCREEN: egui::Rect = egui::Rect {
    min: egui::pos2(0.0, 0.0),
    max: egui::pos2(800.0, 600.0),
};

fn raw(events: Vec<egui::Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(SCREEN),
        events,
        ..Default::default()
    }
}

fn click(pos: egui::Pos2, button: egui::PointerButton) -> Vec<egui::Event> {
    let press = |pressed| egui::Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: egui::Modifiers::default(),
    };
    vec![egui::Event::PointerMoved(pos), press(true), press(false)]
}

/// Mitte des ersten Textes, der mit `prefix` beginnt.
fn find_text(out: &egui::FullOutput, prefix: &str) -> Option<egui::Pos2> {
    fn walk(shape: &egui::epaint::Shape, prefix: &str) -> Option<egui::Pos2> {
        match shape {
            egui::epaint::Shape::Text(t) if t.galley.text().starts_with(prefix) => {
                Some(t.visual_bounding_rect().center())
            }
            egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| walk(s, prefix)),
            _ => None,
        }
    }
    out.shapes.iter().find_map(|c| walk(&c.shape, prefix))
}

/// Ein Bild des Baums mit `events`; liefert Texte und die Lage von `prefix` (falls gezeichnet).
fn frame(
    c: &egui::Context,
    s: &mut AppState,
    events: Vec<egui::Event>,
    prefix: &str,
) -> (String, Option<egui::Pos2>) {
    let out = c.run_ui(raw(events), |ui| browser::show(ui, s));
    let r = (texts(&out), find_text(&out, prefix));
    out.drop_without_applying_deltas();
    r
}

/// Rechtsklick auf die Zeile `row` und Klick auf den Menüeintrag `entry`.
fn context_menu(s: &mut AppState, row: &str, entry: &str) {
    let c = ctx();
    let (_, pos) = frame(&c, s, vec![], row);
    let pos = pos.unwrap_or_else(|| panic!("Zeile {row}"));
    frame(&c, s, click(pos, egui::PointerButton::Secondary), "");
    let (text, item) = frame(&c, s, vec![], entry);
    let item = item.unwrap_or_else(|| panic!("Menüeintrag {entry}:\n{text}"));
    frame(&c, s, click(item, egui::PointerButton::Primary), "");
}

fn sketch_with_rectangle(s: &mut AppState) -> planora_feature::FeatureId {
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    s.session_sketch_mut()
        .unwrap()
        .1
        .add_rectangle(DVec2::ZERO, DVec2::new(4.0, 3.0))
        .unwrap();
    s.close_sketch();
    id
}

#[test]
fn the_context_menu_suppresses_and_activates() {
    // „Zurückrollen bis hier“ und „Zum Ende“ gibt es im Baum nicht mehr (Verlaufsleiste,
    // Aufgabe 6); der globale Marker steht im Menü „Bearbeiten“.
    let mut s = AppState::new();
    let a = s.create_box(DVec3::splat(4.0)).unwrap();
    s.create_box(DVec3::splat(2.0)).unwrap();
    context_menu(&mut s, "Quader 1", "Unterdrücken");
    assert!(s.document.feature(a).unwrap().suppressed);
    let c = ctx();
    let (text, _) = frame(&c, &mut s, vec![], "");
    assert!(text.contains("unterdrückt"), "{text}");
    context_menu(&mut s, "Quader 1", "Aktivieren");
    assert!(!s.document.feature(a).unwrap().suppressed);
    let menu = menu_texts(&mut s, "Quader 1");
    assert!(!menu.contains("Zurückrollen bis hier"), "{menu}");
    assert!(!menu.contains("Zum Ende"), "{menu}");
}

#[test]
fn deleting_with_consequences_from_the_menu_asks_first() {
    // Die Körperzeile „Quader 1“ löscht den Ursprung des Körpers samt Verschieben.
    let mut s = AppState::new();
    let a = s.create_box(DVec3::splat(4.0)).unwrap();
    s.move_body(0, DVec3::X).unwrap();
    s.create_box(DVec3::splat(2.0)).unwrap();
    context_menu(&mut s, "Quader 1", "Löschen");
    assert!(s.document.feature(a).is_some(), "erst fragen");
    assert!(s.pending_delete.is_some());
    let c = ctx();
    // Ein neues Fenster wird im ersten Bild nur vermessen.
    frame(&c, &mut s, vec![], "");
    let (text, _) = frame(&c, &mut s, vec![], "");
    assert!(
        text.contains("davon abhängige Feature(s) werden gelöscht"),
        "{text}"
    );
    // „Abbrechen“ lässt alles stehen
    let (_, cancel) = frame(&c, &mut s, vec![], "Abbrechen");
    frame(
        &c,
        &mut s,
        click(cancel.unwrap(), egui::PointerButton::Primary),
        "",
    );
    assert!(s.pending_delete.is_none());
    assert_eq!(s.document.features().len(), 3);
    // noch einmal, diesmal bestätigen
    context_menu(&mut s, "Quader 1", "Löschen");
    frame(&c, &mut s, vec![], "");
    let (text, confirm) = frame(&c, &mut s, vec![], "Löschen");
    // Der erste Text „Löschen“ ist die Schaltfläche im Rückfragefenster.
    frame(
        &c,
        &mut s,
        click(confirm.unwrap(), egui::PointerButton::Primary),
        "",
    );
    assert!(s.document.feature(a).is_none(), "{text}");
    assert_eq!(s.document.features().len(), 1);
}

#[test]
fn deleting_without_consequences_happens_at_once() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(4.0)).unwrap();
    s.create_box(DVec3::splat(2.0)).unwrap();
    context_menu(&mut s, "Quader 2", "Löschen");
    assert!(s.pending_delete.is_none());
    assert_eq!(s.document.features().len(), 1);
}

#[test]
fn renaming_from_the_menu_opens_a_small_window() {
    let mut s = AppState::new();
    let a = s.create_box(DVec3::splat(4.0)).unwrap();
    context_menu(&mut s, "Quader 1", "Umbenennen");
    assert_eq!(s.rename_draft.as_ref().map(|r| r.0), Some(a));
    s.rename_draft.as_mut().unwrap().1 = "Grundplatte".into();
    browser::apply_rename(&mut s).unwrap();
    assert_eq!(s.document.feature(a).unwrap().name, "Grundplatte");
    assert_eq!(s.bodies[0].name, "Grundplatte");
    assert!(s.rename_draft.is_none());
}

#[test]
fn the_tree_shows_error_and_grey_states_and_body_rows_select_bodies() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(4.0)).unwrap();
    s.add_feature(
        None,
        planora_feature::FeatureKind::Cylinder {
            radius: -1.0,
            height: 2.0,
            origin: DVec3::ZERO,
        },
    )
    .unwrap();
    let c = ctx();
    let (text, _) = frame(&c, &mut s, vec![], "");
    assert!(text.contains("Fehler"), "{text}");
    assert!(browser::status_text(&s).is_some_and(|t| t.contains("Zylinder 1")));
    // Eine Zeile je Körper (keine eigene Schrittzeile mehr, Aufgabe 6).
    s.selected_body = None;
    let out = c.run_ui(raw(vec![]), |ui| browser::show(ui, &mut s));
    let mut rows = Vec::new();
    fn walk(shape: &egui::epaint::Shape, rows: &mut Vec<egui::Pos2>) {
        match shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == "Quader 1" => {
                rows.push(t.visual_bounding_rect().center())
            }
            egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, rows)),
            _ => {}
        }
    }
    out.shapes.iter().for_each(|c| walk(&c.shape, &mut rows));
    out.drop_without_applying_deltas();
    assert_eq!(rows.len(), 1, "eine Körperzeile");
    frame(&c, &mut s, click(rows[0], egui::PointerButton::Primary), "");
    assert_eq!(s.selected_body, Some(0));
    assert_eq!(s.selected_feature, None);
    assert_eq!(s.history_body(), Some(s.document.features()[0].id));
}

#[test]
fn suppressing_or_rolling_back_the_open_sketch_asks_to_end_it_first() {
    let mut s = AppState::new();
    let sk = sketch_with_rectangle(&mut s);
    s.open_sketch(sk).unwrap();
    let e = browser::toggle_suppress(&mut s, sk).unwrap_err();
    assert!(e.contains("Zuerst die Skizze beenden"), "{e}");
    assert!(browser::roll_back_to(&mut s, sk).is_err());
    assert!(s.sketch_session.is_some(), "Skizze bleibt offen");
    assert!(!s.document.feature(sk).unwrap().suppressed);
}

#[test]
fn other_document_actions_end_the_open_sketch_and_keep_its_draft() {
    let mut s = AppState::new();
    let a = s.create_box(DVec3::splat(4.0)).unwrap();
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    s.session_sketch_mut()
        .unwrap()
        .1
        .add_rectangle(DVec2::ZERO, DVec2::new(4.0, 3.0))
        .unwrap();
    browser::toggle_suppress(&mut s, a).unwrap();
    assert!(s.sketch_session.is_none(), "Skizze beendet");
    assert_eq!(
        s.document.sketch_of(sk).unwrap().curves().count(),
        4,
        "Entwurf übernommen"
    );
    // Verschieben im Eigenschaftenfenster beendet die Skizze ebenso
    browser::toggle_suppress(&mut s, a).unwrap();
    s.open_sketch(sk).unwrap();
    let p = s
        .session_sketch_mut()
        .unwrap()
        .1
        .add_point(7.0, 7.0)
        .unwrap();
    s.move_body(0, DVec3::X).unwrap();
    assert!(s.sketch_session.is_none());
    let sketch = s.document.sketch_of(sk).unwrap();
    assert_eq!(sketch.point_xy(p).ok(), Some(DVec2::new(7.0, 7.0)));
}

#[test]
fn the_delete_note_mentions_face_sketches_of_a_union() {
    let mut s = AppState::new();
    s.create_box(DVec3::new(10.0, 10.0, 4.0)).unwrap();
    s.create_box(DVec3::new(10.0, 10.0, 4.0)).unwrap();
    s.move_body(1, DVec3::new(-15.0, 0.0, 2.0)).unwrap();
    let u = s.add_boolean(0, 1, BoolOp::Union).unwrap();
    let top = s.bodies[0]
        .solid
        .faces
        .iter()
        .find(|(id, _)| {
            planora_sketch::plane_of_face(&s.bodies[0].solid, *id)
                .is_some_and(|f| f.z.z > 0.5 && (f.origin.z - 4.0).abs() < 1e-9)
        })
        .map(|(id, _)| id)
        .expect("Deckfläche des ersten Quaders");
    s.new_sketch_on_face(0, top).unwrap();
    let plan = browser::delete_plan(&s, u);
    assert!(plan.dependents.is_empty());
    assert!(
        plan.needs_confirmation(),
        "Hinweis zur Skizze auf der Fläche"
    );
    assert!(plan.text().contains("Grundkörper"), "{}", plan.text());
}

#[test]
fn an_edge_copy_reference_goes_away_with_its_source_feature() {
    // Die Kopie steht nur im Entwurf; „Löschen“ beendet die Skizze vorher, sodass die Rückfrage
    // die Skizze als Folge nennt und danach nichts mehr auf den Quader verweist.
    let mut s = AppState::new();
    let a = s.create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    let solid = s.bodies[0].solid.clone();
    assert!(
        solid
            .edges
            .iter()
            .any(|(e, _)| s.project_edge_into_sketch(0, e).is_ok())
    );
    assert!(
        s.document.dependents(a).is_empty(),
        "nur im Entwurf: das Dokument kennt die Kopie noch nicht"
    );
    browser::request_delete(&mut s, a);
    assert!(s.sketch_session.is_none(), "Skizze vorher beendet");
    let plan = s.pending_delete.clone().expect("Rückfrage");
    assert!(plan.text().contains("Skizze 1"), "{}", plan.text());
    browser::apply_delete(&mut s, &plan).unwrap();
    assert!(
        s.document
            .features()
            .iter()
            .all(|f| !f.kind.dependencies().contains(&a))
    );
    s.document.check_invariants().unwrap();
}

// ---- Fix-Runde 1 ------------------------------------------------------------------------------

#[test]
fn a_stale_delete_confirmation_is_refreshed_instead_of_deleting() {
    let mut s = AppState::new();
    let x = s.create_box(DVec3::splat(10.0)).unwrap();
    s.create_cylinder(2.0, 14.0).unwrap();
    s.move_body(1, DVec3::new(-17.0, 5.0, -2.0)).unwrap();
    s.add_boolean(0, 1, BoolOp::Union).unwrap();
    browser::request_delete(&mut s, x);
    let shown = s.pending_delete.clone().expect("Rückfrage");
    assert!(!shown.text().contains("Skizze 1"));
    // Während die Rückfrage offen ist: Skizze S anlegen und eine Kante von X hineinkopieren.
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    let solid = s.bodies[0].solid.clone();
    let mut copied = false;
    for (e, _) in solid.edges.iter() {
        if s.project_edge_into_sketch(0, e).is_ok() {
            let c = s.sketch_draft.as_ref().unwrap().copies.last().unwrap();
            if c.edge.a.origin == x.0 || c.edge.b.origin == x.0 {
                copied = true;
                break;
            }
        }
    }
    assert!(copied, "eine Kante von X kopiert");
    // „Löschen“ im Rückfragefenster
    let c = ctx();
    frame(&c, &mut s, vec![], "");
    let (_, confirm) = frame(&c, &mut s, vec![], "Löschen");
    frame(
        &c,
        &mut s,
        click(confirm.unwrap(), egui::PointerButton::Primary),
        "",
    );
    assert!(s.document.feature(x).is_some(), "nichts gelöscht");
    assert!(s.document.feature(sk).is_some());
    let plan = s.pending_delete.clone().expect("Rückfrage mit neuer Liste");
    assert!(plan.dependents.contains(&sk), "{}", plan.text());
    assert!(plan.text().contains("Skizze 1"), "{}", plan.text());
    // Jetzt bestätigt: alles weg
    browser::apply_delete(&mut s, &plan).unwrap();
    assert!(s.document.feature(x).is_none() && s.document.feature(sk).is_none());
}

/// Zwei Klicks an derselben Stelle kurz nacheinander (Doppelklick) ab der Zeit `t0` (s).
fn double_click(c: &egui::Context, s: &mut AppState, pos: egui::Pos2, t0: f64) {
    for (i, events) in [
        click(pos, egui::PointerButton::Primary),
        click(pos, egui::PointerButton::Primary),
        vec![],
    ]
    .into_iter()
    .enumerate()
    {
        let input = egui::RawInput {
            screen_rect: Some(SCREEN),
            time: Some(t0 + 0.05 * i as f64),
            events,
            ..Default::default()
        };
        c.run_ui(input, |ui| browser::show(ui, s))
            .drop_without_applying_deltas();
    }
}

#[test]
fn a_double_click_opens_the_sketch_or_asks_for_the_edit_dialog() {
    let mut s = AppState::new();
    let b = s.create_box(DVec3::splat(4.0)).unwrap();
    let sk = sketch_with_rectangle(&mut s);
    let c = ctx();
    let (_, row) = frame(&c, &mut s, vec![], "Skizze 1");
    double_click(&c, &mut s, row.unwrap(), 1.0);
    assert_eq!(s.open_sketch_id(), Some(sk));
    s.close_sketch();
    let (_, row) = frame(&c, &mut s, vec![], "Quader 1");
    double_click(&c, &mut s, row.unwrap(), 10.0);
    assert_eq!(s.edit_request, Some(b));
}

/// Texte des Kontextmenüs der Zeile `row`.
fn menu_texts(s: &mut AppState, row: &str) -> String {
    let c = ctx();
    let (_, pos) = frame(&c, s, vec![], row);
    frame(
        &c,
        s,
        click(pos.unwrap(), egui::PointerButton::Secondary),
        "",
    );
    frame(&c, s, vec![], "").0
}

#[test]
fn only_sketch_rows_offer_extrude_and_revolve() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(4.0)).unwrap();
    sketch_with_rectangle(&mut s);
    let sketch_menu = menu_texts(&mut s, "Skizze 1");
    for must in [
        "Extrudieren …",
        "Drehen …",
        "Bearbeiten",
        "Umbenennen",
        "Löschen",
    ] {
        assert!(sketch_menu.contains(must), "„{must}“ fehlt:\n{sketch_menu}");
    }
    let box_menu = menu_texts(&mut s, "Quader 1");
    assert!(box_menu.contains("Unterdrücken"), "{box_menu}");
    assert!(!box_menu.contains("Extrudieren"), "{box_menu}");
    assert!(!box_menu.contains("Drehen"), "{box_menu}");
}

#[test]
fn error_rows_explain_themselves_and_suppressed_rows_are_grey() {
    let mut s = AppState::new();
    s.add_feature(
        None,
        planora_feature::FeatureKind::Cylinder {
            radius: -1.0,
            height: 2.0,
            origin: DVec3::ZERO,
        },
    )
    .unwrap();
    let reason = match &s.document.features()[0].status {
        FeatureStatus::Error(e) => e.clone(),
        st => panic!("{st:?}"),
    };
    let a = s.create_box(DVec3::splat(4.0)).unwrap();
    s.move_body(0, DVec3::X).unwrap();
    browser::toggle_suppress(&mut s, a).unwrap();
    assert!(matches!(
        s.document.features()[2].status,
        FeatureStatus::Skipped(_)
    ));
    // Tooltip: Zeiger über der Fehlerzeile, die Zeit läuft weiter.
    let c = ctx();
    let (_, row) = frame(&c, &mut s, vec![], "Zylinder 1");
    let row = row.unwrap();
    let mut text = String::new();
    for t in 0..6 {
        let input = egui::RawInput {
            screen_rect: Some(SCREEN),
            time: Some(f64::from(t)),
            events: if t == 0 {
                vec![egui::Event::PointerMoved(row)]
            } else {
                vec![]
            },
            ..Default::default()
        };
        let out = c.run_ui(input, |ui| browser::show(ui, &mut s));
        text = texts(&out);
        out.drop_without_applying_deltas();
    }
    assert!(
        text.contains(&reason),
        "Tooltip mit dem Fehlertext:\n{text}"
    );
    // Der unterdrückte Körper bleibt gelistet: „unterdrückt“ und sein Name sind grau
    // gezeichnet (vor Aufgabe 6: die Schrittzeile „Verschieben 1“ mit „übersprungen“; „übersprungen“ prüft jetzt
    // a_skipped_tip_over_an_active_origin_is_marked_grey_with_its_reason).
    let mut weak = egui::Color32::TRANSPARENT;
    let out = c.run_ui(raw(vec![]), |ui| {
        weak = ui.visuals().weak_text_color();
        browser::show(ui, &mut s)
    });
    let mut colors = Vec::new();
    fn walk(shape: &egui::epaint::Shape, colors: &mut Vec<(String, egui::Color32)>) {
        match shape {
            egui::epaint::Shape::Text(t) => colors.push((
                t.galley.text().to_string(),
                t.galley.job.sections[0].format.color,
            )),
            egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, colors)),
            _ => {}
        }
    }
    out.shapes.iter().for_each(|c| walk(&c.shape, &mut colors));
    out.drop_without_applying_deltas();
    for must in ["unterdrückt", "Quader 1"] {
        let found = colors.iter().find(|(t, _)| t == must);
        assert_eq!(found.map(|f| f.1), Some(weak), "{must}: {colors:?}");
    }
}

#[test]
fn a_skipped_tip_over_an_active_origin_is_marked_grey_with_its_reason() {
    // Vereinigen aus Quader und Zylinder; der Zylinder wird unterdrückt: Das Vereinigen wird
    // übersprungen, der Quader (sein Ursprung) ist aber weiter aktiv und sichtbar.
    let mut s = AppState::new();
    let a = s.create_box(DVec3::splat(4.0)).unwrap();
    let cyl = s.create_cylinder(1.0, 6.0).unwrap();
    let u = s.add_boolean(0, 1, BoolOp::Union).unwrap();
    browser::toggle_suppress(&mut s, cyl).unwrap();
    let why = match &s.document.feature(u).unwrap().status {
        FeatureStatus::Skipped(w) => w.clone(),
        st => panic!("{st:?}"),
    };
    assert!(!s.document.feature(a).unwrap().suppressed);
    assert_eq!(s.document.feature(a).unwrap().status, FeatureStatus::Ok);

    let c = ctx();
    let mut weak = egui::Color32::TRANSPARENT;
    let out = c.run_ui(raw(vec![]), |ui| {
        weak = ui.visuals().weak_text_color();
        browser::show(ui, &mut s)
    });
    let mut colors = Vec::new();
    fn walk(shape: &egui::epaint::Shape, colors: &mut Vec<(String, egui::Color32)>) {
        match shape {
            egui::epaint::Shape::Text(t) => colors.push((
                t.galley.text().to_string(),
                t.galley.job.sections[0].format.color,
            )),
            egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, colors)),
            _ => {}
        }
    }
    out.shapes.iter().for_each(|c| walk(&c.shape, &mut colors));
    out.drop_without_applying_deltas();
    let found = colors.iter().find(|(t, _)| t == "übersprungen");
    assert_eq!(found.map(|f| f.1), Some(weak), "{colors:?}");
    assert!(!why.is_empty());
}
