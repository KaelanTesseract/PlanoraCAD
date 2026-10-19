//! Stufe 31: Modell der Schnittansichten: Arten, Linienlage, Buchstaben, Namen, Schlüssel.

use planora_drawing::section::SectionPlane;
use planora_drawing::sheet::build_sheet;
use planora_drawing::{
    Basis, Drawing, DrawingId, MAX_SECTIONS, Paper, Placement, Projection, SECTION_LETTERS, Scale,
    Sheet, ViewId, ViewKey, ViewKind, ViewLines, plane_offset_from_view, section_kind,
    section_line_coord, sections_ok_on,
};
use std::collections::BTreeMap;

const A3: Sheet = Sheet {
    paper: Paper::A3,
    landscape: true,
};
const A4: Sheet = Sheet {
    paper: Paper::A4,
    landscape: false,
};
const S11: Scale = Scale { num: 1, den: 1 };
const FREE: Placement = Placement::Free { x: 0.0, y: 0.0 };

fn drawing_with_front() -> (Drawing, ViewId) {
    let mut d = Drawing::new(DrawingId(1), "Blatt".into(), A3, Projection::First);
    let front = d.add_view(ViewKind::Front, S11, FREE, None);
    (d, front)
}

#[test]
fn section_kind_pins() {
    use ViewKind::*;
    // (Quelle, waagerecht, umgedreht) -> Art
    let table = [
        (Front, true, false, Top),
        (Front, false, false, Left),
        (Front, true, true, Bottom),
        (Front, false, true, Right),
        (Top, true, false, Back),
        (Top, false, false, Left),
        (Top, true, true, Front),
        (Top, false, true, Right),
        (Right, true, false, Top),
        (Right, false, false, Front),
        (Right, true, true, Bottom),
        (Right, false, true, Back),
        (Back, true, false, Top),
        (Back, false, false, Right),
        (Left, true, false, Top),
        (Left, false, false, Back),
        (Bottom, true, false, Front),
        (Bottom, false, false, Left),
    ];
    for (parent, horizontal, flipped, want) in table {
        assert_eq!(
            section_kind(parent, horizontal, flipped),
            Some(want),
            "{parent:?} horizontal={horizontal} flipped={flipped}"
        );
    }
    for horizontal in [true, false] {
        for flipped in [true, false] {
            assert_eq!(section_kind(Isometric, horizontal, flipped), None);
        }
    }
}

#[test]
fn line_coord_roundtrip() {
    use ViewKind::*;
    let p = |kind, offset| SectionPlane { kind, offset };
    assert_eq!(section_line_coord(Front, &p(Top, 5.0)), Some((true, 5.0)));
    assert_eq!(
        section_line_coord(Front, &p(Left, -3.0)),
        Some((false, -3.0))
    );
    assert_eq!(section_line_coord(Top, &p(Back, 7.0)), Some((true, 7.0)));
    // Vorzeichen: Unten dreht die Blattachse um (ey = −Y), Hinten blickt gegen Vorne.
    assert_eq!(
        section_line_coord(Bottom, &p(Front, 4.0)),
        Some((true, -4.0))
    );
    assert_eq!(
        section_line_coord(Left, &p(Front, 3.0)),
        Some((false, -3.0))
    );
    assert_eq!(
        section_line_coord(Right, &p(Front, 6.0)),
        Some((false, 6.0))
    );
    // Ebene quer zur Blickrichtung der Quelle oder isometrisch: keine Linie.
    assert_eq!(section_line_coord(Front, &p(Back, 1.0)), None);
    assert_eq!(section_line_coord(Front, &p(Isometric, 1.0)), None);
    assert_eq!(section_line_coord(Isometric, &p(Top, 1.0)), None);
    // Umkehrung jedes Falls.
    for parent in ViewKind::ALL {
        for kind in ViewKind::ALL {
            for off in [-12.5, 0.0, 3.25, 40.0] {
                let plane = p(kind, off);
                match section_line_coord(parent, &plane) {
                    Some((_, coord)) => {
                        assert_eq!(plane_offset_from_view(parent, kind, coord), Some(off));
                    }
                    None => assert_eq!(plane_offset_from_view(parent, kind, off), None),
                }
            }
        }
    }
}

