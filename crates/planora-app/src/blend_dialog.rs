//! Fenster „Verrunden“ bzw. „Fase“ (Menü „Bearbeiten“, Spec §6): Kante im Fenster anklicken,
//! Maße eintragen, Art wählen, „Erzeugen“. Zahlen nehmen Komma und Punkt an.

use crate::browser::error_color;
use crate::dialog::{Dialog, DialogSize};
use crate::formula_field::{FieldInput, FormulaField, classify, names_hint};
use crate::sketch_view::{format_number, parse_number};
use crate::state::{AppState, MAX_COORD_MM};
use crate::theme;
use planora_feature::{
    BlendArt, FieldKey, Formulas, MAX_CHAMFER_ANGLE_DEG, MIN_CHAMFER_ANGLE_DEG, ParamScope,
    ParamUnit,
};
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::blend::{MAX_RADIUS_POINTS, min_blend_size_request};
use planora_kernel::{BlendKind, Edge, Id, Solid};
use planora_math::Tolerance;
use planora_render::{EdgeRef, Pick};

/// Meldung, solange keine Kante angeklickt ist.
pub const NO_EDGE: &str = "Bitte zuerst eine Kante im Fenster anklicken.";

/// Vorgabe des zweiten Abstands bei „Fase: zwei Abstände“ in mm (wie der erste).
const DEFAULT_SECOND_DISTANCE: &str = "1";
/// Vorgabe des Fasenwinkels bei „Fase: Abstand und Winkel“ in Grad.
const DEFAULT_ANGLE: &str = "45";
/// Dasselbe als Zahl (Umwandlung einer bestehenden Fase).
const DEFAULT_ANGLE_DEG: f64 = 45.0;

/// Art im Fenster.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtChoice {
    Rundung,
    RundungVariabel,
    FaseGleich,
    FaseZwei,
    FaseWinkel,
}

impl ArtChoice {
    pub fn label(self) -> &'static str {
        match self {
            ArtChoice::Rundung => "Rundung",
            ArtChoice::RundungVariabel => "Rundung mit variablem Radius",
            ArtChoice::FaseGleich => "Fase: gleicher Abstand",
            ArtChoice::FaseZwei => "Fase: zwei Abstände",
            ArtChoice::FaseWinkel => "Fase: Abstand und Winkel",
        }
    }

    /// Bezeichnungen der Maßfelder (das zweite fehlt bei Rundung und gleicher Fase).
    pub fn fields(self) -> (&'static str, Option<&'static str>) {
        match self {
            ArtChoice::Rundung | ArtChoice::RundungVariabel => ("Radius", None),
            ArtChoice::FaseGleich => ("Abstand", None),
            ArtChoice::FaseZwei => ("Abstand 1", Some("Abstand 2")),
            ArtChoice::FaseWinkel => ("Abstand", Some("Winkel")),
        }
    }

    /// Formelfelder zu [`ArtChoice::fields`] (die variable Rundung nimmt keine Formeln, ihr
    /// Eintrag wird nicht benutzt).
    pub fn keys(self) -> (FieldKey, Option<FieldKey>) {
        match self {
            ArtChoice::Rundung | ArtChoice::RundungVariabel => (FieldKey::Radius, None),
            ArtChoice::FaseGleich => (FieldKey::Abstand, None),
            ArtChoice::FaseZwei => (FieldKey::Abstand1, Some(FieldKey::Abstand2)),
            ArtChoice::FaseWinkel => (FieldKey::Abstand, Some(FieldKey::Winkel)),
        }
    }

    /// Art einer bestehenden Verrundung.
    pub fn of(art: &BlendArt) -> ArtChoice {
        match art {
            BlendArt::Rundung { .. } => ArtChoice::Rundung,
            BlendArt::RundungVariabel { .. } => ArtChoice::RundungVariabel,
            BlendArt::FaseGleich { .. } => ArtChoice::FaseGleich,
            BlendArt::FaseZwei { .. } => ArtChoice::FaseZwei,
            BlendArt::FaseWinkel { .. } => ArtChoice::FaseWinkel,
        }
    }

    /// `art` in diese Art umwandeln: das erste Maß bleibt, fehlende Werte bekommen Vorgaben.
    pub fn convert(self, art: &BlendArt) -> BlendArt {
        let first = match art {
            BlendArt::Rundung { radius } => *radius,
            BlendArt::RundungVariabel { punkte } => punkte.first().map_or(1.0, |p| p.1),
            BlendArt::FaseGleich { abstand } | BlendArt::FaseWinkel { abstand, .. } => *abstand,
            BlendArt::FaseZwei { abstand1, .. } => *abstand1,
        };
        match self {
            ArtChoice::Rundung => BlendArt::Rundung { radius: first },
            ArtChoice::RundungVariabel => BlendArt::RundungVariabel {
                punkte: vec![(0.0, first), (100.0, first)],
            },
            ArtChoice::FaseGleich => BlendArt::FaseGleich { abstand: first },
            ArtChoice::FaseZwei => BlendArt::FaseZwei {
                abstand1: first,
                abstand2: first,
            },
            ArtChoice::FaseWinkel => BlendArt::FaseWinkel {
                abstand: first,
                winkel: DEFAULT_ANGLE_DEG,
            },
        }
    }

    /// Hängt das Ergebnis von der Wahl der Fläche 1 ab?
    pub fn uses_first_face(self) -> bool {
        matches!(self, ArtChoice::FaseZwei | ArtChoice::FaseWinkel)
    }
}

