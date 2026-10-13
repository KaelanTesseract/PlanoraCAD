//! Dialog „Bearbeiten“ für ein Feature der Zeitleiste (alle Arten außer Skizzen, die im
//! Skizzen-Modus bearbeitet werden). Zahlen nehmen Komma und Punkt an.

use crate::browser::ERROR_COLOR;
use crate::create_dialog::axis_candidates;
use crate::sketch_view::{format_number, parse_number};
use crate::state::{AppState, MAX_COORD_MM};
use planora_feature::{BlendArt, BodyRef, FeatureId, FeatureKind, FeatureStatus};
use planora_kernel::BoolOp;
use planora_math::DVec3;

/// Offener Bearbeiten-Dialog: eine Kopie der Feature-Art, die Zahlenfelder als Text und die
/// letzte Fehlermeldung.
#[derive(Clone, Debug)]
pub struct FeatureDialog {
    id: FeatureId,
    name: String,
    kind: FeatureKind,
    /// Zahlenfelder in der Reihenfolge von [`numbers_of`]: Bezeichnung und eingegebener Text.
    numbers: Vec<(&'static str, String)>,
    error: Option<String>,
    /// Stand des Features beim Öffnen (siehe [`planora_feature::Feature::version`]).
    version: u64,
}

/// Meldung, wenn sich das Feature geändert hat, seit der Dialog geöffnet wurde.
pub const STALE_MESSAGE: &str = "Das Feature wurde inzwischen geändert; der Dialog zeigt jetzt \
                                 den aktuellen Stand. Bitte die Werte prüfen und erneut \
                                 übernehmen.";

/// Die Zahlen einer Feature-Art mit ihren Bezeichnungen (Millimeter).
fn numbers_of(kind: &FeatureKind) -> Vec<(&'static str, f64)> {
    let pos = |o: &DVec3| {
        [
            ("Position x", o.x),
            ("Position y", o.y),
            ("Position z", o.z),
        ]
    };
    match kind {
        FeatureKind::Box { size, origin } => {
            let mut v = vec![("Breite", size.x), ("Tiefe", size.y), ("Höhe", size.z)];
            v.extend(pos(origin));
            v
        }
        FeatureKind::Cylinder {
            radius,
            height,
            origin,
        }
        | FeatureKind::Cone {
            radius,
            height,
            origin,
        } => {
            let mut v = vec![("Radius", *radius), ("Höhe", *height)];
            v.extend(pos(origin));
            v
        }
        FeatureKind::Sphere { radius, origin } => {
            let mut v = vec![("Radius", *radius)];
            v.extend(pos(origin));
            v
        }
        FeatureKind::Extrude { height, .. } => vec![("Höhe", *height)],
        FeatureKind::Move { delta, .. } => vec![("Δx", delta.x), ("Δy", delta.y), ("Δz", delta.z)],
        FeatureKind::Verrunden { art, .. } => match art {
            BlendArt::Rundung { radius } => vec![("Radius", *radius)],
            BlendArt::FaseGleich { abstand } => vec![("Abstand", *abstand)],
            BlendArt::FaseZwei { abstand1, abstand2 } => {
                vec![("Abstand 1", *abstand1), ("Abstand 2", *abstand2)]
            }
            BlendArt::FaseWinkel { abstand, winkel } => {
                vec![("Abstand", *abstand), ("Winkel", *winkel)]
            }
            BlendArt::RundungVariabel { .. } => Vec::new(),
        },
        FeatureKind::Sketch(_) | FeatureKind::Revolve { .. } | FeatureKind::Boolean { .. } => {
            Vec::new()
        }
    }
}

/// Einheit eines Zahlenfelds.
fn unit(label: &str) -> &'static str {
    if label == "Winkel" { "°" } else { "mm" }
}

