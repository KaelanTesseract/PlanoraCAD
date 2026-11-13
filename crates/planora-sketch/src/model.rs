//! Skizze: Punkte, Kurven, Zahlenvektor. Abhängigkeiten stehen in `constraint.rs`.

use crate::curves::{conic_apex_ok, conic_nurbs, conic_rho_ok, control_spline_layout};
use crate::error::SketchError;
use crate::solver::SolveCache;
use planora_i18n::tr;
use planora_kernel::{Arena, Id};
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use std::sync::Arc;
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

/// Krümmung an einem Tangentengriff (Skizzen 7, B).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HandleCurvature {
    /// Die Krümmung ergibt sich aus der Kurve.
    Form,
    /// Krümmung 0 (entsteht nur durch Glatt an einer Linie).
    Straight,
    /// Krümmungsgriff: ein Konstruktionsbogen um `center` zwischen `at` und `other`.
    Arc {
        arc: CurveId,
        center: PointId,
        other: PointId,
    },
}

/// Griff an einem Stützpunkt eines Splines: Die Konstruktionslinie `line` läuft von `at` nach `tip`,
/// die Ableitung der Kurve am Stützpunkt ist `3·(tip − at)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplineHandle {
    pub at: PointId,
    pub line: CurveId,
    pub tip: PointId,
    pub curvature: HandleCurvature,
}

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
    /// Kubischer Spline durch die Stützpunkte; natürlich, solange `handles` leer ist (dann derselbe
    /// Codeweg wie vor den Griffen), sonst nach `spline_handles.rs`.
    Spline {
        points: Vec<PointId>,
        handles: Vec<SplineHandle>,
    },
    /// Gegen den Uhrzeigersinn von `start` nach `end` auf der Ellipse
    /// `center + a·cos t + b̂·minor·sin t` (große Halbachse `a = (ax, ay)`, siehe
    /// [`Sketch::ellipse_point`]). Die interne Abhängigkeit `EllipticArcEnds` hält beide Enden
    /// auf der Ellipse.
    EllipticArc {
        center: PointId,
        ax: usize,
        ay: usize,
        minor: usize,
        start: PointId,
        end: PointId,
    },
    /// Nicht rationale B-Spline-Kurve über echte Skizzenpunkte als Kontrollpunkte (2 bis
    /// [`MAX_SPLINE_POINTS`]); erster und letzter liegen auf der Kurve. `degree` ist der wirksame
    /// Grad, `knots` der geklemmte Knotenvektor; er steht in der Kurve, weil Stutzen und Einfügen
    /// ihn ungleichmäßig machen.
    ControlSpline {
        points: Vec<PointId>,
        degree: u8,
        knots: Vec<f64>,
    },
    /// Rationale quadratische Bézierkurve von `start` nach `end` mit dem Scheitel `apex` (Schnitt
    /// der Endtangenten), Gewicht `w = rho / (1 − rho)`, Parameter `t ∈ [0, 1]`. Der Schulterpunkt
    /// `C(½)` liegt bei `M + rho·(apex − M)`, `M` = Sehnenmitte. `rho` ist eine Zahl in der Kurve,
    /// keine Löservariable: Die Kurve hat die 6 Freiheitsgrade ihrer drei Punkte.
    Conic {
        start: PointId,
        end: PointId,
        apex: PointId,
        rho: f64,
    },
    /// Projizierte Freiformkurve (Spec A3, F): unveränderlich in Skizzenkoordinaten (`z = 0`), auch
    /// rational; `start` und `end` tragen `Fixed`. Nur die Projektion legt sie an. Bei einer
    /// geschlossenen Kante ist `start == end`.
    FixedSpline {
        start: PointId,
        end: PointId,
        curve: Arc<NurbsCurve>,
    },
}

