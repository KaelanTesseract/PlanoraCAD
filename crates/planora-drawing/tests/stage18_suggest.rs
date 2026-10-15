//! Stufe 18 (Teilprojekt 12): Vorschlag der Grundmaße. Sollwerte aus den Eingabemaßen der
//! Prüfkörper (Formelmodelle); jeder Vorschlag wird mit `measure` gemessen, seine Bezüge werden
//! mit einer eigenen Auflösung der Herkunft (`common::dim_model::resolve_origin`) verglichen.

mod common;

use common::dim_model::{formula_radii, geo_near, image, resolve_origin};
use common::mesh_ref::fine_mesh;
use common::model::*;
use planora_drawing::dimension::{ChainMode, DimKind, Geo3, measure};
use planora_drawing::model::{Placement, Scale, ViewId, ViewKind, ViewSpec};
use planora_drawing::suggest::{Candidate, CandidateOrigin, suggest};
use planora_drawing::{DrawBody, DrawError, ViewLines, compute_view};
use planora_exchange::Progress;
use planora_kernel::Solid;
use planora_math::{DVec2, DVec3};

/// Toleranz der Werte in den Einzelfällen (Maße in mm, Größenordnung 10).
const TOL: f64 = 1e-9;

fn spec(id: u64, kind: ViewKind) -> ViewSpec {
    ViewSpec {
        id: ViewId(id),
        kind,
        scale: Scale { num: 1, den: 1 },
        placement: Placement::Free { x: 0.0, y: 0.0 },
        show_hidden: true,
        parent: None,
        centerlines: kind.is_normal(),
    }
}

fn bodies_of<'a>(solids: &[&'a Solid]) -> Vec<DrawBody<'a>> {
    solids
        .iter()
        .enumerate()
        .map(|(i, s)| DrawBody {
            name: format!("K{i}"),
            solid: s,
        })
        .collect()
}

/// Ansichten (Kennungen 1, 2, …) und ihre berechneten Linien.
struct Sheet {
    specs: Vec<ViewSpec>,
    lines: Vec<ViewLines>,
}

impl Sheet {
    fn new(solids: &[&Solid], kinds: &[ViewKind]) -> Sheet {
        let bodies = bodies_of(solids);
        let specs: Vec<ViewSpec> = kinds
            .iter()
            .enumerate()
            .map(|(i, k)| spec(i as u64 + 1, *k))
            .collect();
        let lines = kinds
            .iter()
            .map(|k| compute_view(&bodies, *k, &Progress::new()).unwrap())
            .collect();
        Sheet { specs, lines }
    }

    fn views(&self) -> Vec<(ViewSpec, &ViewLines)> {
        self.specs.iter().cloned().zip(self.lines.iter()).collect()
    }

    fn suggest(&self, solids: &[&Solid], existing: &[(ViewId, DimKind, String)]) -> Vec<Candidate> {
        suggest(&bodies_of(solids), &self.views(), existing)
    }

    fn index(&self, id: ViewId) -> usize {
        self.specs.iter().position(|s| s.id == id).unwrap()
    }

    /// Wert eines Vorschlags über `measure` (muss gelingen).
    fn value(&self, c: &Candidate) -> f64 {
        let i = self.index(c.view);
        measure(
            c.kind,
            &c.geos,
            &self.lines[i].basis,
            self.specs[i].kind,
            self.lines[i].extent,
        )
        .unwrap_or_else(|e| panic!("{c:?}: {e}"))
        .value
    }

    fn kind_of(&self, c: &Candidate) -> ViewKind {
        self.specs[self.index(c.view)].kind
    }
}

/// Kurzbeschreibung eines Vorschlags: Ansichtsart, Maßart, Wert, Kettenart.
fn summary(sheet: &Sheet, cs: &[Candidate]) -> Vec<(ViewKind, DimKind, f64, Option<ChainMode>)> {
    cs.iter()
        .map(|c| {
            (
                sheet.kind_of(c),
                c.kind,
                sheet.value(c),
                c.chain.map(|ch| ch.mode),
            )
        })
        .collect()
}

/// Gibt es genau einen Vorschlag mit dieser Ansicht, Art und diesem Wert?
fn count(
    s: &[(ViewKind, DimKind, f64, Option<ChainMode>)],
    view: ViewKind,
    kind: DimKind,
    value: f64,
) -> usize {
    s.iter()
        .filter(|(v, k, x, _)| *v == view && *k == kind && (x - value).abs() <= TOL)
        .count()
}

