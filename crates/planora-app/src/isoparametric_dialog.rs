//! Dialog „Isoparametrische Kurve“ (Skizzen 11b, Spec 6), wie „Isoparametric Curve“ in Autodesk
//! Fusion.
//!
//! Er steht am Knopf „Projizieren/Einbeziehen“ als Art [`ToolKind::Isoparametric`] und gehört wie
//! „Drehprofil“ zu [`crate::surface_project_dialog`] (Öffnen, Klicks aus der Ansicht, Abschluss).
//! Zwei Wählfelder: **Fläche** (eine Körperfläche vor der offenen Skizze) und **Punkt** (ein
//! Klick auf die gewählte Fläche setzt die **Entfernung**, den Anteil 0–1 am Parameterbereich,
//! über [`planora_feature::isoparametric::iso_value_at`]). Dazu **Richtung** (U, V, U und V),
//! **Art** (**Einzeln** mit der Entfernung, **Verteilt** mit der **Anzahl**, gleichmäßig ohne die
//! Ränder) und **Projektionsverknüpfung**. Die Vorschau rechnet mit derselben Rechnung wie das
//! Anlegen ([`AppState::iso_pieces`]) und nur dann neu, wenn sich im Dialog etwas ändert; sie
//! zeichnet die Kurven in die Ansicht ([`crate::sketch_view::iso_preview_shapes`]). Erst „OK“
//! legt an ([`AppState::add_isoparametric`], ein Rückgängig-Schritt, die Skizze wird zur
//! 3D-Skizze); Abbrechen, ✕, Esc und ein Werkzeugwechsel hinterlassen nichts.
//!
//! [`ToolKind::Isoparametric`]: crate::sketch_kinds::ToolKind::Isoparametric

use crate::dialog::{Dialog, DialogSize};
use crate::msg::Msg;
use crate::state::{AppState, IncludePick};
use crate::surface_project_dialog::{
    DialogOutcome, SurfaceDialog, count_text, feature_name, footer, keys, outcome, session_error,
};
use planora_feature::{FaceRef, FeatureId, IncludeRef, IsoDirection};
use planora_i18n::tr;
use planora_kernel::{Face, Id};
use planora_math::{DVec3, Frame, TAU};
use planora_sketch::helpers::Included;

/// Ein Paar (Richtung, Anteil 0–1) einer Isokurve.
pub type IsoPair = (IsoDirection, f64);

/// Größte Anzahl bei „Verteilt“ (**Annahme**, in `offene-punkte.md`).
pub const MAX_COUNT: u32 = 50;
/// Abtastpunkte je Vorschaukurve.
const PREVIEW_SAMPLES: usize = 48;

/// Richtung der Isokurven im Dialog.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IsoDirections {
    /// Die Kurven `u = const`.
    #[default]
    U,
    /// Die Kurven `v = const`.
    V,
    /// Beide.
    Both,
}

impl IsoDirections {
    /// Die einzelnen Richtungen in der Reihenfolge des Anlegens.
    pub fn list(self) -> &'static [IsoDirection] {
        match self {
            IsoDirections::U => &[IsoDirection::U],
            IsoDirections::V => &[IsoDirection::V],
            IsoDirections::Both => &[IsoDirection::U, IsoDirection::V],
        }
    }
}

/// Art der Lage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IsoMode {
    /// „Einzeln“: eine Kurve bei der Entfernung.
    #[default]
    Single,
    /// „Verteilt“: `count` Kurven gleichmäßig über den Bereich, ohne die Ränder (in einer
    /// geschlossenen Richtung mit gleichen Abständen rundum, keine auf der Naht).
    Distributed,
}

/// Ein Wählfeld des Dialogs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IsoField {
    #[default]
    Face,
    Point,
}

