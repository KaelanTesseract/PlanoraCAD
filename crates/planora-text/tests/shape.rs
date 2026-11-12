//! Setzen und Umrisse (`TextShaper for FontLibrary`). Alle Tests benutzen nur die
//! mitgelieferten Schriften, nie die des Systems.

use planora_math::DVec2;
use planora_sketch::text_shape::{
    Contour, Glyph, MAX_TEXT_CURVES, Seg, TextShape, TextShapeError, TextShaper, TextStyle,
};
use planora_text::FontLibrary;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen, pen::PathStyle};
use skrifa::raw::TableProvider;
use skrifa::raw::tables::glyf::Glyph as RawGlyph;
use skrifa::{FontRef, MetadataProvider};

const MONTSERRAT: &[u8] = include_bytes!("../../planora-app/assets/fonts/Montserrat-Regular.ttf");
const ROBOTO: &[u8] = include_bytes!("../../planora-app/assets/fonts/Roboto-Regular.ttf");

/// Nachgebildete Schnitte laut Vorgabe: Scherung, Stärke beim Fettmachen (gesamt, nicht je Seite).
const SHEAR: f64 = 0.2126;
const BOLD: f64 = 0.02;

fn style(font: &str, bold: bool, italic: bool, spacing: f64) -> TextStyle {
    TextStyle {
        font: font.to_string(),
        bold,
        italic,
        spacing,
    }
}

fn shape(text: &str, style: &TextStyle) -> TextShape {
    FontLibrary::bundled_only()
        .shape(text, style)
        .unwrap_or_else(|e| panic!("{text:?}: {e:?}"))
}

fn montserrat(text: &str) -> TextShape {
    shape(text, &style("Montserrat", false, false, 0.0))
}

fn roboto(text: &str, bold: bool, italic: bool) -> TextShape {
    shape(text, &style("Roboto", bold, italic, 0.0))
}

fn only_glyph(shape: &TextShape) -> &Glyph {
    assert_eq!(shape.lines.len(), 1);
    assert_eq!(shape.lines[0].glyphs.len(), 1, "{:?}", shape.lines[0]);
    &shape.lines[0].glyphs[0]
}

fn units_per_em(data: &[u8]) -> f64 {
    f64::from(FontRef::new(data).unwrap().head().unwrap().units_per_em())
}

/// Punkt auf einem Stück bei `t`.
fn point_at(start: DVec2, seg: &Seg, t: f64) -> DVec2 {
    let s = 1.0 - t;
    match *seg {
        Seg::Line(p) => start * s + p * t,
        Seg::Quad(c, p) => start * (s * s) + c * (2.0 * s * t) + p * (t * t),
        Seg::Cubic(c1, c2, p) => {
            start * (s * s * s) + c1 * (3.0 * s * s * t) + c2 * (3.0 * s * t * t) + p * (t * t * t)
        }
    }
}

/// Stücke mit ihrem Anfangspunkt.
fn with_starts(contour: &Contour) -> Vec<(DVec2, Seg)> {
    let mut start = contour.start;
    contour
        .segs
        .iter()
        .map(|seg| {
            let item = (start, *seg);
            start = seg.end();
            item
        })
        .collect()
}

/// Hüllquader aus dicht abgetasteten Punkten (min, max).
fn bbox(contours: &[Contour]) -> (DVec2, DVec2) {
    let mut min = DVec2::splat(f64::INFINITY);
    let mut max = DVec2::splat(f64::NEG_INFINITY);
    for contour in contours {
        for (start, seg) in with_starts(contour) {
            for i in 0..=64 {
                let p = point_at(start, &seg, f64::from(i) / 64.0);
                min = min.min(p);
                max = max.max(p);
            }
        }
    }
    (min, max)
}

/// Alle Punkte einer Kontur: Anfang und alle Punkte der Stücke.
fn points(contour: &Contour) -> Vec<DVec2> {
    let mut out = vec![contour.start];
    for seg in &contour.segs {
        match *seg {
            Seg::Line(p) => out.push(p),
            Seg::Quad(c, p) => out.extend([c, p]),
            Seg::Cubic(c1, c2, p) => out.extend([c1, c2, p]),
        }
    }
    out
}