/// Jeder Vorschlag: Bezüge stimmen mit der eigenen Auflösung der Herkunft überein.
fn assert_origins_match(solids: &[&Solid], cs: &[Candidate], tol: f64) {
    for c in cs {
        assert_eq!(c.geos.len(), c.origins.len(), "{c:?}");
        for (g, o) in c.geos.iter().zip(&c.origins) {
            let own = resolve_origin(solids, o).unwrap_or_else(|| panic!("{o:?} nicht auflösbar"));
            assert!(geo_near(g, &own, tol), "{c:?}: {g:?} gegen {own:?}");
        }
    }
}

#[test]
fn box_gets_three_overall_dimensions_over_three_views() {
    // Quader 10 (x) × 20 (y) × 30 (z): Vorne Breite 10 und Höhe 30, Oben die Tiefe 20 (in der
    // Draufsicht senkrecht); die Breite nicht noch einmal oben, Tiefe und Höhe nicht links.
    let s = box_solid(10.0, 20.0, 30.0);
    let solids = [&s];
    let sheet = Sheet::new(&solids, &[ViewKind::Front, ViewKind::Top, ViewKind::Left]);
    let cs = sheet.suggest(&solids, &[]);
    let sm = summary(&sheet, &cs);
    assert_eq!(sm.len(), 3, "{sm:?}");
    assert_eq!(
        count(&sm, ViewKind::Front, DimKind::Horizontal, 10.0),
        1,
        "{sm:?}"
    );
    assert_eq!(
        count(&sm, ViewKind::Front, DimKind::Vertical, 30.0),
        1,
        "{sm:?}"
    );
    assert_eq!(
        count(&sm, ViewKind::Top, DimKind::Vertical, 20.0),
        1,
        "{sm:?}"
    );
    assert!(cs.iter().all(|c| c.chain.is_none()), "{cs:?}");
    // Gesamtmaße beziehen sich auf Kanten.
    for c in &cs {
        assert!(
            c.origins
                .iter()
                .all(|o| matches!(o, CandidateOrigin::Edge { body: 0, .. })),
            "{c:?}"
        );
    }
    assert_origins_match(&solids, &cs, 1e-9);
    // Ohne Vorderansicht: Breite und Höhe wandern nach oben bzw. links.
    let sheet = Sheet::new(&solids, &[ViewKind::Top, ViewKind::Left]);
    let sm = summary(&sheet, &sheet.suggest(&solids, &[]));
    assert_eq!(sm.len(), 3, "{sm:?}");
    assert_eq!(
        count(&sm, ViewKind::Top, DimKind::Horizontal, 10.0),
        1,
        "{sm:?}"
    );
    assert_eq!(
        count(&sm, ViewKind::Top, DimKind::Vertical, 20.0),
        1,
        "{sm:?}"
    );
    assert_eq!(
        count(&sm, ViewKind::Left, DimKind::Vertical, 30.0),
        1,
        "{sm:?}"
    );
}

#[test]
fn cylinder_gets_diameter_in_top_view_and_height_in_front_view() {
    // Zylinder r = 5, h = 12: Ø10 von oben (als Kreis), Höhe 12 von vorn; das Ø nicht noch einmal
    // von vorn oder links (dort nur als Umrisspaar), keine Breite ohne tragfähige Bezüge.
    let s = cylinder_solid(5.0, 12.0);
    let solids = [&s];
    let sheet = Sheet::new(&solids, &[ViewKind::Front, ViewKind::Top, ViewKind::Left]);
    let cs = sheet.suggest(&solids, &[]);
    let sm = summary(&sheet, &cs);
    assert_eq!(sm.len(), 2, "{sm:?}");
    assert_eq!(
        count(&sm, ViewKind::Top, DimKind::Diameter, 10.0),
        1,
        "{sm:?}"
    );
    assert_eq!(
        count(&sm, ViewKind::Front, DimKind::Vertical, 12.0),
        1,
        "{sm:?}"
    );
    let d = cs.iter().find(|c| c.kind == DimKind::Diameter).unwrap();
    assert!(matches!(d.geos[0], Geo3::Circle { .. }), "{d:?}");
    assert_origins_match(&solids, &cs, 1e-9);
    // Nur die Vorderansicht: dort das Ø quer über die Umrisslinien (Zylinderfläche).
    let sheet = Sheet::new(&solids, &[ViewKind::Front]);
    let cs = sheet.suggest(&solids, &[]);
    let sm = summary(&sheet, &cs);
    assert_eq!(sm.len(), 2, "{sm:?}");
    assert_eq!(
        count(&sm, ViewKind::Front, DimKind::Diameter, 10.0),
        1,
        "{sm:?}"
    );
    assert_eq!(
        count(&sm, ViewKind::Front, DimKind::Vertical, 12.0),
        1,
        "{sm:?}"
    );
    let d = cs.iter().find(|c| c.kind == DimKind::Diameter).unwrap();
    assert!(
        matches!(d.origins[0], CandidateOrigin::Face { body: 0, .. })
            && matches!(d.geos[0], Geo3::Axis { .. }),
        "{d:?}"
    );
    assert_origins_match(&solids, &cs, 1e-6);
}

