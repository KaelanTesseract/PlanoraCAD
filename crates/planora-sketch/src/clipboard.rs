//! Zwischenablage im Skizzenmodell (Skizzen 11b, Spec 1): Die Auswahl wird als eigene, neue
//! Skizze abgelegt ([`Sketch::extract`]) und in eine beliebige Skizze neu eingefügt
//! ([`Sketch::paste_from`]). Die Ablage hält Werte, keine Ids der Quelle: Sie hat eigene, dichte
//! Ids und hängt an nichts außerhalb.
//!
//! Beide Richtungen gehen über dieselbe innere Funktion `graft`: Sie überträgt die gewählten,
//! lebenden Einträge einer fremden Skizze roh (Stil, Zweig, Gegenwinkel, `driven`, `enabled`,
//! Zusatzvariable, interne Abhängigkeiten) mit neuen Ids und neuen Variablen. Was dabei
//! nicht mitgeht: `Fixed`/`Fixed3` (die Kopie ist frei wie bei `copy_transformed`, **Annahme**;
//! nur die Enden eines festen Splines bleiben fest, sonst wäre er kein fester Spline mehr),
//! Musterwerte und Muster (die Exemplare werden gewöhnliche Geometrie) und jede Abhängigkeit, die
//! etwas außerhalb der Auswahl berührt. Geprüft wird das Ergebnis wie beim Laden
//! ([`Sketch::from_data`]).

use crate::constraint::{ConstraintData, ConstraintId, ConstraintKind};
use crate::error::SketchError;
use crate::model::{CurveData, CurveId, CurveKind, HandleCurvature, PointData, PointId, Sketch};
use crate::text::{TextData, TextId, TextPlacement};
use crate::transform::{CopyMap, map_curve, map_ids};
use planora_i18n::tr;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

/// Die Auswahl, die in die Zwischenablage geht: Kurven, einzelne Punkte und Texte (ein Text
/// bringt seine Buchstaben und beim Rahmen die Rahmenlinien mit).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ClipSet {
    pub curves: Vec<CurveId>,
    pub points: Vec<PointId>,
    pub texts: Vec<TextId>,
}

/// Ergebnis von [`Sketch::extract`]: die abgelegte Skizze und die Abbildung Quelle → Clip.
#[derive(Clone, Debug)]
pub struct Extracted {
    pub clip: Sketch,
    pub map: CopyMap,
}

/// Was `graft` aus der Quelle überträgt (Ids der Quelle).
struct Pick {
    curves: HashSet<CurveId>,
    points: HashSet<PointId>,
    texts: HashSet<TextId>,
}

impl Sketch {
    /// Legt die Auswahl als eigene, neue Skizze ab (gleiche Ebene und Toleranz, ohne Namen):
    /// Kurven mit Stil, Punkte, Texte mit Rahmen und ihren internen Abhängigkeiten, alle
    /// Abhängigkeiten und Maße, deren Beteiligte alle in der Menge liegen. Die Menge wächst wie
    /// beim Verschieben ([`Sketch::transform_set`]: Hilfsgeometrie, Griffe, ganze Texte). Ist der
    /// Inhalt 3D, ist der Clip eine 3D-Skizze. Die Quelle bleibt unverändert.
    ///
    /// Fehler: leere Menge (`sketch-nothing-selected`), Text auf Pfad ohne seinen Pfad
    /// (`sketch-text-path-follows`).
    pub fn extract(&self, set: &ClipSet) -> Result<Extracted, SketchError> {
        let mut curves = set.curves.clone();
        let mut texts: Vec<TextId> = Vec::new();
        for &t in &set.texts {
            let Some(d) = self.texts.get(t).filter(|d| d.alive) else {
                continue;
            };
            texts.push(t);
            curves.extend(&d.curves);
            match d.placement {
                TextPlacement::Box { lines, .. } => curves.extend(lines),
                TextPlacement::Path { .. } => {}
            }
        }
        let grown = self.transform_set(&curves, &set.points);
        for t in self.texts_in_set(&grown) {
            if !texts.contains(&t) {
                texts.push(t);
            }
        }
        if grown.points.is_empty() && texts.is_empty() {
            return Err(SketchError::InvalidInput(tr!("sketch-nothing-selected")));
        }
        if self.text_off_its_path(&grown, &texts) {
            return Err(SketchError::InvalidInput(tr!("sketch-text-path-follows")));
        }
        let pick = Pick {
            curves: grown.curves.iter().copied().collect(),
            points: grown.points.iter().copied().collect(),
            texts: texts.into_iter().collect(),
        };
        let mut clip = Sketch::new(*self.frame(), "", *self.tolerance());
        let map = clip.graft(self, &pick)?;
        // Nur der Inhalt zählt: Ist er 3D, wird der Clip eine 3D-Skizze.
        clip.three_d = clip.has_3d_entry();
        // Durch das Laden geprüft und frisch: dichte Ids, Zwischenspeicher leer.
        let clip = Sketch::from_data(clip.to_data())?;
        Ok(Extracted { clip, map })
    }

