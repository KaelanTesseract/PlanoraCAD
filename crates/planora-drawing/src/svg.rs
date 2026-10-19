//! SVG-Schreiber für das Blattmodell.
//!
//! Alle Zahlen bleiben Blattkoordinaten (mm, y nach oben). Die Umkehrung der y-Achse (SVG zählt
//! y nach unten) geschieht **einmal** durch `transform="matrix(1 0 0 -1 0 H)"` auf der
//! Wurzelgruppe; die Ansichtsgruppen und Pfade darunter tragen keine eigene Umrechnung.
//!
//! Bogenrichtung: Die Schalterstellung `sweep-flag = 1` der SVG-Spezifikation heißt „in Richtung
//! wachsender Winkel im Koordinatensystem des Elements“, also von der x- zur y-Achse der
//! hingeschriebenen Zahlen. Ein Bogen gegen den Uhrzeigersinn im y-aufwärts-Blattsystem hat
//! wachsende Winkel (`Out2d::Arc`: `start < end`) und bekommt deshalb `sweep = 1`; die
//! Spiegelung der Wurzelgruppe wirkt erst danach auf das fertige Bild (sie vertauscht die
//! Anzeige-Richtung, nicht die Zahlen).
//!
//! Blattrahmen (`SheetModel.frame`): Gruppe `<g id="Blattrahmen">` als erste Gruppe unter der
//! gespiegelten Wurzelgruppe, vor den Ansichten; Strichbreite und Farbe je Markierung
//! (`SheetPen::width_at` mit `SheetFrame.scale`, `#000`). Ohne Rahmen entsteht weder die Gruppe
//! noch ein `<rect>`.
//!
//! Markierungen (`SheetView.marks`): Je Ansicht, die Markierungen hat, folgen auf die Liniengruppe
//! bis zu zwei weitere Gruppen: `„{Ansicht} Maße“` (Maße und verlorene Maße) und
//! `„{Ansicht} Mittellinien“`. Ansichten ohne Markierungen erzeugen keine zusätzlichen Zeilen. Text
//! trägt `scale(1 -1)`, das die Spiegelung der Wurzelgruppe für die Schrift wieder aufhebt, sonst
//! stünde sie spiegelverkehrt; der Winkel (Grad, gegen den Uhrzeigersinn auf dem Blatt) steht als
//! `rotate(-Winkel)` dahinter. Pfeile sind gefüllte Dreiecke, Bögen werden wie die Kreisbögen der
//! Linien in Stücke von höchstens 90° zerlegt.
//!
//! Schnittansichten (`MarkKind::Hatch`, `MarkKind::SectionLine`): nach den bisherigen Gruppen
//! folgen, nur wenn vorhanden, `„{Ansicht} Schraffur“` (dünn wie Maßlinien, ohne Muster) und
//! `„{Ansicht} Schnitt“` (Schnittlinie, Pfeile, Buchstaben, Titel). Die Kette der Schnittlinie
//! ist `DIM_LINE_W` mit `CENTER_PATTERN`, die Endstücke haben `SECTION_END_W`, Pfeile die Größe
//! aus `arrow_size` (5 × 2 mm mal Blattfaktor).
//!
//! * Kreisbögen und Ellipsenbögen werden in Stücke von höchstens 90° zerlegt (ein voller Kreis
//!   besteht aus vier Viertelbögen). Die Mitte eines SVG-Bogens wird beim Lesen aus den
//!   auf sechs Nachkommastellen gerundeten Endpunkten zurückgerechnet; bei einem Halbkreis ist
//!   das schlecht gestellt (Rundung von 5e-7 verschiebt die Mitte um bis zu 3e-3 mm), bei
//!   höchstens 90° nur um die Rundung selbst.
//! * Splines werden in Bézier-Stücke zerlegt (Knoteneinfügen bis Vielfachheit `degree`):
//!   Grad 3 → `C`, Grad 2 → `Q`, Grad 1 → `L`, alle anderen (Grad ≥ 4 und rationale ab Grad 2)
//!   als kubische Hermite-Näherung: 8 gleiche Teile, das schlechteste Teil wird halbiert, bis
//!   die gemessene Abweichung höchstens `FIT_TOL` beträgt oder 64 Teile erreicht sind.

