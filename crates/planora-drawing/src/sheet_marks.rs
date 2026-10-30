//! Der Blattrahmen als Markierungen: Rahmen, Zentriermarken, Bezugsraster, Schriftfeld mit
//! Inhalten, Projektionssymbol, Änderungstabelle, Hinweisfeld und Stückliste. Die Geometrie liefert
//! [`SheetLayout`](crate::sheet_frame::SheetLayout), die Texte [`TitleTexts`]; hier entstehen
//! daraus [`Mark`]s in Blattkoordinaten (mm, y nach oben), die SVG und DXF wie alle anderen
//! Markierungen ausgeben. Alle Markierungen tragen [`MarkKind::Sheet`] mit der Linienstärke;
//! Texte haben die Stärke `Thin` und den Winkel 0. Alle Längen und Schrifthöhen sind die A3-Werte
//! mal [`SheetLayout::scale`]; die Strichbreiten dazu stehen in [`SheetFrame::scale`].
//!
//! DXF: Die Höhe eines DXF-Texts ist die Großbuchstabenhöhe, die Schriften der CAD-Programme
//! laufen damit 1,1- bis 1,7-mal breiter als die Schätzung. Für DXF passt [`sheet_frame_dxf`]
//! die Texte deshalb mit [`DXF_TEXT_WIDTH_FACTOR`] auf die geschätzte Breite ein (dieselbe Regel:
//! erst kleiner bis drei Fünftel, dann hinten gekürzt mit „…“).

use crate::annotate::{Mark, MarkKind, TextAnchor};
use crate::model::Projection;
use crate::parts::{PartsCells, parts_head};
use crate::sheet_frame::{
    GRID_TEXT_H, LABEL_H, NOTE_LINES_MAX, NOTES_LINE_MM, NOTES_PAD_MM, NOTES_TEXT_H, PARTS_ROW_H,
    PROJECTION_CENTRE_Y, PROJECTION_X0, PartsSpace, REV_ROW_H, REV_VISIBLE, SheetLayout, SheetPen,
    TitleField, VALUE_H, projection_symbol,
};
use crate::sheet_info::TitleTexts;
use planora_i18n::tr;
use planora_math::{DVec2, TAU};

/// Der Blattrahmen eines Blatts als Markierungen.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetFrame {
    pub marks: Vec<Mark>,
    /// Faktor des Blattzubehörs ([`SheetLayout::scale`]); die Ausgabe zeichnet jede Linienart
    /// mit [`SheetPen::width_at`] dieses Faktors.
    pub scale: f64,
}

/// Zuschlag auf die geschätzte Textbreite für DXF (siehe Moduldoku).
pub const DXF_TEXT_WIDTH_FACTOR: f64 = 1.45;

/// Ein eingepasster Text.
#[derive(Clone, Debug, PartialEq)]
pub struct Fitted {
    pub text: String,
    pub height: f64,
    /// Der Text musste gekürzt werden.
    pub cut: bool,
}

/// Kleinste Höhe beim Einpassen, als Anteil der Sollhöhe (3/5).
const MIN_SHRINK_NUM: f64 = 3.0;
const MIN_SHRINK_DEN: f64 = 5.0;
const ELLIPSIS: char = '…';

/// Geschätzte Breite eines Blatttextes (bewusst reichlich): je Zeichen `M W m w @ %` 0,95·h,
/// `i l j I 1 . , : ; ' | !` 0,3·h, übrige Großbuchstaben und Ziffern 0,72·h, sonst 0,6·h.
pub fn sheet_text_width(text: &str, height: f64) -> f64 {
    let factor = |c: char| -> f64 {
        match c {
            'M' | 'W' | 'm' | 'w' | '@' | '%' => 0.95,
            'i' | 'l' | 'j' | 'I' | '1' | '.' | ',' | ':' | ';' | '\'' | '|' | '!' => 0.3,
            c if c.is_uppercase() || c.is_ascii_digit() => 0.72,
            _ => 0.6,
        }
    };
    height * text.chars().map(factor).sum::<f64>()
}

