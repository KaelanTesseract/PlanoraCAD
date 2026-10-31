//! Werkzeug „Schnitt“ (Teilprojekt 3d, Aufgabe 9): Schnittlinie mit zwei Klicks in einer geraden
//! Ansicht, daraus eine Schnittansicht; dazu die Änderungen im Ansichtsfeld (Lage der Ebene,
//! Blickrichtung, Schraffur).
//!
//! **Ablauf:** der erste Klick liegt in der Hüllbox einer geraden Ansicht ohne Schnitt (sonst
//! [`not_from_here`]) und merkt Ansicht und Punkt (in Ansichtskoordinaten). Der zweite Klick legt
//! die Richtung fest ([`snap`]: waagerecht, wenn |dx| ≥ |dy|); die quer liegende Koordinate ist die
//! des ersten Klicks und rastet auf die Mitte der Hüllbox ein, wenn sie auf dem Bildschirm näher
//! als [`SNAP_PX`] daran liegt ([`line_for`]). [`create`] ist genau eine Änderung des Dokuments.
//!
//! **Lage der neuen Ansicht (S12):** abhängig von der Quellansicht mit deren Maßstab, im
//! Vorgabeabstand; liegen dort schon Ansichten, dahinter. Ragt sie so auf Schriftfeld, Tabellen
//! oder über den Rahmen, auf die Gegenseite, enger heran oder mit rückender Ausgangsgruppe
//! ([`section_spot`]).

use crate::dimension_state::{no_drawing, no_view, views_pending};
use crate::dimension_tool::sentence;
use crate::drawing_dialog::sketch_blocked;
use crate::drawing_state::{default_distance, estimate_view_size};
use crate::drawing_view;
use crate::msg::Msg;
use crate::sketch_view::parse_number;
use crate::state::AppState;
use egui::Pos2;
use planora_drawing::annotate::{
    Mark, MarkKind, SECTION_ARROW_LEN, SECTION_OVERHANG_MM, SECTION_SHAFT_MM, SECTION_TITLE_GAP,
    SECTION_TITLE_H, SectionPen,
};
use planora_drawing::sheet::{SheetTransform, view_transform};
use planora_drawing::sheet_frame::sheet_scale;
use planora_drawing::{
    Basis, Drawing, DrawingId, DrawingKind, GAP_MM, MAX_SECTIONS, Placement, ViewId, ViewKind,
    ViewLines, ViewSpec, align_dir, plane_offset_from_view, section_kind, section_line_coord,
};
use planora_feature::FeatureError;
use planora_i18n::tr;
use planora_math::DVec2;
use std::collections::BTreeMap;

/// Beschriftung des Schalters.
pub fn tool() -> String {
    tr!("section-tool")
}
/// Tooltip des Schalters.
pub fn tool_tip() -> String {
    tr!("section-tool-tip")
}
/// Hinweis vor dem ersten Klick.
pub fn hint_first() -> String {
    tr!("section-hint-first")
}
/// Hinweis vor dem zweiten Klick.
pub fn hint_second() -> String {
    tr!("section-hint-second")
}
/// Ablehnung: der Schnitt geht nicht aus dieser Ansicht.
pub fn not_from_here() -> Msg {
    crate::msg!("section-not-from-here")
}
/// Ablehnung: beide Klicks liegen (fast) auf demselben Punkt.
pub fn too_short() -> Msg {
    crate::msg!("section-too-short")
}
/// Ablehnung: zu viele Schnitte auf dem Blatt.
pub fn full() -> Msg {
    crate::msg!("section-full", max = MAX_SECTIONS)
}
/// Hinweis im Ansichtsfeld, wenn die Ebene verschoben werden musste.
pub fn nudged() -> String {
    tr!("section-nudged")
}
/// Fangweite der Mitte der Hüllbox in Bildschirmpunkten.
pub const SNAP_PX: f32 = 8.0;
/// Erklärung am gesperrten Schalter in einer alten Version.
pub fn read_only_tip() -> String {
    tr!("section-read-only-tip")
}
/// Erklärung am gesperrten Schalter eines Stücklistenblatts.
pub fn parts_sheet_tip() -> String {
    tr!("section-parts-sheet-tip")
}
/// Erklärung, wenn es noch keine gerade Ansicht gibt.
pub fn no_straight_tip() -> String {
    tr!("section-no-straight-tip")
}
/// Tooltip des gesperrten Maßstabs einer Schnittansicht.
pub fn scale_locked_tip() -> String {
    tr!("section-scale-locked-tip")
}
/// Meldung bei einer Eingabe, die keine Zahl ist.
pub fn not_a_number() -> Msg {
    crate::msg!("section-not-a-number")
}
/// Mindestabstand der beiden Klicks auf dem Blatt in Millimetern.
pub const MIN_LENGTH_MM: f64 = 1.0;

