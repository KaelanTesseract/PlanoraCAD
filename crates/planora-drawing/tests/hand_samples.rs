//! Proben für den Handtest: je Körper eine Zeichnung (A3 quer, Ansichten Vorne, Oben, Links und
//! Isometrisch, verdeckte Kanten sichtbar) als SVG und DXF, dazu `ERWARTUNG.txt` mit den
//! erwarteten Gesamtlängen und der Liste der Dinge, die man in Fusion 360 und im Browser prüft.
//!
//! Der Test `write_samples_for_the_hand_test` ist ausgeschaltet (er rechnet lange, besser mit
//! `--release`) und wird so gestartet:
//!
//! `cargo test --release -p planora-drawing --test hand_samples -- --ignored`
//!
//! Der Zielordner ist `$PLANORA_SAMPLES_DIR`, sonst `D:\Testumgebung\zeichnung-proben`.
//!
//! Die erwarteten Längen kommen aus einfachen Formeln (Kantenlängen der Körper), nicht aus der
//! Bibliothek, und werden beim Schreiben gegen die Bibliothek geprüft. Wo es keine einfache
//! Formel gibt, steht nur die Länge der Bibliothek in der Datei (zum Vergleichen mit dem, was
//! das Fremdprogramm misst).

mod common;

use common::check::piece_length_sum;
use common::model::*;
use planora_drawing::layout::{GAP_MM, align_dir, view_extent};
use planora_drawing::model::{
    Drawing, DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind,
};
use planora_drawing::sheet::build_sheet;
use planora_drawing::sheet_frame::EDGE_MARGIN_MM;
use planora_drawing::{DrawBody, ViewLines, Visibility, compute_view, dxf_string, svg_string};
use planora_exchange::Progress;
use planora_kernel::Solid;
use planora_math::DVec3;
use std::collections::BTreeMap;
use std::f64::consts::PI;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Namen aller Proben in der Reihenfolge der Ausgabe.
const NAMES: [&str; 13] = [
    "Quader",
    "Zylinder",
    "Kugel",
    "Kegelstumpf",
    "Bohrung_X",
    "Bohrung_Y",
    "Winkel",
    "Tasche",
    "Durchbruch",
    "Rundung",
    "Torus",
    "Freiform",
    "Dritter_Winkel",
];

/// Die vier Ansichten jeder Probe.
const KINDS: [ViewKind; 4] = [
    ViewKind::Front,
    ViewKind::Top,
    ViewKind::Left,
    ViewKind::Isometric,
];

/// Erwartete Gesamtlängen (sichtbar, verdeckt) in mm einer Ansicht, aus einer Formel.
struct Expect {
    kind: ViewKind,
    visible: f64,
    hidden: f64,
}

struct Sample {
    name: &'static str,
    /// Was der Körper ist (für ERWARTUNG.txt).
    about: &'static str,
    solid: Solid,
    projection: Projection,
    expect: Vec<Expect>,
}

fn ex(kind: ViewKind, visible: f64, hidden: f64) -> Expect {
    Expect {
        kind,
        visible,
        hidden,
    }
}

/// Gesamtlängen der Ansichten eines Quaders `a × b × c` ohne weitere Merkmale (Formeln: die
/// Umrisse der Ansichten; die hinteren Kanten decken die vorderen).
fn box_expect(a: f64, b: f64, c: f64) -> Vec<Expect> {
    // Isometrisch: jede Kante der Länge L erscheint mit L·√(2/3); neun sichtbare, drei
    // verdeckte Kanten (je Richtung drei und eine).
    let k = (2.0f64 / 3.0).sqrt();
    vec![
        ex(ViewKind::Front, 2.0 * (a + c), 0.0),
        ex(ViewKind::Top, 2.0 * (a + b), 0.0),
        ex(ViewKind::Left, 2.0 * (b + c), 0.0),
        ex(ViewKind::Isometric, 3.0 * (a + b + c) * k, (a + b + c) * k),
    ]
}