/// Schreibt Zahlen (in der Reihenfolge von [`numbers_of`]) in die Feature-Art.
fn set_numbers(kind: &mut FeatureKind, v: &[f64]) {
    let p = |v: &[f64]| DVec3::new(v[0], v[1], v[2]);
    match (kind, v) {
        (FeatureKind::Box { size, origin }, [x, y, z, rest @ ..]) if rest.len() == 3 => {
            *size = DVec3::new(*x, *y, *z);
            *origin = p(rest);
        }
        (
            FeatureKind::Cylinder {
                radius,
                height,
                origin,
            }
            | FeatureKind::Cone {
                radius,
                height,
                origin,
            },
            [r, h, rest @ ..],
        ) if rest.len() == 3 => {
            (*radius, *height) = (*r, *h);
            *origin = p(rest);
        }
        (FeatureKind::Sphere { radius, origin }, [r, rest @ ..]) if rest.len() == 3 => {
            *radius = *r;
            *origin = p(rest);
        }
        (FeatureKind::Extrude { height, .. }, [h]) => *height = *h,
        (FeatureKind::Move { delta, .. }, [_, _, _]) => *delta = p(v),
        (FeatureKind::Verrunden { art, .. }, values) => match (art, values) {
            (BlendArt::Rundung { radius }, [r]) => *radius = *r,
            (BlendArt::FaseGleich { abstand }, [a]) => *abstand = *a,
            (BlendArt::FaseZwei { abstand1, abstand2 }, [a, b]) => {
                (*abstand1, *abstand2) = (*a, *b)
            }
            (BlendArt::FaseWinkel { abstand, winkel }, [a, w]) => (*abstand, *winkel) = (*a, *w),
            _ => {}
        },
        _ => {}
    }
}

/// Zahl als Text für das Eingabefeld: ohne Rundung, mit Komma.
fn number_text(v: f64) -> String {
    v.to_string().replace('.', ",")
}

fn op_label(op: BoolOp) -> &'static str {
    match op {
        BoolOp::Union => "Vereinigen",
        BoolOp::Difference => "Abziehen (Ziel − Werkzeug)",
        BoolOp::Intersection => "Schneiden",
    }
}

impl FeatureDialog {
    /// Dialog für das Feature `id`; `None` für Skizzen und unbekannte Features.
    pub fn new(state: &AppState, id: FeatureId) -> Option<FeatureDialog> {
        let f = state.document.feature(id)?;
        if matches!(f.kind, FeatureKind::Sketch(_)) {
            return None;
        }
        Some(FeatureDialog {
            id,
            name: f.name.clone(),
            kind: f.kind.clone(),
            numbers: numbers_of(&f.kind)
                .into_iter()
                .map(|(l, v)| (l, number_text(v)))
                .collect(),
            error: None,
            version: f.version(),
        })
    }

    /// Lädt den aktuellen Stand des Features neu (Eingaben gehen verloren); `false`, wenn es
    /// das Feature nicht mehr gibt.
    pub fn reload(&mut self, state: &AppState) -> bool {
        match FeatureDialog::new(state, self.id) {
            Some(d) => {
                *self = d;
                true
            }
            None => false,
        }
    }

    /// Das bearbeitete Feature.
    pub fn feature(&self) -> FeatureId {
        self.id
    }

    /// Setzt den Text des Zahlenfelds `label`; `false`, wenn es das Feld nicht gibt.
    pub fn set_text(&mut self, label: &str, text: &str) -> bool {
        match self.numbers.iter_mut().find(|(l, _)| *l == label) {
            Some((_, t)) => {
                *t = text.to_string();
                true
            }
            None => false,
        }
    }

    /// Breite, Tiefe und Höhe eines Quaders (für Tests).
    pub fn set_box_size(&mut self, size: DVec3) {
        for (label, v) in [("Breite", size.x), ("Tiefe", size.y), ("Höhe", size.z)] {
            self.set_text(label, &number_text(v));
        }
    }

    /// Operation einer Booleschen Operation.
    pub fn set_boolean_op(&mut self, new: BoolOp) {
        if let FeatureKind::Boolean { op, .. } = &mut self.kind {
            *op = new;
        }
    }

    /// Körper, die als Ziel oder Werkzeug wählbar sind: die unmittelbar vor dem Feature
    /// sichtbaren, mit Namen.
    pub fn body_choices(&self, state: &AppState) -> Vec<(BodyRef, String)> {
        state
            .document
            .visible_bodies_before(self.id)
            .iter()
            .map(|v| (v.body, state.body_ref_name(v.body)))
            .collect()
    }

