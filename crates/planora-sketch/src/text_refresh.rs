//! Erzeugte Kurven eines Textes anlegen und nachführen (Skizzen 8, Spec B2 und B3).
//!
//! Aus den Konturen in Skizzenkoordinaten wird eine feste Folge von Punkten und Kurven. Die
//! vorhandenen Punkte und Kurven des Textes werden der Reihe nach wiederverwendet: Punkte immer,
//! Kurven, wenn die Art passt; sonst wird die Kurve gelöscht und eine neue angehängt.
//! Überzählige werden gelöscht, fehlende angehängt. Ändert sich nur die Lage, schreibt das nur
//! Werte, und alle Ids bleiben.

use crate::constraint::ConstraintId;
use crate::model::{CurveData, CurveId, CurveKind, PointData, PointId, Sketch};
use crate::text::{TextId, WalkCache};
use crate::text_shape::{Contour, Seg};
use planora_math::DVec2;
use std::collections::HashSet;
use std::sync::Arc;

/// Kegelschnitt mit diesem Rho ist die quadratische Bézierkurve (Gewicht 1).
const QUAD_RHO: f64 = 0.5;
/// Knoten der kubischen Bézierkurve als Spline mit Kontrollpunkten (Grad 3, 4 Punkte).
const CUBIC_KNOTS: [f64; 8] = [0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0];

/// Ein Stück mit den Indizes seiner Punkte in der Punktfolge des Textes.
#[derive(Clone, Copy, Debug)]
enum Piece {
    Line(usize, usize),
    /// Anfang, Scheitel (Kontrollpunkt), Ende.
    Quad(usize, usize, usize),
    Cubic([usize; 4]),
}

/// Punkte und Stücke aller Konturen in fester Reihenfolge. Je Kontur zuerst ihr Anfang, dann je
/// Stück die Kontrollpunkte und das Ende; das letzte Stück endet am Anfang. Eine Kontur mit
/// weniger als zwei Stücken umschließt nichts und entfällt.
fn layout(contours: &[Contour]) -> (Vec<DVec2>, Vec<Piece>) {
    let mut points: Vec<DVec2> = Vec::new();
    let mut pieces: Vec<Piece> = Vec::new();
    let push = |points: &mut Vec<DVec2>, p: DVec2| {
        points.push(p);
        points.len() - 1
    };
    for c in contours.iter().filter(|c| c.segs.len() >= 2) {
        let first = push(&mut points, c.start);
        let last = c.segs.len() - 1;
        let mut prev = first;
        for (k, seg) in c.segs.iter().enumerate() {
            // Das Ende des Stücks: ein neuer Punkt, beim letzten Stück der Anfang der Kontur.
            let end = |points: &mut Vec<DVec2>, p: DVec2| {
                if k == last { first } else { push(points, p) }
            };
            let piece = match *seg {
                Seg::Line(p) => Piece::Line(prev, end(&mut points, p)),
                Seg::Quad(a, p) => {
                    let a = push(&mut points, a);
                    Piece::Quad(prev, a, end(&mut points, p))
                }
                Seg::Cubic(a, b, p) => {
                    let a = push(&mut points, a);
                    let b = push(&mut points, b);
                    Piece::Cubic([prev, a, b, end(&mut points, p)])
                }
            };
            prev = match piece {
                Piece::Line(_, e) | Piece::Quad(_, _, e) | Piece::Cubic([_, _, _, e]) => e,
            };
            pieces.push(piece);
        }
    }
    (points, pieces)
}

impl Piece {
    /// Die Art der Kurve mit den Punkten `ids` (Indizes wie in [`layout`]).
    fn kind(self, ids: &[PointId]) -> CurveKind {
        match self {
            Piece::Line(a, b) => CurveKind::Line {
                a: ids[a],
                b: ids[b],
            },
            Piece::Quad(s, a, e) => CurveKind::Conic {
                start: ids[s],
                end: ids[e],
                apex: ids[a],
                rho: QUAD_RHO,
            },
            Piece::Cubic(q) => CurveKind::ControlSpline {
                points: q.iter().map(|&i| ids[i]).collect(),
                degree: 3,
                knots: CUBIC_KNOTS.to_vec(),
            },
        }
    }

    /// Passt `kind` zu diesem Stück? `None`: andere Art; `Some(true)`: gleiche Art und gleiche
    /// Punkte; `Some(false)`: gleiche Art, andere Punkte. Ohne Speicher anzulegen, denn das läuft
    /// in jedem Bild für alle Kurven.
    fn fits(self, kind: &CurveKind, ids: &[PointId]) -> Option<bool> {
        match (self, kind) {
            (Piece::Line(a, b), CurveKind::Line { a: x, b: y }) => {
                Some(ids[a] == *x && ids[b] == *y)
            }
            (
                Piece::Quad(s, a, e),
                CurveKind::Conic {
                    start,
                    end,
                    apex,
                    rho,
                },
            ) if *rho == QUAD_RHO => Some(ids[s] == *start && ids[a] == *apex && ids[e] == *end),
            (
                Piece::Cubic(q),
                CurveKind::ControlSpline {
                    points,
                    degree: 3,
                    knots,
                },
            ) if points.len() == 4 && knots[..] == CUBIC_KNOTS[..] => {
                Some(q.iter().zip(points).all(|(&i, p)| ids[i] == *p))
            }
            _ => None,
        }
    }
}

