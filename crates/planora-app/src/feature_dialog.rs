//! Dialog „Bearbeiten“ für ein Feature der Zeitleiste (alle Arten außer Skizzen, die im
//! Skizzen-Modus bearbeitet werden). Zahlen nehmen Komma und Punkt an.

use crate::blend_dialog::{
    ArtChoice, check_min_sizes, insert_point, min_size_for, min_text, set_first_face,
    too_many_points,
};
use crate::browser::error_color;
use crate::create_dialog::axis_candidates;
use crate::dialog::{Dialog, DialogSize};
use crate::formula_field::{
    FieldInput, FormulaField, classify, names_hint, number_field_text, resolve,
};
use crate::msg::Msg;
use crate::sketch_view::format_number;
use crate::state::{AppState, MAX_COORD_MM};
use crate::theme;
use planora_feature::expr::number_text;
use planora_feature::{
    BlendArt, BodyRef, FeatureId, FeatureKind, FeatureStatus, FieldKey, Formulas, ParamScope,
    field_keys, field_label, field_unit, field_value, set_field_value,
};
use planora_i18n::tr;
use planora_kernel::BoolOp;
use planora_math::DVec3;

/// Offener Bearbeiten-Dialog: eine Kopie der Feature-Art, die Zahlenfelder als Text und die
/// letzte Fehlermeldung.
#[derive(Clone, Debug)]
pub struct FeatureDialog {
    id: FeatureId,
    name: String,
    kind: FeatureKind,
    /// Zahlenfelder in der Reihenfolge von [`fields_of`].
    numbers: Vec<NumberField>,
    /// Formeln des Features beim Öffnen (eine unveränderte kaputte Formel geht mit).
    saved: Formulas,
    error: Option<Msg>,
    /// Stand des Features beim Öffnen (siehe [`planora_feature::Feature::version`]).
    version: u64,
}

/// Bezeichnung eines Zahlenfelds; der Text entsteht erst beim Zeichnen (Sprache!).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FieldLabel {
    /// Bezeichnung des Feldes in der Feature-Art (Bibliothek).
    Library(FieldKey),
    /// Position des n-ten Punkts (ab 1) einer variablen Rundung.
    Position(usize),
    /// Radius des n-ten Punkts (ab 1) einer variablen Rundung.
    Radius(usize),
}

impl FieldLabel {
    fn text(self, kind: &FeatureKind) -> String {
        match self {
            FieldLabel::Library(key) => field_label(kind, key).to_string(),
            FieldLabel::Position(n) => tr!("blend-field-position-n", n = n),
            FieldLabel::Radius(n) => tr!("blend-field-radius-n", n = n),
        }
    }
}

/// Ein Zahlenfeld des Dialogs.
#[derive(Clone, Debug)]
struct NumberField {
    label: FieldLabel,
    /// Formelfähiges Feld (`None`: nur Zahlen, die Punkte der variablen Rundung).
    key: Option<FieldKey>,
    /// Einheit hinter einem Feld ohne Formel („%“ oder „mm“; Formelfelder zeigen ihre Einheit
    /// selbst).
    suffix: &'static str,
    /// Eingegebener Text (Zahl oder Formel).
    text: String,
}

/// Meldung, wenn sich das Feature geändert hat, seit der Dialog geöffnet wurde.
pub fn stale_message() -> Msg {
    crate::msg!("edit-stale")
}

/// Meldung eines Feldes, das nur Zahlen annimmt, bei einer Formel.
pub fn numbers_only(label: &str) -> Msg {
    crate::msg!("edit-numbers-only", label = label)
}

/// Die Zahlenfelder einer Feature-Art: die formelfähigen Felder ([`field_keys`]) mit ihrer
/// gespeicherten Formel (sonst dem Wert), bei der variablen Rundung die Punkte (nur Zahlen).
fn fields_of(kind: &FeatureKind, formulas: &Formulas) -> Vec<NumberField> {
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
                    // Anteil der Kettenlänge in %.
                    (FieldLabel::Position(k + 1), "%", *p),
                    (FieldLabel::Radius(k + 1), "mm", *r),
                ]
            })
            .map(|(label, suffix, v)| NumberField {
                label,
                key: None,
                suffix,
                text: number_field_text(v),
            })
            .collect();
    }
    field_keys(kind)
        .into_iter()
        .map(|key| NumberField {
            label: FieldLabel::Library(key),
            key: Some(key),
            suffix: field_unit(kind, key).suffix(),
            // Eine gespeicherte Formel bleibt, wie sie getippt wurde; ein Wert zeigt den
            // Dezimaltrenner der Sprache.
            text: formulas
                .get(&key)
                .cloned()
                .unwrap_or_else(|| number_field_text(field_value(kind, key).unwrap_or(0.0))),
        })
        .collect()
}

