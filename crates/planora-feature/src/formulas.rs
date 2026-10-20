//! Formeln in den Zahlenfeldern der Features (Spec §4a.1, Plan P1–P3).
//!
//! Die Formeln liegen als Text in einer Nebentabelle je Feature ([`Formulas`], Feldschlüssel →
//! Text); die `f64`-Felder der [`FeatureKind`] bleiben der zuletzt berechnete Wert. Vor jeder
//! Neuberechnung wertet ein Durchgang alle Formeln mit den aktuellen Parametern aus, schreibt die
//! Werte in die Felder und gibt nur Features mit geänderten Werten (bitgenau) oder geändertem
//! Formelfehler eine neue Version; so bleibt der Zwischenspeicher richtig und Unverändertes wird
//! nicht neu gerechnet.
//!
//! Fehler je Formel (P3): „Formelfehler“ ([`FormulaIssue::Broken`], die Formel selbst lässt sich
//! nicht auswerten) oder „Wertfehler“ ([`FormulaIssue::Value`], das Ergebnis passt nicht, etwa
//! eine Breite ≤ 0; der Wert wird trotzdem geschrieben). Ein Feature mit Fehler bekommt den
//! Status `Error(meldung)` und kein Ergebnis. Öffentliche Änderungen werden vorher auf einer Kopie
//! geprüft und abgelehnt, wenn danach eine Formel einen Formelfehler hätte, den sie vorher nicht
//! hatte; beim Laden entstehen Formelfehler ohne Ablehnung.

use crate::error::FeatureError;
use crate::expr::{self, ParamUnit};
use crate::model::{Document, FeatureId, FeatureKind, MAX_MESH_SCALE, MIN_MESH_SCALE};
use crate::params::ParamScope;
use std::collections::{BTreeMap, HashSet};

/// Ein formelfähiges Zahlenfeld eines Features.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FieldKey {
    Breite,
    Tiefe,
    Hoehe,
    Radius,
    X,
    Y,
    Z,
    Abstand,
    Abstand1,
    Abstand2,
    Winkel,
    Massstab,
}

/// Formeln eines Features: Feld → Formeltext.
pub type Formulas = BTreeMap<FieldKey, String>;

impl FieldKey {
    const ALL: [FieldKey; 12] = [
        FieldKey::Breite,
        FieldKey::Tiefe,
        FieldKey::Hoehe,
        FieldKey::Radius,
        FieldKey::X,
        FieldKey::Y,
        FieldKey::Z,
        FieldKey::Abstand,
        FieldKey::Abstand1,
        FieldKey::Abstand2,
        FieldKey::Winkel,
        FieldKey::Massstab,
    ];

    /// Schlüssel in der Projektdatei.
    pub fn file_key(self) -> &'static str {
        match self {
            FieldKey::Breite => "breite",
            FieldKey::Tiefe => "tiefe",
            FieldKey::Hoehe => "hoehe",
            FieldKey::Radius => "radius",
            FieldKey::X => "x",
            FieldKey::Y => "y",
            FieldKey::Z => "z",
            FieldKey::Abstand => "abstand",
            FieldKey::Abstand1 => "abstand1",
            FieldKey::Abstand2 => "abstand2",
            FieldKey::Winkel => "winkel",
            FieldKey::Massstab => "massstab",
        }
    }

    /// Umkehrung von [`FieldKey::file_key`] (genaue Schreibweise).
    pub fn from_file_key(s: &str) -> Option<FieldKey> {
        FieldKey::ALL.into_iter().find(|k| k.file_key() == s)
    }
}

/// Die formelfähigen Felder der Art, in der Reihenfolge des Fensters „bearbeiten“.
pub fn field_keys(kind: &FeatureKind) -> Vec<FieldKey> {
    use FieldKey::*;
    match kind {
        FeatureKind::Box { .. } => vec![Breite, Tiefe, Hoehe, X, Y, Z],
        FeatureKind::Cylinder { .. } | FeatureKind::Cone { .. } => vec![Radius, Hoehe, X, Y, Z],
        FeatureKind::Sphere { .. } => vec![Radius, X, Y, Z],
        FeatureKind::Extrude { .. } => vec![Hoehe],
        FeatureKind::Move { .. } => vec![X, Y, Z],
        // Die Drehung bleibt ein Ziehfeld (P10).
        FeatureKind::MeshBody { .. } => vec![X, Y, Z, Massstab],
        FeatureKind::Sketch(_)
        | FeatureKind::Revolve { .. }
        | FeatureKind::Boolean { .. }
        | FeatureKind::Verrunden { .. } => Vec::new(),
    }
}

