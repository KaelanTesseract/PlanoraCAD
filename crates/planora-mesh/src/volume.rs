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

/// Kleinster Spielraum der Volumenprobe (mm³): Rundungsrauschen der Volumenberechnung, wenn
/// sich die Schätzungen aufheben und ebene Flächen keinen Netzfehler beitragen.
pub const BLEND_ABS_SLACK: f64 = 1e-6;

/// Spielraum der Volumenprobe relativ zur Summe der Beträge, wenn konvexe und konkave Kanten
/// zusammen gerundet werden (Review Stufe 2, I1 Runde 2). Ohne Vorzeichenprüfung ist die Probe
/// dort die einzige Sicherung gegen falsch herum gebaute Stücke; mit [`BLEND_REL`] = 0,5 der
/// Summe ginge ein Körper durch, dessen konkave Stücke Material abtragen statt hinzufügen.
///
/// Gemessen (Zufallsreihen in `planora-mesh/tests/blend_mixed.rs`, richtige Körper, Release):
/// größte Abweichung |ΔV − Schätzung| / Summe 0,035 bei 441 gemischten Fällen (Prismen mit
/// einspringenden Ecken, Quader mit gerundeter Tasche oder gerundetem Zapfen, Ketten über glatte
/// Übergänge, Gehrungen) und 0,064 bei 299 gemischten Fällen mit Gehrungen an kurzen Kanten und
/// großem Maß. Die Ungenauigkeit stammt aus der Schätzung der Gehrung (Prisma bis zur
/// Gehrungsebene, nicht exakt); 0,2 lässt dazu etwa den dreifachen Abstand.
///
/// Bekannte, hingenommene Grenze (Task 20 nennt sie): Fehlt im Ergebnis die *kleinere* Sorte
/// Stücke ganz (etwa alle konkaven, wenn sie höchstens 20 % der Summe ausmachen), bleibt die
/// Abweichung innerhalb der Schranke und die Probe besteht – sie findet nur Fehler, die größer
/// sind als die Ungenauigkeit der Schätzung. Umgekehrt könnten Gehrungen mit sehr großem Maß an
/// sehr kurzen Kanten in einer gemischten Anfrage die Schranke überschreiten (dann benannte
/// Ablehnung „unplausibel“, nie ein falscher Körper).
pub const BLEND_REL_MIXED: f64 = 0.2;

/// Haben alle Stücke der Schätzung dasselbe Vorzeichen? Dann ist `|Summe| = Summe der Beträge`
/// bis auf Rundung; diese relative Schranke deckt die Rundung ab.
const SAME_SIGN_REL: f64 = 1e-9;

