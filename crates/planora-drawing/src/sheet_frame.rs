//! Blattgeometrie nach ISO 5457 und ISO 7200: Rahmen, Zentriermarken, Bezugsraster, Schriftfeld,
//! Änderungstabelle, Hinweisfeld, freier Bereich und Projektionssymbol. Reine Geometrie in
//! Blattkoordinaten (mm, Ursprung links unten, y nach oben); keine Zeichenausgabe.
//!
//! Skalierung: Alle Längen unten (Ränder, Raster, Schriftfeld, Tabellen, Schrifthöhen, Symbol)
//! sind die Werte für A3 (Bezugsgröße). Auf anderen Formaten wächst oder schrumpft das ganze
//! Blattzubehör mit dem Faktor [`sheet_scale`] = lange Blattseite / 420 mm, wie ein A3-Blatt,
//! das vergrößert oder verkleinert gedruckt wird. Der Faktor steht in [`SheetLayout::scale`];
//! [`sheet_layout`] und die Hilfen von [`SheetLayout`] wenden ihn an. Linienstärken wachsen mit,
//! werden aber nie dünner als [`MIN_PEN_MM`].

use crate::model::{Projection, Sheet};
use planora_math::DVec2;

/// Lange Blattseite des Bezugsformats A3; dort ist der Faktor 1.
pub const REFERENCE_LONG_SIDE_MM: f64 = 420.0;
/// Dünnste Linie des Blattzubehörs (kleinste Stufe der ISO-Linienreihe).
pub const MIN_PEN_MM: f64 = 0.18;

/// Heftrand links.
pub const FILING_MARGIN_MM: f64 = 20.0;
/// Rand oben, rechts, unten.
pub const EDGE_MARGIN_MM: f64 = 10.0;
/// Die Zentriermarken reichen so weit hinter den Rahmen.
pub const CENTRE_MARK_INSET_MM: f64 = 5.0;
/// Teilung des Bezugsrasters.
pub const GRID_FIELD_MM: f64 = 50.0;
pub const GRID_TEXT_H: f64 = 3.5;
/// Länge der Teilstriche des Bezugsrasters.
pub const GRID_TICK_MM: f64 = 5.0;
pub const TITLE_W: f64 = 180.0;
pub const TITLE_H: f64 = 56.0;
pub const LABEL_H: f64 = 2.2;
pub const VALUE_H: f64 = 3.5;
pub const TITLE_TEXT_H: f64 = 5.0;
pub const REV_W: f64 = 176.0;
pub const REV_ROW_H: f64 = 7.0;
pub const REV_VISIBLE: usize = 6;
/// Spaltenbreiten der Änderungstabelle: Rev., Änderung, Datum, Name.
pub const REV_COLUMNS: [f64; 4] = [12.0, 104.0, 30.0, 30.0];
/// Höchste Zahl sichtbarer Hinweiszeilen.
pub const NOTE_LINES_MAX: usize = 6;
pub const NOTES_TEXT_H: f64 = 3.3;
pub const NOTES_LINE_MM: f64 = 6.0;
pub const NOTES_PAD_MM: f64 = 2.0;
pub const NOTES_SIDE_W: f64 = 110.0;
pub const NOTES_GAP_MM: f64 = 10.0;
/// Ab dieser Rahmenbreite steht das Hinweisfeld neben dem Schriftfeld.
pub const NOTES_SIDE_MIN_FRAME_W: f64 = 310.0;
pub const SYMBOL_H: f64 = 3.5;
/// Lage des Projektionssymbols in der Schriftfeldzelle „Projektion“ (relativ zur linken unteren
/// Ecke des Schriftfelds): linke Kante und Mittellinie.
pub const PROJECTION_X0: f64 = 156.0;
pub const PROJECTION_CENTRE_Y: f64 = 51.5;

/// Linienstärken des Blatts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SheetPen {
    Heavy,
    Medium,
    Thin,
}

impl SheetPen {
    /// Strichbreite in Millimetern auf A3 (Faktor 1).
    pub fn width_mm(self) -> f64 {
        match self {
            SheetPen::Heavy => 0.7,
            SheetPen::Medium => 0.5,
            SheetPen::Thin => 0.25,
        }
    }

