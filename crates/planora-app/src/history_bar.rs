//! Verlaufsleiste: die Schritte des gewählten Körpers mit einem Marker je Körper. Hier steht
//! das Modell ohne Oberfläche (Schritte, Markerstelle, Bewegen, Farbe, Sperre); es ist ohne
//! Fenster testbar. Der globale Marker des Feature-Baums (`browser::roll_back_to`) bleibt
//! daneben unverändert.
//!
//! Der Marker eines Körpers steht hinter dem letzten aktiven Schritt von vorn: Position `p`
//! heißt, die ersten `p` Schritte der Kette sind aktiv, der Schritt an `p` trägt die
//! Rückrollstelle. `p == Anzahl der Schritte` heißt, der Körper hat keine Stelle.
//!
//! Darunter die Oberfläche ([`show_with`]): ein schwebendes Feld zwischen den Spalten über dem
//! Befehlsfeld (Lage aus [`crate::shell::layout_in_mode`]) mit Abspielknöpfen, einem Symbol je
//! Schritt in der Körperfarbe, dem Marker als senkrechter Linie mit Griff und rechts dem Stand.

use crate::icons::{self, FeatureIcon, Icon, IconSource};
use crate::shell::{self, ShellMode, ShellState};
use crate::state::AppState;
pub use crate::theme::BodyColor;
use crate::{browser, properties, shell_panels, theme, viewport};
use egui::{Rect, pos2, vec2};
use planora_feature::{Document, FeatureId, FeatureKind, FeatureStatus};
use std::collections::{HashMap, HashSet};

/// Hinweis, wenn eine neue Aktion an einem zurückgesetzten Körper gesperrt ist.
pub const LOCK_HINT: &str =
    "Dieser Körper ist zurückgesetzt. Springe zuerst zum Ende des Verlaufs.";

/// Zustand eines Schritts in der Leiste.
#[derive(Clone, Debug, PartialEq)]
pub enum StepState {
    /// Wird berechnet.
    Active,
    /// Hinter dem Marker (Rückrollstelle oder globaler Marker), unterdrückt oder übersprungen.
    Inactive,
    /// Berechnung fehlgeschlagen; der Text nennt den Grund.
    Error(String),
}

/// Ein Schritt der Leiste (aus dem Dokument kopiert).
#[derive(Clone, Debug, PartialEq)]
pub struct StepView {
    pub id: FeatureId,
    pub name: String,
    pub icon: FeatureIcon,
    pub state: StepState,
    /// Der Schritt liegt auch in der Kette eines anderen Körpers (etwa eine zweimal extrudierte
    /// Skizze).
    pub shared: bool,
}

/// Meldung, wenn der Marker eines Körpers hinter Schritte soll, die nur der globale Marker
/// zurücksetzt.
pub const GLOBAL_HINT: &str =
    "Der ganze Verlauf ist zurückgesetzt. Aktiviere ihn zuerst mit „Alles aktivieren“.";

/// Aktivität aller Features, einmal je Aufruf aus dem Dokument berechnet (wie
/// `Document::is_active`, ohne den Satz für jeden Schritt neu zu bauen).
struct Activity {
    index: HashMap<FeatureId, usize>,
    rolled: HashSet<FeatureId>,
    marker: usize,
}

impl Activity {
    fn of(doc: &Document) -> Activity {
        Activity {
            index: doc
                .features()
                .iter()
                .enumerate()
                .map(|(i, f)| (f.id, i))
                .collect(),
            rolled: rolled_back(doc, doc.rollbacks()),
            marker: doc.marker(),
        }
    }

    /// Vor dem globalen Marker und von keiner Rückrollstelle erfasst.
    fn is_active(&self, id: FeatureId) -> bool {
        self.index.get(&id).is_some_and(|&i| i < self.marker) && !self.rolled.contains(&id)
    }

    /// Steht hinter dem globalen Marker.
    fn behind_global(&self, id: FeatureId) -> bool {
        self.index.get(&id).is_some_and(|&i| i >= self.marker)
    }
}

/// Die Stellen `stellen` und alles, was von ihnen abhängt (Verweise zeigen nur nach vorn, ein
/// Durchlauf in Listenordnung reicht).
fn rolled_back(doc: &Document, stellen: &[FeatureId]) -> HashSet<FeatureId> {
    let mut found: HashSet<FeatureId> = stellen.iter().copied().collect();
    if found.is_empty() {
        return found;
    }
    for f in doc.features() {
        if !found.contains(&f.id) && f.kind.dependencies().iter().any(|d| found.contains(d)) {
            found.insert(f.id);
        }
    }
    found
}

