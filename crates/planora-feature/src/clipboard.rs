//! Zwischenablage am Skizzen-Feature (Skizzen 11b, Spec 1): Die Auswahl einer Skizze wird als
//! [`FeatureClip`] abgelegt (Werte, keine Ids der Quelle) und in eine beliebige Skizze eines
//! beliebigen Dokuments eingefügt. Zur Skizze kommen hier die Ebene (Lage), die Maßformeln
//! und die Kantenkopien (Projektionen) dazu.
//!
//! - Kopieren: Ziele aus `copies` in der Auswahl werden freie Kurven/Punkte (die Skizzenablage
//!   hängt an nichts; `made_free` zählt sie). Jede Formel an einem kopierten Maß geht mit ihrem
//!   gespeicherten Wert mit.
//! - Einfügen: 2D-Inhalt kommt an dieselben x/y der Zielebene; 3D-Inhalt in eine 3D-Skizze an
//!   dieselben Weltkoordinaten; 3D-Inhalt in eine 2D-Skizze schaltet sie auf 3D (dieselben
//!   x/y/z). Eine Formel kommt mit, wenn sie sich im Ziel in der Einheit des Maßes rechnen lässt
//!   (Parameter vorhanden), sonst bleibt der Wert (Spec-Entscheidung 3). Nie ein Eintrag in
//!   `copies`. Bei einem Fehler bleibt die Skizze unverändert.

use crate::expr::ParamUnit;
use crate::formulas::constraint_unit;
use crate::model::{CopyTarget, SketchFeature};
use crate::params::ParamScope;
use planora_math::{DMat3, DQuat, Frame};
use planora_sketch::clipboard::ClipSet;
use planora_sketch::constraint::ConstraintId;
use planora_sketch::error::SketchError;
use planora_sketch::model::Sketch;
use planora_sketch::text::TextId;
use planora_sketch::transform::{CopyMap, Rigid3, TransformSet};
use std::collections::HashSet;

/// Wohin eine abgelegte Formel gehört (Ids im Clip).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipSlot {
    Dim(ConstraintId),
    /// Die Höhe eines Textes (Skizzen 11b, Texthöhe als Formel).
    TextHeight(TextId),
}

/// Eine Formel in der Ablage mit dem Wert, den das Maß (bzw. die Texthöhe) beim Kopieren hatte
/// (gespeichert, bei Winkeln im Bogenmaß).
#[derive(Clone, Debug, PartialEq)]
pub struct ClipFormula {
    pub slot: ClipSlot,
    pub text: String,
    pub value: f64,
}

/// Die Zwischenablage des Skizzen-Features.
#[derive(Clone, Debug)]
pub struct FeatureClip {
    pub sketch: Sketch,
    /// Weltlage der Quellebene.
    pub frame: Frame,
    pub formulas: Vec<ClipFormula>,
    /// Wie viele Projektionsziele in der Auswahl lagen und nun freie Geometrie sind.
    pub made_free: usize,
}

/// Was das Einfügen getan hat.
#[derive(Clone, Debug, PartialEq)]
pub struct PasteReport {
    /// Abbildung Clip → Ziel.
    pub map: CopyMap,
    /// Formeln, die im Ziel nicht rechenbar waren; das Maß trägt nur noch den Wert.
    pub formulas_dropped: usize,
    /// Die Zielskizze wurde auf 3D umgeschaltet.
    pub made_3d: bool,
    /// Abhängigkeiten, die das Verschieben bei 3D in 3D entfernt hat (`MoveReport::removed_constraints`).
    pub constraints_dropped: usize,
}

/// Legt die Auswahl der Skizze `sf` (Ebene mit der Weltlage `frame`) ab.
pub fn copy_selection(
    sf: &SketchFeature,
    frame: &Frame,
    set: &ClipSet,
) -> Result<FeatureClip, SketchError> {
    let ex = sf.sketch.extract(set)?;
    let made_free = sf
        .copies
        .iter()
        .filter(|c| match c.target {
            CopyTarget::Curve(id) => ex.map.curves.contains_key(&id),
            CopyTarget::Point(id) => ex.map.points.contains_key(&id),
        })
        .map(|c| c.target)
        .collect::<Vec<_>>();
    let mut distinct: Vec<CopyTarget> = Vec::new();
    for t in made_free {
        if !distinct.contains(&t) {
            distinct.push(t);
        }
    }
    let mut formulas = Vec::new();
    for (id, text) in &sf.formulas {
        let Some(&clip_id) = ex.map.constraints.get(id) else {
            continue;
        };
        let Some(value) = ex.clip.constraint(clip_id).and_then(|c| c.kind.value()) else {
            continue;
        };
        formulas.push(ClipFormula {
            slot: ClipSlot::Dim(clip_id),
            text: text.clone(),
            value,
        });
    }
    for (id, text) in &sf.text_formulas {
        let Some(&clip_id) = ex.map.texts.get(id) else {
            continue;
        };
        let Some(value) = ex.clip.text(clip_id).map(|t| t.height) else {
            continue;
        };
        formulas.push(ClipFormula {
            slot: ClipSlot::TextHeight(clip_id),
            text: text.clone(),
            value,
        });
    }
    Ok(FeatureClip {
        sketch: ex.clip,
        frame: *frame,
        formulas,
        made_free: distinct.len(),
    })
}