/// Schlüssel einer Meldung: Stand des Dokuments und gewählte Ansicht; ändert sich einer, verfällt
/// die Meldung.
pub type MessageKey = (u64, Option<ViewId>);

/// Zustand des Werkzeugs „Schnitt“.
#[derive(Debug, Default)]
pub struct SectionTool {
    active: bool,
    /// Erster Klick: Ansicht und Punkt in Ansichtskoordinaten.
    start: Option<(ViewId, DVec2)>,
    message: Option<Msg>,
    message_key: Option<MessageKey>,
}

impl SectionTool {
    /// Ist das Werkzeug eingeschaltet?
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Schaltet das Werkzeug ein oder aus; der erste Klick und die Meldung verfallen dabei.
    pub fn set_active(&mut self, on: bool) {
        self.active = on;
        self.start = None;
        self.message = None;
        self.message_key = None;
    }

    /// Der gemerkte erste Klick (Ansicht und Punkt in Ansichtskoordinaten).
    pub fn start(&self) -> Option<(ViewId, DVec2)> {
        self.start
    }

    /// Verwirft den ersten Klick; `true`, wenn es einen gab.
    pub fn cancel(&mut self) -> bool {
        self.start.take().is_some()
    }

    /// Letzte Ablehnung (rot in der Hinweiszeile) in der aktuellen Sprache.
    pub fn message(&self) -> Option<String> {
        self.message.as_ref().map(Msg::text)
    }

    /// Setzt die Meldung für die Hinweiszeile.
    pub fn set_message(&mut self, message: Option<Msg>) {
        self.message = message;
        self.message_key = None;
    }

    /// Lässt die Meldung verfallen, sobald sich der Stand `key` gegenüber dem Bild ihres
    /// Entstehens ändert.
    pub fn expire(&mut self, key: MessageKey) {
        if self.message.is_none() {
            return;
        }
        match self.message_key {
            None => self.message_key = Some(key),
            Some(k) if k != key => {
                self.message = None;
                self.message_key = None;
            }
            Some(_) => {}
        }
    }

    /// Hinweis zur Bedienung bei eingeschaltetem Werkzeug.
    pub fn hint(&self) -> Option<String> {
        self.active.then(|| {
            if self.start.is_some() {
                hint_second()
            } else {
                hint_first()
            }
        })
    }
}

/// Kann das Werkzeug benutzt werden? Sonst die Erklärung (alte Version, Stücklistenblatt, keine
/// gerade Ansicht ohne Schnitt, gewählte Ansicht ist Isometrie oder selbst ein Schnitt, schon 12
/// Schnitte).
pub fn blocked(drawing: &Drawing, selected: Option<&ViewSpec>, editable: bool) -> Option<String> {
    blocked_msg(drawing, selected, editable).map(|m| m.text())
}

/// Wie [`blocked`], als Meldung (übersetzt erst beim Zeichnen).
pub fn blocked_msg(drawing: &Drawing, selected: Option<&ViewSpec>, editable: bool) -> Option<Msg> {
    if !editable {
        Some(crate::msg!("section-read-only-tip"))
    } else if matches!(drawing.kind, DrawingKind::PartsList { .. }) {
        Some(crate::msg!("section-parts-sheet-tip"))
    } else if !drawing
        .views
        .iter()
        .any(|v| v.kind.is_normal() && v.section.is_none())
    {
        Some(crate::msg!("section-no-straight-tip"))
    } else if selected.is_some_and(|v| !v.kind.is_normal() || v.section.is_some()) {
        Some(not_from_here())
    } else if drawing.views.iter().filter(|v| v.section.is_some()).count() >= MAX_SECTIONS {
        Some(full())
    } else {
        None
    }
}

