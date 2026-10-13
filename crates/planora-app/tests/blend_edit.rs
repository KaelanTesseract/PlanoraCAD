//! Verrunden nachträglich ändern (Teilprojekt 8, Task 19).

use planora_app::blend_dialog::{ArtChoice, BlendDialog};
use planora_app::feature_dialog::{FeatureDialog, STALE_MESSAGE};
use planora_app::state::*;
use planora_feature::{BlendArt, FeatureId, FeatureKind, FeatureStatus};
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::{Edge, Id, Solid};
use planora_math::DVec3;
use planora_render::{EdgeRef, Pick};

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

fn with_fillet() -> (AppState, FeatureId) {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s.bodies[0].solid, "Deckel", "Wand 1");
    let id = s
        .add_blend(0, &[e], BlendArt::Rundung { radius: 2.0 }, true)
        .unwrap();
    (s, id)
}

fn art(s: &AppState, id: FeatureId) -> BlendArt {
    match &s.document.feature(id).unwrap().kind {
        FeatureKind::Verrunden { art, .. } => art.clone(),
        k => panic!("{k:?}"),
    }
}

#[test]
fn radius_and_art_change_through_the_dialog() {
    let (mut s, id) = with_fillet();
    let mut d = FeatureDialog::new(&s, id).unwrap();
    assert_eq!(d.labels(), ["Radius"]);
    assert!(d.set_text("Radius", "1,5"));
    d.apply(&mut s).unwrap();
    assert_eq!(art(&s, id), BlendArt::Rundung { radius: 1.5 });
    let mut d = FeatureDialog::new(&s, id).unwrap();
    d.set_blend_art(ArtChoice::FaseWinkel);
    assert_eq!(d.labels(), ["Abstand", "Winkel"]);
    d.set_text("Winkel", "30");
    d.apply(&mut s).unwrap();
    assert_eq!(
        art(&s, id),
        BlendArt::FaseWinkel {
            abstand: 1.5,
            winkel: 30.0
        }
    );
    assert_eq!(s.document.feature(id).unwrap().status, FeatureStatus::Ok);
    assert_eq!(
        s.document.feature(id).unwrap().name,
        "Rundung 1",
        "der Name bleibt"
    );
}

#[test]
fn variable_points_are_added_removed_and_applied() {
    let (mut s, id) = with_fillet();
    let mut d = FeatureDialog::new(&s, id).unwrap();
    d.set_blend_art(ArtChoice::RundungVariabel);
    assert_eq!(
        d.labels(),
        ["Position 1", "Radius 1", "Position 2", "Radius 2"]
    );
    d.add_point();
    assert_eq!(d.labels().len(), 6);
    d.set_text("Radius 2", "1,2");
    d.apply(&mut s).unwrap();
    assert_eq!(
        art(&s, id),
        BlendArt::RundungVariabel {
            punkte: vec![(0.0, 2.0), (50.0, 1.2), (100.0, 2.0)]
        }
    );
    assert_eq!(s.document.feature(id).unwrap().status, FeatureStatus::Ok);
    let mut d = FeatureDialog::new(&s, id).unwrap();
    d.remove_point(1);
    d.remove_point(0);
    assert_eq!(d.labels().len(), 4, "mindestens zwei Punkte bleiben");
}

#[test]
fn the_tangent_chain_and_stale_dialogs() {
    let (mut s, id) = with_fillet();
    let mut d = FeatureDialog::new(&s, id).unwrap();
    d.set_tangent_chain(false);
    let stale = FeatureDialog::new(&s, id).unwrap();
    d.apply(&mut s).unwrap();
    match &s.document.feature(id).unwrap().kind {
        FeatureKind::Verrunden { tangentenkette, .. } => assert!(!tangentenkette),
        k => panic!("{k:?}"),
    }
    assert_eq!(stale.apply(&mut s), Err(STALE_MESSAGE.to_string()));
}

#[test]
fn the_create_dialog_offers_a_variable_radius() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s.bodies[0].solid, "Deckel", "Wand 1");
    s.set_selection(Some(Pick::Edge(EdgeRef { body: 0, edge: e })));
    let mut d = BlendDialog::variable();
    assert_eq!(d.art, ArtChoice::RundungVariabel);
    d.points = vec![("0".into(), "1".into()), ("100".into(), "2".into())];
    d.add_point();
    assert_eq!(d.points.len(), 3);
    d.remove_point(0);
    d.remove_point(0);
    assert_eq!(d.points.len(), 2);
    d.points = vec![("0".into(), "1".into()), ("100".into(), "2".into())];
    assert!(!d.execute(&mut s));
    let f = s.document.features().last().unwrap();
    assert!(matches!(
        &f.kind,
        FeatureKind::Verrunden {
            art: BlendArt::RundungVariabel { .. },
            ..
        }
    ));
    assert_eq!(f.status, FeatureStatus::Ok);
}