#[test]
fn hole_diameter_and_position_from_left_bottom_edge() {
    // Quader 40 × 20 × 30 mit Ø10-Loch längs Y bei x = 20, z = 15: vorn Ø10, die Lage als
    // Bezugsmaße von der linken Kante (20) und der unteren Kante (15); dazu Breite 40, Höhe 30
    // vorn und Tiefe 20 oben.
    let s = box_with_hole(40.0, 20.0, 30.0, 5.0, DVec3::Y);
    let solids = [&s];
    let sheet = Sheet::new(&solids, &[ViewKind::Front, ViewKind::Top, ViewKind::Left]);
    let cs = sheet.suggest(&solids, &[]);
    let sm = summary(&sheet, &cs);
    assert_eq!(sm.len(), 6, "{sm:?}");
    assert_eq!(
        count(&sm, ViewKind::Front, DimKind::Diameter, 10.0),
        1,
        "{sm:?}"
    );
    assert_eq!(
        count(&sm, ViewKind::Front, DimKind::Horizontal, 40.0),
        1,
        "{sm:?}"
    );
    assert_eq!(
        count(&sm, ViewKind::Front, DimKind::Vertical, 30.0),
        1,
        "{sm:?}"
    );
    assert_eq!(
        count(&sm, ViewKind::Top, DimKind::Vertical, 20.0),
        1,
        "{sm:?}"
    );
    for (kind, value) in [(DimKind::Horizontal, 20.0), (DimKind::Vertical, 15.0)] {
        let c: Vec<&Candidate> = cs
            .iter()
            .filter(|c| c.kind == kind && (sheet.value(c) - value).abs() <= TOL)
            .collect();
        assert_eq!(c.len(), 1, "{kind:?} {value}: {cs:?}");
        let c = c[0];
        assert_eq!(sheet.kind_of(c), ViewKind::Front);
        let chain = c.chain.expect("Bezugsmaß");
        assert_eq!((chain.mode, chain.order), (ChainMode::Baseline, 0), "{c:?}");
        // Erster Bezug: die linke bzw. untere Kante (x = 0 bzw. z = 0), zweiter: die Mitte.
        assert!(
            matches!(c.origins[0], CandidateOrigin::Edge { .. }),
            "{c:?}"
        );
        assert!(
            matches!(c.origins[1], CandidateOrigin::Center { .. }),
            "{c:?}"
        );
        match (&c.geos[0], &c.geos[1]) {
            (Geo3::Line { a, b }, Geo3::Point(p)) => {
                if kind == DimKind::Horizontal {
                    assert!(a.x.abs() <= TOL && b.x.abs() <= TOL, "{c:?}");
                } else {
                    assert!(a.z.abs() <= TOL && b.z.abs() <= TOL, "{c:?}");
                }
                assert!(
                    (p.x - 20.0).abs() <= TOL && (p.z - 15.0).abs() <= TOL,
                    "{p:?}"
                );
            }
            other => panic!("{other:?}"),
        }
    }
    // Die beiden Bezugsmaße bilden zwei Gruppen (je Richtung eine).
    let groups: Vec<u64> = cs.iter().filter_map(|c| c.chain.map(|g| g.group)).collect();
    assert_eq!(groups.len(), 2);
    assert_ne!(groups[0], groups[1]);
    assert_origins_match(&solids, &cs, 1e-9);
}

#[test]
fn filleted_box_gets_radii() {
    // Quader 30 × 20 × 10 mit vier senkrechten Rundungen r = 4: von oben vier R4 um (4, 4),
    // (26, 4), (4, 16), (26, 16); kein Ø (keine vollen Kreise); von vorn keine R (die Rundungen
    // erscheinen dort nicht als Bogen).
    let s = rounded_box(30.0, 20.0, 10.0, 4.0);
    let solids = [&s];
    let sheet = Sheet::new(&solids, &[ViewKind::Front, ViewKind::Top]);
    let cs = sheet.suggest(&solids, &[]);
    let radii: Vec<&Candidate> = cs.iter().filter(|c| c.kind == DimKind::Radius).collect();
    assert_eq!(radii.len(), 4, "{cs:?}");
    let mut centers: Vec<DVec2> = Vec::new();
    for c in &radii {
        assert_eq!(sheet.kind_of(c), ViewKind::Top);
        assert!((sheet.value(c) - 4.0).abs() <= TOL, "{c:?}");
        match &c.geos[0] {
            Geo3::Circle { center, .. } => centers.push(DVec2::new(center.x, center.y)),
            other => panic!("{other:?}"),
        }
    }
    for want in [(4.0, 4.0), (26.0, 4.0), (4.0, 16.0), (26.0, 16.0)] {
        let w = DVec2::new(want.0, want.1);
        assert_eq!(
            centers.iter().filter(|c| c.distance(w) <= 1e-9).count(),
            1,
            "{want:?}: {centers:?}"
        );
    }
    assert!(cs.iter().all(|c| c.kind != DimKind::Diameter), "{cs:?}");
    // Gesamtmaße oben: Breite 30 (vorn) und Tiefe 20 (oben), Höhe 10 vorn.
    let sm = summary(&sheet, &cs);
    assert_eq!(
        count(&sm, ViewKind::Top, DimKind::Vertical, 20.0),
        1,
        "{sm:?}"
    );
    assert_eq!(
        count(&sm, ViewKind::Front, DimKind::Vertical, 10.0),
        1,
        "{sm:?}"
    );
    assert_origins_match(&solids, &cs, 1e-9);
}

