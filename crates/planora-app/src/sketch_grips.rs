//! Griffe von Verschieben/Kopieren und Maßstab (Entscheidungen 10 und 12 des Plans Skizzen 4): Lage
//! und Treffer in Bildpunkten, die reine Griff-Rechnung und der Ablauf an der Sitzung. Ein Griffzug
//! ändert nur `transform`, die Skizze bleibt unberührt, bis der Befehl abschließt; Vorschau und
//! Abschluss rechnen mit denselben Funktionen aus `planora_sketch::transform`.

use crate::msg::Msg;
use crate::sketch_cursor::DrawCursor;
use crate::sketch_input::FieldKind;
use crate::sketch_kinds::PREVIEW_STEPS_PER_TURN;
use crate::sketch_state::{Item, SketchSession, Snap, Tool, TransformSet, TransformStage, err};
use egui::{Color32, Pos2, Shape, Stroke, Vec2};
use planora_feature::SketchFeature;
use planora_math::DVec2;
use planora_sketch::model::{CurveId, LineStyle, PointId, Sketch};
use planora_sketch::transform::{
    CopyMap, Similarity, TransformBlock, TransformSet as SketchTransformSet,
};

/// Länge der Pfeile und Abstand der Handhabe des Maßstabs in Bildpunkten.
pub const GRIP_ARROW_PX: f32 = 60.0;
/// Radius des Rings in Bildpunkten.
pub const GRIP_RING_PX: f32 = 40.0;
/// So nah muss der Zeiger an einem Griff liegen, in Bildpunkten.
pub const GRIP_HIT_PX: f32 = 8.0;
/// Kleinster Faktor am Griff: Durch den Bezugspunkt hindurch bleibt er über null.
const GRIP_MIN_FACTOR: f64 = 1e-6;
/// Radius des Griffs in der Mitte und halbe Größe der Handhabe in Bildpunkten.
const GRIP_CENTER_PX: f32 = 5.0;
/// Länge und halbe Breite der Pfeilspitze in Bildpunkten.
const GRIP_HEAD_PX: (f32, f32) = (12.0, 5.0);
/// Strichstärke der Griffe in Bildpunkten, normal und unter dem Zeiger.
const GRIP_WIDTH: (f32, f32) = (1.5, 2.5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grip {
    Free,
    AxisX,
    AxisY,
    Rotate,
    Scale,
}

/// Lage der Griffe auf dem Bildschirm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GripLayout {
    pub center: Pos2,
    pub x_tip: Pos2,
    pub y_tip: Pos2,
    pub ring: f32,
    /// Nur die Handhabe des Maßstabs.
    pub scale: bool,
}

/// Griffe um `center`; `x_dir` und `y_dir` sind die normierten Bilder der Skizzenachsen, also auf
/// gekippten Ansichten richtig.
pub fn layout(center: Pos2, x_dir: Vec2, y_dir: Vec2, scale: bool) -> GripLayout {
    GripLayout {
        center,
        x_tip: center + x_dir * GRIP_ARROW_PX,
        y_tip: center + y_dir * GRIP_ARROW_PX,
        ring: GRIP_RING_PX,
        scale,
    }
}

/// Abstand des Punkts `p` zur Strecke `a`–`b`.
fn to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = if ab.length_sq() > 0.0 {
        ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a + ab * t).distance(p)
}

/// Der Griff unter `p`: Mitte, dann Pfeile, dann Ring; im Maßstab nur die Handhabe.
pub fn hit(l: &GripLayout, p: Pos2) -> Option<Grip> {
    if l.scale {
        return (l.x_tip.distance(p) <= GRIP_HIT_PX).then_some(Grip::Scale);
    }
    if l.center.distance(p) <= GRIP_HIT_PX {
        Some(Grip::Free)
    } else if to_segment(p, l.center, l.x_tip) <= GRIP_HIT_PX {
        Some(Grip::AxisX)
    } else if to_segment(p, l.center, l.y_tip) <= GRIP_HIT_PX {
        Some(Grip::AxisY)
    } else if (l.center.distance(p) - l.ring).abs() <= GRIP_HIT_PX {
        Some(Grip::Rotate)
    } else {
        None
    }
}