    /// Legt alles Lebende aus `clip` mit neuen Ids in dieser Skizze an, mit denselben Werten
    /// (2D: dieselben x/y in der Skizzenebene). Vorhandene Ids und der übrige Inhalt bleiben,
    /// gesetzte Texte behalten ihre Form; die neuen Texte sind noch nicht gesetzt, ihre Kurven
    /// stehen. Atomar: Bei einem Fehler bleibt die Skizze unverändert. Liefert die Abbildung
    /// Clip → Ziel.
    ///
    /// 3D-Inhalt in eine 2D-Skizze gibt `sketch-err-tool-not-in-3d`; das Umschalten zur
    /// 3D-Skizze ist Sache des Aufrufers.
    pub fn paste_from(&mut self, clip: &Sketch) -> Result<CopyMap, SketchError> {
        if clip.has_3d_entry() && !self.three_d {
            return Err(SketchError::InvalidInput(tr!("sketch-err-tool-not-in-3d")));
        }
        let pick = Pick {
            curves: clip.curves().map(|(c, _)| c).collect(),
            points: clip.points().map(|(p, _)| p).collect(),
            texts: clip.texts().map(|(t, _)| t).collect(),
        };
        if pick.curves.is_empty() && pick.points.is_empty() && pick.texts.is_empty() {
            return Err(SketchError::InvalidInput(tr!("sketch-nothing-selected")));
        }
        self.atomically(|sk| {
            let map = sk.graft(clip, &pick)?;
            // Dieselbe Prüfung wie beim Laden, damit nichts Halbes in der Skizze steht.
            Sketch::from_data(sk.to_data())?;
            Ok(map)
        })
    }