#[test]
fn existing_dimensions_are_not_duplicated() {
    let s = box_with_hole(40.0, 20.0, 30.0, 5.0, DVec3::Y);
    let solids = [&s];
    let sheet = Sheet::new(&solids, &[ViewKind::Front, ViewKind::Top, ViewKind::Left]);
    let first = sheet.suggest(&solids, &[]);
    assert_eq!(first.len(), 6);
    let existing: Vec<(ViewId, DimKind, String)> = first
        .iter()
        .map(|c| (c.view, c.kind, c.key.clone()))
        .collect();
    assert!(sheet.suggest(&solids, &existing).is_empty());
    // Nur das Ø vorhanden: alles andere wird noch vorgeschlagen, das Ø nicht.
    let d = first.iter().find(|c| c.kind == DimKind::Diameter).unwrap();
    let again = sheet.suggest(&solids, &[(d.view, d.kind, d.key.clone())]);
    assert_eq!(again.len(), 5);
    assert!(again.iter().all(|c| c.key != d.key));
    // Der Schlüssel eines vorhandenen Maßes lässt sich aus seinen Bezügen bilden.
    for c in &first {
        let i = sheet.index(c.view);
        let key = planora_drawing::dimension_key(
            c.kind,
            &c.geos,
            &sheet.lines[i].basis,
            sheet.specs[i].kind,
            sheet.lines[i].extent,
        );
        assert_eq!(key.as_deref(), Some(c.key.as_str()), "{c:?}");
    }
}

#[test]
fn isometric_gets_nothing() {
    let s = box_with_hole(40.0, 20.0, 30.0, 5.0, DVec3::Y);
    let solids = [&s];
    let sheet = Sheet::new(&solids, &[ViewKind::Isometric]);
    assert!(sheet.suggest(&solids, &[]).is_empty());
    let sheet = Sheet::new(&solids, &[ViewKind::Isometric, ViewKind::Front]);
    let cs = sheet.suggest(&solids, &[]);
    assert!(!cs.is_empty());
    assert!(
        cs.iter().all(|c| sheet.kind_of(c) == ViewKind::Front),
        "{cs:?}"
    );
}

#[test]
fn empty_and_mesh_only_give_empty_list() {
    assert!(suggest(&[], &[], &[]).is_empty());
    // Ansichten ohne exakte Körper (etwa ein reines Netz-Dokument mit alten Ergebnissen): keine
    // Vorschläge, kein Absturz.
    let s = box_with_hole(40.0, 20.0, 30.0, 5.0, DVec3::Y);
    let sheet = Sheet::new(&[&s], &[ViewKind::Front, ViewKind::Top]);
    assert!(suggest(&[], &sheet.views(), &[]).is_empty());
    // Körper, der nicht zu den Linien passt (weniger Kanten): kein Absturz, nur gültige Bezüge.
    let small = box_solid(1.0, 1.0, 1.0);
    let cs = suggest(&bodies_of(&[&small]), &sheet.views(), &[]);
    for c in &cs {
        for o in &c.origins {
            assert!(resolve_origin(&[&small], o).is_some(), "{c:?}");
        }
    }
    // Ansichtsart passt nicht zu den Linien: übergangen.
    let mut wrong = sheet.specs[0].clone();
    wrong.kind = ViewKind::Left;
    assert!(suggest(&bodies_of(&[&s]), &[(wrong, &sheet.lines[0])], &[]).is_empty());
}