/// Die Anteile 0–1 der Kurven einer Richtung: bei „Einzeln“ `[distance]` (auf 0–1 begrenzt), bei
/// „Verteilt“ in einer offenen Richtung `k / (count + 1)` für `k = 1..=count` (ohne die Ränder),
/// in einer geschlossenen (`closed`: voller Umlauf mit Naht) `(k + 0,5) / count` für
/// `k = 0..count` (gleiche Abstände rundum, keine Kurve auf der Naht; Entscheidung M4, **Annahme**
/// zu Fusion). `count` wird auf 1–[`MAX_COUNT`] begrenzt.
pub fn iso_values(mode: IsoMode, distance: f64, count: u32, closed: bool) -> Vec<f64> {
    match mode {
        IsoMode::Single => {
            let d = if distance.is_finite() { distance } else { 0.5 };
            vec![d.clamp(0.0, 1.0)]
        }
        IsoMode::Distributed => {
            let n = count.clamp(1, MAX_COUNT);
            if closed {
                (0..n)
                    .map(|k| (f64::from(k) + 0.5) / f64::from(n))
                    .collect()
            } else {
                (1..=n).map(|k| f64::from(k) / f64::from(n + 1)).collect()
            }
        }
    }
}

/// Ausdehnung ab diesem Bruchteil der Periode gilt als voller Umlauf (wie in
/// `planora_feature::isoparametric`).
const FULL_TURN: f64 = 1.0 - 1e-6;

/// Ist die Richtung `dir` der Fläche `geom` geschlossen: Läuft ihr Anteil (bei `U` der u-Bereich)
/// über den vollen Umlauf einer periodischen Richtung?
fn closed(geom: &planora_kernel::boolean::FaceGeom, dir: IsoDirection) -> bool {
    let range = planora_feature::isoparametric::iso_range(geom);
    let ((lo, hi), period) = match dir {
        IsoDirection::U => (range.u, geom.surface.u_period()),
        IsoDirection::V => (range.v, geom.surface.v_period()),
    };
    period.is_some_and(|t| hi - lo >= t * FULL_TURN)
}

/// Der offene Dialog „Isoparametrische Kurve“ ([`SurfaceDialog::Iso`]).
#[derive(Clone, Debug)]
pub struct IsoDialog {
    /// Die gewählte Fläche; `None`, solange keine gewählt ist.
    pub face: Option<FaceRef>,
    pub directions: IsoDirections,
    pub mode: IsoMode,
    /// „Entfernung“: Anteil 0–1 am Parameterbereich (bei „Einzeln“).
    pub distance: f64,
    /// „Anzahl“ (bei „Verteilt“), 1–[`MAX_COUNT`].
    pub count: u32,
    /// „Projektionsverknüpfung“: violette, verknüpfte Kurven statt freier.
    pub linked: bool,
    /// Das aktive Wählfeld.
    pub focus: IsoField,
    /// Der Klick der Fläche (Körper, Fläche), für die Hervorhebung und den Punkt.
    face_pick: Option<(usize, Id<Face>)>,
    /// Der zuletzt gewählte Punkt als Anteile `[u, v]` (beim Wählen mit
    /// [`planora_feature::isoparametric::iso_value_at`] gerechnet). Bei „Einzeln“ bekommt jede
    /// Richtung ihren Anteil daraus (die Kurven laufen durch den Punkt); ein getippter Wert in
    /// „Entfernung“ nimmt ihn weg.
    point: Option<[f64; 2]>,
    /// Ist die Richtung `[U, V]` der gewählten Fläche geschlossen (voller Umlauf)? Beim Wählen
    /// der Fläche gerechnet.
    closed: [bool; 2],
    /// Meldung eines abgelehnten Klicks oder OKs.
    pub(crate) error: Option<Msg>,
    /// Ergebnis der Vorschau: Zahl der Stücke oder die Meldung; `None` ohne Fläche.
    preview: Option<Result<usize, Msg>>,
    /// Die Kurven der Vorschau als Linienzüge in Weltkoordinaten.
    lines: Vec<Vec<DVec3>>,
    /// Die zuletzt gerechneten Eingaben; die Vorschau rechnet nur neu, wenn sie sich ändern.
    seen: Option<(Option<FaceRef>, Vec<IsoPair>)>,
}

impl Default for IsoDialog {
    fn default() -> Self {
        Self {
            face: None,
            directions: IsoDirections::U,
            mode: IsoMode::Single,
            distance: 0.5,
            count: 3,
            linked: true,
            focus: IsoField::Face,
            face_pick: None,
            point: None,
            closed: [false; 2],
            error: None,
            preview: None,
            lines: Vec::new(),
            seen: None,
        }
    }
}

