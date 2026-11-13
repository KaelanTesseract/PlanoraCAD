//! DXF lesen (Skizzen 9). Das Ergebnis ist die neutrale 2D-Geometrie aus [`crate::import2d`].
//!
//! Gelesen wird mit der Bibliothek `dxf`; sie kennt aber kein HATCH, darum zählt ein eigener
//! kleiner Durchlauf über den Text die Schraffuren. Jede Entität wandert durch
//! [`Reader::entity`]; INSERT löst Blöcke rekursiv mit einer 2D-Affinabbildung auf, und
//! [`Reader::emit`] ist die einzige Stelle, an der Elemente in Layer landen: Dort wird die
//! Abbildung angewandt, werden Winkel normiert und die Elemente gezählt (Grenze
//! [`MAX_IMPORT_ELEMENTS`]).

use crate::error::ExchangeError;
use crate::import2d::{Element2d, HAlign2d, Import2d, Layer2d, Skipped, Text2d, Unit2d, VAlign2d};
use crate::limits::{CHECK_EVERY, MAX_BLOCK_DEPTH, MAX_DXF_BYTES, MAX_IMPORT_ELEMENTS};
use dxf::entities::{Entity, EntityType, Insert, Polyline, Spline};
use dxf::enums::{AttachmentPoint, HorizontalTextJustification, Units, VerticalTextJustification};
use dxf::{Block, Drawing, Vector};
use planora_math::DVec2;
use std::collections::HashMap;
use std::f64::consts::{PI, TAU};
use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};

/// Toleranz, bis zu der ein Wert als „null“ oder „gleich“ gilt (Zeichnungseinheiten).
const EPS: f64 = 1e-9;

/// Höchstzahl der Entitäten, die beim Auflösen von Blöcken besucht werden dürfen. Schützt vor
/// Dateien, deren Blöcke sich vielfach ineinander einfügen, aber kaum Elemente erzeugen.
const MAX_BLOCK_VISITS: usize = 10 * MAX_IMPORT_ELEMENTS;

/// Liest eine DXF-Datei (Text oder binär) und liefert die Elemente des Modellbereichs.
/// `cancel` wird regelmäßig gefragt; `true` bricht mit [`ExchangeError::Cancelled`] ab.
/// Größer als [`MAX_DXF_BYTES`] darf die Datei nicht sein.
pub fn read_dxf(bytes: &[u8], cancel: &dyn Fn() -> bool) -> Result<Import2d, ExchangeError> {
    read_dxf_limited(bytes, cancel, MAX_DXF_BYTES)
}

/// Wie [`read_dxf`] mit eigener Größengrenze (Tests ohne riesige Dateien).
fn read_dxf_limited(
    bytes: &[u8],
    cancel: &dyn Fn() -> bool,
    max_bytes: u64,
) -> Result<Import2d, ExchangeError> {
    if bytes.len() as u64 > max_bytes {
        return Err(ExchangeError::DxfTooLarge);
    }
    if cancel() {
        return Err(ExchangeError::Cancelled);
    }
    // Die Bibliothek `dxf` ist nicht gegen kaputte Dateien gehärtet und kann bei zerstörten
    // Eingaben in Panik geraten (Überlauf, Indexfehler). Eine Datei darf das Programm aber nie
    // abstürzen lassen: Die Panik wird darum hier abgefangen und wie ein Lesefehler gemeldet.
    // Die App ruft den Leser im Merker `crash::expected_panics` auf (`insert::guarded`), damit
    // der Absturz-Hook dafür keinen Bericht schreibt.
    let drawing = catch_unwind(AssertUnwindSafe(|| Drawing::load(&mut Cursor::new(bytes))))
        .map_err(|_| ExchangeError::DxfBroken("internal error".into()))?
        .map_err(|e| ExchangeError::DxfBroken(e.to_string()))?;

    // Blocknamen gelten ohne Rücksicht auf Groß- und Kleinschreibung.
    let blocks: HashMap<String, &Block> = drawing
        .blocks()
        .map(|b| (b.name.to_uppercase(), b))
        .collect();
    let fonts: HashMap<String, String> = drawing
        .styles()
        .map(|s| (s.name.to_uppercase(), font_of_style(s)))
        .collect();

    let mut reader = Reader {
        cancel,
        blocks,
        fonts,
        layers: Vec::new(),
        index: HashMap::new(),
        skipped: Skipped::default(),
        seen: 0,
        visits: 0,
        count: 0,
        dimensions_seen: 0,
        xf: Aff::IDENTITY,
        depth: 0,
        inherit: None,
        fail: None,
    };
    for e in drawing.entities() {
        if reader.step(false) {
            break;
        }
        reader.entity(e);
    }
    if let Some(err) = reader.fail.take() {
        return Err(err); // Abbruch oder zu viele Elemente: kein Teilergebnis
    }
    match count_marked(bytes, cancel)? {
        Some((hatches, dimensions)) => {
            reader.skipped.hatches = hatches;
            reader.skipped.dimensions = dimensions;
        }
        // Binär: Bemaßungen kommen aus der Bibliothek, Schraffuren sind nicht erkennbar.
        None => reader.skipped.dimensions = reader.dimensions_seen,
    }
    Ok(Import2d {
        layers: reader.layers,
        units: unit_of(drawing.header.default_drawing_units),
        skipped: reader.skipped,
    })
}

/// Schriftname eines Textstils: Name der Schriftdatei ohne Endung, sonst der Stilname.
fn font_of_style(s: &dxf::tables::Style) -> String {
    let file = s.primary_font_file_name.trim();
    let stem = file.rsplit(['/', '\\']).next().unwrap_or(file);
    let stem = stem.rsplit_once('.').map_or(stem, |(a, _)| a);
    if stem.is_empty() {
        s.name.clone()
    } else {
        stem.to_string()
    }
}