/// Bezeichnung des Feldes wie im Fenster „bearbeiten“.
pub fn field_label(kind: &FeatureKind, key: FieldKey) -> &'static str {
    match (kind, key) {
        (FeatureKind::Move { .. }, FieldKey::X) => "Δx",
        (FeatureKind::Move { .. }, FieldKey::Y) => "Δy",
        (FeatureKind::Move { .. }, FieldKey::Z) => "Δz",
        (_, FieldKey::Breite) => "Breite",
        (_, FieldKey::Tiefe) => "Tiefe",
        (_, FieldKey::Hoehe) => "Höhe",
        (_, FieldKey::Radius) => "Radius",
        (_, FieldKey::X) => "Position x",
        (_, FieldKey::Y) => "Position y",
        (_, FieldKey::Z) => "Position z",
        (_, FieldKey::Abstand) => "Abstand",
        (_, FieldKey::Abstand1) => "Abstand 1",
        (_, FieldKey::Abstand2) => "Abstand 2",
        (_, FieldKey::Winkel) => "Winkel",
        (_, FieldKey::Massstab) => "Maßstab",
    }
}

/// Einheit, die eine Formel in diesem Feld ergeben muss (eine reine Zahl passt immer, P4).
pub fn field_unit(_kind: &FeatureKind, key: FieldKey) -> ParamUnit {
    match key {
        FieldKey::Winkel => ParamUnit::Angle,
        FieldKey::Massstab => ParamUnit::Number,
        _ => ParamUnit::Length,
    }
}

/// Die **eine** Zuordnung Feldschlüssel → Feld der Art. `$r` ist `&` oder `&mut`; daraus
/// entstehen [`field_ref`] (Lesen) und [`field_mut`] (Schreiben), damit jedes Feld nur einmal
/// eingetragen wird (Aufgaben 5–6 ergänzen hier).
macro_rules! field_table {
    ($kind:expr, $key:expr, $($r:tt)+) => {{
        use FieldKey::*;
        match $kind {
            FeatureKind::Box { size, origin } => match $key {
                Breite => Some($($r)+ size.x),
                Tiefe => Some($($r)+ size.y),
                Hoehe => Some($($r)+ size.z),
                X => Some($($r)+ origin.x),
                Y => Some($($r)+ origin.y),
                Z => Some($($r)+ origin.z),
                _ => None,
            },
            FeatureKind::Cylinder {
                radius,
                height,
                origin,
            }
            | FeatureKind::Cone {
                radius,
                height,
                origin,
            } => match $key {
                Radius => Some($($r)+ *radius),
                Hoehe => Some($($r)+ *height),
                X => Some($($r)+ origin.x),
                Y => Some($($r)+ origin.y),
                Z => Some($($r)+ origin.z),
                _ => None,
            },
            FeatureKind::Sphere { radius, origin } => match $key {
                Radius => Some($($r)+ *radius),
                X => Some($($r)+ origin.x),
                Y => Some($($r)+ origin.y),
                Z => Some($($r)+ origin.z),
                _ => None,
            },
            FeatureKind::Extrude { height, .. } => match $key {
                Hoehe => Some($($r)+ *height),
                _ => None,
            },
            FeatureKind::Move { delta, .. } => match $key {
                X => Some($($r)+ delta.x),
                Y => Some($($r)+ delta.y),
                Z => Some($($r)+ delta.z),
                _ => None,
            },
            FeatureKind::MeshBody { offset, scale, .. } => match $key {
                X => Some($($r)+ offset.x),
                Y => Some($($r)+ offset.y),
                Z => Some($($r)+ offset.z),
                Massstab => Some($($r)+ *scale),
                _ => None,
            },
            _ => None,
        }
    }};
}

/// Verweis auf das Feld; `None`, wenn die Art es nicht hat.
fn field_ref(kind: &FeatureKind, key: FieldKey) -> Option<&f64> {
    field_table!(kind, key, &)
}

/// Veränderbarer Verweis auf das Feld; `None`, wenn die Art es nicht hat.
fn field_mut(kind: &mut FeatureKind, key: FieldKey) -> Option<&mut f64> {
    field_table!(kind, key, &mut)
}

/// Wert des Feldes; `None`, wenn die Art es nicht hat.
pub fn field_value(kind: &FeatureKind, key: FieldKey) -> Option<f64> {
    field_ref(kind, key).copied()
}