/// Passt `text` in `width`: unverändert, wenn er passt; sonst kleiner (nicht unter drei Fünftel
/// von `height`); passt er auch so nicht, in der kleinsten Höhe hinten gekürzt und mit „…“
/// beendet.
pub fn fit_text(text: &str, height: f64, width: f64) -> Fitted {
    let w = sheet_text_width(text, height);
    if w <= width {
        return Fitted {
            text: text.to_string(),
            height,
            cut: false,
        };
    }
    let min_h = height * MIN_SHRINK_NUM / MIN_SHRINK_DEN;
    let mut h = height * width / w;
    if h >= min_h {
        // Gegen Rundung: so lange verkleinern, bis die Schätzung sicher passt.
        while sheet_text_width(text, h) > width && h >= min_h {
            h *= 1.0 - 1e-12;
        }
        if h >= min_h {
            return Fitted {
                text: text.to_string(),
                height: h,
                cut: false,
            };
        }
    }
    h = min_h;
    let chars: Vec<char> = text.chars().collect();
    for n in (0..=chars.len()).rev() {
        let mut cand: String = chars[..n].iter().collect();
        let trimmed = cand.trim_end().len();
        cand.truncate(trimmed);
        cand.push(ELLIPSIS);
        if sheet_text_width(&cand, h) <= width {
            return Fitted {
                text: cand,
                height: h,
                cut: true,
            };
        }
    }
    Fitted {
        text: String::new(),
        height: h,
        cut: true,
    }
}

/// Wird der Eintrag dieses Felds im Schriftfeld des Blatts `layout` gekürzt (für die Warnung im
/// Fenster)? Dieselbe Regel wie beim Zeichnen, mit den Zellbreiten des Blatts.
pub fn field_is_cut(field: TitleField, text: &str, layout: &SheetLayout) -> bool {
    layout
        .title_slots()
        .iter()
        .find(|s| s.field == field)
        .is_some_and(|s| fit_text(text, s.height, s.width).cut)
}

/// Wird der Eintrag in Spalte `col` (0 Buchstabe, 1 Beschreibung, 2 Datum, 3 Name) der
/// Änderungstabelle des Blatts `layout` gekürzt? Dieselbe Regel wie beim Zeichnen.
pub fn revision_cell_is_cut(col: usize, text: &str, layout: &SheetLayout) -> bool {
    layout
        .rev_columns()
        .get(col)
        .is_some_and(|w| fit_text(text, layout.mm(VALUE_H), w - layout.mm(2.0)).cut)
}

/// Wird die Hinweiszeile `text` im Hinweisfeld des Blatts `layout` gekürzt? Ohne Hinweisfeld
/// gibt es nichts zu kürzen.
pub fn note_is_cut(text: &str, layout: &SheetLayout) -> bool {
    layout.notes.is_some_and(|(lo, hi)| {
        fit_text(
            text,
            layout.mm(NOTES_TEXT_H),
            hi.x - lo.x - layout.mm(2.0 * NOTES_PAD_MM),
        )
        .cut
    })
}

/// Wird der Eintrag in Spalte `col` (0 Pos., 1 Benennung, 2 Menge, 3 Werkstoff, 4 Bemerkung)
/// der Stückliste des Blatts `layout` gekürzt? Dieselbe Regel wie beim Zeichnen.
pub fn parts_cell_is_cut(col: usize, text: &str, layout: &SheetLayout) -> bool {
    layout
        .parts_columns()
        .get(col)
        .is_some_and(|w| fit_text(text, layout.mm(VALUE_H), w - layout.mm(2.0)).cut)
}

fn line(a: DVec2, b: DVec2, pen: SheetPen) -> Mark {
    Mark::Line {
        a,
        b,
        kind: MarkKind::Sheet(pen),
    }
}

fn rect(marks: &mut Vec<Mark>, lo: DVec2, hi: DVec2, pen: SheetPen) {
    let p = DVec2::new;
    marks.push(line(p(lo.x, lo.y), p(hi.x, lo.y), pen));
    marks.push(line(p(hi.x, lo.y), p(hi.x, hi.y), pen));
    marks.push(line(p(hi.x, hi.y), p(lo.x, hi.y), pen));
    marks.push(line(p(lo.x, hi.y), p(lo.x, lo.y), pen));
}

