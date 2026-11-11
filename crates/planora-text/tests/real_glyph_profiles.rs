//! Profile aus echten Glyphen (Skizzen 8, Spec D): Montserrat aus den mitgelieferten Schriften
//! (`FontLibrary::bundled_only()`), nie Systemschriften.
//!
//! Die Messtests laufen nur im Release-Build:
//! `cargo test --release -p planora-text --test real_glyph_profiles -- --ignored`

use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::model::{CurveId, Sketch};
use planora_sketch::profile::ExtractedProfiles;
use planora_sketch::solver::Drag;
use planora_sketch::text::{HAlign, TextId, TextParams, TextPlacement, VAlign};
use planora_sketch::text_shape::{TextShape, TextShaper, TextStyle};
use planora_text::FontLibrary;
use std::sync::Arc;
use std::time::Instant;

/// Läufe je Messung; gemeldet und geprüft wird der Median.
const RUNS: usize = 11;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn params(content: &str) -> TextParams {
    TextParams {
        content: content.into(),
        style: TextStyle {
            font: "Montserrat".into(),
            bold: false,
            italic: false,
            spacing: 0.0,
        },
        height: 5.0,
        flip_h: false,
        flip_v: false,
    }
}

fn shape(lib: &FontLibrary, p: &TextParams) -> Arc<TextShape> {
    Arc::new(
        lib.shape(&p.content, &p.style)
            .unwrap_or_else(|e| panic!("{:?}: {e:?}", p.content)),
    )
}

/// Text in Montserrat (Höhe 5) im Rahmen mit den Ecken `a` und `b`, links unten ausgerichtet.
fn box_text(s: &mut Sketch, lib: &FontLibrary, content: &str, a: DVec2, b: DVec2) -> TextId {
    let p = params(content);
    let sh = shape(lib, &p);
    s.add_text_box(a, b, p, HAlign::Left, VAlign::Bottom, sh)
        .unwrap()
}

/// Schwerpunkt des Linienzugs (Mittel der Punkte reicht zum Ordnen nach x).
fn mean(pts: &[DVec2]) -> DVec2 {
    pts.iter().copied().sum::<DVec2>() / pts.len().max(1) as f64
}

/// Lochzahl je Profil, von links nach rechts.
fn holes_left_to_right(e: &ExtractedProfiles) -> Vec<usize> {
    let mut by_x: Vec<(f64, usize)> = e
        .outlines
        .iter()
        .map(|o| (mean(&o[0]).x, o.len() - 1))
        .collect();
    by_x.sort_by(|a, b| a.0.total_cmp(&b.0));
    by_x.into_iter().map(|(_, h)| h).collect()
}

fn median(mut ms: Vec<f64>) -> f64 {
    ms.sort_by(f64::total_cmp);
    ms[ms.len() / 2]
}

#[test]
fn montserrat_hello_profiles() {
    let lib = FontLibrary::bundled_only();
    let mut s = sketch();
    let t = box_text(&mut s, &lib, "Hallo", v(0.0, 0.0), v(30.0, 10.0));
    let e = s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    // H, a, l, l, o; die Innenflächen von a und o sind leer wie in der Schrift (keine Profile).
    assert_eq!(e.profiles.len(), 5, "{:?}", holes_left_to_right(&e));
    assert_eq!(holes_left_to_right(&e), vec![0, 1, 0, 0, 1]);
    for (i, p) in e.profiles.iter().enumerate() {
        assert_eq!(p.holes.len(), e.outlines[i].len() - 1);
    }
    for r in &e.refs {
        assert_eq!(s.text_of_profile(r), Some(t));
    }
}

#[test]
fn overlapping_contours_give_pieces_without_error() {
    let lib = FontLibrary::bundled_only();
    let mut s = sketch();
    let p = params("OO");
    let sh = shape(&lib, &p);
    let advance = sh.lines[0].glyphs[0].advance * p.height;
    // Pfad kürzer als „OO“: „An Pfad anpassen“ gibt eine negative Lücke, die Mitten der beiden O
    // liegen 2,5 mm auseinander, die Ringe überlappen.
    let len = advance + 2.5;
    assert!(len < 2.0 * advance);
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(len, 0.0).unwrap();
    let path = s.add_line(a, b).unwrap();
    s.set_construction(path, true).unwrap();
    let t = s
        .add_text_path(path, p, true, true, HAlign::Left, sh)
        .unwrap();
    let e = s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    // Zwei Ringe überlappen: Teilflächen statt zwei Profile, alle aus demselben Text.
    assert!(e.profiles.len() > 2, "{}", e.profiles.len());
    for r in &e.refs {
        assert_eq!(s.text_of_profile(r), Some(t));
    }
    // Hüllrechteck je O (die erste Hälfte der Kurven gehört zum ersten O).
    let d = s.text(t).unwrap();
    let half = d.curves.len() / 2;
    let bbox = |cs: &[CurveId]| {
        cs.iter()
            .flat_map(|&c| s.curve_polyline(c, 64).unwrap())
            .fold((DVec2::INFINITY, DVec2::NEG_INFINITY), |(lo, hi), q| {
                (lo.min(q), hi.max(q))
            })
    };
    let (lo1, hi1) = bbox(&d.curves[..half]);
    let (lo2, hi2) = bbox(&d.curves[half..]);
    assert!(lo2.x < hi1.x, "die O überlappen");
    // Die Mitte jedes O liegt in seinem leeren Inneren, außerhalb des anderen O: kein Profil.
    for (lo, hi) in [(lo1, hi1), (lo2, hi2)] {
        let mid = (lo + hi) * 0.5;
        assert_eq!(e.profile_at(mid), None, "leeres Inneres bei {mid:?}");
    }
    // Ganz links am ersten und ganz rechts am zweiten O ist Tinte.
    let y = 0.5 * (lo1.y + hi1.y);
    assert!(e.profile_at(v(lo1.x + 0.1, y)).is_some());
    assert!(e.profile_at(v(hi2.x - 0.1, y)).is_some());
}

