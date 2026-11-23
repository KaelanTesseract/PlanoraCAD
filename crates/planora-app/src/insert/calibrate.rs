//! „Kalibrieren“ einer Leinwand (Skizzen 9, Spec C und „Nach der Prüfung festgelegt“): ein
//! eigener Klickmodus in der Ansicht außerhalb der Skizze, gestartet aus dem Kontextmenü der
//! Leinwand im Baum (wie Fusion). Man klickt zwei Punkte auf dem Bild an, dann erscheint am
//! zweiten Punkt ein Feld mit dem gemessenen Abstand (Einheiten und Formeln wie überall). Enter
//! skaliert die Leinwand um den ersten Punkt (gleicher Faktor für X und Y, Spiegeln bleibt) in
//! einem Rückgängig-Schritt „Kalibrieren“. Esc oder Rechtsklick brechen in jeder Stufe ohne Spur
//! ab. Gesperrt, solange eine Skizze offen ist ([`AppState::canvas_lock`]).
//!
//! Die Punkte liegen in der Ebene des Trägers der Leinwand, in mm und im selben System wie
//! [`CanvasFeature::offset`] (ohne Versatz, Winkel und Maßstab). Ob ein Pixel durchsichtig ist,
//! zählt hier nicht: Getroffen werden muss nur das Rechteck des Bildes.

use crate::formula_field::{FormulaField, resolve, value_field_text};
use crate::state::AppState;
use crate::theme;
use egui::{Event, ImeEvent, Key, Order, Pos2, Rect, Shape, Stroke, pos2};
use glam::Vec2;
use planora_feature::{
    CanvasFeature, CanvasImage, FeatureId, FeatureKind, MAX_CANVAS_SCALE, MIN_CANVAS_SCALE,
    ParamUnit,
};
use planora_i18n::tr;
use planora_math::{DVec2, DVec3, Frame};
use planora_render::canvas_hit;

/// Kleinster Abstand der beiden Punkte in mm (darunter gelten sie als derselbe Punkt).
pub const MIN_PICK_DISTANCE: f64 = 1e-6;
/// Halbmesser der gezeichneten Punkte.
const POINT_RADIUS: f32 = 4.0;
/// Breite der Linie zwischen den Punkten.
const LINE_WIDTH: f32 = 1.5;
/// Abstand des Feldes vom zweiten Punkt, je Richtung.
const FIELD_GAP: f32 = 24.0;
/// Breite des Textes im Feld.
const FIELD_WIDTH: f32 = 90.0;
/// Breite des Feldes samt Name, solange es noch nicht gemessen ist.
const FIELD_WIDTH_GUESS: f32 = 200.0;
/// Höhe des Feldes, solange es noch nicht gemessen ist.
const FIELD_HEIGHT_GUESS: f32 = 30.0;

/// Laufendes Kalibrieren der Leinwand `canvas`.
#[derive(Clone, Debug, PartialEq)]
pub struct Calibrate {
    pub canvas: FeatureId,
    /// Erster Punkt in der Trägerebene (mm); er bleibt beim Skalieren an seiner Stelle.
    pub first: Option<DVec2>,
    /// Zweiter Punkt in der Trägerebene (mm); danach erscheint das Abstandsfeld.
    pub second: Option<DVec2>,
    /// Punkt auf dem Bild unter dem Zeiger (Linie vom ersten Punkt bis zum Zeiger).
    pub hover: Option<DVec2>,
    /// Text des Abstandsfelds (vorbelegt mit dem gemessenen Abstand in der Anzeigeeinheit).
    pub text: String,
    /// Wurde getippt? Sonst steht der gemessene Abstand da, und Enter ändert nichts.
    pub touched: bool,
    /// Meldung zum Feld (ungültige Eingabe), bis wieder getippt wird.
    pub error: Option<String>,
}

impl Calibrate {
    pub fn new(canvas: FeatureId) -> Calibrate {
        Calibrate {
            canvas,
            first: None,
            second: None,
            hover: None,
            text: String::new(),
            touched: false,
            error: None,
        }
    }

