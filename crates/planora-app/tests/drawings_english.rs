//! Zeichnungen auf Englisch (Sprachplan, Task 9): Browser (Zeichnungen, Maße, Positionen),
//! Zeichnungsbereich (Werkzeugfeld, Ansichtsfeld, Hinweisfeld), Fenster „Neue Zeichnung“,
//! „Blattangaben“ und „Maß bearbeiten“ sowie Rückfragen, unter `with_lang(&english(), …)`
//! gezeichnet, ohne Fenster. Kein Test ruft `set_language` oder `set_global` (Ruling 4): die
//! Sprache gilt nur für den Thread des Tests.
//!
//! Texte der Bibliotheken `planora-drawing` und `planora-feature` (Fehlertexte, Vorgaben des
//! Schriftfelds, Spaltenköpfe der Stückliste) bleiben bis Task 14 deutsch; sie sind unten in
//! `LATER` aufgeführt und nur von der Deutsch-Prüfung ausgenommen.

mod common;

use common::{Seen, assert_has};
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, vec2};
use planora_app::dimension_dialog::DimensionDialog;
use planora_app::dimension_state::suggest_for;
use planora_app::dimension_tool::{LinearDir, ToolMode, missed};
use planora_app::drawing_dialog::NewDrawingDialog;
use planora_app::drawing_state::WorkArea;
use planora_app::msg::Msg;
use planora_app::sheet_dialog::SheetInfoDialog;
use planora_app::state::AppState;
use planora_app::{
    balloon_tool, browser, drawing_browser, drawing_view, fonts, section_tool, theme,
};
use planora_drawing::sheet_info::Stamp;
use planora_drawing::{
    BodyKey, DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind,
};
use planora_feature::FeatureId;
use planora_i18n::{english, german, with_lang};
use planora_math::DVec3;

const SIZE: egui::Vec2 = vec2(1920.0, 1080.0);

const A4_QUER: Sheet = Sheet {
    paper: Paper::A4,
    landscape: true,
};

/// Texte aus `planora-drawing` und `planora-feature`, die bis Task 14 deutsch bleiben und einen
/// deutschen Buchstaben enthalten: Vorgabehinweise des Schriftfelds („Oberflächen Ra …“). Ohne
/// solche Buchstaben, aber ebenfalls deutsch: „Fertigungszeichnung“, „in Arbeit“, „1 von 1“ und
/// die Spaltenköpfe der Stückliste.
const LATER: [&str; 1] = ["Oberflächen"];

struct Rig {
    ctx: egui::Context,
    time: f64,
}

impl Rig {
    fn new() -> Rig {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        ctx.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
        Rig { ctx, time: 0.0 }
    }

    fn input(&mut self, events: Vec<Event>) -> egui::RawInput {
        self.time += 0.05;
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            events,
            time: Some(self.time),
            ..Default::default()
        }
    }

    /// Ein Bild; `add` zeichnet.
    fn frame(&mut self, events: Vec<Event>, mut add: impl FnMut(&mut egui::Ui)) -> Vec<Seen> {
        let raw = self.input(events);
        let out = self.ctx.run_ui(raw, |ui| add(ui));
        let seen = common::collect_seen(&out.shapes);
        out.drop_without_applying_deltas();
        seen
    }

    /// Drei Bilder ohne Ereignisse (Fenster und Layout setzen sich).
    fn settle(&mut self, mut add: impl FnMut(&mut egui::Ui)) -> Vec<Seen> {
        self.frame(vec![], &mut add);
        self.frame(vec![], &mut add);
        self.frame(vec![], &mut add)
    }

    /// Mauszeiger auf `p`, einige Bilder (Tooltip erscheint).
    fn hover(&mut self, p: Pos2, mut add: impl FnMut(&mut egui::Ui)) -> Vec<Seen> {
        self.frame(vec![Event::PointerMoved(p)], &mut add);
        for _ in 0..5 {
            self.frame(vec![], &mut add);
        }
        self.frame(vec![], &mut add)
    }

    /// Rechtsklick auf `p` (Kontextmenü), danach das Bild mit dem Menü.
    fn right_click(&mut self, p: Pos2, mut add: impl FnMut(&mut egui::Ui)) -> Vec<Seen> {
        let press = |pressed| Event::PointerButton {
            pos: p,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![Event::PointerMoved(p)], &mut add);
        self.frame(vec![press(true)], &mut add);
        self.frame(vec![press(false)], &mut add);
        self.frame(vec![], &mut add);
        self.frame(vec![], &mut add)
    }
}