/// Zeichnet die Griffe; `hot` hebt einen hervor. Die Farben kommen vom Aufrufer aus dem Thema.
pub fn paint(
    painter: &egui::Painter,
    l: &GripLayout,
    hot: Option<Grip>,
    color: Color32,
    hot_color: Color32,
) {
    let stroke = |grip: Grip| {
        if hot == Some(grip) {
            Stroke::new(GRIP_WIDTH.1, hot_color)
        } else {
            Stroke::new(GRIP_WIDTH.0, color)
        }
    };
    if l.scale {
        let s = stroke(Grip::Scale);
        painter.line_segment([l.center, l.x_tip], Stroke::new(GRIP_WIDTH.0, color));
        painter.rect_filled(
            egui::Rect::from_center_size(l.x_tip, Vec2::splat(GRIP_CENTER_PX * 2.0)),
            0.0,
            s.color,
        );
        painter.circle_filled(l.center, GRIP_CENTER_PX * 0.6, color);
        return;
    }
    painter.circle_stroke(l.center, l.ring, stroke(Grip::Rotate));
    for (grip, tip) in [(Grip::AxisX, l.x_tip), (Grip::AxisY, l.y_tip)] {
        let s = stroke(grip);
        painter.line_segment([l.center, tip], s);
        let along = (tip - l.center).normalized();
        let side = along.rot90() * GRIP_HEAD_PX.1;
        let root = tip - along * GRIP_HEAD_PX.0;
        painter.add(Shape::convex_polygon(
            vec![tip, root + side, root - side],
            s.color,
            Stroke::NONE,
        ));
    }
    let center = if hot == Some(Grip::Free) {
        hot_color
    } else {
        color
    };
    painter.circle_filled(l.center, GRIP_CENTER_PX, center);
}

/// Laufender Zug an einem Griff: Stelle des Drückens und Stand von `transform` davor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GripDrag {
    pub grip: Grip,
    pub press: DVec2,
    pub start_offset: DVec2,
    pub start_angle: f64,
    pub start_factor: f64,
}

/// Verschiebung, Winkel und Faktor für den Zeiger `now`, rein. `base` ist der Drehpunkt auf der
/// Skizze. Der Winkel liegt im halben Umlauf um `start_angle`; ganze Umläufe zählt
/// [`unwrapped_angle`]. Der Faktor ist nie null oder negativ, und ein Zeiger auf `base` oder außerhalb
/// der Zahlen lässt Winkel und Faktor beim Startwert.
pub fn grip_motion(d: &GripDrag, base: DVec2, now: DVec2) -> (DVec2, f64, f64) {
    let mut out = (d.start_offset, d.start_angle, d.start_factor);
    if !now.is_finite() {
        return out;
    }
    let moved = now - d.press;
    let (from, to) = (d.press - base, now - base);
    match d.grip {
        Grip::Free => out.0 += moved,
        Grip::AxisX => out.0.x += moved.x,
        Grip::AxisY => out.0.y += moved.y,
        Grip::Rotate => {
            if from.length() > 0.0 && to.length() > 0.0 {
                out.1 += from.perp_dot(to).atan2(from.dot(to));
            }
        }
        Grip::Scale => {
            if from.length() > 0.0 {
                out.2 = (d.start_factor * to.length() / from.length()).max(GRIP_MIN_FACTOR);
            }
        }
    }
    out
}

/// Der Winkel `raw` in dem Umlauf, der `previous` am nächsten liegt: So läuft der Griff über ±180°
/// hinweg durch, und 720° bleiben 720°.
pub fn unwrapped_angle(previous: f64, raw: f64) -> f64 {
    let d = raw - previous;
    previous + d.sin().atan2(d.cos())
}

