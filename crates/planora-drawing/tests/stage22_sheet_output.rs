//! Stufe 22 (Teilprojekt 3c-1): der Blattrahmen als Markierungen und seine Ausgabe in SVG und
//! DXF; der alte Innenrahmen ist weg.

mod common;

use common::dxf_read::{self, Geom};
use common::svg_marks::{box_sheet, cylinder_sheet, manual_sheet};
use common::svg_read;
use planora_drawing::annotate::{Mark, MarkKind, TextAnchor};
use planora_drawing::dxf::dxf_char_ok;
use planora_drawing::sheet_frame::*;
use planora_drawing::sheet_info::TitleTexts;
use planora_drawing::sheet_marks::{
    SheetFrame, field_is_cut, fit_text, sheet_frame, sheet_text_width,
};
use planora_drawing::{
    DrawError, Paper, Projection, Sheet, SheetPen, TitleField, dxf_string, svg_string, write_dxf,
    write_svg,
};
use planora_math::DVec2;
use std::collections::BTreeMap;

fn sh(paper: Paper, landscape: bool) -> Sheet {
    Sheet { paper, landscape }
}

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn texts_with(fields: &[(TitleField, &str)]) -> TitleTexts {
    TitleTexts {
        fields: fields.iter().map(|(f, s)| (*f, s.to_string())).collect(),
        revisions: Vec::new(),
        notes: Vec::new(),
    }
}

/// Alle Texte der Markierungen: (Anker, Höhe, Anker-Art, Text).
fn texts_of(f: &SheetFrame) -> Vec<(DVec2, f64, TextAnchor, String)> {
    f.marks
        .iter()
        .filter_map(|m| match m {
            Mark::Text {
                pos,
                height,
                anchor,
                text,
                ..
            } => Some((*pos, *height, *anchor, text.clone())),
            _ => None,
        })
        .collect()
}

fn lines_of(f: &SheetFrame, pen: SheetPen) -> Vec<(DVec2, DVec2)> {
    f.marks
        .iter()
        .filter_map(|m| match m {
            Mark::Line {
                a,
                b,
                kind: MarkKind::Sheet(p),
            } if *p == pen => Some((*a, *b)),
            _ => None,
        })
        .collect()
}

fn has_line(lines: &[(DVec2, DVec2)], a: DVec2, b: DVec2) -> bool {
    lines.iter().any(|(p, q)| {
        ((p.x - a.x).abs() < 1e-9
            && (p.y - a.y).abs() < 1e-9
            && (q.x - b.x).abs() < 1e-9
            && (q.y - b.y).abs() < 1e-9)
            || ((p.x - b.x).abs() < 1e-9
                && (p.y - b.y).abs() < 1e-9
                && (q.x - a.x).abs() < 1e-9
                && (q.y - a.y).abs() < 1e-9)
    })
}

// ---------------------------------------------------------------------------------------------
// (a) Einpassen
// ---------------------------------------------------------------------------------------------

