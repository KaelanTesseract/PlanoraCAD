//! Positionsnummern im Dokument: Setzen mit Prüfung, Grenze, Nummern nach der Stückliste
//! (Teilprojekt 3c-2, Task 4).

use planora_drawing::model::{Balloon, BalloonId};
use planora_drawing::{
    BodyKey, Drawing, DrawingId, DrawingKind, Paper, Placement, Projection, Scale, Sheet, ViewId,
    ViewKind,
};
use planora_feature::*;
use planora_kernel::BoolOp;
use planora_math::DVec3;
use std::collections::BTreeMap;

fn cube_at(d: &mut Document, name: &str, x: f64) -> FeatureId {
    d.add_feature(
        name,
        FeatureKind::Box {
            size: DVec3::splat(10.0),
            origin: DVec3::new(x, 0.0, 0.0),
        },
    )
    .unwrap()
}

fn body(f: FeatureId) -> BodyRef {
    BodyRef {
        feature: f,
        index: 0,
    }
}

fn key(f: FeatureId) -> BodyKey {
    BodyKey {
        feature: f.0,
        index: 0,
    }
}

fn a3() -> Sheet {
    Sheet {
        paper: Paper::A3,
        landscape: true,
    }
}

const ONE: Scale = Scale { num: 1, den: 1 };

/// Dokument mit einer Zeichnung und einer Ansicht „Vorne“.
fn with_view(d: &mut Document) -> (DrawingId, ViewId) {
    let dr = d.add_drawing("Z", a3(), Projection::First).unwrap();
    let v = d
        .add_view(
            dr,
            ViewKind::Front,
            ONE,
            Placement::Free { x: 200.0, y: 150.0 },
            None,
        )
        .unwrap();
    (dr, v)
}

fn snapshot(d: &Document) -> (Vec<Drawing>, u64) {
    (d.drawings().to_vec(), d.edit_count())
}

#[test]
fn add_and_number() {
    let mut d = Document::new();
    let bolzen = cube_at(&mut d, "Bolzen", 0.0);
    let platte = cube_at(&mut d, "Platte", 20.0);
    let (dr, view) = with_view(&mut d);
    let before = d.edit_count();
    let id = d
        .add_balloon(dr, view, key(platte), (0.25, 0.75), (10.0, 10.0))
        .unwrap();
    assert_eq!(id, BalloonId(1));
    assert_eq!(d.edit_count(), before + 1);
    let b = d.drawing(dr).unwrap().balloon(id).unwrap().clone();
    assert_eq!(
        b,
        Balloon {
            id,
            view,
            body: key(platte),
            anchor: (0.25, 0.75),
            offset: (10.0, 10.0),
        }
    );
    // Zeilen „Bolzen“ (1), „Platte“ (2).
    assert_eq!(d.balloon_numbers(dr), BTreeMap::from([(id, Some(2))]));
    let ib = d
        .add_balloon(dr, view, key(bolzen), (0.0, 0.0), (-10.0, 10.0))
        .unwrap();
    assert_eq!(ib, BalloonId(2));
    assert_eq!(
        d.balloon_numbers(dr),
        BTreeMap::from([(id, Some(2)), (ib, Some(1))])
    );
    // Unbekannte Zeichnung: leer.
    assert!(d.balloon_numbers(DrawingId(99)).is_empty());
    // Anker außerhalb wird auf [0, 1] begrenzt.
    let ic = d
        .add_balloon(dr, view, key(bolzen), (1.5, -0.25), (0.0, 0.0))
        .unwrap();
    assert_eq!(
        d.drawing(dr).unwrap().balloon(ic).unwrap().anchor,
        (1.0, 0.0)
    );

    // Fehler lassen das Dokument unverändert (auch den Zähler).
    let snap = snapshot(&d);
    let fails: Vec<Result<BalloonId, FeatureError>> = vec![
        d.add_balloon(dr, ViewId(42), key(bolzen), (0.5, 0.5), (1.0, 1.0)),
        d.add_balloon(DrawingId(42), view, key(bolzen), (0.5, 0.5), (1.0, 1.0)),
        d.add_balloon(dr, view, key(bolzen), (f64::NAN, 0.5), (1.0, 1.0)),
        d.add_balloon(dr, view, key(bolzen), (0.5, f64::INFINITY), (1.0, 1.0)),
        d.add_balloon(dr, view, key(bolzen), (0.5, 0.5), (f64::NAN, 1.0)),
        d.add_balloon(dr, view, key(bolzen), (0.5, 0.5), (2.0e5, 0.0)),
    ];
    for f in fails {
        assert!(f.is_err(), "{f:?}");
        assert_eq!(snapshot(&d), snap);
    }
    // Größter erlaubter Versatz geht.
    d.add_balloon(dr, view, key(bolzen), (0.5, 0.5), (MAX_POS_MM, 0.0))
        .unwrap();

    // Stücklistenblatt: keine Kreise.
    let stamp = planora_drawing::sheet_info::Stamp {
        date: "2026-10-18".into(),
        user: "dennis".into(),
    };
    let parts = d
        .add_parts_sheets(
            Sheet {
                paper: Paper::A4,
                landscape: false,
            },
            &stamp,
        )
        .unwrap();
    let snap = snapshot(&d);
    assert!(
        d.add_balloon(parts, ViewId(1), key(bolzen), (0.5, 0.5), (1.0, 1.0))
            .is_err()
    );
    assert_eq!(snapshot(&d), snap);
}

