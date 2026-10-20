//! Parametertabelle des Dokuments: benannte Werte mit Formel, Art und Kommentar.
//!
//! Jede öffentliche Änderung baut die neue Tabelle als Kopie, prüft sie ganz (Namen, Anzahl,
//! Formeln, Kreise, Einheiten, Kommentare) und übernimmt sie erst dann; eine abgelehnte
//! Änderung lässt das Dokument und seine Zähler unverändert. Eine gelungene Änderung zählt als
//! **eine** Änderung am Modell ([`Document::model_edit_count`]) und rechnet neu, damit
//! Rückgängig, Speichern und Ansichten sie wie jede Feature-Änderung behandeln.

use crate::error::FeatureError;
use crate::expr::{self, MAX_COMMENT_CHARS, MAX_PARAMETERS, ParamDef, ParamUnit, TableError};
use crate::formulas::{Formulas, mentions, rewrite_formulas};
use crate::model::{Document, FeatureKind};
use crate::recompute::RecomputeReport;
use std::collections::HashMap;

/// Dauerhafte Kennung eines Parameters (ab 1); wird nie wiederverwendet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ParamId(pub u64);

/// Eine Zeile der Parametertabelle. Der Wert steht nicht hier, er wird aus der Formel
/// berechnet ([`Document::parameter_value`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    pub id: ParamId,
    pub name: String,
    pub formula: String,
    pub unit: ParamUnit,
    pub comment: String,
}

/// Neuer Inhalt einer Zeile für [`Document::set_parameter`].
#[derive(Clone, Debug, PartialEq)]
pub struct ParamEdit {
    pub name: String,
    pub formula: String,
    pub unit: ParamUnit,
    pub comment: String,
}

/// Die berechneten Parameter zum Auswerten von Formeln (Namen ohne Rücksicht auf
/// Groß-/Kleinschreibung).
#[derive(Clone, Debug, Default)]
pub struct ParamScope {
    /// Name klein → (Wert, Art).
    values: HashMap<String, (f64, ParamUnit)>,
    /// Namen wie geschrieben, in Tabellenreihenfolge.
    names: Vec<String>,
}

impl ParamScope {
    fn new(params: &[Parameter], values: &[f64]) -> ParamScope {
        ParamScope {
            values: params
                .iter()
                .zip(values)
                .map(|(p, v)| (p.name.to_lowercase(), (*v, p.unit)))
                .collect(),
            names: params.iter().map(|p| p.name.clone()).collect(),
        }
    }

    /// Wert und Art des Parameters `name` (klein verglichen).
    pub fn lookup(&self, name: &str) -> Option<(f64, ParamUnit)> {
        self.values.get(&name.to_lowercase()).copied()
    }

    /// Wertet eine Formel mit diesen Parametern aus; das Ergebnis muss zu `want` passen.
    pub fn eval(&self, text: &str, want: ParamUnit) -> Result<f64, String> {
        expr::eval_text(text, want, &|n| self.lookup(n)).map_err(|e| e.0)
    }

    /// Namen aller Parameter in Tabellenreihenfolge, wie geschrieben.
    pub fn names(&self) -> Vec<String> {
        self.names.clone()
    }
}

const COMMENT_TOO_LONG: &str = "Der Kommentar ist zu lang (höchstens 200 Zeichen).";
const UNKNOWN_PARAMETER: &str = "Den Parameter gibt es in diesem Dokument nicht";

/// Prüft die ganze Tabelle und liefert die Werte in Tabellenreihenfolge, sonst die Meldung.
/// Namens-, Anzahl- und Kreisfehler stehen ohne Vorsilbe (sie nennen die Namen selbst),
/// Formel- und Einheitenfehler als „Parameter „{name}“: {meldung}“.
pub(crate) fn check_table(params: &[Parameter]) -> Result<Vec<f64>, String> {
    let defs: Vec<ParamDef<'_>> = params
        .iter()
        .map(|p| ParamDef {
            name: &p.name,
            formula: &p.formula,
            unit: p.unit,
        })
        .collect();
    let values = expr::evaluate_table(&defs).map_err(|e| table_message(params, e))?;
    if params
        .iter()
        .any(|p| p.comment.chars().count() > MAX_COMMENT_CHARS)
    {
        return Err(COMMENT_TOO_LONG.into());
    }
    Ok(values)
}

fn table_message(params: &[Parameter], e: TableError) -> String {
    if params.len() > MAX_PARAMETERS {
        return e.message;
    }
    let Some(p) = params.get(e.index) else {
        return e.message;
    };
    let name_problem = expr::check_name(&p.name).is_err()
        || params[..e.index]
            .iter()
            .any(|q| expr::same_name(&q.name, &p.name));
    let names_itself = e.message == format!("Der Parameter „{}“ hängt von sich selbst ab.", p.name)
        || (e.message.starts_with("Die Parameter „")
            && e.message.ends_with("hängen im Kreis voneinander ab."));
    if name_problem || names_itself {
        e.message
    } else {
        format!("Parameter „{}“: {}", p.name, e.message)
    }
}