    /// Gemessener Abstand der beiden Punkte in mm (sobald beide gesetzt sind).
    pub fn measured(&self) -> Option<f64> {
        Some(self.first?.distance(self.second?))
    }

    /// Steht das Abstandsfeld da?
    pub fn asks_distance(&self) -> bool {
        self.first.is_some() && self.second.is_some()
    }
}

/// Faktor, der den Abstand von `a` nach `b` auf `distance_mm` bringt; `None` bei gleichen
/// Punkten oder einem Abstand, der nicht größer als 0 ist.
fn factor(a: DVec2, b: DVec2, distance_mm: f64) -> Option<f64> {
    let measured = a.distance(b);
    if !(measured.is_finite() && measured > MIN_PICK_DISTANCE) {
        return None;
    }
    if !(distance_mm.is_finite() && distance_mm > 0.0) {
        return None;
    }
    let k = distance_mm / measured;
    (k.is_finite() && k > 0.0).then_some(k)
}

/// Neuer Maßstab, wenn der Abstand von `a` nach `b` (Trägerebene, mm) `distance_mm` sein soll:
/// derselbe Faktor für X und Y. `None` bei gleichen Punkten, einem Abstand ≤ 0 oder einem
/// Maßstab außerhalb von [`MIN_CANVAS_SCALE`]..=[`MAX_CANVAS_SCALE`]. Der erste Punkt bleibt
/// dabei an seiner Stelle, wenn `offset` mit [`calibrated`] angepasst wird.
pub fn calibrated_scale(old: DVec2, a: DVec2, b: DVec2, distance_mm: f64) -> Option<DVec2> {
    let s = old * factor(a, b, distance_mm)?;
    let ok = |v: f64| v.is_finite() && (MIN_CANVAS_SCALE..=MAX_CANVAS_SCALE).contains(&v);
    (ok(s.x) && ok(s.y)).then_some(s)
}

/// Die Leinwand `f` nach dem Kalibrieren: Maßstab aus [`calibrated_scale`], der Versatz so, dass
/// der Punkt `a` an seiner Stelle bleibt (Skalieren um `a`). Winkel und Spiegeln bleiben.
pub fn calibrated(
    f: &CanvasFeature,
    a: DVec2,
    b: DVec2,
    distance_mm: f64,
) -> Option<CanvasFeature> {
    let scale = calibrated_scale(f.scale, a, b, distance_mm)?;
    let k = factor(a, b, distance_mm)?;
    let offset = a + (f.offset - a) * k;
    offset.is_finite().then(|| CanvasFeature {
        scale,
        offset,
        ..f.clone()
    })
}

/// Ursprung und Achsen des Trägers aus der Lage `frame` der Leinwand `f` zurückgerechnet (die
/// Lage enthält Versatz, Winkel, Maßstab und Spiegeln).
fn base_of(f: &CanvasFeature, frame: &Frame) -> Option<(DVec3, DVec3, DVec3)> {
    let sx = if f.flip_h { -f.scale.x } else { f.scale.x };
    let sy = if f.flip_v { -f.scale.y } else { f.scale.y };
    if sx == 0.0 || sy == 0.0 || !sx.is_finite() || !sy.is_finite() {
        return None;
    }
    let (ux, uy) = (frame.x / sx, frame.y / sy);
    let (s, c) = f.angle.sin_cos();
    let bx = ux * c - uy * s;
    let by = ux * s + uy * c;
    let origin = frame.origin - bx * f.offset.x - by * f.offset.y;
    Some((origin, bx, by))
}

/// Text für das Feld: der gemessene Abstand in der Anzeigeeinheit.
fn suggestion(mm: f64) -> String {
    value_field_text(mm, ParamUnit::Length)
}

/// Name des Rückgängig-Schritts.
fn undo_label() -> String {
    tr!("undo-canvas-calibrate")
}