fn pos_of(seen: &[Seen], exact: &str) -> Pos2 {
    seen.iter()
        .find(|s| s.text == exact)
        .unwrap_or_else(|| panic!("„{exact}“ fehlt in {:?}", texts(seen)))
        .rect
        .center()
}

fn texts(seen: &[Seen]) -> Vec<&str> {
    seen.iter().map(|s| s.text.as_str()).collect()
}

fn has(seen: &[Seen], exact: &str) -> bool {
    seen.iter().any(|s| s.text == exact)
}

/// Ein Datum wie „2026-10-28“ (Vorgabe im Feld „Ausgabedatum“) sähe wie eine Schlüssel-Id aus.
fn is_date(text: &str) -> bool {
    text.len() == 10
        && text.chars().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                c == '-'
            } else {
                c.is_ascii_digit()
            }
        })
}

fn assert_english(what: &str, seen: &[Seen]) {
    let kept: Vec<Seen> = seen
        .iter()
        .filter(|s| !is_date(&s.text))
        .map(|s| Seen {
            text: s.text.clone(),
            rect: s.rect,
            clip: s.clip,
        })
        .collect();
    common::assert_english(what, &kept, &[], &LATER);
}

fn key(f: FeatureId) -> BodyKey {
    BodyKey {
        feature: f.0,
        index: 0,
    }
}

struct Fixture {
    st: AppState,
    drawing: DrawingId,
    front: ViewId,
    top: ViewId,
}

/// „Platte“ und „Bolzen“, Zeichnung „Blatt 1“ mit Vorder- und Draufsicht (berechnet), zwei
/// Positionsnummern, Maße (Vorschlag) und eine Schnittansicht.
fn fixture() -> Fixture {
    let mut st = AppState::new();
    let plate = st.create_box(DVec3::new(40.0, 30.0, 5.0)).unwrap();
    let bolt = st.create_cylinder(3.0, 20.0).unwrap();
    st.rename_feature(plate, "Platte").unwrap();
    st.rename_feature(bolt, "Bolzen").unwrap();
    let d = st
        .document
        .add_drawing("Blatt 1", A4_QUER, Projection::First)
        .unwrap();
    let s11 = Scale { num: 1, den: 1 };
    let front = st
        .document
        .add_view(
            d,
            ViewKind::Front,
            s11,
            Placement::Free { x: 80.0, y: 100.0 },
            None,
        )
        .unwrap();
    let top = st
        .document
        .add_view(
            d,
            ViewKind::Top,
            s11,
            Placement::Free { x: 200.0, y: 100.0 },
            None,
        )
        .unwrap();
    for (view, f) in [(front, plate), (top, plate), (front, bolt)] {
        st.document
            .add_balloon(d, view, key(f), (0.5, 0.5), (10.0, 10.0))
            .unwrap();
    }
    st.drawing_ui.open(d);
    st.drawing_ui.tick(&st.bodies, &st.document, true);
    let _ = suggest_for(&mut st, d, None);
    let model = st.document.model_edit_count();
    let ((_, y0), (_, y1)) = {
        let owned = st
            .drawing_ui
            .results_for(st.document.drawing(d).unwrap(), model);
        owned[&front].bbox
    };
    section_tool::create(&mut st, d, front, true, 0.5 * (y0 + y1)).expect("Schnitt");
    st.drawing_ui.tick(&st.bodies, &st.document, true);
    st.drawing_ui.selected_view = None;
    Fixture {
        st,
        drawing: d,
        front,
        top,
    }
}

/// Nur die Texte der schwebenden Felder des Zeichnungsbereichs (das Blatt selbst zeigt Texte der
/// Bibliothek `planora-drawing`, deutsch bis Task 14).
fn fields_only(ctx: &egui::Context, seen: &[Seen]) -> Vec<Seen> {
    let rects: Vec<Rect> = planora_app::shell_panels::field_rects(ctx)
        .into_iter()
        .filter(|(n, _)| ["Werkzeugfeld", "Ansichtsfeld", "Hinweisfeld"].contains(n))
        .map(|(_, r)| r)
        .collect();
    seen.iter()
        .filter(|s| rects.iter().any(|r| r.contains(s.rect.center())))
        .map(|s| Seen {
            text: s.text.clone(),
            rect: s.rect,
            clip: s.clip,
        })
        .collect()
}