/// Steht im Feld `key` unverändert die beim Öffnen gespeicherte Formel (`saved`)?
fn is_kept_broken(saved: &Formulas, key: FieldKey, text: &str) -> bool {
    saved.get(&key).is_some_and(|s| s.trim() == text.trim())
}

/// Liest eine Zahl eines Feldes ohne Formeln; der Fehlertext nennt das Feld.
fn plain_value(label: &str, text: &str) -> Result<f64, Msg> {
    match classify(text) {
        Some(FieldInput::Number(v)) => Ok(v),
        Some(FieldInput::Formula(_)) => Err(numbers_only(label)),
        None => Err(crate::msg!(
            "edit-invalid-number",
            label = label,
            text = text
        )),
    }
}

/// Prüft den Betrag eines Wertes (höchstens [`MAX_COORD_MM`]).
fn check_size(label: &str, v: f64) -> Result<f64, Msg> {
    if v.abs() <= MAX_COORD_MM {
        Ok(v)
    } else {
        Err(crate::msg!(
            "edit-too-big",
            label = label,
            max = format!("{MAX_COORD_MM:e}")
        ))
    }
}

fn op_label(op: BoolOp) -> String {
    match op {
        BoolOp::Union => tr!("chip-union"),
        BoolOp::Difference => tr!("edit-op-difference"),
        BoolOp::Intersection => tr!("chip-intersect"),
    }
}

