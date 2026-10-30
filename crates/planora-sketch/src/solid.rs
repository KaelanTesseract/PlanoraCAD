//! Von der Skizze zum Körper: Extrusion und Rotation.

use crate::error::SketchError;
use crate::model::{CurveId, CurveKind, Sketch};
use planora_i18n::tr;
use planora_kernel::{Kernel, Profile, ProfileLoop, Segment, Solid, segment_end};
use planora_math::{DVec2, DVec3, Frame};

/// Abtastung je Vollkreis für die Seitenprüfung der Bögen (wie `ProfileLoop::signed_area`).
const SIDE_CHECK_STEPS_PER_TURN: usize = 64;

fn kernel_error(e: impl std::fmt::Display) -> SketchError {
    SketchError::Kernel(e.to_string())
}

/// Extrudiert ein Skizzenprofil um `height` entlang der Skizzennormalen (`frame.z`).
pub fn extrude_profile<K: Kernel>(
    kernel: &K,
    sketch: &Sketch,
    profile: &Profile,
    height: f64,
) -> Result<Solid, SketchError> {
    kernel
        .extrude(profile, sketch.frame(), height)
        .map_err(kernel_error)
}

/// Dreht ein Skizzenprofil um die Linie `axis` der Skizze (360°).
pub fn revolve_profile<K: Kernel>(
    kernel: &K,
    sketch: &Sketch,
    profile: &Profile,
    axis: CurveId,
) -> Result<Solid, SketchError> {
    let (chain, frame) = sketch.revolve_input(profile, axis)?;
    kernel.revolve(&chain, &frame).map_err(kernel_error)
}

impl Sketch {
    /// Bereitet ein Profil für die Rotation um die Linie `axis` vor: offene Kette in der
    /// (r, z)-Halbebene und das räumliche Achssystem (z = Drehachse, x = Seite des Profils).
    ///
    /// Das Profil darf keine Löcher haben und muss genau ein Linienstück auf der Achse
    /// besitzen; es muss ganz auf einer Seite der Achse liegen.
    pub fn revolve_input(
        &self,
        profile: &Profile,
        axis: CurveId,
    ) -> Result<(ProfileLoop, Frame), SketchError> {
        let tol = self.tolerance().linear;
        if !profile.holes.is_empty() {
            return Err(SketchError::UnsupportedForSolid(tr!("sketch-err-hole")));
        }
        self.check_curve(axis)?;
        let CurveKind::Line { a, b } = self.curves[axis].kind else {
            return Err(SketchError::WrongType(tr!("sketch-err-axis-line")));
        };
        let (pa, pb) = (self.pos(a), self.pos(b));
        if pa.distance(pb) <= tol {
            return Err(SketchError::Degenerate(tr!("sketch-err-axis-zero")));
        }
        let d = (pb - pa).normalize();
        let perp_of = |p: DVec2| d.x * (p - pa).y - d.y * (p - pa).x;
        let along_of = |p: DVec2| (p - pa).dot(d);

        // Punkte der Schleife und das Stück auf der Achse suchen
        let lp = &profile.outer;
        let pts = lp.chain_points();
        let m = lp.segments.len();
        let on_axis = |i: usize| {
            matches!(lp.segments[i], Segment::Line { .. })
                && perp_of(pts[i]).abs() <= tol
                && perp_of(pts[i + 1]).abs() <= tol
        };
        let Some(axis_seg) = (0..m).find(|&i| on_axis(i)) else {
            return Err(SketchError::Degenerate(tr!(
                "sketch-err-profile-no-axis-edge"
            )));
        };
        let side: Vec<f64> = lp
            .sample(SIDE_CHECK_STEPS_PER_TURN)
            .iter()
            .map(|&p| perp_of(p))
            .collect();
        let any_left = side.iter().any(|&v| v > tol);
        let any_right = side.iter().any(|&v| v < -tol);
        if any_left && any_right {
            return Err(SketchError::Degenerate(tr!(
                "sketch-err-profile-both-sides"
            )));
        }
        let s = if any_right { -1.0 } else { 1.0 };
        let to_rz = |p: DVec2| DVec2::new(s * perp_of(p), along_of(p));

        // Kette: Segmente nach dem Achsstück, einmal herum bis vor das Achsstück.
        // `pts` hat `m + 1` Einträge, `pts[axis_seg + 1]` ist das Ende des Achsstücks.
        let start_point = pts[axis_seg + 1];
        let mut segments = Vec::with_capacity(m.saturating_sub(1));
        let mut at = start_point;
        for k in 1..m {
            let seg = &lp.segments[(axis_seg + k) % m];
            let end = segment_end(at, seg);
            segments.push(match seg {
                Segment::Line { .. } => Segment::Line { to: to_rz(end) },
                // Die Abbildung (Skizze → (r, z)) kehrt für s = +1 den Drehsinn um.
                &Segment::Arc { center, sweep } => Segment::Arc {
                    center: to_rz(center),
                    sweep: -s * sweep,
                },
                // Freiformkurven werden mit derselben (affinen) Abbildung umgerechnet. Anders als
                // beim Bogen ist keine Korrektur nötig: Die abgebildeten Kontrollpunkte beschreiben
                // die gespiegelte Kurve schon in derselben Durchlaufrichtung (Anfang und Ende
                // bleiben an ihren Kettenpunkten). Eine Umkehr würde die Kette zerreißen; den
                // Umlaufsinn der ganzen Kette richtet der Kernel selbst (`revolve`, Flächeninhalt).
                Segment::Nurbs { curve } => Segment::Nurbs {
                    curve: std::sync::Arc::new(
                        curve
                            .transformed(|p| {
                                let q = to_rz(DVec2::new(p.x, p.y));
                                DVec3::new(q.x, q.y, 0.0)
                            })
                            .map_err(kernel_error)?,
                    ),
                },
            });
            at = end;
        }
        let chain = ProfileLoop {
            start: to_rz(start_point),
            segments,
        };

        let frame = self.frame();
        let z3 = frame.x * d.x + frame.y * d.y;
        let x3 = (frame.x * (-d.y) + frame.y * d.x) * s;
        let y3 = z3.cross(x3);
        Ok((
            chain,
            Frame {
                origin: frame.point_2d(pa),
                x: x3,
                y: y3,
                z: z3,
            },
        ))
    }
}