/// Die Schritte der Kette von `tip` in Listenordnung, Skizzen eingeschlossen; für einen
/// unbekannten Körper keine.
pub fn steps(state: &AppState, tip: FeatureId) -> Vec<StepView> {
    let doc = &state.document;
    let activity = Activity::of(doc);
    // Andere Körper: alle Spitzen außer `tip` und denen, in denen `tip` selbst aufgeht.
    let others: HashSet<FeatureId> = doc
        .body_tips()
        .into_iter()
        .filter(|t| *t != tip)
        .map(|t| doc.chain(t))
        .filter(|c| !c.contains(&tip))
        .flatten()
        .collect();
    doc.chain(tip)
        .into_iter()
        .filter_map(|id| doc.feature(id))
        .map(|f| {
            let state = if !activity.is_active(f.id) {
                StepState::Inactive
            } else {
                match &f.status {
                    FeatureStatus::Ok => StepState::Active,
                    FeatureStatus::Error(e) => StepState::Error(e.clone()),
                    FeatureStatus::Skipped(_)
                    | FeatureStatus::Suppressed
                    | FeatureStatus::NotComputed => StepState::Inactive,
                }
            };
            StepView {
                id: f.id,
                name: f.name.clone(),
                icon: FeatureIcon::of(&f.kind),
                state,
                shared: others.contains(&f.id),
            }
        })
        .collect()
}

/// Anzahl der aktiven Schritte von vorn: Index des ersten inaktiven Schritts der Kette (vor dem
/// globalen Marker und außerhalb jeder Rückrollstelle, siehe `Document::is_active`), sonst die
/// Länge. Liegen in der Kette eines Vereinigens zwei Stellen, zählt die vordere.
pub fn marker_position(state: &AppState, tip: FeatureId) -> usize {
    let activity = Activity::of(&state.document);
    let chain = state.document.chain(tip);
    chain
        .iter()
        .position(|id| !activity.is_active(*id))
        .unwrap_or(chain.len())
}

/// Setzt den Marker von `tip` auf `position`: `position == Anzahl der Schritte` hebt die
/// Rückrollstellen der Kette auf, sonst kommt die Stelle auf den Schritt an `position`. Eine
/// offene Skizze wird vorher beendet (wie bei `browser::roll_back_to`); betrifft die Stelle die
/// geöffnete Skizze selbst, bleibt alles unverändert. Danach einmal `refresh`.
///
/// **Lineare Leiste:** Die Leiste eines Körpers ist eine Reihe über seine ganze Kette in
/// Listenordnung, auch nach einem Vereinigen (die Eingangskörper sind dann verbraucht und haben
/// keine eigene Leiste mehr). Danach sind die ersten `position` Schritte aktiv: Die neue Stelle
/// ersetzt die vergleichbaren Stellen (`Document::set_rollback`); eine Stelle aus einer anderen
/// Eingangskette bleibt nur, wenn sie keinen Schritt vor `position` zurücksetzt, sonst entfällt
/// sie. Die Liste der entfallenden Stellen wird vorher einmal bestimmt. Die Schnittstelle des
/// Dokuments kennt kein Setzen einer ganzen Liste: Jede entfallende Stelle braucht einen
/// eigenen Aufruf (Neuberechnung aus dem Zwischenspeicher); im üblichen Fall ist es einer.
///
/// Abgelehnt (Dokument unverändert, Meldung [`GLOBAL_HINT`]) wird ein Ziel hinter Schritten,
/// die nur der globale Marker zurücksetzt: Eine Stelle dort hätte keine sichtbare Wirkung,
/// würde den Körper aber sperren.
pub fn move_marker(state: &mut AppState, tip: FeatureId, position: usize) -> Result<(), String> {
    state.message = None;
    if state.document.feature(tip).is_none() {
        return state.fail("Den Körper gibt es nicht mehr".into());
    }
    let chain = state.document.chain(tip);
    if position > chain.len() {
        return state.fail("Diese Stelle gibt es im Verlauf nicht".into());
    }
    let activity = Activity::of(&state.document);
    if chain[..position]
        .iter()
        .any(|id| activity.behind_global(*id))
    {
        return state.fail(GLOBAL_HINT.into());
    }
    let target = chain.get(position).copied();
    state.end_sketch_for(target)?;
    let result = match target {
        None => state.document.clear_rollbacks(tip),
        Some(step) => {
            let dropped = dropped_rollbacks(&state.document, &chain, position, step);
            dropped
                .into_iter()
                .try_for_each(|r| state.document.clear_rollbacks(r))
                .and_then(|()| state.document.set_rollback(tip, step))
        }
    };
    state.refresh();
    if let Err(e) = result {
        return state.fail(e.to_string());
    }
    Ok(())
}