/// Einheit aus `$INSUNITS`: jede Längeneinheit mit ihrem Faktor; nur „ohne Einheit“ ergibt
/// `None` (Gesamtprüfung I3: nie „nennt keine Einheit“, wenn die Datei eine nennt).
fn unit_of(u: Units) -> Option<Unit2d> {
    match u {
        Units::Unitless => None,
        Units::Millimeters => Some(Unit2d::Millimeter),
        Units::Centimeters => Some(Unit2d::Centimeter),
        Units::Meters => Some(Unit2d::Meter),
        Units::Inches => Some(Unit2d::Inch),
        Units::Feet => Some(Unit2d::Foot),
        Units::Miles => Some(Unit2d::Mile),
        Units::Kilometers => Some(Unit2d::Kilometer),
        Units::Microinches => Some(Unit2d::Microinch),
        Units::Mils => Some(Unit2d::Mil),
        Units::Yards => Some(Unit2d::Yard),
        Units::Angstroms => Some(Unit2d::Angstrom),
        Units::Nanometers => Some(Unit2d::Nanometer),
        Units::Microns => Some(Unit2d::Micron),
        Units::Decimeters => Some(Unit2d::Decimeter),
        Units::Decameters => Some(Unit2d::Decameter),
        Units::Hectometers => Some(Unit2d::Hectometer),
        Units::Gigameters => Some(Unit2d::Gigameter),
        Units::AstronomicalUnits => Some(Unit2d::AstronomicalUnit),
        Units::LightYears => Some(Unit2d::LightYear),
        Units::Parsecs => Some(Unit2d::Parsec),
        Units::USSurveyFeet => Some(Unit2d::UsSurveyFoot),
        Units::USSurveyInch => Some(Unit2d::UsSurveyInch),
        Units::USSurveyYard => Some(Unit2d::UsSurveyYard),
        Units::USSurveyMile => Some(Unit2d::UsSurveyMile),
    }
}

/// Lage der Zeichenebene einer Entität: Die Ebene ist die XY-Ebene, nach oben oder unten gedreht.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Plane {
    /// Extrusionsrichtung (0,0,1).
    Up,
    /// Extrusionsrichtung (0,0,−1): das Objektkoordinatensystem ist an der Y-Achse gespiegelt.
    Down,
}

impl Plane {
    /// `None`, wenn die Richtung nicht ±Z ist (dann liegt die Entität schräg im Raum).
    fn of(n: &Vector) -> Option<Plane> {
        let len = (n.x * n.x + n.y * n.y + n.z * n.z).sqrt();
        if !len.is_finite() || len < EPS {
            return None;
        }
        let (x, y, z) = (n.x / len, n.y / len, n.z / len);
        if x.abs() > EPS || y.abs() > EPS {
            None
        } else if z > 0.0 {
            Some(Plane::Up)
        } else {
            Some(Plane::Down)
        }
    }

    /// Objektkoordinaten (x, y) in Weltkoordinaten der Ebene.
    fn point(self, x: f64, y: f64) -> DVec2 {
        match self {
            Plane::Up => DVec2::new(x, y),
            Plane::Down => DVec2::new(-x, y),
        }
    }

    /// Bogenwinkel (Radiant, gegen den Uhrzeigersinn von `start` nach `end`) in der Welt.
    /// Beim Spiegeln dreht sich die Laufrichtung um, darum tauschen die Enden.
    fn arc(self, start: f64, end: f64) -> (f64, f64) {
        match self {
            Plane::Up => (start, end),
            Plane::Down => (PI - end, PI - start),
        }
    }
}

/// Affine 2D-Abbildung: x' = a·x + c·y + tx, y' = b·x + d·y + ty.
#[derive(Clone, Copy, Debug)]
struct Aff {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    tx: f64,
    ty: f64,
}