fn assert_closed(shape: &TextShape) {
    for line in &shape.lines {
        for glyph in &line.glyphs {
            for contour in &glyph.contours {
                assert!(!contour.segs.is_empty());
                assert_eq!(contour.segs.last().unwrap().end(), contour.start);
            }
        }
    }
}

#[test]
fn o_has_two_contours() {
    let shape = montserrat("o");
    assert_closed(&shape);
    let glyph = only_glyph(&shape);
    assert_eq!(glyph.contours.len(), 2);
    let (outer, inner) = {
        let a = bbox(&glyph.contours[..1]);
        let b = bbox(&glyph.contours[1..]);
        if (a.1 - a.0).x > (b.1 - b.0).x {
            (a, b)
        } else {
            (b, a)
        }
    };
    // Das Loch liegt ganz im Äußeren.
    assert!(inner.0.x > outer.0.x && inner.1.x < outer.1.x);
    assert!(inner.0.y > outer.0.y && inner.1.y < outer.1.y);
    // Kleinbuchstabe: etwa halbe Höhe, knapp unter der Grundlinie beginnend.
    assert!(outer.0.y > -0.03 && outer.0.y < 0.0, "{outer:?}");
    assert!(outer.1.y > 0.45 && outer.1.y < 0.6, "{outer:?}");
    for contour in &glyph.contours {
        assert!(contour.segs.len() >= 4, "{contour:?}");
    }
}

#[test]
fn i_has_two_contours() {
    let shape = montserrat("i");
    assert_closed(&shape);
    let glyph = only_glyph(&shape);
    assert_eq!(glyph.contours.len(), 2);
    let a = bbox(&glyph.contours[..1]);
    let b = bbox(&glyph.contours[1..]);
    let (stem, dot) = if a.0.y < b.0.y { (a, b) } else { (b, a) };
    // Der Punkt sitzt über dem Stamm.
    assert!(dot.0.y > stem.1.y, "{stem:?} {dot:?}");
    assert!(stem.0.y.abs() < 1e-9, "{stem:?}");
}

#[test]
fn b_upper_has_three_contours() {
    let shape = montserrat("B");
    assert_closed(&shape);
    let glyph = only_glyph(&shape);
    assert_eq!(glyph.contours.len(), 3);
    let (min, max) = bbox(&glyph.contours);
    // Großbuchstaben sind etwa 70 % der Höhe.
    assert!(min.y.abs() < 1e-9, "{min:?}");
    assert!(max.y > 0.65 && max.y < 0.75, "{max:?}");
    assert!(min.x > 0.0 && max.x < glyph.advance, "{min:?} {max:?}");
}

/// Zeichnet mit `skrifa` und merkt sich die quadratischen Stücke (Anfang, Kontrolle, Ende) in
/// Einheiten der Höhe.
#[derive(Default)]
struct QuadRecorder {
    scale: f64,
    current: DVec2,
    quads: Vec<[DVec2; 3]>,
}

impl OutlinePen for QuadRecorder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.current = DVec2::new(f64::from(x), f64::from(y)) * self.scale;
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.current = DVec2::new(f64::from(x), f64::from(y)) * self.scale;
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let c = DVec2::new(f64::from(cx), f64::from(cy)) * self.scale;
        let p = DVec2::new(f64::from(x), f64::from(y)) * self.scale;
        self.quads.push([self.current, c, p]);
        self.current = p;
    }
    fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, x: f32, y: f32) {
        self.current = DVec2::new(f64::from(x), f64::from(y)) * self.scale;
    }
    fn close(&mut self) {}
}

fn collinear(q: &[DVec2; 3]) -> bool {
    let chord = q[2] - q[0];
    let len = chord.length();
    len > 1e-9 && (chord.perp_dot(q[1] - q[0]) / len).abs() < 1e-9
}

