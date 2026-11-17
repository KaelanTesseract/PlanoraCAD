//! Dialoge „Auf Fläche projizieren“ (Spec 6) und „Schnittkurve“ (Spec 7) der Skizzen 10, wie in
//! Autodesk Fusion.
//!
//! Beide stehen am Knopf „Projizieren/Einbeziehen“ als eigene Arten ([`ToolKind::has_dialog`]):
//! Ist eine davon gewählt, steht ihr Dialog offen ([`sync`]). Ein Klick in die 3D-Szene wählt
//! wie „3D-Geometrie einbeziehen“ und füllt das aktive Feld ([`deliver`]), wie die Auswahlfelder
//! des Ebenen-Fensters ([`crate::plane_dialog::PlanePick`]). Ein zweiter Klick auf dasselbe nimmt
//! es wieder aus einem Mehrfachfeld. Erst „OK“ oder Eingabe legt an, als verknüpfte 3D-Geometrie
//! in der offenen Skizze mit der Herkunft [`CopySource::OnSurface`] bzw.
//! [`CopySource::Intersection`] (ein Rückgängig-Schritt). „Abbrechen“, ✕, Esc und ein
//! Werkzeugwechsel hinterlassen nichts. Trifft nichts oder dauert die Rechnung zu lange, bleibt
//! der Dialog mit der Meldung offen.

use crate::browser::error_color;
use crate::dialog::{Dialog, DialogSize};
use crate::msg::Msg;
use crate::sketch_kinds::ToolKind;
use crate::sketch_state::Tool;
use crate::spun_profile_dialog::SpunProfileDialog;
use crate::state::{AppState, IncludePick};
use planora_feature::planes::StdAxis;
use planora_feature::surface_project::{
    CurveSource, FaceTarget, IntersectTarget, IntersectionSpec, ProjectType, SurfaceProjection,
};
use planora_feature::{
    CopySource, EdgeRef, FaceRef, FeatureId, IncludeRef, PlaneId, StandardPlane,
};
use planora_i18n::tr;
use planora_kernel::Curve;
use planora_math::DVec3;
use planora_sketch::model::CurveId;
use planora_sketch::plane_of_face;

/// Was ein Bild mit dem Dialog ergab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogOutcome {
    /// Der Dialog bleibt offen.
    Open,
    /// „OK“: angelegt (ein Rückgängig-Schritt), der Dialog ist zu.
    Applied,
    /// Abgebrochen (Abbrechen, ✕, Esc), nichts geändert.
    Cancelled,
}

/// Ein Wählfeld der beiden Dialoge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    /// „Flächen“ (Auf Fläche projizieren).
    Faces,
    /// „Kurven“ (Auf Fläche projizieren).
    Curves,
    /// „Richtung“ bei „Entlang Vektor“.
    Direction,
    /// „Kurve“ (Schnittkurve).
    Curve,
    /// „Kurven/Flächen“ (Schnittkurve).
    Targets,
}

/// Projektionsart des Dialogs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectKind {
    /// „Nächster Punkt“.
    ClosestPoint,
    /// „Entlang Vektor“ mit dem Feld „Richtung“.
    AlongVector,
}

/// Woher die Richtung von „Entlang Vektor“ stammt (nur für die Anzeige; gerechnet wird mit dem
/// Vektor, wie [`ProjectType::AlongVector`] ihn speichert).
#[derive(Clone, Debug, PartialEq)]
pub enum DirectionSource {
    Edge(EdgeRef),
    /// Die Normale einer ebenen Körperfläche.
    Face(FaceRef),
    /// Die Normale einer Grund- oder Konstruktionsebene.
    Plane(PlaneId),
    Axis(StdAxis),
    SketchLine {
        sketch: FeatureId,
        curve: CurveId,
    },
}

/// Die gewählte Richtung: Weltvektor (Länge 1) und Herkunft.
#[derive(Clone, Debug, PartialEq)]
pub struct Direction {
    pub vector: DVec3,
    pub source: DirectionSource,
}

/// Dialog „Auf Fläche projizieren“ (Spec 6).
#[derive(Clone, Debug)]
pub struct SurfaceProjectDialog {
    /// „Flächen“: Körperflächen oder Ebenen.
    pub faces: Vec<FaceTarget>,
    /// „Kurven“: Kurven früherer Skizzen, Körperkanten und -ecken.
    pub curves: Vec<CurveSource>,
    /// „Projektionsart“.
    pub kind: ProjectKind,
    /// „Richtung“ (nur bei „Entlang Vektor“).
    pub direction: Option<Direction>,
    /// Das Feld, das den nächsten Klick bekommt.
    pub focus: Field,
    error: Option<Msg>,
    /// Die Klicks hinter den Feldern, für die Hervorhebung in der Ansicht (Task H3).
    picks: Picks,
}

