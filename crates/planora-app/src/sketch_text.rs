//! Werkzeug „Text“ (Skizzen 8, Spec C1 bis C7): Der Befehl startet mit der Art „Text“ und wartet
//! auf zwei Klicks für den Rahmen (Fang und vorhandene Punkte gelten; ein gefangener Punkt bekommt
//! beim Anlegen ein `Coincident` wie beim Rechteck). Danach steht im Skizzenfeld der Abschnitt
//! „Text“ mit den Feldern in Fusions Reihenfolge. Mit der Art „Text auf Pfad“ wartet das Werkzeug
//! auf einen Klick auf eine Kurve; ein schon gezogener Rahmen bleibt dabei nur Vorschau.
//!
//! Bis OK schreibt das Werkzeug nichts in die Skizze: Die Vorschau setzt den Text mit der
//! Schriftliste ([`SketchSession::update_text_preview`], höchstens einmal je Bild und nur, wenn
//! sich Inhalt oder Stil geändert haben) und legt ihn mit derselben reinen Lage wie das Anlegen in
//! den Rahmen bzw. auf den Pfad ([`place_box`], [`Sketch::text_path_preview`]). OK legt den Text
//! als **einen** Rückgängig-Schritt an ([`SketchSession::confirm_text`]); Esc, Rechtsklick,
//! Werkzeugwechsel und Abbrechen hinterlassen nichts. Gesetzt wird nur über
//! [`crate::fonts_library::Guarded`].

use crate::fonts_library::Guarded;
use crate::formula_field::{FormulaField, number_field_text, resolve, value_field_text};
use crate::msg::Msg;
use crate::sketch_cursor::DrawCursor;
use crate::sketch_state::{SketchSession, Tool, err};
use crate::theme;
use egui::{Key, Modifiers, RichText};
use planora_feature::{ParamScope, ParamUnit, SketchFeature};
use planora_i18n::{DisplayUnit, current_unit, tr};
use planora_math::DVec2;
use planora_sketch::model::{CurveId, Sketch};
use planora_sketch::snap::SnapHit;
use planora_sketch::text::{HAlign, TextId, TextParams, TextPlacement, VAlign, path_content};
use planora_sketch::text_layout::place_box;
use planora_sketch::text_shape::{
    Contour, MAX_TEXT_CHARS, MAX_TEXT_CURVES, Seg, TextShape, TextShapeError, TextShaper, TextStyle,
};
use planora_text::{FALLBACK_FAMILY, FontLibrary};
use std::sync::Arc;

/// Vorgabeschrift, wenn sie installiert ist (Spec C4, **Annahme**); sonst [`FALLBACK_FAMILY`].
pub const DEFAULT_FONT: &str = "Arial";
/// Vorgabehöhe in mm.
pub const DEFAULT_HEIGHT_MM: f64 = 5.0;
/// Vorgabehöhe in Zoll-Dokumenten (in Zoll).
pub const DEFAULT_HEIGHT_IN: f64 = 0.2;
/// Größter Zeichenabstand in Prozent (Spec A7).
const MAX_SPACING: f64 = 1000.0;
/// Breite der Zahlenfelder im Abschnitt.
const FIELD_W: f32 = 80.0;
/// Zeilen des Textfelds.
const CONTENT_ROWS: usize = 3;
/// Höhe der Liste der Schriften, ab der sie rollt.
const FONT_LIST_H: f32 = 240.0;
/// Abschnitte je Bézierstück in der Vorschau.
const PREVIEW_STEPS: usize = 8;
/// Feste Kennungen der Felder: Sie bleiben, wenn aus einer Zahl eine Formel wird.
const CONTENT_ID: &str = "planora-sketch-text-content";
const HEIGHT_ID: &str = "planora-sketch-text-height";
const SPACING_ID: &str = "planora-sketch-text-spacing";
const ANGLE_ID: &str = "planora-sketch-text-angle";
const FONT_ID: &str = "planora-sketch-text-font";
const FONT_SEARCH_ID: &str = "planora-sketch-text-font-search";

/// Die Vorgabeschrift: „Arial“, wenn `has` sie kennt, sonst Montserrat (Spec C4).
pub fn default_font(has: impl Fn(&str) -> bool) -> String {
    if has(DEFAULT_FONT) {
        DEFAULT_FONT.to_string()
    } else {
        FALLBACK_FAMILY.to_string()
    }
}

/// Die Art im Abschnitt: Text im Rahmen oder Text auf Pfad (Fusion „Text“, „Text On Path“).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextToolKind {
    Box,
    Path,
}