impl Aff {
    const IDENTITY: Aff = Aff {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    fn translate(x: f64, y: f64) -> Aff {
        Aff {
            tx: x,
            ty: y,
            ..Aff::IDENTITY
        }
    }

    fn rotate(angle: f64) -> Aff {
        let (s, c) = angle.sin_cos();
        Aff {
            a: c,
            b: s,
            c: -s,
            d: c,
            tx: 0.0,
            ty: 0.0,
        }
    }

    fn scale(x: f64, y: f64) -> Aff {
        Aff {
            a: x,
            d: y,
            ..Aff::IDENTITY
        }
    }

    /// Spiegelung der x-Achse (Zeichenebene mit Richtung −Z).
    fn mirror_x() -> Aff {
        Aff::scale(-1.0, 1.0)
    }

    /// Erst `other`, dann `self` anwenden.
    fn then_after(self, other: Aff) -> Aff {
        Aff {
            a: self.a * other.a + self.c * other.b,
            b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d,
            d: self.b * other.c + self.d * other.d,
            tx: self.a * other.tx + self.c * other.ty + self.tx,
            ty: self.b * other.tx + self.d * other.ty + self.ty,
        }
    }

    fn apply(&self, p: DVec2) -> DVec2 {
        DVec2::new(
            self.a * p.x + self.c * p.y + self.tx,
            self.b * p.x + self.d * p.y + self.ty,
        )
    }

    /// Nur der lineare Teil (für Richtungen und Vektoren).
    fn lin(&self, v: DVec2) -> DVec2 {
        DVec2::new(self.a * v.x + self.c * v.y, self.b * v.x + self.d * v.y)
    }

    fn is_finite(&self) -> bool {
        [self.a, self.b, self.c, self.d, self.tx, self.ty]
            .iter()
            .all(|v| v.is_finite())
    }

    /// Ist die Abbildung eine Ähnlichkeit (gleicher Maßstab in beiden Achsen, rechtwinklig)?
    /// Dann `(Maßstab, Drehwinkel, spiegelt)`; `spiegelt` bei negativer Determinante.
    fn similarity(&self) -> Option<(f64, f64, bool)> {
        let (l0, l1) = (self.a.hypot(self.b), self.c.hypot(self.d));
        let dot = self.a * self.c + self.b * self.d;
        if l0 > EPS && (l0 - l1).abs() <= 1e-9 * l0 && dot.abs() <= 1e-9 * l0 * l1 {
            let det = self.a * self.d - self.b * self.c;
            Some((l0, self.b.atan2(self.a), det < 0.0))
        } else {
            None
        }
    }

    /// Bildet ein Element ab. Ein Kreis oder Bogen unter ungleichem Maßstab wird zur Ellipse.
    fn element(&self, el: Element2d) -> Element2d {
        let pts = |v: Vec<DVec2>| v.into_iter().map(|p| self.apply(p)).collect::<Vec<_>>();
        match el {
            Element2d::Point(p) => Element2d::Point(self.apply(p)),
            Element2d::Line(a, b) => Element2d::Line(self.apply(a), self.apply(b)),
            Element2d::Quad(p) => Element2d::Quad(p.map(|q| self.apply(q))),
            Element2d::Cubic(p) => Element2d::Cubic(p.map(|q| self.apply(q))),
            Element2d::Nurbs {
                degree,
                knots,
                points,
                weights,
            } => Element2d::Nurbs {
                degree,
                knots,
                points: pts(points),
                weights,
            },
            Element2d::FitSpline(p) => Element2d::FitSpline(pts(p)),
            Element2d::Circle { c, r } => match self.similarity() {
                Some((s, _, _)) => Element2d::Circle {
                    c: self.apply(c),
                    r: r * s,
                },
                None => self.ellipse_from(c, DVec2::new(r, 0.0), DVec2::new(0.0, r), 0.0, TAU),
            },
            Element2d::Arc { c, r, start, end } => match self.similarity() {
                // Eine Spiegelung kehrt den Drehsinn um: Die Enden tauschen und spiegeln an der
                // Drehachse (θ − Winkel).
                Some((s, theta, true)) => Element2d::Arc {
                    c: self.apply(c),
                    r: r * s,
                    start: theta - end,
                    end: theta - start,
                },
                Some((s, theta, false)) => Element2d::Arc {
                    c: self.apply(c),
                    r: r * s,
                    start: start + theta,
                    end: end + theta,
                },
                None => self.ellipse_from(c, DVec2::new(r, 0.0), DVec2::new(0.0, r), start, end),
            },
            Element2d::Ellipse {
                c,
                major,
                ratio,
                start,
                end,
            } => {
                let minor = DVec2::new(-major.y, major.x) * ratio;
                self.ellipse_from(c, major, minor, start, end)
            }
            Element2d::Text(t) => {
                let dir = self.lin(DVec2::new(t.angle.cos(), t.angle.sin()));
                let up = self.lin(DVec2::new(-t.angle.sin(), t.angle.cos()));
                Element2d::Text(Text2d {
                    pos: self.apply(t.pos),
                    angle: dir.y.atan2(dir.x),
                    height: t.height * up.length(),
                    ..t
                })
            }
        }
    }

    /// Bild der Ellipse `c + a·cos s + b·sin s` (s von `s0` bis `s1`) mit den konjugierten
    /// Halbmessern `a`, `b`, wieder als Ellipse mit senkrechten Achsen.
    fn ellipse_from(&self, c: DVec2, a: DVec2, b: DVec2, s0: f64, s1: f64) -> Element2d {
        let (a, b) = (self.lin(a), self.lin(b));
        // Parameterverschiebung t0, bei der der Punkt auf der großen Halbachse liegt.
        let t0 = 0.5 * (2.0 * a.dot(b)).atan2(a.length_squared() - b.length_squared());
        let (st, ct) = t0.sin_cos();
        let major = a * ct + b * st;
        let minor = b * ct - a * st;
        let ratio = (minor.length() / major.length()).min(1.0);
        // Liegt `minor` rechts von `major`, läuft der Parameter rückwärts.
        let flipped = major.x * minor.y - major.y * minor.x < 0.0;
        let (start, end) = if flipped {
            (-(s1 - t0), -(s0 - t0))
        } else {
            (s0 - t0, s1 - t0)
        };
        Element2d::Ellipse {
            c: self.apply(c),
            major,
            ratio,
            start,
            end,
        }
    }
}

/// Normiert Winkel: Bogen `start` in [0, 2π), `end` in (`start`, `start`+2π]; die volle Ellipse
/// ist genau 0..2π.
fn normalize(el: Element2d) -> Element2d {
    match el {
        Element2d::Arc { c, r, start, end } => {
            let s = start.rem_euclid(TAU);
            let mut span = (end - start).rem_euclid(TAU);
            if span < 1e-12 {
                span = TAU; // gleiche Enden aus einer vollen Umdrehung
            }
            Element2d::Arc {
                c,
                r,
                start: s,
                end: s + span,
            }
        }
        Element2d::Ellipse {
            c,
            major,
            ratio,
            start,
            end,
        } => {
            let (s, e) = if (end - start).abs() >= TAU - 1e-9 {
                (0.0, TAU)
            } else {
                let s = start.rem_euclid(TAU);
                let mut span = (end - start).rem_euclid(TAU);
                if span < 1e-12 {
                    span = TAU;
                }
                (s, s + span)
            };
            Element2d::Ellipse {
                c,
                major,
                ratio,
                start: s,
                end: e,
            }
        }
        other => other,
    }
}

struct Reader<'a> {
    cancel: &'a dyn Fn() -> bool,
    /// Blöcke nach Name in Großbuchstaben.
    blocks: HashMap<String, &'a Block>,
    /// Textstil (Großbuchstaben) zu Schriftname.
    fonts: HashMap<String, String>,
    layers: Vec<Layer2d>,
    /// Layername (kleingeschrieben) zu Platz in `layers`, in der Reihenfolge des ersten
    /// Auftretens; die erste Schreibweise gilt.
    index: HashMap<String, usize>,
    skipped: Skipped,
    /// Zahl der bisher gelesenen Entitäten, für den Abbruchtakt.
    seen: u64,
    /// Besuchte Entitäten innerhalb von Blöcken (gegen übergroße Blockketten).
    visits: usize,
    /// Zahl der bisher ausgegebenen Elemente.
    count: usize,
    /// Bemaßungen, die die Bibliothek geliefert hat (nur für binäre Dateien maßgebend).
    dimensions_seen: usize,
    /// Abbildung vom Blockkoordinatensystem in die Zeichnung.
    xf: Aff,
    /// Zahl der gerade geöffneten Blöcke.
    depth: usize,
    /// Layer der umgebenden INSERT-Zeile: Entitäten auf Layer „0“ übernehmen ihn.
    inherit: Option<String>,
    /// Erster Fehler (Abbruch, zu viele Elemente); danach wird nichts mehr verarbeitet.
    fail: Option<ExchangeError>,
}

