//! Fenster „Verrunden“ bzw. „Fase“ (Menü „Bearbeiten“, Spec §6): Kante im Fenster anklicken,
//! Maße eintragen, Art wählen, „Erzeugen“. Zahlen nehmen Komma und Punkt an.

use crate::browser::ERROR_COLOR;
use crate::sketch_view::parse_number;
use crate::state::{AppState, MAX_COORD_MM};
use planora_feature::BlendArt;
use planora_kernel::{Edge, Id};
use planora_render::{EdgeRef, Pick};

/// Meldung, solange keine Kante angeklickt ist.
pub const NO_EDGE: &str = "Bitte zuerst eine Kante im Fenster anklicken.";

/// Kleinster und größter Fasenwinkel in Grad (wie im Kernel, Entscheidung P4).
const MIN_ANGLE_DEG: f64 = 1.0;
const MAX_ANGLE_DEG: f64 = 179.0;

/// Vorgabe des zweiten Abstands bei „Fase: zwei Abstände“ in mm (wie der erste).
const DEFAULT_SECOND_DISTANCE: &str = "1";
/// Vorgabe des Fasenwinkels bei „Fase: Abstand und Winkel“ in Grad.
const DEFAULT_ANGLE: &str = "45";

/// Art im Fenster.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtChoice {
    Rundung,
    FaseGleich,
    FaseZwei,
    FaseWinkel,
}

impl ArtChoice {
    pub fn label(self) -> &'static str {
        match self {
            ArtChoice::Rundung => "Rundung",
            ArtChoice::FaseGleich => "Fase: gleicher Abstand",
            ArtChoice::FaseZwei => "Fase: zwei Abstände",
            ArtChoice::FaseWinkel => "Fase: Abstand und Winkel",
        }
    }

    /// Bezeichnungen der Maßfelder (das zweite fehlt bei Rundung und gleicher Fase).
    pub fn fields(self) -> (&'static str, Option<&'static str>) {
        match self {
            ArtChoice::Rundung => ("Radius", None),
            ArtChoice::FaseGleich => ("Abstand", None),
            ArtChoice::FaseZwei => ("Abstand 1", Some("Abstand 2")),
            ArtChoice::FaseWinkel => ("Abstand", Some("Winkel")),
        }
    }
}

/// Offenes Fenster „Verrunden“ oder „Fase“.
#[derive(Clone, Debug)]
pub struct BlendDialog {
    pub art: ArtChoice,
    pub first: String,
    pub second: String,
    pub tangentenkette: bool,
    /// Die beim Öffnen angeklickte Kante ist schon in die Markierung übernommen.
    seeded: bool,
    error: Option<String>,
}

/// Liest ein Maß (größer als null, höchstens `MAX_COORD_MM`).
fn size(label: &str, text: &str) -> Result<f64, String> {
    match parse_number(text) {
        Some(v) if v > 0.0 && v <= MAX_COORD_MM => Ok(v),
        _ => Err(format!(
            "„{label}“ muss eine Zahl größer als null sein (eingegeben: „{text}“)"
        )),
    }
}

impl BlendDialog {
    pub fn fillet() -> Self {
        BlendDialog {
            art: ArtChoice::Rundung,
            first: "1".into(),
            second: String::new(),
            tangentenkette: true,
            seeded: false,
            error: None,
        }
    }

    pub fn chamfer() -> Self {
        BlendDialog {
            art: ArtChoice::FaseGleich,
            first: "1".into(),
            second: DEFAULT_ANGLE.into(),
            tangentenkette: true,
            seeded: false,
            error: None,
        }
    }