/// Die zuletzt benutzten Werte außer dem Text (Spec C4): Ein gelungenes OK merkt sie sich für
/// den Programmlauf, der nächste Text beginnt damit. Art und Winkel beginnen immer neu
/// (**Annahme**: Sie gehören zur Lage, nicht zum Stil).
#[derive(Clone, Debug, PartialEq)]
pub struct TextMemory {
    pub style: TextStyle,
    pub height: f64,
    pub flip_h: bool,
    pub flip_v: bool,
    pub h: HAlign,
    pub v: VAlign,
    pub above: bool,
    pub fit: bool,
}

impl TextMemory {
    fn of(t: &TextTool) -> TextMemory {
        TextMemory {
            style: t.params.style.clone(),
            height: t.params.height,
            flip_h: t.params.flip_h,
            flip_v: t.params.flip_v,
            h: t.h,
            v: t.v,
            above: t.above,
            fit: t.fit,
        }
    }
}

/// Ein Eintrag der Klappliste „Schriftart“: Familie und Beschriftung (eine fehlende mit
/// „(fehlt)“).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontChoice {
    pub family: String,
    pub label: String,
}

/// Die Einträge der Klappliste (Spec C3): die Familien in der Reihenfolge der Schriftliste
/// (mitgelieferte zuerst, sonst alphabetisch), davor die gewählte, wenn sie fehlt
/// (`present == false`), mit „(fehlt)“. `filter` sucht ohne Groß/Klein im Namen.
pub fn font_choices(
    families: &[String],
    current: &str,
    present: bool,
    filter: &str,
) -> Vec<FontChoice> {
    let needle = filter.trim().to_lowercase();
    let hit = |name: &str| needle.is_empty() || name.to_lowercase().contains(&needle);
    let mut out = Vec::new();
    if !present && !current.trim().is_empty() && hit(current) {
        out.push(FontChoice {
            family: current.to_string(),
            label: tr!("sketch-text-font-missing", font = current.to_string()),
        });
    }
    out.extend(families.iter().filter(|f| hit(f)).map(|f| FontChoice {
        family: f.clone(),
        label: f.clone(),
    }));
    out
}

/// Zustand des Werkzeugs „Text“ (siehe Modulkopf).
#[derive(Clone, Debug)]
pub struct TextTool {
    pub kind: TextToolKind,
    /// Die geklickten Ecken des Rahmens (zwei gegenüberliegende).
    pub corners: Vec<DVec2>,
    /// Der gewählte Pfad (nur „Text auf Pfad“).
    pub path: Option<CurveId>,
    /// Inhalt (mit Zeilen, wie getippt), Schrift, Höhe in mm, Umkehren.
    pub params: TextParams,
    pub h: HAlign,
    pub v: VAlign,
    /// Platzierung oberhalb (sonst unterhalb) des Pfades.
    pub above: bool,
    /// „An Pfad anpassen“.
    pub fit: bool,
    /// Winkel der Grundlinie im Bogenmaß (nur im Rahmen); der Rahmen dreht um Ecke 0.
    pub angle: f64,
    /// Der bearbeitete Text (Ändern, Spec C8).
    pub editing: Option<TextId>,
    /// Was zuletzt gesetzt wurde (Vorschau); passt zu `shaped_for`.
    preview: Option<Arc<TextShape>>,
    /// Texte der Felder Höhe (Anzeigeeinheit), Zeichenabstand (%) und Winkel (Grad).
    pub height_text: String,
    pub spacing_text: String,
    pub angle_text: String,
    /// Fang der geklickten Ecken: OK bindet sie an.
    snaps: Vec<SnapHit>,
    /// Inhalt (wie gesetzt) und Stil der letzten Vorschau.
    shaped_for: Option<(String, TextStyle)>,
    /// Warum die letzte Vorschau nicht gesetzt werden konnte.
    shape_error: Option<TextShapeError>,
    height_error: Option<String>,
    spacing_error: Option<String>,
    angle_error: Option<String>,
    /// Die Schrift ist noch die Vorgabe und wird gewählt, sobald die Schriftliste da ist.
    font_pending: bool,
    /// Das Textfeld holt sich im nächsten Bild die Tastatur (der Abschnitt ist gerade aufgegangen).
    pub(crate) focus_content: bool,
    /// Suchtext der Klappliste und ob sie im letzten Bild offen war.
    font_search: String,
    font_open: bool,
}