/// Ist der Dialog „Isoparametrische Kurve“ offen? Dann wählt ein Klick in der Ansicht nur Flächen
/// (keine Ebenen, die die Fläche verdecken, und keine Kanten).
pub fn is_open(state: &AppState) -> bool {
    matches!(state.surface_dialog, Some(SurfaceDialog::Iso(_)))
}

/// Ein Klick in der Ansicht auf die Fläche `face` des Körpers `body` an der Weltstelle `at`: im
/// Feld „Fläche“ wählt er die Fläche, im Feld „Punkt“ setzt er die Entfernung. `true`, wenn er
/// passte.
pub fn click(state: &mut AppState, body: usize, face: Id<Face>, at: DVec3) -> bool {
    let focus = match state.surface_dialog.as_ref() {
        Some(SurfaceDialog::Iso(d)) => d.focus,
        _ => return false,
    };
    match focus {
        IsoField::Face => {
            crate::surface_project_dialog::deliver(state, IncludePick::Face(body, face))
        }
        IsoField::Point => deliver_point(state, body, face, at),
    }
}

/// Gibt einen Punkt `at` (Welt) auf der Fläche `face` des Körpers `body` an das Feld „Punkt“ des
/// offenen Dialogs; `true`, wenn er auf der gewählten Fläche liegt.
pub fn deliver_point(state: &mut AppState, body: usize, face: Id<Face>, at: DVec3) -> bool {
    let Some(SurfaceDialog::Iso(mut d)) = state.surface_dialog.take() else {
        return false;
    };
    let taken = d.take_point(state, body, face, at);
    state.surface_dialog = Some(SurfaceDialog::Iso(d));
    taken
}

impl IsoDialog {
    /// Meldung des Dialogs in der aktuellen Sprache: ein abgelehnter Klick oder, sonst, der
    /// Fehler der Vorschau.
    pub fn error(&self) -> Option<String> {
        self.error
            .as_ref()
            .or_else(|| self.preview.as_ref().and_then(|p| p.as_ref().err()))
            .map(Msg::text)
    }

    /// Die Zahl der Kurvenstücke der Vorschau; `None` ohne Fläche oder bei einem Fehler.
    pub fn preview_count(&self) -> Option<usize> {
        self.preview.as_ref().and_then(|p| p.as_ref().ok()).copied()
    }

    /// Die Kurven der Vorschau (Weltkoordinaten), für die Ansicht.
    pub fn preview_lines(&self) -> &[Vec<DVec3>] {
        &self.lines
    }

    /// Nur mit Fläche und mindestens einem Stück ist OK frei; ein abgelehnter Klick sperrt OK
    /// nicht (er ist ein Hinweis).
    pub fn ok_enabled(&self) -> bool {
        self.preview_count().is_some_and(|n| n > 0)
    }

    /// Die Paare (Richtung, Anteil) der Kurven aus den Feldern. Bei „Einzeln“ mit Punkt hat jede
    /// Richtung ihren Anteil aus dem Punkt, ohne Punkt gilt „Entfernung“ für alle Richtungen. Bei
    /// „Verteilt“ zählt, ob die Richtung geschlossen ist ([`iso_values`]).
    pub fn pairs(&self) -> Vec<IsoPair> {
        let mut out = Vec::new();
        for &dir in self.directions.list() {
            let k = usize::from(dir == IsoDirection::V);
            match (self.mode, self.point) {
                (IsoMode::Single, Some(shares)) => out.push((dir, shares[k])),
                _ => out.extend(
                    iso_values(self.mode, self.distance, self.count, self.closed[k])
                        .into_iter()
                        .map(|v| (dir, v)),
                ),
            }
        }
        out
    }

