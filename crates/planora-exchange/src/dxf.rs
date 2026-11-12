//! DXF lesen (Skizzen 9). Das Ergebnis ist die neutrale 2D-Geometrie aus [`crate::import2d`].
//!
//! Gelesen wird mit der Bibliothek `dxf`; sie kennt aber kein HATCH, darum zählt ein eigener
//! kleiner Durchlauf über den Text die Schraffuren. Jede Entität wandert durch
//! [`Reader::entity`]; Blöcke, Texte und Härtung gegen kaputte Dateien kommen in Aufgabe 3 hinzu,
//! dafür ist [`Reader::emit`] die einzige Stelle, an der Elemente in Layer landen.

use crate::error::ExchangeError;
use crate::import2d::{Element2d, Import2d, Layer2d, Skipped, Unit2d};
use crate::limits::CHECK_EVERY;
use dxf::entities::{Entity, EntityType, Polyline, Spline};
use dxf::enums::Units;
use dxf::{Drawing, Vector};
use planora_math::DVec2;
use std::collections::HashMap;
use std::f64::consts::{PI, TAU};
use std::io::Cursor;

/// Toleranz, bis zu der ein Wert als „null“ oder „gleich“ gilt (Zeichnungseinheiten).
const EPS: f64 = 1e-9;

/// Liest eine DXF-Datei (Text oder binär) und liefert die Elemente des Modellbereichs.
/// `cancel` wird regelmäßig gefragt; `true` bricht mit [`ExchangeError::Cancelled`] ab.
pub fn read_dxf(bytes: &[u8], cancel: &dyn Fn() -> bool) -> Result<Import2d, ExchangeError> {
    if cancel() {
        return Err(ExchangeError::Cancelled);
    }
    let drawing = Drawing::load(&mut Cursor::new(bytes))
        .map_err(|e| ExchangeError::DxfBroken(e.to_string()))?;

    let mut reader = Reader {
        layers: Vec::new(),
        index: HashMap::new(),
        skipped: Skipped::default(),
        seen: 0,
        dimensions_seen: 0,
    };
    for e in drawing.entities() {
        reader.tick(cancel)?;
        reader.entity(e);
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

/// Einheit aus `$INSUNITS`; ohne Angabe oder bei Einheiten, die es hier nicht gibt, `None`.
fn unit_of(u: Units) -> Option<Unit2d> {
    match u {
        Units::Millimeters => Some(Unit2d::Millimeter),
        Units::Centimeters => Some(Unit2d::Centimeter),
        Units::Meters => Some(Unit2d::Meter),
        Units::Inches => Some(Unit2d::Inch),
        Units::Feet => Some(Unit2d::Foot),
        _ => None,
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

struct Reader {
    layers: Vec<Layer2d>,
    /// Layername zu Platz in `layers`, in der Reihenfolge des ersten Auftretens.
    index: HashMap<String, usize>,
    skipped: Skipped,
    /// Zahl der bisher gelesenen Entitäten, für den Abbruchtakt.
    seen: u64,
    /// Bemaßungen, die die Bibliothek geliefert hat (nur für binäre Dateien maßgebend).
    dimensions_seen: usize,
}

impl Reader {
    fn tick(&mut self, cancel: &dyn Fn() -> bool) -> Result<(), ExchangeError> {
        self.seen += 1;
        if self.seen.is_multiple_of(CHECK_EVERY) && cancel() {
            return Err(ExchangeError::Cancelled);
        }
        Ok(())
    }

    /// Hängt ein Element an den Layer; legt ihn beim ersten Mal an.
    fn emit(&mut self, layer: &str, el: Element2d) {
        let i = match self.index.get(layer) {
            Some(i) => *i,
            None => {
                self.layers.push(Layer2d {
                    name: layer.to_string(),
                    elements: Vec::new(),
                });
                self.index.insert(layer.to_string(), self.layers.len() - 1);
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
        let layer = e.common.layer.as_str();
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
                let pt = DVec2::new(p.location.x, p.location.y);
                if p.location.z.abs() > EPS {
                    self.skipped.three_d += 1;
                    return;
                }
                if finite2(pt) {
                    self.emit(layer, Element2d::Point(pt));
                } else {
                    self.skipped.degenerate += 1;
                }
            }
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
            // INSERT, TEXT und MTEXT folgen in Aufgabe 3; bis dahin zählen sie als unbekannt.
            _ => self.skipped.unknown += 1,
        }
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
        let all = s.control_points.iter().chain(s.fit_points.iter());
        if all.clone().any(|p| p.z.is_finite() && p.z.abs() > EPS) {
            self.skipped.three_d += 1;
            return;
        }
        let xy = |p: &dxf::Point| DVec2::new(p.x, p.y);
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
