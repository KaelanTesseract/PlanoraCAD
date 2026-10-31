//! Adaptives Bodenraster der 3D-Hauptansicht und die Koordinatenachsen.
//!
//! Das Raster liegt auf der Ebene z = 0, folgt dem Zielpunkt der Kamera und blendet nach allen
//! Seiten mit dem Abstand zum Ziel weich aus (wie ein Horizont). Der Linienabstand springt in
//! Zehnerschritten (…, 1 mm, 10 mm, 100 mm, 1 m, …) mit dem Kameraabstand; beim Wechsel blendet
//! die feinere Stufe aus, während die gröbere einblendet. Jede fünfte Linie ist kräftiger, ebenso
//! jede Linie der gröberen Stufe. Die Achsen bleiben am Ursprung und werden nicht ausgeblendet.

use crate::scene::{LineVertex, Palette};
use glam::Vec3;

/// Angenommene Breite des sichtbaren Bereichs am Ziel im Verhältnis zum Kameraabstand
/// (Sehfeld 45°, Seitenverhältnis um 1,4).
const VIEW_SPAN: f32 = 1.2;
/// Linien über die Ansichtsbreite: bis hierher bleibt die feine Stufe voll sichtbar.
const FULL_LINES: f32 = 40.0;
/// Bei dieser Zahl Linien über die Ansichtsbreite ist die feine Stufe ganz ausgeblendet
/// (und die nächstgröbere hat dann zehnmal weniger, also zehn Linien).
const GONE_LINES: f32 = 100.0;
/// Radius, an dem das Raster ganz ausgeblendet ist, im Verhältnis zum Kameraabstand.
const RIM_FACTOR: f32 = 1.5;
/// Teilstücke je Halblinie bei voller Länge; die Deckkraft ändert sich nur an den Eckpunkten.
const SEGMENTS_PER_HALF: f32 = 3.0;
/// Deckkraft einer gewöhnlichen Linie im Verhältnis zur Hauptlinie.
const MINOR_STRENGTH: f32 = 0.55;
/// Jede wievielte Linie eine Hauptlinie ist.
const MAJOR_EVERY: i64 = 5;
/// Gröbere Stufe: jede so vielte Linie der feinen Stufe (Zehnerschritt).
const DECADE: i64 = 10;
/// Linien, die an keiner Stelle stärker als dies erscheinen, werden weggelassen.
const MIN_ALPHA: f32 = 0.01;

/// Linienabstand für den Kameraabstand `distance`: `(fein, übergang)`. `fein` ist eine
/// Zehnerpotenz (in mm), bei der 10 bis 100 Linien über die Ansichtsbreite passen. `übergang`
/// (0..1) ist der Anteil der nächstgröberen Stufe (zehnfacher Abstand): 0, solange die feine
/// Stufe höchstens 40 Linien zeigt, 1 bei 100 Linien (dann ist die feine Stufe unsichtbar).
/// `unit_mm` ist die Länge der Anzeigeeinheit in mm (1 für mm, 25,4 für Zoll); die Stufen sind
/// Zehnerpotenzen dieser Einheit, das Ergebnis bleibt in mm. Ungültig (≤ 0, nicht endlich) → 1.
pub fn grid_step(distance: f32, unit_mm: f32) -> (f32, f32) {
    let unit = if unit_mm.is_finite() && unit_mm > 0.0 {
        unit_mm
    } else {
        1.0
    };
    let span = if distance.is_finite() && distance > 0.0 {
        distance * VIEW_SPAN
    } else {
        VIEW_SPAN
    } / unit;
    let mut exp = (span / 10.0).log10().floor() as i32;
    let step = |e: i32| 10.0_f32.powi(e);
    // Rundungsfehler an den Dekadengrenzen ausgleichen.
    while span / step(exp) >= GONE_LINES {
        exp += 1;
    }
    while span / step(exp) < GONE_LINES / 10.0 {
        exp -= 1;
    }
    let lines = span / step(exp);
    let t = ((lines / FULL_LINES).ln() / (GONE_LINES / FULL_LINES).ln()).clamp(0.0, 1.0);
    (step(exp) * unit, t * t * (3.0 - 2.0 * t))
}

