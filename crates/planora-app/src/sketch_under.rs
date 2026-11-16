//! „Unterbestimmte finden“ und „Skizze berechnen“ (Skizzen 11a, Fusion „Find Under-Constrained“
//! und „Compute Sketch“): die Liste der freien Elemente im Skizzenfeld, Zeilenklick mit Hervorheben
//! und Hinschauen, Löschen mit dem X und das Dämpfen voll bestimmter Geometrie.
//!
//! Die Zeilen werden je Bild aus dem letzten Lösungsbericht gebaut. Es gibt keinen gespeicherten
//! Bestand an Kennungen, darum auch keine toten Zeilen nach Löschen oder Rückgängig.

use crate::sketch_state::{Item, SketchSession, Tool};
use crate::state::AppState;
use glam::Vec3;
use planora_i18n::tr;
use planora_math::DVec2;
use planora_sketch::check::CheckItem;
use planora_sketch::model::{CurveKind, Sketch};

/// Rand um das Element in der Ansicht: Anteil der Ansichtsgröße je Seite.
pub const FOCUS_MARGIN: f64 = 0.15;

/// Die offene Liste der unterbestimmten Elemente.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnderList {
    /// Haken „Bestimmte ausblenden“: voll bestimmte Geometrie wird gedämpft gezeichnet.
    pub hide_determined: bool,
}

/// Was die Ansicht tun muss, damit ein Element mit Rand zu sehen ist.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FocusMove {
    /// Das Element ist mit Rand schon sichtbar.
    Keep,
    /// Die Ansichtsmitte um diesen Vektor verschieben (die Größe passt).
    Pan(DVec2),
    /// Auf `center` zoomen, so dass die halbe Ansichtshöhe `half_height` wird.
    Zoom { center: DVec2, half_height: f64 },
}

/// Wie die Ansicht `view` (min, max) verändert werden muss, damit `item` (min, max) mit dem Rand
/// `margin` (Anteil der Ansichtsgröße je Seite) zu sehen ist: passt es schon hinein `Keep`, passt es
/// in der Größe, aber nicht in der Lage `Pan`, sonst `Zoom`.
pub fn focus_move(view: (DVec2, DVec2), item: (DVec2, DVec2), margin: f64) -> FocusMove {
    let size = view.1 - view.0;
    let inner_min = view.0 + size * margin;
    let inner_max = view.1 - size * margin;
    let inside = item.0.x >= inner_min.x
        && item.0.y >= inner_min.y
        && item.1.x <= inner_max.x
        && item.1.y <= inner_max.y;
    if inside {
        return FocusMove::Keep;
    }
    let item_size = item.1 - item.0;
    let room = inner_max - inner_min;
    let center = (item.0 + item.1) * 0.5;
    if item_size.x <= room.x && item_size.y <= room.y {
        return FocusMove::Pan(center - (view.0 + view.1) * 0.5);
    }
    // Die Ansicht muss so groß werden, dass das Element samt Rand hineinpasst.
    let keep = (1.0 - 2.0 * margin).max(0.1);
    let aspect = if size.y > 0.0 { size.x / size.y } else { 1.0 };
    let height = (item_size.y / keep).max(item_size.x / keep / aspect.max(1e-9));
    FocusMove::Zoom {
        center,
        half_height: height * 0.5,
    }
}

/// Umgebendes Rechteck eines Elements in Skizzenkoordinaten.
pub fn item_bounds(sk: &Sketch, it: CheckItem) -> Option<(DVec2, DVec2)> {
    let mut pts: Vec<DVec2> = Vec::new();
    match it {
        CheckItem::Curve(c) => pts.extend(sk.curve_polyline(c, 96)?),
        CheckItem::Point(p) => pts.push(sk.point_xy(p).ok()?),
        CheckItem::Text(t) => {
            let d = sk.text(t)?;
            for &c in &d.curves {
                if let Some(line) = sk.curve_polyline(c, 24) {
                    pts.extend(line);
                }
            }
            for &p in &d.points {
                if let Ok(q) = sk.point_xy(p) {
                    pts.push(q);
                }
            }
        }
    }
    let first = *pts.first()?;
    Some(
        pts.iter()
            .fold((first, first), |(lo, hi), &p| (lo.min(p), hi.max(p))),
    )
}