/// Stellen, die neben der neuen Stelle `step` (an `position` der Kette) entfallen müssen,
/// damit die ersten `position` Schritte aktiv sind: Stellen der Kette, die `set_rollback` nicht
/// als vergleichbar ersetzt und die einen Schritt vor `position` zurücksetzen. Keine liegt in
/// der Kette einer anderen Stelle, `clear_rollbacks(r)` entfernt also genau `r`.
fn dropped_rollbacks(
    doc: &Document,
    chain: &[FeatureId],
    position: usize,
    step: FeatureId,
) -> Vec<FeatureId> {
    let before: HashSet<FeatureId> = chain[..position].iter().copied().collect();
    if before.is_empty() {
        return Vec::new();
    }
    let comparable: HashSet<FeatureId> = doc
        .chain(step)
        .into_iter()
        .chain(doc.dependents(step))
        .collect();
    doc.rollbacks()
        .iter()
        .copied()
        .filter(|r| *r != step && !comparable.contains(r) && chain.contains(r))
        .filter(|r| rolled_back(doc, &[*r]).iter().any(|id| before.contains(id)))
        .collect()
}

/// Ein Schritt vor (`forward`) oder zurück; am Anfang bzw. Ende bleibt der Marker stehen.
pub fn step(state: &mut AppState, tip: FeatureId, forward: bool) -> Result<(), String> {
    let len = state.document.chain(tip).len();
    let pos = marker_position(state, tip);
    let next = if forward {
        (pos + 1).min(len)
    } else {
        pos.saturating_sub(1)
    };
    if next == pos && state.document.feature(tip).is_some() {
        return Ok(());
    }
    move_marker(state, tip, next)
}

/// Marker vor den ersten Schritt: der Körper ist ganz zurückgesetzt.
pub fn to_start(state: &mut AppState, tip: FeatureId) -> Result<(), String> {
    move_marker(state, tip, 0)
}

/// Marker ans Ende: die Rückrollstellen der Kette entfallen.
pub fn to_end(state: &mut AppState, tip: FeatureId) -> Result<(), String> {
    let len = state.document.chain(tip).len();
    move_marker(state, tip, len)
}

/// Farbe des Körpers: aus der Reihe [`crate::theme::BODY_COLORS`], Index aus der kleinsten
/// Kennung der Körperschritte der Kette (Skizzen zählen nicht, sonst hätten zwei Körper aus
/// einer Skizze dieselbe Farbe) modulo Länge. Ändert sich nicht, wenn andere Körper gelöscht
/// werden; ein unbekannter Körper bekommt die erste Farbe.
pub fn body_color(state: &AppState, tip: FeatureId) -> BodyColor {
    let colors = &crate::theme::BODY_COLORS;
    let doc = &state.document;
    let first = doc
        .chain(tip)
        .into_iter()
        .filter(|id| {
            doc.feature(*id)
                .is_some_and(|f| !matches!(f.kind, FeatureKind::Sketch(_)))
        })
        .min()
        .map_or(0, |id| id.0);
    colors[(first % colors.len() as u64) as usize]
}

/// Ist der Körper zurückgesetzt (eine Rückrollstelle in seiner Kette)? Dann sind neue Aktionen
/// an ihm gesperrt ([`LOCK_HINT`]).
pub fn is_locked(state: &AppState, tip: FeatureId) -> bool {
    state.document.rollback_in_chain(tip).is_some()
}

/// Anzahl der Schritte von vorn, die der globale Marker nicht zurücksetzt: Index des ersten
/// Schritts der Kette hinter dem globalen Marker, sonst die Länge. Weiter als bis hierher kann der
/// Marker des Körpers nicht (dafür gibt es „Alles aktivieren“).
pub fn reachable_end(state: &AppState, tip: FeatureId) -> usize {
    let activity = Activity::of(&state.document);
    let chain = state.document.chain(tip);
    chain
        .iter()
        .position(|id| activity.behind_global(*id))
        .unwrap_or(chain.len())
}

