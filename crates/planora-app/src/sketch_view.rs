//! Skizzen-Modus in der 3D-Ansicht: Abbildung Bildschirm ↔ Skizzenebene, Darstellung der Skizze
//! über der Szene und Weiterleiten der Eingaben an [`SketchSession`]. Die Logik (Werkzeuge, Fang,
//! Abhängigkeiten) liegt in `sketch_state`; hier wird nur gezeichnet und weitergereicht.

use crate::sketch_state::{Item, SketchSession, Snap, Tool};
use crate::state::AppState;
use crate::theme::{lighten as lighter, sketch_colors as colors};
use crate::viewport;
use egui::{Color32, FontId, PointerButton, Pos2, Rect, Sense, Shape, Stroke, pos2, vec2};
use glam::{Vec2, Vec3};
use planora_math::{DVec2, DVec3, Frame};
use planora_render::{EdgeRef, OrbitCamera, Pick};
use planora_sketch::constraint::{ConstraintKind, LineOrPoints};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::solver::{SolveReport, SolveStatus};
use std::f64::consts::{FRAC_PI_4, TAU};

/// Fangradius in Pixeln für Klicks und Hervorhebung.
pub const PICK_RADIUS_PX: f32 = 8.0;
/// Betrag des Kosinus zwischen Sehstrahl und Ebenennormale, unter dem der Strahl als parallel
/// zur Skizzenebene gilt (dann gibt es keinen brauchbaren Schnittpunkt).
const PARALLEL_COS: f64 = 1e-6;
/// Abschnitte je Vollkreis für Kreise, Bögen und Ellipsen in der Darstellung.
const STEPS_PER_TURN: usize = 96;
/// Länge der Achsenkreuz-Striche im Skizzenursprung in Pixeln.
const AXIS_PX: f32 = 28.0;
/// Radius der Punktsymbole in Pixeln.
const POINT_PX: f32 = 3.5;
/// Strichstärke normaler Kurven in Pixeln.
const CURVE_WIDTH: f32 = 2.0;
/// Schriftgröße der Abhängigkeitssymbole und Maße.
const LABEL_SIZE: f32 = 12.0;

fn to_vec3(v: DVec3) -> Vec3 {
    Vec3::new(v.x as f32, v.y as f32, v.z as f32)
}

fn to_dvec3(v: Vec3) -> DVec3 {
    DVec3::new(f64::from(v.x), f64::from(v.y), f64::from(v.z))
}

// ---- Abbildung Bildschirm ↔ Skizzenebene ------------------------------------------------------

/// Schneidet den Sehstrahl durch `pixel` (Ursprung oben links im Viewport der Größe `viewport`)
/// mit der Skizzenebene und liefert den Punkt in Skizzenkoordinaten; `None`, wenn der Strahl
/// parallel zur Ebene verläuft oder die Ebene hinter der Kamera liegt.
pub fn screen_to_sketch(
    camera: &OrbitCamera,
    frame: &Frame,
    viewport: Vec2,
    pixel: Vec2,
) -> Option<DVec2> {
    if viewport.x < 1.0 || viewport.y < 1.0 {
        return None;
    }
    let ndc = Vec2::new(
        pixel.x / viewport.x * 2.0 - 1.0,
        1.0 - pixel.y / viewport.y * 2.0,
    );
    let (origin, dir) = camera.ray(ndc);
    let (origin, dir) = (to_dvec3(origin), to_dvec3(dir));
    let denom = dir.dot(frame.z);
    if !denom.is_finite() || denom.abs() < PARALLEL_COS {
        return None;
    }
    let t = (frame.origin - origin).dot(frame.z) / denom;
    if !t.is_finite() || t < 0.0 {
        return None;
    }
    let local = frame.to_local(origin + dir * t);
    Some(DVec2::new(local.x, local.y))
}

/// Bildschirmposition (Pixel, Ursprung oben links) eines Skizzenpunkts; `None` hinter der Kamera.
pub fn sketch_to_screen(
    camera: &OrbitCamera,
    frame: &Frame,
    viewport: Vec2,
    p: DVec2,
) -> Option<Vec2> {
    camera.project(to_vec3(frame.point_2d(p)), viewport)
}