#[test]
fn sphere_gets_one_diameter() {
    let s = sphere_solid(4.0);
    let solids = [&s];
    let sheet = Sheet::new(
        &solids,
        &[
            ViewKind::Front,
            ViewKind::Top,
            ViewKind::Left,
            ViewKind::Bottom,
        ],
    );
    let cs = sheet.suggest(&solids, &[]);
    let sm = summary(&sheet, &cs);
    assert_eq!(sm.len(), 1, "{sm:?}");
    assert_eq!(
        count(&sm, ViewKind::Front, DimKind::Diameter, 8.0),
        1,
        "{sm:?}"
    );
    assert!(matches!(
        cs[0].origins[0],
        CandidateOrigin::Face { body: 0, .. }
    ));
    assert_origins_match(&solids, &cs, 1e-9);
}

#[test]
fn baseline_order_increases_with_distance() {
    // Platte 60 (x) × 10 (y) × 40 (z) mit Ø4-Löchern längs Y bei (x, z) = (50, 10), (10, 30),
    // (30, 20): vorn Bezugsmaße von links 10, 30, 50 (Reihenfolge 0, 1, 2) und von unten
    // 10, 20, 30; die Reihenfolge folgt dem Abstand, nicht der Erzeugung.
    let mut s = box_solid(60.0, 10.0, 40.0);
    for (x, z) in [(50.0, 10.0), (10.0, 30.0), (30.0, 20.0)] {
        let drill = axis_cylinder(2.0, 20.0, DVec3::Y, DVec3::new(x, 5.0, z));
        s = planora_kernel::boolean::boolean(
            &s,
            &drill,
            planora_kernel::BoolOp::Difference,
            &planora_math::Tolerance::default(),
        )
        .unwrap();
    }
    let solids = [&s];
    let sheet = Sheet::new(&solids, &[ViewKind::Front, ViewKind::Top]);
    let cs = sheet.suggest(&solids, &[]);
    for (kind, want) in [
        (DimKind::Horizontal, [10.0, 30.0, 50.0]),
        (DimKind::Vertical, [10.0, 20.0, 30.0]),
    ] {
        let mut members: Vec<(u32, f64, u64)> = cs
            .iter()
            .filter(|c| c.kind == kind && c.chain.is_some())
            .map(|c| {
                let ch = c.chain.unwrap();
                assert_eq!(ch.mode, ChainMode::Baseline);
                assert_eq!(sheet.kind_of(c), ViewKind::Front);
                (ch.order, sheet.value(c), ch.group)
            })
            .collect();
        members.sort_by_key(|m| m.0);
        assert_eq!(members.len(), 3, "{kind:?}: {members:?}");
        for (i, (order, value, group)) in members.iter().enumerate() {
            assert_eq!(*order, i as u32);
            assert!((value - want[i]).abs() <= TOL, "{kind:?}: {members:?}");
            assert_eq!(*group, members[0].2);
        }
    }
    let sm = summary(&sheet, &cs);
    assert_eq!(
        count(&sm, ViewKind::Front, DimKind::Diameter, 4.0),
        3,
        "{sm:?}"
    );
    // Reihenfolge der Liste: deterministisch, Bezugsmaße nach `order`.
    assert_eq!(cs, sheet.suggest(&solids, &[]));
    assert_origins_match(&solids, &cs, 1e-9);
}

// --- Nachbesserung 1 ---------------------------------------------------------------------

#[test]
fn many_strokes_with_corner_vertices_stay_fast() {
    // Die Linien der Vorderansicht eines Quaders 10 × 20 × 30, `COPIES`-mal wiederholt; von jedem
    // Strich ist nur die mittlere Hälfte sichtbar. Die Eckpunkte liegen an den äußersten Lagen,
    // sind aber nie Ende eines sichtbaren Stücks: Ein Vergleich jedes Eckpunkts mit allen Enden
    // wäre quadratisch (hier über 10⁹ Vergleiche, im Prüfbau weit über der Zeitgrenze).
    const COPIES: usize = 5000;
    let s = box_solid(10.0, 20.0, 30.0);
    let solids = [&s];
    let sheet = Sheet::new(&solids, &[ViewKind::Front]);
    let base = sheet.lines[0].clone();
    let mut lines = base.clone();
    lines.strokes.clear();
    lines.pieces.clear();
    for _ in 0..COPIES {
        for st in &base.strokes {
            let (a, b) = (st.t0, st.t1);
            lines.pieces.push(planora_drawing::Piece {
                stroke: lines.strokes.len(),
                t0: a + 0.25 * (b - a),
                t1: a + 0.75 * (b - a),
                vis: planora_drawing::Visibility::Visible,
            });
            lines.strokes.push(st.clone());
        }
    }
    assert!(lines.strokes.len() >= 4 * COPIES, "{}", lines.strokes.len());
    let t = std::time::Instant::now();
    let cs = suggest(
        &bodies_of(&solids),
        &[(sheet.specs[0].clone(), &lines)],
        &[],
    );
    let took = t.elapsed();
    // Sichtbar: die mittleren Hälften; sie reichen quer bis an die äußersten Lagen (x = 0, 10
    // und z = 0, 30): Breite 10 und Höhe 30 an Kanten.
    let sm: Vec<(DimKind, f64)> = cs
        .iter()
        .map(|c| {
            let m = measure(c.kind, &c.geos, &lines.basis, ViewKind::Front, lines.extent);
            (c.kind, m.unwrap().value)
        })
        .collect();
    assert_eq!(sm.len(), 2, "{sm:?}");
    assert_eq!((sm[0].0, sm[1].0), (DimKind::Horizontal, DimKind::Vertical));
    assert!(
        (sm[0].1 - 10.0).abs() <= TOL && (sm[1].1 - 30.0).abs() <= TOL,
        "{sm:?}"
    );
    assert!(
        cs.iter().all(|c| c
            .origins
            .iter()
            .all(|o| matches!(o, CandidateOrigin::Edge { .. }))),
        "{cs:?}"
    );
    eprintln!("suggest: {took:?}");
    // Vorher (Vergleich mit allen Enden) im Prüfbau 44 s, jetzt 0,2 s; Grenze mit viel Luft.
    assert!(took.as_secs_f64() < 10.0, "{took:?}");
}