/// Warum die Bewegung nicht geht, als Meldung.
fn block_message(block: TransformBlock) -> Msg {
    match block {
        TransformBlock::FixedInSet(_) | TransformBlock::BoundToFixed(_) => {
            crate::msg!("sketch-move-fixed")
        }
        TransformBlock::NothingToDo => crate::msg!("sketch-move-nothing"),
        TransformBlock::Degenerate => crate::msg!("sketch-move-degenerate"),
    }
}

/// Mitte der Hülle der Kurven und Punkte; `None` ohne Lage.
fn hull_center(sk: &Sketch, curves: &[CurveId], points: &[PointId]) -> Option<DVec2> {
    let lines = curves
        .iter()
        .filter_map(|&c| sk.curve_polyline(c, PREVIEW_STEPS_PER_TURN))
        .flatten();
    let spots = points.iter().filter_map(|&p| sk.point_xy(p).ok());
    let (lo, hi) = lines.chain(spots).fold(None, |range, p| match range {
        None => Some((p, p)),
        Some((lo, hi)) => Some((p.min(lo), p.max(hi))),
    })?;
    Some((lo + hi) * 0.5)
}

impl SketchSession {
    /// Geht die Stufenfolge von Verschieben/Kopieren für dieses Werkzeug? Der Maßstab kommt mit
    /// Task 10 dazu.
    pub fn runs_transform(&self) -> bool {
        self.tool == Tool::MoveCopy
    }

    /// Wird gerade gewählt? Dann gilt der Rahmen wie im Auswahlwerkzeug.
    pub fn transform_picking(&self) -> bool {
        matches!(self.tool, Tool::MoveCopy | Tool::Scale)
            && self.transform.stage == TransformStage::Picking
    }

    /// Sind Griffe zu sehen? Nur in der aktiven Stufe mit gesetzter Menge.
    pub fn grips_shown(&self) -> bool {
        matches!(self.tool, Tool::MoveCopy | Tool::Scale)
            && self.transform.stage == TransformStage::Active
            && !(self.transform.set.curves.is_empty() && self.transform.set.points.is_empty())
    }

    /// Mit Vorauswahl aktiv, Bezugspunkt in der Mitte der Hülle; ohne bleibt die Stufe beim Wählen.
    /// Der Haken „Kopie erstellen“ bleibt, wie er ist.
    pub fn begin_transform(&mut self, sk: &Sketch) {
        let mut set = TransformSet::default();
        for item in &self.selection {
            match *item {
                Item::Curve(c) if sk.curve(c).is_some() => set.curves.push(c),
                Item::Point(p) if sk.point_xy(p).is_ok() => set.points.push(p),
                _ => {}
            }
        }
        let Some(center) = hull_center(sk, &set.curves, &set.points) else {
            self.transform.stage = TransformStage::Picking;
            return;
        };
        self.transform.stage = TransformStage::Active;
        self.transform.set = set;
        self.transform.base = Some(center);
        self.transform.offset = DVec2::ZERO;
        self.transform.angle = 0.0;
        self.transform.factor = 1.0;
    }

    /// Die Bewegung dieses Bilds: Gesperrte Felder gehen vor dem Griff. `None`, solange gewählt wird.
    pub fn transform_now(&self) -> Option<Similarity> {
        let t = &self.transform;
        if t.stage == TransformStage::Picking {
            return None;
        }
        let input = &self.input;
        Some(Similarity {
            base: t.base?,
            offset: DVec2::new(
                input.locked(FieldKind::MoveX).unwrap_or(t.offset.x),
                input.locked(FieldKind::MoveY).unwrap_or(t.offset.y),
            ),
            angle: input.locked(FieldKind::MoveAngle).unwrap_or(t.angle),
            factor: input.locked(FieldKind::ScaleFactor).unwrap_or(t.factor),
        })
    }

    /// Beginnt den Zug an einem Griff bei `at` (Skizzeneinheiten).
    pub fn begin_grip(&mut self, grip: Grip, at: DVec2) {
        if !self.grips_shown() || !at.is_finite() {
            return;
        }
        self.grip_drag = Some(GripDrag {
            grip,
            press: at,
            start_offset: self.transform.offset,
            start_angle: self.transform.angle,
            start_factor: self.transform.factor,
        });
    }