impl TextTool {
    /// Ein neues Werkzeug mit den gemerkten Werten, sonst den Vorgaben (Spec C4). Die Höhe ist
    /// 5 mm, in einem Zoll-Dokument 0,2 in.
    pub fn new(memory: Option<&TextMemory>, unit: DisplayUnit) -> TextTool {
        let height = match unit {
            DisplayUnit::Zoll => DisplayUnit::Zoll.from_display(DEFAULT_HEIGHT_IN),
            DisplayUnit::Millimeter => DEFAULT_HEIGHT_MM,
        };
        let m = memory.cloned().unwrap_or(TextMemory {
            style: TextStyle {
                font: String::new(),
                bold: false,
                italic: false,
                spacing: 0.0,
            },
            height,
            flip_h: false,
            flip_v: false,
            h: HAlign::Left,
            v: VAlign::Bottom,
            // **Annahme**: Fusion setzt Text auf Pfad zuerst oberhalb.
            above: true,
            fit: false,
        });
        TextTool {
            kind: TextToolKind::Box,
            corners: Vec::new(),
            path: None,
            height_text: value_field_text(m.height, ParamUnit::Length),
            spacing_text: number_field_text(m.style.spacing),
            angle_text: number_field_text(0.0),
            params: TextParams {
                content: String::new(),
                style: m.style,
                height: m.height,
                flip_h: m.flip_h,
                flip_v: m.flip_v,
            },
            h: m.h,
            v: m.v,
            above: m.above,
            fit: m.fit,
            angle: 0.0,
            editing: None,
            preview: None,
            snaps: Vec::new(),
            shaped_for: None,
            shape_error: None,
            height_error: None,
            spacing_error: None,
            angle_error: None,
            font_pending: memory.is_none(),
            focus_content: false,
            font_search: String::new(),
            font_open: false,
        }
    }

    /// Wird gerade der Rahmen aufgezogen (Art „Text“, noch keine zwei Ecken)?
    pub fn placing_frame(&self) -> bool {
        self.kind == TextToolKind::Box && self.editing.is_none() && self.corners.len() < 2
    }

    /// Ist der Abschnitt „Text“ offen? Nach dem Rahmen, auf dem Pfad und beim Ändern.
    pub fn section_open(&self) -> bool {
        self.editing.is_some() || self.kind == TextToolKind::Path || self.corners.len() >= 2
    }

    /// „An Pfad anpassen“ sperrt Ausrichtung und Zeichenabstand (Spec B7, wie die API).
    pub fn align_locked(&self) -> bool {
        self.kind == TextToolKind::Path && self.fit
    }

    /// Das zuletzt gesetzte Ergebnis der Vorschau.
    pub fn preview_shape(&self) -> Option<&Arc<TextShape>> {
        self.preview.as_ref()
    }

    /// Die Ecken des Rahmens wie beim Anlegen: achsenparallel, Ecke 0 unten links, Linie 0 → 1 die
    /// Grundlinie, gedreht um Ecke 0 um den Winkel. `None` vor dem zweiten Klick.
    pub fn frame_corners(&self) -> Option<[DVec2; 4]> {
        let [a, b] = self.corners.as_slice() else {
            return None;
        };
        let (min, max) = (a.min(*b), a.max(*b));
        let (sin, cos) = self.angle.sin_cos();
        let turn = |p: DVec2| {
            let r = p - min;
            min + DVec2::new(r.x * cos - r.y * sin, r.x * sin + r.y * cos)
        };
        Some([
            min,
            turn(DVec2::new(max.x, min.y)),
            turn(max),
            turn(DVec2::new(min.x, max.y)),
        ])
    }

    /// Was gesetzt wird: im Rahmen der Inhalt, auf dem Pfad mit verbundenen Zeilen.
    fn shaping_content(&self) -> String {
        match self.kind {
            TextToolKind::Box => self.params.content.clone(),
            TextToolKind::Path => path_content(&self.params.content),
        }
    }

    /// Wählt die Vorgabeschrift, sobald die Schriftliste da ist (Spec C4).
    fn resolve_font(&mut self, lib: &FontLibrary) {
        if self.font_pending {
            self.params.style.font = default_font(|f| lib.has_family(f));
            self.font_pending = false;
        }
    }

    /// Text des Felds „Höhe“ (Länge in der Anzeigeeinheit, auch eine Formel): Ein gültiger Wert
    /// über null gilt sofort, sonst steht die Meldung unter dem Feld.
    pub fn set_height_text(&mut self, text: &str, scope: &ParamScope) {
        self.height_text = text.to_string();
        self.height_error = match resolve(text, ParamUnit::Length, scope) {
            Ok((v, _)) if v.is_finite() && v > 0.0 => {
                self.params.height = v;
                None
            }
            Ok(_) => Some(tr!("sketch-text-height")),
            Err(e) => Some(e),
        };
    }