/// Dialog „Schnittkurve“ (Spec 7).
#[derive(Clone, Debug)]
pub struct IntersectionCurveDialog {
    /// „Kurve“: eine 2D-Kurve einer früheren Skizze.
    pub curve: Option<CurveSource>,
    /// „Kurven/Flächen“: Körperflächen, Ebenen oder 2D-Kurven anderer früherer Skizzen.
    pub targets: Vec<IntersectTarget>,
    /// Das Feld, das den nächsten Klick bekommt.
    pub focus: Field,
    error: Option<Msg>,
    /// Die Klicks hinter den Feldern, für die Hervorhebung in der Ansicht (Task H3).
    picks: Picks,
}

/// Die Klicks, die die Felder eines Dialogs gefüllt haben, je mit ihrem Feld: Daraus entsteht die
/// Hervorhebungsliste der Ansicht ([`AppState::dialog_marks`]). Die Felder selbst speichern
/// Verweise, die Ansicht braucht die Körperindizes.
#[derive(Clone, Debug, Default)]
struct Picks(Vec<(Field, IncludePick)>);

impl Picks {
    /// Nimmt `pick` ins Mehrfachfeld `field` auf oder, wenn er schon darin steht, heraus (wie
    /// [`toggle`] für die Werte).
    fn toggle(&mut self, field: Field, pick: IncludePick) {
        toggle(&mut self.0, (field, pick));
    }

    /// Ersetzt den Klick des Einzelfelds `field`.
    fn set(&mut self, field: Field, pick: IncludePick) {
        self.clear(field);
        self.0.push((field, pick));
    }

    /// Leert das Feld `field`.
    fn clear(&mut self, field: Field) {
        self.0.retain(|(f, _)| *f != field);
    }

    /// Die Klicks der Felder, für die `shown` gilt.
    fn marks(&self, shown: impl Fn(Field) -> bool) -> Vec<IncludePick> {
        self.0
            .iter()
            .filter(|(f, _)| shown(*f))
            .map(|(_, p)| p.clone())
            .collect()
    }
}

/// Der offene Dialog der Skizzensitzung ([`AppState::surface_dialog`]).
#[derive(Clone, Debug)]
pub enum SurfaceDialog {
    Project(SurfaceProjectDialog),
    Intersection(IntersectionCurveDialog),
    /// „Drehprofil“ (Skizzen 11b).
    Spun(SpunProfileDialog),
}

// ---- Öffnen, Klicks, Zeigen -----------------------------------------------------------------------

/// Die Art der offenen Skizze, die einen Dialog hat (Werkzeug „Projizieren/Einbeziehen“).
fn wanted(state: &AppState) -> Option<ToolKind> {
    state
        .sketch_session
        .as_ref()
        .filter(|se| se.tool == Tool::ProjectEdge && se.kinds.project.has_dialog())
        .map(|se| se.kinds.project)
}

/// Hält den Dialog passend zur gewählten Art: öffnet ihn leer, wenn „Auf Fläche projizieren“ oder
/// „Schnittkurve“ gewählt ist, und schließt ihn ohne Spur, wenn das Werkzeug oder die Art wechselt
/// oder die Skizze zugeht.
pub fn sync(state: &mut AppState) {
    let want = wanted(state);
    let have = state.surface_dialog.as_ref().map(SurfaceDialog::kind);
    if want != have {
        state.surface_dialog = want.map(|k| match k {
            ToolKind::IntersectionCurve => {
                SurfaceDialog::Intersection(IntersectionCurveDialog::new())
            }
            ToolKind::SpunProfile => SurfaceDialog::Spun(SpunProfileDialog::default()),
            _ => SurfaceDialog::Project(SurfaceProjectDialog::new()),
        });
    }
    refresh_marks(state);
}

/// Hält die Hervorhebungsliste der Ansicht passend zu den Feldern des offenen Dialogs (ohne
/// Dialog leer).
fn refresh_marks(state: &mut AppState) {
    let marks = state
        .surface_dialog
        .as_ref()
        .map(SurfaceDialog::marks)
        .unwrap_or_default();
    state.set_dialog_marks(marks);
}

/// Gibt einen Klick in der Ansicht an das aktive Feld des offenen Dialogs; `true`, wenn er passte.
pub fn deliver(state: &mut AppState, pick: IncludePick) -> bool {
    sync(state);
    let Some(mut d) = state.surface_dialog.take() else {
        return false;
    };
    let taken = d.take_pick(state, pick);
    state.surface_dialog = Some(d);
    refresh_marks(state);
    taken
}

/// Ein Klick in der Ansicht traf nichts Wählbares: Der Hinweis, was zu wählen ist, steht im
/// offenen Dialog (nicht in der Skizzenpalette).
pub fn refuse(state: &mut AppState) {
    sync(state);
    match state.surface_dialog.as_mut() {
        Some(SurfaceDialog::Project(d)) => {
            d.error = Some(crate::msg!("sketch-pick-project-to-surface"));
        }
        Some(SurfaceDialog::Intersection(d)) => {
            d.error = Some(crate::msg!("sketch-pick-intersection-curve"));
        }
        Some(SurfaceDialog::Spun(d)) => {
            d.error = Some(crate::msg!("sketch-pick-spun-profile"));
        }
        None => {}
    }
}