fn text(marks: &mut Vec<Mark>, pos: DVec2, height: f64, anchor: TextAnchor, text: &str) {
    text_in(marks, pos, height, anchor, text, None);
}

/// Wie [`text`], dazu die Breite, in die der Text eingepasst wurde (`fit_width`).
fn text_in(
    marks: &mut Vec<Mark>,
    pos: DVec2,
    height: f64,
    anchor: TextAnchor,
    text: &str,
    fit_width: Option<f64>,
) {
    if text.is_empty() {
        return;
    }
    marks.push(Mark::Text {
        pos,
        height,
        angle: 0.0,
        anchor,
        text: text.to_string(),
        kind: MarkKind::Sheet(SheetPen::Thin),
        fit_width,
    });
}

/// Eingepasster Text bei `pos` (Anfang auf der Grundlinie); `widen` ist der Zuschlag auf die
/// geschätzte Breite (1 für SVG und Anzeige, [`DXF_TEXT_WIDTH_FACTOR`] für DXF).
fn fitted_text(marks: &mut Vec<Mark>, pos: DVec2, height: f64, width: f64, widen: f64, s: &str) {
    let room = width / widen;
    let f = fit_text(s, height, room);
    text_in(marks, pos, f.height, TextAnchor::Start, &f.text, Some(room));
}

/// Trennlinien eines Rasters aus Zellen: jede innere Kante einmal, aneinanderstoßende Stücke
/// zu einer Strecke vereint. Zellen als `(x0, x1, y0, y1)` in Blattkoordinaten.
fn cell_dividers(cells: &[(f64, f64, f64, f64)], lo: DVec2, hi: DVec2) -> Vec<(DVec2, DVec2)> {
    const EPS: f64 = 1e-9;
    // (senkrecht?, Lage, von, bis)
    let mut edges: Vec<(bool, f64, f64, f64)> = Vec::new();
    for &(x0, x1, y0, y1) in cells {
        for x in [x0, x1] {
            if x > lo.x + EPS && x < hi.x - EPS {
                edges.push((true, x, y0, y1));
            }
        }
        for y in [y0, y1] {
            if y > lo.y + EPS && y < hi.y - EPS {
                edges.push((false, y, x0, x1));
            }
        }
    }
    edges.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.total_cmp(&b.1))
            .then(a.2.total_cmp(&b.2))
    });
    let mut merged: Vec<(bool, f64, f64, f64)> = Vec::new();
    for e in edges {
        match merged.last_mut() {
            Some(last) if last.0 == e.0 && (last.1 - e.1).abs() <= EPS && e.2 <= last.3 + EPS => {
                last.3 = last.3.max(e.3);
            }
            _ => merged.push(e),
        }
    }
    merged
        .into_iter()
        .map(|(vertical, c, from, to)| {
            if vertical {
                (DVec2::new(c, from), DVec2::new(c, to))
            } else {
                (DVec2::new(from, c), DVec2::new(to, c))
            }
        })
        .collect()
}

/// Baut den Blattrahmen: Rahmen (4 Linien `Heavy`), Zentriermarken (`Medium`), Teilstriche und
/// Rasterbeschriftung, Schriftfeld (Außenlinie `Heavy`, Zelltrennlinien `Thin`, Feldnamen und
/// Inhalte), Projektionssymbol (`Medium`), Änderungstabelle und Hinweisfeld.
pub fn sheet_frame(layout: &SheetLayout, texts: &TitleTexts, projection: Projection) -> SheetFrame {
    build_frame(layout, texts, projection, 1.0)
}

/// Wie [`sheet_frame`], die Texte aber mit [`DXF_TEXT_WIDTH_FACTOR`] eingepasst (für DXF).
pub fn sheet_frame_dxf(
    layout: &SheetLayout,
    texts: &TitleTexts,
    projection: Projection,
) -> SheetFrame {
    build_frame(layout, texts, projection, DXF_TEXT_WIDTH_FACTOR)
}