fn sample(name: &str) -> Sample {
    use ViewKind::{Front, Left, Top};
    let first = Projection::First;
    match name {
        "Quader" => Sample {
            name: "Quader",
            about: "Quader 10 x 20 x 30 mm",
            solid: box_solid(10.0, 20.0, 30.0),
            projection: first,
            expect: box_expect(10.0, 20.0, 30.0),
        },
        "Zylinder" => {
            let (r, h) = (5.0, 12.0);
            Sample {
                name: "Zylinder",
                about: "Zylinder Radius 5, Höhe 12 mm, stehend",
                solid: cylinder_solid(r, h),
                projection: first,
                expect: vec![
                    // Von der Seite: zwei Mantellinien und die beiden Kreise von der Kante her.
                    ex(Front, 2.0 * h + 4.0 * r, 0.0),
                    ex(Left, 2.0 * h + 4.0 * r, 0.0),
                    // Von oben: ein Kreis (der untere liegt darunter).
                    ex(Top, 2.0 * PI * r, 0.0),
                ],
            }
        }
        "Kugel" => {
            let r = 7.0;
            let circle = 2.0 * PI * r;
            Sample {
                name: "Kugel",
                about: "Kugel Radius 7 mm (jede Ansicht ist ein Kreis)",
                solid: sphere_solid(r),
                projection: first,
                expect: KINDS.iter().map(|&k| ex(k, circle, 0.0)).collect(),
            }
        }
        "Kegelstumpf" => {
            let (r0, r1, h): (f64, f64, f64) = (8.0, 4.0, 12.0);
            let slant = (h * h + (r0 - r1) * (r0 - r1)).sqrt();
            Sample {
                name: "Kegelstumpf",
                about: "Kegelstumpf unten Radius 8, oben Radius 4, Höhe 12 mm",
                solid: cone_solid(r0, r1, h),
                projection: first,
                expect: vec![
                    ex(Front, 2.0 * slant + 2.0 * r0 + 2.0 * r1, 0.0),
                    ex(Left, 2.0 * slant + 2.0 * r0 + 2.0 * r1, 0.0),
                    ex(Top, 2.0 * PI * (r0 + r1), 0.0),
                ],
            }
        }
        "Bohrung_X" => hole_x_sample("Bohrung_X", first),
        "Dritter_Winkel" => hole_x_sample("Dritter_Winkel", Projection::Third),
        "Bohrung_Y" => {
            let (a, b, c, r) = (40.0, 20.0, 30.0, 5.0);
            Sample {
                name: "Bohrung_Y",
                about: "Quader 40 x 20 x 30 mm mit durchgehendem Loch, Durchmesser 10, in Y-Richtung",
                solid: box_with_hole(a, b, c, r, DVec3::Y),
                projection: first,
                expect: vec![
                    // Von vorne schaut man durchs Loch: ein Kreis, nichts verdeckt.
                    ex(Front, 2.0 * (a + c) + 2.0 * PI * r, 0.0),
                    // Von oben und von links: zwei gestrichelte Linien so lang wie das Loch.
                    ex(Top, 2.0 * (a + b), 2.0 * b),
                    ex(Left, 2.0 * (b + c), 2.0 * b),
                ],
            }
        }
        "Winkel" => {
            let (a, b, c) = (40.0, 30.0, 10.0);
            Sample {
                name: "Winkel",
                about: "L-Winkel: Schenkel 40 und 30 mm lang, 8 mm dick, 10 mm hoch",
                solid: l_bracket(a, b, c, 8.0),
                projection: first,
                expect: vec![
                    // Die innere Ecke ist von vorne bzw. von links durch den Schenkel verdeckt.
                    ex(Front, 2.0 * (a + c), c),
                    // Von oben ist der Umfang des L so lang wie der des umschließenden Rechtecks.
                    ex(Top, 2.0 * (a + b), 0.0),
                    ex(Left, 2.0 * (b + c), c),
                ],
            }
        }
        "Tasche" => {
            let (a, b, c, pw, pl, depth) = (40.0, 30.0, 20.0, 20.0, 10.0, 8.0);
            Sample {
                name: "Tasche",
                about: "Quader 40 x 30 x 20 mm mit Tasche 20 x 10 mm, 8 mm tief, oben mittig",
                solid: box_with_pocket(a, b, c, pw, pl, depth),
                projection: first,
                expect: vec![
                    // Von vorne: Taschenboden (Breite) und zwei Taschenecken (Tiefe) verdeckt.
                    ex(Front, 2.0 * (a + c), pw + 2.0 * depth),
                    // Von oben: Außenumriss und Taschenumriss, der Boden liegt darunter.
                    ex(Top, 2.0 * (a + b) + 2.0 * (pw + pl), 0.0),
                    ex(Left, 2.0 * (b + c), pl + 2.0 * depth),
                ],
            }
        }
        "Durchbruch" => Sample {
            name: "Durchbruch",
            about: "Zylinder Radius 10, Höhe 30 mm mit quadratischem Durchbruch quer, um 45 Grad gedreht",
            solid: cylinder_cross_hole(10.0, 30.0, 4.0),
            projection: first,
            expect: vec![],
        },
        "Rundung" => {
            let (a, b, c, r) = (30.0, 20.0, 15.0, 4.0);
            Sample {
                name: "Rundung",
                about: "Quader 30 x 20 x 15 mm, senkrechte Kanten mit Radius 4 gerundet",
                solid: rounded_box(a, b, c, r),
                projection: first,
                expect: vec![
                    // Die glatten Übergänge werden nicht gezeichnet, die Umrisse der Rundungen
                    // aber schon; von vorne und links bleibt es der Rechteckumriss.
                    ex(Front, 2.0 * (a + c), 0.0),
                    ex(Left, 2.0 * (b + c), 0.0),
                    ex(Top, 2.0 * (a + b) - 8.0 * r + 2.0 * PI * r, 0.0),
                ],
            }
        }
        "Torus" => {
            let (major, minor) = (10.0, 3.0);
            Sample {
                name: "Torus",
                about: "Ring (Torus) mit Mittelkreis-Radius 10 und Rohrradius 3 mm, liegend",
                solid: torus_solid(major, minor),
                projection: first,
                expect: vec![
                    // Von oben: der äußere und der innere Kreis.
                    ex(
                        Top,
                        2.0 * PI * (major + minor) + 2.0 * PI * (major - minor),
                        0.0,
                    ),
                ],
            }
        }
        "Freiform" => Sample {
            name: "Freiform",
            about: "Kuppel mit Freiformwand auf einem Sockel (Zylinder Radius 8, Höhe 4 mm)",
            solid: nurbs_tangent_dome_solid().0,
            projection: first,
            expect: vec![],
        },
        other => panic!("unbekannte Probe {other}"),
    }
}

