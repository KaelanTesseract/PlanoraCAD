//! DXF-Schreiber für ebene Modelle (z. B. eine Skizze für Laser oder Fräse): derselbe R2000-Schreiber
//! wie beim Blatt (`dxf.rs`, gleiche Pflichtteile, gleiche Reihenfolge der Abschnitte und
//! Tabellen), aber alles im Modellraum, ohne Blöcke je Ansicht und ohne Blattrahmen.
//!
//! Die Koordinaten stehen schon in der Einheit `unit`; der Schreiber rechnet nichts um, er nennt
//! die Einheit nur im Kopf (`$INSUNITS`, `$MEASUREMENT`). `$EXTMIN`/`$EXTMAX` folgen aus den
//! Objekten. Ein leeres Modell ist ein Fehler, es entsteht keine Datei.

use crate::dxf::{
    Handles, Ids, LayerSpec, Out, View, assemble, header, layer_name, write_curve,
    write_space_blocks, write_tables,
};
use crate::error::DrawError;
use crate::out2d::Out2d;
use planora_i18n::tr;
use planora_math::DVec2;
use std::collections::BTreeSet;
use std::io::Write;

/// Zeicheneinheit der Datei (`$INSUNITS`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DxfUnit {
    Millimeter,
    Centimeter,
    Meter,
    Inch,
    Foot,
}

impl DxfUnit {
    /// Millimeter je Einheit.
    pub fn mm_per_unit(self) -> f64 {
        match self {
            DxfUnit::Millimeter => 1.0,
            DxfUnit::Centimeter => 10.0,
            DxfUnit::Meter => 1000.0,
            DxfUnit::Inch => 25.4,
            DxfUnit::Foot => 304.8,
        }
    }

    /// Wert von `$INSUNITS`.
    pub fn insunits(self) -> i16 {
        match self {
            DxfUnit::Millimeter => 4,
            DxfUnit::Centimeter => 5,
            DxfUnit::Meter => 6,
            DxfUnit::Inch => 1,
            DxfUnit::Foot => 2,
        }
    }

    /// `$MEASUREMENT`: metrisch (1) oder englisch (0).
    pub fn metric(self) -> bool {
        !matches!(self, DxfUnit::Inch | DxfUnit::Foot)
    }
}

/// Eine Ebene mit AutoCAD-Farbnummer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlainLayer {
    pub name: String,
    pub color: i16,
}

/// Ein Objekt des ebenen Modells.
#[derive(Clone, Debug, PartialEq)]
pub enum PlainEntity {
    Curve(Out2d),
    Point(DVec2),
}

/// Ebenes Modell; `entities` tragen den Index ihrer Ebene in `layers`.
#[derive(Clone, Debug, PartialEq)]
pub struct PlainDxf {
    pub unit: DxfUnit,
    pub layers: Vec<PlainLayer>,
    pub entities: Vec<(usize, PlainEntity)>,
}

/// Schreibt das ebene Modell als DXF. Bei einem Fehler wird nichts geschrieben.
pub fn write_plain_dxf(m: &PlainDxf, out: &mut dyn Write) -> Result<(), DrawError> {
    let text = plain_dxf_string(m)?;
    out.write_all(text.as_bytes())?;
    Ok(())
}

/// Das ebene Modell als DXF-Text.
pub fn plain_dxf_string(m: &PlainDxf) -> Result<String, DrawError> {
    if m.entities.is_empty() {
        return Err(DrawError::Unsupported(tr!("drawing-err-plain-empty")));
    }
    if m.entities.iter().any(|(i, _)| *i >= m.layers.len()) {
        return Err(DrawError::Unsupported(tr!("drawing-err-plain-layer")));
    }
    let (min, max) = extent(m)?;

    // Ebenen: „0“ gibt es immer (Block- und Rahmenebene); Namen eindeutig ohne Rücksicht auf
    // Groß-/Kleinschreibung.
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut specs: Vec<LayerSpec> = Vec::new();
    let mut spec = |name: String, color: i64, seen: &mut BTreeSet<String>| {
        let mut unique = name.clone();
        let mut n = 1;
        while !seen.insert(unique.to_lowercase()) {
            n += 1;
            unique = format!("{name}_{n}");
        }
        specs.push(LayerSpec {
            name: unique.clone(),
            ltype: "CONTINUOUS",
            color,
            weight: -3,
        });
        unique
    };
    let mut names: Vec<String> = Vec::with_capacity(m.layers.len());
    let has_zero = m.layers.iter().any(|l| layer_name(&l.name) == "0");
    if !has_zero {
        spec("0".to_string(), 7, &mut seen);
    }
    for l in &m.layers {
        let base = layer_name(&l.name);
        let base = if base.is_empty() {
            "_".to_string()
        } else {
            base
        };
        names.push(spec(base, i64::from(l.color), &mut seen));
    }

    let mut hs = Handles::new();
    let Ids {
        t,
        br_model,
        br_paper,
        br_views,
        dict,
    } = Ids::alloc(&mut hs, 0);
    let (w, h) = ((max.x - min.x).max(1.0), (max.y - min.y).max(1.0));
    let mut tables = Out::new();
    write_tables(
        &mut tables,
        &mut hs,
        &t,
        View {
            center: (min + max) * 0.5,
            w,
            h,
        },
        &[],
        [br_model, br_paper],
        &br_views,
        &specs,
        false,
    )?;
    let mut blocks = Out::new();
    write_space_blocks(&mut blocks, &mut hs, [br_model, br_paper])?;

    let mut entities = Out::new();
    for (layer, e) in &m.entities {
        let layer = names[*layer].as_str();
        match e {
            PlainEntity::Curve(c) => write_curve(&mut entities, &mut hs, br_model, layer, c)?,
            PlainEntity::Point(p) => {
                entities.entity_head("POINT", hs.next(), br_model, layer);
                entities.text(100, "AcDbPoint");
                entities.point(10, *p)?;
            }
        }
    }
    let head = header(
        i64::from(m.unit.insunits()),
        m.unit.metric(),
        min,
        max,
        hs.seed(),
    )?;
    Ok(assemble(&head, &tables, &blocks, &entities, &dict))
}

/// Umschließendes Rechteck aller Objekte; nicht endliche Zahlen sind ein Fehler.
fn extent(m: &PlainDxf) -> Result<(DVec2, DVec2), DrawError> {
    let mut min = DVec2::splat(f64::INFINITY);
    let mut max = DVec2::splat(f64::NEG_INFINITY);
    for (_, e) in &m.entities {
        let (a, b) = match e {
            PlainEntity::Curve(c) => {
                if !c.is_finite() {
                    return Err(DrawError::NonFinite);
                }
                c.bounds()
            }
            PlainEntity::Point(p) => (*p, *p),
        };
        if !(a.is_finite() && b.is_finite()) {
            return Err(DrawError::NonFinite);
        }
        min = min.min(a);
        max = max.max(b);
    }
    Ok((min, max))
}