    /// Strichbreite bei dem Faktor `scale`, nie unter [`MIN_PEN_MM`].
    pub fn width_at(self, scale: f64) -> f64 {
        (self.width_mm() * scale).max(MIN_PEN_MM)
    }
}

/// Faktor des Blattzubehörs: lange Blattseite / 420 mm (A4 0,707, A3 1, A2 1,414, A1 2,002,
/// A0 2,831), für hoch und quer gleich, ohne Grenzen.
pub fn sheet_scale(sheet: Sheet) -> f64 {
    let (w, h) = sheet.size_mm();
    w.max(h) / REFERENCE_LONG_SIDE_MM
}

/// Felder des Schriftfelds, die Text tragen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TitleField {
    Owner,
    Number,
    Title,
    Subtitle,
    DocType,
    Status,
    Issued,
    Revision,
    SheetOf,
    Format,
    Scale,
    Creator,
    Checked,
    Approved,
    Material,
    Weight,
    Language,
}

impl TitleField {
    pub const ALL: [TitleField; 17] = [
        TitleField::Owner,
        TitleField::Number,
        TitleField::Title,
        TitleField::Subtitle,
        TitleField::DocType,
        TitleField::Status,
        TitleField::Issued,
        TitleField::Revision,
        TitleField::SheetOf,
        TitleField::Format,
        TitleField::Scale,
        TitleField::Creator,
        TitleField::Checked,
        TitleField::Approved,
        TitleField::Material,
        TitleField::Weight,
        TitleField::Language,
    ];

    /// Feldname im Schriftfeld.
    pub fn label(self) -> &'static str {
        match self {
            TitleField::Owner => "Eigentümer",
            TitleField::Number => "Zeichnungsnummer",
            TitleField::Title => "Titel",
            TitleField::Subtitle => "Zusatztitel",
            TitleField::DocType => "Dokumentart",
            TitleField::Status => "Dokumentstatus",
            TitleField::Issued => "Ausgabedatum",
            TitleField::Revision => "Änderung",
            TitleField::SheetOf => "Blatt",
            TitleField::Format => "Format",
            TitleField::Scale => "Maßstab",
            TitleField::Creator => "Erstellt",
            TitleField::Checked => "Geprüft",
            TitleField::Approved => "Freigabe",
            TitleField::Material => "Werkstoff",
            TitleField::Weight => "Gewicht",
            TitleField::Language => "Sprache",
        }
    }

    /// Höchste Zeichenzahl des Eintrags.
    pub fn max_chars(self) -> usize {
        match self {
            TitleField::Title => 60,
            _ => 80,
        }
    }
}

/// Zelle des Schriftfelds, relativ zu dessen linker unterer Ecke.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TitleCell {
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
    pub label: &'static str,
}

/// Platz für den Inhalt eines Felds, relativ zur linken unteren Ecke des Schriftfelds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextSlot {
    pub field: TitleField,
    pub x0: f64,
    pub width: f64,
    pub baseline: f64,
    pub height: f64,
}

const fn cell(x0: f64, x1: f64, y0: f64, y1: f64, label: &'static str) -> TitleCell {
    TitleCell {
        x0,
        x1,
        y0,
        y1,
        label,
    }
}

/// Die 17 Zellen des Schriftfelds (mit der Projektion), Zeilen von unten.
pub const TITLE_CELLS: [TitleCell; 17] = [
    cell(0.0, 100.0, 0.0, 10.0, "Zeichnungsnummer"),
    cell(100.0, 125.0, 0.0, 10.0, "Änderung"),
    cell(125.0, 155.0, 0.0, 10.0, "Blatt"),
    cell(155.0, 180.0, 0.0, 10.0, "Sprache"),
    cell(0.0, 180.0, 10.0, 28.0, "Titel"),
    cell(0.0, 80.0, 28.0, 38.0, "Eigentümer"),
    cell(80.0, 135.0, 28.0, 38.0, "Dokumentart"),
    cell(135.0, 180.0, 28.0, 38.0, "Dokumentstatus"),
    cell(0.0, 45.0, 38.0, 47.0, "Erstellt"),
    cell(45.0, 90.0, 38.0, 47.0, "Geprüft"),
    cell(90.0, 135.0, 38.0, 47.0, "Freigabe"),
    cell(135.0, 180.0, 38.0, 47.0, "Ausgabedatum"),
    cell(0.0, 60.0, 47.0, 56.0, "Werkstoff"),
    cell(60.0, 90.0, 47.0, 56.0, "Gewicht"),
    cell(90.0, 115.0, 47.0, 56.0, "Maßstab"),
    cell(115.0, 135.0, 47.0, 56.0, "Format"),
    cell(135.0, 180.0, 47.0, 56.0, "Projektion"),
];

