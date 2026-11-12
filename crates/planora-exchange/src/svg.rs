//! SVG lesen (Skizzen 9). Das Ergebnis ist die neutrale 2D-Geometrie aus [`crate::import2d`].
//!
//! Gelesen wird mit `usvg` (ohne Schriften). usvg löst Formen, Gruppen, `use` und Transformationen
//! auf; hier werden nur die Pfade durchlaufen, mit ihrer absoluten Transformation in Millimeter
//! umgerechnet (1 px = 25,4/96 mm) und an der X-Achse gespiegelt (SVG-y zeigt nach unten).
//! Bézier-Segmente bleiben exakt als `Quad`/`Cubic`. Füllung, Linienbreite, Ausschnitte
//! (`clip-path`) und Masken bleiben unberücksichtigt: Es zählt nur die Pfadgeometrie.
//!
//! Ohne das usvg-Feature `text` macht usvg aus `<text>` keine Knoten, und `<image>` ohne ladbares
//! Bild fällt weg. Beide werden darum vorab mit `roxmltree` (von usvg bereitgestellt) gezählt.

use crate::error::ExchangeError;
use crate::import2d::{Element2d, Import2d, Layer2d, Skipped, Unit2d};
use crate::limits::{CHECK_EVERY, MAX_IMPORT_ELEMENTS};
use planora_math::DVec2;
use std::panic::{AssertUnwindSafe, catch_unwind};
use usvg::roxmltree;
use usvg::tiny_skia_path::PathSegment;
use usvg::{ImageHrefResolver, Node, Options, Transform, Tree};

/// Millimeter je SVG-Pixel (96 dpi).
const MM_PER_PX: f64 = 25.4 / 96.0;
/// Ab diesem Abstand (mm) gilt ein Pfad als nicht geschlossen.
const GAP_EPS: f64 = 1e-6;
/// SVG-Namensraum.
const SVG_NS: &str = "http://www.w3.org/2000/svg";

/// Liest eine SVG-Datei (Text oder gzip-komprimiert als SVGZ). Ein Layer „SVG“, Werte in
/// Millimeter. `cancel` wird regelmäßig gefragt; `true` bricht mit [`ExchangeError::Cancelled`]
/// ab. Mehr als [`MAX_IMPORT_ELEMENTS`] Elemente ergeben einen Fehler, kein Teilergebnis.
pub fn read_svg(bytes: &[u8], cancel: &dyn Fn() -> bool) -> Result<Import2d, ExchangeError> {
    if cancel() {
        return Err(ExchangeError::Cancelled);
    }
    // usvg und roxmltree sind nicht für feindliche Dateien gedacht; eine Panik darf das Programm
    // aber nie abstürzen lassen. Sie wird hier abgefangen und wie ein Lesefehler gemeldet.
    let parsed = catch_unwind(AssertUnwindSafe(|| parse(bytes)))
        .map_err(|_| ExchangeError::SvgBroken("internal error".into()))??;
    let (tree, texts, images) = parsed;

    let mut skipped = Skipped {
        texts,
        images,
        ..Skipped::default()
    };
    let mut elements: Vec<Element2d> = Vec::new();
    let mut steps: u64 = 0;
    // Iterativ statt rekursiv: Verschachtelte Gruppen dürfen den Stapel nicht füllen.
    let mut stack: Vec<&Node> = tree.root().children().iter().rev().collect();
    while let Some(node) = stack.pop() {
        steps += 1;
        if steps.is_multiple_of(CHECK_EVERY) && cancel() {
            return Err(ExchangeError::Cancelled);
        }
        match node {
            Node::Group(g) => stack.extend(g.children().iter().rev()),
            Node::Path(p) if p.is_visible() => {
                let mut cx = Emit {
                    ts: p.abs_transform(),
                    elements: &mut elements,
                    skipped: &mut skipped,
                    steps: &mut steps,
                    cancel,
                };
                cx.path(p.data())?;
            }
            // Bilder zählt die Vorabsuche; Text gibt es ohne das Feature `text` nicht.
            _ => {}
        }
    }
    let layers = if elements.is_empty() {
        Vec::new()
    } else {
        vec![Layer2d {
            name: "SVG".into(),
            elements,
        }]
    };
    Ok(Import2d {
        layers,
        units: Some(Unit2d::Millimeter),
        skipped,
    })
}

/// Entpackt (SVGZ), parst mit usvg und zählt `<text>` und `<image>`.
fn parse(bytes: &[u8]) -> Result<(Tree, usize, usize), ExchangeError> {
    let unpacked;
    let raw = if bytes.starts_with(&[0x1f, 0x8b]) {
        unpacked =
            usvg::decompress_svgz(bytes).map_err(|e| ExchangeError::SvgBroken(e.to_string()))?;
        &unpacked[..]
    } else {
        bytes
    };
    let text = std::str::from_utf8(raw).map_err(|e| ExchangeError::SvgBroken(e.to_string()))?;
    let opt = Options {
        // Bilder werden nie geladen (auch keine Dateien von der Platte): Sie werden nur gezählt.
        image_href_resolver: ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Options::default()
    };
    let tree = Tree::from_str(text, &opt).map_err(|e| ExchangeError::SvgBroken(e.to_string()))?;
    let (texts, images) = count_texts_and_images(text);
    Ok((tree, texts, images))
}