impl<'a> Reader<'a> {
    /// Zählt eine Entität. Liefert `true`, wenn das Lesen enden soll (Fehler steht in `fail`).
    fn step(&mut self, nested: bool) -> bool {
        if self.fail.is_some() {
            return true;
        }
        self.seen += 1;
        if self.seen.is_multiple_of(CHECK_EVERY) && (self.cancel)() {
            self.fail = Some(ExchangeError::Cancelled);
            return true;
        }
        if nested {
            self.visits += 1;
            if self.visits > MAX_BLOCK_VISITS {
                self.fail = Some(ExchangeError::TooManyElements);
                return true;
            }
        }
        false
    }

    /// Hängt ein Element an den Layer; legt ihn beim ersten Mal an. Wendet die Blockabbildung
    /// an, normiert Winkel und zählt gegen [`MAX_IMPORT_ELEMENTS`].
    fn emit(&mut self, layer: &str, el: Element2d) {
        if self.fail.is_some() {
            return;
        }
        if self.count >= MAX_IMPORT_ELEMENTS {
            self.fail = Some(ExchangeError::TooManyElements);
            return;
        }
        let el = normalize(self.xf.element(el));
        self.count += 1;
        let key = layer.to_lowercase();
        let i = match self.index.get(&key) {
            Some(i) => *i,
            None => {
                self.layers.push(Layer2d {
                    name: layer.to_string(),
                    elements: Vec::new(),
                });
                self.index.insert(key, self.layers.len() - 1);
                self.layers.len() - 1
            }
        };
        self.layers[i].elements.push(el);
    }

    /// Wandelt eine Entität des Modellbereichs um oder zählt sie als übersprungen.
    fn entity(&mut self, e: &Entity) {
        if e.common.is_in_paper_space {
            return; // Papierbereich zählt nicht
        }
        // Auf Layer „0“ im Block gilt der Layer der INSERT-Zeile.
        let layer = match (&self.inherit, e.common.layer.as_str()) {
            (Some(l), "0") => l.clone(),
            (_, own) => own.to_string(),
        };
        let layer = layer.as_str();
        match &e.specific {
            EntityType::Line(l) => {
                if [l.p1.z, l.p2.z]
                    .iter()
                    .any(|z| !z.is_finite() || z.abs() > EPS)
                {
                    // Nicht in der XY-Ebene (und NaN in z wird hier ebenso abgefangen).
                    self.skipped.three_d += 1;
                    return;
                }
                let (a, b) = (DVec2::new(l.p1.x, l.p1.y), DVec2::new(l.p2.x, l.p2.y));
                self.line(layer, a, b);
            }
            EntityType::Circle(c) => {
                let Some(pl) = self.plane(&c.normal) else {
                    return;
                };
                let center = pl.point(c.center.x, c.center.y);
                if !finite2(center) || !(c.radius.is_finite() && c.radius > EPS) {
                    self.skipped.degenerate += 1;
                    return;
                }
                self.emit(
                    layer,
                    Element2d::Circle {
                        c: center,
                        r: c.radius,
                    },
                );
            }
            EntityType::Arc(a) => {
                let Some(pl) = self.plane(&a.normal) else {
                    return;
                };
                let center = pl.point(a.center.x, a.center.y);
                let (s, e) = pl.arc(a.start_angle.to_radians(), a.end_angle.to_radians());
                self.arc(layer, center, a.radius, s, e);
            }
            EntityType::Ellipse(el) => self.ellipse(layer, el),
            EntityType::LwPolyline(p) => {
                let Some(pl) = self.plane(&p.extrusion_direction) else {
                    return;
                };
                let pts: Vec<(DVec2, f64)> = p
                    .vertices
                    .iter()
                    .map(|v| (pl.point(v.x, v.y), v.bulge))
                    .collect();
                self.poly(layer, pts, p.is_closed(), pl);
            }
            EntityType::Polyline(p) => self.polyline(layer, p),
            EntityType::Spline(s) => self.spline(layer, s),
            EntityType::ModelPoint(p) => {
                let Some(pl) = self.plane(&p.extrusion_direction) else {
                    return;
                };
                if !p.location.z.is_finite() || p.location.z.abs() > EPS {
                    self.skipped.three_d += 1;
                    return;
                }
                let pt = pl.point(p.location.x, p.location.y);
                if finite2(pt) {
                    self.emit(layer, Element2d::Point(pt));
                } else {
                    self.skipped.degenerate += 1;
                }
            }
            EntityType::Insert(ins) => self.insert(layer, ins),
            EntityType::Text(t) => self.text(layer, t),
            EntityType::MText(t) => self.mtext(layer, t),
            EntityType::RotatedDimension(_)
            | EntityType::RadialDimension(_)
            | EntityType::DiameterDimension(_)
            | EntityType::AngularThreePointDimension(_)
            | EntityType::OrdinateDimension(_) => self.dimensions_seen += 1,
            EntityType::Image(_) => self.skipped.images += 1,
            EntityType::Face3D(_)
            | EntityType::Solid(_)
            | EntityType::Solid3D(_)
            | EntityType::Body(_)
            | EntityType::Region(_)
            | EntityType::Trace(_) => self.skipped.three_d += 1,
            _ => self.skipped.unknown += 1,
        }
    }