/// Fangradius in Skizzeneinheiten für `pixel_radius` Pixel an der Stelle `pixel`: Abstand zweier
/// benachbarter Pixel auf der Ebene mal `pixel_radius`.
pub fn pick_radius_units(
    camera: &OrbitCamera,
    frame: &Frame,
    viewport: Vec2,
    pixel: Vec2,
    pixel_radius: f32,
) -> Option<f64> {
    let a = screen_to_sketch(camera, frame, viewport, pixel)?;
    let b = screen_to_sketch(camera, frame, viewport, pixel + Vec2::X)?;
    Some(a.distance(b) * f64::from(pixel_radius))
}

// ---- Zahlen und Namen ---------------------------------------------------------------------

/// Zahl mit `decimals` Nachkommastellen und Komma als Dezimaltrenner (nur für die Anzeige).
pub fn format_number(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    // „-0,00“ wäre irritierend.
    let s = match s.strip_prefix('-') {
        Some(rest) if rest.chars().all(|c| c == '0' || c == '.') => rest.to_string(),
        _ => s,
    };
    s.replace('.', ",")
}

/// Liest eine Zahl; Komma und Punkt gelten als Dezimaltrenner. Nicht endliche Werte sind ungültig.
pub fn parse_number(text: &str) -> Option<f64> {
    text.trim()
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
}

/// Länge mit Einheit, z. B. „40,00 mm“.
pub fn format_length(v: f64) -> String {
    format!("{} mm", format_number(v, 2))
}

/// Winkel (Radiant) in Grad mit Einheit, z. B. „90,0°“.
pub fn format_angle(rad: f64) -> String {
    format!("{}°", format_number(rad.to_degrees(), 1))
}

/// Ist das ein Winkelmaß (Anzeige und Eingabe in Grad)?
pub fn is_angle(kind: &ConstraintKind) -> bool {
    matches!(
        kind,
        ConstraintKind::Angle(..) | ConstraintKind::EllipseAngle(..)
    )
}

/// Gespeicherter Wert eines Maßes.
pub fn stored_value(kind: &ConstraintKind) -> Option<f64> {
    use ConstraintKind as K;
    match kind {
        K::Distance(_, _, v)
        | K::DistancePointLine(_, _, v)
        | K::Radius(_, v)
        | K::Diameter(_, v)
        | K::Angle(_, _, v)
        | K::EllipseMajor(_, v)
        | K::EllipseMinor(_, v)
        | K::EllipseAngle(_, v) => Some(*v),
        _ => None,
    }
}

/// Anzuzeigender Wert eines Maßes: bei anzeigenden (`driven`) Maßen der gemessene Wert.
pub fn shown_value(sk: &Sketch, kind: &ConstraintKind, driven: bool) -> Option<f64> {
    if driven {
        sk.measure(kind).or_else(|| stored_value(kind))
    } else {
        stored_value(kind)
    }
}

/// Wert mit Einheit (mm oder °).
pub fn value_text(kind: &ConstraintKind, v: f64) -> String {
    if is_angle(kind) {
        format_angle(v)
    } else {
        format_length(v)
    }
}

/// Deutscher Name der Abhängigkeitsart.
pub fn kind_label(kind: &ConstraintKind) -> &'static str {
    use ConstraintKind as K;
    match kind {
        K::Coincident(..) => "Deckungsgleich",
        K::PointOnCurve(..) => "Punkt auf Kurve",
        K::Horizontal(..) => "Waagerecht",
        K::Vertical(..) => "Senkrecht",
        K::Parallel(..) => "Parallel",
        K::Perpendicular(..) => "Rechtwinklig",
        K::Tangent(..) => "Tangential",
        K::Equal(..) => "Gleich",
        K::Fixed(..) => "Fest",
        K::Symmetric(..) => "Symmetrisch",
        K::Midpoint(..) => "Mittelpunkt",
        K::Distance(..) => "Abstand",
        K::DistancePointLine(..) => "Abstand Punkt–Linie",
        K::Radius(..) => "Radius",
        K::Diameter(..) => "Durchmesser",
        K::Angle(..) => "Winkel",
        K::ArcRadius(..) => "Bogenradius",
        K::EllipseMajor(..) => "Große Halbachse",
        K::EllipseMinor(..) => "Kleine Halbachse",
        K::EllipseAngle(..) => "Ellipsenwinkel",
    }
}

