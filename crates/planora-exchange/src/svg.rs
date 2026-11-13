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
//!
//! Schutz vor feindlichen Dateien, bevor usvg etwas aufbläht: Größengrenze (auch entpackt),
//! keine Entitäten oder inneren DTD-Teile, Grenze für XML-Knoten und eine Vorabrechnung der
//! durch `<use>` entstehenden Formen.

use crate::error::ExchangeError;
use crate::import2d::{Element2d, Import2d, Layer2d, Skipped, Unit2d};
use crate::limits::{CHECK_EVERY, MAX_IMPORT_ELEMENTS, MAX_SVG_BYTES};
use planora_math::DVec2;
use std::collections::{HashMap, HashSet};
use std::io::Read;
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
/// XLink-Namensraum (für `xlink:href`).
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";
/// Höchstzahl der XML-Knoten (Elemente, Text, Kommentare) eines Dokuments.
const MAX_XML_NODES: u32 = 1_000_000;
/// Tiefste Verschachtelung beim Vorabrechnen der `<use>`-Formen (XML-Tiefe plus Verweise).
const MAX_COST_DEPTH: usize = 1500;

/// Liest eine SVG-Datei (Text oder gzip-komprimiert als SVGZ). Ein Layer „SVG“, Werte in
/// Millimeter. `cancel` wird regelmäßig gefragt; `true` bricht mit [`ExchangeError::Cancelled`]
/// ab. Mehr als [`MAX_IMPORT_ELEMENTS`] Elemente ergeben einen Fehler, kein Teilergebnis.
/// Größer als [`MAX_SVG_BYTES`] (auch entpackt) darf die Datei nicht sein.
pub fn read_svg(bytes: &[u8], cancel: &dyn Fn() -> bool) -> Result<Import2d, ExchangeError> {
    read_svg_limited(bytes, cancel, MAX_SVG_BYTES)
}

/// Wie [`read_svg`], mit frei wählbarer Größengrenze (für Tests).
fn read_svg_limited(
    bytes: &[u8],
    cancel: &dyn Fn() -> bool,
    max_bytes: u64,
) -> Result<Import2d, ExchangeError> {
    if cancel() {
        return Err(ExchangeError::Cancelled);
    }
    // usvg und roxmltree sind nicht für feindliche Dateien gedacht; eine Panik darf das Programm
    // aber nie abstürzen lassen. Sie wird hier abgefangen und wie ein Lesefehler gemeldet.
    // Die App ruft den Leser im Merker `crash::expected_panics` auf (`insert::guarded`), damit
    // der Absturz-Hook dafür keinen Bericht schreibt.
    let parsed = catch_unwind(AssertUnwindSafe(|| parse(bytes, max_bytes)))
        .map_err(|_| ExchangeError::SvgBroken("internal error".into()))??;
    let (tree, texts, images, markers) = parsed;

    let mut skipped = Skipped {
        texts,
        images,
        unknown: markers,
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

/// Entpackt (SVGZ) mit Größengrenze, prüft das Dokument auf Verstärkungsangriffe, parst mit usvg
/// und zählt `<text>` und `<image>`.
fn parse(bytes: &[u8], max_bytes: u64) -> Result<(Tree, usize, usize, usize), ExchangeError> {
    let unpacked;
    let raw = if bytes.starts_with(&[0x1f, 0x8b]) {
        // Nur bis zur Grenze (plus ein Byte) entpacken: Eine „Zip-Bombe“ füllt so nicht den Speicher.
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(bytes)
            .take(max_bytes.saturating_add(1))
            .read_to_end(&mut out)
            .map_err(|e| ExchangeError::SvgBroken(e.to_string()))?;
        unpacked = out;
        &unpacked[..]
    } else {
        bytes
    };
    if raw.len() as u64 > max_bytes {
        return Err(ExchangeError::SvgTooLarge);
    }
    let text = std::str::from_utf8(raw).map_err(|e| ExchangeError::SvgBroken(e.to_string()))?;
    reject_entities(text)?;
    let mut doc = parse_xml(text)?;
    // Markierungen (`<marker>`) setzt usvg an jeden Pfadpunkt neu ein, Muster (`<pattern>`),
    // Clip-Pfade und Masken (Einheit `objectBoundingBox`) für jedes Element, das sie nutzt, und
    // `feImage` in Filtern je Filter. Das ließe sich nicht vorab begrenzen (ein kleines SVG
    // ergäbe Hunderte Millionen Kopien). Sie werden darum samt Inhalt aus dem Text geschnitten
    // (Gesamtprüfung I4; die Spec liest sie ohnehin nicht). Muster und Filter bleiben als leere
    // Stellvertreter mit derselben Kennung stehen, damit Formen, die sie nutzen, ihren Umriss
    // behalten (usvg ließe sonst ein Element mit verlorenem Filter ganz weg und eine Fläche mit
    // verlorenem Muster unsichtbar).
    let cuts = resource_cuts(&doc);
    let markers = cuts.len();
    let cleaned;
    let mut text = text;
    if markers > 0 {
        cleaned = cut_ranges(text, &cuts);
        text = &cleaned;
        doc = parse_xml(text)?;
    }
    check_use_expansion(&doc)?;
    let (texts, images) = count_texts_and_images(&doc);
    let opt = Options {
        // Bilder werden nie geladen (auch keine Dateien von der Platte): Sie werden nur gezählt.
        image_href_resolver: ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Options::default()
    };
    let tree = Tree::from_str(text, &opt).map_err(|e| ExchangeError::SvgBroken(e.to_string()))?;
    Ok((tree, texts, images, markers))
}

/// Parst den XML-Text mit Knotengrenze (gleiche Einstellungen wie usvg, aber begrenzt).
fn parse_xml(text: &str) -> Result<roxmltree::Document<'_>, ExchangeError> {
    roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: MAX_XML_NODES,
            ..Default::default()
        },
    )
    .map_err(|e| match e {
        roxmltree::Error::NodesLimitReached => ExchangeError::TooManyElements,
        e => ExchangeError::SvgBroken(e.to_string()),
    })
}

