//! Vorschlag der Grundmaße (Teilprojekt 12): aus den berechneten Linien der Normansichten
//! Gesamtmaße, Durchmesser/Radien und Bohrungsmitten, jedes Maß im ganzen Blatt einmal.
//!
//! Regeln je Normansicht (die Isometrie bekommt nichts):
//! - **Gesamtmaße:** Breite (`Horizontal`) und Höhe (`Vertical`) der **sichtbaren** Linien. Bezug
//!   an jeder Seite ist eine sichtbare Kante, die quer zur Messrichtung steht und deren Bildlage
//!   die äußerste Lage ist (bis auf `EXTREME_REL · Ausdehnung`): eine gerade Kante oder ein von
//!   der Seite gesehener Kreis (als Strecke im Bild). Gibt es keine solche Kante, dient ein
//!   Eckpunkt (Ende einer Kante, das Ende eines sichtbaren Stücks ist) an der äußersten Lage als
//!   Bezug. Fehlt an einer Seite jeder Bezug (etwa am Umriss eines Zylinders oder einer Kugel),
//!   gibt es für diese Richtung kein Gesamtmaß. Sind beide Bezüge Eckpunkte auf demselben vollen
//!   Kreis (etwa die Nahtpunkte eines aus zwei Halbkreisen gebauten Zylinders), wäre das
//!   Gesamtmaß nur dessen Ø noch einmal: es entfällt.
//! - **Ø/R:** Kreiskanten mit Blick in Achsrichtung werden nach Mitte (im Raum) und Radius
//!   zusammengefasst; ist eine davon sichtbar und ergeben sie zusammen einen vollen Kreis → Ø,
//!   sonst (Bogen, etwa eine Rundung) → R. Jede sichtbare Kugel → Ø. Jede Zylinderfläche, deren
//!   Achse quer zur Blickrichtung steht und deren beide Umrisslinien sichtbar sind → Ø quer.
//! - **Bohrungsmitten:** je vollem Kreis ein waagerechtes Maß von der linken und ein senkrechtes
//!   von der unteren Bezugskante der Gesamtmaße zum Mittelpunkt, nur wenn diese Bezugskante eine
//!   Kante ist (kein Eckpunkt). Die Maße einer Ansicht und Richtung bilden eine Gruppe
//!   `Baseline`, `order` aufsteigend nach Abstand.
//!
//! **Eindeutigkeit:** Der Schlüssel ([`dimension_key`]) beschreibt das Maß geometrisch nach
//! Messrichtung und Lage der Bezüge im Raum, unabhängig von der Ansicht: dieselbe Breite in
//! Vorder- und Draufsicht hat denselben Schlüssel, obwohl sie dort an verschiedenen Kanten hängt.
//! Je Schlüssel bleibt ein Vorschlag: zuerst der, dessen Kreis als Kreis erscheint, dann nach der
//! Ansicht (Breite: Vorne, Oben, Hinten, Unten; Höhe: Vorne, Links, Rechts, Hinten; Tiefe: Oben,
//! Links, Rechts, Unten; Ø/R: Vorne, Oben, Links, Rechts, Hinten, Unten), zuletzt nach der
//! Reihenfolge der Ansichten. Schlüssel, die in `existing` stehen, fallen weg. Bewusst so
//! gewollt: Zwei Maße mit derselben Messrichtung zwischen denselben Lagen (gleiche Bezugsebenen,
//! gleicher Wert) gelten als **ein** Maß, auch wenn sie an verschiedenen Kanten hängen – etwa
//! eine Bohrungsmitte, die genau so weit vom Rand liegt wie die Gesamtbreite, oder zwei Kanten in
//! derselben Ebene.
//!
//! Jeder Vorschlag wird vor der Ausgabe mit [`measure`] gemessen; was dort scheitert (etwa die
//! Länge null), wird nicht vorgeschlagen. Reihenfolge der Ausgabe: nach Ansicht (Reihenfolge in
//! `views`), darin Breite, Höhe, Ø, R, Bohrungsmitten waagerecht, senkrecht; innerhalb einer Art
//! nach Schlüssel, Bohrungsmitten nach `order`.

