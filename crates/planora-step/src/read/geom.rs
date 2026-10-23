//! Analytische Geometrie aus STEP (STEP-Import, Aufgabe 2): Punkte, Richtungen,
//! Koordinatensysteme, Kurven und Flächen als Typen des Kernels.
//!
//! Die Parametrisierungen von STEP (ISO 10303-42) und Kernel stimmen für Ebene, Zylinder,
//! Kegel (`radius + v · tan α`), Kugel, Torus, Kreis und Ellipse überein; die natürliche Normale
//! zeigt bei beiden von der Achse weg. Deshalb ist `flipped` dort immer `false`. Längen werden
//! mit der Längeneinheit in Millimeter umgerechnet, Winkel mit der Winkeleinheit in Radiant.
//!
//! Freiformkurven und -flächen (B-Splines in allen Knotenformen), Dreh- und Strangflächen
//! (Aufgabe 5) rechnet [`crate::read::spline`] um; `flipped` wird dort gemessen bzw. folgt aus
//! dem Tausch von u und v.
//!
//! Ergebnisse werden je Nummer zwischengespeichert (`StepFile::entity` zerlegt bei jedem Aufruf
//! neu). Der Weg der Verweise ([`RefPath`]) erkennt Verweise im Kreis und begrenzt die Tiefe auf
//! [`MAX_RESOLVE_DEPTH`]. Ein Scheitern an der Tiefe hängt vom Aufrufweg ab und wird deshalb nie
//! gespeichert; Erfolge und Fehler, die nur vom Eintrag selbst abhängen (auch Kreise), schon.

use crate::read::error::StepReadError;
use crate::read::index::{RefPath, StepFile};
use crate::read::spline::{
    clamp_knots, expand_knots, extrusion, finish_nurbs_surface, implicit_knots, normalize_weights,
    revolution_trimmed,
};
use crate::read::units::Units;
use crate::read::value::{Entity, Value};
use planora_kernel::{Curve, Surface};
use planora_math::nurbs::{MAX_DEGREE, NurbsCurve, NurbsSurface};
use planora_math::{DVec3, Frame};
use std::collections::HashMap;
use std::f64::consts::FRAC_PI_2;
use std::sync::Arc;

/// Höchste Tiefe des Auflösungswegs (kleiner als `MAX_REF_DEPTH`, weil die Rahmen groß sind).
pub const MAX_RESOLVE_DEPTH: usize = 64;

/// Kürzeste Richtung, die noch eine Richtung ist (vor dem Normieren).
const MIN_DIRECTION: f64 = 1e-12;

/// Eine angegebene Bezugsrichtung, deren Anteil senkrecht zur Achse (nach dem Normieren)
/// kürzer ist, liegt auf der Achse (ISO 10303-42 verbietet das).
const MIN_REF_PERP: f64 = 1e-9;

/// Rundungsspielraum (Länge² − 1 bzw. Skalarprodukt), in dem eine Richtung schon Länge 1 hat
/// bzw. eine Bezugsrichtung schon senkrecht zur Achse steht. Solche Werte bleiben bitgleich:
/// Normieren eines schon normierten Vektors ändert sonst die letzten Stellen, und unser eigener
/// Text käme nach Schreiben und Wiederlesen nicht gleich zurück (Aufgabe 9: ein gelesener
/// Körper soll für den Schreiber ein Fixpunkt sein). Eine Richtung der Länge 1 + 1e-7 oder eine
/// um 1e-6 schiefe Bezugsrichtung liegt weit außerhalb und wird normiert bzw. senkrecht gemacht.
const UNIT_SLACK: f64 = 4.0 * f64::EPSILON;

