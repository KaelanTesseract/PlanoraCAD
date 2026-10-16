//! Maße einer Zeichnung im Dokument: Hinzufügen, Ändern, Löschen, Prüfen und Auflösen gegen
//! die exakten Körper.

use crate::compute::MAX_COORD_MM;
use crate::drawings::MAX_POS_MM;
use crate::error::FeatureError;
use crate::georef::resolve_geo;
use crate::model::Document;
use planora_drawing::annotate::ResolvedDims;
use planora_drawing::dimension::{
    ChainMode, DimId, DimKind, DimSpec, Geo3, GeoRef, GeoRefData, TagData,
    Tolerance as DimTolerance, ref_count,
};
use planora_drawing::limits::MAX_DIMENSIONS;
use planora_drawing::stroke::extent_of;
use planora_drawing::{Basis, DrawBody, Drawing, DrawingId, ViewId, ViewKind, measure};
use planora_kernel::Solid;
use planora_math::{DVec3, Tolerance};
use std::collections::{HashMap, HashSet};
use std::convert::Infallible;

/// Höchstzahl der Nachkommastellen eines Maßes.
pub const MAX_DECIMALS: u8 = 3;
/// Höchstzahl der Zeichen eines eigenen Maßtextes.
pub const MAX_DIM_TEXT_CHARS: usize = 200;
/// Höchstzahl der Zeichen einer Passungsangabe (z. B. „H7“).
pub const MAX_FIT_CHARS: usize = 50;
/// Größter Betrag eines Toleranzwerts in Millimetern.
pub const MAX_TOL_MM: f64 = 1000.0;
/// Längste Rolle einer Flächenmarke in einem Maßbezug (die Rollen des Programms sind kurz,
/// z. B. „Loch 3, Wand 12“).
pub const MAX_ROLE_CHARS: usize = 64;
/// Größter Betrag einer Koordinate eines Hinweispunkts (wie bei den übrigen Verweisen in der
/// Datei: Ursprung plus Größe am Rand des Rechenbereichs).
pub const MAX_HINT_MM: f64 = 2.0 * MAX_COORD_MM;

fn bad(m: impl Into<String>) -> FeatureError {
    FeatureError::InvalidInput(m.into())
}

/// Die Daten des Bezugs passen zu seiner Art (Kantenbezüge mit Kantendaten, Flächenbezüge mit
/// Flächendaten), und alle Hinweispunkte sind endlich mit Beträgen höchstens [`MAX_HINT_MM`].
fn ref_ok(r: &GeoRef) -> bool {
    let p = |v: DVec3| v.is_finite() && v.abs().max_element() <= MAX_HINT_MM;
    let data_ok = |d: &GeoRefData| match d {
        GeoRefData::Face(t) => p(t.hint),
        GeoRefData::Edge { a, b, hint } => p(a.hint) && p(b.hint) && p(*hint),
    };
    match r {
        GeoRef::Edge(d @ GeoRefData::Edge { .. })
        | GeoRef::EdgeEnd {
            edge: d @ GeoRefData::Edge { .. },
            ..
        }
        | GeoRef::Center(d @ GeoRefData::Edge { .. })
        | GeoRef::Face(d @ GeoRefData::Face(_)) => data_ok(d),
        _ => false,
    }
}

/// Alle Flächenmarken des Bezugs haben Rollen mit höchstens [`MAX_ROLE_CHARS`] Zeichen.
fn roles_ok(r: &GeoRef) -> bool {
    let short = |t: &TagData| t.role.chars().count() <= MAX_ROLE_CHARS;
    let data = match r {
        GeoRef::Edge(d) | GeoRef::EdgeEnd { edge: d, .. } | GeoRef::Center(d) | GeoRef::Face(d) => {
            d
        }
    };
    match data {
        GeoRefData::Face(t) => short(t),
        GeoRefData::Edge { a, b, .. } => short(a) && short(b),
    }
}

