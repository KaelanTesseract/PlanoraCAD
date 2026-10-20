//! Maße im Dokument (Teilprojekt 12, Aufgabe 8): Hinzufügen, Ändern, Löschen, Prüfung,
//! Snapshot und Auflösung gegen die Körper.

use planora_drawing::dimension::{
    ChainMode, ChainSpec, DimError, DimId, DimKind, DimSpec, EndSel, GeoRef, GeoRefData, TagData,
    Tolerance as DimTolerance,
};
use planora_drawing::limits::MAX_DIMENSIONS;
use planora_drawing::{
    Drawing, DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind,
};
use planora_feature::*;
use planora_kernel::{Curve, Edge, Id, Solid};
use planora_math::{DVec3, Tolerance};

const A4: Sheet = Sheet {
    paper: Paper::A4,
    landscape: false,
};
const S11: Scale = Scale { num: 1, den: 1 };
const FREE: Placement = Placement::Free { x: 0.0, y: 0.0 };
const ALIGNED: Placement = Placement::Aligned { distance: 50.0 };
const EPS: f64 = 1e-9;

fn invalid(r: Result<impl std::fmt::Debug, FeatureError>) -> String {
    match r {
        Err(FeatureError::InvalidInput(m)) => m,
        other => panic!("InvalidInput erwartet, bekam {other:?}"),
    }
}

fn tag(role: &str) -> TagData {
    TagData {
        origin: 1,
        role: role.into(),
        hint: DVec3::ZERO,
    }
}

/// Kantendaten ohne Körper (für Prüfungen, die nichts auflösen).
fn edge_data(a: &str, b: &str) -> GeoRefData {
    GeoRefData::Edge {
        a: tag(a),
        b: tag(b),
        hint: DVec3::new(1.0, 2.0, 3.0),
    }
}

fn spec(view: ViewId, kind: DimKind, refs: Vec<GeoRef>) -> DimSpec {
    DimSpec {
        id: DimId(0),
        view,
        kind,
        refs,
        offset: 10.0,
        angle: 45.0,
        text_shift: (0.0, 0.0),
        tolerance: DimTolerance::None,
        decimals: 1,
        text_override: None,
        chain: None,
        suggested: false,
    }
}

/// Ein waagerechtes Maß zwischen zwei Kanten (Bezüge ohne Körper).
fn horizontal(view: ViewId) -> DimSpec {
    spec(
        view,
        DimKind::Horizontal,
        vec![
            GeoRef::Edge(edge_data("Wand 1", "Wand 2")),
            GeoRef::Edge(edge_data("Wand 1", "Wand 4")),
        ],
    )
}

fn doc_with_views() -> (Document, DrawingId, ViewId, ViewId) {
    let mut d = Document::new();
    let id = d.add_drawing("Blatt", A4, Projection::First).unwrap();
    let front = d.add_view(id, ViewKind::Front, S11, FREE, None).unwrap();
    let top = d
        .add_view(id, ViewKind::Top, S11, ALIGNED, Some(front))
        .unwrap();
    (d, id, front, top)
}

fn snapshot_with(drawings: Vec<Drawing>, next_drawing: u64) -> Snapshot {
    Snapshot {
        features: vec![],
        marker: 0,
        next_id: 1,
        meshes: Default::default(),
        drawings,
        next_drawing,
        project_info: None,
        parts_overrides: Default::default(),
        parameters: Vec::new(),
        next_param: 1,
    }
}

/// Zeichnung mit Vorne + Oben und zwei Maßen in Vorne (Kennungen 1, 2).
fn drawing_with_dims() -> Drawing {
    let mut dr = Drawing::new(DrawingId(1), "A".into(), A4, Projection::First);
    let front = dr.add_view(ViewKind::Front, S11, FREE, None);
    dr.add_view(ViewKind::Top, S11, ALIGNED, Some(front));
    dr.add_dimension(horizontal(front));
    dr.add_dimension(horizontal(front));
    dr
}