/// Kurve mit umgekehrter Laufrichtung: `point'(t) = point(−t)` bei Gerade, Kreis und Ellipse
/// (Kreis/Ellipse: y und z umgekehrt, das System bleibt rechtshändig).
fn reversed(c: Curve) -> Result<Curve, String> {
    let flip = |f: Frame| Frame {
        origin: f.origin,
        x: f.x,
        y: -f.y,
        z: -f.z,
    };
    Ok(match c {
        Curve::Line { origin, dir } => Curve::Line { origin, dir: -dir },
        Curve::Circle { frame, radius } => Curve::Circle {
            frame: flip(frame),
            radius,
        },
        Curve::Ellipse {
            frame,
            major,
            minor,
        } => Curve::Ellipse {
            frame: flip(frame),
            major,
            minor,
        },
        Curve::Nurbs(n) => {
            Curve::Nurbs(Arc::new(n.reversed().map_err(|e| {
                format!("Eine Freiformkurve lässt sich nicht umkehren: {e}")
            })?))
        }
    })
}

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
    /// Strangfläche aus einer Freiformkurve entlang der Einheitsrichtung `dir`: Der Bereich folgt
    /// aus den Schleifen ([`crate::read::spline::finish_extrusion`]); nie gekippt.
    Extrusion { curve: NurbsCurve, dir: DVec3 },
    /// Kegel aus einer Geraden, die die Drehachse schneidet: Die Hälfte des Doppelkegels folgt
    /// aus den Schleifen ([`crate::read::spline::finish_cone`]).
    LineCone {
        line_origin: DVec3,
        dir: DVec3,
        axis_origin: DVec3,
        axis: DVec3,
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
    path: RefPath,
    /// Zahl der Fehler an der Tiefengrenze; ändert sie sich während einer Auflösung, wird deren
    /// Fehler nicht gespeichert.
    depth_failures: usize,
}

