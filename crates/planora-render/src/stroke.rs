//! Skizzenlinien und -flächen mit Tiefenprüfung (Skizzen 12): reine Geometrie ohne GPU.
//!
//! Die Linien werden auf der CPU in Dreiecke in Bildschirmbreite zerlegt, damit die Breite
//! in Pixeln stimmt und die Strichelung über die ganze Polylinie läuft. Die Eckpunkte liegen
//! schon in Bildschirmkoordinaten (NDC); der Shader reicht sie nur durch.

use bytemuck::{Pod, Zeroable};
use glam::{DVec3, Mat4, Vec2, Vec3, Vec4};

/// Tiefenversatz als Bruchteil des Abstands zur Kamera: Die Linie wird in Blickrichtung um
/// `depth_bias * w` zur Kamera hin geschoben (siehe [`StrokeView::depth_bias`]). 2e-3 = 0,2 %.
///
/// Ein fester Versatz in NDC-Tiefe wüchse mit dem Quadrat des Abstands (bei 3 facher Entfernung
/// 9 fach so tief in der Welt) und ließe weit entfernte Linien durch dünne Wände scheinen; ein
/// Versatz in Abstandsbruchteilen wächst nur linear. Bei `near = 0,01 * Abstand` sind das im
/// Abstand der Szene rund 2e-5 NDC (weit über der f32-Genauigkeit 6e-8), im Dreifachen
/// Abstand 0,6 % des Szenenabstands. Gegen die Fläche zusätzlich wirkt deren Hardware-Bias
/// (`slope_scale` 2,0 in `gpu.rs`): bei streifendem Blick trägt er die Hauptlast, siehe
/// den Test `stroke_bias_covers_half_width_at_grazing_angles`.
pub const STROKE_DEPTH_BIAS: f32 = 2e-3;

/// `slope_scale` des Tiefenversatzes der Flächen-Pipeline in `gpu.rs`: Flächen rutschen bei
/// schrägem Blick um das Doppelte ihres Tiefengefälles je Pixel nach hinten. Die Linien
/// verlassen sich bei streifendem Blick darauf (siehe Test).
pub const FACE_SLOPE_SCALE: f32 = 2.0;

/// Kleinster erlaubter `w`-Wert; davor wird abgeschnitten.
const W_EPSILON: f32 = 1e-4;

/// Kleinste Musterlänge (Strich + Lücke) in Pixeln; kürzer wird durchgezogen gezeichnet.
const MIN_DASH_PERIOD_PX: f32 = 0.5;

/// Ein Eckpunkt der Skizzendreiecke. `ndc` ist fertig projiziert (x, y in −1…1, z Tiefe 0…1),
/// `edge` läuft quer zur Linie von −1 bis 1 (Mitte 0, Flächen 0) für die Kantenglättung.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct StrokeVertex {
    pub ndc: [f32; 3],
    pub color: [f32; 4],
    pub edge: f32,
}

/// Ansicht, in die projiziert wird.
#[derive(Clone, Copy, Debug)]
pub struct StrokeView {
    pub view_proj: Mat4,
    /// Größe des Viewports in physischen Pixeln.
    pub size_px: Vec2,
    /// Bruchteil des Abstands, um den die Linien zur Kamera hin geschoben werden
    /// (siehe [`STROKE_DEPTH_BIAS`]); 0 = kein Versatz.
    pub depth_bias: f32,
}

impl StrokeView {
    fn clip(&self, p: DVec3) -> Vec4 {
        self.view_proj * Vec3::new(p.x as f32, p.y as f32, p.z as f32).extend(1.0)
    }

    /// Bildschirmpunkt in Pixeln (Mitte = Ursprung) und Tiefe.
    fn screen(&self, c: Vec4) -> (Vec2, f32) {
        let ndc = c.truncate() / c.w;
        (
            Vec2::new(ndc.x * self.size_px.x * 0.5, ndc.y * self.size_px.y * 0.5),
            self.biased_depth(ndc.z),
        )
    }

    /// Tiefe nach Verschiebung um `depth_bias * w` zur Kamera. Mit `d = -A + B / w`
    /// (`A ≈ -1` bei `far >> near`) ist die neue Tiefe `d - k / (1 - k) * (1 - d)`.
    fn biased_depth(&self, d: f32) -> f32 {
        let k = self.depth_bias;
        if k == 0.0 {
            return d;
        }
        d - k / (1.0 - k) * (1.0 - d).max(0.0)
    }

    fn vertex(&self, px: Vec2, z: f32, color: [f32; 4], edge: f32) -> StrokeVertex {
        StrokeVertex {
            ndc: [px.x * 2.0 / self.size_px.x, px.y * 2.0 / self.size_px.y, z],
            color,
            edge,
        }
    }
}

