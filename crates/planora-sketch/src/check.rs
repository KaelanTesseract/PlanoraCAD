//! Skizzenstatus und Liste der unterbestimmten Elemente (Skizzen 11a, A4).
//!
//! Alles hier liest nur den [`SolveReport`] des Lösers und das Modell; es rechnet nichts neu.

use crate::model::{CurveId, PointId, Sketch};
use crate::solver::{SolveReport, SolveStatus};
use crate::text::{TextId, TextPlacement};
use std::collections::{HashMap, HashSet};
use std::mem::discriminant;

/// Der Zustand der Skizze wie in Fusion: unterbestimmt, bestimmt, Widerspruch, nicht gelöst.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SketchStatus {
    Determined,
    Under,
    Conflict,
    NotSolved,
}

/// Ein Element der Liste „unterbestimmt“: eine Kurve, ein alleinstehender Punkt oder ein Text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CheckItem {
    Curve(CurveId),
    Point(PointId),
    Text(TextId),
}

impl Sketch {
    /// Status der Skizze: Widerspruch und „nicht gelöst“ gehen vor; sonst „bestimmt“, wenn nichts
    /// unterbestimmt ist.
    pub fn status(&self, r: &SolveReport) -> SketchStatus {
        match r.status {
            SolveStatus::Conflicting => SketchStatus::Conflict,
            SolveStatus::NotConverged => SketchStatus::NotSolved,
            SolveStatus::Solved if self.under_items(r).is_empty() => SketchStatus::Determined,
            SolveStatus::Solved => SketchStatus::Under,
        }
    }

    /// Die unterbestimmten Elemente (A4): Kurven aus `free_curves` (ohne die erzeugten Kurven
    /// und Rahmenlinien eines Textes), alleinstehende Punkte aus `free_points` (kein Punkt einer
    /// lebenden Kurve, kein Punkt eines Textes) und Texte mit einem freien Lagepunkt. Reihenfolge:
    /// Kurven, Punkte, Texte, je nach Id.
    pub fn under_items(&self, r: &SolveReport) -> Vec<CheckItem> {
        let mut curves: Vec<CurveId> = r
            .free_curves
            .iter()
            .copied()
            .filter(|&c| {
                self.curve(c).is_some()
                    && self.text_of_curve(c).is_none()
                    && self.text_of_frame(c).is_none()
            })
            .collect();
        curves.sort();
        curves.dedup();

        let owned = self.owned_points();
        let alive: HashSet<PointId> = self.points().map(|(q, _)| q).collect();
        let mut points: Vec<PointId> = r
            .free_points
            .iter()
            .copied()
            .filter(|p| alive.contains(p) && !owned.contains(p))
            .collect();
        points.sort();
        points.dedup();

        let free: HashSet<PointId> = r.free_points.iter().copied().collect();
        let mut texts: Vec<TextId> = self
            .texts()
            .filter(|(_, t)| match t.placement {
                TextPlacement::Box { corners, .. } => corners.iter().any(|c| free.contains(c)),
                TextPlacement::Path { .. } => false,
            })
            .map(|(t, _)| t)
            .collect();
        texts.sort();

        curves
            .into_iter()
            .map(CheckItem::Curve)
            .chain(points.into_iter().map(CheckItem::Point))
            .chain(texts.into_iter().map(CheckItem::Text))
            .collect()
    }

    /// Punkte, die zu einer lebenden Kurve oder einem Text gehören.
    fn owned_points(&self) -> HashSet<PointId> {
        let mut owned: HashSet<PointId> = HashSet::new();
        for (_, c) in self.curves() {
            owned.extend(c.kind.points());
        }
        for (_, t) in self.texts() {
            owned.extend(t.points.iter().copied());
            if let TextPlacement::Box { corners, .. } = t.placement {
                owned.extend(corners);
            }
        }
        owned
    }

    /// Rotes Schloss (A4): Die Skizze ist bestimmt und hat mindestens ein lebendes Element.
    pub fn is_locked(&self, r: &SolveReport) -> bool {
        self.status(r) == SketchStatus::Determined
            && (self.curves().next().is_some()
                || self.points().next().is_some()
                || self.texts().next().is_some())
    }

    /// Wie [`item_ordinal`](Self::item_ordinal) für viele Elemente auf einmal: ein Durchlauf über
    /// die Skizze statt einer je Element. Die Zahlen stehen in der Reihenfolge von `items`.
    pub fn item_ordinals(&self, items: &[CheckItem]) -> Vec<usize> {
        let mut curve_n: HashMap<CurveId, usize> = HashMap::new();
        let mut per_kind: HashMap<std::mem::Discriminant<crate::model::CurveKind>, usize> =
            HashMap::new();
        for (c, d) in self.curves() {
            let n = per_kind.entry(discriminant(&d.kind)).or_insert(0);
            *n += 1;
            curve_n.insert(c, *n);
        }
        let owned = self.owned_points();
        let mut point_n: HashMap<PointId, usize> = HashMap::new();
        for (i, (p, _)) in self
            .points()
            .filter(|(p, _)| !owned.contains(p))
            .enumerate()
        {
            point_n.insert(p, i + 1);
        }
        let text_n: HashMap<TextId, usize> = self
            .texts()
            .enumerate()
            .map(|(i, (t, _))| (t, i + 1))
            .collect();
        items
            .iter()
            .map(|&it| match it {
                CheckItem::Curve(c) => curve_n.get(&c).copied(),
                CheckItem::Point(p) => point_n.get(&p).copied(),
                CheckItem::Text(t) => text_n.get(&t).copied(),
            })
            // Ein totes Element zählt wie bei `item_ordinal` als nächste freie Nummer.
            .enumerate()
            .map(|(i, n)| n.unwrap_or_else(|| self.item_ordinal(items[i])))
            .collect()
    }

    /// Laufende Nummer ab 1 des Elements unter den lebenden Elementen derselben Art (Kurven:
    /// derselben Kurvenart, also „Linie 3“; Punkte: nur alleinstehende; Texte), in Id-Reihenfolge. Ein gelöschtes
    /// Element vor ihm verschiebt die Nummer. Ein totes Element zählt als nächste freie Nummer.
    pub fn item_ordinal(&self, it: CheckItem) -> usize {
        match it {
            CheckItem::Curve(c) => {
                let kind = self.curve(c).map(|d| discriminant(&d.kind));
                1 + self
                    .curves()
                    .filter(|(i, d)| *i < c && Some(discriminant(&d.kind)) == kind)
                    .count()
            }
            CheckItem::Point(p) => {
                // Nur alleinstehende Punkte zählen (Ruling T16-1), keine Kurven- oder Textpunkte.
                let owned = self.owned_points();
                1 + self
                    .points()
                    .filter(|(i, _)| *i < p && !owned.contains(i))
                    .count()
            }
            CheckItem::Text(t) => 1 + self.texts().filter(|(i, _)| *i < t).count(),
        }
    }
}
