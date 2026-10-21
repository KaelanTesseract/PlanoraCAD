//! Einheiten und Genauigkeit eines geometrischen Kontexts (STEP-Import, Aufgabe 2).
//!
//! Längen werden in Millimeter umgerechnet, Winkel in Radiant. Die Längeneinheit steht in
//! `GLOBAL_UNIT_ASSIGNED_CONTEXT` (`SI_UNIT` mit Vorsatz oder `CONVERSION_BASED_UNIT`, auch
//! verschachtelt bis [`MAX_UNIT_DEPTH`] Stufen), die Genauigkeit in
//! `GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT` (`UNCERTAINTY_MEASURE_WITH_UNIT`, kleinster Wert).
//! Daraus folgt die Toleranz für das Zusammenziehen (`weld_tol`, Spec §4a.3, P4).

use crate::read::error::StepReadError;
use crate::read::index::StepFile;
use crate::read::value::{Entity, Value};
use std::f64::consts::PI;

/// Genauigkeit, wenn die Datei keine nennt (mm).
pub const DEFAULT_FILE_TOL_MM: f64 = 1e-3;
/// Größte Toleranz für das Zusammenziehen (mm).
pub const MAX_WELD_TOL_MM: f64 = 1e-3;
/// Kleinste Toleranz für das Zusammenziehen (mm).
pub const MIN_WELD_TOL_MM: f64 = 1e-6;
/// Ab dieser Toleranz (mm, ausschließlich) gilt die Datei als grob.
pub const COARSE_TOL_MM: f64 = 1e-5;

/// Höchstens so viele Stufen verschachtelter Einheiten (`CONVERSION_BASED_UNIT` auf
/// `CONVERSION_BASED_UNIT` …).
pub const MAX_UNIT_DEPTH: usize = 8;

/// Relative Abweichung, bis zu der ein Faktor als bekannte Einheit gilt.
const UNIT_MATCH: f64 = 1e-9;

/// Längeneinheit einer Datei.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LengthUnit {
    Millimeter,
    Zentimeter,
    Meter,
    Zoll,
    /// Andere Einheit: so viele Millimeter je Einheit.
    Other {
        mm: f64,
    },
}

impl LengthUnit {
    /// Millimeter je Einheit.
    pub fn mm(self) -> f64 {
        match self {
            LengthUnit::Millimeter => 1.0,
            LengthUnit::Zentimeter => 10.0,
            LengthUnit::Meter => 1000.0,
            LengthUnit::Zoll => 25.4,
            LengthUnit::Other { mm } => mm,
        }
    }

    /// Name für die Oberfläche.
    pub fn label(self) -> String {
        match self {
            LengthUnit::Millimeter => "Millimeter".into(),
            LengthUnit::Zentimeter => "Zentimeter".into(),
            LengthUnit::Meter => "Meter".into(),
            LengthUnit::Zoll => "Zoll".into(),
            LengthUnit::Other { mm } => format!("1 Einheit = {} mm", decimal(mm)),
        }
    }

    /// Bekannte Einheit, wenn `mm` ihr bis auf relativ 1e-9 gleicht, sonst `Other`.
    pub fn from_mm(mm: f64) -> LengthUnit {
        [
            LengthUnit::Millimeter,
            LengthUnit::Zentimeter,
            LengthUnit::Meter,
            LengthUnit::Zoll,
        ]
        .into_iter()
        .find(|u| ((mm - u.mm()) / u.mm()).abs() <= UNIT_MATCH)
        .unwrap_or(LengthUnit::Other { mm })
    }
}