/// Wählt einen Zwischenspeicher des Resolvers.
type Store<T> = for<'a, 'f> fn(&'a mut Resolver<'f>) -> &'a mut HashMap<u64, Result<T, Skip>>;

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
            path: RefPath::with_limit(MAX_RESOLVE_DEPTH),
            depth_failures: 0,
        }
    }

    pub fn units(&self) -> &Units {
        &self.units
    }

    pub fn file(&self) -> &'f StepFile {
        self.file
    }

    /// Zahl der gespeicherten Ergebnisse (alle Arten; zur Prüfung des Zwischenspeichers).
    pub fn cached_count(&self) -> usize {
        self.points.len()
            + self.directions.len()
            + self.placements.len()
            + self.curves.len()
            + self.surfaces.len()
    }

    /// Ergebnis aus dem Zwischenspeicher oder neu berechnet (auf dem Weg der Verweise) und
    /// gespeichert, außer es scheiterte an der Tiefengrenze.
    fn cached<T: Clone>(
        &mut self,
        id: u64,
        store: Store<T>,
        now: fn(&mut Self, u64) -> Result<T, Skip>,
    ) -> Result<T, Skip> {
        if let Some(r) = store(self).get(&id) {
            return r.clone();
        }
        let before = self.depth_failures;
        let r = self.guarded(id, |s| now(s, id));
        if r.is_ok() || self.depth_failures == before {
            store(self).insert(id, r.clone());
        }
        r
    }

    /// Grund mit der Zeile von `#id`.
    fn bad(&self, id: u64, msg: impl Into<String>) -> Skip {
        self.file.fail(id, msg).into()
    }

    /// Führt `f` mit `#id` auf dem Weg der Verweise aus: schon auf dem Weg → `Cycle`, mehr als
    /// [`MAX_RESOLVE_DEPTH`] Ebenen → „Die Verweise sind zu tief verschachtelt …“.
    fn guarded<T>(
        &mut self,
        id: u64,
        f: impl FnOnce(&mut Self) -> Result<T, Skip>,
    ) -> Result<T, Skip> {
        if let Err(e) = self.path.enter(self.file, id) {
            if matches!(e, StepReadError::Syntax { .. }) {
                self.depth_failures += 1;
            }
            return Err(e.into());
        }
        let r = f(self);
        self.path.leave();
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
        self.cached(id, |s| &mut s.points, Self::point_now)
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
        self.cached(id, |s| &mut s.directions, Self::direction_now)
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
        // Erst durch den größten Betrag teilen: kein Überlauf bei sehr großen Werten (1e200).
        // Die Werte sind endlich (der Lexer lehnt anderes ab).
        let d = DVec3::new(x, y, z);
        if (d.length_squared() - 1.0).abs() <= UNIT_SLACK {
            return Ok(d);
        }
        let m = d.abs().max_element();
        if m == 0.0 {
            return Err(self.bad(id, "Eine Richtung hat die Länge null."));
        }
        let unit = d / m;
        let len = unit.length();
        if m * len < MIN_DIRECTION {
            return Err(self.bad(id, "Eine Richtung hat die Länge null."));
        }
        Ok(unit / len)
    }

    /// `AXIS2_PLACEMENT_3D` als rechtshändiges, orthonormales Koordinatensystem: `z` = Achse
    /// (`$` → Welt-z), `x` = Bezugsrichtung senkrecht zu `z` gemacht, `y = z × x`. Fehlt die
    /// Bezugsrichtung (`$`), gilt Welt-x (liegt sie auf der Achse: beliebige Senkrechte); eine
    /// angegebene Bezugsrichtung auf der Achse wird abgelehnt. Ein schiefes oder linkshändiges
    /// System kann so nicht entstehen.
    pub fn placement(&mut self, id: u64) -> Result<Frame, Skip> {
        self.cached(id, |s| &mut s.placements, Self::placement_now)
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
        let (r, given) = match self.arg(id, &e, P, 3)? {
            v if v.is_unset() => (DVec3::X, false),
            v => (
                self.direction(v.reference().map_err(|m| self.bad(id, m))?)?,
                true,
            ),
        };
        let x = r - z * r.dot(z);
        let x = if r.dot(z).abs() <= UNIT_SLACK {
            r
        } else if x.length() >= MIN_REF_PERP {
            x.normalize()
        } else if given {
            return Err(self.bad(id, "Die Bezugsrichtung liegt auf der Achse."));
        } else {
            z.any_orthonormal_vector()
        };
        let y = z.cross(x);
        Ok(Frame { origin, x, y, z })
    }

    /// Trägerkurve einer Kante.
    pub fn curve(&mut self, id: u64) -> Result<Curve, Skip> {
        self.cached(id, |s| &mut s.curves, Self::curve_now)
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
        if is_spline(&e, &CURVE_SPLINE) {
            return self.spline_curve(id, &e);
        }
        if e.is("TRIMMED_CURVE") {
            // Die Ecken der Kante begrenzen die Kurve; die Grundkurve genügt, aber in der
            // Laufrichtung der Trimmkurve (`sense_agreement` .F. → umgekehrt).
            const T: &str = "TRIMMED_CURVE";
            let c = self.reference(id, &e, T, 1)?;
            let sense = self
                .arg(id, &e, T, 4)?
                .logical()
                .map_err(|m| self.bad(id, m))?;
            let base = self.curve(c)?;
            return match sense {
                Some(true) => Ok(base),
                Some(false) => reversed(base).map_err(|m| self.bad(id, m)),
                None => Err(self.bad(
                    id,
                    "Die Laufrichtung einer Trimmkurve ist unbestimmt („.U.“).",
                )),
            };
        }
        Err(Skip(format!(
            "Kante vom Typ {} wird noch nicht unterstützt.",
            type_name(&e)
        )))
    }

    /// Profil einer Drehfläche: die Kurve und, bei `TRIMMED_CURVE`, der benutzte Bereich `a < b`
    /// in ihrem Parameter. Die Kurve läuft wie das Profil (`sense_agreement` .F.: umgekehrt, der
    /// Bereich mit ihr gespiegelt). Grenzen als `PARAMETER_VALUE` (Kreis und Ellipse in der
    /// Winkeleinheit, Gerade in der Längeneinheit) oder als Punkt (auf die Kurve projiziert); gibt
    /// es beides, entscheidet `master_representation` (sonst der Parameter).
    // Der negierte Vergleich lehnt absichtlich auch NaN ab.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    fn profile(&mut self, id: u64) -> Result<(Curve, Option<(f64, f64)>), Skip> {
        const T: &str = "TRIMMED_CURVE";
        let e = self.file.entity(id)?;
        if !e.is(T) {
            return Ok((self.curve(id)?, None));
        }
        let base_id = self.reference(id, &e, T, 1)?;
        let base = self.curve(base_id)?;
        let sense = match self
            .arg(id, &e, T, 4)?
            .logical()
            .map_err(|m| self.bad(id, m))?
        {
            Some(s) => s,
            None => {
                return Err(self.bad(
                    id,
                    "Die Laufrichtung einer Trimmkurve ist unbestimmt („.U.“).",
                ));
            }
        };
        let cartesian = matches!(
            self.arg(id, &e, T, 5)
                .ok()
                .and_then(|v| v.enumeration().ok()),
            Some("CARTESIAN")
        );
        let t1 = self.trim(id, &e, 2, &base, cartesian)?;
        let t2 = self.trim(id, &e, 3, &base, cartesian)?;
        // Bereich auf der Grundkurve, von t1 nach t2 in Laufrichtung.
        let (a, b) = match base.period() {
            Some(per) => {
                let span = if sense {
                    (t2 - t1).rem_euclid(per)
                } else {
                    (t1 - t2).rem_euclid(per)
                };
                let span = if span == 0.0 { per } else { span };
                if sense {
                    (t1, t1 + span)
                } else {
                    (t1 - span, t1)
                }
            }
            None => (t1.min(t2), t1.max(t2)),
        };
        if !(b > a) {
            return Err(self.bad(id, "Eine Trimmkurve hat keine Ausdehnung."));
        }
        if sense {
            return Ok((base, Some((a, b))));
        }
        // Umgekehrt: Gerade, Kreis, Ellipse `t' = −t`; Freiformkurve `t' = d0 + d1 − t`.
        let mirror = match &base {
            Curve::Nurbs(n) => {
                let (d0, d1) = n.domain();
                d0 + d1
            }
            _ => 0.0,
        };
        let rev = reversed(base).map_err(|m| self.bad(id, m))?;
        Ok((rev, Some((mirror - b, mirror - a))))
    }

    /// Eine Grenze (`trim_1`/`trim_2`, Parameter `i`) einer Trimmkurve als Parameter der
    /// Grundkurve `base`.
    fn trim(
        &mut self,
        id: u64,
        e: &Entity,
        i: usize,
        base: &Curve,
        cartesian: bool,
    ) -> Result<f64, Skip> {
        let items = self
            .arg(id, e, "TRIMMED_CURVE", i)?
            .list()
            .map_err(|m| self.bad(id, m))?
            .to_vec();
        let mut param = None;
        let mut point = None;
        for v in &items {
            match v {
                Value::Typed(name, _) if name.eq_ignore_ascii_case("PARAMETER_VALUE") => {
                    param = Some(v.measure().map_err(|m| self.bad(id, m))?);
                }
                Value::Ref(r) => point = Some(self.point(*r)?),
                _ => {}
            }
        }
        let from_point = |p: DVec3| base.project(p);
        let from_param = |x: f64| match base {
            Curve::Line { .. } => x * self.units.length.mm(),
            Curve::Circle { .. } | Curve::Ellipse { .. } => x * self.units.angle_rad,
            Curve::Nurbs(_) => x,
        };
        let t = match (param, point) {
            (_, Some(p)) if cartesian => from_point(p),
            (Some(x), _) => from_param(x),
            (None, Some(p)) => from_point(p),
            (None, None) => {
                return Err(self.bad(id, "Eine Trimmkurve hat keine Grenze."));
            }
        };
        if !t.is_finite() {
            return Err(self.bad(id, "Eine Grenze einer Trimmkurve ist keine endliche Zahl."));
        }
        Ok(t)
    }

    /// Richtung eines `VECTOR` (Länge 1; der Betrag zählt für eine Gerade nicht).
    fn vector_direction(&mut self, id: u64) -> Result<DVec3, Skip> {
        self.guarded(id, |s| {
            let e = s.file.entity(id)?;
            let d = s.reference(id, &e, "VECTOR", 1)?;
            let m = s
                .arg(id, &e, "VECTOR", 2)?
                .measure()
                .map_err(|m| s.bad(id, m))?;
            if m < 0.0 {
                return Err(s.bad(id, "Der Betrag eines Vektors ist negativ."));
            }
            s.direction(d)
        })
    }

    /// Trägerfläche einer Fläche.
    pub fn surface(&mut self, id: u64) -> Result<SurfaceGeom, Skip> {
        self.cached(id, |s| &mut s.surfaces, Self::surface_now)
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
        if is_spline(&e, &SURFACE_SPLINE) {
            return self.spline_surface(id, &e);
        }
        if e.is("SURFACE_OF_REVOLUTION") {
            const N: &str = "SURFACE_OF_REVOLUTION";
            let c = self.reference(id, &e, N, 1)?;
            let a = self.reference(id, &e, N, 2)?;
            let (curve, range) = self.profile(c)?;
            let (origin, axis) = self.axis1(a)?;
            return revolution_trimmed(&curve, range, origin, axis, self.units.weld_tol_mm());
        }
        if e.is("SURFACE_OF_LINEAR_EXTRUSION") {
            const N: &str = "SURFACE_OF_LINEAR_EXTRUSION";
            let c = self.reference(id, &e, N, 1)?;
            let v = self.reference(id, &e, N, 2)?;
            let curve = self.curve(c)?;
            let dir = self.extrusion_vector(v)?;
            return extrusion(&curve, dir);
        }
        Err(Skip(format!(
            "Fläche vom Typ {} wird noch nicht unterstützt.",
            type_name(&e)
        )))
    }

    /// `AXIS1_PLACEMENT`: Punkt und Einheitsrichtung (`$` → Welt-z).
    fn axis1(&mut self, id: u64) -> Result<(DVec3, DVec3), Skip> {
        self.guarded(id, |s| {
            const A: &str = "AXIS1_PLACEMENT";
            let e = s.file.entity(id)?;
            let p = s.reference(id, &e, A, 1)?;
            let origin = s.point(p)?;
            let axis = match s.arg(id, &e, A, 2)? {
                v if v.is_unset() => DVec3::Z,
                v => s.direction(v.reference().map_err(|m| s.bad(id, m))?)?,
            };
            Ok((origin, axis))
        })
    }

    /// Richtung des `VECTOR` einer Strangfläche (Betrag größer als null).
    // Der negierte Vergleich lehnt absichtlich auch NaN ab.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    fn extrusion_vector(&mut self, id: u64) -> Result<DVec3, Skip> {
        let e = self.file.entity(id)?;
        let m = self
            .arg(id, &e, "VECTOR", 2)?
            .measure()
            .map_err(|m| self.bad(id, m))?;
        if !(m > 0.0) {
            return Err(self.bad(id, "Eine Strangfläche hat keine Länge (Betrag null)."));
        }
        self.vector_direction(id)
    }

    /// Kontrollpunkte einer Liste von Verweisen (Fehler der Liste mit dem Vorsatz `what`).
    fn points_of(&mut self, id: u64, v: &Value, what: &str) -> Result<Vec<DVec3>, Skip> {
        let refs = v.refs().map_err(|m| self.not_taken(id, what, m))?;
        refs.into_iter().map(|r| self.point(r)).collect()
    }

    /// Grund „{what} lässt sich nicht übernehmen: {m}“ mit der Zeile von `#id`.
    fn not_taken(&self, id: u64, what: &str, m: impl std::fmt::Display) -> Skip {
        self.bad(id, format!("{what} lässt sich nicht übernehmen: {m}"))
    }

    /// Knoten einer Richtung: ausdrücklich (Vielfachheiten, Werte) oder aus der Knotenart.
    fn knots_of(
        &self,
        id: u64,
        what: &str,
        source: &KnotSource,
        lists: (usize, usize),
        degree: usize,
        count: usize,
    ) -> Result<Vec<f64>, Skip> {
        let why = |m: String| self.not_taken(id, what, m);
        match source {
            KnotSource::Explicit(a) => {
                let mults = arg_at(a, lists.0)
                    .and_then(Value::list)
                    .and_then(|l| l.iter().map(Value::int).collect::<Result<Vec<_>, _>>())
                    .map_err(why)?;
                let values = arg_at(a, lists.1).and_then(Value::reals).map_err(why)?;
                expand_knots(&mults, &values).map_err(|Skip(m)| why(m))
            }
            KnotSource::Implicit(kind) => {
                implicit_knots(kind, degree, count).map_err(|Skip(m)| why(m))
            }
        }
    }

    /// B-Spline-Kurve in jeder Form (Aufgabe 5): geklemmt, Gewichte normiert.
    fn spline_curve(&mut self, id: u64, e: &Entity) -> Result<Curve, Skip> {
        const WHAT: &str = "Eine Freiformkurve";
        let head = spline_head(e, &CURVE_SPLINE).map_err(|m| self.not_taken(id, WHAT, m))?;
        let degree = spline_degree(&head.base, 0).map_err(|m| self.not_taken(id, WHAT, m))?;
        let cps = head
            .base
            .get(1)
            .ok_or_else(|| self.not_taken(id, WHAT, "Die Kontrollpunkte fehlen."))?;
        let points = self.points_of(id, cps, WHAT)?;
        let n = points.len();
        let knots = self.knots_of(id, WHAT, &head.knots, (0, 1), degree, n)?;
        let mut w = match &head.weights {
            Some(v) => v.reals().map_err(|m| self.not_taken(id, WHAT, m))?,
            None => vec![1.0; n],
        };
        if w.len() != n {
            return Err(self.not_taken(
                id,
                WHAT,
                "Gewichte und Kontrollpunkte haben verschiedene Anzahl.",
            ));
        }
        normalize_weights(&mut w).map_err(|Skip(m)| self.not_taken(id, WHAT, m))?;
        let hom: Vec<_> = points
            .iter()
            .zip(&w)
            .map(|(p, w)| (*p * *w).extend(*w))
            .collect();
        let (knots, hom) =
            clamp_knots(degree, &knots, &hom).map_err(|Skip(m)| self.not_taken(id, WHAT, m))?;
        let (control, weights): (Vec<DVec3>, Vec<f64>) =
            hom.iter().map(|h| (h.truncate() / h.w, h.w)).unzip();
        let c = NurbsCurve::new(degree, knots, control, weights)
            .map_err(|err| self.not_taken(id, WHAT, err))?;
        Ok(Curve::nurbs(c))
    }

    /// B-Spline-Fläche in jeder Form (Aufgabe 5): `control_points_list[i][j]` mit i in u,
    /// je Spalte in u und je Reihe in v geklemmt, Gewichte normiert, geschlossen nach den
    /// Kontrollreihen; bei einem Pol in u-Richtung u und v getauscht (`flipped`).
    fn spline_surface(&mut self, id: u64, e: &Entity) -> Result<SurfaceGeom, Skip> {
        const WHAT: &str = "Eine Freiformfläche";
        let head = spline_head(e, &SURFACE_SPLINE).map_err(|m| self.not_taken(id, WHAT, m))?;
        let du = spline_degree(&head.base, 0).map_err(|m| self.not_taken(id, WHAT, m))?;
        let dv = spline_degree(&head.base, 1).map_err(|m| self.not_taken(id, WHAT, m))?;
        let rows = head
            .base
            .get(2)
            .ok_or_else(|| "Die Kontrollpunkte fehlen.".to_string())
            .and_then(|v| v.list().map(<[Value]>::to_vec))
            .map_err(|m| self.not_taken(id, WHAT, m))?;
        let mut grid = Vec::with_capacity(rows.len());
        for row in &rows {
            grid.push(self.points_of(id, row, WHAT)?);
        }
        let nu = grid.len();
        let nv = grid.first().map_or(0, Vec::len);
        if nu < 2 || nv < 2 || grid.iter().any(|r| r.len() != nv) {
            return Err(self.not_taken(id, WHAT, "Die Kontrollpunkte bilden kein Rechteck."));
        }
        let ku = self.knots_of(id, WHAT, &head.knots, (0, 2), du, nu)?;
        let kv = self.knots_of(id, WHAT, &head.knots, (1, 3), dv, nv)?;
        let mut w = match &head.weights {
            Some(v) => {
                let rows = v.list().map_err(|m| self.not_taken(id, WHAT, m))?;
                let mut flat = Vec::with_capacity(nu * nv);
                for r in rows {
                    let r = r.reals().map_err(|m| self.not_taken(id, WHAT, m))?;
                    if r.len() != nv {
                        flat.clear();
                        break;
                    }
                    flat.extend(r);
                }
                if rows.len() != nu || flat.len() != nu * nv {
                    return Err(self.not_taken(
                        id,
                        WHAT,
                        "Gewichte und Kontrollpunkte haben verschiedene Anzahl.",
                    ));
                }
                flat
            }
            None => vec![1.0; nu * nv],
        };
        normalize_weights(&mut w).map_err(|Skip(m)| self.not_taken(id, WHAT, m))?;
        let hom = |i: usize, j: usize| (grid[i][j] * w[i * nv + j]).extend(w[i * nv + j]);
        // Je Spalte in u klemmen (alle Spalten ergeben dieselben Knoten), dann je Reihe in v.
        let mut columns = Vec::with_capacity(nv);
        let mut ku2 = Vec::new();
        for j in 0..nv {
            let col: Vec<_> = (0..nu).map(|i| hom(i, j)).collect();
            let (k, c) =
                clamp_knots(du, &ku, &col).map_err(|Skip(m)| self.not_taken(id, WHAT, m))?;
            ku2 = k;
            columns.push(c);
        }
        let nu2 = columns.first().map_or(0, Vec::len);
        let mut kv2 = Vec::new();
        let mut net = Vec::with_capacity(nu2 * nv);
        for i in 0..nu2 {
            let row: Vec<_> = columns.iter().map(|c| c[i]).collect();
            let (k, r) =
                clamp_knots(dv, &kv, &row).map_err(|Skip(m)| self.not_taken(id, WHAT, m))?;
            kv2 = k;
            net.push(r);
        }
        let nv2 = net.first().map_or(0, Vec::len);
        let (control, weights): (Vec<DVec3>, Vec<f64>) = net
            .iter()
            .flatten()
            .map(|h| (h.truncate() / h.w, h.w))
            .unzip();
        let ns = NurbsSurface::new(du, dv, ku2, kv2, nu2, nv2, control, weights)
            .map_err(|err| self.not_taken(id, WHAT, err))?;
        let (surface, swapped) = finish_nurbs_surface(ns, self.units.weld_tol_mm())
            .map_err(|Skip(m)| self.not_taken(id, WHAT, m))?;
        Ok(SurfaceGeom::Ready {
            surface,
            flipped: swapped,
        })
    }
}

