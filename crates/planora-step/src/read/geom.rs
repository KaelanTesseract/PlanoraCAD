//! Analytische Geometrie aus STEP (STEP-Import, Aufgabe 2): Punkte, Richtungen,
//! Koordinatensysteme, Kurven und Flächen als Typen des Kernels.
//!
//! Die Parametrisierungen von STEP (ISO 10303-42) und Kernel stimmen für Ebene, Zylinder,
//! Kegel (`radius + v · tan α`), Kugel, Torus, Kreis und Ellipse überein; die natürliche Normale
//! zeigt bei beiden von der Achse weg. Deshalb ist `flipped` hier immer `false`. Längen werden
//! mit der Längeneinheit in Millimeter umgerechnet, Winkel mit der Winkeleinheit in Radiant.
//!
//! Ergebnisse werden je Nummer zwischengespeichert (`StepFile::entity` zerlegt bei jedem Aufruf
//! neu). Der Auflösungsstapel erkennt Verweise im Kreis und begrenzt die Tiefe auf
//! [`MAX_RESOLVE_DEPTH`].

use crate::read::error::StepReadError;
use crate::read::index::StepFile;
use crate::read::units::Units;
use crate::read::value::{Entity, Value};
use planora_kernel::{Curve, Surface};
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec3, Frame};
use std::collections::HashMap;
use std::f64::consts::FRAC_PI_2;

/// Höchste Tiefe des Auflösungsstapels.
pub const MAX_RESOLVE_DEPTH: usize = 64;

/// Kürzeste Richtung, die noch eine Richtung ist (vor dem Normieren).
const MIN_DIRECTION: f64 = 1e-12;

/// Grund, weshalb ein Körper ausgelassen wird; fertig für die Oberfläche.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skip(pub String);

impl From<StepReadError> for Skip {
    fn from(e: StepReadError) -> Skip {
        Skip(e.to_string())
    }
}

/// Trägerfläche einer Fläche.
#[derive(Clone, Debug)]
pub enum SurfaceGeom {
    /// Fertige Fläche; `flipped`: Die Normale von STEP zeigt gegen die natürliche des Kernels.
    Ready { surface: Surface, flipped: bool },
    /// Strangfläche aus einer Freiformkurve: der Bereich entlang `dir` folgt aus den Schleifen (Aufgabe 5).
    Extrusion {
        curve: NurbsCurve,
        dir: DVec3,
        flipped: bool,
    },
}

/// Typen, die in zusammengesetzten Einträgen nur Obertypen sind und keinen Namen abgeben.
const GENERIC: &[&str] = &[
    "BOUNDED_CURVE",
    "BOUNDED_SURFACE",
    "CURVE",
    "SURFACE",
    "GEOMETRIC_REPRESENTATION_ITEM",
    "REPRESENTATION_ITEM",
    "B_SPLINE_CURVE",
    "B_SPLINE_SURFACE",
];

/// Typname eines Eintrags für Meldungen: einfacher Typ oder der erste kennzeichnende Teil eines
/// zusammengesetzten Eintrags.
fn type_name(e: &Entity) -> String {
    let names = e.names();
    names
        .iter()
        .find(|n| !GENERIC.contains(n))
        .or(names.first())
        .map_or_else(String::new, |n| (*n).to_string())
}

/// Löst Geometrie-Einträge einer Datei auf.
pub struct Resolver<'f> {
    file: &'f StepFile,
    units: Units,
    points: HashMap<u64, Result<DVec3, Skip>>,
    directions: HashMap<u64, Result<DVec3, Skip>>,
    placements: HashMap<u64, Result<Frame, Skip>>,
    curves: HashMap<u64, Result<Curve, Skip>>,
    surfaces: HashMap<u64, Result<SurfaceGeom, Skip>>,
    stack: Vec<u64>,
}

impl<'f> Resolver<'f> {
    pub fn new(file: &'f StepFile, units: Units) -> Resolver<'f> {
        Resolver {
            file,
            units,
            points: HashMap::new(),
            directions: HashMap::new(),
            placements: HashMap::new(),
            curves: HashMap::new(),
            surfaces: HashMap::new(),
            stack: Vec::new(),
        }
    }

    pub fn units(&self) -> &Units {
        &self.units
    }

    pub fn file(&self) -> &'f StepFile {
        self.file
    }