#[test]
fn add_update_delete_dimension_with_edit_counters() {
    let (mut d, id, front, top) = doc_with_views();
    let (edits, model) = (d.edit_count(), d.model_edit_count());
    let a = d.add_dimension(id, horizontal(front)).unwrap();
    let b = d.add_dimension(id, horizontal(top)).unwrap();
    assert_eq!((a, b), (DimId(1), DimId(2)));
    assert_eq!(d.edit_count(), edits + 2);
    assert_eq!(d.model_edit_count(), model);
    let dr = d.drawing(id).unwrap();
    assert_eq!(dr.dimensions.len(), 2);
    assert_eq!(dr.dimension(b).unwrap().view, top);

    d.update_dimension(id, a, |m| m.offset = 17.5).unwrap();
    assert_eq!(d.drawing(id).unwrap().dimension(a).unwrap().offset, 17.5);
    assert_eq!(d.edit_count(), edits + 3);
    // Keine Änderung zählt nicht.
    d.update_dimension(id, a, |_| {}).unwrap();
    assert_eq!(d.edit_count(), edits + 3);
    // Die Kennung lässt sich nicht ändern; ungültige Änderung lässt alles unverändert.
    let before = d.drawing(id).unwrap().clone();
    invalid(d.update_dimension(id, a, |m| {
        m.offset = 3.0;
        m.id = DimId(9);
    }));
    invalid(d.update_dimension(id, a, |m| m.decimals = 9));
    assert_eq!(d.drawing(id).unwrap(), &before);
    assert_eq!(d.edit_count(), edits + 3);

    d.delete_dimension(id, a).unwrap();
    assert_eq!(d.edit_count(), edits + 4);
    assert!(d.drawing(id).unwrap().dimension(a).is_none());
    // Unbekanntes Maß, unbekannte Zeichnung: Fehler, nichts gezählt.
    invalid(d.delete_dimension(id, a));
    invalid(d.update_dimension(id, a, |m| m.offset = 1.0));
    invalid(d.add_dimension(DrawingId(77), horizontal(front)));
    invalid(d.delete_dimension(DrawingId(77), b));
    assert_eq!(d.edit_count(), edits + 4);
    // Kennungen werden nie wieder vergeben.
    assert_eq!(d.add_dimension(id, horizontal(front)).unwrap(), DimId(3));
    assert_eq!(d.model_edit_count(), model);
}

/// Das Maß muss abgelehnt werden; die Meldung enthält `part`; das Dokument bleibt gleich.
fn refused(d: &mut Document, id: DrawingId, m: DimSpec, part: &str) {
    let before = d.drawing(id).unwrap().clone();
    let edits = d.edit_count();
    let msg = invalid(d.add_dimension(id, m));
    assert!(msg.contains(part), "„{msg}“ enthält nicht „{part}“");
    assert_eq!(d.drawing(id).unwrap(), &before);
    assert_eq!(d.edit_count(), edits);
}

#[test]
fn validation_refuses_isometric_view_and_unknown_view() {
    let (mut d, id, front, _) = doc_with_views();
    let iso = d
        .add_view(id, ViewKind::Isometric, S11, FREE, None)
        .unwrap();
    refused(&mut d, id, horizontal(iso), "isometrischen");
    refused(&mut d, id, horizontal(ViewId(99)), "keiner Ansicht");
    // Auch im Snapshot.
    let mut dr = drawing_with_dims();
    let iso = dr.add_view(ViewKind::Isometric, S11, FREE, None);
    dr.dimensions[1].view = iso;
    invalid(Document::from_snapshot(snapshot_with(vec![dr], 2), 0.05));
    d.add_dimension(id, horizontal(front)).unwrap();
}

#[test]
fn validation_refuses_wrong_reference_count() {
    let (mut d, id, front, _) = doc_with_views();
    let mut one = horizontal(front);
    one.refs.pop();
    refused(&mut d, id, one, "Zahl von Bezügen");
    let mut three = horizontal(front);
    three.refs.push(three.refs[0].clone());
    refused(&mut d, id, three, "Zahl von Bezügen");
    let two_for_diameter = spec(front, DimKind::Diameter, horizontal(front).refs);
    refused(&mut d, id, two_for_diameter, "Zahl von Bezügen");
    // Flächendaten in einem Kantenbezug und umgekehrt.
    let face = GeoRefData::Face(tag("Mantel"));
    let mut wrong = horizontal(front);
    wrong.refs[0] = GeoRef::Edge(face.clone());
    refused(&mut d, id, wrong, "Bezug");
    refused(
        &mut d,
        id,
        spec(
            front,
            DimKind::Diameter,
            vec![GeoRef::Face(edge_data("a", "b"))],
        ),
        "Bezug",
    );
    let mut nan_hint = horizontal(front);
    nan_hint.refs[1] = GeoRef::EdgeEnd {
        edge: GeoRefData::Edge {
            a: tag("a"),
            b: tag("b"),
            hint: DVec3::new(f64::NAN, 0.0, 0.0),
        },
        end: EndSel::End,
    };
    refused(&mut d, id, nan_hint, "Bezug");
    // Passend: ein Ø mit Flächenbezug.
    d.add_dimension(id, spec(front, DimKind::Diameter, vec![GeoRef::Face(face)]))
        .unwrap();
}