/// Eintrag der Abhängigkeitsliste, z. B. „Abstand 40,00 mm“; anzeigende Maße in Klammern.
pub fn constraint_name(sk: &Sketch, kind: &ConstraintKind, driven: bool) -> String {
    let label = kind_label(kind);
    match shown_value(sk, kind, driven) {
        Some(v) if driven => format!("{label} ({})", value_text(kind, v)),
        Some(v) => format!("{label} {}", value_text(kind, v)),
        None => label.to_string(),
    }
}

/// Liefert `preferred`, wenn die Schrift alle Zeichen hat, sonst `fallback`.
fn symbol(ctx: &egui::Context, preferred: &'static str, fallback: &'static str) -> &'static str {
    let font = FontId::proportional(LABEL_SIZE);
    if ctx.fonts_mut(|f| f.has_glyphs(&font, preferred)) {
        preferred
    } else {
        fallback
    }
}

/// Beschriftung einer Abhängigkeit in der Ansicht: Symbol bzw. Maß mit Einheit.
pub fn overlay_text(
    ctx: &egui::Context,
    sk: &Sketch,
    kind: &ConstraintKind,
    driven: bool,
) -> String {
    use ConstraintKind as K;
    let plain = match kind {
        K::Coincident(..) | K::ArcRadius(..) => return String::new(),
        K::Horizontal(..) => "H",
        K::Vertical(..) => "V",
        K::Parallel(..) => symbol(ctx, "∥", "||"),
        K::Perpendicular(..) => symbol(ctx, "⟂", "Lot"),
        K::Equal(..) => "=",
        K::Tangent(..) => "T",
        K::Fixed(..) => "fix",
        K::Midpoint(..) => "Mitte",
        K::Symmetric(..) => "Sym",
        K::PointOnCurve(..) => "auf",
        _ => "",
    };
    if !plain.is_empty() {
        return plain.to_string();
    }
    let Some(v) = shown_value(sk, kind, driven) else {
        return String::new();
    };
    let text = match kind {
        K::Radius(..) => format!("R {}", format_length(v)),
        // Ø ohne Ersatz: Montserrat hat das Zeichen (tests/fonts.rs prüft das), und die
        // Glyphenabfrage von egui meldete es fälschlich als fehlend.
        K::Diameter(..) => format!("Ø {}", format_length(v)),
        K::EllipseMajor(..) => format!("a {}", format_length(v)),
        K::EllipseMinor(..) => format!("b {}", format_length(v)),
        _ => value_text(kind, v),
    };
    if driven { format!("({text})") } else { text }
}

// ---- Lage der Beschriftungen ----------------------------------------------------------------

fn pos(sk: &Sketch, p: PointId) -> Option<DVec2> {
    sk.point_xy(p).ok()
}

/// Gut sichtbare Stelle an einer Kurve: Mitte einer Linie, 45° am Kreis, sonst die Mitte des
/// Linienzugs.
fn curve_anchor(sk: &Sketch, c: CurveId) -> Option<DVec2> {
    match &sk.curve(c)?.kind {
        CurveKind::Line { a, b } => Some((pos(sk, *a)? + pos(sk, *b)?) * 0.5),
        CurveKind::Circle { center, .. } => {
            let r = sk.radius(c)?;
            Some(pos(sk, *center)? + DVec2::new(FRAC_PI_4.cos(), FRAC_PI_4.sin()) * r)
        }
        _ => {
            let pl = sk.curve_polyline(c, STEPS_PER_TURN)?;
            pl.get(pl.len() / 2).copied()
        }
    }
}