const fn slot(field: TitleField, c: usize, baseline: f64, height: f64) -> TextSlot {
    TextSlot {
        field,
        x0: TITLE_CELLS[c].x0 + 1.0,
        width: TITLE_CELLS[c].x1 - TITLE_CELLS[c].x0 - 2.0,
        baseline,
        height,
    }
}

/// Inhaltsplätze der 17 Felder (Reihenfolge wie [`TitleField::ALL`]).
pub const TITLE_SLOTS: [TextSlot; 17] = [
    slot(TitleField::Owner, 5, 29.5, VALUE_H),
    slot(TitleField::Number, 0, 1.5, VALUE_H),
    slot(TitleField::Title, 4, 18.5, TITLE_TEXT_H),
    slot(TitleField::Subtitle, 4, 12.5, VALUE_H),
    slot(TitleField::DocType, 6, 29.5, VALUE_H),
    slot(TitleField::Status, 7, 29.5, VALUE_H),
    slot(TitleField::Issued, 11, 39.5, VALUE_H),
    slot(TitleField::Revision, 1, 1.5, VALUE_H),
    slot(TitleField::SheetOf, 2, 1.5, VALUE_H),
    slot(TitleField::Format, 15, 48.5, VALUE_H),
    slot(TitleField::Scale, 14, 48.5, VALUE_H),
    slot(TitleField::Creator, 8, 39.5, VALUE_H),
    slot(TitleField::Checked, 9, 39.5, VALUE_H),
    slot(TitleField::Approved, 10, 39.5, VALUE_H),
    slot(TitleField::Material, 12, 48.5, VALUE_H),
    slot(TitleField::Weight, 13, 48.5, VALUE_H),
    slot(TitleField::Language, 3, 1.5, VALUE_H),
];

/// Rechteck als (links unten, rechts oben).
pub type Box2 = (DVec2, DVec2);

/// Alle Blattgeometrie für ein Blatt.
#[derive(Clone, Debug)]
pub struct SheetLayout {
    pub sheet: Sheet,
    pub frame: Box2,
    /// Links, rechts, unten, oben; vom Blattrand nach innen.
    pub centre_marks: [(DVec2, DVec2); 4],
    /// Teilstriche vom Rahmen nach außen.
    pub grid_ticks: Vec<(DVec2, DVec2)>,
    /// Beschriftungen des Rasters (Textmitte).
    pub grid_labels: Vec<(DVec2, String)>,
    pub title: Box2,
    pub revisions: Box2,
    pub notes: Option<Box2>,
    pub free: Box2,
    /// Faktor des Blattzubehörs ([`sheet_scale`]).
    pub scale: f64,
}

impl SheetLayout {
    /// Eine A3-Länge auf diesem Blatt.
    pub fn mm(&self, a3_mm: f64) -> f64 {
        a3_mm * self.scale
    }

    /// Strichbreite einer Linienart auf diesem Blatt.
    pub fn pen_width(&self, pen: SheetPen) -> f64 {
        pen.width_at(self.scale)
    }

    /// Die Zellen des Schriftfelds auf diesem Blatt (relativ zu dessen linker unterer Ecke).
    pub fn title_cells(&self) -> [TitleCell; 17] {
        TITLE_CELLS.map(|c| TitleCell {
            x0: self.mm(c.x0),
            x1: self.mm(c.x1),
            y0: self.mm(c.y0),
            y1: self.mm(c.y1),
            label: c.label,
        })
    }