/// Quader 40 × 20 × 30 mm mit Loch Ø 10 längs X (für `Bohrung_X` und `Dritter_Winkel`).
fn hole_x_sample(name: &'static str, projection: Projection) -> Sample {
    let (a, b, c, r) = (40.0, 20.0, 30.0, 5.0);
    Sample {
        name,
        about: "Quader 40 x 20 x 30 mm mit durchgehendem Loch, Durchmesser 10, in X-Richtung",
        solid: box_with_hole(a, b, c, r, DVec3::X),
        projection,
        expect: vec![
            // Von vorne und von oben: zwei gestrichelte Linien so lang wie das Loch (40 mm).
            ex(ViewKind::Front, 2.0 * (a + c), 2.0 * a),
            ex(ViewKind::Top, 2.0 * (a + b), 2.0 * a),
            // Von links schaut man durchs Loch: ein Kreis, nichts verdeckt.
            ex(ViewKind::Left, 2.0 * (b + c) + 2.0 * PI * r, 0.0),
        ],
    }
}

/// Rechteck `(x0, y0, x1, y1)`.
type Rect = (f64, f64, f64, f64);

fn overlaps_or_close(a: Rect, b: Rect, gap: f64) -> bool {
    a.0 < b.2 + gap && b.0 < a.2 + gap && a.1 < b.3 + gap && b.1 < a.3 + gap
}

