//! Werkzeug „Text“ (Skizzen 8, Spec C1 bis C7): Der Befehl startet mit der Art „Text“ und wartet
//! auf zwei Klicks für den Rahmen (Fang und vorhandene Punkte gelten; ein gefangener Punkt bekommt
//! beim Anlegen ein `Coincident` wie beim Rechteck). Danach steht im Skizzenfeld der Abschnitt
//! „Text“ mit den Feldern in Fusions Reihenfolge. Mit der Art „Text auf Pfad“ wartet das Werkzeug
//! auf einen Klick auf eine Kurve; ein schon gezogener Rahmen bleibt dabei nur Vorschau.
//!
//! Bis OK schreibt das Werkzeug nichts in die Skizze: Die Vorschau setzt den Text mit der
//! Schriftliste ([`SketchSession::update_text_preview`], höchstens einmal je Bild und nur, wenn
//! sich Inhalt oder Stil geändert haben) und legt ihn mit derselben reinen Lage wie das Anlegen in
//! den Rahmen bzw. auf den Pfad ([`place_box`], [`Sketch::text_path_preview`]). Auch das Legen
//! geschieht nur, wenn sich Lage, Werte, gesetzte Form oder der Stand der Skizze geändert haben;
//! ruhige Bilder zeichnen das Gemerkte ([`SketchSession::placed_text_preview`]). OK legt den Text
//! als **einen** Rückgängig-Schritt an ([`SketchSession::confirm_text`]); Esc, Rechtsklick,
//! Werkzeugwechsel und Abbrechen hinterlassen nichts. Gesetzt wird nur über
//! [`crate::fonts_library::Guarded`].
//!
//! **Ändern** (Spec C8): Doppelklick auf einen Buchstaben, der Knopf „Text bearbeiten“ und die
//! Aktion öffnen denselben Abschnitt mit den Werten des Textes ([`SketchSession::begin_text_edit`]).
//! Die Vorschau ist das Ergebnis von OK auf einer Kopie der Skizze; an ihrer Stelle zeichnet die
//! Ansicht vom Text nichts: keine Buchstaben, keinen Rahmen samt Ecken, keine Hilfslinien und
//! keine Profilfüllung ([`SketchSession::text_hidden`], Handtest B2). OK ändert den Text in
//! **einem** Schritt „Text ändern“ ([`Sketch::edit_text`], dazu [`Sketch::set_text_angle`]),
//! Abbrechen hinterlässt nichts. Die gemerkten Werte für neue Texte bleiben dabei, wie sie sind
//! (**Annahme**). **Text auflösen** (Spec C9) wirkt auf alle gewählten Texte in einem Schritt
//! ([`SketchSession::explode_selected_texts`]).

use crate::fonts_library::Guarded;
use crate::formula_field::{FormulaField, number_field_text, resolve, value_field_text};
use crate::msg::Msg;
use crate::sketch_cursor::DrawCursor;
use crate::sketch_draw3::{Cursor3, Hit3, Plane3, bind_new3};
use crate::sketch_state::{Item, SketchSession, Tool, err};
use crate::sketch_triad::Triad;
use crate::theme;
use egui::{Key, Modifiers, RichText};
use planora_feature::{ParamScope, ParamUnit, SketchFeature};
use planora_i18n::{DisplayUnit, current_unit, tr};
use planora_math::{DVec2, DVec3};
use planora_sketch::SketchError;
use planora_sketch::constraint::ConstraintKind;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch, plane_basis};
use planora_sketch::snap::SnapHit;
use planora_sketch::text::{
    HAlign, MAX_SPACING, TextId, TextKindEdit, TextParams, TextPlacement, VAlign, path_content,
};
use planora_sketch::text_layout::place_box;
use planora_sketch::text_shape::{
    Contour, MAX_TEXT_CHARS, MAX_TEXT_CURVES, Seg, TextShape, TextShapeError, TextShaper, TextStyle,
};
use planora_text::{FALLBACK_FAMILY, FontLibrary};
use std::collections::{HashMap, HashSet};
use std::f64::consts::TAU;
use std::sync::{Arc, Mutex};

/// Vorgabeschrift, wenn sie installiert ist (Spec C4, **Annahme**); sonst [`FALLBACK_FAMILY`].
pub const DEFAULT_FONT: &str = "Arial";
/// Vorgabehöhe in mm.
pub const DEFAULT_HEIGHT_MM: f64 = 5.0;
/// Vorgabehöhe in Zoll-Dokumenten (in Zoll).
pub const DEFAULT_HEIGHT_IN: f64 = 0.2;
/// Breite der Zahlenfelder im Abschnitt.
const FIELD_W: f32 = 80.0;
/// Zeilen des Textfelds.
const CONTENT_ROWS: usize = 3;
/// Höhe der Liste der Schriften, ab der sie rollt.
const FONT_LIST_H: f32 = 240.0;
/// Abschnitte je Bézierstück in der Vorschau.
const PREVIEW_STEPS: usize = 8;
/// Abschnitte je Umlauf für Kurven der Vorschau beim Ändern, die weder Strecke noch Bézierstück
/// sind (Buchstaben sind es immer; nur zur Sicherheit).
const PREVIEW_STEPS_PER_TURN: usize = 64;
/// Nachkommastellen eines vorbelegten Winkelfelds beim Ändern.
const FIELD_DIGITS: i32 = 9;
/// Ab dieser Abweichung im Bogenmaß gilt der Winkel beim Ändern als geändert.
const ANGLE_EPS: f64 = 1e-12;
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

