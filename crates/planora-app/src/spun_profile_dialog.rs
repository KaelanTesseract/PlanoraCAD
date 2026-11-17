//! Dialog „Drehprofil“ (Skizzen 11b, Spec 5), wie „Spun Profile“ in Autodesk Fusion.
//!
//! Er steht am Knopf „Projizieren/Einbeziehen“ als Art [`ToolKind::SpunProfile`] und gehört wie
//! „Auf Fläche projizieren“ zu [`crate::surface_project_dialog`] (Öffnen, Klicks aus der Ansicht,
//! Abschluss). Zwei Wählfelder: **Auswahl** (je nach Filter ganze Körper oder bestimmte
//! Elemente: Kanten, Ecken, Kurven früherer Skizzen) und **Achse** (gerade Skizzenlinie, auch
//! der eigenen Skizze, gerade Körperkante oder Achse des Ursprungs). Dazu „Achse projizieren“,
//! „Toleranz“, „Umkehren“ und „Projektionsverknüpfung“. Die Vorschau rechnet mit derselben
//! Rechnung wie das Anlegen ([`AppState::spun_pieces`]) und nur dann neu, wenn sich im Dialog
//! etwas ändert (nicht in jedem Bild); der Fehlertext steht im Dialog, OK ist dann gesperrt.
//! Erst „OK“ legt an ([`AppState::add_spun_profile`], ein Rückgängig-Schritt); Abbrechen, ✕, Esc
//! und ein Werkzeugwechsel hinterlassen nichts.

use crate::dialog::{Dialog, DialogSize};
use crate::msg::Msg;
use crate::state::{AppState, IncludePick};
use crate::surface_project_dialog::{
    DialogOutcome, count_text, curve_source, curve_source_label, edge_label, existing_curve,
    feature_name, footer, keys, outcome, session_error,
};
use planora_feature::planes::{AxisRef, StdAxis};
use planora_feature::surface_project::CurveSource;
use planora_feature::{BodyRef, IncludeRef, SpunAxis, SpunSelection, SpunSpec};
use planora_i18n::tr;
use planora_kernel::Curve;
use planora_math::DVec3;
use planora_sketch::model::{CurveId, CurveKind, Sketch};

/// Auswahlfilter des Dialogs (Fusion „Auswahlfilter“).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpunFilter {
    /// „Körper“: ein Klick auf Fläche, Kante oder Ecke wählt den ganzen Körper.
    #[default]
    Bodies,
    /// „Bestimmte Elemente“: Kanten, Ecken und Kurven früherer Skizzen.
    Elements,
}

/// Ein Wählfeld des Dialogs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpunField {
    /// „Auswahl“ (Körper oder Elemente).
    #[default]
    Selection,
    /// „Achse“.
    Axis,
}

/// Der offene Dialog „Drehprofil“ ([`crate::surface_project_dialog::SurfaceDialog::Spun`]).
#[derive(Clone, Debug)]
pub struct SpunProfileDialog {
    pub filter: SpunFilter,
    /// Die Klicks der Auswahl (für die Hervorhebung in der Ansicht).
    pub picks: Vec<IncludePick>,
    /// Die gewählte Achse; `None`, solange keine gewählt ist.
    pub axis: Option<SpunAxis>,
    /// „Achse projizieren“: Die Achse kommt zusätzlich als Projektion in die Skizze.
    pub project_axis: bool,
    /// Toleranz (mm) für angenäherte Kurven (**Annahme** 0,01 mm).
    pub tolerance: f64,
    /// „Umkehren“: das Ergebnis auf der anderen Seite der Achse.
    pub flip: bool,
    /// „Projektionsverknüpfung“: violette, verknüpfte Kurven statt freier.
    pub linked: bool,
    /// Das aktive Wählfeld.
    pub focus: SpunField,
    bodies: Vec<BodyRef>,
    elements: Vec<CurveSource>,
    /// Der Klick der Achse (nicht bei einer Linie der eigenen Skizze), für die Hervorhebung.
    axis_pick: Option<IncludePick>,
    /// Meldung eines abgelehnten Klicks oder OKs.
    pub(crate) error: Option<Msg>,
    /// Ergebnis der Vorschau: Zahl der Stücke oder die Meldung; `None`, solange Auswahl oder
    /// Achse fehlen.
    preview: Option<Result<usize, Msg>>,
    /// Hat sich seit der letzten Vorschau etwas geändert?
    dirty: bool,
}