#[test]
fn validation_refuses_bad_numbers() {
    let (mut d, id, front, _) = doc_with_views();
    for off in [f64::NAN, f64::INFINITY, 1.0e5 + 1.0, -1.0e9] {
        let mut m = horizontal(front);
        m.offset = off;
        refused(&mut d, id, m, "Maßlinie");
    }
    let mut m = horizontal(front);
    m.angle = f64::NAN;
    refused(&mut d, id, m, "Winkel");
    let mut m = horizontal(front);
    m.text_shift = (0.0, f64::NEG_INFINITY);
    refused(&mut d, id, m, "Verschiebung");
    for t in [
        DimTolerance::Symmetric(f64::NAN),
        DimTolerance::Symmetric(1000.5),
        DimTolerance::Limits {
            upper: 0.1,
            lower: f64::INFINITY,
        },
        DimTolerance::LimitSizes {
            upper: -1001.0,
            lower: 0.0,
        },
    ] {
        let mut m = horizontal(front);
        m.tolerance = t;
        refused(&mut d, id, m, "Toleranz");
    }
    // Grenzen selbst sind erlaubt.
    let mut m = horizontal(front);
    m.offset = -1.0e5;
    m.tolerance = DimTolerance::Limits {
        upper: 1000.0,
        lower: -1000.0,
    };
    d.add_dimension(id, m).unwrap();
}

#[test]
fn validation_refuses_four_decimals_and_long_texts() {
    let (mut d, id, front, _) = doc_with_views();
    let mut m = horizontal(front);
    m.decimals = 4;
    refused(&mut d, id, m, "Nachkommastellen");
    let mut m = horizontal(front);
    m.text_override = Some("x".repeat(201));
    refused(&mut d, id, m, "200");
    let mut m = horizontal(front);
    m.tolerance = DimTolerance::Fit("H".repeat(51));
    refused(&mut d, id, m, "50");
    // Grenzen: 3 Stellen, 200 Zeichen (Umlaute zählen als eins), 50 Zeichen Passung.
    let mut m = horizontal(front);
    m.decimals = 3;
    m.text_override = Some("ä".repeat(200));
    d.add_dimension(id, m).unwrap();
    let mut m = horizontal(front);
    m.tolerance = DimTolerance::Fit("ö".repeat(50));
    d.add_dimension(id, m).unwrap();
}

#[test]
fn validation_refuses_duplicate_and_out_of_range_ids_in_snapshot() {
    assert!(Document::from_snapshot(snapshot_with(vec![drawing_with_dims()], 2), 0.05).is_ok());
    let load = |dr: Drawing| invalid(Document::from_snapshot(snapshot_with(vec![dr], 2), 0.05));
    let mut dup = drawing_with_dims();
    dup.dimensions[1].id = DimId(1);
    let m = load(dup);
    assert!(m.contains("doppelt"), "{m}");
    assert!(m.contains("„A“"), "die Zeichnung wird genannt: {m}");
    let mut zero = drawing_with_dims();
    zero.dimensions[0].id = DimId(0);
    load(zero);
    let mut high = drawing_with_dims();
    high.dimensions[1].id = DimId(3); // next_dim ist 3
    load(high);
    for n in [0, u64::MAX] {
        let mut counter = drawing_with_dims();
        counter.next_dim = n;
        load(counter);
    }
}