/// Marker von `tip` auf `position` für die Leiste: höchstens bis [`reachable_end`]. Steht der
/// globale Marker vor Schritten des Körpers, hebt das Ziel `reachable_end` die Stelle des Körpers
/// auf, statt eine Stelle auf einen ohnehin zurückgesetzten Schritt zu setzen (die den Körper
/// ohne sichtbare Wirkung sperren würde). Sonst wie [`move_marker`].
pub fn go_to(state: &mut AppState, tip: FeatureId, position: usize) -> Result<(), String> {
    let len = state.document.chain(tip).len();
    let reach = reachable_end(state, tip);
    let target = position.min(reach);
    if target == reach && reach < len {
        if state.document.rollback_in_chain(tip).is_none() {
            return Ok(());
        }
        state.message = None;
        state.end_sketch_for(None)?;
        let result = state.document.clear_rollbacks(tip);
        state.refresh();
        return match result {
            Ok(()) => Ok(()),
            Err(e) => state.fail(e.to_string()),
        };
    }
    if target == marker_position(state, tip) && state.document.feature(tip).is_some() {
        return Ok(());
    }
    move_marker(state, tip, target)
}

// ---------------------------------------------------------------------------------------------
// Oberfläche
// ---------------------------------------------------------------------------------------------

/// Kante eines Schrittsymbols.
const STEP_SIZE: f32 = 30.0;
/// Rand zwischen Schrittfläche und gezeichnetem Symbol.
const STEP_PAD: f32 = 7.0;
/// Kante des Warnzeichens an einem Schritt mit Fehler.
const WARN_SIZE: f32 = 12.0;
/// Abstand zwischen zwei Schritten; in seiner Mitte steht der Marker.
const STEP_GAP: f32 = theme::SPACE_1;
/// Breite des Markers (Fläche zum Ziehen; gezeichnet wird eine Linie mit Griff).
const MARKER_W: f32 = 12.0;
/// Takt beim Abspielen in Sekunden.
const PLAY_INTERVAL: f64 = 0.6;
/// Höchstzahl der Maße im Hinweis zu einem Schritt.
const HOVER_ROWS: usize = 2;

/// Kennung des Felds (auch für [`crate::shell_panels::field_rects`]).
pub(crate) fn bar_id() -> egui::Id {
    egui::Id::new("planora-field-history")
}

fn rects_key() -> egui::Id {
    bar_id().with("rects")
}

/// Abspielen im Tab `instance`: Körper und Zeit des letzten Schritts (fehlt: es läuft nicht).
fn play_key(instance: u64) -> egui::Id {
    bar_id().with(("play", instance))
}

/// Körper und Markerstelle, zu denen die Reihe im Tab `instance` zuletzt gerollt wurde.
fn revealed_key(instance: u64) -> egui::Id {
    bar_id().with(("revealed", instance))
}

fn marker_id(instance: u64) -> egui::Id {
    bar_id().with(("marker", instance))
}

/// Lage der Leiste und ihrer Teile im letzten Bild (für Tests und die Lage der Felder).
#[derive(Clone, Debug)]
pub struct BarRects {
    /// Das ganze Feld.
    pub bar: Rect,
    /// Sichtbarer Ausschnitt der Schrittreihe (sie rollt waagerecht).
    pub strip: Rect,
    /// Je Schritt die Fläche des Symbols (auch außerhalb des sichtbaren Ausschnitts).
    pub steps: Vec<(FeatureId, Rect)>,
    /// Fläche des Markers; leer ohne Körper.
    pub marker: Rect,
    /// Abgeblendete Schritte (hinter dem Marker, unterdrückt oder übersprungen).
    pub dimmed: Vec<FeatureId>,
    /// Knöpfe: Beschriftung, Fläche, bedienbar.
    pub buttons: Vec<(String, Rect, bool)>,
}

impl Default for BarRects {
    fn default() -> Self {
        BarRects {
            bar: Rect::NOTHING,
            strip: Rect::NOTHING,
            steps: Vec::new(),
            marker: Rect::NOTHING,
            dimmed: Vec::new(),
            buttons: Vec::new(),
        }
    }
}

/// Lage der Leiste aus dem letzten Bild; leer, wenn sie nicht gezeigt wurde.
pub fn bar_rects(ctx: &egui::Context) -> BarRects {
    ctx.data(|d| d.get_temp(rects_key())).unwrap_or_default()
}

/// Die Leiste ist in diesem Bild nicht da (Skizze, Zeichnung): keine alte Lage melden.
pub(crate) fn forget(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove::<BarRects>(rects_key()));
}

/// Lage der Leiste in einem Fenster `content` ohne gemessene Felder (Geometrie des Modells mit
/// leeren Breiten).
pub fn default_rect(content: Rect) -> Rect {
    shell::layout_in_mode(
        content,
        ShellMode::Model,
        &ShellState::default(),
        (0.0, 0.0),
        0.0,
        0.0,
        false,
    )
    .history
    .unwrap_or(Rect::NOTHING)
}

