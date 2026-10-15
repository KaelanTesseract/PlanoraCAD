//! Darstellung von Maßen und Mittellinien als Markierungen auf dem Blatt (Teilprojekt 12).
//!
//! Alle Längen sind Blatt-mm; `SheetTransform` bildet Ansichts- auf Blattkoordinaten ab
//! (gleichmäßiger Maßstab, keine Drehung). Ergebnis sind einfache Markierungen ([`Mark`]),
//! die SVG und DXF ausgeben. Nicht endliche Eingaben oder Zwischenwerte ergeben **keine**
//! Markierungen (nie eine Panik); Markierungen weit außerhalb des Blatts werden trotzdem
//! erzeugt (die Warnung „außerhalb des Blatts“ ist Sache der Oberfläche).
//!
//! **Textbreite** ist eine feste Schätzung: `TEXT_ADVANCE · Höhe · Zeichenzahl` (andere Schriften
//! können breiter sein).
//!
//! **Lineare Maße** (`Anchor::Linear`): Messrichtung `u` = `dir`, zum Lesen gedreht (Winkel in
//! `(−π/2, π/2]`); Querachse `w` = `u` um 90° gedreht, nach rechts bzw. bei senkrechter Lage
//! nach oben (waagerecht `(0, 1)`, senkrecht `(1, 0)`; dieselbe Querachse wie im Anker).
//! - **Bezugslage:** bei `offset ≥ 0` die größte Querlage `·w` aller vier Spannenenden beider
//!   Bezüge, bei `offset < 0` die kleinste (bei Gruppen über alle Glieder, [`group_base`]).
//!   Die Maßlinie liegt bei `Bezugslage + offset` (Bezugsmaße: `offset ± Stufe·DIM_STEP`,
//!   weiter nach außen).
//! - **Hilfslinien:** je Bezug vom Spannenende, das der Maßlinie am nächsten liegt (bei
//!   `offset ≥ 0` das mit der größeren, sonst das mit der kleineren Querlage), um `WITNESS_GAP`
//!   abgesetzt bis `WITNESS_OVER` über die Maßlinie hinaus.
//! - **Pfeile:** Spitzen auf den Hilfslinien. Ist der Abstand `L` der Hilfslinien
//!   `≥ 2·ARROW_LEN + 1`, innen (Körper zeigt zur Mitte, Maßlinie von Spitze zu Spitze), sonst
//!   außen (Körper nach außen, Maßlinie je Seite um `OUTSIDE_EXT` verlängert).
//! - **Text:** in Leserichtung „über“ der Maßlinie (`TEXT_GAP` entlang der Lesenormalen
//!   `u⊥`), mittig (`Middle`), Winkel von `u`; passt er nicht zwischen die Pfeile
//!   (`Breite > L − 2·ARROW_LEN`), steht er hinter dem zweiten Ende bei
//!   `ARROW_LEN + TEXT_AFTER` (`Start`). `text_shift` verschiebt ihn zuletzt.
//!
//! **Ø/R** (`Anchor::Round`): Richtung `u = (cos θ, sin θ)`, `θ = spec.angle` Grad. Pfeilspitze
//! auf dem Kreis bei `center + u·r`, Führungslinie bis `center + u·(r + offset)`, Schulter
//! `SHOULDER` waagerecht zur Seite von `u.x` (bei `u.x = 0` nach rechts), Text dahinter mit
//! `TEXT_GAP` Abstand, mittig zur Schulter (`Start` rechts, `End` links), waagerecht. Ø mit
//! `2r ≥ LARGE_DIAMETER` auf dem Blatt: stattdessen Maßlinie durch die Mitte von
//! `center − u·r` bis `center + u·r` mit zwei Pfeilen innen, Text mittig über der Linie.
//!
//! **Winkel** (`Anchor::Angle`): Bogen um den Scheitel mit Radius `|offset|` (null →
//! `DEFAULT_OFFSET`) zwischen `d1` und `d2` in der kürzeren Richtung (gegen den Uhrzeigersinn,
//! `start ∈ [0, 2π)`), Hilfslinien vom Scheitel entlang `d1`, `d2` bis Radius + `WITNESS_OVER`,
//! Pfeile tangential an den Bogenenden (bei zu kurzem Bogen außen), Text an der Bogenmitte
//! außen, tangential und lesbar (gedreht → eine Texthöhe weiter außen).