#[test]
fn fit_rules() {
    // Passt: unverändert.
    let f = fit_text("Muster GmbH", 3.5, 98.0);
    assert_eq!(
        (f.text.as_str(), f.height, f.cut),
        ("Muster GmbH", 3.5, false)
    );
    // Viel zu lang: kleinste Höhe, hinten gekürzt, endet auf „…“, passt.
    let long = "W".repeat(80);
    let f = fit_text(&long, 3.5, 98.0);
    assert!((f.height - 2.1).abs() < 1e-12, "{}", f.height);
    assert!(f.cut && f.text.ends_with('…'));
    assert!(f.text.chars().count() < 80);
    assert!(sheet_text_width(&f.text, f.height) <= 98.0);
    // Eines mehr passt nicht mehr (nicht übermäßig gekürzt).
    let more: String = std::iter::repeat_n('W', f.text.chars().count()).collect::<String>() + "…";
    assert!(sheet_text_width(&more, f.height) > 98.0);
    // Knapp zu breit: nur verkleinert, nicht gekürzt.
    let text = "W".repeat(30); // 30 · 0,95 · 3,5 = 99,75
    assert!(sheet_text_width(&text, 3.5) > 98.0);
    let f = fit_text(&text, 3.5, 98.0);
    assert!(!f.cut && f.text == text);
    assert!(f.height < 3.5 && f.height >= 2.1);
    assert!(sheet_text_width(&f.text, f.height) <= 98.0 + 1e-9);
    // Leer bleibt leer.
    let f = fit_text("", 3.5, 98.0);
    assert_eq!((f.text.as_str(), f.height, f.cut), ("", 3.5, false));
    // Die Schätzung nach F11.
    assert!((sheet_text_width("M", 10.0) - 9.5).abs() < 1e-12);
    assert!((sheet_text_width("i.", 10.0) - 6.0).abs() < 1e-12);
    assert!((sheet_text_width("A9", 10.0) - 14.4).abs() < 1e-12);
    assert!((sheet_text_width("ae", 10.0) - 12.0).abs() < 1e-12);
    // Warnung im Fenster: gekürzt nur, wenn es nicht ohne Kürzen passt.
    assert!(!field_is_cut(TitleField::Title, "Halter"));
    // Zahl: 98 mm Platz, 80 × „W“ passen auch in 2,1 mm Höhe nicht; 30 × „W“ nur verkleinert.
    assert!(field_is_cut(TitleField::Number, &"W".repeat(80)));
    assert!(!field_is_cut(TitleField::Number, &"W".repeat(30)));
}

// ---------------------------------------------------------------------------------------------
// (b) Längste Texte bleiben in ihren Zellen
// ---------------------------------------------------------------------------------------------

fn cycle(pattern: &str, n: usize) -> String {
    pattern.chars().cycle().take(n).collect()
}

/// (Links unten, rechts oben) eines Texts auf dem Blatt (Höhe = Schriftgröße über der Grundlinie).
fn text_box(pos: DVec2, h: f64, anchor: TextAnchor, s: &str) -> (DVec2, DVec2) {
    let w = sheet_text_width(s, h);
    let x0 = match anchor {
        TextAnchor::Start => pos.x,
        TextAnchor::Middle => pos.x - w / 2.0,
        TextAnchor::End => pos.x - w,
    };
    (v(x0, pos.y), v(x0 + w, pos.y + h))
}

fn inside(inner: (DVec2, DVec2), outer: (DVec2, DVec2)) -> bool {
    const E: f64 = 1e-9;
    inner.0.x >= outer.0.x - E
        && inner.1.x <= outer.1.x + E
        && inner.0.y >= outer.0.y - E
        && inner.1.y <= outer.1.y + E
}

