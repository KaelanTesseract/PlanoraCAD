//! Umrisse einer Glyphe aus `skrifa` als geschlossene Konturen in Einheiten der Höhe.

use planora_math::DVec2;
use planora_sketch::text_shape::{Contour, Seg};
use skrifa::GlyphId;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlineGlyphCollection, OutlinePen, pen::PathStyle};

/// Grenze für „Länge 0“ und „auf einer Geraden“, in Einheiten der Höhe (Spec A5: `1e-9` × Höhe).
pub(crate) const EPS: f64 = 1e-9;

/// Zeichnet eine Glyphe unskaliert und teilt durch units per em (`scale = 1 / upem`).
/// `Err`, wenn `skrifa` die Glyphe nicht lesen oder zeichnen kann (kaputte Daten); eine Glyphe
/// ohne Umriss (Leerzeichen) ergibt eine leere Liste.
pub(crate) fn glyph_contours(
    outlines: &OutlineGlyphCollection<'_>,
    location: LocationRef<'_>,
    gid: u32,
    scale: f64,
) -> Result<Vec<Contour>, ()> {
    // `None` heißt: Die Daten der Glyphe lassen sich nicht lesen. Eine leere Glyphe kommt
    // dagegen als Glyphe ohne Konturen.
    let glyph = outlines.get(GlyphId::new(gid)).ok_or(())?;
    let mut pen = ContourPen::new(scale);
    // HarfBuzz-Art: Die versteckten Punkte auf der Kurve (TrueType) liegen genau in der Mitte
    // der beiden Kontrollpunkte. Die FreeType-Art rundet sie auf 1/64 Einheit.
    let settings =
        DrawSettings::unhinted(Size::unscaled(), location).with_path_style(PathStyle::HarfBuzz);
    glyph.draw(settings, &mut pen).map_err(|_| ())?;
    Ok(pen.finish())
}

/// Stift für `skrifa`: sammelt Stücke, schließt jede Kontur ausdrücklich und räumt auf
/// (Länge 0 entfällt, gerade Bézierkurven werden Strecken).
pub(crate) struct ContourPen {
    scale: f64,
    contours: Vec<Contour>,
    open: Option<(DVec2, Vec<Seg>)>,
}

impl ContourPen {
    pub(crate) fn new(scale: f64) -> Self {
        Self {
            scale,
            contours: Vec::new(),
            open: None,
        }
    }

    /// Schließt eine offene Kontur und liefert alle Konturen.
    pub(crate) fn finish(mut self) -> Vec<Contour> {
        self.close_open();
        self.contours
    }

    fn point(&self, x: f32, y: f32) -> DVec2 {
        DVec2::new(f64::from(x), f64::from(y)) * self.scale
    }

    fn push(&mut self, seg: Seg) {
        match &mut self.open {
            Some((_, segs)) => segs.push(seg),
            // Ein Stück ohne vorheriges `move_to`: Es beginnt dort, wo es endet.
            None => self.open = Some((seg.end(), Vec::new())),
        }
    }

    fn close_open(&mut self) {
        if let Some((start, segs)) = self.open.take()
            && let Some(contour) = tidy(start, segs)
        {
            self.contours.push(contour);
        }
    }
}

impl OutlinePen for ContourPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close_open();
        self.open = Some((self.point(x, y), Vec::new()));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.point(x, y);
        self.push(Seg::Line(p));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let seg = Seg::Quad(self.point(cx0, cy0), self.point(x, y));
        self.push(seg);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let seg = Seg::Cubic(self.point(cx0, cy0), self.point(cx1, cy1), self.point(x, y));
        self.push(seg);
    }

    fn close(&mut self) {
        self.close_open();
    }
}

