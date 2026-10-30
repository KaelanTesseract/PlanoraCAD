//! Skizze: Punkte, Kurven, Zahlenvektor. Abhängigkeiten stehen in `constraint.rs`.

use crate::error::SketchError;
use crate::solver::SolveCache;
use planora_i18n::tr;
use planora_kernel::{Arena, Id};
use planora_math::{DVec2, Frame, Tolerance};
use std::sync::atomic::{AtomicU64, Ordering};

/// Punkt: zwei Einträge im Variablenvektor.
#[derive(Clone, Debug)]
pub struct PointData {
    pub x: usize,
    pub y: usize,
    pub construction: bool,
    pub(crate) alive: bool,
}
pub type PointId = Id<PointData>;

#[derive(Clone, Debug, PartialEq)]
pub enum CurveKind {
    Line {
        a: PointId,
        b: PointId,
    },
    Circle {
        center: PointId,
        radius: usize,
    },
    /// Gegen den Uhrzeigersinn von `start` nach `end`; Radius = |start − center|.
    Arc {
        center: PointId,
        start: PointId,
        end: PointId,
    },
    /// Große Halbachse als Vektor (ax, ay), kleine Halbachse `minor`.
    Ellipse {
        center: PointId,
        ax: usize,
        ay: usize,
        minor: usize,
    },
    /// Natürlicher kubischer Spline durch die Stützpunkte.
    Spline {
        points: Vec<PointId>,
    },
}

