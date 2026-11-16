//! Werkzeug „Übergangskurve“ (Skizzen 11a, Spec A2; Fusion „Blend Curve“): erst ein Kurvenende
//! wählen, dann das zweite; das nähere der beiden Enden der überfahrenen Kurve zählt. Das Modell
//! steht in `planora_sketch::blend`, hier liegt nur die Bedienung.

use crate::msg::Msg;
use crate::sketch_state::SketchSession;
use planora_math::DVec2;
use planora_sketch::SketchError;
use planora_sketch::blend::{BlendEnd, BlendKind, BlendRefusal};
use planora_sketch::model::{CurveId, Sketch};

/// Die Meldung zu einer abgelehnten Quelle (am Zeiger).
fn refusal_message(r: BlendRefusal) -> Msg {
    match r {
        BlendRefusal::Closed => crate::msg!("sketch-blend-closed"),
        BlendRefusal::ThreeD => crate::msg!("sketch-blend-three-d"),
        BlendRefusal::Text => crate::msg!("sketch-blend-text"),
        BlendRefusal::NotCurve => crate::msg!("sketch-blend-not-curve"),
    }
}

/// Die Meldung zu einem Fehler des Modells: Der Grund steht schon im Text, ohne Vorspann.
fn model_message(e: SketchError) -> Msg {
    match e {
        SketchError::InvalidInput(text) => Msg::Raw(text),
        other => crate::sketch_state::err(other),
    }
}

impl SketchSession {
    /// Wählt die Art der Übergangskurve (Skizzenfeld); eine laufende Wahl bleibt.
    pub fn choose_blend_kind(&mut self, kind: BlendKind) {
        self.blend.kind = kind;
    }

    /// Das Ende, das ein Klick bei `at` wählen würde: `Ok(None)` ohne Kurve unter dem Zeiger,
    /// `Err` bei einer Kurve, die keine Quelle sein kann.
    fn blend_target(
        &self,
        sk: &Sketch,
        at: DVec2,
        radius: f64,
    ) -> Result<Option<BlendEnd>, BlendRefusal> {
        let Some(c) = self.shown_curve(sk, at, radius) else {
            return Ok(None);
        };
        sk.blend_source(c)?;
        Ok(sk.blend_end_near(c, at))
    }

    /// Ein Klick des Werkzeugs im Rahmen von [`atomic`](Self::atomic): der erste wählt, ein Klick
    /// auf die gewählte Kurve hebt die Wahl auf (Spec A2, Ruling F10), der zweite legt an.
    pub(crate) fn blend_step(
        &mut self,
        sk: &mut Sketch,
        at: DVec2,
        prior: &Sketch,
    ) -> Result<Option<CurveId>, Msg> {
        let target = match self.blend_target(sk, at, self.pick_radius) {
            Ok(Some(t)) => t,
            Ok(None) => return Ok(None),
            Err(r) => return Err(refusal_message(r)),
        };
        let Some(first) = self.blend.first else {
            self.blend.first = Some(target);
            return Ok(None);
        };
        if first.curve == target.curve {
            self.blend.first = None;
            self.blend.hover = None;
            self.hover_preview = None;
            return Ok(None);
        }
        let made = sk
            .add_blend_curve(first, target, self.blend.kind)
            .map_err(model_message)?;
        self.solve_checked(sk, prior)?;
        self.blend.first = None;
        self.blend.hover = None;
        self.hover_preview = None;
        self.edit_label = Some(crate::msg!("undo-blend-curve"));
        Ok(Some(made))
    }

    /// Klick bei `at` (Skizzeneinheiten; Fangradius wie beim letzten Klick). Atomar: Scheitert er,
    /// bleibt alles, wie es war, und die Meldung der Sitzung sagt warum. Ein Ergebnis gibt es nur
    /// beim zweiten Klick, er ist ein Rückgängig-Schritt.
    pub fn click_blend(&mut self, sk: &mut Sketch, at: DVec2) -> Result<Option<CurveId>, Msg> {
        if !at.is_finite() {
            let e = crate::msg!("sketch-invalid-position");
            self.message = Some(e.clone());
            return Err(e);
        }
        self.atomic(sk, |se, sk, prior| se.blend_step(sk, at, prior))
            .map_err(|text| self.message.clone().unwrap_or(Msg::Raw(text)))
    }

    /// Das überfahrene Ende (`blend.hover`, hervorgehoben) und, wenn schon ein Ende gewählt ist,
    /// die Vorschau der Übergangskurve dorthin. Fangradius wie beim letzten Klick.
    pub fn blend_hover(&mut self, sk: &Sketch, at: DVec2) -> Option<Vec<DVec2>> {
        let radius = self.pick_radius;
        self.blend_hover_with(sk, at, radius)
    }

    /// [`blend_hover`](Self::blend_hover) mit dem Fangradius der Ansicht.
    pub(crate) fn blend_hover_with(
        &mut self,
        sk: &Sketch,
        at: DVec2,
        radius: f64,
    ) -> Option<Vec<DVec2>> {
        self.blend.hover = None;
        let Ok(Some(target)) = self.blend_target(sk, at, radius) else {
            return None;
        };
        let Some(first) = self.blend.first else {
            self.blend.hover = Some(target);
            return None;
        };
        // Die gewählte Kurve selbst ist kein Ziel: Der Klick hebt die Wahl auf.
        if first.curve == target.curve {
            return None;
        }
        let preview = sk
            .blend_preview(first, target, self.blend.kind)
            .ok()
            .filter(|pl| pl.len() >= 2)?;
        self.blend.hover = Some(target);
        Some(preview)
    }
}