impl CurveKind {
    /// Alle Punkte, die die Kurve benutzt.
    pub fn points(&self) -> Vec<PointId> {
        match self {
            CurveKind::Line { a, b } => vec![*a, *b],
            CurveKind::Circle { center, .. } | CurveKind::Ellipse { center, .. } => vec![*center],
            CurveKind::Arc { center, start, end } => vec![*center, *start, *end],
            CurveKind::Spline { points, handles } => {
                // Die Griffe gehören zur Punktgruppe: So nimmt der Löser sie in dieselbe Gruppe.
                let mut out = points.clone();
                for h in handles {
                    out.push(h.tip);
                    if let HandleCurvature::Arc { center, other, .. } = h.curvature {
                        out.extend([center, other]);
                    }
                }
                out
            }
            CurveKind::ControlSpline { points, .. } => points.clone(),
            CurveKind::EllipticArc {
                center, start, end, ..
            } => vec![*center, *start, *end],
            CurveKind::Conic {
                start, end, apex, ..
            } => vec![*start, *apex, *end],
            CurveKind::FixedSpline { start, end, .. } if start == end => vec![*start],
            CurveKind::FixedSpline { start, end, .. } => vec![*start, *end],
        }
    }

    /// Die Stützpunkte eines Splines bzw. die Kontrollpunkte eines Splines mit Kontrollpunkten, ohne
    /// Griffe; leer bei allen anderen Arten.
    pub fn fit_points(&self) -> &[PointId] {
        match self {
            CurveKind::Spline { points, .. } | CurveKind::ControlSpline { points, .. } => points,
            _ => &[],
        }
    }
}

/// Linienstil: normal, Konstruktion oder Mittellinie (Strichpunkt).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineStyle {
    Normal,
    Construction,
    Centerline,
}

#[derive(Clone, Debug)]
pub struct CurveData {
    pub kind: CurveKind,
    pub construction: bool,
    /// Mittellinie: nur an Linien, setzt `construction` voraus.
    pub centerline: bool,
    /// Kontrollpunktrahmen (Skizzen 9): Die Ansicht zeichnet das Kontrollpolygon dieser Kurve
    /// immer. Vorgabe `false`; gesetzt beim Einfügen mit „Kontrollpunktrahmen“.
    pub show_frame: bool,
    pub(crate) alive: bool,
}

impl CurveData {
    pub fn style(&self) -> LineStyle {
        if self.centerline {
            LineStyle::Centerline
        } else if self.construction {
            LineStyle::Construction
        } else {
            LineStyle::Normal
        }
    }
}
pub type CurveId = Id<CurveData>;