use crate::basis::Basis;
use crate::centerline::axial_range;
use crate::dimension::{
    ACROSS_TOL, Anchor, ChainMode, ChainSpec, DimKind, EndSel, Geo3, Measured, VIEW_TOL, measure,
};
use crate::limits::geom_tol;
use crate::model::{ViewId, ViewKind, ViewSpec};
use crate::stroke::{Stroke, StrokeOrigin};
use crate::{DrawBody, ViewLines, Visibility};
use planora_kernel::{Curve, Edge, Face, Id, Solid, Surface, Vertex};
use planora_math::{DVec2, DVec3};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Eine Lage gilt als äußerste Lage der sichtbaren Linien, wenn sie höchstens
/// `EXTREME_REL · Ausdehnung` (mindestens `geom_tol`) davon entfernt ist.
pub const EXTREME_REL: f64 = 1e-7;
/// Raster der Zahlen im Schlüssel (mm): auf `KEY_STEP` gerundet, mit sechs Nachkommastellen.
pub const KEY_STEP: f64 = 1e-6;
/// Bögen derselben Mitte und desselben Radius gelten zusammen als voller Kreis, wenn ihre
/// Winkel zusammen höchstens so viel (Radiant) unter 2π liegen.
const FULL_TURN_TOL: f64 = 1e-6;

/// Kandidat für ein Maß: beschreibt die Bezüge als `Geo3` (zum Messen) **und** die Herkunft
/// (zum späteren Verweisbau durch die Oberfläche). `geos[i]` ist die Auflösung von
/// `origins[i]` nach den Regeln der Aufgabe 8 (Kante mit Gerade → `Line` zwischen den
/// Eckpunkten, Kreiskante → `Circle`, Kantenende → `Point`, Mitte → `Point`, Zylinderfläche →
/// `Axis` mit dem Bereich der Randkanten, Kugelfläche → `Sphere`).
///
/// `chain`: bei Bohrungsmitten die Gruppe `Baseline` mit `order` (aufsteigend nach Abstand); die
/// Gruppennummer `group` gilt nur innerhalb **eines** Ergebnisses von [`suggest`] (1, 2, …) –
/// die Oberfläche vergibt beim Einfügen eigene, in der Zeichnung eindeutige Nummern.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub kind: DimKind,
    pub view: ViewId,
    pub geos: Vec<Geo3>,
    pub origins: Vec<CandidateOrigin>,
    /// Stabiler Schlüssel des Maßes (siehe [`dimension_key`]).
    pub key: String,
    pub chain: Option<ChainSpec>,
}

/// Herkunft eines Bezugs: Körperindex (in `bodies`) und Kanten- bzw. Flächenindex (in
/// `solid.edges` bzw. `solid.faces`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateOrigin {
    Edge {
        body: usize,
        edge: usize,
    },
    EdgeEnd {
        body: usize,
        edge: usize,
        end: EndSel,
    },
    Center {
        body: usize,
        edge: usize,
    },
    Face {
        body: usize,
        face: usize,
    },
}

/// Art eines Vorschlags in der Ausgabereihenfolge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Cat {
    OverallH,
    OverallV,
    Diameter,
    Radius,
    BaselineH,
    BaselineV,
}

impl Cat {
    fn is_baseline(self) -> bool {
        matches!(self, Cat::BaselineH | Cat::BaselineV)
    }
}

/// Vorschlag vor der Auswahl: Rang `(Kreis als Kreis? 0 : 1, Vorzug der Ansicht, Lage der
/// Ansicht)` – kleiner gewinnt.
struct Proposal {
    cand: Candidate,
    cat: Cat,
    view_pos: usize,
    rank: (u8, u8, usize),
    value: f64,
}

/// Ein Bezug eines Gesamtmaßes.
#[derive(Clone)]
struct Ref {
    geo: Geo3,
    origin: CandidateOrigin,
    /// Kante (sonst Eckpunkt).
    edge: bool,
}

