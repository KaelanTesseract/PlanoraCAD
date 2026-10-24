//! Namen der rücknehmbaren Schritte (Rückgängig/Wiederholen, Aufgabe 5): jede Aktion läuft über
//! ihren echten Einstieg im `AppState` bzw. `Document`; verglichen werden die Bilder davor und
//! danach. Die Reihenfolge der Regeln (erste passende gewinnt) wird mit gleichzeitigen
//! Änderungen geprüft.

use planora_app::state::*;
use planora_app::undo_labels::{FALLBACK, describe_change, describe_sketch_change};
use planora_drawing::dimension::{
    DimId, DimKind, DimSpec, GeoRef, GeoRefData, Tolerance as DimTolerance,
};
use planora_drawing::model::section_kind;
use planora_drawing::sheet_info::{FieldValue, ProjectInfo, SheetInfo, Stamp};
use planora_drawing::{
    DrawingId, Paper, Placement, Projection, RowOverride, Scale, Sheet, ViewId, ViewKind,
};
use planora_exchange::{ExchangeMesh, MeshTransform};
use planora_feature::{BlendArt, FeatureId, FeatureKind, SketchFeature, SketchPlane};
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::boolean::BoolOp;
use planora_kernel::{Edge, Id, Solid};
use planora_math::{DQuat, DVec2, DVec3, Frame, Tolerance};
use planora_sketch::{ConstraintKind, CurveId, LineOrPoints, PointId, Sketch};
use std::sync::Arc;

const A4_QUER: Sheet = Sheet {
    paper: Paper::A4,
    landscape: true,
};
const BOX: DVec3 = DVec3::new(10.0, 20.0, 30.0);

fn s11() -> Scale {
    Scale { num: 1, den: 1 }
}

fn stamp() -> Stamp {
    Stamp {
        date: "2026-10-19".into(),
        user: "dennis".into(),
    }
}

fn free(x: f64, y: f64) -> Placement {
    Placement::Free { x, y }
}

/// Würfel als Netz (wie in den Netz-Tests).
fn cube() -> Arc<ExchangeMesh> {
    Arc::new(ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7,
            3, 1, 2, 6, 1, 6, 5,
        ],
    })
}

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id)
                .iter()
                .map(|u| face_name(s, u.face))
                .collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn edge_where(st: &AppState, pred: impl Fn(DVec3) -> bool) -> GeoRefData {
    let solid = &st.bodies[0].solid;
    let (id, _) = solid
        .edges
        .iter()
        .find(|(_, e)| {
            let a = solid.vertices.get(e.start).unwrap().point;
            let b = solid.vertices.get(e.end).unwrap().point;
            e.start != e.end && pred(a) && pred(b)
        })
        .expect("Kante nicht gefunden");
    planora_feature::geo_ref_for_edge(solid, id, &Tolerance::default()).unwrap()
}

fn width_spec(st: &AppState, view: ViewId, offset: f64) -> DimSpec {
    let (lo, hi) = st.bodies[0].bounds;
    DimSpec {
        id: DimId(0),
        view,
        kind: DimKind::Horizontal,
        refs: vec![
            GeoRef::Edge(edge_where(st, |p| near(p.x, lo.x) && near(p.y, lo.y))),
            GeoRef::Edge(edge_where(st, |p| near(p.x, hi.x) && near(p.y, lo.y))),
        ],
        offset,
        angle: 45.0,
        text_shift: (0.0, 0.0),
        tolerance: DimTolerance::None,
        decimals: 1,
        text_override: None,
        chain: None,
        suggested: false,
    }
}

/// Bild davor, Aktion, Bild danach; liefert den Namen.
fn label(st: &mut AppState, act: impl FnOnce(&mut AppState)) -> &'static str {
    let before = st.snapshot();
    act(st);
    let after = st.snapshot();
    assert!(
        !before.same_content(&after),
        "die Aktion hat nichts geändert"
    );
    describe_change(&before, &after)
}

fn one_box() -> AppState {
    let mut st = AppState::new();
    st.create_box(BOX).unwrap();
    st
}

/// Zwei Quader, der zweite überlappt den ersten (für Boolesche Operationen).
fn overlapping() -> AppState {
    let mut st = AppState::new();
    st.create_box(DVec3::splat(10.0)).unwrap();
    st.create_box(DVec3::splat(10.0)).unwrap();
    let b = st.bodies[0].bounds.0;
    let c = st.bodies[1].bounds.0;
    st.move_body(1, b - c + DVec3::new(5.0, 0.0, 0.0)).unwrap();
    st
}

/// Skizze mit Rechteck und Achse, geschlossen.
fn closed_sketch() -> (AppState, FeatureId, planora_sketch::CurveId) {
    let mut s = AppState::new();
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    let axis = {
        let sketch = s.session_sketch_mut().unwrap().1;
        sketch
            .add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(5.0, 10.0))
            .unwrap();
        let a = sketch.add_point(0.0, -2.0).unwrap();
        let b = sketch.add_point(0.0, 12.0).unwrap();
        let axis = sketch.add_line(a, b).unwrap();
        sketch.set_construction(axis, true).unwrap();
        axis
    };
    s.close_sketch();
    (s, sk, axis)
}

struct Draw {
    st: AppState,
    d: DrawingId,
    front: ViewId,
}

/// Quader mit Zeichnung „Blatt 1“ und einer Vorderansicht.
fn drawing() -> Draw {
    let mut st = one_box();
    let d = st
        .document
        .add_drawing("Blatt 1", A4_QUER, Projection::First)
        .unwrap();
    let front = st
        .document
        .add_view(d, ViewKind::Front, s11(), free(100.0, 100.0), None)
        .unwrap();
    Draw { st, d, front }
}

struct Full {
    st: AppState,
    d: DrawingId,
    dim: DimId,
    balloon: planora_drawing::BalloonId,
    sec: ViewId,
}

