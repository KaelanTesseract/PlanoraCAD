//! Fenster „Neue Zeichnung“ und Menü „Zeichnung“ (Teilprojekt 11, Aufgabe 15).

use crate::browser::error_color;
use crate::cards::card_row;
use crate::drawing_state::{WorkArea, suggest_scale};
use crate::export_dialog::{shorten_for_display, wide_window};
use crate::icons::{self, Icon};
use crate::state::AppState;
use crate::theme;
use planora_drawing::{DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewKind};
use planora_feature::Document;

/// Meldung bei leerem Namen.
pub const EMPTY_NAME: &str = "Bitte einen Namen eingeben.";
/// Meldung bei einem Namen, den es schon gibt (ohne Rücksicht auf Groß- und Kleinschreibung).
pub const DUPLICATE_NAME: &str = "Eine Zeichnung mit diesem Namen gibt es schon.";
/// Hinweis an gesperrten Einträgen im Skizzen-Modus.
pub const SKETCH_BLOCKED: &str = "Erst die Skizze beenden.";
/// Hinweis an „Neue Zeichnung“ in einer alten Version.
const READ_ONLY: &str = "In einer alten Version lässt sich nichts hinzufügen.";

/// Papiergrößen der Auswahl.
pub const PAPERS: [Paper; 5] = [Paper::A4, Paper::A3, Paper::A2, Paper::A1, Paper::A0];

/// Name einer Papiergröße.
pub fn paper_label(p: Paper) -> &'static str {
    match p {
        Paper::A4 => "A4",
        Paper::A3 => "A3",
        Paper::A2 => "A2",
        Paper::A1 => "A1",
        Paper::A0 => "A0",
    }
}

/// Name einer Projektionsart.
pub fn projection_label(p: Projection) -> &'static str {
    match p {
        Projection::First => "Erstwinkel (Europa)",
        Projection::Third => "Dritter Winkel (USA)",
    }
}

fn name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

fn name_taken(doc: &Document, name: &str) -> bool {
    let key = name_key(name);
    doc.drawings().iter().any(|d| name_key(&d.name) == key)
}

/// Namensvorschlag „Zeichnung N“ mit der kleinsten freien Zahl N ≥ 1.
pub fn suggested_name(doc: &Document) -> String {
    (1..=doc.drawings().len() + 1)
        .map(|n| format!("Zeichnung {n}"))
        .find(|n| !name_taken(doc, n))
        .unwrap_or_else(|| format!("Zeichnung {}", doc.drawings().len() + 1))
}

/// Fenster „Neue Zeichnung“.
#[derive(Clone, Debug)]
pub struct NewDrawingDialog {
    pub name: String,
    pub paper: Paper,
    pub landscape: bool,
    pub projection: Projection,
    /// Grundansicht (eine Normansicht).
    pub base: ViewKind,
    pub scale: Scale,
    /// Der Maßstab wurde von Hand gewählt; sonst folgt er dem Vorschlag.
    scale_chosen: bool,
    pub last_rect: Option<egui::Rect>,
    pub last_cards: Vec<egui::Rect>,
}

impl NewDrawingDialog {
    pub fn new(state: &AppState) -> NewDrawingDialog {
        let mut d = NewDrawingDialog {
            name: suggested_name(&state.document),
            paper: Paper::A4,
            landscape: true,
            projection: Projection::First,
            base: ViewKind::Front,
            scale: Scale { num: 1, den: 1 },
            scale_chosen: false,
            last_rect: None,
            last_cards: Vec::new(),
        };
        d.scale = d.suggestion(state);
        d
    }

    pub fn sheet(&self) -> Sheet {
        Sheet {
            paper: self.paper,
            landscape: self.landscape,
        }
    }

    /// Maßstabsvorschlag für Blatt, Projektion und Grundansicht.
    pub fn suggestion(&self, state: &AppState) -> Scale {
        suggest_scale(&state.bodies, self.sheet(), self.projection, self.base)
    }

