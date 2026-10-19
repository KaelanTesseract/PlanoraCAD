//! PDF-Schreiber für Blattmodelle: eine Seite je Blatt, von Hand geschrieben (PDF 1.4, ohne
//! Bibliothek). Inhalt jeder Seite = genau der Inhalt der SVG-Ausgabe desselben Blatts
//! (`svg.rs`): jedes SVG-Element wird genau ein PDF-Element, in derselben Reihenfolge.
//!
//! * **P1 Dateiaufbau** (Zeilenende `\n`, ein Leerzeichen zwischen Elementen): `%PDF-1.4`;
//!   Objekt 1 Katalog, 2 Seitenbaum (`/Kids` in Eingabereihenfolge, `/Count n`), 3 Schrift
//!   (Helvetica, WinAnsi), 4 Dokumenteigenschaften; je Seite i (ab 0) Objekt `5 + 2i` (Seite)
//!   und `6 + 2i` (Inhaltsstrom, unkomprimiert, `/Length` = Bytes zwischen `stream\n` und
//!   `\nendstream`). Danach die Querverweistabelle mit Einträgen zu genau 20 Bytes, Trailer,
//!   `startxref` und `%%EOF\n`. Die ganze Datei ist ASCII.
//! * **P2 Zahlen:** pt = mm · 72/25,4 ([`PT_PER_MM`]), auf 4 Nachkommastellen, ohne Exponent,
//!   ohne nachgestellte Nullen, `-0` → `0` ([`pdf_num`]); auch für `MediaBox`, `w`, `d`, `Tf`,
//!   `Tm`, `Tz`. Eine nicht endliche Zahl → [`DrawError::NonFinite`], nichts geschrieben. Der
//!   Nullpunkt liegt wie im Blattmodell unten links (keine Spiegelung).
//! * **P3 Inhaltsstrom:** beginnt mit `1 J`/`0 j`: runde Linienenden wie SVG
//!   (`stroke-linecap="round"`) und spitze Ecken wie SVG (ohne `stroke-linejoin` gilt dort
//!   `miter`). Die Gehrungsgrenze bleibt die PDF-Vorgabe 10 (SVG-Vorgabe 4); sie wirkt nur bei
//!   Ecken unter etwa 29° innerhalb eines Teilpfads (Spline Grad 1, Polygone), die in den
//!   Zeichnungen kaum vorkommen. Reihenfolge
//!   wie SVG: Rahmen, dann je Ansicht sichtbarer Pfad, verdeckter Pfad, Maße (mit verlorenen),
//!   Mittellinien, Positionen. Strich `{Farbe} RG {w} w [{Muster}] 0 d` + Pfad + `S`; Fläche
//!   (Pfeil) `{Farbe} rg` + `m l l h f`; Text `{Farbe} rg BT /F1 {Größe} Tf {a b c d e f} Tm
//!   [{tz} Tz] ({…}) Tj [100 Tz] ET`. Schwarz `0 0 0`, verloren `0.8 0 0` (= SVG `#c00`). Jede
//!   Anweisung steht auf einer eigenen Zeile.
//! * **P4 Strichbreiten** wie SVG: sichtbar 0,5 mm, verdeckt 0,25 mm mit Muster 3 1;
//!   `Dimension`/`Lost` `DIM_LINE_W`; `CenterLine` `CENTER_W` mit `CENTER_PATTERN`; Rahmenmarken
//!   `width_at(frame.scale)`; Ansichtsmarken `Sheet`/`Position` `width_at(sheet_scale(Blatt))`.
//! * **P5 Kurven:** Kreis- und Ellipsenbögen über `Out2d::normalized_range` in
//!   `n = ceil(Spanne/(π/2) − 1e-9)` Stücke (wie SVG), je Stück eine Kubik mit
//!   `P1 = P0 + κ·P′(t0)`, `P2 = P3 − κ·P′(t1)`, `κ = 4/3·tan(h/4)` (h = Stückspanne); ein voller
//!   Umlauf endet exakt im Anfangspunkt. `Mark::Arc` ebenso (r ≤ 0 oder Spanne ≤ 0 → nichts).
//!   Splines: Grad 1 → `l`, Grad 2 → Gradanhebung, Grad 3 → `c`, rational oder höher → die
//!   kubische Näherung aus `svg.rs` (`fitted_cubics`, gemeinsam genutzt).
//! * **P6 Texte:** Helvetica (`/F1`), WinAnsi; Größe = Texthöhe in pt; Ursprung = Anker −
//!   k·W·(cos α, sin α) mit k = 0/½/1 (Anfang/Mitte/Ende) und der tatsächlich gezeichneten
//!   Helvetica-Breite W (nach einer Stauchung). **Stauchung nur gegen die Zelle:** Trägt der
//!   Text eine Einpassbreite (`Mark::Text::fit_width`, gesetzt von den Zellen des Blattrahmens,
//!   der Stückliste und den Nummernkreisen) und ist die Helvetica-Breite größer als sie, wird
//!   mit `Tz = 100 · Zellbreite / Helvetica-Breite` gestaucht und danach `100 Tz` gesetzt;
//!   Texte ohne Einpassbreite (Maße, Raster, Beschriftungen) werden nie gestaucht. Die
//!   Schätzungen des Blattmodells (`sheet_text_width` zählt z. B. „1“ mit 0,3·h, Helvetica hat
//!   0,556·h) sind kein Maßstab: sonst würden kurze Texte wie „1“ oder „1:1“ sichtbar
//!   schmal. Der Faktor wird mit der geschriebenen (gerundeten) Schriftgröße bestimmt und auf
//!   vier Stellen **abgerundet** (Abweichung vom Plan, der „gerundet“ sagt: mit Rundung zum
//!   Nächsten wäre der gezeichnete Text bis zu ~1e-4 mm breiter als die Zelle). `−` (U+2212)
//!   wird `-`; `( ) \` werden
//!   maskiert, Bytes ≥ 0x80 als `\ddd` (oktal). Ein Zeichen ohne WinAnsi-Platz →
//!   `DrawError::Unsupported(pdf_char_message(c))`, nichts geschrieben.
//! * **P7 Dokumenteigenschaften:** `/Producer` und `/Creator` „PlanoraCAD“, `/Title` und
//!   `/Author` als UTF-16BE-Hex mit BOM (fehlen, wenn leer), `/CreationDate (D:JJJJMMTT000000Z)`
//!   nur bei gültigem Datum `JJJJ-MM-TT`. Keine `/ID`, kein `/ModDate`: gleiche Eingabe ergibt
//!   gleiche Bytes.
//! * **P8 Grenzen:** 0 Seiten → [`NO_PAGES`], mehr als [`MAX_PAGES`] → [`TOO_MANY_PAGES`], eine
//!   Kante über [`MAX_PAGE_PT`] → [`PAGE_TOO_LARGE`].