use crate::Visibility;
use crate::annotate::{
    CENTER_PATTERN, CENTER_W, DIM_LINE_W, Mark, MarkKind, SECTION_END_W, SectionPen, TextAnchor,
    arrow_size,
};
use crate::error::DrawError;
use crate::out2d::{Out2d, bezier_pieces};
use crate::sheet::SheetModel;
use crate::sheet_frame::sheet_scale;
use planora_math::{DVec2, PI, TAU};
use std::collections::BTreeSet;
use std::io::Write;

/// Zielwert für die Abweichung der kubischen Näherung (mm). Strenger als die 1e-6 der Vorgabe,
/// weil die Rundung der Zahlen auf sechs Nachkommastellen bis zu 7e-7 dazukommt.
const FIT_TOL: f64 = 2e-7;
/// Teile, mit denen die Näherung beginnt.
const FIT_START: usize = 8;
/// Höchstzahl der Teile je Bézier-Stück.
const FIT_MAX: usize = 64;
/// Messpunkte je Teilstück bei der Prüfung der Näherung.
const FIT_SAMPLES: usize = 16;
/// Runden der Anpassung der Tangentenlängen (jeweils mit nachgeführten Parametern).
const FIT_ROUNDS: usize = 3;
/// Schritte der Mustersuche nach der kleinsten größten Abweichung.
const FIT_SEARCH: usize = 40;

/// Schreibt das Blattmodell als SVG. Bei einem Fehler wird nichts geschrieben.
pub fn write_svg(model: &SheetModel, out: &mut dyn Write) -> Result<(), DrawError> {
    let text = svg_string(model)?;
    out.write_all(text.as_bytes())?;
    Ok(())
}

/// Das Blattmodell als SVG-Text.
pub fn svg_string(model: &SheetModel) -> Result<String, DrawError> {
    let (w, h) = model.sheet.size_mm();
    let (wn, hn) = (num(w)?, num(h)?);
    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    s.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{wn}mm\" height=\"{hn}mm\" viewBox=\"0 0 {wn} {hn}\">\n"
    ));
    s.push_str(&format!("<g transform=\"matrix(1 0 0 -1 0 {hn})\">\n"));
    let mut used: BTreeSet<String> = BTreeSet::new();
    if let Some(frame) = &model.frame {
        // Der Blattrahmen steht als erste Gruppe unter den Ansichten.
        let id = unique_name(&mut used, FRAME_GROUP);
        let mut body = String::new();
        for m in &frame.marks {
            mark_element(&mut body, m, frame.scale)?;
        }
        s.push_str(&format!("<g id=\"{}\">\n{body}</g>\n", escape(&id)));
    }
    for view in &model.views {
        let name = unique_name(&mut used, &view.name);
        s.push_str(&format!("<g id=\"{}\">\n", escape(&name)));
        let mut visible = String::new();
        let mut hidden = String::new();
        for line in &view.lines {
            let d = match line.vis {
                Visibility::Visible => &mut visible,
                Visibility::Hidden => &mut hidden,
            };
            curve_path(d, &line.curve)?;
        }
        if !visible.is_empty() {
            s.push_str(&format!(
                "<path d=\"{visible}\" stroke=\"#000\" stroke-width=\"0.5\" fill=\"none\" stroke-linecap=\"round\"/>\n"
            ));
        }
        if !hidden.is_empty() {
            s.push_str(&format!(
                "<path d=\"{hidden}\" stroke=\"#000\" stroke-width=\"0.25\" stroke-dasharray=\"3 1\" fill=\"none\" stroke-linecap=\"round\"/>\n"
            ));
        }
        s.push_str("</g>\n");
        mark_groups(
            &mut s,
            &mut used,
            &name,
            &view.marks,
            sheet_scale(model.sheet),
        )?;
    }
    s.push_str("</g>\n</svg>\n");
    Ok(s)
}

// ---------------------------------------------------------------------------------------------
// Markierungen
// ---------------------------------------------------------------------------------------------