#[test]
fn quadratics_stay_exact() {
    // Montserrat ist TrueType; jedes Zeichen ergibt hier genau eine Glyphe.
    let text = "BOQSabcdegosw023&";
    let shape = montserrat(text);
    let font = FontRef::new(MONTSERRAT).unwrap();
    let scale = 1.0 / units_per_em(MONTSERRAT);
    let glyphs = &shape.lines[0].glyphs;
    assert_eq!(glyphs.len(), text.chars().count());
    let mut checked = 0;
    for (ch, glyph) in text.chars().zip(glyphs) {
        let gid = font.charmap().map(ch).unwrap();
        let outline = font.outline_glyphs().get(gid).unwrap();
        let mut pen = QuadRecorder {
            scale,
            ..Default::default()
        };
        let settings = DrawSettings::unhinted(Size::unscaled(), LocationRef::default())
            .with_path_style(PathStyle::HarfBuzz);
        outline.draw(settings, &mut pen).unwrap();

        let ours: Vec<[DVec2; 3]> = glyph
            .contours
            .iter()
            .flat_map(with_starts)
            .filter_map(|(start, seg)| match seg {
                Seg::Quad(c, p) => Some([start, c, p]),
                _ => None,
            })
            .collect();
        let close = |a: &[DVec2; 3], b: &[DVec2; 3]| {
            (0..=32).all(|i| {
                let t = f64::from(i) / 32.0;
                let pa = point_at(a[0], &Seg::Quad(a[1], a[2]), t);
                let pb = point_at(b[0], &Seg::Quad(b[1], b[2]), t);
                (pa - pb).length() < 1e-9
            })
        };
        for q in &ours {
            assert!(pen.quads.iter().any(|p| close(q, p)), "{ch}: {q:?}");
        }
        for p in pen.quads.iter().filter(|p| !collinear(p)) {
            assert!(ours.iter().any(|q| close(q, p)), "{ch}: fehlt {p:?}");
        }
        checked += ours.len();
    }
    assert!(checked > 50, "{checked}");
}

#[test]
fn implied_on_curve_points_are_explicit() {
    let font = FontRef::new(MONTSERRAT).unwrap();
    let scale = 1.0 / units_per_em(MONTSERRAT);
    let glyf = font.glyf().unwrap();
    let loca = font.loca(None).unwrap();
    let mut implied_total = 0;
    for ch in ['O', 'o', 'S', 'e'] {
        let gid = font.charmap().map(ch).unwrap();
        let Some(RawGlyph::Simple(raw)) = loca.get_glyf(gid, &glyf).unwrap() else {
            panic!("{ch}: keine einfache Glyphe");
        };
        // Rohpunkte je Kontur, dazu die versteckten Punkte zwischen zwei Kontrollpunkten.
        let all: Vec<_> = raw.points().collect();
        let mut on_curve = Vec::new();
        let mut off_curve = Vec::new();
        let mut implied = Vec::new();
        let mut first = 0usize;
        for end in raw.end_pts_of_contours() {
            let end = end.get() as usize;
            let pts = &all[first..=end];
            for (i, p) in pts.iter().enumerate() {
                let here = DVec2::new(f64::from(p.x), f64::from(p.y)) * scale;
                if p.on_curve {
                    on_curve.push(here);
                    continue;
                }
                off_curve.push(here);
                let next = pts[(i + 1) % pts.len()];
                if !next.on_curve {
                    let there = DVec2::new(f64::from(next.x), f64::from(next.y)) * scale;
                    implied.push((here + there) * 0.5);
                }
            }
            first = end + 1;
        }
        assert!(!implied.is_empty(), "{ch}: kein versteckter Punkt");
        implied_total += implied.len();

        let shape = montserrat(&ch.to_string());
        let glyph = only_glyph(&shape);
        let near = |list: &[DVec2], p: DVec2| list.iter().any(|q| (*q - p).length() < 1e-9);
        let ends: Vec<DVec2> = glyph
            .contours
            .iter()
            .flat_map(|c| c.segs.iter().map(|s| s.end()))
            .collect();
        // Jeder versteckte Punkt ist ausdrücklich ein Endpunkt eines Stücks.
        for p in &implied {
            assert!(near(&ends, *p), "{ch}: {p:?} fehlt");
        }
        for contour in &glyph.contours {
            let segs = &contour.segs;
            for (i, seg) in segs.iter().enumerate() {
                if let Seg::Quad(c, p) = *seg {
                    assert!(near(&off_curve, c), "{ch}: Kontrollpunkt {c:?}");
                    // Zwei Quads hintereinander treffen sich an einem ausdrücklichen Punkt:
                    // einem Punkt auf der Kurve aus der Datei oder einem versteckten.
                    if let Seg::Quad(..) = segs[(i + 1) % segs.len()] {
                        assert!(near(&on_curve, p) || near(&implied, p), "{ch}: {p:?}");
                    }
                }
            }
        }
    }
    assert!(implied_total > 10, "{implied_total}");
}