/// Abstand von den Beschneidungsebenen: `w ≥ ε` und Nahebene (`z ≥ 0`). Negativ = außerhalb.
fn plane_distances(c: Vec4) -> [f32; 2] {
    [c.w - W_EPSILON, c.z]
}

/// Schneidet eine Strecke im Clip-Raum an `w = ε` und an der Nahebene ab.
fn clip_segment(mut a: Vec4, mut b: Vec4) -> Option<(Vec4, Vec4)> {
    for plane in 0..2 {
        let da = plane_distances(a)[plane];
        let db = plane_distances(b)[plane];
        if da < 0.0 && db < 0.0 {
            return None;
        }
        if da < 0.0 {
            a += (b - a) * (da / (da - db));
        } else if db < 0.0 {
            b = a + (b - a) * (da / (da - db));
        }
    }
    Some((a, b))
}

/// Eine Strecke im Bild: Anfang und Ende in Pixeln samt Tiefe.
#[derive(Clone, Copy)]
struct Seg {
    p0: Vec2,
    z0: f32,
    p1: Vec2,
    z1: f32,
}

fn emit_quad(
    out: &mut Vec<StrokeVertex>,
    view: &StrokeView,
    a: (Vec2, f32),
    b: (Vec2, f32),
    half: f32,
    color: [f32; 4],
) {
    let dir = (b.0 - a.0).normalize_or_zero();
    if dir == Vec2::ZERO {
        return;
    }
    let n = Vec2::new(-dir.y, dir.x) * half;
    let v = |p: Vec2, z: f32, e: f32| view.vertex(p, z, color, e);
    let (a_hi, a_lo) = (v(a.0 + n, a.1, 1.0), v(a.0 - n, a.1, -1.0));
    let (b_hi, b_lo) = (v(b.0 + n, b.1, 1.0), v(b.0 - n, b.1, -1.0));
    out.extend_from_slice(&[a_hi, a_lo, b_hi, b_hi, a_lo, b_lo]);
}

/// Dreieck auf der Außenseite einer Ecke, damit zwischen zwei Vierecken keine Lücke bleibt.
fn emit_join(
    out: &mut Vec<StrokeVertex>,
    view: &StrokeView,
    prev: &Seg,
    next: &Seg,
    half: f32,
    color: [f32; 4],
) {
    let d0 = (prev.p1 - prev.p0).normalize_or_zero();
    let d1 = (next.p1 - next.p0).normalize_or_zero();
    let cross = d0.x * d1.y - d0.y * d1.x;
    if d0 == Vec2::ZERO || d1 == Vec2::ZERO || (cross.abs() < 1e-6 && d0.dot(d1) > 0.0) {
        return;
    }
    // Linkskurve: außen liegt die rechte Seite (−n), sonst die linke.
    let sign = if cross > 0.0 { -1.0 } else { 1.0 };
    let n0 = Vec2::new(-d0.y, d0.x) * half * sign;
    let n1 = Vec2::new(-d1.y, d1.x) * half * sign;
    let at = prev.p1;
    let z = prev.z1;
    out.extend_from_slice(&[
        view.vertex(at, z, color, 0.0),
        view.vertex(at + n0, z, color, sign),
        view.vertex(at + n1, z, color, sign),
    ]);
}

/// Teil der Strecke (als Anteile 0…1), der in einem Rechteck um die Bildmitte liegt
/// (Liang-Barsky). `None`, wenn nichts davon im Rechteck liegt.
fn visible_range(seg: &Seg, half_extent: Vec2) -> Option<(f64, f64)> {
    let p0 = seg.p0.as_dvec2();
    let d = seg.p1.as_dvec2() - p0;
    let h = half_extent.as_dvec2();
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    for (p, q) in [
        (-d.x, p0.x + h.x),
        (d.x, h.x - p0.x),
        (-d.y, p0.y + h.y),
        (d.y, h.y - p0.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
        }
    }
    (t0 < t1).then_some((t0, t1))
}