    /// Die Feature-Art mit den eingegebenen Zahlen; ein Fehlertext nennt das ungültige Feld.
    fn edited_kind(&self) -> Result<FeatureKind, String> {
        let mut values = Vec::with_capacity(self.numbers.len());
        for (label, text) in &self.numbers {
            match parse_number(text) {
                Some(v) if v.abs() <= MAX_COORD_MM => values.push(v),
                Some(_) => {
                    return Err(format!(
                        "Der Wert bei „{label}“ ist zu groß (Betrag höchstens {MAX_COORD_MM:e} mm)"
                    ));
                }
                None => return Err(format!("Ungültige Zahl bei „{label}“: „{text}“")),
            }
        }
        let mut kind = self.kind.clone();
        set_numbers(&mut kind, &values);
        Ok(kind)
    }

    /// „Übernehmen“: ändert das Feature im Dokument und berechnet neu. Ungültige Eingaben und
    /// abgelehnte Änderungen liefern einen Fehler (Dokument unverändert). Lässt sich das
    /// Feature danach nicht berechnen, steht der Grund in `message` und im Baum; das gilt
    /// trotzdem als übernommen. Eine offene Skizze wird vorher beendet. Hat sich das Feature
    /// seit dem Öffnen geändert, wird nichts überschrieben ([`STALE_MESSAGE`]).
    pub fn apply(&self, state: &mut AppState) -> Result<(), String> {
        let kind = match self.edited_kind() {
            Ok(k) => k,
            Err(e) => return state.fail(e),
        };
        let current = state.document.feature(self.id).map(|f| f.version());
        if current != Some(self.version) {
            return state.fail(STALE_MESSAGE.into());
        }
        state.end_sketch_for(None)?;
        state.message = None;
        if let Err(e) = state.document.update(self.id, |k| *k = kind) {
            return state.fail(format!("Die Änderung wurde abgelehnt: {e}"));
        }
        state.refresh();
        if let Some(f) = state.document.feature(self.id)
            && let FeatureStatus::Error(e) = &f.status
        {
            state.message = Some(format!("„{}“: {e}", f.name));
        }
        Ok(())
    }