use crate::Visibility;
use crate::annotate::{
    ARROW_HALF_W, ARROW_LEN, CENTER_PATTERN, CENTER_W, DIM_LINE_W, Mark, MarkKind, TextAnchor,
};
use crate::error::DrawError;
use crate::out2d::{Out2d, bezier_pieces};
use crate::pdf_font::{pdf_text_width, winansi_byte};
use crate::sheet::SheetModel;
use crate::sheet_frame::sheet_scale;
use crate::svg::fitted_cubics;
use planora_math::{DVec2, PI, TAU};
use std::fmt::Write as _;
use std::io::Write;

/// Punkte je Millimeter.
pub const PT_PER_MM: f64 = 72.0 / 25.4;
/// Höchstzahl der Seiten einer PDF.
pub const MAX_PAGES: usize = 200;
/// Längste Seitenkante in pt.
pub const MAX_PAGE_PT: f64 = 14_400.0;
/// Name des Programms in den Dokumenteigenschaften.
pub const PRODUCER: &str = "PlanoraCAD";
pub const NO_PAGES: &str = "Es gibt kein Blatt für die PDF.";
pub const TOO_MANY_PAGES: &str = "Eine PDF kann höchstens 200 Blätter enthalten.";
pub const PAGE_TOO_LARGE: &str = "Das Blatt ist für eine PDF zu groß.";

/// Sichtbare Linien (mm).
const VISIBLE_W: f64 = 0.5;
/// Verdeckte Linien (mm) und ihr Muster.
const HIDDEN_W: f64 = 0.25;
const HIDDEN_PATTERN: [f64; 2] = [3.0, 1.0];
/// Farben wie SVG (`#000`, `#c00`).
const BLACK: &str = "0 0 0";
const LOST_RGB: &str = "0.8 0 0";