/// Stellen (Skizzenkoordinaten), an denen die Beschriftung einer Abhängigkeit erscheint.
fn anchors(sk: &Sketch, kind: &ConstraintKind) -> Vec<DVec2> {
    use ConstraintKind as K;
    let mid = |a: Option<DVec2>, b: Option<DVec2>| a.zip(b).map(|(a, b)| (a + b) * 0.5);
    let found = match kind {
        K::Horizontal(LineOrPoints::Line(c)) | K::Vertical(LineOrPoints::Line(c)) => {
            vec![curve_anchor(sk, *c)]
        }
        K::Horizontal(LineOrPoints::Points(a, b))
        | K::Vertical(LineOrPoints::Points(a, b))
        | K::Distance(a, b, _) => vec![mid(pos(sk, *a), pos(sk, *b))],
        K::Parallel(a, b) | K::Perpendicular(a, b) | K::Equal(a, b) | K::Tangent(a, b) => {
            vec![curve_anchor(sk, *a), curve_anchor(sk, *b)]
        }
        K::Angle(a, b, _) => vec![mid(curve_anchor(sk, *a), curve_anchor(sk, *b))],
        K::Fixed(p, ..) | K::Midpoint(p, _) | K::PointOnCurve(p, _) | K::Coincident(p, _) => {
            vec![pos(sk, *p)]
        }
        K::Symmetric(a, b, _) => vec![pos(sk, *a), pos(sk, *b)],
        K::DistancePointLine(p, c, _) => {
            let at = pos(sk, *p);
            vec![mid(at, at.and_then(|q| sk.nearest_on_curve(*c, q)))]
        }
        K::Radius(c, _)
        | K::Diameter(c, _)
        | K::ArcRadius(c)
        | K::EllipseMajor(c, _)
        | K::EllipseMinor(c, _)
        | K::EllipseAngle(c, _) => vec![curve_anchor(sk, *c)],
    };
    found.into_iter().flatten().collect()
}

// ---- Overlay ----------------------------------------------------------------------------------

/// Abbildung Skizze → Bildschirm für ein Bild (Kamera, Ebene und Viewport festgehalten).
struct Mapper {
    camera: OrbitCamera,
    frame: Frame,
    rect: Rect,
}

impl Mapper {
    fn viewport(&self) -> Vec2 {
        Vec2::new(self.rect.width(), self.rect.height())
    }

    fn to_screen(&self, p: DVec2) -> Option<Pos2> {
        let v = sketch_to_screen(&self.camera, &self.frame, self.viewport(), p)?;
        v.is_finite()
            .then(|| pos2(self.rect.left() + v.x, self.rect.top() + v.y))
    }

    fn local(&self, p: Pos2) -> Vec2 {
        Vec2::new(p.x - self.rect.left(), p.y - self.rect.top())
    }

    fn to_sketch(&self, p: Pos2) -> Option<DVec2> {
        screen_to_sketch(&self.camera, &self.frame, self.viewport(), self.local(p))
    }

    fn radius_at(&self, p: Pos2) -> Option<f64> {
        pick_radius_units(
            &self.camera,
            &self.frame,
            self.viewport(),
            self.local(p),
            PICK_RADIUS_PX,
        )
    }

    /// Linienzug auf dem Bildschirm; zerfällt an Stellen hinter der Kamera in Teilstücke.
    fn paths(&self, points: &[DVec2]) -> Vec<Vec<Pos2>> {
        let mut out: Vec<Vec<Pos2>> = vec![Vec::new()];
        for &p in points {
            match self.to_screen(p) {
                Some(s) => {
                    if let Some(v) = out.last_mut() {
                        v.push(s);
                    }
                }
                None => out.push(Vec::new()),
            }
        }
        out.retain(|v| v.len() >= 2);
        out
    }
}

/// Linienzug als Formen: gestrichelt (Konstruktionsgeometrie) oder durchgezogen.
fn path_shapes(path: Vec<Pos2>, stroke: Stroke, dashed: bool) -> Vec<Shape> {
    if dashed {
        Shape::dashed_line(&path, stroke, 6.0, 4.0)
    } else {
        vec![Shape::line(path, stroke)]
    }
}

fn draw_path(painter: &egui::Painter, path: Vec<Pos2>, stroke: Stroke, dashed: bool) {
    painter.extend(path_shapes(path, stroke, dashed));
}

/// Strichstärke fertiger Skizzen in der 3D-Ansicht (dünner als im Skizzen-Modus).
const FINISHED_WIDTH: f32 = 1.2;
/// Strichstärke der Konstruktionsgeometrie fertiger Skizzen.
const FINISHED_CONSTRUCTION_WIDTH: f32 = 1.0;