#[test]
fn text_of_profile_finds_text() {
    let lib = FontLibrary::bundled_only();
    let mut s = sketch();
    let t1 = box_text(&mut s, &lib, "Ob", v(0.0, 0.0), v(20.0, 10.0));
    let t2 = box_text(&mut s, &lib, "d", v(40.0, 0.0), v(60.0, 10.0));
    let e = s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    // O, b, d: je ein Ring mit Loch.
    assert_eq!(e.profiles.len(), 3);
    let mut found: Vec<TextId> = e
        .refs
        .iter()
        .map(|r| s.text_of_profile(r).expect("Textprofil"))
        .collect();
    found.sort_by_key(|t| t.index());
    assert_eq!(found, vec![t1, t1, t2]);
    // Ein gelöschter Text hat keine Profile mehr; der andere bleibt.
    s.remove_text(t2).unwrap();
    let e = s.profiles();
    assert_eq!(e.profiles.len(), 2);
    assert!(e.refs.iter().all(|r| s.text_of_profile(r) == Some(t1)));
}

#[test]
fn mixed_profile_is_not_text() {
    let lib = FontLibrary::bundled_only();
    let mut s = sketch();
    let t = box_text(&mut s, &lib, "lo", v(0.0, 0.0), v(20.0, 10.0));
    // Rechteck um den Text: seine Fläche hat die Glyphen als Löcher, ist aber kein Textprofil.
    let corners = [v(-5.0, -5.0), v(25.0, -5.0), v(25.0, 15.0), v(-5.0, 15.0)];
    let ids: Vec<_> = corners
        .iter()
        .map(|c| s.add_point(c.x, c.y).unwrap())
        .collect();
    let mut rect: Vec<CurveId> = Vec::new();
    for k in 0..4 {
        rect.push(s.add_line(ids[k], ids[(k + 1) % 4]).unwrap());
    }
    // Eine waagerechte Linie vom Rechteck mitten durch das „l“ bis in die Lücke vor dem „o“
    // teilt das „l“ in zwei gemischte Flächen.
    let polylines: Vec<Vec<DVec2>> = s
        .text(t)
        .unwrap()
        .curves
        .iter()
        .map(|&c| s.curve_polyline(c, 64).unwrap())
        .collect();
    // „l“ ist eine Kontur ganz links, „o“ zwei Konturen rechts davon: Die Kurven des „l“ liegen
    // alle links vom Ursprung des „o“.
    let p = params("lo");
    let o_origin = shape(&lib, &p).lines[0].glyphs[1].origin.x * p.height;
    let (l_curves, o_curves): (Vec<&Vec<DVec2>>, Vec<&Vec<DVec2>>) = polylines
        .iter()
        .partition(|pl| pl.iter().all(|p| p.x < o_origin));
    assert!(!l_curves.is_empty() && !o_curves.is_empty());
    let l_lo_y = l_curves
        .iter()
        .flat_map(|pl| pl.iter())
        .map(|p| p.y)
        .fold(f64::INFINITY, f64::min);
    let l_hi_y = l_curves
        .iter()
        .flat_map(|pl| pl.iter())
        .map(|p| p.y)
        .fold(f64::NEG_INFINITY, f64::max);
    let l_hi_x = l_curves
        .iter()
        .flat_map(|pl| pl.iter())
        .map(|p| p.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let o_lo_x = o_curves
        .iter()
        .flat_map(|pl| pl.iter())
        .map(|p| p.x)
        .fold(f64::INFINITY, f64::min);
    assert!(l_hi_x < o_lo_x);
    let y = 0.5 * (l_lo_y + l_hi_y);
    let a = s.add_point(-5.0, y).unwrap();
    let b = s.add_point(0.5 * (l_hi_x + o_lo_x), y).unwrap();
    let cut = s.add_line(a, b).unwrap();
    let e = s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    let text_profiles = e
        .refs
        .iter()
        .filter(|r| s.text_of_profile(r) == Some(t))
        .count();
    // Nur das „o“ ist noch ein reines Textprofil.
    assert_eq!(text_profiles, 1, "{} Profile", e.profiles.len());
    let mixed: Vec<_> = e
        .refs
        .iter()
        .filter(|r| s.text_of_profile(r).is_none())
        .collect();
    assert!(!mixed.is_empty());
    for r in mixed {
        let curves: Vec<CurveId> = r.outer.iter().chain(&r.holes).map(|p| p.curve).collect();
        assert!(
            curves.iter().any(|c| rect.contains(c) || *c == cut),
            "gemischtes Profil ohne fremde Kante: {curves:?}"
        );
    }
    // Die zwei Hälften des „l“ und die Fläche im Rechteck.
    assert_eq!(e.profiles.len(), 4, "{}", e.profiles.len());
}

/// 40 Zeichen, eine Zeile.
const FORTY: &str = "Hallo Welt, PlanoraCAD setzt Text genau.";
/// 200 Zeichen in fünf Zeilen zu 40 (Zeilenumbrüche zählen mit; 196 sichtbare Zeichen und
/// vier Umbrüche).
fn two_hundred() -> String {
    let lines = [
        "Profile aus Glyphen: Hallo Welt, wir bau",
        "en einen Text mit vielen Kurven fuer den",
        "Messtest. Jede Zeile hat vierzig Zeichen",
        "und alle zusammen ergeben zweihundert Ze",
        "ichen mit Umbruechen. Das ist genug.",
    ];
    let s = lines.join("\n");
    assert_eq!(s.chars().count(), 200, "{}", s.chars().count());
    s
}

fn measure_profiles(s: &Sketch) -> (f64, Vec<f64>, usize) {
    let first = s.profiles();
    assert!(first.rejected.is_empty(), "{:?}", first.rejected);
    let ms: Vec<f64> = (0..RUNS)
        .map(|_| {
            let t = Instant::now();
            let e = s.profiles();
            let dt = t.elapsed().as_secs_f64() * 1e3;
            assert_eq!(e.profiles.len(), first.profiles.len());
            dt
        })
        .collect();
    (median(ms.clone()), ms, first.profiles.len())
}

#[test]
#[ignore = "Messtest, nur im Release-Build sinnvoll"]
fn forty_chars_profiles_under_150ms() {
    let lib = FontLibrary::bundled_only();
    let mut s = sketch();
    assert_eq!(FORTY.chars().count(), 40);
    let t = box_text(&mut s, &lib, FORTY, v(0.0, 0.0), v(150.0, 10.0));
    let curves = s.text(t).unwrap().curves.len();
    let (med, ms, n) = measure_profiles(&s);
    println!("40 Zeichen ({curves} Kurven, {n} Profile): Median {med:.1} ms ({ms:.1?})");
    assert!(med <= 150.0, "Median {med:.1} ms");
}

#[test]
#[ignore = "Messtest, nur im Release-Build sinnvoll"]
fn two_hundred_chars_under_1s() {
    let lib = FontLibrary::bundled_only();
    let mut s = sketch();
    let t = box_text(&mut s, &lib, &two_hundred(), v(0.0, 0.0), v(150.0, 40.0));
    let curves = s.text(t).unwrap().curves.len();
    let (med, ms, n) = measure_profiles(&s);
    println!("200 Zeichen ({curves} Kurven, {n} Profile): Median {med:.1} ms ({ms:.1?})");
    assert!(med <= 1000.0, "Median {med:.1} ms");
}

#[test]
#[ignore = "Messtest, nur im Release-Build sinnvoll"]
fn refresh_200_chars_under_20ms() {
    let lib = FontLibrary::bundled_only();
    let mut s = sketch();
    let t = box_text(&mut s, &lib, &two_hundred(), v(0.0, 0.0), v(150.0, 40.0));
    let TextPlacement::Box { corners, .. } = s.text(t).unwrap().placement else {
        panic!("kein Rahmen");
    };
    let before: Vec<DVec2> = s
        .text(t)
        .unwrap()
        .points
        .iter()
        .map(|&p| s.point_xy(p).unwrap())
        .collect();
    let ids = s.text(t).unwrap().curves.clone();
    // Je Bild ein Zug an Ecke 0: Der ganze Rahmen und damit jede Glyphe bewegt sich.
    let ms: Vec<f64> = (0..RUNS)
        .map(|k| {
            let target = v(0.5 * (k + 1) as f64, 0.25 * (k + 1) as f64);
            let t0 = Instant::now();
            s.drag(&[Drag {
                point: corners[0],
                target,
            }])
            .unwrap();
            t0.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    let d = s.text(t).unwrap();
    assert_eq!(d.curves, ids, "gleiche Ids");
    let after: Vec<DVec2> = d.points.iter().map(|&p| s.point_xy(p).unwrap()).collect();
    assert!(
        before.iter().zip(&after).all(|(a, b)| a.distance(*b) > 1.0),
        "alle Glyphen folgen dem Rahmen"
    );
    let med = median(ms.clone());
    println!(
        "Zug am Rahmen, 200 Zeichen ({} Kurven): Median {med:.2} ms ({ms:.2?})",
        ids.len()
    );
    assert!(med <= 20.0, "Median {med:.2} ms");
}