use crate::centerline::CenterLine;
use crate::dimension::{Anchor, ChainMode, DimError, DimId, DimKind, DimSpec, Measured, Tolerance};
use crate::model::ViewId;
use crate::sheet::SheetTransform;
use planora_math::DVec2;
use std::collections::BTreeMap;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// Texthöhe der Maßzahlen.
pub const DIM_TEXT_H: f64 = 3.5;
/// Zeichenbreite als Anteil der Höhe (Schätzung).
pub const TEXT_ADVANCE: f64 = 0.6;
/// Länge eines Pfeils.
pub const ARROW_LEN: f64 = 3.5;
/// Halbe Breite eines Pfeils am Ende.
pub const ARROW_HALF_W: f64 = 0.6;
/// Abstand der Hilfslinie vom Bezug.
pub const WITNESS_GAP: f64 = 1.0;
/// Überstand der Hilfslinie über die Maßlinie.
pub const WITNESS_OVER: f64 = 2.0;
/// Abstand gestapelter Maßlinien.
pub const DIM_STEP: f64 = 7.0;
/// Abstand der ersten Maßlinie.
pub const DEFAULT_OFFSET: f64 = 10.0;
/// Strichstärke der Maß- und Hilfslinien.
pub const DIM_LINE_W: f64 = 0.25;
/// Abstand des Textes über der Maßlinie.
pub const TEXT_GAP: f64 = 1.0;
/// Überstand der Mittellinien über die Körperkante.
pub const CENTER_OVERSHOOT: f64 = 3.0;
/// Strichstärke der Mittellinien.
pub const CENTER_W: f64 = 0.25;
/// Strichpunkt-Muster der Mittellinien (Strich, Lücke, Punktstrich, Lücke).
pub const CENTER_PATTERN: [f64; 4] = [12.0, 3.0, 2.0, 3.0];
/// Höhe des Toleranztextes als Anteil von `DIM_TEXT_H`.
pub const TOLERANCE_TEXT_FACTOR: f64 = 0.7;
/// Verlängerung der Maßlinie je Seite bei außen liegenden Pfeilen.
pub const OUTSIDE_EXT: f64 = 5.0;
/// Abstand des Textes hinter der Pfeillänge, wenn er nicht zwischen die Pfeile passt.
pub const TEXT_AFTER: f64 = 2.0;
/// Länge der waagerechten Schulter der Ø/R-Führungslinie.
pub const SHOULDER: f64 = 4.0;
/// Ab diesem Durchmesser auf dem Blatt läuft die Ø-Maßlinie durch die Mitte.
pub const LARGE_DIAMETER: f64 = 25.0;
/// Abstand der Grundlinien zweizeiliger Toleranzen als Vielfaches ihrer Höhe.
pub const STACK_SPACING: f64 = 1.2;
/// Höchstzahl der Zeichen des Grundes bei einem verlorenen Maß (danach „…“).
pub const LOST_REASON_MAX: usize = 40;

/// Eine Markierung in Blattkoordinaten (mm, y nach oben).
#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    /// Strecke.
    Line { a: DVec2, b: DVec2, kind: MarkKind },
    /// Gefüllter Pfeil: Spitze und Einheitsvektor von der Spitze weg entlang des Pfeilkörpers
    /// (zur Maß- bzw. Führungslinie hin). Länge `ARROW_LEN`, halbe Breite `ARROW_HALF_W`.
    Arrow {
        tip: DVec2,
        dir: DVec2,
        kind: MarkKind,
    },
    /// Kreisbogen gegen den Uhrzeigersinn von `start` bis `end` (Radiant, `end > start`).
    Arc {
        center: DVec2,
        r: f64,
        start: f64,
        end: f64,
        kind: MarkKind,
    },
    /// Text: Anker auf der Grundlinie, Höhe, Winkel (Radiant, gegen den Uhrzeigersinn).
    Text {
        pos: DVec2,
        height: f64,
        angle: f64,
        anchor: TextAnchor,
        text: String,
        kind: MarkKind,
    },
}

