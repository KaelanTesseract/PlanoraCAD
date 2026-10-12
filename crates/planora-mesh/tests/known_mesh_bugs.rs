//! Früher bekannte Fehler der Vernetzung, jetzt als Regressionstests.

use planora_kernel::boolean::{BoolOp, boolean};
use planora_kernel::{AnalyticKernel, Curve, Kernel, validate};
use planora_math::{DMat4, DVec3, Tolerance};
use planora_mesh::{
    TessellationOptions, check_face_boundaries, is_watertight, mass_properties, tessellate,
};

#[test]
fn box_on_a_rotated_analytic_cylinder_meshes_closed() {
    // Rein analytisch (gefunden im Review von Task 10): Quader [−1,192; 1,192] × [−3; 4] × [−5; 5]
    // auf einem Zylinder r = 3,368 (Achse z von −12,19 bis −5, Mitte (−3,65; 0,33), Naht bei
    // (−4,977; 3,422)). Vereinigung und Zylinder minus Quader sind gültige Körper; ihr Netz war bei
    // jeder Sehnentoleranz offen: Der Randkreis des Mantels oben ist in mehrere Kanten geteilt, der
    // Mantel galt aber als unbeschnitten (volle Parameterfläche) und bekam das gleichmäßige Raster
    // über dem Parameterrechteck, dessen Randpunkte nicht die Teilpunkte der Nachbarfläche sind.
    let k = AnalyticKernel::default();
    let c = k.make_cylinder(3.368, 12.19 - 5.0).unwrap();
    let seam = c
        .edges
        .iter()
        .find(|(_, e)| matches!(e.curve, Curve::Line { .. }))
        .map(|(_, e)| e.curve.point(e.t0))
        .unwrap();
    let turn = (3.422_f64 - 0.33).atan2(-4.977 + 3.65) - seam.y.atan2(seam.x);
    let cyl = k
        .transform(
            &c,
            &(DMat4::from_translation(DVec3::new(-3.65, 0.33, -12.19))
                * DMat4::from_rotation_z(turn)),
        )
        .unwrap();
    let b = k
        .transform(
            &k.make_box(DVec3::new(2.384, 7.0, 10.0)).unwrap(),
            &DMat4::from_translation(DVec3::new(-1.192, -3.0, -5.0)),
        )
        .unwrap();
    let tol = Tolerance::default();
    for (a, b, op) in [(&b, &cyl, BoolOp::Union), (&cyl, &b, BoolOp::Difference)] {
        let s = boolean(a, b, op, &tol).unwrap();
        validate(&s).unwrap();
        let mut volumes = Vec::new();
        for chord in [0.05, 0.002, 0.0005] {
            let m = tessellate(
                &s,
                &TessellationOptions {
                    chord_tolerance: chord,
                },
            )
            .unwrap();
            check_face_boundaries(&s, &m).unwrap();
            assert!(is_watertight(&m, 1e-6), "{op:?}, Sehnentoleranz {chord}");
            volumes.push(mass_properties(&m).volume);
        }
        // feinere Netze nähern sich dem Volumen an (Sehnenfehler der Bögen)
        let (v1, v2) = (volumes[1], volumes[2]);
        assert!(((v1 - v2) / v2).abs() < 1e-3, "{op:?}: {volumes:?}");
    }
}