    /// Rechnet die Vorschau neu (mit der Rechnung des Anlegens). Mit Punkt zeigt „Entfernung“
    /// danach den Anteil der ersten Richtung (bei einem Richtungswechsel neu gerechnet).
    pub fn refresh(&mut self, state: &AppState) {
        let pairs = self.pairs();
        if self.mode == IsoMode::Single
            && self.point.is_some()
            && let Some(&(_, v)) = pairs.first()
        {
            self.distance = v;
        }
        self.seen = Some((self.face.clone(), pairs.clone()));
        self.lines.clear();
        let Some(face) = self.face.clone() else {
            self.preview = None;
            return;
        };
        let frame = state.session_sketch().map(|sk| *sk.frame());
        let mut count = 0;
        for &(dir, value) in &pairs {
            match state.iso_pieces(&face, dir, value) {
                Ok(pieces) => {
                    count += pieces.len();
                    if let Some(f) = frame {
                        self.lines.extend(pieces.iter().map(|g| polyline(g, &f)));
                    }
                }
                Err(m) => {
                    self.lines.clear();
                    self.preview = Some(Err(m));
                    return;
                }
            }
        }
        self.preview = Some(if count == 0 {
            Err(crate::msg!("iso-no-hit"))
        } else {
            Ok(count)
        });
    }

    /// Hat sich seit der letzten Vorschau eine Eingabe geändert?
    fn stale(&self) -> bool {
        self.seen
            .as_ref()
            .is_none_or(|(f, p)| *f != self.face || *p != self.pairs())
    }

    /// Nimmt einen Klick ohne Punkt (aus [`crate::surface_project_dialog::deliver`]) in einem der
    /// beiden Felder: Er wählt die Fläche (nur eine Körperfläche vor der offenen Skizze), nimmt
    /// einen gesetzten Punkt weg und macht „Punkt“ zum aktiven Feld. Ein unpassender Klick lässt
    /// die Felder, wie sie sind, und hinterlässt einen Hinweis. `true`, wenn er passte.
    pub fn take_pick(&mut self, state: &AppState, pick: IncludePick) -> bool {
        if !self.set_face(state, pick) {
            return false;
        }
        self.point = None;
        self.refresh(state);
        true
    }

    /// Setzt die Fläche aus dem Klick `pick`; `false` mit Hinweis, wenn er nicht passt.
    fn set_face(&mut self, state: &AppState, pick: IncludePick) -> bool {
        let IncludePick::Face(body, face) = pick else {
            self.error = Some(crate::msg!("iso-pick-face"));
            return false;
        };
        let r = match state.include_ref(pick) {
            Ok(IncludeRef::FaceBoundary(r)) => r,
            Ok(_) => {
                self.error = Some(crate::msg!("iso-pick-face"));
                return false;
            }
            Err(e) => {
                self.error = Some(e);
                return false;
            }
        };
        let geom = state
            .solid_before_sketch(body)
            .ok()
            .filter(|s| s.faces.get(face).is_some());
        self.closed = geom.map_or([false; 2], |s| {
            let g = planora_kernel::boolean::face_geom(&s, face);
            [closed(&g, IsoDirection::U), closed(&g, IsoDirection::V)]
        });
        self.face = Some(r);
        self.face_pick = Some((body, face));
        self.focus = IsoField::Point;
        self.error = None;
        true
    }

    /// Nimmt einen Punkt `at` (Welt) auf der Fläche `face` des Körpers `body` (Feld „Punkt“):
    /// Liegt er auf einer anderen Fläche, wird diese die Fläche (wie Fusion). Bei „Einzeln“
    /// laufen die Kurven danach durch den Punkt. Ein Klick auf etwas, das keine Körperfläche vor
    /// der Skizze ist, lässt die Felder, wie sie sind, und hinterlässt einen Hinweis.
    fn take_point(&mut self, state: &AppState, body: usize, face: Id<Face>, at: DVec3) -> bool {
        if self.face_pick != Some((body, face))
            && !self.set_face(state, IncludePick::Face(body, face))
        {
            return false;
        }
        let Some(solid) = state
            .solid_before_sketch(body)
            .ok()
            .filter(|s| s.faces.get(face).is_some())
        else {
            self.error = Some(crate::msg!("iso-pick-point"));
            return false;
        };
        let g = planora_kernel::boolean::face_geom(&solid, face);
        let share = |d| planora_feature::isoparametric::iso_value_at(&g, at, d);
        self.point = Some([share(IsoDirection::U), share(IsoDirection::V)]);
        self.focus = IsoField::Point;
        self.error = None;
        self.refresh(state);
        true
    }