/// Zahl mit Komma und höchstens sechs Nachkommastellen, ohne Nullen am Ende
/// (304,79999999999995 → „304,8“); sehr kleine Werte mit Exponent.
fn decimal(x: f64) -> String {
    let s = if x != 0.0 && x.abs() < 1e-4 {
        format!("{x:e}")
    } else {
        let s = format!("{x:.6}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    s.replace('.', ",")
}

/// Einheiten und Genauigkeit eines Kontexts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Units {
    pub length: LengthUnit,
    /// `false`: Die Datei nennt keine Längeneinheit; angenommen werden Millimeter.
    pub length_known: bool,
    /// Radiant je Winkeleinheit der Datei.
    pub angle_rad: f64,
    /// Kleinste Längen-Unsicherheit der Datei in mm. Werte ≤ 0 oder nicht endliche werden
    /// übergangen (dann gilt die Vorgabe und die Datei zählt als grob).
    pub uncertainty_mm: Option<f64>,
    /// `true`: Die Unsicherheit steht in der Längeneinheit des Kontexts und wird beim Umstellen
    /// der Einheit mit umgerechnet; `false`: Sie nennt eine eigene Einheit und bleibt.
    pub uncertainty_follows_length: bool,
}

impl Units {
    /// Millimeter (bekannt), Radiant, keine Unsicherheit.
    pub const MM: Units = Units {
        length: LengthUnit::Millimeter,
        length_known: true,
        angle_rad: 1.0,
        uncertainty_mm: None,
        uncertainty_follows_length: true,
    };

    /// Toleranz für das Zusammenziehen: Unsicherheit der Datei (sonst
    /// [`DEFAULT_FILE_TOL_MM`]), begrenzt auf [`MIN_WELD_TOL_MM`] … [`MAX_WELD_TOL_MM`].
    pub fn weld_tol_mm(&self) -> f64 {
        self.uncertainty_mm
            .unwrap_or(DEFAULT_FILE_TOL_MM)
            .clamp(MIN_WELD_TOL_MM, MAX_WELD_TOL_MM)
    }

    /// Grob: keine Unsicherheit genannt oder Toleranz größer als [`COARSE_TOL_MM`].
    pub fn is_coarse(&self) -> bool {
        self.uncertainty_mm.is_none() || self.weld_tol_mm() > COARSE_TOL_MM
    }

    /// Andere Längeneinheit (P11): Eine Unsicherheit in der Längeneinheit des Kontexts wird mit
    /// umgerechnet, eine mit eigener Einheit (etwa ausdrücklich mm in einer Zoll-Datei) bleibt;
    /// der Winkel bleibt.
    pub fn with_length(self, l: LengthUnit) -> Units {
        let uncertainty_mm = if self.uncertainty_follows_length {
            self.uncertainty_mm.map(|u| u / self.length.mm() * l.mm())
        } else {
            self.uncertainty_mm
        };
        Units {
            length: l,
            length_known: true,
            uncertainty_mm,
            ..self
        }
    }
}

/// Art einer Einheit.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Length,
    Angle,
    Other,
}

/// SI-Vorsätze in Millimeter je Meter-Vielfachem (`$` = Meter = 1000 mm).
fn prefix_mm(prefix: Option<&str>) -> Option<f64> {
    Some(match prefix {
        None => 1e3,
        Some("EXA") => 1e21,
        Some("PETA") => 1e18,
        Some("TERA") => 1e15,
        Some("GIGA") => 1e12,
        Some("MEGA") => 1e9,
        Some("KILO") => 1e6,
        Some("HECTO") => 1e5,
        Some("DECA") => 1e4,
        Some("DECI") => 100.0,
        Some("CENTI") => 10.0,
        Some("MILLI") => 1.0,
        Some("MICRO") => 1e-3,
        Some("NANO") => 1e-6,
        Some("PICO") => 1e-9,
        Some("FEMTO") => 1e-12,
        Some("ATTO") => 1e-15,
        Some(_) => return None,
    })
}

/// Die Parameter von `SI_UNIT`: zwei (zusammengesetzter Eintrag) oder drei (einfacher Eintrag
/// mit der geerbten Dimension vorneweg).
fn si_args(args: &[Value]) -> Option<(&Value, &Value)> {
    match args {
        [p, n] | [_, p, n] => Some((p, n)),
        _ => None,
    }
}