impl SketchSession {
    /// Ist das Element noch da und laut letztem Bericht unterbestimmt?
    fn is_listed(&self, sk: &Sketch, it: CheckItem) -> bool {
        self.last_report
            .as_ref()
            .is_some_and(|r| sk.under_items(r).contains(&it))
    }

    /// Die Zeilen der Liste: Element, Text („Linie 3“, „Punkt 7“, „Text 1“) und ob ein X dabei ist.
    /// Skizzenpunkte lassen sich nicht aus der Liste löschen (Fusion), alles andere schon.
    pub fn under_rows(&self, sk: &Sketch) -> Vec<(CheckItem, String, bool)> {
        let Some(r) = self.last_report.as_ref() else {
            return Vec::new();
        };
        sk.under_items(r)
            .into_iter()
            .map(|it| {
                let deletable = !matches!(it, CheckItem::Point(_));
                (it, row_text(sk, it), deletable)
            })
            .collect()
    }

    /// Zeilenklick: wählt nur dieses Element. `false` bei einem veralteten Element (nichts
    /// geschieht).
    pub fn focus_item(&mut self, sk: &Sketch, it: CheckItem) -> bool {
        if !self.is_listed(sk, it) {
            return false;
        }
        let item = match it {
            CheckItem::Curve(c) => Item::Curve(c),
            CheckItem::Point(p) => Item::Point(p),
            CheckItem::Text(t) => {
                // Ein Buchstabe wählt den ganzen Text (Spec B8).
                let Some(d) = sk.text(t) else {
                    return false;
                };
                match (d.curves.first(), d.points.first()) {
                    (Some(&c), _) => Item::Curve(c),
                    (None, Some(&p)) => Item::Point(p),
                    _ => return false,
                }
            }
        };
        self.selection = vec![item];
        self.drop_curve_picks();
        true
    }

    /// X in der Zeile: löscht das Element (ein Rückgängig-Schritt). Bei einem veralteten Element
    /// oder einem Skizzenpunkt geschieht nichts.
    pub fn delete_under_item(&mut self, sk: &mut Sketch, it: CheckItem) -> bool {
        if matches!(it, CheckItem::Point(_)) || !self.focus_item(sk, it) {
            return false;
        }
        // Im Abhängigkeitsbefehl gehört die Auswahl dem Befehl: Das Löschen läuft in der Auswahl,
        // der Befehl bleibt danach, wie er war.
        let tool = self.tool;
        if matches!(tool, Tool::Constraint(_)) {
            self.tool = Tool::Select;
        }
        self.delete_selection(sk);
        self.tool = tool;
        true
    }

    /// „Skizze berechnen“: löst alles neu. Ein Rückgängig-Schritt entsteht nur, wenn sich Werte
    /// ändern (das entscheidet der Vergleich des Entwurfs am Bildende).
    pub fn compute_sketch(&mut self, sk: &mut Sketch) {
        self.last_report = Some(sk.recompute_all());
        self.edit_label = Some(crate::msg!("undo-compute-sketch"));
    }

    /// „Unterbestimmte finden“: öffnet die Liste (eine offene bleibt, wie sie ist).
    pub fn open_under_list(&mut self) {
        if self.under_list.is_none() {
            self.under_list = Some(UnderList::default());
        }
    }

    /// „Schließen“ in der Liste.
    pub fn close_under_list(&mut self) {
        self.under_list = None;
    }

    /// Ist voll bestimmte Geometrie gedämpft (Haken „Bestimmte ausblenden“)?
    pub fn hides_determined(&self) -> bool {
        self.under_list.is_some_and(|u| u.hide_determined)
    }
}