/// Schlägt die Grundmaße für die Ansichten `views` vor (Regeln siehe Modulbeschreibung).
/// `bodies` sind die exakten Körper, aus denen die Linien berechnet wurden (die Herkunft der
/// Striche zeigt in diese Liste); passen Körper und Linien nicht zusammen, entfallen die
/// betroffenen Bezüge (nie ein Absturz). Ansichten, deren Art nicht zu ihren Linien passt, und
/// die Isometrie werden übergangen. `existing`: schon vorhandene Maße (Ansicht, Art, Schlüssel
/// nach [`dimension_key`]); ihre Schlüssel werden nicht noch einmal vorgeschlagen.
pub fn suggest(
    bodies: &[DrawBody],
    views: &[(ViewSpec, &ViewLines)],
    existing: &[(ViewId, DimKind, String)],
) -> Vec<Candidate> {
    let mut all = Vec::new();
    for (pos, (spec, lines)) in views.iter().enumerate() {
        if !spec.kind.is_normal() || lines.kind != spec.kind {
            continue;
        }
        view_proposals(bodies, pos, spec, lines, &mut all);
    }
    // Eindeutigkeit: je Schlüssel der beste Rang (stabile Sortierung: bei Gleichstand die
    // Erzeugungsreihenfolge); vorhandene Maße fallen weg.
    all.sort_by(|a, b| a.cand.key.cmp(&b.cand.key).then(a.rank.cmp(&b.rank)));
    let mut kept: Vec<Proposal> = Vec::new();
    for p in all {
        if existing.iter().any(|(_, _, k)| *k == p.cand.key) {
            continue;
        }
        if kept.last().is_some_and(|q| q.cand.key == p.cand.key) {
            continue;
        }
        kept.push(p);
    }
    kept.sort_by(|a, b| {
        a.view_pos
            .cmp(&b.view_pos)
            .then(a.cat.cmp(&b.cat))
            .then_with(|| {
                if a.cat.is_baseline() {
                    a.value.total_cmp(&b.value)
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .then_with(|| a.cand.key.cmp(&b.cand.key))
    });
    // Bezugsmaße: eine Gruppe je Ansicht und Richtung, `order` nach Abstand.
    let mut group = 0u64;
    let mut current: Option<(usize, Cat)> = None;
    let mut order = 0u32;
    for p in &mut kept {
        if !p.cat.is_baseline() {
            continue;
        }
        if current != Some((p.view_pos, p.cat)) {
            group += 1;
            order = 0;
            current = Some((p.view_pos, p.cat));
        }
        p.cand.chain = Some(ChainSpec {
            group,
            mode: ChainMode::Baseline,
            order,
        });
        order += 1;
    }
    kept.into_iter().map(|p| p.cand).collect()
}

/// Schlüssel eines Maßes aus seinen aufgelösten Bezügen (für vorhandene Maße wie für
/// Vorschläge gleich): `None`, wenn die Messung scheitert, und für schräge Maße und Winkel.
///
/// - Waagerecht/senkrecht: `L|<Richtung>|<Lage 1>|<Lage 2>` – Messrichtung im Raum (`ex` bzw.
///   `ey`, Vorzeichen so, dass die erste von null verschiedene Komponente positiv ist) und die
///   beiden Lagen der Bezüge entlang dieser Richtung, aufsteigend.
/// - Ø/R an Kreis oder Zylinderfläche: `D|A|…` bzw. `R|A|…` mit Achsrichtung (Vorzeichen wie
///   oben), Fußpunkt der Achse (nächster Punkt zum Ursprung) und Radius – alle Kreise und
///   Flächen derselben Achse mit demselben Radius haben denselben Schlüssel.
/// - Ø/R an einer Kugel: `D|K|<Mitte>|<Radius>`.
///
/// Zahlen auf [`KEY_STEP`] gerundet, sechs Nachkommastellen, `-0` als `0`.
pub fn dimension_key(
    kind: DimKind,
    geos: &[Geo3],
    basis: &Basis,
    view_kind: ViewKind,
    extent: f64,
) -> Option<String> {
    let m = measure(kind, geos, basis, view_kind, extent).ok()?;
    key_from(kind, geos, basis, &m)
}

fn key_from(kind: DimKind, geos: &[Geo3], basis: &Basis, m: &Measured) -> Option<String> {
    match kind {
        DimKind::Horizontal | DimKind::Vertical => {
            let Anchor::Linear {
                dir,
                a_span,
                b_span,
                ..
            } = &m.anchor
            else {
                return None;
            };
            let u3 = if kind == DimKind::Horizontal {
                basis.ex
            } else {
                basis.ey
            };
            let s = canon_sign(u3)?;
            let (pa, pb) = (s * a_span.0.dot(*dir), s * b_span.0.dot(*dir));
            let (lo, hi) = if pa <= pb { (pa, pb) } else { (pb, pa) };
            Some(format!("L|{}|{}|{}", qv(u3 * s), q(lo), q(hi)))
        }
        DimKind::Diameter | DimKind::Radius => {
            let tag = if kind == DimKind::Diameter { "D" } else { "R" };
            match geos.first()? {
                Geo3::Circle {
                    center,
                    normal,
                    radius,
                } => axis_key(tag, *center, *normal, *radius),
                Geo3::Axis {
                    origin,
                    dir,
                    radius,
                    ..
                } => axis_key(tag, *origin, *dir, *radius),
                Geo3::Sphere { center, radius } => {
                    Some(format!("{tag}|K|{}|{}", qv(*center), q(*radius)))
                }
                Geo3::Point(_) | Geo3::Line { .. } => None,
            }
        }
        DimKind::Aligned | DimKind::Angle => None,
    }
}

/// Schlüssel einer Achse (Kreis oder Zylinderfläche) mit Radius.
fn axis_key(tag: &str, p: DVec3, dir: DVec3, r: f64) -> Option<String> {
    let e = dir.normalize_or_zero();
    let e = e * canon_sign(e)?;
    let foot = p - e * p.dot(e);
    Some(format!("{tag}|A|{}|{}|{}", qv(e), qv(foot), q(r)))
}

/// Vorzeichen, das die erste deutlich von null verschiedene Komponente positiv macht.
fn canon_sign(v: DVec3) -> Option<f64> {
    let c = v.to_array().into_iter().find(|c| c.abs() > 1e-12)?;
    Some(if c < 0.0 { -1.0 } else { 1.0 })
}

/// Zahl im Schlüssel.
fn q(x: f64) -> String {
    let v = (x / KEY_STEP).round() * KEY_STEP;
    let v = if v == 0.0 { 0.0 } else { v };
    format!("{v:.6}")
}

fn qv(v: DVec3) -> String {
    format!("{},{},{}", q(v.x), q(v.y), q(v.z))
}

/// Vorzug der Ansicht für ein Maß (kleiner gewinnt): nach der gemessenen Weltachse bzw. für
/// Ø/R nach fester Reihenfolge.
fn view_pref(view: ViewKind, kind: DimKind, basis: &Basis) -> u8 {
    use ViewKind::*;
    let list: &[ViewKind] = match kind {
        DimKind::Horizontal | DimKind::Vertical => {
            let u3 = if kind == DimKind::Horizontal {
                basis.ex
            } else {
                basis.ey
            };
            let a = u3.abs();
            if a.x >= a.y && a.x >= a.z {
                &[Front, Top, Back, Bottom]
            } else if a.y >= a.z {
                &[Top, Left, Right, Bottom]
            } else {
                &[Front, Left, Right, Back]
            }
        }
        _ => &[Front, Top, Left, Right, Back, Bottom],
    };
    list.iter().position(|v| *v == view).unwrap_or(list.len()) as u8
}

/// Kante `edge` des Körpers `body`, wenn es sie gibt.
fn edge_of<'a>(bodies: &[DrawBody<'a>], body: usize, edge: usize) -> Option<(&'a Solid, &'a Edge)> {
    let solid = bodies.get(body)?.solid;
    if edge >= solid.edges.len() {
        return None;
    }
    Some((solid, solid.edges.get(Id::<Edge>::from_index(edge))?))
}

/// Fläche `face` des Körpers `body`, wenn es sie gibt.
fn face_of<'a>(bodies: &[DrawBody<'a>], body: usize, face: usize) -> Option<(&'a Solid, &'a Face)> {
    let solid = bodies.get(body)?.solid;
    if face >= solid.faces.len() {
        return None;
    }
    Some((solid, solid.faces.get(Id::<Face>::from_index(face))?))
}