impl CurveKind {
    /// Alle Punkte, die die Kurve benutzt.
    pub fn points(&self) -> Vec<PointId> {
        match self {
            CurveKind::Line { a, b } => vec![*a, *b],
            CurveKind::Circle { center, .. } | CurveKind::Ellipse { center, .. } => vec![*center],
            CurveKind::Arc { center, start, end } => vec![*center, *start, *end],
            CurveKind::Spline { points } => points.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CurveData {
    pub kind: CurveKind,
    pub construction: bool,
    pub(crate) alive: bool,
}
pub type CurveId = Id<CurveData>;

/// Eine Skizze auf einer Ebene. Alle Zahlenwerte liegen in `vars`.
#[derive(Clone, Debug)]
pub struct Sketch {
    pub name: String,
    frame: Frame,
    tol: Tolerance,
    pub(crate) vars: Vec<f64>,
    pub(crate) points: Arena<PointData>,
    pub(crate) curves: Arena<CurveData>,
    pub(crate) constraints: Arena<crate::constraint::ConstraintData>,
    /// Stand der Skizze, siehe [`Sketch::revision`].
    revision: u64,
    /// Ergebnisse der letzten Solver-Läufe je Gruppe (siehe `solver.rs`). Jede Änderung an
    /// Objekten oder Abhängigkeiten leert ihn; reine Wertänderungen erkennt der Solver selbst.
    pub(crate) solve_cache: SolveCache,
}

/// Quelle eindeutiger Stände: Jede Änderung irgendeiner Skizze bekommt eine neue Nummer, so
/// verwechselt ein Zwischenspeicher auch zwei verschiedene Skizzen nicht.
static NEXT_REVISION: AtomicU64 = AtomicU64::new(1);

fn next_revision() -> u64 {
    NEXT_REVISION.fetch_add(1, Ordering::Relaxed)
}

/// Größter erlaubter Betrag von Koordinaten, Längen und Maßen in mm (1000 km). Bei
/// `f64` liegt der Abstand benachbarter Zahlen dort bei etwa 1e9 · 2⁻⁵² ≈ 2e-7 mm, also noch
/// unter der Standardtoleranz; größere Werte würden Toleranzvergleiche bedeutungslos machen.
pub const MAX_COORD: f64 = 1e9;

/// Höchstzahl der Stützpunkte eines Splines.
pub const MAX_SPLINE_POINTS: usize = 64;

/// Endlich und betragsmäßig höchstens [`MAX_COORD`].
pub(crate) fn finite(v: f64, what: String) -> Result<(), SketchError> {
    if !v.is_finite() {
        Err(SketchError::InvalidInput(tr!(
            "sketch-err-not-finite",
            what = what
        )))
    } else if v.abs() > MAX_COORD {
        Err(SketchError::InvalidInput(tr!(
            "sketch-err-too-large",
            what = what,
            max = format!("{MAX_COORD:e}")
        )))
    } else {
        Ok(())
    }
}

impl Sketch {
    pub fn new(frame: Frame, name: impl Into<String>, tol: Tolerance) -> Sketch {
        Sketch {
            name: name.into(),
            frame,
            tol,
            vars: Vec::new(),
            points: Arena::new(),
            curves: Arena::new(),
            constraints: Arena::new(),
            revision: next_revision(),
            solve_cache: SolveCache::default(),
        }
    }

    /// Stand der Skizze: ändert sich bei jeder Änderung an Objekten, Abhängigkeiten oder
    /// Werten (auch durch den Solver). Die Nummern sind über alle Skizzen eindeutig; gleicher
    /// Stand heißt also gleicher Inhalt (Kopien mit `clone` teilen den Stand ihres Originals).
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Objekte oder Abhängigkeiten haben sich geändert: neuer Stand, Solver-Zwischenspeicher
    /// verworfen.
    pub(crate) fn touch_structure(&mut self) {
        self.revision = next_revision();
        self.solve_cache = SolveCache::default();
    }

    /// Nur Werte im Variablenvektor haben sich geändert: neuer Stand.
    pub(crate) fn touch_values(&mut self) {
        self.revision = next_revision();
    }

    pub fn frame(&self) -> &Frame {
        &self.frame
    }

    /// Ersetzt die Skizzenebene; die 2D-Werte bleiben unverändert. Bei einem ungültigen
    /// Koordinatensystem bleibt die Skizze unverändert.
    pub fn set_frame(&mut self, frame: Frame) -> Result<(), SketchError> {
        if !frame.is_orthonormal(self.tol.linear) {
            return Err(SketchError::InvalidInput(tr!("sketch-err-data-frame")));
        }
        self.frame = frame;
        self.touch_structure();
        Ok(())
    }

    pub fn tolerance(&self) -> &Tolerance {
        &self.tol
    }

    pub fn variable_count(&self) -> usize {
        self.vars.len()
    }

    pub(crate) fn new_var(&mut self, v: f64) -> usize {
        self.touch_structure();
        self.vars.push(v);
        self.vars.len() - 1
    }

    pub(crate) fn check_point(&self, p: PointId) -> Result<(), SketchError> {
        if p.index() < self.points.len() && self.points[p].alive {
            Ok(())
        } else {
            Err(SketchError::UnknownObject)
        }
    }

    pub(crate) fn check_curve(&self, c: CurveId) -> Result<(), SketchError> {
        if c.index() < self.curves.len() && self.curves[c].alive {
            Ok(())
        } else {
            Err(SketchError::UnknownObject)
        }
    }

    /// Position eines (geprüften) Punktes.
    pub(crate) fn pos(&self, p: PointId) -> DVec2 {
        let d = &self.points[p];
        DVec2::new(self.vars[d.x], self.vars[d.y])
    }

    pub fn add_point(&mut self, x: f64, y: f64) -> Result<PointId, SketchError> {
        finite(x, tr!("sketch-err-what-x"))?;
        finite(y, tr!("sketch-err-what-y"))?;
        let (xi, yi) = (self.new_var(x), self.new_var(y));
        Ok(self.points.push(PointData {
            x: xi,
            y: yi,
            construction: false,
            alive: true,
        }))
    }

    pub fn point_xy(&self, p: PointId) -> Result<DVec2, SketchError> {
        self.check_point(p)?;
        Ok(self.pos(p))
    }

    pub fn set_point_xy(&mut self, p: PointId, x: f64, y: f64) -> Result<(), SketchError> {
        self.check_point(p)?;
        finite(x, tr!("sketch-err-what-x"))?;
        finite(y, tr!("sketch-err-what-y"))?;
        let (xi, yi) = (self.points[p].x, self.points[p].y);
        self.vars[xi] = x;
        self.vars[yi] = y;
        self.touch_values();
        Ok(())
    }

    fn push_curve(&mut self, kind: CurveKind) -> CurveId {
        self.touch_structure();
        self.curves.push(CurveData {
            kind,
            construction: false,
            alive: true,
        })
    }

    pub fn add_line(&mut self, a: PointId, b: PointId) -> Result<CurveId, SketchError> {
        self.check_point(a)?;
        self.check_point(b)?;
        if a == b || self.pos(a).distance(self.pos(b)) <= self.tol.linear {
            return Err(SketchError::Degenerate(tr!("sketch-err-line-zero")));
        }
        Ok(self.push_curve(CurveKind::Line { a, b }))
    }

    pub fn add_circle(&mut self, center: PointId, radius: f64) -> Result<CurveId, SketchError> {
        self.check_point(center)?;
        finite(radius, tr!("sketch-err-what-radius"))?;
        if radius <= self.tol.linear {
            return Err(SketchError::Degenerate(tr!("sketch-radius-positive")));
        }
        let r = self.new_var(radius);
        Ok(self.push_curve(CurveKind::Circle { center, radius: r }))
    }

    /// Bogen gegen den Uhrzeigersinn von `start` nach `end` um `center`. Der Radius ist
    /// `|start − center|`; liegt `end` nicht auf diesem Kreis, zieht der Solver ihn dorthin.
    pub fn add_arc(
        &mut self,
        center: PointId,
        start: PointId,
        end: PointId,
    ) -> Result<CurveId, SketchError> {
        self.check_point(center)?;
        self.check_point(start)?;
        self.check_point(end)?;
        let c = self.pos(center);
        if self.pos(start).distance(c) <= self.tol.linear
            || self.pos(end).distance(c) <= self.tol.linear
        {
            return Err(SketchError::Degenerate(tr!("sketch-err-arc-center")));
        }
        let id = self.push_curve(CurveKind::Arc { center, start, end });
        self.push_internal(crate::constraint::ConstraintKind::ArcRadius(id));
        Ok(id)
    }

    /// Ellipse um `center` mit großer Halbachse `major` (Vektor) und kleiner Halbachse `minor`
    /// (links der großen Halbachse, siehe [`Sketch::ellipse_point`]).
    pub fn add_ellipse(
        &mut self,
        center: PointId,
        major: DVec2,
        minor: f64,
    ) -> Result<CurveId, SketchError> {
        self.check_point(center)?;
        finite(major.x, tr!("sketch-err-what-major"))?;
        finite(major.y, tr!("sketch-err-what-major"))?;
        finite(minor, tr!("sketch-err-what-minor"))?;
        let len = major.length();
        if len <= self.tol.linear {
            return Err(SketchError::Degenerate(tr!("sketch-major-axis-positive")));
        }
        if minor <= self.tol.linear || minor > len {
            return Err(SketchError::Degenerate(tr!("sketch-err-minor-range")));
        }
        let (ax, ay, mi) = (
            self.new_var(major.x),
            self.new_var(major.y),
            self.new_var(minor),
        );
        Ok(self.push_curve(CurveKind::Ellipse {
            center,
            ax,
            ay,
            minor: mi,
        }))
    }

    /// Natürlicher kubischer Spline durch 2 bis [`MAX_SPLINE_POINTS`] verschiedene Punkte;
    /// Stützpunkt `i` liegt beim Parameter `t = i`.
    pub fn add_spline(&mut self, points: &[PointId]) -> Result<CurveId, SketchError> {
        if !(2..=MAX_SPLINE_POINTS).contains(&points.len()) {
            return Err(SketchError::InvalidInput(tr!(
                "sketch-err-spline-count",
                max = MAX_SPLINE_POINTS.to_string()
            )));
        }
        for (i, &p) in points.iter().enumerate() {
            self.check_point(p)?;
            if points[..i].contains(&p) {
                return Err(SketchError::InvalidInput(tr!("sketch-err-point-twice")));
            }
        }
        if points
            .windows(2)
            .any(|w| self.pos(w[0]).distance(self.pos(w[1])) <= self.tol.linear)
        {
            return Err(SketchError::Degenerate(tr!("sketch-err-spline-neighbours")));
        }
        Ok(self.push_curve(CurveKind::Spline {
            points: points.to_vec(),
        }))
    }

    /// Punkt der Ellipse beim Parameter `t`: `center + a·cos t + b̂·minor·sin t` mit der großen
    /// Halbachse `a = (ax, ay)` und `b̂ = (−ay, ax)/|a|`. `None` für andere Kurven.
    pub fn ellipse_point(&self, c: CurveId, t: f64) -> Option<DVec2> {
        let CurveKind::Ellipse {
            center,
            ax,
            ay,
            minor,
        } = self.curve(c)?.kind
        else {
            return None;
        };
        let a = DVec2::new(self.vars[ax], self.vars[ay]);
        if !t.is_finite() || a.length() == 0.0 {
            return None;
        }
        Some(
            crate::curves::Sampled::ellipse(self.pos(center), a, self.vars[minor])
                .eval(t)
                .0,
        )
    }

    /// Punkt des Splines beim Parameter `t` (auf `[0, n − 1]` geklemmt). `None` für andere
    /// Kurven oder nicht endliches `t`.
    pub fn spline_point(&self, c: CurveId, t: f64) -> Option<DVec2> {
        let CurveKind::Spline { points } = &self.curve(c)?.kind else {
            return None;
        };
        if !t.is_finite() {
            return None;
        }
        let xy: Vec<DVec2> = points.iter().map(|&p| self.pos(p)).collect();
        let t = t.clamp(0.0, (xy.len() - 1) as f64);
        Some(crate::curves::Sampled::spline(&xy).eval(t).0)
    }

    pub fn curve(&self, c: CurveId) -> Option<&CurveData> {
        self.check_curve(c).ok().map(|()| &self.curves[c])
    }

    pub fn radius(&self, c: CurveId) -> Option<f64> {
        match self.curve(c)?.kind {
            CurveKind::Circle { radius, .. } => Some(self.vars[radius]),
            CurveKind::Arc { center, start, .. } => {
                Some(self.pos(start).distance(self.pos(center)))
            }
            _ => None,
        }
    }

    pub fn line_length(&self, c: CurveId) -> Option<f64> {
        match self.curve(c)?.kind {
            CurveKind::Line { a, b } => Some(self.pos(a).distance(self.pos(b))),
            _ => None,
        }
    }

    pub fn set_construction(&mut self, c: CurveId, construction: bool) -> Result<(), SketchError> {
        self.check_curve(c)?;
        self.curves[c].construction = construction;
        self.touch_structure();
        Ok(())
    }

    /// Alle lebenden Punkte.
    pub fn points(&self) -> impl Iterator<Item = (PointId, &PointData)> {
        self.points.iter().filter(|(_, p)| p.alive)
    }

    /// Alle lebenden Kurven.
    pub fn curves(&self) -> impl Iterator<Item = (CurveId, &CurveData)> {
        self.curves.iter().filter(|(_, c)| c.alive)
    }
}
