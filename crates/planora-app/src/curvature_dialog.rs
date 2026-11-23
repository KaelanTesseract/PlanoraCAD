//! Krümmungskamm in der Oberfläche (Skizzen 11a), wie Fusions „Toggle Curvature Display“
//! (Krümmungsanzeige ein/aus) und „Curvature Display Settings“ (Dichte 2–100, Maßstab).
//!
//! Der Befehl [`toggle`] schaltet den Kamm der gewählten Kurven aus, wenn alle einen haben, und
//! öffnet sonst den Dialog [`CurvatureDialog`] für die Kurven ohne Kamm. Die Vorschau im Dialog
//! zeichnet die Ansicht aus den Werten des Dialogs ([`preview_shapes`]); die Skizze ändert erst
//! „OK“, in einem Rückgängig-Schritt. „Abbrechen“, ✕ und Esc hinterlassen nichts.

use crate::dialog::{Dialog, DialogSize};
use crate::sketch_state::{CombToggle, clamp_comb};
use crate::state::AppState;
use crate::surface_project_dialog::{DialogOutcome, footer, keys};
use egui::{Color32, Pos2, Shape, Stroke};
use planora_i18n::tr;
use planora_math::DVec3;
use planora_sketch::comb::{COMB_DENSITY, COMB_MIN_PX, COMB_SCALE_MAX, COMB_SCALE_MIN};
use planora_sketch::comb::{CombSettings, comb_teeth};
use planora_sketch::model::{CurveId, CurveKind, Sketch};
use std::collections::HashSet;

/// Strichstärke von Zähnen und Hüllkurve.
const COMB_WIDTH: f32 = 1.0;

/// Der Dialog „Krümmungsanzeige einrichten“.
#[derive(Clone, Debug, PartialEq)]
pub struct CurvatureDialog {
    /// Die Kurven, die den Kamm bekommen (gewählte, kammfähige ohne Kamm).
    pub curves: Vec<CurveId>,
    /// „Dichte“, 2 bis 100.
    pub density: u32,
    /// „Maßstab“, 0,01 bis 100.
    pub scale: f64,
}

impl CurvatureDialog {
    /// Dialog mit den Vorgaben des Kamms (Dichte 20, Maßstab 1) für `curves`.
    pub fn new(curves: Vec<CurveId>) -> CurvatureDialog {
        let d = CombSettings::default();
        CurvatureDialog {
            curves,
            density: d.density,
            scale: d.scale,
        }
    }

    /// Die Werte des Dialogs, auf die Bereiche begrenzt.
    pub fn settings(&self) -> CombSettings {
        clamp_comb(CombSettings {
            density: self.density,
            scale: self.scale,
        })
    }

    /// „OK“: setzt den Kamm bei allen Kurven in einem Schritt. `false`, wenn keine Skizze offen
    /// ist oder keine der Kurven mehr da ist.
    pub fn apply(&self, state: &mut AppState) -> bool {
        let settings = self.settings();
        let Some((se, sk)) = state.session_sketch_mut() else {
            return false;
        };
        se.apply_comb(sk, &self.curves, settings) > 0
    }

    /// Abschluss eines Bilds: OK setzt den Kamm, Abbrechen und ✕ schließen ohne Spur, sonst
    /// bleibt der Dialog offen.
    pub fn finish(
        &self,
        state: &mut AppState,
        ok: bool,
        cancel: bool,
        open: bool,
    ) -> DialogOutcome {
        if ok && self.apply(state) {
            DialogOutcome::Applied
        } else if cancel || !open || ok {
            DialogOutcome::Cancelled
        } else {
            DialogOutcome::Open
        }
    }

    /// Zeigt den Dialog.
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> DialogOutcome {
        let mut open = true;
        let (mut ok, mut cancel) = (false, false);
        Dialog::new(tr!("curvature-dialog-title"), DialogSize::Prompt)
            .dock_key("curvature_display")
            .id(egui::Id::unique("curvature_display_dialog"))
            .open(&mut open)
            .show(ctx, |ui| {
                egui::Grid::new("curvature_display_fields")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label(tr!("sketch-comb-density"));
                        ui.add(egui::Slider::new(&mut self.density, COMB_DENSITY));
                        ui.end_row();
                        ui.label(tr!("sketch-comb-scale"));
                        ui.add(
                            egui::Slider::new(&mut self.scale, COMB_SCALE_MIN..=COMB_SCALE_MAX)
                                .logarithmic(true),
                        );
                        ui.end_row();
                    });
                // Abgedockt läuft der Inhalt evtl. mehrmals je Bild: nichts überschreiben.
                let (o, c) = footer(ui, None, true);
                let (enter, esc) = keys(ui, true);
                ok |= o || enter;
                cancel |= c || esc;
            });
        self.finish(state, ok, cancel, open)
    }
}