    /// Die Inhaltsplätze des Schriftfelds auf diesem Blatt.
    pub fn title_slots(&self) -> [TextSlot; 17] {
        TITLE_SLOTS.map(|s| TextSlot {
            field: s.field,
            x0: self.mm(s.x0),
            width: self.mm(s.width),
            baseline: self.mm(s.baseline),
            height: self.mm(s.height),
        })
    }

    /// Spaltenbreiten der Änderungstabelle auf diesem Blatt.
    pub fn rev_columns(&self) -> [f64; 4] {
        REV_COLUMNS.map(|w| self.mm(w))
    }
}

fn b(x0: f64, y0: f64, x1: f64, y1: f64) -> Box2 {
    (DVec2::new(x0, y0), DVec2::new(x1, y1))
}

fn area(r: Box2) -> f64 {
    (r.1.x - r.0.x).max(0.0) * (r.1.y - r.0.y).max(0.0)
}

/// Felderzahl je Kante: `2 · round(Kante / 100)`, mindestens 2.
pub fn grid_count(len_mm: f64) -> usize {
    ((2.0 * (len_mm / 100.0).round()) as usize).max(2)
}

/// Zeilenbuchstabe des Rasters: A bis Z ohne I und O (24 Stück).
pub fn grid_letter(i: usize) -> Option<char> {
    ('A'..='Z').filter(|c| *c != 'I' && *c != 'O').nth(i)
}

/// Rasterlinien einer Kante: Grenzen der Felder (ohne die Mitte als Teilstrich) von `lo` bis `hi`.
fn grid_bounds(lo: f64, hi: f64, centre: f64, n: usize, field: f64) -> (Vec<f64>, Vec<f64>) {
    let half = (n / 2) as i64;
    let mut ticks = Vec::new();
    for k in -(half - 1)..half {
        if k != 0 {
            ticks.push(centre + k as f64 * field);
        }
    }
    let mut bounds = vec![lo];
    bounds.extend((-(half - 1)..half).map(|k| centre + k as f64 * field));
    bounds.push(hi);
    (ticks, bounds)
}

