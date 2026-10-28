//! Namen der rücknehmbaren Schritte („Rückgängig: Quader anlegen“): rein, aus dem Vergleich
//! zweier Zustandsbilder. Die Regeln gelten der Reihe nach, die erste passende gewinnt; passt
//! keine, steht [`fallback`] da. Alle Texte nur mit Zeichen, die die Schriften haben.

use planora_drawing::Drawing;
use planora_drawing::model::{Balloon, DrawingKind, ViewSpec};
use planora_feature::{FeatureKind, Parameter, Snapshot, SnapshotFeature};
use planora_i18n::tr;
use planora_kernel::boolean::BoolOp;
use planora_sketch::CurveKind;
use planora_sketch::constraint::ConstraintKind;
use planora_sketch::model::PointId;

/// Name, wenn keine Regel passt.
pub fn fallback() -> String {
    tr!("undo-change")
}

/// Name des Schritts, der das Bild `before` in das Bild `after` verwandelt hat.
pub fn describe_change(before: &Snapshot, after: &Snapshot) -> String {
    parameters(before, after)
        .or_else(|| features(before, after))
        .or_else(|| drawings(before, after))
        .or_else(|| (before.project_info != after.project_info).then_some(tr!("undo-sheet-info")))
        .or_else(|| {
            (before.parts_overrides != after.parts_overrides).then_some(tr!("undo-parts-text"))
        })
        .unwrap_or_else(fallback)
}

/// Parametertabelle (vor allen anderen Regeln: eine Parameteränderung ändert meist auch Felder
/// und Körper, der Schritt heißt trotzdem nach dem Parameter). Eine neue Kennung: „Parameter
/// anlegen“; eine fehlende (Löschen, auch „Durch den Wert ersetzen“): „Parameter löschen“; sonst
/// eine andere Zeile oder ein anderer Zähler: „Parameter ändern“.
fn parameters(before: &Snapshot, after: &Snapshot) -> Option<String> {
    let missing = |list: &[Parameter], p: &Parameter| list.iter().all(|q| q.id != p.id);
    if after
        .parameters
        .iter()
        .any(|p| missing(&before.parameters, p))
    {
        return Some(tr!("undo-param-add"));
    }
    if before
        .parameters
        .iter()
        .any(|p| missing(&after.parameters, p))
    {
        return Some(tr!("undo-param-delete"));
    }
    (before.parameters != after.parameters || before.next_param != after.next_param)
        .then_some(tr!("undo-param-change"))
}

fn has(list: &[SnapshotFeature], f: &SnapshotFeature) -> bool {
    list.iter().any(|g| g.id == f.id)
}

fn features(before: &Snapshot, after: &Snapshot) -> Option<String> {
    let added: Vec<&SnapshotFeature> = after
        .features
        .iter()
        .filter(|f| !has(&before.features, f))
        .collect();
    // 1. Genau ein Feature neu (hat Vorrang vor allem anderen); mehrere neu: Ändern, außer es sind
    //    lauter STEP-Körper (ein Import legt alle Körper der Datei in einem Schritt an).
    match added.as_slice() {
        [] => {}
        [f] => return Some(created_label(&f.kind)),
        many if many
            .iter()
            .all(|f| matches!(f.kind, FeatureKind::StepBody { .. })) =>
        {
            return Some(created_label(&many[0].kind));
        }
        _ => return Some(fallback()),
    }
    // 2. Features fehlen.
    if before.features.iter().any(|f| !has(&after.features, f)) {
        return Some(tr!("undo-feature-delete"));
    }
    // 3. Gleiche Kennungen: das erste geänderte Feature je Art der Änderung.
    let pairs: Vec<(&SnapshotFeature, &SnapshotFeature)> = before
        .features
        .iter()
        .filter_map(|b| after.features.iter().find(|a| a.id == b.id).map(|a| (b, a)))
        .collect();
    if pairs.iter().any(|(b, a)| b.name != a.name) {
        return Some(tr!("undo-rename"));
    }
    if let Some((_, a)) = pairs.iter().find(|(b, a)| b.suppressed != a.suppressed) {
        return Some(if a.suppressed {
            tr!("undo-suppress")
        } else {
            tr!("undo-unsuppress")
        });
    }
    if before.marker != after.marker {
        return Some(tr!("undo-timeline-marker"));
    }
    // Marker eines Körpers in der Verlaufsleiste (vor- wie zurück): eigener Name neben der
    // globalen Zeitleistenmarke.
    if before.rollbacks != after.rollbacks {
        return Some(tr!("undo-history-marker"));
    }
    // Gleiche Art, aber andere Formeln (etwa nur der Formeltext, der Wert bleibt): benannt wie
    // eine Änderung der Art.
    if let Some((_, a)) = pairs
        .iter()
        .find(|(b, a)| b.kind != a.kind || b.formulas != a.formulas)
    {
        return Some(match a.kind {
            FeatureKind::Move { .. } | FeatureKind::MeshBody { .. } => tr!("undo-move"),
            FeatureKind::Sketch(_) => tr!("undo-sketch-edit"),
            _ => tr!("undo-feature-change"),
        });
    }
    None
}