impl AppState {
    /// Die Leinwand des Kalibrierens mit Lage und Bild (solange sie berechnet ist).
    fn calibrate_target(&self, id: FeatureId) -> Option<(&CanvasFeature, Frame, &CanvasImage)> {
        let FeatureKind::Canvas(c) = &self.document.feature(id)?.kind else {
            return None;
        };
        let frame = self.document.canvas_frame(id)?;
        let img = self.document.image(c.image)?;
        Some((c, frame, img))
    }

    /// Startet „Kalibrieren“ für die Leinwand `id` (Kontextmenü im Baum, Taste). Gesperrt,
    /// solange eine Skizze offen ist; eine ausgeblendete oder nicht berechnete Leinwand gibt
    /// eine Meldung. Andere Wahlmodi enden, die Leinwand wird gewählt.
    pub fn start_calibrate(&mut self, id: FeatureId) -> Result<(), String> {
        if let Some(why) = self.canvas_lock() {
            return self.fail(why);
        }
        if !self.is_canvas(id) {
            return self.fail(tr!("insert-canvas-not-found"));
        }
        if self.is_feature_hidden(id) {
            return self.fail(tr!("calibrate-hidden"));
        }
        if self.calibrate_target(id).is_none() {
            return self.fail(tr!("calibrate-unavailable"));
        }
        self.pending_pick = None;
        self.plane_pick = None;
        self.profile_pick = None;
        self.edge_picking = false;
        self.clear_edge_marks();
        self.select_canvas(id);
        self.message = None;
        self.calibrate = Some(Calibrate::new(id));
        Ok(())
    }

    /// Bricht „Kalibrieren“ ohne Änderung ab (Esc, Rechtsklick).
    pub fn cancel_calibrate(&mut self) {
        self.calibrate = None;
    }

    /// Der Punkt der Trägerebene (mm) unter dem Zeiger, wenn der Sehstrahl das Rechteck der
    /// kalibrierten Leinwand trifft (durchsichtige Pixel zählen mit).
    pub fn calibrate_point_at(&self, cursor: Vec2, viewport: Vec2) -> Option<DVec2> {
        let id = self.calibrate.as_ref()?.canvas;
        let (f, frame, img) = self.calibrate_target(id)?;
        let ray = self.view_ray(cursor, viewport)?;
        let t = canvas_hit(f.corners(&frame, img), ray, |_, _| u8::MAX)?;
        let (origin, bx, by) = base_of(f, &frame)?;
        let d = ray.origin + ray.dir * t - origin;
        let p = DVec2::new(d.dot(bx), d.dot(by));
        p.is_finite().then_some(p)
    }

    /// Weltpunkt eines Punktes `p` der Trägerebene der kalibrierten Leinwand.
    pub fn calibrate_world(&self, p: DVec2) -> Option<DVec3> {
        let id = self.calibrate.as_ref()?.canvas;
        let (f, frame, _) = self.calibrate_target(id)?;
        let (origin, bx, by) = base_of(f, &frame)?;
        Some(origin + bx * p.x + by * p.y)
    }

    /// Zeiger über der Ansicht: merkt den Punkt unter ihm (für die Linie vom ersten Punkt).
    pub fn calibrate_hover(&mut self, cursor: Option<(Vec2, Vec2)>) {
        let p = cursor.and_then(|(c, v)| self.calibrate_point_at(c, v));
        if let Some(c) = self.calibrate.as_mut() {
            c.hover = p;
        }
    }