/// Räumt eine Kontur auf und schließt sie genau:
/// - Stücke der Länge 0 (alle Punkte am Anfang) entfallen.
/// - Eine Bézierkurve, deren Kontrollpunkte höchstens [`EPS`] neben der Sehne und zwischen
///   deren Enden liegen, wird zur Strecke (sie läuft dann genau die Strecke entlang).
/// - Endet die Kontur nicht am Anfang, kommt eine Strecke dorthin dazu; liegt das Ende nur
///   knapp daneben, wird es auf den Anfang gelegt.
/// - Was danach keine Fläche umschließen kann (ein Stück, zwei Strecken), entfällt.
pub(crate) fn tidy(start: DVec2, segs: Vec<Seg>) -> Option<Contour> {
    let mut out: Vec<Seg> = Vec::with_capacity(segs.len() + 1);
    let mut current = start;
    for seg in segs {
        let end = seg.end();
        let kept = match seg {
            Seg::Line(p) => (current.distance(p) > EPS).then_some(seg),
            Seg::Quad(c, p) => curve_or_line(current, &[c], p, seg),
            Seg::Cubic(c1, c2, p) => curve_or_line(current, &[c1, c2], p, seg),
        };
        if let Some(seg) = kept {
            out.push(seg);
            current = end;
        }
    }
    if current.distance(start) > EPS {
        out.push(Seg::Line(start));
    } else if let Some(last) = out.last_mut() {
        set_end(last, start);
    }
    let degenerate =
        out.len() < 2 || (out.len() == 2 && out.iter().all(|seg| matches!(seg, Seg::Line(_))));
    (!degenerate).then_some(Contour { start, segs: out })
}

/// Die Bézierkurve von `from` über `controls` nach `end`: entfällt bei Länge 0, wird zur
/// Strecke, wenn sie gerade ist, sonst bleibt sie.
fn curve_or_line(from: DVec2, controls: &[DVec2], end: DVec2, seg: Seg) -> Option<Seg> {
    let all_at_start =
        from.distance(end) <= EPS && controls.iter().all(|c| from.distance(*c) <= EPS);
    if all_at_start {
        return None;
    }
    if is_straight(from, controls, end) {
        return Some(Seg::Line(end));
    }
    Some(seg)
}

/// Liegen alle Kontrollpunkte höchstens [`EPS`] neben der Sehne und zwischen ihren Enden?
/// Dann läuft die Kurve (Bernstein-Form mit Kontrollpunkten auf der Sehne) gleichsinnig die
/// Strecke entlang und ist von ihr nicht zu unterscheiden.
fn is_straight(from: DVec2, controls: &[DVec2], end: DVec2) -> bool {
    let chord = end - from;
    let len = chord.length();
    if len <= EPS {
        return false;
    }
    controls.iter().all(|c| {
        let rel = *c - from;
        let off = chord.perp_dot(rel).abs() / len;
        let t = chord.dot(rel) / (len * len);
        off <= EPS && (0.0..=1.0).contains(&t)
    })
}