/// Wie `drawing`, mit Maß, Positionsnummer und einem Schnitt aus der Vorderansicht.
fn full() -> Full {
    let Draw { mut st, d, front } = drawing();
    let spec = width_spec(&st, front, 10.0);
    let dim = st.document.add_dimension(d, spec).unwrap();
    let key = st.document.parts_rows()[0].keys[0];
    let balloon = st
        .document
        .add_balloon(d, front, key, (0.5, 0.5), (10.0, 10.0))
        .unwrap();
    let kind = section_kind(ViewKind::Front, true, false).unwrap();
    let mut sec = None;
    st.document
        .update_drawing(d, |dr| {
            sec = Some(dr.add_section(front, kind, 0.0, 80.0).unwrap());
        })
        .unwrap();
    Full {
        st,
        d,
        dim,
        balloon,
        sec: sec.unwrap(),
    }
}

type Case = (&'static str, &'static str);

/// Eine Zeile je Einstieg: erwarteter Name, tatsächlicher Name.
#[allow(clippy::too_many_lines)]
fn entry_cases() -> Vec<(String, Case)> {
    let mut v: Vec<(String, Case)> = Vec::new();
    let mut add = |name: &str, expected: &'static str, got: &'static str| {
        v.push((name.to_string(), (expected, got)));
    };

    // Körper anlegen.
    let mut s = AppState::new();
    add(
        "create_box",
        "Quader anlegen",
        label(&mut s, |s| {
            s.create_box(BOX).unwrap();
        }),
    );
    add(
        "create_cylinder",
        "Zylinder anlegen",
        label(&mut AppState::new(), |s| {
            s.create_cylinder(3.0, 5.0).unwrap();
        }),
    );
    add(
        "create_sphere",
        "Kugel anlegen",
        label(&mut AppState::new(), |s| {
            s.create_sphere(3.0).unwrap();
        }),
    );
    add(
        "create_cone",
        "Kegel anlegen",
        label(&mut AppState::new(), |s| {
            s.create_cone(3.0, 5.0).unwrap();
        }),
    );
    add(
        "add_sketch",
        "Skizze anlegen",
        label(&mut AppState::new(), |s| {
            s.add_sketch(SketchPlane::Standard(StandardPlane::XY))
                .unwrap();
        }),
    );

    // Skizze → Körper.
    let (mut s, sk, axis) = closed_sketch();
    add(
        "extrude_sketch",
        "Extrudieren",
        label(&mut s, |s| {
            s.extrude_sketch(sk, 0, 2.0).unwrap();
        }),
    );
    let (mut s, sk, axis_r) = closed_sketch();
    let _ = axis;
    add(
        "revolve_sketch",
        "Drehen",
        label(&mut s, |s| {
            s.revolve_sketch(sk, 0, axis_r).unwrap();
        }),
    );

    // Boolesche Operationen.
    for (op, text) in [
        (BoolOp::Union, "Vereinigen"),
        (BoolOp::Difference, "Abziehen"),
        (BoolOp::Intersection, "Schneiden"),
    ] {
        add(
            &format!("add_boolean {op:?}"),
            text,
            label(&mut overlapping(), |s| {
                s.add_boolean(0, 1, op).unwrap();
            }),
        );
    }

    // Verschieben: neu und erweitert.
    add(
        "move_body (neu)",
        "Verschieben",
        label(&mut one_box(), |s| s.move_body(0, DVec3::X).unwrap()),
    );
    let mut s = one_box();
    s.move_body(0, DVec3::X).unwrap();
    add(
        "move_body (erweitert)",
        "Verschieben",
        label(&mut s, |s| s.move_body(0, DVec3::Y).unwrap()),
    );

    // Rundung und Fase.
    let mut s = one_box();
    let e = edge_between(&s.bodies[0].solid, "Deckel", "Wand 1");
    add(
        "add_blend Rundung",
        "Verrunden",
        label(&mut s, |s| {
            s.add_blend(0, &[e], BlendArt::Rundung { radius: 2.0 }, true)
                .unwrap();
        }),
    );
    let mut s = one_box();
    let e = edge_between(&s.bodies[0].solid, "Deckel", "Wand 1");
    add(
        "add_blend Fase",
        "Fase",
        label(&mut s, |s| {
            s.add_blend(0, &[e], BlendArt::FaseGleich { abstand: 2.0 }, true)
                .unwrap();
        }),
    );

    // Netz.
    add(
        "import_mesh",
        "Netz importieren",
        label(&mut one_box(), |s| {
            s.import_mesh("Würfel", cube(), 10.0).unwrap();
        }),
    );
    let mut s = one_box();
    s.import_mesh("Würfel", cube(), 10.0).unwrap();
    let mesh_index = s.bodies.len() - 1;
    add(
        "set_mesh_transform",
        "Verschieben",
        label(&mut s, |s| {
            s.set_mesh_transform(
                mesh_index,
                MeshTransform {
                    offset: DVec3::new(0.0, 0.0, 50.0),
                    rotation: DQuat::IDENTITY,
                    scale: 10.0,
                },
            )
            .unwrap();
        }),
    );

    // Löschen.
    let mut s = AppState::new();
    s.create_box(BOX).unwrap();
    let second = s.create_box(BOX).unwrap();
    add(
        "delete_feature",
        "Feature löschen",
        label(&mut s, |s| s.delete_feature(second).unwrap()),
    );
    let mut s = one_box();
    s.move_body(0, DVec3::X).unwrap();
    let first = s.document.features()[0].id;
    add(
        "delete_feature_with_dependents",
        "Feature löschen",
        label(&mut s, |s| {
            let removed = s.delete_feature_with_dependents(first).unwrap();
            assert_eq!(removed.len(), 2);
        }),
    );

    // Umbenennen, Unterdrücken, Marker, Parameter.
    let mut s = one_box();
    let f = s.document.features()[0].id;
    add(
        "rename_feature",
        "Umbenennen",
        label(&mut s, |s| s.rename_feature(f, "Grundplatte").unwrap()),
    );
    let mut s = one_box();
    let f = s.document.features()[0].id;
    add(
        "set_suppressed(true)",
        "Unterdrücken",
        label(&mut s, |s| s.document.set_suppressed(f, true).unwrap()),
    );
    add(
        "set_suppressed(false)",
        "Einblenden",
        label(&mut s, |s| s.document.set_suppressed(f, false).unwrap()),
    );
    let mut s = AppState::new();
    s.create_box(BOX).unwrap();
    s.create_box(BOX).unwrap();
    add(
        "set_marker",
        "Zeitleistenmarke setzen",
        label(&mut s, |s| s.document.set_marker(1).unwrap()),
    );
    let mut s = one_box();
    s.move_body(0, DVec3::X).unwrap();
    let tip = s.document.features()[1].id;
    add(
        "history_bar::move_marker (zurück)",
        "Verlaufsmarke setzen",
        label(&mut s, |s| {
            planora_app::history_bar::move_marker(s, tip, 1).unwrap();
        }),
    );
    add(
        "history_bar::to_end",
        "Verlaufsmarke setzen",
        label(&mut s, |s| {
            planora_app::history_bar::to_end(s, tip).unwrap()
        }),
    );
    let mut s = one_box();
    let f = s.document.features()[0].id;
    add(
        "Document::update (Quadergröße)",
        "Feature ändern",
        label(&mut s, |s| {
            s.document
                .update(f, |k| {
                    if let FeatureKind::Box { size, .. } = k {
                        *size = DVec3::splat(7.0);
                    }
                })
                .unwrap();
        }),
    );

    // Skizze bearbeitet über close_sketch.
    let mut s = AppState::new();
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    add(
        "close_sketch",
        "Skizze bearbeiten",
        label(&mut s, |s| {
            s.session_sketch_mut()
                .unwrap()
                .1
                .add_rectangle(DVec2::ZERO, DVec2::new(5.0, 5.0))
                .unwrap();
            s.close_sketch();
        }),
    );

    // Zeichnungen: anlegen (ein Bild mit der ersten Ansicht), Stücklistenblatt, löschen, umbenennen.
    add(
        "add_drawing_stamped + add_view",
        "Zeichnung anlegen",
        label(&mut one_box(), |s| {
            let d = s
                .document
                .add_drawing_stamped("Blatt 1", A4_QUER, Projection::First, &stamp())
                .unwrap();
            s.document
                .add_view(d, ViewKind::Front, s11(), free(100.0, 100.0), None)
                .unwrap();
        }),
    );
    add(
        "add_parts_sheets",
        "Stücklistenblatt anlegen",
        label(&mut one_box(), |s| {
            s.document.add_parts_sheets(A4_QUER, &stamp()).unwrap();
        }),
    );
    let x = drawing();
    let (mut st, d) = (x.st, x.d);
    add(
        "rename_drawing",
        "Zeichnung umbenennen",
        label(&mut st, |s| s.document.rename_drawing(d, "Teil A").unwrap()),
    );
    add(
        "delete_drawing",
        "Zeichnung löschen",
        label(&mut st, |s| s.document.delete_drawing(d).unwrap()),
    );

    // Ansichten (update_drawing).
    let x = drawing();
    let (mut st, d, front) = (x.st, x.d, x.front);
    add(
        "update_drawing: add_view",
        "Ansicht hinzufügen",
        label(&mut st, |s| {
            s.document
                .add_view(d, ViewKind::Top, s11(), free(100.0, 40.0), None)
                .unwrap();
        }),
    );
    add(
        "update_drawing: remove_view",
        "Ansicht löschen",
        label(&mut st, |s| {
            s.document
                .update_drawing(d, |dr| {
                    dr.remove_view(front);
                })
                .unwrap();
        }),
    );
    let x = drawing();
    let (mut st, d, front) = (x.st, x.d, x.front);
    add(
        "update_drawing: Lage",
        "Ansicht verschieben",
        label(&mut st, |s| {
            s.document
                .update_drawing(d, |dr| dr.views[0].placement = free(150.0, 90.0))
                .unwrap();
        }),
    );
    add(
        "update_drawing: Maßstab",
        "Maßstab ändern",
        label(&mut st, |s| {
            s.document
                .update_drawing(d, |dr| dr.views[0].scale = Scale { num: 1, den: 2 })
                .unwrap();
        }),
    );
    add(
        "update_drawing: verdeckte Kanten",
        "Ansicht ändern",
        label(&mut st, |s| {
            s.document
                .update_drawing(d, |dr| dr.views[0].show_hidden = !dr.views[0].show_hidden)
                .unwrap();
        }),
    );
    let kind = section_kind(ViewKind::Front, true, false).unwrap();
    add(
        "update_drawing: add_section",
        "Schnitt anlegen",
        label(&mut st, |s| {
            s.document
                .update_drawing(d, |dr| {
                    dr.add_section(front, kind, 0.0, 80.0).unwrap();
                })
                .unwrap();
        }),
    );
    let mut f = full();
    let (d, sec) = (f.d, f.sec);
    add(
        "update_drawing: Schnittlage",
        "Schnitt ändern",
        label(&mut f.st, |s| {
            s.document
                .update_drawing(d, |dr| {
                    dr.views
                        .iter_mut()
                        .find(|v| v.id == sec)
                        .unwrap()
                        .section
                        .as_mut()
                        .unwrap()
                        .offset = 3.0;
                })
                .unwrap();
        }),
    );

    // Maße und Positionsnummern.
    let x = drawing();
    let (mut st, d, front) = (x.st, x.d, x.front);
    let spec = width_spec(&st, front, 10.0);
    add(
        "add_dimension",
        "Maß anlegen",
        label(&mut st, |s| {
            s.document.add_dimension(d, spec).unwrap();
        }),
    );
    let mut f = full();
    let (d, dim) = (f.d, f.dim);
    add(
        "update_dimension (verschoben)",
        "Maß ändern",
        label(&mut f.st, |s| {
            s.document
                .update_dimension(d, dim, |m| m.offset = 20.0)
                .unwrap();
        }),
    );
    add(
        "delete_dimension",
        "Maß löschen",
        label(&mut f.st, |s| s.document.delete_dimension(d, dim).unwrap()),
    );
    let x = drawing();
    let (mut st, d, front) = (x.st, x.d, x.front);
    let key = st.document.parts_rows()[0].keys[0];
    add(
        "add_balloon",
        "Positionsnummer anlegen",
        label(&mut st, |s| {
            s.document
                .add_balloon(d, front, key, (0.5, 0.5), (10.0, 10.0))
                .unwrap();
        }),
    );
    let mut f = full();
    let (d, b) = (f.d, f.balloon);
    add(
        "Positionsnummer verschieben",
        "Positionsnummer verschieben",
        label(&mut f.st, |s| {
            s.document
                .update_drawing(d, |dr| dr.balloons[0].offset = (30.0, 5.0))
                .unwrap();
        }),
    );
    add(
        "Positionsnummer löschen",
        "Positionsnummer löschen",
        label(&mut f.st, |s| {
            s.document
                .update_drawing(d, |dr| {
                    dr.remove_balloon(b);
                })
                .unwrap();
        }),
    );

    // Angaben.
    let x = drawing();
    let (mut st, d) = (x.st, x.d);
    add(
        "set_sheet_info",
        "Blattangaben ändern",
        label(&mut st, |s| {
            let mut info = SheetInfo::new(&stamp());
            info.title = FieldValue::Text("Platte".into());
            s.document.set_sheet_info(d, info).unwrap();
        }),
    );
    add(
        "update_drawing: Blatt",
        "Blattangaben ändern",
        label(&mut st, |s| {
            s.document
                .update_drawing(d, |dr| dr.sheet.landscape = false)
                .unwrap();
        }),
    );
    add(
        "update_drawing: Projektionsart",
        "Blattangaben ändern",
        label(&mut st, |s| {
            s.document
                .update_drawing(d, |dr| dr.projection = Projection::Third)
                .unwrap();
        }),
    );
    add(
        "set_project_info",
        "Blattangaben ändern",
        label(&mut st, |s| {
            let mut p = ProjectInfo::new(&stamp());
            p.owner = FieldValue::Text("Firma".into());
            s.document.set_project_info(p).unwrap();
        }),
    );
    add(
        "set_parts_override",
        "Stücklistentext ändern",
        label(&mut st, set_name_override),
    );
    v
}