/// Meldung, wenn ein variabler Radius schon die höchste Zahl Punkte hat.
pub const TOO_MANY_POINTS: &str = "Ein variabler Radius hat höchstens 64 Punkte";

/// Neuer Punkt in der Mitte des größten Abstands (Position, Radius als Mittel der Nachbarn).
/// Bei schon [`MAX_RADIUS_POINTS`] Punkten ohne Wirkung mit Meldung [`TOO_MANY_POINTS`].
pub fn insert_point(points: &mut Vec<(f64, f64)>) -> Result<(), String> {
    if points.len() >= MAX_RADIUS_POINTS {
        return Err(TOO_MANY_POINTS.into());
    }
    if points.len() < 2 {
        return Ok(());
    }
    let k = (0..points.len() - 1)
        .max_by(|&a, &b| {
            (points[a + 1].0 - points[a].0).total_cmp(&(points[b + 1].0 - points[b].0))
        })
        .unwrap_or(0);
    let (p, q) = (points[k], points[k + 1]);
    points.insert(k + 1, (0.5 * (p.0 + q.0), 0.5 * (p.1 + q.1)));
    Ok(())
}

/// Kleinstes Maß in mm, das der Kern für diesen Körper und diese Art zulässt.
pub fn min_size_for(solid: &Solid, variable: bool, tol: &Tolerance) -> f64 {
    let kind = if variable {
        BlendKind::VariableFillet { points: Vec::new() }
    } else {
        BlendKind::Fillet { radius: 1.0 }
    };
    min_blend_size_request(solid, &kind, tol)
}

/// Zahl für Meldungen: Komma, ohne überflüssige Nullen.
pub fn min_text(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.replace('.', ",")
}

/// Prüft die Maße (Radien, Abstände) gegen das Mindestmaß des Kerns.
pub fn check_min_sizes(art: &BlendArt, min: f64) -> Result<(), String> {
    let sizes: Vec<(String, f64)> = match art {
        BlendArt::Rundung { radius } => vec![("Radius".into(), *radius)],
        BlendArt::RundungVariabel { punkte } => punkte
            .iter()
            .enumerate()
            .map(|(k, p)| (format!("Radius {}", k + 1), p.1))
            .collect(),
        BlendArt::FaseGleich { abstand } | BlendArt::FaseWinkel { abstand, .. } => {
            vec![("Abstand".into(), *abstand)]
        }
        BlendArt::FaseZwei { abstand1, abstand2 } => vec![
            ("Abstand 1".into(), *abstand1),
            ("Abstand 2".into(), *abstand2),
        ],
    };
    match sizes.into_iter().find(|(_, v)| *v < min) {
        Some((label, _)) => Err(format!(
            "„{label}“ ist zu klein; das kleinste mögliche Maß ist {} mm",
            min_text(min)
        )),
        None => Ok(()),
    }
}

/// Namen der beiden Flächen an der Kante `edge` (die erste ist die Vorgabe für Fläche 1).
pub fn face_pair(solid: &Solid, edge: Id<Edge>) -> Option<(String, String)> {
    let uses = edge_uses(solid, edge);
    (uses.len() >= 2).then(|| {
        (
            face_name(solid, uses[0].face),
            face_name(solid, uses[1].face),
        )
    })
}

/// Macht an allen Kanten die Fläche mit der Rolle `role` zu Fläche 1 (Kanten ohne diese Rolle
/// bleiben unverändert).
pub fn set_first_face(refs: &mut [planora_feature::EdgeRef], role: &str) {
    for r in refs {
        if r.a.role != role && r.b.role == role {
            std::mem::swap(&mut r.a, &mut r.b);
        }
    }
}