    /// INSERT: löst den Block (auch als Reihen-Feld) mit Lage, Drehung und Maßstab auf.
    /// Zu tief verschachtelte oder unbekannte Blöcke zählen als `unknown`.
    fn insert(&mut self, layer: &str, ins: &Insert) {
        if self.depth >= MAX_BLOCK_DEPTH {
            self.skipped.unknown += 1;
            return;
        }
        let Some(block) = self.blocks.get(&ins.name.to_uppercase()).copied() else {
            self.skipped.unknown += 1;
            return;
        };
        let Some(pl) = self.plane(&ins.extrusion_direction) else {
            return;
        };
        let (sx, sy) = (ins.x_scale_factor, ins.y_scale_factor);
        let nums = [
            ins.location.x,
            ins.location.y,
            ins.rotation,
            ins.column_spacing,
            ins.row_spacing,
            block.base_point.x,
            block.base_point.y,
        ];
        if !nums.iter().all(|v| v.is_finite())
            || !(sx.is_finite() && sy.is_finite() && sx.abs() > EPS && sy.abs() > EPS)
        {
            self.skipped.degenerate += 1;
            return;
        }
        let cols = usize::try_from(ins.column_count).unwrap_or(0).max(1);
        let rows = usize::try_from(ins.row_count).unwrap_or(0).max(1);
        let parent = self.xf;
        let to_world = match pl {
            Plane::Up => Aff::IDENTITY,
            Plane::Down => Aff::mirror_x(),
        };
        let base = Aff::translate(-block.base_point.x, -block.base_point.y);
        let scale = Aff::scale(sx, sy);
        let place = Aff::translate(ins.location.x, ins.location.y)
            .then_after(Aff::rotate(ins.rotation.to_radians()));
        let saved_inherit = self.inherit.replace(layer.to_string());
        self.depth += 1;
        'array: for r in 0..rows {
            for c in 0..cols {
                // Der Reihenabstand liegt im gedrehten, aber nicht skalierten System.
                let offset =
                    Aff::translate(c as f64 * ins.column_spacing, r as f64 * ins.row_spacing);
                let local = place.then_after(offset).then_after(scale).then_after(base);
                let xf = parent.then_after(to_world).then_after(local);
                if self.step(true) {
                    break 'array;
                }
                if !xf.is_finite() {
                    self.skipped.degenerate += 1;
                    continue;
                }
                self.xf = xf;
                for be in &block.entities {
                    if self.step(true) {
                        break 'array;
                    }
                    self.entity(be);
                }
            }
        }
        self.xf = parent;
        self.depth -= 1;
        self.inherit = saved_inherit;
    }

    /// TEXT: ein einzeiliger Text.
    fn text(&mut self, layer: &str, t: &dxf::entities::Text) {
        use HorizontalTextJustification as H;
        use VerticalTextJustification as V;
        let Some(pl) = self.plane(&t.normal) else {
            return;
        };
        // Ausgerichtete Texte (Mitte, rechts, oben …) sitzen am zweiten Punkt, links/Grundlinie
        // am ersten. „Eingepasst“ und „ausgerichtet“ werden links unten übernommen.
        let fitted = matches!(t.horizontal_text_justification, H::Aligned | H::Fit);
        let (halign, mut valign) = match t.horizontal_text_justification {
            H::Left | H::Aligned | H::Fit => (HAlign2d::Left, VAlign2d::Bottom),
            H::Center => (HAlign2d::Center, VAlign2d::Bottom),
            H::Right => (HAlign2d::Right, VAlign2d::Bottom),
            H::Middle => (HAlign2d::Center, VAlign2d::Middle),
        };
        match t.vertical_text_justification {
            V::Baseline | V::Bottom => {}
            V::Middle => valign = VAlign2d::Middle,
            V::Top => valign = VAlign2d::Top,
        }
        let anchored =
            !fitted && (halign != HAlign2d::Left || t.vertical_text_justification != V::Baseline);
        let at = if anchored {
            &t.second_alignment_point
        } else {
            &t.location
        };
        let font = self
            .fonts
            .get(&t.text_style_name.to_uppercase())
            .cloned()
            .unwrap_or_else(|| t.text_style_name.clone());
        self.put_text(
            layer,
            pl,
            Text2d {
                text: clean_text_codes(&t.value),
                font,
                height: t.text_height,
                pos: DVec2::new(at.x, at.y),
                angle: t.rotation.to_radians(),
                halign,
                valign,
            },
        );
    }

    /// MTEXT: mehrzeiliger Text; Formatcodes werden entfernt, `\P` wird zum Zeilenumbruch.
    fn mtext(&mut self, layer: &str, t: &dxf::entities::MText) {
        use AttachmentPoint as A;
        let Some(pl) = self.plane(&t.extrusion_direction) else {
            return;
        };
        let (valign, halign) = match t.attachment_point {
            A::TopLeft => (VAlign2d::Top, HAlign2d::Left),
            A::TopCenter => (VAlign2d::Top, HAlign2d::Center),
            A::TopRight => (VAlign2d::Top, HAlign2d::Right),
            A::MiddleLeft => (VAlign2d::Middle, HAlign2d::Left),
            A::MiddleCenter => (VAlign2d::Middle, HAlign2d::Center),
            A::MiddleRight => (VAlign2d::Middle, HAlign2d::Right),
            A::BottomLeft => (VAlign2d::Bottom, HAlign2d::Left),
            A::BottomCenter => (VAlign2d::Bottom, HAlign2d::Center),
            A::BottomRight => (VAlign2d::Bottom, HAlign2d::Right),
        };
        // Lange Texte kommen in Stücken (Gruppe 3), das letzte Stück steht in Gruppe 1.
        let raw: String = t
            .extended_text
            .iter()
            .map(String::as_str)
            .chain(std::iter::once(t.text.as_str()))
            .collect();
        let (text, inline_font) = strip_mtext(&raw);
        let font = inline_font.unwrap_or_else(|| {
            self.fonts
                .get(&t.text_style_name.to_uppercase())
                .cloned()
                .unwrap_or_else(|| t.text_style_name.clone())
        });
        // Die x-Richtung (Gruppe 11) hat Vorrang vor dem Drehwinkel (Gruppe 50).
        let (xd_x, xd_y) = (t.x_axis_direction.x, t.x_axis_direction.y);
        let angle = if xd_x.hypot(xd_y) > EPS && !(xd_x == 1.0 && xd_y == 0.0) {
            xd_y.atan2(xd_x)
        } else {
            t.rotation_angle.to_radians()
        };
        self.put_text(
            layer,
            pl,
            Text2d {
                text,
                font,
                height: t.initial_text_height,
                pos: DVec2::new(t.insertion_point.x, t.insertion_point.y),
                angle,
                halign,
                valign,
            },
        );
    }

    /// Prüft und legt einen Text ab; `pos` und `angle` sind noch in Objektkoordinaten.
    fn put_text(&mut self, layer: &str, pl: Plane, mut t: Text2d) {
        if t.text.trim().is_empty()
            || !(t.height.is_finite() && t.height > EPS)
            || !finite2(t.pos)
            || !t.angle.is_finite()
        {
            self.skipped.degenerate += 1;
            return;
        }
        if pl == Plane::Down {
            t.pos = pl.point(t.pos.x, t.pos.y);
            t.angle = PI - t.angle;
        }
        self.emit(layer, Element2d::Text(t));
    }

    /// Prüft die Ebene; schräge Ebenen werden gezählt und übersprungen.
    fn plane(&mut self, n: &Vector) -> Option<Plane> {
        let p = Plane::of(n);
        if p.is_none() {
            self.skipped.three_d += 1;
        }
        p
    }

    fn line(&mut self, layer: &str, a: DVec2, b: DVec2) {
        if !finite2(a) || !finite2(b) || a.distance(b) <= EPS {
            self.skipped.degenerate += 1;
            return;
        }
        self.emit(layer, Element2d::Line(a, b));
    }

    fn arc(&mut self, layer: &str, c: DVec2, r: f64, start: f64, end: f64) {
        let span = (end - start).rem_euclid(TAU);
        if !finite2(c)
            || !(r.is_finite() && r > EPS)
            || !start.is_finite()
            || !end.is_finite()
            || (span < 1e-12 && end == start)
        {
            self.skipped.degenerate += 1;
            return;
        }
        self.emit(layer, Element2d::Arc { c, r, start, end });
    }

    fn ellipse(&mut self, layer: &str, el: &dxf::entities::Ellipse) {
        let Some(pl) = self.plane(&el.normal) else {
            return;
        };
        if el.center.z.abs() > EPS || el.major_axis.z.abs() > EPS {
            self.skipped.three_d += 1;
            return;
        }
        let c = DVec2::new(el.center.x, el.center.y);
        let major = DVec2::new(el.major_axis.x, el.major_axis.y);
        let ratio = el.minor_axis_ratio;
        let (s, e) = (el.start_parameter, el.end_parameter);
        let ok = finite2(c)
            && finite2(major)
            && major.length() > EPS
            && ratio.is_finite()
            && ratio > EPS
            && ratio <= 1.0 + EPS
            && s.is_finite()
            && e.is_finite()
            && !(s == e);
        if !ok {
            self.skipped.degenerate += 1;
            return;
        }
        // Mit Richtung (0,0,−1) läuft der Parameter im Uhrzeigersinn: umdrehen.
        let (start, end) = match pl {
            Plane::Up => (s, e),
            Plane::Down => (-e, -s),
        };
        self.emit(
            layer,
            Element2d::Ellipse {
                c,
                major,
                ratio: ratio.min(1.0),
                start,
                end,
            },
        );
    }

    /// Alte POLYLINE mit VERTEX-Folge: nur 2D-Linienzüge, Netze und 3D-Züge werden übersprungen.
    fn polyline(&mut self, layer: &str, p: &Polyline) {
        if p.is_3d_polyline() || p.is_3d_polygon_mesh() || p.is_polyface_mesh() {
            self.skipped.three_d += 1;
            return;
        }
        let Some(pl) = self.plane(&p.normal) else {
            return;
        };
        let pts: Vec<(DVec2, f64)> = p
            .vertices()
            // Zusätzliche Kontrollpunkte der Splinekurve (Flag 16) gehören nicht zur Kontur.
            .filter(|v| v.flags & 16 == 0)
            .map(|v| (pl.point(v.location.x, v.location.y), v.bulge))
            .collect();
        self.poly(layer, pts, p.is_closed(), pl);
    }

    /// Linienzug mit Bulge-Werten. `pts` liegen schon in Weltkoordinaten der Ebene.
    fn poly(&mut self, layer: &str, mut pts: Vec<(DVec2, f64)>, closed: bool, pl: Plane) {
        // Das Spiegeln kehrt die Laufrichtung um, damit auch das Vorzeichen jedes Bulge.
        if pl == Plane::Down {
            for p in &mut pts {
                p.1 = -p.1;
            }
        }
        // Wiederholt der geschlossene Zug den ersten Punkt am Ende, wäre das letzte Segment
        // null lang: Den doppelten Endpunkt weglassen.
        if closed && pts.len() > 1 {
            let (first, last) = (pts[0].0, pts[pts.len() - 1].0);
            if finite2(first) && finite2(last) && first.distance(last) <= EPS {
                pts.pop();
            }
        }
        // Ein geschlossener Zug aus zwei Punkten, dessen beide Segmente gerade sind, hätte eine
        // zweite Linie genau über der ersten: Dann nur eine Linie. Mit Bulge an einem Punkt
        // bleiben beide Segmente (Bogen plus Sehne, bei zwei Bulges eine Linse).
        let closed = closed && !(pts.len() == 2 && pts.iter().all(|p| p.1.abs() < 1e-12));
        if pts.len() < 2 {
            self.skipped.degenerate += 1;
            return;
        }
        let n = pts.len();
        let segments = if closed { n } else { n - 1 };
        for i in 0..segments {
            let (a, bulge) = pts[i];
            let b = pts[(i + 1) % n].0;
            self.segment(layer, a, b, bulge);
        }
    }

    /// Ein Segment: Linie, bei Bulge ≠ 0 ein Bogen. Bulge = tan(Winkel / 4); positiv läuft der
    /// Bogen von `a` nach `b` gegen den Uhrzeigersinn.
    fn segment(&mut self, layer: &str, a: DVec2, b: DVec2, bulge: f64) {
        if !bulge.is_finite() {
            self.skipped.degenerate += 1;
            return;
        }
        if bulge.abs() < 1e-12 {
            return self.line(layer, a, b);
        }
        let chord = a.distance(b);
        if !finite2(a) || !finite2(b) || chord <= EPS {
            self.skipped.degenerate += 1;
            return;
        }
        let theta = 4.0 * bulge.atan(); // eingeschlossener Winkel mit Vorzeichen
        let r = chord / 2.0 / (theta.abs() / 2.0).sin();
        let mid = (a + b) * 0.5;
        let dir = (b - a) / chord;
        let left = DVec2::new(-dir.y, dir.x);
        // Der Mittelpunkt liegt links der Sehne, wenn der Bogen gegen den Uhrzeigersinn läuft.
        let h = chord / 2.0 / (theta / 2.0).tan();
        let c = mid + left * h;
        let (a0, a1) = ((a - c).to_angle(), (b - c).to_angle());
        let (start, end) = if bulge > 0.0 { (a0, a1) } else { (a1, a0) };
        self.arc(layer, c, r, start, end);
    }

    fn spline(&mut self, layer: &str, s: &Spline) {
        let Some(pl) = self.plane(&s.normal) else {
            return;
        };
        let all = s.control_points.iter().chain(s.fit_points.iter());
        if all.clone().any(|p| p.z.is_finite() && p.z.abs() > EPS) {
            self.skipped.three_d += 1;
            return;
        }
        let xy = |p: &dxf::Point| pl.point(p.x, p.y);
        if !s.control_points.is_empty() {
            let points: Vec<DVec2> = s.control_points.iter().map(xy).collect();
            let degree = usize::try_from(s.degree_of_curve).unwrap_or(0);
            let knots = s.knot_values.clone();
            // Ein gültiger NURBS hat Grad ≥ 1 und genau Punkte + Grad + 1 Knoten.
            let valid = degree >= 1
                && points.len() > degree
                && knots.len() == points.len() + degree + 1
                && points.iter().all(|p| finite2(*p))
                && knots.iter().all(|k| k.is_finite());
            // Gewichte nur, wenn es zu jedem Punkt eins gibt und sie nicht alle gleich sind.
            let weights = (s.weight_values.len() == points.len()
                && s.weight_values.iter().all(|w| w.is_finite() && *w > 0.0)
                && s.weight_values.windows(2).any(|w| w[0] != w[1]))
            .then(|| s.weight_values.clone());
            if !valid {
                self.skipped.degenerate += 1;
                return;
            }
            self.emit(
                layer,
                Element2d::Nurbs {
                    degree,
                    knots,
                    points,
                    weights,
                },
            );
        } else if s.fit_points.len() >= 2 {
            let pts: Vec<DVec2> = s.fit_points.iter().map(xy).collect();
            if pts.iter().all(|p| finite2(*p)) {
                self.emit(layer, Element2d::FitSpline(pts));
            } else {
                self.skipped.degenerate += 1;
            }
        } else {
            self.skipped.degenerate += 1;
        }
    }
}