    /// Überträgt die Einträge aus `pick` von `src` in diese Skizze (gemeinsamer Weg von
    /// `extract` und `paste_from`). Die neuen Ids folgen der Reihenfolge in der Quelle. Zu
    /// jeder Kurve und jedem Text kommen ihre Punkte mit; Abhängigkeiten nur, wenn alle
    /// Beteiligten dabei sind. Ohne Prüfung des Ganzen; das macht der Aufrufer.
    fn graft(&mut self, src: &Sketch, pick: &Pick) -> Result<CopyMap, SketchError> {
        // Was mitgeht, in der Reihenfolge der Quelle.
        let curves: Vec<CurveId> = src
            .curves()
            .map(|(c, _)| c)
            .filter(|c| pick.curves.contains(c))
            .collect();
        let texts: Vec<TextId> = src
            .texts()
            .map(|(t, _)| t)
            .filter(|t| pick.texts.contains(t))
            .collect();
        let mut wanted: HashSet<PointId> = pick.points.clone();
        for &c in &curves {
            wanted.extend(src.curves[c].kind.points());
        }
        for &t in &texts {
            let d = &src.texts[t];
            wanted.extend(&d.points);
            if let TextPlacement::Box { corners, .. } = d.placement {
                wanted.extend(corners);
            }
        }
        let points: Vec<PointId> = src
            .points()
            .map(|(p, _)| p)
            .filter(|p| wanted.contains(p))
            .collect();

        // Die neuen Ids stehen vorab fest: angehängt in dieser Reihenfolge.
        let point_map: HashMap<PointId, PointId> = points
            .iter()
            .enumerate()
            .map(|(i, &p)| (p, PointId::from_index(self.points.len() + i)))
            .collect();
        let curve_map: HashMap<CurveId, CurveId> = curves
            .iter()
            .enumerate()
            .map(|(i, &c)| (c, CurveId::from_index(self.curves.len() + i)))
            .collect();
        let p = |x: PointId| point_map.get(&x).copied();
        let c = |x: CurveId| curve_map.get(&x).copied();

        // Variablen werden beim ersten Gebrauch kopiert; geteilte (Halbachsen der Bögen einer
        // Ellipse) bleiben geteilt.
        let base = self.vars.len();
        let vars = RefCell::new((HashMap::<usize, usize>::new(), Vec::<f64>::new()));
        let var = |i: usize| -> usize {
            let mut guard = vars.borrow_mut();
            let (map, out) = &mut *guard;
            *map.entry(i).or_insert_with(|| {
                out.push(src.vars[i]);
                base + out.len() - 1
            })
        };

        let mut new_points = Vec::with_capacity(points.len());
        for &q in &points {
            let d = &src.points[q];
            new_points.push(PointData {
                x: var(d.x),
                y: var(d.y),
                z: d.z.map(var),
                construction: d.construction,
                alive: true,
            });
        }

        let mut new_curves = Vec::with_capacity(curves.len());
        for &k in &curves {
            let d = &src.curves[k];
            // Die Punkte jeder Kurve sind oben in `wanted` aufgenommen.
            let mut kind = map_curve(&d.kind, |x| p(x).expect("Punkt der Kurve dabei"), var);
            // Griff-Linien und Krümmungsbögen sind Kurven-Ids, die `map_curve` nicht abbildet.
            if let CurveKind::Spline { handles, .. } = &mut kind {
                for h in handles {
                    h.line = c(h.line).ok_or(SketchError::UnknownObject)?;
                    if let HandleCurvature::Arc { arc, .. } = &mut h.curvature {
                        *arc = c(*arc).ok_or(SketchError::UnknownObject)?;
                    }
                }
            }
            new_curves.push(CurveData {
                kind,
                construction: d.construction,
                centerline: d.centerline,
                show_frame: d.show_frame,
                normal: d.normal,
                comb: d.comb,
                alive: true,
            });
        }

        // Die Enden fester Splines bleiben fest: Ohne `Fixed` an beiden Enden gibt es keinen
        // lebenden festen Spline (Skizzen 6, A3; Prüfung in `from_data`).
        let held: HashSet<PointId> = curves
            .iter()
            .filter_map(|&k| match src.curves[k].kind {
                CurveKind::FixedSpline { start, end, .. } => Some([start, end]),
                _ => None,
            })
            .flatten()
            .collect();
        // Abhängigkeiten (auch interne) ganz in der Auswahl, ohne Musterwerte und ohne
        // Festlegungen außer an den Enden fester Splines.
        let mut constraint_map: HashMap<ConstraintId, ConstraintId> = HashMap::new();
        let mut new_constraints = Vec::new();
        for (id, d) in src.constraints.iter().filter(|(_, d)| d.alive) {
            if matches!(d.kind, ConstraintKind::PatternValue(..))
                || d.kind.fixed_point().is_some_and(|q| !held.contains(&q))
            {
                continue;
            }
            let inside = d.kind.points().iter().all(|&x| p(x).is_some())
                && d.kind.curves().iter().all(|&x| c(x).is_some());
            if !inside {
                continue;
            }
            let kind = map_ids(&d.kind, |x| p(x).unwrap(), |x| c(x).unwrap());
            constraint_map.insert(
                id,
                ConstraintId::from_index(self.constraints.len() + new_constraints.len()),
            );
            new_constraints.push(ConstraintData {
                kind,
                branch: d.branch,
                shared_sign: d.shared_sign,
                supplement: d.supplement,
                driven: d.driven,
                enabled: d.enabled,
                internal: d.internal,
                extra: d.extra.map(var),
                alive: true,
            });
        }

        let mut new_texts = Vec::with_capacity(texts.len());
        for &t in &texts {
            let d = &src.texts[t];
            let missing = || SketchError::UnknownObject;
            let placement = match d.placement {
                TextPlacement::Box {
                    lines,
                    corners,
                    h,
                    v,
                } => TextPlacement::Box {
                    lines: [
                        c(lines[0]).ok_or_else(missing)?,
                        c(lines[1]).ok_or_else(missing)?,
                        c(lines[2]).ok_or_else(missing)?,
                        c(lines[3]).ok_or_else(missing)?,
                    ],
                    corners: [
                        p(corners[0]).ok_or_else(missing)?,
                        p(corners[1]).ok_or_else(missing)?,
                        p(corners[2]).ok_or_else(missing)?,
                        p(corners[3]).ok_or_else(missing)?,
                    ],
                    h,
                    v,
                },
                TextPlacement::Path {
                    path,
                    above,
                    fit,
                    h,
                } => TextPlacement::Path {
                    path: c(path).ok_or_else(missing)?,
                    above,
                    fit,
                    h,
                },
            };
            new_texts.push(TextData {
                content: d.content.clone(),
                shown: d.shown.clone(),
                style: d.style.clone(),
                height: d.height,
                flip_h: d.flip_h,
                flip_v: d.flip_v,
                placement,
                normal: d.normal,
                curves: d
                    .curves
                    .iter()
                    .map(|&x| c(x).ok_or_else(missing))
                    .collect::<Result<_, _>>()?,
                points: d
                    .points
                    .iter()
                    .map(|&x| p(x).ok_or_else(missing))
                    .collect::<Result<_, _>>()?,
                alive: true,
                // Noch nicht gesetzt: Die Kurven stehen, gesetzt wird beim nächsten Neusetzen.
                shaped: Default::default(),
                walk: Default::default(),
            });
        }

        // Erst jetzt wird geschrieben: Bei einem Fehler oben bleibt die Skizze unberührt.
        let (_, new_vars) = vars.into_inner();
        self.vars.extend(new_vars);
        for d in new_points {
            self.points.push(d);
        }
        for d in new_curves {
            self.curves.push(d);
        }
        for d in new_constraints {
            self.constraints.push(d);
        }
        let mut text_map = HashMap::new();
        for (&t, d) in texts.iter().zip(new_texts) {
            text_map.insert(t, self.texts.push(d));
        }
        self.touch_structure();
        Ok(CopyMap {
            points: point_map,
            curves: curve_map,
            constraints: constraint_map,
            texts: text_map,
        })
    }
}