fn width(shape: &TextShape) -> f64 {
    shape.lines[0].width
}

#[test]
fn kerning_applies() {
    let av = width(&montserrat("AV"));
    let a = width(&montserrat("A"));
    let v = width(&montserrat("V"));
    assert!(av < a + v - 1e-3, "AV {av}, A {a}, V {v}");
}

#[test]
fn spacing_percent_extends_advances_except_last() {
    let plain = montserrat("AB");
    let wide = shape("AB", &style("Montserrat", false, false, 50.0));
    let adv_a = plain.lines[0].glyphs[0].advance;
    assert!(adv_a > 0.3, "{adv_a}");
    assert!((width(&wide) - width(&plain) - 0.5 * adv_a).abs() < 1e-12);
    // Der eigene Vorschub bleibt, nur die Lage der folgenden Glyphe ändert sich.
    for (a, b) in plain.lines[0].glyphs.iter().zip(&wide.lines[0].glyphs) {
        assert_eq!(a.advance, b.advance);
    }
    let shift = wide.lines[0].glyphs[1].origin.x - plain.lines[0].glyphs[1].origin.x;
    assert!((shift - 0.5 * adv_a).abs() < 1e-12);
    // Eine Glyphe allein: Der letzte Vorschub wird nie verlängert.
    let one = width(&shape("A", &style("Montserrat", false, false, 300.0)));
    assert!((one - width(&montserrat("A"))).abs() < 1e-12);
}

#[test]
fn spacing_keeps_combining_accent_over_its_base() {
    // „x“ mit Akzent gibt es nicht als ein Zeichen: Der Akzent bleibt eine eigene Glyphe.
    // Er gehört zum x und wandert mit ihm, nicht mit dem Abstand hinter dem x.
    let text = "ax\u{0301}b";
    let plain = shape(text, &style("Roboto", false, false, 0.0));
    let wide = shape(text, &style("Roboto", false, false, 100.0));
    let (p, w) = (&plain.lines[0].glyphs, &wide.lines[0].glyphs);
    assert_eq!(p.len(), 4, "{p:?}");
    assert!(!p[2].contours.is_empty() && p[2].advance == 0.0);
    let (adv_a, adv_x) = (p[0].advance, p[1].advance);
    let shift = |i: usize| w[i].origin.x - p[i].origin.x;
    assert!(shift(0).abs() < 1e-12);
    assert!((shift(1) - adv_a).abs() < 1e-12);
    assert!((shift(2) - adv_a).abs() < 1e-12, "Akzent verrutscht");
    assert!((shift(3) - adv_a - adv_x).abs() < 1e-12);
}

#[test]
fn newline_makes_lines() {
    let shape = montserrat("a\nb");
    assert_eq!(shape.lines.len(), 2);
    assert_eq!(shape.lines[0].glyphs.len(), 1);
    assert_eq!(shape.lines[1].glyphs.len(), 1);
    let font = FontRef::new(MONTSERRAT).unwrap();
    let hhea = font.hhea().unwrap();
    let upem = units_per_em(MONTSERRAT);
    let asc = f64::from(hhea.ascender().to_i16()) / upem;
    let desc = f64::from(hhea.descender().to_i16()) / upem;
    let gap = f64::from(hhea.line_gap().to_i16()) / upem;
    assert!((shape.ascender - asc).abs() < 1e-12);
    assert!((shape.descender - desc).abs() < 1e-12);
    assert!((shape.line_height - (asc - desc + gap)).abs() < 1e-12);
    assert!(shape.line_height > 1.0 && shape.descender < 0.0);

    // Windows-Zeilenende und leere letzte Zeile.
    let crlf = montserrat("a\r\nb\n");
    assert_eq!(crlf.lines.len(), 3);
    assert_eq!(crlf.lines[0].glyphs.len(), 1);
    assert!(crlf.lines[2].glyphs.is_empty());
    assert_eq!(crlf.lines[2].width, 0.0);
    assert!(!crlf.missing_chars);
}