fn finite2(p: DVec2) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

/// Zählt HATCH und DIMENSION im Modellbereich eines Text-DXF (Schraffuren, Bemaßungen).
/// Die Bibliothek `dxf` überliest HATCH ganz und verwirft Bemaßungen aus R12-Dateien ohne
/// Unterklassen-Marken; darum zählt dieser Durchlauf über den Text beides selbst.
/// Binäre DXF liefern `None`.
fn count_marked(
    bytes: &[u8],
    cancel: &dyn Fn() -> bool,
) -> Result<Option<(usize, usize)>, ExchangeError> {
    if bytes.starts_with(b"AutoCAD Binary DXF") {
        return Ok(None);
    }
    let (mut hatches, mut dimensions) = (0usize, 0usize);
    let mut in_entities = false;
    // Läuft gerade ein HATCH (`'H'`) oder eine DIMENSION (`'D'`)? Dazu, ob sie im
    // Papierbereich liegt (Gruppe 67 = 1); sie wird erst am Ende der Entität gezählt.
    let mut open: Option<(char, bool)> = None;
    let mut code: Option<i32> = None;
    let mut after_section = false;
    let mut finish = |open: &mut Option<(char, bool)>| {
        if let Some((kind, false)) = open.take() {
            if kind == 'H' {
                hatches += 1;
            } else {
                dimensions += 1;
            }
        }
    };
    for (n, raw) in bytes.split(|b| *b == b'\n').enumerate() {
        if (n as u64).is_multiple_of(CHECK_EVERY) && cancel() {
            return Err(ExchangeError::Cancelled);
        }
        let line = String::from_utf8_lossy(raw);
        let line = line.trim();
        let Some(c) = code.take() else {
            code = Some(line.parse().unwrap_or(i32::MIN));
            continue;
        };
        match c {
            0 => {
                finish(&mut open);
                after_section = line == "SECTION";
                if line == "ENDSEC" {
                    in_entities = false;
                }
                if in_entities {
                    open = match line {
                        "HATCH" => Some(('H', false)),
                        "DIMENSION" => Some(('D', false)),
                        _ => None,
                    };
                }
            }
            2 if after_section => {
                in_entities = line == "ENTITIES";
                after_section = false;
            }
            67 if line == "1" => {
                if let Some((_, paper)) = open.as_mut() {
                    *paper = true;
                }
            }
            _ => {}
        }
    }
    finish(&mut open);
    Ok(Some((hatches, dimensions)))
}