fn set_name_override(s: &mut AppState) {
    let key = s.document.parts_rows()[0].keys[0];
    s.document
        .set_parts_override(
            key,
            RowOverride {
                name: FieldValue::Text("Achse".into()),
                ..Default::default()
            },
        )
        .unwrap();
}

#[test]
fn each_entry_point_has_its_label() {
    let bad: Vec<String> = entry_cases()
        .into_iter()
        .filter(|(_, (want, got))| want != got)
        .map(|(n, (want, got))| format!("{n}: erwartet „{want}“, bekommen „{got}“"))
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
}

// (b) Rangfolge und Rückfall ---------------------------------------------------------------

fn grow_box(s: &mut AppState, id: FeatureId) {
    s.document
        .update(id, |k| {
            if let FeatureKind::Box { size, .. } = k {
                *size = DVec3::splat(7.0);
            }
        })
        .unwrap();
}

fn owner_project() -> ProjectInfo {
    let mut p = ProjectInfo::new(&stamp());
    p.owner = FieldValue::Text("Firma".into());
    p
}

#[test]
fn priority_and_fallback() {
    // Blattangaben + Projektangaben + Stücklistentext in einem Bild: Blattangaben.
    let x = drawing();
    let (mut st, d) = (x.st, x.d);
    let got = label(&mut st, |s| {
        let mut info = SheetInfo::new(&stamp());
        info.title = FieldValue::Text("Platte".into());
        s.document.set_sheet_info(d, info).unwrap();
        s.document.set_project_info(owner_project()).unwrap();
        set_name_override(s);
    });
    assert_eq!(got, "Blattangaben ändern");
    // Projektangaben + Stücklistentext (ohne Blatt): Projektangaben vor Stücklistentext.
    let mut st = drawing().st;
    let got = label(&mut st, |s| {
        s.document.set_project_info(owner_project()).unwrap();
        set_name_override(s);
    });
    assert_eq!(got, "Blattangaben ändern");
    // Zwei neue Features: Ändern.
    let got = label(&mut AppState::new(), |s| {
        s.create_box(BOX).unwrap();
        s.create_box(BOX).unwrap();
    });
    assert_eq!(got, FALLBACK);
    // Gleiche Bilder: Ändern.
    let snap = one_box().snapshot();
    assert_eq!(describe_change(&snap, &snap.clone()), FALLBACK);
    // Ein neues Feature schlägt Zeichnungsänderungen im selben Bild.
    let mut st = drawing().st;
    let d = st.document.drawings()[0].id;
    let got = label(&mut st, |s| {
        s.document.rename_drawing(d, "Teil A").unwrap();
        s.create_cylinder(3.0, 5.0).unwrap();
    });
    assert_eq!(got, "Zylinder anlegen");
    // Ein geändertes Feature schlägt Zeichnungsänderungen.
    let x = drawing();
    let (mut st, d) = (x.st, x.d);
    let f = st.document.features()[0].id;
    let got = label(&mut st, |s| {
        s.document.rename_drawing(d, "Teil A").unwrap();
        grow_box(s, f);
    });
    assert_eq!(got, "Feature ändern");
}