#[test]
fn spaces_advance_without_contours() {
    let spaced = montserrat("a b");
    let glyphs = &spaced.lines[0].glyphs;
    assert_eq!(glyphs.len(), 3);
    assert!(glyphs[1].contours.is_empty());
    assert!(glyphs[1].advance > 0.1);
    assert!(width(&spaced) > width(&montserrat("ab")) + 0.1);
    assert!(!spaced.missing_chars);

    // Ein Tabulator setzt Leerraum wie Leerzeichen, ohne „Zeichen fehlen“.
    let tab = montserrat("a\tb");
    assert!(!tab.missing_chars);
    let empty: Vec<&Glyph> = tab.lines[0]
        .glyphs
        .iter()
        .filter(|g| g.contours.is_empty())
        .collect();
    assert!(!empty.is_empty());
    assert!(width(&tab) > width(&spaced));
}

#[test]
fn combining_accent_shapes() {
    let e = montserrat("e");
    let (e_min, e_max) = bbox(&only_glyph(&e).contours);
    let accented = montserrat("e\u{0301}");
    assert!(!accented.missing_chars);
    // Alle Konturen des gesetzten „é“ in Zeilenlage.
    let mut above = Vec::new();
    for glyph in &accented.lines[0].glyphs {
        for contour in &glyph.contours {
            let (min, max) = bbox(std::slice::from_ref(contour));
            let (min, max) = (min + glyph.origin, max + glyph.origin);
            if min.y > e_max.y {
                above.push((min, max));
            }
        }
    }
    assert!(!above.is_empty(), "kein Akzent über dem e");
    // Der Akzent steht über dem e, nicht daneben.
    for (min, max) in above {
        assert!(max.x > e_min.x && min.x < e_max.x, "{min:?} {max:?}");
    }
}

#[test]
fn emoji_without_outline_sets_missing_chars() {
    let shape = montserrat("a😀");
    assert!(shape.missing_chars);
    let glyphs = &shape.lines[0].glyphs;
    assert!(glyphs.len() >= 2);
    assert!(!glyphs[0].contours.is_empty());
    assert!(glyphs[1..].iter().all(|g| g.contours.is_empty()));
    assert!(shape.substituted.is_none());
}

#[test]
fn character_missing_in_font_comes_from_roboto() {
    // Montserrat hat kein griechisches Alpha, Roboto schon.
    let ch = 'α';
    assert!(
        FontRef::new(MONTSERRAT)
            .unwrap()
            .charmap()
            .map(ch)
            .is_none()
    );
    assert!(FontRef::new(ROBOTO).unwrap().charmap().map(ch).is_some());
    let shape = montserrat("aαb");
    assert!(!shape.missing_chars);
    let glyphs = &shape.lines[0].glyphs;
    assert_eq!(glyphs.len(), 3);
    assert!(glyphs.iter().all(|g| !g.contours.is_empty()));
    // In der Höhe der Kleinbuchstaben, also richtig skaliert (Roboto hat 2048 Einheiten je
    // Geviert, Montserrat 1000).
    let (min, max) = bbox(&glyphs[1].contours);
    let (_, a_max) = bbox(&glyphs[0].contours);
    assert!(
        min.y.abs() < 0.03 && (max.y - a_max.y).abs() < 0.05,
        "{min:?} {max:?}"
    );
    // Die Glyphen stehen hintereinander.
    assert!(glyphs[1].origin.x >= glyphs[0].origin.x + glyphs[0].advance - 1e-9);
    assert!(glyphs[2].origin.x >= glyphs[1].origin.x + glyphs[1].advance - 1e-9);
}