/// Ist die Schnittlinie waagerecht? Ja, wenn |dx| ≥ |dy|.
pub fn snap(start: DVec2, end: DVec2) -> bool {
    let d = end - start;
    d.x.abs() >= d.y.abs()
}

/// Richtung und quer liegende Koordinate der Schnittlinie in Ansichtskoordinaten: die Koordinate
/// des Anfangs quer zur Richtung, auf die Mitte der Hüllbox `bbox` eingerastet, wenn sie bei
/// `px_per_unit` Bildschirmpunkten je Einheit näher als [`SNAP_PX`] daran liegt.
pub fn line_for(
    start: DVec2,
    end: DVec2,
    bbox: ((f64, f64), (f64, f64)),
    px_per_unit: f64,
) -> (bool, f64) {
    let horizontal = snap(start, end);
    let ((x0, y0), (x1, y1)) = bbox;
    let (coord, mid) = if horizontal {
        (start.y, 0.5 * (y0 + y1))
    } else {
        (start.x, 0.5 * (x0 + x1))
    };
    let near = (coord - mid).abs() * px_per_unit < f64::from(SNAP_PX);
    (horizontal, if near { mid } else { coord })
}

/// Ansichtskoordinaten des Blattpunkts `at`.
pub fn to_view(xf: &SheetTransform, at: DVec2) -> DVec2 {
    let s = if xf.scale.is_normal() { xf.scale } else { 1.0 };
    (at - DVec2::new(xf.tx, xf.ty)) / s + xf.bbox_center
}

fn feature_text(e: FeatureError) -> Msg {
    match e {
        FeatureError::InvalidInput(m) => Msg::Raw(sentence(&m)),
        e => Msg::Raw(sentence(&e.to_string())),
    }
}

/// Lage einer neuen Schnittansicht: Abstand zur Quellansicht und, wenn die Ansichtsgruppe dafür
/// rücken muss, die neue Mitte ihrer frei liegenden Wurzelansicht.
struct Spot {
    distance: f64,
    root_at: Option<(ViewId, DVec2)>,
}

