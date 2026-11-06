//! Skizzenbemaßung (Skizzen 5, Fusion „Skizzenbemaßung“, Taste D): Der erste Klick wählt ein
//! Objekt (Punkt vor Kurve), der zweite wählt ein zweites oder setzt das Maß; mit zwei Objekten
//! setzt jeder weitere Klick. Die Art wählt der Zeiger über die reine Funktion
//! [`Sketch::dimension_plan`]; Vorschau und Klick rufen sie mit derselben Lage. Gesetzt öffnet das
//! Maßfeld im Skizzenfeld (`EditTarget::Planned`); erst Eingabe oder OK legt das Maß an
//! ([`SketchSession::apply_planned`], ein Rückgängig-Schritt), Esc lässt nichts zurück.

use crate::sketch_grips::{is_plain, is_wrapped, short_factor, split_factor};
use crate::sketch_state::{
    SketchSession, Snap, Tool, angle_value_ok, checked_value, dimension_input, err,
};
use planora_feature::{ParamScope, SketchFeature, constraint_unit};
use planora_math::DVec2;
use planora_sketch::constraint::{ConstraintId, ConstraintKind};
use planora_sketch::dimension::{DimPick, DimPlan};
use planora_sketch::model::Sketch;

/// Die Formel eines Maßes nach dem Umschalten Ø/R mit dem Faktor `k` (2 oder 0,5). Zweimal
/// umgeschaltet ist der Text bytegleich (Entscheidung 11): Vorwärts wird `text * k` (ein Text, der
/// kein Name und keine Zahl ist, in Klammern), ein Faktor am Ende wird zusammengefasst; ergibt
/// das 1, wird genau dieser Schritt zurückgenommen (Kopf ohne die Klammern, die er bekommen hat).
/// Ein Kopf in Klammern um einen bloßen Namen oder eine Zahl stammt nie aus diesem Schritt und
/// wird darum nicht ausgepackt, sondern wie jeder Text weiter multipliziert. Getrimmt; `None`,
/// wenn der Faktor nicht kurz zu schreiben ist.
pub(crate) fn toggled_formula(text: &str, k: f64) -> Option<String> {
    let text = text.trim();
    if let Some((head, n)) = split_factor(text) {
        let m = n * k;
        if m != 1.0 {
            return Some(format!("{head} * {}", short_factor(m)?));
        }
        if is_plain(head) {
            return Some(head.to_string());
        }
        if is_wrapped(head) {
            let inner = &head[1..head.len() - 1];
            if !is_plain(inner) {
                return Some(inner.to_string());
            }
        }
    }
    let f = short_factor(k)?;
    Some(if is_plain(text) {
        format!("{text} * {f}")
    } else {
        format!("({text}) * {f}")
    })
}

/// Dasselbe Objekt? Eine Kurve zählt unabhängig von der Klickstelle.
fn same_object(a: &DimPick, b: &DimPick) -> bool {
    match (a, b) {
        (DimPick::Point(p), DimPick::Point(q)) => p == q,
        (DimPick::Curve(c, _), DimPick::Curve(d, _)) => c == d,
        _ => false,
    }
}

/// Lässt sich das Maß `kind` zwischen Radius und Durchmesser umschalten
/// ([`Sketch::toggle_radius_diameter`])? Ein Punkt-Linie-Abstand nur an einer Mittellinie.
pub fn toggles(sk: &Sketch, kind: &ConstraintKind) -> bool {
    match *kind {
        ConstraintKind::Radius(..)
        | ConstraintKind::Diameter(..)
        | ConstraintKind::CenterlineDiameter(..) => true,
        ConstraintKind::DistancePointLine(_, l, _) => sk.curve(l).is_some_and(|d| d.centerline),
        _ => false,
    }
}