#[test]
fn missing_family_reports_substitute() {
    let shape = shape("Ab", &style("Gibtsnicht", false, false, 0.0));
    assert_eq!(shape.substituted.as_deref(), Some("Gibtsnicht"));
    assert!(!shape.lines[0].glyphs[0].contours.is_empty());
    // Ersatz ist Montserrat: gleiche Form wie direkt mit Montserrat.
    assert_eq!(shape.lines, montserrat("Ab").lines);
    assert!(montserrat("Ab").substituted.is_none());
}

#[test]
fn synthetic_italic_shears() {
    let upright = roboto("i", false, false);
    let italic = roboto("i", false, true);
    assert!(!upright.synthetic);
    assert!(italic.synthetic);
    let a = only_glyph(&upright);
    let b = only_glyph(&italic);
    assert_eq!(a.origin, b.origin);
    assert_eq!(a.advance, b.advance);
    assert_eq!(a.contours.len(), b.contours.len());
    // Die oberste Kontur (der Punkt) ist um 0,2126 · y nach rechts verschoben.
    let top = (0..a.contours.len())
        .max_by(|&i, &j| {
            let yi = bbox(&a.contours[i..=i]).1.y;
            let yj = bbox(&a.contours[j..=j]).1.y;
            yi.total_cmp(&yj)
        })
        .unwrap();
    let (pa, pb) = (points(&a.contours[top]), points(&b.contours[top]));
    assert_eq!(pa.len(), pb.len());
    assert!(pa.iter().all(|p| p.y > 0.5));
    for (p, q) in pa.iter().zip(&pb) {
        assert!((q.x - p.x - SHEAR * p.y).abs() < 1e-12, "{p:?} {q:?}");
        assert_eq!(q.y, p.y);
    }
}

#[test]
fn synthetic_bold_widens() {
    // Roboto „I“ ist ein Rechteck: Das Verbreitern ist dort genau.
    let regular = roboto("I", false, false);
    let bold = roboto("I", true, false);
    assert!(bold.synthetic && !regular.synthetic);
    let a = only_glyph(&regular);
    let b = only_glyph(&bold);
    let (amin, amax) = bbox(&a.contours);
    let (bmin, bmax) = bbox(&b.contours);
    // Wie `FT_Outline_Embolden`: insgesamt um 0,02 breiter und höher (0,01 je Seite), linker
    // und unterer Rand bleiben stehen, nichts reicht unter die Grundlinie.
    let grow = (bmax - bmin) - (amax - amin);
    assert!((grow.x - BOLD).abs() < 1e-9, "{grow:?}");
    assert!((grow.y - BOLD).abs() < 1e-9, "{grow:?}");
    assert!((bmin.x - amin.x).abs() < 1e-9, "{amin:?} {bmin:?}");
    assert!((bmin.y - amin.y).abs() < 1e-9, "{amin:?} {bmin:?}");
    assert!(bmin.y >= -1e-9, "{bmin:?}");
    assert!((bmax - amax - DVec2::splat(BOLD)).length() < 1e-9);
    // Der Vorschub wächst um die Stärke.
    assert!((b.advance - a.advance - BOLD).abs() < 1e-12);
    // Der Stamm selbst ist 0,02 dicker: 0,01 je Seite um seine verschobene Mitte.
    let (a_mid, b_mid) = ((amin.x + amax.x) / 2.0, (bmin.x + bmax.x) / 2.0);
    assert!((b_mid - a_mid - BOLD / 2.0).abs() < 1e-9);

    // „O“ mit Kurven: ungefähr ebenso, das Loch wird je Seite um 0,01 kleiner; der untere Rand
    // (Überhang unter der Grundlinie) bleibt.
    let a = roboto("O", false, false);
    let b = roboto("O", true, false);
    let (amin, amax) = bbox(&only_glyph(&a).contours);
    let (bmin, bmax) = bbox(&only_glyph(&b).contours);
    let grow = (bmax - bmin) - (amax - amin);
    assert!((grow.x - BOLD).abs() < 1e-3, "{grow:?}");
    assert!((grow.y - BOLD).abs() < 1e-3, "{grow:?}");
    assert!((bmin.y - amin.y).abs() < 1e-3, "{amin:?} {bmin:?}");
    assert!((bmin.x - amin.x).abs() < 1e-3, "{amin:?} {bmin:?}");
    let hole = |s: &TextShape| {
        let g = only_glyph(s);
        let boxes: Vec<_> = g
            .contours
            .iter()
            .map(|c| bbox(std::slice::from_ref(c)))
            .collect();
        boxes
            .into_iter()
            .map(|(min, max)| (max - min).x)
            .fold(f64::INFINITY, f64::min)
    };
    let shrink = hole(&a) - hole(&b);
    assert!((shrink - BOLD).abs() < 1e-3, "{shrink}");
}

