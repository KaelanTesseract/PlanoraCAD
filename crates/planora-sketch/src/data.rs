//! Rohdaten einer Skizze zum Speichern und Laden. Alle Einträge der Arenen werden in ihrer
//! Reihenfolge übertragen (auch gelöschte), damit Ids nach dem Laden dieselben sind.

use crate::constraint::{ConstraintData, ConstraintId, ConstraintKind};
use crate::error::SketchError;
use crate::model::{
    CurveData, CurveId, CurveKind, HandleCurvature, MAX_COORD, MAX_SPLINE_POINTS, PointData,
    PointId, Sketch, check_conic_rho, finite,
};
use crate::pattern::{
    CircularMode, PatternData, PatternDir, PatternInstance, PatternKind, PatternProblem,
    PatternSlot,
};
use crate::solver::SolveCache;
use crate::spline_handles::handle_parts;
use crate::text::{MAX_SPACING, ShapedCache, TextData, TextPlacement, WalkCache};
use crate::text_shape::{MAX_TEXT_CHARS, MAX_TEXT_CURVES, TextStyle};
use planora_i18n::tr;
use planora_kernel::Arena;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq)]
pub struct PointRecord {
    pub x: usize,
    pub y: usize,
    /// Index der z-Variable eines 3D-Punkts (Skizzen 10), `None` bei einem 2D-Punkt.
    pub z: Option<usize>,
    pub construction: bool,
    pub alive: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CurveRecord {
    pub kind: CurveKind,
    pub construction: bool,
    pub centerline: bool,
    /// Kontrollpunktrahmen (ab Format 19, Skizzen 9): nur an Kurven mit Kontrollpolygon.
    pub show_frame: bool,
    /// Normale einer ebenen Kurve außerhalb der Skizzenebene (Skizzen 10), normiert; `None` in
    /// der Skizzenebene.
    pub normal: Option<DVec3>,
    pub alive: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConstraintRecord {
    pub kind: ConstraintKind,
    pub branch: i8,
    pub driven: bool,
    pub enabled: bool,
    pub internal: bool,
    pub extra: Option<usize>,
    pub alive: bool,
}

/// Ein Muster wie [`PatternData`], mit `alive`: Aufgelöste Muster bleiben in den Rohdaten, damit
/// die Ids der Musterwerte nach dem Laden dieselben sind.
#[derive(Clone, Debug, PartialEq)]
pub struct PatternRecord {
    pub kind: PatternKind,
    pub sources: Vec<CurveId>,
    pub source_points: Vec<PointId>,
    pub values: Vec<ConstraintId>,
    pub instances: Vec<PatternInstance>,
    pub suppressed: Vec<[i32; 2]>,
    pub alive: bool,
}

/// Ein Text wie [`TextData`], mit `alive` und ohne das gesetzte `TextShape` (Skizzen 8): Nach dem
/// Laden ist ein Text „noch nicht gesetzt“, seine gespeicherten Kurven bleiben, bis
/// [`Sketch::reshape_texts`] ihn neu setzt.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRecord {
    pub content: String,
    pub style: TextStyle,
    pub height: f64,
    pub flip_h: bool,
    pub flip_v: bool,
    pub placement: TextPlacement,
    pub curves: Vec<CurveId>,
    pub points: Vec<PointId>,
    pub alive: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SketchData {
    pub name: String,
    pub frame: Frame,
    pub tolerance: Tolerance,
    pub vars: Vec<f64>,
    pub points: Vec<PointRecord>,
    pub curves: Vec<CurveRecord>,
    pub constraints: Vec<ConstraintRecord>,
    /// Muster, auch aufgelöste (Skizzen 5); leer in Skizzen ohne Muster.
    pub patterns: Vec<PatternRecord>,
    /// Texte, auch gelöschte (Skizzen 8); leer in Skizzen ohne Text.
    pub texts: Vec<TextRecord>,
    /// Haken „3D-Skizze“ (Skizzen 10).
    pub three_d: bool,
}

/// Größter Grad eines Splines mit Kontrollpunkten in einer Datei (im Skizzenfeld wählbar: 3, 5).
const MAX_CONTROL_SPLINE_DEGREE: u8 = 5;

fn bad(msg: impl Into<String>) -> SketchError {
    SketchError::InvalidInput(msg.into())
}

impl Sketch {
    /// Arten aller Kurveneinträge in Arena-Reihenfolge, auch gelöschter (wie in
    /// [`Sketch::to_data`]).
    pub fn curve_entries(&self) -> impl Iterator<Item = &CurveKind> {
        self.curves.iter().map(|(_, c)| &c.kind)
    }

    /// Gibt es einen Texteintrag, auch einen gelöschten? Dann braucht die Datei Format 18
    /// (Skizzen 8): Er steht in der Datei.
    pub fn has_text_entry(&self) -> bool {
        !self.texts.is_empty()
    }

    /// Arten aller Abhängigkeitseinträge in Arena-Reihenfolge, auch gelöschter und interner.
    pub fn constraint_entries(&self) -> impl Iterator<Item = &ConstraintKind> {
        self.constraints.iter().map(|(_, c)| &c.kind)
    }

    pub fn to_data(&self) -> SketchData {
        SketchData {
            name: self.name.clone(),
            frame: *self.frame(),
            tolerance: *self.tolerance(),
            vars: self.vars.clone(),
            points: self
                .points
                .iter()
                .map(|(_, p)| PointRecord {
                    x: p.x,
                    y: p.y,
                    z: p.z,
                    construction: p.construction,
                    alive: p.alive,
                })
                .collect(),
            curves: self
                .curves
                .iter()
                .map(|(_, c)| CurveRecord {
                    kind: c.kind.clone(),
                    construction: c.construction,
                    centerline: c.centerline,
                    show_frame: c.show_frame,
                    normal: c.normal,
                    alive: c.alive,
                })
                .collect(),
            constraints: self
                .constraints
                .iter()
                .map(|(_, c)| ConstraintRecord {
                    kind: c.kind.clone(),
                    branch: c.branch,
                    driven: c.driven,
                    enabled: c.enabled,
                    internal: c.internal,
                    extra: c.extra,
                    alive: c.alive,
                })
                .collect(),
            patterns: self
                .patterns
                .iter()
                .map(|(_, m)| PatternRecord {
                    kind: m.kind,
                    sources: m.sources.clone(),
                    source_points: m.source_points.clone(),
                    values: m.values.clone(),
                    instances: m.instances.clone(),
                    suppressed: m.suppressed.clone(),
                    alive: m.alive,
                })
                .collect(),
            texts: self
                .texts
                .iter()
                .map(|(_, t)| TextRecord {
                    content: t.content.clone(),
                    style: t.style.clone(),
                    height: t.height,
                    flip_h: t.flip_h,
                    flip_v: t.flip_v,
                    placement: t.placement,
                    curves: t.curves.clone(),
                    points: t.points.clone(),
                    alive: t.alive,
                })
                .collect(),
            three_d: self.three_d,
        }
    }

    /// Baut eine Skizze aus Rohdaten auf. Prüft alles (Zahlen endlich und im Bereich, Indizes
    /// vorhanden, lebende Objekte verweisen nur auf lebende); löst und rechnet nichts. Muster in
    /// einer Schleife zwischen Vorlage und eigenem Exemplar werden aufgelöst, siehe
    /// [`Sketch::from_data_counted`].
    pub fn from_data(d: SketchData) -> Result<Sketch, SketchError> {
        Sketch::from_data_counted(d).map(|(s, _)| s)
    }

    /// Wie [`Sketch::from_data`]; liefert dazu die Zahl der Muster, die beim Laden aufgelöst
    /// wurden, weil sie in einer Schleife zwischen Vorlage und eigenem Exemplar lagen (für eine
    /// Meldung).
    pub fn from_data_counted(d: SketchData) -> Result<(Sketch, usize), SketchError> {
        if !(d.tolerance.linear.is_finite()
            && d.tolerance.linear > 0.0
            && d.tolerance.angular.is_finite()
            && d.tolerance.angular > 0.0)
        {
            return Err(bad(tr!("sketch-err-data-tolerance")));
        }
        for v in [d.frame.origin, d.frame.x, d.frame.y, d.frame.z] {
            if !v.is_finite() || v.abs().max_element() > MAX_COORD {
                return Err(bad(tr!("sketch-err-data-frame")));
            }
        }
        if !d.frame.is_orthonormal(d.tolerance.linear) {
            return Err(bad(tr!("sketch-err-data-frame")));
        }
        for v in &d.vars {
            finite(*v, || tr!("sketch-err-what-sketch-value"))?;
        }
        let nv = d.vars.len();
        let var_ok = |i: usize| i < nv;
        let np = d.points.len();
        let nc = d.curves.len();
        let point_ok = |p: PointId| p.index() < np;
        let curve_ok = |c: CurveId| c.index() < nc;
        let point_alive = |p: PointId| d.points[p.index()].alive;
        let curve_alive = |c: CurveId| d.curves[c.index()].alive;

        for p in &d.points {
            if !var_ok(p.x) || !var_ok(p.y) || p.z.is_some_and(|z| !var_ok(z)) {
                return Err(bad(tr!("sketch-err-data-point-value")));
            }
        }
        for c in &d.curves {
            let pts = c.kind.points();
            if pts.iter().any(|p| !point_ok(*p)) {
                return Err(bad(tr!("sketch-err-data-curve-point")));
            }
            if c.centerline && !(c.construction && matches!(c.kind, CurveKind::Line { .. })) {
                return Err(bad(tr!("sketch-err-data-centerline")));
            }
            // Der Rahmen zeigt ein Kontrollpolygon; andere Kurven haben keins.
            if c.show_frame
                && !matches!(
                    c.kind,
                    CurveKind::ControlSpline { .. }
                        | CurveKind::Conic { .. }
                        | CurveKind::FixedSpline { .. }
                )
            {
                return Err(bad(tr!("sketch-err-data-show-frame")));
            }
            check_curve_normal(c)?;
            match &c.kind {
                CurveKind::Circle { radius, .. } if !var_ok(*radius) => {
                    return Err(bad(tr!("sketch-err-data-circle-value")));
                }
                CurveKind::Ellipse {
                    ax, ay, az, minor, ..
                }
                | CurveKind::EllipticArc {
                    ax, ay, az, minor, ..
                } if !(var_ok(*ax) && var_ok(*ay) && var_ok(*minor) && az.is_none_or(var_ok)) => {
                    return Err(bad(tr!("sketch-err-data-ellipse-value")));
                }
                CurveKind::Spline { points, .. }
                    if points.len() < 2 || points.len() > MAX_SPLINE_POINTS =>
                {
                    return Err(bad(tr!("sketch-err-data-spline-count")));
                }
                CurveKind::ControlSpline {
                    points,
                    degree,
                    knots,
                } => check_control_spline(points.len(), *degree, knots)?,
                CurveKind::Conic {
                    start,
                    end,
                    apex,
                    rho,
                } => {
                    if start == end || start == apex || end == apex {
                        return Err(bad(tr!("sketch-err-point-twice")));
                    }
                    check_conic_rho(*rho)?;
                }
                // Knoten, Gewichte und Zahlen hat `NurbsCurve::new` schon geprüft.
                CurveKind::FixedSpline { start, end, curve } => {
                    let flat = curve.control_points().iter().all(|q| q.z == 0.0);
                    if !flat || (start == end && !curve.is_closed(d.tolerance.linear)) {
                        return Err(bad(tr!("sketch-err-data-fixed-spline")));
                    }
                    // Eine lebende Kurve: Die Enden liegen auf ihr und sind fest (A3).
                    if c.alive {
                        let (lo, hi) = curve.domain();
                        let holds = |p: PointId, t: f64| {
                            let r = &d.points[p.index()];
                            let at = DVec2::new(d.vars[r.x], d.vars[r.y]);
                            at.distance(curve.point(t).truncate()) <= d.tolerance.linear
                                && d.constraints.iter().any(|k| {
                                    k.alive
                                        && k.enabled
                                        && !k.driven
                                        && matches!(k.kind, ConstraintKind::Fixed(q, ..) if q == p)
                                })
                        };
                        if !(holds(*start, lo) && holds(*end, hi)) {
                            return Err(bad(tr!("sketch-err-data-fixed-spline")));
                        }
                    }
                }
                _ => {}
            }
            if c.alive && pts.iter().any(|p| !point_alive(*p)) {
                return Err(bad(tr!("sketch-err-data-curve-deleted")));
            }
        }
        check_handles(&d)?;
        for c in &d.constraints {
            if !(-1..=1).contains(&c.branch) {
                return Err(bad(tr!("sketch-err-data-constraint-place")));
            }
            if let Some(e) = c.extra
                && !var_ok(e)
            {
                return Err(bad(tr!("sketch-err-data-constraint-value")));
            }
            let pts = c.kind.points();
            let curves = c.kind.curves();
            let pattern_ok = match c.kind {
                ConstraintKind::PatternValue(m, ..) => m.index() < d.patterns.len(),
                _ => true,
            };
            if pts.iter().any(|p| !point_ok(*p))
                || curves.iter().any(|k| !curve_ok(*k))
                || !pattern_ok
            {
                return Err(bad(tr!("sketch-err-data-constraint-object")));
            }
            if !constraint_numbers_ok(&c.kind) {
                return Err(bad(tr!("sketch-err-data-constraint-number")));
            }
            if c.alive
                && (pts.iter().any(|p| !point_alive(*p)) || curves.iter().any(|k| !curve_alive(*k)))
            {
                return Err(bad(tr!("sketch-err-data-constraint-deleted")));
            }
        }
        check_patterns(&d)?;
        check_texts(&d)?;

        let mut points = Arena::new();
        for p in d.points {
            points.push(PointData {
                x: p.x,
                y: p.y,
                z: p.z,
                construction: p.construction,
                alive: p.alive,
            });
        }
        let mut curves = Arena::new();
        for c in d.curves {
            curves.push(CurveData {
                kind: c.kind,
                construction: c.construction,
                centerline: c.centerline,
                show_frame: c.show_frame,
                normal: c.normal,
                alive: c.alive,
            });
        }
        let mut constraints = Arena::new();
        for c in d.constraints {
            constraints.push(ConstraintData {
                kind: c.kind,
                branch: c.branch,
                shared_sign: 0,
                driven: c.driven,
                enabled: c.enabled,
                internal: c.internal,
                extra: c.extra,
                alive: c.alive,
            });
        }
        let mut patterns = Arena::new();
        for m in d.patterns {
            patterns.push(PatternData {
                kind: m.kind,
                sources: m.sources,
                source_points: m.source_points,
                values: m.values,
                instances: m.instances,
                suppressed: m.suppressed,
                alive: m.alive,
            });
        }
        let mut texts = Arena::new();
        for t in d.texts {
            texts.push(TextData {
                content: t.content,
                style: t.style,
                height: t.height,
                flip_h: t.flip_h,
                flip_v: t.flip_v,
                placement: t.placement,
                curves: t.curves,
                points: t.points,
                alive: t.alive,
                // Noch nicht gesetzt: Die gespeicherten Kurven bleiben (Spec G4).
                shaped: ShapedCache::default(),
                walk: WalkCache::default(),
            });
        }
        let mut s = Sketch::new(d.frame, d.name, d.tolerance);
        s.vars = d.vars;
        s.points = points;
        s.curves = curves;
        s.constraints = constraints;
        s.patterns = patterns;
        s.texts = texts;
        s.three_d = d.three_d;
        s.solve_cache = SolveCache::default();
        s.touch_structure();
        // Typen und Entartungen aller Abhängigkeiten (auch gelöschter) wie bei add_constraint
        // prüfen, sonst könnten spätere Solver-Läufe auf falschen Objekttypen abstürzen.
        for (_, c) in s.constraints.iter() {
            // Ein gelöschter Musterwert darf ein Feld haben, das nicht mehr zur Art seines Musters
            // passt: `edit_pattern` lässt ihn so zurück, wenn Art oder Kreisart wechseln. Den
            // Index des Musters hat die Prüfung oben schon gesehen.
            if !c.alive && matches!(c.kind, ConstraintKind::PatternValue(..)) {
                continue;
            }
            s.check_types(&c.kind)?;
            if c.alive {
                if c.extra.is_none() && s.needs_extra(&c.kind) {
                    return Err(bad(tr!("sketch-err-data-constraint-extra")));
                }
                s.check_constraint_value(&c.kind)?;
            }
        }
        // Ein Muster in einer Schleife zwischen Vorlage und eigenem Exemplar (über Abhängigkeiten
        // des Benutzers, interne oder beide) liefe im Löser nie zusammen und sperrte die Skizze.
        // Es wird aufgelöst, die Geometrie bleibt; so lädt und löst die Datei. Nach jedem
        // Auflösen neu prüfen: Dann sind andere Variablen gehalten.
        let dissolved = s.dissolve_cycles();
        // Die Tangenten an gemeinsamen Enden bleiben, wie die gespeicherte Geometrie sie zeigt.
        s.refresh_shared_signs(None);
        Ok((s, dissolved))
    }
}

/// Prüft die Normale einer Kurve (Skizzen 10): nur an Kreis, Bogen, Ellipse und Ellipsenbogen,
/// endlich und normiert. Eine Ellipse hat `az` genau dann, wenn sie eine Normale hat.
fn check_curve_normal(c: &CurveRecord) -> Result<(), SketchError> {
    let wrong = || bad(tr!("sketch-err-data-curve-normal"));
    let az = match c.kind {
        CurveKind::Ellipse { az, .. } | CurveKind::EllipticArc { az, .. } => az,
        _ => None,
    };
    let Some(n) = c.normal else {
        return if az.is_some() { Err(wrong()) } else { Ok(()) };
    };
    let planar = matches!(
        c.kind,
        CurveKind::Circle { .. }
            | CurveKind::Arc { .. }
            | CurveKind::Ellipse { .. }
            | CurveKind::EllipticArc { .. }
    );
    let ellipse = matches!(
        c.kind,
        CurveKind::Ellipse { .. } | CurveKind::EllipticArc { .. }
    );
    if !planar
        || !n.is_finite()
        || (n.length() - 1.0).abs() > NORMAL_UNIT_TOL
        || ellipse != az.is_some()
    {
        return Err(wrong());
    }
    Ok(())
}

/// Spielraum für `|n| = 1` einer gespeicherten Normale (Rundung beim Normieren und Laden).
const NORMAL_UNIT_TOL: f64 = 1e-9;

/// Prüft die Griffe aller Splines, auch gelöschter Einträge: am Stützpunkt, je Stützpunkt einer, die
/// Spitze kein Stützpunkt, die Griff-Linie eine Konstruktionslinie von `at` nach `tip`, die nur diesem
/// Griff gehört, der Krümmungsgriff ein Bogen um `center` zwischen `at` und `other`. An einem lebenden
/// Spline leben Linie und Bogen. Die Punkte hat die Prüfung der Kurven schon gesehen.
///
/// Zu jedem Krümmungsgriff eines lebenden Splines gehört genau eine wirksame interne
/// `SplineCurvatureHandle`-Zeile, und keine lebende Zeile steht ohne ihn; sonst wäre der Bogen frei
/// oder die Zeile leer.
fn check_handles(d: &SketchData) -> Result<(), SketchError> {
    let wrong = || bad(tr!("sketch-err-data-handle"));
    let mut rows: HashSet<(CurveId, PointId)> = HashSet::new();
    for k in d.constraints.iter().filter(|k| k.alive) {
        if let ConstraintKind::SplineCurvatureHandle(c, at) = k.kind
            && (!k.internal || !k.enabled || k.driven || !rows.insert((c, at)))
        {
            return Err(wrong());
        }
    }
    let mut used: HashSet<CurveId> = HashSet::new();
    for (index, c) in d.curves.iter().enumerate() {
        let CurveKind::Spline { points, handles } = &c.kind else {
            continue;
        };
        let mut seen: HashSet<PointId> = HashSet::new();
        for h in handles {
            if !points.contains(&h.at) || points.contains(&h.tip) || !seen.insert(h.at) {
                return Err(wrong());
            }
            if c.alive
                && matches!(h.curvature, HandleCurvature::Arc { .. })
                && !rows.remove(&(CurveId::from_index(index), h.at))
            {
                return Err(wrong());
            }
            let line = d.curves.get(h.line.index()).ok_or_else(wrong)?;
            let fits = line.kind == CurveKind::Line { a: h.at, b: h.tip } && line.construction;
            if !fits || (c.alive && !line.alive) || !used.insert(h.line) {
                return Err(wrong());
            }
            if let HandleCurvature::Arc { arc, center, other } = h.curvature {
                let bow = d.curves.get(arc.index()).ok_or_else(wrong)?;
                let fits = match bow.kind {
                    CurveKind::Arc {
                        center: m,
                        start,
                        end,
                    } => {
                        m == center
                            && ((start, end) == (h.at, other) || (start, end) == (other, h.at))
                    }
                    _ => false,
                };
                if !fits || !bow.construction || (c.alive && !bow.alive) || !used.insert(arc) {
                    return Err(wrong());
                }
            }
        }
    }
    // Was übrig ist, steht ohne Krümmungsgriff.
    if rows.is_empty() {
        Ok(())
    } else {
        Err(wrong())
    }
}

/// Prüft Punktzahl, Grad und Knoten eines Splines mit Kontrollpunkten: 2 bis
/// [`MAX_SPLINE_POINTS`] Punkte, Grad 1 bis 5 (der größte wählbare), Knoten wie
/// [`NurbsCurve::new`] sie verlangt (auch: passend zu Grad und Punktzahl).
fn check_control_spline(n: usize, degree: u8, knots: &[f64]) -> Result<(), SketchError> {
    if !(2..=MAX_SPLINE_POINTS).contains(&n) {
        return Err(bad(tr!("sketch-err-data-spline-count")));
    }
    if !(1..=MAX_CONTROL_SPLINE_DEGREE).contains(&degree) {
        return Err(bad(tr!("sketch-err-spline-degree")));
    }
    let control = vec![DVec3::ZERO; n];
    let weights = vec![1.0; n];
    NurbsCurve::new(usize::from(degree), knots.to_vec(), control, weights)
        .map(|_| ())
        .map_err(|e| bad(tr!("sketch-err-spline-invalid", detail = e.to_string())))
}

/// Prüft die Muster der Rohdaten. Indizes vorhanden, auch bei aufgelösten Mustern. Bei lebenden
/// Mustern alles, was Löser und Nachführung voraussetzen: Vorlagen, Richtungslinien, Mitte und
/// Exemplare leben; die Vorlagenpunkte sind die Punkte der Vorlagen; jedes Exemplar hat so viele
/// Punkte und Kurven wie die Vorlage, an einer eigenen Lage außerhalb der Abwahl; die Werte sind
/// lebende Musterwerte genau dieses Musters, je Feld einer, alle Felder der Art; kein Exemplar
/// ist zugleich Vorlage, Richtungslinie oder Mitte. Jeder lebende Musterwert gehört zu einem
/// lebenden Muster, das ihn führt.
fn check_patterns(d: &SketchData) -> Result<(), SketchError> {
    let (np, nc, nk) = (d.points.len(), d.curves.len(), d.constraints.len());
    for m in &d.patterns {
        // „Senkrecht zu Richtung 1“ gibt es nur als zweite Richtung.
        if matches!(m.kind, PatternKind::Rect { dirs, .. } if dirs[0] == PatternDir::Perpendicular)
        {
            return Err(bad(tr!("sketch-err-data-pattern-direction")));
        }
        let refs_ok = match m.kind {
            PatternKind::Rect { dirs, .. } => dirs
                .iter()
                .all(|dir| !matches!(dir, PatternDir::Line(l) if l.index() >= nc)),
            PatternKind::Circular { center, .. } => center.index() < np,
        };
        let points_ok = m
            .source_points
            .iter()
            .chain(m.instances.iter().flat_map(|i| &i.points))
            .all(|p| p.index() < np);
        let curves_ok = m
            .sources
            .iter()
            .chain(m.instances.iter().flat_map(|i| &i.curves))
            .all(|c| c.index() < nc);
        if !(refs_ok && points_ok && curves_ok && m.values.iter().all(|c| c.index() < nk)) {
            return Err(bad(tr!("sketch-err-data-pattern-object")));
        }
    }

    let deleted = || bad(tr!("sketch-err-data-pattern-deleted"));
    let instance = || bad(tr!("sketch-err-data-pattern-instance"));
    let value = || bad(tr!("sketch-err-data-pattern-value"));
    let point_alive = |p: PointId| d.points[p.index()].alive;
    let curve_alive = |c: CurveId| d.curves[c.index()].alive;
    // Teile der Exemplare lebender Muster, danach gegen Vorlagen, Richtungen und Mitten geprüft.
    let mut inst_curves: HashSet<CurveId> = HashSet::new();
    let mut inst_points: HashSet<PointId> = HashSet::new();
    for (k, m) in d.patterns.iter().enumerate().filter(|(_, m)| m.alive) {
        // Vorlagen: lebend, ohne Doppelte; ihre Punkte genau `source_points`.
        // Mengen statt Listensuche: Die Daten kommen aus einer Datei und können groß sein.
        let mut own_points: Vec<PointId> = Vec::new();
        let mut seen_points: HashSet<PointId> = HashSet::new();
        let mut seen_sources: HashSet<CurveId> = HashSet::new();
        for &c in &m.sources {
            if !seen_sources.insert(c) {
                return Err(instance());
            }
            if !curve_alive(c) {
                return Err(deleted());
            }
            for p in d.curves[c.index()].kind.points() {
                if seen_points.insert(p) {
                    own_points.push(p);
                }
            }
        }
        if m.sources.is_empty() || own_points != m.source_points {
            return Err(instance());
        }
        match m.kind {
            PatternKind::Rect { dirs, .. } => {
                for dir in dirs {
                    if let PatternDir::Line(l) = dir
                        && !(curve_alive(l)
                            && matches!(d.curves[l.index()].kind, CurveKind::Line { .. }))
                    {
                        return Err(deleted());
                    }
                }
            }
            PatternKind::Circular { center, .. } => {
                if !point_alive(center) {
                    return Err(deleted());
                }
            }
        }
        // Exemplare: so viele Punkte und Kurven wie die Vorlage, lebend, je eine eigene Lage.
        let suppressed: HashSet<[i32; 2]> = m.suppressed.iter().copied().collect();
        let mut lattice: HashSet<[i32; 2]> = HashSet::new();
        for inst in &m.instances {
            if inst.points.len() != m.source_points.len()
                || inst.curves.len() != m.sources.len()
                || inst.at == [0, 0]
                || suppressed.contains(&inst.at)
                || !lattice.insert(inst.at)
            {
                return Err(instance());
            }
            if !(inst.points.iter().all(|&p| point_alive(p))
                && inst.curves.iter().all(|&c| curve_alive(c)))
            {
                return Err(deleted());
            }
            if !(inst.curves.iter().all(|&c| inst_curves.insert(c))
                && inst.points.iter().all(|&p| inst_points.insert(p)))
            {
                return Err(instance());
            }
        }
        // Werte: lebende Musterwerte dieses Musters, je Feld einer, alle Felder der Art.
        let mut slots: Vec<PatternSlot> = Vec::new();
        for &c in &m.values {
            let r = &d.constraints[c.index()];
            match r.kind {
                ConstraintKind::PatternValue(owner, slot, _)
                    if owner.index() == k && r.alive && !slots.contains(&slot) =>
                {
                    slots.push(slot);
                }
                _ => return Err(value()),
            }
        }
        let wanted: &[PatternSlot] = match m.kind {
            PatternKind::Rect { .. } => &[
                PatternSlot::Count1,
                PatternSlot::Spacing1,
                PatternSlot::Count2,
                PatternSlot::Spacing2,
            ],
            PatternKind::Circular {
                mode: CircularMode::Full,
                ..
            } => &[PatternSlot::Count],
            PatternKind::Circular { .. } => &[PatternSlot::Count, PatternSlot::Angle],
        };
        if slots.len() != wanted.len() || !wanted.iter().all(|w| slots.contains(w)) {
            return Err(value());
        }
    }
    for m in d.patterns.iter().filter(|m| m.alive) {
        let used = match m.kind {
            PatternKind::Rect { dirs, .. } => dirs
                .iter()
                .any(|dir| matches!(dir, PatternDir::Line(l) if inst_curves.contains(l))),
            PatternKind::Circular { center, .. } => inst_points.contains(&center),
        };
        if used
            || m.sources.iter().any(|c| inst_curves.contains(c))
            || m.source_points.iter().any(|p| inst_points.contains(p))
        {
            return Err(PatternProblem::InstanceAsSource.error());
        }
    }
    for (i, c) in d.constraints.iter().enumerate() {
        if let ConstraintKind::PatternValue(m, ..) = c.kind
            && c.alive
        {
            let owner = &d.patterns[m.index()];
            if !(owner.alive && owner.values.iter().any(|v| v.index() == i)) {
                return Err(value());
            }
        }
    }
    Ok(())
}

/// Fehler eines Textes in den Rohdaten mit dem Grund `reason` (Schlüssel).
fn bad_text(reason: &str) -> SketchError {
    bad(tr!(
        "sketch-err-text-data",
        reason = planora_i18n::t(reason)
    ))
}

/// Prüft die Texte streng (Spec G5); eine kaputte Skizze wird abgelehnt, nicht repariert.
///
/// Jeder Text, auch ein gelöschter: nur vorhandene Kurven und Punkte (so stürzt nichts ab), Höhe
/// endlich und größer als null, Zeichenabstand in `0 ≤ p ≤ 1000`, Inhalt nicht leer und
/// höchstens [`MAX_TEXT_CHARS`] Zeichen, höchstens [`MAX_TEXT_CURVES`] erzeugte Kurven.
///
/// Jeder lebende Text dazu: alle Teile leben; die erzeugten Kurven sind Strecken, Kegelschnitte
/// mit ρ = 0,5 oder kubische Splines mit vier Kontrollpunkten (Spec B2), weder Teil eines Musters
/// (Vorlage, Exemplar, Richtung; ebenso die erzeugten Punkte samt Mitte) noch Griff eines
/// Splines; der Rahmen besteht aus vier verschiedenen Ecken und vier Konstruktionslinien, Linie
/// `i` zwischen Ecke `i` und Ecke `i + 1`; der Pfad ist weder erzeugte Kurve noch Rahmenlinie
/// eines lebenden Textes (auch nicht eines anderen); keine Kurve (erzeugt oder Rahmen) und kein
/// Punkt (erzeugt oder Ecke) gehört zweimal zu lebenden Texten. Ein gelöschter Text darf auf
/// gelöschte Teile zeigen und seine Kurven abgegeben haben (Text auflösen, Spec F).
fn check_texts(d: &SketchData) -> Result<(), SketchError> {
    let (np, nc) = (d.points.len(), d.curves.len());
    for t in &d.texts {
        let placement_ok = match t.placement {
            TextPlacement::Box { lines, corners, .. } => {
                lines.iter().all(|c| c.index() < nc) && corners.iter().all(|p| p.index() < np)
            }
            TextPlacement::Path { path, .. } => path.index() < nc,
        };
        let parts_ok =
            t.curves.iter().all(|c| c.index() < nc) && t.points.iter().all(|p| p.index() < np);
        if !(placement_ok && parts_ok) {
            return Err(bad_text("sketch-err-text-data-object"));
        }
        if !(t.height.is_finite() && t.height > 0.0) {
            return Err(bad_text("sketch-err-text-data-height"));
        }
        if !(0.0..=MAX_SPACING).contains(&t.style.spacing) {
            return Err(bad_text("sketch-err-text-data-spacing"));
        }
        if t.content.chars().count() > MAX_TEXT_CHARS || t.content.trim().is_empty() {
            return Err(bad_text("sketch-err-text-data-content"));
        }
        if t.curves.len() > MAX_TEXT_CURVES {
            return Err(bad_text("sketch-err-text-data-curves"));
        }
    }
    let live: Vec<&TextRecord> = d.texts.iter().filter(|t| t.alive).collect();
    let point_alive = |p: PointId| d.points[p.index()].alive;
    let curve_alive = |c: CurveId| d.curves[c.index()].alive;
    // Alle erzeugten Kurven und Rahmenlinien lebender Texte: Keine davon ist ein Pfad (Ruling 5c:
    // ein Rahmen ginge mit seinem Text verloren, der Text auf ihm hinge dann an nichts).
    let not_paths: HashSet<CurveId> = live
        .iter()
        .flat_map(|t| {
            let frame = match t.placement {
                TextPlacement::Box { lines, .. } => lines.to_vec(),
                TextPlacement::Path { .. } => Vec::new(),
            };
            t.curves.iter().copied().chain(frame)
        })
        .collect();
    // Teile lebender Muster und Griffe lebender Splines: Die hält das Muster bzw. der Spline,
    // erzeugte Kurven und Punkte hält der Text.
    let (pattern_curves, pattern_points) = pattern_parts(d);
    let handles: HashSet<CurveId> = d
        .curves
        .iter()
        .filter(|c| c.alive)
        .flat_map(|c| match &c.kind {
            CurveKind::Spline { handles, .. } => handles.iter().flat_map(handle_parts).collect(),
            _ => Vec::new(),
        })
        .collect();
    let mut curves: HashSet<CurveId> = HashSet::new();
    let mut points: HashSet<PointId> = HashSet::new();
    for t in live {
        let alive = t.curves.iter().all(|&c| curve_alive(c))
            && t.points.iter().all(|&p| point_alive(p))
            && match t.placement {
                TextPlacement::Box { lines, corners, .. } => {
                    lines.iter().all(|&c| curve_alive(c)) && corners.iter().all(|&p| point_alive(p))
                }
                TextPlacement::Path { path, .. } => curve_alive(path),
            };
        if !alive {
            return Err(bad_text("sketch-err-text-data-deleted"));
        }
        let glyph_kinds = t.curves.iter().all(|c| match &d.curves[c.index()].kind {
            CurveKind::Line { .. } => true,
            CurveKind::Conic { rho, .. } => *rho == 0.5,
            CurveKind::ControlSpline { points, degree, .. } => *degree == 3 && points.len() == 4,
            _ => false,
        });
        if !glyph_kinds {
            return Err(bad_text("sketch-err-text-data-kind"));
        }
        if t.curves.iter().any(|c| pattern_curves.contains(c))
            || t.points.iter().any(|p| pattern_points.contains(p))
        {
            return Err(bad_text("sketch-err-text-data-pattern"));
        }
        if t.curves.iter().any(|c| handles.contains(c)) {
            return Err(bad_text("sketch-err-text-data-handle"));
        }
        let (own_curves, own_points): (Vec<CurveId>, Vec<PointId>) = match t.placement {
            TextPlacement::Box { lines, corners, .. } => {
                let quad = (0..4).all(|i| {
                    let (p, q) = (corners[i], corners[(i + 1) % 4]);
                    matches!(d.curves[lines[i].index()].kind,
                        CurveKind::Line { a, b } if (a, b) == (p, q) || (a, b) == (q, p))
                });
                let distinct = corners.iter().collect::<HashSet<_>>().len() == 4
                    && lines.iter().collect::<HashSet<_>>().len() == 4;
                if !(quad && distinct) {
                    return Err(bad_text("sketch-err-text-data-frame"));
                }
                if !lines.iter().all(|c| d.curves[c.index()].construction) {
                    return Err(bad_text("sketch-err-text-data-construction"));
                }
                (lines.to_vec(), corners.to_vec())
            }
            TextPlacement::Path { path, .. } => {
                if not_paths.contains(&path) {
                    return Err(bad_text("sketch-err-text-data-path"));
                }
                (Vec::new(), Vec::new())
            }
        };
        let exclusive = t
            .curves
            .iter()
            .chain(&own_curves)
            .all(|&c| curves.insert(c))
            && t.points
                .iter()
                .chain(&own_points)
                .all(|&p| points.insert(p));
        if !exclusive {
            return Err(bad_text("sketch-err-text-data-shared"));
        }
    }
    Ok(())
}

/// Kurven und Punkte lebender Muster: Vorlagen, Exemplare, Richtungslinien und Mitten.
fn pattern_parts(d: &SketchData) -> (HashSet<CurveId>, HashSet<PointId>) {
    let mut curves: HashSet<CurveId> = HashSet::new();
    let mut points: HashSet<PointId> = HashSet::new();
    for m in d.patterns.iter().filter(|m| m.alive) {
        curves.extend(&m.sources);
        points.extend(&m.source_points);
        for inst in &m.instances {
            curves.extend(&inst.curves);
            points.extend(&inst.points);
        }
        match m.kind {
            PatternKind::Rect { dirs, .. } => {
                curves.extend(dirs.iter().filter_map(|dir| match dir {
                    PatternDir::Line(l) => Some(*l),
                    _ => None,
                }));
            }
            PatternKind::Circular { center, .. } => {
                points.insert(center);
            }
        }
    }
    (curves, points)
}

/// Alle Zahlen einer Abhängigkeit endlich und im Bereich (Winkel und Maße).
fn constraint_numbers_ok(k: &ConstraintKind) -> bool {
    use ConstraintKind::*;
    let ok = |v: f64| v.is_finite() && v.abs() <= MAX_COORD;
    match k {
        Fixed(_, x, y) => ok(*x) && ok(*y),
        Fixed3(_, x, y, z) => ok(*x) && ok(*y) && ok(*z),
        Distance(_, _, v)
        | DistancePointLine(_, _, v)
        | Radius(_, v)
        | Diameter(_, v)
        | Angle(_, _, v)
        | EllipseMajor(_, v)
        | EllipseMinor(_, v)
        | EllipseAngle(_, v)
        | DistanceX(_, _, v)
        | DistanceY(_, _, v)
        | CenterlineDiameter(_, _, v)
        | SlotOverall(_, _, _, v)
        | PatternValue(_, _, v) => ok(*v),
        _ => true,
    }
}