impl Default for SpunProfileDialog {
    fn default() -> Self {
        Self {
            filter: SpunFilter::Bodies,
            picks: Vec::new(),
            axis: None,
            project_axis: false,
            tolerance: 0.01,
            flip: false,
            linked: true,
            focus: SpunField::Selection,
            bodies: Vec::new(),
            elements: Vec::new(),
            axis_pick: None,
            error: None,
            preview: None,
            dirty: false,
        }
    }
}

/// Punkt und Richtung (Welt) der Linie `c` der Skizze `sk` als Achse eines Drehprofils; `None`,
/// wenn die Kurve fehlt oder keine Linie ist.
pub(crate) fn own_axis_of(sk: &Sketch, c: CurveId) -> Option<(DVec3, DVec3)> {
    if !matches!(sk.curve(c)?.kind, CurveKind::Line { .. }) {
        return None;
    }
    match sk.curve_world(c)? {
        (Curve::Line { origin, dir }, _, _) => Some((origin, dir)),
        _ => None,
    }
}

/// Wartet der offene Dialog auf die Achse? Dann wählt ein Klick in der Ansicht zuerst eine Linie
/// der offenen Skizze.
pub fn wants_own_axis(state: &AppState) -> bool {
    matches!(
        state.surface_dialog.as_ref(),
        Some(crate::surface_project_dialog::SurfaceDialog::Spun(d)) if d.focus == SpunField::Axis
    )
}

/// Wählt ein Klick in der Ansicht gerade nur Körper (Feld „Auswahl“ mit Filter „Körper“)? Dann
/// zählen Ursprungsachsen, Ebenen und der Ursprung nicht, und ein Klick nahe der Mittellinie einer
/// Welle trifft den Körper (wie Fusions Auswahlfilter „Körper“).
pub fn wants_body_only(state: &AppState) -> bool {
    matches!(
        state.surface_dialog.as_ref(),
        Some(crate::surface_project_dialog::SurfaceDialog::Spun(d))
            if d.focus == SpunField::Selection && d.filter == SpunFilter::Bodies
    )
}

/// Was ein Klick in der 3D-Ansicht finden darf: bei der Auswahl „Körper“ nur Flächen und Kanten
/// (keine Konstruktionsebenen, die den Körper verdecken könnten), sonst wie „3D-Geometrie
/// einbeziehen“.
pub fn pick_filter(state: &AppState) -> crate::state::PickFilter {
    if wants_body_only(state) {
        crate::state::PickFilter::GEOMETRY
    } else {
        crate::state::PickFilter::INCLUDE
    }
}

/// Der Klick der Auswahl „Körper“ aus dem Treffer der Ansicht: nur Fläche, Kante oder Ecke eines
/// Körpers, sonst `None`.
pub fn body_pick(picked: Option<planora_render::Pick>) -> Option<IncludePick> {
    use planora_render::{EdgeRef, FaceRef, Pick};
    match picked? {
        Pick::Face(FaceRef { body, face }) => Some(IncludePick::Face(body, face)),
        Pick::Edge(EdgeRef { body, edge }) => Some(IncludePick::Edge(body, edge)),
        Pick::Vertex { body, edge, end } => Some(IncludePick::Vertex { body, edge, end }),
        Pick::Plane(_) => None,
    }
}

impl SpunProfileDialog {
    /// Meldung des Dialogs in der aktuellen Sprache: ein abgelehnter Klick oder, sonst, der
    /// Fehler der Vorschau.
    pub fn error(&self) -> Option<String> {
        self.error
            .as_ref()
            .or_else(|| self.preview.as_ref().and_then(|p| p.as_ref().err()))
            .map(Msg::text)
    }

    /// Die Zahl der Kurvenstücke der Vorschau; `None`, solange etwas fehlt oder die Rechnung
    /// einen Fehler meldet.
    pub fn preview_count(&self) -> Option<usize> {
        self.preview.as_ref().and_then(|p| p.as_ref().ok()).copied()
    }

    /// Ist die Vorschau fehlerfrei? Nur dann ist OK frei. Eine Meldung eines abgelehnten Klicks
    /// sperrt OK nicht: Die Felder sind gültig, die Meldung ist ein Hinweis und verschwindet mit
    /// dem nächsten passenden Klick oder jeder Änderung im Dialog (Filter, Leeren, Toleranz, Umkehren, Haken).
    pub fn ok_enabled(&self) -> bool {
        self.preview_count().is_some()
    }