/// Namen der B-Spline-Formen einer Kurve bzw. Fläche (ISO 10303-42).
struct SplineNames {
    /// Obertyp in zusammengesetzten Einträgen (Parameter ab dem Grad).
    base: &'static str,
    /// Form mit Knotenliste.
    with_knots: &'static str,
    /// Formen ohne Knotenliste und ihre Knotenart.
    forms: [(&'static str, &'static str); 3],
    rational: &'static str,
    /// Zahl der Parameter des Obertyps (ohne Namen).
    base_len: usize,
}

const CURVE_SPLINE: SplineNames = SplineNames {
    base: "B_SPLINE_CURVE",
    with_knots: "B_SPLINE_CURVE_WITH_KNOTS",
    forms: [
        ("UNIFORM_CURVE", "UNIFORM_KNOTS"),
        ("QUASI_UNIFORM_CURVE", "QUASI_UNIFORM_KNOTS"),
        ("BEZIER_CURVE", "PIECEWISE_BEZIER_KNOTS"),
    ],
    rational: "RATIONAL_B_SPLINE_CURVE",
    base_len: 5,
};

const SURFACE_SPLINE: SplineNames = SplineNames {
    base: "B_SPLINE_SURFACE",
    with_knots: "B_SPLINE_SURFACE_WITH_KNOTS",
    forms: [
        ("UNIFORM_SURFACE", "UNIFORM_KNOTS"),
        ("QUASI_UNIFORM_SURFACE", "QUASI_UNIFORM_KNOTS"),
        ("BEZIER_SURFACE", "PIECEWISE_BEZIER_KNOTS"),
    ],
    rational: "RATIONAL_B_SPLINE_SURFACE",
    base_len: 7,
};