    /// Text des Felds „Zeichenabstand“ in Prozent, `0 ≤ p ≤ 1000`.
    pub fn set_spacing_text(&mut self, text: &str, scope: &ParamScope) {
        self.spacing_text = text.to_string();
        self.spacing_error = match resolve(text, ParamUnit::Number, scope) {
            Ok((v, _)) if (0.0..=MAX_SPACING).contains(&v) => {
                self.params.style.spacing = v;
                None
            }
            Ok(_) => Some(tr!("sketch-text-spacing", max = format!("{MAX_SPACING}"))),
            Err(e) => Some(e),
        };
    }

    /// Text des Felds „Winkel“ in Grad: dreht den Rahmen um Ecke 0 (Spec C5).
    pub fn set_angle_text(&mut self, text: &str, scope: &ParamScope) {
        self.angle_text = text.to_string();
        self.angle_error = match resolve(text, ParamUnit::Angle, scope) {
            Ok((v, _)) if v.is_finite() => {
                self.angle = v.to_radians();
                None
            }
            Ok(_) => Some(tr!(
                "sketch-err-not-finite",
                what = tr!("sketch-text-what-angle")
            )),
            Err(e) => Some(e),
        };
    }

    /// Die erste Meldung eines Felds, das gerade gilt.
    fn field_error(&self) -> Option<String> {
        let angle = (self.kind == TextToolKind::Box)
            .then(|| self.angle_error.clone())
            .flatten();
        let spacing = (!self.align_locked())
            .then(|| self.spacing_error.clone())
            .flatten();
        self.height_error.clone().or(spacing).or(angle)
    }
}

/// Meldung zu einem Fehler beim Setzen.
fn shape_error_msg(e: TextShapeError) -> Msg {
    match e {
        TextShapeError::Empty => crate::msg!("sketch-text-empty"),
        TextShapeError::TooLong => crate::msg!("sketch-text-too-long", max = MAX_TEXT_CHARS),
        TextShapeError::TooManyCurves => {
            crate::msg!("sketch-text-too-many-curves", max = MAX_TEXT_CURVES)
        }
    }
}

/// Setzt `content` mit `style`; ein leerer Inhalt (nur Leerraum) wird gar nicht erst gesetzt.
fn shape_now(
    lib: &FontLibrary,
    content: &str,
    style: &TextStyle,
) -> Result<Arc<TextShape>, TextShapeError> {
    if content.trim().is_empty() {
        return Err(TextShapeError::Empty);
    }
    Guarded(lib).shape(content, style).map(Arc::new)
}

/// Die Konturen als Linienzüge für die Vorschau (Bézierstücke abgetastet).
pub fn contour_polylines(contours: &[Contour]) -> Vec<Vec<DVec2>> {
    contours
        .iter()
        .map(|c| {
            let mut line = vec![c.start];
            let mut at = c.start;
            for seg in &c.segs {
                match *seg {
                    Seg::Line(p) => line.push(p),
                    Seg::Quad(a, p) => line.extend((1..=PREVIEW_STEPS).map(|i| {
                        let t = i as f64 / PREVIEW_STEPS as f64;
                        let s = 1.0 - t;
                        at * (s * s) + a * (2.0 * s * t) + p * (t * t)
                    })),
                    Seg::Cubic(a, b, p) => line.extend((1..=PREVIEW_STEPS).map(|i| {
                        let t = i as f64 / PREVIEW_STEPS as f64;
                        let s = 1.0 - t;
                        at * (s * s * s)
                            + a * (3.0 * s * s * t)
                            + b * (3.0 * s * t * t)
                            + p * (t * t * t)
                    })),
                }
                at = seg.end();
            }
            line
        })
        .collect()
}

impl SketchSession {
    /// Läuft das Werkzeug „Text“?
    pub fn runs_text(&self) -> bool {
        self.tool == Tool::Text && self.text.is_some()
    }

    /// Beginnt das Werkzeug neu, mit den gemerkten Werten.
    pub(crate) fn begin_text(&mut self) {
        self.text = Some(TextTool::new(self.text_memory.as_ref(), current_unit()));
    }

    /// Stellt die Art um. Auf dem Pfad wartet das Werkzeug auf einen Klick auf eine Kurve, ein
    /// gezogener Rahmen bleibt nur Vorschau (Spec C2).
    pub fn set_text_kind(&mut self, kind: TextToolKind) {
        if let Some(t) = self.text.as_mut() {
            t.kind = kind;
        }
    }