/// Gemeinsamer Speicher der gemerkten Werte (Spec C4): einer je Arbeitsbereich, also für den
/// Programmlauf; alle Tabs und Skizzensitzungen teilen ihn
/// ([`crate::state::AppState::share_text_memory`]).
#[derive(Clone, Debug, Default)]
pub struct TextMemoryShare(Arc<Mutex<Option<TextMemory>>>);

impl TextMemoryShare {
    /// Die gemerkten Werte; `None` vor dem ersten gelungenen OK.
    pub fn get(&self) -> Option<TextMemory> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn set(&self, memory: TextMemory) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(memory);
    }
}

/// Wofür die gemerkte Vorschau gelegt wurde: gesetzte Form (Zeiger), Art, Rahmen, Pfad samt Stand
/// der Skizze (der Pfad ändert sich nur mit dem Stand) und alle Werte der Lage.
#[derive(Clone, Debug, PartialEq)]
struct PlaceKey {
    shape: usize,
    kind: TextToolKind,
    corners: Option<[DVec2; 4]>,
    path: Option<(CurveId, u64)>,
    h: HAlign,
    v: VAlign,
    above: bool,
    fit: bool,
    height: f64,
    flips: (bool, bool),
    /// Winkel (beim Ändern legt er auch den neuen Rahmen eines Textes auf Pfad).
    angle: f64,
    /// Beim Ändern: Stand der Skizze, denn die Vorschau ist das Ergebnis auf einer Kopie.
    revision: Option<u64>,
    /// Beim Ändern: Inhalt und Stil (die Kopie braucht die Werte, nicht nur die Form).
    params: Option<TextParams>,
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
    /// Die geklickten Ecken des Rahmens (zwei gegenüberliegende); bei einem Rahmen in 3D in der
    /// Basis von [`TextTool::plane3`].
    pub corners: Vec<DVec2>,
    /// Ein Text in 3D (Skizzen 10, Spec Entwurf 4): die Ebene des Rahmens mit der Basis, in der
    /// die Sketch-Seite setzt ([`text_plane`]). Ecken, Rahmen und Vorschau stehen dann in ihrer
    /// Basis und werden zum Zeichnen in den Raum gelegt. `None` in der Skizzenebene.
    pub plane3: Option<Plane3>,
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
    /// Beim Ändern eines Textes im Rahmen: seine Ecken und sein Winkel beim Öffnen. Der Rahmen
    /// der Vorschau dreht daraus um Ecke 0 wie [`Sketch::set_text_angle`].
    edit_frame: Option<([DVec2; 4], f64)>,
    /// Beim Ändern: die Auswahl vor dem Öffnen; Abbrechen stellt sie wieder her.
    pub(crate) selection_before: Option<Vec<Item>>,
    /// Was zuletzt gesetzt wurde (Vorschau); passt zu `shaped_for`.
    preview: Option<Arc<TextShape>>,
    /// Die zuletzt gelegte Vorschau (`None`: nichts zu zeigen) und wofür.
    placed: Option<(PlaceKey, Option<Arc<Vec<Contour>>>)>,
    /// Wie oft die Vorschau gesetzt bzw. gelegt wurde (ruhige Bilder tun beides nicht).
    shapings: u64,
    placements: u64,
    /// Texte der Felder Höhe (Anzeigeeinheit), Zeichenabstand (%) und Winkel (Grad).
    pub height_text: String,
    pub spacing_text: String,
    pub angle_text: String,
    /// Fang der geklickten Ecken: OK bindet sie an.
    snaps: Vec<SnapHit>,
    /// Fang der geklickten Ecken eines Rahmens in 3D (Skizzen 10): OK bindet sie an wie in 2D.
    snaps3: Vec<Hit3>,
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
            plane3: None,
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
            edit_frame: None,
            selection_before: None,
            preview: None,
            placed: None,
            shapings: 0,
            placements: 0,
            snaps: Vec::new(),
            snaps3: Vec::new(),
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

