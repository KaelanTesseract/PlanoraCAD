use crate::mesh::Mesh;
use planora_kernel::boolean::BoolOp;
use planora_kernel::{Solid, Surface};

/// Prüft, ob das Ergebnisvolumen einer Booleschen Operation in den möglichen Grenzen liegt
/// (Spielraum `rel_tol` relativ zur Summe der Eingangsvolumen).
pub fn boolean_volume_plausible(
    op: BoolOp,
    v_a: f64,
    v_b: f64,
    v_result: f64,
    rel_tol: f64,
) -> bool {
    let eps = rel_tol * (v_a + v_b).max(1e-12);
    // auch NaN ablehnen
    if v_result.is_nan() || v_result <= -eps {
        return false;
    }
    match op {
        BoolOp::Union => v_result >= v_a.max(v_b) - eps && v_result <= v_a + v_b + eps,
        BoolOp::Difference => v_result >= v_a - v_b - eps && v_result <= v_a + eps,
        BoolOp::Intersection => v_result <= v_a.min(v_b) + eps,
    }
}

/// Spielraum der Volumenprobe einer Verrundung relativ zur Schätzung des Kernels (Plan P2).
pub const BLEND_REL: f64 = 0.5;

/// Netzfehler je mm² gekrümmter Fläche in Vielfachen der Sehnentoleranz (Plan P2): jedes Dreieck
/// weicht höchstens die Sehnentoleranz von der Fläche ab; Eingang und Ergebnis zusammen.
pub const BLEND_SLACK_FACTOR: f64 = 2.0;

/// Volumenprobe einer Verrundung (Spec §4.5, Plan P2): `estimate` ist die Schätzung des
/// Kernels (negativ = Material abgetragen). Die gemessene Änderung `v_out − v_in` muss das
/// Vorzeichen der Schätzung haben (bis `slack`) und höchstens `rel · |estimate| + slack` von ihr
/// abweichen. `slack` deckt den Netzfehler ab (Sehnentoleranz mal gekrümmte Oberfläche).
pub fn blend_volume_plausible(v_in: f64, v_out: f64, estimate: f64, rel: f64, slack: f64) -> bool {
    let d = v_out - v_in;
    if !(d.is_finite() && estimate.is_finite() && v_out > 0.0) {
        return false;
    }
    if estimate < 0.0 && d > slack {
        return false;
    }
    if estimate > 0.0 && d < -slack {
        return false;
    }
    (d - estimate).abs() <= rel * estimate.abs() + slack
}

/// Summe der Dreiecksflächen aller nicht ebenen Flächen des Körpers (ebene Flächen werden exakt
/// vernetzt und tragen keinen Volumenfehler bei).
pub fn curved_area(solid: &Solid, mesh: &Mesh) -> f64 {
    let mut area = 0.0;
    for r in &mesh.face_ranges {
        let curved = solid
            .faces
            .get(r.face)
            .is_some_and(|f| !matches!(f.surface, Surface::Plane { .. }));
        if !curved {
            continue;
        }
        let start = r.first_index as usize;
        let end = start + r.index_count as usize;
        for t in mesh.indices[start..end].as_chunks::<3>().0 {
            let a = mesh.positions[t[0] as usize];
            let b = mesh.positions[t[1] as usize];
            let c = mesh.positions[t[2] as usize];
            area += 0.5 * (b - a).cross(c - a).length();
        }
    }
    area
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_per_operation() {
        let ok = |op, v| boolean_volume_plausible(op, 10.0, 5.0, v, 0.01);
        assert!(ok(BoolOp::Union, 12.0));
        assert!(!ok(BoolOp::Union, 16.0));
        assert!(ok(BoolOp::Difference, 5.0));
        assert!(!ok(BoolOp::Difference, 4.0));
        assert!(ok(BoolOp::Intersection, 0.0));
        assert!(!ok(BoolOp::Intersection, -1.0));
        assert!(!ok(BoolOp::Union, f64::NAN));
    }

    #[test]
    fn blend_volumes_must_match_sign_and_estimate() {
        // Rundung trägt 30 ab (Schätzung −30), Netz sagt −28: plausibel.
        assert!(blend_volume_plausible(1000.0, 972.0, -30.0, 0.5, 0.5));
        // Falsches Vorzeichen: Material kam hinzu, obwohl abgetragen werden sollte.
        assert!(!blend_volume_plausible(1000.0, 1005.0, -30.0, 0.5, 0.5));
        // Viel zu viel abgetragen.
        assert!(!blend_volume_plausible(1000.0, 900.0, -30.0, 0.5, 0.5));
        // Konkav: Material kommt hinzu.
        assert!(blend_volume_plausible(1000.0, 1010.0, 10.0, 0.5, 0.5));
        assert!(!blend_volume_plausible(1000.0, f64::NAN, 10.0, 0.5, 0.5));
        assert!(!blend_volume_plausible(1000.0, -1.0, -1001.0, 0.5, 0.5));
    }
}
