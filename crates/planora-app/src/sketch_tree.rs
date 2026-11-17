//! Baum des Browser-Ordners „Skizzen“ mit Skizzengruppen (Skizzen 11b): reine Logik ohne
//! Zeichnen. Je Ebene stehen erst die Gruppen in Anlegereihenfolge, dann die Skizzen in
//! Verlaufsreihenfolge (**Annahme**); der Inhalt zugeklappter Gruppen fehlt. Der
//! Aufklappzustand gehört zur Ansicht (wie die Ordner), nicht zum Dokument.

use planora_feature::{Document, FeatureId, FeatureKind, GroupItem, SketchGroupId};
use std::collections::BTreeSet;

/// Eine Zeile des Skizzenbaums: Skizze oder Gruppe, Tiefe (0 = oben im Ordner) und Name.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeRow {
    pub item: GroupItem,
    pub depth: usize,
    pub name: String,
}

/// Die Zeilen des Ordners „Skizzen“ von oben nach unten; `open` sind die aufgeklappten Gruppen.
pub fn sketch_tree(doc: &Document, open: &BTreeSet<SketchGroupId>) -> Vec<TreeRow> {
    let sketches: Vec<(FeatureId, String)> = doc
        .features()
        .iter()
        .filter(|f| matches!(f.kind, FeatureKind::Sketch(_)))
        .map(|f| (f.id, f.name.clone()))
        .collect();
    let mut rows = Vec::new();
    level(doc, &sketches, open, None, 0, &mut rows);
    rows
}

/// Hängt die Zeilen unter `parent` (Tiefe `depth`) an `rows` an.
fn level(
    doc: &Document,
    sketches: &[(FeatureId, String)],
    open: &BTreeSet<SketchGroupId>,
    parent: Option<SketchGroupId>,
    depth: usize,
    rows: &mut Vec<TreeRow>,
) {
    let groups = doc.sketch_groups();
    // Schutz gegen einen Kreis (das Dokument lässt keinen zu): tiefer als Gruppen gibt es nicht.
    if depth > groups.groups.len() {
        return;
    }
    for g in groups.groups.iter().filter(|g| g.parent == parent) {
        rows.push(TreeRow {
            item: GroupItem::Group(g.id),
            depth,
            name: g.name.clone(),
        });
        if open.contains(&g.id) {
            level(doc, sketches, open, Some(g.id), depth + 1, rows);
        }
    }
    for (id, name) in sketches {
        if groups.member.get(id).copied() == parent {
            rows.push(TreeRow {
                item: GroupItem::Sketch(*id),
                depth,
                name: name.clone(),
            });
        }
    }
}

/// Die Gruppe, in der `item` gerade liegt (`None`: oben im Ordner).
fn parent_of(doc: &Document, item: GroupItem) -> Option<SketchGroupId> {
    let groups = doc.sketch_groups();
    match item {
        GroupItem::Sketch(id) => groups.member.get(&id).copied(),
        GroupItem::Group(g) => groups.group(g).and_then(|g| g.parent),
    }
}

/// Lässt sich `item` auf `target` (eine Gruppenzeile, `None`: Ordnerzeile „Skizzen“) ablegen?
/// Nein, wenn eines davon fehlt, `item` schon dort liegt oder eine Gruppe in sich selbst oder
/// eine ihrer Untergruppen käme.
pub fn drop_target_ok(doc: &Document, item: GroupItem, target: Option<SketchGroupId>) -> bool {
    let groups = doc.sketch_groups();
    let exists = match item {
        GroupItem::Sketch(id) => doc
            .feature(id)
            .is_some_and(|f| matches!(f.kind, FeatureKind::Sketch(_))),
        GroupItem::Group(g) => groups.group(g).is_some(),
    };
    if !exists || target.is_some_and(|t| groups.group(t).is_none()) {
        return false;
    }
    if parent_of(doc, item) == target {
        return false;
    }
    if let GroupItem::Group(g) = item {
        // Von `target` nach oben: trifft der Weg `g`, läge die Gruppe in sich selbst.
        let mut at = target;
        let mut steps = 0;
        while let Some(t) = at {
            if t == g || steps > groups.groups.len() {
                return false;
            }
            at = groups.group(t).and_then(|x| x.parent);
            steps += 1;
        }
    }
    true
}

/// Ist die Gruppe `g` ausgeblendet, selbst oder über eine übergeordnete Gruppe?
pub fn group_hidden(doc: &Document, g: SketchGroupId) -> bool {
    let groups = doc.sketch_groups();
    let mut at = Some(g);
    let mut steps = 0;
    while let Some(id) = at {
        let Some(grp) = groups.group(id) else {
            return false;
        };
        if grp.hidden {
            return true;
        }
        if steps > groups.groups.len() {
            return false;
        }
        at = grp.parent;
        steps += 1;
    }
    false
}

/// Verdeckt eine ausgeblendete Gruppe über der Zeile deren Auge? Für eine Skizze zählen alle
/// Gruppen auf dem Weg nach oben, für eine Gruppe nur die darüber, nicht sie selbst. Das eigene
/// Auge der Zeile bleibt, wie es ist; es wird nur gedämpft gezeichnet (Skizzen 11b, Handtest 1).
pub fn covered_by_group(doc: &Document, item: GroupItem) -> bool {
    match item {
        GroupItem::Sketch(id) => doc.sketch_hidden_by_group(id),
        GroupItem::Group(_) => parent_of(doc, item).is_some_and(|p| group_hidden(doc, p)),
    }
}