/// Fertige (nicht geöffnete) Skizzen als dünne Linien über der 3D-Ansicht in `rect`;
/// Konstruktionsgeometrie grau und gestrichelt. Die geöffnete Skizze zeichnet
/// [`show_overlay`]. Nur Anzeige: Skizzenkurven lassen sich in 3D nicht anklicken, und die
/// Linien liegen immer über den Körpern (keine Verdeckung).
pub fn finished_sketch_shapes(state: &AppState, rect: Rect) -> Vec<Shape> {
    let mut out = Vec::new();
    if rect.width() < 2.0 || rect.height() < 2.0 {
        return out;
    }
    let open = state.open_sketch_id();
    let col = colors(state.dark);
    // Die berechneten Skizzen der übrigen Skizzen-Features (aufgelöste Ebene, nachgeführte
    // Kantenkopien); die geöffnete zeichnet das Overlay aus dem Entwurf.
    for (id, _) in state.sketch_features() {
        if open == Some(id) {
            continue;
        }
        let Some(sk) = state.document.sketch_of(id) else {
            continue;
        };
        let map = Mapper {
            camera: state.camera,
            frame: *sk.frame(),
            rect,
        };
        for (c, data) in sk.curves() {
            let Some(pl) = sk.curve_polyline(c, STEPS_PER_TURN) else {
                continue;
            };
            let stroke = if data.construction {
                Stroke::new(FINISHED_CONSTRUCTION_WIDTH, col.construction)
            } else {
                Stroke::new(FINISHED_WIDTH, col.finished)
            };
            for path in map.paths(&pl) {
                out.extend(path_shapes(path, stroke, data.construction));
            }
        }
    }
    out
}

/// Zeichnet die Skizze über die 3D-Ansicht in `rect` und verarbeitet die Eingaben des
/// Skizzen-Modus. In der Skizzenfläche selbst wird nicht gedreht; Verschieben und Zoomen gelten
/// dort nach der Tastenbelegung (Voreinstellung: mittlere Maustaste oder Umschalt + Ziehen,
/// Mausrad). Gedreht wird durch Ziehen am
/// Navigationswürfel (siehe `viewport::show_in`).
///
/// Gibt zurück, ob der Zeiger über der Skizzenfläche steht; das Mausrad-Zoomen macht der Aufrufer
/// (`viewport::show`) erst nach Würfel und Home-Knopf, damit es auch über ihnen wirkt.
pub fn show_overlay(ui: &mut egui::Ui, rect: Rect, state: &mut AppState) -> bool {
    if state.session_sketch().is_none() || rect.width() < 2.0 || rect.height() < 2.0 {
        return false;
    }
    let response = ui.interact(
        rect,
        ui.id().with("planora-sketch-overlay"),
        Sense::click_and_drag(),
    );
    // Die Zeichenfläche nimmt den Fokus an (Tab, Pfeile), liest aber keinen Text: ihr Fokus
    // sperrt Esc, Entf und Eingabe der Skizze nicht.
    viewport::focus_is_not_typing(ui.ctx(), response.id);
    state.camera.set_viewport(rect.width(), rect.height());
    handle_input(ui, &response, rect, state);
    draw(ui, rect, state, response.hover_pos());
    response.hovered()
}

