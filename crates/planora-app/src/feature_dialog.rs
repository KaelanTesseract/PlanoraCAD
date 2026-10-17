//! Dialog „Bearbeiten“ für ein Feature der Zeitleiste (alle Arten außer Skizzen, die im
//! Skizzen-Modus bearbeitet werden). Zahlen nehmen Komma und Punkt an.

use crate::blend_dialog::{
    ArtChoice, TOO_MANY_POINTS, check_min_sizes, insert_point, min_size_for, min_text,
    set_first_face,
};
use crate::browser::error_color;
use crate::create_dialog::axis_candidates;
use crate::dialog::{Dialog, DialogSize};
use crate::sketch_view::{format_number, parse_number};
use crate::state::{AppState, MAX_COORD_MM};
use crate::theme;
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
    numbers: Vec<(String, String)>,
    error: Option<String>,
    /// Stand des Features beim Öffnen (siehe [`planora_feature::Feature::version`]).
    version: u64,
}

/// Meldung, wenn sich das Feature geändert hat, seit der Dialog geöffnet wurde.
pub const STALE_MESSAGE: &str = "Das Feature wurde inzwischen geändert; der Dialog zeigt jetzt \
                                 den aktuellen Stand. Bitte die Werte prüfen und erneut \
                                 übernehmen.";

/// Die Zahlen einer Feature-Art mit ihren Bezeichnungen (Millimeter).
fn numbers_of(kind: &FeatureKind) -> Vec<(String, f64)> {
    if let FeatureKind::Verrunden {
        art: BlendArt::RundungVariabel { punkte },
        ..
    } = kind
    {
        return punkte
            .iter()
            .enumerate()
            .flat_map(|(k, (p, r))| {
                [
                    (format!("Position {}", k + 1), *p),
                    (format!("Radius {}", k + 1), *r),
                ]
            })
            .collect();
    }
    numbers_fixed(kind)
        .into_iter()
        .map(|(l, v)| (l.to_string(), v))
        .collect()
}

/// Die Zahlen mit festen Bezeichnungen (alle Arten außer der variablen Rundung).
fn numbers_fixed(kind: &FeatureKind) -> Vec<(&'static str, f64)> {
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
        FeatureKind::Sketch(_)
        | FeatureKind::Revolve { .. }
        | FeatureKind::Boolean { .. }
        | FeatureKind::MeshBody { .. } => Vec::new(),
    }
}