#[test]
fn feature_rules_are_ordered() {
    // Löschen mit Abhängigen ist ein einziges „Feature löschen“, auch wenn mehrere fehlen.
    let mut s = one_box();
    s.move_body(0, DVec3::X).unwrap();
    let first = s.document.features()[0].id;
    s.create_box(BOX).unwrap();
    s.move_body(1, DVec3::Z).unwrap();
    let got = label(&mut s, |s| {
        let gone = s.delete_feature_with_dependents(first).unwrap();
        assert_eq!(gone.len(), 2);
    });
    assert_eq!(got, "Feature löschen");
    // Unterdrücken vor Marke.
    let mut s = AppState::new();
    s.create_box(BOX).unwrap();
    let b = s.create_box(BOX).unwrap();
    let got = label(&mut s, |s| {
        s.document.set_suppressed(b, true).unwrap();
        s.document.set_marker(1).unwrap();
    });
    assert_eq!(got, "Unterdrücken");
    // Marke vor Art.
    let mut s = AppState::new();
    let a = s.create_box(BOX).unwrap();
    s.create_box(BOX).unwrap();
    let got = label(&mut s, |s| {
        s.document.set_marker(1).unwrap();
        grow_box(s, a);
    });
    assert_eq!(got, "Zeitleistenmarke setzen");
    // Zeitleistenmarke vor Verlaufsmarke; Verlaufsmarke vor Art.
    let mut s = AppState::new();
    let a = s.create_box(BOX).unwrap();
    s.create_box(BOX).unwrap();
    let got = label(&mut s, |s| {
        s.document.set_rollback(a, a).unwrap();
        s.document.set_marker(1).unwrap();
    });
    assert_eq!(got, "Zeitleistenmarke setzen");
    let mut s = AppState::new();
    let a = s.create_box(BOX).unwrap();
    let b = s.create_box(BOX).unwrap();
    let got = label(&mut s, |s| {
        s.document.set_rollback(b, b).unwrap();
        grow_box(s, a);
    });
    assert_eq!(got, "Verlaufsmarke setzen");
    // Umbenennen vor Unterdrücken und Art.
    let mut s = AppState::new();
    let a = s.create_box(BOX).unwrap();
    let got = label(&mut s, |s| {
        grow_box(s, a);
        s.document.set_suppressed(a, true).unwrap();
        s.rename_feature(a, "Platte").unwrap();
    });
    assert_eq!(got, "Umbenennen");
    // Unterdrücken und Einblenden richten sich nach dem neuen Zustand.
    let mut s = AppState::new();
    let a = s.create_box(BOX).unwrap();
    s.document.set_suppressed(a, true).unwrap();
    let got = label(&mut s, |s| s.document.set_suppressed(a, false).unwrap());
    assert_eq!(got, "Einblenden");
    // Gleichzeitig neu und gelöscht: die erste Regel (genau ein Feature neu) gilt.
    let mut s = AppState::new();
    let a = s.create_box(BOX).unwrap();
    s.create_box(BOX).unwrap();
    let got = label(&mut s, |s| {
        s.delete_feature(a).unwrap();
        s.create_sphere(3.0).unwrap();
    });
    assert_eq!(got, "Kugel anlegen");
    // Ein geändertes Verschieben ist „Verschieben“.
    let mut s = one_box();
    s.move_body(0, DVec3::X).unwrap();
    let mv = s.document.features()[1].id;
    let got = label(&mut s, |s| {
        s.document
            .update(mv, |k| {
                if let FeatureKind::Move { delta, .. } = k {
                    *delta = DVec3::new(3.0, 3.0, 0.0);
                }
            })
            .unwrap();
    });
    assert_eq!(got, "Verschieben");
}

