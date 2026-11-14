//! Ziehen an 3D-Geometrie ohne Triade (Skizzen 10, Spec 10 „Ziehen ohne Triade“): Wer mit dem
//! Auswahlwerkzeug einen 3D-Punkt oder eine 3D-Kurve zieht, bewegt sie in der Ebene parallel zum
//! Bildschirm durch den gefassten Punkt. Die Ansicht wirft den Zeiger auf diese Ebene
//! ([`crate::sketch_view::Mapper::screen_to_plane`]); hier werden daraus die Ziele des Lösers
//! ([`Drag`] mit Zugebene). 2D-Punkte, die mitgezogen werden, bleiben in der Skizzenebene.

use crate::sketch_state::{Item, SketchSession, Tool, err, status_ok};
use planora_math::DVec3;
use planora_sketch::model::{PointId, Sketch};
use planora_sketch::solver::Drag;

/// Laufender Zug an 3D-Geometrie in der Ebene parallel zum Bildschirm.
#[derive(Clone, Debug)]
pub struct Drag3 {
    /// Gefasster Punkt in Skizzenkoordinaten; die Zugebene geht durch ihn.
    pub grab: DVec3,
    /// Normierte Normale der Zugebene (die Blickrichtung in Skizzenkoordinaten).
    pub normal: DVec3,
    /// Gezogene Punkte mit ihrer Lage beim Greifen.
    start: Vec<(PointId, DVec3)>,
}

impl SketchSession {
    /// Beginnt im Auswahlwerkzeug den Zug an der 3D-Geometrie `item`, gefasst bei `grab`, in der
    /// Ebene mit der Normale `normal` (Blickrichtung). Ein Punkt zieht allein; eine Kurve außerhalb
    /// der Auswahl ersetzt diese, sonst ziehen alle gewählten Kurven und Punkte mit (wie in 2D).
    /// `false`, wenn nichts zu ziehen ist.
    pub fn begin_drag3(&mut self, sk: &Sketch, item: Item, grab: DVec3, normal: DVec3) -> bool {
        let Some(normal) = normal.try_normalize() else {
            return false;
        };
        if self.tool != Tool::Select || !grab.is_finite() {
            return false;
        }
        self.dragging = None;
        self.curve_drag = None;
        self.drag3 = None;
        let items = match item {
            Item::Point(_) => vec![item],
            Item::Curve(_) => {
                self.forget_stale_picks();
                if !self.selection.contains(&item) {
                    self.selection = vec![item];
                    self.drop_curve_picks();
                }
                self.selection.clone()
            }
            Item::Constraint(_) => return false,
        };
        let mut start: Vec<(PointId, DVec3)> = Vec::new();
        for it in items {
            let points = match it {
                Item::Point(p) => vec![p],
                Item::Curve(c) => sk.curve(c).map_or(Vec::new(), |d| d.kind.points()),
                Item::Constraint(_) => continue,
            };
            for p in points {
                if !start.iter().any(|&(q, _)| q == p)
                    && let Ok(xyz) = sk.point_xyz(p)
                {
                    start.push((p, xyz));
                }
            }
        }
        if start.is_empty() {
            return false;
        }
        self.drag3 = Some(Drag3 {
            grab,
            normal,
            start,
        });
        true
    }

    /// Ein Bild des Zugs: `target` ist der Zeiger auf der Zugebene. Jeder gezogene Punkt bekommt
    /// sein Ziel `Start + (target − grab)` in der Ebene durch seinen Start; scheitert der Löser,
    /// bleibt die Skizze, und `message` erklärt es.
    pub fn drag_to3(&mut self, sk: &mut Sketch, target: DVec3) {
        let Some(drag) = self.drag3.as_ref() else {
            return;
        };
        if !target.is_finite() {
            return;
        }
        let n = drag.normal;
        let delta = target - drag.grab;
        let delta = delta - n * delta.dot(n);
        let drags: Vec<Drag> = drag
            .start
            .iter()
            .map(|&(point, from)| Drag {
                point,
                target: from + delta,
                plane: Some((from, n)),
            })
            .collect();
        let before = sk.clone();
        match sk.drag(&drags) {
            Ok(report) => {
                self.message = (!status_ok(&report)).then(|| crate::msg!("sketch-drag-blocked"));
                self.last_report = Some(report);
            }
            Err(e) => {
                *sk = before;
                self.message = Some(err(e));
            }
        }
    }

    /// Beendet den Zug an 3D-Geometrie: volles Lösen (holt die Diagnose nach), der Schritt heißt
    /// „Verschieben“.
    pub(crate) fn end_drag3(&mut self, sk: &mut Sketch) {
        if self.drag3.take().is_some() {
            self.last_report = Some(sk.solve());
            self.edit_label = Some(crate::msg!("undo-sketch-move"));
        }
    }
}