fn handle_input(ui: &egui::Ui, response: &egui::Response, rect: Rect, state: &mut AppState) {
    // Umschalt zählt auch, wenn sie nur beim Klick selbst gedrückt war (Ereignis und Loslassen der
    // Taste können im selben Frame ankommen).
    let shift = ui.input(|i| {
        i.modifiers.shift
            || i.events.iter().any(
                |e| matches!(e, egui::Event::PointerButton { modifiers, .. } if modifiers.shift),
            )
    });
    let Some(frame) = state.session_sketch().map(|sk| *sk.frame()) else {
        return;
    };

    // Navigation: Verschieben und Zoomen; kein Orbit.
    let dragging_point = state
        .sketch_session
        .as_ref()
        .is_some_and(|s| s.dragging.is_some());
    // Schwenken nach der Tastenbelegung (Voreinstellung: Mitte oder Umschalt+links); nie
    // während ein Punkt gezogen wird. Die linke Taste ohne Umschalttasten bleibt dem Zeichnen.
    // Ein Zug, der als Schwenken beginnt (z. B. Schwenken auf Strg+links umgelegt), zieht keinen
    // Punkt.
    let km = crate::keymap::mouse::current(ui.ctx());
    let panning = crate::keymap::mouse::pan_2d(&km, ui, response);
    if panning && !dragging_point {
        let d = response.drag_motion();
        state.camera.pan(d.x, d.y, rect.height());
    }

    let map = Mapper {
        camera: state.camera,
        frame,
        rect,
    };
    let pointer = response.hover_pos().or(response.interact_pointer_pos());
    let at = pointer.and_then(|p| map.to_sketch(p));
    let radius = pointer.and_then(|p| map.radius_at(p)).unwrap_or(0.0);

    let Some((se, sk)) = state.session_sketch_mut() else {
        return;
    };
    if let Some(at) = at {
        se.cursor = Some(at);
    }

    // Punkt ziehen (Auswahlwerkzeug)
    if response.drag_started_by(PointerButton::Primary) && !shift && !panning {
        let origin = ui.input(|i| i.pointer.press_origin());
        if let Some(start) = origin.and_then(|p| map.to_sketch(p)) {
            let r = origin.and_then(|p| map.radius_at(p)).unwrap_or(radius);
            se.begin_drag(sk, start, r);
        }
    }
    if se.dragging.is_some() {
        if response.dragged_by(PointerButton::Primary)
            && let Some(at) = at
        {
            se.drag_to(sk, at);
        }
        if response.drag_stopped() || !response.dragged() {
            se.end_drag(sk);
        }
    }

    // Klicks
    let mut project_at = None;
    // egui zählt Klicks nur als Doppel-/Dreifachklick, wenn sie höchstens `max_click_dist`
    // (6 Pixel) vom vorigen Klick entfernt sind. Ein „Dreifachklick“ entsteht schon, wenn zwei
    // schnelle Klicks an derselben Stelle einem Klick anderswo folgen (Zeit zum vorletzten Klick,
    // Abstand nur zum letzten) – das ist für den Nutzer ein Doppelklick und beendet ebenso.
    if response.double_clicked() || response.triple_clicked() {
        let _ = se.finish_tool(sk);
    } else if response.clicked_by(PointerButton::Primary)
        && let Some(at) = at
    {
        if se.tool == Tool::ProjectEdge {
            project_at = pointer;
        } else {
            let _ = se.click(sk, at, radius, shift);
        }
    }
    if response.clicked_by(PointerButton::Secondary) {
        se.cancel(sk);
    }

    // Tasten (nicht, solange ein Eingabefeld die Tastatur hat)
    if !viewport::typing(ui.ctx()) {
        let (esc, enter, delete) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::Escape),
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Delete),
            )
        });
        if esc {
            se.cancel(sk);
        }
        if enter {
            let _ = se.finish_tool(sk);
        }
        if delete {
            se.delete_selection(sk);
        }
    }
    let tool = se.tool;

    // Körperkante kopieren: Kante in der 3D-Szene wählen.
    let hover = if tool == Tool::ProjectEdge {
        pointer
            .and_then(|p| state.pick(map.local(p), map.viewport()))
            .filter(|h| matches!(h, Pick::Edge(_)))
    } else {
        None
    };
    state.set_hover(hover);
    if let Some(p) = project_at {
        match state.pick(map.local(p), map.viewport()) {
            // Fehler (etwa ein Körper, der erst nach der Skizze entsteht) stehen in der
            // Meldung der Sitzung.
            Some(Pick::Edge(EdgeRef { body, edge })) => {
                let _ = state.project_edge_into_sketch(body, edge);
            }
            _ => {
                if let Some(se) = state.sketch_session.as_mut() {
                    se.message = Some("Bitte eine Körperkante anklicken".into());
                }
            }
        }
    }
}

