//! Der Blattrahmen als Markierungen: Rahmen, Zentriermarken, Bezugsraster, Schriftfeld mit
//! Inhalten, Projektionssymbol, Änderungstabelle und Hinweisfeld. Die Geometrie liefert
//! [`SheetLayout`](crate::sheet_frame::SheetLayout), die Texte [`TitleTexts`]; hier entstehen
//! daraus [`Mark`]s in Blattkoordinaten (mm, y nach oben), die SVG und DXF wie alle anderen
//! Markierungen ausgeben. Alle Markierungen tragen [`MarkKind::Sheet`] mit der Linienstärke;
//! Texte haben die Stärke `Thin` und den Winkel 0.

use crate::annotate::{Mark, MarkKind, TextAnchor};
use crate::model::Projection;
use crate::sheet_frame::{
    GRID_TEXT_H, LABEL_H, NOTE_LINES_MAX, NOTES_LINE_MM, NOTES_PAD_MM, NOTES_TEXT_H,
    PROJECTION_CENTRE_Y, PROJECTION_X0, REV_COLUMNS, REV_ROW_H, REV_VISIBLE, SheetLayout, SheetPen,
    TITLE_CELLS, TITLE_SLOTS, TitleField, VALUE_H, projection_symbol,
};
use crate::sheet_info::TitleTexts;
use planora_math::{DVec2, TAU};

/// Der Blattrahmen eines Blatts als Markierungen.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetFrame {
    pub marks: Vec<Mark>,
}

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

/// Wird der Eintrag dieses Felds im Schriftfeld gekürzt (für die Warnung im Fenster)?
pub fn field_is_cut(field: TitleField, text: &str) -> bool {
    TITLE_SLOTS
        .iter()
        .find(|s| s.field == field)
        .is_some_and(|s| fit_text(text, s.height, s.width).cut)
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
    });
}

/// Eingepasster Text bei `pos` (Anfang auf der Grundlinie).
fn fitted_text(marks: &mut Vec<Mark>, pos: DVec2, height: f64, width: f64, s: &str) {
    let f = fit_text(s, height, width);
    text(marks, pos, f.height, TextAnchor::Start, &f.text);
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
    let mut m: Vec<Mark> = Vec::new();
    let p = DVec2::new;

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
            p(c.x, c.y - GRID_TEXT_H / 2.0),
            GRID_TEXT_H,
            TextAnchor::Middle,
            label,
        );
    }

    // Schriftfeld.
    let (t0, t1) = layout.title;
    rect(&mut m, t0, t1, SheetPen::Heavy);
    let abs: Vec<(f64, f64, f64, f64)> = TITLE_CELLS
        .iter()
        .map(|c| (t0.x + c.x0, t0.x + c.x1, t0.y + c.y0, t0.y + c.y1))
        .collect();
    for (a, b) in cell_dividers(&abs, t0, t1) {
        m.push(line(a, b, SheetPen::Thin));
    }
    for c in &TITLE_CELLS {
        text(
            &mut m,
            p(t0.x + c.x0 + 1.0, t0.y + c.y1 - 1.0 - LABEL_H),
            LABEL_H,
            TextAnchor::Start,
            c.label,
        );
    }
    for s in &TITLE_SLOTS {
        fitted_text(
            &mut m,
            p(t0.x + s.x0, t0.y + s.baseline),
            s.height,
            s.width,
            texts.get(s.field),
        );
    }

    // Projektionssymbol.
    let (symbol_lines, circles) =
        projection_symbol(projection, t0.y + PROJECTION_CENTRE_Y, t0.x + PROJECTION_X0);
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

    revision_table(&mut m, layout, texts);
    notes_box(&mut m, layout, texts);
    SheetFrame { marks: m }
}

/// Änderungstabelle: Kopfzeile oben, darunter die sichtbaren Änderungen, die neueste unten.
fn revision_table(m: &mut Vec<Mark>, layout: &SheetLayout, texts: &TitleTexts) {
    let (lo, hi) = layout.revisions;
    let p = DVec2::new;
    let rows = (((hi.y - lo.y) / REV_ROW_H).round() as usize)
        .saturating_sub(1)
        .min(REV_VISIBLE);
    rect(m, lo, hi, SheetPen::Thin);
    for k in 1..=rows {
        let y = hi.y - REV_ROW_H * k as f64;
        m.push(line(p(lo.x, y), p(hi.x, y), SheetPen::Thin));
    }
    let mut xs = vec![lo.x];
    for w in REV_COLUMNS {
        xs.push(xs[xs.len() - 1] + w);
    }
    for &x in &xs[1..xs.len() - 1] {
        m.push(line(p(x, lo.y), p(x, hi.y), SheetPen::Thin));
    }
    let shown = texts.visible_revisions();
    let head = ["Rev.", "Änderung", "Datum", "Name"];
    for k in 0..=rows {
        let baseline = hi.y - REV_ROW_H * (k + 1) as f64 + (REV_ROW_H - VALUE_H) / 2.0;
        for (i, col) in REV_COLUMNS.iter().enumerate() {
            let s = if k == 0 {
                head[i]
            } else {
                shown.get(k - 1).map_or("", |r| r[i].as_str())
            };
            fitted_text(m, p(xs[i] + 1.0, baseline), VALUE_H, col - 2.0, s);
        }
    }
}

/// Hinweisfeld: Umriss und die Zeilen von oben nach unten.
fn notes_box(m: &mut Vec<Mark>, layout: &SheetLayout, texts: &TitleTexts) {
    let Some((lo, hi)) = layout.notes else {
        return;
    };
    let rows =
        (((hi.y - lo.y - 2.0 * NOTES_PAD_MM) / NOTES_LINE_MM).round() as usize).min(NOTE_LINES_MAX);
    rect(m, lo, hi, SheetPen::Thin);
    for (i, s) in texts.notes.iter().take(rows).enumerate() {
        let baseline = hi.y - NOTES_PAD_MM - (i + 1) as f64 * NOTES_LINE_MM
            + (NOTES_LINE_MM - NOTES_TEXT_H) / 2.0;
        fitted_text(
            m,
            DVec2::new(lo.x + NOTES_PAD_MM, baseline),
            NOTES_TEXT_H,
            hi.x - lo.x - 2.0 * NOTES_PAD_MM,
            s,
        );
    }
}