/// Läuft das Strichmuster ab `phase` über eine Strecke und gibt die Striche aus.
/// Gibt die Phase am Ende der Strecke zurück. Gezeichnet wird nur, was im Bild (mit Rand)
/// liegt; die Phase läuft über den abgeschnittenen Teil trotzdem weiter. Die Striche werden aus
/// ihrer laufenden Nummer berechnet (f64), nicht durch Aufsummieren, damit es immer endet.
fn emit_dashed(
    out: &mut Vec<StrokeVertex>,
    view: &StrokeView,
    seg: &Seg,
    pattern: [f32; 2],
    phase: f32,
    half: f32,
    color: [f32; 4],
) -> f32 {
    let dash = f64::from(pattern[0]);
    let period = dash + f64::from(pattern[1]);
    let len = f64::from((seg.p1 - seg.p0).length());
    if len <= 0.0 {
        return phase;
    }
    let phase0 = f64::from(phase);
    let next_phase = ((phase0 + len) % period) as f32;
    let guard = view.size_px.max_element() + half * 2.0;
    let Some((t0, t1)) = visible_range(seg, view.size_px * 0.5 + Vec2::splat(guard)) else {
        return next_phase;
    };
    let (s0, s1) = (t0 * len, t1 * len);
    let at = |s: f64| {
        let t = (s / len) as f32;
        (seg.p0.lerp(seg.p1, t), seg.z0 + (seg.z1 - seg.z0) * t)
    };
    let mut k = ((s0 + phase0) / period).floor().max(0.0);
    loop {
        let start = k * period - phase0;
        if start >= s1 {
            break;
        }
        let (a, b) = (start.max(s0), (start + dash).min(s1));
        if b > a {
            emit_quad(out, view, at(a), at(b), half, color);
        }
        k += 1.0;
    }
    next_phase
}

/// Polylinie als Dreiecke in Bildschirmbreite. `width_px` in physischen Pixeln; `dash_px` =
/// [Strich, Lücke] in Pixeln, das Muster läuft über die Stützstellen weiter. Teile hinter
/// der Kamera werden abgeschnitten.
pub fn push_stroke(
    out: &mut Vec<StrokeVertex>,
    view: &StrokeView,
    path: &[DVec3],
    width_px: f32,
    color: [f32; 4],
    dash_px: Option<[f32; 2]>,
) {
    let half = width_px * 0.5;
    let pattern = dash_px.filter(|[d, g]| {
        *d > 0.0 && *g >= 0.0 && d.is_finite() && g.is_finite() && d + g >= MIN_DASH_PERIOD_PX
    });
    let mut phase = 0.0_f32;
    let mut prev: Option<Seg> = None;
    for pair in path.windows(2) {
        let (a, b) = (view.clip(pair[0]), view.clip(pair[1]));
        let Some((ca, cb)) = clip_segment(a, b) else {
            prev = None;
            continue;
        };
        let (p0, z0) = view.screen(ca);
        let (p1, z1) = view.screen(cb);
        let seg = Seg { p0, z0, p1, z1 };
        if !(p0.is_finite() && p1.is_finite() && z0.is_finite() && z1.is_finite()) {
            prev = None;
            continue;
        }
        // Abgeschnittene Anfänge schließen nicht an das Vorherige an.
        let connected = prev.is_some() && ca == a;
        // Strecken ohne Länge ändern weder Phase noch Anschluss.
        if (p1 - p0).length() <= 1e-6 {
            continue;
        }
        match pattern {
            Some(pat) => {
                if connected
                    && let Some(p) = &prev
                    && phase > 0.0
                    && phase < pat[0]
                {
                    emit_join(out, view, p, &seg, half, color);
                }
                phase = emit_dashed(out, view, &seg, pat, phase, half, color);
            }
            _ => {
                if connected && let Some(p) = &prev {
                    emit_join(out, view, p, &seg, half, color);
                }
                emit_quad(out, view, (p0, z0), (p1, z1), half, color);
            }
        }
        // Wurde am Ende abgeschnitten, hängt das nächste Stück nicht mehr an.
        prev = if cb == b { Some(seg) } else { None };
    }
}

/// Dreiecke (Profilflächen) projiziert; Teile hinter der Kamera werden abgeschnitten.
pub fn push_fill(
    out: &mut Vec<StrokeVertex>,
    view: &StrokeView,
    triangles: &[[DVec3; 3]],
    color: [f32; 4],
) {
    for tri in triangles {
        let mut poly: Vec<Vec4> = tri.iter().map(|p| view.clip(*p)).collect();
        for plane in 0..2 {
            poly = clip_polygon(&poly, plane);
            if poly.len() < 3 {
                break;
            }
        }
        if poly.len() < 3 {
            continue;
        }
        let pts: Vec<(Vec2, f32)> = poly.iter().map(|c| view.screen(*c)).collect();
        if pts.iter().any(|(p, z)| !(p.is_finite() && z.is_finite())) {
            continue;
        }
        for i in 1..pts.len() - 1 {
            for (p, z) in [pts[0], pts[i], pts[i + 1]] {
                out.push(view.vertex(p, z, color, 0.0));
            }
        }
    }
}

/// Sutherland-Hodgman gegen eine der Ebenen aus `plane_distances`.
fn clip_polygon(poly: &[Vec4], plane: usize) -> Vec<Vec4> {
    let mut out = Vec::with_capacity(poly.len() + 1);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (da, db) = (plane_distances(a)[plane], plane_distances(b)[plane]);
        if da >= 0.0 {
            out.push(a);
        }
        if (da >= 0.0) != (db >= 0.0) {
            out.push(a + (b - a) * (da / (da - db)));
        }
    }
    out
}