fn created_label(kind: &FeatureKind) -> String {
    match kind {
        FeatureKind::Box { .. } => tr!("undo-box"),
        FeatureKind::Cylinder { .. } => tr!("undo-cylinder"),
        FeatureKind::Sphere { .. } => tr!("undo-sphere"),
        FeatureKind::Cone { .. } => tr!("undo-cone"),
        FeatureKind::Sketch(_) => tr!("undo-sketch-create"),
        FeatureKind::Extrude { .. } => tr!("undo-extrude"),
        FeatureKind::Revolve { .. } => tr!("undo-revolve"),
        FeatureKind::Boolean { op, .. } => match op {
            BoolOp::Union => tr!("undo-union"),
            BoolOp::Difference => tr!("undo-subtract"),
            BoolOp::Intersection => tr!("undo-intersect"),
        },
        FeatureKind::Move { .. } => tr!("undo-move"),
        FeatureKind::Verrunden { art, .. } => {
            if art.is_chamfer() {
                tr!("undo-chamfer")
            } else {
                tr!("undo-fillet")
            }
        }
        FeatureKind::MeshBody { .. } => tr!("undo-mesh-import"),
        FeatureKind::StepBody { .. } => tr!("undo-step-import"),
    }
}

fn drawings(before: &Snapshot, after: &Snapshot) -> Option<String> {
    let new: Vec<&Drawing> = after
        .drawings
        .iter()
        .filter(|d| before.drawings.iter().all(|b| b.id != d.id))
        .collect();
    if !new.is_empty() {
        let only_parts = new
            .iter()
            .all(|d| matches!(d.kind, DrawingKind::PartsList { .. }));
        return Some(if only_parts {
            tr!("undo-parts-sheet")
        } else {
            tr!("undo-drawing-create")
        });
    }
    if before
        .drawings
        .iter()
        .any(|b| after.drawings.iter().all(|d| d.id != b.id))
    {
        return Some(tr!("undo-drawing-delete"));
    }
    let pairs: Vec<(&Drawing, &Drawing)> = before
        .drawings
        .iter()
        .filter_map(|b| after.drawings.iter().find(|a| a.id == b.id).map(|a| (b, a)))
        .filter(|(b, a)| b != a)
        .collect();
    if pairs.is_empty() {
        return None;
    }
    if pairs.iter().any(|(b, a)| b.name != a.name) {
        return Some(tr!("undo-drawing-rename"));
    }
    inside_drawings(&pairs)
}

/// Gleiche Ansicht in beiden Bildern (nur Ansichten, die es vorher und nachher gibt).
fn view_pairs<'a>(pairs: &[(&'a Drawing, &'a Drawing)]) -> Vec<(&'a ViewSpec, &'a ViewSpec)> {
    pairs
        .iter()
        .flat_map(|(b, a)| {
            b.views
                .iter()
                .filter_map(|bv| a.views.iter().find(|av| av.id == bv.id).map(|av| (bv, av)))
        })
        .collect()
}

fn inside_drawings(pairs: &[(&Drawing, &Drawing)]) -> Option<String> {
    let new_views: Vec<&ViewSpec> = pairs
        .iter()
        .flat_map(|(b, a)| a.views.iter().filter(|v| b.view(v.id).is_none()))
        .collect();
    if !new_views.is_empty() {
        return Some(if new_views.iter().any(|v| v.section.is_some()) {
            tr!("undo-section-create")
        } else {
            tr!("undo-view-add")
        });
    }
    if pairs
        .iter()
        .any(|(b, a)| b.views.iter().any(|v| a.view(v.id).is_none()))
    {
        return Some(tr!("undo-view-delete"));
    }
    let vp = view_pairs(pairs);
    if vp.iter().any(|(b, a)| b.placement != a.placement) {
        return Some(tr!("undo-view-move"));
    }
    if vp.iter().any(|(b, a)| b.scale != a.scale) {
        return Some(tr!("undo-scale-change"));
    }
    if vp.iter().any(|(b, a)| b.section != a.section) {
        return Some(tr!("undo-section-change"));
    }
    if vp.iter().any(|(b, a)| b != a) {
        return Some(tr!("undo-view-change"));
    }
    if pairs
        .iter()
        .any(|(b, a)| a.dimensions.iter().any(|m| b.dimension(m.id).is_none()))
    {
        return Some(tr!("undo-dim-add"));
    }
    if pairs
        .iter()
        .any(|(b, a)| b.dimensions.iter().any(|m| a.dimension(m.id).is_none()))
    {
        return Some(tr!("undo-dim-delete"));
    }
    if pairs.iter().any(|(b, a)| b.dimensions != a.dimensions) {
        return Some(tr!("undo-dim-change"));
    }
    if pairs
        .iter()
        .any(|(b, a)| a.balloons.iter().any(|n| b.balloon(n.id).is_none()))
    {
        return Some(tr!("undo-balloon-add"));
    }
    if pairs
        .iter()
        .any(|(b, a)| b.balloons.iter().any(|n| a.balloon(n.id).is_none()))
    {
        return Some(tr!("undo-balloon-delete"));
    }
    if pairs
        .iter()
        .any(|(b, a)| balloons_differ(&b.balloons, &a.balloons))
    {
        return Some(tr!("undo-balloon-move"));
    }
    if pairs
        .iter()
        .any(|(b, a)| b.sheet != a.sheet || b.projection != a.projection || b.info != a.info)
    {
        return Some(tr!("undo-sheet-info"));
    }
    None
}