    /// Klick in die Ansicht: setzt den ersten, dann den zweiten Punkt (daneben auf das Bild
    /// geklickt zählt nicht). Liegt der zweite auf dem ersten, bleibt es beim ersten, mit
    /// Meldung. Nach dem zweiten Punkt steht im Feld der gemessene Abstand.
    pub fn calibrate_click(&mut self, cursor: Vec2, viewport: Vec2) {
        let Some(p) = self.calibrate_point_at(cursor, viewport) else {
            return;
        };
        let Some(c) = self.calibrate.as_mut() else {
            return;
        };
        match (c.first, c.second) {
            (None, _) => c.first = Some(p),
            (Some(a), None) => {
                if a.distance(p) <= MIN_PICK_DISTANCE {
                    self.message = Some(tr!("calibrate-same-point"));
                    return;
                }
                c.second = Some(p);
                c.text = suggestion(a.distance(p));
                c.touched = false;
                c.error = None;
                self.message = None;
            }
            // Das Feld steht schon da: Klicks auf das Bild ändern nichts mehr.
            (Some(_), Some(_)) => {}
        }
    }

    /// Tippen im Abstandsfeld.
    pub fn calibrate_type(&mut self, text: &str) {
        if let Some(c) = self.calibrate.as_mut() {
            c.text = text.to_string();
            c.touched = true;
            c.error = None;
        }
    }

    /// Enter im Abstandsfeld: Der Abstand (Formel mit Einheiten, reine Zahlen in der
    /// Anzeigeeinheit) skaliert die Leinwand um den ersten Punkt in einem Rückgängig-Schritt
    /// „Kalibrieren“, der Modus endet. Unverändert der gemessene Abstand: endet ohne Änderung.
    /// Eine ungültige Eingabe bleibt mit Meldung im Feld stehen.
    pub fn apply_calibrate(&mut self) -> Result<(), String> {
        let Some(c) = self.calibrate.clone() else {
            return Ok(());
        };
        let (Some(a), Some(b)) = (c.first, c.second) else {
            return Ok(());
        };
        if !c.touched {
            self.calibrate = None;
            return Ok(());
        }
        let scope = self.document.param_scope();
        let result = resolve(&c.text, ParamUnit::Length, &scope).and_then(|(mm, _)| {
            let f = match self.document.feature(c.canvas).map(|f| &f.kind) {
                Some(FeatureKind::Canvas(f)) => f,
                _ => return Err(tr!("insert-canvas-not-found")),
            };
            calibrated(f, a, b, mm).ok_or_else(|| tr!("calibrate-bad-distance"))
        });
        let new = match result {
            Ok(f) => f,
            Err(e) => {
                if let Some(c) = self.calibrate.as_mut() {
                    c.error = Some(e.clone());
                }
                return Err(e);
            }
        };
        let before = self.document.edit_count();
        if let Err(e) = self.update_canvas(c.canvas, new) {
            if let Some(c) = self.calibrate.as_mut() {
                c.error = Some(e.clone());
            }
            return Err(e);
        }
        if self.document.edit_count() != before {
            self.undo_label_hint = Some(undo_label);
        }
        self.calibrate = None;
        Ok(())
    }

    /// Ist das Kalibrieren noch möglich (Leinwand da, berechnet, eingeblendet, keine Skizze)?
    fn calibrate_alive(&self) -> bool {
        self.calibrate.as_ref().is_some_and(|c| {
            self.canvas_lock().is_none()
                && !self.is_feature_hidden(c.canvas)
                && self.calibrate_target(c.canvas).is_some()
        })
    }
}

/// Hinweis in der Statuszeile je Stufe.
pub fn status_hint(c: &Calibrate) -> String {
    match (c.first, c.second) {
        (None, _) => tr!("calibrate-first"),
        (Some(_), None) => tr!("calibrate-second"),
        (Some(_), Some(_)) => tr!("calibrate-distance"),
    }
}

fn area_id() -> egui::Id {
    egui::Id::unique("planora-calibrate-field-area")
}

/// Unter diesem Schlüssel steht die Kennung des Textfelds, sobald es einmal gezeichnet wurde.
fn widget_id_key() -> egui::Id {
    egui::Id::unique("planora-calibrate-field-widget")
}

fn widget_id(ctx: &egui::Context) -> Option<egui::Id> {
    ctx.data(|d| d.get_temp(widget_id_key()))
}