    /// Setzt die Vorschau neu, wenn sich Inhalt oder Stil seit dem letzten Setzen geändert haben
    /// (Spec C6: höchstens einmal je Bild, die Oberfläche ruft es einmal je Bild). Ohne
    /// Schriftliste (sie wird noch gelesen) geschieht nichts.
    pub fn update_text_preview(&mut self, fonts: Option<&FontLibrary>) {
        let (Some(t), Some(lib)) = (self.text.as_mut(), fonts) else {
            return;
        };
        t.resolve_font(lib);
        let key = (t.shaping_content(), t.params.style.clone());
        if t.shaped_for.as_ref() == Some(&key) {
            return;
        }
        let shaped = shape_now(lib, &key.0, &key.1);
        t.shape_error = shaped.as_ref().err().copied();
        t.preview = shaped.ok();
        t.shaped_for = Some(key);
    }

    /// Die Konturen der Vorschau in Skizzenkoordinaten, mit derselben Lage wie das Anlegen; `None`,
    /// solange es nichts zu zeigen gibt (kein Text, kein Rahmen, kein Pfad, Pfad ohne Länge).
    pub fn text_preview(&self, sk: &Sketch) -> Option<Vec<Contour>> {
        let t = self.text.as_ref().filter(|_| self.runs_text())?;
        let shape = t.preview.as_ref()?;
        let p = &t.params;
        match t.kind {
            TextToolKind::Box => {
                let corners = t.frame_corners()?;
                let placed = place_box(shape, p.height, corners, t.h, t.v, p.flip_h, p.flip_v);
                let finite = placed
                    .iter()
                    .all(|c| c.start.is_finite() && c.segs.iter().all(|s| s.end().is_finite()));
                finite.then_some(placed)
            }
            TextToolKind::Path => sk.text_path_preview(t.path?, t.above, t.fit, t.h, p, shape),
        }
    }

    /// Klick des Werkzeugs: beim Rahmen die nächste Ecke (mit Fang), auf dem Pfad die Kurve unter
    /// dem Zeiger (`at` roh). Buchstaben, Rahmenlinien und Griffe tragen keinen Text.
    pub(crate) fn click_text(
        &mut self,
        sk: &Sketch,
        cur: &DrawCursor,
        at: DVec2,
        radius: f64,
    ) -> Result<(), Msg> {
        let tol = sk.tolerance().linear;
        let picked = self.shown_curve(sk, at, radius);
        let Some(t) = self.text.as_mut() else {
            return Ok(());
        };
        match t.kind {
            TextToolKind::Box if t.placing_frame() => {
                if let Some(&first) = t.corners.first() {
                    let size = (cur.at - first).abs();
                    if size.x <= tol || size.y <= tol {
                        return Err(crate::msg!("sketch-err-rect-size"));
                    }
                }
                t.corners.push(cur.at);
                t.snaps.push(cur.remember(tol));
                if t.corners.len() == 2 {
                    t.focus_content = true;
                }
            }
            TextToolKind::Box => {}
            TextToolKind::Path => {
                let Some(c) = picked else {
                    return Ok(());
                };
                if sk.text_of_curve(c).is_some()
                    || sk.text_of_frame(c).is_some()
                    || sk.is_handle_curve(c)
                {
                    return Err(crate::msg!("sketch-text-bad-path"));
                }
                t.path = Some(c);
            }
        }
        Ok(())
    }