/// Art einer Markierung (Ausgabe: Ebene/Farbe).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    Dimension,
    CenterLine,
    /// Maß mit verlorenem Bezug (rot).
    Lost,
}

/// Lage des Ankers im Text (entlang der Grundlinie).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAnchor {
    Start,
    Middle,
    End,
}

/// Ergebnis der Auflösung eines Maßes (für die Oberfläche): Wert oder Fehler, dazu die letzte
/// bekannte Lage.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedDim {
    pub spec_id: DimId,
    pub result: Result<Measured, DimError>,
    pub last_known: Option<Anchor>,
}

/// Ein Maß einer Ansicht mit seinem Messergebnis.
pub type DimEntry = (DimSpec, Result<Measured, DimError>);

/// Aufgelöste Maße einer Zeichnung je Ansicht, dazu die letzte bekannte Lage verlorener Maße
/// (für [`layout_lost`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResolvedDims {
    pub by_view: BTreeMap<ViewId, Vec<DimEntry>>,
    pub last_known: BTreeMap<DimId, Anchor>,
}

/// Text eines Maßes: Haupttext (Höhe `DIM_TEXT_H`) und optional zwei gestapelte Zeilen
/// (obere, untere) der Höhe `stack_height` dahinter.
#[derive(Clone, Debug, PartialEq)]
pub struct DimText {
    pub main: String,
    pub stack: Option<(String, String)>,
    pub stack_height: f64,
}

/// Geschätzte Breite eines Textes: `TEXT_ADVANCE · Höhe · Zeichenzahl`.
pub fn text_width(text: &str, height: f64) -> f64 {
    TEXT_ADVANCE * height * text.chars().count() as f64
}

impl DimText {
    /// Breite: Haupttext, bei gestapelten Zeilen ein Zeichen Abstand (nur wenn der Haupttext
    /// nicht leer ist) und die breitere der beiden Zeilen.
    pub fn width(&self) -> f64 {
        let main = text_width(&self.main, DIM_TEXT_H);
        match &self.stack {
            None => main,
            Some((up, lo)) => {
                let gap = if self.main.is_empty() {
                    0.0
                } else {
                    TEXT_ADVANCE * DIM_TEXT_H
                };
                let stack =
                    text_width(up, self.stack_height).max(text_width(lo, self.stack_height));
                main + gap + stack
            }
        }
    }
}

/// Zahl mit `decimals` Nachkommastellen (begrenzt auf 0…3), gerundet wie `format!("{:.n}")`
/// (nächster Wert; bei einem exakten binären Halbwert zur geraden Ziffer: 0,25 → „0,2“,
/// 0,05 ist binär etwas größer → „0,1“), deutsches Komma, Minus als `−` (U+2212). Werte, die
/// auf null runden, ohne Minus („0,0“). Nicht endlich → „?“. Für DXF wandelt der Schreiber `−`
/// in `-` um.
pub fn format_value(value: f64, decimals: u8) -> String {
    if !value.is_finite() {
        return "?".into();
    }
    let n = usize::from(decimals.min(3));
    let s = format!("{value:.n$}");
    let zero = s.chars().all(|c| matches!(c, '-' | '0' | '.'));
    let s = if zero {
        s.trim_start_matches('-').to_string()
    } else {
        s
    };
    s.replace('.', ",").replace('-', "\u{2212}")
}

/// Zahl mit Vorzeichen (`+`, `−`); null ohne Vorzeichen.
fn signed_value(value: f64, decimals: u8) -> String {
    let s = format_value(value, decimals);
    if s.starts_with('\u{2212}') || s == "?" || s.chars().all(|c| matches!(c, '0' | ',')) {
        s
    } else {
        format!("+{s}")
    }
}

