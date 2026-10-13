//! Mehrfachauswahl von Kanten für Rundungen (Teilprojekt 8, Task 10).

use planora_app::blend_dialog::{ArtChoice, BlendDialog};
use planora_app::state::*;
use planora_feature::FeatureKind;
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::{Edge, Id, Kernel, Profile, ProfileLoop, Segment, Solid};
use planora_math::{DVec2, DVec3, PI};
use planora_render::EdgeRef;

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

#[test]
fn clicks_mark_toggle_and_extend() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.edge_picking = true;
    let solid = s.bodies[0].solid.clone();
    let e1 = EdgeRef {
        body: 0,
        edge: edge_between(&solid, "Deckel", "Wand 1"),
    };
    let e2 = EdgeRef {
        body: 0,
        edge: edge_between(&solid, "Boden", "Wand 3"),
    };
    let v = s.scene_version;
    s.toggle_edge_mark(e1, false);
    assert_eq!(s.marked_edges, vec![e1]);
    assert!(s.scene_version > v, "Szene neu aufgebaut");
    s.toggle_edge_mark(e2, true);
    assert_eq!(s.marked_edges, vec![e1, e2]);
    s.toggle_edge_mark(e1, true);
    assert_eq!(s.marked_edges, vec![e2]);
    s.toggle_edge_mark(e2, false);
    assert!(
        s.marked_edges.is_empty(),
        "derselbe Klick ohne Umschalt leert"
    );
    s.toggle_edge_mark(e1, false);
    s.clear_edge_marks();
    assert!(s.marked_edges.is_empty());
}

#[test]
fn the_tangent_chain_marks_the_whole_loop() {
    let mut s = AppState::new();
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(10.0, 0.0),
                },
                Segment::Arc {
                    center: DVec2::new(10.0, 2.0),
                    sweep: PI,
                },
                Segment::Line {
                    to: DVec2::new(0.0, 4.0),
                },
                Segment::Arc {
                    center: DVec2::new(0.0, 2.0),
                    sweep: PI,
                },
            ],
        },
        holes: vec![],
    };
    // Der Körper kommt über eine Skizze nicht einfacher zustande; hier direkt als Extrusion des
    // Kernels geprüft, die Markierung arbeitet nur auf `bodies`.
    s.create_box(DVec3::splat(1.0)).unwrap();
    let stadium = s
        .kernel
        .extrude(&profile, &planora_math::Frame::WORLD, 3.0)
        .unwrap();
    s.bodies[0].solid = stadium;
    s.chain_marking = true;
    let e = EdgeRef {
        body: 0,
        edge: edge_between(&s.bodies[0].solid, "Deckel", "Wand 1"),
    };
    s.toggle_edge_mark(e, false);
    assert_eq!(s.marked_edges.len(), 4);
}

#[test]
fn the_dialog_blends_all_marked_edges_and_clears_them() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    let solid = s.bodies[0].solid.clone();
    let mut d = BlendDialog::fillet();
    d.tangentenkette = false;
    s.edge_picking = true;
    s.toggle_edge_mark(
        EdgeRef {
            body: 0,
            edge: edge_between(&solid, "Deckel", "Wand 1"),
        },
        false,
    );
    s.toggle_edge_mark(
        EdgeRef {
            body: 0,
            edge: edge_between(&solid, "Boden", "Wand 3"),
        },
        true,
    );
    assert_eq!(d.chosen_edges(&s).map(|(_, e)| e.len()), Some(2));
    assert!(!d.execute(&mut s));
    d.finish(&mut s);
    assert!(s.marked_edges.is_empty() && !s.edge_picking);
    let f = s.document.features().last().unwrap();
    match &f.kind {
        FeatureKind::Verrunden { edges, .. } => assert_eq!(edges.len(), 2),
        k => panic!("{k:?}"),
    }
}

#[test]
fn switching_the_chamfer_kind_resets_the_second_value() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    let mut d = BlendDialog::chamfer();
    assert_eq!(d.second, "45", "Vorgabe der Winkelfase");
    d.set_art(ArtChoice::FaseZwei);
    assert_eq!(d.second, "1", "zwei Abstände starten nicht mit 45 mm");
    assert!(d.art_value().is_ok());
    d.second = "3".into();
    d.set_art(ArtChoice::FaseZwei);
    assert_eq!(
        d.second, "3",
        "dieselbe Art noch einmal wählen ändert nichts"
    );
    d.set_art(ArtChoice::FaseWinkel);
    assert_eq!(d.second, "45");
    d.set_art(ArtChoice::FaseGleich);
    d.set_art(ArtChoice::FaseZwei);
    assert_eq!(d.second, "1");
    // Mit der Vorgabe geht die Fase an einem Würfel von 10 mm durch.
    let solid = s.bodies[0].solid.clone();
    s.toggle_edge_mark(
        EdgeRef {
            body: 0,
            edge: edge_between(&solid, "Deckel", "Wand 1"),
        },
        false,
    );
    d.tangentenkette = false;
    assert!(!d.execute(&mut s), "{:?}", d.error());
}

#[test]
fn edges_without_feature_tags_give_a_german_message() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    // Roher Kernelkörper ohne Marken der Flächen.
    s.bodies[0].solid = s.kernel.make_box(DVec3::splat(10.0)).unwrap();
    let edge = s.bodies[0]
        .solid
        .edges
        .iter()
        .next()
        .map(|(id, _)| id)
        .unwrap();
    s.edge_picking = true;
    s.toggle_edge_mark(EdgeRef { body: 0, edge }, false);
    let mut d = BlendDialog::fillet();
    assert!(d.execute(&mut s), "Fenster bleibt offen");
    let m = d.error().expect("Meldung im Fenster").to_string();
    assert!(m.contains("keinem Feature zugeordnet"), "{m}");
    assert_eq!(s.document.features().len(), 1, "Dokument unverändert");
}