    /// OK (Knopf, Strg+Enter im Textfeld, Eingabe): legt den Text an und löst, als **ein**
    /// Rückgängig-Schritt („Text erstellen“). Gefangene Ecken des Rahmens bekommen ihre Bindung,
    /// ein Winkel dreht den Rahmen danach um Ecke 0. Leerer Text, Text nur aus Zeichen ohne Umriss,
    /// fehlender Pfad, ungültige Felder und eine noch nicht gelesene Schriftliste werden mit Meldung
    /// abgelehnt; dann bleibt alles, wie es war, und der Abschnitt offen. Danach geht das Werkzeug
    /// zurück zur Auswahl und merkt sich die Werte (Spec C4).
    pub fn confirm_text(
        &mut self,
        sf: &mut SketchFeature,
        fonts: Option<&FontLibrary>,
    ) -> Result<(), String> {
        if self.in_gesture() {
            return Ok(());
        }
        let Some(mut t) = self.text.clone().filter(|_| self.runs_text()) else {
            return Ok(());
        };
        let refuse = |se: &mut SketchSession, m: Msg| {
            let text = m.text();
            se.message = Some(m);
            Err(text)
        };
        let Some(lib) = fonts else {
            return refuse(self, crate::msg!("sketch-text-fonts-loading"));
        };
        if let Some(e) = t.field_error() {
            return refuse(self, Msg::Raw(e));
        }
        match t.kind {
            TextToolKind::Path if t.path.is_none() => {
                return refuse(self, crate::msg!("sketch-text-need-path"));
            }
            TextToolKind::Box if t.corners.len() < 2 => {
                return refuse(self, crate::msg!("sketch-text-need-frame"));
            }
            _ => {}
        }
        t.resolve_font(lib);
        let key = (t.shaping_content(), t.params.style.clone());
        let shaped = match (&t.shaped_for, &t.preview, t.shape_error) {
            (Some(k), Some(s), _) if *k == key => Ok(s.clone()),
            (Some(k), None, Some(e)) if *k == key => Err(e),
            _ => shape_now(lib, &key.0, &key.1),
        };
        let shape = match shaped {
            Ok(s) => s,
            Err(e) => return refuse(self, shape_error_msg(e)),
        };
        // Ein Text nur aus Zeichen ohne Umriss gilt wie ein leerer (Ledger Task 2).
        if shape.segment_count() == 0 {
            return refuse(self, crate::msg!("sketch-text-empty"));
        }
        let tool = t.clone();
        self.atomic(&mut sf.sketch, |se, sk, prior| {
            let params = tool.params.clone();
            match tool.kind {
                TextToolKind::Box => {
                    let id = sk
                        .add_text_box(
                            tool.corners[0],
                            tool.corners[1],
                            params,
                            tool.h,
                            tool.v,
                            shape,
                        )
                        .map_err(err)?;
                    let own = match sk.text(id).map(|d| d.placement) {
                        Some(TextPlacement::Box { corners, .. }) => corners.to_vec(),
                        _ => Vec::new(),
                    };
                    let hits: Vec<DrawCursor> = tool
                        .snaps
                        .iter()
                        .map(|&hit| DrawCursor::remembered(hit, sk))
                        .collect();
                    se.bind_own(sk, &own, &hits)?;
                    if tool.angle != 0.0 {
                        sk.set_text_angle(id, tool.angle).map_err(err)?;
                    }
                }
                TextToolKind::Path => {
                    let path = tool
                        .path
                        .ok_or_else(|| crate::msg!("sketch-text-need-path"))?;
                    sk.add_text_path(path, params, tool.above, tool.fit, tool.h, shape)
                        .map_err(err)?;
                }
            }
            se.solve_checked(sk, prior)?;
            se.edit_label = Some(crate::msg!("undo-sketch-text-create"));
            Ok(())
        })?;
        self.text_memory = Some(TextMemory::of(&t));
        let note = self.message.take();
        self.selection.clear();
        self.drop_curve_picks();
        self.set_tool(&mut sf.sketch, Tool::Select);
        self.message = note;
        Ok(())
    }
}

/// Was der Abschnitt in diesem Bild ausgelöst hat.
#[derive(Default)]
struct SectionOutcome {
    ok: bool,
    cancel: bool,
}

impl SectionOutcome {
    /// Eingabe (OK) oder Esc (Abbrechen) in einem Feld, das gerade die Tastatur abgegeben hat.
    fn keys(&mut self, ui: &egui::Ui, r: &egui::Response) {
        if r.lost_focus() {
            self.ok |= ui.input(|i| i.key_pressed(Key::Enter));
            self.cancel |= ui.input(|i| i.key_pressed(Key::Escape));
        }
    }
}