/// Zählt `<text>` und `<image>` im Dokument, ohne Definitionen, Ausschnitte, Masken, Muster,
/// Markierungen und Symbole (dort liegender Inhalt wird nicht gezeichnet).
fn count_texts_and_images(text: &str) -> (usize, usize) {
    let opts = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let Ok(doc) = roxmltree::Document::parse_with_options(text, opts) else {
        return (0, 0);
    };
    let (mut texts, mut images) = (0, 0);
    // Tiefensuche mit eigenem Stapel; das Flag sagt, ob der Zweig ungezeichnet ist.
    let mut stack = vec![(doc.root(), false)];
    while let Some((node, hidden)) = stack.pop() {
        let mut hidden = hidden;
        if node.is_element() && node.tag_name().namespace() == Some(SVG_NS) {
            match node.tag_name().name() {
                "defs" | "clipPath" | "mask" | "pattern" | "marker" | "symbol" | "filter" => {
                    hidden = true
                }
                "text" if !hidden => texts += 1,
                "image" if !hidden => images += 1,
                _ => {}
            }
        }
        stack.extend(node.children().map(|c| (c, hidden)));
    }
    (texts, images)
}

/// Wandelt die Segmente eines Pfades um und führt die Elementzahl.
struct Emit<'a> {
    ts: Transform,
    elements: &'a mut Vec<Element2d>,
    skipped: &'a mut Skipped,
    steps: &'a mut u64,
    cancel: &'a dyn Fn() -> bool,
}

impl Emit<'_> {
    /// Punkt in Millimeter, Y gespiegelt. `None` bei nicht endlichen Werten.
    fn pt(&self, p: usvg::tiny_skia_path::Point) -> Option<DVec2> {
        let (x, y) = (f64::from(p.x), f64::from(p.y));
        let t = &self.ts;
        let ux = f64::from(t.sx) * x + f64::from(t.kx) * y + f64::from(t.tx);
        let uy = f64::from(t.ky) * x + f64::from(t.sy) * y + f64::from(t.ty);
        let v = DVec2::new(ux * MM_PER_PX, -uy * MM_PER_PX);
        (v.x.is_finite() && v.y.is_finite()).then_some(v)
    }

    /// Einzige Stelle, an der Elemente entstehen: zählt, prüft Grenze und Abbruch.
    fn push(&mut self, e: Element2d) -> Result<(), ExchangeError> {
        if self.elements.len() >= MAX_IMPORT_ELEMENTS {
            return Err(ExchangeError::TooManyElements);
        }
        *self.steps += 1;
        if self.steps.is_multiple_of(CHECK_EVERY) && (self.cancel)() {
            return Err(ExchangeError::Cancelled);
        }
        self.elements.push(e);
        Ok(())
    }

    fn path(&mut self, path: &usvg::tiny_skia_path::Path) -> Result<(), ExchangeError> {
        let mut start: Option<DVec2> = None;
        let mut cur: Option<DVec2> = None;
        for seg in path.segments() {
            match seg {
                PathSegment::MoveTo(p) => {
                    start = self.pt(p);
                    cur = start;
                }
                PathSegment::LineTo(p) => match (cur, self.pt(p)) {
                    (Some(a), Some(b)) if a.distance(b) > GAP_EPS => {
                        self.push(Element2d::Line(a, b))?;
                        cur = Some(b);
                    }
                    (Some(_), Some(b)) => cur = Some(b),
                    _ => self.skipped.degenerate += 1,
                },
                PathSegment::QuadTo(c, p) => match (cur, self.pt(c), self.pt(p)) {
                    (Some(a), Some(c), Some(b)) => {
                        self.push(Element2d::Quad([a, c, b]))?;
                        cur = Some(b);
                    }
                    _ => self.skipped.degenerate += 1,
                },
                PathSegment::CubicTo(c1, c2, p) => {
                    match (cur, self.pt(c1), self.pt(c2), self.pt(p)) {
                        (Some(a), Some(c1), Some(c2), Some(b)) => {
                            self.push(Element2d::Cubic([a, c1, c2, b]))?;
                            cur = Some(b);
                        }
                        _ => self.skipped.degenerate += 1,
                    }
                }
                PathSegment::Close => {
                    // Nur eine Lücke zwischen Ende und Anfang ergibt eine Schlusslinie.
                    if let (Some(a), Some(s)) = (cur, start)
                        && a.distance(s) > GAP_EPS
                    {
                        self.push(Element2d::Line(a, s))?;
                    }
                    cur = start;
                }
            }
        }
        Ok(())
    }
}