/// Text eines Maßes. Haupttext = Vorsatz (`Ø`/`R`) + Wert (Winkel mit `°`). Eigener Text
/// (`text_override`) ersetzt ihn; `<>` steht dort für den Wert, beginnt der Text mit `<>`, bleibt
/// der Vorsatz. Toleranz dahinter: `Symmetric(t)` → `" ±t"`, `Fit(s)` → `" s"`, `Limits` → zwei
/// Zeilen mit Vorzeichen in `TOLERANCE_TEXT_FACTOR · DIM_TEXT_H`, `LimitSizes` → Höchst- und
/// Mindestmaß (Wert + `upper` / Wert + `lower`, mit Vorsatz) als zwei Zeilen in voller Höhe,
/// ohne Nennmaß (Haupttext leer, außer bei eigenem Text).
pub fn dimension_text(spec: &DimSpec, m: &Measured) -> DimText {
    let dec = spec.decimals;
    let prefix: String = m.prefix.map(String::from).unwrap_or_default();
    let degree = if spec.kind == DimKind::Angle || matches!(m.anchor, Anchor::Angle { .. }) {
        "°"
    } else {
        ""
    };
    let value = format!("{}{degree}", format_value(m.value, dec));
    let main = match &spec.text_override {
        Some(t) if t.starts_with("<>") => format!("{prefix}{}", t.replace("<>", &value)),
        Some(t) => t.replace("<>", &value),
        None => format!("{prefix}{value}"),
    };
    let tol_h = TOLERANCE_TEXT_FACTOR * DIM_TEXT_H;
    match &spec.tolerance {
        Tolerance::None => DimText {
            main,
            stack: None,
            stack_height: tol_h,
        },
        Tolerance::Symmetric(t) => DimText {
            main: format!("{main} ±{}", format_value(t.abs(), dec)),
            stack: None,
            stack_height: tol_h,
        },
        Tolerance::Fit(s) => DimText {
            main: format!("{main} {s}"),
            stack: None,
            stack_height: tol_h,
        },
        Tolerance::Limits { upper, lower } => DimText {
            main,
            stack: Some((signed_value(*upper, dec), signed_value(*lower, dec))),
            stack_height: tol_h,
        },
        Tolerance::LimitSizes { upper, lower } => {
            let line = |d: f64| format!("{prefix}{}{degree}", format_value(m.value + d, dec));
            DimText {
                main: if spec.text_override.is_some() {
                    main
                } else {
                    String::new()
                },
                stack: Some((line(*upper), line(*lower))),
                stack_height: DIM_TEXT_H,
            }
        }
    }
}

/// Gemeinsamer Bezug einer Gruppe (Kette oder Bezugsmaße) quer zur Messrichtung: Querachse `w`
/// (Blatt) und kleinste/größte Querlage aller Spannenenden der linearen Glieder, die parallel
/// zum ersten liegen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroupBase {
    pub w: DVec2,
    pub min: f64,
    pub max: f64,
}

/// Bezug einer Gruppe aus ihren gemessenen Gliedern (siehe [`GroupBase`]); `None` ohne
/// lineares Glied oder bei nicht endlichen Zahlen.
pub fn group_base(members: &[&Measured], xf: &SheetTransform) -> Option<GroupBase> {
    let mut first: Option<DVec2> = None;
    let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
    for m in members {
        let Anchor::Linear {
            dir,
            a_span,
            b_span,
            ..
        } = &m.anchor
        else {
            continue;
        };
        let Some(u) = unit(*dir) else { continue };
        let u0 = *first.get_or_insert(u);
        if u0.perp_dot(u).abs() > PARALLEL_TOL {
            continue;
        }
        let w = cross_axis(u0);
        for p in [a_span.0, a_span.1, b_span.0, b_span.1] {
            let c = xf.apply(p).dot(w);
            min = min.min(c);
            max = max.max(c);
        }
    }
    let w = cross_axis(first?);
    (min.is_finite() && max.is_finite()).then_some(GroupBase { w, min, max })
}

/// Grenze für „parallel“ bei Gruppen (Sinus).
const PARALLEL_TOL: f64 = 1e-9;

/// Markierungen eines gemessenen Maßes ohne Gruppenbezug (siehe [`layout_dimension_in`]).
pub fn layout_dimension(
    spec: &DimSpec,
    m: &Measured,
    xf: &SheetTransform,
    group_index: usize,
) -> Vec<Mark> {
    layout_dimension_in(spec, m, xf, group_index, None)
}