    /// Das Werkzeug zum Ändern des Textes `t` (Spec C8): Art, Inhalt, Schrift, Höhe, Abstand,
    /// Umkehren, Ausrichtung, Pfad und Winkel wie im Text. Werte, die der Text nicht hat (bei
    /// Text auf Pfad die senkrechte Ausrichtung, im Rahmen Platzierung und „An Pfad anpassen“),
    /// kommen aus `memory` bzw. den Vorgaben. `None` für einen unbekannten Text.
    pub fn for_edit(
        sk: &Sketch,
        t: TextId,
        memory: Option<&TextMemory>,
        unit: DisplayUnit,
    ) -> Option<TextTool> {
        let d = sk.text(t)?;
        let mut tool = TextTool::new(memory, unit);
        tool.editing = Some(t);
        tool.font_pending = false;
        // Wie nach dem Rahmen: Das Textfeld hat gleich die Tastatur.
        tool.focus_content = true;
        tool.params = TextParams {
            content: d.content.clone(),
            style: d.style.clone(),
            height: d.height,
            flip_h: d.flip_h,
            flip_v: d.flip_v,
        };
        tool.height_text = value_field_text(d.height, ParamUnit::Length);
        tool.spacing_text = number_field_text(d.style.spacing);
        match d.placement {
            TextPlacement::Box { corners, h, v, .. } => {
                tool.kind = TextToolKind::Box;
                tool.h = h;
                tool.v = v;
                tool.angle = sk.text_angle(t).unwrap_or(0.0);
                // Ein Text in 3D: Rahmen und Vorschau in der Basis seiner Ebene durch Ecke 0.
                tool.plane3 = d.normal.and_then(|n| {
                    let origin = sk.point_xyz(corners[0]).ok()?;
                    Some(basis_plane(origin, n))
                });
                let at = corners.map(|p| match tool.plane3 {
                    Some(plane) => sk.point_xyz(p).map_or(DVec2::ZERO, |q| plane.to2(q)),
                    None => sk.point_xy(p).unwrap_or(DVec2::ZERO),
                });
                tool.edit_frame = Some((at, tool.angle));
            }
            TextPlacement::Path {
                path,
                above,
                fit,
                h,
            } => {
                tool.kind = TextToolKind::Path;
                tool.path = Some(path);
                tool.above = above;
                tool.fit = fit;
                tool.h = h;
            }
        }
        // Im Feld gerundet (20° statt 19,999999999999996°); der Winkel selbst bleibt genau und
        // ändert sich erst, wenn jemand das Feld ändert.
        tool.angle_text = number_field_text(round_digits(tool.angle.to_degrees()));
        Some(tool)
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

    /// Wie oft die Vorschau bisher gesetzt wurde (Schrift, Inhalt oder Stil geändert).
    #[doc(hidden)]
    pub fn shapings(&self) -> u64 {
        self.shapings
    }

    /// Wie oft die Vorschau bisher in den Rahmen oder auf den Pfad gelegt wurde.
    #[doc(hidden)]
    pub fn placements(&self) -> u64 {
        self.placements
    }

    /// Der Schlüssel der Lage zum Stand `sk`; `None` ohne gesetzte Form.
    fn place_key(&self, sk: &Sketch) -> Option<PlaceKey> {
        let shape = self.preview.as_ref()?;
        Some(PlaceKey {
            shape: Arc::as_ptr(shape) as usize,
            kind: self.kind,
            corners: self.frame_corners(),
            path: self.path.map(|p| (p, sk.revision())),
            h: self.h,
            v: self.v,
            above: self.above,
            fit: self.fit,
            height: self.params.height,
            flips: (self.params.flip_h, self.params.flip_v),
            angle: self.angle,
            revision: self.editing.map(|_| sk.revision()),
            params: self.editing.map(|_| self.params.clone()),
        })
    }

    /// Legt die Vorschau in den Rahmen bzw. auf den Pfad (siehe [`SketchSession::text_preview`]).
    fn place(&self, sk: &Sketch) -> Option<Vec<Contour>> {
        let shape = self.preview.as_ref()?;
        if let Some(t) = self.editing {
            // Ändern: genau das Ergebnis von OK, auf einer Kopie.
            let mut copy = sk.clone();
            self.apply_edit(&mut copy, t, shape.clone()).ok()?;
            return text_contours(&copy, t, self.plane3.as_ref());
        }
        let p = &self.params;
        match self.kind {
            TextToolKind::Box => {
                let corners = self.frame_corners()?;
                let placed =
                    place_box(shape, p.height, corners, self.h, self.v, p.flip_h, p.flip_v);
                let finite = placed
                    .iter()
                    .all(|c| c.start.is_finite() && c.segs.iter().all(|s| s.end().is_finite()));
                finite.then_some(placed)
            }
            TextToolKind::Path => {
                sk.text_path_preview(self.path?, self.above, self.fit, self.h, p, shape)
            }
        }
    }

    /// Die Ecken des Rahmens wie beim Anlegen: achsenparallel, Ecke 0 unten links, Linie 0 → 1 die
    /// Grundlinie, gedreht um Ecke 0 um den Winkel. `None` vor dem zweiten Klick.
    pub fn frame_corners(&self) -> Option<[DVec2; 4]> {
        // Ändern: der Rahmen des Textes, um Ecke 0 auf den Winkel gedreht.
        if let Some((frame, start)) = self.edit_frame {
            let (sin, cos) = (self.angle - start).sin_cos();
            let o = frame[0];
            return Some(frame.map(|p| {
                let r = p - o;
                o + DVec2::new(r.x * cos - r.y * sin, r.x * sin + r.y * cos)
            }));
        }
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

    /// Ändert den Text `t` in `sk` nach den Werten des Werkzeugs (Spec C8): Werte, Art und Lage
    /// über [`Sketch::edit_text`], im Rahmen danach der Winkel, wenn er abweicht. Bei einem Fehler
    /// kann `sk` halb geändert sein; der Aufrufer arbeitet atomar oder auf einer Kopie.
    fn apply_edit(
        &self,
        sk: &mut Sketch,
        t: TextId,
        shape: Arc<TextShape>,
    ) -> Result<(), SketchError> {
        let kind = match self.kind {
            TextToolKind::Box => TextKindEdit::Box {
                h: self.h,
                v: self.v,
            },
            TextToolKind::Path => TextKindEdit::Path {
                path: self.path.ok_or(SketchError::UnknownObject)?,
                above: self.above,
                fit: self.fit,
                h: self.h,
            },
        };
        sk.edit_text(t, self.params.clone(), kind, shape)?;
        if self.kind == TextToolKind::Box
            && let Some(now) = sk.text_angle(t)
        {
            let diff = (self.angle - now).rem_euclid(TAU);
            if diff > ANGLE_EPS && TAU - diff > ANGLE_EPS {
                sk.set_text_angle(t, self.angle)?;
            }
        }
        Ok(())
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

/// Rundet auf [`FIELD_DIGITS`] Nachkommastellen (für vorbelegte Felder).
fn round_digits(v: f64) -> f64 {
    let scale = 10f64.powi(FIELD_DIGITS);
    let r = (v * scale).round() / scale;
    if r.is_finite() { r } else { v }
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

/// Die Ebene eines Textes in 3D durch `origin` mit der Normale `n` in der Basis, in der die
/// Skizze ihn setzt ([`plane_basis`]): Grundlinie entlang `u`, oben `v`.
pub fn basis_plane(origin: DVec3, n: DVec3) -> Plane3 {
    let (u, v) = plane_basis(n);
    Plane3 {
        origin,
        u,
        v,
        flat: false,
    }
}

/// Die Ebene eines neuen Textes in 3D: die aktive Ebene der Triade durch `at`, die Normale so
/// gewählt, dass „oben“ im Text nicht nach unten (−Z) zeigt (**Annahme**: lesbar wie in Fusion
/// von vorn, etwa in der Ebene ZX Grundlinie X und oben Z). Ein Drehen der Triade um ihre eigene
/// Normale zählt nicht: Der Rahmen folgt der Basis der Ebene, gedreht wird er über den Winkel.
pub fn text_plane(triad: &Triad, at: DVec3) -> Plane3 {
    let (a, b) = triad.active.axes();
    let mut n = triad.axis(a).cross(triad.axis(b)).normalize();
    if plane_basis(n).1.z < -1e-9 {
        n = -n;
    }
    basis_plane(at, n)
}

/// Die Buchstaben des Textes `t` als Konturen aus je einem Stück für die Vorschau: Strecke,
/// Kegelschnitt als quadratische, Spline mit vier Kontrollpunkten als kubische Bézierkurve.
///
/// Mit `plane` (Text in 3D) in der Basis dieser Ebene statt in Skizzenkoordinaten.
fn text_contours(sk: &Sketch, t: TextId, plane: Option<&Plane3>) -> Option<Vec<Contour>> {
    let d = sk.text(t)?;
    let at = |p: PointId| match plane {
        Some(plane) => sk.point_xyz(p).ok().map(|q| plane.to2(q)),
        None => sk.point_xy(p).ok(),
    };
    let mut out = Vec::with_capacity(d.curves.len());
    for &c in &d.curves {
        let Some(data) = sk.curve(c) else {
            continue;
        };
        let piece = match &data.kind {
            CurveKind::Line { a, b } => Some((at(*a)?, vec![Seg::Line(at(*b)?)])),
            CurveKind::Conic {
                start, end, apex, ..
            } => Some((at(*start)?, vec![Seg::Quad(at(*apex)?, at(*end)?)])),
            CurveKind::ControlSpline { points, degree, .. }
                if *degree == 3 && points.len() == 4 =>
            {
                Some((
                    at(points[0])?,
                    vec![Seg::Cubic(at(points[1])?, at(points[2])?, at(points[3])?)],
                ))
            }
            _ => None,
        };
        let (start, segs) = match piece {
            Some(p) => p,
            None => {
                let mut line = sk.curve_polyline(c, PREVIEW_STEPS_PER_TURN)?;
                if let Some(plane) = plane {
                    for p in &mut line {
                        *p = plane.to2(DVec3::new(p.x, p.y, 0.0));
                    }
                }
                let (&start, rest) = line.split_first()?;
                (start, rest.iter().map(|&p| Seg::Line(p)).collect())
            }
        };
        out.push(Contour { start, segs });
    }
    Some(out)
}

impl SketchSession {
    /// Läuft das Werkzeug „Text“?
    pub fn runs_text(&self) -> bool {
        self.tool == Tool::Text && self.text.is_some()
    }

    /// Ist der Abschnitt „Text“ im Skizzenfeld offen? Dann steht die Meldung der Sitzung direkt
    /// unter OK und Abbrechen statt unten im Skizzenfeld (bei 1920 × 1080 sonst außer Sicht).
    pub fn text_section_open(&self) -> bool {
        self.runs_text() && self.text.as_ref().is_some_and(TextTool::section_open)
    }

    /// Beginnt das Werkzeug neu, mit den gemerkten Werten.
    pub(crate) fn begin_text(&mut self) {
        let memory = self.text_memory.get();
        self.text = Some(TextTool::new(memory.as_ref(), current_unit()));
    }

    /// Stellt die Art um. Auf dem Pfad wartet das Werkzeug auf einen Klick auf eine Kurve, ein
    /// gezogener Rahmen bleibt nur Vorschau (Spec C2).
    pub fn set_text_kind(&mut self, kind: TextToolKind) {
        if let Some(t) = self.text.as_mut() {
            t.kind = kind;
        }
    }

    /// Bringt die Vorschau auf den Stand `sk` (Spec C6; die Oberfläche ruft es einmal je Bild).
    /// Gesetzt wird nur neu, wenn sich Inhalt oder Stil geändert haben, gelegt nur, wenn sich dazu
    /// Lage, Werte oder der Stand der Skizze geändert haben; ruhige Bilder tun nichts. Ein
    /// gelöschter Pfad gilt als nicht gewählt. Ohne Schriftliste (sie wird noch gelesen) wird
    /// nichts gesetzt. `true`, wenn sich die gelegte Vorschau geändert hat: Die Ansicht hat dieses
    /// Bild schon gezeichnet und braucht ein weiteres.
    pub fn update_text_preview(&mut self, sk: &Sketch, fonts: Option<&FontLibrary>) -> bool {
        let Some(t) = self.text.as_mut() else {
            return false;
        };
        if t.path.is_some_and(|p| sk.curve(p).is_none()) {
            t.path = None;
        }
        if let Some(lib) = fonts {
            t.resolve_font(lib);
            let key = (t.shaping_content(), t.params.style.clone());
            if t.shaped_for.as_ref() != Some(&key) {
                let shaped = shape_now(lib, &key.0, &key.1);
                t.shape_error = shaped.as_ref().err().copied();
                t.preview = shaped.ok();
                t.shaped_for = Some(key);
                t.shapings += 1;
            }
        }
        let Some(key) = t.place_key(sk) else {
            let shown = t.placed.as_ref().is_some_and(|(_, p)| p.is_some());
            t.placed = None;
            return shown;
        };
        if t.placed.as_ref().is_some_and(|(k, _)| *k == key) {
            return false;
        }
        let placed = t.place(sk).map(Arc::new);
        t.placements += 1;
        let changed = t
            .placed
            .as_ref()
            .is_none_or(|(_, old)| old.is_some() || placed.is_some());
        t.placed = Some((key, placed));
        changed
    }

    /// Öffnet den Text `t` zum Ändern (Spec C8: Doppelklick, Knopf „Text bearbeiten“, Aktion):
    /// das Werkzeug „Text“ mit den Werten des Textes. Ein laufendes Werkzeug bricht ab, die
    /// Auswahl bleibt. `false` für einen unbekannten Text.
    pub fn begin_text_edit(&mut self, sk: &mut Sketch, t: TextId) -> bool {
        let memory = self.text_memory.get();
        let Some(mut tool) = TextTool::for_edit(sk, t, memory.as_ref(), current_unit()) else {
            return false;
        };
        tool.selection_before = Some(self.selection.clone());
        self.set_tool(sk, Tool::Text);
        self.message = None;
        self.text = Some(tool);
        true
    }

    /// Die gewählten Texte (Spec B9, C9): jeder Text, von dem ein Buchstabe gewählt ist, einmal,
    /// in der Reihenfolge der Auswahl. Rahmenlinien und Ecken allein zählen nicht.
    ///
    /// Das läuft in jedem Bild, auch wenn ein großer Text ganz gewählt ist: ein Durchgang über die
    /// Auswahl und einer über die Teile der Texte, statt je gewähltem Eintrag alle Texte zu
    /// durchsuchen.
    pub fn selected_texts(&self, sk: &Sketch) -> Vec<TextId> {
        if self.selection.is_empty() || sk.texts().next().is_none() {
            return Vec::new();
        }
        // Erste Stelle jedes Eintrags in der Auswahl.
        let mut first: HashMap<Item, usize> = HashMap::with_capacity(self.selection.len());
        for (i, item) in self.selection.iter().enumerate() {
            first.entry(*item).or_insert(i);
        }
        let mut found: Vec<(usize, TextId)> = sk
            .texts()
            .filter_map(|(t, d)| {
                d.curves
                    .iter()
                    .map(|&c| Item::Curve(c))
                    .chain(d.points.iter().map(|&p| Item::Point(p)))
                    .filter_map(|item| first.get(&item).copied())
                    .min()
                    .map(|i| (i, t))
            })
            .collect();
        // Kein Eintrag gehört zu zwei Texten: Die Stellen sind verschieden.
        found.sort_unstable_by_key(|&(i, _)| i);
        found.into_iter().map(|(_, t)| t).collect()
    }

    /// „Text bearbeiten“ (Knopf und Aktion): öffnet den ersten gewählten Text; ohne gewählten
    /// Text eine Meldung.
    pub fn edit_selected_text(&mut self, sk: &mut Sketch) {
        match self.selected_texts(sk).first() {
            Some(&t) => {
                self.begin_text_edit(sk, t);
            }
            None => self.message = Some(crate::msg!("sketch-text-select-first")),
        }
    }

    /// „Text auflösen“ (Spec C9, F): alle gewählten Texte als **ein** Rückgängig-Schritt („Text
    /// auflösen“). Die Kurven bleiben gewählt. Solange die Schriftliste gelesen wird, wartet der
    /// Befehl mit Meldung (Spec G4); ohne gewählten Text ebenso eine Meldung.
    pub fn explode_selected_texts(
        &mut self,
        sk: &mut Sketch,
        fonts: Option<&FontLibrary>,
    ) -> Result<(), String> {
        let refuse = |se: &mut SketchSession, m: Msg| {
            let text = m.text();
            se.message = Some(m);
            Err(text)
        };
        if fonts.is_none() {
            return refuse(self, crate::msg!("sketch-text-fonts-loading"));
        }
        let texts = self.selected_texts(sk);
        if texts.is_empty() {
            return refuse(self, crate::msg!("sketch-text-select-first"));
        }
        if self.runs_text() {
            self.set_tool(sk, Tool::Select);
        }
        self.atomic(sk, |se, sk, prior| {
            sk.explode_texts(&texts).map_err(err)?;
            se.solve_checked(sk, prior)?;
            se.edit_label = Some(crate::msg!("undo-sketch-text-explode"));
            Ok(())
        })?;
        // Rahmenlinien und Ecken sind fort.
        self.forget_removed(sk);
        Ok(())
    }

    /// Der Text, der gerade geändert wird (Abschnitt „Text“ offen zum Ändern).
    pub fn edited_text(&self) -> Option<TextId> {
        self.text
            .as_ref()
            .filter(|_| self.runs_text())
            .and_then(|t| t.editing)
    }

    /// Kurven, die die Ansicht beim Ändern nicht zeichnet (an ihrer Stelle steht nur die
    /// Vorschau, Handtest B2): die Buchstaben des Textes und sein Rahmen. Die Vorschau zeichnet
    /// ihren eigenen Rahmen; der alte bliebe sonst (gewählt sogar hervorgehoben) an der alten Lage
    /// stehen. Punkte von Buchstaben zeichnet die Ansicht ohnehin nie, die Ecken des Rahmens nennt
    /// [`SketchSession::text_hidden_points`].
    pub fn text_hidden(&self, sk: &Sketch) -> HashSet<CurveId> {
        let Some(d) = self.edited_text().and_then(|t| sk.text(t)) else {
            return Default::default();
        };
        let mut curves: HashSet<CurveId> = d.curves.iter().copied().collect();
        if let TextPlacement::Box { lines, .. } = d.placement {
            curves.extend(lines);
        }
        curves
    }

    /// Punkte, die die Ansicht beim Ändern nicht zeichnet: die Ecken des Rahmens (siehe
    /// [`SketchSession::text_hidden`]).
    pub fn text_hidden_points(&self, sk: &Sketch) -> Vec<PointId> {
        match self
            .edited_text()
            .and_then(|t| sk.text(t))
            .map(|d| d.placement)
        {
            Some(TextPlacement::Box { corners, .. }) => corners.to_vec(),
            _ => Vec::new(),
        }
    }

    /// Die zuletzt gelegte Vorschau, ohne neu zu rechnen (zum Zeichnen in jedem Bild).
    pub fn placed_text_preview(&self) -> Option<Arc<Vec<Contour>>> {
        let t = self.text.as_ref().filter(|_| self.runs_text())?;
        t.placed.as_ref()?.1.clone()
    }

    /// Die Konturen der Vorschau in Skizzenkoordinaten, mit derselben Lage wie das Anlegen; `None`,
    /// solange es nichts zu zeigen gibt (kein Text, kein Rahmen, kein Pfad, Pfad ohne Länge). Passt
    /// die gemerkte Lage zum Stand `sk`, kommt sie ohne Rechnen zurück.
    pub fn text_preview(&self, sk: &Sketch) -> Option<Arc<Vec<Contour>>> {
        let t = self.text.as_ref().filter(|_| self.runs_text())?;
        let key = t.place_key(sk)?;
        match &t.placed {
            Some((k, placed)) if *k == key => placed.clone(),
            _ => t.place(sk).map(Arc::new),
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
        // Nur auf dem Pfad sucht ein Klick eine Kurve.
        let picked = self
            .text
            .as_ref()
            .is_some_and(|t| t.kind == TextToolKind::Path)
            .then(|| self.shown_curve(sk, at, radius))
            .flatten();
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
                // „Text auf Pfad“ gibt es nur in 2D (Spec Entwurf 4).
                if sk.is_3d_curve(c) {
                    return Err(crate::msg!("sketch-err-text-path-3d"));
                }
                t.path = Some(c);
            }
        }
        Ok(())
    }

    /// Zieht das Werkzeug gerade einen Rahmen in 3D auf (Skizzen 10)? In einer 3D-Skizze mit der
    /// Art „Text“, solange der Rahmen in 3D begonnen ist oder noch keine Ecke hat und die aktive
    /// Ebene der Triade nicht in der Skizzenebene liegt (dann zeichnet es wie in 2D, mit Fang).
    /// „Text auf Pfad“ bleibt 2D.
    pub fn text_draws_in_3d(&self, sk: &Sketch) -> bool {
        let flat =
            || Plane3::from_triad(&self.triad, self.triad.origin, sk.tolerance().linear).flat;
        sk.three_d
            && self.tool == Tool::Text
            && self.text.as_ref().is_some_and(|t| {
                t.placing_frame()
                    && (t.plane3.is_some()
                        || (t.corners.is_empty() && !self.triad.is_sketch_plane() && !flat()))
            })
    }

    /// Klick des Werkzeugs beim Rahmen in 3D an der Stelle `cur` (Spec Entwurf 4): Die erste Ecke
    /// legt die Ebene fest ([`text_plane`]), die zweite wird in diese Ebene gelegt. Die Skizze
    /// bleibt unverändert; erst OK legt den Text an.
    pub(crate) fn click_text3(&mut self, sk: &Sketch, cur: &Cursor3) -> Result<(), Msg> {
        let tol = sk.tolerance().linear;
        let fresh = text_plane(&self.triad, cur.at);
        let Some(t) = self.text.as_mut().filter(|t| t.placing_frame()) else {
            return Ok(());
        };
        let plane = t.plane3.unwrap_or(fresh);
        let q = plane.to2(cur.at);
        if let Some(&first) = t.corners.first() {
            let size = (q - first).abs();
            if size.x <= tol || size.y <= tol {
                return Err(crate::msg!("sketch-err-rect-size"));
            }
        }
        t.corners.push(q);
        t.plane3 = Some(plane);
        // Der Fang zählt nur, wenn die Ecke in der Ebene genau auf ihm liegt.
        let at = plane.to3(q);
        let on = cur.at.distance(at) <= tol;
        t.snaps3.push(Hit3 {
            at,
            snap: cur.snap.filter(|_| on),
            origin: cur.origin && on,
            mid: cur.mid.filter(|_| on),
        });
        if t.corners.len() == 2 {
            t.focus_content = true;
        }
        // Die Triade springt auf die Ecke; ihre Ebene bleibt dieselbe.
        self.triad.moved_to(plane.to3(q));
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
            TextToolKind::Path if t.path.is_none_or(|p| sf.sketch.curve(p).is_none()) => {
                return refuse(self, crate::msg!("sketch-text-need-path"));
            }
            TextToolKind::Box if t.editing.is_none() && t.corners.len() < 2 => {
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
        if let Some(id) = t.editing {
            return self.confirm_text_edit(sf, &t, id, shape);
        }
        let tool = t.clone();
        self.atomic(&mut sf.sketch, |se, sk, prior| {
            let params = tool.params.clone();
            match tool.kind {
                // Ein Rahmen in 3D (Skizzen 10): in seiner Ebene; gefangene Ecken binden wie
                // in 2D.
                TextToolKind::Box if tool.plane3.is_some() => {
                    if let Some(plane) = tool.plane3 {
                        let corners = [plane.to3(tool.corners[0]), plane.to3(tool.corners[1])];
                        let id = sk
                            .add_text_box3(corners, plane.normal(), params, tool.h, tool.v, shape)
                            .map_err(err)?;
                        let own = match sk.text(id).map(|d| d.placement) {
                            Some(TextPlacement::Box { corners, .. }) => corners.to_vec(),
                            _ => Vec::new(),
                        };
                        bind_corners3(sk, &own, &tool.snaps3)?;
                        if tool.angle != 0.0 {
                            sk.set_text_angle(id, tool.angle).map_err(err)?;
                        }
                    }
                }
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
        self.text_memory.set(TextMemory::of(&t));
        let note = self.message.take();
        self.selection.clear();
        self.drop_curve_picks();
        self.set_tool(&mut sf.sketch, Tool::Select);
        self.message = note;
        Ok(())
    }

    /// OK beim Ändern (Spec C8): ändert den Text `id` nach `tool` als **ein** Rückgängig-Schritt
    /// („Text ändern“); ein OK ohne Änderung ist keiner. Abgelehnt bleibt alles, wie es war, und
    /// der Abschnitt offen. Die gemerkten Werte für neue Texte bleiben (**Annahme**).
    fn confirm_text_edit(
        &mut self,
        sf: &mut SketchFeature,
        tool: &TextTool,
        id: TextId,
        shape: Arc<TextShape>,
    ) -> Result<(), String> {
        let untouched = sf.sketch.to_data();
        self.atomic(&mut sf.sketch, |se, sk, prior| {
            tool.apply_edit(sk, id, shape).map_err(err)?;
            se.solve_checked(sk, prior)?;
            se.edit_label = Some(crate::msg!("undo-sketch-text-edit"));
            Ok(())
        })?;
        if sf.sketch.to_data() == untouched {
            self.edit_label = None;
        }
        let note = self.message.take();
        self.selection.clear();
        self.drop_curve_picks();
        self.set_tool(&mut sf.sketch, Tool::Select);
        self.message = note;
        Ok(())
    }
}

/// Bindet die Ecken `own` eines neuen Rahmens in 3D an den Fang der geklickten Ecken `hits`
/// (Skizzen 10): an einen gefangenen Punkt mit `Coincident`, sonst wie ein neuer Punkt
/// ([`bind_new3`]: Ursprung, Mitte). Eine Ecke, die nicht an der Lage des Klicks liegt, bindet nicht.
fn bind_corners3(sk: &mut Sketch, own: &[PointId], hits: &[Hit3]) -> Result<(), Msg> {
    let tol = sk.tolerance().linear;
    for hit in hits {
        let Some(p) = own
            .iter()
            .copied()
            .find(|&p| sk.point_xyz(p).is_ok_and(|q| q.distance(hit.at) <= tol))
        else {
            continue;
        };
        match hit.snap {
            Some(q) if q != p => {
                sk.add_constraint(ConstraintKind::Coincident(p, q))
                    .map_err(err)?;
            }
            Some(_) => {}
            None => bind_new3(sk, p, hit.origin, hit.mid)?,
        }
    }
    Ok(())
}

/// Der Text, zu dem `item` gehört: ein Buchstabe oder ein Punkt eines Buchstabens. Rahmenlinien
/// und Ecken gehören nicht dazu (sie sind einzeln wählbar, Spec B9).
pub(crate) fn text_of_item(sk: &Sketch, item: Item) -> Option<TextId> {
    match item {
        Item::Curve(c) => sk.text_of_curve(c),
        Item::Point(p) => sk
            .texts()
            .find(|(_, d)| d.points.contains(&p))
            .map(|(t, _)| t),
        Item::Constraint(_) => None,
    }
}

/// Die Buchstaben des Textes `t` als Auswahl (Spec B9: ein Klick wählt den ganzen Text).
pub(crate) fn text_items(sk: &Sketch, t: TextId) -> Vec<Item> {
    sk.text(t)
        .map(|d| d.curves.iter().map(|&c| Item::Curve(c)).collect())
        .unwrap_or_default()
}

/// Die Hinweise zur gesetzten Form (Spec C10): Ersatzschrift, nachgebildetes Fett oder Kursiv,
/// fehlende Zeichen, je eine Zeile.
pub fn shape_hints(shape: &TextShape) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(font) = &shape.substituted {
        out.push(tr!("sketch-text-hint-substituted", font = font.clone()));
    }
    if shape.synthetic {
        out.push(tr!("sketch-text-hint-synthetic"));
    }
    if shape.missing_chars {
        out.push(tr!("sketch-text-hint-missing-chars"));
    }
    out
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
            .char_limit(MAX_TEXT_CHARS)
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
    // Pfad: Der Hinweis bricht in der Spaltenbreite um (Handtest B3); eine Zeile ohne Umbruch
    // machte den Inhalt des Skizzenfelds breiter als die Spalte, rechts wurde dann alles
    // abgeschnitten.
    if t.kind == TextToolKind::Path {
        ui.horizontal_wrapped(|ui| {
            ui.label(tr!("sketch-text-path"));
            // Ein gelöschter Pfad gilt als nicht gewählt.
            let state = if t.path.is_some_and(|p| sf.sketch.curve(p).is_some()) {
                tr!("sketch-text-path-chosen")
            } else {
                tr!("sketch-text-path-none")
            };
            ui.add(egui::Label::new(RichText::new(state).weak()).wrap());
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
    // Hinweise zur Schrift (Spec C10): aus der letzten Vorschau, je eine Zeile.
    for hint in t.preview.as_deref().map(shape_hints).unwrap_or_default() {
        ui.label(RichText::new(hint).small().weak());
    }
    // Ein Text, der sich beim Neusetzen nicht setzen ließ, behält seine Kurven (wie die Warnung
    // im Verlauf).
    if t.editing
        .and_then(|id| sf.sketch.text(id))
        .is_some_and(|d| d.is_unset())
    {
        ui.label(RichText::new(tr!("sketch-text-hint-unset")).small().weak());
    }
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
    // Die Meldung der Sitzung direkt unter OK und Abbrechen (nicht unten im Skizzenfeld).
    if let Some(m) = se.message.as_ref() {
        ui.colored_label(error, m.text());
    }
    if out.cancel {
        se.cancel(&mut sf.sketch);
        ui.ctx().request_repaint();
    } else if out.ok {
        // Fehler stehen in der Meldung der Sitzung; das nächste Bild zeigt sie.
        let _ = se.confirm_text(sf, fonts);
        ui.ctx().request_repaint();
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