#[test]
fn limit_is_200_per_drawing() {
    let mut d = Document::new();
    let a = cube_at(&mut d, "Bolzen", 0.0);
    let (dr, view) = with_view(&mut d);
    for i in 0..200 {
        d.add_balloon(dr, view, key(a), (0.5, 0.5), (i as f64, 0.0))
            .unwrap();
    }
    assert_eq!(d.drawing(dr).unwrap().balloons.len(), 200);
    let snap = snapshot(&d);
    match d.add_balloon(dr, view, key(a), (0.5, 0.5), (1.0, 1.0)) {
        Err(FeatureError::InvalidInput(m)) => assert_eq!(m, too_many_balloons()),
        r => panic!("{r:?}"),
    }
    assert_eq!(snapshot(&d), snap);
    assert_eq!(
        too_many_balloons(),
        "Es sind höchstens 200 Positionsnummern je Zeichnung möglich."
    );
    // Eine zweite Zeichnung hat ihre eigene Grenze.
    let dr2 = d.add_drawing("Z2", a3(), Projection::First).unwrap();
    let v2 = d
        .add_view(
            dr2,
            ViewKind::Top,
            ONE,
            Placement::Free { x: 1.0, y: 1.0 },
            None,
        )
        .unwrap();
    d.add_balloon(dr2, v2, key(a), (0.5, 0.5), (1.0, 1.0))
        .unwrap();
    // Löschen macht wieder Platz.
    let first = d.drawing(dr).unwrap().balloons[0].id;
    d.update_drawing(dr, |x| {
        x.remove_balloon(first);
    })
    .unwrap();
    d.add_balloon(dr, view, key(a), (0.5, 0.5), (1.0, 1.0))
        .unwrap();
    // Die Prüfung selbst: 200 geht, 201 nicht.
    let mut x = d.drawing(dr).unwrap().clone();
    assert!(validate_drawing(&x).is_ok());
    let extra = Balloon {
        id: BalloonId(x.next_balloon),
        ..x.balloons[0].clone()
    };
    x.next_balloon += 1;
    x.balloons.push(extra);
    assert_eq!(validate_drawing(&x), Err(too_many_balloons().to_string()));
}

