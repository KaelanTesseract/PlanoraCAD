//! Fenster „Neue Zeichnung“ und Menü „Zeichnung“ (Teilprojekt 11, Aufgabe 15).

use crate::browser::error_color;
use crate::cards::card_row;
use crate::dialog::{Dialog, DialogSize};
use crate::drawing_state::{WorkArea, suggest_scale_in};
use crate::drawing_view;
use crate::export_dialog::shorten_for_display;
use crate::icons::{self, Icon};
use crate::msg::Msg;
use crate::state::AppState;
use crate::theme;
use planora_drawing::sheet_info::Stamp;
use planora_drawing::{DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewKind};
use planora_feature::Document;
use planora_i18n::tr;

/// Meldung bei leerem Namen.
pub fn empty_name() -> Msg {
    crate::msg!("drawing-name-empty")
}
/// Meldung bei einem Namen, den es schon gibt (ohne Rücksicht auf Groß- und Kleinschreibung).
pub fn duplicate_name() -> Msg {
    crate::msg!("drawing-name-duplicate")
}
/// Hinweis an gesperrten Einträgen im Skizzen-Modus.
pub fn sketch_blocked() -> Msg {
    crate::msg!("drawing-sketch-blocked")
}
/// Hinweis an „Neue Zeichnung“ in einer alten Version.
fn add_read_only() -> String {
    tr!("drawing-add-read-only")
}

/// Papiergrößen der Auswahl.
pub const PAPERS: [Paper; 5] = [Paper::A4, Paper::A3, Paper::A2, Paper::A1, Paper::A0];

/// Name einer Papiergröße.
pub fn paper_label(p: Paper) -> &'static str {
    p.label()
}