fn build_frame(
    layout: &SheetLayout,
    texts: &TitleTexts,
    projection: Projection,
    widen: f64,
) -> SheetFrame {
    let mut m: Vec<Mark> = Vec::new();
    let p = DVec2::new;
    let s = |a3_mm: f64| layout.mm(a3_mm);

    // Rahmen und Zentriermarken.
    rect(&mut m, layout.frame.0, layout.frame.1, SheetPen::Heavy);
    for (a, b) in layout.centre_marks {
        m.push(line(a, b, SheetPen::Medium));
    }

    // Bezugsraster.
    for &(a, b) in &layout.grid_ticks {
        m.push(line(a, b, SheetPen::Thin));
    }
    for (c, label) in &layout.grid_labels {
        text(
            &mut m,
            p(c.x, c.y - s(GRID_TEXT_H / 2.0)),
            s(GRID_TEXT_H),
            TextAnchor::Middle,
            label,
        );
    }

    // Schriftfeld.
    let (t0, t1) = layout.title;
    rect(&mut m, t0, t1, SheetPen::Heavy);
    let cells = layout.title_cells();
    let abs: Vec<(f64, f64, f64, f64)> = cells
        .iter()
        .map(|c| (t0.x + c.x0, t0.x + c.x1, t0.y + c.y0, t0.y + c.y1))
        .collect();
    for (a, b) in cell_dividers(&abs, t0, t1) {
        m.push(line(a, b, SheetPen::Thin));
    }
    for c in &cells {
        text(
            &mut m,
            p(t0.x + c.x0 + s(1.0), t0.y + c.y1 - s(1.0) - s(LABEL_H)),
            s(LABEL_H),
            TextAnchor::Start,
            &c.label.text(),
        );
    }
    for slot in &layout.title_slots() {
        fitted_text(
            &mut m,
            p(t0.x + slot.x0, t0.y + slot.baseline),
            slot.height,
            slot.width,
            widen,
            texts.get(slot.field),
        );
    }

    // Projektionssymbol.
    let (symbol_lines, circles) = projection_symbol(
        projection,
        t0.y + s(PROJECTION_CENTRE_Y),
        t0.x + s(PROJECTION_X0),
        layout.scale,
    );
    for (a, b) in symbol_lines {
        m.push(line(a, b, SheetPen::Medium));
    }
    for (center, r) in circles {
        m.push(Mark::Arc {
            center,
            r,
            start: 0.0,
            end: TAU,
            kind: MarkKind::Sheet(SheetPen::Medium),
        });
    }

    revision_table(&mut m, layout, texts, widen);
    notes_box(&mut m, layout, texts, widen);
    m.extend(parts_table_marks(layout, &texts.parts, widen));
    SheetFrame {
        marks: m,
        scale: layout.scale,
    }
}

/// Änderungstabelle: Kopfzeile oben, darunter die sichtbaren Änderungen, die neueste unten.
fn revision_table(m: &mut Vec<Mark>, layout: &SheetLayout, texts: &TitleTexts, widen: f64) {
    let (lo, hi) = layout.revisions;
    let p = DVec2::new;
    let s = |a3_mm: f64| layout.mm(a3_mm);
    let row_h = s(REV_ROW_H);
    let rows = (((hi.y - lo.y) / row_h).round() as usize)
        .saturating_sub(1)
        .min(REV_VISIBLE);
    rect(m, lo, hi, SheetPen::Thin);
    for k in 1..=rows {
        let y = hi.y - row_h * k as f64;
        m.push(line(p(lo.x, y), p(hi.x, y), SheetPen::Thin));
    }
    let columns = layout.rev_columns();
    let mut xs = vec![lo.x];
    for w in columns {
        xs.push(xs[xs.len() - 1] + w);
    }
    for &x in &xs[1..xs.len() - 1] {
        m.push(line(p(x, lo.y), p(x, hi.y), SheetPen::Thin));
    }
    let shown = texts.visible_revisions();
    let head = [
        tr!("drawing-rev-head-letter"),
        tr!("drawing-rev-head-change"),
        tr!("sheet-rev-date"),
        tr!("sheet-rev-name"),
    ];
    for k in 0..=rows {
        let baseline = hi.y - row_h * (k + 1) as f64 + s((REV_ROW_H - VALUE_H) / 2.0);
        for (i, col) in columns.iter().enumerate() {
            let t = if k == 0 {
                head[i].as_str()
            } else {
                shown.get(k - 1).map_or("", |r| r[i].as_str())
            };
            fitted_text(
                m,
                p(xs[i] + s(1.0), baseline),
                s(VALUE_H),
                col - s(2.0),
                widen,
                t,
            );
        }
    }
}

