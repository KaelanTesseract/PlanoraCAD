use crate::state::BodyKind;
use egui::{Color32, Painter, Pos2, Rect, Stroke, pos2, vec2};

/// Zeichnet ein kleines Körpersymbol in `rect`.
pub fn paint_body_icon(painter: &Painter, rect: Rect, kind: BodyKind, color: Color32) {
    let stroke = Stroke::new(1.5, color);
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.38;
    match kind {
        BodyKind::Box => {
            let front =
                Rect::from_center_size(c + vec2(-r * 0.15, r * 0.15), vec2(r * 1.4, r * 1.4));
            let off = vec2(r * 0.4, -r * 0.4);
            painter.rect_stroke(front, 1.0, stroke, egui::StrokeKind::Inside);
            painter.line_segment([front.left_top(), front.left_top() + off], stroke);
            painter.line_segment([front.right_top(), front.right_top() + off], stroke);
            painter.line_segment([front.right_bottom(), front.right_bottom() + off], stroke);
            painter.line_segment([front.left_top() + off, front.right_top() + off], stroke);
            painter.line_segment(
                [front.right_top() + off, front.right_bottom() + off],
                stroke,
            );
        }
        BodyKind::Cylinder | BodyKind::Extrusion => {
            let top = c + vec2(0.0, -r * 0.7);
            let bottom = c + vec2(0.0, r * 0.7);
            let rx = r * 0.8;
            let ry = r * 0.3;
            painter.add(egui::Shape::ellipse_stroke(top, vec2(rx, ry), stroke));
            painter.add(egui::Shape::ellipse_stroke(bottom, vec2(rx, ry), stroke));
            painter.line_segment([pos2(c.x - rx, top.y), pos2(c.x - rx, bottom.y)], stroke);
            painter.line_segment([pos2(c.x + rx, top.y), pos2(c.x + rx, bottom.y)], stroke);
        }
        BodyKind::Sphere | BodyKind::Revolution => {
            painter.circle_stroke(c, r, stroke);
            painter.add(egui::Shape::ellipse_stroke(c, vec2(r, r * 0.35), stroke));
        }
        BodyKind::Cone => {
            let apex: Pos2 = c + vec2(0.0, -r);
            let base_y = c.y + r * 0.7;
            let rx = r * 0.85;
            painter.line_segment([apex, pos2(c.x - rx, base_y)], stroke);
            painter.line_segment([apex, pos2(c.x + rx, base_y)], stroke);
            painter.add(egui::Shape::ellipse_stroke(
                pos2(c.x, base_y),
                vec2(rx, r * 0.28),
                stroke,
            ));
        }
        BodyKind::Combined => {
            // zwei überlappende Quadrate: aus zwei Körpern entstanden
            let side = vec2(r * 1.2, r * 1.2);
            let back = Rect::from_center_size(c + vec2(r * 0.3, -r * 0.3), side);
            let front = Rect::from_center_size(c + vec2(-r * 0.3, r * 0.3), side);
            painter.rect_stroke(back, 1.0, stroke, egui::StrokeKind::Inside);
            painter.rect_stroke(front, 1.0, stroke, egui::StrokeKind::Inside);
        }
    }
}

/// Symbol einer Zeile im Feature-Baum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeatureIcon {
    Sketch,
    Body(BodyKind),
    Move,
}

impl FeatureIcon {
    /// Symbol zur Art eines Features.
    pub fn of(kind: &planora_feature::FeatureKind) -> FeatureIcon {
        use planora_feature::FeatureKind as K;
        match kind {
            K::Sketch(_) => FeatureIcon::Sketch,
            K::Box { .. } => FeatureIcon::Body(BodyKind::Box),
            K::Cylinder { .. } => FeatureIcon::Body(BodyKind::Cylinder),
            K::Sphere { .. } => FeatureIcon::Body(BodyKind::Sphere),
            K::Cone { .. } => FeatureIcon::Body(BodyKind::Cone),
            K::Extrude { .. } => FeatureIcon::Body(BodyKind::Extrusion),
            K::Revolve { .. } => FeatureIcon::Body(BodyKind::Revolution),
            K::Boolean { .. } => FeatureIcon::Body(BodyKind::Combined),
            K::Move { .. } => FeatureIcon::Move,
        }
    }
}

/// Zeichnet das Symbol eines Features in `rect`.
pub fn paint_feature_icon(painter: &Painter, rect: Rect, icon: FeatureIcon, color: Color32) {
    match icon {
        FeatureIcon::Sketch => paint_sketch_icon(painter, rect, color),
        FeatureIcon::Body(kind) => paint_body_icon(painter, rect, kind, color),
        FeatureIcon::Move => paint_move_icon(painter, rect, color),
    }
}

/// Verschieben: ein Pfeil nach rechts oben mit Spitze.
fn paint_move_icon(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.5, color);
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.38;
    let from = c + vec2(-r, r);
    let to = c + vec2(r, -r);
    painter.line_segment([from, to], stroke);
    painter.line_segment([to, to + vec2(-r * 0.7, 0.0)], stroke);
    painter.line_segment([to, to + vec2(0.0, r * 0.7)], stroke);
}

/// Zeichnet das Skizzensymbol in `rect`: ein Rechteck mit Eckpunkten und ein Kreis.
pub fn paint_sketch_icon(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.5, color);
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.38;
    let frame = Rect::from_center_size(c + vec2(-r * 0.2, r * 0.2), vec2(r * 1.4, r * 1.1));
    painter.rect_stroke(frame, 0.0, stroke, egui::StrokeKind::Middle);
    for corner in [
        frame.left_top(),
        frame.right_top(),
        frame.left_bottom(),
        frame.right_bottom(),
    ] {
        painter.circle_filled(corner, 1.8, color);
    }
    painter.circle_stroke(
        frame.right_top() + vec2(-r * 0.1, r * 0.1),
        r * 0.45,
        stroke,
    );
}