/// Prüft die Zahlen und Texte eines einzelnen Maßes.
fn validate_spec(m: &DimSpec) -> Result<(), String> {
    let (lo, hi) = ref_count(m.kind);
    if m.refs.len() < lo || m.refs.len() > hi {
        return Err("Ein Maß hat die falsche Zahl von Bezügen".into());
    }
    if !m.refs.iter().all(ref_ok) {
        return Err("Ein Bezug eines Maßes ist ungültig".into());
    }
    if !m.refs.iter().all(roles_ok) {
        return Err(format!(
            "Die Rolle einer Fläche ist zu lang (höchstens {MAX_ROLE_CHARS} Zeichen)"
        ));
    }
    let pos = |x: f64| x.is_finite() && x.abs() <= MAX_POS_MM;
    if !pos(m.offset) {
        return Err("Der Abstand der Maßlinie liegt außerhalb des Blatts".into());
    }
    if !pos(m.angle) {
        return Err("Der Winkel der Führungslinie ist ungültig".into());
    }
    if !pos(m.text_shift.0) || !pos(m.text_shift.1) {
        return Err("Die Verschiebung des Maßtextes liegt außerhalb des Blatts".into());
    }
    if m.decimals > MAX_DECIMALS {
        return Err(format!(
            "Ein Maß darf höchstens {MAX_DECIMALS} Nachkommastellen haben"
        ));
    }
    if m.text_override
        .as_ref()
        .is_some_and(|t| t.chars().count() > MAX_DIM_TEXT_CHARS)
    {
        return Err(format!(
            "Der eigene Maßtext ist zu lang (höchstens {MAX_DIM_TEXT_CHARS} Zeichen)"
        ));
    }
    let tol_ok = |x: f64| x.is_finite() && x.abs() <= MAX_TOL_MM;
    let tolerance_ok = match &m.tolerance {
        DimTolerance::None => true,
        DimTolerance::Symmetric(v) => tol_ok(*v),
        DimTolerance::Limits { upper, lower } | DimTolerance::LimitSizes { upper, lower } => {
            tol_ok(*upper) && tol_ok(*lower)
        }
        DimTolerance::Fit(text) => {
            if text.chars().count() > MAX_FIT_CHARS {
                return Err(format!(
                    "Die Passungsangabe ist zu lang (höchstens {MAX_FIT_CHARS} Zeichen)"
                ));
            }
            true
        }
    };
    if !tolerance_ok {
        return Err(format!(
            "Die Toleranz eines Maßes ist ungültig (Beträge höchstens {MAX_TOL_MM} mm)"
        ));
    }
    Ok(())
}

/// Prüft die Maße einer Zeichnung (Teil von [`crate::validate_drawing`]): Kennungen eindeutig,
/// größer als null und kleiner als der Maßzähler; Ansicht vorhanden und keine Isometrie;
/// Bezugszahl, Zahlen und Texte (siehe `validate_spec`); Maße einer Gruppe mit gleicher Art,
/// Ansicht und Kettenart (Kette oder Bezugsmaße), Reihenfolge in der Gruppe eindeutig; höchstens [`MAX_DIMENSIONS`] Maße.
pub(crate) fn validate_dimensions(d: &Drawing) -> Result<(), String> {
    if d.next_dim == 0 || d.next_dim == u64::MAX {
        return Err("Der Maßzähler der Zeichnung ist ungültig".into());
    }
    if d.dimensions.len() > MAX_DIMENSIONS {
        return Err(format!(
            "Eine Zeichnung darf höchstens {MAX_DIMENSIONS} Maße haben"
        ));
    }
    let views: HashMap<ViewId, ViewKind> = d.views.iter().map(|v| (v.id, v.kind)).collect();
    let mut ids = HashSet::with_capacity(d.dimensions.len());
    let mut groups: HashMap<u64, (DimKind, ViewId, ChainMode)> = HashMap::new();
    let mut orders: HashSet<(u64, u32)> = HashSet::new();
    for m in &d.dimensions {
        if m.id.0 == 0 || m.id.0 >= d.next_dim {
            return Err("Ein Maß hat eine ungültige Kennung".into());
        }
        if !ids.insert(m.id) {
            return Err("Eine Maßkennung kommt doppelt vor".into());
        }
        match views.get(&m.view) {
            None => return Err("Ein Maß gehört zu keiner Ansicht der Zeichnung".into()),
            Some(kind) if !kind.is_normal() => {
                return Err("In der isometrischen Ansicht gibt es keine Maße".into());
            }
            Some(_) => {}
        }
        validate_spec(m)?;
        if let Some(c) = m.chain {
            let first = *groups.entry(c.group).or_insert((m.kind, m.view, c.mode));
            if first.2 != c.mode {
                return Err("Die Maße einer Kette mischen Kettenmaße und Bezugsmaße".into());
            }
            if (first.0, first.1) != (m.kind, m.view) {
                return Err(
                    "Die Maße einer Kette müssen dieselbe Art haben und in derselben Ansicht liegen"
                        .into(),
                );
            }
            if !orders.insert((c.group, c.order)) {
                return Err("Eine Stelle in einer Maßkette kommt doppelt vor".into());
            }
        }
    }
    Ok(())
}