fn vertex(solid: &Solid, id: Id<Vertex>) -> Option<DVec3> {
    solid
        .vertices
        .get(id)
        .map(|v| v.point)
        .filter(|p| p.is_finite())
}

/// Eine Ansicht beim Vorschlagen: Körper, Lage in der Ansichtsliste, Ansicht, Linien, Toleranz.
struct ViewCtx<'v, 'b> {
    bodies: &'v [DrawBody<'b>],
    pos: usize,
    spec: &'v ViewSpec,
    lines: &'v ViewLines,
    tol: f64,
}

impl ViewCtx<'_, '_> {
    /// Abstand, bis zu dem eine Lage als äußerste Lage gilt (und Punkte als gleich).
    fn near(&self) -> f64 {
        (EXTREME_REL * self.lines.extent).max(self.tol)
    }

    /// Misst den Vorschlag und legt ihn ab (scheitert die Messung, entfällt er).
    fn push(
        &self,
        out: &mut Vec<Proposal>,
        kind: DimKind,
        geos: Vec<Geo3>,
        origins: Vec<CandidateOrigin>,
        cat: Cat,
        as_circle: bool,
    ) {
        let basis = &self.lines.basis;
        let Ok(m) = measure(kind, &geos, basis, self.spec.kind, self.lines.extent) else {
            return;
        };
        if !(m.value.is_finite() && m.value >= self.tol) {
            return;
        }
        let Some(key) = key_from(kind, &geos, basis, &m) else {
            return;
        };
        out.push(Proposal {
            rank: (
                u8::from(!as_circle),
                view_pref(self.spec.kind, kind, basis),
                self.pos,
            ),
            cand: Candidate {
                kind,
                view: self.spec.id,
                geos,
                origins,
                key,
                chain: None,
            },
            cat,
            view_pos: self.pos,
            value: m.value,
        });
    }
}