/// Zeichnet die Leiste an der Stelle aus der Lage der Felder (ohne Felder, etwa in Tests, aus
/// [`default_rect`]); nicht gesperrt.
pub fn show(ctx: &egui::Context, state: &mut AppState) {
    let rect = shell_panels::last_layout(ctx)
        .and_then(|r| r.history)
        .unwrap_or_else(|| default_rect(ctx.content_rect()));
    show_with(ctx, state, rect, None);
}

/// Was ein Bedienelement der Leiste auslöst; ausgeführt nach dem Zeichnen.
#[derive(Clone, Copy)]
enum Action {
    Select(FeatureId),
    Go(usize),
    Play,
    Pause,
    ActivateAll,
}

/// Stand der Leiste für den gewählten Körper.
struct Bar {
    tip: FeatureId,
    instance: u64,
    steps: Vec<StepView>,
    /// [`marker_position`].
    pos: usize,
    /// [`reachable_end`].
    reach: usize,
    color: egui::Color32,
}

impl Bar {
    fn len(&self) -> usize {
        self.steps.len()
    }

    /// Der globale Marker setzt Schritte dieses Körpers zurück.
    fn hidden(&self) -> bool {
        self.reach < self.len()
    }
}

/// Zeichnet die Leiste in `rect`. `lock` (offene Rückfrage, alte Version; wie beim Befehlsfeld)
/// sperrt alle Bedienelemente und hält das Abspielen an; beim Überfahren steht der Grund.
pub fn show_with(
    ctx: &egui::Context,
    state: &mut AppState,
    rect: Rect,
    lock: Option<&'static str>,
) {
    let dark = state.dark;
    let instance = state.instance;
    let bar = state.history_body().map(|tip| Bar {
        tip,
        instance,
        steps: steps(state, tip),
        pos: marker_position(state, tip),
        reach: reachable_end(state, tip),
        color: body_color(state, tip).get(dark),
    });
    let playing = ctx
        .data(|d| d.get_temp::<(FeatureId, f64)>(play_key(instance)))
        .is_some_and(|(tip, _)| bar.as_ref().is_some_and(|b| b.tip == tip) && lock.is_none());
    let frame = theme::panel_frame(dark).inner_margin(egui::Margin::symmetric(
        theme::SPACE_3 as i8,
        theme::SPACE_1 as i8,
    ));
    let inner = shell_panels::inner_size(&frame, rect.size());
    let mut out = BarRects {
        bar: rect,
        ..BarRects::default()
    };
    let mut action = None;
    let view: &AppState = state;
    shell_panels::field(ctx, bar_id(), rect.min, frame, |ui| {
        ui.set_width(inner.x);
        ui.set_height(inner.y);
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_1;
            ui.spacing_mut().button_padding.x = theme::SPACE_1;
            ui.add_enabled_ui(lock.is_none(), |ui| {
                let controls = (bar.as_ref(), playing, lock);
                playback(ui, controls, &mut out, &mut action);
                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    status(ui, bar.as_ref(), dark, &mut out, &mut action);
                    ui.separator();
                    let w = ui.available_width();
                    ui.allocate_ui_with_layout(
                        vec2(w, inner.y),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            if let Some(bar) = &bar {
                                strip(ui, view, bar, inner.y, &mut out, &mut action);
                            }
                        },
                    );
                });
            });
        });
    });
    ctx.data_mut(|d| d.insert_temp(rects_key(), out));
    if let (Some(bar), Some(action)) = (&bar, action) {
        run(ctx, state, bar, action);
    }
    tick(ctx, state, lock);
}