/// Markierungen eines gemessenen Maßes (Regeln im Modulkommentar), Art `Dimension`.
/// `group_index` ist die Stufe eines Bezugsmaßes (`ChainMode::Baseline`): der Abstand wächst um
/// `group_index · DIM_STEP` nach außen; bei anderen Maßen ohne Wirkung. `base` ist der
/// gemeinsame Bezug der Gruppe ([`group_base`]); er gilt nur für lineare Maße parallel zu ihm.
/// Nicht endliche Eingaben oder Ergebnisse → leer.
pub fn layout_dimension_in(
    spec: &DimSpec,
    m: &Measured,
    xf: &SheetTransform,
    group_index: usize,
    base: Option<&GroupBase>,
) -> Vec<Mark> {
    let text = dimension_text(spec, m);
    let marks = layout_with_text(spec, m, xf, group_index, base, &text, MarkKind::Dimension);
    finite_or_empty(marks)
}

fn finite_or_empty(marks: Vec<Mark>) -> Vec<Mark> {
    if marks.iter().all(mark_is_finite) {
        marks
    } else {
        Vec::new()
    }
}

fn mark_is_finite(m: &Mark) -> bool {
    match m {
        Mark::Line { a, b, .. } => a.is_finite() && b.is_finite(),
        Mark::Arrow { tip, dir, .. } => tip.is_finite() && dir.is_finite(),
        Mark::Arc {
            center,
            r,
            start,
            end,
            ..
        } => center.is_finite() && r.is_finite() && start.is_finite() && end.is_finite(),
        Mark::Text {
            pos, height, angle, ..
        } => pos.is_finite() && height.is_finite() && angle.is_finite(),
    }
}

fn xf_ok(xf: &SheetTransform) -> bool {
    xf.scale.is_finite()
        && xf.scale > 0.0
        && xf.tx.is_finite()
        && xf.ty.is_finite()
        && xf.bbox_center.is_finite()
}

fn layout_with_text(
    spec: &DimSpec,
    m: &Measured,
    xf: &SheetTransform,
    group_index: usize,
    base: Option<&GroupBase>,
    text: &DimText,
    kind: MarkKind,
) -> Vec<Mark> {
    let (sx, sy) = spec.text_shift;
    if !(xf_ok(xf) && spec.offset.is_finite() && sx.is_finite() && sy.is_finite()) {
        return Vec::new();
    }
    let level = match spec.chain {
        Some(c) if c.mode == ChainMode::Baseline => group_index as f64,
        _ => 0.0,
    };
    let sign = if spec.offset < 0.0 { -1.0 } else { 1.0 };
    let offset = spec.offset + sign * level * DIM_STEP;
    let ctx = Ctx {
        offset,
        shift: DVec2::new(sx, sy),
        text,
        kind,
    };
    match &m.anchor {
        Anchor::Linear {
            a,
            b,
            dir,
            a_span,
            b_span,
        } => {
            let map = |s: &(DVec2, DVec2)| (xf.apply(s.0), xf.apply(s.1));
            linear(
                &ctx,
                (xf.apply(*a), xf.apply(*b)),
                *dir,
                (map(a_span), map(b_span)),
                base,
            )
        }
        Anchor::Round { center, radius } => {
            let large = m.prefix == Some('Ø');
            round(
                &ctx,
                xf.apply(*center),
                radius * xf.scale,
                spec.angle,
                large,
            )
        }
        Anchor::Angle { vertex, d1, d2 } => angle(&ctx, xf.apply(*vertex), *d1, *d2),
    }
}

/// Gemeinsame Angaben eines Maßes beim Zeichnen.
struct Ctx<'a> {
    /// Wirksamer Abstand (mit Stufe der Bezugsmaße).
    offset: f64,
    shift: DVec2,
    text: &'a DimText,
    kind: MarkKind,
}