/// Eine Skizze auf einer Ebene. Alle Zahlenwerte liegen in `vars`.
#[derive(Clone, Debug)]
pub struct Sketch {
    pub name: String,
    frame: Frame,
    pub(crate) tol: Tolerance,
    pub(crate) vars: Vec<f64>,
    pub(crate) points: Arena<PointData>,
    pub(crate) curves: Arena<CurveData>,
    pub(crate) constraints: Arena<crate::constraint::ConstraintData>,
    /// Muster (auch gelöschte), siehe `pattern.rs`.
    pub(crate) patterns: Arena<crate::pattern::PatternData>,
    /// Texte (auch gelöschte), siehe `text.rs` (Skizzen 8).
    pub(crate) texts: Arena<crate::text::TextData>,
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

/// Beide Koordinaten endlich und betragsmäßig höchstens [`MAX_COORD`].
pub(crate) fn within_coord_range(p: DVec2) -> bool {
    p.is_finite() && p.x.abs() <= MAX_COORD && p.y.abs() <= MAX_COORD
}

/// `Ok`, wenn `rho` als Rho eines Kegelschnitts taugt (siehe [`conic_rho_ok`]).
pub(crate) fn check_conic_rho(rho: f64) -> Result<(), SketchError> {
    if conic_rho_ok(rho) {
        Ok(())
    } else {
        Err(SketchError::InvalidInput(tr!("sketch-err-conic-rho")))
    }
}

/// Endlich und betragsmäßig höchstens [`MAX_COORD`].
pub(crate) fn finite(v: f64, what: impl FnOnce() -> String) -> Result<(), SketchError> {
    if !v.is_finite() {
        Err(SketchError::InvalidInput(tr!(
            "sketch-err-not-finite",
            what = what()
        )))
    } else if v.abs() > MAX_COORD {
        Err(SketchError::InvalidInput(tr!(
            "sketch-err-too-large",
            what = what(),
            max = planora_i18n::len_arg(MAX_COORD, format!("{MAX_COORD:e}")),
            unit = planora_i18n::unit_symbol()
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
            patterns: Arena::new(),
            texts: Arena::new(),
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
        finite(x, || tr!("sketch-err-what-x"))?;
        finite(y, || tr!("sketch-err-what-y"))?;
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

    /// Setzt die Lage des Punktes `p`. Einen erzeugten Punkt eines Textes hält der Text; er wird
    /// abgelehnt (`sketch-text-no-edit`).
    pub fn set_point_xy(&mut self, p: PointId, x: f64, y: f64) -> Result<(), SketchError> {
        self.check_point(p)?;
        self.refuse_glyph_point(p)?;
        finite(x, || tr!("sketch-err-what-x"))?;
        finite(y, || tr!("sketch-err-what-y"))?;
        let (xi, yi) = (self.points[p].x, self.points[p].y);
        self.vars[xi] = x;
        self.vars[yi] = y;
        self.touch_values();
        Ok(())
    }

    pub(crate) fn push_curve(&mut self, kind: CurveKind) -> CurveId {
        self.touch_structure();
        self.curves.push(CurveData {
            kind,
            construction: false,
            centerline: false,
            show_frame: false,
            alive: true,
        })
    }

    /// Ersetzt die Art einer lebenden Kurve (Stutzen, Dehnen, Brechen). Prüft nichts; interne
    /// Abhängigkeiten der neuen Art legt der Aufrufer an. Ist die Kurve Vorlage oder Exemplar
    /// eines Musters, wird das Muster aufgelöst.
    pub(crate) fn set_curve_kind(&mut self, c: CurveId, kind: CurveKind) {
        self.dissolve_patterns_at(&[c]);
        self.touch_structure();
        self.curves[c].kind = kind;
        self.refresh_shared_signs(Some(c));
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
        finite(radius, || tr!("sketch-err-what-radius"))?;
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
        self.with_internal_checked(|s| {
            let id = s.push_curve(CurveKind::Arc { center, start, end });
            let k = s.push_internal(crate::constraint::ConstraintKind::ArcRadius(id));
            (id, k)
        })
    }

    /// Legt eine Kurve mit ihrer internen Abhängigkeit an (`make` liefert beide Ids). Mit Mustern
    /// wird abgelehnt, wenn die interne Abhängigkeit Vorlage und eigenes Exemplar verbände (etwa
    /// ein Bogen um einen Vorlagenpunkt mit Enden auf einem Exemplar, wie der Fang sie anbietet):
    /// Die Exemplare folgten der Vorlage, die Vorlage wieder dem Bogen, und der Löser liefe nie
    /// zusammen. Dann bleibt die Skizze unverändert.
    fn with_internal_checked(
        &mut self,
        make: impl FnOnce(&mut Sketch) -> (CurveId, crate::constraint::ConstraintId),
    ) -> Result<CurveId, SketchError> {
        if self.patterns().next().is_none() {
            return Ok(make(self).0);
        }
        self.atomically(|s| {
            let (id, k) = make(s);
            if s.check_activation_cycle(k).is_err() {
                return Err(SketchError::InvalidInput(tr!("sketch-pattern-cycle-arc")));
            }
            Ok(id)
        })
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
        self.check_ellipse_axes(major, minor)?;
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

    /// Prüft die Halbachsen einer neuen Ellipse: endlich, große Halbachse länger als
    /// `tol.linear`, kleine Halbachse in `(tol.linear, |major|]`.
    fn check_ellipse_axes(&self, major: DVec2, minor: f64) -> Result<(), SketchError> {
        finite(major.x, || tr!("sketch-err-what-major"))?;
        finite(major.y, || tr!("sketch-err-what-major"))?;
        finite(minor, || tr!("sketch-err-what-minor"))?;
        let len = major.length();
        if len <= self.tol.linear {
            return Err(SketchError::Degenerate(tr!("sketch-major-axis-positive")));
        }
        if minor <= self.tol.linear || minor > len {
            return Err(SketchError::Degenerate(tr!("sketch-err-minor-range")));
        }
        Ok(())
    }

    /// Ellipsenbogen um `center` mit großer Halbachse `major` (Vektor) und kleiner Halbachse
    /// `minor` (Prüfungen wie [`Sketch::add_ellipse`]), gegen den Uhrzeigersinn von `start` nach
    /// `end`. Liegen die Enden nicht auf der Ellipse, zieht der Solver sie dorthin (interne
    /// Abhängigkeit `EllipticArcEnds`).
    pub fn add_elliptic_arc(
        &mut self,
        center: PointId,
        major: DVec2,
        minor: f64,
        start: PointId,
        end: PointId,
    ) -> Result<CurveId, SketchError> {
        self.check_elliptic_arc(center, major, minor, start, end)?;
        self.with_internal_checked(|s| {
            let (ax, ay, mi) = (s.new_var(major.x), s.new_var(major.y), s.new_var(minor));
            s.add_elliptic_arc_vars(center, ax, ay, mi, start, end)
        })
    }

    /// Prüfungen von [`Sketch::add_elliptic_arc`]: lebende Punkte, Halbachsen wie bei
    /// [`Sketch::add_ellipse`], beide Enden nicht in der Mitte.
    pub(crate) fn check_elliptic_arc(
        &self,
        center: PointId,
        major: DVec2,
        minor: f64,
        start: PointId,
        end: PointId,
    ) -> Result<(), SketchError> {
        self.check_point(center)?;
        self.check_point(start)?;
        self.check_point(end)?;
        self.check_ellipse_axes(major, minor)?;
        let c = self.pos(center);
        if self.pos(start).distance(c) <= self.tol.linear
            || self.pos(end).distance(c) <= self.tol.linear
        {
            return Err(SketchError::Degenerate(tr!("sketch-err-arc-center")));
        }
        Ok(())
    }

    /// Ellipsenbogen auf vorhandenen Variablen (etwa geteilt mit der Ellipse, aus der er
    /// entsteht); legt die interne Abhängigkeit `EllipticArcEnds` an und liefert beide Ids. Prüft nichts.
    pub(crate) fn add_elliptic_arc_vars(
        &mut self,
        center: PointId,
        ax: usize,
        ay: usize,
        minor: usize,
        start: PointId,
        end: PointId,
    ) -> (CurveId, crate::constraint::ConstraintId) {
        let id = self.push_curve(CurveKind::EllipticArc {
            center,
            ax,
            ay,
            minor,
            start,
            end,
        });
        let k = self.push_internal(crate::constraint::ConstraintKind::EllipticArcEnds(id));
        (id, k)
    }

    /// Natürlicher kubischer Spline durch 2 bis [`MAX_SPLINE_POINTS`] verschiedene Punkte;
    /// Stützpunkt `i` liegt beim Parameter `t = i`.
    pub fn add_spline(&mut self, points: &[PointId]) -> Result<CurveId, SketchError> {
        self.check_spline_points(points)?;
        Ok(self.push_curve(CurveKind::Spline {
            points: points.to_vec(),
            handles: Vec::new(),
        }))
    }

    /// Spline mit den Kontrollpunkten `points` (2 bis [`MAX_SPLINE_POINTS`] verschiedene Punkte)
    /// und gleichmäßigem geklemmtem Knotenvektor. `degree` ist 3 oder 5; wirksam ist der kleinere
    /// Wert aus `degree` und `n − 1` (**Annahme**, deckt den Foreneintrag zu Fusion).
    pub fn add_control_spline(
        &mut self,
        points: &[PointId],
        degree: u8,
    ) -> Result<CurveId, SketchError> {
        if !matches!(degree, 3 | 5) {
            return Err(SketchError::InvalidInput(tr!("sketch-err-spline-degree")));
        }
        self.check_spline_points(points)?;
        let (degree, knots) = control_spline_layout(degree, points.len());
        Ok(self.push_curve(CurveKind::ControlSpline {
            points: points.to_vec(),
            degree,
            knots,
        }))
    }

    /// Spline mit Kontrollpunkten aus vorhandenen Daten (Teilstück, Kopie): Grad und Knoten werden
    /// übernommen. Anders als beim Zeichnen dürfen benachbarte Kontrollpunkte aufeinanderliegen, die
    /// Kurve bleibt gültig; das Lösen kann das ebenso erzeugen.
    pub(crate) fn add_control_spline_as(
        &mut self,
        points: &[PointId],
        degree: u8,
        knots: Vec<f64>,
    ) -> Result<CurveId, SketchError> {
        self.check_spline_ids(points)?;
        Ok(self.push_curve(CurveKind::ControlSpline {
            points: points.to_vec(),
            degree,
            knots,
        }))
    }

    /// Kegelschnitt von `start` nach `end` mit dem Scheitel `apex` (Schnitt der Endtangenten) und
    /// `rho` zwischen etwa 0,000001 und 0,999999, denn das Gewicht muss im Bereich der NURBS liegen
    /// (siehe [`conic_rho_ok`] und [`CurveKind::Conic`]). Der Scheitel darf nicht auf der Geraden
    /// durch Start und Ende liegen.
    pub fn add_conic(
        &mut self,
        start: PointId,
        end: PointId,
        apex: PointId,
        rho: f64,
    ) -> Result<CurveId, SketchError> {
        self.check_conic(start, end, apex, rho)?;
        let (p0, a, p2) = (self.pos(start), self.pos(apex), self.pos(end));
        if !conic_apex_ok(p0, a, p2, self.tol.linear) {
            return Err(SketchError::Degenerate(tr!("sketch-err-conic-apex")));
        }
        self.add_conic_as(start, end, apex, rho)
    }

    /// Kegelschnitt aus vorhandenen Daten (Teilstück, Kopie). Anders als beim Zeichnen darf der
    /// Scheitel auf der Sehne liegen, die Kurve bleibt gültig; das Lösen kann das ebenso erzeugen.
    pub(crate) fn add_conic_as(
        &mut self,
        start: PointId,
        end: PointId,
        apex: PointId,
        rho: f64,
    ) -> Result<CurveId, SketchError> {
        self.check_conic(start, end, apex, rho)?;
        Ok(self.push_curve(CurveKind::Conic {
            start,
            end,
            apex,
            rho,
        }))
    }

    /// Lebende, verschiedene Punkte und ein gültiges Rho.
    fn check_conic(
        &self,
        start: PointId,
        end: PointId,
        apex: PointId,
        rho: f64,
    ) -> Result<(), SketchError> {
        for p in [start, end, apex] {
            self.check_point(p)?;
        }
        if start == end || start == apex || end == apex {
            return Err(SketchError::InvalidInput(tr!("sketch-err-point-twice")));
        }
        check_conic_rho(rho)
    }

    /// Setzt das Rho eines Kegelschnitts. Die Punkte bleiben; Abhängigkeiten an der Kurve folgen
    /// beim nächsten Lösen. Ist `c` Vorlage oder Exemplar eines Musters, wird es aufgelöst. Ein
    /// Buchstabe eines Textes wird abgelehnt (`sketch-text-no-edit`).
    pub fn set_conic_rho(&mut self, c: CurveId, rho: f64) -> Result<(), SketchError> {
        self.check_curve(c)?;
        self.refuse_text_curves(&[c], || tr!("sketch-text-no-edit"))?;
        let mut kind = self.curves[c].kind.clone();
        let CurveKind::Conic { rho: slot, .. } = &mut kind else {
            return Err(SketchError::WrongType(tr!("sketch-err-conic-kind")));
        };
        check_conic_rho(rho)?;
        if *slot != rho {
            *slot = rho;
            self.set_curve_kind(c, kind);
            // Die Parameter neu suchen: Bei einem großen Sprung im Rho läge der alte Parameter weit
            // von der neuen Stelle, und der Löser schösse damit über die Enden hinaus.
            self.reseed_extras(c);
        }
        Ok(())
    }

    /// Punktzahl und lebende Punkte ohne Doppelte, für jeden Spline.
    fn check_spline_ids(&self, points: &[PointId]) -> Result<(), SketchError> {
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
        Ok(())
    }

    /// Prüfungen eines neuen Splines: Punktzahl, lebende Punkte ohne Doppelte, Nachbarn getrennt.
    fn check_spline_points(&self, points: &[PointId]) -> Result<(), SketchError> {
        self.check_spline_ids(points)?;
        if points
            .windows(2)
            .any(|w| self.pos(w[0]).distance(self.pos(w[1])) <= self.tol.linear)
        {
            return Err(SketchError::Degenerate(tr!("sketch-err-spline-neighbours")));
        }
        Ok(())
    }

    /// Die lebende Kurve als NURBS-Kurve in Skizzenkoordinaten (`z = 0`): alle Freiformarten außer
    /// dem Spline durch Stützpunkte. `None` für andere Kurven und außerhalb des Rechenbereichs
    /// liegende Punkte.
    pub fn nurbs_of(&self, c: CurveId) -> Option<Arc<NurbsCurve>> {
        match &self.curve(c)?.kind {
            CurveKind::ControlSpline {
                points,
                degree,
                knots,
            } => {
                let control: Vec<DVec3> = points.iter().map(|&p| self.pos(p).extend(0.0)).collect();
                let weights = vec![1.0; control.len()];
                NurbsCurve::new(usize::from(*degree), knots.clone(), control, weights)
                    .ok()
                    .map(Arc::new)
            }
            CurveKind::Conic {
                start,
                end,
                apex,
                rho,
            } => conic_nurbs(self.pos(*start), self.pos(*apex), self.pos(*end), *rho)
                .ok()
                .map(Arc::new),
            CurveKind::FixedSpline { curve, .. } => Some(curve.clone()),
            _ => None,
        }
    }

    /// Punkt der Ellipse beim Parameter `t`: `center + a·cos t + b̂·minor·sin t` mit der großen
    /// Halbachse `a = (ax, ay)` und `b̂ = (−ay, ax)/|a|`; bei einem Ellipsenbogen der Punkt seiner
    /// ganzen Ellipse. `None` für andere Kurven.
    pub fn ellipse_point(&self, c: CurveId, t: f64) -> Option<DVec2> {
        let (m, a, minor) = self.ellipse_axes(c)?;
        if !t.is_finite() || a.length() == 0.0 {
            return None;
        }
        Some(crate::curves::Sampled::ellipse(m, a, minor).eval(t).0)
    }

    /// Parameter `t` (siehe [`Sketch::ellipse_point`]) des Punktes `p` auf einer Ellipse oder
    /// einem Ellipsenbogen: `atan2` der Achskoordinaten `(x/|a|, y/minor)`, Wert in `(−π, π]`.
    /// Für Punkte neben der Ellipse der Parameter des Strahls von der Mitte in Achskoordinaten.
    /// `None` für andere Kurven, entartete Halbachsen oder `p` in der Mitte.
    pub fn ellipse_param_of(&self, c: CurveId, p: DVec2) -> Option<f64> {
        let (m, a, minor) = self.ellipse_axes(c)?;
        let len = a.length();
        if !(p.is_finite() && len > 0.0 && minor > 0.0) {
            return None;
        }
        let d = p - m;
        let (x, y) = (d.dot(a) / (len * len), a.perp_dot(d) / (len * minor));
        (x != 0.0 || y != 0.0).then(|| y.atan2(x))
    }

    /// Mittelpunkt, große Halbachse (Vektor) und kleine Halbachse einer lebenden Ellipse oder
    /// eines Ellipsenbogens; `None` für andere Kurven.
    pub(crate) fn ellipse_axes(&self, c: CurveId) -> Option<(DVec2, DVec2, f64)> {
        let (CurveKind::Ellipse {
            center,
            ax,
            ay,
            minor,
        }
        | CurveKind::EllipticArc {
            center,
            ax,
            ay,
            minor,
            ..
        }) = self.curve(c)?.kind
        else {
            return None;
        };
        Some((
            self.pos(center),
            DVec2::new(self.vars[ax], self.vars[ay]),
            self.vars[minor],
        ))
    }

    /// Punkt des Splines beim Parameter `t` (auf `[0, n − 1]` geklemmt). `None` für andere
    /// Kurven oder nicht endliches `t`.
    pub fn spline_point(&self, c: CurveId, t: f64) -> Option<DVec2> {
        let CurveKind::Spline { points, handles } = &self.curve(c)?.kind else {
            return None;
        };
        if !t.is_finite() {
            return None;
        }
        let t = t.clamp(0.0, (points.len() - 1) as f64);
        Some(self.spline_sampled(points, handles).eval(t).0)
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

    /// Gehört die Kurve zu einem Muster, gilt der Stil für die Vorlage und alle ihre Exemplare
    /// (Stil wie die Vorlage, auch nach späterem Umschalten). Griffe bleiben Konstruktion. Eine
    /// erzeugte Kurve eines Textes schaltet den ganzen Text um (Skizzen 8, B10), eine Rahmenlinie
    /// bleibt Konstruktion, auch als Vorlage eines Musters, dessen Exemplar umgeschaltet wird.
    pub fn set_construction(&mut self, c: CurveId, construction: bool) -> Result<(), SketchError> {
        self.check_curve(c)?;
        if !construction && self.is_handle_curve(c) {
            return Err(SketchError::WrongType(tr!(
                "sketch-err-handle-construction"
            )));
        }
        let group = self
            .text_style_group(c)
            .unwrap_or_else(|| self.pattern_style_group(c));
        if !construction {
            self.refuse_frame_styles(&group)?;
        }
        for x in group {
            self.curves[x].construction = construction;
            self.curves[x].centerline = false;
        }
        self.touch_structure();
        Ok(())
    }

    /// Schaltet die Mittellinie um (nur Linien): an ⇒ Konstruktion und Mittellinie, aus ⇒
    /// beides aus. Im Muster wie [`Sketch::set_construction`].
    pub fn set_centerline(&mut self, c: CurveId, on: bool) -> Result<(), SketchError> {
        self.check_curve(c)?;
        if !matches!(self.curves[c].kind, CurveKind::Line { .. }) {
            return Err(SketchError::WrongType(tr!("sketch-err-centerline-type")));
        }
        if self.is_handle_curve(c) {
            return Err(SketchError::WrongType(tr!(
                "sketch-err-handle-construction"
            )));
        }
        // Text wird nie Mittellinie (Skizzen 8, B10), eine Rahmenlinie bleibt Konstruktion, auch
        // als Vorlage eines Musters, dessen Exemplar umgeschaltet wird.
        if self.text_of_curve(c).is_some() {
            return Err(SketchError::WrongType(tr!("sketch-text-no-centerline")));
        }
        let group = self.pattern_style_group(c);
        self.refuse_frame_styles(&group)?;
        for x in group {
            self.curves[x].construction = on;
            self.curves[x].centerline = on;
        }
        self.touch_structure();
        Ok(())
    }

    /// Gibt es einen Kurveneintrag mit Mittellinie, auch einen gelöschten? Dann braucht die
    /// Datei Format 14.
    pub fn has_centerline_entry(&self) -> bool {
        self.curves.iter().any(|(_, c)| c.centerline)
    }

    /// Gibt es einen Kurveneintrag mit Kontrollpunktrahmen, auch einen gelöschten? Dann braucht
    /// die Datei Format 19 (Skizzen 9).
    pub fn has_frame_entry(&self) -> bool {
        self.curves.iter().any(|(_, c)| c.show_frame)
    }

    /// Gibt es einen Mustereintrag oder einen Abhängigkeitseintrag, der Format 15 braucht, auch
    /// einen gelöschten (ein aufgelöstes Muster, ein Musterwert)? Dann braucht die Datei Format
    /// 15.
    pub fn needs_format_15(&self) -> bool {
        !self.patterns.is_empty()
            || self
                .constraints
                .iter()
                .any(|(_, c)| c.kind.needs_format_15())
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