/// Die Bewegung der Skizzenkoordinaten von Ebene `from` nach Ebene `to` (gleiche Weltpunkte).
fn frame_move(from: &Frame, to: &Frame) -> Rigid3 {
    let rc = DMat3::from_cols(from.x, from.y, from.z);
    let rt = DMat3::from_cols(to.x, to.y, to.z);
    let rt_inv = rt.transpose();
    Rigid3 {
        rot: DQuat::from_mat3(&(rt_inv * rc)).normalize(),
        shift: rt_inv * (from.origin - to.origin),
        pivot: planora_math::DVec3::ZERO,
    }
}

/// Fügt die Ablage in `sf` ein (Ebene mit der Weltlage `frame`); `scope` rechnet die Formeln.
pub fn paste_clip(
    sf: &mut SketchFeature,
    frame: &Frame,
    clip: &FeatureClip,
    scope: &ParamScope,
) -> Result<PasteReport, SketchError> {
    let mut work = sf.clone();
    let three = clip.sketch.has_3d_entry();
    let made_3d = three && !work.sketch.has_3d_entry();
    if made_3d {
        work.sketch.set_three_d(true);
    }
    let map = work.sketch.paste_from(&clip.sketch)?;
    let mut constraints_dropped = 0;
    // 3D-Inhalt in eine schon 3D-Skizze: dieselben Weltpunkte.
    if three && !made_3d {
        let m = frame_move(&clip.frame, frame);
        let tol = *work.sketch.tolerance();
        let moves =
            m.rot.angle_between(DQuat::IDENTITY) > tol.angular || m.shift.length() > tol.linear;
        if moves {
            let set = TransformSet {
                points: map.points.values().copied().collect(),
                curves: map.curves.values().copied().collect(),
            };
            constraints_dropped = work
                .sketch
                .move_rigid3(&set, &m, false)?
                .removed_constraints;
        }
    }
    let mut formulas_dropped = 0;
    let mut seen: HashSet<ConstraintId> = HashSet::new();
    for f in &clip.formulas {
        let clip_id = match f.slot {
            ClipSlot::Dim(clip_id) => clip_id,
            ClipSlot::TextHeight(clip_text) => {
                // Dieselbe Regel wie bei Maßen: Rechnet die Formel im Ziel nicht (Parameter
                // fehlt), bleibt nur der Wert, und die Meldung zählt es.
                let ok = map.texts.get(&clip_text).copied().filter(|&t| {
                    work.sketch.text(t).is_some()
                        && scope
                            .eval(&f.text, ParamUnit::Length)
                            .is_ok_and(|v| v.is_finite() && v > work.sketch.tolerance().linear)
                });
                match ok {
                    Some(t) => {
                        work.text_formulas.insert(t, f.text.clone());
                    }
                    None => formulas_dropped += 1,
                }
                continue;
            }
        };
        let Some(&id) = map.constraints.get(&clip_id) else {
            formulas_dropped += 1;
            continue;
        };
        // Fehlt das Maß nach dem Verschieben, zählt es als entfernte Abhängigkeit, nicht als Formel.
        let Some(c) = work.sketch.constraint(id) else {
            continue;
        };
        let ok = c.kind.is_dimension()
            && !c.driven
            && scope.eval(&f.text, constraint_unit(&c.kind)).is_ok();
        if ok && seen.insert(id) {
            work.formulas.insert(id, f.text.clone());
        } else {
            formulas_dropped += 1;
        }
    }
    *sf = work;
    Ok(PasteReport {
        map,
        formulas_dropped,
        made_3d,
        constraints_dropped,
    })
}