impl Ctx<'_> {
    fn line(&self, a: DVec2, b: DVec2) -> Mark {
        Mark::Line {
            a,
            b,
            kind: self.kind,
        }
    }

    fn arrow(&self, tip: DVec2, dir: DVec2) -> Mark {
        Mark::Arrow {
            tip,
            dir,
            kind: self.kind,
        }
    }

    /// Text (mit Toleranzzeilen) mit Anker `pos` auf der Grundlinie, Winkel `angle`.
    fn text(&self, out: &mut Vec<Mark>, pos: DVec2, angle: f64, anchor: TextAnchor) {
        let pos = pos + self.shift;
        let t = self.text;
        let Some((up, lo)) = &t.stack else {
            out.push(self.text_mark(pos, DIM_TEXT_H, angle, anchor, t.main.clone()));
            return;
        };
        let dir = DVec2::new(angle.cos(), angle.sin());
        let width = t.width();
        let start = match anchor {
            TextAnchor::Start => pos,
            TextAnchor::Middle => pos - dir * (0.5 * width),
            TextAnchor::End => pos - dir * width,
        };
        let mut x = start;
        if !t.main.is_empty() {
            out.push(self.text_mark(start, DIM_TEXT_H, angle, TextAnchor::Start, t.main.clone()));
            x = start + dir * (text_width(&t.main, DIM_TEXT_H) + TEXT_ADVANCE * DIM_TEXT_H);
        }
        let h = t.stack_height;
        out.push(self.text_mark(x, h, angle, TextAnchor::Start, lo.clone()));
        let up_pos = x + dir.perp() * (STACK_SPACING * h);
        out.push(self.text_mark(up_pos, h, angle, TextAnchor::Start, up.clone()));
    }

    fn text_mark(
        &self,
        pos: DVec2,
        height: f64,
        angle: f64,
        anchor: TextAnchor,
        text: String,
    ) -> Mark {
        Mark::Text {
            pos,
            height,
            angle,
            anchor,
            text,
            kind: self.kind,
        }
    }
}

/// Einheitsvektor oder `None` (null, nicht endlich).
fn unit(v: DVec2) -> Option<DVec2> {
    let n = v.normalize_or_zero();
    (n != DVec2::ZERO && n.is_finite()).then_some(n)
}

/// Einheitsrichtung nach rechts, bei senkrechter Lage nach oben (wie im Maßanker).
fn rightward(t: DVec2) -> DVec2 {
    if t.x < -1e-9 || (t.x.abs() <= 1e-9 && t.y < 0.0) {
        -t
    } else {
        t
    }
}

/// Querachse zur Messrichtung (wie [`Anchor::Linear`]).
fn cross_axis(u: DVec2) -> DVec2 {
    rightward(u.perp())
}

/// Winkel lesbar gemacht: in `(−π/2, π/2]`; `true`, wenn dafür um π gedreht wurde.
fn readable(a: f64) -> (f64, bool) {
    let r = (a + PI).rem_euclid(TAU) - PI;
    const EPS: f64 = 1e-12;
    if r > FRAC_PI_2 + EPS {
        (r - PI, true)
    } else if r <= -FRAC_PI_2 + EPS {
        (r + PI, true)
    } else {
        (r, false)
    }
}

type Span = (DVec2, DVec2);

fn linear(
    ctx: &Ctx,
    (a, b): (DVec2, DVec2),
    dir: DVec2,
    (a_span, b_span): (Span, Span),
    base: Option<&GroupBase>,
) -> Vec<Mark> {
    let Some(u) = unit(dir) else {
        return Vec::new();
    };
    let u = if readable(u.y.atan2(u.x)).1 { -u } else { u };
    let w = cross_axis(u);
    let outward = ctx.offset >= 0.0;
    let s = if outward { 1.0 } else { -1.0 };
    // Je Bezug das der Maßlinie nähere Spannenende.
    let near = |sp: Span| {
        let (c0, c1) = (sp.0.dot(w), sp.1.dot(w));
        if (c1 > c0) == outward { sp.1 } else { sp.0 }
    };
    let (mut ea, mut eb) = (near(a_span), near(b_span));
    let mut c0 = if outward {
        ea.dot(w).max(eb.dot(w))
    } else {
        ea.dot(w).min(eb.dot(w))
    };
    if let Some(g) = base
        && g.w.dot(w) >= 1.0 - PARALLEL_TOL
    {
        c0 = if outward {
            c0.max(g.max)
        } else {
            c0.min(g.min)
        };
    }
    let c = c0 + ctx.offset;
    let (mut pa, mut pb) = (a.dot(u), b.dot(u));
    if pa > pb {
        std::mem::swap(&mut pa, &mut pb);
        std::mem::swap(&mut ea, &mut eb);
    }
    let len = pb - pa;
    if !(len.is_finite() && c.is_finite()) {
        return Vec::new();
    }
    let at = |p: f64| u * p + w * c;
    let (ta, tb) = (at(pa), at(pb));
    let witness = |e: DVec2| {
        ctx.line(
            e + w * (s * WITNESS_GAP),
            e + w * (c - e.dot(w) + s * WITNESS_OVER),
        )
    };
    let mut out = vec![witness(ea), witness(eb)];
    if len >= 2.0 * ARROW_LEN + 1.0 {
        out.push(ctx.line(ta, tb));
        out.push(ctx.arrow(ta, u));
        out.push(ctx.arrow(tb, -u));
    } else {
        out.push(ctx.line(ta - u * OUTSIDE_EXT, tb + u * OUTSIDE_EXT));
        out.push(ctx.arrow(ta, -u));
        out.push(ctx.arrow(tb, u));
    }
    let angle = u.y.atan2(u.x);
    let up = u.perp() * TEXT_GAP;
    if ctx.text.width() <= len - 2.0 * ARROW_LEN {
        ctx.text(&mut out, (ta + tb) * 0.5 + up, angle, TextAnchor::Middle);
    } else {
        let pos = tb + u * (ARROW_LEN + TEXT_AFTER) + up;
        ctx.text(&mut out, pos, angle, TextAnchor::Start);
    }
    out
}