/// Alle Vorschläge einer Normansicht.
fn view_proposals(
    bodies: &[DrawBody],
    pos: usize,
    spec: &ViewSpec,
    lines: &ViewLines,
    out: &mut Vec<Proposal>,
) {
    let basis = &lines.basis;
    let extent = lines.extent;
    if !(extent.is_finite() && extent >= 0.0) {
        return;
    }
    let ctx = ViewCtx {
        bodies,
        pos,
        spec,
        lines,
        tol: geom_tol(extent),
    };
    // Sichtbare Striche, Hüllbox und Enden der sichtbaren Stücke.
    let mut visible = vec![false; lines.strokes.len()];
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    let mut ends: Vec<DVec2> = Vec::new();
    for p in &lines.pieces {
        if p.vis != Visibility::Visible {
            continue;
        }
        let Some(s) = lines.strokes.get(p.stroke) else {
            continue;
        };
        visible[p.stroke] = true;
        let part = Stroke {
            t0: p.t0,
            t1: p.t1,
            ..s.clone()
        };
        let (a, b) = part.bbox2(basis, 0.0);
        lo = lo.min(a);
        hi = hi.max(b);
        ends.push(s.point2(basis, p.t0));
        ends.push(s.point2(basis, p.t1));
    }
    let refs = if lo.is_finite() && hi.is_finite() {
        extreme_refs(&ctx, &visible, &ends, (lo, hi))
    } else {
        Default::default()
    };

    // (a) Gesamtmaße. Zwei Eckpunkte auf demselben vollen Kreis messen dessen Ø noch einmal
    // (etwa die Nahtpunkte eines Zylinders bei 0° und 180°): kein Gesamtmaß.
    let full = full_circles(&ctx);
    let on_one_circle = |a: &Ref, b: &Ref| match (&a.geo, &b.geo, a.edge || b.edge) {
        (Geo3::Point(p), Geo3::Point(q), false) => full.iter().any(|&(c, n, r)| {
            let on = |x: DVec3| {
                let v = x - c;
                (v.length() - r).abs() <= ctx.near() && v.dot(n).abs() <= ctx.near()
            };
            on(*p) && on(*q)
        }),
        _ => false,
    };
    for (k, kind, cat) in [
        (0, DimKind::Horizontal, Cat::OverallH),
        (1, DimKind::Vertical, Cat::OverallV),
    ] {
        if let [Some(a), Some(b)] = &refs[k]
            && !on_one_circle(a, b)
        {
            ctx.push(
                out,
                kind,
                vec![a.geo.clone(), b.geo.clone()],
                vec![a.origin, b.origin],
                cat,
                true,
            );
        }
    }

    // (b), (c) Kreise mit Blick in Achsrichtung: zusammengefasst nach Mitte und Radius.
    struct Round {
        geo: Geo3,
        body: usize,
        edge: usize,
        center: DVec3,
        turn: f64,
        visible: bool,
    }
    let d = basis.d;
    let mut rounds: BTreeMap<String, Round> = BTreeMap::new();
    for (i, s) in lines.strokes.iter().enumerate() {
        let StrokeOrigin::Edge { body, edge } = s.origin else {
            continue;
        };
        let Some((_, e)) = edge_of(bodies, body, edge) else {
            continue;
        };
        let Curve::Circle { frame, radius } = &e.curve else {
            continue;
        };
        let n = frame.z.normalize_or_zero();
        if !(radius.is_finite() && *radius > 0.0 && frame.origin.is_finite())
            || n.dot(d).abs() < 1.0 - VIEW_TOL
        {
            continue;
        }
        let key = format!("{}|{}", qv(frame.origin), q(*radius));
        let r = rounds.entry(key).or_insert_with(|| Round {
            geo: Geo3::Circle {
                center: frame.origin,
                normal: frame.z,
                radius: *radius,
            },
            body,
            edge,
            center: frame.origin,
            turn: 0.0,
            visible: false,
        });
        r.turn += (e.t1 - e.t0).abs();
        // Herkunft: die erste sichtbare Kante der Gruppe (vor verdeckten).
        if visible[i] && !r.visible {
            r.geo = Geo3::Circle {
                center: frame.origin,
                normal: frame.z,
                radius: *radius,
            };
            (r.body, r.edge, r.center) = (body, edge, frame.origin);
            r.visible = true;
        }
    }
    for r in rounds.values() {
        if !r.visible {
            continue;
        }
        let edge = CandidateOrigin::Edge {
            body: r.body,
            edge: r.edge,
        };
        if r.turn < std::f64::consts::TAU - FULL_TURN_TOL {
            ctx.push(
                out,
                DimKind::Radius,
                vec![r.geo.clone()],
                vec![edge],
                Cat::Radius,
                true,
            );
            continue;
        }
        ctx.push(
            out,
            DimKind::Diameter,
            vec![r.geo.clone()],
            vec![edge],
            Cat::Diameter,
            true,
        );
        // Bohrungsmitte von der linken bzw. unteren Bezugskante.
        let center = CandidateOrigin::Center {
            body: r.body,
            edge: r.edge,
        };
        for (k, kind, cat) in [
            (0, DimKind::Horizontal, Cat::BaselineH),
            (1, DimKind::Vertical, Cat::BaselineV),
        ] {
            if let Some(a) = refs[k][0].as_ref().filter(|a| a.edge) {
                ctx.push(
                    out,
                    kind,
                    vec![a.geo.clone(), Geo3::Point(r.center)],
                    vec![a.origin, center],
                    cat,
                    true,
                );
            }
        }
    }

    // (b) Kugeln und Zylinder von der Seite: aus den sichtbaren Umrisslinien.
    let mut sides: BTreeMap<(usize, usize), (bool, bool)> = BTreeMap::new();
    let mut spheres: BTreeSet<(usize, usize)> = BTreeSet::new();
    for (i, s) in lines.strokes.iter().enumerate() {
        let StrokeOrigin::Silhouette { body, face } = s.origin else {
            continue;
        };
        if !visible[i] {
            continue;
        }
        let Some((_, f)) = face_of(bodies, body, face) else {
            continue;
        };
        match &f.surface {
            Surface::Sphere { .. } => {
                spheres.insert((body, face));
            }
            Surface::Cylinder { frame, .. } => {
                let z = frame.z.normalize_or_zero();
                let qd = basis.project(z).normalize_or_zero();
                if z.dot(d).abs() > VIEW_TOL || qd == DVec2::ZERO {
                    continue;
                }
                let mid = s.point2(basis, 0.5 * (s.t0 + s.t1));
                let side = (mid - basis.project(frame.origin)).perp_dot(qd);
                let e = sides.entry((body, face)).or_default();
                if side > ctx.tol {
                    e.0 = true;
                } else if side < -ctx.tol {
                    e.1 = true;
                }
            }
            _ => {}
        }
    }
    for &(body, face) in &spheres {
        let Some((_, f)) = face_of(bodies, body, face) else {
            continue;
        };
        if let Surface::Sphere { frame, radius } = &f.surface {
            ctx.push(
                out,
                DimKind::Diameter,
                vec![Geo3::Sphere {
                    center: frame.origin,
                    radius: *radius,
                }],
                vec![CandidateOrigin::Face { body, face }],
                Cat::Diameter,
                true,
            );
        }
    }
    for (&(body, face), &both) in &sides {
        if both != (true, true) {
            continue;
        }
        let Some((solid, f)) = face_of(bodies, body, face) else {
            continue;
        };
        let Surface::Cylinder { frame, radius } = &f.surface else {
            continue;
        };
        let z = frame.z.normalize_or_zero();
        let Some((t0, t1)) = axial_range(solid, f, frame.origin, z) else {
            continue;
        };
        ctx.push(
            out,
            DimKind::Diameter,
            vec![Geo3::Axis {
                origin: frame.origin,
                dir: z,
                radius: *radius,
                t0,
                t1,
            }],
            vec![CandidateOrigin::Face { body, face }],
            Cat::Diameter,
            false,
        );
    }
}

