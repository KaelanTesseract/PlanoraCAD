//! Skizze: Punkte, Kurven, Zahlenvektor. Abhängigkeiten stehen in `constraint.rs`.

use crate::error::SketchError;
use planora_kernel::{Arena, Id};
use planora_math::{DVec2, Frame, Tolerance};

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
}

fn finite(v: f64, what: &str) -> Result<(), SketchError> {
    if v.is_finite() {
        Ok(())
    } else {
        Err(SketchError::InvalidInput(format!(
            "{what} muss eine endliche Zahl sein"
        )))
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
        }
    }

    pub fn frame(&self) -> &Frame {
        &self.frame
    }

    pub fn tolerance(&self) -> &Tolerance {
        &self.tol
    }

    pub fn variable_count(&self) -> usize {
        self.vars.len()
    }

    pub(crate) fn new_var(&mut self, v: f64) -> usize {
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
        finite(x, "Die x-Koordinate")?;
        finite(y, "Die y-Koordinate")?;
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
        finite(x, "Die x-Koordinate")?;
        finite(y, "Die y-Koordinate")?;
        let (xi, yi) = (self.points[p].x, self.points[p].y);
        self.vars[xi] = x;
        self.vars[yi] = y;
        Ok(())
    }

    fn push_curve(&mut self, kind: CurveKind) -> CurveId {
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
            return Err(SketchError::Degenerate(
                "Die Linie hätte die Länge null".into(),
            ));
        }
        Ok(self.push_curve(CurveKind::Line { a, b }))
    }

    pub fn add_circle(&mut self, center: PointId, radius: f64) -> Result<CurveId, SketchError> {
        self.check_point(center)?;
        finite(radius, "Der Radius")?;
        if radius <= self.tol.linear {
            return Err(SketchError::Degenerate(
                "Der Radius muss größer als null sein".into(),
            ));
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
            return Err(SketchError::Degenerate(
                "Start und Ende dürfen nicht in der Mitte liegen".into(),
            ));
        }
        let id = self.push_curve(CurveKind::Arc { center, start, end });
        self.push_internal(crate::constraint::ConstraintKind::ArcRadius(id));
        Ok(id)
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