/// Volumenprobe einer Verrundung (Spec §4.5, Plan P2): `estimate` ist die Schätzung des
/// Kernels (Summe über Kanten und Eckstücke, negativ = Material abgetragen), `scale` die Summe
/// der Beträge dieser Stücke (`Blended::volume_scale`). Die gemessene Änderung `v_out − v_in`
/// darf höchstens `rel · scale + slack + BLEND_ABS_SLACK` von der Schätzung abweichen. Haben
/// alle Stücke dasselbe Vorzeichen (nur konvexe oder nur konkave Kanten), muss die Änderung
/// außerdem dieses Vorzeichen haben (bis `slack`); bei gemischten Kanten heben sich Abtrag und
/// Zugabe auf, dann gibt es kein erwartetes Vorzeichen, und statt `rel` gilt höchstens
/// [`BLEND_REL_MIXED`]. `slack` deckt den Netzfehler ab
/// (Sehnentoleranz mal gekrümmte Oberfläche).
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn blend_volume_plausible(
    v_in: f64,
    v_out: f64,
    estimate: f64,
    scale: f64,
    rel: f64,
    slack: f64,
) -> bool {
    let d = v_out - v_in;
    if !(d.is_finite() && estimate.is_finite() && scale.is_finite() && v_out > 0.0) {
        return false;
    }
    // Die Summe der Beträge ist nie kleiner als der Betrag der Summe.
    let scale = scale.max(estimate.abs());
    let same_sign = estimate.abs() >= scale * (1.0 - SAME_SIGN_REL);
    if same_sign && estimate < 0.0 && d > slack {
        return false;
    }
    if same_sign && estimate > 0.0 && d < -slack {
        return false;
    }
    let rel = if same_sign {
        rel
    } else {
        rel.min(BLEND_REL_MIXED)
    };
    (d - estimate).abs() <= rel * scale + slack + BLEND_ABS_SLACK
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
        assert!(blend_volume_plausible(1000.0, 972.0, -30.0, 30.0, 0.5, 0.5));
        // Falsches Vorzeichen: Material kam hinzu, obwohl abgetragen werden sollte.
        assert!(!blend_volume_plausible(
            1000.0, 1005.0, -30.0, 30.0, 0.5, 0.5
        ));
        // Viel zu viel abgetragen.
        assert!(!blend_volume_plausible(
            1000.0, 900.0, -30.0, 30.0, 0.5, 0.5
        ));
        // Konkav: Material kommt hinzu.
        assert!(blend_volume_plausible(1000.0, 1010.0, 10.0, 10.0, 0.5, 0.5));
        assert!(!blend_volume_plausible(
            1000.0,
            f64::NAN,
            10.0,
            10.0,
            0.5,
            0.5
        ));
        assert!(!blend_volume_plausible(
            1000.0, -1.0, -1001.0, 1001.0, 0.5, 0.5
        ));
        // Eine zu kleine Summe der Beträge zählt wie der Betrag der Schätzung.
        assert!(!blend_volume_plausible(
            1000.0, 1005.0, -30.0, 0.0, 0.5, 0.5
        ));
        assert!(!blend_volume_plausible(
            1000.0,
            972.0,
            -30.0,
            f64::NAN,
            0.5,
            0.5
        ));
    }

    #[test]
    fn mixed_convex_and_concave_blends_use_the_gross_estimate() {
        // Abtrag 1 und Zugabe 1 heben sich auf (Rauschen 1e-15); ebene Fasen, kein Netzfehler.
        let ok = |d: f64, net: f64, gross: f64| {
            blend_volume_plausible(320.0, 320.0 + d, net, gross, BLEND_REL, 0.0)
        };
        let (net, gross) = (-1e-15, 2.0);
        for d in [0.0, 1e-12, -3e-13, 0.39, -0.39] {
            assert!(ok(d, net, gross), "{d}");
        }
        // Mehr als BLEND_REL_MIXED · 2 = 0,4 daneben: abgelehnt (in beide Richtungen).
        for d in [0.41, -0.41, 1.01, -1.01, 5.0] {
            assert!(!ok(d, net, gross), "{d}");
        }
        // Überwiegt die Zugabe (netto +0,5 von brutto 1,5), gibt es kein Vorzeichen zu prüfen;
        // die Abweichung bleibt auf 0,2 · 1,5 = 0,3 beschränkt.
        assert!(ok(0.29, 0.5, 1.5) && ok(0.71, 0.5, 1.5));
        assert!(!ok(-0.1, 0.5, 1.5) && !ok(0.85, 0.5, 1.5));
        // Nur eine Sorte Kanten: Vorzeichenprüfung und BLEND_REL bleiben.
        assert!(!ok(0.1, -0.2, 0.2));
        assert!(ok(-0.29, -0.2, 0.2));
    }

    #[test]
    fn flipped_or_missing_pieces_of_a_mixed_blend_are_rejected() {
        // Review Stufe 2, Runde 2: L-Profil, Rundung r 0,5 an der konkaven Kante (4, 4) und den
        // konvexen (10, 0), (10, 4), (0, 10), (0, 0), Höhe 5; je Kante p = (1 − π/4) · 0,25 · 5.
        let p = (1.0 - std::f64::consts::PI / 4.0) * 0.25 * 5.0;
        let (v, net, gross) = (320.0, -3.0 * p, 5.0 * p);
        let ok =
            |measured: f64| blend_volume_plausible(v, v + measured, net, gross, BLEND_REL, 0.0);
        // Richtiger Körper.
        assert!(ok(-3.0 * p));
        // Konkaves Stück falsch herum (trägt ab statt hinzuzufügen): −5p.
        assert!(!ok(-5.0 * p));
        // Die konvexe Sorte fehlt ganz (nur die Zugabe): +p.
        assert!(!ok(p));
        // Keine Kante gerundet.
        assert!(!ok(0.0));
        // Hingenommene Grenze: fehlt nur die kleinere Sorte (das eine konkave Stück, 20 % der
        // Summe), liegt die Abweichung p genau auf der Schranke 0,2 · 5p.
        assert!(((-4.0 * p - net).abs() - BLEND_REL_MIXED * gross).abs() < 1e-12);
        // Mit der alten Schranke (0,5 der Summe) ging der falsch herum gebaute Körper durch.
        assert!((-5.0 * p - net).abs() <= BLEND_REL * gross);
    }
}