#[test]
fn validation_refuses_mixed_groups() {
    let (mut d, id, front, top) = doc_with_views();
    let chain = |order: u32| {
        Some(ChainSpec {
            group: 4,
            mode: ChainMode::Chain,
            order,
        })
    };
    let mut first = horizontal(front);
    first.chain = chain(0);
    d.add_dimension(id, first).unwrap();
    // Andere Art in derselben Gruppe.
    let mut vertical = horizontal(front);
    vertical.kind = DimKind::Vertical;
    vertical.chain = chain(1);
    refused(&mut d, id, vertical, "Kette");
    // Andere Ansicht in derselben Gruppe.
    let mut other_view = horizontal(top);
    other_view.chain = chain(1);
    refused(&mut d, id, other_view, "Kette");
    // Gleiche Stelle doppelt.
    let mut same_order = horizontal(front);
    same_order.chain = chain(0);
    refused(&mut d, id, same_order, "doppelt");
    // Stelle über der Grenze (MAX_DIMENSIONS − 1).
    for order in [MAX_DIMENSIONS as u32, u32::MAX] {
        let mut far = horizontal(front);
        far.chain = chain(order);
        refused(&mut d, id, far, "Die Stelle in einer Maßkette ist zu groß");
    }
    let mut last = horizontal(front);
    last.chain = chain(MAX_DIMENSIONS as u32 - 1);
    let last = d.add_dimension(id, last).unwrap();
    d.delete_dimension(id, last).unwrap();
    // Passend: nächste Stelle; eine andere Gruppe darf eine andere Art haben.
    let mut next = horizontal(front);
    next.chain = chain(1);
    d.add_dimension(id, next).unwrap();
    let mut other_group = horizontal(top);
    other_group.kind = DimKind::Vertical;
    other_group.chain = Some(ChainSpec {
        group: 5,
        mode: ChainMode::Baseline,
        order: 0,
    });
    d.add_dimension(id, other_group).unwrap();
    // Im Snapshot ebenso.
    let mut dr = drawing_with_dims();
    dr.dimensions[0].chain = chain(0);
    dr.dimensions[1].chain = chain(1);
    dr.dimensions[1].kind = DimKind::Vertical;
    invalid(Document::from_snapshot(snapshot_with(vec![dr], 2), 0.05));
}

#[test]
fn validation_refuses_mixed_chain_modes_in_one_group() {
    let (mut d, id, front, _) = doc_with_views();
    let with = |mode: ChainMode, order: u32| {
        let mut m = horizontal(front);
        m.chain = Some(ChainSpec {
            group: 9,
            mode,
            order,
        });
        m
    };
    d.add_dimension(id, with(ChainMode::Chain, 0)).unwrap();
    refused(&mut d, id, with(ChainMode::Baseline, 1), "mischen");
    d.add_dimension(id, with(ChainMode::Chain, 1)).unwrap();
    // Im Snapshot ebenso.
    let mut dr = drawing_with_dims();
    for (i, mode) in [ChainMode::Baseline, ChainMode::Chain]
        .into_iter()
        .enumerate()
    {
        dr.dimensions[i].chain = Some(ChainSpec {
            group: 1,
            mode,
            order: i as u32,
        });
    }
    let m = invalid(Document::from_snapshot(snapshot_with(vec![dr], 2), 0.05));
    assert!(m.contains("mischen"), "{m}");
}

#[test]
fn validation_bounds_hints_and_roles() {
    let (mut d, id, front, _) = doc_with_views();
    let with_hint = |h: DVec3, role: &str| {
        let mut m = horizontal(front);
        m.refs[0] = GeoRef::Edge(GeoRefData::Edge {
            a: TagData {
                origin: 1,
                role: role.into(),
                hint: h,
            },
            b: tag("b"),
            hint: DVec3::ZERO,
        });
        m
    };
    // Hinweispunkte: Modellpunkte bis 2·MAX_COORD_MM.
    assert_eq!(MAX_HINT_MM, 2.0 * MAX_COORD_MM);
    for h in [2.000001e9, -1.0e300, f64::INFINITY] {
        refused(&mut d, id, with_hint(DVec3::new(0.0, h, 0.0), "a"), "Bezug");
    }
    d.add_dimension(id, with_hint(DVec3::new(1.0e9, -MAX_HINT_MM, 0.0), "a"))
        .unwrap();
    // Rollen höchstens 64 Zeichen (Umlaute zählen als eins).
    refused(&mut d, id, with_hint(DVec3::ZERO, &"r".repeat(65)), "Rolle");
    let mut face = spec(
        front,
        DimKind::Diameter,
        vec![GeoRef::Face(GeoRefData::Face(tag(&"x".repeat(65))))],
    );
    refused(&mut d, id, face.clone(), "Rolle");
    face.refs = vec![GeoRef::Face(GeoRefData::Face(tag(&"ä".repeat(64))))];
    d.add_dimension(id, face).unwrap();
    d.add_dimension(id, with_hint(DVec3::ZERO, &"ö".repeat(64)))
        .unwrap();
}

#[test]
fn validation_refuses_the_2001st_dimension() {
    let (mut d, id, front, _) = doc_with_views();
    d.update_drawing(id, |dr| {
        for _ in 0..2000 {
            dr.add_dimension(horizontal(front));
        }
    })
    .unwrap();
    assert_eq!(d.drawing(id).unwrap().dimensions.len(), 2000);
    refused(&mut d, id, horizontal(front), "2000");
    // Snapshot mit 2000 gültig, mit 2001 nicht.
    let mut s = d.snapshot();
    assert!(Document::from_snapshot(s.clone(), 0.05).is_ok());
    let dr = &mut s.drawings[0];
    dr.add_dimension(horizontal(front));
    assert_eq!(dr.dimensions.len(), 2001);
    let m = invalid(Document::from_snapshot(s, 0.05));
    assert!(m.contains("2000"), "{m}");
}