/// Volle Kreise der Ansicht (jede Lage): Kreiskanten der Striche, zusammengefasst nach Mitte,
/// Achse und Radius, deren Winkel zusammen einen vollen Kreis ergeben; je Kreis
/// `(Mitte, Einheitsnormale, Radius)`.
fn full_circles(ctx: &ViewCtx) -> Vec<(DVec3, DVec3, f64)> {
    let mut groups: BTreeMap<String, ((DVec3, DVec3, f64), f64)> = BTreeMap::new();
    for s in &ctx.lines.strokes {
        let StrokeOrigin::Edge { body, edge } = s.origin else {
            continue;
        };
        let Some((_, e)) = edge_of(ctx.bodies, body, edge) else {
            continue;
        };
        let Curve::Circle { frame, radius } = &e.curve else {
            continue;
        };
        let n = frame.z.normalize_or_zero();
        if !(radius.is_finite() && *radius > 0.0 && frame.origin.is_finite()) || n == DVec3::ZERO {
            continue;
        }
        let Some(key) = axis_key("C", frame.origin, n, *radius) else {
            continue;
        };
        let key = format!("{key}|{}", qv(frame.origin));
        groups
            .entry(key)
            .or_insert(((frame.origin, n, *radius), 0.0))
            .1 += (e.t1 - e.t0).abs();
    }
    groups
        .into_values()
        .filter(|(_, turn)| *turn >= std::f64::consts::TAU - FULL_TURN_TOL)
        .map(|(c, _)| c)
        .collect()
}