/// Kennung der Gruppe des Blattrahmens.
const FRAME_GROUP: &str = "Blattrahmen";
/// Farbe der Maße und Mittellinien.
const MARK_COLOR: &str = "#000";
/// Farbe verlorener Maße.
const LOST_COLOR: &str = "#c00";

fn color(kind: MarkKind) -> &'static str {
    match kind {
        MarkKind::Lost => LOST_COLOR,
        MarkKind::Dimension
        | MarkKind::CenterLine
        | MarkKind::Sheet(_)
        | MarkKind::Position(_)
        | MarkKind::Hatch
        | MarkKind::SectionLine(_) => MARK_COLOR,
    }
}

/// Gruppe einer Markierung innerhalb ihrer Ansicht.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MarkGroup {
    Dims,
    Centre,
    Positions,
    Hatch,
    Section,
}

fn mark_group(m: &Mark) -> MarkGroup {
    let kind = match m {
        Mark::Line { kind, .. }
        | Mark::Arrow { kind, .. }
        | Mark::Arc { kind, .. }
        | Mark::Text { kind, .. } => *kind,
    };
    match kind {
        MarkKind::CenterLine => MarkGroup::Centre,
        MarkKind::Position(_) => MarkGroup::Positions,
        MarkKind::Hatch => MarkGroup::Hatch,
        MarkKind::SectionLine(_) => MarkGroup::Section,
        _ => MarkGroup::Dims,
    }
}

/// Schreibt die Gruppen `„{name} Maße“`, `„{name} Mittellinien“` und `„{name} Positionen“`
/// (nur wenn es Markierungen dieser Art gibt; verlorene Kreise stehen wie verlorene Maße bei den
/// Maßen); die Kennungen sind unter allen Gruppen eindeutig. `sheet_scale` ist der Faktor des
/// Blatts (Strichbreite der Nummernkreise).
fn mark_groups(
    s: &mut String,
    used: &mut BTreeSet<String>,
    name: &str,
    marks: &[Mark],
    sheet_scale: f64,
) -> Result<(), DrawError> {
    for (suffix, group) in [
        ("Maße", MarkGroup::Dims),
        ("Mittellinien", MarkGroup::Centre),
        ("Positionen", MarkGroup::Positions),
        ("Schraffur", MarkGroup::Hatch),
        ("Schnitt", MarkGroup::Section),
    ] {
        let mut body = String::new();
        for m in marks.iter().filter(|m| mark_group(m) == group) {
            mark_element(&mut body, m, sheet_scale)?;
        }
        if body.is_empty() {
            continue;
        }
        let id = unique_name(used, &format!("{name} {suffix}"));
        s.push_str(&format!(
            "<g id=\"{}\">
{body}</g>
",
            escape(&id)
        ));
    }
    Ok(())
}

/// Ein Strich-Element mit den Darstellungsmerkmalen einer Markierung.
fn stroke_path(d: &str, kind: MarkKind, sheet_scale: f64) -> Result<String, DrawError> {
    let (width, dash) = match kind {
        MarkKind::CenterLine => (
            CENTER_W,
            format!(
                " stroke-dasharray=\"{} {} {} {}\"",
                num(CENTER_PATTERN[0])?,
                num(CENTER_PATTERN[1])?,
                num(CENTER_PATTERN[2])?,
                num(CENTER_PATTERN[3])?
            ),
        ),
        MarkKind::SectionLine(SectionPen::Chain) => (
            DIM_LINE_W,
            format!(
                " stroke-dasharray=\"{} {} {} {}\"",
                num(CENTER_PATTERN[0])?,
                num(CENTER_PATTERN[1])?,
                num(CENTER_PATTERN[2])?,
                num(CENTER_PATTERN[3])?
            ),
        ),
        MarkKind::SectionLine(SectionPen::End) => (SECTION_END_W, String::new()),
        MarkKind::Dimension | MarkKind::Lost | MarkKind::Hatch => (DIM_LINE_W, String::new()),
        MarkKind::Sheet(pen) => (pen.width_at(sheet_scale), String::new()),
        MarkKind::Position(pen) => (pen.width_at(sheet_scale), String::new()),
    };
    Ok(format!(
        "<path d=\"{d}\" stroke=\"{}\" stroke-width=\"{}\"{dash} fill=\"none\" stroke-linecap=\"round\"/>
",
        color(kind),
        num(width)?
    ))
}

