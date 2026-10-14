//! Normalen je Punkt mit harten Kanten (Teilprojekt 9, Task 8).

use planora_exchange::*;

/// Einheitswürfel, Dreiecke von außen gegen den Uhrzeigersinn.
fn cube() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7,
            3, 1, 2, 6, 1, 6, 5,
        ],
    }
}

/// Tetraeder mit Kantenlänge 10 an den Achsen (Volumen 1000/6).
fn tetra() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, 10.0],
        ],
        indices: vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3],
    }
}

fn is_unit(n: [f32; 3]) -> bool {
    ((n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() - 1.0).abs() < 1e-6
}

#[test]
fn a_cube_gets_hard_edges() {
    let c = cube();
    let s = shade(&c, SMOOTH_ANGLE_DEG);
    assert_eq!(
        (s.positions.len(), s.normals.len(), s.indices.len()),
        (24, 24, 36)
    );
    for (t, orig) in s
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .zip(c.indices.as_chunks::<3>().0)
    {
        let [a, b, d] = orig.map(|i| c.point(i as usize));
        let face = (b - a).cross(d - a).normalize();
        for (&i, &o) in t.iter().zip(orig) {
            let n = s.normals[i as usize];
            let diff = (f64::from(n[0]) - face.x).abs()
                + (f64::from(n[1]) - face.y).abs()
                + (f64::from(n[2]) - face.z).abs();
            assert!(diff < 1e-6, "{n:?} / {face}");
            assert_eq!(
                s.positions[i as usize], c.positions[o as usize],
                "Lage bleibt"
            );
        }
    }
}

#[test]
fn a_tetrahedron_gets_three_points_per_corner() {
    let s = shade(&tetra(), SMOOTH_ANGLE_DEG);
    assert_eq!(s.positions.len(), 12);
    assert!(s.normals.iter().all(|&n| is_unit(n)));
}

#[test]
fn a_flat_fold_stays_smooth_and_a_sharp_fold_splits() {
    // Zwei Dreiecke an der Kante 0–1; das zweite steigt um den Winkel atan(h) an.
    let fold = |h: f32| ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.5, 1.0, 0.0],
            [0.5, -1.0, h],
        ],
        indices: vec![0, 1, 2, 1, 0, 3],
    };
    let flat = shade(&fold(10f32.to_radians().tan()), SMOOTH_ANGLE_DEG);
    assert_eq!(flat.positions.len(), 4, "10° Knick bleibt glatt");
    let sharp = shade(&fold(60f32.to_radians().tan()), SMOOTH_ANGLE_DEG);
    assert_eq!(sharp.positions.len(), 6, "60° Knick wird hart");
    assert!(
        flat.normals
            .iter()
            .chain(&sharp.normals)
            .all(|&n| is_unit(n))
    );
}

#[test]
fn degenerate_triangles_do_not_change_their_neighbours() {
    let mut m = cube();
    m.indices.extend_from_slice(&[0, 0, 1]);
    let s = shade(&m, SMOOTH_ANGLE_DEG);
    let plain = shade(&cube(), SMOOTH_ANGLE_DEG);
    assert_eq!(s.indices.len(), 39);
    for (a, b) in s.indices[..36].iter().zip(&plain.indices) {
        assert_eq!(s.normals[*a as usize], plain.normals[*b as usize]);
    }
}