    /// Führt den Griff zum Zeiger `at`. Gedreht und skaliert wird um den Bezugspunkt an seiner
    /// jetzigen Lage.
    pub fn grip_to(&mut self, at: DVec2) {
        let (Some(d), Some(now)) = (self.grip_drag, self.transform_now()) else {
            return;
        };
        let (offset, angle, factor) = grip_motion(&d, now.base + now.offset, at);
        self.transform.offset = offset;
        self.transform.angle = unwrapped_angle(self.transform.angle, angle);
        self.transform.factor = factor;
    }

    /// Beendet den Griff; der Stand bleibt in `transform`.
    pub fn end_grip(&mut self) {
        self.grip_drag = None;
    }

    /// Nimmt nur den laufenden Griff zurück; bis alle Tasten los sind, ist nichts ein Klick oder ein
    /// neuer Griff ([`box_aborted`](SketchSession::box_aborted)).
    pub fn abort_grip(&mut self) {
        if let Some(d) = self.grip_drag.take() {
            self.transform.offset = d.start_offset;
            self.transform.angle = d.start_angle;
            self.transform.factor = d.start_factor;
            self.box_aborted = true;
        }
    }

    /// „Bezugspunkt setzen“: Der nächste Klick setzt ihn, Verschiebung und Winkel bleiben. Getipptes
    /// wird dafür zum Stand, denn die Felder der aktiven Stufe gehen mit ihr.
    pub fn begin_set_base(&mut self) {
        if self.transform.stage != TransformStage::Active {
            return;
        }
        if let Some(now) = self.transform_now() {
            self.transform.offset = now.offset;
            self.transform.angle = now.angle;
            self.transform.factor = now.factor;
        }
        self.grip_drag = None;
        self.draw_cursor = None;
        self.transform.stage = TransformStage::SetBase;
    }

    /// Klick: Beim Wählen schaltet er Kurve oder Punkt unter dem Zeiger, beim Bezugspunkt setzt er
    /// ihn an der aufgelösten Lage; in der aktiven Stufe tut er nichts.
    pub(crate) fn click_transform(&mut self, sk: &Sketch, cur: &DrawCursor) {
        match self.transform.stage {
            TransformStage::Picking => {
                let item = match self.snap(sk, cur.raw, self.pick_radius) {
                    Snap::Point(p) => Item::Point(p),
                    Snap::Curve(c) => Item::Curve(c),
                    _ => return,
                };
                match self.selection.iter().position(|&s| s == item) {
                    Some(k) => {
                        self.selection.remove(k);
                    }
                    None => self.selection.push(item),
                }
            }
            TransformStage::SetBase => {
                self.transform.base = Some(cur.at);
                self.transform.stage = TransformStage::Active;
            }
            TransformStage::Active => {}
        }
    }

    /// Eingabe im Werkzeug: Beim Wählen weiter zu den Griffen, beim Bezugspunkt zurück, sonst
    /// abschließen.
    pub fn confirm_transform(&mut self, sf: &mut SketchFeature) -> Result<(), String> {
        if self.transform.stage == TransformStage::Active {
            self.finish_transform(sf)
        } else {
            self.finish_tool(&mut sf.sketch)
        }
    }