    /// Zeigt den Dialog; `true`, solange er offen bleiben soll.
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> bool {
        let mut open = true;
        let (mut apply, mut cancel) = (false, false);
        egui::Window::new(format!("„{}“ bearbeiten", self.name))
            .id(egui::Id::new(("feature_dialog", self.id)))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.weak(self.kind.label());
                ui.add_space(4.0);
                self.choices(ui, state);
                if !self.numbers.is_empty() {
                    egui::Grid::new(("feature_numbers", self.id))
                        .num_columns(3)
                        .show(ui, |ui| {
                            for (label, text) in &mut self.numbers {
                                ui.label(*label);
                                ui.add(egui::TextEdit::singleline(text).desired_width(90.0));
                                ui.label(unit(label));
                                ui.end_row();
                            }
                        });
                }
                if let Some(e) = &self.error {
                    ui.add_space(4.0);
                    ui.colored_label(ERROR_COLOR, e);
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Übernehmen").clicked() {
                        apply = true;
                    }
                    if ui.button("Abbrechen").clicked() {
                        cancel = true;
                    }
                });
            });
        if apply {
            return match self.apply(state) {
                Ok(()) => false,
                Err(e) => {
                    if e == STALE_MESSAGE && !self.reload(state) {
                        return false;
                    }
                    self.error = Some(e);
                    true
                }
            };
        }
        open && !cancel && state.document.feature(self.id).is_some()
    }

    /// Auswahlfelder je Art: Skizze, Profil und Achse; Ziel, Werkzeug und Operation.
    fn choices(&mut self, ui: &mut egui::Ui, state: &AppState) {
        let own = state.document.index_of(self.id).unwrap_or(0);
        let choices = self.body_choices(state);
        match &mut self.kind {
            FeatureKind::Extrude {
                sketch, profile, ..
            } => {
                if sketch_choice(ui, state, own, sketch) {
                    *profile = 0;
                }
                profile_choice(ui, state, *sketch, profile);
            }
            FeatureKind::Revolve {
                sketch,
                profile,
                axis,
            } => {
                if sketch_choice(ui, state, own, sketch) {
                    *profile = 0;
                    if let Some(first) = state
                        .document
                        .sketch_of(*sketch)
                        .and_then(|sk| axis_candidates(sk).first().map(|(id, _)| *id))
                    {
                        *axis = first;
                    }
                }
                profile_choice(ui, state, *sketch, profile);
                let candidates = state
                    .document
                    .sketch_of(*sketch)
                    .map(axis_candidates)
                    .unwrap_or_default();
                let current = candidates
                    .iter()
                    .find(|(id, _)| id == axis)
                    .map(|(_, n)| n.clone())
                    .unwrap_or_else(|| "(bitte wählen)".into());
                egui::ComboBox::from_label("Drehachse")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (id, name) in &candidates {
                            ui.selectable_value(axis, *id, name);
                        }
                    });
            }
            FeatureKind::Boolean { target, tool, op } => {
                for (label, slot) in [("Ziel", target), ("Werkzeug", tool)] {
                    let current = choices
                        .iter()
                        .find(|(r, _)| r == slot)
                        .map(|(_, n)| n.clone())
                        .unwrap_or_else(|| state.body_ref_name(*slot));
                    egui::ComboBox::from_label(label)
                        .selected_text(current)
                        .show_ui(ui, |ui| {
                            for (r, name) in &choices {
                                ui.selectable_value(slot, *r, name);
                            }
                        });
                }
                for o in [BoolOp::Union, BoolOp::Difference, BoolOp::Intersection] {
                    ui.radio_value(op, o, op_label(o));
                }
                ui.add_space(4.0);
            }
            FeatureKind::Move { body, .. } => {
                ui.label(format!("Körper: {}", state.body_ref_name(*body)));
                ui.add_space(4.0);
            }
            _ => {}
        }
    }
}

/// Auswahl unter den Skizzen vor dem Feature (Index `own`); `true`, wenn gewechselt wurde.
fn sketch_choice(ui: &mut egui::Ui, state: &AppState, own: usize, sketch: &mut FeatureId) -> bool {
    let before = *sketch;
    let sketches: Vec<(FeatureId, String)> = state.document.features()[..own]
        .iter()
        .filter(|f| matches!(f.kind, FeatureKind::Sketch(_)))
        .map(|f| (f.id, f.name.clone()))
        .collect();
    let current = sketches
        .iter()
        .find(|(id, _)| id == sketch)
        .map(|(_, n)| n.clone())
        .unwrap_or_else(|| "(keine)".into());
    egui::ComboBox::from_label("Skizze")
        .selected_text(current)
        .show_ui(ui, |ui| {
            for (id, name) in &sketches {
                ui.selectable_value(sketch, *id, name);
            }
        });
    *sketch != before
}

/// Auswahl des Profils der berechneten Skizze.
fn profile_choice(ui: &mut egui::Ui, state: &AppState, sketch: FeatureId, profile: &mut usize) {
    let Some(sk) = state.document.sketch_of(sketch) else {
        ui.weak("Die Skizze ist nicht berechnet.");
        return;
    };
    let extracted = state.profile_cache.get(sk);
    if extracted.profiles.is_empty() {
        ui.weak("Die Skizze enthält kein geschlossenes Profil.");
        return;
    }
    let label = |i: usize| -> String {
        let p = &extracted.profiles[i];
        let area = p.outer.signed_area().abs()
            - p.holes.iter().map(|h| h.signed_area().abs()).sum::<f64>();
        format!("Profil {}: {} mm²", i + 1, format_number(area, 2))
    };
    let current = if *profile < extracted.profiles.len() {
        label(*profile)
    } else {
        "(bitte wählen)".into()
    };
    egui::ComboBox::from_label("Profil")
        .selected_text(current)
        .show_ui(ui, |ui| {
            for i in 0..extracted.profiles.len() {
                ui.selectable_value(profile, i, label(i));
            }
        });
}