/// Dokumenteigenschaften der PDF.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PdfMeta {
    pub title: String,
    pub author: Option<String>,
    /// Datum „JJJJ-MM-TT“; ungültig oder `None` → kein Erstelldatum.
    pub date: Option<String>,
}

/// Zeichen für eine Meldung: sichtbare Zeichen in Anführungszeichen, unsichtbare oder
/// verwechselbare (Leerzeichen, Steuer-, Format-, Kombinationszeichen, Sonderstriche) als „U+XXXX“.
pub fn pdf_char_label(c: char) -> String {
    let u = c as u32;
    let hidden = c.is_whitespace()
        || c.is_control()
        || matches!(u, 0x300..=0x36F | 0x2010..=0x2015 | 0x200B..=0x200F | 0x2028..=0x202F | 0x2060..=0x206F | 0xFEFF);
    if hidden {
        format!("U+{u:04X}")
    } else {
        format!("„{c}“")
    }
}

/// Meldung für ein Zeichen, das eine PDF nicht ausgeben kann.
pub fn pdf_char_message(c: char) -> String {
    format!(
        "Das Zeichen {} lässt sich in einer PDF nicht ausgeben.",
        pdf_char_label(c)
    )
}

/// Zahl auf vier Nachkommastellen, ohne Exponent und ohne nachgestellte Nullen; `-0` wird
/// `0`. Nicht endliche Zahlen sind ein Fehler.
pub fn pdf_num(x: f64) -> Result<String, DrawError> {
    if !x.is_finite() {
        return Err(DrawError::NonFinite);
    }
    let t = format!("{x:.4}");
    let t = t.trim_end_matches('0').trim_end_matches('.');
    Ok(if t == "-0" || t.is_empty() {
        "0".to_string()
    } else {
        t.to_string()
    })
}

/// „JJJJ-MM-TT“ → „D:JJJJMMTT000000Z“ (Monat 1–12, Tag passend zum Monat, 29. Februar nur in
/// Schaltjahren); sonst `None`.
pub fn pdf_date(date: &str) -> Option<String> {
    let b = date.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let digits = |r: std::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
    if !(digits(0..4) && digits(5..7) && digits(8..10)) {
        return None;
    }
    let year: u32 = date[0..4].parse().ok()?;
    let month: u32 = date[5..7].parse().ok()?;
    let day: u32 = date[8..10].parse().ok()?;
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    if !(1..=days).contains(&day) {
        return None;
    }
    Some(format!(
        "D:{}{}{}000000Z",
        &date[0..4],
        &date[5..7],
        &date[8..10]
    ))
}

/// Passt eine Seite der Größe `w × h` (pt) in eine PDF?
pub fn fits_page_pt(w: f64, h: f64) -> bool {
    w <= MAX_PAGE_PT && h <= MAX_PAGE_PT
}

/// Schreibt die Blätter als PDF (eine Seite je Blatt). Bei einem Fehler wird nichts geschrieben.
pub fn write_pdf(
    pages: &[&SheetModel],
    meta: &PdfMeta,
    out: &mut dyn Write,
) -> Result<(), DrawError> {
    let bytes = pdf_bytes(pages, meta)?;
    out.write_all(&bytes)?;
    Ok(())
}