#[test]
fn diameter_origin_prefers_a_visible_edge() {
    // Zweimal derselbe Körper; vor die Linien der Vorderansicht kommt je Kreiskante eine verdeckte
    // Kopie mit Herkunft im zweiten Körper. Die Gruppe (gleiche Mitte, gleicher Radius) muss die
    // sichtbare Kante des ersten Körpers als Herkunft nehmen.
    let s = box_with_hole(40.0, 20.0, 30.0, 5.0, DVec3::Y);
    let solids = [&s, &s];
    let sheet = Sheet::new(&[&s], &[ViewKind::Front]);
    let base = &sheet.lines[0];
    let mut lines = base.clone();
    let hidden: Vec<_> = base
        .strokes
        .iter()
        .filter(|st| matches!(st.curve, planora_kernel::Curve::Circle { .. }))
        .map(|st| {
            let mut c = st.clone();
            if let planora_drawing::stroke::StrokeOrigin::Edge { edge, .. } = st.origin {
                c.origin = planora_drawing::stroke::StrokeOrigin::Edge { body: 1, edge };
            }
            c
        })
        .collect();
    assert!(!hidden.is_empty());
    let shift = hidden.len();
    lines.strokes = hidden
        .iter()
        .cloned()
        .chain(base.strokes.iter().cloned())
        .collect();
    lines.pieces = hidden
        .iter()
        .enumerate()
        .map(|(i, st)| planora_drawing::Piece {
            stroke: i,
            t0: st.t0,
            t1: st.t1,
            vis: planora_drawing::Visibility::Hidden,
        })
        .chain(base.pieces.iter().map(|p| planora_drawing::Piece {
            stroke: p.stroke + shift,
            ..*p
        }))
        .collect();
    let cs = suggest(
        &bodies_of(&solids),
        &[(sheet.specs[0].clone(), &lines)],
        &[],
    );
    let d: Vec<&Candidate> = cs.iter().filter(|c| c.kind == DimKind::Diameter).collect();
    assert_eq!(d.len(), 1, "{cs:?}");
    assert!(
        matches!(d[0].origins[0], CandidateOrigin::Edge { body: 0, .. }),
        "{:?}",
        d[0]
    );
    for c in cs.iter().filter(|c| c.chain.is_some()) {
        assert!(
            matches!(c.origins[1], CandidateOrigin::Center { body: 0, .. }),
            "{c:?}"
        );
    }
}

#[test]
fn circle_edges_with_broken_radius_are_no_reference() {
    // Zylinder r = 5, h = 12, dessen Kreiskanten den Radius NaN tragen (Linien vom heilen
    // Zylinder): die Kreise taugen nicht als Bezug der Höhe, die Höhe hängt an den Eckpunkten
    // der Naht; kein Absturz, kein Vorschlag mit kaputten Zahlen.
    let good = cylinder_solid(5.0, 12.0);
    let bad = cylinder_with_bad_numbers(f64::NAN, 5.0);
    let sheet = Sheet::new(&[&good], &[ViewKind::Front]);
    let cs = sheet.suggest(&[&bad], &[]);
    let v: Vec<&Candidate> = cs.iter().filter(|c| c.kind == DimKind::Vertical).collect();
    assert_eq!(v.len(), 1, "{cs:?}");
    assert!((sheet.value(v[0]) - 12.0).abs() <= TOL);
    assert!(
        v[0].origins
            .iter()
            .all(|o| matches!(o, CandidateOrigin::EdgeEnd { .. })),
        "{:?}",
        v[0]
    );
    // Ø nur über die (heile) Zylinderfläche, nie über die kaputten Kreise.
    assert!(
        cs.iter()
            .filter(|c| c.kind == DimKind::Diameter)
            .all(|c| matches!(c.geos[0], Geo3::Axis { .. })),
        "{cs:?}"
    );
}