/// Hinweisfeld: Umriss und die Zeilen von oben nach unten.
fn notes_box(m: &mut Vec<Mark>, layout: &SheetLayout, texts: &TitleTexts, widen: f64) {
    let Some((lo, hi)) = layout.notes else {
        return;
    };
    let s = |a3_mm: f64| layout.mm(a3_mm);
    let (pad, line_h) = (s(NOTES_PAD_MM), s(NOTES_LINE_MM));
    let rows = (((hi.y - lo.y - 2.0 * pad) / line_h).round() as usize).min(NOTE_LINES_MAX);
    rect(m, lo, hi, SheetPen::Thin);
    for (i, t) in texts.notes.iter().take(rows).enumerate() {
        let baseline =
            hi.y - pad - (i + 1) as f64 * line_h + s((NOTES_LINE_MM - NOTES_TEXT_H) / 2.0);
        fitted_text(
            m,
            DVec2::new(lo.x + pad, baseline),
            s(NOTES_TEXT_H),
            hi.x - lo.x - 2.0 * pad,
            widen,
            t,
        );
    }
}

/// Die Stückliste als Markierungen (alle `Thin`): Umriss, Zeilen- und Spaltentrennlinien, Kopf
/// und die Zeilen. Gezeichnet werden der Kopf und so viele Zeilen von `rows`, wie in
/// [`SheetLayout::parts`] passen. Auf dem Zeichnungsblatt liegt der Kopf unten an der Tabelle und
/// Pos. 1 direkt darüber; auf dem Stücklistenblatt liegt der Kopf oben, die Zeilen folgen nach
/// unten. Ohne Tabelle im Blatt nichts. `widen` wie bei den übrigen Texten.
pub fn parts_table_marks(layout: &SheetLayout, rows: &[PartsCells], widen: f64) -> Vec<Mark> {
    let Some((lo, hi)) = layout.parts else {
        return Vec::new();
    };
    let mut m = Vec::new();
    let p = DVec2::new;
    let s = |a3_mm: f64| layout.mm(a3_mm);
    let row_h = s(PARTS_ROW_H);
    let capacity = (((hi.y - lo.y) / row_h + 1e-9).floor() as usize).saturating_sub(1);
    let n = rows.len().min(capacity);
    let top_down = layout.parts_space == PartsSpace::ListPage;
    // Unterkante der Zeile `k` (0 = Kopf).
    let row_bottom = |k: usize| {
        if top_down {
            hi.y - row_h * (k + 1) as f64
        } else {
            lo.y + row_h * k as f64
        }
    };
    let (y0, y1) = if top_down {
        (row_bottom(n), hi.y)
    } else {
        (lo.y, row_bottom(n) + row_h)
    };
    rect(&mut m, p(lo.x, y0), p(hi.x, y1), SheetPen::Thin);
    for k in 1..=n {
        let y = if top_down {
            row_bottom(k) + row_h
        } else {
            row_bottom(k)
        };
        m.push(line(p(lo.x, y), p(hi.x, y), SheetPen::Thin));
    }
    let columns = layout.parts_columns();
    let mut xs = vec![lo.x];
    for w in columns {
        xs.push(xs[xs.len() - 1] + w);
    }
    for &x in &xs[1..xs.len() - 1] {
        m.push(line(p(x, y0), p(x, y1), SheetPen::Thin));
    }
    let head = parts_head();
    for k in 0..=n {
        let baseline = row_bottom(k) + s((PARTS_ROW_H - VALUE_H) / 2.0);
        for (i, col) in columns.iter().enumerate() {
            let t = if k == 0 {
                head[i].as_str()
            } else {
                rows[k - 1][i].as_str()
            };
            fitted_text(
                &mut m,
                p(xs[i] + s(1.0), baseline),
                s(VALUE_H),
                col - s(2.0),
                widen,
                t,
            );
        }
    }
    m
}