/// Legt die Ansichten für den Maßstab `scale` aus: Vorne (frei) mit abgeleiteten Ansichten Oben
/// und Links (Abstände wie in der Oberfläche: halbe Ausdehnungen plus [`GAP_MM`]) unten links im
/// Rahmen, Isometrisch frei oben rechts. `None`, wenn etwas nicht in den Rahmen passt oder sich
/// die Gruppen berühren.
fn arrange(
    projection: Projection,
    sheet: Sheet,
    lines: &[ViewLines; 4],
    scale: Scale,
) -> Option<Drawing> {
    let ext = |i: usize| view_extent(lines[i].bbox, scale);
    let (fw, fh) = ext(0);
    let mut rects: Vec<Rect> = vec![(-fw / 2.0, -fh / 2.0, fw / 2.0, fh / 2.0)];
    let mut dists = Vec::new();
    for i in [1usize, 2] {
        let (dx, dy) = align_dir(projection, ViewKind::Front, KINDS[i]).unwrap();
        let (cw, ch) = ext(i);
        let dist = if dx != 0.0 {
            (fw + cw) / 2.0 + GAP_MM
        } else {
            (fh + ch) / 2.0 + GAP_MM
        };
        dists.push(dist);
        let (cx, cy) = (dx * dist, dy * dist);
        rects.push((cx - cw / 2.0, cy - ch / 2.0, cx + cw / 2.0, cy + ch / 2.0));
    }
    let union = rects.iter().fold(
        (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ),
        |u, r| (u.0.min(r.0), u.1.min(r.1), u.2.max(r.2), u.3.max(r.3)),
    );
    let (w, h) = sheet.size_mm();
    let m = EDGE_MARGIN_MM + 0.5 * GAP_MM;
    // Gruppe unten links: Rechteck der drei Ansichten an den Rahmen gelegt.
    let (ox, oy) = (m - union.0, m - union.1);
    let group = (union.0 + ox, union.1 + oy, union.2 + ox, union.3 + oy);
    // Isometrisch oben rechts.
    let (iw, ih) = ext(3);
    let iso = (w - m - iw, h - m - ih, w - m, h - m);
    let inside =
        |r: Rect| r.0 >= m - 1e-9 && r.1 >= m - 1e-9 && r.2 <= w - m + 1e-9 && r.3 <= h - m + 1e-9;
    if !inside(group) || !inside(iso) || overlaps_or_close(group, iso, GAP_MM) {
        return None;
    }
    let mut d = Drawing::new(DrawingId(1), "Probe".into(), sheet, projection);
    let front = d.add_view(
        ViewKind::Front,
        scale,
        Placement::Free { x: ox, y: oy },
        None,
    );
    for (i, dist) in [1usize, 2].into_iter().zip(dists) {
        d.add_view(
            KINDS[i],
            scale,
            Placement::Aligned { distance: dist },
            Some(front),
        );
    }
    d.add_view(
        ViewKind::Isometric,
        scale,
        Placement::Free {
            x: 0.5 * (iso.0 + iso.2),
            y: 0.5 * (iso.1 + iso.3),
        },
        None,
    );
    for v in &mut d.views {
        v.show_hidden = true;
    }
    Some(d)
}

/// Größter Maßstab der Reihe, bei dem alles in den Rahmen passt und sich die Gruppen nicht
/// berühren; passt keiner, der kleinste (dann ragt etwas heraus, die Probe ist trotzdem lesbar).
fn choose_layout(projection: Projection, sheet: Sheet, lines: &[ViewLines; 4]) -> (Drawing, Scale) {
    let mut series = Scale::SERIES;
    series.sort_by(|a, b| b.factor().total_cmp(&a.factor()));
    for s in series {
        if let Some(d) = arrange(projection, sheet, lines, s) {
            return (d, s);
        }
    }
    let smallest = series[series.len() - 1];
    let mut d = Drawing::new(DrawingId(1), "Probe".into(), sheet, projection);
    let f = d.add_view(
        ViewKind::Front,
        smallest,
        Placement::Free { x: 100.0, y: 100.0 },
        None,
    );
    for k in [ViewKind::Top, ViewKind::Left] {
        d.add_view(k, smallest, Placement::Aligned { distance: 60.0 }, Some(f));
    }
    d.add_view(
        ViewKind::Isometric,
        smallest,
        Placement::Free { x: 300.0, y: 200.0 },
        None,
    );
    for v in &mut d.views {
        v.show_hidden = true;
    }
    (d, smallest)
}

fn mm(x: f64) -> String {
    let x = if x.abs() < 0.005 { 0.0 } else { x };
    format!("{x:.2}").replace('.', ",")
}

fn view_name(k: ViewKind) -> String {
    k.label()
}