impl FeatureDialog {
    /// Dialog für das Feature `id`; `None` für Skizzen, Netz-Körper (Lage im Eigenschaftenfenster),
    /// STEP-Körper (keine Felder; verschoben wird über das Verschieben-Feature, P6) und unbekannte
    /// Features.
    pub fn new(state: &AppState, id: FeatureId) -> Option<FeatureDialog> {
        let f = state.document.feature(id)?;
        if matches!(
            f.kind,
            FeatureKind::Sketch(_) | FeatureKind::MeshBody { .. } | FeatureKind::StepBody { .. }
        ) {
            return None;
        }
        Some(FeatureDialog {
            id,
            name: f.name.clone(),
            kind: f.kind.clone(),
            numbers: fields_of(&f.kind, f.formulas()),
            saved: f.formulas().clone(),
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
        let kind = &self.kind;
        match self
            .numbers
            .iter_mut()
            .find(|f| f.label.text(kind) == label)
        {
            Some(f) => {
                f.text = text.to_string();
                true
            }
            None => false,
        }
    }

    /// Text des Zahlenfelds `label` (Zahl oder Formel); `None`, wenn es das Feld nicht gibt.
    pub fn text(&self, label: &str) -> Option<&str> {
        self.numbers
            .iter()
            .find(|f| f.label.text(&self.kind) == label)
            .map(|f| f.text.as_str())
    }

    /// Aktuelle Meldung des Dialogs in der aktuellen Sprache (für Tests).
    pub fn error(&self) -> Option<String> {
        self.error.as_ref().map(Msg::text)
    }

    /// Bezeichnungen der Zahlenfelder (für Tests).
    pub fn labels(&self) -> Vec<String> {
        self.numbers
            .iter()
            .map(|f| f.label.text(&self.kind))
            .collect()
    }

    /// Die Formeltexte der Felder (Feld mit einer Formel statt einer Zahl).
    fn formula_texts(&self) -> Formulas {
        self.numbers
            .iter()
            .filter_map(|f| match (f.key, classify(&f.text)) {
                (Some(key), Some(FieldInput::Formula(t))) => Some((key, t)),
                _ => None,
            })
            .collect()
    }

    /// Baut die Felder aus der Kopie der Art neu; Formeln in `keep` bleiben bei Feldern, die es
    /// weiterhin gibt.
    fn renumber(&mut self, keep: &Formulas) {
        self.numbers = fields_of(&self.kind, keep);
    }

    /// Übernimmt eingegebene Zahlen, soweit gültig, in die Kopie der Art (vor einem Umbau);
    /// liefert die Formeltexte, die der Umbau behalten soll.
    fn absorb(&mut self) -> Formulas {
        let mut kind = self.kind.clone();
        if let FeatureKind::Verrunden {
            art: BlendArt::RundungVariabel { punkte },
            ..
        } = &mut kind
        {
            if let Ok(values) = self.variable_points() {
                *punkte = values;
            }
        } else {
            for f in &self.numbers {
                if let (Some(key), Some(FieldInput::Number(v))) = (f.key, classify(&f.text))
                    && v.abs() <= MAX_COORD_MM
                {
                    set_field_value(&mut kind, key, v);
                }
            }
        }
        self.kind = kind;
        self.formula_texts()
    }

    /// Art einer Verrundung wechseln (das erste Maß bleibt).
    pub fn set_blend_art(&mut self, choice: ArtChoice) {
        let keep = self.absorb();
        if let FeatureKind::Verrunden { art, .. } = &mut self.kind {
            *art = choice.convert(art);
        }
        self.renumber(&keep);
    }

    /// Punkt einer variablen Rundung einfügen (Mitte des größten Abstands).
    pub fn add_point(&mut self) {
        let keep = self.absorb();
        if let FeatureKind::Verrunden {
            art: BlendArt::RundungVariabel { punkte },
            ..
        } = &mut self.kind
            && let Err(e) = insert_point(punkte)
        {
            self.error = Some(e);
        }
        self.renumber(&keep);
    }

    /// Punkt `i` einer variablen Rundung entfernen; mindestens zwei bleiben.
    pub fn remove_point(&mut self, i: usize) {
        let keep = self.absorb();
        if let FeatureKind::Verrunden {
            art: BlendArt::RundungVariabel { punkte },
            ..
        } = &mut self.kind
            && punkte.len() > 2
            && i < punkte.len()
        {
            punkte.remove(i);
            if self.error == Some(too_many_points()) {
                self.error = None;
            }
        }
        self.renumber(&keep);
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

    /// Die Punkte der variablen Rundung aus den Feldern (nur Zahlen).
    fn variable_points(&self) -> Result<Vec<(f64, f64)>, Msg> {
        let mut values = Vec::with_capacity(self.numbers.len());
        for f in &self.numbers {
            let label = f.label.text(&self.kind);
            values.push(check_size(&label, plain_value(&label, &f.text)?)?);
        }
        Ok(values
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&[p, r]| (p, r))
            .collect())
    }

    /// Die Feature-Art mit den eingegebenen Werten und die Formeln aller Felder (ein Feld mit
    /// einer Zahl hat keine Formel, P2); ein Fehlertext nennt das ungültige Feld
    /// („Breite: Den Namen „Breit“ gibt es nicht.“).
    fn edited(&self, scope: &ParamScope) -> Result<(FeatureKind, Formulas), Msg> {
        let mut kind = self.kind.clone();
        let mut formulas = Formulas::new();
        if let FeatureKind::Verrunden {
            art: BlendArt::RundungVariabel { punkte },
            ..
        } = &mut kind
        {
            let values = self.variable_points()?;
            if values.len() == punkte.len() {
                *punkte = values;
            }
            return Ok((kind, formulas));
        }
        for f in &self.numbers {
            let Some(key) = f.key else {
                continue;
            };
            let (v, formula) = match resolve(&f.text, field_unit(&kind, key), scope) {
                Ok(r) => r,
                // Eine gespeicherte kaputte Formel (etwa aus einer Datei) geht unverändert mit;
                // das Dokument duldet sie (gleiches Feature, Feld und Text), andere Felder lassen
                // sich trotzdem ändern. Der Wert bleibt der Feldwert.
                Err(_) if is_kept_broken(&self.saved, key, &f.text) => (
                    field_value(&kind, key).unwrap_or(0.0),
                    self.saved.get(&key).cloned(),
                ),
                Err(e) => {
                    return Err(crate::msg!(
                        "field-error",
                        label = f.label.text(&kind),
                        error = e
                    ));
                }
            };
            let checked = check_size(&f.label.text(&kind), v)?;
            set_field_value(&mut kind, key, checked);
            if let Some(t) = formula {
                formulas.insert(key, t);
            }
        }
        Ok((kind, formulas))
    }

    /// „Übernehmen“: ändert das Feature im Dokument und berechnet neu. Ungültige Eingaben und
    /// abgelehnte Änderungen liefern einen Fehler (Dokument unverändert). Lässt sich das
    /// Feature danach nicht berechnen, steht der Grund in `message` und im Baum; das gilt
    /// trotzdem als übernommen. Eine offene Skizze wird vorher beendet. Hat sich das Feature
    /// seit dem Öffnen geändert, wird nichts überschrieben ([`stale_message`]). Die Formeln der
    /// Felder ersetzen die des Features (Zahl statt Formel: nur diese Formel entfällt).
    pub fn apply(&self, state: &mut AppState) -> Result<(), String> {
        self.apply_msg(state).map_err(|m| m.text())
    }

    /// Wie [`FeatureDialog::apply`], der Fehler ist aber der Grund (der Dialog behält ihn und
    /// übersetzt ihn beim Zeichnen).
    fn apply_msg(&self, state: &mut AppState) -> Result<(), Msg> {
        fn fail(state: &mut AppState, m: Msg) -> Result<(), Msg> {
            state.message = Some(m.text());
            Err(m)
        }
        let scope = state.document.param_scope();
        let (kind, formulas) = match self.edited(&scope) {
            Ok(k) => k,
            Err(e) => return fail(state, e),
        };
        if let FeatureKind::Verrunden { art, .. } = &kind
            && let Some(min) = self.min_size(state)
            && let Err(e) = check_min_sizes(art, min)
        {
            return fail(state, e);
        }
        let current = state.document.feature(self.id).map(|f| f.version());
        if current != Some(self.version) {
            return fail(state, stale_message());
        }
        state.end_sketch_for(None).map_err(Msg::Raw)?;
        state.message = None;
        if let Err(e) = state
            .document
            .update_with_formulas(self.id, |k| *k = kind, formulas)
        {
            return fail(state, crate::msg!("edit-rejected", error = e.to_string()));
        }
        state.refresh();
        if let Some(f) = state.document.feature(self.id)
            && let FeatureStatus::Error(e) = &f.status
        {
            state.message = Some(tr!("tree-status-line", name = &f.name, error = e));
        }
        Ok(())
    }

    /// Zeigt den Dialog; `true`, solange er offen bleiben soll.
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> bool {
        let mut open = true;
        let (mut apply, mut cancel) = (false, false);
        let scope = state.document.param_scope();
        Dialog::new(tr!("edit-title", name = &self.name), DialogSize::Normal)
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
                // Meldungen der Formelfelder stehen gesammelt unter dem Raster, damit sich die
                // Felder nicht verschieben.
                let mut errors = Vec::new();
                // Meldungen gespeicherter kaputter Formeln (unverändert): gezeigt, sperren aber
                // „Übernehmen“ nicht, damit sich andere Felder ändern lassen.
                let mut kept_broken = Vec::new();
                if !self.numbers.is_empty() {
                    let (id, kind, saved) = (self.id, &self.kind, &self.saved);
                    egui::Grid::new(("feature_numbers", id))
                        .num_columns(2)
                        .show(ui, |ui| {
                            for (k, f) in self.numbers.iter_mut().enumerate() {
                                let label = f.label.text(kind);
                                ui.label(label.as_str());
                                match f.key {
                                    Some(key) => {
                                        let out = FormulaField::new(
                                            ("feature-field", id, key),
                                            &mut f.text,
                                            field_unit(kind, key),
                                        )
                                        .error_below(false)
                                        .label(label.as_str())
                                        .show(ui, &scope);
                                        if let Some(e) = out.error {
                                            let msg = tr!("field-error", label = &label, error = e);
                                            if is_kept_broken(saved, key, &f.text) {
                                                kept_broken.push(msg);
                                            } else {
                                                errors.push(msg);
                                            }
                                        }
                                    }
                                    // Punkte der variablen Rundung: Feld, Einheit und „Punkt
                                    // entfernen“ in einer Zelle (zwei Spalten: das Formelfeld
                                    // steht in der letzten Spalte, sein Wert bekommt die
                                    // ganze Breite).
                                    None => {
                                        ui.horizontal(|ui| {
                                            let r = theme::text_field(
                                                ui,
                                                egui::TextEdit::singleline(&mut f.text)
                                                    .desired_width(90.0),
                                            );
                                            r.widget_info(|| {
                                                egui::WidgetInfo::labeled(
                                                    egui::WidgetType::TextEdit,
                                                    true,
                                                    label.as_str(),
                                                )
                                            });
                                            ui.label(f.suffix);
                                            if variable
                                                && matches!(f.label, FieldLabel::Radius(_))
                                                && ui.button(tr!("edit-remove-point")).clicked()
                                            {
                                                remove = Some(k / 2);
                                            }
                                        });
                                    }
                                }
                                ui.end_row();
                            }
                        });
                    if self.numbers.iter().any(|f| f.key.is_some())
                        && let Some(hint) = names_hint(&scope)
                    {
                        ui.add_space(4.0);
                        ui.weak(hint);
                    }
                }
                if let Some(k) = remove {
                    self.remove_point(k);
                }
                if let Some(min) = self.min_size(state) {
                    ui.weak(tr!("blend-min-size", min = min_text(min)));
                }
                let dark = ui.visuals().dark_mode;
                let own = self.error();
                for e in errors.iter().chain(&kept_broken).chain(&own) {
                    ui.add_space(4.0);
                    ui.colored_label(error_color(dark), e);
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(errors.is_empty(), egui::Button::new(tr!("tree-apply")))
                        .clicked()
                    {
                        apply = true;
                    }
                    if ui.button(tr!("common-cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        if apply {
            return match self.apply_msg(state) {
                Ok(()) => false,
                Err(e) => {
                    if e == stale_message() && !self.reload(state) {
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
                    .unwrap_or_else(|| tr!("create-choose"));
                egui::ComboBox::from_label(tr!("create-axis"))
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (id, name) in &candidates {
                            ui.selectable_value(axis, *id, name);
                        }
                    });
            }
            FeatureKind::Boolean { target, tool, op } => {
                for (label, slot) in [(tr!("edit-target"), target), (tr!("edit-tool"), tool)] {
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
                ui.label(tr!("edit-body", name = state.body_ref_name(*body)));
                ui.add_space(4.0);
            }
            FeatureKind::Verrunden {
                body,
                edges,
                art,
                tangentenkette,
            } => {
                ui.label(tr!("edit-body", name = state.body_ref_name(*body)));
                ui.label(tr!("edit-edges", n = edges.len()));
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
                    egui::ComboBox::from_label(tr!("blend-face-1"))
                        .selected_text(a.clone())
                        .show_ui(ui, |ui| {
                            for name in [&a, &b] {
                                if ui.selectable_label(*name == a, name).clicked() {
                                    face = Some(name.clone());
                                }
                            }
                        });
                }
                ui.checkbox(tangentenkette, tr!("blend-chain"));
                if matches!(art, BlendArt::RundungVariabel { .. })
                    && ui.button(tr!("blend-add-point")).clicked()
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
        .unwrap_or_else(|| tr!("create-none"));
    egui::ComboBox::from_label(tr!("create-sketch"))
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
        ui.weak(tr!("edit-sketch-not-computed"));
        return;
    };
    let extracted = state.profile_cache.get(sk);
    if extracted.profiles.is_empty() {
        ui.weak(tr!("create-no-profile"));
        return;
    }
    let label = |i: usize| -> String {
        let p = &extracted.profiles[i];
        let area = p.outer.signed_area().abs()
            - p.holes.iter().map(|h| h.signed_area().abs()).sum::<f64>();
        tr!(
            "create-profile-label",
            n = i + 1,
            area = format_number(area, 2)
        )
    };
    let current = if *profile < extracted.profiles.len() {
        label(*profile)
    } else {
        tr!("create-choose")
    };
    egui::ComboBox::from_label(tr!("create-profile"))
        .selected_text(current)
        .show_ui(ui, |ui| {
            for i in 0..extracted.profiles.len() {
                ui.selectable_value(profile, i, label(i));
            }
        });
}