#[test]
fn limits() {
    let lib = FontLibrary::bundled_only();
    let regular = style("Montserrat", false, false, 0.0);
    // 500 Zeichen gehen (Leerzeichen ergeben keine Kurven), 501 nicht.
    let five_hundred = format!("a{}", " ".repeat(499));
    assert!(lib.shape(&five_hundred, &regular).is_ok());
    assert_eq!(
        lib.shape(&format!("{five_hundred}b"), &regular),
        Err(TextShapeError::TooLong)
    );
    assert_eq!(
        lib.shape(&"a".repeat(501), &regular),
        Err(TextShapeError::TooLong)
    );
    // Zeilenumbrüche zählen als Zeichen.
    assert_eq!(
        lib.shape(&"ab\n".repeat(167), &regular),
        Err(TextShapeError::TooLong)
    );

    let per_w = lib.shape("W", &regular).unwrap().segment_count();
    assert!(per_w * 500 > MAX_TEXT_CURVES, "{per_w}");
    let fits = MAX_TEXT_CURVES / per_w;
    let ok = lib.shape(&"W".repeat(fits), &regular).unwrap();
    assert_eq!(ok.segment_count(), fits * per_w);
    assert_eq!(
        lib.shape(&"W".repeat(fits + 1), &regular),
        Err(TextShapeError::TooManyCurves)
    );
    assert_eq!(
        lib.shape(&"W".repeat(500), &regular),
        Err(TextShapeError::TooManyCurves)
    );
}

#[test]
fn whitespace_only_is_empty() {
    let lib = FontLibrary::bundled_only();
    let regular = style("Montserrat", false, false, 0.0);
    for text in ["", " ", "   ", "\t", "\n", "\n\n", " \t\r\n "] {
        assert_eq!(
            lib.shape(text, &regular),
            Err(TextShapeError::Empty),
            "{text:?}"
        );
    }
}

#[test]
fn rtl_text_shapes_without_panic() {
    for font in ["Roboto", "Montserrat"] {
        for text in ["שלום עולם", "a שלום b", "مرحبا"] {
            let shape = shape(text, &style(font, false, false, 10.0));
            let has_contours = shape
                .lines
                .iter()
                .flat_map(|l| &l.glyphs)
                .any(|g| !g.contours.is_empty());
            assert!(has_contours || shape.missing_chars, "{font} {text:?}");
            for glyph in &shape.lines[0].glyphs {
                assert!(glyph.origin.is_finite() && glyph.advance.is_finite());
            }
            assert!(shape.lines[0].width.is_finite());
        }
    }
}

#[test]
fn every_coordinate_is_finite_and_contours_closed() {
    let text = "Hallo Welt! ÄÖÜ äöüß €@&%? 0123456789 fi fl ff";
    for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
        for font in ["Montserrat", "Roboto"] {
            let shape = shape(text, &style(font, bold, italic, 25.0));
            assert_closed(&shape);
            assert!(!shape.missing_chars, "{font} {bold} {italic}");
            for glyph in shape.lines.iter().flat_map(|l| &l.glyphs) {
                for contour in &glyph.contours {
                    assert!(points(contour).iter().all(|p| p.is_finite()));
                }
            }
        }
    }
}

/// Eigener Ordner je Test, wird am Ende wieder gelöscht.
struct TempDir(std::path::PathBuf);