#[test]
fn drawing_rules_are_ordered() {
    // Zeichnung neu schlägt die dabei entstandenen Projektangaben.
    let got = label(&mut one_box(), |s| {
        let d = s
            .document
            .add_drawing_stamped("Blatt 1", A4_QUER, Projection::First, &stamp())
            .unwrap();
        s.document
            .add_view(d, ViewKind::Front, s11(), free(100.0, 100.0), None)
            .unwrap();
    });
    assert_eq!(got, "Zeichnung anlegen");
    // Stücklistenblatt zusammen mit einer gewöhnlichen Zeichnung: Zeichnung anlegen.
    let got = label(&mut one_box(), |s| {
        s.document.add_parts_sheets(A4_QUER, &stamp()).unwrap();
        s.document
            .add_drawing("Blatt 1", A4_QUER, Projection::First)
            .unwrap();
    });
    assert_eq!(got, "Zeichnung anlegen");
    // Seite 1 der Stückliste löschen: eine Zeichnung löschen.
    let mut st = one_box();
    let p = st.document.add_parts_sheets(A4_QUER, &stamp()).unwrap();
    let got = label(&mut st, |s| s.document.delete_drawing(p).unwrap());
    assert_eq!(got, "Zeichnung löschen");
    // Umbenennen vor Ansichtsänderungen.
    let x = drawing();
    let (mut st, d) = (x.st, x.d);
    let got = label(&mut st, |s| {
        s.document
            .update_drawing(d, |dr| {
                dr.name = "Teil A".into();
                dr.add_view(ViewKind::Top, s11(), free(1.0, 1.0), None);
            })
            .unwrap();
    });
    assert_eq!(got, "Zeichnung umbenennen");
    // Ansicht samt Maß, Nummer und Schnitt löschen: ein einziges „Ansicht löschen“.
    let mut f = full();
    let d = f.d;
    let first = f.st.document.drawing(d).unwrap().views[0].id;
    let got = label(&mut f.st, |s| {
        s.document
            .update_drawing(d, |dr| {
                let gone = dr.remove_view(first);
                assert_eq!(gone.len(), 2, "Ansicht und ihr Schnitt");
                assert!(dr.dimensions.is_empty() && dr.balloons.is_empty());
            })
            .unwrap();
    });
    assert_eq!(got, "Ansicht löschen");
    // Neue Ansicht: Schnitt vor gewöhnlicher Ansicht, beides vor anderen Änderungen.
    let x = drawing();
    let (mut st, d, front) = (x.st, x.d, x.front);
    let kind = section_kind(ViewKind::Front, true, false).unwrap();
    let got = label(&mut st, |s| {
        s.document
            .update_drawing(d, |dr| {
                dr.add_view(ViewKind::Top, s11(), free(1.0, 1.0), None);
                dr.add_section(front, kind, 0.0, 80.0).unwrap();
                dr.views[0].placement = free(5.0, 5.0);
            })
            .unwrap();
    });
    assert_eq!(got, "Schnitt anlegen");
    // Lage vor Maßstab.
    let mut f = full();
    let (d, sec) = (f.d, f.sec);
    let got = label(&mut f.st, |s| {
        s.document
            .update_drawing(d, |dr| {
                dr.views[0].placement = free(1.0, 1.0);
                for v in &mut dr.views {
                    v.scale = Scale { num: 1, den: 2 };
                }
            })
            .unwrap();
    });
    assert_eq!(got, "Ansicht verschieben");
    // Maßstab der Ausgangsansicht samt Schnitt (Folgeansichten): ein Name; vor übrigen Feldern.
    let mut f = full();
    let got = label(&mut f.st, |s| {
        s.document
            .update_drawing(d, |dr| {
                for v in &mut dr.views {
                    v.scale = Scale { num: 1, den: 2 };
                }
                dr.views
                    .iter_mut()
                    .find(|v| v.id == sec)
                    .unwrap()
                    .show_hidden = true;
            })
            .unwrap();
    });
    assert_eq!(got, "Maßstab ändern");
    // Schnitt vor übrigen Feldern.
    let mut f = full();
    let got = label(&mut f.st, |s| {
        s.document
            .update_drawing(d, |dr| {
                let v = dr.views.iter_mut().find(|v| v.id == sec).unwrap();
                v.section.as_mut().unwrap().offset = 4.0;
                v.show_hidden = true;
            })
            .unwrap();
    });
    assert_eq!(got, "Schnitt ändern");
    // Maß geändert vor Nummer verschoben vor Blattangaben.
    let mut f = full();
    let got = label(&mut f.st, |s| {
        s.document
            .update_drawing(d, |dr| {
                dr.dimensions[0].offset = 25.0;
                dr.balloons[0].offset = (1.0, 1.0);
                dr.projection = Projection::Third;
            })
            .unwrap();
    });
    assert_eq!(got, "Maß ändern");
    // Maß gelöscht vor Nummer verschoben; Nummer gelöscht vor Blattangaben.
    let mut f = full();
    let dim = f.dim;
    let got = label(&mut f.st, |s| {
        s.document
            .update_drawing(d, |dr| {
                dr.remove_dimension(dim);
                dr.balloons[0].offset = (1.0, 1.0);
            })
            .unwrap();
    });
    assert_eq!(got, "Maß löschen");
    let mut f = full();
    let balloon = f.balloon;
    let got = label(&mut f.st, |s| {
        s.document
            .update_drawing(d, |dr| {
                dr.remove_balloon(balloon);
                dr.projection = Projection::Third;
            })
            .unwrap();
    });
    assert_eq!(got, "Positionsnummer löschen");
    // Neues Maß vor neuer Nummer.
    let x = drawing();
    let (mut st, d, front) = (x.st, x.d, x.front);
    let spec = width_spec(&st, front, 10.0);
    let key = st.document.parts_rows()[0].keys[0];
    let got = label(&mut st, |s| {
        s.document
            .add_balloon(d, front, key, (0.5, 0.5), (10.0, 10.0))
            .unwrap();
        s.document.add_dimension(d, spec).unwrap();
    });
    assert_eq!(got, "Maß anlegen");
}