/// Einheit eines Zahlenfelds.
fn unit(label: &str) -> &'static str {
    if label == "Winkel" {
        "°"
    } else if label
        .strip_prefix("Position ")
        .is_some_and(|k| k.chars().all(|c| c.is_ascii_digit()))
    {
        // „Position k“ der variablen Rundung (Anteil der Kettenlänge); „Position x“ ist mm.
        "%"
    } else {
        "mm"
    }
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
            (BlendArt::RundungVariabel { punkte }, values) if values.len() == 2 * punkte.len() => {
                for (k, p) in punkte.iter_mut().enumerate() {
                    *p = (values[2 * k], values[2 * k + 1]);
                }
            }
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
    /// Dialog für das Feature `id`; `None` für Skizzen, Netz-Körper (Lage im Eigenschaftenfenster) und unbekannte Features.
    pub fn new(state: &AppState, id: FeatureId) -> Option<FeatureDialog> {
        let f = state.document.feature(id)?;
        if matches!(
            f.kind,
            FeatureKind::Sketch(_) | FeatureKind::MeshBody { .. }
        ) {
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
        match self.numbers.iter_mut().find(|(l, _)| l == label) {
            Some((_, t)) => {
                *t = text.to_string();
                true
            }
            None => false,
        }
    }

    /// Aktuelle Meldung des Dialogs (für Tests).
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Bezeichnungen der Zahlenfelder (für Tests).
    pub fn labels(&self) -> Vec<String> {
        self.numbers.iter().map(|(l, _)| l.clone()).collect()
    }

    fn renumber(&mut self) {
        self.numbers = numbers_of(&self.kind)
            .into_iter()
            .map(|(l, v)| (l, number_text(v)))
            .collect();
    }

    /// Übernimmt eingegebene Zahlen, soweit gültig, in die Kopie der Art (vor einem Umbau).
    fn absorb(&mut self) {
        if let Ok(kind) = self.edited_kind() {
            self.kind = kind;
        }
    }

    /// Art einer Verrundung wechseln (das erste Maß bleibt).
    pub fn set_blend_art(&mut self, choice: ArtChoice) {
        self.absorb();
        if let FeatureKind::Verrunden { art, .. } = &mut self.kind {
            *art = choice.convert(art);
        }
        self.renumber();
    }

    /// Punkt einer variablen Rundung einfügen (Mitte des größten Abstands).
    pub fn add_point(&mut self) {
        self.absorb();
        if let FeatureKind::Verrunden {
            art: BlendArt::RundungVariabel { punkte },
            ..
        } = &mut self.kind
            && let Err(e) = insert_point(punkte)
        {
            self.error = Some(e);
        }
        self.renumber();
    }

    /// Punkt `i` einer variablen Rundung entfernen; mindestens zwei bleiben.
    pub fn remove_point(&mut self, i: usize) {
        self.absorb();
        if let FeatureKind::Verrunden {
            art: BlendArt::RundungVariabel { punkte },
            ..
        } = &mut self.kind
            && punkte.len() > 2
            && i < punkte.len()
        {
            punkte.remove(i);
            if self.error.as_deref() == Some(TOO_MANY_POINTS) {
                self.error = None;
            }
        }
        self.renumber();
    }

    pub fn set_tangent_chain(&mut self, on: bool) {
        if let FeatureKind::Verrunden { tangentenkette, .. } = &mut self.kind {
            *tangentenkette = on;
        }
    }

    /// Namen der beiden Flächen an der ersten Kante; die erste ist Fläche 1.
    pub fn first_faces(&self) -> Option<(String, String)> {
        match &self.kind {
            FeatureKind::Verrunden { edges, .. } => {
                edges.first().map(|e| (e.a.role.clone(), e.b.role.clone()))
            }
            _ => None,
        }
    }

    /// Macht die Fläche mit der Rolle `role` zu Fläche 1 (an allen Kanten).
    pub fn set_first_face(&mut self, role: &str) {
        if let FeatureKind::Verrunden { edges, .. } = &mut self.kind {
            set_first_face(edges, role);
        }
    }

    /// Kleinstes Maß des Kerns für den Körper, an dem die Verrundung ansetzt.
    pub fn min_size(&self, state: &AppState) -> Option<f64> {
        let FeatureKind::Verrunden { body, art, .. } = &self.kind else {
            return None;
        };
        let visible = state.document.visible_bodies_before(self.id);
        let solid = visible.iter().find(|v| v.body == *body)?.solid;
        Some(min_size_for(
            solid,
            matches!(art, BlendArt::RundungVariabel { .. }),
            &state.kernel.tolerance,
        ))
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
        if let FeatureKind::Verrunden { art, .. } = &kind
            && let Some(min) = self.min_size(state)
            && let Err(e) = check_min_sizes(art, min)
        {
            return state.fail(e);
        }
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
        Dialog::new(format!("„{}“ bearbeiten", self.name), DialogSize::Normal)
            .id(egui::Id::new(("feature_dialog", self.id)))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.weak(self.kind.label());
                ui.add_space(4.0);
                self.choices(ui, state);
                let variable = matches!(
                    self.kind,
                    FeatureKind::Verrunden {
                        art: BlendArt::RundungVariabel { .. },
                        ..
                    }
                );
                let mut remove = None;
                if !self.numbers.is_empty() {
                    egui::Grid::new(("feature_numbers", self.id))
                        .num_columns(4)
                        .show(ui, |ui| {
                            for (k, (label, text)) in self.numbers.iter_mut().enumerate() {
                                ui.label(label.as_str());
                                theme::text_field(
                                    ui,
                                    egui::TextEdit::singleline(text).desired_width(90.0),
                                );
                                ui.label(unit(label));
                                if variable
                                    && label.starts_with("Radius")
                                    && ui.button("Punkt entfernen").clicked()
                                {
                                    remove = Some(k / 2);
                                }
                                ui.end_row();
                            }
                        });
                }
                if let Some(k) = remove {
                    self.remove_point(k);
                }
                if let Some(min) = self.min_size(state) {
                    ui.weak(format!("Kleinstes Maß: {} mm", min_text(min)));
                }
                if let Some(e) = &self.error {
                    ui.add_space(4.0);
                    ui.colored_label(error_color(ui.visuals().dark_mode), e);
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
        let (mut switch, mut add, mut face) = (None, false, None);
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
            FeatureKind::Verrunden {
                body,
                edges,
                art,
                tangentenkette,
            } => {
                ui.label(format!("Körper: {}", state.body_ref_name(*body)));
                ui.label(format!("Kanten: {}", edges.len()));
                let current = ArtChoice::of(art);
                let options: &[ArtChoice] = if art.is_chamfer() {
                    &[
                        ArtChoice::FaseGleich,
                        ArtChoice::FaseZwei,
                        ArtChoice::FaseWinkel,
                    ]
                } else {
                    &[ArtChoice::Rundung, ArtChoice::RundungVariabel]
                };
                for c in options {
                    if ui.radio(current == *c, c.label()).clicked() && current != *c {
                        switch = Some(*c);
                    }
                }
                if current.uses_first_face()
                    && let Some(e) = edges.first()
                {
                    let (a, b) = (e.a.role.clone(), e.b.role.clone());
                    egui::ComboBox::from_label("Fläche 1")
                        .selected_text(a.clone())
                        .show_ui(ui, |ui| {
                            for name in [&a, &b] {
                                if ui.selectable_label(*name == a, name).clicked() {
                                    face = Some(name.clone());
                                }
                            }
                        });
                }
                ui.checkbox(
                    tangentenkette,
                    "Tangentenkette (glatt anschließende Kanten mitnehmen)",
                );
                if matches!(art, BlendArt::RundungVariabel { .. })
                    && ui.button("Punkt hinzufügen").clicked()
                {
                    add = true;
                }
                ui.add_space(4.0);
            }
            _ => {}
        }
        if let Some(c) = switch {
            self.set_blend_art(c);
        }
        if add {
            self.add_point();
        }
        if let Some(role) = face {
            self.set_first_face(&role);
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