fn mark_element(out: &mut String, m: &Mark, sheet_scale: f64) -> Result<(), DrawError> {
    match m {
        Mark::Line { a, b, kind } => {
            let mut d = String::new();
            move_to(&mut d, *a)?;
            push_cmd(&mut d, 'L', &[b.x, b.y])?;
            out.push_str(&stroke_path(&d, *kind, sheet_scale)?);
        }
        Mark::Arrow { tip, dir, kind } => {
            let (len, half) = arrow_size(*kind, sheet_scale);
            let base = *tip + *dir * len;
            let perp = DVec2::new(-dir.y, dir.x) * half;
            let (p, q) = (base + perp, base - perp);
            let mut d = String::new();
            move_to(&mut d, *tip)?;
            push_cmd(&mut d, 'L', &[p.x, p.y])?;
            push_cmd(&mut d, 'L', &[q.x, q.y])?;
            sep(&mut d);
            d.push('Z');
            out.push_str(&format!(
                "<path d=\"{d}\" fill=\"{}\" stroke=\"none\"/>
",
                color(*kind)
            ));
        }
        Mark::Arc {
            center,
            r,
            start,
            end,
            kind,
        } => {
            let all = [center.x, center.y, *r, *start, *end];
            if all.iter().any(|v| !v.is_finite()) {
                return Err(DrawError::NonFinite);
            }
            let span = (end - start).min(TAU);
            if span <= 0.0 || *r <= 0.0 {
                return Ok(());
            }
            // Wie bei den Bögen der Linien: Stücke von höchstens 90°, also nie `large-arc`; die
            // Bogenrichtung wachsender Winkel ist `sweep = 1`.
            let n = ((span / (0.5 * PI)) - 1e-9).ceil().max(1.0) as usize;
            let pt = |t: f64| *center + DVec2::new(t.cos(), t.sin()) * *r;
            let first = pt(*start);
            let mut d = String::new();
            move_to(&mut d, first)?;
            for i in 1..=n {
                let e = if i == n && (span - TAU).abs() < 1e-12 {
                    first
                } else {
                    pt(start + span * i as f64 / n as f64)
                };
                let rr = num(*r)?;
                sep(&mut d);
                d.push_str(&format!("A {rr} {rr} 0 0 1 {} {}", num(e.x)?, num(e.y)?));
            }
            out.push_str(&stroke_path(&d, *kind, sheet_scale)?);
        }
        Mark::Text {
            pos,
            height,
            angle,
            anchor,
            text,
            kind,
            ..
        } => {
            let anchor = match anchor {
                TextAnchor::Start => "start",
                TextAnchor::Middle => "middle",
                TextAnchor::End => "end",
            };
            out.push_str(&format!(
                "<text transform=\"translate({} {}) scale(1 -1) rotate({})\" font-family=\"sans-serif\" font-size=\"{}\" text-anchor=\"{anchor}\" fill=\"{}\">{}</text>
",
                num(pos.x)?,
                num(pos.y)?,
                num(-angle.to_degrees())?,
                num(*height)?,
                color(*kind),
                escape(text)
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Zahlen, Namen
// ---------------------------------------------------------------------------------------------

/// Zahl mit höchstens sechs Nachkommastellen, ohne Exponent und ohne nachgestellte Nullen;
/// `-0` wird `0`. Nicht endliche Zahlen sind ein Fehler.
fn num(x: f64) -> Result<String, DrawError> {
    if !x.is_finite() {
        return Err(DrawError::NonFinite);
    }
    let t = format!("{x:.6}");
    let t = t.trim_end_matches('0').trim_end_matches('.');
    Ok(if t == "-0" || t.is_empty() {
        "0".to_string()
    } else {
        t.to_string()
    })
}

/// Hängt `" x y"` an (mit führendem Leerzeichen, wenn der Text nicht leer ist).
fn sep(d: &mut String) {
    if !d.is_empty() {
        d.push(' ');
    }
}

fn push_cmd(d: &mut String, cmd: char, vals: &[f64]) -> Result<(), DrawError> {
    sep(d);
    d.push(cmd);
    for &v in vals {
        d.push(' ');
        d.push_str(&num(v)?);
    }
    Ok(())
}

fn move_to(d: &mut String, p: DVec2) -> Result<(), DrawError> {
    push_cmd(d, 'M', &[p.x, p.y])
}

/// Eindeutiger Name: bei Wiederholung ` 2`, ` 3`, … anhängen; leer wird zu „Ansicht“.
fn unique_name(used: &mut BTreeSet<String>, base: &str) -> String {
    let base = if base.is_empty() { "Ansicht" } else { base };
    let mut name = base.to_string();
    let mut n = 2;
    while used.contains(&name) {
        name = format!("{base} {n}");
        n += 1;
    }
    used.insert(name.clone());
    name
}

/// XML-Maskierung für Attributwerte; Zeichen, die XML 1.0 verbietet, werden zu Leerzeichen.
fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&apos;"),
            '\t' => o.push_str("&#9;"),
            '\n' => o.push_str("&#10;"),
            '\r' => o.push_str("&#13;"),
            c if c < ' ' || c == '\u{FFFE}' || c == '\u{FFFF}' => o.push(' '),
            c => o.push(c),
        }
    }
    o
}

// ---------------------------------------------------------------------------------------------
// Pfade
// ---------------------------------------------------------------------------------------------

/// Hängt die Pfadbefehle einer Kurve an `d` an (jede Kurve beginnt mit eigenem `M`).
fn curve_path(d: &mut String, curve: &Out2d) -> Result<(), DrawError> {
    if !curve.is_finite() {
        return Err(DrawError::NonFinite);
    }
    match curve {
        Out2d::Line { a, b } => {
            move_to(d, *a)?;
            push_cmd(d, 'L', &[b.x, b.y])
        }
        Out2d::Arc { c, r, .. } => {
            let (c, r) = (*c, *r);
            let pt = move |t: f64| c + DVec2::new(t.cos(), t.sin()) * r;
            let rr = r.abs();
            conic_path(d, curve, &pt, [rr, rr, 0.0], 1)
        }
        Out2d::Ellipse {
            c, major, ratio, ..
        } => {
            let (c, major, ratio) = (*c, *major, *ratio);
            let minor = DVec2::new(-major.y, major.x) * ratio;
            let pt = move |t: f64| c + major * t.cos() + minor * t.sin();
            let rx = major.length();
            let rot = major.y.atan2(major.x).to_degrees();
            // Negatives Verhältnis kehrt die Umlaufrichtung um.
            let sweep = i32::from(ratio >= 0.0);
            conic_path(d, curve, &pt, [rx, ratio.abs() * rx, rot], sweep)
        }
        Out2d::Spline {
            degree,
            knots,
            ctrl,
            weights,
        } => {
            let pieces =
                bezier_pieces(*degree, knots, ctrl, weights).ok_or(DrawError::NonFinite)?;
            spline_path(d, &pieces)
        }
    }
}

/// Bogen oder Ellipsenbogen in Stücken von höchstens 90°; alle Stücke mit `large-arc = 0`.
fn conic_path(
    d: &mut String,
    curve: &Out2d,
    pt: &dyn Fn(f64) -> DVec2,
    [rx, ry, rot]: [f64; 3],
    sweep: i32,
) -> Result<(), DrawError> {
    let Some((s0, span)) = curve.normalized_range() else {
        return Err(DrawError::NonFinite);
    };
    if span <= 0.0 {
        return Ok(());
    }
    let n = ((span / (0.5 * PI)) - 1e-9).ceil().max(1.0) as usize;
    let start = pt(s0);
    move_to(d, start)?;
    let full = (span - TAU).abs() < 1e-12;
    for i in 1..=n {
        let end = if i == n && full {
            start
        } else {
            pt(s0 + span * i as f64 / n as f64)
        };
        sep(d);
        d.push('A');
        for v in [rx, ry, rot] {
            d.push(' ');
            d.push_str(&num(v)?);
        }
        d.push_str(&format!(" 0 {sweep} {} {}", num(end.x)?, num(end.y)?));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Splines
// ---------------------------------------------------------------------------------------------

type H3 = [f64; 3];

fn cart(h: H3) -> DVec2 {
    DVec2::new(h[0] / h[2], h[1] / h[2])
}

/// de Casteljau in homogenen Koordinaten auf `[0, 1]`.
fn bez_eval(q: &[H3], u: f64) -> H3 {
    let mut t = q.to_vec();
    for r in 1..t.len() {
        for i in 0..t.len() - r {
            let next = t[i + 1];
            for (v, n) in t[i].iter_mut().zip(next) {
                *v = (1.0 - u) * *v + u * n;
            }
        }
    }
    t[0]
}

/// Kontrollpunkte der Ableitung (`p·(Q[i+1] − Q[i])`).
fn hodograph(q: &[H3]) -> Vec<H3> {
    let p = (q.len() - 1) as f64;
    q.windows(2)
        .map(|w| {
            [
                p * (w[1][0] - w[0][0]),
                p * (w[1][1] - w[0][1]),
                p * (w[1][2] - w[0][2]),
            ]
        })
        .collect()
}

/// Punkt und Ableitung nach `u` der rationalen Bézier-Kurve.
fn point_and_deriv(q: &[H3], dq: &[H3], u: f64) -> (DVec2, DVec2) {
    let h = bez_eval(q, u);
    let c = cart(h);
    let dh = bez_eval(dq, u);
    let d = (DVec2::new(dh[0], dh[1]) - c * dh[2]) / h[2];
    (c, d)
}

fn spline_path(d: &mut String, pieces: &[Vec<H3>]) -> Result<(), DrawError> {
    let mut prev_end: Option<DVec2> = None;
    for q in pieces {
        let p = q.len() - 1;
        let pts: Vec<DVec2> = q.iter().map(|&h| cart(h)).collect();
        let (first, last) = (pts[0], pts[p]);
        let continues = prev_end.is_some_and(|e| (e - first).length() <= 1e-9 * (1.0 + e.length()));
        if !continues {
            move_to(d, first)?;
        }
        prev_end = Some(last);
        let (wmin, wmax) = q
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), h| {
                (lo.min(h[2]), hi.max(h[2]))
            });
        let rational = wmax - wmin > 1e-12 * wmax;
        match (p, rational) {
            (1, _) => push_cmd(d, 'L', &[last.x, last.y])?,
            (2, false) => push_cmd(d, 'Q', &[pts[1].x, pts[1].y, last.x, last.y])?,
            (3, false) => push_cmd(
                d,
                'C',
                &[pts[1].x, pts[1].y, pts[2].x, pts[2].y, last.x, last.y],
            )?,
            _ => {
                for [c1, c2, e] in fitted_cubics(q, first, last) {
                    push_cmd(d, 'C', &[c1.x, c1.y, c2.x, c2.y, e.x, e.y])?;
                }
            }
        }
    }
    Ok(())
}

/// Abstand des Punkts `c` zur kubischen Bézier-Kurve `b` (Newton-Verfahren für den nächsten
/// Punkt, Start bei `t0`).
fn cubic_closest(b: [DVec2; 4], c: DVec2, t0: f64) -> (f64, f64) {
    let ev = |t: f64| {
        let s = 1.0 - t;
        let p = b[0] * (s * s * s)
            + b[1] * (3.0 * s * s * t)
            + b[2] * (3.0 * s * t * t)
            + b[3] * (t * t * t);
        let d1 =
            ((b[1] - b[0]) * (s * s) + (b[2] - b[1]) * (2.0 * s * t) + (b[3] - b[2]) * (t * t))
                * 3.0;
        let d2 = ((b[2] - b[1] * 2.0 + b[0]) * s + (b[3] - b[2] * 2.0 + b[1]) * t) * 6.0;
        (p, d1, d2)
    };
    let mut t = t0;
    let mut best = ((ev(t).0 - c).length(), t);
    for _ in 0..8 {
        let (p, d1, d2) = ev(t);
        let g = (p - c).dot(d1);
        let h = d1.dot(d1) + (p - c).dot(d2);
        if h.abs() <= 1e-300 || h.is_nan() {
            break;
        }
        t = (t - g / h).clamp(0.0, 1.0);
        let dist = (ev(t).0 - c).length();
        if dist < best.0 {
            best = (dist, t);
        }
    }
    best
}

/// Ein Teilstück der kubischen Näherung auf dem Parameterbereich `[u0, u1]`.
struct FitPiece {
    u0: f64,
    u1: f64,
    p0: DVec2,
    d0: DVec2,
    p1: DVec2,
    d1: DVec2,
    c1: DVec2,
    c2: DVec2,
    dev: f64,
}

impl FitPiece {
    /// Hermite-Kubik zwischen den Randpunkten mit den Randtangenten der Kurve; `dev` ist die
    /// größte Abweichung bei gleichem Parameter an `FIT_SAMPLES − 1` inneren Messpunkten (eine
    /// obere Schranke des Abstands der beiden Kurven).
    fn new(
        q: &[H3],
        dq: &[H3],
        u0: f64,
        u1: f64,
        (p0, d0): (DVec2, DVec2),
        (p1, d1): (DVec2, DVec2),
    ) -> FitPiece {
        let h = u1 - u0;
        let bern = |s: f64| {
            let t = 1.0 - s;
            [t * t * t, 3.0 * t * t * s, 3.0 * t * s * s, s * s * s]
        };
        let on_curve: Vec<DVec2> = (1..FIT_SAMPLES)
            .map(|k| point_and_deriv(q, dq, u0 + k as f64 / FIT_SAMPLES as f64 * h).0)
            .collect();
        // Messpunkte mit Sehnenlängen-Parameter (Anteil der Länge des Polygonzugs), damit die
        // Anpassung nicht von der Geschwindigkeit der Parametrisierung abhängt.
        let mut cum = vec![0.0f64];
        let mut prev = p0;
        for &c in on_curve.iter().chain(std::iter::once(&p1)) {
            cum.push(cum[cum.len() - 1] + (c - prev).length());
            prev = c;
        }
        let total = cum[cum.len() - 1];
        let sample: Vec<(f64, DVec2)> = on_curve
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                let t = if total > 0.0 {
                    cum[i + 1] / total
                } else {
                    (i + 1) as f64 / FIT_SAMPLES as f64
                };
                (t, c)
            })
            .collect();
        // Die Längen der Randtangenten `a`, `b` (Steuerpunkte `p0 + a·d0`, `p1 − b·d1`) im Sinne
        // der kleinsten Quadrate wählen (Richtung bleibt die der Kurve, also G1 an den Rändern);
        // die Parameter der Messpunkte werden dabei mehrmals auf die Kubik nachgeführt.
        let mut ts: Vec<f64> = sample.iter().map(|&(t, _)| t).collect();
        let (mut a, mut b) = (h / 3.0, h / 3.0);
        for round in 0..FIT_ROUNDS {
            let (mut suu, mut suv, mut svv, mut sru, mut srv) = (0.0, 0.0, 0.0, 0.0, 0.0);
            for (k, &(_, c)) in sample.iter().enumerate() {
                let w = bern(ts[k]);
                let base = p0 * (w[0] + w[1]) + p1 * (w[2] + w[3]);
                let (r, u, v) = (c - base, d0 * w[1], -d1 * w[2]);
                suu += u.dot(u);
                suv += u.dot(v);
                svv += v.dot(v);
                sru += r.dot(u);
                srv += r.dot(v);
            }
            let det = suu * svv - suv * suv;
            if det.abs() > 1e-12 * (suu * svv).abs() && det.is_finite() {
                let (la, lb) = ((sru * svv - srv * suv) / det, (srv * suu - sru * suv) / det);
                // Nur übernehmen, wenn sinnvoll (positiv, nicht viel länger als die Parameterbreite).
                if la.is_finite() && lb.is_finite() && la > 0.0 && lb > 0.0 && la < h && lb < h {
                    (a, b) = (la, lb);
                }
            }
            if round + 1 < FIT_ROUNDS {
                let cb = [p0, p0 + d0 * a, p1 - d1 * b, p1];
                for (k, &(_, c)) in sample.iter().enumerate() {
                    ts[k] = cubic_closest(cb, c, ts[k]).1;
                }
            }
        }
        // Abweichung geometrisch: Abstand jedes Messpunkts der Kurve zur Kubik (nächster Punkt
        // über Newton-Schritte). Danach die Längen noch direkt auf die kleinste größte
        // Abweichung verbessern (Mustersuche).
        let max_dev = |a: f64, b: f64| {
            let cb = [p0, p0 + d0 * a, p1 - d1 * b, p1];
            sample
                .iter()
                .zip(&ts)
                .map(|(&(_, c), &t)| cubic_closest(cb, c, t).0)
                .fold(0.0f64, f64::max)
        };
        let mut dev = max_dev(a, b);
        let mut step = 0.05 * a;
        for _ in 0..FIT_SEARCH {
            let mut moved = false;
            for (da, db) in [(step, 0.0), (-step, 0.0), (0.0, step), (0.0, -step)] {
                let (na, nb) = (a + da, b + db);
                if na > 0.0 && nb > 0.0 {
                    let d = max_dev(na, nb);
                    if d < dev {
                        (a, b, dev, moved) = (na, nb, d, true);
                        break;
                    }
                }
            }
            if !moved {
                step *= 0.5;
            }
        }
        let c1 = p0 + d0 * a;
        let c2 = p1 - d1 * b;
        FitPiece {
            u0,
            u1,
            p0,
            d0,
            p1,
            d1,
            c1,
            c2,
            dev,
        }
    }
}