#[test]
fn snapshot_roundtrip_keeps_dimensions_and_centerlines() {
    let (mut d, id, front, top) = doc_with_views();
    d.update_drawing(id, |dr| dr.views[1].centerlines = false)
        .unwrap();
    let mut a = horizontal(front);
    a.tolerance = DimTolerance::LimitSizes {
        upper: 0.1,
        lower: -0.05,
    };
    a.text_override = Some("Ø <&> \"x\"".into());
    a.chain = Some(ChainSpec {
        group: 2,
        mode: ChainMode::Baseline,
        order: 3,
    });
    a.suggested = true;
    a.text_shift = (1.5, -2.25);
    a.decimals = 3;
    d.add_dimension(id, a).unwrap();
    let mut b = spec(
        top,
        DimKind::Diameter,
        vec![GeoRef::Face(GeoRefData::Face(tag("Mantel")))],
    );
    b.tolerance = DimTolerance::Fit("H7".into());
    b.angle = 135.0;
    let b = d.add_dimension(id, b).unwrap();
    d.delete_dimension(id, b).unwrap();
    let e = Document::from_snapshot(d.snapshot(), 0.05).unwrap();
    assert_eq!(e.drawings(), d.drawings());
    let dr = e.drawing(id).unwrap();
    assert!(dr.views[0].centerlines && !dr.views[1].centerlines);
    assert_eq!(dr.next_dim, 3);
    assert_eq!(e.edit_count(), 0);
    // Der Zähler bleibt: die nächste Kennung ist 3.
    let mut e = e;
    assert_eq!(e.add_dimension(id, horizontal(front)).unwrap(), DimId(3));
}

#[test]
fn deleting_a_view_removes_its_dimensions_in_the_document() {
    let (mut d, id, front, top) = doc_with_views();
    let other = d.add_view(id, ViewKind::Back, S11, FREE, None).unwrap();
    d.add_dimension(id, horizontal(front)).unwrap();
    d.add_dimension(id, horizontal(top)).unwrap();
    let keep = d.add_dimension(id, horizontal(other)).unwrap();
    let edits = d.edit_count();
    // Vorne samt der abgeleiteten Ansicht Oben: deren Maße verschwinden mit.
    d.update_drawing(id, |dr| {
        dr.remove_view(front);
    })
    .unwrap();
    let dr = d.drawing(id).unwrap();
    let ids: Vec<DimId> = dr.dimensions.iter().map(|m| m.id).collect();
    assert_eq!(ids, vec![keep]);
    assert_eq!(d.edit_count(), edits + 1);
    // Ein Maß zur gelöschten Ansicht wird abgelehnt.
    refused(&mut d, id, horizontal(front), "keiner Ansicht");
    // Die gespeicherte Fassung enthält keine verwaisten Maße und lädt wieder.
    let e = Document::from_snapshot(d.snapshot(), 0.05).unwrap();
    assert_eq!(e.drawings(), d.drawings());
}

#[test]
fn old_drawings_without_dimensions_load_unchanged() {
    let mut old = Drawing::new(DrawingId(1), "Alt".into(), A4, Projection::First);
    let front = old.add_view(ViewKind::Front, S11, FREE, None);
    old.add_view(ViewKind::Top, S11, ALIGNED, Some(front));
    old.views[0].centerlines = false;
    old.views[1].centerlines = false;
    assert!(old.dimensions.is_empty() && old.next_dim == 1);
    validate_drawing(&old).unwrap();
    let d = Document::from_snapshot(snapshot_with(vec![old.clone()], 2), 0.05).unwrap();
    assert_eq!(d.drawings(), std::slice::from_ref(&old));
    assert_eq!(d.edit_count(), 0);
    let s = d.snapshot();
    assert_eq!(s.drawings, vec![old]);
}

// ---- Auflösung gegen die Körper -----------------------------------------------------------

