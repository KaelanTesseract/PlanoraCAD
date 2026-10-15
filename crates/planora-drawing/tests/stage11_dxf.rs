//! Stufe 11: DXF-Schreiber. Gelesen wird mit dem eigenen Leser (`common/dxf_read.rs`, prüft auch
//! den Aufbau einer R2000-Datei), die Sollwerte kommen aus Formeln und von Hand gerechneten
//! Zahlen, nicht aus der Bibliothek.
//!
//! Genauigkeit: Der Schreiber rundet jede Zahl auf neun Nachkommastellen (Fehler höchstens 5e-10).
//! Für Strecken, Kreise und Splines mit unverändert übernommenen Kontrollpunkten bleibt der
//! Punktfehler deshalb unter 1e-9, bei Splines aber nur, wenn die Knoten runde Werte haben:
//! werden beliebige Knoten auf neun Stellen gerundet, verschiebt das die Kurve um etwa
//! Hebel · 5e-10 / Knotenabstand. Bei Ellipsen und rationalen Splines kommen Fehler der
//! gerundeten Hauptachse und des Verhältnisses bzw. der Gewichte hinzu (die Parameter der
//! Ellipse stehen in voller Genauigkeit); dafür gilt
//! eine aus der Rundung abgeleitete Schranke (`ell_tol`, `rat_tol`; der Hebel `0,5·R` von
//! `rat_tol` gilt für Kegelschnittstücke bis etwa 120°), nicht ein fester Wert.

mod common;

use common::dxf_read::{self, Dxf, Geom, P, ReadEntity, dist};
use common::model::*;
use planora_drawing::model::{Drawing, DrawingId, Paper, Placement, Projection, Scale, Sheet};
use planora_drawing::out2d::Out2d;
use planora_drawing::sheet::{SheetLine, SheetModel, SheetView, build_sheet};
use planora_drawing::stroke::{Stroke, StrokeOrigin};
use planora_drawing::{
    Basis, DrawBody, DrawError, Piece, ViewId, ViewKind, ViewLines, Visibility, compute_view,
    dxf_string, write_dxf,
};
use planora_exchange::Progress;
use planora_kernel::{Curve, Solid};
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3, Frame, PI, TAU};
use std::collections::BTreeMap;
use std::sync::Arc;

// ---------------------------------------------------------------------------------------------
// Eigene Hilfen
// ---------------------------------------------------------------------------------------------

fn v3(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z)
}

fn cross(a: DVec3, b: DVec3) -> DVec3 {
    v3(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

/// Blattachsen `(ex, ey)` der Ansicht, von Hand: Blick `d`, „oben“ `ey`, `ex = d × ey`.
fn axes(kind: ViewKind) -> (DVec3, DVec3) {
    let (d, ey) = match kind {
        ViewKind::Front => (v3(0.0, 1.0, 0.0), v3(0.0, 0.0, 1.0)),
        ViewKind::Back => (v3(0.0, -1.0, 0.0), v3(0.0, 0.0, 1.0)),
        ViewKind::Left => (v3(1.0, 0.0, 0.0), v3(0.0, 0.0, 1.0)),
        ViewKind::Right => (v3(-1.0, 0.0, 0.0), v3(0.0, 0.0, 1.0)),
        ViewKind::Top => (v3(0.0, 0.0, -1.0), v3(0.0, 1.0, 0.0)),
        ViewKind::Bottom => (v3(0.0, 0.0, 1.0), v3(0.0, -1.0, 0.0)),
        ViewKind::Isometric => {
            let (s3, s6) = (3f64.sqrt(), 6f64.sqrt());
            (
                v3(-1.0 / s3, 1.0 / s3, -1.0 / s3),
                v3(-1.0 / s6, 1.0 / s6, 2.0 / s6),
            )
        }
    };
    (cross(d, ey), ey)
}

fn img(p: DVec3, kind: ViewKind) -> P {
    let (ex, ey) = axes(kind);
    (p.dot(ex), p.dot(ey))
}

fn frame(origin: DVec3, x: DVec3, y: DVec3) -> Frame {
    Frame {
        origin,
        x,
        y,
        z: cross(x, y),
    }
}

fn sheet_of(paper: Paper, landscape: bool) -> Sheet {
    Sheet { paper, landscape }
}

/// Blattmodell von Hand: Ansichten mit Namen und Linien (sichtbar/verdeckt), Rahmen 10 mm.
fn manual(sheet: Sheet, views: Vec<(&str, Vec<(Visibility, Out2d)>)>) -> SheetModel {
    let (w, h) = sheet.size_mm();
    SheetModel {
        sheet,
        frame: (DVec2::new(10.0, 10.0), DVec2::new(w - 10.0, h - 10.0)),
        views: views
            .into_iter()
            .enumerate()
            .map(|(i, (name, lines))| SheetView {
                id: ViewId(i as u64 + 1),
                name: name.to_string(),
                lines: lines
                    .into_iter()
                    .map(|(vis, curve)| SheetLine { vis, curve })
                    .collect(),
                extent_mm: (0.0, 0.0),
            })
            .collect(),
    }
}

fn one_view(lines: Vec<(Visibility, Out2d)>) -> SheetModel {
    manual(sheet_of(Paper::A3, true), vec![("Vorne", lines)])
}

fn vis(curve: Out2d) -> (Visibility, Out2d) {
    (Visibility::Visible, curve)
}

fn line(a: (f64, f64), b: (f64, f64)) -> Out2d {
    Out2d::Line {
        a: DVec2::new(a.0, a.1),
        b: DVec2::new(b.0, b.1),
    }
}

fn arc(c: (f64, f64), r: f64, start: f64, end: f64) -> Out2d {
    Out2d::Arc {
        c: DVec2::new(c.0, c.1),
        r,
        start,
        end,
    }
}

fn ellipse(c: P, major: P, ratio: f64, start: f64, end: f64) -> Out2d {
    Out2d::Ellipse {
        c: DVec2::new(c.0, c.1),
        major: DVec2::new(major.0, major.1),
        ratio,
        start,
        end,
    }
}

fn spline(p: usize, knots: Vec<f64>, ctrl: &[P], weights: Vec<f64>) -> Out2d {
    Out2d::Spline {
        degree: p,
        knots,
        ctrl: ctrl.iter().map(|c| DVec2::new(c.0, c.1)).collect(),
        weights,
    }
}

fn dxf_of(m: &SheetModel) -> String {
    dxf_string(m).expect("DXF sollte sich schreiben lassen")
}

/// Text und gelesene (und geprüfte) Datei.
fn read_of(m: &SheetModel) -> (String, Dxf) {
    let s = dxf_of(m);
    let d = dxf_read::read(&s).unwrap_or_else(|e| panic!("Aufbau ungültig: {e}"));
    (s, d)
}

/// Objekte einer Ansicht (Block) mit gelesener Geometrie.
fn ents(d: &Dxf, block: &str) -> Vec<(ReadEntity, Geom)> {
    let b = d
        .block(block)
        .unwrap_or_else(|| panic!("Block {block} fehlt in {:?}", d.view_block_names()));
    b.entities
        .iter()
        .map(|e| (e.clone(), e.geom().unwrap()))
        .collect()
}

fn on_layer<'a>(
    v: &'a [(ReadEntity, Geom)],
    layer: &'a str,
) -> impl Iterator<Item = &'a (ReadEntity, Geom)> + 'a {
    v.iter().filter(move |(e, _)| e.layer == layer)
}

fn kinds(v: &[(ReadEntity, Geom)]) -> Vec<&str> {
    v.iter().map(|(e, _)| e.kind.as_str()).collect()
}

fn close(a: P, b: P, tol: f64) -> bool {
    dist(a, b) <= tol
}

/// Modell eines Körpers in einer Ansicht, Mitte der Ansicht bei (200, 150) auf A3 quer.
fn solid_model(solid: &Solid, kind: ViewKind, show_hidden: bool) -> SheetModel {
    let body = DrawBody {
        name: "K".into(),
        solid,
    };
    let vl = compute_view(&[body], kind, &Progress::new()).expect("Ansicht");
    let mut d = Drawing::new(
        DrawingId(1),
        "Z".into(),
        sheet_of(Paper::A3, true),
        Projection::First,
    );
    let id = d.add_view(
        kind,
        Scale { num: 1, den: 1 },
        Placement::Free { x: 200.0, y: 150.0 },
        None,
    );
    d.views[0].show_hidden = show_hidden;
    let res: BTreeMap<ViewId, &ViewLines> = BTreeMap::from([(id, &vl)]);
    build_sheet(&d, &res)
}

/// Eine Ansicht aus Strichen von Hand: jeder Eintrag `(Strich, Stück-Beginn, Stück-Ende)`,
/// alles sichtbar; die Hüllbox wird vorgegeben (Mitte → bekannte Verschiebung auf das Blatt).
fn hand_model(
    kind: ViewKind,
    items: Vec<(Stroke, f64, f64)>,
    bbox: ((f64, f64), (f64, f64)),
    center: (f64, f64),
) -> SheetModel {
    let mut strokes = Vec::new();
    let mut pieces = Vec::new();
    for (i, (s, t0, t1)) in items.into_iter().enumerate() {
        pieces.push(Piece {
            stroke: i,
            t0,
            t1,
            vis: Visibility::Visible,
        });
        strokes.push(s);
    }
    let vl = ViewLines {
        kind,
        basis: Basis::of(kind),
        extent: 100.0,
        strokes,
        pieces,
        bbox,
        dropped_splits: 0,
    };
    let mut d = Drawing::new(
        DrawingId(1),
        "Z".into(),
        sheet_of(Paper::A3, true),
        Projection::First,
    );
    let id = d.add_view(
        kind,
        Scale { num: 1, den: 1 },
        Placement::Free {
            x: center.0,
            y: center.1,
        },
        None,
    );
    let res: BTreeMap<ViewId, &ViewLines> = BTreeMap::from([(id, &vl)]);
    build_sheet(&d, &res)
}

fn stroke(curve: Curve, t0: f64, t1: f64) -> Stroke {
    Stroke {
        curve,
        t0,
        t1,
        origin: StrokeOrigin::Edge { body: 0, edge: 0 },
    }
}

// ---------------------------------------------------------------------------------------------
// Formelkurven (eigene Auswertung, unabhängig von der Bibliothek)
// ---------------------------------------------------------------------------------------------

/// Formelkurve mit Stützpunkten; Abstände werden am nächsten Stützpunkt grob gesucht und auf der
/// Formel selbst (Goldener Schnitt) verfeinert.
struct Curve2<'a> {
    f: &'a dyn Fn(f64) -> P,
    t0: f64,
    t1: f64,
    pts: Vec<P>,
}

fn poly<'a>(f: &'a dyn Fn(f64) -> P, t0: f64, t1: f64, n: usize) -> Curve2<'a> {
    Curve2 {
        f,
        t0,
        t1,
        pts: (0..=n)
            .map(|i| f(t0 + (t1 - t0) * i as f64 / n as f64))
            .collect(),
    }
}

fn dist_poly(p: P, c: &Curve2) -> f64 {
    let n = c.pts.len() - 1;
    let (mut bi, mut bd) = (0usize, f64::INFINITY);
    for (i, q) in c.pts.iter().enumerate() {
        let d = dist(p, *q);
        if d < bd {
            (bi, bd) = (i, d);
        }
    }
    let at = |i: usize| c.t0 + (c.t1 - c.t0) * i as f64 / n as f64;
    let mut cands = vec![bi];
    if bi == 0 && dist(c.pts[0], c.pts[n]) < 1e-9 {
        cands.push(n);
    } else if bi == n && dist(c.pts[0], c.pts[n]) < 1e-9 {
        cands.push(0);
    }
    let g = 0.5 * (5f64.sqrt() - 1.0);
    let d = |t: f64| dist(p, (c.f)(t));
    let mut best = bd;
    for bi in cands {
        let (mut lo, mut hi) = (at(bi.saturating_sub(1)), at((bi + 1).min(n)));
        for _ in 0..80 {
            let (m1, m2) = (hi - g * (hi - lo), lo + g * (hi - lo));
            if d(m1) < d(m2) {
                hi = m2;
            } else {
                lo = m1;
            }
        }
        best = best.min(d(0.5 * (lo + hi)));
    }
    best
}

/// Größte Abweichung in beiden Richtungen: Punkte der gelesenen Objekte zur Formelkurve und
/// Formelpunkte zu den gelesenen Objekten.
fn two_way_deviation(geoms: &[&Geom], formula: &Curve2, formula_checks: usize) -> f64 {
    let mut dev = 0.0f64;
    for g in geoms {
        for p in g.points(200) {
            dev = dev.max(dist_poly(p, formula));
        }
    }
    let step = (formula.pts.len() / formula_checks).max(1);
    for q in formula.pts.iter().step_by(step) {
        let d = geoms
            .iter()
            .map(|g| g.distance_to(*q))
            .fold(f64::INFINITY, f64::min);
        dev = dev.max(d);
    }
    dev
}