fn edge_refs(s: &AppState, id: FeatureId) -> Vec<planora_feature::EdgeRef> {
    match &s.document.feature(id).unwrap().kind {
        FeatureKind::Verrunden { edges, .. } => edges.clone(),
        k => panic!("{k:?}"),
    }
}

#[test]
fn face_one_is_visible_and_selectable() {
    let (mut s, id) = with_fillet();
    let mut d = FeatureDialog::new(&s, id).unwrap();
    let (a, b) = d.first_faces().unwrap();
    assert_ne!(a, b);
    d.set_blend_art(ArtChoice::FaseZwei);
    assert_eq!(d.labels(), ["Abstand 1", "Abstand 2"]);
    d.set_first_face(&b);
    d.apply(&mut s).unwrap();
    let r = edge_refs(&s, id);
    assert_eq!((r[0].a.role.clone(), r[0].b.role.clone()), (b.clone(), a));
    assert_eq!(s.document.feature(id).unwrap().status, FeatureStatus::Ok);
    assert_eq!(
        FeatureDialog::new(&s, id).unwrap().first_faces().unwrap().0,
        b
    );
}

#[test]
fn the_create_dialog_picks_face_one_and_resets_the_second_size() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s.bodies[0].solid, "Deckel", "Wand 1");
    s.set_selection(Some(Pick::Edge(EdgeRef { body: 0, edge: e })));
    let mut d = BlendDialog::chamfer();
    d.set_art(ArtChoice::FaseWinkel);
    d.set_art(ArtChoice::FaseZwei);
    assert_eq!(d.second, "1", "kein Winkel 45 als zweiter Abstand");
    let (a, b) = d.faces(&s).unwrap();
    assert_ne!(a, b);
    d.first_face = Some(b.clone());
    d.first = "1".into();
    d.second = "2".into();
    assert!(!d.execute(&mut s));
    let f = s.document.features().last().unwrap();
    assert_eq!(f.status, FeatureStatus::Ok);
    match &f.kind {
        FeatureKind::Verrunden { edges, .. } => assert_eq!(edges[0].a.role, b),
        k => panic!("{k:?}"),
    }
}

#[test]
fn the_smallest_size_comes_from_the_kernel() {
    let (mut s, id) = with_fillet();
    let mut d = FeatureDialog::new(&s, id).unwrap();
    let min = d.min_size(&s).unwrap();
    assert!(min > 0.0 && min < 0.01, "{min}");
    d.set_text("Radius", "0,00001");
    let err = d.apply(&mut s).unwrap_err();
    assert!(err.contains("zu klein") && err.contains("Radius"), "{err}");
    assert_eq!(art(&s, id), BlendArt::Rundung { radius: 2.0 });
    let variable = {
        let mut v = FeatureDialog::new(&s, id).unwrap();
        v.set_blend_art(ArtChoice::RundungVariabel);
        v.min_size(&s).unwrap()
    };
    assert!(variable >= min, "variabel mindestens so streng");
    let mut c = BlendDialog::fillet();
    let e = edge_between(&s.bodies[0].solid, "Deckel", "Wand 2");
    s.set_selection(Some(Pick::Edge(EdgeRef { body: 0, edge: e })));
    assert_eq!(c.min_size(&s), Some(min));
    c.first = "0,00001".into();
    assert!(c.execute(&mut s), "das Fenster bleibt offen");
    assert!(c.error().unwrap().contains("zu klein"));
}

#[test]
fn the_new_dialog_parts_draw() {
    let ctx = egui::Context::default();
    ctx.set_fonts(planora_app::fonts::definitions());
    let (mut s, id) = with_fillet();
    let mut create = BlendDialog::variable();
    let mut chamfer = BlendDialog::chamfer();
    chamfer.set_art(ArtChoice::FaseZwei);
    let mut edit = FeatureDialog::new(&s, id).unwrap();
    edit.set_blend_art(ArtChoice::RundungVariabel);
    let mut edit2 = FeatureDialog::new(&s, id).unwrap();
    edit2.set_blend_art(ArtChoice::FaseWinkel);
    for _ in 0..3 {
        ctx.run_ui(egui::RawInput::default(), |ui| {
            assert!(create.show(ui.ctx(), &mut s));
            assert!(chamfer.show(ui.ctx(), &mut s));
            assert!(edit.show(ui.ctx(), &mut s));
            assert!(edit2.show(ui.ctx(), &mut s));
        })
        .drop_without_applying_deltas();
    }
    assert_eq!(art(&s, id), BlendArt::Rundung { radius: 2.0 });
}