/// Zeigt den offenen Dialog (nach [`sync`]); `None`, wenn keiner offen ist.
pub fn show_open(ctx: &egui::Context, state: &mut AppState) -> Option<DialogOutcome> {
    sync(state);
    let mut d = state.surface_dialog.take()?;
    let out = d.show(ctx, state);
    if out == DialogOutcome::Open && state.surface_dialog.is_none() {
        state.surface_dialog = Some(d);
    }
    refresh_marks(state);
    Some(out)
}

/// Beendet den Befehl: zurück zum Auswahlwerkzeug (wie Fusion nach OK oder Abbrechen).
fn finish(state: &mut AppState) {
    state.surface_dialog = None;
    state.set_dialog_marks(Vec::new());
    if let Some((se, sk)) = state.session_sketch_mut() {
        se.choose_tool(sk, Tool::Select);
    }
    state.set_hover(None);
}

impl SurfaceDialog {
    /// Die Art des Dialogs am Knopf.
    pub fn kind(&self) -> ToolKind {
        match self {
            SurfaceDialog::Project(_) => ToolKind::ProjectToSurface,
            SurfaceDialog::Intersection(_) => ToolKind::IntersectionCurve,
            SurfaceDialog::Spun(_) => ToolKind::SpunProfile,
        }
    }

    /// Nimmt einen Klick für das aktive Feld; `true`, wenn er passte.
    pub fn take_pick(&mut self, state: &AppState, pick: IncludePick) -> bool {
        match self {
            SurfaceDialog::Project(d) => d.take_pick(state, pick),
            SurfaceDialog::Intersection(d) => d.take_pick(state, pick),
            SurfaceDialog::Spun(d) => d.take_pick(state, pick),
        }
    }

    /// Zeigt den Dialog.
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> DialogOutcome {
        match self {
            SurfaceDialog::Project(d) => d.show(ctx, state),
            SurfaceDialog::Intersection(d) => d.show(ctx, state),
            SurfaceDialog::Spun(d) => d.show(ctx, state),
        }
    }

    /// Was die Felder gewählt haben, für die Hervorhebung in der Ansicht.
    pub fn marks(&self) -> Vec<IncludePick> {
        match self {
            SurfaceDialog::Project(d) => d.marks(),
            SurfaceDialog::Intersection(d) => d.marks(),
            SurfaceDialog::Spun(d) => d.marks(),
        }
    }
}

// ---- Umwandeln der Klicks -------------------------------------------------------------------------

/// Nimmt `v` in die Liste auf oder, wenn es schon darin steht, wieder heraus.
fn toggle<T: PartialEq>(list: &mut Vec<T>, v: T) {
    match list.iter().position(|x| *x == v) {
        Some(i) => {
            list.remove(i);
        }
        None => list.push(v),
    }
}

/// Eine Kurve der aktiven Skizze ist nie Quelle (Spec 6, wie Fusions Rat, eine neue Skizze zu
/// beginnen; verhindert Kreisbezüge).
fn refuse_active(state: &AppState, pick: &IncludePick) -> Result<(), Msg> {
    match pick {
        IncludePick::SketchCurve { sketch, .. } if state.open_sketch_id() == Some(*sketch) => {
            Err(crate::msg!("surface-curve-active-sketch"))
        }
        _ => Ok(()),
    }
}

/// Die Kurve `curve` der früheren Skizze `sketch` muss es geben.
pub(crate) fn existing_curve(
    state: &AppState,
    sketch: FeatureId,
    curve: CurveId,
) -> Result<(), Msg> {
    state
        .document
        .sketch_of(sketch)
        .and_then(|sk| sk.curve(curve))
        .map(|_| ())
        .ok_or_else(|| crate::msg!("sketch-include-failed"))
}

/// Feld „Flächen“: Körperfläche (vor der Skizze) oder Grund- bzw. Konstruktionsebene (davor).
fn face_target(state: &AppState, pick: IncludePick) -> Result<FaceTarget, Msg> {
    let wrong = || crate::msg!("surface-pick-faces");
    if !matches!(pick, IncludePick::Face(..) | IncludePick::Plane(_)) {
        return Err(wrong());
    }
    match state.include_ref(pick)? {
        IncludeRef::FaceBoundary(r) => Ok(FaceTarget::Face(r)),
        IncludeRef::Plane(p) => Ok(FaceTarget::Plane(p)),
        _ => Err(wrong()),
    }
}