/// Lage der neuen Schnittansicht `kind` aus `parent` (S12): auf der Ausrichtungsachse im
/// Vorgabeabstand, hinter Ansichten, die dort schon liegen. Ragt sie so über den Rahmen oder auf
/// Schriftfeld, Änderungstabelle, Hinweisfeld oder Stückliste, kommt sie auf die Gegenseite der
/// Quellansicht (negativer Abstand); passt sie auf keiner Seite, wird die Lücke zwischen den
/// Ansichten von [`GAP_MM`] auf die Hälfte enger. Passt sie auch dann nicht, rückt die Gruppe
/// der Quellansicht (ihre frei liegende Wurzelansicht mit allem, was daran hängt) längs der Achse
/// so wenig wie nötig, dass alles im freien Bereich liegt. Geht auch das nicht, die Lage, in der
/// sie am wenigsten verdeckt (bei Gleichstand die übliche). „Passt“ heißt: mit Abstand
/// [`SECTION_TITLE_GAP`] zu Rahmen und Tabellen, samt Titel unter der Schnittansicht.
fn section_spot(state: &AppState, d: &Drawing, parent: &ViewSpec, kind: ViewKind) -> Spot {
    let base = default_distance(&state.bodies, parent.kind, kind, parent.scale, parent.scale);
    let fixed = |distance: f64| Spot {
        distance,
        root_at: None,
    };
    let Some((ax, ay)) = align_dir(d.projection, parent.kind, kind) else {
        return fixed(base);
    };
    let dir = DVec2::new(ax, ay);
    let Some(centre) = drawing_view::center_of(d, parent.id, d.views.len()) else {
        return fixed(base);
    };
    let half = |k: ViewKind, f: f64| {
        let (w, h) = estimate_view_size(&state.bodies, k);
        DVec2::new(w, h) * f / 2.0
    };
    let size = half(kind, parent.scale.factor());
    // Frei liegende Wurzel der Gruppe einer Ansicht; Ansichten, die (über ihre Eltern) an ihr
    // hängen, rücken mit ihr.
    let root_of = |v: &ViewSpec| {
        let mut v = v.clone();
        for _ in 0..d.views.len() {
            match (v.placement, v.parent.and_then(|p| d.view(p))) {
                (Placement::Aligned { .. }, Some(p)) => v = p.clone(),
                _ => break,
            }
        }
        v
    };
    let root = root_of(parent);
    // Geschätzte Blattflächen der vorhandenen Ansichten; `true` für die Gruppe der Quellansicht.
    let others: Vec<((DVec2, DVec2), bool)> = d
        .views
        .iter()
        .filter_map(|v| {
            let c = drawing_view::center_of(d, v.id, d.views.len())?;
            let h = half(v.kind, v.scale.factor());
            Some(((c - h, c + h), root_of(v).id == root.id))
        })
        .collect();
    let rect = |dist: f64| {
        let c = centre + dir * dist;
        (c - size, c + size)
    };
    let hit = |a: (DVec2, DVec2), b: &(DVec2, DVec2)| {
        a.0.x < b.1.x && b.0.x < a.1.x && a.0.y < b.1.y && b.0.y < a.1.y
    };
    // Auf einer Seite (`sign`) mit der Lücke `gap` hinter alle Ansichten rücken, die im Weg
    // liegen.
    let past = |sign: f64, gap: f64| {
        let mut dist = sign * (base - GAP_MM + gap);
        for _ in 0..=others.len() {
            let r = rect(dist);
            let Some(far) = others
                .iter()
                .filter(|(o, _)| hit(r, o))
                .map(|(o, _)| {
                    // Fernste Kante der Ansicht im Weg, gemessen längs `sign · dir` ab `centre`.
                    let along = |p: DVec2| (p - centre).dot(dir) * sign;
                    along(o.0).max(along(o.1))
                })
                .reduce(f64::max)
            else {
                break;
            };
            dist = sign * (far + gap + size.dot(dir.abs()));
        }
        dist
    };
    let layout = drawing_view::placement_layout(state, d);
    let blocked: Vec<(DVec2, DVec2)> = [Some(layout.title), Some(layout.revisions)]
        .into_iter()
        .chain([layout.notes, layout.parts])
        .flatten()
        .collect();
    // Fläche über dem Rahmen hinaus und auf Schriftfeld, Änderungstabelle, Hinweisfeld oder
    // Stückliste.
    let outside = |r: (DVec2, DVec2)| {
        let area = |a: DVec2, b: DVec2| (b.x - a.x).max(0.0) * (b.y - a.y).max(0.0);
        let cut = |b: &(DVec2, DVec2)| area(r.0.max(b.0), r.1.min(b.1));
        area(r.0, r.1) - cut(&layout.frame) + blocked.iter().map(cut).sum::<f64>()
    };
    // Platz, den eine Ansicht braucht: ihre Fläche mit Abstand [`SECTION_TITLE_GAP`] ringsum,
    // bei der neuen Schnittansicht dazu ihr Titel darunter.
    let f = layout.scale;
    let clear = SECTION_TITLE_GAP * f;
    let pad = |r: (DVec2, DVec2), below: f64| {
        (
            r.0 - DVec2::new(clear, clear + below),
            r.1 + DVec2::splat(clear),
        )
    };
    let caption = (SECTION_TITLE_GAP + SECTION_TITLE_H) * f;
    let room = |dist: f64| pad(rect(dist), caption);
    let candidates: Vec<f64> = [GAP_MM, GAP_MM / 2.0]
        .into_iter()
        .flat_map(|gap| [past(1.0, gap), past(-1.0, gap)])
        .collect();
    // 1. Ohne Rücken.
    let clean =
        |dist: f64| outside(room(dist)) <= 1e-9 && !others.iter().any(|(o, _)| hit(rect(dist), o));
    if let Some(&dist) = candidates.iter().find(|&&dist| clean(dist)) {
        return fixed(dist);
    }
    // 2. Die Gruppe rückt längs der Achse in den freien Bereich.
    if let Placement::Free { x, y } = root.placement {
        let axis = dir.abs();
        let (flo, fhi) = (layout.free.0.dot(axis), layout.free.1.dot(axis));
        for &dist in &candidates {
            // (Fläche, Platz) der Gruppe samt neuer Ansicht.
            let group: Vec<((DVec2, DVec2), (DVec2, DVec2))> = others
                .iter()
                .filter(|(_, mine)| *mine)
                .map(|(o, _)| (*o, pad(*o, 0.0)))
                .chain([(rect(dist), room(dist))])
                .collect();
            let lo = group
                .iter()
                .map(|(_, r)| r.0.dot(axis))
                .fold(f64::INFINITY, f64::min);
            let hi = group
                .iter()
                .map(|(_, r)| r.1.dot(axis))
                .fold(f64::NEG_INFINITY, f64::max);
            if hi - lo > fhi - flo {
                continue;
            }
            let t = 0.0_f64.clamp(flo - lo, fhi - hi);
            let moved = |r: (DVec2, DVec2)| (r.0 + axis * t, r.1 + axis * t);
            let fits = group.iter().all(|(view, space)| {
                outside(moved(*space)) <= 1e-9
                    && !others
                        .iter()
                        .any(|(o, mine)| !*mine && hit(moved(*view), o))
            });
            if fits {
                return Spot {
                    distance: dist,
                    root_at: Some((root.id, DVec2::new(x, y) + axis * t)),
                };
            }
        }
    }
    // 3. Am wenigsten verdeckt.
    let mut best = (f64::INFINITY, base);
    for &dist in &candidates {
        let o = outside(room(dist));
        if o < best.0 - 1e-9 {
            best = (o, dist);
        }
    }
    fixed(best.1)
}