impl Document {
    /// Alle Parameter in Tabellenreihenfolge.
    pub fn parameters(&self) -> &[Parameter] {
        &self.parameters
    }

    pub fn parameter(&self, id: ParamId) -> Option<&Parameter> {
        self.parameters.iter().find(|p| p.id == id)
    }

    /// Berechneter Wert des Parameters.
    pub fn parameter_value(&self, id: ParamId) -> Option<f64> {
        let idx = self.parameters.iter().position(|p| p.id == id)?;
        self.param_values.get(idx).copied()
    }

    /// Die berechneten Parameter zum Auswerten von Formeln.
    pub fn param_scope(&self) -> ParamScope {
        ParamScope::new(&self.parameters, &self.param_values)
    }

    /// Legt einen Parameter am Ende der Tabelle an.
    pub fn add_parameter(
        &mut self,
        name: &str,
        formula: &str,
        unit: ParamUnit,
        comment: &str,
    ) -> Result<ParamId, FeatureError> {
        let id = ParamId(self.next_param);
        let next_param = self.next_param.checked_add(1).ok_or_else(|| {
            FeatureError::InvalidInput("Die Parameter-Kennungen sind aufgebraucht".into())
        })?;
        let mut table = self.parameters.clone();
        table.push(Parameter {
            id,
            name: name.to_string(),
            formula: formula.to_string(),
            unit,
            comment: comment.to_string(),
        });
        self.apply_table(table, next_param, Vec::new(), None)?;
        Ok(id)
    }

    /// Neue Zeile mit Vorgaben (Plan P13): `ParameterN` mit dem kleinsten freien N ≥ 1 (ohne
    /// Rücksicht auf Groß-/Kleinschreibung), Formel „10“, Länge, ohne Kommentar.
    pub fn add_parameter_default(&mut self) -> Result<ParamId, FeatureError> {
        let name = (1..)
            .map(|n| format!("Parameter{n}"))
            .find(|c| !self.parameters.iter().any(|p| expr::same_name(&p.name, c)))
            .expect("höchstens 500 Namen belegt");
        self.add_parameter(&name, "10", ParamUnit::Length, "")
    }

    /// Ändert eine Zeile (Name, Formel, Art, Kommentar) und rechnet neu. Ein neuer Name wird
    /// in allen Parameterformeln ersetzt (ganze Namen, siehe [`expr::rename_in`]). Ist `edit`
    /// gleich der Zeile, ändert sich nichts (kein Zähler, keine Neuberechnung, leerer Bericht).
    pub fn set_parameter(
        &mut self,
        id: ParamId,
        edit: ParamEdit,
    ) -> Result<RecomputeReport, FeatureError> {
        let idx = self.param_index(id)?;
        let old = &self.parameters[idx];
        if old.name == edit.name
            && old.formula == edit.formula
            && old.unit == edit.unit
            && old.comment == edit.comment
        {
            return Ok(RecomputeReport::default());
        }
        let old_name = old.name.clone();
        // Ein neuer Name, den eine andere Zeile schon trägt: Meldung mit dem getippten Namen
        // (die Tabellenprüfung nennt sonst den Namen der späteren Zeile).
        if self
            .parameters
            .iter()
            .enumerate()
            .any(|(i, p)| i != idx && expr::same_name(&p.name, &edit.name))
        {
            return Err(FeatureError::Formula(format!(
                "Den Namen „{}“ gibt es schon.",
                edit.name
            )));
        }
        let mut table = self.parameters.clone();
        let mut features = Vec::new();
        let renamed = old_name != edit.name;
        if renamed {
            for (i, p) in table.iter_mut().enumerate() {
                if i != idx {
                    p.formula = expr::rename_in(&p.formula, &old_name, &edit.name);
                }
            }
            // Feature- und Skizzenformeln ebenso (P6: ganze Namen, übriger Text zeichengleich).
            for (i, f) in self.features.iter().enumerate() {
                if mentions(&f.kind, &f.formulas, &old_name) {
                    let (kind, formulas) = rewrite_formulas(&f.kind, &f.formulas, |t| {
                        Some(expr::rename_in(t, &old_name, &edit.name))
                    });
                    features.push((i, kind, formulas));
                }
            }
        }
        let new_name = edit.name.clone();
        table[idx] = Parameter {
            id,
            name: edit.name,
            formula: edit.formula,
            unit: edit.unit,
            comment: edit.comment,
        };
        let renamed = renamed.then_some((old_name.as_str(), new_name.as_str()));
        self.apply_table(table, self.next_param, features, renamed)
    }