/// Elemente, die vor usvg ausgeschnitten werden (siehe `parse`).
const CUT_ELEMENTS: [&str; 5] = ["marker", "pattern", "clipPath", "mask", "filter"];

/// Ein ausgeschnittener Bereich und was an seiner Stelle steht.
struct Cut {
    range: std::ops::Range<usize>,
    replacement: String,
}

/// Bereiche aller äußersten auszuschneidenden Elemente in Dokumentreihenfolge. An ihrer Stelle
/// stehen Stellvertreter für jedes Muster und jeden Filter darin (auch verschachtelte), mit
/// derselben Kennung: ein Verlauf mit einer Farbe für ein Muster (usvg macht daraus eine
/// Farbe), ein wirkungsloser Filter in Nutzerkoordinaten (einmal umgesetzt, für alle Nutzer).
fn resource_cuts(doc: &roxmltree::Document) -> Vec<Cut> {
    let is_cut = |n: &roxmltree::Node| {
        n.is_element()
            && n.tag_name().namespace() == Some(SVG_NS)
            && CUT_ELEMENTS.contains(&n.tag_name().name())
    };
    let mut out: Vec<Cut> = Vec::new();
    for n in doc.descendants() {
        if !is_cut(&n) {
            continue;
        }
        let r = n.range();
        // Verschachtelte liegen schon im Bereich des äußeren.
        if out.last().is_some_and(|l| r.start < l.range.end) {
            continue;
        }
        let mut replacement = String::new();
        for d in n.descendants().filter(|d| is_cut(d)) {
            let Some(id) = d.attribute("id") else {
                continue;
            };
            let id = xml_escape(id);
            match d.tag_name().name() {
                "pattern" => replacement.push_str(&format!(
                    r#"<linearGradient id="{id}"><stop offset="0" stop-color="black"/></linearGradient>"#
                )),
                "filter" => replacement.push_str(&format!(
                    r#"<filter id="{id}" filterUnits="userSpaceOnUse" x="0" y="0" width="1" height="1"><feOffset/></filter>"#
                )),
                _ => {}
            }
        }
        out.push(Cut {
            range: r,
            replacement,
        });
    }
    out
}

/// Text für einen Attributwert in doppelten Anführungszeichen.
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// Der Text mit den (geordneten, getrennten) Bereichen durch ihren Ersatz ersetzt.
fn cut_ranges(text: &str, cuts: &[Cut]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for c in cuts {
        out.push_str(&text[at..c.range.start]);
        out.push_str(&c.replacement);
        at = c.range.end;
    }
    out.push_str(&text[at..]);
    out
}