/// Legt eine Schnittansicht aus `parent` an: Schnittlinie `horizontal` bei der Ansichtskoordinate
/// `coord`, Blickrichtung nach S5. Eine Änderung des Dokuments; bei einem Fehler bleibt es
/// unverändert, und der Grund kommt als Satz zurück. Die neue Ansicht ist danach gewählt.
pub fn create(
    state: &mut AppState,
    drawing: DrawingId,
    parent: ViewId,
    horizontal: bool,
    coord: f64,
) -> Result<ViewId, Msg> {
    if state.sketch_session.is_some() {
        return Err(sketch_blocked());
    }
    let d = state.document.drawing(drawing).ok_or_else(no_drawing)?;
    let source = d.view(parent).ok_or_else(no_view)?;
    if let Some(why) = blocked_msg(d, Some(source), true) {
        return Err(why);
    }
    if !coord.is_finite() {
        return Err(too_short());
    }
    let kind = section_kind(source.kind, horizontal, false).ok_or_else(not_from_here)?;
    let offset = plane_offset_from_view(source.kind, kind, coord).ok_or_else(not_from_here)?;
    let spot = section_spot(state, d, source, kind);
    let mut made = None;
    state
        .document
        .update_drawing(drawing, |dr| {
            // Rücken der Gruppe und neue Ansicht in einem Schritt.
            if let Some((root, at)) = spot.root_at
                && let Some(v) = dr.views.iter_mut().find(|v| v.id == root)
            {
                v.placement = Placement::Free { x: at.x, y: at.y };
            }
            made = Some(dr.add_section(parent, kind, offset, spot.distance));
        })
        .map_err(feature_text)?;
    let id = made
        .ok_or_else(no_drawing)?
        .map_err(|m| Msg::Raw(sentence(&m)))?;
    let ui = &mut state.drawing_ui;
    ui.selected_view = Some(id);
    ui.selected_dim = None;
    ui.selected_balloon = None;
    Ok(id)
}

/// Dreht die Blickrichtung der Schnittansicht `view` um (Gegenansicht, `offset` bleibt). Eine
/// Änderung des Dokuments.
pub fn flip(state: &mut AppState, drawing: DrawingId, view: ViewId) -> Result<(), Msg> {
    let d = state.document.drawing(drawing).ok_or_else(no_drawing)?;
    let v = d.view(view).ok_or_else(no_view)?;
    if v.section.is_none() {
        return Err(not_from_here());
    }
    let opposite = ViewKind::ALL
        .into_iter()
        .filter(|k| k.is_normal())
        .find(|k| (Basis::of(*k).d + Basis::of(v.kind).d).length() < 1e-9)
        .ok_or_else(not_from_here)?;
    edit(state, drawing, view, |v| v.kind = opposite)
}

