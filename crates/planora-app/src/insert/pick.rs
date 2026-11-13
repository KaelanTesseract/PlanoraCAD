//! Wählfeld der Einfügen-Fenster (Spec C, Vorbild [`crate::plane_dialog::PlanePick`]): nimmt
//! eine Ursprungsebene, eine Konstruktionsebene oder eine ebene Körperfläche, aus einem Klick in
//! der Ansicht, im Browser oder aus der Auswahl beim Öffnen. Ist eine Skizze offen, entfällt das
//! Feld: Dann wird in diese Skizze eingefügt.

use crate::msg::Msg;
use crate::plane_dialog::{PlaneClick, PlanePick};
use crate::state::{AppState, PickFilter};
use planora_feature::{FeatureId, SketchPlane, StandardPlane};
use planora_i18n::tr;
use planora_math::Frame;
use planora_render::Pick;
use planora_sketch::plane_of_face;

/// Wohin eingefügt wird.
#[derive(Clone, Debug, PartialEq)]
pub enum InsertTarget {
    /// In die offene Skizze.
    OpenSketch,
    /// In neue Skizzen auf dieser Ebene.
    Plane(SketchPlane),
}

/// Gewählte Ebene samt Lage und Anzeigetext.
#[derive(Clone, Debug)]
struct Chosen {
    plane: SketchPlane,
    frame: Frame,
    label: String,
}

/// Das Wählfeld.
#[derive(Clone, Debug)]
pub struct TargetPick {
    /// Offene Skizze beim Öffnen des Fensters (dann gibt es kein Feld).
    open: Option<FeatureId>,
    chosen: Option<Chosen>,
    /// Warum der letzte Klick nicht passte.
    pub error: Option<Msg>,
}

fn standard_label(p: StandardPlane) -> String {
    match p {
        StandardPlane::XY => tr!("tree-plane-xy"),
        StandardPlane::XZ => tr!("tree-plane-xz"),
        StandardPlane::YZ => tr!("tree-plane-yz"),
    }
}

fn feature_name(state: &AppState, id: FeatureId) -> String {
    state
        .document
        .feature(id)
        .map_or_else(|| tr!("state-feature-n", n = id.0), |f| f.name.clone())
}

impl TargetPick {
    /// Feld für den Zustand beim Öffnen: offene Skizze, sonst die passende Auswahl (gewählte
    /// Ursprungsebene, Konstruktionsebene oder ebene Fläche), sonst leer.
    pub fn new(state: &AppState) -> TargetPick {
        let mut pick = TargetPick {
            open: state.open_sketch_id(),
            chosen: None,
            error: None,
        };
        if pick.open.is_none() {
            let click = if let Some(p) = state.selected_origin_plane() {
                Some(PlaneClick::Standard(p))
            } else if let Some(id) = state.selected_plane() {
                Some(PlaneClick::View(Pick::Plane(id)))
            } else if let Some(Pick::Face(f)) = state.selection {
                Some(PlaneClick::View(Pick::Face(f)))
            } else {
                None
            };
            if let Some(c) = click
                && let Ok(chosen) = convert(state, c)
            {
                pick.chosen = Some(chosen);
            }
        }
        pick
    }

    /// Feld mit einer schon bekannten Ebene `plane` in der Lage `frame` (Fenster „Leinwand
    /// bearbeiten“: der Träger der Leinwand); ohne offene Skizze.
    pub fn with_plane(state: &AppState, plane: SketchPlane, frame: Frame) -> TargetPick {
        let label = match &plane {
            SketchPlane::Standard(p) => standard_label(*p),
            SketchPlane::OnPlane(id) => feature_name(state, *id),
            SketchPlane::OnFace(r) => tr!(
                "plane-value-face",
                name = feature_name(state, FeatureId(r.origin))
            ),
        };
        TargetPick {
            open: None,
            chosen: Some(Chosen {
                plane,
                frame,
                label,
            }),
            error: None,
        }
    }

    /// Fügt das Fenster in die offene Skizze ein?
    pub fn into_open(&self) -> bool {
        self.open.is_some()
    }

    /// Die offene Skizze beim Öffnen.
    pub fn open_sketch(&self) -> Option<FeatureId> {
        self.open
    }

    /// Das Ziel; `None`, solange keine Ebene gewählt ist.
    pub fn target(&self) -> Option<InsertTarget> {
        if self.open.is_some() {
            return Some(InsertTarget::OpenSketch);
        }
        self.chosen
            .as_ref()
            .map(|c| InsertTarget::Plane(c.plane.clone()))
    }