#[test]
fn line_coord_agrees_with_the_projection() {
    // Ein Punkt auf der Ebene bildet sich in der Quellansicht auf die Linienkoordinate ab.
    use ViewKind::*;
    for parent in [Front, Back, Left, Right, Top, Bottom] {
        for kind in [Front, Back, Left, Right, Top, Bottom] {
            let plane = SectionPlane { kind, offset: 9.0 };
            let Some((horizontal, coord)) = section_line_coord(parent, &plane) else {
                continue;
            };
            let (a, sign) = plane.axis().unwrap();
            let _ = sign;
            let mut pt = planora_math::DVec3::new(1.5, 2.5, 3.5);
            pt[a] = 9.0;
            let img = Basis::of(parent).project(pt);
            assert_eq!(if horizontal { img.y } else { img.x }, coord);
        }
    }
}

#[test]
fn letters_and_names() {
    assert_eq!(SECTION_LETTERS.chars().count(), 23);
    for c in ['I', 'O', 'Q'] {
        assert!(!SECTION_LETTERS.contains(c));
    }
    let (mut d, front) = drawing_with_front();
    assert_eq!(d.next_letter(), Some('A'));
    let a = d.add_section(front, ViewKind::Top, 0.0, 80.0).unwrap();
    let b = d.add_section(front, ViewKind::Top, 1.0, 120.0).unwrap();
    let c = d.add_section(front, ViewKind::Left, 2.0, 80.0).unwrap();
    let letters: Vec<char> = [a, b, c]
        .iter()
        .map(|id| d.view(*id).unwrap().section.unwrap().letter)
        .collect();
    assert_eq!(letters, vec!['A', 'B', 'C']);
    // Maßstab, verdeckte Kanten, Mittellinien, Lage, Schraffur.
    let v = d.view(a).unwrap();
    assert_eq!(v.scale, S11);
    assert!(!v.show_hidden && v.centerlines);
    assert_eq!(v.parent, Some(front));
    assert_eq!(v.placement, Placement::Aligned { distance: 80.0 });
    let h = v.section.unwrap().hatch;
    assert!(h.on && h.spacing == 3.0);
    // B löschen: der nächste Schnitt bekommt wieder B.
    d.remove_view(b);
    assert_eq!(d.next_letter(), Some('B'));
    let again = d.add_section(front, ViewKind::Top, 4.0, 90.0).unwrap();
    assert_eq!(d.view(again).unwrap().section.unwrap().letter, 'B');
}

#[test]
fn at_most_twelve_sections() {
    let (mut d, front) = drawing_with_front();
    for i in 0..MAX_SECTIONS {
        d.add_section(front, ViewKind::Top, i as f64, 80.0).unwrap();
    }
    assert_eq!(d.views.len(), 13);
    let e = d.add_section(front, ViewKind::Top, 99.0, 80.0).unwrap_err();
    assert_eq!(e, "Es sind höchstens 12 Schnitte je Blatt möglich.");
    assert_eq!(d.views.len(), 13, "ein abgelehnter Schnitt ändert nichts");
    let next = d.next_view;
    assert!(d.add_section(front, ViewKind::Top, 99.0, 80.0).is_err());
    assert_eq!(d.next_view, next, "der Zähler bleibt");
}

#[test]
fn add_section_needs_a_straight_source() {
    let (mut d, front) = drawing_with_front();
    let iso = d.add_view(ViewKind::Isometric, S11, FREE, None);
    let msg = "Ein Schnitt geht nur aus einer geraden Ansicht.";
    assert_eq!(
        d.add_section(iso, ViewKind::Top, 0.0, 80.0).unwrap_err(),
        msg
    );
    assert_eq!(
        d.add_section(ViewId(99), ViewKind::Top, 0.0, 80.0)
            .unwrap_err(),
        msg
    );
    let s = d.add_section(front, ViewKind::Top, 0.0, 80.0).unwrap();
    assert_eq!(
        d.add_section(s, ViewKind::Left, 0.0, 80.0).unwrap_err(),
        msg
    );
}

#[test]
fn add_section_takes_the_scale_of_its_source() {
    let (mut d, _) = drawing_with_front();
    let scaled = d.add_view(ViewKind::Right, Scale { num: 2, den: 1 }, FREE, None);
    let s = d.add_section(scaled, ViewKind::Top, 0.0, 80.0).unwrap();
    assert_eq!(d.view(s).unwrap().scale, Scale { num: 2, den: 1 });
}