/// Offenes Fenster „Verrunden“ oder „Fase“.
#[derive(Clone, Debug)]
pub struct BlendDialog {
    pub art: ArtChoice,
    pub first: String,
    pub second: String,
    pub tangentenkette: bool,
    /// Punkte der variablen Rundung: Position in % und Radius.
    pub points: Vec<(String, String)>,
    /// Rolle der gewählten Fläche 1 (nur bei „zwei Abstände“ und „Abstand und Winkel“);
    /// `None`: die erste Fläche der Kante.
    pub first_face: Option<String>,
    /// Die beim Öffnen angeklickte Kante ist schon in die Markierung übernommen.
    seeded: bool,
    error: Option<String>,
}

/// Zulässiges Maß: größer als null, höchstens `MAX_COORD_MM`.
fn size_ok(v: f64) -> bool {
    v > 0.0 && v <= MAX_COORD_MM
}

/// Zulässiger Fasenwinkel in Grad (Grenzen wie im Dokument und im Kernel eingeschlossen).
fn angle_ok(w: f64) -> bool {
    (MIN_CHAMFER_ANGLE_DEG..=MAX_CHAMFER_ANGLE_DEG).contains(&w)
}

fn size_message(label: &str, text: &str) -> String {
    format!("„{label}“ muss eine Zahl größer als null sein (eingegeben: „{text}“)")
}

/// Meldung bei einem unzulässigen Fasenwinkel.
pub fn angle_message(text: &str) -> String {
    format!(
        "„Winkel“ muss zwischen {MIN_CHAMFER_ANGLE_DEG:.0}° und {MAX_CHAMFER_ANGLE_DEG:.0}° liegen (eingegeben: „{text}“)"
    )
}

/// Liest ein Maß (nur Zahlen).
fn size(label: &str, text: &str) -> Result<f64, String> {
    match parse_number(text) {
        Some(v) if size_ok(v) => Ok(v),
        _ => Err(size_message(label, text)),
    }
}

/// Liest ein Feld: Zahl oder Formel (in `unit`, mit den Parametern `scope`), geprüft mit `ok`;
/// ein unzulässiger Wert ergibt `bad()`, eine fehlerhafte Formel „{label}: {Meldung}“. Ohne
/// Parameter gilt ein Text, der sich nicht auswerten lässt, wie bisher als ungültige Zahl
/// (`bad()`, etwa bei „nan“).
fn read_field(
    label: &str,
    text: &str,
    unit: ParamUnit,
    scope: &ParamScope,
    ok: fn(f64) -> bool,
    bad: impl Fn() -> String,
) -> Result<(f64, Option<String>), String> {
    if let Some(FieldInput::Formula(f)) = classify(text) {
        match scope.eval(&f, unit) {
            Ok(v) if ok(v) => return Ok((v, Some(f))),
            Ok(_) => return Err(bad()),
            Err(e) if !scope.names().is_empty() => return Err(format!("{label}: {e}")),
            Err(_) => return Err(bad()),
        }
    }
    match parse_number(text) {
        Some(v) if ok(v) => Ok((v, None)),
        _ => Err(bad()),
    }
}

impl BlendDialog {
    pub fn fillet() -> Self {
        BlendDialog {
            art: ArtChoice::Rundung,
            first: "1".into(),
            second: String::new(),
            tangentenkette: true,
            points: Vec::new(),
            first_face: None,
            seeded: false,
            error: None,
        }
    }

    pub fn chamfer() -> Self {
        BlendDialog {
            art: ArtChoice::FaseGleich,
            first: "1".into(),
            second: DEFAULT_ANGLE.into(),
            tangentenkette: true,
            points: Vec::new(),
            first_face: None,
            seeded: false,
            error: None,
        }
    }

    pub fn variable() -> Self {
        BlendDialog {
            art: ArtChoice::RundungVariabel,
            points: vec![("0".into(), "1".into()), ("100".into(), "1".into())],
            ..BlendDialog::fillet()
        }
    }