    /// Rechnet die Vorschau neu (mit der Rechnung des Anlegens, nur bei Änderungen im Dialog).
    pub fn refresh(&mut self, state: &AppState) {
        self.dirty = false;
        self.preview = self
            .spec()
            .map(|spec| state.spun_pieces(&spec).map(|p| p.len()));
    }

    /// Das Drehprofil aus den Feldern; `None`, solange Auswahl oder Achse fehlen.
    pub fn spec(&self) -> Option<SpunSpec> {
        let axis = self.axis.clone()?;
        let selection = match self.filter {
            SpunFilter::Bodies if !self.bodies.is_empty() => {
                SpunSelection::Bodies(self.bodies.clone())
            }
            SpunFilter::Elements if !self.elements.is_empty() => {
                SpunSelection::Elements(self.elements.clone())
            }
            _ => return None,
        };
        Some(SpunSpec {
            selection,
            axis,
            tolerance: self.tolerance,
            flip: self.flip,
        })
    }

    /// Wechselt den Filter; die Auswahl wird geleert (Körper und Elemente passen nicht
    /// zueinander).
    pub fn set_filter(&mut self, filter: SpunFilter) {
        if filter != self.filter {
            self.filter = filter;
            self.clear_selection();
            self.error = None;
            self.dirty = true;
        }
    }

    fn clear_selection(&mut self) {
        self.bodies.clear();
        self.elements.clear();
        self.picks.clear();
    }

    /// Nimmt einen Klick für das aktive Feld; ein unpassender lässt die Felder, wie sie sind, und
    /// hinterlässt eine Meldung. `true`, wenn er passte.
    pub fn take_pick(&mut self, state: &AppState, pick: IncludePick) -> bool {
        let r = match self.focus {
            SpunField::Selection => self.pick_selection(state, pick),
            SpunField::Axis => self.pick_axis(state, pick),
        };
        match r {
            Ok(()) => {
                self.error = None;
                self.refresh(state);
                true
            }
            Err(e) => {
                self.error = Some(e);
                false
            }
        }
    }

    fn pick_selection(&mut self, state: &AppState, pick: IncludePick) -> Result<(), Msg> {
        match self.filter {
            SpunFilter::Bodies => {
                let wrong = || crate::msg!("spun-pick-bodies");
                let body = match pick {
                    IncludePick::Face(b, _) | IncludePick::Edge(b, _) => b,
                    IncludePick::Vertex { body, .. } => body,
                    _ => return Err(wrong()),
                };
                // Nur Körper vor der offenen Skizze.
                state.solid_before_sketch(body)?;
                let source = state.bodies.get(body).map(|b| b.source).ok_or_else(wrong)?;
                match self.bodies.iter().position(|b| *b == source) {
                    Some(i) => {
                        // Ein zweiter Klick auf denselben Körper nimmt ihn wieder heraus.
                        self.bodies.remove(i);
                        self.picks.retain(|p| pick_body(p) != Some(body));
                    }
                    None => {
                        self.bodies.push(source);
                        self.picks.push(pick);
                    }
                }
            }
            SpunFilter::Elements => {
                // Der genaue Grund des Ablehnens (aktive Skizze, Körper nach der Skizze, ...)
                // geht durch; nur ein Klick auf etwas ganz anderes bekommt den allgemeinen Hinweis.
                let source = curve_source(state, pick.clone()).map_err(|e| {
                    if matches!(
                        pick,
                        IncludePick::Edge(..)
                            | IncludePick::Vertex { .. }
                            | IncludePick::SketchCurve { .. }
                    ) {
                        e
                    } else {
                        crate::msg!("spun-pick-elements")
                    }
                })?;
                match self.elements.iter().position(|e| *e == source) {
                    Some(i) => {
                        self.elements.remove(i);
                        self.picks.retain(|p| *p != pick);
                    }
                    None => {
                        self.elements.push(source);
                        self.picks.push(pick);
                    }
                }
            }
        }
        Ok(())
    }