    /// Enter oder OK: verschiebt oder kopiert, löst, und das Werkzeug geht zurück zur Auswahl. Die
    /// Auswahl bleibt, bei einer Kopie sind es die Kopien. Was nicht geht, meldet `message`, und alles
    /// bleibt, wie es war.
    pub fn finish_transform(&mut self, sf: &mut SketchFeature) -> Result<(), String> {
        self.sync_input(&sf.sketch);
        if let Some(e) = self.input.fields.iter().find_map(|f| f.error.clone()) {
            self.message = Some(Msg::Raw(e.clone()));
            return Err(e);
        }
        let copied = self.atomic(&mut sf.sketch, |se, sk, prior| {
            let (Some(now), Some(set)) = (se.transform_now(), se.sketch_set(sk)) else {
                return Err(crate::msg!("sketch-nothing-selected"));
            };
            let mut note = None;
            let mut copied = None;
            if se.transform.copy {
                copied = Some(sk.copy_transformed(&set, &now).map_err(err)?);
            } else {
                let report = sk.apply_transform(&set, &now).map_err(block_message)?;
                if !report.dropped.is_empty() {
                    note = Some(crate::msg!("sketch-move-dropped", n = report.dropped.len()));
                }
            }
            se.solve_checked(sk, prior)?;
            if note.is_some() {
                se.message = note;
            }
            se.edit_label = Some(crate::msg!("undo-move-copy"));
            se.modify_edits = (se.modify_edits.0 + 1, Some(se.tool));
            Ok(copied)
        })?;
        if let Some(map) = &copied {
            self.take_copies(sf, map);
        }
        sf.prune_formulas();
        self.forget_removed(&sf.sketch);
        self.set_tool(&mut sf.sketch, Tool::Select);
        Ok(())
    }

    /// Nach dem Kopieren: Die Formeln gehen mit den Maßen mit, die Auswahl wird zu den Kopien.
    fn take_copies(&mut self, sf: &mut SketchFeature, map: &CopyMap) {
        for (old, new) in &map.constraints {
            if let Some(text) = sf.formulas.get(old).cloned() {
                sf.formulas.insert(*new, text);
            }
        }
        let curves = self.transform.set.curves.iter();
        let points = self.transform.set.points.iter();
        self.selection = curves
            .filter_map(|c| map.curves.get(c).map(|&c| Item::Curve(c)))
            .chain(points.filter_map(|p| map.points.get(p).map(|&p| Item::Point(p))))
            .collect();
    }

    /// Die gewählte Menge samt mitgezogener Hilfsgeometrie; `None` ohne Menge.
    fn sketch_set(&self, sk: &Sketch) -> Option<SketchTransformSet> {
        let t = &self.transform.set;
        let set = sk.transform_set(&t.curves, &t.points);
        (!set.points.is_empty()).then_some(set)
    }

    /// Vorschau als Linienzüge in der Reihenfolge der Menge; `None`, solange nichts zu zeigen ist oder
    /// die Bewegung nicht geht. Dieselbe Rechnung wie der Abschluss.
    pub fn transform_preview(&self, sk: &Sketch) -> Option<Vec<Vec<DVec2>>> {
        let styled = self.transform_preview_styled(sk)?;
        Some(styled.into_iter().map(|(_, line)| line).collect())
    }

    /// [`transform_preview`](Self::transform_preview) mit dem Stil jeder Kurve: Mitgezogene
    /// Hilfsgeometrie erscheint gestrichelt wie auf dem Blatt.
    pub fn transform_preview_styled(&self, sk: &Sketch) -> Option<Vec<(LineStyle, Vec<DVec2>)>> {
        let now = self.transform_now()?;
        let set = self.sketch_set(sk)?;
        let mut out = sk.clone();
        let copies = if self.transform.copy {
            Some(out.copy_transformed(&set, &now).ok()?)
        } else {
            out.apply_transform(&set, &now).ok()?;
            None
        };
        let lines = set.curves.iter().filter_map(|c| {
            let c = copies
                .as_ref()
                .map_or(Some(*c), |m| m.curves.get(c).copied())?;
            Some((
                out.curve(c)?.style(),
                out.curve_polyline(c, PREVIEW_STEPS_PER_TURN)?,
            ))
        });
        Some(lines.collect())
    }

    /// Wert der Felder von Verschieben/Kopieren: der Stand aus Griff und Tippen.
    pub(crate) fn transform_field_value(&self, kind: FieldKind) -> Option<f64> {
        let t = &self.transform;
        match kind {
            FieldKind::MoveX => Some(t.offset.x),
            FieldKind::MoveY => Some(t.offset.y),
            FieldKind::MoveAngle => Some(t.angle),
            FieldKind::ScaleFactor => Some(t.factor),
            _ => None,
        }
    }
}