/// Die gerade Kante mit den Endpunkten `a` und `b`.
fn line_edge(s: &Solid, a: DVec3, b: DVec3) -> Id<Edge> {
    let near = |p: DVec3, q: DVec3| p.distance(q) <= EPS;
    s.edges
        .iter()
        .find(|(_, e)| {
            let (p, q) = (s.vertices[e.start].point, s.vertices[e.end].point);
            matches!(e.curve, Curve::Line { .. })
                && ((near(p, a) && near(q, b)) || (near(p, b) && near(q, a)))
        })
        .map(|(i, _)| i)
        .unwrap_or_else(|| panic!("Kante {a} – {b}"))
}

fn values(d: &Document, id: DrawingId) -> Vec<(ViewId, Result<f64, DimError>)> {
    let solids: Vec<&Solid> = d.visible_bodies().into_iter().map(|b| b.solid).collect();
    let r = d.resolve_dimensions(id, &solids).unwrap();
    assert!(r.last_known.is_empty());
    r.by_view
        .iter()
        .flat_map(|(v, list)| {
            list.iter()
                .map(move |(_, m)| (*v, m.clone().map(|m| m.value)))
        })
        .collect()
}

#[test]
fn resolve_dimensions_full_pipeline() {
    // Quader 10 × 30 × 20; Vorne (x/z), Oben (x/y), Rechts (y/z).
    let (a, b, c) = (10.0, 30.0, 20.0);
    let mut d = Document::new();
    let f = d
        .add_feature(
            "Quader",
            FeatureKind::Box {
                size: DVec3::new(a, b, c),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let id = d.add_drawing("Blatt", A4, Projection::First).unwrap();
    let front = d.add_view(id, ViewKind::Front, S11, FREE, None).unwrap();
    let top = d
        .add_view(id, ViewKind::Top, S11, ALIGNED, Some(front))
        .unwrap();
    let side = d
        .add_view(id, ViewKind::Left, S11, ALIGNED, Some(front))
        .unwrap();
    let v = DVec3::new;
    let solid = d.visible_bodies()[0].solid;
    let edge = |p: DVec3, q: DVec3| {
        GeoRef::Edge(
            geo_ref_for_edge(solid, line_edge(solid, p, q), &Tolerance::default()).unwrap(),
        )
    };
    let corner = |p: DVec3, q: DVec3, end: EndSel| GeoRef::EdgeEnd {
        edge: geo_ref_for_edge(solid, line_edge(solid, p, q), &Tolerance::default()).unwrap(),
        end,
    };
    let dims = vec![
        // Breite in Vorne: senkrechte Kanten bei x = 0 und x = a.
        spec(
            front,
            DimKind::Horizontal,
            vec![
                edge(v(0.0, 0.0, 0.0), v(0.0, 0.0, c)),
                edge(v(a, 0.0, 0.0), v(a, 0.0, c)),
            ],
        ),
        // Höhe in Vorne: waagerechte Kanten bei z = 0 und z = c.
        spec(
            front,
            DimKind::Vertical,
            vec![
                edge(v(0.0, 0.0, 0.0), v(a, 0.0, 0.0)),
                edge(v(0.0, 0.0, c), v(a, 0.0, c)),
            ],
        ),
        // Tiefe in Oben: Kanten entlang x bei y = 0 und y = b.
        spec(
            top,
            DimKind::Vertical,
            vec![
                edge(v(0.0, 0.0, c), v(a, 0.0, c)),
                edge(v(0.0, b, c), v(a, b, c)),
            ],
        ),
        // Tiefe in der Seitenansicht über zwei Eckpunkte.
        spec(
            side,
            DimKind::Horizontal,
            vec![
                corner(v(0.0, 0.0, 0.0), v(0.0, 0.0, c), EndSel::Start),
                corner(v(0.0, b, 0.0), v(0.0, b, c), EndSel::End),
            ],
        ),
    ];
    for m in dims {
        d.add_dimension(id, m).unwrap();
    }
    let got = values(&d, id);
    assert_eq!(got.len(), 4);
    let want = [(front, a), (front, c), (top, b), (side, b)];
    for ((view, value), (wv, w)) in got.iter().zip(want) {
        assert_eq!(*view, wv);
        let value = value.as_ref().unwrap();
        assert!((value - w).abs() <= EPS, "{value} statt {w}");
    }
    // Das Modell ändert sich: die Werte folgen.
    d.update(f, |k| {
        if let FeatureKind::Box { size, .. } = k {
            *size = DVec3::new(12.0, 31.0, 22.0);
        }
    })
    .unwrap();
    let got: Vec<f64> = values(&d, id)
        .into_iter()
        .map(|(_, v)| v.unwrap())
        .collect();
    for (g, w) in got.iter().zip([12.0, 22.0, 31.0, 31.0]) {
        assert!((g - w).abs() <= EPS, "{g} statt {w}");
    }
    // Feature unterdrückt: jedes Maß verloren, kein Abbruch.
    d.set_suppressed(f, true).unwrap();
    for (_, r) in values(&d, id) {
        assert!(matches!(r, Err(DimError::Lost(_))), "{r:?}");
    }
    // Unbekannte Zeichnung.
    invalid(d.resolve_dimensions(DrawingId(42), &[]));
}

// ---- Maße in Schnittansichten (Teilprojekt 3d, Aufgabe 10) --------------------------------

/// Quader 40 × 20 × 10 (x, y, z) mit durchgehender Bohrung r 5 um die Achse (20, 13, z).
/// Die Ebene y = 10 (Schnitt „Vorne“, Versatz 10) trifft die Bohrung 3 mm neben der Achse:
/// halbe Sehne √(5² − 3²) = 4, die Schnittfläche besteht also aus x 0…16 und x 24…40 (z 0…10),
/// der Abstand der beiden Kanten an der Bohrung ist 8. Keine Ecke liegt auf der Ebene (die Naht
/// der Bohrung liegt bei y = 13), es gibt also keine Verschiebung um 0,001 mm.
fn holed_block() -> (Document, FeatureId) {
    let mut d = Document::new();
    let block = d
        .add_feature(
            "Block",
            FeatureKind::Box {
                size: DVec3::new(40.0, 20.0, 10.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let bore = d
        .add_feature(
            "Bohrung",
            FeatureKind::Cylinder {
                radius: 5.0,
                height: 20.0,
                origin: DVec3::new(20.0, 13.0, -5.0),
            },
        )
        .unwrap();
    let cut = d
        .add_feature(
            "Abzug",
            FeatureKind::Boolean {
                target: BodyRef {
                    feature: block,
                    index: 0,
                },
                tool: BodyRef {
                    feature: bore,
                    index: 0,
                },
                op: planora_kernel::boolean::BoolOp::Difference,
            },
        )
        .unwrap();
    assert_eq!(d.visible_bodies().len(), 1);
    (d, cut)
}

fn cut_solids(solids: &[&Solid], kind: ViewKind, offset: f64) -> Vec<Solid> {
    let bodies: Vec<planora_drawing::DrawBody> = solids
        .iter()
        .map(|s| planora_drawing::DrawBody {
            name: "Block".into(),
            solid: s,
        })
        .collect();
    let plane = planora_drawing::section::SectionPlane { kind, offset };
    let out = planora_drawing::compute_section(
        &bodies,
        &plane,
        planora_drawing::compute_view,
        &planora_exchange::Progress::new(),
    )
    .unwrap();
    assert!(!out.data.nudged, "die Ebene liegt auf keiner Ecke");
    out.data.solids
}

fn value_of(
    dims: &planora_drawing::annotate::ResolvedDims,
    view: ViewId,
) -> Vec<Result<f64, DimError>> {
    dims.by_view
        .get(&view)
        .map(|l| l.iter().map(|(_, r)| r.clone().map(|m| m.value)).collect())
        .unwrap_or_default()
}

#[test]
fn resolve_against_cut_solids() {
    use planora_drawing::section::SECTION_ORIGIN_MIN;
    use std::collections::BTreeMap;
    let (mut d, _) = holed_block();
    let orig = d.visible_bodies()[0].solid.clone();
    let id = d.add_drawing("Blatt", A4, Projection::First).unwrap();
    let top = d
        .add_view(
            id,
            ViewKind::Top,
            S11,
            Placement::Free { x: 100.0, y: 200.0 },
            None,
        )
        .unwrap();
    let front = d
        .add_view(
            id,
            ViewKind::Front,
            S11,
            Placement::Free { x: 100.0, y: 80.0 },
            None,
        )
        .unwrap();
    let mut sec = None;
    d.update_drawing(id, |dr| {
        sec = Some(dr.add_section(top, ViewKind::Front, 10.0, 40.0).unwrap());
    })
    .unwrap();
    let sec = sec.unwrap();
    let cut = cut_solids(&[&orig], ViewKind::Front, 10.0);
    assert_eq!(cut.len(), 1);
    let tol = Tolerance::default();
    let v = DVec3::new;
    let at_bore = |x: f64| {
        let r = geo_ref_for_edge(
            &cut[0],
            line_edge(&cut[0], v(x, 10.0, 0.0), v(x, 10.0, 10.0)),
            &tol,
        )
        .unwrap();
        let GeoRefData::Edge { a, b, .. } = &r else {
            panic!("Kantendaten erwartet");
        };
        assert!(
            a.origin >= SECTION_ORIGIN_MIN || b.origin >= SECTION_ORIGIN_MIN,
            "eine Fläche der Kante ist die Schnittfläche"
        );
        GeoRef::Edge(r)
    };
    let bore_dim = |view| {
        spec(
            view,
            DimKind::Horizontal,
            vec![at_bore(16.0), at_bore(24.0)],
        )
    };
    // Breite in Vorne an den Originalkanten (x = 0 und x = 40 vorn bei y = 0).
    let width = spec(
        front,
        DimKind::Horizontal,
        vec![
            GeoRef::Edge(
                geo_ref_for_edge(
                    &orig,
                    line_edge(&orig, v(0.0, 0.0, 0.0), v(0.0, 0.0, 10.0)),
                    &tol,
                )
                .unwrap(),
            ),
            GeoRef::Edge(
                geo_ref_for_edge(
                    &orig,
                    line_edge(&orig, v(40.0, 0.0, 0.0), v(40.0, 0.0, 10.0)),
                    &tol,
                )
                .unwrap(),
            ),
        ],
    );
    d.add_dimension(id, bore_dim(sec)).unwrap();
    d.add_dimension(id, width).unwrap();
    // Dasselbe Bohrungsmaß in Vorne: dort gibt es keine Schnittfläche.
    d.add_dimension(id, bore_dim(front)).unwrap();
    let dr = d.drawing(id).unwrap().clone();
    let solids: Vec<&Solid> = vec![&orig];
    let map: BTreeMap<ViewId, Vec<&Solid>> = BTreeMap::from([(sec, vec![&cut[0]])]);
    let ok = |r: Result<(), std::convert::Infallible>| r;
    let (with, keys) = resolve_drawing_dimensions_views(&dr, &solids, &map, || ok(Ok(()))).unwrap();
    let got = value_of(&with, sec);
    assert_eq!(got.len(), 1);
    let x = got[0].as_ref().unwrap();
    assert!((x - 8.0).abs() <= EPS, "{x} statt 8");
    let fr = value_of(&with, front);
    assert_eq!(fr.len(), 2);
    assert!((fr[0].as_ref().unwrap() - 40.0).abs() <= EPS);
    assert!(matches!(fr[1], Err(DimError::Lost(_))), "{:?}", fr[1]);
    // Der Schlüssel des Schnittmaßes entsteht aus den geschnittenen Körpern.
    let sec_id = with.by_view[&sec][0].0.id;
    assert!(keys.contains_key(&sec_id));
    // Ohne Eintrag für die Schnittansicht: ihre Maße fehlen (nicht verloren), Vorne unverändert.
    let (without, keys0) =
        resolve_drawing_dimensions_views(&dr, &solids, &BTreeMap::new(), || ok(Ok(()))).unwrap();
    assert!(!without.by_view.contains_key(&sec));
    assert!(!keys0.contains_key(&sec_id));
    assert_eq!(value_of(&without, front), fr);
    // Die alten Einstiege rechnen genauso wie die leere Karte: Sie lassen die Maße der
    // Schnittansicht ganz weg (festgehalten, damit niemand sie für Zeichnungen mit Schnitten
    // nimmt und sich über fehlende Maße wundert).
    let (keyed, keyed_keys) =
        resolve_drawing_dimensions_keyed(&dr, &solids, || ok(Ok(()))).unwrap();
    assert_eq!(keyed.by_view, without.by_view);
    assert!(!keyed.by_view.contains_key(&sec));
    assert!(!keyed_keys.contains_key(&sec_id));
    let plain = resolve_drawing_dimensions(&dr, &solids, || ok(Ok(()))).unwrap();
    assert_eq!(plain.by_view, without.by_view);
    assert!(!plain.by_view.contains_key(&sec));
    let doc = d.resolve_dimensions(id, &solids).unwrap();
    assert_eq!(doc.by_view, without.by_view);
    assert!(!doc.by_view.contains_key(&sec));
    // Auch mit den Schnittkörpern unter den Körpern: weggelassen, nicht aufgelöst.
    let both: Vec<&Solid> = vec![&orig, &cut[0]];
    let mixed = resolve_drawing_dimensions(&dr, &both, || ok(Ok(()))).unwrap();
    assert!(!mixed.by_view.contains_key(&sec));
    assert!(
        !d.resolve_dimensions(id, &both)
            .unwrap()
            .by_view
            .contains_key(&sec)
    );
}