#[test]
fn validate_rules() {
    let mut d = Drawing::new(DrawingId(1), "Z".into(), a3(), Projection::First);
    let v = d.add_view(
        ViewKind::Front,
        ONE,
        Placement::Free { x: 10.0, y: 10.0 },
        None,
    );
    let good = Balloon {
        id: BalloonId(0),
        view: v,
        body: BodyKey {
            feature: 1,
            index: 0,
        },
        anchor: (0.0, 1.0),
        offset: (MAX_POS_MM * 0.6, MAX_POS_MM * 0.8),
    };
    d.add_balloon(good.clone());
    assert_eq!(validate_drawing(&d), Ok(()));
    let check = |f: &dyn Fn(&mut Drawing)| {
        let mut x = d.clone();
        f(&mut x);
        validate_drawing(&x)
    };
    // Anker endlich in [0, 1].
    for a in [
        (-0.01, 0.5),
        (0.5, 1.01),
        (f64::NAN, 0.5),
        (0.5, f64::INFINITY),
    ] {
        assert!(check(&|x| x.balloons[0].anchor = a).is_err(), "{a:?}");
    }
    // Versatz endlich, Betrag höchstens MAX_POS_MM (0,6/0,8 ergibt genau 1; etwas mehr nicht).
    assert!(check(&|x| x.balloons[0].offset = (MAX_POS_MM * 0.6, MAX_POS_MM * 0.81)).is_err());
    // Schräg: der Betrag zählt, nicht die einzelnen Richtungen (0,75·√2 > 1, 0,7·√2 < 1).
    assert!(check(&|x| x.balloons[0].offset = (MAX_POS_MM * 0.75, MAX_POS_MM * 0.75)).is_err());
    assert!(check(&|x| x.balloons[0].offset = (-MAX_POS_MM * 0.75, MAX_POS_MM * 0.75)).is_err());
    assert!(check(&|x| x.balloons[0].offset = (MAX_POS_MM * 0.7, -MAX_POS_MM * 0.7)).is_ok());
    assert!(check(&|x| x.balloons[0].offset = (f64::NAN, 0.0)).is_err());
    assert!(check(&|x| x.balloons[0].offset = (0.0, f64::NEG_INFINITY)).is_err());
    // Ansicht vorhanden.
    assert!(check(&|x| x.balloons[0].view = ViewId(9)).is_err());
    // Kennungen 1..next_balloon, eindeutig.
    assert!(check(&|x| x.balloons[0].id = BalloonId(0)).is_err());
    assert!(check(&|x| x.balloons[0].id = BalloonId(2)).is_err());
    assert!(check(&|x| x.next_balloon = 1).is_err());
    assert!(check(&|x| x.next_balloon = 0).is_err());
    // Auch ohne Kreise: Zähler 0 und u64::MAX sind ungültig.
    assert!(
        check(&|x| {
            x.balloons.clear();
            x.next_balloon = 0;
        })
        .is_err()
    );
    assert!(
        check(&|x| {
            x.balloons.clear();
            x.next_balloon = u64::MAX;
        })
        .is_err()
    );
    assert!(check(&|x| x.next_balloon = u64::MAX).is_err());
    assert!(
        check(&|x| {
            let mut b = x.balloons[0].clone();
            x.next_balloon = 5;
            b.id = BalloonId(1);
            x.balloons.push(b);
        })
        .is_err()
    );
    // Zwei verschiedene gültige Kennungen sind in Ordnung, auch mit Lücke.
    assert!(
        check(&|x| {
            let mut b = x.balloons[0].clone();
            x.next_balloon = 5;
            b.id = BalloonId(4);
            x.balloons.push(b);
        })
        .is_ok()
    );
    // Stücklistenblatt: kein Kreis (auch ohne Ansicht nicht).
    let mut p = Drawing::new(DrawingId(2), "Stückliste".into(), a3(), Projection::First);
    p.kind = DrawingKind::PartsList { page: 1 };
    assert_eq!(validate_drawing(&p), Ok(()));
    p.next_balloon = 2;
    assert_eq!(validate_drawing(&p), Ok(()));
    p.balloons.push(Balloon {
        id: BalloonId(1),
        ..good
    });
    // Benannte Meldung (nicht bloß „Ansicht fehlt“).
    let e = validate_drawing(&p).unwrap_err();
    assert!(
        e.contains("Stücklistenblatt") && e.contains("Positionsnummern"),
        "{e}"
    );
}