/// Die Blätter als PDF-Datei (eine Seite je Blatt).
pub fn pdf_bytes(pages: &[&SheetModel], meta: &PdfMeta) -> Result<Vec<u8>, DrawError> {
    if pages.is_empty() {
        return Err(DrawError::Unsupported(NO_PAGES.into()));
    }
    if pages.len() > MAX_PAGES {
        return Err(DrawError::Unsupported(TOO_MANY_PAGES.into()));
    }
    let mut objects: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        String::new(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(),
        info_dict(meta),
    ];
    let mut kids = Vec::with_capacity(pages.len());
    for (i, model) in pages.iter().enumerate() {
        let (w, h) = model.sheet.size_mm();
        let (w, h) = (w * PT_PER_MM, h * PT_PER_MM);
        if !fits_page_pt(w, h) {
            return Err(DrawError::Unsupported(PAGE_TOO_LARGE.into()));
        }
        let content = page_content(model)?;
        let (page, stream) = (5 + 2 * i, 6 + 2 * i);
        kids.push(format!("{page} 0 R"));
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 3 0 R >> >> /Contents {stream} 0 R >>",
            pdf_num(w)?,
            pdf_num(h)?
        ));
        objects.push(format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        ));
    }
    objects[1] = format!(
        "<< /Type /Pages /Kids [{}] /Count {} >>",
        kids.join(" "),
        pages.len()
    );

    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = Vec::with_capacity(objects.len());
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        let _ = write!(out, "{} 0 obj\n{body}\nendobj\n", i + 1);
    }
    let xref = out.len();
    let size = objects.len() + 1;
    let _ = write!(out, "xref\n0 {size}\n0000000000 65535 f \n");
    for off in offsets {
        // Genau 20 Bytes: 10 Ziffern, Leerzeichen, 5 Ziffern, „ n “, Zeilenende.
        let _ = writeln!(out, "{off:010} 00000 n ");
    }
    let _ = write!(
        out,
        "trailer\n<< /Size {size} /Root 1 0 R /Info 4 0 R >>\nstartxref\n{xref}\n%%EOF\n"
    );
    debug_assert!(out.is_ascii());
    Ok(out.into_bytes())
}

/// Dokumenteigenschaften (P7).
fn info_dict(meta: &PdfMeta) -> String {
    let mut s = format!("<< /Producer ({PRODUCER}) /Creator ({PRODUCER})");
    if !meta.title.is_empty() {
        let _ = write!(s, " /Title {}", utf16_hex(&meta.title));
    }
    if let Some(a) = meta.author.as_deref().filter(|a| !a.is_empty()) {
        let _ = write!(s, " /Author {}", utf16_hex(a));
    }
    if let Some(d) = meta.date.as_deref().and_then(pdf_date) {
        let _ = write!(s, " /CreationDate ({d})");
    }
    s.push_str(" >>");
    s
}

/// Text als UTF-16BE-Hex-String mit BOM.
fn utf16_hex(text: &str) -> String {
    let mut s = String::from("<FEFF");
    for u in text.encode_utf16() {
        let _ = write!(s, "{u:04X}");
    }
    s.push('>');
    s
}

// ---------------------------------------------------------------------------------------------
// Inhaltsstrom
// ---------------------------------------------------------------------------------------------

/// Millimeter als pt-Zahl.
fn pt(mm: f64) -> Result<String, DrawError> {
    pdf_num(mm * PT_PER_MM)
}

/// Der Inhaltsstrom einer Seite (Zeilen mit `\n` verbunden, ohne Zeilenende am Schluss).
fn page_content(model: &SheetModel) -> Result<String, DrawError> {
    // Runde Enden, spitze Ecken (wie SVG, siehe P3).
    let mut lines: Vec<String> = vec!["1 J".into(), "0 j".into()];
    if let Some(frame) = &model.frame {
        for m in &frame.marks {
            mark_element(&mut lines, m, frame.scale)?;
        }
    }
    let scale = sheet_scale(model.sheet);
    for view in &model.views {
        let mut visible = Vec::new();
        let mut hidden = Vec::new();
        for line in &view.lines {
            let ops = match line.vis {
                Visibility::Visible => &mut visible,
                Visibility::Hidden => &mut hidden,
            };
            curve_ops(ops, &line.curve)?;
        }
        if !visible.is_empty() {
            stroke(&mut lines, BLACK, VISIBLE_W, &[], visible)?;
        }
        if !hidden.is_empty() {
            stroke(&mut lines, BLACK, HIDDEN_W, &HIDDEN_PATTERN, hidden)?;
        }
        for group in [MarkGroup::Dims, MarkGroup::Centre, MarkGroup::Positions] {
            for m in view.marks.iter().filter(|m| mark_group(m) == group) {
                mark_element(&mut lines, m, scale)?;
            }
        }
    }
    Ok(lines.join("\n"))
}