fn set_end(seg: &mut Seg, p: DVec2) {
    match seg {
        Seg::Line(e) | Seg::Quad(_, e) | Seg::Cubic(_, _, e) => *e = p,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draw(f: impl FnOnce(&mut ContourPen)) -> Vec<Contour> {
        let mut pen = ContourPen::new(1.0);
        f(&mut pen);
        pen.finish()
    }

    fn v(x: f64, y: f64) -> DVec2 {
        DVec2::new(x, y)
    }

    #[test]
    fn collinear_quad_becomes_line() {
        let contours = draw(|pen| {
            pen.move_to(0.0, 0.0);
            // Kontrollpunkt auf der Sehne.
            pen.quad_to(5.0, 0.0, 10.0, 0.0);
            // Kubisch mit beiden Kontrollpunkten auf der Sehne.
            pen.curve_to(10.0, 2.0, 10.0, 8.0, 10.0, 10.0);
            // Eine echte Kurve bleibt.
            pen.quad_to(5.0, 15.0, 0.0, 10.0);
            pen.close();
        });
        assert_eq!(contours.len(), 1);
        assert_eq!(
            contours[0].segs,
            vec![
                Seg::Line(v(10.0, 0.0)),
                Seg::Line(v(10.0, 10.0)),
                Seg::Quad(v(5.0, 15.0), v(0.0, 10.0)),
                Seg::Line(v(0.0, 0.0)),
            ]
        );
    }

    #[test]
    fn collinear_quad_needs_control_between_ends_and_tolerance() {
        // Der Kontrollpunkt liegt auf der Geraden, aber hinter dem Ende: Die Kurve läuft
        // über das Ende hinaus und zurück, also keine Strecke.
        let tidy_quad = |c: DVec2| {
            tidy(
                v(0.0, 0.0),
                vec![Seg::Quad(c, v(1.0, 0.0)), Seg::Line(v(0.0, 1.0))],
            )
        };
        let beyond = tidy_quad(v(2.0, 0.0)).unwrap();
        assert!(matches!(beyond.segs[0], Seg::Quad(..)));
        // Abweichung unter der Grenze: Strecke; darüber: Kurve.
        let inside = tidy_quad(v(0.5, 0.5 * EPS)).unwrap();
        assert!(matches!(inside.segs[0], Seg::Line(_)));
        let outside = tidy_quad(v(0.5, 4.0 * EPS)).unwrap();
        assert!(matches!(outside.segs[0], Seg::Quad(..)));
    }

    #[test]
    fn zero_length_segment_dropped() {
        let contours = draw(|pen| {
            pen.move_to(0.0, 0.0);
            pen.line_to(0.0, 0.0);
            pen.line_to(10.0, 0.0);
            pen.quad_to(10.0, 0.0, 10.0, 0.0);
            pen.curve_to(10.0, 0.0, 10.0, 0.0, 10.0, 0.0);
            pen.line_to(10.0, 10.0);
            pen.line_to(0.0, 10.0);
            pen.line_to(0.0, 0.0);
            pen.close();
        });
        assert_eq!(contours.len(), 1);
        assert_eq!(
            contours[0].segs,
            vec![
                Seg::Line(v(10.0, 0.0)),
                Seg::Line(v(10.0, 10.0)),
                Seg::Line(v(0.0, 10.0)),
                Seg::Line(v(0.0, 0.0)),
            ]
        );
    }

    #[test]
    fn open_end_is_closed_exactly() {
        // Ohne Strecke zurück: Sie kommt dazu.
        let contours = draw(|pen| {
            pen.move_to(0.0, 0.0);
            pen.line_to(10.0, 0.0);
            pen.line_to(10.0, 10.0);
            pen.close();
        });
        assert_eq!(contours[0].segs.len(), 3);
        assert_eq!(contours[0].segs[2], Seg::Line(v(0.0, 0.0)));
        // Ende knapp neben dem Anfang: Es wird auf den Anfang gelegt, ohne neues Stück.
        let near = tidy(
            v(0.0, 0.0),
            vec![
                Seg::Line(v(10.0, 0.0)),
                Seg::Quad(v(10.0, 10.0), v(0.5 * EPS, 0.0)),
            ],
        )
        .unwrap();
        assert_eq!(near.segs.len(), 2);
        assert_eq!(near.segs[1].end(), near.start);
    }

    #[test]
    fn degenerate_contours_vanish() {
        let contours = draw(|pen| {
            // Nur ein Punkt.
            pen.move_to(1.0, 1.0);
            pen.close();
            // Hin und zurück auf einer Strecke.
            pen.move_to(0.0, 0.0);
            pen.line_to(5.0, 0.0);
            pen.close();
            // Ein Dreieck bleibt.
            pen.move_to(0.0, 0.0);
            pen.line_to(5.0, 0.0);
            pen.line_to(0.0, 5.0);
            pen.close();
        });
        assert_eq!(contours.len(), 1);
        assert_eq!(contours[0].segs.len(), 3);
    }
}