/// Radius, an dem das Raster bei Kameraabstand `distance` ausgeblendet ist.
pub fn grid_radius(distance: f32) -> f32 {
    distance * RIM_FACTOR
}

/// Deckkraft 1 → 0 von der Mitte (`r = 0`) bis zum Rand (`r = radius`), weich.
fn rim_fade(r: f32, radius: f32) -> f32 {
    let s = (1.0 - r / radius).clamp(0.0, 1.0);
    s * s * (3.0 - 2.0 * s)
}

/// Stärke der Linie Nr. `k` ihrer Stufe: jede fünfte kräftig, die übrigen gedämpft.
fn strength(k: i64) -> (bool, f32) {
    if k.rem_euclid(MAJOR_EVERY) == 0 {
        (true, 1.0)
    } else {
        (false, MINOR_STRENGTH)
    }
}

/// Farbe und Grunddeckkraft der Linie Nr. `k` (Position `k · fein`) beim Übergang `blend`:
/// Beitrag der feinen Stufe (`1 − blend`) und, auf jeder zehnten Linie, der gröberen (`blend`).
fn line_style(k: i64, blend: f32, palette: &Palette) -> ([f32; 3], f32) {
    let part = |index: i64, weight: f32| {
        let (major, s) = strength(index);
        let c = if major {
            palette.grid_major
        } else {
            palette.grid
        };
        ([c[0], c[1], c[2]], weight * s * c[3])
    };
    let (c1, a1) = part(k, 1.0 - blend);
    let (c2, a2) = if k.rem_euclid(DECADE) == 0 {
        part(k / DECADE, blend)
    } else {
        ([0.0; 3], 0.0)
    };
    let sum = a1 + a2;
    if sum <= 0.0 {
        return ([0.0; 3], 0.0);
    }
    let rgb = [0, 1, 2].map(|i| (c1[i] * a1 + c2[i] * a2) / sum);
    (rgb, a1.max(a2))
}