/// Feld „Kurven“: Kurve einer früheren Skizze (nie der aktiven), Körperkante oder -ecke.
pub(crate) fn curve_source(state: &AppState, pick: IncludePick) -> Result<CurveSource, Msg> {
    refuse_active(state, &pick)?;
    if !matches!(
        pick,
        IncludePick::SketchCurve { .. }
            | IncludePick::Edge(..)
            | IncludePick::Vertex { .. }
            | IncludePick::ConstructionPoint(_)
    ) {
        return Err(crate::msg!("surface-pick-curves"));
    }
    match state.include_ref(pick)? {
        IncludeRef::SketchCurve { sketch, curve } => {
            existing_curve(state, sketch, curve)?;
            Ok(CurveSource::SketchCurve { sketch, curve })
        }
        IncludeRef::Edge(e) => Ok(CurveSource::Edge(e)),
        IncludeRef::Vertex(v) => Ok(CurveSource::Vertex(v)),
        _ => Err(crate::msg!("surface-pick-curves")),
    }
}

/// Feld „Richtung“: gerade Kante oder Skizzenlinie, Achse des Ursprungs, Normale einer Ebene oder
/// ebenen Fläche.
fn direction(state: &AppState, pick: IncludePick) -> Result<Direction, Msg> {
    let wrong = || crate::msg!("surface-pick-direction");
    let line_dir = |c: &Curve| match c {
        Curve::Line { dir, .. } => Some(*dir),
        _ => None,
    };
    let (vector, source) = match pick {
        IncludePick::Edge(b, e) => {
            let edge = state
                .bodies
                .get(b)
                .and_then(|body| body.solid.edges.get(e))
                .ok_or_else(|| crate::msg!("state-body-not-found"))?;
            let v = line_dir(&edge.curve).ok_or_else(wrong)?;
            let IncludeRef::Edge(r) = state.include_ref(pick)? else {
                return Err(wrong());
            };
            (v, DirectionSource::Edge(r))
        }
        IncludePick::SketchCurve { sketch, curve } => {
            refuse_active(state, &pick)?;
            state.check_source_sketch(sketch)?;
            let (c, ..) = state
                .document
                .sketch_of(sketch)
                .and_then(|sk| sk.curve_world(curve))
                .ok_or_else(wrong)?;
            let v = line_dir(&c).ok_or_else(wrong)?;
            (v, DirectionSource::SketchLine { sketch, curve })
        }
        IncludePick::Axis(a) => {
            let v = match a {
                StdAxis::X => DVec3::X,
                StdAxis::Y => DVec3::Y,
                StdAxis::Z => DVec3::Z,
            };
            (v, DirectionSource::Axis(a))
        }
        IncludePick::Plane(ref p) => {
            let p = p.clone();
            state.include_ref(pick)?;
            let frame = match p {
                PlaneId::Standard(sp) => sp.frame(),
                PlaneId::Feature(id) => state
                    .document
                    .plane_frame(id)
                    .ok_or_else(|| crate::msg!("state-plane-not-computed"))?,
            };
            (frame.z, DirectionSource::Plane(p))
        }
        IncludePick::Face(b, f) => {
            let frame = state
                .bodies
                .get(b)
                .and_then(|body| plane_of_face(&body.solid, f))
                .ok_or_else(wrong)?;
            let IncludeRef::FaceBoundary(r) = state.include_ref(pick)? else {
                return Err(wrong());
            };
            (frame.z, DirectionSource::Face(r))
        }
        _ => return Err(wrong()),
    };
    let vector = vector.normalize_or_zero();
    if !vector.is_finite() || vector == DVec3::ZERO {
        return Err(wrong());
    }
    Ok(Direction { vector, source })
}

/// Eine 2D-Kurve einer früheren Skizze (nie der aktiven): Nur sie lässt sich entlang der Normale
/// ihrer Skizze ausziehen (Fusion: „2D-Skizzenkurven“). 3D-Kurven werden mit Hinweis abgelehnt.
fn sketch_curve_2d(
    state: &AppState,
    pick: IncludePick,
    wrong: Msg,
) -> Result<(FeatureId, CurveId), Msg> {
    let IncludePick::SketchCurve { sketch, curve } = pick else {
        return Err(wrong);
    };
    refuse_active(state, &pick)?;
    state.check_source_sketch(sketch)?;
    existing_curve(state, sketch, curve)?;
    if state
        .document
        .sketch_of(sketch)
        .is_some_and(|sk| sk.is_3d_curve(curve))
    {
        return Err(crate::msg!("surface-curve-not-2d"));
    }
    Ok((sketch, curve))
}

// ---- Anzeige ------------------------------------------------------------------------------------

pub(crate) fn feature_name(state: &AppState, id: FeatureId) -> String {
    state
        .document
        .feature(id)
        .map_or_else(|| tr!("state-feature-n", n = id.0), |f| f.name.clone())
}