/// Wert und Einheit eines `…MEASURE_WITH_UNIT` (einfach oder als Teil eines zusammengesetzten
/// Eintrags).
fn measure_args(e: &Entity) -> Option<(&Value, &Value)> {
    [
        "MEASURE_WITH_UNIT",
        "LENGTH_MEASURE_WITH_UNIT",
        "PLANE_ANGLE_MEASURE_WITH_UNIT",
        "UNCERTAINTY_MEASURE_WITH_UNIT",
    ]
    .iter()
    .filter_map(|n| e.part(n))
    .find(|a| a.len() >= 2)
    .map(|a| (&a[0], &a[1]))
}

/// Art und Faktor einer Einheit: Länge in mm, Winkel in rad; andere Arten mit Faktor 1.
fn unit_value(file: &StepFile, id: u64, depth: usize) -> Result<(Kind, f64), StepReadError> {
    if depth > MAX_UNIT_DEPTH {
        return Err(file.fail(
            id,
            format!("Die Einheit ist zu tief verschachtelt (höchstens {MAX_UNIT_DEPTH} Stufen)."),
        ));
    }
    let e = file.entity(id)?;
    if let Some(args) = e.part("SI_UNIT") {
        let Some((prefix, name)) = si_args(args) else {
            return Err(file.fail(id, "SI_UNIT: Die Parameter passen nicht."));
        };
        let prefix = if prefix.is_unset() {
            None
        } else {
            Some(prefix.enumeration().map_err(|m| file.fail(id, m))?)
        };
        let name = name.enumeration().map_err(|m| file.fail(id, m))?;
        let kind = match name {
            "METRE" => Kind::Length,
            "RADIAN" => Kind::Angle,
            _ => return Ok((Kind::Other, 1.0)),
        };
        let Some(mm) = prefix_mm(prefix) else {
            return Err(file.fail(
                id,
                format!(
                    "Der Vorsatz „{}“ ist unbekannt.",
                    prefix.unwrap_or_default()
                ),
            ));
        };
        return Ok(match kind {
            Kind::Length => (Kind::Length, mm),
            _ => (Kind::Angle, snap_angle(mm / 1e3)),
        });
    }
    if let Some(args) = e.part("CONVERSION_BASED_UNIT") {
        let Some(factor) = args.last() else {
            return Err(file.fail(id, "CONVERSION_BASED_UNIT: Der Umrechnungswert fehlt."));
        };
        let m = factor.reference().map_err(|msg| file.fail(id, msg))?;
        let me = file.entity(m)?;
        let Some((value, unit)) = measure_args(&me) else {
            return Err(file.fail(m, "Erwartet wird ein Wert mit Einheit."));
        };
        let value = value.measure().map_err(|msg| file.fail(m, msg))?;
        let unit = unit.reference().map_err(|msg| file.fail(m, msg))?;
        let (base, f) = unit_value(file, unit, depth + 1)?;
        let declared = if e.is("LENGTH_UNIT") || me.is("LENGTH_MEASURE_WITH_UNIT") {
            Kind::Length
        } else if e.is("PLANE_ANGLE_UNIT") || me.is("PLANE_ANGLE_MEASURE_WITH_UNIT") {
            Kind::Angle
        } else {
            base
        };
        if base != declared {
            return Err(file.fail(
                id,
                "Die Einheit wird aus einer Einheit anderer Art umgerechnet.",
            ));
        }
        let x = value * f;
        if !x.is_finite() {
            return Err(file.fail(
                id,
                "Der Umrechnungswert der Einheit ist keine endliche Zahl.",
            ));
        }
        if x <= 0.0 {
            return Err(file.fail(
                id,
                "Der Umrechnungswert der Einheit ist nicht größer als null.",
            ));
        }
        // Bekannte Einheiten je Stufe genau (2,54 · 10 mm = 25,4 mm, Grad = π/180), damit
        // verschachtelte Einheiten und mehrere Schreibweisen derselben Einheit übereinstimmen.
        let x = match declared {
            Kind::Length => LengthUnit::from_mm(x).mm(),
            Kind::Angle => snap_angle(x),
            Kind::Other => x,
        };
        return Ok((declared, x));
    }
    Ok((Kind::Other, 1.0))
}