impl Sketch {
    /// Bringt Punkte und Kurven des Textes `t` auf die Konturen `placed` (Skizzenkoordinaten,
    /// geprüft). Neue Kurven bekommen den Stil der vorhandenen. Liefert, ob sich etwas geändert
    /// hat; nur dann gibt es einen neuen Stand.
    pub(crate) fn sync_text(&mut self, t: TextId, placed: &[Contour]) -> bool {
        let (want_points, pieces) = layout(placed);
        let old_points = std::mem::take(&mut self.texts[t].points);
        let old_curves = std::mem::take(&mut self.texts[t].curves);
        let construction = old_curves
            .iter()
            .find(|&&c| self.curves[c].alive)
            .is_some_and(|&c| self.curves[c].construction);
        let mut structure = false;
        let mut values = false;
        let mut dead_points: HashSet<PointId> = HashSet::new();
        let mut dead_curves: HashSet<CurveId> = HashSet::new();

        let mut points: Vec<PointId> = Vec::with_capacity(want_points.len());
        for (j, &at) in want_points.iter().enumerate() {
            match old_points.get(j) {
                Some(&p) if self.points[p].alive => {
                    let (xi, yi) = (self.points[p].x, self.points[p].y);
                    for (i, v) in [(xi, at.x), (yi, at.y)] {
                        if self.vars[i].to_bits() != v.to_bits() {
                            self.vars[i] = v;
                            values = true;
                        }
                    }
                    points.push(p);
                }
                _ => {
                    points.push(self.push_text_point(at));
                    structure = true;
                }
            }
        }

        let mut curves: Vec<CurveId> = Vec::with_capacity(pieces.len());
        for (i, &piece) in pieces.iter().enumerate() {
            let old = old_curves.get(i).copied();
            let reuse = old.filter(|&c| self.curves[c].alive);
            match reuse.and_then(|c| piece.fits(&self.curves[c].kind, &points).map(|s| (c, s))) {
                Some((c, true)) => curves.push(c),
                Some((c, false)) => {
                    self.curves[c].kind = piece.kind(&points);
                    structure = true;
                    curves.push(c);
                }
                None => {
                    if let Some(c) = reuse {
                        self.curves[c].alive = false;
                        dead_curves.insert(c);
                    }
                    let c = self.curves.push(CurveData {
                        kind: piece.kind(&points),
                        construction,
                        centerline: false,
                        alive: true,
                    });
                    curves.push(c);
                    structure = true;
                }
            }
        }
        for &c in old_curves.iter().skip(pieces.len()) {
            if self.curves[c].alive {
                self.curves[c].alive = false;
                dead_curves.insert(c);
                structure = true;
            }
        }
        // Überzählige Punkte erst jetzt, wenn die Kurven des Textes feststehen: Einen Punkt, den
        // noch eine andere lebende Kurve benutzt, gibt der Text nur frei; er bleibt als
        // gewöhnlicher Punkt stehen.
        let surplus: Vec<PointId> = old_points
            .iter()
            .skip(want_points.len())
            .copied()
            .filter(|&p| self.points[p].alive)
            .collect();
        if !surplus.is_empty() {
            structure = true;
            let own: HashSet<CurveId> = curves.iter().copied().collect();
            let used = self.points_used_outside(&own);
            for p in surplus.into_iter().filter(|p| !used.contains(p)) {
                self.points[p].alive = false;
                dead_points.insert(p);
            }
        }

        self.drop_constraints_at(&dead_points, &dead_curves);
        let d = &mut self.texts[t];
        d.points = points;
        d.curves = curves;
        if structure {
            self.touch_structure();
        } else if values {
            self.touch_values();
        }
        structure || values
    }

    /// Neuer Punkt eines Textes (die Lage ist geprüft).
    fn push_text_point(&mut self, at: DVec2) -> PointId {
        let (x, y) = (self.new_var(at.x), self.new_var(at.y));
        self.points.push(PointData {
            x,
            y,
            construction: false,
            alive: true,
        })
    }

    /// Löscht die Abhängigkeiten an den gelöschten erzeugten Punkten und Kurven. Nach Spec E gibt
    /// es keine; das hält nur die Daten stimmig (lebende Abhängigkeiten nur an lebenden Teilen).
    fn drop_constraints_at(&mut self, points: &HashSet<PointId>, curves: &HashSet<CurveId>) {
        if points.is_empty() && curves.is_empty() {
            return;
        }
        let dead: Vec<ConstraintId> = self
            .constraints
            .iter()
            .filter(|(_, k)| {
                k.alive
                    && (k.kind.points().iter().any(|p| points.contains(p))
                        || k.kind.curves().iter().any(|c| curves.contains(c)))
            })
            .map(|(i, _)| i)
            .collect();
        for i in dead {
            self.constraints[i].alive = false;
        }
    }

    /// Schreibt die Punkte aller gesetzten Texte neu aus ihrem `TextShape` und der aktuellen Lage
    /// von Rahmen oder Pfad (vor und nach jedem Lösen, wie `refresh_patterns`). Ein Text ohne
    /// gesetztes `TextShape` (gerade geladen) bleibt, wie er ist; ebenso einer, dessen Lage gerade
    /// nicht geht (letzte gute Lage). Liefert, ob sich etwas geändert hat.
    pub(crate) fn refresh_texts(&mut self) -> bool {
        if self.texts.is_empty() {
            return false;
        }
        let shaped: Vec<(TextId, Arc<_>)> = self
            .texts()
            .filter_map(|(t, d)| d.shaped.0.clone().map(|s| (t, s)))
            .collect();
        let mut changed = false;
        for (t, shape) in shaped {
            let (placed, walk) = self.placed_contours_walk(t, &shape);
            // Die Längentabelle des Pfades merken: Beim nächsten Lösen mit gleichem Pfad wird sie
            // nicht neu gebaut.
            if walk.is_some() {
                self.texts[t].walk = WalkCache(walk);
            }
            if let Some(placed) = placed {
                changed |= self.sync_text(t, &placed);
            }
        }
        changed
    }
}