/// Hat das Abstandsfeld die Tastatur?
fn field_has_keyboard(ctx: &egui::Context) -> bool {
    widget_id(ctx).is_some_and(|id| ctx.memory(|m| m.has_focus(id)))
}

fn starts_typing(event: &Event) -> bool {
    match event {
        Event::Text(_) | Event::Paste(_) => true,
        Event::Ime(ImeEvent::Preedit { text, .. } | ImeEvent::Commit(text)) => !text.is_empty(),
        _ => false,
    }
}

/// Bildschirmpunkt eines Punktes der Trägerebene in der Ansicht `rect`.
fn screen_of(state: &AppState, rect: Rect, p: DVec2) -> Option<Pos2> {
    let w = state.calibrate_world(p)?;
    let v = state
        .camera
        .project(w.as_vec3(), Vec2::new(rect.width(), rect.height()))
        .filter(|v| v.is_finite())?;
    Some(pos2(rect.left() + v.x, rect.top() + v.y))
}

/// Eingaben der Ansicht im Modus „Kalibrieren“: Zeigen und Klicken setzen die Punkte, Esc und
/// Rechtsklick brechen ab, nach dem zweiten Punkt das Abstandsfeld. Verschwindet die Leinwand
/// (Löschen, Ausblenden, Rückgängig) oder öffnet sich eine Skizze, endet der Modus.
pub fn view_input(ui: &mut egui::Ui, rect: Rect, state: &mut AppState, response: &egui::Response) {
    let ctx = ui.ctx().clone();
    if !state.calibrate_alive() {
        state.cancel_calibrate();
        return;
    }
    let esc = ui.input(|i| i.key_pressed(Key::Escape))
        && (field_has_keyboard(&ctx) || !crate::viewport::typing(&ctx));
    if esc || response.secondary_clicked() {
        if field_has_keyboard(&ctx) {
            ctx.memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
        }
        state.cancel_calibrate();
        return;
    }
    let viewport = Vec2::new(rect.width(), rect.height());
    let cursor = response
        .hover_pos()
        .map(|p| Vec2::new(p.x - rect.left(), p.y - rect.top()));
    state.calibrate_hover(cursor.map(|c| (c, viewport)));
    if response.clicked()
        && let Some(c) = cursor
    {
        state.calibrate_click(c, viewport);
    }
    if state
        .calibrate
        .as_ref()
        .is_some_and(Calibrate::asks_distance)
    {
        distance_field(&ctx, rect, state);
    }
}