impl SketchSession {
    /// Klick der Skizzenbemaßung an der Stelle `at` (Fangradius `radius`): wählt ein Objekt
    /// (Punkt vor Kurve) oder setzt das Maß der Vorschau an dieser Stelle (siehe
    /// [`crate::sketch_dimension`]). Die Skizze bleibt unberührt: Über die Oberfläche
    /// (`commit_input`) hat [`accept_planned`](Self::accept_planned) ein gesetztes Maß schon
    /// angelegt; ist hier noch eins gesetzt, verwirft der Klick es und beginnt neu.
    pub fn click_dimension(&mut self, sk: &Sketch, at: DVec2, radius: f64) {
        if self.tool != Tool::Dimension || !at.is_finite() {
            return;
        }
        if self.dim.plan.take().is_some() {
            self.dim = Default::default();
        }
        let pick = match self.snap(sk, at, radius) {
            Snap::Point(p) => Some(DimPick::Point(p)),
            Snap::Curve(c) => Some(DimPick::Curve(c, at)),
            _ => None,
        };
        let again = pick.is_some_and(|p| self.dim.picks.iter().any(|q| same_object(q, &p)));
        let place = match self.dim.picks.len() {
            0 => false,
            1 => pick.is_none() || again,
            _ => true,
        };
        if place {
            match self.planned_at(sk, at) {
                Some(plan) => {
                    if self.message == Some(crate::msg!("sketch-dimension-none")) {
                        self.message = None;
                    }
                    self.dim.plan = Some(plan);
                    self.dim.picks.clear();
                    self.dim.toggled = false;
                }
                None => self.message = Some(crate::msg!("sketch-dimension-none")),
            }
            return;
        }
        let Some(pick) = pick else {
            return;
        };
        self.dim.picks.push(pick);
        // Passt das zweite Objekt zu keinem Maß, zählt es nicht.
        if self.dim.picks.len() == 2 && sk.dimension_plan(&self.dim.picks, at).is_none() {
            self.dim.picks.pop();
            self.message = Some(crate::msg!("sketch-dimension-none"));
        } else if self.message == Some(crate::msg!("sketch-dimension-none")) {
            // Nur die eigene Meldung fällt weg; die zum eben angelegten Maß bleibt stehen.
            self.message = None;
        }
    }

    /// Das Maß, das ein Klick an der Stelle `at` setzen würde (mit dem Umschalter Ø/R).
    fn planned_at(&self, sk: &Sketch, at: DVec2) -> Option<DimPlan> {
        let plan = sk.dimension_plan(&self.dim.picks, at)?;
        if self.dim.toggled {
            return Some(plan.toggled(sk).unwrap_or(plan));
        }
        Some(plan)
    }

    /// Die Vorschau der Skizzenbemaßung: das gesetzte Maß, sonst das Maß, das ein Klick am Zeiger
    /// setzen würde (dieselbe Rechnung wie [`click_dimension`](Self::click_dimension)).
    pub fn dimension_preview(&self, sk: &Sketch) -> Option<DimPlan> {
        if self.tool != Tool::Dimension {
            return None;
        }
        if let Some(plan) = &self.dim.plan {
            return Some(plan.clone());
        }
        let at = self.draw_cursor.as_ref().map(|c| c.raw).or(self.cursor)?;
        self.planned_at(sk, at)
    }

    /// Schaltet Ø/R der Vorschau um (Tab); beim gesetzten Maß dieses selbst. `false`, wenn es
    /// nichts umzuschalten gibt.
    pub fn toggle_dimension_preview(&mut self, sk: &Sketch) -> bool {
        if let Some(plan) = &self.dim.plan {
            return match plan.toggled(sk) {
                Some(t) => {
                    self.dim.plan = Some(t);
                    true
                }
                None => false,
            };
        }
        let Some(at) = self.draw_cursor.as_ref().map(|c| c.raw).or(self.cursor) else {
            return false;
        };
        match sk.dimension_plan(&self.dim.picks, at) {
            Some(plan) if plan.toggled(sk).is_some() => {
                self.dim.toggled = !self.dim.toggled;
                true
            }
            _ => false,
        }
    }

    /// Legt das gesetzte Maß der Skizzenbemaßung an, wenn eins da ist: mit dem getippten Text
    /// (`dim.text`), sonst mit dem gemessenen Wert. Für den Klick neben das offene Feld (Fusion
    /// übernimmt den Wert beim Wegklicken, **Annahme**). Ohne gesetztes Maß nichts.
    pub fn accept_planned(
        &mut self,
        sf: &mut SketchFeature,
        scope: &ParamScope,
    ) -> Result<(), String> {
        let Some(plan) = self
            .dim
            .plan
            .clone()
            .filter(|_| self.tool == Tool::Dimension)
        else {
            return Ok(());
        };
        match self.dim.text.clone() {
            Some(text) => self.apply_planned(sf, plan.kind, &text, scope)?,
            None => {
                let value = plan.kind.value().unwrap_or(0.0);
                self.apply_planned_value(sf, plan.kind, value)?
            }
        };
        Ok(())
    }

    /// Verwirft das gesetzte Maß (Esc oder Abbrechen im Feld); die Wahl beginnt neu, nichts bleibt.
    pub fn cancel_planned(&mut self) {
        self.dim = Default::default();
    }