/// Das Ziel eines Verweises wie in usvg (`parse.rs`, `resolve_href`): ein `href` ohne
/// Namensraum geht vor `xlink:href`; der Wert ist wie bei `svgtypes::IRI` aufgebaut (führende
/// Leerzeichen, dann `#`, die Kennung bis zum nächsten Leerzeichen, dahinter nur Leerzeichen).
fn href_target<'a>(node: roxmltree::Node<'a, 'a>) -> Option<&'a str> {
    let value = node
        .attributes()
        .find(|a| a.name() == "href" && a.namespace().is_none())
        .or_else(|| {
            node.attributes()
                .find(|a| a.name() == "href" && a.namespace() == Some(XLINK_NS))
        })?
        .value();
    let is_space = |c: char| matches!(c, ' ' | '\t' | '\n' | '\r');
    let rest = value.trim_start_matches(is_space).strip_prefix('#')?;
    let end = rest.find(' ').unwrap_or(rest.len());
    let (id, tail) = rest.split_at(end);
    (!id.is_empty() && tail.trim_start_matches(is_space).is_empty()).then_some(id)
}

/// Weist Dokumente mit eigenen Entitäten oder einem inneren DTD-Teil ab („Billion laughs“).
/// Ein einfaches `<!DOCTYPE svg PUBLIC …>` ohne `[…]` bleibt erlaubt (Inkscape, Illustrator).
fn reject_entities(text: &str) -> Result<(), ExchangeError> {
    if text.contains("<!ENTITY") {
        return Err(ExchangeError::SvgBroken("entity declarations".into()));
    }
    if let Some(at) = text.find("<!DOCTYPE") {
        let rest = &text[at..];
        let end = rest.find('>').unwrap_or(rest.len());
        if rest[..end].contains('[') {
            return Err(ExchangeError::SvgBroken("internal DTD subset".into()));
        }
    }
    Ok(())
}

/// Ob der Elementname im SVG-Namensraum zu den Formen gehört, und deren Größenschätzung.
fn shape_cost(node: roxmltree::Node) -> Option<usize> {
    // Ein Segment braucht im Text mindestens etwa vier Zeichen; je 16 Zeichen zählt eins, damit
    // ein sehr langer Pfad, oft per `use` eingefügt, nicht als „eine“ Form durchgeht.
    let by_len = |attr: &str| node.attribute(attr).map_or(1, |s| (s.len() / 16).max(1));
    match node.tag_name().name() {
        "path" => Some(by_len("d")),
        "polyline" | "polygon" => Some(by_len("points")),
        "rect" | "circle" | "ellipse" | "line" => Some(1),
        _ => None,
    }
}

/// Elemente, deren Inhalt nur gezeichnet wird, wenn er über `use` oder Verweise geholt wird.
fn is_hidden_container(name: &str) -> bool {
    matches!(
        name,
        "defs" | "clipPath" | "mask" | "pattern" | "marker" | "symbol" | "filter"
    )
}

/// Rechnet vorab aus, wie viele Formen nach dem Auflösen aller `<use>` entstehen, und bricht
/// ab, wenn es mehr als [`MAX_IMPORT_ELEMENTS`] werden. So kann eine Kette ineinander
/// verwiesener Gruppen usvg nicht um Größenordnungen aufblähen, bevor gezählt wird.
fn check_use_expansion(doc: &roxmltree::Document) -> Result<(), ExchangeError> {
    let mut ids: HashMap<&str, roxmltree::Node> = HashMap::new();
    for n in doc.descendants().filter(|n| n.is_element()) {
        if let Some(id) = n.attribute("id") {
            ids.entry(id).or_insert(n);
        }
    }
    let mut cx = Cost {
        ids,
        memo: HashMap::new(),
        active: HashSet::new(),
    };
    if cx.of(doc.root(), 0)? > MAX_IMPORT_ELEMENTS {
        return Err(ExchangeError::TooManyElements);
    }
    Ok(())
}

/// Zustand der Vorabrechnung: Elemente nach `id`, bereits berechnete Knoten, laufende Knoten
/// (gegen Verweise im Kreis).
struct Cost<'a> {
    ids: HashMap<&'a str, roxmltree::Node<'a, 'a>>,
    memo: HashMap<roxmltree::NodeId, usize>,
    active: HashSet<roxmltree::NodeId>,
}