    /// Grund mit der Zeile von `#id`.
    fn bad(&self, id: u64, msg: impl Into<String>) -> Skip {
        self.file.fail(id, msg).into()
    }

    /// Führt `f` mit `#id` auf dem Stapel aus: schon auf dem Stapel → `Cycle`, mehr als
    /// [`MAX_RESOLVE_DEPTH`] Ebenen → `TooDeep`.
    fn guarded<T>(
        &mut self,
        id: u64,
        f: impl FnOnce(&mut Self) -> Result<T, Skip>,
    ) -> Result<T, Skip> {
        let line = self.file.line(id).ok_or(StepReadError::NoEntity { id })?;
        if self.stack.contains(&id) {
            return Err(StepReadError::Cycle { line, id }.into());
        }
        if self.stack.len() >= MAX_RESOLVE_DEPTH {
            return Err(StepReadError::TooDeep { line }.into());
        }
        self.stack.push(id);
        let r = f(self);
        self.stack.pop();
        r
    }

    /// Parameter `i` des Teils `name` von `e` (Fehler mit Zeile).
    fn arg<'e>(&self, id: u64, e: &'e Entity, name: &str, i: usize) -> Result<&'e Value, Skip> {
        e.arg(name, i).map_err(|m| self.bad(id, m))
    }

    fn reference(&self, id: u64, e: &Entity, name: &str, i: usize) -> Result<u64, Skip> {
        self.arg(id, e, name, i)?
            .reference()
            .map_err(|m| self.bad(id, m))
    }

    /// Länge in mm; endlich und größer als null.
    fn radius(&self, id: u64, e: &Entity, name: &str, i: usize, what: &str) -> Result<f64, Skip> {
        self.length(id, e, name, i, what, false)
    }

    /// Länge in mm; endlich und größer als null (mit `zero_ok` auch null).
    fn length(
        &self,
        id: u64,
        e: &Entity,
        name: &str,
        i: usize,
        what: &str,
        zero_ok: bool,
    ) -> Result<f64, Skip> {
        let raw = self
            .arg(id, e, name, i)?
            .measure()
            .map_err(|m| self.bad(id, m))?;
        let r = raw * self.units.length.mm();
        if !r.is_finite() {
            return Err(self.bad(id, format!("{what} ist keine endliche Zahl.")));
        }
        if r < 0.0 || (r == 0.0 && !zero_ok) {
            return Err(self.bad(id, format!("{what} ist nicht größer als null.")));
        }
        Ok(r)
    }

    /// `CARTESIAN_POINT` in mm.
    pub fn point(&mut self, id: u64) -> Result<DVec3, Skip> {
        if let Some(r) = self.points.get(&id) {
            return r.clone();
        }
        let r = self.guarded(id, |s| s.point_now(id));
        self.points.insert(id, r.clone());
        r
    }

    fn point_now(&mut self, id: u64) -> Result<DVec3, Skip> {
        let e = self.file.entity(id)?;
        let c = self
            .arg(id, &e, "CARTESIAN_POINT", 1)?
            .reals()
            .map_err(|m| self.bad(id, m))?;
        let [x, y, z] = c[..] else {
            return Err(self.bad(id, "Ein Punkt braucht drei Koordinaten."));
        };
        let p = DVec3::new(x, y, z) * self.units.length.mm();
        if !p.is_finite() {
            return Err(self.bad(id, "Ein Punkt hat keine endlichen Koordinaten."));
        }
        Ok(p)
    }

    /// `DIRECTION`, normiert auf Länge 1.
    pub fn direction(&mut self, id: u64) -> Result<DVec3, Skip> {
        if let Some(r) = self.directions.get(&id) {
            return r.clone();
        }
        let r = self.guarded(id, |s| s.direction_now(id));
        self.directions.insert(id, r.clone());
        r
    }

    fn direction_now(&mut self, id: u64) -> Result<DVec3, Skip> {
        let e = self.file.entity(id)?;
        let c = self
            .arg(id, &e, "DIRECTION", 1)?
            .reals()
            .map_err(|m| self.bad(id, m))?;
        let [x, y, z] = c[..] else {
            return Err(self.bad(id, "Eine Richtung braucht drei Werte."));
        };
        let d = DVec3::new(x, y, z);
        let len = d.length();
        if !len.is_finite() {
            return Err(self.bad(id, "Eine Richtung hat keine endlichen Werte."));
        }
        if len < MIN_DIRECTION {
            return Err(self.bad(id, "Eine Richtung hat die Länge null."));
        }
        Ok(d / len)
    }

    /// `AXIS2_PLACEMENT_3D` als rechtshändiges, orthonormales Koordinatensystem: `z` = Achse
    /// (`$` → Welt-z), `x` = Bezugsrichtung (`$` → Welt-x) senkrecht zu `z` gemacht (parallel zu
    /// `z` → beliebige Senkrechte), `y = z × x`. Ein schiefes oder linkshändiges System kann so
    /// nicht entstehen.
    pub fn placement(&mut self, id: u64) -> Result<Frame, Skip> {
        if let Some(r) = self.placements.get(&id) {
            return r.clone();
        }
        let r = self.guarded(id, |s| s.placement_now(id));
        self.placements.insert(id, r.clone());
        r
    }

    fn placement_now(&mut self, id: u64) -> Result<Frame, Skip> {
        const P: &str = "AXIS2_PLACEMENT_3D";
        let e = self.file.entity(id)?;
        let origin = self.reference(id, &e, P, 1)?;
        let origin = self.point(origin)?;
        let z = match self.arg(id, &e, P, 2)? {
            v if v.is_unset() => DVec3::Z,
            v => self.direction(v.reference().map_err(|m| self.bad(id, m))?)?,
        };
        let r = match self.arg(id, &e, P, 3)? {
            v if v.is_unset() => DVec3::X,
            v => self.direction(v.reference().map_err(|m| self.bad(id, m))?)?,
        };
        let x = r - z * r.dot(z);
        let x = if x.length() < MIN_DIRECTION {
            z.any_orthonormal_vector()
        } else {
            x.normalize()
        };
        let y = z.cross(x);
        Ok(Frame { origin, x, y, z })
    }

    /// Trägerkurve einer Kante.
    pub fn curve(&mut self, id: u64) -> Result<Curve, Skip> {
        if let Some(r) = self.curves.get(&id) {
            return r.clone();
        }
        let r = self.guarded(id, |s| s.curve_now(id));
        self.curves.insert(id, r.clone());
        r
    }

    fn curve_now(&mut self, id: u64) -> Result<Curve, Skip> {
        let e = self.file.entity(id)?;
        if e.is("LINE") {
            let p = self.reference(id, &e, "LINE", 1)?;
            let origin = self.point(p)?;
            let v = self.reference(id, &e, "LINE", 2)?;
            let dir = self.vector_direction(v)?;
            return Ok(Curve::Line { origin, dir });
        }
        if e.is("CIRCLE") {
            let a = self.reference(id, &e, "CIRCLE", 1)?;
            let frame = self.placement(a)?;
            let radius = self.radius(id, &e, "CIRCLE", 2, "Der Radius eines Kreises")?;
            return Ok(Curve::Circle { frame, radius });
        }
        if e.is("ELLIPSE") {
            let a = self.reference(id, &e, "ELLIPSE", 1)?;
            let frame = self.placement(a)?;
            let what = "Eine Halbachse einer Ellipse";
            // Wie der Kernel: erste Halbachse entlang x, zweite entlang y (auch wenn kürzer).
            let major = self.radius(id, &e, "ELLIPSE", 2, what)?;
            let minor = self.radius(id, &e, "ELLIPSE", 3, what)?;
            return Ok(Curve::Ellipse {
                frame,
                major,
                minor,
            });
        }
        for name in ["SURFACE_CURVE", "SEAM_CURVE", "INTERSECTION_CURVE"] {
            if e.is(name) {
                let c = self.reference(id, &e, name, 1)?;
                return self.curve(c);
            }
        }
        if e.is("TRIMMED_CURVE") {
            // Die Ecken der Kante begrenzen die Kurve; die Grundkurve genügt.
            let c = self.reference(id, &e, "TRIMMED_CURVE", 1)?;
            return self.curve(c);
        }
        Err(Skip(format!(
            "Kante vom Typ {} wird noch nicht unterstützt.",
            type_name(&e)
        )))
    }

    /// Richtung eines `VECTOR` (Länge 1; der Betrag zählt für eine Gerade nicht).
    fn vector_direction(&mut self, id: u64) -> Result<DVec3, Skip> {
        self.guarded(id, |s| {
            let e = s.file.entity(id)?;
            let d = s.reference(id, &e, "VECTOR", 1)?;
            s.direction(d)
        })
    }

    /// Trägerfläche einer Fläche.
    pub fn surface(&mut self, id: u64) -> Result<SurfaceGeom, Skip> {
        if let Some(r) = self.surfaces.get(&id) {
            return r.clone();
        }
        let r = self.guarded(id, |s| s.surface_now(id));
        self.surfaces.insert(id, r.clone());
        r
    }

    fn surface_now(&mut self, id: u64) -> Result<SurfaceGeom, Skip> {
        let e = self.file.entity(id)?;
        let ready = |surface| {
            Ok(SurfaceGeom::Ready {
                surface,
                flipped: false,
            })
        };
        let frame_of = |s: &mut Self, name: &str| -> Result<Frame, Skip> {
            let a = s.reference(id, &e, name, 1)?;
            s.placement(a)
        };
        if e.is("PLANE") {
            let frame = frame_of(self, "PLANE")?;
            return ready(Surface::Plane { frame });
        }
        if e.is("CYLINDRICAL_SURFACE") {
            const N: &str = "CYLINDRICAL_SURFACE";
            let frame = frame_of(self, N)?;
            let radius = self.radius(id, &e, N, 2, "Der Radius eines Zylinders")?;
            return ready(Surface::Cylinder { frame, radius });
        }
        if e.is("CONICAL_SURFACE") {
            const N: &str = "CONICAL_SURFACE";
            let frame = frame_of(self, N)?;
            // ISO 10303-42 erlaubt am Ursprung den Radius null (Spitze); der Kernel auch.
            let radius = self.length(id, &e, N, 2, "Der Radius eines Kegels", true)?;
            let raw = self
                .arg(id, &e, N, 3)?
                .measure()
                .map_err(|m| self.bad(id, m))?;
            let alpha = raw * self.units.angle_rad;
            if !(alpha.is_finite() && alpha > 0.0 && alpha < FRAC_PI_2) {
                return Err(self.bad(
                    id,
                    "Der halbe Öffnungswinkel eines Kegels liegt nicht zwischen 0 und 90 Grad.",
                ));
            }
            return ready(Surface::Cone {
                frame,
                radius,
                slope: alpha.tan(),
            });
        }
        if e.is("SPHERICAL_SURFACE") {
            const N: &str = "SPHERICAL_SURFACE";
            let frame = frame_of(self, N)?;
            let radius = self.radius(id, &e, N, 2, "Der Radius einer Kugel")?;
            return ready(Surface::Sphere { frame, radius });
        }
        if e.is("TOROIDAL_SURFACE") {
            const N: &str = "TOROIDAL_SURFACE";
            let frame = frame_of(self, N)?;
            let major = self.radius(id, &e, N, 2, "Der große Radius eines Torus")?;
            let minor = self.radius(id, &e, N, 3, "Der kleine Radius eines Torus")?;
            if minor >= major {
                return Err(Skip(
                    "Eine Torusfläche mit kleinem Radius größer oder gleich dem großen wird noch nicht unterstützt."
                        .into(),
                ));
            }
            return ready(Surface::Torus {
                frame,
                major,
                minor,
            });
        }
        Err(Skip(format!(
            "Fläche vom Typ {} wird noch nicht unterstützt.",
            type_name(&e)
        )))
    }
}