/// Tabs des Browsers: Zeichnungen, Maße und Positionen der offenen Zeichnung.
fn browser_frame(rig: &mut Rig, st: &mut AppState) -> Vec<Seen> {
    rig.settle(|ui| browser::show_with(ui, st, false))
}

#[test]
fn drawings_speak_english() {
    with_lang(&english(), || {
        let Fixture {
            mut st,
            drawing: d,
            front,
            ..
        } = fixture();
        let mut rig = Rig::new();

        // ---- Browser: Zeichnungen, Maße, Positionen ----
        let seen = browser_frame(&mut rig, &mut st);
        assert_has(
            "Browser",
            &seen,
            &[
                "Blatt 1",
                "A4 landscape, First angle",
                "Front",
                "Top",
                "Dimensions",
                "Balloons",
            ],
        );
        assert!(
            seen.iter().any(|s| s.text.starts_with("Section A–A")),
            "Schnittansicht: {:?}",
            texts(&seen)
        );
        assert!(
            seen.iter().any(|s| s.text.starts_with("Balloon 1 – ")),
            "Positionsnummern: {:?}",
            texts(&seen)
        );
        assert_english("Browser", &seen);

        // Tooltip einer Zeile.
        let seen = browser_frame(&mut rig, &mut st);
        let tip = rig.hover(pos_of(&seen, "Blatt 1"), |ui| {
            browser::show_with(ui, &mut st, false)
        });
        assert_has(
            "Tooltip",
            &tip,
            &["Double-click opens the drawing; right-click shows more commands."],
        );

        // Kontextmenü einer Zeichnung und einer Ansicht.
        let seen = browser_frame(&mut rig, &mut st);
        let menu = rig.right_click(pos_of(&seen, "Blatt 1"), |ui| {
            browser::show_with(ui, &mut st, false)
        });
        assert_has("Menü Zeichnung", &menu, &["Open", "Rename", "Delete"]);
        assert_english("Menü Zeichnung", &menu);
        rig.settle(|ui| browser::show_with(ui, &mut st, false));
        // Menü schließen (Klick daneben), dann das der Ansicht.
        let away = Pos2::new(1500.0, 900.0);
        rig.frame(vec![Event::PointerMoved(away)], |ui| {
            browser::show_with(ui, &mut st, false)
        });
        for pressed in [true, false] {
            rig.frame(
                vec![Event::PointerButton {
                    pos: away,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                }],
                |ui| browser::show_with(ui, &mut st, false),
            );
        }
        rig.settle(|ui| browser::show_with(ui, &mut st, false));
        let menu = rig.right_click(pos_of(&seen, "Front"), |ui| {
            browser::show_with(ui, &mut st, false)
        });
        assert_has("Menü Ansicht", &menu, &["Delete", "Hide hidden edges"]);
        assert_english("Menü Ansicht", &menu);

        // Schreibgeschützt: Hinweis am gesperrten Eintrag.
        let seen = rig.settle(|ui| browser::show_with(ui, &mut st, true));
        assert_english("Browser schreibgeschützt", &seen);

        // ---- Rückfragen: unter Deutsch gespeichert, unter Englisch gezeichnet ----
        with_lang(&german(), || {
            drawing_browser::request_delete_view(&mut st, d, front);
        });
        assert!(st.pending_delete_drawing.is_some(), "Rückfrage fehlt");
        let seen = browser_frame(&mut rig, &mut st);
        assert_has("Rückfrage", &seen, &["Confirm"]);
        assert!(
            seen.iter()
                .any(|s| s.text.contains("They will be deleted too")),
            "Rückfragetext: {:?}",
            texts(&seen)
        );
        assert_english("Rückfrage", &seen);
        st.pending_delete_drawing = None;

        drawing_browser::request_rename(&mut st, d);
        let seen = browser_frame(&mut rig, &mut st);
        assert_has("Umbenennen", &seen, &["Rename", "Apply", "Cancel"]);
        assert_english("Umbenennen", &seen);
        st.rename_drawing_draft = Some((d, String::new()));
        let seen = browser_frame(&mut rig, &mut st);
        assert_has("Umbenennen leer", &seen, &["Please enter a name."]);
        st.rename_drawing_draft = None;

        // Leerer Browser.
        let mut empty = AppState::new();
        let seen = rig.settle(|ui| browser::show_with(ui, &mut empty, false));
        assert_has(
            "leerer Browser",
            &seen,
            &["No drawing yet. Use \"Drawing\" in the menu to create one."],
        );
        assert_english("leerer Browser", &seen);
    });
}