/// Ein Strich: Farbe, Breite (mm), Muster (mm), Pfad, `S`.
fn stroke(
    lines: &mut Vec<String>,
    rgb: &str,
    width: f64,
    dash: &[f64],
    path: Vec<String>,
) -> Result<(), DrawError> {
    let dash = dash.iter().map(|&d| pt(d)).collect::<Result<Vec<_>, _>>()?;
    lines.push(format!("{rgb} RG"));
    lines.push(format!("{} w", pt(width)?));
    lines.push(format!("[{}] 0 d", dash.join(" ")));
    lines.extend(path);
    lines.push("S".into());
    Ok(())
}

fn move_to(ops: &mut Vec<String>, p: DVec2) -> Result<(), DrawError> {
    ops.push(format!("{} {} m", pt(p.x)?, pt(p.y)?));
    Ok(())
}

fn line_to(ops: &mut Vec<String>, p: DVec2) -> Result<(), DrawError> {
    ops.push(format!("{} {} l", pt(p.x)?, pt(p.y)?));
    Ok(())
}

fn curve_to(ops: &mut Vec<String>, c1: DVec2, c2: DVec2, e: DVec2) -> Result<(), DrawError> {
    ops.push(format!(
        "{} {} {} {} {} {} c",
        pt(c1.x)?,
        pt(c1.y)?,
        pt(c2.x)?,
        pt(c2.y)?,
        pt(e.x)?,
        pt(e.y)?
    ));
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Markierungen
// ---------------------------------------------------------------------------------------------

/// Gruppe einer Markierung innerhalb ihrer Ansicht (Reihenfolge wie `svg::mark_groups`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum MarkGroup {
    Dims,
    Centre,
    Positions,
}

fn mark_kind(m: &Mark) -> MarkKind {
    match m {
        Mark::Line { kind, .. }
        | Mark::Arrow { kind, .. }
        | Mark::Arc { kind, .. }
        | Mark::Text { kind, .. } => *kind,
    }
}

fn mark_group(m: &Mark) -> MarkGroup {
    match mark_kind(m) {
        MarkKind::CenterLine => MarkGroup::Centre,
        MarkKind::Position(_) => MarkGroup::Positions,
        _ => MarkGroup::Dims,
    }
}

fn color(kind: MarkKind) -> &'static str {
    match kind {
        MarkKind::Lost => LOST_RGB,
        MarkKind::Dimension | MarkKind::CenterLine | MarkKind::Sheet(_) | MarkKind::Position(_) => {
            BLACK
        }
    }
}

/// Ein Strich mit den Darstellungsmerkmalen einer Markierung (wie `svg::stroke_path`).
fn mark_stroke(
    lines: &mut Vec<String>,
    kind: MarkKind,
    sheet_scale: f64,
    path: Vec<String>,
) -> Result<(), DrawError> {
    let (width, dash): (f64, &[f64]) = match kind {
        MarkKind::CenterLine => (CENTER_W, &CENTER_PATTERN),
        MarkKind::Dimension | MarkKind::Lost => (DIM_LINE_W, &[]),
        MarkKind::Sheet(pen) => (pen.width_at(sheet_scale), &[]),
        MarkKind::Position(pen) => (pen.width_at(sheet_scale), &[]),
    };
    stroke(lines, color(kind), width, dash, path)
}

fn mark_element(lines: &mut Vec<String>, m: &Mark, sheet_scale: f64) -> Result<(), DrawError> {
    match m {
        Mark::Line { a, b, kind } => {
            let mut ops = Vec::new();
            move_to(&mut ops, *a)?;
            line_to(&mut ops, *b)?;
            mark_stroke(lines, *kind, sheet_scale, ops)?;
        }
        Mark::Arrow { tip, dir, kind } => {
            let base = *tip + *dir * ARROW_LEN;
            let perp = DVec2::new(-dir.y, dir.x) * ARROW_HALF_W;
            let mut ops = Vec::new();
            move_to(&mut ops, *tip)?;
            line_to(&mut ops, base + perp)?;
            line_to(&mut ops, base - perp)?;
            lines.push(format!("{} rg", color(*kind)));
            lines.extend(ops);
            lines.push("h".into());
            lines.push("f".into());
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
            let (c, r) = (*center, *r);
            let mut ops = Vec::new();
            arc_pieces(
                &mut ops,
                *start,
                span,
                &|t| c + DVec2::new(t.cos(), t.sin()) * r,
                &|t| DVec2::new(-t.sin(), t.cos()) * r,
            )?;
            mark_stroke(lines, *kind, sheet_scale, ops)?;
        }
        Mark::Text { .. } => text_element(lines, m)?,
    }
    Ok(())
}