/// Grad (π/180) genau, wenn `rad` ihm bis auf relativ 1e-9 gleicht.
fn snap_angle(rad: f64) -> f64 {
    let degree = PI / 180.0;
    if ((rad - degree) / degree).abs() <= UNIT_MATCH {
        degree
    } else {
        rad
    }
}

/// Einheiten und Genauigkeit des Kontexts `#context`. Fehlt die Längeneinheit, gelten
/// Millimeter mit `length_known: false`; fehlt der Winkel, Radiant. Mehrere verschiedene
/// Einheiten derselben Art sind ein Fehler; eine Unsicherheit ≤ 0 oder nicht endlich wird
/// übergangen.
pub fn context_units(file: &StepFile, context: u64) -> Result<Units, StepReadError> {
    let ctx = file.entity(context)?;
    let mut length: Option<f64> = None;
    let mut angle: Option<f64> = None;
    if let Some(args) = ctx.part("GLOBAL_UNIT_ASSIGNED_CONTEXT") {
        let list = args
            .first()
            .ok_or_else(|| file.fail(context, "Die Liste der Einheiten fehlt."))?
            .refs()
            .map_err(|m| file.fail(context, m))?;
        for u in list {
            let (kind, f) = unit_value(file, u, 0)?;
            let slot = match kind {
                Kind::Length => &mut length,
                Kind::Angle => &mut angle,
                Kind::Other => continue,
            };
            match slot {
                Some(old) if *old != f => {
                    return Err(file.fail(
                        context,
                        "Der Kontext nennt mehrere verschiedene Einheiten derselben Art.",
                    ));
                }
                _ => *slot = Some(f),
            }
        }
    }
    let (unit, known) = match length {
        Some(mm) => (LengthUnit::from_mm(mm), true),
        None => (LengthUnit::Millimeter, false),
    };
    let angle_rad = angle.unwrap_or(1.0);
    // Kleinste Unsicherheit in mm und ob sie in der Längeneinheit des Kontexts steht.
    let mut uncertainty: Option<(f64, bool)> = None;
    if let Some(args) = ctx.part("GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT") {
        let list = args
            .first()
            .ok_or_else(|| file.fail(context, "Die Liste der Genauigkeiten fehlt."))?
            .refs()
            .map_err(|m| file.fail(context, m))?;
        for u in list {
            let e = file.entity(u)?;
            let Some((value, unit)) = measure_args(&e) else {
                return Err(file.fail(u, "Erwartet wird eine Genauigkeit mit Einheit."));
            };
            let unit = unit.reference().map_err(|m| file.fail(u, m))?;
            let (kind, f) = unit_value(file, unit, 0)?;
            if kind != Kind::Length {
                continue;
            }
            let value = value.measure().map_err(|m| file.fail(u, m))?;
            let mm = value * f;
            if !(mm.is_finite() && mm > 0.0) {
                // Unbrauchbar: übergehen; ohne andere gilt die Vorgabe (Genauigkeit unbekannt).
                continue;
            }
            let follows = length == Some(f);
            if uncertainty.is_none_or(|(old, _)| mm < old) {
                uncertainty = Some((mm, follows));
            }
        }
    }
    Ok(Units {
        length: unit,
        length_known: known,
        angle_rad,
        uncertainty_mm: uncertainty.map(|(mm, _)| mm),
        uncertainty_follows_length: uncertainty.is_none_or(|(_, follows)| follows),
    })
}