impl Document {
    /// Fügt einer Zeichnung ein Maß hinzu; die Kennung wird vergeben (`spec.id` wird ersetzt).
    /// Bei ungültigem Maß bleibt das Dokument unverändert.
    pub fn add_dimension(
        &mut self,
        drawing: DrawingId,
        spec: DimSpec,
    ) -> Result<DimId, FeatureError> {
        let mut new_id = None;
        self.update_drawing(drawing, |d| {
            if d.next_dim != u64::MAX {
                new_id = Some(d.add_dimension(spec));
            }
        })?;
        new_id.ok_or_else(|| bad("Der Maßzähler der Zeichnung ist erschöpft"))
    }

    /// Ändert ein Maß auf einer Kopie und prüft das Ergebnis; die Kennung lässt sich nicht
    /// ändern. Bei Fehler bleibt das Dokument unverändert.
    pub fn update_dimension(
        &mut self,
        drawing: DrawingId,
        id: DimId,
        f: impl FnOnce(&mut DimSpec),
    ) -> Result<(), FeatureError> {
        let mut spec = self.dimension_spec(drawing, id)?.clone();
        f(&mut spec);
        if spec.id != id {
            return Err(bad("Die Kennung eines Maßes lässt sich nicht ändern"));
        }
        self.update_drawing(drawing, |d| {
            if let Some(m) = d.dimensions.iter_mut().find(|m| m.id == id) {
                *m = spec;
            }
        })
    }

    /// Löscht ein Maß.
    pub fn delete_dimension(&mut self, drawing: DrawingId, id: DimId) -> Result<(), FeatureError> {
        self.dimension_spec(drawing, id)?;
        self.update_drawing(drawing, |d| {
            d.remove_dimension(id);
        })
    }

    fn dimension_spec(&self, drawing: DrawingId, id: DimId) -> Result<&DimSpec, FeatureError> {
        self.drawing(drawing)
            .ok_or_else(|| bad("Die Zeichnung gibt es nicht"))?
            .dimension(id)
            .ok_or_else(|| bad("Das Maß gibt es nicht"))
    }

    /// Löst alle Maße der Zeichnung gegen die Körper `solids` auf und misst sie: je Maß die
    /// Bezüge über [`resolve_geo`], dann [`measure`] mit der Basis der Ansicht und der
    /// Ausdehnung der Körper. Fehler bleiben im Ergebnis des einzelnen Maßes (kein Abbruch);
    /// `last_known` bleibt leer (das führt die Oberfläche).
    pub fn resolve_dimensions(
        &self,
        drawing: DrawingId,
        solids: &[&Solid],
    ) -> Result<ResolvedDims, FeatureError> {
        let d = self
            .drawing(drawing)
            .ok_or_else(|| bad("Die Zeichnung gibt es nicht"))?;
        match resolve_drawing_dimensions(d, solids, || Ok::<(), Infallible>(())) {
            Ok(out) => Ok(out),
            Err(never) => match never {},
        }
    }
}

/// Wie [`Document::resolve_dimensions`], aber für eine Zeichnung ohne Dokument (etwa eine Kopie
/// im Arbeitsthread des Exports). `check` wird vor jedem Maß gerufen; liefert es einen Fehler
/// (z. B. „Abgebrochen“), endet die Auflösung mit diesem Fehler.
pub fn resolve_drawing_dimensions<E>(
    d: &Drawing,
    solids: &[&Solid],
    mut check: impl FnMut() -> Result<(), E>,
) -> Result<ResolvedDims, E> {
    let tol = Tolerance::default();
    let bodies: Vec<DrawBody> = solids
        .iter()
        .map(|s| DrawBody {
            name: String::new(),
            solid: s,
        })
        .collect();
    let extent = extent_of(&bodies);
    let mut out = ResolvedDims::default();
    for m in &d.dimensions {
        check()?;
        // Die Prüfung der Zeichnung stellt sicher, dass die Ansicht existiert.
        let Some(view) = d.view(m.view) else {
            continue;
        };
        let result = m
            .refs
            .iter()
            .map(|r| resolve_geo(solids, r, &tol))
            .collect::<Result<Vec<Geo3>, _>>()
            .and_then(|geos| measure(m.kind, &geos, &Basis::of(view.kind), view.kind, extent));
        out.by_view
            .entry(m.view)
            .or_default()
            .push((m.clone(), result));
    }
    Ok(out)
}