#[test]
fn names_are_not_influenced_by_sections() {
    let (mut d, front) = drawing_with_front();
    let top = d.add_view(
        ViewKind::Top,
        S11,
        Placement::Aligned { distance: 50.0 },
        Some(front),
    );
    let s1 = d.add_section(front, ViewKind::Top, 0.0, 80.0).unwrap();
    let s2 = d.add_section(front, ViewKind::Left, 1.0, 80.0).unwrap();
    let top2 = d.add_view(
        ViewKind::Top,
        S11,
        Placement::Aligned { distance: 90.0 },
        Some(front),
    );
    let names = d.view_names();
    assert_eq!(names[&front], "Vorne");
    assert_eq!(names[&top], "Oben");
    assert_eq!(names[&s1], "Schnitt A\u{2013}A");
    assert_eq!(names[&s2], "Schnitt B\u{2013}B");
    assert_eq!(
        names[&top2], "Oben 2",
        "Schnitte (Art Oben) zählen nicht mit"
    );
    assert_eq!(names.len(), 5);

    // build_sheet nutzt dieselben Namen.
    let lines = ViewLines {
        kind: ViewKind::Front,
        basis: Basis::of(ViewKind::Front),
        extent: 1.0,
        strokes: vec![],
        pieces: vec![],
        bbox: ((0.0, 0.0), (10.0, 10.0)),
        centerlines: vec![],
        dropped_splits: 0,
    };
    let results: BTreeMap<ViewId, &ViewLines> = d.views.iter().map(|v| (v.id, &lines)).collect();
    let sheet = build_sheet(&d, &results);
    let sheet_names: BTreeMap<ViewId, String> =
        sheet.views.iter().map(|v| (v.id, v.name.clone())).collect();
    assert_eq!(sheet_names, names);
}

#[test]
fn view_key() {
    let (mut d, front) = drawing_with_front();
    let a = d.add_section(front, ViewKind::Top, 5.0, 80.0).unwrap();
    let b = d.add_section(front, ViewKind::Top, 5.0, 120.0).unwrap();
    let c = d.add_section(front, ViewKind::Top, 6.0, 80.0).unwrap();
    let e = d.add_section(front, ViewKind::Bottom, 5.0, 80.0).unwrap();
    d.views
        .iter_mut()
        .find(|v| v.id == b)
        .unwrap()
        .section
        .as_mut()
        .unwrap()
        .hatch
        .spacing = 7.0;
    d.views
        .iter_mut()
        .find(|v| v.id == b)
        .unwrap()
        .section
        .as_mut()
        .unwrap()
        .hatch
        .on = false;
    let key = |id| ViewKey::of(d.view(id).unwrap());
    assert_eq!(
        key(a),
        key(b),
        "Schraffur und Abstand gehören nicht zum Schlüssel"
    );
    assert_ne!(key(a), key(c), "anderer Versatz, anderer Schlüssel");
    assert_eq!(key(front), ViewKey::Plain(ViewKind::Front));
    assert_ne!(key(front), key(a));
    assert_eq!(
        key(a),
        ViewKey::Section(SectionPlane {
            kind: ViewKind::Top,
            offset: 5.0
        })
    );
    // Gegenansicht bei gleichem Versatz: anderer Schlüssel.
    assert_ne!(key(a), key(e));
}

#[test]
fn hatch_spacing_depends_on_the_sheet() {
    let (mut d, front) = drawing_with_front();
    let s = d.add_section(front, ViewKind::Top, 0.0, 80.0).unwrap();
    let set = |d: &mut Drawing, x: f64| {
        d.views
            .iter_mut()
            .find(|v| v.id == s)
            .unwrap()
            .section
            .as_mut()
            .unwrap()
            .hatch
            .spacing = x;
    };
    set(&mut d, 0.7);
    assert!(sections_ok_on(&d, A3).is_ok());
    let e = sections_ok_on(&d, A4).unwrap_err();
    assert_eq!(
        e,
        "Der Schraffurabstand von „Schnitt A\u{2013}A“ wäre auf diesem Blatt kleiner als 0,5 mm."
    );
    set(&mut d, 0.5);
    assert!(sections_ok_on(&d, A3).is_ok(), "genau 0,5 mm ist erlaubt");
    set(&mut d, 0.4999);
    assert!(sections_ok_on(&d, A3).is_err());
    set(&mut d, 50.0);
    assert!(sections_ok_on(&d, A3).is_ok());
    set(&mut d, 50.001);
    assert_eq!(
        sections_ok_on(&d, A3).unwrap_err(),
        "Der Schraffurabstand ist zu groß (höchstens 50 mm)."
    );
    set(&mut d, f64::NAN);
    assert!(sections_ok_on(&d, A3).is_err());
}