    /// Punkt in der Mitte des größten Abstands einfügen (ungültige Felder: ans Ende, 100 %).
    pub fn add_point(&mut self) {
        if self.points.len() >= MAX_RADIUS_POINTS {
            self.error = Some(TOO_MANY_POINTS.into());
            return;
        }
        let parsed: Option<Vec<(f64, f64)>> = self
            .points
            .iter()
            .map(|(p, r)| Some((parse_number(p)?, parse_number(r)?)))
            .collect();
        match parsed {
            Some(mut pts) if pts.len() >= 2 => {
                if let Err(e) = insert_point(&mut pts) {
                    self.error = Some(e);
                    return;
                }
                self.points = pts
                    .iter()
                    .map(|(p, r)| (format_number(*p, 2), format_number(*r, 3)))
                    .collect();
            }
            _ => self.points.push(("100".into(), "1".into())),
        }
    }

    /// Punkt `i` entfernen; mindestens zwei bleiben.
    pub fn remove_point(&mut self, i: usize) {
        if self.points.len() > 2 && i < self.points.len() {
            self.points.remove(i);
            if self.error.as_deref() == Some(TOO_MANY_POINTS) {
                self.error = None;
            }
        }
    }

    pub fn title(&self) -> &'static str {
        if matches!(self.art, ArtChoice::Rundung | ArtChoice::RundungVariabel) {
            "Verrunden"
        } else {
            "Fase"
        }
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Wechselt die Art. Das zweite Maß bekommt dabei die Vorgabe der neuen Art (sonst stünde
    /// nach „Abstand und Winkel“ bei „zwei Abstände“ noch 45 mm).
    pub fn set_art(&mut self, art: ArtChoice) {
        if self.art == art {
            return;
        }
        // Das erste Maß bleibt: beim Wechsel von „variabel“ der Radius des ersten Punkts,
        // beim Wechsel zu „variabel“ zwei Punkte mit diesem Radius.
        if self.art == ArtChoice::RundungVariabel
            && let Some((_, r)) = self.points.first()
        {
            self.first = r.clone();
        }
        self.art = art;
        self.second = match art {
            ArtChoice::FaseZwei => DEFAULT_SECOND_DISTANCE.into(),
            ArtChoice::FaseWinkel => DEFAULT_ANGLE.into(),
            ArtChoice::Rundung | ArtChoice::RundungVariabel | ArtChoice::FaseGleich => {
                String::new()
            }
        };
        if art == ArtChoice::RundungVariabel {
            self.points = vec![
                ("0".into(), self.first.clone()),
                ("100".into(), self.first.clone()),
            ];
        }
        if !art.uses_first_face() {
            self.first_face = None;
        }
    }

    /// Gewählte Kanten: die markierten (alle aus einem Körper), sonst die angeklickte Kante.
    pub fn chosen_edges(&self, state: &AppState) -> Option<(usize, Vec<Id<Edge>>)> {
        if let Some(first) = state.marked_edges.first() {
            let edges = state
                .marked_edges
                .iter()
                .filter(|m| m.body == first.body)
                .map(|m| m.edge)
                .collect();
            return Some((first.body, edges));
        }
        match state.selection {
            Some(Pick::Edge(EdgeRef { body, edge })) => Some((body, vec![edge])),
            _ => None,
        }
    }

    /// Kleinstes Maß des Kerns für den gewählten Körper und die gewählte Art.
    pub fn min_size(&self, state: &AppState) -> Option<f64> {
        let (body, _) = self.chosen_edges(state)?;
        let solid = &state.bodies.get(body)?.solid;
        Some(min_size_for(
            solid,
            self.art == ArtChoice::RundungVariabel,
            &state.kernel.tolerance,
        ))
    }

    /// Namen der beiden Flächen an der ersten gewählten Kante (Fläche 1 zur Wahl).
    pub fn faces(&self, state: &AppState) -> Option<(String, String)> {
        let (body, edges) = self.chosen_edges(state)?;
        face_pair(&state.bodies.get(body)?.solid, *edges.first()?)
    }

    /// Beendet das Markieren (Fenster geschlossen).
    pub fn finish(&self, state: &mut AppState) {
        state.edge_picking = false;
        state.clear_edge_marks();
    }

    /// Meldung, wenn die Markierung Kanten mehrerer Körper enthält (Körper mit Namen).
    fn mixed_bodies(state: &AppState) -> Option<String> {
        let first = state.marked_edges.first()?.body;
        if state.marked_edges.iter().all(|m| m.body == first) {
            return None;
        }
        let mut names: Vec<String> = Vec::new();
        for m in &state.marked_edges {
            let name = state
                .bodies
                .get(m.body)
                .map_or_else(|| format!("Körper {}", m.body + 1), |b| b.name.clone());
            if !names.contains(&name) {
                names.push(name);
            }
        }
        Some(format!(
            "Die Kanten gehören zu verschiedenen Körpern ({}); es geht nur ein Körper auf einmal",
            names.join(", ")
        ))
    }

    /// Art mit den eingegebenen Maßen (nur Zahlen, ohne Parameter); ein Fehlertext nennt das
    /// ungültige Feld.
    pub fn art_value(&self) -> Result<BlendArt, String> {
        self.art_formulas(&ParamScope::default())
            .map(|(art, _)| art)
    }

    /// Art mit den eingegebenen Maßen und die Formeln der Felder (Formeln mit den Parametern
    /// `scope` ausgewertet; die Punkte der variablen Rundung nehmen nur Zahlen).
    pub fn art_formulas(&self, scope: &ParamScope) -> Result<(BlendArt, Formulas), String> {
        let mut formulas = Formulas::new();
        if self.art == ArtChoice::RundungVariabel {
            let mut punkte = Vec::with_capacity(self.points.len());
            for (k, (p, r)) in self.points.iter().enumerate() {
                let pos = parse_number(p)
                    .filter(|v| (0.0..=100.0).contains(v))
                    .ok_or_else(|| {
                        format!("„Position {}“ muss zwischen 0 und 100 % liegen", k + 1)
                    })?;
                punkte.push((pos, size(&format!("Radius {}", k + 1), r)?));
            }
            return Ok((BlendArt::RundungVariabel { punkte }, formulas));
        }
        let (l1, l2) = self.art.fields();
        let (k1, k2) = self.art.keys();
        let mut read = |label: &str, key: FieldKey, text: &str| -> Result<f64, String> {
            let (v, formula) = if key == FieldKey::Winkel {
                read_field(label, text, ParamUnit::Angle, scope, angle_ok, || {
                    angle_message(text)
                })?
            } else {
                read_field(label, text, ParamUnit::Length, scope, size_ok, || {
                    size_message(label, text)
                })?
            };
            if let Some(f) = formula {
                formulas.insert(key, f);
            }
            Ok(v)
        };
        let a = read(l1, k1, &self.first)?;
        let second = match (l2, k2) {
            (Some(l2), Some(k2)) => Some(read(l2, k2, &self.second)?),
            _ => None,
        };
        let art = match (self.art, second) {
            (ArtChoice::FaseZwei, Some(b)) => BlendArt::FaseZwei {
                abstand1: a,
                abstand2: b,
            },
            (ArtChoice::FaseWinkel, Some(w)) => BlendArt::FaseWinkel {
                abstand: a,
                winkel: w,
            },
            (ArtChoice::FaseGleich, _) => BlendArt::FaseGleich { abstand: a },
            _ => BlendArt::Rundung { radius: a },
        };
        Ok((art, formulas))
    }

    /// „Erzeugen“: `true`, wenn das Fenster offen bleiben soll (Fehler stehen dann im Fenster
    /// und in der Statuszeile).
    pub fn execute(&mut self, state: &mut AppState) -> bool {
        state.message = None;
        if let Some(e) = Self::mixed_bodies(state) {
            state.message = Some(e.clone());
            self.error = Some(e);
            return true;
        }
        let Some((body, edges)) = self.chosen_edges(state) else {
            self.error = Some(NO_EDGE.into());
            state.message = Some(NO_EDGE.into());
            return true;
        };
        let scope = state.document.param_scope();
        let (art, formulas) = match self.art_formulas(&scope).and_then(|(a, f)| {
            let min = self.min_size(state).unwrap_or(0.0);
            check_min_sizes(&a, min).map(|()| (a, f))
        }) {
            Ok(a) => a,
            Err(e) => {
                state.message = Some(e.clone());
                self.error = Some(e);
                return true;
            }
        };
        let first_face = self
            .first_face
            .as_deref()
            .filter(|_| self.art.uses_first_face());
        match state.add_blend_with(body, &edges, art, self.tangentenkette, first_face, formulas) {
            Ok(_) => {
                self.error = None;
                false
            }
            Err(e) => {
                self.error = Some(e);
                true
            }
        }
    }

    /// Zeigt das Fenster; `true`, solange es offen bleiben soll.
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> bool {
        state.edge_picking = true;
        state.set_chain_marking(self.tangentenkette);
        if !self.seeded {
            self.seeded = true;
            if let Some(Pick::Edge(e)) = state.selection {
                state.toggle_edge_mark(e, false);
            }
        }
        let mut open = true;
        let (mut run, mut cancel) = (false, false);
        let scope = state.document.param_scope();
        let mut errors: Vec<String> = Vec::new();
        Dialog::new(self.title(), DialogSize::Normal)
            .open(&mut open)
            .show(ctx, |ui| {
                match self.chosen_edges(state) {
                    Some((_, edges)) => ui.label(format!("Gewählte Kanten: {}", edges.len())),
                    None => ui.weak("Kante im Fenster anklicken (Umschalt: mehrere)."),
                };
                ui.add_space(4.0);
                let choices: &[ArtChoice] =
                    if matches!(self.art, ArtChoice::Rundung | ArtChoice::RundungVariabel) {
                        &[ArtChoice::Rundung, ArtChoice::RundungVariabel]
                    } else {
                        &[
                            ArtChoice::FaseGleich,
                            ArtChoice::FaseZwei,
                            ArtChoice::FaseWinkel,
                        ]
                    };
                let mut chosen = self.art;
                for c in choices {
                    ui.radio_value(&mut chosen, *c, c.label());
                }
                self.set_art(chosen);
                if self.art == ArtChoice::RundungVariabel {
                    if self.points.is_empty() {
                        self.points = vec![
                            ("0".into(), self.first.clone()),
                            ("100".into(), self.first.clone()),
                        ];
                    }
                    let mut remove = None;
                    egui::Grid::new("blend_points")
                        .num_columns(5)
                        .show(ui, |ui| {
                            for (k, (p, r)) in self.points.iter_mut().enumerate() {
                                ui.label(format!("Punkt {}", k + 1));
                                theme::text_field(
                                    ui,
                                    egui::TextEdit::singleline(p).desired_width(60.0),
                                );
                                ui.label("%");
                                theme::text_field(
                                    ui,
                                    egui::TextEdit::singleline(r).desired_width(60.0),
                                );
                                if ui.button("Entfernen").clicked() {
                                    remove = Some(k);
                                }
                                ui.end_row();
                            }
                        });
                    if let Some(k) = remove {
                        self.remove_point(k);
                    }
                    if ui.button("Punkt hinzufügen").clicked() {
                        self.add_point();
                    }
                } else {
                    let (l1, l2) = self.art.fields();
                    let (k1, k2) = self.art.keys();
                    let unit = |k: FieldKey| {
                        if k == FieldKey::Winkel {
                            ParamUnit::Angle
                        } else {
                            ParamUnit::Length
                        }
                    };
                    egui::Grid::new("blend_numbers")
                        .num_columns(2)
                        .show(ui, |ui| {
                            let rows = [
                                Some((l1, k1, &mut self.first)),
                                l2.zip(k2).map(|(l, k)| (l, k, &mut self.second)),
                            ];
                            for (label, key, text) in rows.into_iter().flatten() {
                                ui.label(label);
                                let out = FormulaField::new(("blend-field", key), text, unit(key))
                                    .error_below(false)
                                    .label(label)
                                    .show(ui, &scope);
                                if let Some(e) = out.error {
                                    errors.push(format!("{label}: {e}"));
                                }
                                ui.end_row();
                            }
                        });
                    if let Some(hint) = names_hint(&scope) {
                        ui.weak(hint);
                    }
                }
                if self.art.uses_first_face()
                    && let Some((a, b)) = self.faces(state)
                {
                    let current = self.first_face.clone().unwrap_or_else(|| a.clone());
                    egui::ComboBox::from_label("Fläche 1")
                        .selected_text(current.clone())
                        .show_ui(ui, |ui| {
                            for name in [&a, &b] {
                                if ui.selectable_label(*name == current, name).clicked() {
                                    self.first_face = Some(name.clone());
                                }
                            }
                        });
                }
                if let Some(min) = self.min_size(state) {
                    ui.weak(format!("Kleinstes Maß: {} mm", min_text(min)));
                }
                ui.checkbox(
                    &mut self.tangentenkette,
                    "Tangentenkette (glatt anschließende Kanten mitnehmen)",
                );
                let dark = ui.visuals().dark_mode;
                for e in errors.iter().chain(&self.error) {
                    ui.colored_label(error_color(dark), e);
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(errors.is_empty(), egui::Button::new("Erzeugen"))
                        .clicked()
                    {
                        run = true;
                    }
                    if ui.button("Abbrechen").clicked() {
                        cancel = true;
                    }
                });
            });
        let keep = if run {
            self.execute(state)
        } else {
            open && !cancel
        };
        if !keep {
            self.finish(state);
        }
        keep
    }
}