/// Ist der Punkt vollständig bestimmt (laut letztem Bericht)?
fn point_determined(report: Option<&SolveReport>, p: PointId) -> bool {
    report.is_some_and(|r| r.status == SolveStatus::Solved && !r.free_points.contains(&p))
}

fn curve_determined(report: Option<&SolveReport>, c: CurveId) -> bool {
    report.is_some_and(|r| r.status == SolveStatus::Solved && !r.free_curves.contains(&c))
}

fn draw(ui: &egui::Ui, rect: Rect, state: &AppState, pointer: Option<Pos2>) {
    let (Some(se), Some(sk)) = (state.sketch_session.as_ref(), state.session_sketch()) else {
        return;
    };
    let painter = ui.painter_at(rect);
    let col = colors(state.dark);
    let map = Mapper {
        camera: state.camera,
        frame: *sk.frame(),
        rect,
    };
    let report = se.last_report.as_ref();

    // Achsenkreuz im Skizzenursprung
    if let Some(o) = map.to_screen(DVec2::ZERO) {
        for (dir, color) in [(DVec2::X, col.axis_x), (DVec2::Y, col.axis_y)] {
            if let Some(e) = map.to_screen(dir) {
                let d = (e - o).normalized() * AXIS_PX;
                if d.is_finite() {
                    painter.line_segment([o, o + d], Stroke::new(1.5, color));
                }
            }
        }
    }

    // Element unter dem Mauszeiger
    let hovered: Option<Item> = pointer
        .filter(|_| se.dragging.is_none())
        .and_then(|p| Some((map.to_sketch(p)?, map.radius_at(p)?)))
        .and_then(|(at, r)| match se.snap(sk, at, r) {
            Snap::Point(p) => Some(Item::Point(p)),
            Snap::Curve(c) => Some(Item::Curve(c)),
            Snap::None => None,
        });

    // Kurven
    for (c, data) in sk.curves() {
        let Some(pl) = sk.curve_polyline(c, STEPS_PER_TURN) else {
            continue;
        };
        let selected = se.selection.contains(&Item::Curve(c));
        let mut color = if selected {
            col.selected
        } else if data.construction {
            col.construction
        } else if curve_determined(report, c) {
            col.determined
        } else {
            col.free
        };
        if hovered == Some(Item::Curve(c)) {
            color = lighter(color);
        }
        let width = if data.construction { 1.2 } else { CURVE_WIDTH };
        for path in map.paths(&pl) {
            draw_path(&painter, path, Stroke::new(width, color), data.construction);
        }
    }

    // Punkte
    for (p, _) in sk.points() {
        let Some(s) = pos(sk, p).and_then(|q| map.to_screen(q)) else {
            continue;
        };
        let selected = se.selection.contains(&Item::Point(p)) || se.dragging == Some(p);
        let determined = point_determined(report, p);
        let mut color = if selected {
            col.selected
        } else if determined {
            col.determined
        } else {
            col.free
        };
        let hover = hovered == Some(Item::Point(p));
        if hover {
            color = lighter(color);
        }
        let r = if hover { POINT_PX + 1.5 } else { POINT_PX };
        if determined {
            painter.circle_filled(s, r, color);
        } else {
            painter.circle(s, r, ui.visuals().extreme_bg_color, Stroke::new(1.5, color));
        }
    }

    draw_preview(&painter, &map, sk, se, col.preview);

    // Abhängigkeiten und Maße
    let mut placed: Vec<Rect> = Vec::new();
    let bad: Vec<_> = report
        .map(|r| r.conflicting.iter().chain(&r.redundant).copied().collect())
        .unwrap_or_default();
    for (id, c) in sk.constraints() {
        let text = overlay_text(ui.ctx(), sk, &c.kind, c.driven);
        if text.is_empty() {
            continue;
        }
        let color = if bad.contains(&id) {
            col.error
        } else if se.selection.contains(&Item::Constraint(id)) {
            col.selected
        } else if c.driven {
            col.construction
        } else {
            col.text
        };
        for a in anchors(sk, &c.kind) {
            if let Some(s) = map.to_screen(a) {
                label(
                    &painter,
                    s + vec2(6.0, -18.0),
                    &text,
                    color,
                    col.label_bg,
                    &mut placed,
                );
            }
        }
    }

    // Fanghinweis unter dem Mauszeiger beim Zeichnen
    if se.tool != Tool::Select
        && let Some(Item::Point(p)) = hovered
        && let Some(s) = pos(sk, p).and_then(|q| map.to_screen(q))
    {
        painter.circle_stroke(s, POINT_PX + 5.0, Stroke::new(1.5, col.selected));
    }
}