impl<'a> Cost<'a> {
    /// Anzahl der Formen unter `node` einschließlich der über `<use>` eingefügten. Der Wert ist
    /// nach oben gedeckelt (über [`MAX_IMPORT_ELEMENTS`] genügt „zu viel“).
    fn of(&mut self, node: roxmltree::Node<'a, 'a>, depth: usize) -> Result<usize, ExchangeError> {
        if depth > MAX_COST_DEPTH {
            return Err(ExchangeError::TooManyElements);
        }
        if let Some(&c) = self.memo.get(&node.id()) {
            return Ok(c);
        }
        // Ein Verweis im Kreis liefert nichts (usvg verwirft ihn ebenfalls).
        if !self.active.insert(node.id()) {
            return Ok(0);
        }
        let mut cost = 0usize;
        if node.is_element() && node.tag_name().namespace() == Some(SVG_NS) {
            if let Some(c) = shape_cost(node) {
                cost = c;
            } else if node.tag_name().name() == "use" {
                let target = href_target(node).and_then(|id| self.ids.get(id).copied());
                if let Some(t) = target {
                    cost = self.of(t, depth + 1)?;
                }
            }
        }
        for c in node.children().filter(|c| c.is_element()) {
            let hidden = c.tag_name().namespace() == Some(SVG_NS)
                && is_hidden_container(c.tag_name().name());
            if !hidden {
                cost = cost.saturating_add(self.of(c, depth + 1)?);
            }
            if cost > MAX_IMPORT_ELEMENTS {
                cost = MAX_IMPORT_ELEMENTS + 1;
                break;
            }
        }
        self.active.remove(&node.id());
        self.memo.insert(node.id(), cost);
        Ok(cost)
    }
}

/// Zählt `<text>` und `<image>` im Dokument, ohne Definitionen, Ausschnitte, Masken, Muster,
/// Markierungen und Symbole (dort liegender Inhalt wird nicht gezeichnet).
fn count_texts_and_images(doc: &roxmltree::Document) -> (usize, usize) {
    let (mut texts, mut images) = (0, 0);
    // Tiefensuche mit eigenem Stapel; das Flag sagt, ob der Zweig ungezeichnet ist.
    let mut stack = vec![(doc.root(), false)];
    while let Some((node, hidden)) = stack.pop() {
        let mut hidden = hidden;
        if node.is_element() && node.tag_name().namespace() == Some(SVG_NS) {
            let name = node.tag_name().name();
            if is_hidden_container(name) {
                hidden = true;
            } else if name == "text" && !hidden {
                texts += 1;
            } else if name == "image" && !hidden {
                images += 1;
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

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::GzEncoder};
    use std::io::Write;

    const NEVER: &dyn Fn() -> bool = &|| false;
    const SMALL: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><path d="M0 0 L1 1" fill="none" stroke="black"/></svg>"#;

    fn gzip(data: &[u8]) -> Vec<u8> {
        let mut enc = GzEncoder::new(Vec::new(), Compression::best());
        enc.write_all(data).unwrap();
        enc.finish().unwrap()
    }

    #[test]
    fn gzip_bomb_is_cut_at_the_limit() {
        // 8 MiB Nullen packen auf wenige KiB; die Grenze (64 KiB) wird beim Entpacken erreicht.
        let packed = gzip(&vec![b' '; 8 * 1024 * 1024]);
        assert!(packed.len() < 64 * 1024, "{}", packed.len());
        let err = read_svg_limited(&packed, NEVER, 64 * 1024).unwrap_err();
        assert_eq!(err, ExchangeError::SvgTooLarge);
    }

    #[test]
    fn plain_input_above_limit_is_rejected() {
        let err = read_svg_limited(SMALL, NEVER, SMALL.len() as u64 - 1).unwrap_err();
        assert_eq!(err, ExchangeError::SvgTooLarge);
        assert!(read_svg_limited(SMALL, NEVER, SMALL.len() as u64).is_ok());
    }

    #[test]
    fn gzip_exactly_at_limit_is_accepted() {
        let packed = gzip(SMALL);
        let imp = read_svg_limited(&packed, NEVER, SMALL.len() as u64).unwrap();
        assert_eq!(imp.element_count(), 1);
    }
}