/// Ändert die Schnittansicht `view` in einer Änderung des Dokuments.
fn edit(
    state: &mut AppState,
    drawing: DrawingId,
    view: ViewId,
    f: impl FnOnce(&mut ViewSpec),
) -> Result<(), Msg> {
    state
        .document
        .update_drawing(drawing, |d| {
            if let Some(v) = d
                .views
                .iter_mut()
                .find(|v| v.id == view && v.section.is_some())
            {
                f(v);
            }
        })
        .map_err(feature_text)
}

/// Setzt die Schnittposition (Weltkoordinate der Ebene in mm) aus einem eingegebenen Text in
/// der Anzeigeeinheit.
pub fn set_offset_text(
    state: &mut AppState,
    drawing: DrawingId,
    view: ViewId,
    text: &str,
) -> Result<(), Msg> {
    let value = parse_number(text).ok_or_else(not_a_number)?;
    let value = planora_i18n::current_unit().from_display(value);
    edit(state, drawing, view, |v| {
        if let Some(s) = v.section.as_mut() {
            s.offset = value;
        }
    })
}

/// Schaltet die Schraffur der Schnittansicht `view` ein oder aus.
pub fn set_hatch(
    state: &mut AppState,
    drawing: DrawingId,
    view: ViewId,
    on: bool,
) -> Result<(), Msg> {
    edit(state, drawing, view, |v| {
        if let Some(s) = v.section.as_mut() {
            s.hatch.on = on;
        }
    })
}

/// Setzt den Schraffurabstand (A3-Millimeter) aus einem eingegebenen Text. Ein Abstand, der auf
/// diesem Blatt unter 0,5 mm käme, wird abgelehnt (S11).
pub fn set_spacing_text(
    state: &mut AppState,
    drawing: DrawingId,
    view: ViewId,
    text: &str,
) -> Result<(), Msg> {
    let value = parse_number(text).ok_or_else(not_a_number)?;
    edit(state, drawing, view, |v| {
        if let Some(s) = v.section.as_mut() {
            s.hatch.spacing = value;
        }
    })
}

/// Setzt den Maßstab der Ansicht `view` und überträgt ihn in derselben Änderung auf ihre
/// Schnittansichten (sie folgen der Ausgangsansicht).
pub fn set_scale(
    state: &mut AppState,
    drawing: DrawingId,
    view: ViewId,
    scale: planora_drawing::Scale,
) -> Result<(), Msg> {
    state
        .document
        .update_drawing(drawing, |d| {
            for v in &mut d.views {
                if v.id == view || (v.parent == Some(view) && v.section.is_some()) {
                    v.scale = scale;
                }
            }
        })
        .map_err(feature_text)
}

/// Text der Schnittposition (Modelllänge `v` in mm) in der Anzeigeeinheit, wie
/// [`field_text`].
pub fn offset_text(v: f64) -> String {
    field_text(planora_i18n::current_unit().to_display(v))
}

/// Zahl für ein Eingabefeld: bis zu drei Nachkommastellen, ohne überflüssige Nullen, mit dem
/// Dezimaltrenner der Sprache.
pub fn field_text(v: f64) -> String {
    let s = crate::sketch_view::format_number(v, 3);
    if s.contains([',', '.']) {
        s.trim_end_matches('0')
            .trim_end_matches([',', '.'])
            .to_string()
    } else {
        s
    }
}

// ---- Klicks ---------------------------------------------------------------------------------

/// Eine Ansicht mit Abbildung aufs Blatt und ihrer Hüllbox.
struct Shown<'a> {
    spec: &'a ViewSpec,
    lines: &'a ViewLines,
    xf: SheetTransform,
}

impl Shown<'_> {
    /// Hüllbox auf dem Blatt: (min, max).
    fn sheet_box(&self) -> (DVec2, DVec2) {
        let ((x0, y0), (x1, y1)) = self.lines.bbox;
        let a = self.xf.apply(DVec2::new(x0, y0));
        let b = self.xf.apply(DVec2::new(x1, y1));
        (
            DVec2::new(a.x.min(b.x), a.y.min(b.y)),
            DVec2::new(a.x.max(b.x), a.y.max(b.y)),
        )
    }
}

