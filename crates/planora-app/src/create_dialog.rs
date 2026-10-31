use crate::dialog::{Dialog, DialogSize};
use crate::formula_field::{FormulaField, names_hint, resolve_kept, value_field_text};
use crate::sketch_view::{format_length, format_number};
use crate::state::AppState;
use crate::theme;
use planora_feature::{FeatureId, FieldKey, Formulas, ParamUnit};
use planora_i18n::tr;
use planora_math::DVec3;
use planora_sketch::model::{CurveId, CurveKind, Sketch};
use std::collections::BTreeMap;

/// Voreingestellte Höhe für „Skizze extrudieren“ in Millimetern.
pub const DEFAULT_SKETCH_HEIGHT: f64 = 10.0;

/// Feste Namen aller Erzeugen-Fenster (für das Aufräumen der eingegebenen Texte); sie hängen
/// nicht von der Sprache ab, damit ein Wechsel der Sprache bei offenem Fenster nichts verliert.
const KEYS: [&str; 6] = ["box", "cylinder", "sphere", "cone", "extrude", "revolve"];

/// Kennung der eingegebenen Texte eines Erzeugen-Fensters im egui-Speicher: je Fenster und
/// Dokument-Tab (`AppState::instance`), damit zwei Tabs sich nicht die Texte teilen.
fn texts_id(key: &str, instance: u64) -> egui::Id {
    texts_id_in(key, instance, planora_i18n::current_unit())
}

/// Kennung der Texte in einer Anzeigeeinheit: Nach dem Umstellen gelten die getippten Texte
/// (in der alten Einheit) nicht mehr, das Fenster zeigt wieder die Vorgaben.
fn texts_id_in(key: &str, instance: u64, unit: planora_i18n::DisplayUnit) -> egui::Id {
    let id = egui::Id::new(("create-formulas", key)).with(instance);
    match unit {
        planora_i18n::DisplayUnit::Millimeter => id,
        u => id.with(u.id()),
    }
}

/// Offener Erzeugen-Dialog samt Eingabewerten.
#[derive(Clone, Debug, PartialEq)]
pub enum CreateDialog {
    Box {
        x: f64,
        y: f64,
        z: f64,
    },
    Cylinder {
        radius: f64,
        height: f64,
    },
    Sphere {
        radius: f64,
    },
    Cone {
        radius: f64,
        height: f64,
    },
    /// Profil `profile` (Index in `Sketch::profiles()`) der Skizze `sketch` um `height` entlang
    /// der Skizzennormalen extrudieren.
    ExtrudeSketch {
        sketch: FeatureId,
        profile: usize,
        height: f64,
    },
    /// Profil `profile` der Skizze `sketch` um die Linie `axis` der Skizze drehen (360°).
    RevolveSketch {
        sketch: FeatureId,
        profile: usize,
        axis: Option<CurveId>,
    },
}

/// Linien der Skizze, die als Drehachse taugen: Konstruktionslinien zuerst, dann die übrigen,
/// jeweils in der Reihenfolge der Kurven. Dazu der Name für die Auswahlliste.
pub fn axis_candidates(sk: &Sketch) -> Vec<(CurveId, String)> {
    let mut lines: Vec<(CurveId, bool)> = sk
        .curves()
        .filter(|(_, c)| matches!(c.kind, CurveKind::Line { .. }))
        .map(|(id, c)| (id, c.construction))
        .collect();
    // stabil: innerhalb der Gruppen bleibt die Kurvenreihenfolge
    lines.sort_by_key(|&(_, construction)| !construction);
    let (mut n_construction, mut n_line) = (0, 0);
    lines
        .into_iter()
        .map(|(id, construction)| {
            let length = sk.line_length(id).map(format_length).unwrap_or_default();
            let name = if construction {
                n_construction += 1;
                tr!(
                    "create-axis-construction",
                    n = n_construction,
                    length = length
                )
            } else {
                n_line += 1;
                tr!("create-axis-line", n = n_line, length = length)
            };
            (id, name)
        })
        .collect()
}