    /// Lage der gewählten Ebene (bei offener Skizze die der Skizze).
    pub fn frame(&self, state: &AppState) -> Option<Frame> {
        if self.open.is_some() {
            return state.session_sketch().map(|s| *s.frame());
        }
        self.chosen.as_ref().map(|c| c.frame)
    }

    /// Text des Felds.
    pub fn label(&self, state: &AppState) -> String {
        if self.open.is_some() {
            let name = self
                .open
                .map(|id| feature_name(state, id))
                .unwrap_or_default();
            return tr!("insert-target-open-sketch", name = name);
        }
        self.chosen
            .as_ref()
            .map_or_else(|| tr!("insert-target-pick"), |c| c.label.clone())
    }

    /// Nimmt einen Klick; ein unpassender lässt das Feld, wie es ist, und hinterlässt eine
    /// Meldung. `true`, wenn das Feld gefüllt wurde.
    pub fn take_click(&mut self, state: &AppState, click: PlaneClick) -> bool {
        if self.open.is_some() {
            return false;
        }
        match convert(state, click) {
            Ok(c) => {
                self.chosen = Some(c);
                self.error = None;
                true
            }
            Err(e) => {
                self.error = Some(e);
                false
            }
        }
    }

    /// Holt einen Klick aus Ansicht oder Browser ab und meldet den Wahlmodus an (nur ohne offene
    /// Skizze).
    pub fn sync(&mut self, state: &mut AppState) {
        if self.open.is_some() {
            return;
        }
        if let Some(click) = state.plane_pick.as_mut().and_then(|p| p.clicked.take()) {
            self.take_click(state, click);
        }
        state.plane_pick = Some(PlanePick {
            filter: PickFilter::BASES,
            clicked: None,
            preview: None,
        });
    }

    /// Zeichnet das Feld: Text (anklickbar ohne Wirkung, wie das aktive Feld eines
    /// Ebenen-Fensters) und die Ursprungsebenen zur Wahl ohne Klick in die Ansicht.
    pub fn show(&mut self, ui: &mut egui::Ui, state: &AppState) {
        let text = self.label(state);
        if self.open.is_some() {
            ui.add(egui::Label::new(text).wrap());
            return;
        }
        let mut chosen = None;
        ui.horizontal_wrapped(|ui| {
            let _ = ui.selectable_label(true, text);
            ui.menu_button(tr!("plane-origin-planes"), |ui| {
                for p in [StandardPlane::XY, StandardPlane::XZ, StandardPlane::YZ] {
                    if ui.button(standard_label(p)).clicked() {
                        chosen = Some(p);
                        ui.close();
                    }
                }
            });
        });
        ui.weak(tr!("insert-target-hint"));
        if let Some(p) = chosen {
            self.take_click(state, PlaneClick::Standard(p));
        }
    }

    /// Beendet den Wahlmodus: Ansicht und Browser wählen wieder normal.
    pub fn finish(state: &mut AppState) {
        state.plane_pick = None;
        state.set_hover(None);
    }
}

/// Wandelt einen Klick in eine Ebene mit Lage.
fn convert(state: &AppState, click: PlaneClick) -> Result<Chosen, Msg> {
    match click {
        PlaneClick::Standard(p) => Ok(Chosen {
            plane: SketchPlane::Standard(p),
            frame: p.frame(),
            label: standard_label(p),
        }),
        PlaneClick::View(Pick::Plane(id)) => {
            let frame = state
                .document
                .plane_frame(id)
                .ok_or_else(|| crate::msg!("state-plane-not-computed"))?;
            Ok(Chosen {
                plane: SketchPlane::OnPlane(id),
                frame,
                label: feature_name(state, id),
            })
        }
        PlaneClick::View(Pick::Face(f)) => {
            let r = state
                .face_ref_for_sketch(f.body, f.face)
                .map_err(Msg::Raw)?;
            let frame = state
                .bodies
                .get(f.body)
                .and_then(|b| plane_of_face(&b.solid, f.face))
                .ok_or_else(|| crate::msg!("state-sketch-planar-only"))?;
            let label = tr!(
                "plane-value-face",
                name = feature_name(state, FeatureId(r.origin))
            );
            Ok(Chosen {
                plane: SketchPlane::OnFace(r),
                frame,
                label,
            })
        }
        PlaneClick::View(_) => Err(crate::msg!("insert-target-empty")),
    }
}