    /// Löscht einen Parameter; gesperrt, solange eine Formel ihn nennt
    /// ([`FeatureError::ParameterInUse`]).
    pub fn remove_parameter(&mut self, id: ParamId) -> Result<(), FeatureError> {
        let idx = self.param_index(id)?;
        let users = self.parameter_users(id);
        if !users.is_empty() {
            return Err(FeatureError::ParameterInUse {
                name: self.parameters[idx].name.clone(),
                users,
            });
        }
        let mut table = self.parameters.clone();
        table.remove(idx);
        self.apply_table(table, self.next_param, Vec::new(), None)
            .map(|_| ())
    }

    /// „Durch den Wert ersetzen“ (Plan P7): Jede Formel, die den Parameter direkt nennt, wird
    /// durch ihren aktuellen Wert ersetzt ([`expr::value_text`]), dann wird der Parameter
    /// gelöscht. Ein Schritt; kein Wert ändert sich. Lässt sich ein Wert nicht als Formel
    /// schreiben, wird abgelehnt und nichts ändert sich.
    pub fn replace_parameter_by_value(&mut self, id: ParamId) -> Result<(), FeatureError> {
        let idx = self.param_index(id)?;
        let name = self.parameters[idx].name.clone();
        let mut table = self.parameters.clone();
        for (i, p) in table.iter_mut().enumerate() {
            if i != idx && expr::references(&p.formula, &name) {
                p.formula = expr::value_text(self.param_values[i]).map_err(|e| {
                    FeatureError::Formula(format!("Parameter „{}“: {}", p.name, e.0))
                })?;
            }
        }
        table.remove(idx);
        // Feature-Felder und Skizzenmaße: Die Formel entfällt, Feld bzw. Maß trägt schon ihren
        // Wert.
        let features = self
            .features
            .iter()
            .enumerate()
            .filter(|(_, f)| mentions(&f.kind, &f.formulas, &name))
            .map(|(i, f)| {
                let (kind, kept) = rewrite_formulas(&f.kind, &f.formulas, |t| {
                    (!expr::references(t, &name)).then(|| t.to_string())
                });
                (i, kind, kept)
            })
            .collect();
        self.apply_table(table, self.next_param, features, None)
            .map(|_| ())
    }

    /// Wo der Parameter direkt benutzt wird, für Meldungen: zuerst die Namen der Features, deren
    /// Formeln ihn nennen (Zeitleistenreihenfolge), dann „Parameter {name}“ je Parameter, dessen
    /// Formel ihn nennt (Tabellenreihenfolge). Leer bei unbekannter Kennung.
    pub fn parameter_users(&self, id: ParamId) -> Vec<String> {
        let Some(me) = self.parameter(id) else {
            return Vec::new();
        };
        let features = self
            .features
            .iter()
            .filter(|f| mentions(&f.kind, &f.formulas, &me.name))
            .map(|f| f.name.clone());
        let params = self
            .parameters
            .iter()
            .filter(|p| p.id != id && expr::references(&p.formula, &me.name))
            .map(|p| format!("Parameter {}", p.name));
        features.chain(params).collect()
    }

    fn param_index(&self, id: ParamId) -> Result<usize, FeatureError> {
        self.parameters
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| FeatureError::InvalidInput(UNKNOWN_PARAMETER.into()))
    }

    /// Prüft die neue Tabelle und mit ihren Werten alle Feature-Formeln (`features`: neue Art
    /// mit Skizzenformeln und neue Feldformeln je Feature-Index, etwa nach dem Umbenennen; die
    /// Art unterscheidet sich nur in den Skizzenformeln; P3: kein neuer Formelfehler) und
    /// übernimmt erst dann alles zusammen; zählt eine Änderung am Modell und rechnet neu (der
    /// Durchgang schreibt dabei die neuen Formelwerte in die Felder). `renamed` (alt, neu) lässt
    /// schon bestehende Formelfehler auch in umgeschriebener Form gelten.
    fn apply_table(
        &mut self,
        table: Vec<Parameter>,
        next_param: u64,
        features: Vec<(usize, FeatureKind, Formulas)>,
        renamed: Option<(&str, &str)>,
    ) -> Result<RecomputeReport, FeatureError> {
        let values = check_table(&table).map_err(FeatureError::Formula)?;
        let scope = ParamScope::new(&table, &values);
        let known = self.broken_set(renamed);
        for (i, f) in self.features.iter().enumerate() {
            let (kind, formulas) = features
                .iter()
                .find(|(j, _, _)| *j == i)
                .map_or((&f.kind, &f.formulas), |(_, k, t)| (k, t));
            Document::check_new_broken(Some(f.id), &f.name, kind, formulas, &scope, &known)?;
        }
        self.parameters = table;
        self.param_values = values;
        self.next_param = next_param;
        for (i, kind, formulas) in features {
            self.features[i].kind = kind;
            self.features[i].formulas = formulas;
        }
        self.edits += 1;
        Ok(self.recompute())
    }
}