/// Zylinder (Radius `r`, Höhe `h`, Achse Z) aus zwei Halbkreisbögen: Eckpunkte bei 0° und 180°
/// auf Boden- und Deckelkreis.
fn two_arc_cylinder(r: f64, h: f64) -> Solid {
    use planora_kernel::{Profile, ProfileLoop, Segment};
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::new(r, 0.0),
            segments: vec![
                Segment::Arc {
                    center: DVec2::ZERO,
                    sweep: std::f64::consts::PI,
                },
                Segment::Arc {
                    center: DVec2::ZERO,
                    sweep: std::f64::consts::PI,
                },
            ],
        },
        holes: vec![],
    };
    planora_kernel::extrude(
        &profile,
        &planora_math::Frame::WORLD,
        h,
        &planora_math::Tolerance::default(),
    )
    .unwrap()
}

#[test]
fn vertices_on_one_circle_give_no_second_diameter() {
    // Zylinder r = 5, h = 12 aus zwei Halbkreisen: Die Eckpunkte bei x = ±5 sind die äußersten
    // Lagen von oben und von vorn; ein Gesamtmaß 10 zwischen ihnen wäre das Ø noch einmal.
    let s = two_arc_cylinder(5.0, 12.0);
    let solids = [&s];
    let sheet = Sheet::new(&solids, &[ViewKind::Front, ViewKind::Top]);
    let cs = sheet.suggest(&solids, &[]);
    let sm = summary(&sheet, &cs);
    assert_eq!(sm.len(), 2, "{sm:?}");
    assert_eq!(
        count(&sm, ViewKind::Top, DimKind::Diameter, 10.0),
        1,
        "{sm:?}"
    );
    assert_eq!(
        count(&sm, ViewKind::Front, DimKind::Vertical, 12.0),
        1,
        "{sm:?}"
    );
    assert_origins_match(&solids, &cs, 1e-9);
}

// --- Prüfreihe ---------------------------------------------------------------------------

/// Bildlage entlang der Messrichtung eines linearen Maßes in der Normansicht `kind` (eigene
/// Achsentabelle): waagerecht `image(p).x`, senkrecht `image(p).y`.
fn along(kind: ViewKind, dim: DimKind, p: DVec3) -> f64 {
    let q = image(kind, p);
    if dim == DimKind::Horizontal { q.x } else { q.y }
}

#[derive(Default, Debug)]
struct SeriesCounts {
    overall: usize,
    round: usize,
    baseline: usize,
    views: usize,
    named: usize,
}