#[test]
fn longest_texts_stay_in_their_cells() {
    let sheets = [
        sh(Paper::A4, false),
        sh(Paper::A4, true),
        sh(Paper::A3, true),
        sh(Paper::A0, false),
    ];
    for pattern in ["W", "ÄÖÜß", "Ø°±"] {
        let mut fields = BTreeMap::new();
        for f in TitleField::ALL {
            fields.insert(f, cycle(pattern, f.max_chars()));
        }
        let texts = TitleTexts {
            fields,
            revisions: (0..23)
                .map(|_| {
                    [
                        cycle(pattern, 120),
                        cycle(pattern, 120),
                        cycle(pattern, 120),
                        cycle(pattern, 120),
                    ]
                })
                .collect(),
            notes: (0..6).map(|_| cycle(pattern, 100)).collect(),
        };
        for sheet in sheets {
            let layout = sheet_layout(sheet, 23, 6);
            let frame = sheet_frame(&layout, &texts, Projection::First);
            let (t0, _) = layout.title;
            // Alle Zellen als Rechtecke: Schriftfeld, Änderungstabelle (Spalte × Zeile), Hinweisfeld.
            let mut cells: Vec<(DVec2, DVec2)> = TITLE_CELLS
                .iter()
                .map(|c| (v(t0.x + c.x0, t0.y + c.y0), v(t0.x + c.x1, t0.y + c.y1)))
                .collect();
            let (r0, r1) = layout.revisions;
            let mut x = r0.x;
            for w in REV_COLUMNS {
                for k in 0..=REV_VISIBLE {
                    let top = r1.y - REV_ROW_H * k as f64;
                    cells.push((v(x, top - REV_ROW_H), v(x + w, top)));
                }
                x += w;
            }
            cells.push(layout.notes.expect("Hinweisfeld"));
            let mut checked = 0;
            for (pos, h, anchor, s) in texts_of(&frame) {
                if anchor == TextAnchor::Middle {
                    // Rasterbeschriftung: im Rand des Blatts.
                    let b = text_box(pos, h, anchor, &s);
                    let (w, hh) = sheet.size_mm();
                    assert!(inside(b, (v(0.0, 0.0), v(w, hh))), "{s}");
                    continue;
                }
                let b = text_box(pos, h, anchor, &s);
                let probe = v(pos.x + 0.01, pos.y + 0.01);
                let cell = cells
                    .iter()
                    .find(|c| {
                        probe.x >= c.0.x && probe.x <= c.1.x && probe.y >= c.0.y && probe.y <= c.1.y
                    })
                    .unwrap_or_else(|| panic!("{sheet:?} {pattern}: {s:?} in keiner Zelle"));
                assert!(
                    inside(b, *cell),
                    "{sheet:?} {pattern}: {s:?} {b:?} liegt nicht in {cell:?}"
                );
                checked += 1;
            }
            // 17 Feldnamen, 16 Inhalte (ohne Projektion; alle Felder haben Text), 4 Kopfzeilen und
            // 6 × 4 Änderungstexte, 6 Hinweise.
            assert_eq!(checked, 17 + 17 + 4 + 24 + 6, "{sheet:?} {pattern}");
            // Inhalte stehen unter dem Feldnamen ihrer Zelle.
            for s in &TITLE_SLOTS {
                let (_, h, _, _) = texts_of(&frame)
                    .into_iter()
                    .find(|(p, _, a, _)| {
                        *a == TextAnchor::Start
                            && (p.x - (t0.x + s.x0)).abs() < 1e-9
                            && (p.y - (t0.y + s.baseline)).abs() < 1e-9
                    })
                    .expect("Inhalt");
                let cell = TITLE_CELLS
                    .iter()
                    .find(|c| {
                        s.x0 >= c.x0
                            && s.x0 + s.width <= c.x1
                            && s.baseline > c.y0
                            && s.baseline < c.y1
                    })
                    .unwrap();
                let label_baseline = cell.y1 - 1.0 - LABEL_H;
                assert!(s.baseline + h <= label_baseline + 1e-9, "{:?}", s.field);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// (c) Inhalt des Rahmens
// ---------------------------------------------------------------------------------------------

fn a3_texts() -> TitleTexts {
    TitleTexts {
        fields: [
            (TitleField::Title, "Halter"),
            (TitleField::SheetOf, "2 von 3"),
            (TitleField::DocType, "Fertigungszeichnung"),
            (TitleField::Status, "in Arbeit"),
        ]
        .iter()
        .map(|(f, s)| (*f, s.to_string()))
        .collect(),
        revisions: vec![
            [
                "R1".into(),
                "Erste".into(),
                "2026-01-02".into(),
                "dg".into(),
            ],
            [
                "R2".into(),
                "Zweite".into(),
                "2026-02-03".into(),
                "dg".into(),
            ],
        ],
        notes: vec!["Eins".into(), "Zwei".into(), "Drei".into()],
    }
}

#[test]
fn frame_marks_contents() {
    let layout = sheet_layout(sh(Paper::A3, true), 2, 3);
    let frame = sheet_frame(&layout, &a3_texts(), Projection::First);
    let ((fx0, fy0), (fx1, fy1)) = (
        (layout.frame.0.x, layout.frame.0.y),
        (layout.frame.1.x, layout.frame.1.y),
    );
    assert_eq!((fx0, fy0, fx1, fy1), (20.0, 10.0, 410.0, 287.0));
    // Rahmen: vier Linien Heavy (dazu die vier der Schriftfeld-Außenlinie).
    let heavy = lines_of(&frame, SheetPen::Heavy);
    assert_eq!(heavy.len(), 8);
    for (a, b) in [
        (v(fx0, fy0), v(fx1, fy0)),
        (v(fx1, fy0), v(fx1, fy1)),
        (v(fx1, fy1), v(fx0, fy1)),
        (v(fx0, fy1), v(fx0, fy0)),
    ] {
        assert!(has_line(&heavy, a, b));
    }
    let (t0, t1) = layout.title;
    for (a, b) in [
        (v(t0.x, t0.y), v(t1.x, t0.y)),
        (v(t1.x, t0.y), v(t1.x, t1.y)),
        (v(t1.x, t1.y), v(t0.x, t1.y)),
        (v(t0.x, t1.y), v(t0.x, t0.y)),
    ] {
        assert!(has_line(&heavy, a, b));
    }
    // Zentriermarken: Medium (dazu die vier Strecken des Projektionssymbols).
    let medium = lines_of(&frame, SheetPen::Medium);
    assert_eq!(medium.len(), 4 + 4);
    for (a, b) in layout.centre_marks {
        assert!(has_line(&medium, a, b));
    }
    // Teilstriche Thin.
    let thin = lines_of(&frame, SheetPen::Thin);
    for (a, b) in &layout.grid_ticks {
        assert!(has_line(&thin, *a, *b));
    }
    // Zelltrennlinien: jede innere Kante einmal (keine zwei gleichen Linien).
    for (i, l) in thin.iter().enumerate() {
        assert!(
            !thin[i + 1..].iter().any(|m| has_line(&[*m], l.0, l.1)),
            "doppelte Linie {l:?}"
        );
    }
    // Texte.
    let texts = texts_of(&frame);
    let find = |s: &str| -> Vec<&(DVec2, f64, TextAnchor, String)> {
        texts.iter().filter(|t| t.3 == s).collect()
    };
    let title = find("Halter");
    assert_eq!(title.len(), 1);
    assert!((title[0].1 - 5.0).abs() < 1e-12);
    assert_eq!(
        (title[0].0.x, title[0].0.y),
        (t0.x + 1.0, t0.y + 18.5),
        "Titel in seinem Platz"
    );
    let label = find("Blatt");
    assert_eq!(label.len(), 1);
    assert!((label[0].1 - 2.2).abs() < 1e-12);
    assert_eq!(find("2 von 3").len(), 1);
    assert_eq!(find("Fertigungszeichnung").len(), 1);
    let status_label = find("Dokumentstatus");
    assert_eq!(status_label.len(), 1);
    assert!((status_label[0].1 - 2.2).abs() < 1e-12);
    assert_eq!(find("in Arbeit").len(), 1);
    for n in 1..=8 {
        assert_eq!(find(&n.to_string()).len(), 2, "Spalte {n}");
    }
    for c in 'A'..='F' {
        assert_eq!(find(&c.to_string()).len(), 2, "Zeile {c}");
    }
    // Rasterbeschriftung: Mitte, Grundlinie = Mitte − h/2.
    let one = find("1");
    assert!(one.iter().all(|t| t.2 == TextAnchor::Middle));
    assert!(
        layout
            .grid_labels
            .iter()
            .any(|(c, s)| s == "1" && one.iter().any(|t| t.0 == v(c.x, c.y - GRID_TEXT_H / 2.0)))
    );
    // Änderungstabelle: Kopf und Zeilen, neueste unten.
    for h in ["Rev.", "Änderung", "Datum", "Name"] {
        assert!(!find(h).is_empty(), "{h}");
    }
    let (first, second) = (find("Erste")[0], find("Zweite")[0]);
    assert!(first.0.y > second.0.y, "neueste unten");
    // Hinweise in Reihenfolge, von oben nach unten.
    let notes: Vec<_> = ["Eins", "Zwei", "Drei"]
        .iter()
        .map(|s| find(s)[0])
        .collect();
    assert!(notes[0].0.y > notes[1].0.y && notes[1].0.y > notes[2].0.y);
    let nb = layout.notes.unwrap();
    assert_eq!(notes[0].0.x, nb.0.x + 2.0);
    assert!((notes[0].0.y - (nb.1.y - 2.0 - 6.0 + (6.0 - 3.3) / 2.0)).abs() < 1e-9);
    // Alle Texte: Thin, Winkel 0.
    for m in &frame.marks {
        if let Mark::Text { kind, angle, .. } = m {
            assert_eq!((*kind, *angle), (MarkKind::Sheet(SheetPen::Thin), 0.0));
        }
    }
    // Projektionssymbol: Kreise als Bogen 0 … 2π, Medium.
    let arcs: Vec<_> = frame
        .marks
        .iter()
        .filter_map(|m| match m {
            Mark::Arc {
                center,
                r,
                start,
                end,
                kind,
            } => Some((*center, *r, *start, *end, *kind)),
            _ => None,
        })
        .collect();
    assert_eq!(arcs.len(), 2);
    for a in &arcs {
        assert_eq!(
            (a.2, a.3, a.4),
            (
                0.0,
                std::f64::consts::TAU,
                MarkKind::Sheet(SheetPen::Medium)
            )
        );
    }
    let mut radii: Vec<f64> = arcs.iter().map(|a| a.1).collect();
    radii.sort_by(f64::total_cmp);
    assert_eq!(radii, [1.75, 3.5]);

    // Methode 3: Kreise links, Kegelstumpf rechts (nach F7).
    let third = sheet_frame(&layout, &a3_texts(), Projection::Third);
    let first_arc_x = arcs[0].0.x;
    let third_arc_x = third
        .marks
        .iter()
        .find_map(|m| match m {
            Mark::Arc { center, .. } => Some(center.x),
            _ => None,
        })
        .unwrap();
    assert!(first_arc_x > t0.x + PROJECTION_X0 + 2.0 * SYMBOL_H);
    assert!(third_arc_x < t0.x + PROJECTION_X0 + 2.0 * SYMBOL_H);
    assert_ne!(frame, third);
    // Sonst gleich: ohne Symbol dieselben Texte.
    assert_eq!(texts_of(&frame), texts_of(&third));
}

// ---------------------------------------------------------------------------------------------
// (d) Ohne Rahmen nur Inhalt
// ---------------------------------------------------------------------------------------------

#[test]
fn no_frame_is_content_only() {
    let b = box_sheet(false);
    let c = cylinder_sheet(false);
    assert!(b.frame.is_none() && c.frame.is_none());
    let svg = svg_string(&b).unwrap();
    assert!(!svg.contains("<rect") && !svg.contains("Blattrahmen"));
    let dxf = dxf_string(&b).unwrap();
    assert!(!dxf.contains("LWPOLYLINE"));
    for layer in ["BLATTRAHMEN", "BLATT_MITTE", "BLATT"] {
        assert!(!dxf.contains(layer), "{layer}");
    }
    let d = dxf_read::read(&dxf).unwrap();
    assert_eq!(d.entities.len(), 3);
    assert!(d.entities.iter().all(|e| e.kind == "INSERT"));
    // Goldene Dateien: ohne Rahmen genau diese Bytes.
    assert_eq!(svg, include_str!("fixtures/svg_without_marks_box.svg"));
    assert_eq!(
        svg_string(&c).unwrap(),
        include_str!("fixtures/svg_without_marks_cylinder.svg")
    );
    assert_eq!(
        dxf.as_bytes(),
        include_bytes!("fixtures/dxf_without_marks_box.dxf")
    );
    assert_eq!(
        dxf_string(&c).unwrap().as_bytes(),
        include_bytes!("fixtures/dxf_without_marks_cylinder.dxf")
    );
}

// ---------------------------------------------------------------------------------------------
// (e) Mit Rahmen liegt alles auf dem Papier
// ---------------------------------------------------------------------------------------------

fn framed_box() -> planora_drawing::sheet::SheetModel {
    let mut m = box_sheet(false);
    let layout = sheet_layout(m.sheet, 2, 3);
    let mut texts = a3_texts();
    texts.fields.insert(TitleField::Subtitle, "Ø 12 ä".into());
    m.frame = Some(sheet_frame(&layout, &texts, Projection::First));
    m
}

#[test]
fn framed_output_is_on_the_paper() {
    let m = framed_box();
    let svg = svg_string(&m).unwrap();
    assert!(svg.contains("width=\"420mm\""));
    let frame_at = svg.find("<g id=\"Blattrahmen\">").expect("Gruppe");
    let first_view = svg.find("<g id=\"Vorne\">").unwrap();
    let root = svg
        .find("<g transform=\"matrix(1 0 0 -1 0 297)\">")
        .unwrap();
    assert!(root < frame_at && frame_at < first_view);
    assert!(!svg.contains("<rect"));
    // Strichbreiten des Rahmens.
    let group = &svg[frame_at..first_view];
    for w in ["0.7", "0.5", "0.25"] {
        assert!(group.contains(&format!("stroke-width=\"{w}\"")), "{w}");
    }
    // Alle Koordinaten auf dem Papier.
    let r = svg_read::read(&svg);
    let mut count = 0;
    for p in r.paths_of("Blattrahmen") {
        for s in &p.segments {
            let (lo, hi) = s.bbox();
            assert!(
                lo.0 >= 0.0 && lo.1 >= 0.0 && hi.0 <= 420.0 && hi.1 <= 297.0,
                "{lo:?} {hi:?}"
            );
            count += 1;
        }
    }
    assert!(count > 40);
    let tx: Vec<_> = r.texts_of("Blattrahmen").collect();
    assert!(!tx.is_empty());
    for t in &tx {
        assert!((0.0..=420.0).contains(&t.pos.0) && (0.0..=297.0).contains(&t.pos.1));
        assert!(t.upright, "Schrift nicht gespiegelt");
    }
    assert!(tx.iter().any(|t| t.text == "Ø 12 ä"));
    assert_eq!(r.group_ids.first().map(String::as_str), Some("Blattrahmen"));
    // Eine Ansicht gleichen Namens bekommt eine andere Kennung.
    let mut m2 = manual_sheet(Paper::A3, vec![("Blattrahmen", Vec::new())]);
    m2.frame = m.frame.clone();
    let r2 = svg_read::read(&svg_string(&m2).unwrap());
    assert_eq!(r2.group_ids, ["Blattrahmen", "Blattrahmen 2"]);

    // DXF.
    let dxf = dxf_string(&m).unwrap();
    let d = dxf_read::read(&dxf).unwrap_or_else(|e| panic!("Aufbau: {e}"));
    let layers = d.table("LAYER").unwrap();
    let weight = |name: &str| -> (String, String) {
        let r = layers
            .records
            .iter()
            .find(|r| r.name() == name)
            .unwrap_or_else(|| panic!("Ebene {name}"));
        (
            r.get(62).unwrap().to_string(),
            r.get(370).unwrap().to_string(),
        )
    };
    assert_eq!(weight("BLATTRAHMEN"), ("7".into(), "70".into()));
    assert_eq!(weight("BLATT_MITTE"), ("7".into(), "50".into()));
    assert_eq!(weight("BLATT"), ("7".into(), "25".into()));
    // Rahmenobjekte stehen vor den INSERTs.
    let n_insert = d.entities.iter().filter(|e| e.kind == "INSERT").count();
    assert_eq!(n_insert, 3);
    let first_insert = d.entities.iter().position(|e| e.kind == "INSERT").unwrap();
    assert_eq!(first_insert, d.entities.len() - 3);
    assert!(first_insert > 40);
    for e in &d.entities[..first_insert] {
        assert!(
            ["BLATTRAHMEN", "BLATT_MITTE", "BLATT"].contains(&e.layer.as_str()),
            "{}",
            e.layer
        );
        match e.geom().unwrap() {
            Geom::Line { a, b } => {
                for p in [a, b] {
                    assert!((0.0..=420.0).contains(&p.0) && (0.0..=297.0).contains(&p.1));
                }
            }
            Geom::Circle { c, r } => {
                assert!(c.0 - r >= 0.0 && c.0 + r <= 420.0 && c.1 - r >= 0.0 && c.1 + r <= 297.0);
            }
            Geom::Text { .. } => {}
            g => panic!("unerwartet: {g:?}"),
        }
    }
    // Die Zentriermarken liegen auf BLATT_MITTE, der Rahmen auf BLATTRAHMEN.
    assert!(
        d.entities
            .iter()
            .any(|e| e.kind == "LINE" && e.layer == "BLATT_MITTE")
    );
    assert!(
        d.entities
            .iter()
            .any(|e| e.kind == "LINE" && e.layer == "BLATTRAHMEN")
    );
    assert!(d.entities.iter().any(|e| e.kind == "CIRCLE"));
    // Texte: Ø → %%c, ä → \U+00E4.
    let raw: Vec<String> = d
        .entities
        .iter()
        .filter(|e| e.kind == "TEXT")
        .map(|e| e.get(1).unwrap().to_string())
        .collect();
    assert!(raw.iter().any(|s| s == "%%c 12 \\U+00E4"), "{raw:?}");
    assert!(
        d.entities
            .iter()
            .filter(|e| e.kind == "TEXT")
            .all(|e| e.layer == "BLATT")
    );
    // Die Datei ist reines ASCII mit \r\n und enthält keine LWPOLYLINE.
    assert!(dxf.is_ascii() && !dxf.contains("LWPOLYLINE"));
    // Schreiben in einen Strom gibt dieselben Bytes.
    let mut out = Vec::new();
    write_dxf(&m, &mut out).unwrap();
    assert_eq!(out, dxf.as_bytes());
}

#[test]
fn long_sheet_texts_do_not_fail_the_dxf() {
    // Ein Titel aus lauter Umlauten (je 7 Zeichen in der Datei) wird eingepasst, nicht abgelehnt.
    let mut m = box_sheet(false);
    let layout = sheet_layout(m.sheet, 0, 0);
    let texts = texts_with(&[(TitleField::Title, &"Ä".repeat(60))]);
    m.frame = Some(sheet_frame(&layout, &texts, Projection::First));
    let dxf = dxf_string(&m).expect("DXF");
    dxf_read::read(&dxf).unwrap();
}

// ---------------------------------------------------------------------------------------------
// (f) Zeichen
// ---------------------------------------------------------------------------------------------

#[test]
fn dxf_chars() {
    assert!(dxf_char_ok('a') && dxf_char_ok('ä') && dxf_char_ok('Ø') && dxf_char_ok('…'));
    assert!(!dxf_char_ok('😀'));
    assert!(!dxf_char_ok('\u{7}') && !dxf_char_ok('\n') && !dxf_char_ok('\u{85}'));
}

// ---------------------------------------------------------------------------------------------
// (g) Nicht endliche Zahlen im Rahmen
// ---------------------------------------------------------------------------------------------

#[test]
fn nan_in_frame_is_rejected() {
    let bad_marks = [
        Mark::Line {
            a: v(f64::NAN, 0.0),
            b: v(1.0, 1.0),
            kind: MarkKind::Sheet(SheetPen::Heavy),
        },
        Mark::Arc {
            center: v(0.0, f64::INFINITY),
            r: 1.0,
            start: 0.0,
            end: 1.0,
            kind: MarkKind::Sheet(SheetPen::Medium),
        },
        Mark::Text {
            pos: v(1.0, f64::NAN),
            height: 3.5,
            angle: 0.0,
            anchor: TextAnchor::Start,
            text: "x".into(),
            kind: MarkKind::Sheet(SheetPen::Thin),
        },
    ];
    for bad in bad_marks {
        let mut m = framed_box();
        m.frame.as_mut().unwrap().marks.push(bad.clone());
        assert!(
            matches!(svg_string(&m), Err(DrawError::NonFinite)),
            "{bad:?}"
        );
        assert!(
            matches!(dxf_string(&m), Err(DrawError::NonFinite)),
            "{bad:?}"
        );
        let (mut s, mut d) = (Vec::new(), Vec::new());
        assert!(write_svg(&m, &mut s).is_err() && s.is_empty());
        assert!(write_dxf(&m, &mut d).is_err() && d.is_empty());
    }
}