// (c) Skizzen ------------------------------------------------------------------------------

fn sketch_feature() -> SketchFeature {
    SketchFeature {
        sketch: Sketch::new(Frame::WORLD, "Skizze", Tolerance::default()),
        plane: SketchPlane::Standard(StandardPlane::XY),
        copies: vec![],
        formulas: Default::default(),
    }
}

struct Base {
    f: SketchFeature,
    a: PointId,
    b: PointId,
    line: CurveId,
    circle: CurveId,
}

/// Skizze mit zwei Punkten, einer Linie und einem Kreis.
fn base_sketch() -> Base {
    let mut f = sketch_feature();
    let a = f.sketch.add_point(0.0, 0.0).unwrap();
    let b = f.sketch.add_point(3.0, 1.0).unwrap();
    let line = f.sketch.add_line(a, b).unwrap();
    let c = f.sketch.add_point(10.0, 10.0).unwrap();
    let circle = f.sketch.add_circle(c, 2.0).unwrap();
    Base {
        f,
        a,
        b,
        line,
        circle,
    }
}

fn changed(base: &SketchFeature, f: impl FnOnce(&mut Sketch)) -> &'static str {
    let mut after = base.clone();
    f(&mut after.sketch);
    describe_sketch_change(base, &after)
}

fn two_points(s: &mut Sketch) -> (PointId, PointId) {
    (
        s.add_point(0.0, 0.0).unwrap(),
        s.add_point(5.0, 0.0).unwrap(),
    )
}

#[test]
fn sketch_labels() {
    let Base {
        f: base,
        a,
        b,
        line,
        circle,
    } = base_sketch();
    let empty = sketch_feature();
    // Zeichnen.
    let got = changed(&empty, |s| {
        let (p, q) = two_points(s);
        s.add_line(p, q).unwrap();
    });
    assert_eq!(got, "Linie zeichnen");
    let got = changed(&empty, |s| {
        let p = s.add_point(1.0, 1.0).unwrap();
        s.add_circle(p, 2.0).unwrap();
    });
    assert_eq!(got, "Kreis zeichnen");
    let got = changed(&empty, |s| {
        let c = s.add_point(0.0, 0.0).unwrap();
        let st = s.add_point(5.0, 0.0).unwrap();
        let en = s.add_point(0.0, 5.0).unwrap();
        s.add_arc(c, st, en).unwrap();
    });
    assert_eq!(got, "Bogen zeichnen");
    let got = changed(&empty, |s| {
        let c = s.add_point(0.0, 0.0).unwrap();
        s.add_ellipse(c, DVec2::new(5.0, 0.0), 2.0).unwrap();
    });
    assert_eq!(got, "Ellipse zeichnen");
    let got = changed(&empty, |s| {
        let ps: Vec<PointId> = (0..3)
            .map(|i| s.add_point(f64::from(i), f64::from(i * i)).unwrap())
            .collect();
        s.add_spline(&ps).unwrap();
    });
    assert_eq!(got, "Spline zeichnen");
    let got = changed(&empty, |s| {
        s.add_rectangle(DVec2::ZERO, DVec2::new(4.0, 3.0)).unwrap();
    });
    assert_eq!(got, "Rechteck zeichnen");
    let got = changed(&empty, |s| {
        let (p, q) = two_points(s);
        s.add_line(p, q).unwrap();
        let r = s.add_point(9.0, 9.0).unwrap();
        s.add_line(q, r).unwrap();
    });
    assert_eq!(got, "Zeichnen", "zwei Linien");
    let got = changed(&empty, |s| {
        let c = s.add_point(0.0, 0.0).unwrap();
        let p = s.add_point(0.0, 9.0).unwrap();
        let q = s.add_point(1.0, 9.0).unwrap();
        s.add_circle(c, 2.0).unwrap();
        s.add_line(p, q).unwrap();
    });
    assert_eq!(got, "Zeichnen", "Kreis und Linie");
    let got = changed(&base, |s| {
        s.add_point(20.0, 20.0).unwrap();
    });
    assert_eq!(got, "Punkt setzen");
    // Maße und Abhängigkeiten.
    let got = changed(&base, |s| {
        s.add_constraint(ConstraintKind::Distance(a, b, 6.0))
            .unwrap();
    });
    assert_eq!(got, "Maß setzen", "Abstand");
    let got = changed(&base, |s| {
        s.add_constraint(ConstraintKind::Radius(circle, 4.0))
            .unwrap();
    });
    assert_eq!(got, "Maß setzen", "Radius");
    let got = changed(&base, |s| {
        s.add_constraint(ConstraintKind::Diameter(circle, 8.0))
            .unwrap();
    });
    assert_eq!(got, "Maß setzen", "Durchmesser");
    let got = changed(&base, |s| {
        s.add_constraint(ConstraintKind::Horizontal(LineOrPoints::Line(line)))
            .unwrap();
    });
    assert_eq!(got, "Abhängigkeit setzen", "waagerecht");
    // Löschen.
    assert_eq!(changed(&base, |s| s.remove_curve(line).unwrap()), "Löschen");
    assert_eq!(
        changed(&base, |s| s.remove_curve(circle).unwrap()),
        "Löschen"
    );
    let mut with_constraint = base.clone();
    with_constraint
        .sketch
        .add_constraint(ConstraintKind::Horizontal(LineOrPoints::Line(line)))
        .unwrap();
    let id = with_constraint.sketch.constraints().next().unwrap().0;
    let got = changed(&with_constraint, |s| s.remove_constraint(id).unwrap());
    assert_eq!(got, "Löschen", "Abhängigkeit weg");
    // Hilfslinie, gezogener Punkt.
    let got = changed(&base, |s| s.set_construction(line, true).unwrap());
    assert_eq!(got, "Hilfslinie umschalten");
    let got = changed(&base, |s| s.set_point_xy(b, 7.0, 7.0).unwrap());
    assert_eq!(got, "Skizze ändern", "Punkt gezogen");
    assert_eq!(
        describe_sketch_change(&base, &base.clone()),
        "Skizze ändern"
    );
    // Rangfolge: Löschen vor Zeichnen; Zeichnen vor Punkt vor Maß vor Hilfslinie.
    let got = changed(&base, |s| {
        s.remove_curve(circle).unwrap();
        let p = s.add_point(30.0, 30.0).unwrap();
        let q = s.add_point(31.0, 30.0).unwrap();
        s.add_line(p, q).unwrap();
    });
    assert_eq!(got, "Löschen");
    let got = changed(&base, |s| {
        let p = s.add_point(30.0, 30.0).unwrap();
        let q = s.add_point(31.0, 30.0).unwrap();
        s.add_line(p, q).unwrap();
        s.add_constraint(ConstraintKind::Radius(circle, 4.0))
            .unwrap();
    });
    assert_eq!(got, "Linie zeichnen");
    let got = changed(&base, |s| {
        s.add_point(30.0, 30.0).unwrap();
        s.add_constraint(ConstraintKind::Radius(circle, 4.0))
            .unwrap();
    });
    assert_eq!(got, "Punkt setzen", "Punkt vor Abhängigkeit");
    let got = changed(&base, |s| {
        s.add_constraint(ConstraintKind::Radius(circle, 4.0))
            .unwrap();
        s.set_construction(line, true).unwrap();
    });
    assert_eq!(got, "Maß setzen", "Abhängigkeit vor Hilfslinie");
}