    fn pick_axis(&mut self, state: &AppState, pick: IncludePick) -> Result<(), Msg> {
        let wrong = || crate::msg!("spun-pick-axis");
        let (axis, shown) = match pick.clone() {
            IncludePick::Axis(a) => (SpunAxis::Ref(AxisRef::Standard(a)), true),
            IncludePick::Edge(b, e) => {
                let edge = state
                    .bodies
                    .get(b)
                    .and_then(|body| body.solid.edges.get(e))
                    .ok_or_else(|| crate::msg!("state-body-not-found"))?;
                if !matches!(edge.curve, Curve::Line { .. }) {
                    return Err(wrong());
                }
                let IncludeRef::Edge(r) = state.include_ref(pick.clone())? else {
                    return Err(wrong());
                };
                (SpunAxis::Ref(AxisRef::Edge(r)), true)
            }
            IncludePick::SketchCurve { sketch, curve } => {
                if state.open_sketch_id() == Some(sketch) {
                    // Eine Linie der eigenen Skizze.
                    let sk = state.session_sketch().ok_or_else(wrong)?;
                    own_axis_of(sk, curve).ok_or_else(wrong)?;
                    (SpunAxis::Own(curve), false)
                } else {
                    state.check_source_sketch(sketch)?;
                    existing_curve(state, sketch, curve)?;
                    let line = state
                        .document
                        .sketch_of(sketch)
                        .and_then(|sk| sk.curve_world(curve))
                        .is_some_and(|(c, ..)| matches!(c, Curve::Line { .. }));
                    if !line {
                        return Err(wrong());
                    }
                    (SpunAxis::Ref(AxisRef::SketchLine { sketch, curve }), true)
                }
            }
            _ => return Err(wrong()),
        };
        self.axis = Some(axis);
        self.axis_pick = shown.then_some(pick);
        if self.picks.is_empty() {
            self.focus = SpunField::Selection;
        }
        Ok(())
    }

    /// Was die Felder gewählt haben, für die Hervorhebung in der Ansicht.
    pub fn marks(&self) -> Vec<IncludePick> {
        self.picks
            .iter()
            .chain(self.axis_pick.iter())
            .cloned()
            .collect()
    }

    /// „OK“: legt das Drehprofil in die offene Skizze (ein Schritt); liefert die Zahl der
    /// Stücke. Bei einem Fehler bleibt die Skizze unverändert und die Meldung steht im Dialog.
    pub fn ok(&mut self, state: &mut AppState) -> Result<usize, String> {
        let Some(spec) = self.spec() else {
            let m = crate::msg!("sketch-pick-spun-profile");
            let text = m.text();
            self.error = Some(m);
            return Err(text);
        };
        match state.add_spun_profile(spec, self.project_axis, self.linked) {
            Ok(n) => {
                self.error = None;
                Ok(n)
            }
            Err(m) => {
                let text = m.text();
                self.error = Some(session_error(state, text.clone()));
                Err(text)
            }
        }
    }

