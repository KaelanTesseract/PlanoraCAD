//! Gesetzter Text als reine Daten (Skizzen 8).
//!
//! Hier stehen nur die Datentypen und die schmale Schnittstelle [`TextShaper`]. Das Setzen
//! selbst (Schriften lesen, `harfrust`, `skrifa`) liegt in `planora-text`; die Skizze kennt
//! keine Schrift und bekommt das Ergebnis als [`TextShape`].
//!
//! Alle Maße sind in Einheiten der Höhe: Höhe 1 ist das Geviert (units per em) der Schrift.
//! Die y-Achse zeigt nach oben, die Grundlinie der Zeile liegt bei y = 0.

use planora_math::DVec2;

/// Höchstens so viele Zeichen je Text (Unicode-Zeichen, Zeilenumbrüche zählen mit).
pub const MAX_TEXT_CHARS: usize = 500;
/// Höchstens so viele erzeugte Kurven (Segmente aller Konturen) je Text.
pub const MAX_TEXT_CURVES: usize = 4000;

/// Wie der Text gesetzt wird.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    /// Familienname, wie gewählt (auch wenn die Schrift auf diesem Rechner fehlt).
    pub font: String,
    pub bold: bool,
    pub italic: bool,
    /// Zeichenabstand in Prozent zusätzlich zum Abstand der Schrift (`0 ≤ p ≤ 1000`).
    pub spacing: f64,
}

/// Ein Stück einer Kontur. Es beginnt am Ende des vorigen Stücks (bzw. am Anfang der Kontur)
/// und enthält nur die folgenden Punkte, zuletzt immer den Endpunkt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seg {
    /// Strecke zum Endpunkt.
    Line(DVec2),
    /// Quadratische Bézierkurve: Kontrollpunkt, Endpunkt.
    Quad(DVec2, DVec2),
    /// Kubische Bézierkurve: zwei Kontrollpunkte, Endpunkt.
    Cubic(DVec2, DVec2, DVec2),
}

impl Seg {
    /// Endpunkt des Stücks.
    pub fn end(&self) -> DVec2 {
        match *self {
            Seg::Line(p) | Seg::Quad(_, p) | Seg::Cubic(_, _, p) => p,
        }
    }
}

/// Eine geschlossene Kontur: Das letzte Stück endet genau bei `start`.
#[derive(Clone, Debug, PartialEq)]
pub struct Contour {
    pub start: DVec2,
    pub segs: Vec<Seg>,
}

/// Eine gesetzte Glyphe.
#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    /// Lage des Glyphen-Ursprungs relativ zum Anfang der Zeile (auf deren Grundlinie).
    pub origin: DVec2,
    /// Eigener Vorschub der Glyphe (mit Kerning, ohne den Zeichenabstand in Prozent).
    pub advance: f64,
    /// Umrisse relativ zu `origin`. Leer bei Leerzeichen und fehlenden Zeichen.
    pub contours: Vec<Contour>,
}

/// Eine Zeile, die Glyphen in Leserichtung von links nach rechts (auch bei
/// Rechts-nach-links-Schrift, wie vom Setzer geliefert).
#[derive(Clone, Debug, PartialEq)]
pub struct ShapedLine {
    pub glyphs: Vec<Glyph>,
    /// Breite der Zeile, also der Vorschub bis hinter die letzte Glyphe.
    pub width: f64,
}

/// Ergebnis des Setzens.
#[derive(Clone, Debug, PartialEq)]
pub struct TextShape {
    pub lines: Vec<ShapedLine>,
    /// Oberlänge der Schrift (positiv).
    pub ascender: f64,
    /// Unterlänge der Schrift (negativ, unter der Grundlinie).
    pub descender: f64,
    /// Abstand der Grundlinien zweier Zeilen (`ascender − descender + lineGap`).
    pub line_height: f64,
    /// Die gewünschte Familie fehlt; hier steht ihr Name, gesetzt wurde mit der Ersatzschrift.
    pub substituted: Option<String>,
    /// Fett oder Kursiv ist nachgebildet, weil der Schnitt fehlt.
    pub synthetic: bool,
    /// Einige Zeichen fehlen in der Schrift und in der Ersatzschrift; ihr Platz bleibt leer.
    pub missing_chars: bool,
}

impl TextShape {
    /// Anzahl aller Stücke aller Konturen, also der Kurven, die daraus entstehen.
    pub fn segment_count(&self) -> usize {
        self.lines
            .iter()
            .flat_map(|line| &line.glyphs)
            .flat_map(|glyph| &glyph.contours)
            .map(|contour| contour.segs.len())
            .sum()
    }
}

/// Warum ein Text nicht gesetzt wird.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextShapeError {
    /// Mehr als [`MAX_TEXT_CHARS`] Zeichen.
    TooLong,
    /// Mehr als [`MAX_TEXT_CURVES`] Kurven.
    TooManyCurves,
    /// Leer oder nur Leerraum (Leerzeichen, Tabulatoren, Zeilenumbrüche).
    Empty,
}

/// Setzt Text. Die Skizze ruft das nur außerhalb des Lösers auf.
pub trait TextShaper {
    fn shape(&self, content: &str, style: &TextStyle) -> Result<TextShape, TextShapeError>;
}
