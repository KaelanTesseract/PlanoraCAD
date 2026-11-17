//! Skizze als ebenes DXF-Modell (Skizzen 11b, Spec 2): Aus der berechneten Skizze eines
//! Skizzen-Features entsteht ein [`PlainDxf`] für Laser oder Fräse. Die Schreibweise der Datei
//! übernimmt `planora_drawing::plain_dxf`; hier wird nur gewählt und umgerechnet.
//!
//! Koordinaten sind die lokalen x/y der Skizze (die Lage der Ebene im Raum bleibt außen vor),
//! umgerechnet von Millimeter in die gewählte Einheit. Texte stehen als ihre Buchstabenkurven
//! in der Datei, nie als TEXT. Ebene „0“ (Farbe 7) trägt die Geometrie, die Ebene
//! „Konstruktion“ (Farbe 8) die Konstruktions- und Mittellinien samt Rahmenlinien der Texte.

use crate::model::{CopyTarget, Document, FeatureId, FeatureKind};
use planora_drawing::basis::Basis;
use planora_drawing::out2d::piece_to_outs;
use planora_drawing::plain_dxf::{DxfUnit, PlainDxf, PlainEntity, PlainLayer};
use planora_drawing::sheet::SheetTransform;
use planora_drawing::stroke::{Stroke, StrokeOrigin};
use planora_i18n::tr;
use planora_math::DVec2;
use std::collections::HashSet;

/// Auswahl beim Speichern: Einheit und welche Geometriearten mitkommen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SketchDxfOptions {
    pub unit: DxfUnit,
    /// Freie Punkte (keine Kurve benutzt sie) als POINT.
    pub points: bool,
    /// Projizierte und einbezogene Geometrie (Ziele der Kantenkopien der Skizze).
    pub projected: bool,
    /// Konstruktions- und Mittellinien (und die Rahmenlinien der Texte) auf eigener Ebene.
    pub construction: bool,
}

/// Warum aus der Skizze kein DXF-Modell wird.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SketchDxfError {
    /// Die Skizze enthält 3D-Geometrie.
    NotPlanar,
    /// Nach dem Filter bleibt nichts übrig.
    Empty,
    /// Das Feature ist keine berechnete Skizze.
    Unknown,
}

impl SketchDxfError {
    /// Die Meldung für die Oberfläche in der aktuellen Sprache.
    pub fn message(self) -> String {
        match self {
            SketchDxfError::NotPlanar => tr!("sketch-dxf-not-planar"),
            SketchDxfError::Empty => tr!("sketch-dxf-empty"),
            SketchDxfError::Unknown => tr!("sketch-dxf-unknown"),
        }
    }
}

/// Das ebene DXF-Modell der berechneten Skizze `id`.
pub fn sketch_dxf(
    doc: &Document,
    id: FeatureId,
    o: &SketchDxfOptions,
) -> Result<PlainDxf, SketchDxfError> {
    let sk = doc.sketch_of(id).ok_or(SketchDxfError::Unknown)?;
    let FeatureKind::Sketch(def) = &doc.feature(id).ok_or(SketchDxfError::Unknown)?.kind else {
        return Err(SketchDxfError::Unknown);
    };
    // Nur lebender Inhalt sperrt: ein gelöschter 3D-Eintrag oder der Haken „3D-Skizze“ allein
    // nicht (`texts()`, `curves()` und `points()` liefern nur lebende Einträge).
    let three_d = sk.curves().any(|(c, _)| sk.is_3d_curve(c))
        || sk.points().any(|(p, _)| sk.is_3d_point(p))
        || sk.texts().any(|(_, t)| t.normal.is_some());
    if three_d {
        return Err(SketchDxfError::NotPlanar);
    }
    let projected_curves: HashSet<_> = def
        .copies
        .iter()
        .filter_map(|c| match c.target {
            CopyTarget::Curve(c) => Some(c),
            CopyTarget::Point(_) => None,
        })
        .collect();
    let projected_points: HashSet<_> = def
        .copies
        .iter()
        .filter_map(|c| match c.target {
            CopyTarget::Point(p) => Some(p),
            CopyTarget::Curve(_) => None,
        })
        .collect();

    // Lokale Koordinaten in der Einheit: Der Ursprung der Skizze liegt bei (0, 0).
    let f = *sk.frame();
    let basis = Basis {
        d: f.z,
        ex: f.x,
        ey: f.y,
    };
    let xf = SheetTransform {
        scale: 1.0 / o.unit.mm_per_unit(),
        tx: 0.0,
        ty: 0.0,
        bbox_center: basis.project(f.origin),
    };

    let mut layers = vec![PlainLayer {
        name: "0".to_string(),
        color: 7,
    }];
    let mut construction_layer: Option<usize> = None;
    let mut entities: Vec<(usize, PlainEntity)> = Vec::new();
    let mut layer_for = |construction: bool, layers: &mut Vec<PlainLayer>| -> usize {
        if !construction {
            return 0;
        }
        *construction_layer.get_or_insert_with(|| {
            layers.push(PlainLayer {
                name: tr!("sketch-dxf-layer-construction"),
                color: 8,
            });
            layers.len() - 1
        })
    };

    for (c, data) in sk.curves() {
        if (data.construction && !o.construction) || (projected_curves.contains(&c) && !o.projected)
        {
            continue;
        }
        let Some((curve, t0, t1)) = sk.curve_world(c) else {
            continue;
        };
        let stroke = Stroke {
            curve,
            t0,
            t1,
            origin: StrokeOrigin::Edge { body: 0, edge: 0 },
        };
        let layer = layer_for(data.construction, &mut layers);
        for out in piece_to_outs(&stroke, &basis, t0, t1, &xf) {
            entities.push((layer, PlainEntity::Curve(out)));
        }
    }

    if o.points || o.projected {
        let used: HashSet<_> = sk
            .curves()
            .flat_map(|(_, d)| d.kind.points())
            .chain(sk.glyph_points())
            .collect();
        for (p, data) in sk.points() {
            let projected = projected_points.contains(&p);
            let wanted = if projected { o.projected } else { o.points };
            if used.contains(&p) || !wanted || (data.construction && !o.construction) {
                continue;
            }
            let Ok(q) = sk.point_xyz(p) else { continue };
            let layer = layer_for(data.construction, &mut layers);
            let at = DVec2::new(q.x, q.y) * xf.scale;
            entities.push((layer, PlainEntity::Point(at)));
        }
    }

    if entities.is_empty() {
        return Err(SketchDxfError::Empty);
    }
    Ok(PlainDxf {
        unit: o.unit,
        layers,
        entities,
    })
}