impl CreateDialog {
    /// Dialog „Skizze extrudieren“ für die Skizze `sketch` mit voreingestellten Werten.
    pub fn extrude_sketch(sketch: FeatureId) -> Self {
        CreateDialog::ExtrudeSketch {
            sketch,
            profile: 0,
            height: DEFAULT_SKETCH_HEIGHT,
        }
    }

    /// Dialog „Skizze drehen“ für die Skizze `sketch`; als Achse ist die erste Konstruktionslinie
    /// (sonst die erste Linie) vorgewählt.
    pub fn revolve_sketch(state: &AppState, sketch: FeatureId) -> Self {
        let axis = state
            .document
            .sketch_of(sketch)
            .and_then(|sk| axis_candidates(sk).first().map(|(id, _)| *id));
        CreateDialog::RevolveSketch {
            sketch,
            profile: 0,
            axis,
        }
    }

    pub fn title(&self) -> String {
        match self {
            CreateDialog::Box { .. } => tr!("create-box-title"),
            CreateDialog::Cylinder { .. } => tr!("create-cylinder-title"),
            CreateDialog::Sphere { .. } => tr!("create-sphere-title"),
            CreateDialog::Cone { .. } => tr!("create-cone-title"),
            CreateDialog::ExtrudeSketch { .. } => tr!("create-extrude-title"),
            CreateDialog::RevolveSketch { .. } => tr!("create-revolve-title"),
        }
    }