/// Kubische Näherung eines (rationalen oder höhergradigen) Bézier-Stücks in homogenen
/// Kontrollpunkten `q` mit den kartesischen Endpunkten `first`, `last` (siehe [`adaptive_fit`]).
/// Gemeinsam für SVG und PDF; liefert je Teil `[Steuerpunkt 1, Steuerpunkt 2, Endpunkt]`.
pub(crate) fn fitted_cubics(q: &[H3], first: DVec2, last: DVec2) -> Vec<[DVec2; 3]> {
    let dq = hodograph(q);
    adaptive_fit(q, &dq, first, last)
}

/// Kubische Hermite-Näherung eines Bézier-Stücks: zuerst 8 gleiche Teile, dann wird das Teil
/// mit der größten gemessenen Abweichung halbiert, bis alle Teile höchstens `FIT_TOL`
/// abweichen oder 64 Teile erreicht sind. Liefert je Teil `[Steuerpunkt 1, Steuerpunkt 2,
/// Endpunkt]`.
fn adaptive_fit(q: &[H3], dq: &[H3], first: DVec2, last: DVec2) -> Vec<[DVec2; 3]> {
    let n0 = FIT_START;
    let at = |u: f64| -> (DVec2, DVec2) {
        let (p, d) = point_and_deriv(q, dq, u);
        if u <= 0.0 {
            (first, d)
        } else if u >= 1.0 {
            (last, d)
        } else {
            (p, d)
        }
    };
    let knots: Vec<(DVec2, DVec2)> = (0..=n0).map(|i| at(i as f64 / n0 as f64)).collect();
    let mut pieces: Vec<FitPiece> = (0..n0)
        .map(|i| {
            FitPiece::new(
                q,
                dq,
                i as f64 / n0 as f64,
                (i + 1) as f64 / n0 as f64,
                knots[i],
                knots[i + 1],
            )
        })
        .collect();
    while pieces.len() < FIT_MAX {
        let Some((worst, _)) = pieces
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.dev.total_cmp(&b.1.dev))
            .filter(|(_, p)| p.dev > FIT_TOL)
            .map(|(i, p)| (i, p.dev))
        else {
            break;
        };
        let w = &pieces[worst];
        let (u0, u1) = (w.u0, w.u1);
        let um = 0.5 * (u0 + u1);
        let mid = at(um);
        let left = FitPiece::new(q, dq, u0, um, (w.p0, w.d0), mid);
        let right = FitPiece::new(q, dq, um, u1, mid, (w.p1, w.d1));
        pieces[worst] = left;
        pieces.insert(worst + 1, right);
    }
    pieces.iter().map(|p| [p.c1, p.c2, p.p1]).collect()
}
