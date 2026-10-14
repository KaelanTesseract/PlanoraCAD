//! Rundungen und Fasen in der App (Teilprojekt 8, Task 8).

use planora_app::blend_dialog::{ArtChoice, BlendDialog, NO_EDGE};
use planora_app::state::*;
use planora_feature::{BlendArt, FeatureKind};
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::{Edge, Id, Solid};
use planora_math::{DVec3, PI};
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

fn with_box() -> (AppState, Id<Edge>) {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s.bodies[0].solid, "Deckel", "Wand 1");
    (s, e)
}

#[test]
fn add_blend_replaces_the_body_and_keeps_its_name() {
    let (mut s, e) = with_box();
    let id = s
        .add_blend(0, &[e], BlendArt::Rundung { radius: 2.0 }, true)
        .unwrap();
    assert_eq!(s.bodies.len(), 1);
    assert_eq!(s.bodies[0].name, "Quader 1");
    assert_eq!(s.document.feature(id).unwrap().name, "Rundung 1");
    assert_eq!(s.selected_body, Some(0));
    let want = 1000.0 - (1.0 - PI / 4.0) * 40.0;
    // Sehnentoleranz der App (0,05): Netzfehler der Rundungsfläche etwa 1 mm³.
    assert!(
        (s.bodies[0].mass.volume - want).abs() / want < 3e-3,
        "{}",
        s.bodies[0].mass.volume
    );
    assert!(matches!(
        s.document.feature(id).unwrap().kind,
        FeatureKind::Verrunden { .. }
    ));
}

#[test]
fn a_failing_blend_leaves_the_document_unchanged() {
    let (mut s, e) = with_box();
    let n = s.document.features().len();
    let r = s.add_blend(0, &[e], BlendArt::Rundung { radius: 20.0 }, true);
    let msg = r.unwrap_err();
    assert!(msg.contains("zu groß"), "{msg}");
    assert_eq!(s.document.features().len(), n);
    assert_eq!(s.message.as_deref(), Some(msg.as_str()));
    assert!(
        s.add_blend(5, &[e], BlendArt::Rundung { radius: 1.0 }, true)
            .is_err()
    );
    assert!(
        s.add_blend(0, &[], BlendArt::Rundung { radius: 1.0 }, true)
            .is_err()
    );
    assert_eq!(s.document.features().len(), n);
}

#[test]
fn the_dialog_needs_a_clicked_edge_and_valid_numbers() {
    let (mut s, e) = with_box();
    let mut d = BlendDialog::fillet();
    assert!(d.execute(&mut s), "ohne Kante bleibt das Fenster offen");
    assert_eq!(d.error(), Some(NO_EDGE));
    s.set_selection(Some(Pick::Edge(EdgeRef { body: 0, edge: e })));
    d.first = "abc".into();
    assert!(d.execute(&mut s));
    assert!(d.error().unwrap().contains("Radius"));
    d.first = "2,5".into();
    assert_eq!(d.art_value(), Ok(BlendArt::Rundung { radius: 2.5 }));
    assert!(!d.execute(&mut s), "nach Erfolg schließt das Fenster");
    assert_eq!(s.document.features().len(), 2);
}

#[test]
fn chamfer_choices_read_their_fields() {
    let mut d = BlendDialog::chamfer();
    assert_eq!(d.title(), "Fase");
    d.art = ArtChoice::FaseWinkel;
    d.first = "1".into();
    d.second = "30".into();
    assert_eq!(
        d.art_value(),
        Ok(BlendArt::FaseWinkel {
            abstand: 1.0,
            winkel: 30.0
        })
    );
    d.second = "180".into();
    assert!(d.art_value().unwrap_err().contains("Winkel"));
    d.art = ArtChoice::FaseZwei;
    d.second = "2".into();
    assert_eq!(
        d.art_value(),
        Ok(BlendArt::FaseZwei {
            abstand1: 1.0,
            abstand2: 2.0
        })
    );
    assert_eq!(
        ArtChoice::FaseZwei.fields(),
        ("Abstand 1", Some("Abstand 2"))
    );
}

#[test]
fn the_chamfer_angle_range_is_one_to_179_degrees() {
    // Wie im Kernel (Entscheidung P4, Ruling R-9): 1° bis 179°, Grenzen eingeschlossen.
    let mut d = BlendDialog::chamfer();
    d.art = ArtChoice::FaseWinkel;
    d.first = "1".into();
    for (text, ok) in [
        ("0", false),
        ("0,5", false),
        ("1", true),
        ("45", true),
        ("179", true),
        ("179,5", false),
        ("180", false),
        ("-30", false),
        ("nan", false),
        ("inf", false),
        ("", false),
    ] {
        d.second = text.into();
        let r = d.art_value();
        assert_eq!(r.is_ok(), ok, "{text}: {r:?}");
        if let Err(e) = r {
            assert!(e.contains("zwischen 1° und 179°"), "{e}");
        }
    }
    // Maße: größer als null und im darstellbaren Bereich.
    d.art = ArtChoice::FaseGleich;
    for text in ["0", "-1", "1e300", "inf", "nan", "x"] {
        d.first = text.into();
        assert!(d.art_value().is_err(), "{text}");
    }
}

#[test]
fn a_failing_dialog_keeps_the_window_open_and_shows_the_message() {
    let (mut s, e) = with_box();
    s.set_selection(Some(Pick::Edge(EdgeRef { body: 0, edge: e })));
    let mut d = BlendDialog::fillet();
    d.first = "20".into();
    assert!(d.execute(&mut s));
    assert!(d.error().unwrap().contains("zu groß"));
    assert_eq!(s.document.features().len(), 1);
    assert!(s.message.as_deref().unwrap().contains("zu groß"));
}