/// Knöpfe Anfang, zurück, Abspielen (beim Abspielen: Anhalten), vor, Ende. `controls`: Stand,
/// ob es gerade abspielt, Sperrgrund.
fn playback(
    ui: &mut egui::Ui,
    (bar, playing, lock): (Option<&Bar>, bool, Option<&'static str>),
    out: &mut BarRects,
    action: &mut Option<Action>,
) {
    let dark = ui.visuals().dark_mode;
    let (pos, reach) = bar.map_or((0, 0), |b| (b.pos, b.reach));
    let some = bar.is_some();
    let (play_icon, play_label, play_action) = if playing {
        (Icon::HistoryPause, "Anhalten", Action::Pause)
    } else {
        (Icon::HistoryPlay, "Abspielen", Action::Play)
    };
    let entries = [
        (Icon::HistoryStart, "Zum Anfang", pos > 0, Action::Go(0)),
        (
            Icon::HistoryBack,
            "Ein Schritt zurück",
            pos > 0,
            Action::Go(pos.saturating_sub(1)),
        ),
        (play_icon, play_label, playing || reach > 0, play_action),
        (
            Icon::HistoryForward,
            "Ein Schritt vor",
            pos < reach,
            Action::Go(pos + 1),
        ),
        (
            Icon::ToTimelineEnd,
            "Zum Ende",
            pos < reach,
            Action::Go(reach),
        ),
    ];
    let reason = lock.or((!some).then_some("Zuerst einen Körper wählen."));
    for (icon, label, possible, act) in entries {
        let on = some && possible;
        let r = ui
            .add_enabled_ui(on, |ui| {
                icons::source_icon_button(ui, IconSource::Font(icon), label, dark)
            })
            .inner;
        let r = match reason {
            Some(why) => r.on_disabled_hover_ui(|ui| {
                ui.strong(label);
                ui.label(why);
            }),
            None => r.on_hover_text(label),
        };
        if r.clicked() {
            *action = Some(act);
        }
        out.buttons
            .push((label.to_owned(), r.rect, on && ui.is_enabled()));
    }
}

/// Rechts der Stand: „Schritt n von m“, ohne Körper „Körper wählen“, bei vom globalen Marker
/// zurückgesetztem Verlauf der Hinweis mit „Alles aktivieren“ (Zeile von rechts nach links, also
/// in umgekehrter Reihenfolge gesetzt).
fn status(
    ui: &mut egui::Ui,
    bar: Option<&Bar>,
    dark: bool,
    out: &mut BarRects,
    action: &mut Option<Action>,
) {
    let p = theme::palette(dark);
    match bar {
        None => {
            ui.label(egui::RichText::new("Körper wählen").color(p.text_secondary));
        }
        Some(bar) if bar.hidden() => {
            let r = ui.button("Alles aktivieren");
            theme::focus_ring(ui, &r);
            let r = r.on_hover_text("Den ganzen Verlauf wieder berechnen");
            if r.clicked() {
                *action = Some(Action::ActivateAll);
            }
            out.buttons
                .push(("Alles aktivieren".into(), r.rect, ui.is_enabled()));
            ui.label(egui::RichText::new("Gesamter Verlauf zurückgesetzt").color(p.warning));
        }
        Some(bar) => {
            ui.label(
                egui::RichText::new(format!("Schritt {} von {}", bar.pos, bar.len()))
                    .color(p.text_secondary),
            );
        }
    }
}

/// Die Schritte in einer waagerecht rollenden Reihe und der Marker darüber.
fn strip(
    ui: &mut egui::Ui,
    state: &AppState,
    bar: &Bar,
    height: f32,
    out: &mut BarRects,
    action: &mut Option<Action>,
) {
    let area = egui::ScrollArea::horizontal()
        .id_salt(bar_id().with(("strip", bar.instance)))
        .auto_shrink([false, false]);
    let scroll = theme::scroll_area(ui, area, |ui| {
        ui.set_height(height);
        ui.spacing_mut().item_spacing.x = STEP_GAP;
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            // Platz für den Marker vor dem ersten und hinter dem letzten Schritt.
            ui.add_space(MARKER_W / 2.0);
            let mut rects = Vec::with_capacity(bar.len());
            for v in &bar.steps {
                let r = step_button(ui, state, v, bar.color);
                if r.clicked() {
                    *action = Some(Action::Select(v.id));
                    // Danach bewegen die Pfeiltasten den Marker.
                    ui.memory_mut(|m| m.request_focus(marker_id(bar.instance)));
                }
                rects.push((v.id, r.rect));
                if v.state == StepState::Inactive {
                    out.dimmed.push(v.id);
                }
            }
            ui.add_space(MARKER_W / 2.0);
            let row = ui.max_rect();
            out.marker = marker(ui, bar, &rects, row, action);
            out.steps = rects;
        });
    });
    out.strip = scroll.inner_rect;
}