    /// Legt das geplante Maß `kind` mit Zahl oder Formel `text` an (Parameter `scope`, Winkel in
    /// Grad): Anlegen, Wert und Lösen in einem Schritt, mit den Wächtern von
    /// [`apply`](Self::apply) (Falten, Widerspruch, überzählig ⇒ anzeigend). Die Formel kommt erst
    /// nach dem Lösen dazu und entfällt, wenn das Maß anzeigend wird; in Zoll ist auch eine Zahl
    /// eine Formel („1 in“). Danach ist das gesetzte Maß fort, das Werkzeug bleibt.
    pub fn apply_planned(
        &mut self,
        sf: &mut SketchFeature,
        kind: ConstraintKind,
        text: &str,
        scope: &ParamScope,
    ) -> Result<ConstraintId, String> {
        self.message = None;
        let (value, formula) = match dimension_input(text, constraint_unit(&kind), scope) {
            Ok(r) => r,
            Err(e) => {
                let text = e.text();
                self.message = Some(e);
                return Err(text);
            }
        };
        let id = self.add_planned(&mut sf.sketch, kind, value, true)?;
        let driven = sf.sketch.constraint(id).is_none_or(|c| c.driven);
        match formula {
            Some(f) if !driven => {
                sf.formulas.insert(id, f);
            }
            _ => {
                sf.formulas.remove(&id);
            }
        }
        self.note_formula_problem(sf);
        Ok(id)
    }

    /// Wie [`apply_planned`](Self::apply_planned) mit dem Wert `value` (Skizzeneinheit, ohne
    /// Formel): für ein unverändert bestätigtes Feld, dessen Text gerundet ist.
    pub fn apply_planned_value(
        &mut self,
        sf: &mut SketchFeature,
        kind: ConstraintKind,
        value: f64,
    ) -> Result<ConstraintId, String> {
        self.message = None;
        let id = self.add_planned(&mut sf.sketch, kind, value, false)?;
        sf.formulas.remove(&id);
        self.note_formula_problem(sf);
        Ok(id)
    }

    fn add_planned(
        &mut self,
        sk: &mut Sketch,
        kind: ConstraintKind,
        value: f64,
        typed: bool,
    ) -> Result<ConstraintId, String> {
        let label = crate::sketch_view::kind_msg(&kind);
        let id = self.atomic(sk, |se, sk, prior| {
            if !kind.is_dimension() {
                return Err(crate::msg!("sketch-not-dimension"));
            }
            let kind = checked_value(sk, &kind, value)?;
            // Ein getippter Winkel gilt nur bis auf 180° (wie bei `apply`).
            if typed
                && matches!(kind, ConstraintKind::Angle(..))
                && !angle_value_ok(sk.measure(&kind), value)
            {
                return Err(crate::msg!("sketch-angle-range"));
            }
            se.add_checked(sk, prior, kind, label)
        })?;
        let note = self.message.take();
        self.dim = Default::default();
        self.message = note;
        Ok(id)
    }

    /// Schaltet das Maß `id` zwischen Radius und Durchmesser um (Knopf „Ø/R“,
    /// [`Sketch::toggle_radius_diameter`]): dieselbe Id, der Wert verdoppelt bzw. halbiert, eine
    /// Formel wird mit `* 2` bzw. `* 0.5` umgeschrieben (Entscheidung 11). Ein Schritt; bei einem
    /// Fehler bleiben Skizze und Formeln, wie sie waren.
    pub fn toggle_radius_diameter(
        &mut self,
        sf: &mut SketchFeature,
        id: ConstraintId,
    ) -> Result<(), String> {
        let factor = match sf.sketch.constraint(id).map(|c| &c.kind) {
            Some(ConstraintKind::Radius(..) | ConstraintKind::DistancePointLine(..)) => 2.0,
            Some(ConstraintKind::Diameter(..) | ConstraintKind::CenterlineDiameter(..)) => 0.5,
            _ => 1.0,
        };
        // Die Formel zuerst: Lässt sie sich nicht umschreiben, wird gar nicht umgeschaltet.
        let formula = match sf.formulas.get(&id) {
            None => None,
            Some(text) => match toggled_formula(text, factor)
                .filter(|t| planora_feature::expr::parse(t).is_ok())
            {
                Some(t) => Some(t),
                None => {
                    let e = crate::msg!("sketch-toggle-formula-failed");
                    let text = e.text();
                    self.message = Some(e);
                    return Err(text);
                }
            },
        };
        self.atomic(&mut sf.sketch, |se, sk, prior| {
            sk.toggle_radius_diameter(id).map_err(err)?;
            se.solve_checked(sk, prior)?;
            Ok(())
        })?;
        if let Some(t) = formula {
            sf.formulas.insert(id, t);
        }
        self.note_formula_problem(sf);
        Ok(())
    }
}