#[test]
fn drawing_area_speaks_english() {
    with_lang(&english(), || {
        let Fixture {
            mut st,
            drawing: d,
            front,
            ..
        } = fixture();
        let mut rig = Rig::new();
        let area = |rig: &mut Rig, st: &mut AppState| {
            let seen = rig.settle(|ui| drawing_view::show_with(ui, st, true));
            fields_only(&rig.ctx, &seen)
        };

        // Werkzeugfeld, Hinweisfeld, Ansichtsfeld ohne Auswahl.
        let seen = area(&mut rig, &mut st);
        assert_has(
            "Zeichnungsbereich",
            &seen,
            &[
                "3D model",
                "Drawing: Blatt 1",
                "Add view",
                "Section",
                "Suggest dimensions",
                "Dimension",
                "Balloon",
                "View",
                "Select a view.",
            ],
        );
        assert_english("Zeichnungsbereich", &seen);

        // Ansicht gewählt: Ansichtsfeld.
        st.drawing_ui.selected_view = Some(front);
        let seen = area(&mut rig, &mut st);
        assert_has(
            "Ansichtsfeld",
            &seen,
            &[
                "View \"Front\"",
                "Scale",
                "Show hidden edges",
                "Centerlines",
                "Delete view",
            ],
        );
        assert_english("Ansichtsfeld", &seen);

        // Schnittansicht gewählt: Felder der Schnittansicht, Hinweis zum Maßstab.
        let section = st
            .document
            .drawing(d)
            .unwrap()
            .views
            .iter()
            .find(|v| v.section.is_some())
            .map(|v| v.id)
            .expect("Schnittansicht");
        st.drawing_ui.selected_view = Some(section);
        let seen = area(&mut rig, &mut st);
        assert_has(
            "Schnittansicht",
            &seen,
            &[
                "Section position (mm)",
                "Reverse viewing direction",
                "Hatching",
                "Hatch spacing (mm)",
            ],
        );
        assert_english("Schnittansicht", &seen);

        // Hinweise der Werkzeuge.
        st.drawing_ui.selected_view = Some(front);
        for mode in ToolMode::CHOICES {
            st.dimension_tool.set_mode(mode);
            let seen = area(&mut rig, &mut st);
            assert_english(&format!("Maß {mode:?}"), &seen);
            let hint = st.dimension_tool.hint().unwrap();
            assert!(
                has(&seen, &hint),
                "{mode:?}: {hint:?} in {:?}",
                texts(&seen)
            );
        }
        st.dimension_tool
            .set_mode(ToolMode::Linear(LinearDir::Horizontal));
        // Eine unter Deutsch gespeicherte Ablehnung erscheint unter Englisch.
        with_lang(&german(), || {
            st.dimension_tool.set_message(Some(missed()));
        });
        let seen = area(&mut rig, &mut st);
        assert_has(
            "Ablehnung Maß",
            &seen,
            &["Please click a line of the view."],
        );
        st.dimension_tool.set_mode(ToolMode::Off);

        st.balloon_tool.set_active(true);
        with_lang(&german(), || {
            st.balloon_tool.set_message(Some(balloon_tool::missed()));
        });
        let seen = area(&mut rig, &mut st);
        assert_has("Position", &seen, &["Please click a line of a part."]);
        st.balloon_tool.set_active(false);

        st.section_tool.set_active(true);
        let seen = area(&mut rig, &mut st);
        assert_has(
            "Schnitt",
            &seen,
            &["Click the start of the section line in an orthographic view. Esc ends the tool."],
        );
        with_lang(&german(), || {
            st.section_tool.set_message(Some(section_tool::too_short()));
        });
        let seen = area(&mut rig, &mut st);
        assert_has(
            "Ablehnung Schnitt",
            &seen,
            &["Please click two different points."],
        );
        assert_english("Schnitt", &seen);
        st.section_tool.set_active(false);

        // Meldung des Vorschlags (unter Deutsch gespeichert) und Hinweis ohne Werkzeug.
        with_lang(&german(), || {
            st.drawing_ui.dim_message =
                Some(Msg::Join(vec![planora_app::msg!("dim-added", n = 3usize)]));
        });
        st.drawing_ui.dim_message_key = None;
        let seen = area(&mut rig, &mut st);
        assert_english("Hinweis", &seen);
        assert!(st.drawing_ui.area == WorkArea::Drawing(d));
    });
}