    /// Fester, sprachunabhängiger Name des Fensters (siehe [`KEYS`]).
    fn key(&self) -> &'static str {
        match self {
            CreateDialog::Box { .. } => KEYS[0],
            CreateDialog::Cylinder { .. } => KEYS[1],
            CreateDialog::Sphere { .. } => KEYS[2],
            CreateDialog::Cone { .. } => KEYS[3],
            CreateDialog::ExtrudeSketch { .. } => KEYS[4],
            CreateDialog::RevolveSketch { .. } => KEYS[5],
        }
    }

    /// Auswahl der Skizze; liefert `true`, wenn sie gewechselt wurde.
    fn sketch_choice(ui: &mut egui::Ui, state: &AppState, sketch: &mut FeatureId) -> bool {
        let before = *sketch;
        let sketches = state.sketch_features();
        let current = sketches
            .iter()
            .find(|(id, _)| id == sketch)
            .map(|(_, name)| name.clone())
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

    /// Liste der Profile der Skizze zum Auswählen, darunter die nicht verwendbaren Schleifen
    /// mit Grund.
    fn profile_list(ui: &mut egui::Ui, state: &AppState, sketch: FeatureId, profile: &mut usize) {
        // Die berechnete Skizze (aufgelöste Ebene, nachgeführte Kantenkopien).
        let Some(sk) = state.document.sketch_of(sketch) else {
            ui.weak(tr!("create-sketch-missing"));
            return;
        };
        // Zwischengespeichert je Stand der Skizze: neu berechnet nur nach einer Änderung.
        let extracted = state.profile_cache.get(sk);
        ui.label(tr!("create-profile"));
        if extracted.profiles.is_empty() {
            ui.weak(tr!("create-no-profile"));
        }
        for (i, p) in extracted.profiles.iter().enumerate() {
            let area = p.outer.signed_area().abs()
                - p.holes.iter().map(|h| h.signed_area().abs()).sum::<f64>();
            let label = match p.holes.len() {
                0 => tr!(
                    "create-profile-label",
                    n = i + 1,
                    area = format_number(area, 2)
                ),
                holes => tr!(
                    "create-profile-label-holes",
                    n = i + 1,
                    area = format_number(area, 2),
                    holes = holes
                ),
            };
            ui.selectable_value(profile, i, label);
        }
        for r in &extracted.rejected {
            ui.weak(tr!("create-rejected", reason = r.reason.to_string()));
        }
    }

    /// Auswahl der Drehachse (Linien der Skizze, Konstruktionslinien zuerst).
    fn axis_choice(
        ui: &mut egui::Ui,
        state: &AppState,
        sketch: FeatureId,
        axis: &mut Option<CurveId>,
    ) {
        let candidates = state
            .document
            .sketch_of(sketch)
            .map(axis_candidates)
            .unwrap_or_default();
        if candidates.is_empty() {
            ui.weak(tr!("create-no-axis"));
            return;
        }
        let current = candidates
            .iter()
            .find(|(id, _)| Some(*id) == *axis)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| tr!("create-choose"));
        egui::ComboBox::from_label(tr!("create-axis"))
            .selected_text(current)
            .show_ui(ui, |ui| {
                for (id, name) in &candidates {
                    ui.selectable_value(axis, Some(*id), name);
                }
            });
    }

    /// Die Zahlenfelder des Dialogs: Feld, Bezeichnung, Vorgabe (mm). Alle nehmen Formeln an
    /// (Längen).
    pub fn fields(&self) -> Vec<(FieldKey, String, f64)> {
        match self {
            CreateDialog::Box { x, y, z } => vec![
                (FieldKey::Breite, tr!("create-field-width"), *x),
                (FieldKey::Tiefe, tr!("create-field-depth"), *y),
                (FieldKey::Hoehe, tr!("create-field-height-z"), *z),
            ],
            CreateDialog::Cylinder { radius, height } | CreateDialog::Cone { radius, height } => {
                vec![
                    (FieldKey::Radius, tr!("create-field-radius"), *radius),
                    (FieldKey::Hoehe, tr!("create-field-height"), *height),
                ]
            }
            CreateDialog::Sphere { radius } => {
                vec![(FieldKey::Radius, tr!("create-field-radius"), *radius)]
            }
            CreateDialog::ExtrudeSketch { height, .. } => {
                vec![(FieldKey::Hoehe, tr!("create-field-height"), *height)]
            }
            CreateDialog::RevolveSketch { .. } => Vec::new(),
        }
    }

    /// Kennung der eingegebenen Texte im egui-Speicher (je Fenster und Dokument-Tab).
    pub fn texts_id(&self, state: &AppState) -> egui::Id {
        texts_id(self.key(), state.instance)
    }

    /// Die eingegebenen Texte der Felder (fehlende Felder: Vorgabe als Zahl).
    pub fn texts(&self, ctx: &egui::Context, state: &AppState) -> BTreeMap<FieldKey, String> {
        let mut texts = ctx
            .data(|d| d.get_temp::<BTreeMap<FieldKey, String>>(self.texts_id(state)))
            .unwrap_or_default();
        for (key, _, value) in self.fields() {
            texts
                .entry(key)
                .or_insert_with(|| value_field_text(value, ParamUnit::Length));
        }
        texts
    }

    /// Setzt den Text eines Feldes (wie getippt).
    pub fn set_text(&self, ctx: &egui::Context, state: &AppState, key: FieldKey, text: &str) {
        let mut texts = self.texts(ctx, state);
        texts.insert(key, text.to_string());
        ctx.data_mut(|d| d.insert_temp(self.texts_id(state), texts));
    }

    /// Vergisst die eingegebenen Texte aller Erzeugen-Fenster des Tabs `instance` (das Fenster
    /// wurde geschlossen; beim nächsten Öffnen stehen wieder die Vorgaben darin).
    pub fn forget_texts(ctx: &egui::Context, instance: u64) {
        ctx.data_mut(|d| {
            for key in KEYS {
                for unit in [
                    planora_i18n::DisplayUnit::Millimeter,
                    planora_i18n::DisplayUnit::Zoll,
                ] {
                    d.remove::<BTreeMap<FieldKey, String>>(texts_id_in(key, instance, unit));
                }
            }
        });
    }

    /// Zeigt den Dialog. Gibt `true` zurück, solange er offen bleiben soll.
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> bool {
        let mut open = true;
        let mut create = false;
        let mut cancel = false;
        let title = self.title();
        let key = self.key();
        // Ein Tab hat höchstens ein Erzeugen-Fenster: Texte anderer Fenster (etwa eines
        // Quaders, der durch eine Kugel ersetzt wurde) gelten nicht mehr.
        ctx.data_mut(|d| {
            for other in KEYS.into_iter().filter(|k| *k != key) {
                d.remove::<BTreeMap<FieldKey, String>>(texts_id(other, state.instance));
            }
        });
        let mut texts = self.texts(ctx, state);
        let fields = self.fields();
        let scope = state.document.param_scope();
        Dialog::new(title, DialogSize::Normal)
            .id(egui::Id::new(("create_dialog", key)))
            .open(&mut open)
            .show(ctx, |ui| {
                match self {
                    CreateDialog::Box { .. }
                    | CreateDialog::Cylinder { .. }
                    | CreateDialog::Sphere { .. }
                    | CreateDialog::Cone { .. } => {}
                    CreateDialog::ExtrudeSketch {
                        sketch, profile, ..
                    } => {
                        if Self::sketch_choice(ui, state, sketch) {
                            *profile = 0;
                        }
                        Self::profile_list(ui, state, *sketch, profile);
                        ui.add_space(4.0);
                    }
                    CreateDialog::RevolveSketch {
                        sketch,
                        profile,
                        axis,
                    } => {
                        if Self::sketch_choice(ui, state, sketch) {
                            *profile = 0;
                            *axis = state
                                .document
                                .sketch_of(*sketch)
                                .and_then(|sk| axis_candidates(sk).first().map(|(id, _)| *id));
                        }
                        Self::profile_list(ui, state, *sketch, profile);
                        ui.add_space(4.0);
                        Self::axis_choice(ui, state, *sketch, axis);
                    }
                }
                // Formelfelder; Meldungen stehen gesammelt unter dem Raster, damit sich die
                // Felder nicht verschieben.
                let mut errors = Vec::new();
                if !fields.is_empty() {
                    egui::Grid::new(("create_fields", key))
                        .num_columns(2)
                        .show(ui, |ui| {
                            for (field, label, _) in &fields {
                                ui.label(label);
                                let text = texts.entry(*field).or_default();
                                let out = FormulaField::new(
                                    ("create-field", key, *field),
                                    text,
                                    ParamUnit::Length,
                                )
                                .error_below(false)
                                .label(label)
                                .show(ui, &scope);
                                if let Some(e) = out.error {
                                    errors.push(tr!("field-error", label = label, error = e));
                                }
                                ui.end_row();
                            }
                        });
                    if let Some(hint) = names_hint(&scope) {
                        ui.add_space(4.0);
                        ui.weak(hint);
                    }
                }
                for e in &errors {
                    ui.add_space(4.0);
                    ui.colored_label(theme::palette(ui.visuals().dark_mode).error, e);
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(errors.is_empty(), egui::Button::new(tr!("create-create")))
                        .clicked()
                    {
                        create = true;
                    }
                    if ui.button(tr!("common-cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        let id = self.texts_id(state);
        let keep = if create {
            // Bei einem Fehler (Meldung in der Statuszeile) bleibt das Fenster offen.
            !self.create_with(state, &texts)
        } else {
            open && !cancel
        };
        if keep {
            ctx.data_mut(|d| d.insert_temp(id, texts));
        } else {
            ctx.data_mut(|d| d.remove::<BTreeMap<FieldKey, String>>(id));
        }
        keep
    }

    /// Erzeugt den Körper mit den Vorgaben des Dialogs als Feature in `state` (wie
    /// [`CreateDialog::create_with`] ohne eingegebene Texte).
    pub fn create(&self, state: &mut AppState) -> bool {
        self.create_with(state, &BTreeMap::new())
    }

    /// Erzeugt den Körper als Feature in `state`; `texts` sind die eingegebenen Texte der Felder
    /// (Zahl oder Formel; fehlende Felder: Vorgabe des Dialogs). Eine Formel wird gespeichert und
    /// bestimmt ihr Feld. Liefert `true` bei Erfolg; sonst steht der Fehler in `state.message`
    /// („Breite (x): Den Namen „Breit“ gibt es nicht.“) und nichts wurde angelegt.
    pub fn create_with(&self, state: &mut AppState, texts: &BTreeMap<FieldKey, String>) -> bool {
        state.message = None;
        let scope = state.document.param_scope();
        let mut values = Vec::new();
        let mut formulas = Formulas::new();
        for (key, label, value) in self.fields() {
            // Ein unverändertes Feld (oder ein fehlender Text) behält die Vorgabe bitgenau.
            let read = match texts.get(&key) {
                Some(text) => resolve_kept(text, ParamUnit::Length, &scope, value, None),
                None => Ok((value, None)),
            };
            match read {
                Ok((v, formula)) => {
                    values.push(v);
                    if let Some(f) = formula {
                        formulas.insert(key, f);
                    }
                }
                Err(e) => {
                    state.message = Some(tr!("field-error", label = label, error = e));
                    return false;
                }
            }
        }
        let result = match (self, values.as_slice()) {
            (CreateDialog::Box { .. }, [x, y, z]) => {
                state.create_box_with(DVec3::new(*x, *y, *z), formulas)
            }
            (CreateDialog::Cylinder { .. }, [r, h]) => state.create_cylinder_with(*r, *h, formulas),
            (CreateDialog::Sphere { .. }, [r]) => state.create_sphere_with(*r, formulas),
            (CreateDialog::Cone { .. }, [r, h]) => state.create_cone_with(*r, *h, formulas),
            // Körper aus Skizzen entstehen an Ort und Stelle; Fehler legt `AppState` ab.
            (
                CreateDialog::ExtrudeSketch {
                    sketch, profile, ..
                },
                [h],
            ) => {
                return state
                    .extrude_sketch_with(*sketch, *profile, *h, formulas)
                    .is_ok();
            }
            (CreateDialog::ExtrudeSketch { .. }, _)
            | (CreateDialog::Box { .. }, _)
            | (CreateDialog::Cylinder { .. }, _)
            | (CreateDialog::Sphere { .. }, _)
            | (CreateDialog::Cone { .. }, _) => return false,
            (
                CreateDialog::RevolveSketch {
                    sketch,
                    profile,
                    axis,
                },
                _,
            ) => {
                let Some(axis) = axis else {
                    state.message = Some(tr!("create-pick-axis"));
                    return false;
                };
                // Die Achse kann veraltet sein (Linie oder Skizze inzwischen gelöscht, Index
                // verschoben): nur Linien aus der Auswahlliste der aktuellen Skizze gelten.
                let valid = state
                    .document
                    .sketch_of(*sketch)
                    .is_some_and(|sk| axis_candidates(sk).iter().any(|(id, _)| id == axis));
                if !valid {
                    state.message = Some(tr!("create-axis-gone"));
                    return false;
                }
                return state.revolve_sketch(*sketch, *profile, *axis).is_ok();
            }
        };
        // Ein Feature mit Fehler gilt nicht als Erfolg (die Meldung steht in `message`).
        result.is_ok() && state.message.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::{CreateDialog, KEYS};
    use planora_feature::FeatureId;

    #[test]
    fn every_title_is_cleaned_up() {
        let all = [
            CreateDialog::Box {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            CreateDialog::Cylinder {
                radius: 1.0,
                height: 1.0,
            },
            CreateDialog::Sphere { radius: 1.0 },
            CreateDialog::Cone {
                radius: 1.0,
                height: 1.0,
            },
            CreateDialog::extrude_sketch(FeatureId(1)),
            CreateDialog::RevolveSketch {
                sketch: FeatureId(1),
                profile: 0,
                axis: None,
            },
        ];
        let keys: Vec<&str> = all.iter().map(CreateDialog::key).collect();
        assert_eq!(keys, KEYS);
    }
}