/// Ein Schritt: Symbol in der Körperfarbe (abgeblendet, wenn er nicht berechnet wird), bei einem
/// Fehler mit Warnzeichen; gewählt mit Fläche. Beim Überfahren Name, die wichtigsten Maße und
/// gegebenenfalls der Fehler.
fn step_button(
    ui: &mut egui::Ui,
    state: &AppState,
    v: &StepView,
    color: egui::Color32,
) -> egui::Response {
    let dark = ui.visuals().dark_mode;
    let p = theme::palette(dark);
    let (rect, response) = ui.allocate_exact_size(vec2(STEP_SIZE, STEP_SIZE), egui::Sense::click());
    let enabled = ui.is_enabled();
    let selected = state.selected_feature == Some(v.id);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, selected, &v.name)
    });
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let radius = egui::CornerRadius::same(theme::RADIUS_SM);
        if selected {
            painter.rect_filled(rect, radius, p.bg_active);
        } else if enabled && response.hovered() {
            painter.rect_filled(rect, radius, p.bg_field);
        }
        let mut ink = color;
        if v.state == StepState::Inactive {
            ink = ink.gamma_multiply(theme::HISTORY_DIM_ALPHA);
        }
        if !enabled {
            ink = ink.gamma_multiply(theme::DISABLED_ALPHA);
        }
        icons::paint_feature_icon(painter, rect.shrink(STEP_PAD), v.icon, ink);
        if matches!(v.state, StepState::Error(_)) {
            let warn = Rect::from_min_size(
                pos2(rect.right() - WARN_SIZE, rect.top()),
                vec2(WARN_SIZE, WARN_SIZE),
            );
            icons::paint_source(painter, warn, IconSource::Font(Icon::Warning), p.error);
        }
    }
    theme::focus_ring(ui, &response);
    response.on_hover_ui(|ui| {
        ui.strong(&v.name);
        if let Some(f) = state.document.feature(v.id) {
            for (label, value) in hover_rows(state, &f.kind) {
                ui.label(format!("{label}: {value}"));
            }
        }
        match &v.state {
            StepState::Error(e) => {
                ui.colored_label(p.error, e);
            }
            StepState::Inactive => {
                ui.label(egui::RichText::new("Zurückgesetzt").color(p.text_secondary));
            }
            StepState::Active => {}
        }
    })
}

/// Die wichtigsten Maße eines Schritts für den Hinweis: höchstens [`HOVER_ROWS`] Zeilen aus
/// [`properties::parameter_rows`], Maße (mm, Grad) zuerst; ohne Maße die ersten Zeilen.
pub fn hover_rows(state: &AppState, kind: &FeatureKind) -> Vec<(&'static str, String)> {
    let (mut rows, rest): (Vec<_>, Vec<_>) = properties::parameter_rows(state, kind)
        .into_iter()
        .partition(|(_, value)| value.ends_with(" mm") || value.ends_with('°'));
    rows.extend(rest);
    rows.truncate(HOVER_ROWS);
    rows
}

/// Lage der Lücken vor jedem Schritt und hinter dem letzten (die möglichen Markerstellen).
fn gaps(rects: &[(FeatureId, Rect)]) -> Vec<f32> {
    let mut gaps: Vec<f32> = rects
        .iter()
        .map(|(_, r)| r.left() - STEP_GAP / 2.0)
        .collect();
    if let Some((_, last)) = rects.last() {
        gaps.push(last.right() + STEP_GAP / 2.0);
    }
    gaps
}

/// Der Marker: senkrechte Linie mit Griff in der Lücke an der Markerstelle. Ziehen setzt ihn in
/// die nächste Lücke (höchstens bis [`reachable_end`]); hat er den Fokus, bewegen ihn die Pfeile
/// nach links und rechts. Ändert sich die Stelle, rollt die Reihe, bis er sichtbar ist. Liefert
/// seine Fläche (leer ohne Schritte).
fn marker(
    ui: &mut egui::Ui,
    bar: &Bar,
    rects: &[(FeatureId, Rect)],
    row: Rect,
    action: &mut Option<Action>,
) -> Rect {
    let gaps = gaps(rects);
    let Some(&x) = gaps.get(bar.pos) else {
        return Rect::NOTHING;
    };
    let dark = ui.visuals().dark_mode;
    let p = theme::palette(dark);
    let rect = Rect::from_center_size(pos2(x, row.center().y), vec2(MARKER_W, row.height()));
    let response = ui.interact(rect, marker_id(bar.instance), egui::Sense::click_and_drag());
    let enabled = ui.is_enabled();
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Slider,
            enabled,
            format!("Marker: Schritt {} von {}", bar.pos, bar.len()),
        )
    });
    // Der Marker liest nur die Pfeiltasten: Esc, Entf und Eingabe bleiben für die Ansicht.
    viewport::focus_is_not_typing(ui.ctx(), response.id);
    if response.clicked() || response.drag_started() {
        response.request_focus();
    }
    if response.dragged()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let reach = bar.reach.min(gaps.len() - 1);
        let nearest = gaps[..=reach]
            .iter()
            .enumerate()
            .min_by(|a, b| (a.1 - pointer.x).abs().total_cmp(&(b.1 - pointer.x).abs()))
            .map(|(i, _)| i);
        if let Some(i) = nearest.filter(|i| *i != bar.pos) {
            *action = Some(Action::Go(i));
        }
    }
    if response.has_focus() {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                response.id,
                egui::EventFilter {
                    horizontal_arrows: true,
                    ..Default::default()
                },
            );
        });
        let (left, right) = ui.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight),
            )
        });
        if left && bar.pos > 0 {
            *action = Some(Action::Go(bar.pos - 1));
        } else if right && bar.pos < bar.reach {
            *action = Some(Action::Go(bar.pos + 1));
        }
    }
    // Nach einer Änderung der Stelle (oder des Körpers) in den sichtbaren Ausschnitt springen
    // (ohne Gleiten: beim Ziehen und Abspielen folgte die Reihe sonst verzögert); erst als
    // erledigt merken, wenn dieser Durchgang gilt.
    let shown = (bar.tip, bar.pos);
    let key = revealed_key(bar.instance);
    if ui.ctx().data(|d| d.get_temp::<(FeatureId, usize)>(key)) != Some(shown) {
        ui.scroll_to_rect_animation(rect, None, egui::style::ScrollAnimation::none());
        if !ui.ctx().will_discard() {
            ui.ctx().data_mut(|d| d.insert_temp(key, shown));
        }
    }
    let color = if enabled {
        p.marker
    } else {
        p.marker.gamma_multiply(theme::DISABLED_ALPHA)
    };
    let painter = ui.painter();
    painter.line_segment(
        [pos2(x, row.top() + 2.0), pos2(x, row.bottom() - 2.0)],
        egui::Stroke::new(2.0, color),
    );
    let handle = Rect::from_center_size(pos2(x, row.top() + 5.0), vec2(10.0, 8.0));
    painter.rect_filled(
        handle,
        egui::CornerRadius::same(theme::RADIUS_XS / 2),
        color,
    );
    if response.has_focus() {
        painter.rect_stroke(
            rect.expand(1.0),
            egui::CornerRadius::same(theme::RADIUS_XS),
            egui::Stroke::new(2.0, theme::accent(dark)),
            egui::StrokeKind::Outside,
        );
    }
    rect
}