/// Kleine Beschriftung mit Hintergrund; weicht bereits gesetzten Beschriftungen nach unten aus.
fn label(
    painter: &egui::Painter,
    at: Pos2,
    text: &str,
    color: Color32,
    bg: Color32,
    placed: &mut Vec<Rect>,
) {
    /// Höchstens so oft wird einer Überlappung ausgewichen.
    const MAX_SHIFTS: usize = 8;
    let galley = painter.layout_no_wrap(text.to_string(), FontId::proportional(LABEL_SIZE), color);
    let mut r = Rect::from_min_size(at, galley.size()).expand(2.0);
    for _ in 0..MAX_SHIFTS {
        if !placed.iter().any(|q| q.intersects(r)) {
            break;
        }
        r = r.translate(vec2(0.0, r.height() + 1.0));
    }
    placed.push(r);
    painter.rect_filled(r, 3.0, bg);
    painter.galley(r.min + vec2(2.0, 2.0), galley, color);
}

/// Vorschau des laufenden Werkzeugs vom zuletzt gesetzten Punkt zum Mauszeiger.
fn draw_preview(
    painter: &egui::Painter,
    map: &Mapper,
    sk: &Sketch,
    se: &SketchSession,
    color: Color32,
) {
    let Some(cursor) = se.cursor else {
        return;
    };
    let stroke = Stroke::new(1.5, color);
    let pending: Vec<DVec2> = se.pending.iter().filter_map(|&p| pos(sk, p)).collect();
    let circle = |m: DVec2, r: f64| -> Vec<DVec2> {
        (0..=STEPS_PER_TURN)
            .map(|k| {
                let t = TAU * k as f64 / STEPS_PER_TURN as f64;
                m + DVec2::new(t.cos(), t.sin()) * r
            })
            .collect()
    };
    let shape: Vec<DVec2> = match (se.tool, pending.as_slice(), se.pending_pos.as_slice()) {
        (Tool::Line, [.., last], _) => vec![*last, cursor],
        (Tool::Rectangle, _, [a, ..]) => vec![
            *a,
            DVec2::new(cursor.x, a.y),
            cursor,
            DVec2::new(a.x, cursor.y),
            *a,
        ],
        (Tool::Circle, [m, ..], _) => circle(*m, m.distance(cursor)),
        (Tool::Arc, [m], _) => vec![*m, cursor],
        (Tool::Arc, [m, s, ..], _) => {
            let r = s.distance(*m);
            let a0 = (*s - *m).y.atan2((*s - *m).x);
            let d = cursor - *m;
            let a1 = d.y.atan2(d.x);
            let sweep = (a1 - a0).rem_euclid(TAU);
            let steps = ((sweep / TAU * STEPS_PER_TURN as f64).ceil() as usize).max(1);
            (0..=steps)
                .map(|k| {
                    let t = a0 + sweep * k as f64 / steps as f64;
                    *m + DVec2::new(t.cos(), t.sin()) * r
                })
                .collect()
        }
        (Tool::Ellipse, [m, ..], []) => vec![*m, cursor],
        (Tool::Ellipse, [m, ..], [major_end, ..]) => {
            let major = *major_end - *m;
            let len = major.length();
            if len <= sk.tolerance().linear {
                return;
            }
            let u = major / len;
            let minor = u.perp_dot(cursor - *m).abs().min(len);
            (0..=STEPS_PER_TURN)
                .map(|k| {
                    let t = TAU * k as f64 / STEPS_PER_TURN as f64;
                    *m + major * t.cos() + u.perp() * minor * t.sin()
                })
                .collect()
        }
        (Tool::Spline, [_, ..], _) => pending.iter().copied().chain([cursor]).collect(),
        _ => return,
    };
    for path in map.paths(&shape) {
        draw_path(painter, path, stroke, true);
    }
}