fn round(ctx: &Ctx, c: DVec2, r: f64, angle_deg: f64, diameter: bool) -> Vec<Mark> {
    if !(r.is_finite() && r > 0.0 && angle_deg.is_finite()) {
        return Vec::new();
    }
    let th = angle_deg.to_radians();
    let u = DVec2::new(th.cos(), th.sin());
    let mut out = Vec::new();
    if diameter && 2.0 * r >= LARGE_DIAMETER {
        let (p1, p2) = (c - u * r, c + u * r);
        out.push(ctx.line(p1, p2));
        out.push(ctx.arrow(p2, -u));
        out.push(ctx.arrow(p1, u));
        let (a, flipped) = readable(th);
        let t = if flipped { -u } else { u };
        ctx.text(&mut out, c + t.perp() * TEXT_GAP, a, TextAnchor::Middle);
        return out;
    }
    let tip = c + u * r;
    let s = if ctx.offset < 0.0 { -1.0 } else { 1.0 };
    let knee = c + u * (r + ctx.offset);
    let side = if u.x < 0.0 { -1.0 } else { 1.0 };
    let end = knee + DVec2::new(side * SHOULDER, 0.0);
    out.push(ctx.arrow(tip, u * s));
    out.push(ctx.line(tip, knee));
    out.push(ctx.line(knee, end));
    let pos = end + DVec2::new(side * TEXT_GAP, -0.5 * DIM_TEXT_H);
    let anchor = if side > 0.0 {
        TextAnchor::Start
    } else {
        TextAnchor::End
    };
    ctx.text(&mut out, pos, 0.0, anchor);
    out
}

fn angle(ctx: &Ctx, v: DVec2, d1: DVec2, d2: DVec2) -> Vec<Mark> {
    let (Some(d1), Some(d2)) = (unit(d1), unit(d2)) else {
        return Vec::new();
    };
    let r = if ctx.offset.abs() > 1e-9 {
        ctx.offset.abs()
    } else {
        DEFAULT_OFFSET
    };
    let (a1, a2) = (d1.y.atan2(d1.x), d2.y.atan2(d2.x));
    // Differenz in (−π, π].
    let mut delta = (a2 - a1 + PI).rem_euclid(TAU) - PI;
    if delta <= -PI {
        delta += TAU;
    }
    if delta.abs() < 1e-12 {
        return Vec::new();
    }
    let start = if delta >= 0.0 { a1 } else { a2 }.rem_euclid(TAU);
    let end = start + delta.abs();
    let at = |t: f64| v + DVec2::new(t.cos(), t.sin()) * r;
    let tan = |t: f64| DVec2::new(-t.sin(), t.cos());
    let mut out = vec![
        Mark::Arc {
            center: v,
            r,
            start,
            end,
            kind: ctx.kind,
        },
        ctx.line(v, v + d1 * (r + WITNESS_OVER)),
        ctx.line(v, v + d2 * (r + WITNESS_OVER)),
    ];
    let inside = if r * delta.abs() >= 2.0 * ARROW_LEN + 1.0 {
        1.0
    } else {
        -1.0
    };
    out.push(ctx.arrow(at(start), tan(start) * inside));
    out.push(ctx.arrow(at(end), -tan(end) * inside));
    let mid = 0.5 * (start + end);
    let (a, flipped) = readable(mid - FRAC_PI_2);
    let radial = r + TEXT_GAP + if flipped { DIM_TEXT_H } else { 0.0 };
    ctx.text(
        &mut out,
        v + DVec2::new(mid.cos(), mid.sin()) * radial,
        a,
        TextAnchor::Middle,
    );
    out
}