/// Name einer Projektionsart.
pub fn projection_label(p: Projection) -> String {
    match p {
        Projection::First => tr!("drawing-projection-first"),
        Projection::Third => tr!("drawing-projection-third"),
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
        .map(|n| tr!("drawing-suggested-name", n = n))
        .find(|n| !name_taken(doc, n))
        .unwrap_or_else(|| tr!("drawing-suggested-name", n = doc.drawings().len() + 1))
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

    /// Maßstabsvorschlag für Blatt, Projektion und Grundansicht (im freien Bereich eines neuen
    /// Blatts mit den Hinweiszeilen der Projektangaben).
    pub fn suggestion(&self, state: &AppState) -> Scale {
        let (lo, hi) = drawing_view::new_sheet_layout(state, self.sheet(), self.projection).free;
        suggest_scale_in(&state.bodies, (hi.x - lo.x, hi.y - lo.y), self.base)
    }

    /// Warum sich (noch) nichts anlegen lässt.
    pub fn error(&self, state: &AppState) -> Option<Msg> {
        if self.name.trim().is_empty() {
            Some(empty_name())
        } else if name_taken(&state.document, &self.name) {
            Some(duplicate_name())
        } else {
            None
        }
    }

    /// Legt die Zeichnung (mit Datum und Benutzer von heute) mit der Grundansicht in der Mitte
    /// des freien Bereichs an und öffnet sie.
    pub fn create(&self, state: &mut AppState) -> Result<DrawingId, Msg> {
        if let Some(e) = self.error(state) {
            return Err(e);
        }
        let sheet = self.sheet();
        let had_project = state.document.project_info().is_some();
        // Stand davor: Schlägt die Ansicht fehl, kommt er ganz zurück, auch der Zähler der
        // Zeichnungskennungen. So bleibt das Dokument inhaltsgleich und Rückgängig sieht keinen
        // Schritt.
        let before = state.document.snapshot();
        let id = state
            .document
            // Die Zeichnung übernimmt die Maßeinheit der Einstellung beim Anlegen.
            .add_drawing_stamped_in(
                &self.name,
                sheet,
                self.projection,
                &Stamp::now(),
                planora_i18n::current_unit(),
            )
            .map_err(|e| Msg::Raw(e.to_string()))?;
        let free = state
            .document
            .drawing(id)
            .map(|d| drawing_view::placement_layout(state, d).free)
            .ok_or_else(|| crate::msg!("drawing-missing"))?;
        // Die Grundansicht allein liegt in der Mitte des freien Bereichs; mit der ersten
        // weiteren Ansicht rückt sie so, dass der Block mittig liegt (`drawing_view::add_view`).
        let (x, y) = drawing_view::free_centre(((free.0.x, free.0.y), (free.1.x, free.1.y)));
        let placement = Placement::Free { x, y };
        if let Err(e) = state
            .document
            .add_view(id, self.base, self.scale, placement, None)
        {
            // Alles zurück, auch die Projektangaben, die das Anlegen miterzeugt hat (gleiches
            // Modell: nur Zeichnungen und Angaben werden ersetzt, nichts wird neu gerechnet).
            if state.document.restore(before).is_err() {
                let _ = state.document.delete_drawing(id);
                if !had_project {
                    state.document.clear_project_info();
                }
            }
            return Err(Msg::Raw(e.to_string()));
        }
        state.drawing_ui.open(id);
        state.drawing_ui.tick(&state.bodies, &state.document, false);
        Ok(id)
    }

    /// Zeichnet das Fenster; `true`, solange es offen bleiben soll.
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> bool {
        let error = self.error(state).map(|m| m.text());
        let (mut open, mut close, mut create) = (true, false, false);
        let mut cards = Vec::new();
        let (mut name, mut paper, mut landscape) = (self.name.clone(), self.paper, self.landscape);
        let (mut projection, mut base) = (self.projection, self.base);
        let (mut scale, mut scale_chosen) = (self.scale, self.scale_chosen);
        let response = Dialog::new(tr!("drawing-new-title"), DialogSize::Wide)
            .open(&mut open)
            .show(ctx, |ui| {
                let mut sheet_card = |ui: &mut egui::Ui| {
                    ui.strong(tr!("drawing-new-sheet"));
                    egui::Grid::new("planora-new-drawing-sheet")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label(tr!("drawing-new-name"));
                            theme::text_field(ui, egui::TextEdit::singleline(&mut name));
                            ui.end_row();
                            ui.label(tr!("drawing-new-paper"));
                            egui::ComboBox::from_id_salt("planora-new-drawing-paper")
                                .selected_text(paper_label(paper))
                                .show_ui(ui, |ui| {
                                    for p in PAPERS {
                                        ui.selectable_value(&mut paper, p, paper_label(p));
                                    }
                                });
                            ui.end_row();
                            ui.label("");
                            ui.checkbox(&mut landscape, tr!("drawing-new-landscape"));
                            ui.end_row();
                        });
                };
                let mut view_card = |ui: &mut egui::Ui| {
                    ui.strong(tr!("drawing-new-views"));
                    for p in [Projection::First, Projection::Third] {
                        ui.radio_value(&mut projection, p, projection_label(p));
                    }
                    egui::Grid::new("planora-new-drawing-view")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label(tr!("drawing-new-base-view"));
                            egui::ComboBox::from_id_salt("planora-new-drawing-base")
                                .selected_text(drawing_view::view_kind_label(base))
                                .show_ui(ui, |ui| {
                                    for k in ViewKind::ALL.into_iter().filter(|k| k.is_normal()) {
                                        ui.selectable_value(
                                            &mut base,
                                            k,
                                            drawing_view::view_kind_label(k),
                                        );
                                    }
                                });
                            ui.end_row();
                            ui.label(tr!("drawing-scale"));
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
                if let Some(e) = &error {
                    ui.colored_label(error_color(ui.visuals().dark_mode), e);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(error.is_none(), egui::Button::new(tr!("projects-create")))
                        .clicked()
                    {
                        create = true;
                    }
                    if ui.button(tr!("common-cancel")).clicked() {
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
                Err(e) => state.message = Some(e.text()),
            }
        }
        open && !close
    }
}

/// Warum das Menü „Zeichnung“ gesperrt ist (Skizzen-Modus).
pub fn drawing_menu_blocked(state: &AppState) -> Option<Msg> {
    state.sketch_session.is_some().then(sketch_blocked)
}

/// Einträge des Menüs „Zeichnung“: „Neue Zeichnung …“, je Zeichnung „Öffnen: {Name}“ und
/// „Zum 3D-Modell“. Im Skizzen-Modus gesperrt; in einer alten Version lässt sich nur öffnen.
pub fn drawing_menu(
    ui: &mut egui::Ui,
    state: &mut AppState,
    dialog: &mut Option<NewDrawingDialog>,
    read_only: bool,
) {
    let blocked = drawing_menu_blocked(state).map(|m| m.text());
    let free = blocked.is_none();
    // Randlose Menüeinträge: der Tastaturfokus zeigt sich über `theme::entry` als Ring.
    if theme::entry(
        ui,
        free && !read_only,
        icons::button(Icon::NewDrawing, tr!("drawing-menu-new")),
    )
    .on_disabled_hover_text(blocked.clone().unwrap_or_else(add_read_only))
    .clicked()
    {
        *dialog = Some(NewDrawingDialog::new(state));
        ui.close();
    }
    // Blattangaben der geöffneten Zeichnung.
    let shown = match state.drawing_ui.area {
        WorkArea::Drawing(id) if state.document.drawing(id).is_some() => Some(id),
        _ => None,
    };
    let why = if let Some(b) = blocked.clone() {
        b
    } else if read_only {
        crate::drawing_browser::read_only_hint()
    } else {
        crate::sheet_dialog::no_drawing_open()
    };
    if theme::entry(
        ui,
        free && !read_only && shown.is_some(),
        icons::button(Icon::SheetInfo, crate::sheet_dialog::menu_entry()),
    )
    .on_disabled_hover_text(why)
    .clicked()
        && let Some(id) = shown
    {
        crate::sheet_dialog::open(state, id);
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
            icons::button(Icon::OpenDrawing, tr!("drawing-menu-open", name = name)),
        )
        .on_disabled_hover_text(sketch_blocked().text())
        .clicked()
        {
            // Die zuletzt offene Zeichnung behält Auswahl, Zoom und Verschiebung.
            state.drawing_ui.show_drawing(id);
            state.drawing_ui.tick(&state.bodies, &state.document, false);
            ui.close();
        }
    }
    if theme::entry(
        ui,
        free,
        icons::button(Icon::ToModel, tr!("drawing-menu-to-model")),
    )
    .on_disabled_hover_text(sketch_blocked().text())
    .clicked()
    {
        state.drawing_ui.area = WorkArea::Model;
        ui.close();
    }
}