#[test]
fn edges_of_two_bodies_are_refused_by_body_name() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.create_box(DVec3::splat(10.0)).unwrap();
    assert_eq!(s.bodies.len(), 2);
    let edge = |s: &AppState, i: usize| EdgeRef {
        body: i,
        edge: edge_between(&s.bodies[i].solid, "Deckel", "Wand 1"),
    };
    // Klicks mischen nie Körper: ein Klick in den anderen Körper ersetzt die Markierung.
    s.edge_picking = true;
    let (e0, e1) = (edge(&s, 0), edge(&s, 1));
    s.toggle_edge_mark(e0, false);
    s.toggle_edge_mark(e1, true);
    assert_eq!(s.marked_edges, vec![e1]);
    // Von Hand gemischt (öffentliches Feld): benannt abgelehnt, nichts angelegt.
    s.marked_edges = vec![e0, e1];
    let features = s.document.features().len();
    let mut d = BlendDialog::fillet();
    assert!(d.execute(&mut s));
    let m = d.error().unwrap().to_string();
    assert!(m.contains("verschiedenen Körpern"), "{m}");
    assert!(
        m.contains(&s.bodies[0].name) && m.contains(&s.bodies[1].name),
        "{m}"
    );
    assert_eq!(s.document.features().len(), features);
}

#[test]
fn showing_the_dialog_takes_over_the_clicked_edge_and_switches_picking_on() {
    let ctx = egui::Context::default();
    ctx.set_fonts(planora_app::fonts::definitions());
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    let edge = EdgeRef {
        body: 0,
        edge: edge_between(&s.bodies[0].solid, "Deckel", "Wand 1"),
    };
    s.set_selection(Some(planora_render::Pick::Edge(edge)));
    let mut d = BlendDialog::fillet();
    for _ in 0..2 {
        ctx.run_ui(egui::RawInput::default(), |ui| {
            assert!(d.show(ui.ctx(), &mut s));
        })
        .drop_without_applying_deltas();
    }
    assert!(s.edge_picking);
    assert!(s.chain_marking, "Tangentenkette ist vorgewählt");
    assert!(s.marked_edges.contains(&edge), "angeklickte Kante markiert");
    d.finish(&mut s);
    assert!(!s.edge_picking && s.marked_edges.is_empty());
}

#[test]
fn marks_survive_an_unrelated_refresh_but_not_a_change_of_their_body() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.edge_picking = true;
    let e = EdgeRef {
        body: 0,
        edge: edge_between(&s.bodies[0].solid, "Deckel", "Wand 1"),
    };
    s.toggle_edge_mark(e, false);
    // Unbeteiligte Neuberechnung: weiterer Körper, Hover, Auswahl eines anderen Körpers.
    s.create_box(DVec3::splat(5.0)).unwrap();
    s.set_hover(None);
    s.refresh();
    assert_eq!(s.marked_edges, vec![e], "Markierung bleibt");
    // Änderung davor, die den markierten Körper verändert: Markierung entfällt.
    let id = s.document.features()[0].id;
    s.document
        .update(id, |k| {
            if let FeatureKind::Box { size, .. } = k {
                *size = DVec3::new(12.0, 12.0, 12.0);
            }
        })
        .unwrap();
    s.refresh();
    assert!(s.marked_edges.is_empty(), "Markierung entfällt");
}

#[test]
fn an_unknown_edge_click_keeps_the_marks() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.edge_picking = true;
    let e = EdgeRef {
        body: 0,
        edge: edge_between(&s.bodies[0].solid, "Deckel", "Wand 1"),
    };
    s.toggle_edge_mark(e, false);
    let unknown = EdgeRef {
        body: 0,
        edge: Id::from_index(10_000),
    };
    s.toggle_edge_mark(unknown, false);
    s.toggle_edge_mark(unknown, true);
    s.toggle_edge_mark(
        EdgeRef {
            body: 9,
            edge: e.edge,
        },
        false,
    );
    assert_eq!(s.marked_edges, vec![e]);
}

#[test]
fn switching_the_tangent_chain_on_expands_the_marks_at_once() {
    let mut s = AppState::new();
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(10.0, 0.0),
                },
                Segment::Arc {
                    center: DVec2::new(10.0, 2.0),
                    sweep: PI,
                },
                Segment::Line {
                    to: DVec2::new(0.0, 4.0),
                },
                Segment::Arc {
                    center: DVec2::new(0.0, 2.0),
                    sweep: PI,
                },
            ],
        },
        holes: vec![],
    };
    s.create_box(DVec3::splat(1.0)).unwrap();
    s.bodies[0].solid = s
        .kernel
        .extrude(&profile, &planora_math::Frame::WORLD, 3.0)
        .unwrap();
    s.chain_marking = false;
    let e = EdgeRef {
        body: 0,
        edge: edge_between(&s.bodies[0].solid, "Deckel", "Wand 1"),
    };
    s.toggle_edge_mark(e, false);
    assert_eq!(s.marked_edges.len(), 1);
    let v = s.scene_version;
    s.set_chain_marking(true);
    assert_eq!(s.marked_edges.len(), 4);
    assert!(s.scene_version > v);
    s.set_chain_marking(false);
    assert_eq!(s.marked_edges.len(), 4, "Ausschalten lässt die Markierung");
}