fn balloons_differ(b: &[Balloon], a: &[Balloon]) -> bool {
    b.iter().any(|x| a.iter().any(|y| y.id == x.id && y != x))
}

/// Name des Schritts, der die Skizze `before` in `after` verwandelt hat (Zeichnen, Löschen,
/// Maße, Hilfslinie ...).
pub fn describe_sketch_change(
    before: &planora_feature::SketchFeature,
    after: &planora_feature::SketchFeature,
) -> String {
    describe_sketch_change_beside(before, after, &[])
}

/// Wie [`describe_sketch_change`]; die offenen Punkte `open` eines laufenden Werkzeugs (erster
/// Klick, samt ihrer Fangbindung) zählen nicht als „Punkt setzen“ bzw. neue Abhängigkeit: Sie
/// gehören zum nächsten Element, der Name beschreibt die übrige Änderung.
pub fn describe_sketch_change_beside(
    before: &planora_feature::SketchFeature,
    after: &planora_feature::SketchFeature,
    open: &[PointId],
) -> String {
    let (bs, as_) = (&before.sketch, &after.sketch);
    if after.copies.len() > before.copies.len() {
        return tr!("undo-edge-project");
    }
    // Gelöscht heißt: eine Kennung von vorher gibt es nachher nicht mehr (nicht nur: weniger).
    let removed = bs.curves().any(|(id, _)| as_.curve(id).is_none())
        || bs
            .points()
            .any(|(id, _)| as_.points().all(|(p, _)| p != id))
        || bs.constraints().any(|(id, _)| as_.constraint(id).is_none());
    if removed {
        return tr!("undo-delete");
    }
    let new_curves: Vec<&CurveKind> = as_
        .curves()
        .filter(|(id, _)| bs.curve(*id).is_none())
        .map(|(_, c)| &c.kind)
        .collect();
    match new_curves.as_slice() {
        [] => {}
        [one] => {
            return match one {
                CurveKind::Line { .. } => tr!("undo-line"),
                CurveKind::Circle { .. } => tr!("undo-circle"),
                CurveKind::Arc { .. } => tr!("undo-arc"),
                CurveKind::Ellipse { .. } => tr!("undo-ellipse"),
                CurveKind::Spline { .. } => tr!("undo-spline"),
            };
        }
        many => {
            let lines = many.iter().all(|k| matches!(k, CurveKind::Line { .. }));
            return if many.len() == 4 && lines {
                tr!("undo-rectangle")
            } else {
                tr!("undo-draw")
            };
        }
    }
    if as_
        .points()
        .any(|(id, _)| !open.contains(&id) && bs.points().all(|(p, _)| p != id))
    {
        return tr!("undo-point");
    }
    let open_snap =
        |k: &ConstraintKind| matches!(k, ConstraintKind::PointOnCurve(p, _) if open.contains(p));
    let new_constraints: Vec<&ConstraintKind> = as_
        .constraints()
        .filter(|(id, c)| bs.constraint(*id).is_none() && !open_snap(&c.kind))
        .map(|(_, c)| &c.kind)
        .collect();
    if !new_constraints.is_empty() {
        let any_dim = new_constraints.iter().any(|k| k.is_dimension());
        return if any_dim {
            tr!("undo-dim-set")
        } else {
            tr!("undo-constraint")
        };
    }
    if as_.curves().any(|(id, c)| {
        bs.curve(id)
            .is_some_and(|o| o.construction != c.construction)
    }) {
        return tr!("undo-construction");
    }
    // Nach allen Strukturregeln: eine Formel neu, anders oder entfernt (auch wenn sich dabei der
    // Wert des Maßes ändert).
    if before.formulas != after.formulas {
        return tr!("undo-formula");
    }
    tr!("undo-sketch-change")
}