/// Entfernt die Sonderzeichen-Codes von TEXT: `%%c` Ø, `%%d` °, `%%p` ±, `%%%` %; `%%u`, `%%o`
/// (Unter- und Überstreichen) fallen weg.
fn clean_text_codes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '%' && it.peek() == Some(&'%') {
            it.next();
            match it.next().map(|k| k.to_ascii_lowercase()) {
                Some('c') => out.push('Ø'),
                Some('d') => out.push('°'),
                Some('p') => out.push('±'),
                Some('%') => out.push('%'),
                Some('u' | 'o' | 'k') | None => {}
                Some(other) => {
                    out.push_str("%%");
                    out.push(other);
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Entfernt die Formatcodes von MTEXT und liefert den Klartext und die erste genannte Schrift
/// (Code f mit Schriftname, etwa `\fArial|b1;`). `\P` ist ein Zeilenumbruch, `\~` ein
/// Leerzeichen, `\\`, `\{` und `\}` stehen für sich selbst, geschweifte Klammern gruppieren
/// nur, `\S` (Stapelbruch) wird zu `a/b`.
fn strip_mtext(s: &str) -> (String, Option<String>) {
    let mut out = String::with_capacity(s.len());
    let mut font: Option<String> = None;
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    // Liest bis zum nächsten `;` und liefert den Inhalt davor.
    let until_semicolon = |from: usize| -> (String, usize) {
        let mut j = from;
        let mut v = String::new();
        while j < chars.len() && chars[j] != ';' {
            v.push(chars[j]);
            j += 1;
        }
        (v, (j + 1).min(chars.len()))
    };
    while i < chars.len() {
        let c = chars[i];
        match c {
            '{' | '}' => i += 1,
            '\\' if i + 1 < chars.len() => {
                let k = chars[i + 1];
                i += 2;
                match k {
                    // Absatz- und Spaltenumbruch; `\p...;` ist nur ein Absatzformat.
                    'P' | 'N' => out.push('\n'),
                    'p' => i = until_semicolon(i).1,
                    '~' => out.push(' '),
                    '\\' | '{' | '}' => out.push(k),
                    'f' | 'F' => {
                        let (v, next) = until_semicolon(i);
                        i = next;
                        let name = v.split('|').next().unwrap_or("").trim();
                        if font.is_none() && !name.is_empty() {
                            font = Some(name.to_string());
                        }
                    }
                    'S' => {
                        let (v, next) = until_semicolon(i);
                        i = next;
                        // Zähler^Nenner (Hochstellung) wird zu „Zähler Nenner“, sonst bleibt es.
                        out.push_str(&v.replace('^', " ").replace('#', "/"));
                    }
                    'U' if chars.get(i) == Some(&'+') => {
                        let hex: String = chars[i + 1..].iter().take(4).collect();
                        match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                            Some(ch) => {
                                out.push(ch);
                                i += 1 + hex.chars().count();
                            }
                            None => out.push('U'),
                        }
                    }
                    // Befehle mit Argument bis `;`: Höhe, Farbe, Breite, Abstand, Neigung …
                    'H' | 'C' | 'c' | 'W' | 'T' | 'Q' | 'A' | 'h' | 'w' | 'q' | 'a' => {
                        i = until_semicolon(i).1;
                    }
                    // Schalter ohne Argument: Unterstreichen, Überstreichen, Durchstreichen.
                    'L' | 'l' | 'O' | 'o' | 'K' | 'k' => {}
                    other => out.push(other),
                }
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    (clean_text_codes(&out), font)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_over_the_limit_is_refused_before_parsing() {
        let never: &dyn Fn() -> bool = &|| false;
        // Kein gültiges DXF: Würde gelesen, käme `DxfBroken`.
        let bytes = vec![b'x'; 65];
        assert_eq!(
            read_dxf_limited(&bytes, never, 64).unwrap_err(),
            ExchangeError::DxfTooLarge
        );
        assert!(matches!(
            read_dxf_limited(&bytes, never, 65).unwrap_err(),
            ExchangeError::DxfBroken(_)
        ));
        assert!(!ExchangeError::DxfTooLarge.to_string().is_empty());
    }
}