#[test]
fn drawing_dialogs_speak_english() {
    with_lang(&english(), || {
        let Fixture {
            mut st, drawing: d, ..
        } = fixture();
        let mut rig = Rig::new();

        // Neue Zeichnung (leerer Name: Fehlermeldung).
        let mut dialog = NewDrawingDialog::new(&st);
        dialog.name.clear();
        let seen = rig.settle(|ui| {
            dialog.show(ui.ctx(), &mut st);
        });
        assert_has(
            "Neue Zeichnung",
            &seen,
            &[
                "New drawing",
                "Sheet",
                "Name",
                "Paper",
                "Landscape",
                "Views",
                "Base view",
                "Scale",
                "Please enter a name.",
                "Create",
                "Cancel",
            ],
        );
        assert_english("Neue Zeichnung", &seen);
        let named = NewDrawingDialog::new(&st);
        assert_eq!(named.name, "Drawing 1", "Namensvorschlag englisch");

        // Blattangaben.
        let mut dialog = SheetInfoDialog::new(&st, d, &Stamp::now());
        let seen = rig.settle(|ui| {
            dialog.show(ui.ctx(), &mut st);
        });
        assert_has(
            "Blattangaben",
            &seen,
            &[
                "Sheet info",
                "Project",
                "This sheet",
                "Revisions",
                "Owner",
                "Drawing number",
                "Title",
                "Add revision",
                "Apply",
                "Cancel",
            ],
        );
        assert_english("Blattangaben", &seen);
        // Die 24. Änderung geht nicht; die Meldung steht im Fenster und folgt der Sprache.
        with_lang(&german(), || {
            for _ in 0..24 {
                dialog.add_revision(&Stamp::now(), "dennis");
            }
        });
        let message = dialog.message.clone().expect("Meldung");
        assert_eq!(message.text(), "At most 23 revisions are possible.");
        assert_eq!(
            with_lang(&german(), || message.text()),
            "Es sind höchstens 23 Änderungen möglich."
        );
        let seen = rig.settle(|ui| {
            dialog.show(ui.ctx(), &mut st);
        });
        assert_has("Blattangaben Änderungen", &seen, &["Letter", "Description"]);
        assert_english("Blattangaben Änderungen", &seen);
        // Abschnitt „Stückliste“: Texte der Funktionen.
        use planora_app::parts_section as ps;
        assert_eq!(ps::section(), "Parts list");
        assert_eq!(ps::switch(), "Parts list on this sheet");
        assert_eq!(ps::create(), "Create parts list sheet");
        assert_eq!(ps::open(), "Open parts list sheet");
        assert_eq!(ps::no_parts(), "There are no parts yet.");
        assert_eq!(
            ps::too_many_rows(),
            "More than 200 rows do not fit on the parts list sheets."
        );

        // Maß bearbeiten (alle Toleranzarten, mit Fehlern).
        let dim = st.document.drawing(d).unwrap().dimensions[0].id;
        let spec = st
            .document
            .drawing(d)
            .unwrap()
            .dimension(dim)
            .unwrap()
            .clone();
        let mut dialog = DimensionDialog::new(d, &spec, false);
        let seen = rig.settle(|ui| {
            dialog.show(ui.ctx(), &mut st);
        });
        assert_has(
            "Maß bearbeiten",
            &seen,
            &[
                "Edit dimension",
                "Dimension",
                "Tolerance",
                "Value",
                "Decimal places",
                "Dimension text",
                "Delete dimension",
            ],
        );
        assert_english("Maß bearbeiten", &seen);
        use planora_app::dimension_dialog::ToleranceKind as K;
        for kind in K::ALL {
            dialog.kind = kind;
            dialog.symmetric = "x".into();
            dialog.upper = "1".into();
            dialog.lower = "2".into();
            dialog.fit = String::new();
            let seen = rig.settle(|ui| {
                dialog.show(ui.ctx(), &mut st);
            });
            assert_english(&format!("Toleranz {kind:?}"), &seen);
        }
        let mut ro = DimensionDialog::new(d, &spec, true);
        let seen = rig.settle(|ui| {
            ro.show(ui.ctx(), &mut st);
        });
        assert_has(
            "Maß schreibgeschützt",
            &seen,
            &["Nothing can be changed in an old version.", "Close"],
        );
        assert_english("Maß schreibgeschützt", &seen);
    });
}