/// Ein verlorenes Maß in Rot (`MarkKind::Lost`): mit letzter bekannter Lage deren Maß-,
/// Hilfs- und Führungslinien und Pfeile (wie [`layout_dimension`], Bezugsmaße mit ihrer Stufe
/// `order`) und an der Textstelle `"? " + Grund` (Grund auf `LOST_REASON_MAX` Zeichen gekürzt,
/// dann „…“); ohne letzte Lage (oder wenn sie sich nicht zeichnen lässt) nur ein `?` in der
/// Mitte der Ansicht (`xf.apply(xf.bbox_center)`).
pub fn layout_lost(
    spec: &DimSpec,
    last: Option<&Anchor>,
    xf: &SheetTransform,
    reason: &str,
) -> Vec<Mark> {
    let center_mark = || {
        finite_or_empty(vec![Mark::Text {
            pos: xf.apply(xf.bbox_center),
            height: DIM_TEXT_H,
            angle: 0.0,
            anchor: TextAnchor::Middle,
            text: "?".into(),
            kind: MarkKind::Lost,
        }])
    };
    let Some(anchor) = last else {
        return center_mark();
    };
    let short: String = if reason.chars().count() > LOST_REASON_MAX {
        let mut s: String = reason.chars().take(LOST_REASON_MAX).collect();
        s.push('…');
        s
    } else {
        reason.to_string()
    };
    let label = if short.is_empty() {
        "?".to_string()
    } else {
        format!("? {short}")
    };
    let prefix = match spec.kind {
        DimKind::Diameter => Some('Ø'),
        DimKind::Radius => Some('R'),
        _ => None,
    };
    let m = Measured {
        value: 0.0,
        anchor: anchor.clone(),
        prefix,
    };
    let text = DimText {
        main: label,
        stack: None,
        stack_height: DIM_TEXT_H,
    };
    let level = spec.chain.map_or(0, |c| c.order as usize);
    // Lage des Textes wie bei einem kurzen Wert, damit die Linien dieselben bleiben.
    let mut plain = spec.clone();
    plain.tolerance = Tolerance::None;
    plain.text_override = None;
    let probe = dimension_text(&plain, &m);
    let mut marks = layout_with_text(&plain, &m, xf, level, None, &probe, MarkKind::Lost);
    let mut first = true;
    marks.retain_mut(|mk| {
        if let Mark::Text { text: t, .. } = mk {
            if !first {
                return false;
            }
            first = false;
            *t = text.main.clone();
        }
        true
    });
    let marks = finite_or_empty(marks);
    if marks.is_empty() {
        center_mark()
    } else {
        marks
    }
}

/// Eine Mittellinie auf dem Blatt: die abgebildete Strecke auf beiden Seiten um
/// `CENTER_OVERSHOOT` (Blatt-mm) verlängert, Art `CenterLine`. Entartet (Länge null) oder nicht
/// endlich → leer.
pub fn layout_centerline(line: &CenterLine, xf: &SheetTransform) -> Vec<Mark> {
    if !xf_ok(xf) {
        return Vec::new();
    }
    let (a, b) = (xf.apply(line.a), xf.apply(line.b));
    let Some(d) = unit(b - a) else {
        return Vec::new();
    };
    finite_or_empty(vec![Mark::Line {
        a: a - d * CENTER_OVERSHOOT,
        b: b + d * CENTER_OVERSHOOT,
        kind: MarkKind::CenterLine,
    }])
}