/// Schranke für Ellipsenpunkte: die Parameter stehen in voller Genauigkeit, gerundet (neun
/// Stellen) werden Mittelpunkt (7e-10), Hauptachse (7e-10) und Verhältnis; dessen Fehler 5e-10
/// wirkt auf die Nebenachse, also mit höchstens `a` · 5e-10.
fn ell_tol(a: f64) -> f64 {
    1.5e-9 + 5e-10 * a
}

/// Schranke für rationale Splines mit auf neun Stellen gerundeten Gewichten: Kontrollpunkte
/// 7e-10, ein Gewichtsfehler von 5e-10 wirkt mit einem Hebel von höchstens `0,5·R`.
fn rat_tol(radius: f64) -> f64 {
    7e-10 + 3e-10 * radius
}

// ---------------------------------------------------------------------------------------------
// Eigene Splineauswertung (Basisfunktionen nach Algorithmus A2.2 des „NURBS Book“), unabhängig
// vom Leser (de Boor) und von der Bibliothek
// ---------------------------------------------------------------------------------------------

fn nurbs_eval(p: usize, k: &[f64], ctrl: &[P], w: &[f64], u: f64) -> P {
    let n = ctrl.len();
    // Knotenspanne mit k[i] <= u < k[i+1]; am rechten Rand die letzte nichtleere Spanne.
    let mut span = p;
    for i in p..n {
        if k[i] < k[i + 1] {
            span = i;
            if u < k[i + 1] {
                break;
            }
        }
    }
    let mut basis = vec![0.0; p + 1];
    let mut left = vec![0.0; p + 1];
    let mut right = vec![0.0; p + 1];
    basis[0] = 1.0;
    for j in 1..=p {
        left[j] = u - k[span + 1 - j];
        right[j] = k[span + j] - u;
        let mut saved = 0.0;
        for r in 0..j {
            let tmp = basis[r] / (right[r + 1] + left[j - r]);
            basis[r] = saved + right[r + 1] * tmp;
            saved = left[j - r] * tmp;
        }
        basis[j] = saved;
    }
    let (mut x, mut y, mut s) = (0.0, 0.0, 0.0);
    for (j, b) in basis.iter().enumerate() {
        let i = span - p + j;
        let bw = b * w[i];
        x += bw * ctrl[i].0;
        y += bw * ctrl[i].1;
        s += bw;
    }
    (x / s, y / s)
}

/// Geklemmter gleichmäßiger Knotenvektor für `n` Kontrollpunkte und Grad `p`.
fn clamped(p: usize, n: usize) -> Vec<f64> {
    let mut k = vec![0.0; p + 1];
    let inner = n - p - 1;
    for j in 1..=inner {
        k.push(j as f64);
    }
    k.extend(vec![(inner + 1) as f64; p + 1]);
    k
}

fn cubic_ctrl() -> Vec<P> {
    vec![
        (0.0, 1.0),
        (2.0, 4.0),
        (5.0, -2.0),
        (7.0, 3.0),
        (9.0, 0.0),
        (12.0, 5.0),
    ]
}

/// Der Spline des einzigen Objekts, auseinandergenommen.
fn spline_parts(g: &Geom) -> (usize, i64, Vec<f64>, Vec<P>, Vec<f64>) {
    match g {
        Geom::Spline {
            degree,
            flags,
            knots,
            ctrl,
            weights,
            ..
        } => (
            *degree,
            *flags,
            knots.clone(),
            ctrl.clone(),
            weights.clone(),
        ),
        other => panic!("SPLINE erwartet, gefunden {other:?}"),
    }
}

fn assert_vec_close(got: &[f64], want: &[f64], tol: f64, what: &str) {
    assert_eq!(got.len(), want.len(), "{what}: Länge {got:?} vs {want:?}");
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        assert!((g - w).abs() <= tol, "{what}[{i}]: {g} vs {w}");
    }
}

// ---------------------------------------------------------------------------------------------
// (a) Aufbau
// ---------------------------------------------------------------------------------------------

#[test]
fn structure_is_valid() {
    let m = manual(
        sheet_of(Paper::A3, true),
        vec![
            ("Vorne", vec![vis(line((20.0, 20.0), (60.0, 20.0)))]),
            (
                "Oben",
                vec![
                    vis(arc((100.0, 100.0), 10.0, 0.0, PI)),
                    (Visibility::Hidden, line((1.0, 2.0), (3.0, 4.0))),
                ],
            ),
        ],
    );
    let (s, d) = read_of(&m);
    // Abschnitte, Ende, Zeilenende, nur ASCII.
    assert_eq!(
        d.sections,
        [
            "HEADER", "CLASSES", "TABLES", "BLOCKS", "ENTITIES", "OBJECTS"
        ]
    );
    // OBJECTS: Wurzelverzeichnis (Besitzer 0) mit ACAD_GROUP und ACAD_PLOTSTYLENAME, darunter die
    // Plotstil-Kette Vorgabeverzeichnis -> Platzhalter; jede Ebene verweist mit 390 darauf.
    let kinds: Vec<&str> = d.objects.iter().map(|o| o.kind.as_str()).collect();
    assert_eq!(
        kinds,
        [
            "DICTIONARY",
            "DICTIONARY",
            "ACDBDICTIONARYWDFLT",
            "ACDBPLACEHOLDER"
        ]
    );
    assert_eq!(d.objects[0].owner, "0");
    assert_eq!(d.objects[1].owner, d.objects[0].handle);
    assert_eq!(d.objects[0].get(350), Some(d.objects[1].handle.as_str()));
    assert_eq!(d.objects[2].owner, d.objects[0].handle);
    assert_eq!(d.objects[3].owner, d.objects[2].handle);
    assert_eq!(d.objects[2].get(340), Some(d.objects[3].handle.as_str()));
    let plot_dict = d.objects[0]
        .groups
        .iter()
        .position(|g| g.value == "ACAD_PLOTSTYLENAME")
        .unwrap();
    assert_eq!(
        d.objects[0].groups[plot_dict + 1].value,
        d.objects[2].handle
    );
    for r in &d.table("LAYER").unwrap().records {
        assert_eq!(
            r.get(390),
            Some(d.objects[3].handle.as_str()),
            "{}",
            r.name()
        );
        // Reihenfolge der Gruppen wie in Fusion-tauglichen Dateien: ... 62, 6, 370, 390.
        let codes: Vec<i32> = r.groups.iter().map(|g| g.code).collect();
        assert_eq!(codes[codes.len() - 4..], [62, 6, 370, 390], "{codes:?}");
    }
    // CLASSES: genau die zwei Klassen der Kette.
    let classes: Vec<(&str, &str)> = d
        .classes
        .iter()
        .map(|c| (c.get(1).unwrap(), c.get(2).unwrap()))
        .collect();
    assert_eq!(
        classes,
        [
            ("ACDBDICTIONARYWDFLT", "AcDbDictionaryWithDefault"),
            ("ACDBPLACEHOLDER", "AcDbPlaceHolder")
        ]
    );
    for c in &d.classes {
        assert_eq!(c.get(3), Some("ObjectDBX Classes"));
        assert_eq!(
            (c.get(90), c.get(280), c.get(281)),
            (Some("0"), Some("0"), Some("0"))
        );
    }
    assert!(s.ends_with("  0\r\nENDSEC\r\n  0\r\nEOF\r\n"), "{s:?}");
    assert!(s.is_ascii());
    assert_eq!(s.matches('\n').count(), s.matches("\r\n").count());
    assert!(s.starts_with("  0\r\nSECTION\r\n  2\r\nHEADER\r\n"));
    // Kopf.
    let v = d.header_var("$ACADVER").unwrap();
    assert_eq!((v[0].code, v[0].value.as_str()), (1, "AC1015"));
    let v = d.header_var("$INSUNITS").unwrap();
    assert_eq!((v[0].code, v[0].value.as_str()), (70, "4"));
    let v = d.header_var("$MEASUREMENT").unwrap();
    assert_eq!((v[0].code, v[0].value.as_str()), (70, "1"));
    // Blattgröße A3 quer: 420 × 297.
    let ext = |name: &str| -> Vec<f64> {
        d.header_var(name)
            .unwrap()
            .iter()
            .map(|g| g.num())
            .collect()
    };
    assert_eq!(ext("$EXTMIN"), [0.0, 0.0, 0.0]);
    assert_eq!(ext("$EXTMAX"), [420.0, 297.0, 0.0]);
    // Tabellen in der vorgeschriebenen Reihenfolge, Pflichteinträge.
    let names: Vec<&str> = d.tables.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, dxf_read::TABLE_ORDER);
    let rec_names = |t: &str| -> Vec<String> {
        d.table(t)
            .unwrap()
            .records
            .iter()
            .map(|r| r.name())
            .collect()
    };
    assert_eq!(
        rec_names("LTYPE"),
        ["ByBlock", "ByLayer", "CONTINUOUS", "HIDDEN"]
    );
    assert_eq!(rec_names("LAYER"), ["0", "SICHTBAR", "VERDECKT"]);
    assert_eq!(rec_names("STYLE"), ["STANDARD"]);
    assert_eq!(rec_names("APPID"), ["ACAD"]);
    assert_eq!(rec_names("VIEW").len() + rec_names("UCS").len(), 0);
    assert_eq!(
        rec_names("BLOCK_RECORD"),
        ["*Model_Space", "*Paper_Space", "Vorne", "Oben"]
    );
    // Jedes Handle der Datei einmalig (unabhängig von der Prüfung des Lesers gezählt).
    let hs = d.all_handles();
    let mut sorted = hs.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(hs.len(), sorted.len(), "Handles doppelt: {hs:?}");
    assert!(hs.len() > 30);
    // $HANDSEED über allen Handles.
    let seed = u64::from_str_radix(&d.header_var("$HANDSEED").unwrap()[0].value, 16).unwrap();
    let max = hs
        .iter()
        .map(|h| u64::from_str_radix(h, 16).unwrap())
        .max()
        .unwrap();
    assert!(seed > max, "{seed:X} <= {max:X}");
    // Der Block *Paper_Space ist als Papierraum gekennzeichnet, beide Raumblöcke sind leer.
    assert!(d.block("*Model_Space").unwrap().entities.is_empty());
    assert!(d.block("*Paper_Space").unwrap().entities.is_empty());
    assert_eq!(d.block("*Paper_Space").unwrap().begin.get(67), Some("1"));
    assert_eq!(d.block("*Paper_Space").unwrap().end.get(67), Some("1"));
    assert_eq!(d.block("*Model_Space").unwrap().begin.get(67), None);
}