/// Ein Text nach P6 (`m` ist ein `Mark::Text`).
fn text_element(lines: &mut Vec<String>, m: &Mark) -> Result<(), DrawError> {
    let &Mark::Text {
        pos,
        height,
        angle,
        anchor,
        ref text,
        kind,
        fit_width,
    } = m
    else {
        return Ok(());
    };
    let mut bytes = Vec::with_capacity(text.len());
    for c in text.chars() {
        bytes.push(winansi_byte(c).ok_or_else(|| DrawError::Unsupported(pdf_char_message(c)))?);
    }
    let size_s = pt(height)?;
    let size: f64 = size_s.parse().map_err(|_| DrawError::NonFinite)?;
    // Breiten in pt mit der geschriebenen Schriftgröße.
    let helv = pdf_text_width(text, size).ok_or(DrawError::NonFinite)?;
    // Gestaucht wird nur gegen die Zelle, in die der Text eingepasst wurde (P6).
    let cell = match fit_width {
        Some(w) if !w.is_finite() => return Err(DrawError::NonFinite),
        Some(w) => Some(w * PT_PER_MM),
        None => None,
    };
    let tz = match cell {
        Some(cell) if helv > cell => {
            // Auf vier Stellen abgerundet: der gezeichnete Text ist nie breiter als die Zelle.
            let mut t = (100.0 * cell / helv * 1e4).floor();
            while t > 0.0 && helv * (t / 1e4) / 100.0 > cell {
                t -= 1.0;
            }
            let s = pdf_num(t.max(0.0) / 1e4)?;
            let v: f64 = s.parse().map_err(|_| DrawError::NonFinite)?;
            Some((s, v))
        }
        _ => None,
    };
    let width = helv * tz.as_ref().map_or(100.0, |t| t.1) / 100.0;
    let k = match anchor {
        TextAnchor::Start => 0.0,
        TextAnchor::Middle => 0.5,
        TextAnchor::End => 1.0,
    };
    let (sin, cos) = angle.sin_cos();
    let x = pos.x * PT_PER_MM - k * width * cos;
    let y = pos.y * PT_PER_MM - k * width * sin;
    lines.push(format!("{} rg", color(kind)));
    lines.push("BT".into());
    lines.push(format!("/F1 {size_s} Tf"));
    lines.push(format!(
        "{} {} {} {} {} {} Tm",
        pdf_num(cos)?,
        pdf_num(sin)?,
        pdf_num(-sin)?,
        pdf_num(cos)?,
        pdf_num(x)?,
        pdf_num(y)?
    ));
    if let Some((s, _)) = &tz {
        lines.push(format!("{s} Tz"));
    }
    lines.push(format!("({}) Tj", escape(&bytes)));
    if tz.is_some() {
        lines.push("100 Tz".into());
    }
    lines.push("ET".into());
    Ok(())
}

/// PDF-String-Inhalt: `( ) \` maskiert, Bytes ab 0x80 oktal.
fn escape(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len());
    for &b in bytes {
        match b {
            b'(' | b')' | b'\\' => {
                s.push('\\');
                s.push(char::from(b));
            }
            0x80.. => {
                let _ = write!(s, "\\{b:03o}");
            }
            _ => s.push(char::from(b)),
        }
    }
    s
}

// ---------------------------------------------------------------------------------------------
// Kurven
// ---------------------------------------------------------------------------------------------