impl TempDir {
    fn with_font(name: &str, bytes: &[u8]) -> Self {
        let path = std::env::temp_dir().join(format!(
            "planora-text-shape-{}-{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join(format!("{name}.ttf")), bytes).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Index des Tabelleneintrags `tag` im Inhaltsverzeichnis einer Schrift.
fn table_entry(font: &[u8], tag: &[u8; 4]) -> usize {
    let num_tables = u16::from_be_bytes([font[4], font[5]]) as usize;
    (0..num_tables)
        .map(|t| 12 + 16 * t)
        .find(|&at| &font[at..at + 4] == tag)
        .expect("Tabelle fehlt in der Testschrift")
}

/// Ersetzt jedes Vorkommen eines Namens (ASCII und UTF-16BE) durch einen gleich langen.
fn rename_family(font: &[u8], from: &str, to: &str) -> Vec<u8> {
    assert_eq!(from.len(), to.len());
    let mut out = font.to_vec();
    let utf16 = |s: &str| -> Vec<u8> { s.encode_utf16().flat_map(u16::to_be_bytes).collect() };
    for (a, b) in [
        (from.as_bytes().to_vec(), to.as_bytes().to_vec()),
        (utf16(from), utf16(to)),
    ] {
        let mut i = 0;
        while i + a.len() <= out.len() {
            if out[i..i + a.len()] == a[..] {
                out[i..i + a.len()].copy_from_slice(&b);
                i += a.len();
            } else {
                i += 1;
            }
        }
    }
    out
}

#[test]
fn font_without_latin_cmap_falls_back_to_roboto() {
    // Montserrat unter anderem Namen und ohne `cmap`: Kein Zeichen hat eine Glyphe.
    let mut bytes = rename_family(MONTSERRAT, "Montserrat", "Nocmapfont");
    let at = table_entry(&bytes, b"cmap");
    bytes[at..at + 4].copy_from_slice(b"xxxx");
    let dir = TempDir::with_font("nocmap", &bytes);
    let lib = FontLibrary::scan(std::slice::from_ref(&dir.0));
    assert!(lib.has_family("Nocmapfont"), "{:?}", lib.skipped());

    let shape = lib
        .shape("Hallo", &style("Nocmapfont", false, false, 0.0))
        .unwrap();
    assert!(shape.substituted.is_none());
    assert!(!shape.missing_chars);
    // Alle Zeichen kommen aus Roboto.
    let robo = roboto("Hallo", false, false);
    assert_eq!(shape.lines[0].glyphs.len(), robo.lines[0].glyphs.len());
    for (a, b) in shape.lines[0].glyphs.iter().zip(&robo.lines[0].glyphs) {
        assert_eq!(a.contours, b.contours);
        assert!((a.advance - b.advance).abs() < 1e-12);
    }
    // Die Kennzahlen bleiben die der gewählten Schrift.
    assert_eq!(shape.line_height, montserrat("Hallo").line_height);
}

#[test]
fn font_with_cut_glyph_table_does_not_crash() {
    // Roboto unter anderem Namen, die Tabelle `glyf` auf 64 Bytes gekürzt: Die meisten Glyphen
    // lassen sich nicht zeichnen. Erwartet: kein Absturz, Platz leer mit Hinweis.
    let mut bytes = rename_family(ROBOTO, "Roboto", "Kaputt");
    let at = table_entry(&bytes, b"glyf");
    bytes[at + 12..at + 16].copy_from_slice(&64u32.to_be_bytes());
    let dir = TempDir::with_font("cutglyf", &bytes);
    let lib = FontLibrary::scan(std::slice::from_ref(&dir.0));
    // Die Datei besteht das Durchsuchen (Name und `glyf` sind da); erst das Zeichnen scheitert.
    assert!(lib.has_family("Kaputt"), "{:?}", lib.skipped());
    let shape = lib
        .shape("Hallo Welt", &style("Kaputt", true, true, 10.0))
        .unwrap();
    // Gesetzt mit der kaputten Schrift selbst, nicht mit der Ersatzschrift.
    assert!(shape.substituted.is_none());
    assert!(shape.missing_chars);
    let glyphs: Vec<&Glyph> = shape.lines.iter().flat_map(|l| &l.glyphs).collect();
    assert_eq!(glyphs.len(), "Hallo Welt".chars().count());
    for glyph in glyphs {
        assert!(glyph.origin.is_finite() && glyph.advance.is_finite());
    }
}