/// Schreibt `<Name>.svg` und `<Name>.dxf` in `dir`, prüft die Formelwerte gegen die Bibliothek
/// und gibt den Abschnitt für `ERWARTUNG.txt` zurück.
fn write_sample(s: &Sample, dir: &Path) -> String {
    let body = [DrawBody {
        name: s.name.to_string(),
        solid: &s.solid,
    }];
    let computed: Vec<ViewLines> = KINDS
        .iter()
        .map(|&k| {
            compute_view(&body, k, &Progress::new())
                .unwrap_or_else(|e| panic!("{}: Ansicht {}: {e}", s.name, view_name(k)))
        })
        .collect();
    let lines: [ViewLines; 4] = computed.try_into().unwrap();
    let sheet = Sheet {
        paper: Paper::A3,
        landscape: true,
    };
    let (drawing, scale) = choose_layout(s.projection, sheet, &lines);
    let results: BTreeMap<ViewId, &ViewLines> = drawing
        .views
        .iter()
        .map(|v| {
            let i = KINDS.iter().position(|&k| k == v.kind).unwrap();
            (v.id, &lines[i])
        })
        .collect();
    let model = build_sheet(&drawing, &results);
    assert_eq!(
        model.views.len(),
        4,
        "{}: nicht alle Ansichten gelegt",
        s.name
    );
    let svg = svg_string(&model).unwrap_or_else(|e| panic!("{}: SVG: {e}", s.name));
    let dxf = dxf_string(&model).unwrap_or_else(|e| panic!("{}: DXF: {e}", s.name));
    std::fs::write(dir.join(format!("{}.svg", s.name)), svg).unwrap();
    std::fs::write(dir.join(format!("{}.dxf", s.name)), dxf).unwrap();

    let mut text = String::new();
    let proj = match s.projection {
        Projection::First => "Erstwinkel",
        Projection::Third => "Dritter Winkel",
    };
    writeln!(
        text,
        "== {} == {}, {}, A3 quer, Maßstab {}",
        s.name,
        s.about,
        proj,
        scale.label()
    )
    .unwrap();
    for (&k, v) in KINDS.iter().zip(&lines) {
        let vis = piece_length_sum(v, Visibility::Visible);
        let hid = piece_length_sum(v, Visibility::Hidden);
        match s.expect.iter().find(|e| e.kind == k) {
            Some(e) => {
                let tol = |want: f64| 1e-3 + 1e-4 * want;
                assert!(
                    (vis - e.visible).abs() <= tol(e.visible),
                    "{} {}: sichtbar erwartet {}, Bibliothek {}",
                    s.name,
                    view_name(k),
                    e.visible,
                    vis
                );
                assert!(
                    (hid - e.hidden).abs() <= tol(e.hidden),
                    "{} {}: verdeckt erwartet {}, Bibliothek {}",
                    s.name,
                    view_name(k),
                    e.hidden,
                    hid
                );
                writeln!(
                    text,
                    "  {:<12} sichtbar {} mm, verdeckt {} mm (aus der Formel; Bibliothek stimmt überein)",
                    view_name(k),
                    mm(e.visible),
                    mm(e.hidden)
                )
                .unwrap();
            }
            None => {
                writeln!(
                    text,
                    "  {:<12} keine einfache Formel; Bibliothek: sichtbar {} mm, verdeckt {} mm (nach Augenmaß prüfen)",
                    view_name(k),
                    mm(vis),
                    mm(hid)
                )
                .unwrap();
            }
        }
    }
    text.push('\n');
    text
}

/// Der feste Teil von `ERWARTUNG.txt`: was man ansehen soll.
const CHECKLIST: &str = "\
WAS MAN ANSEHEN SOLL

Im Browser oder in Inkscape (SVG-Dateien)
- Die Datei öffnen: Ein Blatt mit Rahmen, drei Ansichten unten links (Vorne, Oben, Links)
  und die Isometrische Ansicht oben rechts. Bei \"Dritter_Winkel\" liegt die Draufsicht
  über der Vorderansicht und die Seitenansicht links davon.
- Sichtbare Linien sind durchgezogen, verdeckte Linien gestrichelt.
- Bei \"Bohrung_X\": In Vorne und Oben zwei gestrichelte Linien, in Links ein Kreis.
- Eine Kante ausmessen (Quader: 10, 20 und 30 mm lang, auf dem Blatt entsprechend dem
  Maßstab) und bei einem Kreis den Durchmesser prüfen (Kugel: 14 mm im Maßstab 1:1).
- Die Linien dürfen nirgends Lücken oder doppelte Striche haben, auch nicht bei den Rundungen
  (Torus, Rundung, Freiform).