/// Der Abschnitt „Text“ im Skizzenfeld (Spec C3), offen nach dem Rahmen, auf dem Pfad und beim
/// Ändern. Felder in Fusions Reihenfolge: Art, Text, Pfad, Schriftart, Schrift, Höhe,
/// Zeichenabstand, Winkel, Platzierung und „An Pfad anpassen“, Umkehren, Ausrichtung, OK und
/// Abbrechen. Solange die Schriftliste gelesen wird, steht „Schriften werden gelesen …“ da und OK
/// ist gesperrt (Spec G4).
pub(crate) fn text_options(
    ui: &mut egui::Ui,
    se: &mut SketchSession,
    sf: &mut SketchFeature,
    fonts: Option<&FontLibrary>,
    scope: &ParamScope,
) {
    if !se.runs_text() || !se.text.as_ref().is_some_and(TextTool::section_open) {
        return;
    }
    let dark = ui.visuals().dark_mode;
    let error = theme::palette(dark).error;
    let mut out = SectionOutcome::default();
    theme::section_header(ui, &tr!("sketch-text-section"), theme::SUB_HEADER_H);
    if fonts.is_none() {
        ui.label(
            RichText::new(tr!("sketch-text-fonts-loading"))
                .small()
                .weak(),
        );
    }
    // Art
    let mut kind = se.text.as_ref().map_or(TextToolKind::Box, |t| t.kind);
    ui.horizontal_wrapped(|ui| {
        ui.radio_value(&mut kind, TextToolKind::Box, tr!("sketch-text-kind-box"));
        ui.radio_value(&mut kind, TextToolKind::Path, tr!("sketch-text-kind-path"));
    });
    se.set_text_kind(kind);
    let Some(t) = se.text.as_mut() else {
        return;
    };
    // Text: Enter bricht die Zeile, Strg+Enter ist OK.
    ui.label(tr!("sketch-text-content"));
    let r = theme::field(
        ui,
        egui::TextEdit::multiline(&mut t.params.content)
            .id_salt(CONTENT_ID)
            .desired_rows(CONTENT_ROWS)
            .desired_width(f32::INFINITY),
    );
    if std::mem::take(&mut t.focus_content) {
        r.request_focus();
    }
    if r.has_focus() && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter)) {
        out.ok = true;
    }
    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
        out.cancel = true;
    }
    if let Some(e) = t.shape_error.filter(|e| *e != TextShapeError::Empty) {
        ui.colored_label(error, shape_error_msg(e).text());
    }
    // Pfad
    if t.kind == TextToolKind::Path {
        ui.horizontal(|ui| {
            ui.label(tr!("sketch-text-path"));
            let state = if t.path.is_some() {
                tr!("sketch-text-path-chosen")
            } else {
                tr!("sketch-text-path-pick")
            };
            ui.label(RichText::new(state).weak());
        });
    }
    // Schriftart
    ui.label(tr!("sketch-text-font"));
    font_combo(ui, t, fonts);
    // Schrift
    ui.horizontal_wrapped(|ui| {
        ui.label(tr!("sketch-text-style"));
        ui.toggle_value(&mut t.params.style.bold, tr!("sketch-text-bold"));
        ui.toggle_value(&mut t.params.style.italic, tr!("sketch-text-italic"));
    });
    // Höhe, Zeichenabstand, Winkel
    let locked = t.align_locked();
    number_row(ui, &mut out, t, NumberField::Height, true, scope);
    number_row(ui, &mut out, t, NumberField::Spacing, !locked, scope);
    if t.kind == TextToolKind::Box {
        number_row(ui, &mut out, t, NumberField::Angle, true, scope);
    }
    for e in [&t.height_error, &t.spacing_error]
        .into_iter()
        .chain((t.kind == TextToolKind::Box).then_some(&t.angle_error))
        .flatten()
    {
        ui.colored_label(error, e);
    }
    // Platzierung und „An Pfad anpassen“
    if t.kind == TextToolKind::Path {
        ui.horizontal_wrapped(|ui| {
            ui.label(tr!("sketch-text-placement"));
            ui.radio_value(&mut t.above, true, tr!("sketch-text-above"));
            ui.radio_value(&mut t.above, false, tr!("sketch-text-below"));
        });
        ui.checkbox(&mut t.fit, tr!("sketch-text-fit"));
    }
    // Umkehren
    ui.horizontal_wrapped(|ui| {
        ui.label(tr!("sketch-text-flip"));
        ui.toggle_value(&mut t.params.flip_h, tr!("sketch-text-flip-h"));
        ui.toggle_value(&mut t.params.flip_v, tr!("sketch-text-flip-v"));
    });
    // Ausrichtung (senkrecht nur im Rahmen); „An Pfad anpassen“ sperrt sie.
    ui.label(tr!("sketch-text-align"));
    let locked = t.align_locked();
    ui.add_enabled_ui(!locked, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.radio_value(&mut t.h, HAlign::Left, tr!("sketch-text-align-left"));
            ui.radio_value(&mut t.h, HAlign::Center, tr!("sketch-text-align-center"));
            ui.radio_value(&mut t.h, HAlign::Right, tr!("sketch-text-align-right"));
        });
    });
    if t.kind == TextToolKind::Box {
        ui.horizontal_wrapped(|ui| {
            ui.radio_value(&mut t.v, VAlign::Top, tr!("sketch-text-align-top"));
            ui.radio_value(&mut t.v, VAlign::Middle, tr!("sketch-text-align-middle"));
            ui.radio_value(&mut t.v, VAlign::Bottom, tr!("sketch-text-align-bottom"));
        });
    }
    ui.horizontal(|ui| {
        if ui
            .add_enabled(fonts.is_some(), egui::Button::new(tr!("common-ok")))
            .on_hover_text(tr!("sketch-text-ok-hint"))
            .on_disabled_hover_text(tr!("sketch-text-fonts-loading"))
            .clicked()
        {
            out.ok = true;
        }
        if ui.button(tr!("common-cancel")).clicked() {
            out.cancel = true;
        }
    });
    if out.cancel {
        se.cancel(&mut sf.sketch);
    } else if out.ok {
        // Fehler stehen in der Meldung der Sitzung.
        let _ = se.confirm_text(sf, fonts);
    }
}