fn plane_label(state: &AppState, p: &PlaneId) -> String {
    match p {
        PlaneId::Standard(StandardPlane::XY) => tr!("tree-plane-xy"),
        PlaneId::Standard(StandardPlane::XZ) => tr!("tree-plane-xz"),
        PlaneId::Standard(StandardPlane::YZ) => tr!("tree-plane-yz"),
        PlaneId::Feature(id) => feature_name(state, *id),
    }
}

fn face_label(state: &AppState, r: &FaceRef) -> String {
    tr!(
        "plane-value-face",
        name = feature_name(state, FeatureId(r.origin))
    )
}

pub(crate) fn edge_label(state: &AppState, e: &EdgeRef) -> String {
    tr!(
        "plane-value-edge",
        name = feature_name(state, FeatureId(e.a.origin))
    )
}

fn sketch_curve_label(state: &AppState, sketch: FeatureId) -> String {
    tr!(
        "surface-value-sketch-curve",
        name = feature_name(state, sketch)
    )
}

fn face_target_label(state: &AppState, t: &FaceTarget) -> String {
    match t {
        FaceTarget::Face(r) => face_label(state, r),
        FaceTarget::Plane(p) => plane_label(state, p),
    }
}

pub(crate) fn curve_source_label(state: &AppState, c: &CurveSource) -> String {
    match c {
        CurveSource::SketchCurve { sketch, .. } => sketch_curve_label(state, *sketch),
        CurveSource::Edge(e) => edge_label(state, e),
        CurveSource::Vertex(v) => tr!(
            "plane-value-vertex",
            name = feature_name(state, FeatureId(v.edge.a.origin))
        ),
        CurveSource::ConstructionPoint(id) => feature_name(state, *id),
    }
}

fn target_label(state: &AppState, t: &IntersectTarget) -> String {
    match t {
        IntersectTarget::Face(r) => face_label(state, r),
        IntersectTarget::Plane(p) => plane_label(state, p),
        IntersectTarget::SketchCurve { sketch, .. } => sketch_curve_label(state, *sketch),
    }
}

fn direction_label(state: &AppState, d: &DirectionSource) -> String {
    match d {
        DirectionSource::Edge(e) => edge_label(state, e),
        DirectionSource::Face(r) => face_label(state, r),
        DirectionSource::Plane(p) => plane_label(state, p),
        DirectionSource::Axis(StdAxis::X) => tr!("plane-axis-x"),
        DirectionSource::Axis(StdAxis::Y) => tr!("plane-axis-y"),
        DirectionSource::Axis(StdAxis::Z) => tr!("plane-axis-z"),
        DirectionSource::SketchLine { sketch, .. } => tr!(
            "plane-value-sketch-line",
            name = feature_name(state, *sketch)
        ),
    }
}

/// Text eines Mehrfachfelds: „Auswählen“ oder die Zahl der gewählten Dinge.
pub(crate) fn count_text(n: usize) -> String {
    if n == 0 {
        tr!("surface-select")
    } else {
        tr!("surface-selected", count = n)
    }
}