/// Bodenraster (ausgeblendet) und Achsen (nicht ausgeblendet) für eine Kamera mit Ziel `target`
/// im Abstand `distance`, als Linienliste: je zwei aufeinanderfolgende Eckpunkte bilden eine
/// Linie; die Deckkraft steckt im Alphawert der Farbe. Rasterlinien liegen auf Vielfachen des
/// Linienabstands und bleiben beim Verschieben der Ansicht an ihrem Platz. Die Achsen sind so
/// lang, dass sie das ganze Raster durchziehen (die z-Achse zeigt nach oben). `unit_mm` wie bei
/// [`grid_step`].
pub fn grid_lines(target: Vec3, distance: f32, unit_mm: f32, palette: &Palette) -> Vec<LineVertex> {
    if !target.is_finite() || !distance.is_finite() || distance <= 0.0 {
        return Vec::new();
    }
    let (fine, blend) = grid_step(distance, unit_mm);
    let radius = grid_radius(distance);
    let mut out = Vec::new();

    for along_x in [true, false] {
        // Linien parallel zur x-Achse liegen bei konstantem y (und umgekehrt).
        let (across, along) = if along_x {
            (target.y, target.x)
        } else {
            (target.x, target.y)
        };
        let first = ((across - radius) / fine).ceil() as i64;
        let last = ((across + radius) / fine).floor() as i64;
        for k in first..=last {
            if k == 0 {
                continue; // dort liegt die Achse
            }
            let pos = k as f32 * fine;
            let offset = pos - across;
            if offset.abs() >= radius {
                continue;
            }
            let (rgb, alpha) = line_style(k, blend, palette);
            if alpha * rim_fade(offset.abs(), radius) < MIN_ALPHA {
                continue;
            }
            let half = (radius * radius - offset * offset).sqrt();
            let n = ((SEGMENTS_PER_HALF * half / radius).ceil() as i64).max(1);
            let point = |j: i64| {
                let s = half * j as f32 / n as f32;
                let r = (offset * offset + s * s).sqrt();
                let a = alpha * rim_fade(r, radius);
                let (x, y) = if along_x {
                    (along + s, pos)
                } else {
                    (pos, along + s)
                };
                LineVertex {
                    position: [x, y, 0.0],
                    color: [rgb[0], rgb[1], rgb[2], a],
                }
            };
            for j in -n..n {
                out.push(point(j));
                out.push(point(j + 1));
            }
        }
    }

    // Achsen: durch den Ursprung, lang genug für das ganze sichtbare Raster.
    let l = target.length() + radius;
    let axis = |a: [f32; 3], b: [f32; 3], color: [f32; 4]| {
        [
            LineVertex { position: a, color },
            LineVertex { position: b, color },
        ]
    };
    out.extend(axis([-l, 0.0, 0.0], [l, 0.0, 0.0], palette.axis_x));
    out.extend(axis([0.0, -l, 0.0], [0.0, l, 0.0], palette.axis_y));
    out.extend(axis([0.0, 0.0, 0.0], [0.0, 0.0, l], palette.axis_z));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn palettes() -> [Palette; 2] {
        [Palette::dark(), Palette::light()]
    }

    fn is_axis(v: &LineVertex, p: &Palette) -> bool {
        [p.axis_x, p.axis_y, p.axis_z].contains(&v.color)
    }

    /// Größte Deckkraft je Rasterlinie (Richtung, Position in µm).
    fn line_alphas(lines: &[LineVertex], p: &Palette) -> HashMap<(bool, i64), f32> {
        let mut map = HashMap::new();
        for pair in lines.chunks(2).filter(|c| !is_axis(&c[0], p)) {
            let (a, b) = (pair[0].position, pair[1].position);
            let key = if (a[1] - b[1]).abs() < 1e-6 {
                (true, (a[1] * 1000.0).round() as i64)
            } else {
                (false, (a[0] * 1000.0).round() as i64)
            };
            let e = map.entry(key).or_insert(0.0_f32);
            *e = e.max(pair[0].color[3]).max(pair[1].color[3]);
        }
        map
    }

    #[test]
    fn step_is_a_decade_and_never_shrinks_with_distance() {
        let mut last = 0.0;
        let mut d = 0.5_f32;
        while d < 1.0e6 {
            let (fine, blend) = grid_step(d, 1.0);
            let exp = fine.log10();
            assert!((exp - exp.round()).abs() < 1e-4, "{d}: {fine}");
            assert!(fine >= last, "{d}: {fine} < {last}");
            assert!((0.0..=1.0).contains(&blend), "{d}: {blend}");
            last = fine;
            d *= 1.01;
        }
    }

    #[test]
    fn lines_across_the_view_stay_between_ten_and_forty() {
        let mut d = 0.5_f32;
        while d < 1.0e6 {
            let (fine, blend) = grid_step(d, 1.0);
            let across = d * VIEW_SPAN / fine;
            assert!((9.99..100.01).contains(&across), "{d}: {across}");
            if blend == 0.0 {
                assert!((9.99..=40.01).contains(&across), "{d}: {across}");
            } else {
                // Die gröbere Stufe zeigt dann 4 bis 10 Linien.
                assert!((3.99..=10.01).contains(&(across / 10.0)), "{d}");
            }
            d *= 1.013;
        }
    }

    #[test]
    fn default_distance_gives_ten_millimetre_lines() {
        let (fine, blend) = grid_step(150.0, 1.0);
        assert_eq!(fine, 10.0);
        assert_eq!(blend, 0.0);
    }

    #[test]
    fn blend_is_continuous_and_the_step_change_swaps_the_levels() {
        // Direkt vor und nach dem Sprung der feinen Stufe: alt voll gröber, neu voll fein.
        let mut d = 1.0_f32;
        let mut jumps = 0;
        let (mut prev_fine, mut prev_blend) = grid_step(d, 1.0);
        while d < 1.0e5 {
            d *= 1.0005;
            let (fine, blend) = grid_step(d, 1.0);
            if fine != prev_fine {
                jumps += 1;
                assert!(prev_blend > 0.999, "{d}: {prev_blend}");
                assert!(blend < 0.001, "{d}: {blend}");
            } else {
                assert!((blend - prev_blend).abs() < 0.01, "{d}");
            }
            (prev_fine, prev_blend) = (fine, blend);
        }
        assert!(jumps >= 3);
    }

    #[test]
    fn alpha_of_every_line_is_continuous_over_the_zoom_range() {
        for p in palettes() {
            let target = Vec3::new(37.0, -12.0, 5.0);
            let mut d = 5.0_f32;
            let mut prev = line_alphas(&grid_lines(target, d, 1.0, &p), &p);
            while d < 5.0e4 {
                d *= 1.002;
                let cur = line_alphas(&grid_lines(target, d, 1.0, &p), &p);
                for key in prev.keys().chain(cur.keys()) {
                    let a = prev.get(key).copied().unwrap_or(0.0);
                    let b = cur.get(key).copied().unwrap_or(0.0);
                    assert!((a - b).abs() < 0.03, "d = {d}, Linie {key:?}: {a} -> {b}");
                }
                prev = cur;
            }
        }
    }

    #[test]
    fn every_fifth_line_is_stronger_and_brighter() {
        for p in palettes() {
            let lines = grid_lines(Vec3::ZERO, 150.0, 1.0, &p); // Stufe 10 mm, kein Übergang
            let mut major = 0;
            let mut minor = 0;
            for pair in lines.chunks(2).filter(|c| !is_axis(&c[0], &p)) {
                let (a, b) = (pair[0].position, pair[1].position);
                if (a[1] - b[1]).abs() > 1e-6 {
                    continue; // nur die Linien parallel zur x-Achse
                }
                // Mittlerer Eckpunkt der Linie liegt im Zielpunkt: stärkste Deckkraft.
                let k = (a[1] / 10.0).round() as i64;
                let near_centre = pair[0].position[0].abs().min(pair[1].position[0].abs()) < 1e-3;
                if !near_centre {
                    continue;
                }
                let centre = if pair[0].position[0].abs() < 1e-3 {
                    &pair[0]
                } else {
                    &pair[1]
                };
                let color = centre.color;
                if k % 5 == 0 {
                    major += 1;
                    assert_eq!(color[..3], p.grid_major[..3], "k = {k}");
                    assert!(
                        (color[3] - p.grid_major[3] * rim_fade(a[1].abs(), grid_radius(150.0)))
                            .abs()
                            < 1e-4
                    );
                } else {
                    minor += 1;
                    assert_eq!(color[..3], p.grid[..3], "k = {k}");
                    assert!(color[3] < p.grid[3] * rim_fade(a[1].abs(), grid_radius(150.0)) + 1e-4);
                }
            }
            assert!(major >= 4 && minor >= 4 * 4, "{major} {minor}");
        }
    }

    #[test]
    fn lines_of_the_coarser_decade_are_major_when_blended_in() {
        let p = Palette::dark();
        // Übergang fast ganz: die feine Stufe ist fast weg, nur jede zehnte Linie bleibt.
        let d = 830.0; // 99,6 Linien der feinen Stufe (10 mm) über die Ansicht
        let (fine, blend) = grid_step(d, 1.0);
        assert!(blend > 0.95, "{blend}");
        let alphas = line_alphas(&grid_lines(Vec3::ZERO, d, 1.0, &p), &p);
        let at = |k: i64| alphas.get(&(true, (k as f32 * fine * 1000.0).round() as i64));
        let on_coarse = at(50).copied().unwrap_or(0.0); // 5. Linie der gröberen Stufe: Haupt
        let off_coarse = at(7).copied().unwrap_or(0.0);
        assert!(on_coarse > 0.4, "{on_coarse}");
        assert!(off_coarse < 0.05, "{off_coarse}");
    }

    #[test]
    fn alpha_fades_to_zero_at_the_rim_on_all_sides() {
        for p in palettes() {
            let target = Vec3::new(20.0, 30.0, 0.0);
            let d = 150.0;
            let radius = grid_radius(d);
            let lines = grid_lines(target, d, 1.0, &p);
            let mut max = 0.0_f32;
            for v in lines.iter().filter(|v| !is_axis(v, &p)) {
                let r = ((v.position[0] - target.x).powi(2) + (v.position[1] - target.y).powi(2))
                    .sqrt();
                assert!(r <= radius * 1.0001, "{r}");
                assert!(v.color[3] <= 1.0 && v.color[3] >= 0.0);
                if r >= radius * 0.999 {
                    assert!(v.color[3] < 1e-3, "Rand r = {r}: {}", v.color[3]);
                }
                max = max.max(v.color[3]);
            }
            assert!(max > 0.5, "{max}");
            // Alle vier Seiten haben Linien, die zum Rand hin auslaufen.
            for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                let outer = lines.iter().filter(|v| !is_axis(v, &p)).any(|v| {
                    let (x, y) = (v.position[0] - target.x, v.position[1] - target.y);
                    x * dx + y * dy > radius * 0.9 && v.color[3] < 0.02
                });
                assert!(outer, "Seite ({dx}, {dy})");
            }
        }
    }

    #[test]
    fn grid_follows_the_target_and_snaps_to_step_multiples() {
        let p = Palette::dark();
        let (fine, _) = grid_step(150.0, 1.0);
        for target in [Vec3::new(4.0, 3.0, 0.0), Vec3::new(1234.5, -987.6, 0.0)] {
            let lines = grid_lines(target, 150.0, 1.0, &p);
            let grid: Vec<_> = lines.iter().filter(|v| !is_axis(v, &p)).collect();
            assert!(!grid.is_empty());
            for pair in lines.chunks(2).filter(|c| !is_axis(&c[0], &p)) {
                // Die Querkoordinate jeder Linie ist ein Vielfaches des Abstands.
                let (a, b) = (pair[0].position, pair[1].position);
                let c = if (a[1] - b[1]).abs() < 1e-6 {
                    a[1]
                } else {
                    a[0]
                };
                let q = c / fine;
                assert!((q - q.round()).abs() < 1e-3, "{c}");
                assert_eq!(a[2], 0.0);
            }
            // Das Raster reicht in alle Richtungen um das Ziel, nie bis zu einem harten Ende.
            let (minx, maxx) = grid.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| {
                (lo.min(v.position[0]), hi.max(v.position[0]))
            });
            assert!(minx < target.x - 150.0 && maxx > target.x + 150.0);
        }
        // Verschobenes Ziel verschiebt die Linien (neue Querpositionen am einen Rand).
        let a = line_alphas(&grid_lines(Vec3::ZERO, 150.0, 1.0, &p), &p);
        let b = line_alphas(&grid_lines(Vec3::new(400.0, 0.0, 0.0), 150.0, 1.0, &p), &p);
        assert!(a.contains_key(&(false, (-200.0_f32 * 1000.0) as i64)));
        assert!(!b.contains_key(&(false, (-200.0_f32 * 1000.0) as i64)));
        assert!(b.contains_key(&(false, (600.0_f32 * 1000.0) as i64)));
    }

    #[test]
    fn axes_are_unfaded_at_the_origin_even_with_a_far_target() {
        for p in palettes() {
            for target in [Vec3::ZERO, Vec3::new(5000.0, -3000.0, 100.0)] {
                let lines = grid_lines(target, 150.0, 1.0, &p);
                for (color, dir) in [
                    (p.axis_x, [1.0, 0.0, 0.0]),
                    (p.axis_y, [0.0, 1.0, 0.0]),
                    (p.axis_z, [0.0, 0.0, 1.0]),
                ] {
                    let v: Vec<_> = lines.iter().filter(|v| v.color == color).collect();
                    assert_eq!(v.len(), 2);
                    assert_eq!(color[3], 1.0);
                    for p in &v {
                        // liegt auf der Achse durch den Ursprung
                        for (i, d) in dir.iter().enumerate() {
                            if *d == 0.0 {
                                assert_eq!(p.position[i], 0.0);
                            }
                        }
                    }
                    // Die Achse reicht über das Raster um das Ziel hinaus.
                    let reach = v
                        .iter()
                        .map(|p| {
                            p.position[0] * dir[0] + p.position[1] * dir[1] + p.position[2] * dir[2]
                        })
                        .fold(0.0_f32, |m, x| m.max(x.abs()));
                    assert!(reach >= target.length() + grid_radius(150.0) - 1.0);
                }
            }
        }
    }

    #[test]
    fn vertex_count_stays_bounded_over_the_whole_zoom_range() {
        for p in palettes() {
            let mut d = 0.5_f32;
            let mut worst = 0;
            while d < 1.0e6 {
                let lines = grid_lines(Vec3::new(3.0, 4.0, 0.0), d, 1.0, &p);
                assert_eq!(lines.len() % 2, 0);
                worst = worst.max(lines.len());
                d *= 1.007;
            }
            assert!(worst < 8000, "{worst}");
        }
    }

    #[test]
    fn invalid_cameras_give_no_lines() {
        let p = Palette::dark();
        assert!(grid_lines(Vec3::ZERO, f32::NAN, 1.0, &p).is_empty());
        assert!(grid_lines(Vec3::ZERO, 0.0, 1.0, &p).is_empty());
        assert!(grid_lines(Vec3::new(f32::INFINITY, 0.0, 0.0), 10.0, 1.0, &p).is_empty());
    }

    #[test]
    fn major_lines_contrast_with_minor_lines_in_both_palettes() {
        let lum = |c: [f32; 4]| c[0] + c[1] + c[2];
        let (d, l) = (Palette::dark(), Palette::light());
        // Dunkler Grund: Hauptlinien heller; heller Grund: dunkler.
        assert!(lum(d.grid_major) > lum(d.grid));
        assert!(lum(l.grid_major) < lum(l.grid));
    }

    #[test]
    fn inch_steps_are_decades_of_an_inch() {
        let mut d = 0.5_f32;
        while d < 1.0e6 {
            let (fine, blend) = grid_step(d, 25.4);
            let exp = (fine / 25.4).log10();
            assert!((exp - exp.round()).abs() < 1e-4, "{d}: {fine}");
            assert!((0.0..=1.0).contains(&blend), "{d}: {blend}");
            d *= 1.01;
        }
        // Gleiche Linienzahl wie in mm bei 25,4-fachem Abstand.
        let (mm, b_mm) = grid_step(150.0, 1.0);
        let (inch, b_in) = grid_step(150.0 * 25.4, 25.4);
        assert!((inch / mm - 25.4).abs() < 1e-3);
        assert!((b_mm - b_in).abs() < 1e-4);
    }

    #[test]
    fn invalid_unit_falls_back_to_millimetre() {
        assert_eq!(grid_step(150.0, 0.0), grid_step(150.0, 1.0));
        assert_eq!(grid_step(150.0, f32::NAN), grid_step(150.0, 1.0));
    }
}