/// Schreibt den Wert in das Feld; `false`, wenn die Art es nicht hat.
pub fn set_field_value(kind: &mut FeatureKind, key: FieldKey, v: f64) -> bool {
    match field_mut(kind, key) {
        Some(slot) => {
            *slot = v;
            true
        }
        None => false,
    }
}

/// Wertfehler eines Formelergebnisses `v` im Feld, sonst `None`. Maße (Breite, Tiefe, Höhe,
/// Radius, auch die Extrusionshöhe, die der Kernel nur positiv kennt) müssen größer als 0 sein;
/// der Maßstab eines Netz-Körpers muss in `MIN_MESH_SCALE..=MAX_MESH_SCALE` liegen (wie beim
/// Eintippen); Lagen und Verschiebungen dürfen jedes Vorzeichen haben.
fn value_issue(kind: &FeatureKind, key: FieldKey, v: f64, tolerance: f64) -> Option<String> {
    match key {
        FieldKey::Breite | FieldKey::Tiefe | FieldKey::Hoehe | FieldKey::Radius
            if v <= tolerance =>
        {
            Some(format!(
                "{} ergibt {} mm, es muss größer als 0 sein",
                field_label(kind, key),
                expr::short_number(v)
            ))
        }
        FieldKey::Massstab if !(MIN_MESH_SCALE..=MAX_MESH_SCALE).contains(&v) => Some(format!(
            "Maßstab ergibt {}, er muss zwischen 0,000001 und 1 000 000 liegen",
            expr::short_number(v)
        )),
        _ => None,
    }
}

/// Fehler einer Formel nach dem Durchgang (P3).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FormulaIssue {
    /// Die Formel lässt sich nicht auswerten; das Feld behält seinen letzten Wert.
    Broken(String),
    /// Das Ergebnis passt nicht zum Feld; der Wert steht trotzdem im Feld.
    Value(String),
}

impl FormulaIssue {
    pub(crate) fn message(&self) -> &str {
        match self {
            FormulaIssue::Broken(m) | FormulaIssue::Value(m) => m,
        }
    }
}

/// Ergebnis der Auswertung eines Features.
pub(crate) struct Evaluated {
    /// Art mit den Formelwerten (Felder mit Formelfehler unverändert).
    pub(crate) kind: FeatureKind,
    /// Erster Fehler in Feldreihenfolge.
    pub(crate) issue: Option<FormulaIssue>,
}

/// Wertet alle Formeln eines Features aus. Schlüssel, die die Art nicht hat, bleiben
/// unbeachtet (sie werden an den Eingängen abgelehnt).
pub(crate) fn evaluate(
    kind: &FeatureKind,
    formulas: &Formulas,
    scope: &ParamScope,
    tolerance: f64,
) -> Evaluated {
    let mut out = kind.clone();
    let mut issue = None;
    if formulas.is_empty() {
        return Evaluated { kind: out, issue };
    }
    for key in field_keys(kind) {
        let Some(text) = formulas.get(&key) else {
            continue;
        };
        match scope.eval(text, field_unit(kind, key)) {
            Err(m) => {
                issue.get_or_insert(FormulaIssue::Broken(m));
            }
            Ok(v) => {
                set_field_value(&mut out, key, v);
                if let Some(m) = value_issue(kind, key, v, tolerance) {
                    issue.get_or_insert(FormulaIssue::Value(m));
                }
            }
        }
    }
    Evaluated { kind: out, issue }
}

/// Alle Schlüssel müssen Felder der Art sein.
pub(crate) fn check_keys(kind: &FeatureKind, formulas: &Formulas) -> Result<(), FeatureError> {
    let keys = field_keys(kind);
    if formulas.keys().all(|k| keys.contains(k)) {
        Ok(())
    } else {
        Err(FeatureError::InvalidInput(
            "Dieses Feld kann keine Formel haben".into(),
        ))
    }
}

/// Gleiche Felder bitgenau (−0 ≠ 0, NaN = NaN mit gleichem Bitmuster).
pub(crate) fn same_fields(a: &FeatureKind, b: &FeatureKind, key: FieldKey) -> bool {
    field_value(a, key).map(f64::to_bits) == field_value(b, key).map(f64::to_bits)
}

/// Formelfehler, die schon bestehen: (Feature, Feld, Formeltext).
pub(crate) type BrokenSet = HashSet<(FeatureId, FieldKey, String)>;