    /// Was die Felder gewählt haben, für die Hervorhebung in der Ansicht.
    pub fn marks(&self) -> Vec<IncludePick> {
        self.face_pick
            .iter()
            .map(|&(b, f)| IncludePick::Face(b, f))
            .collect()
    }

    /// „OK“: legt die Isokurven in die offene Skizze (ein Schritt); liefert die Zahl der Stücke.
    /// Bei einem Fehler bleibt die Skizze unverändert, und die Meldung steht im Dialog.
    pub fn ok(&mut self, state: &mut AppState) -> Result<usize, String> {
        let Some(face) = self.face.clone() else {
            let m = crate::msg!("sketch-pick-isoparametric");
            let text = m.text();
            self.error = Some(m);
            return Err(text);
        };
        let pairs = self.pairs();
        match state.add_isoparametric(face, &pairs, self.linked) {
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
        let mut field_enter = false;
        if self.stale() {
            self.refresh(state);
        }
        // Die Beschriftungen entstehen vor dem Dialog; der Inhalt ändert nur Felder von `self`
        // (abgedockt läuft er evtl. mehrmals je Bild, alles darin ist wiederholbar).
        let face: Vec<String> = self
            .face
            .iter()
            .map(|r| {
                tr!(
                    "plane-value-face",
                    name = feature_name(state, FeatureId(r.origin))
                )
            })
            .collect();
        let point: Vec<String> = self.point.iter().map(|_| tr!("iso-field-point")).collect();
        let ready = self.ok_enabled();
        let result = self.preview_count().map(|n| tr!("iso-result", count = n));
        let mut clear_face = false;
        let mut clear_point = false;
        Dialog::new(tr!("action-sketch-isoparametric"), DialogSize::Normal)
            .dock_key("sketch_isoparametric")
            .id(egui::Id::new("isoparametric_dialog"))
            .open(&mut open)
            .show(ctx, |ui| {
                egui::Grid::new("isoparametric_fields")
                    .num_columns(2)
                    .show(ui, |ui| {
                        clear_face |= pick_row(
                            ui,
                            &mut self.focus,
                            IsoField::Face,
                            tr!("iso-field-face"),
                            &face,
                        );
                        clear_point |= pick_row(
                            ui,
                            &mut self.focus,
                            IsoField::Point,
                            tr!("iso-field-point"),
                            &point,
                        );
                        ui.label(tr!("iso-direction"));
                        ui.horizontal_wrapped(|ui| {
                            for (d, label) in [
                                (IsoDirections::U, tr!("iso-dir-u")),
                                (IsoDirections::V, tr!("iso-dir-v")),
                                (IsoDirections::Both, tr!("iso-dir-both")),
                            ] {
                                if ui.radio_value(&mut self.directions, d, label).changed() {
                                    self.error = None;
                                }
                            }
                        });
                        ui.end_row();
                        ui.label(tr!("iso-mode"));
                        ui.horizontal_wrapped(|ui| {
                            for (m, label) in [
                                (IsoMode::Single, tr!("iso-mode-single")),
                                (IsoMode::Distributed, tr!("iso-mode-distributed")),
                            ] {
                                if ui.radio_value(&mut self.mode, m, label).changed() {
                                    self.error = None;
                                }
                            }
                        });
                        ui.end_row();
                        match self.mode {
                            IsoMode::Single => {
                                ui.label(tr!("iso-distance"));
                                let r = crate::theme::field(
                                    ui,
                                    egui::DragValue::new(&mut self.distance)
                                        .speed(0.005)
                                        .range(0.0..=1.0)
                                        .max_decimals(4),
                                );
                                field_enter |= crate::dialog::left_with_enter(ui, &r);
                                if r.changed() {
                                    // Ein getippter Wert gilt für alle Richtungen; der Punkt
                                    // ist dann nicht mehr die Lage.
                                    self.point = None;
                                    self.error = None;
                                }
                            }
                            IsoMode::Distributed => {
                                ui.label(tr!("iso-count"));
                                let r = crate::theme::field(
                                    ui,
                                    egui::DragValue::new(&mut self.count)
                                        .speed(0.1)
                                        .range(1..=MAX_COUNT),
                                );
                                field_enter |= crate::dialog::left_with_enter(ui, &r);
                                if r.changed() {
                                    self.error = None;
                                }
                            }
                        }
                        ui.end_row();
                        ui.label("");
                        if ui
                            .checkbox(&mut self.linked, tr!("iso-link"))
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
                ok |= o || enter || (field_enter && ready);
                cancel |= c || esc;
            });
        if clear_face {
            self.face = None;
            self.face_pick = None;
            self.point = None;
            self.error = None;
        }
        if clear_point {
            self.point = None;
            self.error = None;
        }
        // Begrenzen (auch getippte Werte): Entfernung 0–1, Anzahl 1–50.
        if !self.distance.is_finite() {
            self.distance = 0.5;
        }
        self.distance = self.distance.clamp(0.0, 1.0);
        self.count = self.count.clamp(1, MAX_COUNT);
        if self.stale() {
            self.refresh(state);
        }
        outcome(state, ok, cancel, open, |state| self.ok(state).is_ok())
    }
}

/// Eine Zeile eines Wählfelds: Name, Feld (ein Klick macht es aktiv) und „Leeren“, wenn etwas
/// gewählt ist. `true`, wenn „Leeren“ geklickt wurde.
fn pick_row(
    ui: &mut egui::Ui,
    focus: &mut IsoField,
    field: IsoField,
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

/// Linienzug (Welt) eines Kurvenstücks in Skizzenkoordinaten von `frame`.
fn polyline(g: &Included, frame: &Frame) -> Vec<DVec3> {
    let n = PREVIEW_SAMPLES;
    let local: Vec<DVec3> = match g {
        Included::Point(p) => vec![*p],
        Included::Line(a, b) => vec![*a, *b],
        Included::Circle { c, r, n: axis } => {
            let (x, y) = basis(*axis);
            (0..=n)
                .map(|i| {
                    let t = TAU * i as f64 / n as f64;
                    *c + (x * t.cos() + y * t.sin()) * *r
                })
                .collect()
        }
        Included::Arc {
            c,
            start,
            end,
            n: axis,
        } => {
            let a = *start - *c;
            let b = *end - *c;
            let r = a.length();
            let x = a / r.max(f64::MIN_POSITIVE);
            let y = axis.normalize_or_zero().cross(x);
            let mut sweep = b.dot(y).atan2(b.dot(x));
            if sweep <= 0.0 {
                sweep += TAU;
            }
            (0..=n)
                .map(|i| {
                    let t = sweep * i as f64 / n as f64;
                    *c + (x * t.cos() + y * t.sin()) * r
                })
                .collect()
        }
        Included::Ellipse {
            c,
            major,
            minor,
            n: axis,
        } => {
            let y = axis.normalize_or_zero().cross(major.normalize_or_zero()) * *minor;
            (0..=n)
                .map(|i| {
                    let t = TAU * i as f64 / n as f64;
                    *c + *major * t.cos() + y * t.sin()
                })
                .collect()
        }
        Included::EllipticArc {
            c,
            major,
            minor,
            start,
            end,
            n: axis,
        } => {
            let a = major.length();
            let x = major.normalize_or_zero();
            let y = axis.normalize_or_zero().cross(x);
            let angle = |p: DVec3| {
                let d = p - *c;
                (d.dot(y) / minor.max(f64::MIN_POSITIVE)).atan2(d.dot(x) / a.max(f64::MIN_POSITIVE))
            };
            let t0 = angle(*start);
            let mut sweep = angle(*end) - t0;
            if sweep <= 0.0 {
                sweep += TAU;
            }
            (0..=n)
                .map(|i| {
                    let t = t0 + sweep * i as f64 / n as f64;
                    *c + x * (a * t.cos()) + y * (*minor * t.sin())
                })
                .collect()
        }
        Included::Spline { curve } => {
            let (t0, t1) = curve.domain();
            (0..=n)
                .map(|i| curve.point(t0 + (t1 - t0) * i as f64 / n as f64))
                .collect()
        }
    };
    local
        .into_iter()
        .map(|p| frame.point(p.x, p.y, p.z))
        .collect()
}

/// Zwei Einheitsvektoren senkrecht zu `n` und zueinander.
fn basis(n: DVec3) -> (DVec3, DVec3) {
    let n = n.normalize_or_zero();
    let x = n.any_orthonormal_vector();
    (x, n.cross(x))
}