/// „Linie 3“, „Punkt 7“, „Text 1“.
fn row_text(sk: &Sketch, it: CheckItem) -> String {
    let n = sk.item_ordinal(it);
    match it {
        CheckItem::Point(_) => tr!("sketch-check-row-point", n = n),
        CheckItem::Text(_) => tr!("sketch-check-row-text", n = n),
        CheckItem::Curve(c) => match sk.curve(c).map(|d| &d.kind) {
            Some(CurveKind::Line { .. }) | None => tr!("sketch-check-row-line", n = n),
            Some(CurveKind::Circle { .. }) => tr!("sketch-check-row-circle", n = n),
            Some(CurveKind::Arc { .. }) => tr!("sketch-check-row-arc", n = n),
            Some(CurveKind::Ellipse { .. }) => tr!("sketch-check-row-ellipse", n = n),
            Some(CurveKind::EllipticArc { .. }) => tr!("sketch-check-row-elliptic-arc", n = n),
            Some(CurveKind::Spline { .. }) => tr!("sketch-check-row-spline", n = n),
            Some(CurveKind::ControlSpline { .. }) => {
                tr!("sketch-check-row-control-spline", n = n)
            }
            Some(CurveKind::Conic { .. }) => tr!("sketch-check-row-conic", n = n),
            Some(CurveKind::FixedSpline { .. }) => tr!("sketch-check-row-fixed-spline", n = n),
        },
    }
}

impl AppState {
    /// Sichtbarer Bereich der Skizzenebene `(min, max)` für das freie Rechteck der Kamera.
    pub fn visible_sketch_rect(&self) -> Option<(DVec2, DVec2)> {
        let frame = *self.sketch_draft.as_ref()?.sketch.frame();
        let cam = &self.camera;
        // Pixelmaß: beliebige Höhe, das Verhältnis kommt aus der Kamera.
        let h = 1000.0_f32;
        let full = glam::Vec2::new(cam.aspect * h, h);
        let vis_h = cam.visible_height_fraction * h;
        let vis = glam::Vec2::new(vis_h * cam.visible_aspect, vis_h);
        let centre = glam::Vec2::new(
            (cam.ndc_offset.x + 1.0) * 0.5 * full.x,
            (1.0 - cam.ndc_offset.y) * 0.5 * full.y,
        );
        let (lo, hi) = (centre - vis * 0.5, centre + vis * 0.5);
        let a = crate::sketch_view::screen_to_sketch(cam, &frame, full, lo)?;
        let b = crate::sketch_view::screen_to_sketch(cam, &frame, full, hi)?;
        Some((a.min(b), a.max(b)))
    }

    /// Zeilenklick in der Liste: wählt das Element und passt die Ansicht an (nur verschieben,
    /// wenn die Größe passt, zoomen, wenn nicht, nichts, wenn es schon mit Rand zu sehen ist).
    pub fn focus_under_item(&mut self, it: CheckItem) {
        let Some((se, sk)) = self.session_sketch_mut() else {
            return;
        };
        if !se.focus_item(sk, it) {
            return;
        }
        let Some(bounds) = item_bounds(sk, it) else {
            return;
        };
        let frame = *sk.frame();
        let Some(view) = self.visible_sketch_rect() else {
            return;
        };
        let vec3 = |v: planora_math::DVec3| Vec3::new(v.x as f32, v.y as f32, v.z as f32);
        match focus_move(view, bounds, FOCUS_MARGIN) {
            FocusMove::Keep => return,
            FocusMove::Pan(d) => {
                let o = frame.point_2d(DVec2::ZERO);
                self.camera.target += vec3(frame.point_2d(d) - o);
            }
            FocusMove::Zoom {
                center,
                half_height,
            } => {
                let now = (view.1.y - view.0.y) * 0.5;
                if now > 0.0 {
                    let k = (half_height / now) as f32;
                    self.camera.distance = (self.camera.distance * k).clamp(0.01, 1.0e6);
                }
                self.camera.target = vec3(frame.point_2d(center));
            }
        }
        // Der Benutzer hat die Ansicht bewegt: kein Nachpassen mehr.
        self.cancel_sketch_fit();
    }
}