fn shown_views<'a>(d: &'a Drawing, results: &'a BTreeMap<ViewId, &'a ViewLines>) -> Vec<Shown<'a>> {
    d.views
        .iter()
        .filter_map(|spec| {
            let lines = *results.get(&spec.id)?;
            let xf = view_transform(d, spec.id, results)?;
            Some(Shown { spec, lines, xf })
        })
        .collect()
}

/// Klick mit eingeschaltetem Werkzeug am Blattpunkt `at` bei `px_per_mm` Bildschirmpunkten je
/// Blatt-Millimeter. Der erste Klick merkt Ansicht und Punkt, der zweite legt die Schnittansicht
/// an. Eine Ablehnung steht danach in der Meldung des Werkzeugs.
pub fn click(state: &mut AppState, drawing: DrawingId, at: DVec2, px_per_mm: f64) {
    state.section_tool.set_message(None);
    match try_click(state, drawing, at, px_per_mm) {
        Ok(()) => {}
        Err(m) => state.section_tool.set_message(Some(m)),
    }
}

fn try_click(state: &mut AppState, id: DrawingId, at: DVec2, px_per_mm: f64) -> Result<(), Msg> {
    if state.sketch_session.is_some() {
        return Err(sketch_blocked());
    }
    let d = state.document.drawing(id).ok_or_else(no_drawing)?;
    let owned = state
        .drawing_ui
        .results_for(d, state.document.model_edit_count());
    let refs: BTreeMap<ViewId, &ViewLines> = owned.iter().map(|(k, l)| (*k, l.as_ref())).collect();
    let shown = shown_views(d, &refs);
    let Some((view, p0)) = state.section_tool.start else {
        // Erster Klick: die oberste Ansicht, in deren Hüllbox der Punkt liegt.
        let hit = shown.iter().rev().find(|s| {
            let (lo, hi) = s.sheet_box();
            at.x >= lo.x && at.x <= hi.x && at.y >= lo.y && at.y <= hi.y
        });
        let Some(s) = hit else {
            return Err(not_from_here());
        };
        if !s.spec.kind.is_normal() || s.spec.section.is_some() {
            return Err(not_from_here());
        }
        let p = to_view(&s.xf, at);
        let view = s.spec.id;
        state.section_tool.start = Some((view, p));
        return Ok(());
    };
    // Zweiter Klick.
    let Some(s) = shown.iter().find(|s| s.spec.id == view) else {
        state.section_tool.start = None;
        return Err(views_pending());
    };
    let end = to_view(&s.xf, at);
    if (end - p0).length() * s.xf.scale < MIN_LENGTH_MM {
        return Err(too_short());
    }
    let (horizontal, coord) = line_for(p0, end, s.lines.bbox, s.xf.scale * px_per_mm);
    state.section_tool.start = None;
    create(state, id, view, horizontal, coord).map(|_| ())
}