#[test]
fn edge_copies_are_named_first() {
    let mut s = one_box();
    let top = {
        let solid = &s.bodies[0].solid;
        let top_z = s.bodies[0].bounds.1.z;
        solid
            .faces
            .iter()
            .find(|(id, _)| {
                planora_sketch::plane_of_face(solid, *id)
                    .is_some_and(|f| f.z.z > 0.5 && (f.origin.z - top_z).abs() < 1e-9)
            })
            .map(|(id, _)| id)
            .unwrap()
    };
    let edge = edge_between(&s.bodies[0].solid, "Deckel", "Wand 1");
    let sk = s.new_sketch_on_face(0, top).unwrap();
    s.open_sketch(sk).unwrap();
    let before = s.snapshot();
    s.project_edge_into_sketch(0, edge).unwrap();
    s.close_sketch();
    let after = s.snapshot();
    let pick = |snap: &planora_feature::Snapshot| match &snap.features[1].kind {
        FeatureKind::Sketch(f) => f.clone(),
        k => panic!("{k:?}"),
    };
    let (b, a) = (pick(&before), pick(&after));
    assert!(a.copies.len() > b.copies.len());
    assert_eq!(describe_sketch_change(&b, &a), "Kante übernehmen");
    assert_eq!(describe_change(&before, &after), "Skizze bearbeiten");
    // Auch zusammen mit Löschen gewinnt „Kante übernehmen“.
    let mut a2 = a.clone();
    let curve = a2.copies[0].curve;
    a2.sketch.remove_curve(curve).unwrap();
    assert!(a2.copies.len() > b.copies.len());
    assert_eq!(describe_sketch_change(&b, &a2), "Kante übernehmen");
}

// (d) Schriftzeichen -----------------------------------------------------------------------

/// Alle Texte in Anführungszeichen im Quelltext der Namen (ohne Kommentarzeilen).
fn literals(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in source.lines() {
        if line.trim_start().starts_with("//") {
            continue;
        }
        for (i, part) in line.split('"').enumerate() {
            if i % 2 == 1 {
                out.push(part.to_string());
            }
        }
    }
    out
}

#[test]
fn labels_use_allowed_glyphs() {
    let fonts_src = include_str!("fonts.rs");
    let allowed_line = fonts_src
        .lines()
        .find(|l| l.starts_with("const ALLOWED: &str = \""))
        .expect("Erlaubnisliste in tests/fonts.rs");
    let allowed: String = allowed_line.split('"').nth(1).unwrap().to_string();
    assert!(allowed.contains('ä'));
    let texts = literals(include_str!("../src/undo_labels.rs"));
    assert!(texts.len() > 50, "Namen im Quelltext: {}", texts.len());
    for t in &texts {
        assert!(!t.ends_with('…') && !t.ends_with("..."), "{t}");
        for c in t.chars() {
            assert!(c.is_ascii() || allowed.contains(c), "{t:?}: Zeichen {c:?}");
        }
    }
    // Die tatsächlich gelieferten Namen stehen alle im Quelltext.
    for (name, (want, got)) in entry_cases() {
        assert!(texts.iter().any(|t| t == got), "{name}: {got}");
        assert!(texts.iter().any(|t| t == want), "{name}: {want}");
    }
}

// (e) Parameter und Formeln (Aufgabe 8) ----------------------------------------------------