    /// Warum sich (noch) nichts anlegen lässt.
    pub fn error(&self, state: &AppState) -> Option<&'static str> {
        if self.name.trim().is_empty() {
            Some(EMPTY_NAME)
        } else if name_taken(&state.document, &self.name) {
            Some(DUPLICATE_NAME)
        } else {
            None
        }
    }

    /// Legt die Zeichnung mit der Grundansicht in der Blattmitte an und öffnet sie.
    pub fn create(&self, state: &mut AppState) -> Result<DrawingId, String> {
        if let Some(e) = self.error(state) {
            return Err(e.into());
        }
        let sheet = self.sheet();
        let (w, h) = sheet.size_mm();
        let id = state
            .document
            .add_drawing(&self.name, sheet, self.projection)
            .map_err(|e| e.to_string())?;
        let placement = Placement::Free {
            x: w / 2.0,
            y: h / 2.0,
        };
        if let Err(e) = state
            .document
            .add_view(id, self.base, self.scale, placement, None)
        {
            let _ = state.document.delete_drawing(id);
            return Err(e.to_string());
        }
        state.drawing_ui.open(id);
        state.drawing_ui.tick(&state.bodies, &state.document, false);
        Ok(id)
    }

    /// Zeichnet das Fenster; `true`, solange es offen bleiben soll.
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> bool {
        let error = self.error(state);
        let (mut open, mut close, mut create) = (true, false, false);
        let mut cards = Vec::new();
        let (mut name, mut paper, mut landscape) = (self.name.clone(), self.paper, self.landscape);
        let (mut projection, mut base) = (self.projection, self.base);
        let (mut scale, mut scale_chosen) = (self.scale, self.scale_chosen);
        let response = wide_window("Neue Zeichnung", ctx)
            .open(&mut open)
            .show(ctx, |ui| {
                let mut sheet_card = |ui: &mut egui::Ui| {
                    ui.strong("Blatt");
                    egui::Grid::new("planora-new-drawing-sheet")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Name");
                            theme::text_field(ui, egui::TextEdit::singleline(&mut name));
                            ui.end_row();
                            ui.label("Papier");
                            egui::ComboBox::from_id_salt("planora-new-drawing-paper")
                                .selected_text(paper_label(paper))
                                .show_ui(ui, |ui| {
                                    for p in PAPERS {
                                        ui.selectable_value(&mut paper, p, paper_label(p));
                                    }
                                });
                            ui.end_row();
                            ui.label("");
                            ui.checkbox(&mut landscape, "Querformat");
                            ui.end_row();
                        });
                };
                let mut view_card = |ui: &mut egui::Ui| {
                    ui.strong("Ansichten");
                    for p in [Projection::First, Projection::Third] {
                        ui.radio_value(&mut projection, p, projection_label(p));
                    }
                    egui::Grid::new("planora-new-drawing-view")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Grundansicht");
                            egui::ComboBox::from_id_salt("planora-new-drawing-base")
                                .selected_text(base.label())
                                .show_ui(ui, |ui| {
                                    for k in ViewKind::ALL.into_iter().filter(|k| k.is_normal()) {
                                        ui.selectable_value(&mut base, k, k.label());
                                    }
                                });
                            ui.end_row();
                            ui.label("Maßstab");
                            egui::ComboBox::from_id_salt("planora-new-drawing-scale")
                                .selected_text(scale.label())
                                .show_ui(ui, |ui| {
                                    for s in Scale::SERIES {
                                        if ui.selectable_label(s == scale, s.label()).clicked() {
                                            scale = s;
                                            scale_chosen = true;
                                        }
                                    }
                                });
                            ui.end_row();
                        });
                };
                let row: &mut [&mut dyn FnMut(&mut egui::Ui)] =
                    &mut [&mut sheet_card, &mut view_card];
                cards = card_row(ui, "new-drawing", row);
                ui.add_space(8.0);
                if let Some(e) = error {
                    ui.colored_label(error_color(ui.visuals().dark_mode), e);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(error.is_none(), egui::Button::new("Anlegen"))
                        .clicked()
                    {
                        create = true;
                    }
                    if ui.button("Abbrechen").clicked() {
                        close = true;
                    }
                });
            });
        (self.name, self.paper, self.landscape) = (name, paper, landscape);
        (self.projection, self.base) = (projection, base);
        (self.scale, self.scale_chosen) = (scale, scale_chosen);
        if !self.scale_chosen {
            self.scale = self.suggestion(state);
        }
        self.last_rect = response.map(|r| r.response.rect);
        self.last_cards = cards;
        if create {
            match self.create(state) {
                Ok(_) => return false,
                Err(e) => state.message = Some(e),
            }
        }
        open && !close
    }
}

/// Warum das Menü „Zeichnung“ gesperrt ist (Skizzen-Modus).
pub fn drawing_menu_blocked(state: &AppState) -> Option<&'static str> {
    state.sketch_session.is_some().then_some(SKETCH_BLOCKED)
}

/// Einträge des Menüs „Zeichnung“: „Neue Zeichnung …“, je Zeichnung „Öffnen: {Name}“ und
/// „Zum 3D-Modell“. Im Skizzen-Modus gesperrt; in einer alten Version lässt sich nur öffnen.
pub fn drawing_menu(
    ui: &mut egui::Ui,
    state: &mut AppState,
    dialog: &mut Option<NewDrawingDialog>,
    read_only: bool,
) {
    let blocked = drawing_menu_blocked(state);
    let free = blocked.is_none();
    // Randlose Menüeinträge: der Tastaturfokus zeigt sich über `theme::entry` als Ring.
    if theme::entry(
        ui,
        free && !read_only,
        icons::button(Icon::NewDrawing, "Neue Zeichnung …"),
    )
    .on_disabled_hover_text(blocked.unwrap_or(READ_ONLY))
    .clicked()
    {
        *dialog = Some(NewDrawingDialog::new(state));
        ui.close();
    }
    ui.separator();
    let list: Vec<(DrawingId, String)> = state
        .document
        .drawings()
        .iter()
        .map(|d| (d.id, shorten_for_display(&d.name)))
        .collect();
    for (id, name) in list {
        if theme::entry(
            ui,
            free,
            icons::button(Icon::OpenDrawing, format!("Öffnen: {name}")),
        )
        .on_disabled_hover_text(SKETCH_BLOCKED)
        .clicked()
        {
            // Die zuletzt offene Zeichnung behält Auswahl, Zoom und Verschiebung.
            state.drawing_ui.show_drawing(id);
            state.drawing_ui.tick(&state.bodies, &state.document, false);
            ui.close();
        }
    }
    if theme::entry(ui, free, icons::button(Icon::ToModel, "Zum 3D-Modell"))
        .on_disabled_hover_text(SKETCH_BLOCKED)
        .clicked()
    {
        state.drawing_ui.area = WorkArea::Model;
        ui.close();
    }
}