fn is_spline(e: &Entity, n: &SplineNames) -> bool {
    e.is(n.base) || e.is(n.with_knots) || n.forms.iter().any(|(f, _)| e.is(f))
}

/// Woher die Knoten kommen.
enum KnotSource {
    /// Parameter der Form mit Knotenliste (Kurve: Vielfachheiten, Werte; Fläche: Vielfachheiten
    /// u, v, Werte u, v).
    Explicit(Vec<Value>),
    /// Knotenart ohne Liste.
    Implicit(&'static str),
}

/// Kopf einer B-Spline: Parameter ab dem Grad, Knotenquelle, Gewichte.
struct SplineHead {
    base: Vec<Value>,
    knots: KnotSource,
    weights: Option<Value>,
}

fn arg_at(a: &[Value], i: usize) -> Result<&Value, String> {
    a.get(i)
        .ok_or_else(|| format!("Der {}. Parameter der Knoten fehlt.", i + 1))
}

fn spline_degree(base: &[Value], i: usize) -> Result<usize, String> {
    let d = base
        .get(i)
        .ok_or_else(|| "Der Grad fehlt.".to_string())?
        .int()?;
    usize::try_from(d)
        .ok()
        .filter(|d| (1..=MAX_DEGREE).contains(d))
        .ok_or_else(|| format!("Der Grad {d} ist nicht erlaubt (1 bis {MAX_DEGREE})."))
}

/// Zerlegt einfache (`B_SPLINE_CURVE_WITH_KNOTS('', …)`, `UNIFORM_CURVE('', …)` …) und
/// zusammengesetzte Einträge (`(B_SPLINE_CURVE(…) B_SPLINE_CURVE_WITH_KNOTS(…)
/// RATIONAL_B_SPLINE_CURVE(…) …)`).
fn spline_head(e: &Entity, n: &SplineNames) -> Result<SplineHead, String> {
    match e {
        Entity::Simple(r) => {
            let args = r.args.get(1..).unwrap_or(&[]);
            let name = r.name.to_ascii_uppercase();
            let knots = if name == n.with_knots {
                KnotSource::Explicit(args.get(n.base_len..).unwrap_or(&[]).to_vec())
            } else if let Some((_, k)) = n.forms.iter().find(|(f, _)| *f == name) {
                KnotSource::Implicit(k)
            } else {
                return Err(format!("Die Form {} wird nicht unterstützt.", r.name));
            };
            Ok(SplineHead {
                base: args.to_vec(),
                knots,
                weights: None,
            })
        }
        Entity::Complex(_) => {
            let base = e
                .part(n.base)
                .ok_or_else(|| format!("Der Teil {} fehlt.", n.base))?;
            let knots = if let Some(a) = e.part(n.with_knots) {
                KnotSource::Explicit(a.to_vec())
            } else if let Some((_, k)) = n.forms.iter().find(|(f, _)| e.is(f)) {
                KnotSource::Implicit(k)
            } else {
                return Err("Die Knoten fehlen.".into());
            };
            Ok(SplineHead {
                base: base.to_vec(),
                knots,
                weights: e.part(n.rational).and_then(|a| a.first()).cloned(),
            })
        }
    }
}