impl Document {
    /// Die Formeln, die mit den aktuellen Parametern einen Formelfehler haben. Mit `renamed`
    /// (alter, neuer Parametername) stehen die Texte so darin, wie das Umbenennen sie umschreibt.
    pub(crate) fn broken_set(&self, renamed: Option<(&str, &str)>) -> BrokenSet {
        let scope = self.param_scope();
        let mut set = BrokenSet::new();
        for f in &self.features {
            for (key, text) in &f.formulas {
                if scope.eval(text, field_unit(&f.kind, *key)).is_err() {
                    let text = match renamed {
                        Some((old, new)) => expr::rename_in(text, old, new),
                        None => text.clone(),
                    };
                    set.insert((f.id, *key, text));
                }
            }
        }
        set
    }

    /// Strenge Prüfung eines Kandidaten (P3): Lehnt ab, wenn eine Formel mit `scope` einen
    /// Formelfehler hat, der nicht schon in `known` steht. Meldung „{Feature}, {Feld}: {Text}“.
    pub(crate) fn check_new_broken(
        id: Option<FeatureId>,
        name: &str,
        kind: &FeatureKind,
        formulas: &Formulas,
        scope: &ParamScope,
        known: &BrokenSet,
    ) -> Result<(), FeatureError> {
        for key in field_keys(kind) {
            let Some(text) = formulas.get(&key) else {
                continue;
            };
            if let Err(m) = scope.eval(text, field_unit(kind, key))
                && !id.is_some_and(|id| known.contains(&(id, key, text.clone())))
            {
                return Err(FeatureError::Formula(format!(
                    "{name}, {}: {m}",
                    field_label(kind, key)
                )));
            }
        }
        Ok(())
    }

    /// Auswertungsdurchgang (P1): schreibt die Formelwerte aller Features in ihre Felder und
    /// setzt die Formelfehler. Nur Features mit bitgenau geänderten Werten oder geändertem
    /// Fehler bekommen eine neue Version.
    pub(crate) fn apply_formula_pass(&mut self) {
        let scope = self.param_scope();
        let tolerance = self.kernel.tolerance.linear;
        for idx in 0..self.features.len() {
            let f = &self.features[idx];
            if f.formulas.is_empty() && f.issue.is_none() {
                continue;
            }
            let ev = evaluate(&f.kind, &f.formulas, &scope, tolerance);
            let changed = field_keys(&f.kind)
                .into_iter()
                .any(|k| !same_fields(&f.kind, &ev.kind, k));
            if changed || ev.issue != f.issue {
                let version = self.bump();
                let f = &mut self.features[idx];
                f.kind = ev.kind;
                f.issue = ev.issue;
                f.version = version;
            }
        }
    }

    /// Wie [`Document::add_feature`], mit Formeln. Schlüssel, die die Art nicht hat →
    /// [`FeatureError::InvalidInput`]; eine Formel, die sich nicht auswerten lässt →
    /// [`FeatureError::Formula`] („{name}, {Feld}: {Meldung}“). Bei einem Fehler bleibt das
    /// Dokument unverändert. Ein unzulässiges Ergebnis (etwa Breite ≤ 0) ist keine Ablehnung,
    /// sondern ergibt den Fehlerstatus.
    pub fn add_feature_with_formulas(
        &mut self,
        name: impl Into<String>,
        kind: FeatureKind,
        formulas: Formulas,
    ) -> Result<FeatureId, FeatureError> {
        let name = name.into();
        self.check_references(&kind, self.marker)?;
        check_keys(&kind, &formulas)?;
        Document::check_new_broken(
            None,
            &name,
            &kind,
            &formulas,
            &self.param_scope(),
            &BrokenSet::new(),
        )?;
        self.insert_feature(name, kind, formulas)
    }

    /// Ändert Art und Formeln eines Features in einem Schritt; `formulas` ersetzt die ganze
    /// Tabelle des Features. Prüfungen wie [`Document::update_with_report`] und
    /// [`Document::add_feature_with_formulas`]; ein Formelfehler, den das Feature schon vorher
    /// mit demselben Text hatte (etwa nach dem Laden), wird geduldet.
    pub fn update_with_formulas(
        &mut self,
        id: FeatureId,
        f: impl FnOnce(&mut FeatureKind),
        formulas: Formulas,
    ) -> Result<crate::RecomputeReport, FeatureError> {
        let (idx, kind) = self.updated_kind(id, f)?;
        check_keys(&kind, &formulas)?;
        self.commit_update(idx, kind, formulas)
    }
}