#[test]
fn validator_rejects_corrupted_files() {
    // Der Leser darf nicht alles durchwinken: jede einzelne Beschädigung muss auffallen.
    let m = manual(
        sheet_of(Paper::A4, true),
        vec![("Vorne", vec![vis(line((20.0, 20.0), (60.0, 20.0)))])],
    );
    let good = dxf_of(&m);
    assert!(dxf_read::read(&good).is_ok());
    let parsed = dxf_read::read(&good).unwrap();
    let hs = parsed.all_handles();
    let root = parsed.objects[0].handle.clone();
    let wdflt = parsed.objects[2].handle.clone();
    let placeholder = parsed.objects[3].handle.clone();
    let seed = parsed.header_var("$HANDSEED").unwrap()[0].value.clone();
    let model_br = parsed
        .table("BLOCK_RECORD")
        .unwrap()
        .records
        .iter()
        .find(|r| r.name() == "*Model_Space")
        .unwrap()
        .handle
        .clone();
    let last_handle = hs.last().unwrap().clone();
    let first_table_handle = hs[0].clone();
    let cases: Vec<(&str, String)> = vec![
        ("LF statt CRLF", good.replacen("\r\n", "\n", 1)),
        ("ohne EOF", good.replace("  0\r\nEOF\r\n", "")),
        ("Nachlauf nach EOF", format!("{good}  0\r\nEOF\r\n")),
        ("Abschnitt vertauscht", good.replacen("HEADER", "BLOCKS", 1)),
        ("falsche Version", good.replace("AC1015", "AC1014")),
        (
            "Handle doppelt",
            good.replacen(
                &format!("  5\r\n{last_handle}\r\n"),
                &format!("  5\r\n{first_table_handle}\r\n"),
                1,
            ),
        ),
        (
            "Handseed zu klein",
            good.replacen(
                &format!("$HANDSEED\r\n  5\r\n{seed}\r\n"),
                "$HANDSEED\r\n  5\r\n1\r\n",
                1,
            ),
        ),
        (
            "Ebene unbekannt",
            good.replacen("  8\r\nSICHTBAR\r\n", "  8\r\nNIRGENDS\r\n", 1),
        ),
        (
            "Linienart unbekannt",
            good.replacen("  6\r\nHIDDEN\r\n", "  6\r\nGESTRICHELT\r\n", 1),
        ),
        (
            "INSERT ohne Block",
            good.replacen("  2\r\nVorne\r\n 10", "  2\r\nWeg\r\n 10", 1),
        ),
        (
            "falsche Untertypmarke",
            good.replacen("100\r\nAcDbLine", "100\r\nAcDbCircle", 1),
        ),
        (
            "Besitzer unbekannt",
            good.replacen(&format!("330\r\n{model_br}\r\n"), "330\r\nFFFFF\r\n", 1),
        ),
        (
            "Zahl kaputt",
            good.replacen(" 10\r\n20", " 10\r\nzwanzig", 1),
        ),
        (
            "Zahl mit Exponent",
            good.replacen(" 10\r\n20", " 10\r\n2e1", 1),
        ),
        ("Zahl -0", good.replacen(" 10\r\n20", " 10\r\n-0", 1)),
        (
            "zu viele Nachkommastellen",
            good.replacen(" 10\r\n20", " 10\r\n20.1234567891", 1),
        ),
        (
            "Tabellenzähler",
            good.replacen(
                "AcDbSymbolTable\r\n 70\r\n3",
                "AcDbSymbolTable\r\n 70\r\n4",
                1,
            ),
        ),
        (
            "ungerade Zeilen",
            good.replacen("EOF\r\n", "EOF\r\n  0\r\n", 1),
        ),
        (
            "OBJECTS fehlt",
            good.replacen(
                "  0\r\nSECTION\r\n  2\r\nOBJECTS\r\n",
                "  0\r\nSECTION\r\n  2\r\nOBJEKTE\r\n",
                1,
            ),
        ),
        (
            "Wurzelverzeichnis mit Besitzer",
            good.replacen(
                &format!("  5\r\n{root}\r\n330\r\n0\r\n100\r\nAcDbDictionary"),
                &format!("  5\r\n{root}\r\n330\r\n{model_br}\r\n100\r\nAcDbDictionary"),
                1,
            ),
        ),
        ("350 ins Leere", good.replacen("350\r\n", "350\r\nFFFFF", 1)),
        ("390 fehlt", good.replacen("390\r\n", "391\r\n", 1)),
        ("390 ins Leere", good.replacen("390\r\n", "390\r\nFFFFF", 1)),
        (
            "CLASS fehlt",
            good.replacen(
                "  1\r\nACDBPLACEHOLDER\r\n",
                "  1\r\nACDBPLACEHOLDEX\r\n",
                1,
            ),
        ),
        (
            "Platzhalter mit falschem Besitzer",
            good.replacen(
                &format!("  5\r\n{placeholder}\r\n330\r\n{wdflt}\r\n"),
                &format!("  5\r\n{placeholder}\r\n330\r\n{root}\r\n"),
                1,
            ),
        ),
        (
            "ACAD_PLOTSTYLENAME fehlt",
            good.replacen("ACAD_PLOTSTYLENAME", "ACAD_PLOTSTYLEXAME", 1),
        ),
        ("Nicht-ASCII", good.replacen("Vorne", "Vörne", 1)),
        (
            "Tabellenfolge",
            good.replacen("LTYPE", "XTYPE", 1)
                .replacen("LAYER", "LTYPE", 1)
                .replacen("XTYPE", "LAYER", 1),
        ),
    ];
    for (what, bad) in cases {
        assert!(
            dxf_read::read(&bad).is_err(),
            "Beschädigung nicht erkannt: {what}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// (b) Quader von vorne: vier Strecken
// ---------------------------------------------------------------------------------------------

#[test]
fn box_front_roundtrips_to_four_lines() {
    // Quader 25 × 10 × 15, Vorderansicht: Rechteck 25 × 15, Mitte bei (200, 150).
    let m = solid_model(&box_solid(25.0, 10.0, 15.0), ViewKind::Front, false);
    let (_, d) = read_of(&m);
    assert_eq!(d.view_block_names(), ["Vorne"]);
    let v = ents(&d, "Vorne");
    assert_eq!(kinds(&v), ["LINE"; 4]);
    assert!(v.iter().all(|(e, _)| e.layer == "SICHTBAR"));
    let mut lens: Vec<f64> = v.iter().map(|(_, g)| g.length()).collect();
    lens.sort_by(f64::total_cmp);
    assert_vec_close(&lens, &[15.0, 15.0, 25.0, 25.0], 1e-9, "Längen");
    // Ecken von Hand: Mitte (200, 150) ± (12,5 | 7,5); jede gehört zu genau zwei Strecken.
    let corners = [
        (187.5, 142.5),
        (212.5, 142.5),
        (212.5, 157.5),
        (187.5, 157.5),
    ];
    for c in corners {
        let n: usize = v
            .iter()
            .map(|(_, g)| {
                let Geom::Line { a, b } = g else { panic!() };
                usize::from(close(*a, c, 1e-9)) + usize::from(close(*b, c, 1e-9))
            })
            .sum();
        assert_eq!(n, 2, "Ecke {c:?}");
    }
    // Außerhalb des Blocks: Randlinie (geschlossen, SICHTBAR) und ein INSERT.
    assert_eq!(d.entities.len(), 2);
    let border = &d.entities[0];
    assert_eq!(
        (border.kind.as_str(), border.layer.as_str()),
        ("LWPOLYLINE", "SICHTBAR")
    );
    let Geom::Poly { closed, pts } = border.geom().unwrap() else {
        panic!()
    };
    assert!(closed);
    assert_eq!(
        pts,
        [(10.0, 10.0), (410.0, 10.0), (410.0, 287.0), (10.0, 287.0)]
    );
    assert_eq!(d.entities[1].kind, "INSERT");
}

// ---------------------------------------------------------------------------------------------
// (c) Zylinder von oben = Kreis
// ---------------------------------------------------------------------------------------------

#[test]
fn cylinder_top_is_a_circle_entity() {
    let r0 = 7.0;
    let m = solid_model(&cylinder_solid(r0, 20.0), ViewKind::Top, false);
    let (_, d) = read_of(&m);
    let v = ents(&d, "Oben");
    assert_eq!(kinds(&v), ["CIRCLE"], "genau ein Kreis, keine Bögen");
    assert_eq!(v[0].0.layer, "SICHTBAR");
    let Geom::Circle { c, r } = v[0].1 else {
        panic!()
    };
    assert!(close(c, (200.0, 150.0), 1e-9), "{c:?}");
    assert!((r - r0).abs() < 1e-9, "{r}");
    assert!((v[0].1.length() - TAU * r0).abs() < 1e-8);
    // Ein CIRCLE trägt keine Winkelgruppen.
    assert!(v[0].0.get(50).is_none() && v[0].0.get(51).is_none());
}

// ---------------------------------------------------------------------------------------------
// (d) Halbkreis: Winkel in Grad
// ---------------------------------------------------------------------------------------------

#[test]
fn half_circle_is_arc_in_degrees() {
    let m = one_view(vec![vis(arc((60.0, 70.0), 12.0, 0.0, PI))]);
    let (_, d) = read_of(&m);
    let v = ents(&d, "Vorne");
    assert_eq!(kinds(&v), ["ARC"]);
    let Geom::Arc { c, r, start, end } = v[0].1 else {
        panic!()
    };
    assert!(close(c, (60.0, 70.0), 1e-9) && (r - 12.0).abs() < 1e-9);
    assert!(start.abs() < 1e-9, "Anfang {start}");
    assert!((end - 180.0).abs() < 1e-9, "Ende {end}");
    // Rohwerte: Grad, nicht Radiant (3,14 wäre Radiant).
    assert_eq!(v[0].0.get(50), Some("0"));
    assert_eq!(v[0].0.get(51), Some("180"));
    // Weitere Bögen von Hand: Viertel und Dreiviertelkreis.
    let m = one_view(vec![
        vis(arc((0.0, 0.0), 5.0, PI / 2.0, PI)),
        vis(arc((0.0, 0.0), 5.0, PI / 2.0, 3.0 * PI / 2.0)),
        vis(arc((0.0, 0.0), 5.0, 0.0, 1.5 * PI)),
    ]);
    let (_, d) = read_of(&m);
    let v = ents(&d, "Vorne");
    let got: Vec<(f64, f64)> = v
        .iter()
        .map(|(_, g)| match g {
            Geom::Arc { start, end, .. } => (*start, *end),
            _ => panic!(),
        })
        .collect();
    let want = [(90.0, 180.0), (90.0, 270.0), (0.0, 270.0)];
    for (g, w) in got.iter().zip(want) {
        assert!(
            (g.0 - w.0).abs() < 1e-9 && (g.1 - w.1).abs() < 1e-9,
            "{g:?} vs {w:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// (e) Zylinder isometrisch: Ellipsen
// ---------------------------------------------------------------------------------------------

#[test]
fn iso_cylinder_is_ellipse_entity() {
    let (r0, h0) = (10.0, 30.0);
    let kind = ViewKind::Isometric;
    let m = solid_model(&cylinder_solid(r0, h0), kind, true);
    let (_, d) = read_of(&m);
    let v = ents(&d, "Isometrisch");
    // Hüllbox von Hand: Mitte (0, h/√6) → Verschiebung auf das Blatt (200, 150) − (0, h/√6).
    let t = (200.0, 150.0 - h0 / 6f64.sqrt());
    let ring = |z: f64| {
        move |th: f64| {
            let q = img(v3(r0 * th.cos(), r0 * th.sin(), z), kind);
            (q.0 + t.0, q.1 + t.1)
        }
    };
    let (ring_top, ring_bottom) = (ring(h0), ring(0.0));
    let top = poly(&ring_top, 0.0, TAU, 40_000);
    let bottom = poly(&ring_bottom, 0.0, TAU, 40_000);
    let ells: Vec<&(ReadEntity, Geom)> = v.iter().filter(|(e, _)| e.kind == "ELLIPSE").collect();
    assert!(
        ells.len() >= 3,
        "obere Ellipse und die zwei Teile der unteren"
    );
    let mut top_len = 0.0;
    let mut top_geoms = Vec::new();
    let mut bottom_vis = Vec::new();
    let mut bottom_hid = Vec::new();
    for (e, g) in &ells {
        let Geom::Ellipse { major, ratio, .. } = g else {
            panic!()
        };
        // Verhältnis 1/√3 und Hauptachse (r, 0): die Hauptachse liegt in x-Richtung des Blatts.
        assert!(
            (ratio - 1.0 / 3f64.sqrt()).abs() < 1e-9,
            "Verhältnis {ratio}"
        );
        assert!(
            (dist((0.0, 0.0), *major) - r0).abs() < 1e-9,
            "Hauptachse {major:?}"
        );
        assert!(
            major.1.abs() < 1e-9,
            "Hauptachse liegt nicht in x: {major:?}"
        );
        assert!(*ratio <= 1.0 && *ratio > 0.0);
        // Jeder Punkt liegt auf der oberen oder der unteren Ellipse (Formel).
        let pts = g.points(64);
        let on_top = pts.iter().all(|&q| dist_poly(q, &top) < ell_tol(r0));
        let on_bottom = pts.iter().all(|&q| dist_poly(q, &bottom) < ell_tol(r0));
        assert!(on_top || on_bottom, "Ellipse liegt auf keiner Formelkurve");
        if on_top {
            top_len += g.length();
            top_geoms.push(g);
            assert_eq!(e.layer, "SICHTBAR");
        } else if e.layer == "SICHTBAR" {
            bottom_vis.push(g);
        } else {
            assert_eq!(e.layer, "VERDECKT");
            bottom_hid.push(g);
        }
    }
    // Obere Ellipse ganz: Umfang aus der Formel (Polygonzug, 40000 Sehnen) und jeder Punkt gedeckt.
    let formula_len: f64 = top.pts.windows(2).map(|w| dist(w[0], w[1])).sum();
    assert!(
        (top_len - formula_len).abs() < 1e-5,
        "{top_len} vs {formula_len}"
    );
    for q in top.pts.iter().step_by(400) {
        let dd = top_geoms
            .iter()
            .map(|g| g.distance_to(*q))
            .fold(f64::INFINITY, f64::min);
        assert!(dd < ell_tol(r0), "obere Ellipse {q:?}: {dd}");
    }
    // Untere Ellipse: sichtbar, wo sin θ < cos θ (n·d < 0), sonst verdeckt (Streifen um die
    // Wechselstellen ausgenommen).
    let (mut seen_v, mut seen_h) = (0, 0);
    for i in (0..40_000).step_by(100) {
        let th = TAU * i as f64 / 40_000.0;
        let g = th.sin() - th.cos();
        if g.abs() < 0.05 {
            continue;
        }
        let q = bottom.pts[i];
        let dv = bottom_vis
            .iter()
            .map(|g| g.distance_to(q))
            .fold(f64::INFINITY, f64::min);
        let dh = bottom_hid
            .iter()
            .map(|g| g.distance_to(q))
            .fold(f64::INFINITY, f64::min);
        if g < 0.0 {
            assert!(dv < ell_tol(r0), "sichtbare Hälfte θ = {th}: {dv}");
            seen_v += 1;
        } else {
            assert!(dh < ell_tol(r0), "verdeckte Hälfte θ = {th}: {dh}");
            assert!(dv > 1e-3, "verdeckte Hälfte auch sichtbar θ = {th}");
            seen_h += 1;
        }
    }
    assert!(seen_v > 100 && seen_h > 100);
    // Alle Parameter liegen im zulässigen Bereich [0, 2π].
    for (e, _) in &ells {
        for code in [41, 42] {
            let x: f64 = e.get(code).unwrap().parse().unwrap();
            assert!((0.0..=TAU).contains(&x), "Parameter {x}");
        }
    }
}

// ---------------------------------------------------------------------------------------------
// (f) Splines
// ---------------------------------------------------------------------------------------------

fn top_spline_model(t0: f64, t1: f64) -> SheetModel {
    // NURBS-Kante (Grad 3, innere Knoten 0,3 und 0,6) in der Draufsicht (x, y), Hüllbox-Mitte
    // (6, 1,5) → Verschiebung (200, 150) − (6, 1,5) = (194, 148,5).
    let ctrl3: Vec<DVec3> = cubic_ctrl().iter().map(|c| v3(c.0, c.1, 0.0)).collect();
    let knots = vec![0.0, 0.0, 0.0, 0.0, 0.3, 0.6, 1.0, 1.0, 1.0, 1.0];
    let c = NurbsCurve::new(3, knots, ctrl3, vec![1.0; 6]).unwrap();
    hand_model(
        ViewKind::Top,
        vec![(stroke(Curve::Nurbs(Arc::new(c)), 0.0, 1.0), t0, t1)],
        ((0.0, -3.0), (12.0, 6.0)),
        (200.0, 150.0),
    )
}

#[test]
fn spline_roundtrips() {
    let t = (194.0, 148.5);
    let src_knots = [0.0, 0.0, 0.0, 0.0, 0.3, 0.6, 1.0, 1.0, 1.0, 1.0];
    let src_ctrl = cubic_ctrl();
    let f = |u: f64| {
        let q = nurbs_eval(3, &src_knots, &src_ctrl, &[1.0; 6], u);
        (q.0 + t.0, q.1 + t.1)
    };
    // Ganzer Bereich: Grad, Knoten und Kontrollpunkte genau wie die Quelle (um die Verschiebung).
    let (_, d) = read_of(&top_spline_model(0.0, 1.0));
    let v = ents(&d, "Oben");
    assert_eq!(kinds(&v), ["SPLINE"]);
    assert_eq!(v[0].0.layer, "SICHTBAR");
    let (deg, flags, knots, ctrl, weights) = spline_parts(&v[0].1);
    assert_eq!(deg, 3);
    assert_eq!(flags, 8, "planar, nicht rational");
    assert_vec_close(&knots, &src_knots, 1e-9, "Knoten");
    let want: Vec<P> = src_ctrl.iter().map(|c| (c.0 + t.0, c.1 + t.1)).collect();
    assert_eq!(ctrl.len(), want.len());
    for (g, w) in ctrl.iter().zip(&want) {
        assert!(close(*g, *w, 1e-9), "Kontrollpunkt {g:?} vs {w:?}");
    }
    assert!(weights.iter().all(|&w| w == 1.0));
    assert!(
        v[0].0.get(41).is_none(),
        "keine Gewichte bei nicht rationalem Spline"
    );
    let formula = poly(&f, 0.0, 1.0, 4_000);
    let dev = two_way_deviation(&[&v[0].1], &formula, 2000);
    eprintln!("Spline ganz: Abweichung {dev:e}");
    assert!(dev < 1e-9, "{dev}");

    // Teilbereich [0,2; 0,8]: Knoten von Hand (Randknoten vierfach, innere Knoten bleiben),
    // sechs Kontrollpunkte; die Kurve ist genau das Stück der Quelle.
    let (_, d) = read_of(&top_spline_model(0.2, 0.8));
    let v = ents(&d, "Oben");
    assert_eq!(kinds(&v), ["SPLINE"]);
    let (deg, _, knots, ctrl, _) = spline_parts(&v[0].1);
    assert_eq!(deg, 3);
    assert_vec_close(
        &knots,
        &[0.2, 0.2, 0.2, 0.2, 0.3, 0.6, 0.8, 0.8, 0.8, 0.8],
        1e-9,
        "Knoten Teilbereich",
    );
    assert_eq!(ctrl.len(), 6);
    let formula = poly(&f, 0.2, 0.8, 4_000);
    let dev = two_way_deviation(&[&v[0].1], &formula, 2000);
    eprintln!("Spline Teilbereich: Abweichung {dev:e}");
    assert!(dev < 1e-9, "{dev}");
    assert!(close(v[0].1.at(0.2), f(0.2), 1e-9) && close(v[0].1.at(0.8), f(0.8), 1e-9));
    // Länge gegen den Polygonzug der Formel.
    let flen: f64 = formula.pts.windows(2).map(|w| dist(w[0], w[1])).sum();
    assert!(
        (v[0].1.length() - flen).abs() < 1e-5,
        "{} vs {flen}",
        v[0].1.length()
    );
}

#[test]
fn spline_degrees_roundtrip() {
    // Nicht rationale Splines der Grade 1 bis 6 mit geklemmten Knoten, danach mit Doppelknoten:
    // Grad, Knoten, Kontrollpunkte unverändert; Punkte auf 1e-9 gegen die eigene Auswertung.
    let ctrl: Vec<P> = (0..9)
        .map(|i| {
            let x = i as f64 * 6.0 + [0.0, 2.0, -3.0, 1.0, 4.0, -2.0, 3.0, 0.5, -1.0][i];
            let y = [0.0, 9.0, -4.0, 7.0, -8.0, 5.0, 2.0, -6.0, 3.0][i];
            (x + 20.0, y + 40.0)
        })
        .collect();
    let mut cases: Vec<(usize, Vec<f64>, Vec<P>)> =
        (1..=6).map(|p| (p, clamped(p, 9), ctrl.clone())).collect();
    cases.push((
        3,
        vec![0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 2.0, 3.0, 4.0, 4.0, 4.0, 4.0],
        ctrl[..8].to_vec(),
    ));
    for (p, kn, cp) in cases {
        let n = cp.len();
        let m = one_view(vec![vis(spline(p, kn.clone(), &cp, vec![1.0; n]))]);
        let (_, d) = read_of(&m);
        let v = ents(&d, "Vorne");
        assert_eq!(kinds(&v), ["SPLINE"]);
        let (deg, flags, knots, ctrl_got, _) = spline_parts(&v[0].1);
        assert_eq!((deg, flags), (p, 8));
        assert_vec_close(&knots, &kn, 1e-9, "Knoten");
        for (g, w) in ctrl_got.iter().zip(&cp) {
            assert!(close(*g, *w, 1e-9));
        }
        let (a, b) = (kn[p], kn[n]);
        let f = |u: f64| nurbs_eval(p, &kn, &cp, &vec![1.0; n], u);
        let formula = poly(&f, a, b, 4_000);
        let dev = two_way_deviation(&[&v[0].1], &formula, 1500);
        assert!(dev < 1e-9, "Grad {p}: {dev}");
        // Zähler der Datei.
        assert_eq!(v[0].0.get(72).unwrap().parse::<usize>().unwrap(), kn.len());
        assert_eq!(v[0].0.get(73).unwrap().parse::<usize>().unwrap(), n);
        assert_eq!(v[0].0.get(74), Some("0"));
    }
}

#[test]
fn rational_spline_roundtrips() {
    // Viertelkreis Grad 2: Radius 10 um (50, 60); w = √½ (auf neun Stellen gerundet geschrieben).
    let w = 0.5f64.sqrt();
    let quarter = spline(
        2,
        vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        &[(60.0, 60.0), (60.0, 70.0), (50.0, 70.0)],
        vec![1.0, w, 1.0],
    );
    let (s, d) = read_of(&one_view(vec![vis(quarter)]));
    let v = ents(&d, "Vorne");
    let (deg, flags, knots, ctrl, weights) = spline_parts(&v[0].1);
    assert_eq!((deg, flags), (2, 12), "planar + rational");
    assert_vec_close(&knots, &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0], 1e-9, "Knoten");
    assert_eq!(ctrl, [(60.0, 60.0), (60.0, 70.0), (50.0, 70.0)]);
    assert_vec_close(&weights, &[1.0, w, 1.0], 5e-10 + 1e-12, "Gewichte");
    assert!(
        s.contains(" 41\r\n0.707106781\r\n"),
        "Gewicht √½ auf neun Stellen"
    );
    for q in v[0].1.points(100) {
        assert!(
            (dist(q, (50.0, 60.0)) - 10.0).abs() < rat_tol(10.0),
            "{q:?}"
        );
    }
    // Vollkreis aus neun Kontrollpunkten, Radius 120 (große Koordinaten).
    let (cx, cy, rr) = (300.0, 200.0, 120.0);
    let cp: Vec<P> = vec![
        (cx + rr, cy),
        (cx + rr, cy + rr),
        (cx, cy + rr),
        (cx - rr, cy + rr),
        (cx - rr, cy),
        (cx - rr, cy - rr),
        (cx, cy - rr),
        (cx + rr, cy - rr),
        (cx + rr, cy),
    ];
    let ww: Vec<f64> = (0..9).map(|i| if i % 2 == 0 { 1.0 } else { w }).collect();
    let kn = vec![
        0.0, 0.0, 0.0, 0.25, 0.25, 0.5, 0.5, 0.75, 0.75, 1.0, 1.0, 1.0,
    ];
    let (_, d) = read_of(&one_view(vec![vis(spline(2, kn.clone(), &cp, ww.clone()))]));
    let v = ents(&d, "Vorne");
    let (_, flags, knots, ctrl, weights) = spline_parts(&v[0].1);
    assert_eq!(flags, 12);
    assert_vec_close(&knots, &kn, 1e-9, "Knoten");
    assert_vec_close(&weights, &ww, 5e-10 + 1e-12, "Gewichte");
    for (g, w0) in ctrl.iter().zip(&cp) {
        assert!(close(*g, *w0, 1e-9));
    }
    for q in v[0].1.points(400) {
        assert!((dist(q, (cx, cy)) - rr).abs() < rat_tol(rr), "{q:?}");
    }
    // Wilde Gewichte, die auf neun Stellen exakt sind: Daten genau, Punkte auf 1e-9 gegen die
    // eigene Auswertung (Grad 2 und 3).
    let ctrl5: Vec<P> = vec![
        (20.0, 30.0),
        (35.0, 80.0),
        (60.0, 10.0),
        (90.0, 70.0),
        (120.0, 25.0),
    ];
    let wild = vec![1.0, 2.0, 0.5, 3.0, 1.0];
    for (p, kn) in [
        (2usize, vec![0.0, 0.0, 0.0, 0.4, 0.7, 1.0, 1.0, 1.0]),
        (3, vec![0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0]),
    ] {
        let (_, d) = read_of(&one_view(vec![vis(spline(
            p,
            kn.clone(),
            &ctrl5,
            wild.clone(),
        ))]));
        let v = ents(&d, "Vorne");
        let (deg, flags, knots, ctrl_got, weights) = spline_parts(&v[0].1);
        assert_eq!((deg, flags), (p, 12));
        assert_vec_close(&knots, &kn, 1e-9, "Knoten");
        assert_vec_close(&weights, &wild, 1e-9, "Gewichte");
        assert_eq!(ctrl_got, ctrl5);
        assert!(v[0].0.get(41).is_some());
        let f = |u: f64| nurbs_eval(p, &kn, &ctrl5, &wild, u);
        let formula = poly(&f, 0.0, 1.0, 4_000);
        let dev = two_way_deviation(&[&v[0].1], &formula, 1500);
        assert!(dev < 1e-9, "rational Grad {p}: {dev}");
    }
    // Aus einem rationalen Strich (Kernel-Kurve) gebaut: Gewichte kommen unverändert in die Datei.
    let ctrl3: Vec<DVec3> = [(60.0, 60.0), (60.0, 70.0), (50.0, 70.0)]
        .iter()
        .map(|c| v3(c.0, c.1, 0.0))
        .collect();
    let c = NurbsCurve::new(
        2,
        vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        ctrl3,
        vec![1.0, w, 1.0],
    )
    .unwrap();
    let m = hand_model(
        ViewKind::Top,
        vec![(stroke(Curve::Nurbs(Arc::new(c)), 0.0, 1.0), 0.0, 1.0)],
        ((50.0, 60.0), (60.0, 70.0)),
        (55.0, 65.0),
    );
    let (_, d) = read_of(&m);
    let v = ents(&d, "Oben");
    let (_, flags, _, _, weights) = spline_parts(&v[0].1);
    assert_eq!(flags, 12);
    assert_vec_close(&weights, &[1.0, w, 1.0], 1e-9, "Gewichte aus dem Strich");
    // Der Viertelkreis liegt um (50, 60) → verschoben um die Mitte (55,65) − (55,65) = 0.
    for q in v[0].1.points(100) {
        assert!(
            (dist(q, (50.0, 60.0)) - 10.0).abs() < rat_tol(10.0),
            "{q:?}"
        );
    }
}

#[test]
fn closed_nurbs_seam_pieces_roundtrip() {
    // Geschlossene Freiformkurve; ein Stück über die Naht [0,7; 1,3] ergibt zwei Splines.
    // Draufsicht, Hüllbox (−13,−13)–(13,13) → Mitte 0 → Verschiebung (200, 150).
    let c = closed_nurbs_loop();
    let ctrl: Vec<P> = vec![
        (10.0, 0.0),
        (10.0, 10.0),
        (-10.0, 10.0),
        (-10.0, -10.0),
        (10.0, -10.0),
        (10.0, -5.0),
        (10.0, 0.0),
    ];
    let knots = vec![0.0, 0.0, 0.0, 0.0, 0.25, 0.5, 0.75, 1.0, 1.0, 1.0, 1.0];
    let m = hand_model(
        ViewKind::Top,
        vec![(stroke(Curve::Nurbs(Arc::new(c)), 0.0, 1.0), 0.7, 1.3)],
        ((-13.0, -13.0), (13.0, 13.0)),
        (200.0, 150.0),
    );
    let (_, d) = read_of(&m);
    let v = ents(&d, "Oben");
    assert_eq!(kinds(&v), ["SPLINE", "SPLINE"], "zwei Stücke über die Naht");
    let f = |u: f64| {
        let q = nurbs_eval(3, &knots, &ctrl, &[1.0; 7], u);
        (q.0 + 200.0, q.1 + 150.0)
    };
    let a = poly(&f, 0.7, 1.0, 2_000);
    let b = poly(&f, 0.0, 0.3, 2_000);
    let mut dev = 0.0f64;
    for (_, g) in &v {
        for q in g.points(200) {
            dev = dev.max(dist_poly(q, &a).min(dist_poly(q, &b)));
        }
    }
    for q in a.pts.iter().step_by(10).chain(b.pts.iter().step_by(10)) {
        let dq = v
            .iter()
            .map(|(_, g)| g.distance_to(*q))
            .fold(f64::INFINITY, f64::min);
        dev = dev.max(dq);
    }
    eprintln!("Naht: Abweichung {dev:e}");
    assert!(dev < 1e-9, "{dev}");
    // Beide Splines haben an der Naht (10, 0) + Verschiebung einen Endpunkt.
    let seam = (210.0, 150.0);
    let ends: Vec<P> = v
        .iter()
        .flat_map(|(_, g)| {
            let (lo, hi) = g.range();
            [g.at(lo), g.at(hi)]
        })
        .collect();
    assert!(
        ends.iter().filter(|&&e| close(e, seam, 1e-9)).count() >= 2,
        "{ends:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// (g) Verdeckt = Ebene VERDECKT mit Linienart HIDDEN
// ---------------------------------------------------------------------------------------------

#[test]
fn hidden_goes_to_layer_verdeckt_with_hidden_linetype() {
    // Quader 30 × 20 × 20 mit Bohrung (r = 4) längs X; von vorne sind die zwei Mantellinien der
    // Bohrung bei z = 10 ± 4 verdeckt.
    let m = solid_model(
        &box_with_hole(30.0, 20.0, 20.0, 4.0, v3(1.0, 0.0, 0.0)),
        ViewKind::Front,
        true,
    );
    let (_, d) = read_of(&m);
    let v = ents(&d, "Vorne");
    let hid: Vec<_> = on_layer(&v, "VERDECKT").collect();
    let seen: Vec<_> = on_layer(&v, "SICHTBAR").collect();
    assert_eq!(hid.len() + seen.len(), v.len(), "nur diese zwei Ebenen");
    assert_eq!(hid.len(), 2);
    let mut ys = Vec::new();
    for (e, g) in &hid {
        assert_eq!(e.kind, "LINE");
        let Geom::Line { a, b } = g else { panic!() };
        assert!((a.1 - b.1).abs() < 1e-9);
        assert!((g.length() - 30.0).abs() < 1e-9);
        assert!((a.0.min(b.0) - 185.0).abs() < 1e-9 && (a.0.max(b.0) - 215.0).abs() < 1e-9);
        ys.push(a.1);
        // Linienart und Farbe kommen von der Ebene: das Objekt trägt keine eigenen.
        assert!(e.get(6).is_none() && e.get(62).is_none());
    }
    ys.sort_by(f64::total_cmp);
    assert!(
        (ys[0] - 146.0).abs() < 1e-9 && (ys[1] - 154.0).abs() < 1e-9,
        "{ys:?}"
    );
    let total: f64 = seen.iter().map(|(_, g)| g.length()).sum();
    assert!((total - 100.0).abs() < 1e-9, "{total}");
    assert!(
        seen.iter()
            .all(|(e, _)| e.kind == "LINE" && e.get(6).is_none())
    );
    // Ebenentabelle.
    let layers = d.table("LAYER").unwrap();
    let layer = |n: &str| layers.records.iter().find(|r| r.name() == n).unwrap();
    let (s, h) = (layer("SICHTBAR"), layer("VERDECKT"));
    assert_eq!(
        (s.get(6), s.get(62), s.get(370), s.get(70)),
        (Some("CONTINUOUS"), Some("7"), Some("50"), Some("0"))
    );
    assert_eq!(
        (h.get(6), h.get(62), h.get(370), h.get(70)),
        (Some("HIDDEN"), Some("7"), Some("25"), Some("0"))
    );
    assert_eq!(layer("0").get(6), Some("CONTINUOUS"));
    // Linienarten: CONTINUOUS ohne Muster, HIDDEN mit Strich 3 mm und Lücke 1 mm.
    let lt = d.table("LTYPE").unwrap();
    let ltype = |n: &str| lt.records.iter().find(|r| r.name() == n).unwrap();
    let hidden = ltype("HIDDEN");
    assert_eq!(hidden.get(73), Some("2"));
    assert_eq!(hidden.get(40), Some("4"));
    assert_eq!(hidden.get(72), Some("65"));
    let dashes: Vec<f64> = hidden
        .groups
        .iter()
        .filter(|g| g.code == 49)
        .map(|g| g.num())
        .collect();
    assert_eq!(dashes, [3.0, -1.0]);
    let cont = ltype("CONTINUOUS");
    assert_eq!((cont.get(73), cont.get(40)), (Some("0"), Some("0")));
    assert!(cont.groups.iter().all(|g| g.code != 49));
    // Ohne show_hidden entfällt die Ebene ganz; kein verdecktes Objekt in der Datei.
    let m = solid_model(
        &box_with_hole(30.0, 20.0, 20.0, 4.0, v3(1.0, 0.0, 0.0)),
        ViewKind::Front,
        false,
    );
    let (_, d) = read_of(&m);
    let v = ents(&d, "Vorne");
    assert_eq!(on_layer(&v, "VERDECKT").count(), 0);
    assert_eq!(v.len(), 4);
    assert_eq!(
        d.table("LAYER").unwrap().records.len(),
        3,
        "Ebenen bleiben immer angelegt"
    );
}

// ---------------------------------------------------------------------------------------------
// (h) Ansichten sind Blöcke mit INSERT
// ---------------------------------------------------------------------------------------------

#[test]
fn views_are_blocks_with_inserts() {
    let m = manual(
        sheet_of(Paper::A3, true),
        vec![
            (
                "Vorne",
                vec![
                    vis(line((20.0, 20.0), (60.0, 20.0))),
                    (Visibility::Hidden, line((20.0, 30.0), (60.0, 30.0))),
                ],
            ),
            ("Seite", vec![]),
            ("Oben", vec![vis(arc((100.0, 100.0), 10.0, 0.0, TAU))]),
        ],
    );
    let (_, d) = read_of(&m);
    let names: Vec<&str> = d.blocks.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(
        names,
        ["*Model_Space", "*Paper_Space", "Vorne", "Seite", "Oben"]
    );
    assert_eq!(kinds(&ents(&d, "Vorne")), ["LINE", "LINE"]);
    assert!(ents(&d, "Seite").is_empty());
    assert_eq!(kinds(&ents(&d, "Oben")), ["CIRCLE"]);
    // ENTITIES: Randlinie, dann ein INSERT je Ansicht in der Reihenfolge der Ansichten, bei (0, 0).
    assert_eq!(
        d.entities
            .iter()
            .map(|e| e.kind.as_str())
            .collect::<Vec<_>>(),
        ["LWPOLYLINE", "INSERT", "INSERT", "INSERT"]
    );
    let ins: Vec<(String, P)> = d.entities[1..]
        .iter()
        .map(|e| match e.geom().unwrap() {
            Geom::Insert { name, at } => (name, at),
            _ => panic!(),
        })
        .collect();
    assert_eq!(
        ins,
        [
            ("Vorne".to_string(), (0.0, 0.0)),
            ("Seite".to_string(), (0.0, 0.0)),
            ("Oben".to_string(), (0.0, 0.0)),
        ]
    );
    // Die Blöcke gehören dem Blockeintrag gleichen Namens (Leser prüft Besitzer), die Objekte
    // der Blöcke den Blöcken, die der Datei dem Modellraum.
    let br = d.table("BLOCK_RECORD").unwrap();
    for b in &d.blocks {
        let rec = br.records.iter().find(|r| r.name() == b.name).unwrap();
        assert_eq!(b.begin.owner, rec.handle);
        assert!(b.entities.iter().all(|e| e.owner == rec.handle));
    }
    // Basispunkt jedes Blocks (0, 0, 0).
    for b in &d.blocks {
        let base: Vec<&str> = [10, 20, 30]
            .iter()
            .map(|&c| b.begin.get(c).unwrap())
            .collect();
        assert_eq!(base, ["0", "0", "0"], "{}", b.name);
    }
    // Der Blockname steht in Gruppe 2 und 3 (Leser prüft), die Ansicht bekommt keinen Anonymblock.
    assert!(d.view_block_names().iter().all(|n| !n.starts_with('*')));
}

// ---------------------------------------------------------------------------------------------
// (i) Namen
// ---------------------------------------------------------------------------------------------

#[test]
fn names_are_sanitized_and_unique() {
    let l = || vec![vis(line((20.0, 20.0), (30.0, 30.0)))];
    let m = manual(
        sheet_of(Paper::A3, true),
        vec![
            // Jedes der zehn verbotenen Zeichen `<>/\":;?*|=` und der Gravis wird zu „_“.
            ("a<b>c/d\\e\"f:g;h?i*j|k=l`m", l()),
            ("Vorne", l()),
            ("Vorne", l()),
            ("Vorne 2", l()),
            ("Vorne", l()),
            ("VORNE", l()),
            ("", l()),
            ("   ", l()),
            ("a<b", l()),
            ("a>b", l()),
            ("*Model_Space", l()),
            ("*PAPER_SPACE", l()),
            ("Tab\tZeile\nEnde\u{1}\u{7f}", l()),
        ],
    );
    let (s, d) = read_of(&m);
    assert_eq!(
        d.view_block_names(),
        [
            "a_b_c_d_e_f_g_h_i_j_k_l_m",
            "Vorne",
            "Vorne 2",
            "Vorne 2 2",
            "Vorne 3",
            "VORNE 4",
            "Ansicht",
            "Ansicht 2",
            "a_b",
            "a_b 2",
            "_Model_Space",
            "_PAPER_SPACE",
            "Tab_Zeile_Ende__",
        ]
    );
    // Namen nicht verwechselbar mit den Raumblöcken, alle Blöcke einmalig (Leser prüft ohne
    // Rücksicht auf Groß-/Kleinschreibung), jede Ansicht hat ihren Block mit genau einer Linie.
    for n in d.view_block_names() {
        assert_eq!(ents(&d, &n).len(), 1, "{n}");
    }
    assert!(!s.contains('`'));
    // INSERT nennt denselben Namen wie der Block.
    let ins: Vec<String> = d.entities[1..]
        .iter()
        .map(|e| match e.geom().unwrap() {
            Geom::Insert { name, .. } => name,
            _ => panic!(),
        })
        .collect();
    assert_eq!(ins, d.view_block_names());
    // Umlaute: ASCII-Datei, Unicode als \U+XXXX; der Leser stellt den Namen wieder her.
    let m = manual(
        sheet_of(Paper::A3, true),
        vec![("Rückansicht", l()), ("Größe ✓", l()), ("𝄞 Note", l())],
    );
    let (s, d) = read_of(&m);
    assert!(s.is_ascii());
    assert!(s.contains("R\\U+00FCckansicht"), "{s}");
    assert_eq!(
        d.view_block_names(),
        ["Rückansicht", "Größe ✓", "_ Note"],
        "Zeichen außerhalb der Grundebene werden zu „_“"
    );
    // Sehr lange Namen werden gekürzt (höchstens 255 Zeichen in der Datei), bleiben aber
    // unterscheidbar und einmalig.
    let long = "x".repeat(400);
    let m = manual(
        sheet_of(Paper::A3, true),
        vec![
            (long.as_str(), l()),
            (long.as_str(), l()),
            ("ü".repeat(300).as_str(), l()),
        ],
    );
    let (s, d) = read_of(&m);
    let names = d.view_block_names();
    assert_eq!(names.len(), 3);
    assert_eq!(names[0], "x".repeat(255));
    assert_eq!(names[1], format!("{} 2", "x".repeat(253)));
    for line in s.split("\r\n") {
        assert!(line.len() <= 255, "Zeile zu lang: {}", line.len());
    }
}

// ---------------------------------------------------------------------------------------------
// (j) Nicht endliche Zahlen
// ---------------------------------------------------------------------------------------------

fn refused(bad: Out2d) {
    let m = one_view(vec![
        vis(line((0.0, 0.0), (1.0, 1.0))),
        (Visibility::Hidden, bad.clone()),
    ]);
    assert!(
        matches!(dxf_string(&m), Err(DrawError::NonFinite)),
        "{bad:?}"
    );
    let mut sink: Vec<u8> = Vec::new();
    assert!(matches!(
        write_dxf(&m, &mut sink),
        Err(DrawError::NonFinite)
    ));
    assert!(sink.is_empty(), "bei einem Fehler wird nichts geschrieben");
}

#[test]
fn non_finite_is_refused() {
    let nan = f64::NAN;
    let inf = f64::INFINITY;
    refused(line((nan, 0.0), (1.0, 1.0)));
    refused(line((0.0, 0.0), (1.0, inf)));
    refused(line((0.0, 0.0), (-inf, 1.0)));
    refused(arc((0.0, 0.0), nan, 0.0, 1.0));
    refused(arc((0.0, 0.0), 1.0, inf, 2.0));
    refused(arc((nan, 0.0), 1.0, 0.0, 2.0));
    refused(arc((0.0, 0.0), 1.0, 0.0, nan));
    refused(ellipse((0.0, 0.0), (1.0, 0.0), nan, 0.0, 1.0));
    refused(ellipse((0.0, 0.0), (inf, 0.0), 0.5, 0.0, 1.0));
    refused(ellipse((0.0, 0.0), (1.0, 0.0), 0.5, nan, 1.0));
    let kn = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    let cp = [(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)];
    refused(spline(2, kn.clone(), &cp, vec![1.0, nan, 1.0]));
    refused(spline(
        2,
        kn.clone(),
        &[(0.0, 0.0), (inf, 1.0), (2.0, 0.0)],
        vec![1.0; 3],
    ));
    refused(spline(
        2,
        vec![0.0, 0.0, nan, 1.0, 1.0, 1.0],
        &cp,
        vec![1.0; 3],
    ));
    // Ungültig aufgebaut (leer, Knotenzahl falsch, Gewicht ≤ 0): ebenfalls Fehler, keine Panik.
    refused(spline(3, vec![], &[], vec![]));
    refused(spline(2, vec![0.0, 1.0], &cp, vec![1.0; 3]));
    refused(spline(2, kn, &cp, vec![1.0, -1.0, 1.0]));
    // Rahmen und Blattgröße.
    let mut m = one_view(vec![]);
    m.frame.1.x = nan;
    assert!(matches!(dxf_string(&m), Err(DrawError::NonFinite)));
    let mut m = one_view(vec![]);
    m.frame.0.y = -inf;
    assert!(matches!(dxf_string(&m), Err(DrawError::NonFinite)));
}

// ---------------------------------------------------------------------------------------------
// (k) Gespiegelte Ansichten: Bögen gegen den Uhrzeigersinn
// ---------------------------------------------------------------------------------------------

#[test]
fn reflected_back_view_has_ccw_arcs() {
    // Rückansicht: (x, y, z) → (−x, z), gespiegelt. Kreis in der XZ-Ebene um (2, 0, 3), r = 5.
    // Ein Raumbogen t ∈ [0,3; 2,2] (gegen den Uhrzeigersinn im Raum) erscheint im Bild als Bogen
    // über die Winkel π − 2,2 … π − 0,3, also 0,9416 … 2,8416 rad = 53,95° … 162,81°
    // (von Hand), und bleibt gegen den Uhrzeigersinn mit start < end.
    let kind = ViewKind::Back;
    let s = stroke(
        Curve::Circle {
            frame: frame(v3(2.0, 0.0, 3.0), v3(1.0, 0.0, 0.0), v3(0.0, 0.0, 1.0)),
            radius: 5.0,
        },
        0.3,
        2.2,
    );
    let m = hand_model(
        kind,
        vec![(s, 0.3, 2.2)],
        ((-20.0, -10.0), (20.0, 10.0)),
        (200.0, 150.0),
    );
    let (_, d) = read_of(&m);
    let v = ents(&d, "Hinten");
    assert_eq!(kinds(&v), ["ARC"]);
    let Geom::Arc { c, r, start, end } = v[0].1 else {
        panic!()
    };
    assert!(start < end, "start {start} < end {end}");
    assert!((start - (PI - 2.2).to_degrees()).abs() < 1e-9, "{start}");
    assert!((end - (PI - 0.3).to_degrees()).abs() < 1e-9, "{end}");
    assert!((r - 5.0).abs() < 1e-9);
    // Mittelpunkt: Bildpunkt (−2, 3) plus Verschiebung (200, 150) (Hüllbox-Mitte ist (0, 0)).
    assert!(close(c, (198.0, 153.0), 1e-9), "{c:?}");
    let f = |t: f64| {
        let q = img(v3(2.0 + 5.0 * t.cos(), 0.0, 3.0 + 5.0 * t.sin()), kind);
        (q.0 + 200.0, q.1 + 150.0)
    };
    let formula = poly(&f, 0.3, 2.2, 4_000);
    let dev = two_way_deviation(&[&v[0].1], &formula, 400);
    assert!(dev < 1e-9, "{dev}");

    // Längerer Bogen [−1, 4]: im Bild π − 4 … π + 1 = −0,858 … 4,142 rad; auf [0, 360) gebracht
    // beginnt er bei 310,8° und endet bei 237,3°, läuft also über 0° hinweg (start > end ist hier
    // die Schreibweise des Umlaufs, die Richtung bleibt gegen den Uhrzeigersinn).
    let s = stroke(
        Curve::Circle {
            frame: frame(v3(2.0, 0.0, 3.0), v3(1.0, 0.0, 0.0), v3(0.0, 0.0, 1.0)),
            radius: 5.0,
        },
        -1.0,
        4.0,
    );
    let m = hand_model(
        kind,
        vec![(s, -1.0, 4.0)],
        ((-20.0, -10.0), (20.0, 10.0)),
        (200.0, 150.0),
    );
    let (_, d) = read_of(&m);
    let v = ents(&d, "Hinten");
    let Geom::Arc { start, end, .. } = v[0].1 else {
        panic!()
    };
    assert!((0.0..360.0).contains(&start) && (0.0..360.0).contains(&end));
    assert!(
        (start - (PI - 4.0).to_degrees().rem_euclid(360.0)).abs() < 1e-9,
        "{start}"
    );
    assert!(
        (end - (PI + 1.0).to_degrees().rem_euclid(360.0)).abs() < 1e-9,
        "{end}"
    );
    assert!(start > end);
    let formula = poly(&f, -1.0, 4.0, 4_000);
    let dev = two_way_deviation(&[&v[0].1], &formula, 400);
    assert!(dev < 1e-9, "{dev}");
    assert!(
        (v[0].1.length() - 5.0 * 5.0).abs() < 1e-8,
        "Länge {}",
        v[0].1.length()
    );

    // Unterseite: (x, y, z) → (x, −y), ebenfalls gespiegelt; Teilkreis um (3, 4, 5), r = 6.
    let kind = ViewKind::Bottom;
    let s = stroke(
        Curve::Circle {
            frame: frame(v3(3.0, 4.0, 5.0), v3(1.0, 0.0, 0.0), v3(0.0, 1.0, 0.0)),
            radius: 6.0,
        },
        0.3,
        2.2,
    );
    let m = hand_model(
        kind,
        vec![(s, 0.3, 2.2)],
        ((-10.0, -15.0), (20.0, 5.0)),
        (200.0, 150.0),
    );
    let (_, d) = read_of(&m);
    let v = ents(&d, "Unten");
    let Geom::Arc { start, end, .. } = v[0].1 else {
        panic!()
    };
    // Bild: (3 + 6 cos t, −4 − 6 sin t) → Winkel −t: −2,2 … −0,3 rad = 233,94° … 342,8°.
    assert!(start < end, "{start} {end}");
    assert!(
        (start - (-2.2f64).to_degrees().rem_euclid(360.0)).abs() < 1e-9,
        "{start}"
    );
    assert!(
        (end - (-0.3f64).to_degrees().rem_euclid(360.0)).abs() < 1e-9,
        "{end}"
    );
    let f = |t: f64| {
        let q = img(v3(3.0 + 6.0 * t.cos(), 4.0 + 6.0 * t.sin(), 5.0), kind);
        (q.0 + 195.0, q.1 + 155.0)
    };
    let formula = poly(&f, 0.3, 2.2, 4_000);
    assert!(two_way_deviation(&[&v[0].1], &formula, 400) < 1e-9);

    // Rückansicht mit gekipptem Kreis: ELLIPSE, Parameter in [0, 2π], Punkte auf der Formel.
    let fy = v3(0.0, 0.6, 0.8);
    let kind = ViewKind::Back;
    let s = stroke(
        Curve::Circle {
            frame: frame(v3(2.0, 0.0, 3.0), v3(1.0, 0.0, 0.0), fy),
            radius: 5.0,
        },
        -1.0,
        4.0,
    );
    let m = hand_model(
        kind,
        vec![(s, -1.0, 4.0)],
        ((-20.0, -10.0), (20.0, 10.0)),
        (200.0, 150.0),
    );
    let (_, d) = read_of(&m);
    let v = ents(&d, "Hinten");
    assert_eq!(kinds(&v), ["ELLIPSE"]);
    let f = |t: f64| {
        let pnt = v3(2.0, 0.0, 3.0) + v3(1.0, 0.0, 0.0) * (5.0 * t.cos()) + fy * (5.0 * t.sin());
        let q = img(pnt, kind);
        (q.0 + 200.0, q.1 + 150.0)
    };
    let formula = poly(&f, -1.0, 4.0, 4_000);
    let dev = two_way_deviation(&[&v[0].1], &formula, 400);
    assert!(dev < ell_tol(5.0), "gekippt: {dev}");
}

// ---------------------------------------------------------------------------------------------
// Winkel: negativ, über TAU, Naht
// ---------------------------------------------------------------------------------------------

#[test]
fn arc_angles_negative_and_beyond_tau() {
    let c = (60.0, 70.0);
    let r0 = 12.0;
    let cases: &[(f64, f64)] = &[
        (-7.0, -5.0),     // unter −TAU
        (-0.5, 2.0),      // über null hinweg
        (20.0, 22.5),     // über TAU
        (6.0, 7.0),       // über die Nahtstelle 2π
        (3.0, 3.0 + 4.9), // knapp ¾ Kreis mit Anfang über π/2
        (0.1, 0.101),     // winziger Bogen
        (PI, TAU),        // Ende genau bei 360° → 0°
        (-PI, 0.0),       // Anfang bei −180° → 180°, Ende 0°
        (TAU, TAU + 1.0), // Anfang genau bei 360° → 0°
    ];
    for &(a, b) in cases {
        let m = one_view(vec![vis(arc(c, r0, a, b))]);
        let (_, d) = read_of(&m);
        let v = ents(&d, "Vorne");
        assert_eq!(kinds(&v), ["ARC"], "[{a}, {b}]");
        for code in [50, 51] {
            let x: f64 = v[0].0.get(code).unwrap().parse().unwrap();
            assert!(
                (0.0..360.0).contains(&x),
                "[{a}, {b}] Winkel {x} außerhalb [0, 360)"
            );
        }
        let f = |t: f64| (c.0 + r0 * t.cos(), c.1 + r0 * t.sin());
        let formula = poly(&f, a, b, 4_000);
        let dev = two_way_deviation(&[&v[0].1], &formula, 400);
        assert!(dev < 1e-9, "[{a}, {b}]: {dev}");
        assert!(
            (v[0].1.length() - r0 * (b - a)).abs() < 1e-8,
            "[{a}, {b}] Länge"
        );
        // Anfang und Ende in der Richtung gegen den Uhrzeigersinn.
        let (lo, hi) = v[0].1.range();
        assert!(close(v[0].1.at(lo), f(a), 1e-9) && close(v[0].1.at(hi), f(b), 1e-9));
    }
    // Voller Umlauf, egal wo er beginnt: CIRCLE.
    for a in [0.0, -1.0, 3.0, 20.0, -7.0] {
        let m = one_view(vec![vis(arc(c, r0, a, a + TAU))]);
        let (_, d) = read_of(&m);
        assert_eq!(kinds(&ents(&d, "Vorne")), ["CIRCLE"], "Vollkreis ab {a}");
    }
    // Mehr als ein Umlauf zählt als voller Kreis; Öffnungswinkel 0 und Radius 0 zeichnen nichts.
    let (_, d) = read_of(&one_view(vec![vis(arc(c, r0, 0.0, 9.0))]));
    assert_eq!(kinds(&ents(&d, "Vorne")), ["CIRCLE"]);
    let (_, d) = read_of(&one_view(vec![
        vis(arc(c, r0, 1.0, 1.0)),
        vis(arc(c, 0.0, 0.0, 1.0)),
        vis(arc(c, r0, 1.0, 1.0 + 1e-13)),
    ]));
    assert!(ents(&d, "Vorne").is_empty());
    // Negativer Radius spiegelt den Punkt durch den Mittelpunkt: (−r, a…b) = (+r, a+π…b+π).
    let (_, d) = read_of(&one_view(vec![vis(arc(c, -r0, 0.5, 1.5))]));
    let v = ents(&d, "Vorne");
    let f = |t: f64| (c.0 - r0 * t.cos(), c.1 - r0 * t.sin());
    let formula = poly(&f, 0.5, 1.5, 4_000);
    assert!(two_way_deviation(&[&v[0].1], &formula, 400) < 1e-9);
}

#[test]
fn ellipse_arcs_convert_the_parameter_correctly() {
    // Ellipse mit Hauptachse unter 33° (a = 40, Verhältnis 0,35). Der Parameter der DXF-Ellipse
    // ist wie in `Out2d` der Parameter t in c + major·cos t + minor·sin t (nicht der Polarwinkel),
    // gegen den Uhrzeigersinn; die Schreibweise bringt ihn nach [0, 2π].
    let (c, a, ratio) = ((150.0, 120.0), 40.0, 0.35);
    let phi = 33f64.to_radians();
    let major = (a * phi.cos(), a * phi.sin());
    let f = |t: f64| {
        let (u, v) = (a * t.cos(), a * ratio * t.sin());
        (
            c.0 + u * phi.cos() - v * phi.sin(),
            c.1 + u * phi.sin() + v * phi.cos(),
        )
    };
    let cases: &[(f64, f64)] = &[
        (0.5, 2.5),
        (-2.0, 3.0),  // negativer Anfang, Ende dahinter: Umlauf über 2π
        (6.0, 7.0),   // über 2π
        (12.0, 14.0), // weit über 2π
        (-9.0, -6.5),
        (1.0, 1.0 + TAU), // voller Umlauf
        (-2.0, 5.0),      // länger als ein Umlauf: ganz
        (PI, TAU),        // Ende genau bei 2π
    ];
    for &(s, e) in cases {
        let curve = ellipse(c, major, ratio, s, e);
        let (_, d) = read_of(&one_view(vec![vis(curve.clone())]));
        let v = ents(&d, "Vorne");
        assert_eq!(kinds(&v), ["ELLIPSE"], "[{s}, {e}]");
        let Geom::Ellipse {
            c: cc,
            major: mj,
            ratio: rt,
            start,
            end,
        } = v[0].1
        else {
            panic!()
        };
        assert!(close(cc, c, 1e-9));
        assert!(close(mj, major, 1e-9), "Hauptachse {mj:?}");
        assert!((rt - ratio).abs() < 1e-9 && rt <= 1.0);
        assert!(
            (0.0..=TAU).contains(&start) && (0.0..=TAU).contains(&end),
            "[{s}, {e}] → {start} {end}"
        );
        assert_eq!(v[0].0.get(210), Some("0"));
        assert_eq!(v[0].0.get(230), Some("1"));
        // Punkte: Formel über den (bei mehr als einem Umlauf auf einen Umlauf gekürzten) Bereich.
        let hi = s + (e - s).min(TAU);
        let formula = poly(&f, s, hi, 8_000);
        let dev = two_way_deviation(&[&v[0].1], &formula, 400);
        assert!(dev < ell_tol(a), "[{s}, {e}]: {dev}");
        // Und gegen die Bibliothek als Bezug (`Out2d::point`); bei einem vollen Umlauf beginnt
        // die Datei bei 0, die Quelle bei `s`: dort vergleicht der Abstand zur Formelkurve oben.
        if e - s < TAU {
            let (lo, hi_g) = v[0].1.range();
            for i in 0..=200 {
                let tt = i as f64 / 200.0;
                let p = v[0].1.at(lo + (hi_g - lo) * tt);
                let q = curve.point(s + (hi - s) * tt);
                assert!(close(p, (q.x, q.y), ell_tol(a)), "[{s}, {e}] t = {tt}");
            }
        }
        // Länge.
        let flen: f64 = formula.pts.windows(2).map(|w| dist(w[0], w[1])).sum();
        assert!((v[0].1.length() - flen).abs() < 1e-4, "[{s}, {e}] Länge");
    }
    // Der Parameter ist nicht der Polarwinkel: bei t = 0,5 liegt der Punkt bei atan2(…) ≠ 0,5.
    let (_, d) = read_of(&one_view(vec![vis(ellipse(c, major, ratio, 0.5, 2.5))]));
    let v = ents(&d, "Vorne");
    let Geom::Ellipse { start, .. } = v[0].1 else {
        panic!()
    };
    assert!(
        (start - 0.5).abs() < 1e-9,
        "Parameter bleibt Parameter: {start}"
    );
    // Volle Ellipse: 0 … 2π.
    let (_, d) = read_of(&one_view(vec![vis(ellipse(c, major, ratio, 0.0, TAU))]));
    let v = ents(&d, "Vorne");
    assert_eq!(v[0].0.get(41), Some("0"));
    assert_eq!(v[0].0.get(42), Some("6.283185307179586"));
}

#[test]
fn ellipse_ratio_conventions_are_normalised() {
    // Nicht zulässige Verhältnisse werden in die DXF-Form (0 < Verhältnis ≤ 1) umgerechnet,
    // ohne die Kurve zu ändern: > 1 vertauscht die Achsen, < 0 kehrt die Richtung um.
    let c = (100.0, 80.0);
    for (major, ratio, s, e) in [
        ((10.0, 0.0), 2.0, 0.3, 2.0),
        ((6.0, 8.0), 3.5, -1.0, 4.0),
        ((10.0, 0.0), -0.5, 0.3, 2.0),
        ((6.0, 8.0), -0.25, -1.0, 4.0),
        ((3.0, -4.0), -1.5, 1.0, 5.0),
    ] {
        let curve = ellipse(c, major, ratio, s, e);
        let (_, d) = read_of(&one_view(vec![vis(curve.clone())]));
        let v = ents(&d, "Vorne");
        assert_eq!(kinds(&v), ["ELLIPSE"], "{major:?} {ratio}");
        let Geom::Ellipse { ratio: rt, .. } = v[0].1 else {
            panic!()
        };
        assert!(rt > 0.0 && rt <= 1.0, "{rt}");
        // Gleiche Punktmenge wie `Out2d::point` über den Bereich (beide Richtungen).
        let a = dist((0.0, 0.0), major) * ratio.abs().max(1.0);
        let n = 400;
        let pts: Vec<P> = (0..=n)
            .map(|i| {
                let p = curve.point(s + (e - s) * i as f64 / n as f64);
                (p.x, p.y)
            })
            .collect();
        let fc = |t: f64| {
            let p = curve.point(t);
            (p.x, p.y)
        };
        let formula = Curve2 {
            f: &fc,
            t0: s,
            t1: e,
            pts,
        };
        let dev = two_way_deviation(&[&v[0].1], &formula, 200);
        assert!(dev < ell_tol(a), "{major:?} {ratio}: {dev}");
    }
}

#[test]
fn degenerate_curves_are_skipped() {
    // Nichts zu zeichnen: Strecke bleibt (Länge 0 ist gültig), Ellipse ohne Hauptachse, Ellipse
    // ohne Öffnung entfallen; kein Absturz, Datei gültig.
    let (_, d) = read_of(&one_view(vec![
        vis(line((5.0, 5.0), (5.0, 5.0))),
        vis(ellipse((0.0, 0.0), (0.0, 0.0), 0.5, 0.0, 1.0)),
        vis(ellipse((0.0, 0.0), (3.0, 0.0), 0.5, 1.0, 1.0)),
    ]));
    assert_eq!(kinds(&ents(&d, "Vorne")), ["LINE"]);
}

// ---------------------------------------------------------------------------------------------
// Zahlen
// ---------------------------------------------------------------------------------------------

#[test]
fn numbers_are_short_and_plain() {
    let m = one_view(vec![
        vis(line((0.123_456_789_012, -1e-10), (1e-9, 123_456.5))),
        vis(line((-1e-12, 2.0), (1_000.100_000_000_4, 7.0))),
        vis(line((1e12 + 0.25, -3.0), (-0.5, 1e-7))),
    ]);
    let (s, d) = read_of(&m);
    let v = ents(&d, "Vorne");
    let raw = |i: usize, codes: &[i32]| -> Vec<String> {
        codes
            .iter()
            .map(|&c| v[i].0.get(c).unwrap().to_string())
            .collect()
    };
    assert_eq!(
        raw(0, &[10, 20, 11, 21]),
        ["0.123456789", "0", "0.000000001", "123456.5"]
    );
    assert_eq!(raw(1, &[10, 20, 11, 21]), ["0", "2", "1000.1", "7"]);
    assert_eq!(
        raw(2, &[10, 20, 11, 21]),
        ["1000000000000.25", "-3", "-0.5", "0.0000001"]
    );
    // Der Leser verwirft schon beim Einlesen Exponenten, „-0“ und mehr als neun Stellen.
    assert!(!s.contains("e-") && !s.contains("E-") && !s.contains("-0\r\n"));
    // Die z-Koordinaten sind genau 0.
    assert_eq!(v[0].0.get(30), Some("0"));
}

#[test]
fn many_handles_stay_unique_and_hex() {
    // Viele Objekte: die Handles laufen über mehrere Hexadezimalstellen und bleiben einmalig;
    // $HANDSEED liegt darüber.
    let lines: Vec<(Visibility, Out2d)> = (0..700)
        .map(|i| {
            let x = i as f64 * 0.5;
            vis(line((x, 0.0), (x, 10.0)))
        })
        .collect();
    let (_, d) = read_of(&one_view(lines));
    let hs = d.all_handles();
    assert!(hs.len() > 700);
    let mut sorted = hs.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), hs.len());
    assert!(hs.iter().any(|h| h.len() >= 3), "dreistellige Handles");
    assert!(
        hs.iter()
            .any(|h| h.chars().any(|c| ('A'..='F').contains(&c)))
    );
    assert!(hs.iter().all(|h| {
        h.chars()
            .all(|c| c.is_ascii_digit() || ('A'..='F').contains(&c))
    }));
    let seed = u64::from_str_radix(&d.header_var("$HANDSEED").unwrap()[0].value, 16).unwrap();
    assert!(
        seed > hs
            .iter()
            .map(|h| u64::from_str_radix(h, 16).unwrap())
            .max()
            .unwrap()
    );
}

// ---------------------------------------------------------------------------------------------
// Große Blätter
// ---------------------------------------------------------------------------------------------

#[test]
fn large_coordinates_on_a0_roundtrip() {
    // A0 quer (1189 × 841): Elemente nahe der oberen rechten Ecke.
    let sheet = sheet_of(Paper::A0, true);
    let (c, r0) = ((900.5, 500.25), 300.0);
    let ell_c = (600.123, 400.456);
    let ell_a = 150.0;
    let phi = 17f64.to_radians();
    let ctrl: Vec<P> = vec![
        (300.0, 300.0),
        (500.5, 780.0),
        (800.0, 350.0),
        (1100.0, 800.25),
        (1150.0, 600.0),
    ];
    let kn = vec![0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0];
    let m = manual(
        sheet,
        vec![(
            "Oben",
            vec![
                vis(arc(c, r0, 0.0, TAU)),
                vis(line((11.0, 11.0), (1178.0, 830.0))),
                (
                    Visibility::Hidden,
                    ellipse(ell_c, (ell_a * phi.cos(), ell_a * phi.sin()), 0.5, 0.2, 4.0),
                ),
                vis(spline(3, kn.clone(), &ctrl, vec![1.0; 5])),
                vis(arc((1100.0, 700.0), 80.0, 0.3, 4.4)),
            ],
        )],
    );
    let (s, d) = read_of(&m);
    let ext = |name: &str| -> Vec<f64> {
        d.header_var(name)
            .unwrap()
            .iter()
            .map(|g| g.num())
            .collect()
    };
    assert_eq!(ext("$EXTMIN"), [0.0, 0.0, 0.0]);
    assert_eq!(ext("$EXTMAX"), [1189.0, 841.0, 0.0]);
    // Randlinie des Innenrahmens: 10 mm vom Rand.
    let Geom::Poly { pts, .. } = d.entities[0].geom().unwrap() else {
        panic!()
    };
    assert_eq!(
        pts,
        [(10.0, 10.0), (1179.0, 10.0), (1179.0, 831.0), (10.0, 831.0)]
    );
    let v = ents(&d, "Oben");
    assert_eq!(kinds(&v), ["CIRCLE", "LINE", "ELLIPSE", "SPLINE", "ARC"]);
    // Kreis.
    let Geom::Circle { c: cc, r } = v[0].1 else {
        panic!()
    };
    assert!(close(cc, c, 1e-9) && (r - r0).abs() < 1e-9);
    // Strecke.
    let Geom::Line { a, b } = v[1].1 else {
        panic!()
    };
    assert!(close(a, (11.0, 11.0), 1e-9) && close(b, (1178.0, 830.0), 1e-9));
    // Ellipse (verdeckt).
    assert_eq!(v[2].0.layer, "VERDECKT");
    let g = |t: f64| {
        let (u, w) = (ell_a * t.cos(), 0.5 * ell_a * t.sin());
        (
            ell_c.0 + u * phi.cos() - w * phi.sin(),
            ell_c.1 + u * phi.sin() + w * phi.cos(),
        )
    };
    let formula = poly(&g, 0.2, 4.0, 8_000);
    let dev = two_way_deviation(&[&v[2].1], &formula, 400);
    eprintln!("A0 Ellipse a=150: {dev:e}");
    assert!(dev < ell_tol(ell_a), "{dev}");
    // Spline.
    let f = |u: f64| nurbs_eval(3, &kn, &ctrl, &[1.0; 5], u);
    let formula = poly(&f, 0.0, 1.0, 4_000);
    let dev = two_way_deviation(&[&v[3].1], &formula, 1500);
    eprintln!("A0 Spline: {dev:e}");
    assert!(dev < 1e-9, "{dev}");
    // Bogen: Mittelpunkt, Winkel in Grad.
    let Geom::Arc {
        c: ac,
        r: ar,
        start,
        end,
    } = v[4].1
    else {
        panic!()
    };
    assert!(close(ac, (1100.0, 700.0), 1e-9) && (ar - 80.0).abs() < 1e-9);
    assert!((start - 0.3f64.to_degrees()).abs() < 1e-9 && (end - 4.4f64.to_degrees()).abs() < 1e-9);
    // Hochformat A0.
    let (_, d) = read_of(&manual(sheet_of(Paper::A0, false), vec![]));
    let ext = |name: &str| -> Vec<f64> {
        d.header_var(name)
            .unwrap()
            .iter()
            .map(|g| g.num())
            .collect()
    };
    assert_eq!(ext("$EXTMAX"), [841.0, 1189.0, 0.0]);
    assert!(s.contains("1189"));
}

// ---------------------------------------------------------------------------------------------
// Verhalten der Schnittstelle
// ---------------------------------------------------------------------------------------------

#[test]
fn write_matches_string_and_reports_io_errors() {
    let m = manual(
        sheet_of(Paper::A4, false),
        vec![(
            "Vorne",
            vec![(Visibility::Hidden, line((20.0, 20.0), (40.0, 50.0)))],
        )],
    );
    let mut buf: Vec<u8> = Vec::new();
    write_dxf(&m, &mut buf).unwrap();
    assert_eq!(String::from_utf8(buf).unwrap(), dxf_of(&m));
    // Zwei Läufe liefern dieselben Bytes (keine Zufallsanteile).
    assert_eq!(dxf_of(&m), dxf_of(&m));

    struct Broken;
    impl std::io::Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("voll"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert!(matches!(write_dxf(&m, &mut Broken), Err(DrawError::Io(_))));
}

#[test]
fn empty_sheets_are_valid() {
    // Kein einziges Blatt-Objekt: Datei bleibt gültig (Randlinie, zwei Raumblöcke).
    let (_, d) = read_of(&manual(sheet_of(Paper::A4, true), vec![]));
    assert_eq!(d.entities.len(), 1);
    assert!(d.view_block_names().is_empty());
    // Ansicht ohne Linien: leerer Block mit INSERT.
    let (_, d) = read_of(&manual(sheet_of(Paper::A4, true), vec![("Leer", vec![])]));
    assert_eq!(d.view_block_names(), ["Leer"]);
    assert_eq!(d.entities.len(), 2);
}

#[test]
fn compare_with_build_sheet_of_real_bodies() {
    // Mehrere Ansichten eines Körpers aus `build_sheet`: jede Ansicht ihr Block, alle Datei-
    // Prüfungen des Lesers bestehen, die Strecken stimmen mit dem Quader überein.
    let solid = box_solid(25.0, 10.0, 15.0);
    let body = DrawBody {
        name: "K".into(),
        solid: &solid,
    };
    let mut d = Drawing::new(
        DrawingId(1),
        "Z".into(),
        sheet_of(Paper::A3, true),
        Projection::First,
    );
    let mut vls = Vec::new();
    for (i, kind) in [
        ViewKind::Front,
        ViewKind::Top,
        ViewKind::Left,
        ViewKind::Isometric,
    ]
    .into_iter()
    .enumerate()
    {
        let vl = compute_view(std::slice::from_ref(&body), kind, &Progress::new()).unwrap();
        let id = d.add_view(
            kind,
            Scale { num: 1, den: 1 },
            Placement::Free {
                x: 80.0 + 90.0 * i as f64,
                y: 150.0,
            },
            None,
        );
        vls.push((id, vl));
    }
    let res: BTreeMap<ViewId, &ViewLines> = vls.iter().map(|(id, vl)| (*id, vl)).collect();
    let (_, dx) = read_of(&build_sheet(&d, &res));
    assert_eq!(
        dx.view_block_names(),
        ["Vorne", "Oben", "Links", "Isometrisch"]
    );
    // Vorne: 25 × 15, Oben: 25 × 10, Links: 10 × 15.
    for (name, want) in [("Vorne", 80.0), ("Oben", 70.0), ("Links", 50.0)] {
        let total: f64 = ents(&dx, name).iter().map(|(_, g)| g.length()).sum();
        assert!((total - want).abs() < 1e-9, "{name}: {total}");
    }
    // Isometrisch: zwölf Kanten, davon drei verdeckt und nicht gezeichnet (show_hidden aus):
    // neun sichtbare Strecken.
    let iso = ents(&dx, "Isometrisch");
    assert!(iso.iter().all(|(e, _)| e.kind == "LINE"));
    assert!(iso.len() >= 6);
}

#[test]
fn nearly_full_arcs_and_ellipses_become_full() {
    // Öffnung so knapp unter einem Umlauf, dass Anfang und Ende nach dem Runden zusammenfallen:
    // nicht auslassen, sondern voller Kreis bzw. volle Ellipse.
    let c = (50.0, 60.0);
    let (_, d) = read_of(&one_view(vec![
        vis(arc(c, 7.0, 0.3, 0.3 + TAU - 3e-12)),
        vis(ellipse(c, (9.0, 0.0), 0.5, 0.4, 0.4 + TAU - 1e-11)),
    ]));
    let v = ents(&d, "Vorne");
    assert_eq!(kinds(&v), ["CIRCLE", "ELLIPSE"]);
    assert_eq!(v[1].0.get(41), Some("0"));
    assert_eq!(v[1].0.get(42), Some("6.283185307179586"));
    // Öffnung null bleibt ausgelassen; eine winzige, aber von null verschiedene Öffnung bleibt
    // erhalten (die Parameter stehen in voller Genauigkeit).
    let (_, d) = read_of(&one_view(vec![vis(ellipse(c, (9.0, 0.0), 0.5, 0.4, 0.4))]));
    assert!(ents(&d, "Vorne").is_empty());
    let (_, d) = read_of(&one_view(vec![vis(ellipse(
        c,
        (9.0, 0.0),
        0.5,
        0.4,
        0.4 + 1e-12,
    ))]));
    assert_eq!(kinds(&ents(&d, "Vorne")), ["ELLIPSE"]);
}

#[test]
fn full_ellipses_end_exactly_at_two_pi() {
    // Fusion 360 lässt eine volle Ellipse mit Endparameter 6.283185307 (gerundetes 2π) aus; sie
    // muss mit 0 und genau 2π (6.283185307179586) geschrieben werden, in jeder Ansicht.
    let exact_tau = format!("{TAU}");
    assert_eq!(exact_tau, "6.283185307179586");
    // Zylinder isometrisch: die obere Ellipse ist voll.
    let m = solid_model(&cylinder_solid(10.0, 30.0), ViewKind::Isometric, true);
    let (s, d) = read_of(&m);
    let v = ents(&d, "Isometrisch");
    let full: Vec<_> = v
        .iter()
        .filter(|(e, _)| {
            e.kind == "ELLIPSE" && e.get(41) == Some("0") && e.get(42) == Some(exact_tau.as_str())
        })
        .collect();
    assert_eq!(full.len(), 1, "genau eine volle Ellipse (oben)");
    assert!(
        !s.contains(
            "6.283185307
"
        ),
        "kein gerundetes 2π in der Datei"
    );
    // Gekippter Kreis (volle Umlauf) in jeder Ansichtsart: wo er als Ellipse erscheint, ist sie
    // voll mit Ende genau 2π; sonst Kreis oder Strecke.
    let mut ellipses = 0;
    for kind in ViewKind::ALL {
        let s = stroke(
            Curve::Circle {
                frame: frame(v3(2.0, 1.0, 3.0), v3(1.0, 0.0, 0.0), v3(0.0, 0.6, 0.8)),
                radius: 7.0,
            },
            0.0,
            TAU,
        );
        let m = hand_model(
            kind,
            vec![(s, 0.0, TAU)],
            ((-20.0, -20.0), (20.0, 20.0)),
            (200.0, 150.0),
        );
        let (_, d) = read_of(&m);
        let name = kind.label();
        for (e, _) in ents(&d, name) {
            if e.kind == "ELLIPSE" {
                ellipses += 1;
                assert_eq!(
                    (e.get(41), e.get(42)),
                    (Some("0"), Some(exact_tau.as_str())),
                    "{name}"
                );
            }
        }
    }
    assert!(
        ellipses >= 3,
        "mehrere Ansichten zeigen eine Ellipse: {ellipses}"
    );
    // Halbe Ellipsen: Anfang und Ende hängen zusammen (π = 3.141592653589793, Ende der zweiten
    // Hälfte genau 2π und nicht 0).
    let pi_text = format!("{PI}");
    assert_eq!(pi_text, "3.141592653589793");
    let c = (100.0, 80.0);
    let (_, d) = read_of(&one_view(vec![
        vis(ellipse(c, (9.0, 0.0), 0.5, 0.0, PI)),
        vis(ellipse(c, (9.0, 0.0), 0.5, PI, TAU)),
    ]));
    let v = ents(&d, "Vorne");
    assert_eq!(
        (v[0].0.get(41), v[0].0.get(42)),
        (Some("0"), Some(pi_text.as_str()))
    );
    assert_eq!(
        (v[1].0.get(41), v[1].0.get(42)),
        (Some(pi_text.as_str()), Some(exact_tau.as_str()))
    );
    // Parameter mit vielen Stellen bleiben erhalten (kein Runden auf neun Stellen).
    let (s, d) = read_of(&one_view(vec![vis(ellipse(
        c,
        (9.0, 0.0),
        0.5,
        0.123_456_789_012_345,
        2.5,
    ))]));
    assert!(s.contains("0.123456789012345"), "{s}");
    let v = ents(&d, "Vorne");
    assert_eq!(v[0].0.get(41), Some("0.123456789012345"));
}

#[test]
fn nearly_full_arc_within_a_billionth_degree_is_a_circle() {
    // Öffnung höchstens 1e-9° unter 360°: voller Kreis, kein ARC mit Anfang ≈ Ende.
    let c = (50.0, 60.0);
    let almost = TAU - 0.5e-9f64.to_radians();
    let (_, d) = read_of(&one_view(vec![
        vis(arc(c, 7.0, 0.3, 0.3 + almost)),
        vis(arc(c, 7.0, 0.3, 0.3 + TAU - 5e-9f64.to_radians())),
    ]));
    assert_eq!(kinds(&ents(&d, "Vorne")), ["CIRCLE", "ARC"]);
}