/// Das Abstandsfeld am zweiten Punkt (rechts unterhalb, am Rand der Ansicht geklappt). Es hat
/// die Tastatur, solange kein anderes Feld sie hat; das erste Zeichen ersetzt den gemessenen
/// Abstand. Enter übernimmt ([`AppState::apply_calibrate`]).
fn distance_field(ctx: &egui::Context, rect: Rect, state: &mut AppState) {
    let Some(second) = state.calibrate.as_ref().and_then(|c| c.second) else {
        return;
    };
    let anchor = screen_of(state, rect, second).unwrap_or(rect.center());
    let size = ctx
        .memory(|m| m.area_rect(area_id()))
        .map_or(egui::vec2(FIELD_WIDTH_GUESS, FIELD_HEIGHT_GUESS), |r| {
            r.size()
        });
    let x = if anchor.x + FIELD_GAP + size.x > rect.right() {
        anchor.x - FIELD_GAP - size.x
    } else {
        anchor.x + FIELD_GAP
    };
    let y = if anchor.y + FIELD_GAP + size.y > rect.bottom() {
        anchor.y - FIELD_GAP - size.y
    } else {
        anchor.y + FIELD_GAP
    };
    let scope = state.document.param_scope();
    let Some(c) = state.calibrate.as_mut() else {
        return;
    };
    let has_keys = field_has_keyboard(ctx);
    let mut text = c.text.clone();
    // Das erste Zeichen ersetzt den Vorschlag, die Rücktaste davor leert das Feld.
    let (replace, wipe) = ctx.input(|i| {
        (
            i.events.iter().any(starts_typing),
            i.key_pressed(Key::Backspace) || i.key_pressed(Key::Delete),
        )
    });
    let wipe = wipe && has_keys && !c.touched;
    if wipe || (replace && has_keys && !c.touched) {
        text.clear();
    }
    let error = c.error.clone();
    let label = tr!("calibrate-field");
    let response = egui::Area::new(area_id())
        .fixed_pos(pos2(x, y))
        .order(Order::Foreground)
        .show(ctx, |ui| {
            let p = theme::palette(ui.visuals().dark_mode);
            egui::Frame::new()
                .fill(p.bg_panel)
                .corner_radius(3.0)
                .inner_margin(1.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(&label);
                        let outcome = FormulaField::new(
                            "planora-calibrate-field",
                            &mut text,
                            ParamUnit::Length,
                        )
                        .lock_focus(true)
                        .error_below(false)
                        .width(FIELD_WIDTH)
                        .label(label.clone())
                        .show(ui, &scope);
                        if let Some(e) = &error {
                            ui.colored_label(p.error, e);
                        }
                        outcome.response
                    })
                    .inner
                })
                .inner
        })
        .inner;
    ctx.data_mut(|d| d.insert_temp(widget_id_key(), response.id));
    if response.changed() || wipe {
        state.calibrate_type(&text);
    }
    let enter = response.lost_focus() && ctx.input(|i| i.key_pressed(Key::Enter));
    if enter && state.apply_calibrate().is_ok() {
        return;
    }
    // Das Feld hat die Tastatur, solange kein anderes Eingabefeld sie hat (auch nach Enter mit
    // Fehler und nach einem Klick neben das Feld).
    if ctx.memory(|m| m.focused()).is_none() || enter {
        ctx.memory_mut(|m| m.request_focus(response.id));
    }
}

/// Die beiden Punkte und die Linie zwischen ihnen (bis zum zweiten Punkt, sonst bis zum Zeiger)
/// über der Ansicht `rect`.
pub fn shapes(state: &AppState, rect: Rect) -> Vec<Shape> {
    let Some(c) = state.calibrate.as_ref() else {
        return Vec::new();
    };
    let color = theme::sketch_colors(state.dark).selected;
    let Some(a) = c.first.and_then(|p| screen_of(state, rect, p)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let end = c.second.or(c.hover).and_then(|p| screen_of(state, rect, p));
    if let Some(b) = end {
        out.push(Shape::line_segment([a, b], Stroke::new(LINE_WIDTH, color)));
    }
    out.push(Shape::circle_filled(a, POINT_RADIUS, color));
    if let Some(b) = c.second.and_then(|p| screen_of(state, rect, p)) {
        out.push(Shape::circle_filled(b, POINT_RADIUS, color));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calibrated_scale_doubles() {
        let a = DVec2::new(1.0, 2.0);
        let b = DVec2::new(11.0, 2.0);
        assert_eq!(
            calibrated_scale(DVec2::ONE, a, b, 20.0),
            Some(DVec2::splat(2.0))
        );
        assert_eq!(
            calibrated_scale(DVec2::new(1.0, 3.0), a, b, 20.0),
            Some(DVec2::new(2.0, 6.0)),
            "gleicher Faktor für X und Y"
        );
    }

    #[test]
    fn same_point_and_bad_distance_rejected() {
        let a = DVec2::new(1.0, 2.0);
        let b = DVec2::new(4.0, 6.0);
        assert_eq!(calibrated_scale(DVec2::ONE, a, a, 10.0), None);
        for d in [0.0, -5.0, f64::NAN, f64::INFINITY] {
            assert_eq!(calibrated_scale(DVec2::ONE, a, b, d), None, "{d}");
        }
        // Außerhalb der Grenzen des Maßstabs.
        assert_eq!(calibrated_scale(DVec2::ONE, a, b, 5.0 * 1e7), None);
    }
}