/// Vorschau der Schnittlinie mit Pfeilen in Blickrichtung, wenn der erste Klick gesetzt ist und
/// der Zeiger am Blattpunkt `at` steht.
pub fn preview(state: &AppState, id: DrawingId, at: DVec2, px_per_mm: f64) -> Vec<Mark> {
    let Some((view, p0)) = state.section_tool.start else {
        return Vec::new();
    };
    let Some(d) = state.document.drawing(id) else {
        return Vec::new();
    };
    let owned = state
        .drawing_ui
        .results_for(d, state.document.model_edit_count());
    let refs: BTreeMap<ViewId, &ViewLines> = owned.iter().map(|(k, l)| (*k, l.as_ref())).collect();
    let shown = shown_views(d, &refs);
    let Some(s) = shown.iter().find(|s| s.spec.id == view) else {
        return Vec::new();
    };
    let end = to_view(&s.xf, at);
    let (horizontal, coord) = line_for(p0, end, s.lines.bbox, s.xf.scale * px_per_mm);
    let f = sheet_scale(d.sheet);
    let Some(kind) = section_kind(s.spec.kind, horizontal, false) else {
        return Vec::new();
    };
    let ((x0, y0), (x1, y1)) = s.lines.bbox;
    let (pa, pb) = if horizontal {
        (DVec2::new(x0, coord), DVec2::new(x1, coord))
    } else {
        (DVec2::new(coord, y0), DVec2::new(coord, y1))
    };
    let u = if horizontal {
        DVec2::new(1.0, 0.0)
    } else {
        DVec2::new(0.0, 1.0)
    };
    let v = view_direction(s.spec.kind, kind);
    let a = s.xf.apply(pa) - u * (SECTION_OVERHANG_MM * f);
    let b = s.xf.apply(pb) + u * (SECTION_OVERHANG_MM * f);
    let pen = MarkKind::SectionLine(SectionPen::Chain);
    let mut marks = vec![Mark::Line { a, b, kind: pen }];
    for e in [a, b] {
        marks.push(Mark::Line {
            a: e,
            b: e + v * (SECTION_SHAFT_MM * f),
            kind: pen,
        });
        marks.push(Mark::Arrow {
            tip: e + v * ((SECTION_SHAFT_MM + SECTION_ARROW_LEN) * f),
            dir: DVec2::new(-v.x, -v.y),
            kind: pen,
        });
    }
    marks
}

/// Blickrichtung der Schnittansicht `kind` auf dem Blatt der Quellansicht `source` (S5).
fn view_direction(source: ViewKind, kind: ViewKind) -> DVec2 {
    let pb = Basis::of(source);
    let kd = Basis::of(kind).d;
    DVec2::new(kd.dot(pb.ex).round(), kd.dot(pb.ey).round())
}

/// Die Schnittlinien der Zeichnung auf dem Blatt: Schnittansicht und Endpunkte (mit Überstand).
pub fn section_lines(
    d: &Drawing,
    results: &BTreeMap<ViewId, &ViewLines>,
) -> Vec<(ViewId, DVec2, DVec2)> {
    let f = sheet_scale(d.sheet);
    let mut out = Vec::new();
    for spec in &d.views {
        let (Some(sec), Some(parent)) = (&spec.section, spec.parent) else {
            continue;
        };
        let (Some(src), Some(lines), Some(xf)) = (
            d.view(parent),
            results.get(&parent),
            view_transform(d, parent, results),
        ) else {
            continue;
        };
        let Some((horizontal, coord)) = section_line_coord(src.kind, &sec.plane(spec.kind)) else {
            continue;
        };
        let ((x0, y0), (x1, y1)) = lines.bbox;
        let (pa, pb, u) = if horizontal {
            (
                DVec2::new(x0, coord),
                DVec2::new(x1, coord),
                DVec2::new(1.0, 0.0),
            )
        } else {
            (
                DVec2::new(coord, y0),
                DVec2::new(coord, y1),
                DVec2::new(0.0, 1.0),
            )
        };
        out.push((
            spec.id,
            xf.apply(pa) - u * (SECTION_OVERHANG_MM * f),
            xf.apply(pb) + u * (SECTION_OVERHANG_MM * f),
        ));
    }
    out
}

/// Schnittansicht, deren Schnittlinie höchstens `pick_px` Bildschirmpunkte von `pos` entfernt
/// liegt (die nächste).
pub fn section_line_hit(
    lines: &[(ViewId, DVec2, DVec2)],
    to_screen: &dyn Fn(DVec2) -> Pos2,
    pos: Pos2,
    pick_px: f32,
) -> Option<ViewId> {
    let mut best: Option<(f32, ViewId)> = None;
    for (id, a, b) in lines {
        let d = crate::drawing_view::screen_seg_dist(pos, to_screen(*a), to_screen(*b));
        if d <= pick_px && best.is_none_or(|(x, _)| d < x) {
            best = Some((d, *id));
        }
    }
    best.map(|(_, id)| id)
}

/// Wählt die Schnittansicht `view` der offenen Zeichnung (Maß und Kreis sind dann nicht gewählt).
pub fn select(state: &mut AppState, view: ViewId) {
    let ui = &mut state.drawing_ui;
    ui.selected_view = Some(view);
    ui.selected_dim = None;
    ui.selected_balloon = None;
}