/// Die Zahlenfelder des Abschnitts.
#[derive(Clone, Copy)]
enum NumberField {
    Height,
    Spacing,
    Angle,
}

/// Eine Zeile mit Name und Formelfeld; Änderungen gelten sofort (Vorschau).
fn number_row(
    ui: &mut egui::Ui,
    out: &mut SectionOutcome,
    t: &mut TextTool,
    field: NumberField,
    enabled: bool,
    scope: &ParamScope,
) {
    let (label, id, unit, mut text) = match field {
        NumberField::Height => (
            tr!("sketch-text-height-label"),
            HEIGHT_ID,
            ParamUnit::Length,
            t.height_text.clone(),
        ),
        NumberField::Spacing => (
            tr!("sketch-text-spacing-label"),
            SPACING_ID,
            ParamUnit::Number,
            t.spacing_text.clone(),
        ),
        NumberField::Angle => (
            tr!("sketch-text-angle-label"),
            ANGLE_ID,
            ParamUnit::Angle,
            t.angle_text.clone(),
        ),
    };
    ui.horizontal(|ui| {
        ui.label(&label);
        let o = FormulaField::new(id, &mut text, unit)
            .width(FIELD_W)
            .enabled(enabled)
            .error_below(false)
            .label(label.clone())
            .show(ui, scope);
        if matches!(field, NumberField::Spacing) {
            ui.label(RichText::new("%").weak());
        }
        out.keys(ui, &o.response);
        if o.response.changed() || o.fx_clicked {
            match field {
                NumberField::Height => t.set_height_text(&text, scope),
                NumberField::Spacing => t.set_spacing_text(&text, scope),
                NumberField::Angle => t.set_angle_text(&text, scope),
            }
        }
    });
}

/// Klappliste „Schriftart“ mit Suche (Spec C3): die gewählte Familie, eine fehlende mit
/// „(fehlt)“; offen das Suchfeld (es hat gleich die Tastatur) und die Einträge nach
/// [`font_choices`]. Solange die Schriftliste gelesen wird, nur die gewählte, gesperrt.
fn font_combo(ui: &mut egui::Ui, t: &mut TextTool, fonts: Option<&FontLibrary>) {
    let current = t.params.style.font.clone();
    let present = fonts.is_none_or(|lib| current.is_empty() || lib.has_family(&current));
    let shown = if present {
        current.clone()
    } else {
        tr!("sketch-text-font-missing", font = current.clone())
    };
    let Some(lib) = fonts else {
        ui.add_enabled(false, egui::Button::new(shown));
        return;
    };
    let mut open = false;
    egui::ComboBox::from_id_salt(FONT_ID)
        .selected_text(shown)
        .width(ui.available_width())
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show_ui(ui, |ui| {
            open = true;
            let r = theme::text_field(
                ui,
                egui::TextEdit::singleline(&mut t.font_search)
                    .id_salt(FONT_SEARCH_ID)
                    .hint_text(tr!("sketch-text-font-search"))
                    .desired_width(f32::INFINITY),
            );
            if !t.font_open {
                r.request_focus();
            }
            let families = lib.families();
            let area = egui::ScrollArea::vertical()
                .id_salt("planora-sketch-text-font-list")
                .max_height(FONT_LIST_H);
            theme::scroll_area(ui, area, |ui| {
                for choice in font_choices(&families, &current, present, &t.font_search) {
                    if ui
                        .selectable_label(choice.family == current, &choice.label)
                        .clicked()
                    {
                        t.params.style.font = choice.family;
                        t.font_pending = false;
                        ui.close();
                    }
                }
            });
        });
    if !open {
        t.font_search.clear();
    }
    t.font_open = open;
}