/// Führt eine Aktion der Leiste aus. Fehler stehen danach in `state.message`.
fn run(ctx: &egui::Context, state: &mut AppState, bar: &Bar, action: Action) {
    let key = play_key(bar.instance);
    match action {
        Action::Select(id) => {
            // Wie ein Klick auf die Zeile im Feature-Baum: nur der Schritt ist hervorgehoben.
            state.selected_feature = Some(id);
            state.selected_body = None;
            state.drawing_ui.selected_drawing = None;
            state.set_selection(None);
        }
        Action::Go(position) => {
            ctx.data_mut(|d| d.remove::<(FeatureId, f64)>(key));
            let _ = go_to(state, bar.tip, position);
        }
        Action::Play => {
            // Am Ende beginnt das Abspielen von vorn.
            if bar.pos >= bar.reach && go_to(state, bar.tip, 0).is_err() {
                return;
            }
            let now = ctx.input(|i| i.time);
            ctx.data_mut(|d| d.insert_temp(key, (bar.tip, now)));
            ctx.request_repaint_after_secs(PLAY_INTERVAL as f32);
        }
        Action::Pause => ctx.data_mut(|d| d.remove::<(FeatureId, f64)>(key)),
        Action::ActivateAll => {
            ctx.data_mut(|d| d.remove::<(FeatureId, f64)>(key));
            let _ = browser::roll_to_end(state);
        }
    }
}

/// Abspielen: alle [`PLAY_INTERVAL`] Sekunden ein Schritt vor; hält am Ende (bei
/// [`reachable_end`]), bei einem Fehler, einem anderen Körper oder einer Sperre an. Nur solange
/// es läuft, wird ein neues Bild angefordert.
fn tick(ctx: &egui::Context, state: &mut AppState, lock: Option<&'static str>) {
    let key = play_key(state.instance);
    let Some((tip, last)) = ctx.data(|d| d.get_temp::<(FeatureId, f64)>(key)) else {
        return;
    };
    let stop = || ctx.data_mut(|d| d.remove::<(FeatureId, f64)>(key));
    if lock.is_some() || state.history_body() != Some(tip) {
        stop();
        return;
    }
    if marker_position(state, tip) >= reachable_end(state, tip) {
        stop();
        return;
    }
    let now = ctx.input(|i| i.time);
    let waited = now - last;
    if waited < PLAY_INTERVAL {
        ctx.request_repaint_after_secs((PLAY_INTERVAL - waited) as f32);
        return;
    }
    let next = marker_position(state, tip) + 1;
    if go_to(state, tip, next).is_err() || next >= reachable_end(state, tip) {
        stop();
        return;
    }
    ctx.data_mut(|d| d.insert_temp(key, (tip, now)));
    ctx.request_repaint_after_secs(PLAY_INTERVAL as f32);
}