/// Berechnet die Blattgeometrie. `revision_rows` und `note_lines` werden auf je 6 begrenzt. Alle
/// Längen sind die A3-Werte mal [`sheet_scale`].
pub fn sheet_layout(sheet: Sheet, revision_rows: usize, note_lines: usize) -> SheetLayout {
    let (w, h) = sheet.size_mm();
    let f = sheet_scale(sheet);
    let s = |a3_mm: f64| a3_mm * f;
    let rev_rows = revision_rows.min(REV_VISIBLE);
    let n_lines = note_lines.min(NOTE_LINES_MAX);
    let (fx0, fy0, fx1, fy1) = (
        s(FILING_MARGIN_MM),
        s(EDGE_MARGIN_MM),
        w - s(EDGE_MARGIN_MM),
        h - s(EDGE_MARGIN_MM),
    );
    let frame = b(fx0, fy0, fx1, fy1);
    let (cx, cy) = (w / 2.0, h / 2.0);
    let (lr, tb) = (
        s(FILING_MARGIN_MM + CENTRE_MARK_INSET_MM),
        s(EDGE_MARGIN_MM + CENTRE_MARK_INSET_MM),
    );
    let p = DVec2::new;
    let centre_marks = [
        (p(0.0, cy), p(lr, cy)),
        (p(w, cy), p(w - tb, cy)),
        (p(cx, 0.0), p(cx, tb)),
        (p(cx, h), p(cx, h - tb)),
    ];

    // Bezugsraster: Felderzahl wie auf dem entsprechend vergrößerten A3-Blatt (gerade Zahl),
    // Feldlänge 50 mm mal Faktor.
    let (nx, ny) = (grid_count(w / f), grid_count(h / f));
    let field = s(GRID_FIELD_MM);
    let (xt, xb) = grid_bounds(fx0, fx1, cx, nx, field);
    let (yt, yb) = grid_bounds(fy0, fy1, cy, ny, field);
    let tick = s(GRID_TICK_MM);
    let mut grid_ticks = Vec::new();
    for &x in &xt {
        grid_ticks.push((p(x, fy1), p(x, fy1 + tick)));
        grid_ticks.push((p(x, fy0), p(x, fy0 - tick)));
    }
    for &y in &yt {
        grid_ticks.push((p(fx0, y), p(fx0 - tick, y)));
        grid_ticks.push((p(fx1, y), p(fx1 + tick, y)));
    }
    // Beschriftung mitten in den Rändern.
    let mut grid_labels = Vec::new();
    for y in [h - s(EDGE_MARGIN_MM / 2.0), s(EDGE_MARGIN_MM / 2.0)] {
        for (i, win) in xb.windows(2).enumerate() {
            grid_labels.push((p((win[0] + win[1]) / 2.0, y), (i + 1).to_string()));
        }
    }
    for x in [s(FILING_MARGIN_MM / 2.0), w - s(EDGE_MARGIN_MM / 2.0)] {
        for (j, win) in yb.windows(2).enumerate() {
            let from_top = ny - 1 - j;
            if let Some(c) = grid_letter(from_top) {
                grid_labels.push((p(x, (win[0] + win[1]) / 2.0), c.to_string()));
            }
        }
    }

    // Schriftfeld, Änderungstabelle, Hinweisfeld.
    let title = b(fx1 - s(TITLE_W), fy0, fx1, fy0 + s(TITLE_H));
    let rev_h = s(REV_ROW_H) * (1.0 + rev_rows as f64);
    let revisions = b(fx1 - s(REV_W), fy1 - rev_h, fx1, fy1);
    let notes_side = fx1 - fx0 >= s(NOTES_SIDE_MIN_FRAME_W);
    let notes = (n_lines > 0).then(|| {
        let nh = s(2.0 * NOTES_PAD_MM) + n_lines as f64 * s(NOTES_LINE_MM);
        if notes_side {
            let x1 = title.0.x - s(NOTES_GAP_MM);
            b(x1 - s(NOTES_SIDE_W), fy0, x1, fy0 + nh)
        } else {
            b(title.0.x, title.1.y, title.1.x, title.1.y + nh)
        }
    });

    // Freier Bereich: größere zweier Kandidaten, bei Gleichstand der erste.
    let stack_top = match notes {
        Some(n) if !notes_side => n.1.y,
        _ => title.1.y,
    };
    let cand_a = b(fx0, stack_top, fx1, revisions.0.y);
    let mut left = title.0.x.min(revisions.0.x);
    if let Some(n) = notes {
        left = left.min(n.0.x);
    }
    let cand_b = b(fx0, fy0, left, fy1);
    let free = if area(cand_b) > area(cand_a) {
        cand_b
    } else {
        cand_a
    };

    SheetLayout {
        sheet,
        frame,
        centre_marks,
        grid_ticks,
        grid_labels,
        title,
        revisions,
        notes,
        free,
        scale: f,
    }
}

/// Projektionssymbol (ISO 5456-2) mit der Höhe `SYMBOL_H · scale`: die vier Strecken des
/// Kegelstumpf-Trapezes (auf A3 Länge 7, linke Seite 3,5 hoch, rechte 7) und zwei Kreise (Mitte,
/// Radius) Ø 7 und Ø 3,5. Erstwinkel: Trapez links, Kreise rechts; Dritter Winkel: Kreise links,
/// Trapez rechts. `x0` ist die linke Kante des Symbols, `centre_y` seine Mittellinie.
#[allow(clippy::type_complexity)]
pub fn projection_symbol(
    projection: Projection,
    centre_y: f64,
    x0: f64,
    scale: f64,
) -> (Vec<(DVec2, DVec2)>, [(DVec2, f64); 2]) {
    let sh = SYMBOL_H * scale;
    let len = 2.0 * sh;
    let gap = sh;
    let (tx, cxm) = match projection {
        Projection::First => (x0, x0 + len + gap + sh),
        Projection::Third => (x0 + 2.0 * sh + gap, x0 + sh),
    };
    let p = DVec2::new;
    let (hl, hr) = (sh / 2.0, sh);
    let (a, bb, c, d) = (
        p(tx, centre_y - hl),
        p(tx, centre_y + hl),
        p(tx + len, centre_y + hr),
        p(tx + len, centre_y - hr),
    );
    let lines = vec![(a, bb), (bb, c), (c, d), (d, a)];
    let m = p(cxm, centre_y);
    (lines, [(m, sh), (m, sh / 2.0)])
}