/// Prüfreihe: Zufallskörper `seeds` (wie TP11, `random_body_csg`); gerade Startwerte mit
/// Vorne/Oben/Links, ungerade mit Hinten/Unten/Rechts. Jeder Vorschlag wird gemessen (`measure`
/// gelingt); Gesamtmaße = Ausdehnung des feinen Referenznetzes längs der Messrichtung (die Bezüge
/// sind Kanten oder Ecken, deren Punkte im Netz liegen); Bezugsmaße = Abstand einer bekannten
/// Mitte (Achse eines Zylinders/Kegels längs der Blickrichtung oder Kugelmitte) vom linken bzw.
/// unteren Rand des Netzes; Ø/R = 2r bzw. r eines Radius aus den Formeln des Körpers. Kein
/// Schlüssel doppelt, Bezüge passen zur eigenen Auflösung der Herkunft, zweimal dasselbe Ergebnis.
fn series(seeds: std::ops::Range<u64>) -> SeriesCounts {
    let mut n = SeriesCounts::default();
    let mut failures: Vec<String> = Vec::new();
    for seed in seeds {
        let (s, csg) = random_body_csg(seed);
        let solids = [&s];
        let bodies = bodies_of(&solids);
        let kinds = if seed % 2 == 0 {
            [ViewKind::Front, ViewKind::Top, ViewKind::Left]
        } else {
            [ViewKind::Back, ViewKind::Bottom, ViewKind::Right]
        };
        let mut specs = Vec::new();
        let mut lines = Vec::new();
        for (i, k) in kinds.iter().enumerate() {
            match compute_view(&bodies, *k, &Progress::new()) {
                Ok(v) => {
                    specs.push(spec(i as u64 + 1, *k));
                    lines.push(v);
                }
                Err(
                    DrawError::AmbiguousVisibility(_)
                    | DrawError::TooComplex(_)
                    | DrawError::Unsupported(_),
                ) => n.named += 1,
                Err(e) => failures.push(format!("Startwert {seed} {k:?}: {e}")),
            }
        }
        n.views += specs.len();
        let sheet = Sheet { specs, lines };
        let cs = sheet.suggest(&solids, &[]);
        if cs != sheet.suggest(&solids, &[]) {
            failures.push(format!("Startwert {seed}: nicht deterministisch"));
        }
        let mut keys: Vec<&str> = cs.iter().map(|c| c.key.as_str()).collect();
        keys.sort_unstable();
        if keys.windows(2).any(|w| w[0] == w[1]) {
            failures.push(format!("Startwert {seed}: Schlüssel doppelt {keys:?}"));
        }
        if cs.is_empty() {
            continue;
        }
        let extent = sheet.lines[0].extent;
        let mesh = fine_mesh(&s, extent);
        let radii = formula_radii(&csg);
        let centers = prim_centers(&csg);
        let tol = 1e-6 * extent;
        for c in &cs {
            let kind = sheet.kind_of(c);
            let i = sheet.index(c.view);
            let value = match measure(
                c.kind,
                &c.geos,
                &sheet.lines[i].basis,
                kind,
                sheet.lines[i].extent,
            ) {
                Ok(m) => m.value,
                Err(e) => {
                    failures.push(format!("Startwert {seed} {kind:?}: {c:?}: {e}"));
                    continue;
                }
            };
            for (g, o) in c.geos.iter().zip(&c.origins) {
                match resolve_origin(&solids, o) {
                    Some(own) if geo_near(g, &own, 1e-5 * extent) => {}
                    own => failures.push(format!(
                        "Startwert {seed} {kind:?}: Bezug {g:?} gegen eigene Auflösung {own:?}"
                    )),
                }
            }
            let ok = match (c.kind, c.chain) {
                (DimKind::Horizontal | DimKind::Vertical, None) => {
                    n.overall += 1;
                    let (lo, hi) = mesh.positions.iter().fold(
                        (f64::INFINITY, f64::NEG_INFINITY),
                        |(lo, hi), p| {
                            let x = along(kind, c.kind, *p);
                            (lo.min(x), hi.max(x))
                        },
                    );
                    (value - (hi - lo)).abs() <= tol
                }
                (DimKind::Horizontal | DimKind::Vertical, Some(_)) => {
                    n.baseline += 1;
                    let lo = mesh
                        .positions
                        .iter()
                        .map(|p| along(kind, c.kind, *p))
                        .fold(f64::INFINITY, f64::min);
                    let depth = match kind {
                        ViewKind::Front | ViewKind::Back => 1,
                        ViewKind::Left | ViewKind::Right => 0,
                        _ => 2,
                    };
                    centers.iter().any(|pc| {
                        let p = match *pc {
                            PrimCenter::Axis { base, axis, .. } if axis == depth => base,
                            PrimCenter::Point { c } => c,
                            _ => return false,
                        };
                        (value - (along(kind, c.kind, p) - lo)).abs() <= tol
                    })
                }
                (DimKind::Diameter | DimKind::Radius, _) => {
                    n.round += 1;
                    let r = if c.kind == DimKind::Diameter {
                        value / 2.0
                    } else {
                        value
                    };
                    radii.iter().any(|x| (x - r).abs() <= tol)
                }
                _ => false,
            };
            if !ok {
                failures.push(format!(
                    "Startwert {seed} {kind:?}: Wert {value} passt zu keiner Formel: {c:?}"
                ));
            }
        }
    }
    for f in &failures {
        eprintln!("FEHLER {f}");
    }
    assert!(
        failures.is_empty(),
        "{} Fehler: {failures:#?}",
        failures.len()
    );
    eprintln!("{n:?}");
    // Benannte Meldungen höchstens 10 % der Ansichten (wie TP11).
    assert!(n.named * 10 <= n.views + n.named, "{n:?}");
    n
}

// 200 Körper in vier Teilreihen zu 50 Startwerten (parallel, Fehler genau zuzuordnen).
#[test]
fn random_series_no_panic_values_match_formulas() {
    let n = series(0..50);
    assert!(n.overall > 20 && n.round > 10 && n.baseline > 2, "{n:?}");
}
#[test]
fn random_series_no_panic_values_match_formulas_050_100() {
    series(50..100);
}
#[test]
fn random_series_no_panic_values_match_formulas_100_150() {
    series(100..150);
}
#[test]
fn random_series_no_panic_values_match_formulas_150_200() {
    let n = series(150..200);
    assert!(n.overall > 20 && n.round > 10 && n.baseline > 2, "{n:?}");
}
