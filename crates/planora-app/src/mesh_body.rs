//! Netz-Körper in der Oberfläche (Teilprojekt 9, Plan P13): Anzeige-Netz mit harten Kanten,
//! Vorschau bei sehr großen Netzen, Maße aus dem ganzen Netz, Drehung in Grad, Berichtstext.

use crate::state::{Body, BodyKind};
use planora_exchange::{ExchangeMesh, MeshReport, MeshTransform, SMOOTH_ANGLE_DEG, shade};
use planora_feature::BodyRef;
use planora_i18n::tr;
use planora_kernel::{Id, Solid};
use planora_math::{DQuat, DVec3, EulerRot};
use planora_mesh::{FaceRange, Mesh, mass_properties};

/// Ab so vielen Dreiecken zeigt die Ansicht nur jedes k-te Dreieck (Spec §11); gespeichert und
/// exportiert wird immer das ganze Netz.
pub const DISPLAY_TRIANGLES: usize = 5_000_000;

/// Hinweis in der Statuszeile für einen Netz-Körper in der Vorschau.
pub fn preview_note() -> String {
    tr!("mesh-preview-note")
}

/// Farbe für Warnungen (nicht geschlossenes Netz).
pub const WARNING_COLOR: egui::Color32 = crate::theme::DARK.warning;

/// Warnfarbe des Themas.
pub fn warning_color(dark: bool) -> egui::Color32 {
    crate::theme::palette(dark).warning
}

/// Netz für die Ansicht: Punkte nach der Lage `t`, Normalen mit harten Kanten (30°), eine
/// Fläche über das ganze Netz, keine Kantenlinien. Über `max_triangles` Dreiecken nur jedes
/// k-te Dreieck; dann ist der zweite Wert `true` (Vorschau).
pub fn display_mesh(mesh: &ExchangeMesh, t: &MeshTransform, max_triangles: usize) -> (Mesh, bool) {
    let stride = mesh.triangle_count().div_ceil(max_triangles.max(1)).max(1);
    let positions: Vec<[f32; 3]> = (0..mesh.positions.len())
        .map(|i| {
            let p = t.apply(mesh.point(i));
            [p.x as f32, p.y as f32, p.z as f32]
        })
        .collect();
    let indices: Vec<u32> = mesh
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .step_by(stride)
        .flatten()
        .copied()
        .collect();
    let shaded = shade(&ExchangeMesh { positions, indices }, SMOOTH_ANGLE_DEG);
    let to_d = |p: &[f32; 3]| DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]));
    let index_count = shaded.indices.len() as u32;
    let out = Mesh {
        positions: shaded.positions.iter().map(to_d).collect(),
        normals: shaded.normals.iter().map(to_d).collect(),
        indices: shaded.indices,
        face_ranges: vec![FaceRange {
            face: Id::from_index(0),
            first_index: 0,
            index_count,
        }],
        edge_lines: Vec::new(),
        inexact_faces: Vec::new(),
    };
    (out, stride > 1)
}

/// Körper der Oberfläche für einen Netz-Körper: Anzeige-Netz, Volumen, Oberfläche und Hülle
/// aus dem ganzen Netz, leeres `Solid`.
pub fn make_body(
    name: String,
    source: BodyRef,
    mesh: &ExchangeMesh,
    t: &MeshTransform,
    report: MeshReport,
    max_triangles: usize,
) -> Body {
    let placed = mesh.placed(t);
    let mass = mass_properties(&placed);
    let bounds = placed.positions.iter().fold(
        (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    let (display, preview) = display_mesh(mesh, t, max_triangles);
    Body {
        name,
        kind: BodyKind::Mesh,
        source,
        solid: Solid::default(),
        mesh: display,
        mass,
        bounds,
        mesh_inexact: false,
        mesh_report: Some(report),
        preview,
    }
}

/// Winkel (in Grad), unter dem `euler_degrees` genau +0 liefert; sonst zeigte das Eingabefeld
/// „-0 °“ für Rundungsreste. Liegt weit unter jeder Eingabegenauigkeit des Feldes.
const ANGLE_ZERO_DEG: f64 = 1e-9;

/// Drehung als Euler-Winkel in Grad (Reihenfolge XYZ, Plan P17); winzige Beträge sind +0.
pub fn euler_degrees(q: DQuat) -> DVec3 {
    let (x, y, z) = q.to_euler(EulerRot::XYZ);
    let deg = |a: f64| {
        let d = a.to_degrees();
        if d.abs() < ANGLE_ZERO_DEG { 0.0 } else { d }
    };
    DVec3::new(deg(x), deg(y), deg(z))
}

/// Drehung aus Euler-Winkeln in Grad (Reihenfolge XYZ), normiert.
pub fn rotation_from_degrees(d: DVec3) -> DQuat {
    DQuat::from_euler(
        EulerRot::XYZ,
        d.x.to_radians(),
        d.y.to_radians(),
        d.z.to_radians(),
    )
    .normalize()
}

/// Prüfbericht in einfachen Worten.
pub fn report_text(r: &MeshReport) -> String {
    let mut parts = Vec::new();
    if r.open_edges > 0 {
        parts.push(tr!("mesh-open-edges", n = r.open_edges));
    }
    if r.flipped_edges > 0 {
        parts.push(tr!("mesh-flipped-edges", n = r.flipped_edges));
    }
    if r.non_manifold_edges > 0 {
        parts.push(tr!("mesh-non-manifold-edges", n = r.non_manifold_edges));
    }
    let mut text = if r.closed {
        tr!("mesh-closed")
    } else if parts.is_empty() {
        tr!("mesh-not-closed")
    } else {
        tr!("mesh-not-fully-closed", parts = parts.join(", "))
    };
    if r.degenerate_triangles > 0 {
        text.push(' ');
        text.push_str(&tr!("mesh-degenerate", n = r.degenerate_triangles));
    }
    text
}
