//! Ziehstreifen und Ränder zum Ziehen der Größe.

use super::*;

/// Teile des Ziehstreifens `strip` ohne die Knöpfe, die der Inhalt in diesem Bild darin angelegt
/// hat: Felder, die nur Klicks annehmen (Knopf, Haken, Umschalter) und eingeschaltet sind. Was
/// auch Ziehen annimmt (Text zum Auswählen, Rollfläche) und Hintergründe über die ganze Breite
/// bleiben unter dem Streifen, dort zieht weiter das Fenster.
pub(super) fn strip_pieces(ui: &egui::Ui, strip: egui::Rect) -> Vec<egui::Rect> {
    let layer = ui.layer_id();
    let holes: Vec<egui::Rect> = ui.ctx().viewport(|vp| {
        vp.this_pass
            .widgets
            .get_layer(layer)
            .filter(|w| w.enabled && w.sense.senses_click() && !w.sense.senses_drag())
            .map(|w| w.interact_rect.intersect(strip))
            .filter(|r| r.is_positive())
            .filter(|r| r.min.x > strip.min.x + 1.0 || r.max.x < strip.max.x - 1.0)
            .collect()
    });
    let mut pieces = vec![strip];
    for hole in holes {
        pieces = pieces.into_iter().flat_map(|p| cut_out(p, hole)).collect();
    }
    pieces
}

/// `piece` ohne `hole`: bis zu vier Rechtecke (links, rechts, darüber, darunter).
pub(super) fn cut_out(piece: egui::Rect, hole: egui::Rect) -> Vec<egui::Rect> {
    let h = piece.intersect(hole);
    if !h.is_positive() {
        return vec![piece];
    }
    let (p, pos2) = (piece, egui::pos2);
    [
        egui::Rect::from_min_max(p.min, pos2(h.min.x, p.max.y)),
        egui::Rect::from_min_max(pos2(h.max.x, p.min.y), p.max),
        egui::Rect::from_min_max(pos2(h.min.x, p.min.y), pos2(h.max.x, h.min.y)),
        egui::Rect::from_min_max(pos2(h.min.x, h.max.y), pos2(h.max.x, p.max.y)),
    ]
    .into_iter()
    .filter(|r| r.is_positive())
    .collect()
}

/// Randstreifen ([`RESIZE_EDGE`]) zum Ändern der Größe über das Betriebssystem. Nach dem
/// Inhalt angelegt, damit sie über ihm liegen.
pub(super) fn resize_edges(ui: &mut egui::Ui, opened: &mut Opened, outer: Option<egui::Rect>) {
    use egui::viewport::ResizeDirection as D;
    let r = ui.max_rect();
    let e = RESIZE_EDGE;
    let (x0, x1, y0, y1) = (r.left(), r.right(), r.top(), r.bottom());
    let rect = |a: egui::Pos2, b: egui::Pos2| egui::Rect::from_two_pos(a, b);
    // Ecken zuletzt: sie liegen über den Kanten.
    let strips = [
        (D::North, rect(egui::pos2(x0, y0), egui::pos2(x1, y0 + e))),
        (D::South, rect(egui::pos2(x0, y1 - e), egui::pos2(x1, y1))),
        (D::West, rect(egui::pos2(x0, y0), egui::pos2(x0 + e, y1))),
        (D::East, rect(egui::pos2(x1 - e, y0), egui::pos2(x1, y1))),
        (
            D::NorthWest,
            rect(egui::pos2(x0, y0), egui::pos2(x0 + e, y0 + e)),
        ),
        (
            D::NorthEast,
            rect(egui::pos2(x1 - e, y0), egui::pos2(x1, y0 + e)),
        ),
        (
            D::SouthWest,
            rect(egui::pos2(x0, y1 - e), egui::pos2(x0 + e, y1)),
        ),
        (
            D::SouthEast,
            rect(egui::pos2(x1 - e, y1 - e), egui::pos2(x1, y1)),
        ),
    ];
    for (dir, strip) in strips {
        let response = ui.interact(
            strip,
            egui::Id::unique(("detach_rand", format!("{dir:?}"))),
            egui::Sense::drag(),
        );
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(match dir {
                D::North | D::South => egui::CursorIcon::ResizeVertical,
                D::West | D::East => egui::CursorIcon::ResizeHorizontal,
                D::NorthWest | D::SouthEast => egui::CursorIcon::ResizeNwSe,
                D::NorthEast | D::SouthWest => egui::CursorIcon::ResizeNeSw,
            });
        }
        if response.drag_started_by(egui::PointerButton::Primary)
            && opened.drag_frame != Some(opened.parent_frame)
        {
            opened.drag_frame = Some(opened.parent_frame);
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
            opened.moving = outer;
        }
    }
}