    pub fn title(&self) -> &'static str {
        if self.art == ArtChoice::Rundung {
            "Verrunden"
        } else {
            "Fase"
        }
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Wechselt die Art. Das zweite Maß bekommt dabei die Vorgabe der neuen Art (sonst stünde
    /// nach „Abstand und Winkel“ bei „zwei Abstände“ noch 45 mm).
    pub fn set_art(&mut self, art: ArtChoice) {
        if self.art == art {
            return;
        }
        self.art = art;
        self.second = match art {
            ArtChoice::FaseZwei => DEFAULT_SECOND_DISTANCE.into(),
            ArtChoice::FaseWinkel => DEFAULT_ANGLE.into(),
            ArtChoice::Rundung | ArtChoice::FaseGleich => String::new(),
        };
    }

    /// Gewählte Kanten: die markierten (alle aus einem Körper), sonst die angeklickte Kante.
    pub fn chosen_edges(&self, state: &AppState) -> Option<(usize, Vec<Id<Edge>>)> {
        if let Some(first) = state.marked_edges.first() {
            let edges = state
                .marked_edges
                .iter()
                .filter(|m| m.body == first.body)
                .map(|m| m.edge)
                .collect();
            return Some((first.body, edges));
        }
        match state.selection {
            Some(Pick::Edge(EdgeRef { body, edge })) => Some((body, vec![edge])),
            _ => None,
        }
    }

    /// Beendet das Markieren (Fenster geschlossen).
    pub fn finish(&self, state: &mut AppState) {
        state.edge_picking = false;
        state.clear_edge_marks();
    }

    /// Meldung, wenn die Markierung Kanten mehrerer Körper enthält (Körper mit Namen).
    fn mixed_bodies(state: &AppState) -> Option<String> {
        let first = state.marked_edges.first()?.body;
        if state.marked_edges.iter().all(|m| m.body == first) {
            return None;
        }
        let mut names: Vec<String> = Vec::new();
        for m in &state.marked_edges {
            let name = state
                .bodies
                .get(m.body)
                .map_or_else(|| format!("Körper {}", m.body + 1), |b| b.name.clone());
            if !names.contains(&name) {
                names.push(name);
            }
        }
        Some(format!(
            "Die Kanten gehören zu verschiedenen Körpern ({}); es geht nur ein Körper auf einmal",
            names.join(", ")
        ))
    }

    /// Art mit den eingegebenen Maßen; ein Fehlertext nennt das ungültige Feld.
    pub fn art_value(&self) -> Result<BlendArt, String> {
        let (l1, l2) = self.art.fields();
        let a = size(l1, &self.first)?;
        Ok(match self.art {
            ArtChoice::Rundung => BlendArt::Rundung { radius: a },
            ArtChoice::FaseGleich => BlendArt::FaseGleich { abstand: a },
            ArtChoice::FaseZwei => BlendArt::FaseZwei {
                abstand1: a,
                abstand2: size(l2.unwrap_or("Abstand 2"), &self.second)?,
            },
            ArtChoice::FaseWinkel => match parse_number(&self.second) {
                Some(w) if (MIN_ANGLE_DEG..=MAX_ANGLE_DEG).contains(&w) => BlendArt::FaseWinkel {
                    abstand: a,
                    winkel: w,
                },
                _ => {
                    return Err(format!(
                        "„Winkel“ muss zwischen {MIN_ANGLE_DEG:.0}° und {MAX_ANGLE_DEG:.0}° liegen (eingegeben: „{}“)",
                        self.second
                    ));
                }
            },
        })
    }

    /// „Erzeugen“: `true`, wenn das Fenster offen bleiben soll (Fehler stehen dann im Fenster
    /// und in der Statuszeile).
    pub fn execute(&mut self, state: &mut AppState) -> bool {
        state.message = None;
        if let Some(e) = Self::mixed_bodies(state) {
            state.message = Some(e.clone());
            self.error = Some(e);
            return true;
        }
        let Some((body, edges)) = self.chosen_edges(state) else {
            self.error = Some(NO_EDGE.into());
            state.message = Some(NO_EDGE.into());
            return true;
        };
        let art = match self.art_value() {
            Ok(a) => a,
            Err(e) => {
                state.message = Some(e.clone());
                self.error = Some(e);
                return true;
            }
        };
        match state.add_blend(body, &edges, art, self.tangentenkette) {
            Ok(_) => {
                self.error = None;
                false
            }
            Err(e) => {
                self.error = Some(e);
                true
            }
        }
    }

    /// Zeigt das Fenster; `true`, solange es offen bleiben soll.
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> bool {
        state.edge_picking = true;
        state.chain_marking = self.tangentenkette;
        if !self.seeded {
            self.seeded = true;
            if let Some(Pick::Edge(e)) = state.selection {
                state.toggle_edge_mark(e, false);
            }
        }
        let mut open = true;
        let (mut run, mut cancel) = (false, false);
        egui::Window::new(self.title())
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                match self.chosen_edges(state) {
                    Some((_, edges)) => ui.label(format!("Gewählte Kanten: {}", edges.len())),
                    None => ui.weak("Kante im Fenster anklicken (Umschalt: mehrere)."),
                };
                ui.add_space(4.0);
                let choices: &[ArtChoice] = if self.art == ArtChoice::Rundung {
                    &[ArtChoice::Rundung]
                } else {
                    &[
                        ArtChoice::FaseGleich,
                        ArtChoice::FaseZwei,
                        ArtChoice::FaseWinkel,
                    ]
                };
                let mut chosen = self.art;
                for c in choices {
                    ui.radio_value(&mut chosen, *c, c.label());
                }
                self.set_art(chosen);
                let (l1, l2) = self.art.fields();
                egui::Grid::new("blend_numbers")
                    .num_columns(3)
                    .show(ui, |ui| {
                        ui.label(l1);
                        ui.add(egui::TextEdit::singleline(&mut self.first).desired_width(90.0));
                        ui.label("mm");
                        ui.end_row();
                        if let Some(l2) = l2 {
                            ui.label(l2);
                            ui.add(
                                egui::TextEdit::singleline(&mut self.second).desired_width(90.0),
                            );
                            ui.label(if l2 == "Winkel" { "°" } else { "mm" });
                            ui.end_row();
                        }
                    });
                ui.checkbox(
                    &mut self.tangentenkette,
                    "Tangentenkette (glatt anschließende Kanten mitnehmen)",
                );
                if let Some(e) = &self.error {
                    ui.colored_label(ERROR_COLOR, e);
                }
                ui.horizontal(|ui| {
                    if ui.button("Erzeugen").clicked() {
                        run = true;
                    }
                    if ui.button("Abbrechen").clicked() {
                        cancel = true;
                    }
                });
            });
        let keep = if run {
            self.execute(state)
        } else {
            open && !cancel
        };
        if !keep {
            self.finish(state);
        }
        keep
    }
}