/// Hängt die Pfadanweisungen einer Kurve an (jede Kurve beginnt mit eigenem `m`).
fn curve_ops(ops: &mut Vec<String>, curve: &Out2d) -> Result<(), DrawError> {
    if !curve.is_finite() {
        return Err(DrawError::NonFinite);
    }
    match curve {
        Out2d::Line { a, b } => {
            move_to(ops, *a)?;
            line_to(ops, *b)
        }
        Out2d::Arc { c, r, .. } => {
            let (c, r) = (*c, *r);
            conic_ops(
                ops,
                curve,
                &|t| c + DVec2::new(t.cos(), t.sin()) * r,
                &|t| DVec2::new(-t.sin(), t.cos()) * r,
            )
        }
        Out2d::Ellipse {
            c, major, ratio, ..
        } => {
            let (c, major) = (*c, *major);
            let minor = DVec2::new(-major.y, major.x) * *ratio;
            conic_ops(
                ops,
                curve,
                &|t| c + major * t.cos() + minor * t.sin(),
                &|t| minor * t.cos() - major * t.sin(),
            )
        }
        Out2d::Spline {
            degree,
            knots,
            ctrl,
            weights,
        } => {
            let pieces =
                bezier_pieces(*degree, knots, ctrl, weights).ok_or(DrawError::NonFinite)?;
            spline_ops(ops, &pieces)
        }
    }
}

/// Bogen oder Ellipsenbogen über den genormten Bereich der Kurve.
fn conic_ops(
    ops: &mut Vec<String>,
    curve: &Out2d,
    pt_at: &dyn Fn(f64) -> DVec2,
    deriv: &dyn Fn(f64) -> DVec2,
) -> Result<(), DrawError> {
    let Some((s0, span)) = curve.normalized_range() else {
        return Err(DrawError::NonFinite);
    };
    if span <= 0.0 {
        return Ok(());
    }
    arc_pieces(ops, s0, span, pt_at, deriv)
}

/// Stücke von höchstens 90° ab `s0` über `span` (> 0), je Stück eine Kubik; ein voller Umlauf
/// endet exakt im Anfangspunkt.
fn arc_pieces(
    ops: &mut Vec<String>,
    s0: f64,
    span: f64,
    pt_at: &dyn Fn(f64) -> DVec2,
    deriv: &dyn Fn(f64) -> DVec2,
) -> Result<(), DrawError> {
    let n = ((span / (0.5 * PI)) - 1e-9).ceil().max(1.0) as usize;
    let kappa = 4.0 / 3.0 * (span / n as f64 / 4.0).tan();
    let start = pt_at(s0);
    move_to(ops, start)?;
    let full = (span - TAU).abs() < 1e-12;
    let (mut p0, mut t0) = (start, s0);
    for i in 1..=n {
        let t1 = s0 + span * i as f64 / n as f64;
        let p3 = if i == n && full { start } else { pt_at(t1) };
        let c1 = p0 + deriv(t0) * kappa;
        let c2 = p3 - deriv(t1) * kappa;
        curve_to(ops, c1, c2, p3)?;
        (p0, t0) = (p3, t1);
    }
    Ok(())
}

fn cart(h: [f64; 3]) -> DVec2 {
    DVec2::new(h[0] / h[2], h[1] / h[2])
}

/// Bézier-Stücke eines Splines (neuer Teilpfad nur wie in `svg::spline_path`).
fn spline_ops(ops: &mut Vec<String>, pieces: &[Vec<[f64; 3]>]) -> Result<(), DrawError> {
    let mut prev_end: Option<DVec2> = None;
    for q in pieces {
        let p = q.len() - 1;
        let pts: Vec<DVec2> = q.iter().map(|&h| cart(h)).collect();
        let (first, last) = (pts[0], pts[p]);
        let continues = prev_end.is_some_and(|e| (e - first).length() <= 1e-9 * (1.0 + e.length()));
        if !continues {
            move_to(ops, first)?;
        }
        prev_end = Some(last);
        let (wmin, wmax) = q
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), h| {
                (lo.min(h[2]), hi.max(h[2]))
            });
        let rational = wmax - wmin > 1e-12 * wmax;
        match (p, rational) {
            (1, _) => line_to(ops, last)?,
            (2, false) => {
                // Gradanhebung: P0 + ⅔(P1 − P0), P2 + ⅔(P1 − P2).
                let c1 = first + (pts[1] - first) * (2.0 / 3.0);
                let c2 = last + (pts[1] - last) * (2.0 / 3.0);
                curve_to(ops, c1, c2, last)?;
            }
            (3, false) => curve_to(ops, pts[1], pts[2], last)?,
            _ => {
                for [c1, c2, e] in fitted_cubics(q, first, last) {
                    curve_to(ops, c1, c2, e)?;
                }
            }
        }
    }
    Ok(())
}