// F5: drei gleichnamige Körper in einer Zeile; der Kreis am mittleren Körper trägt die
// Nummer der Zeile (nicht nur der erste oder letzte Körper zählt).
#[test]
fn middle_body_of_a_row_has_the_row_number() {
    let mut d = Document::new();
    let platte = cube_at(&mut d, "Platte", 0.0);
    let b1 = cube_at(&mut d, "Bolzen", 20.0);
    let b2 = cube_at(&mut d, "Bolzen", 40.0);
    let b3 = cube_at(&mut d, "Bolzen", 60.0);
    let (dr, view) = with_view(&mut d);
    let add = |d: &mut Document, k: BodyKey| {
        d.add_balloon(dr, view, k, (0.5, 0.5), (10.0, 10.0))
            .unwrap()
    };
    let mid = add(&mut d, key(b2));
    let first = add(&mut d, key(b1));
    let last = add(&mut d, key(b3));
    let p = add(&mut d, key(platte));
    assert_eq!(
        d.balloon_numbers(dr),
        BTreeMap::from([
            (mid, Some(2)),
            (first, Some(2)),
            (last, Some(2)),
            (p, Some(1))
        ])
    );
    // Durch Umbenennen entstanden: „Stift“ wird zu „Platte“ und liegt dann in der Mitte.
    let mut d = Document::new();
    let a = cube_at(&mut d, "Platte", 0.0);
    let s = cube_at(&mut d, "Stift", 20.0);
    let c = cube_at(&mut d, "Platte", 40.0);
    let (dr, view) = with_view(&mut d);
    let bs = d
        .add_balloon(dr, view, key(s), (0.5, 0.5), (10.0, 10.0))
        .unwrap();
    assert_eq!(d.balloon_numbers(dr)[&bs], Some(2));
    d.rename(s, "Platte").unwrap();
    assert_eq!(d.parts_rows()[0].keys, vec![key(a), key(s), key(c)]);
    assert_eq!(d.balloon_numbers(dr)[&bs], Some(1));
}

#[test]
fn numbers_follow_the_part() {
    let mut d = Document::new();
    let bolzen = cube_at(&mut d, "Bolzen", 0.0);
    let platte = cube_at(&mut d, "Platte", 20.0);
    let stift = cube_at(&mut d, "Stift", 40.0);
    let (dr, view) = with_view(&mut d);
    let add = |d: &mut Document, k: BodyKey| {
        d.add_balloon(dr, view, k, (0.5, 0.5), (10.0, 10.0))
            .unwrap()
    };
    let bb = add(&mut d, key(bolzen));
    let bp = add(&mut d, key(platte));
    let bs = add(&mut d, key(stift));
    assert_eq!(
        d.balloon_numbers(dr),
        BTreeMap::from([(bb, Some(1)), (bp, Some(2)), (bs, Some(3))])
    );
    // Unterdrücken: verloren; Zurückholen: wieder da, Nummern danach rücken.
    d.set_suppressed(bolzen, true).unwrap();
    assert_eq!(
        d.balloon_numbers(dr),
        BTreeMap::from([(bb, None), (bp, Some(1)), (bs, Some(2))])
    );
    d.set_suppressed(bolzen, false).unwrap();
    assert_eq!(d.balloon_numbers(dr)[&bb], Some(1));
    // Umbenennen in den Namen eines anderen Körpers: eine Zeile, dieselbe Nummer (F5).
    d.rename(stift, "Bolzen").unwrap();
    assert_eq!(
        d.balloon_numbers(dr),
        BTreeMap::from([(bb, Some(1)), (bp, Some(2)), (bs, Some(1))])
    );
    d.rename(stift, "Stift").unwrap();
    assert_eq!(d.balloon_numbers(dr)[&bs], Some(3));
    // Vereinigung: das Ziel behält seine Nummer, das Werkzeug ist verloren.
    let u = d
        .add_feature(
            "Vereinigung",
            FeatureKind::Boolean {
                target: body(platte),
                tool: body(stift),
                op: BoolOp::Union,
            },
        )
        .unwrap();
    assert_eq!(d.feature(u).unwrap().status, FeatureStatus::Ok);
    let n = d.balloon_numbers(dr);
    assert_eq!(n[&bs], None);
    assert_eq!(n[&bp], Some(2), "Ziel: Zeile „Vereinigung“ an Stelle 2");
    assert_eq!(n[&bb], Some(1));
    // Körper löschen: verloren; die anderen rücken auf.
    d.delete(u).unwrap();
    d.delete(bolzen).unwrap();
    let n = d.balloon_numbers(dr);
    assert_eq!(n[&bb], None);
    assert_eq!(n[&bp], Some(1));
    assert_eq!(n[&bs], Some(2));
    // Die Kreise selbst bleiben in der Zeichnung.
    assert_eq!(d.drawing(dr).unwrap().balloons.len(), 3);
}