    /// Zeigt den Dialog.
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> DialogOutcome {
        let mut open = true;
        let (mut ok, mut cancel) = (false, false);
        if self.dirty {
            self.refresh(state);
        }
        // Die Beschriftungen entstehen vor dem Dialog; der Inhalt ändert nur Felder von `self`
        // (abgedockt läuft er evtl. mehrmals je Bild, alles darin ist wiederholbar).
        let selection: Vec<String> = match self.filter {
            SpunFilter::Bodies => self
                .bodies
                .iter()
                .map(|b| feature_name(state, b.feature))
                .collect(),
            SpunFilter::Elements => self
                .elements
                .iter()
                .map(|e| curve_source_label(state, e))
                .collect(),
        };
        let axis: Vec<String> = self.axis.iter().map(|a| axis_label(state, a)).collect();
        let ready = self.ok_enabled();
        let result = self.preview_count().map(|n| tr!("spun-result", count = n));
        let mut filter = self.filter;
        let mut clear_selection = false;
        let mut clear_axis = false;
        Dialog::new(tr!("action-sketch-spun-profile"), DialogSize::Normal)
            .dock_key("sketch_spun_profile")
            .id(egui::Id::new("spun_profile_dialog"))
            .open(&mut open)
            .show(ctx, |ui| {
                egui::Grid::new("spun_profile_fields")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label(tr!("spun-field-filter"));
                        ui.horizontal_wrapped(|ui| {
                            ui.radio_value(
                                &mut filter,
                                SpunFilter::Bodies,
                                tr!("spun-filter-bodies"),
                            );
                            ui.radio_value(
                                &mut filter,
                                SpunFilter::Elements,
                                tr!("spun-filter-elements"),
                            );
                        });
                        ui.end_row();
                        clear_selection |= pick_row(
                            ui,
                            &mut self.focus,
                            SpunField::Selection,
                            tr!("spun-field-selection"),
                            &selection,
                        );
                        clear_axis |= pick_row(
                            ui,
                            &mut self.focus,
                            SpunField::Axis,
                            tr!("spun-field-axis"),
                            &axis,
                        );
                        ui.label("");
                        if ui
                            .checkbox(&mut self.project_axis, tr!("spun-project-axis"))
                            .changed()
                        {
                            self.error = None;
                        }
                        ui.end_row();
                        ui.label(tr!("spun-tolerance"));
                        if crate::theme::field(
                            ui,
                            egui::DragValue::new(&mut self.tolerance)
                                .speed(0.001)
                                .range(0.0001..=10.0)
                                .suffix(" mm"),
                        )
                        .changed()
                        {
                            self.error = None;
                            self.dirty = true;
                        }
                        ui.end_row();
                        ui.label("");
                        if ui.checkbox(&mut self.flip, tr!("spun-flip")).changed() {
                            self.error = None;
                            self.dirty = true;
                        }
                        ui.end_row();
                        ui.label("");
                        if ui
                            .checkbox(&mut self.linked, tr!("spun-link"))
                            .on_hover_text(tr!("sketch-project-link-hint"))
                            .changed()
                        {
                            self.error = None;
                        }
                        ui.end_row();
                    });
                if let Some(r) = &result {
                    ui.add_space(4.0);
                    ui.label(r);
                }
                let (o, c) = footer(ui, self.error(), ready);
                let (enter, esc) = keys(ui, ready);
                ok |= o || enter;
                cancel |= c || esc;
            });
        if clear_selection {
            self.clear_selection();
            self.error = None;
            self.dirty = true;
        }
        if clear_axis {
            self.axis = None;
            self.axis_pick = None;
            self.error = None;
            self.dirty = true;
        }
        self.set_filter(filter);
        if self.dirty {
            self.refresh(state);
        }
        outcome(state, ok, cancel, open, |state| self.ok(state).is_ok())
    }
}

/// Der Körperindex, den ein Klick der Auswahl trifft.
fn pick_body(p: &IncludePick) -> Option<usize> {
    match p {
        IncludePick::Face(b, _) | IncludePick::Edge(b, _) => Some(*b),
        IncludePick::Vertex { body, .. } => Some(*body),
        _ => None,
    }
}

/// Anzeigetext der Achse.
fn axis_label(state: &AppState, a: &SpunAxis) -> String {
    match a {
        SpunAxis::Own(_) => tr!("spun-axis-own"),
        SpunAxis::Ref(AxisRef::Standard(StdAxis::X)) => tr!("plane-axis-x"),
        SpunAxis::Ref(AxisRef::Standard(StdAxis::Y)) => tr!("plane-axis-y"),
        SpunAxis::Ref(AxisRef::Standard(StdAxis::Z)) => tr!("plane-axis-z"),
        SpunAxis::Ref(AxisRef::Edge(e)) => edge_label(state, e),
        SpunAxis::Ref(AxisRef::SketchLine { sketch, .. }) => tr!(
            "plane-value-sketch-line",
            name = feature_name(state, *sketch)
        ),
    }
}

/// Eine Zeile eines Wählfelds: Name, Feld (ein Klick macht es aktiv; Tooltip mit den gewählten
/// Dingen) und „Leeren“, wenn etwas gewählt ist. `true`, wenn „Leeren“ geklickt wurde.
fn pick_row(
    ui: &mut egui::Ui,
    focus: &mut SpunField,
    field: SpunField,
    label: String,
    items: &[String],
) -> bool {
    let mut clear = false;
    ui.label(label);
    ui.horizontal(|ui| {
        let r = ui.selectable_label(*focus == field, count_text(items.len()));
        let r = if items.is_empty() {
            r
        } else {
            r.on_hover_text(items.join("\n"))
        };
        if r.clicked() {
            *focus = field;
        }
        if !items.is_empty() && ui.small_button(tr!("surface-clear")).clicked() {
            clear = true;
            *focus = field;
        }
    });
    ui.end_row();
    clear
}