/// Befehl „Krümmungsanzeige ein/aus“ (Menüpunkt, Aktion, Schalter): schaltet den Kamm der
/// gewählten Kurven aus, öffnet den Dialog oder meldet, dass keine passende Kurve gewählt ist.
pub fn toggle(state: &mut AppState) {
    let Some((se, sk)) = state.session_sketch_mut() else {
        return;
    };
    se.curve_menu = None;
    match se.toggle_curvature(sk) {
        CombToggle::Dialog(curves) => state.curvature_dialog = Some(CurvatureDialog::new(curves)),
        CombToggle::Off(_) => {}
        CombToggle::Refused(msg) => se.message = Some(msg),
    }
}

/// Zeigt den offenen Dialog; er schließt mit der Skizze. Ein Wechsel auf ein anderes Werkzeug
/// lässt ihn offen (er gehört zur Auswahl, nicht zum Werkzeug).
pub fn show_open(ctx: &egui::Context, state: &mut AppState) -> Option<DialogOutcome> {
    let mut d = state.curvature_dialog.take()?;
    if state.session_sketch().is_none() {
        return Some(DialogOutcome::Cancelled);
    }
    let out = d.show(ctx, state);
    if out == DialogOutcome::Open && state.curvature_dialog.is_none() {
        state.curvature_dialog = Some(d);
    }
    Some(out)
}

/// Zähne und Hüllkurve des Kamms der Kurve `c` mit den Einstellungen `settings`.
/// `to_screen` bildet Skizzenkoordinaten auf den Bildschirm ab (nicht endliche Punkte, etwa
/// hinter der Kamera, fallen weg), `px_per_mm` ist der Maßstab der Ansicht.
pub fn comb_curve_shapes(
    sk: &Sketch,
    c: CurveId,
    settings: CombSettings,
    to_screen: &impl Fn(DVec3) -> Pos2,
    px_per_mm: f64,
    color: Color32,
) -> Vec<Shape> {
    let mut out = Vec::new();
    if !(px_per_mm.is_finite() && px_per_mm > 0.0) {
        return out;
    }
    let (Some(samples), Some(len)) = (sk.comb_samples(c, settings.density), sk.curve_length(c))
    else {
        return out;
    };
    let teeth = comb_teeth(&samples, len, settings.scale, COMB_MIN_PX / px_per_mm);
    let stroke = Stroke::new(COMB_WIDTH, color);
    let mut tips: Vec<Pos2> = Vec::with_capacity(teeth.len() + 1);
    for (foot, tip) in &teeth {
        let (a, b) = (to_screen(*foot), to_screen(*tip));
        if !(a.is_finite() && b.is_finite()) {
            continue;
        }
        if a.distance(b) > 0.05 {
            out.push(Shape::line_segment([a, b], stroke));
        }
        tips.push(b);
    }
    let closed = sk
        .curve(c)
        .is_some_and(|d| matches!(d.kind, CurveKind::Circle { .. } | CurveKind::Ellipse { .. }));
    if closed && let Some(first) = tips.first().copied() {
        tips.push(first);
    }
    // Eine gerade Stelle ohne Zahn gibt keine Hüllkurve her.
    if tips.len() >= 2 && !out.is_empty() {
        out.push(Shape::line(tips, stroke));
    }
    out
}

/// Alle Kämme der Skizze `sk` (Kurven mit Kamm), in der Farbe `color`.
pub fn comb_shapes(
    sk: &Sketch,
    to_screen: impl Fn(DVec3) -> Pos2,
    px_per_mm: f64,
    color: Color32,
) -> Vec<Shape> {
    comb_shapes_skipping(sk, to_screen, px_per_mm, color, &HashSet::new())
}

/// Wie [`comb_shapes`], ohne die Kurven in `hidden` (ausgeblendete Hilfskurven wie Griffe,
/// Musterquellen und Text zeichnen keinen Kamm).
pub fn comb_shapes_skipping(
    sk: &Sketch,
    to_screen: impl Fn(DVec3) -> Pos2,
    px_per_mm: f64,
    color: Color32,
    hidden: &HashSet<CurveId>,
) -> Vec<Shape> {
    let mut out = Vec::new();
    for (c, data) in sk.curves() {
        if hidden.contains(&c) {
            continue;
        }
        if let Some(settings) = data.comb {
            out.extend(comb_curve_shapes(
                sk, c, settings, &to_screen, px_per_mm, color,
            ));
        }
    }
    out
}

/// Vorschau des offenen Dialogs: Die Kämme seiner Kurven mit den Werten des Dialogs.
pub fn preview_shapes(
    sk: &Sketch,
    dialog: &CurvatureDialog,
    to_screen: impl Fn(DVec3) -> Pos2,
    px_per_mm: f64,
    color: Color32,
) -> Vec<Shape> {
    let settings = dialog.settings();
    let mut out = Vec::new();
    for c in &dialog.curves {
        out.extend(comb_curve_shapes(
            sk, *c, settings, &to_screen, px_per_mm, color,
        ));
    }
    out
}
