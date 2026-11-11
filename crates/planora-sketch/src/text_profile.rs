//! Profile aus Text (Skizzen 8, Spec D1): leere Innenflächen der Glyphen erkennen.
//!
//! Schriften füllen nach der Regel „nicht null“ (TrueType wie CFF): Ein Punkt ist Tinte, wenn ihn
//! die Konturen mit einer Umlaufzahl ungleich null umlaufen. Eine Fläche der Anordnung, deren
//! Ränder (außen und alle Löcher) Kurven desselben Textes sind und in deren Innerem die Umlaufzahl
//! der Textkurven null ist, ist eine leere Innenfläche, etwa das Innere von „o“, „a“ oder „B“. Sie
//! ist kein Profil (**Annahme**: so zeigt Fusion einen Text als Profil, siehe
//! `docs/superpowers/offene-punkte.md`). Die Teilflächen überlappender Glyphen haben die
//! Umlaufzahl 2 und bleiben Profile. Grenzt eine Fläche auch an andere Kurven (ein Kreis im
//! Inneren des „o“, ein Rechteck um den Text), bleibt alles wie bisher.
//!
//! Die Umlaufzahl wird am Linienzug der Stücke gezählt, aus dem auch die Fläche stammt (64
//! Schritte je Vollkreis, siehe `arrangement.rs`). Der Punkt im Inneren ist die Mitte des
//! breitesten Abschnitts einiger waagerechter Prüflinien durch die Fläche.

use crate::arrangement::{Arrangement, STEPS_PER_TURN};
use crate::model::{CurveId, Sketch};
use crate::text::TextId;
use planora_math::DVec2;
use std::collections::{HashMap, HashSet};

/// Waagerechte Prüflinien je Fläche bei der Suche nach einem Punkt im Inneren.
const SCAN_LINES: usize = 8;

/// Linienzug eines Stücks in Kurvenrichtung mit seinem Hüllrechteck.
struct PieceLine {
    lo: DVec2,
    hi: DVec2,
    points: Vec<DVec2>,
}

impl Sketch {
    /// Die Flächen aus `faces` (Umläufe der Anordnung `arr`, Löcher je Umlauf in `holes`), die
    /// leere Innenflächen eines Textes sind (siehe Modulbeschreibung).
    pub(crate) fn text_counters(
        &self,
        arr: &Arrangement,
        faces: &[usize],
        holes: &[Vec<usize>],
    ) -> HashSet<usize> {
        let mut out = HashSet::new();
        if self.texts().next().is_none() {
            return out;
        }
        let index: HashMap<CurveId, TextId> = self
            .texts()
            .flat_map(|(t, d)| d.curves.iter().map(move |&c| (c, t)))
            .collect();
        // Text je Kurve der Anordnung.
        let text_of: Vec<Option<TextId>> = arr
            .curves
            .iter()
            .map(|(id, _)| index.get(id).copied())
            .collect();
        // Der Text einer Fläche, wenn alle ihre Ränder zu ihm gehören.
        let face_text = |f: usize| -> Option<TextId> {
            let mut text = None;
            for c in std::iter::once(f).chain(holes[f].iter().copied()) {
                for &h in &arr.cycles[c].half_edges {
                    let t = text_of[arr.pieces[h / 2].input]?;
                    if *text.get_or_insert(t) != t {
                        return None;
                    }
                }
            }
            text
        };
        let candidates: Vec<(usize, TextId)> = faces
            .iter()
            .filter_map(|&f| face_text(f).map(|t| (f, t)))
            .collect();
        if candidates.is_empty() {
            return out;
        }
        let wanted: HashSet<TextId> = candidates.iter().map(|&(_, t)| t).collect();
        // Alle Stücke der Texte, auch doppelte: Die Schrift zählt jede Kontur.
        let mut lines: HashMap<TextId, Vec<PieceLine>> = HashMap::new();
        for (k, p) in arr.pieces.iter().enumerate() {
            let Some(t) = text_of[p.input].filter(|t| wanted.contains(t)) else {
                continue;
            };
            let points = arr.geom_of(k).polyline(p.t0, p.t1, STEPS_PER_TURN);
            let (lo, hi) = points
                .iter()
                .fold((DVec2::INFINITY, DVec2::NEG_INFINITY), |(lo, hi), q| {
                    (lo.min(*q), hi.max(*q))
                });
            lines
                .entry(t)
                .or_default()
                .push(PieceLine { lo, hi, points });
        }
        for (f, t) in candidates {
            let Some(p) = interior_point(arr, f, &holes[f]) else {
                continue;
            };
            if lines.get(&t).is_some_and(|l| winding(l, p) == 0) {
                out.insert(f);
            }
        }
        out
    }
}

/// Ein Punkt im Inneren der Fläche `f` mit den Löchern `holes`: die Mitte des breitesten
/// Abschnitts im Inneren auf einer der waagerechten Prüflinien. `None` bei einer Fläche ohne
/// Ausdehnung.
fn interior_point(arr: &Arrangement, f: usize, holes: &[usize]) -> Option<DVec2> {
    let (lo, hi) = arr.cycles[f].bbox;
    if !(lo.is_finite() && hi.is_finite() && hi.y > lo.y) {
        return None;
    }
    let polygons: Vec<&[DVec2]> = std::iter::once(f)
        .chain(holes.iter().copied())
        .map(|c| arr.cycles[c].polygon.as_slice())
        .collect();
    let mut best: Option<(f64, DVec2)> = None;
    let mut xs: Vec<f64> = Vec::new();
    for k in 0..SCAN_LINES {
        let y = lo.y + (hi.y - lo.y) * (k as f64 + 0.5) / SCAN_LINES as f64;
        xs.clear();
        for poly in &polygons {
            let n = poly.len();
            for i in 0..n {
                let (a, b) = (poly[i], poly[(i + 1) % n]);
                if (a.y > y) != (b.y > y) {
                    xs.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
                }
            }
        }
        xs.sort_by(f64::total_cmp);
        // Gerade/ungerade: Abschnitte zwischen den Schnitten 0–1, 2–3, … liegen in der Fläche
        // (außen gegen den Uhrzeigersinn, Löcher darin).
        for &[a, b] in xs.as_chunks::<2>().0 {
            let w = b - a;
            if w > 0.0 && best.is_none_or(|(bw, _)| w > bw) {
                best = Some((w, DVec2::new(0.5 * (a + b), y)));
            }
        }
    }
    best.map(|(_, p)| p).filter(|p| p.is_finite())
}

/// Umlaufzahl der gerichteten Linienzüge `lines` (zusammen geschlossene Konturen) um `p`:
/// vorzeichenbehaftete Kreuzungen eines Strahls nach rechts.
fn winding(lines: &[PieceLine], p: DVec2) -> i64 {
    let mut w = 0;
    for l in lines {
        if l.hi.x < p.x || l.lo.y > p.y || l.hi.y < p.y {
            continue;
        }
        for s in l.points.windows(2) {
            let (a, b) = (s[0], s[1]);
            let side = (b - a).perp_dot(p - a);
            if a.y <= p.y {
                if b.y > p.y && side > 0.0 {
                    w += 1;
                }
            } else if b.y <= p.y && side < 0.0 {
                w -= 1;
            }
        }
    }
    w
}