fn fx(pairs: &[(planora_feature::FieldKey, &str)]) -> planora_feature::Formulas {
    pairs.iter().map(|(k, t)| (*k, t.to_string())).collect()
}

fn set_param(st: &mut AppState, id: planora_feature::ParamId, name: &str, formula: &str) {
    let p = st.document.parameter(id).unwrap().clone();
    st.edit_parameter(
        id,
        planora_feature::ParamEdit {
            name: name.into(),
            formula: formula.into(),
            unit: p.unit,
            comment: p.comment,
        },
    )
    .unwrap();
}

#[test]
fn parameter_labels() {
    use planora_feature::FieldKey;
    let mut s = AppState::new();
    let mut breite = None;
    assert_eq!(
        label(&mut s, |s| breite = Some(s.add_parameter().unwrap())),
        "Parameter anlegen"
    );
    let breite = breite.unwrap();
    assert_eq!(
        label(&mut s, |s| set_param(s, breite, "Breite", "40")),
        "Parameter ändern"
    );
    let q = s
        .create_box_with(
            DVec3::new(40.0, 20.0, 10.0),
            fx(&[(FieldKey::Breite, "Breite")]),
        )
        .unwrap();
    // Der Quader ändert sich mit, der Name bleibt „Parameter ändern“ (Regel vor den Features).
    let before = s.snapshot();
    set_param(&mut s, breite, "Breite", "60");
    let after = s.snapshot();
    assert_ne!(before.features[0].kind, after.features[0].kind);
    assert_eq!(describe_change(&before, &after), "Parameter ändern");
    // Kommentar ändern.
    assert_eq!(
        label(&mut s, |s| {
            let p = s.document.parameter(breite).unwrap().clone();
            s.edit_parameter(
                breite,
                planora_feature::ParamEdit {
                    comment: "außen".into(),
                    name: p.name,
                    formula: p.formula,
                    unit: p.unit,
                },
            )
            .unwrap();
        }),
        "Parameter ändern"
    );
    // Löschen und Ersetzen heißen beide „Parameter löschen“.
    let other = s.add_parameter().unwrap();
    assert_eq!(
        label(&mut s, |s| s.delete_parameter(other).unwrap()),
        "Parameter löschen"
    );
    // Nur der Zähler anders (ein Parameter angelegt und wieder gelöscht): „Parameter ändern“.
    let mut counted = s.snapshot();
    counted.next_param += 1;
    assert_eq!(describe_change(&s.snapshot(), &counted), "Parameter ändern");
    assert_eq!(
        label(&mut s, |s| s.replace_parameter(breite).unwrap()),
        "Parameter löschen"
    );
    assert!(s.document.feature(q).unwrap().formulas().is_empty());

    // Nur der Formeltext eines Features anders (gleicher Wert): wie eine Änderung der Art.
    let mut s = AppState::new();
    let b = s.add_parameter().unwrap();
    set_param(&mut s, b, "Breite", "40");
    let q = s
        .create_box_with(
            DVec3::new(40.0, 20.0, 10.0),
            fx(&[(FieldKey::Breite, "Breite")]),
        )
        .unwrap();
    let before = s.snapshot();
    s.document
        .update_with_formulas(q, |_| {}, fx(&[(FieldKey::Breite, "breite")]))
        .unwrap();
    let after = s.snapshot();
    assert_eq!(before.features[0].kind, after.features[0].kind);
    assert_eq!(describe_change(&before, &after), "Feature ändern");
    // Verschieben mit Formel.
    let body = s.bodies[0].source;
    let m = s
        .document
        .add_feature_with_formulas(
            "Verschieben 1",
            FeatureKind::Move {
                body,
                delta: DVec3::ZERO,
            },
            fx(&[(FieldKey::X, "Breite")]),
        )
        .unwrap();
    let before = s.snapshot();
    s.document
        .update_with_formulas(m, |_| {}, fx(&[(FieldKey::X, "BREITE")]))
        .unwrap();
    assert_eq!(describe_change(&before, &s.snapshot()), "Verschieben");
    // Formel entfernt (Wert bleibt): ebenso.
    let before = s.snapshot();
    s.document
        .update_with_formulas(m, |_| {}, Default::default())
        .unwrap();
    assert_eq!(describe_change(&before, &s.snapshot()), "Verschieben");
}

#[test]
fn sketch_formula_labels() {
    let Base { f: base, a, b, .. } = base_sketch();
    let mut with_dim = base.clone();
    let c = with_dim
        .sketch
        .add_constraint(ConstraintKind::Distance(a, b, 3.0))
        .unwrap();
    // Neues Maß mit Formel: „Maß setzen“ (Strukturregel vor der Formel).
    let mut dim_formula = with_dim.clone();
    dim_formula.formulas.insert(c, "3".into());
    assert_eq!(describe_sketch_change(&base, &dim_formula), "Maß setzen");
    // Nur die Formel neu, anders oder entfernt: „Formel ändern“.
    assert_eq!(
        describe_sketch_change(&with_dim, &dim_formula),
        "Formel ändern"
    );
    let mut other = dim_formula.clone();
    other.formulas.insert(c, "1 + 2".into());
    assert_eq!(
        describe_sketch_change(&dim_formula, &other),
        "Formel ändern"
    );
    assert_eq!(
        describe_sketch_change(&dim_formula, &with_dim),
        "Formel ändern"
    );
    // Formel samt Maß gelöscht: „Löschen“ (vor „Formel ändern“).
    let mut removed = dim_formula.clone();
    removed.sketch.remove_constraint(c).unwrap();
    removed.formulas.clear();
    assert_eq!(describe_sketch_change(&dim_formula, &removed), "Löschen");
    // Hilfslinie und Formel zugleich: die Strukturregel gewinnt.
    let mut both = dim_formula.clone();
    both.formulas.clear();
    let line = both.sketch.curves().next().unwrap().0;
    both.sketch.set_construction(line, true).unwrap();
    assert_eq!(
        describe_sketch_change(&dim_formula, &both),
        "Hilfslinie umschalten"
    );
    // Ohne Formelunterschied bleibt es beim bisherigen Namen.
    let mut moved = dim_formula.clone();
    moved.sketch.set_constraint_value(c, 4.0).unwrap();
    assert_eq!(
        describe_sketch_change(&dim_formula, &moved),
        "Skizze ändern"
    );
}