/// Eine Zeile eines Wählfelds: Name, Feld (ein Klick macht es aktiv; Tooltip mit den gewählten
/// Dingen) und „Leeren“, wenn etwas gewählt ist. `true`, wenn „Leeren“ geklickt wurde.
fn pick_row(
    ui: &mut egui::Ui,
    focus: &mut Field,
    field: Field,
    label: String,
    text: String,
    items: &[String],
) -> bool {
    let mut clear = false;
    ui.label(label);
    ui.horizontal(|ui| {
        let r = ui.selectable_label(*focus == field, text);
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

/// Meldung, OK und Abbrechen; liefert (OK, Abbrechen).
pub(crate) fn footer(ui: &mut egui::Ui, error: Option<String>, ready: bool) -> (bool, bool) {
    if let Some(e) = error {
        ui.add_space(4.0);
        let dark = ui.visuals().dark_mode;
        ui.colored_label(error_color(dark), e);
    }
    ui.add_space(6.0);
    let (mut ok, mut cancel) = (false, false);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(ready, egui::Button::new(tr!("common-ok")))
            .clicked()
        {
            ok = true;
        }
        if ui.button(tr!("common-cancel")).clicked() {
            cancel = true;
        }
    });
    (ok, cancel)
}

/// Eingabe (übernehmen, wenn `ready`) und Esc (abbrechen), im Inhalt des Dialogs gelesen: im
/// Fenster, in dem der Dialog steht, solange dort kein Feld tippt. Abgedockt zählt auch Esc im
/// Hauptfenster ([`crate::dialog::escape_pressed`]), Enter des Hauptfensters nie.
pub(crate) fn keys(ui: &egui::Ui, ready: bool) -> (bool, bool) {
    let ctx = ui.ctx();
    let enter =
        ready && !crate::viewport::typing(ctx) && ctx.input(|i| i.key_pressed(egui::Key::Enter));
    (enter, crate::dialog::escape_pressed(ui, false))
}

/// Fehlermeldung nach einem abgelehnten OK: die Meldung der Sitzung, sonst der Text. Sie steht
/// danach nur im Dialog, nicht zusätzlich in der Skizzenpalette (Task H3).
pub(crate) fn session_error(state: &mut AppState, text: String) -> Msg {
    state
        .sketch_session
        .as_mut()
        .and_then(|se| se.message.take())
        .unwrap_or(Msg::Raw(text))
}

/// Abschluss eines Bilds: OK legt an (bei Erfolg zu), Abbrechen schließt ohne Spur.
pub(crate) fn outcome(
    state: &mut AppState,
    ok: bool,
    cancel: bool,
    open: bool,
    apply: impl FnOnce(&mut AppState) -> bool,
) -> DialogOutcome {
    if ok && apply(state) {
        finish(state);
        DialogOutcome::Applied
    } else if cancel || !open {
        finish(state);
        DialogOutcome::Cancelled
    } else {
        DialogOutcome::Open
    }
}

// ---- Auf Fläche projizieren ---------------------------------------------------------------------

impl Default for SurfaceProjectDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl SurfaceProjectDialog {
    /// Leerer Dialog: „Nächster Punkt“, das Feld „Flächen“ ist aktiv (wie Fusion).
    pub fn new() -> Self {
        Self {
            faces: Vec::new(),
            curves: Vec::new(),
            kind: ProjectKind::ClosestPoint,
            direction: None,
            focus: Field::Faces,
            error: None,
            picks: Picks::default(),
        }
    }

    /// Setzt die Projektionsart; „Entlang Vektor“ ohne Richtung macht das Feld „Richtung“ aktiv.
    pub fn set_kind(&mut self, kind: ProjectKind) {
        self.kind = kind;
        match kind {
            ProjectKind::AlongVector if self.direction.is_none() => self.focus = Field::Direction,
            ProjectKind::ClosestPoint if self.focus == Field::Direction => {
                self.focus = Field::Curves;
            }
            _ => {}
        }
    }

    /// Meldung des Dialogs in der aktuellen Sprache.
    pub fn error(&self) -> Option<String> {
        self.error.as_ref().map(Msg::text)
    }

    /// Nimmt einen Klick für das aktive Feld; ein unpassender lässt die Felder, wie sie sind, und
    /// hinterlässt eine Meldung. `true`, wenn er passte.
    pub fn take_pick(&mut self, state: &AppState, pick: IncludePick) -> bool {
        let focus = self.focus;
        let r = match focus {
            Field::Faces => face_target(state, pick.clone()).map(|f| {
                toggle(&mut self.faces, f);
                self.picks.toggle(focus, pick);
            }),
            Field::Curves => curve_source(state, pick.clone()).map(|c| {
                toggle(&mut self.curves, c);
                self.picks.toggle(focus, pick);
            }),
            Field::Direction => direction(state, pick.clone()).map(|d| {
                self.direction = Some(d);
                self.picks.set(focus, pick);
                if self.faces.is_empty() {
                    self.focus = Field::Faces;
                } else if self.curves.is_empty() {
                    self.focus = Field::Curves;
                }
            }),
            Field::Curve | Field::Targets => Err(crate::msg!("sketch-pick-project-to-surface")),
        };
        match r {
            Ok(()) => {
                self.error = None;
                true
            }
            Err(e) => {
                self.error = Some(e);
                false
            }
        }
    }

    /// Was die Felder gewählt haben (die Richtung nur bei „Entlang Vektor“), für die
    /// Hervorhebung in der Ansicht.
    pub fn marks(&self) -> Vec<IncludePick> {
        let along = self.kind == ProjectKind::AlongVector;
        self.picks.marks(|f| f != Field::Direction || along)
    }

    /// Sind alle nötigen Felder gefüllt (Flächen, Kurven und bei „Entlang Vektor“ die Richtung)?
    pub fn ok_enabled(&self) -> bool {
        self.spec().is_some()
    }

    /// Die Projektion aus den Feldern; `None`, solange etwas fehlt.
    pub fn spec(&self) -> Option<SurfaceProjection> {
        if self.faces.is_empty() || self.curves.is_empty() {
            return None;
        }
        let kind = match self.kind {
            ProjectKind::ClosestPoint => ProjectType::ClosestPoint,
            ProjectKind::AlongVector => ProjectType::AlongVector(self.direction.as_ref()?.vector),
        };
        Some(SurfaceProjection {
            sources: self.curves.clone(),
            faces: self.faces.clone(),
            kind,
        })
    }

    /// „OK“: legt die Kurven in die offene Skizze (ein Schritt); liefert die Zahl der Stücke. Bei
    /// einem Fehler bleibt die Skizze unverändert und die Meldung steht im Dialog.
    pub fn ok(&mut self, state: &mut AppState) -> Result<usize, String> {
        let Some(spec) = self.spec() else {
            let m = crate::msg!("sketch-pick-project-to-surface");
            let text = m.text();
            self.error = Some(m);
            return Err(text);
        };
        match state.surface_curves_into_sketch(CopySource::OnSurface(spec)) {
            Ok(n) => {
                self.error = None;
                Ok(n)
            }
            Err(text) => {
                self.error = Some(session_error(state, text.clone()));
                Err(text)
            }
        }
    }

    /// Zeigt den Dialog.
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> DialogOutcome {
        let mut open = true;
        let (mut ok, mut cancel) = (false, false);
        let mut kind = self.kind;
        let faces: Vec<String> = self
            .faces
            .iter()
            .map(|f| face_target_label(state, f))
            .collect();
        let curves: Vec<String> = self
            .curves
            .iter()
            .map(|c| curve_source_label(state, c))
            .collect();
        let dir = self
            .direction
            .as_ref()
            .map(|d| direction_label(state, &d.source));
        let ready = self.ok_enabled();
        Dialog::new(tr!("action-sketch-project-to-surface"), DialogSize::Normal)
            .dock_key("sketch_project_to_surface")
            .id(egui::Id::new("surface_project_dialog"))
            .open(&mut open)
            .show(ctx, |ui| {
                egui::Grid::new("surface_project_fields")
                    .num_columns(2)
                    .show(ui, |ui| {
                        if pick_row(
                            ui,
                            &mut self.focus,
                            Field::Faces,
                            tr!("surface-field-faces"),
                            count_text(faces.len()),
                            &faces,
                        ) {
                            self.faces.clear();
                            self.picks.clear(Field::Faces);
                        }
                        if pick_row(
                            ui,
                            &mut self.focus,
                            Field::Curves,
                            tr!("surface-field-curves"),
                            count_text(curves.len()),
                            &curves,
                        ) {
                            self.curves.clear();
                            self.picks.clear(Field::Curves);
                        }
                        ui.label(tr!("surface-field-project-type"));
                        ui.horizontal_wrapped(|ui| {
                            ui.radio_value(
                                &mut kind,
                                ProjectKind::ClosestPoint,
                                tr!("surface-kind-closest"),
                            );
                            ui.radio_value(
                                &mut kind,
                                ProjectKind::AlongVector,
                                tr!("surface-kind-along"),
                            );
                        });
                        ui.end_row();
                        if self.kind == ProjectKind::AlongVector {
                            let items: Vec<String> = dir.iter().cloned().collect();
                            let text = dir.clone().unwrap_or_else(|| tr!("surface-select"));
                            if pick_row(
                                ui,
                                &mut self.focus,
                                Field::Direction,
                                tr!("surface-field-direction"),
                                text,
                                &items,
                            ) {
                                self.direction = None;
                                self.picks.clear(Field::Direction);
                            }
                        }
                    });
                // Abgedockt läuft der Inhalt evtl. mehrmals je Bild: nichts überschreiben.
                let (o, c) = footer(ui, self.error(), ready);
                let (enter, esc) = keys(ui, ready);
                ok |= o || enter;
                cancel |= c || esc;
            });
        if kind != self.kind {
            self.set_kind(kind);
        }
        outcome(state, ok, cancel, open, |state| self.ok(state).is_ok())
    }
}

// ---- Schnittkurve ---------------------------------------------------------------------------------

impl Default for IntersectionCurveDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl IntersectionCurveDialog {
    /// Leerer Dialog; das Feld „Kurve“ ist aktiv.
    pub fn new() -> Self {
        Self {
            curve: None,
            targets: Vec::new(),
            focus: Field::Curve,
            error: None,
            picks: Picks::default(),
        }
    }

    /// Meldung des Dialogs in der aktuellen Sprache.
    pub fn error(&self) -> Option<String> {
        self.error.as_ref().map(Msg::text)
    }

    /// Nimmt einen Klick für das aktive Feld; `true`, wenn er passte. Eine gewählte Kurve macht
    /// „Kurven/Flächen“ aktiv.
    pub fn take_pick(&mut self, state: &AppState, pick: IncludePick) -> bool {
        let clicked = pick.clone();
        let r = match self.focus {
            Field::Curve => sketch_curve_2d(state, pick, crate::msg!("surface-pick-curve"))
                .and_then(|(sketch, curve)| {
                    // Spec 7: Die Ziele sind Kurven *anderer* Skizzen.
                    let clash = self.targets.iter().any(
                        |t| matches!(t, IntersectTarget::SketchCurve { sketch: s, .. } if *s == sketch),
                    );
                    if clash {
                        return Err(crate::msg!("surface-intersect-same-sketch"));
                    }
                    self.curve = Some(CurveSource::SketchCurve { sketch, curve });
                    self.picks.set(Field::Curve, clicked);
                    self.focus = Field::Targets;
                    Ok(())
                }),
            Field::Targets => self.target(state, pick).map(|t| {
                toggle(&mut self.targets, t);
                self.picks.toggle(Field::Targets, clicked);
            }),
            _ => Err(crate::msg!("sketch-pick-intersection-curve")),
        };
        match r {
            Ok(()) => {
                self.error = None;
                true
            }
            Err(e) => {
                self.error = Some(e);
                false
            }
        }
    }

    /// Feld „Kurven/Flächen“: Körperfläche, Ebene oder 2D-Kurve einer anderen früheren Skizze.
    fn target(&self, state: &AppState, pick: IncludePick) -> Result<IntersectTarget, Msg> {
        let wrong = || crate::msg!("surface-pick-targets");
        match pick {
            IncludePick::SketchCurve { .. } => {
                let (sketch, curve) = sketch_curve_2d(state, pick, wrong())?;
                // Spec 7: nur Kurven einer anderen Skizze als die „Kurve“ (deren Auszüge laufen
                // sonst parallel und schneiden sich nie).
                if matches!(&self.curve, Some(CurveSource::SketchCurve { sketch: s, .. }) if *s == sketch)
                {
                    return Err(crate::msg!("surface-intersect-same-sketch"));
                }
                Ok(IntersectTarget::SketchCurve { sketch, curve })
            }
            IncludePick::Face(..) | IncludePick::Plane(_) => match state.include_ref(pick)? {
                IncludeRef::FaceBoundary(r) => Ok(IntersectTarget::Face(r)),
                IncludeRef::Plane(p) => Ok(IntersectTarget::Plane(p)),
                _ => Err(wrong()),
            },
            _ => Err(wrong()),
        }
    }

    /// Was die Felder gewählt haben, für die Hervorhebung in der Ansicht.
    pub fn marks(&self) -> Vec<IncludePick> {
        self.picks.marks(|_| true)
    }

    /// Sind „Kurve“ und „Kurven/Flächen“ gefüllt?
    pub fn ok_enabled(&self) -> bool {
        self.spec().is_some()
    }

    /// Die Schnittkurve aus den Feldern; `None`, solange etwas fehlt.
    pub fn spec(&self) -> Option<IntersectionSpec> {
        let curve = self.curve.clone()?;
        if self.targets.is_empty() {
            return None;
        }
        Some(IntersectionSpec {
            curves: vec![curve],
            targets: self.targets.clone(),
        })
    }

    /// „OK“: legt die Schnittkurve in die offene Skizze (ein Schritt), wie
    /// [`SurfaceProjectDialog::ok`].
    pub fn ok(&mut self, state: &mut AppState) -> Result<usize, String> {
        let Some(spec) = self.spec() else {
            let m = crate::msg!("sketch-pick-intersection-curve");
            let text = m.text();
            self.error = Some(m);
            return Err(text);
        };
        match state.surface_curves_into_sketch(CopySource::Intersection(spec)) {
            Ok(n) => {
                self.error = None;
                Ok(n)
            }
            Err(text) => {
                self.error = Some(session_error(state, text.clone()));
                Err(text)
            }
        }
    }

    /// Zeigt den Dialog.
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> DialogOutcome {
        let mut open = true;
        let (mut ok, mut cancel) = (false, false);
        let curve: Vec<String> = self
            .curve
            .iter()
            .map(|c| curve_source_label(state, c))
            .collect();
        let targets: Vec<String> = self
            .targets
            .iter()
            .map(|t| target_label(state, t))
            .collect();
        let ready = self.ok_enabled();
        Dialog::new(tr!("action-sketch-intersection-curve"), DialogSize::Normal)
            .dock_key("sketch_intersection_curve")
            .id(egui::Id::new("intersection_curve_dialog"))
            .open(&mut open)
            .show(ctx, |ui| {
                egui::Grid::new("intersection_curve_fields")
                    .num_columns(2)
                    .show(ui, |ui| {
                        let text = curve
                            .first()
                            .cloned()
                            .unwrap_or_else(|| tr!("surface-select"));
                        if pick_row(
                            ui,
                            &mut self.focus,
                            Field::Curve,
                            tr!("surface-field-curve"),
                            text,
                            &curve,
                        ) {
                            self.curve = None;
                            self.picks.clear(Field::Curve);
                        }
                        if pick_row(
                            ui,
                            &mut self.focus,
                            Field::Targets,
                            tr!("surface-field-targets"),
                            count_text(targets.len()),
                            &targets,
                        ) {
                            self.targets.clear();
                            self.picks.clear(Field::Targets);
                        }
                    });
                // Abgedockt läuft der Inhalt evtl. mehrmals je Bild: nichts überschreiben.
                let (o, c) = footer(ui, self.error(), ready);
                let (enter, esc) = keys(ui, ready);
                ok |= o || enter;
                cancel |= c || esc;
            });
        outcome(state, ok, cancel, open, |state| self.ok(state).is_ok())
    }
}