In Fusion 360 (DXF-Dateien)
- Als Skizze einfügen. Erscheint die Zeichnung überhaupt? Falls die Skizze leer bleibt: Das
  ist ein bekannter Verdachtspunkt, die Linien liegen in Unterblöcken der Datei. Dann
  melden, damit die Linien direkt statt in Blöcken geschrieben werden.
- Stehen die Namen der Ansichten (Vorne, Hinten, Links...) richtig da, oder als Rohtext mit
  \\U+... bei Umlauten? Dann ASCII-Namen verwenden.
- Gibt es die Ebenen SICHTBAR und VERDECKT? Die verdeckten Linien müssen gestrichelt sein
  (3 mm Strich, 1 mm Lücke), der Abstand der Striche muss zum Maßstab passen.
- Maße: Einheit Millimeter, Blattgröße 420 x 297 mm, Rahmen 10 mm vom Rand.
- Kreise, Bögen, Ellipsen und die Freiformlinien (Kugel, Torus, Freiform) liegen an der
  richtigen Stelle und sind glatt; keine Warnung beim Öffnen.
- Falls Fusion warnt oder die Datei \"repariert\": das notieren; dann müssen die Layouts
  (ACAD_LAYOUT) und die Block-Einträge in der Datei ergänzt werden.

Hinweise
- Alle Proben sind im Erstwinkel (Europa), nur \"Dritter_Winkel\" im Dritten Winkel (USA).
- Torus und Freiform rechnen länger (bis zu einigen Sekunden je Ansicht).
- \"Durchbruch\" ist ein quadratischer Durchbruch, um 45 Grad gedreht (der Kern kann zwei
  sich kreuzende runde Bohrungen noch nicht schneiden).
";

/// Schreibt die Proben `names` und `ERWARTUNG.txt` nach `dir`.
fn write_samples(dir: &Path, names: &[&str]) {
    std::fs::create_dir_all(dir).unwrap();
    let mut all = String::from(
        "ERWARTUNG: erwartete Gesamtlängen der Linien je Probe und Ansicht\n\
         (sichtbar und verdeckt, in mm, aus einfachen Formeln der Kantenlängen)\n\
         Alle Längen sind Körpermaße (Maßstab 1:1); auf dem Blatt gilt zusätzlich der Maßstab.\n\n",
    );
    for name in names {
        let s = sample(name);
        all.push_str(&write_sample(&s, dir));
    }
    all.push_str(CHECKLIST);
    std::fs::write(dir.join("ERWARTUNG.txt"), all).unwrap();
}

fn samples_dir() -> PathBuf {
    match std::env::var_os("PLANORA_SAMPLES_DIR") {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => PathBuf::from(r"D:\Testumgebung\zeichnung-proben"),
    }
}

/// Schreibt alle Proben für den Handtest (siehe Kopf der Datei).
#[test]
#[ignore = "schreibt Dateien für den Handtest und rechnet lange: --release -- --ignored"]
fn write_samples_for_the_hand_test() {
    let dir = samples_dir();
    write_samples(&dir, &NAMES);
    for n in &NAMES {
        for ext in ["svg", "dxf"] {
            let p = dir.join(format!("{n}.{ext}"));
            assert!(std::fs::metadata(&p).unwrap().len() > 100, "{p:?} leer");
        }
    }
    assert!(dir.join("ERWARTUNG.txt").is_file());
    println!("Proben liegen in {}", dir.display());
}

/// Damit der Schreiber nicht verrottet: zwei Proben in einen Ordner unter dem Temp-Verzeichnis.
#[test]
fn sample_writer_still_works() {
    let dir = std::env::temp_dir().join(format!("planora-hand-samples-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    write_samples(&dir, &["Quader", "Zylinder"]);
    for n in ["Quader", "Zylinder"] {
        let svg = std::fs::read_to_string(dir.join(format!("{n}.svg"))).unwrap();
        assert!(svg.contains("<svg"), "{n}.svg ohne SVG-Wurzel");
        let dxf = std::fs::read_to_string(dir.join(format!("{n}.dxf"))).unwrap();
        assert!(dxf.contains("EOF"), "{n}.dxf ohne Dateiende");
    }
    let text = std::fs::read_to_string(dir.join("ERWARTUNG.txt")).unwrap();
    assert!(text.contains("== Quader =="));
    assert!(text.contains("== Zylinder =="));
    assert!(text.contains("WAS MAN ANSEHEN SOLL"));
    let _ = std::fs::remove_dir_all(&dir);
}