#[test]
fn stored_drawing_messages_follow_the_language() {
    let Fixture {
        mut st,
        drawing: d,
        front,
        top,
    } = fixture();
    let mut rig = Rig::new();

    // Rückfrage unter Deutsch gespeichert; dasselbe Bild deutsch und englisch.
    with_lang(&german(), || {
        drawing_browser::request_delete_view(&mut st, d, front);
    });
    let de = with_lang(&german(), || browser_frame(&mut rig, &mut st));
    assert!(
        de.iter()
            .any(|s| s.text.contains("Sie werden mit gelöscht")),
        "deutsch: {:?}",
        texts(&de)
    );
    let en = with_lang(&english(), || browser_frame(&mut rig, &mut st));
    assert!(
        en.iter()
            .any(|s| s.text.contains("They will be deleted too")),
        "englisch: {:?}",
        texts(&en)
    );
    st.pending_delete_drawing = None;

    // Ansicht mit abgeleiteter Ansicht und Maßen: Namen der Ansichten wechseln mit der Sprache.
    let names_de = with_lang(&german(), || {
        drawing_view::view_names(st.document.drawing(d).unwrap())
    });
    let names_en = with_lang(&english(), || {
        drawing_view::view_names(st.document.drawing(d).unwrap())
    });
    assert_eq!(names_de[&front], "Vorne");
    assert_eq!(names_en[&front], "Front");
    assert_eq!(names_de[&top], "Oben");
    assert_eq!(names_en[&top], "Top");
    let sec = st
        .document
        .drawing(d)
        .unwrap()
        .views
        .iter()
        .find(|v| v.section.is_some())
        .map(|v| v.id)
        .unwrap();
    assert_eq!(names_de[&sec], "Schnitt A\u{2013}A");
    assert_eq!(names_en[&sec], "Section A\u{2013}A");

    // Vorschlag der Maße: Meldung unter Deutsch gespeichert, unter Englisch gezeichnet.
    with_lang(&german(), || {
        drawing_view::suggest_dimensions(&mut st, d);
    });
    let msg = st.drawing_ui.dim_message.clone().expect("Meldung");
    let de = with_lang(&german(), || msg.text());
    let en = with_lang(&english(), || msg.text());
    assert_ne!(de, en, "{de} / {en}");
    assert!(en.contains("dimension"), "englische Meldung erwartet: {en}");
    assert!(
        !en.chars().any(|c| "äöüßÄÖÜ".contains(c)),
        "keine deutschen Buchstaben: {en}"
    );

    // Zusammengesetzte Meldungen (unter Deutsch gebaut, englisch gelesen).
    let partly = with_lang(&german(), || {
        planora_app::dimension_state::SuggestOutcome::Partly {
            added: 2,
            skipped: vec![
                planora_app::dimension_state::not_bound(),
                Msg::Join(vec![
                    planora_app::dimension_state::full_text(),
                    planora_app::msg!("dim-rest-not-set", n = 3usize),
                ]),
            ],
        }
        .message()
    });
    assert_eq!(
        with_lang(&english(), || partly.text()),
        "2 dimensions were added. A suggested dimension could not be attached to the model \
         reliably and was skipped. The drawing already has the maximum of 2000 dimensions. \
         3 more dimensions were not placed."
    );
    assert_eq!(
        with_lang(&english(), || {
            planora_app::dimension_tool::incomplete_text(1, 2).text()
        }),
        "The dimensions are incomplete: only 1 of 2 were placed."
    );
    assert_eq!(
        with_lang(&english(), || balloon_tool::lost_note(2).text()),
        "2 balloon numbers have lost their part."
    );
    // Rückfrage beim Löschen einer Schnittansicht.
    with_lang(&german(), || {
        drawing_browser::request_delete_view(&mut st, d, sec)
    });
    let plan = st.pending_delete_drawing.clone().expect("Rückfrage");
    assert_eq!(
        with_lang(&english(), || plan.text.text()),
        "Section view \"Section A\u{2013}A\" will be deleted. With it, the section line in \
         \"Front\" disappears."
    );
    st.pending_delete_drawing = None;

    // Bezeichnungen: Maßart, Papier und Projektionsart.
    let shown = with_lang(&english(), || {
        drawing_browser::drawing_detail(st.document.drawing(d).unwrap())
    });
    assert_eq!(shown, "A4 landscape, First angle");
}