/// Bezüge an den äußersten Lagen der sichtbaren Linien: `[Richtung x, y][Seite min, max]`.
/// Kanten vor Eckpunkten, sonst der erste Strich in der Reihenfolge der Linien.
fn extreme_refs(
    ctx: &ViewCtx,
    visible: &[bool],
    ends: &[DVec2],
    (lo, hi): (DVec2, DVec2),
) -> [[Option<Ref>; 2]; 2] {
    let basis = &ctx.lines.basis;
    let d = basis.d;
    let near = ctx.near();
    let ext = [[lo.x, hi.x], [lo.y, hi.y]];
    let at_extreme =
        |p: DVec2| (0..2).any(|k| (0..2).any(|side| (p[k] - ext[k][side]).abs() <= near));
    // Nur Enden an einer äußersten Lage zählen; in Zellen der Größe `near` abgelegt, damit
    // die Suche je Eckpunkt nicht alle Enden durchläuft.
    let cell = |p: DVec2| ((p.x / near).floor() as i64, (p.y / near).floor() as i64);
    let mut grid: HashMap<(i64, i64), Vec<DVec2>> = HashMap::new();
    for &p in ends {
        if p.is_finite() && at_extreme(p) {
            grid.entry(cell(p)).or_default().push(p);
        }
    }
    let is_visible_end = |q: DVec2| {
        let (cx, cy) = cell(q);
        (cx - 1..=cx + 1).any(|x| {
            (cy - 1..=cy + 1).any(|y| {
                grid.get(&(x, y))
                    .is_some_and(|v| v.iter().any(|p| p.distance(q) <= near))
            })
        })
    };
    let mut out: [[Option<Ref>; 2]; 2] = Default::default();
    let offer = |out: &mut [[Option<Ref>; 2]; 2], k: usize, at: f64, r: &Ref| {
        for side in 0..2 {
            if (at - ext[k][side]).abs() > near {
                continue;
            }
            let slot = &mut out[k][side];
            if slot.as_ref().is_none_or(|s| r.edge && !s.edge) {
                *slot = Some(r.clone());
            }
        }
    };
    for (i, s) in ctx.lines.strokes.iter().enumerate() {
        if !visible[i] {
            continue;
        }
        let StrokeOrigin::Edge { body, edge } = s.origin else {
            continue;
        };
        let Some((solid, e)) = edge_of(ctx.bodies, body, edge) else {
            continue;
        };
        let origin = CandidateOrigin::Edge { body, edge };
        // Kanten quer zur Messrichtung.
        match &e.curve {
            Curve::Line { .. } => {
                if let (Some(a), Some(b)) = (vertex(solid, e.start), vertex(solid, e.end)) {
                    let v = basis.project(b - a);
                    let len = v.length();
                    let mid = 0.5 * (basis.project(a) + basis.project(b));
                    if len > 16.0 * ctx.tol {
                        let r = Ref {
                            geo: Geo3::Line { a, b },
                            origin,
                            edge: true,
                        };
                        for (k, u) in [DVec2::X, DVec2::Y].into_iter().enumerate() {
                            if v.dot(u).abs() <= ACROSS_TOL * len {
                                offer(&mut out, k, mid.dot(u), &r);
                            }
                        }
                    }
                }
            }
            Curve::Circle { frame, radius } => {
                let n = frame.z.normalize_or_zero();
                let t = basis.project(n).perp().normalize_or_zero();
                if n.dot(d).abs() <= VIEW_TOL
                    && t != DVec2::ZERO
                    && frame.origin.is_finite()
                    && radius.is_finite()
                    && *radius > 0.0
                {
                    let r = Ref {
                        geo: Geo3::Circle {
                            center: frame.origin,
                            normal: frame.z,
                            radius: *radius,
                        },
                        origin,
                        edge: true,
                    };
                    let c = basis.project(frame.origin);
                    for (k, u) in [DVec2::X, DVec2::Y].into_iter().enumerate() {
                        if t.dot(u).abs() <= ACROSS_TOL {
                            offer(&mut out, k, c.dot(u), &r);
                        }
                    }
                }
            }
            _ => {}
        }
        // Eckpunkte, die Ende eines sichtbaren Stücks sind.
        for (end, id) in [(EndSel::Start, e.start), (EndSel::End, e.end)] {
            let Some(p) = vertex(solid, id) else {
                continue;
            };
            let qp = basis.project(p);
            if !(at_extreme(qp) && is_visible_end(qp)) {
                continue;
            }
            let r = Ref {
                geo: Geo3::Point(p),
                origin: CandidateOrigin::EdgeEnd { body, edge, end },
                edge: false,
            };
            for (k, u) in [DVec2::X, DVec2::Y].into_iter().enumerate() {
                offer(&mut out, k, qp.dot(u), &r);
            }
        }
    }
    out
}
