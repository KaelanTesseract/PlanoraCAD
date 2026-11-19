//! Skizzengruppen im Dokument (Skizzen 11b, Task 8): Gruppen anlegen, umbenennen, auflösen,
//! verschieben, Auge der Gruppe und Aufräumen beim Löschen einer Skizze.

use planora_feature::*;
use planora_math::{Frame, Tolerance};
use planora_sketch::Sketch;

fn feature(sketch: Sketch) -> SketchFeature {
    SketchFeature {
        sketch,
        plane: SketchPlane::Standard(StandardPlane::XY),
        copies: vec![],
        formulas: Default::default(),
        text_formulas: TextFormulas::new(),
    }
}

fn sketch(d: &mut Document, n: usize) -> FeatureId {
    let name = format!("Skizze{n}");
    d.add_feature(
        &name,
        FeatureKind::Sketch(feature(Sketch::new(
            Frame::WORLD,
            &name,
            Tolerance::default(),
        ))),
    )
    .unwrap()
}

fn doc_with(n: usize) -> (Document, Vec<FeatureId>) {
    let mut d = Document::new();
    let ids = (1..=n).map(|i| sketch(&mut d, i)).collect();
    (d, ids)
}

fn items(ids: &[FeatureId]) -> Vec<GroupItem> {
    ids.iter().map(|i| GroupItem::Sketch(*i)).collect()
}

fn hide(d: &mut Document, id: FeatureId) {
    let mut v = d.visibility().clone();
    v.features.insert(id);
    d.set_visibility(v);
}

fn name_of(d: &Document, id: SketchGroupId) -> String {
    d.sketch_groups()
        .groups
        .iter()
        .find(|g| g.id == id)
        .unwrap()
        .name
        .clone()
}

#[test]
fn ten_sketches_two_groups() {
    let (mut d, ids) = doc_with(10);
    let a = d.create_sketch_group(&items(&ids[..5])).unwrap();
    let b = d.create_sketch_group(&items(&ids[5..])).unwrap();
    assert_ne!(a, b);
    assert_eq!(d.sketch_groups().groups.len(), 2);
    assert!(ids.iter().all(|i| !d.sketch_hidden_by_group(*i)));
    d.set_sketch_group_hidden(a, true);
    for (k, i) in ids.iter().enumerate() {
        assert_eq!(d.sketch_hidden_by_group(*i), k < 5, "Skizze {k}");
    }
    d.set_sketch_group_hidden(a, false);
    assert!(ids.iter().all(|i| !d.sketch_hidden_by_group(*i)));
    d.check_invariants().unwrap();
}

#[test]
fn hidden_group_keeps_own_eye() {
    let (mut d, ids) = doc_with(2);
    hide(&mut d, ids[0]);
    let g = d.create_sketch_group(&items(&ids)).unwrap();
    d.set_sketch_group_hidden(g, true);
    d.set_sketch_group_hidden(g, false);
    assert!(d.visibility().features.contains(&ids[0]));
    assert!(!d.visibility().features.contains(&ids[1]));
    assert!(!d.sketch_hidden_by_group(ids[0]));
}

#[test]
fn nested_hidden_parent_hides_child_group() {
    let (mut d, ids) = doc_with(3);
    let inner = d.create_sketch_group(&items(&ids[..2])).unwrap();
    let outer = d
        .create_sketch_group(&[GroupItem::Group(inner), GroupItem::Sketch(ids[2])])
        .unwrap();
    d.set_sketch_group_hidden(outer, true);
    assert!(d.sketch_hidden_by_group(ids[0]));
    assert!(d.sketch_hidden_by_group(ids[2]));
    d.set_sketch_group_hidden(outer, false);
    d.set_sketch_group_hidden(inner, true);
    assert!(d.sketch_hidden_by_group(ids[1]));
    assert!(!d.sketch_hidden_by_group(ids[2]));
}

/// Skizzen 12, Task 12: `sketch_hidden_by_group` hält einen Zwischenspeicher je Dokumentstand.
/// Er folgt jeder Änderung: Auge, Verschieben, Auflösen, Rückgängig und Wiederholen, Kopie.
#[test]
fn hidden_by_group_follows_every_change() {
    let (mut d, ids) = doc_with(3);
    let inner = d.create_sketch_group(&items(&ids[..2])).unwrap();
    let outer = d.create_sketch_group(&[GroupItem::Group(inner)]).unwrap();
    let visible = d.snapshot();
    assert!(!d.sketch_hidden_by_group(ids[0]));
    d.set_sketch_group_hidden(outer, true);
    assert!(d.sketch_hidden_by_group(ids[0]));
    assert!(!d.sketch_hidden_by_group(ids[2]));
    let hidden = d.snapshot();
    // Eine Kopie rechnet selbst, die Änderung am Original erreicht sie nicht.
    let copy = d.clone();
    // Rückgängig und Wiederholen (nur Anzeige, kurzer Weg im Zurückladen).
    d.restore(visible.clone()).unwrap();
    assert!(!d.sketch_hidden_by_group(ids[0]));
    d.restore(hidden).unwrap();
    assert!(d.sketch_hidden_by_group(ids[1]));
    assert!(copy.sketch_hidden_by_group(ids[1]));
    // Verschieben aus der Gruppe und wieder hinein.
    d.move_to_group(GroupItem::Sketch(ids[0]), None).unwrap();
    assert!(!d.sketch_hidden_by_group(ids[0]));
    assert!(d.sketch_hidden_by_group(ids[1]));
    d.move_to_group(GroupItem::Sketch(ids[2]), Some(inner))
        .unwrap();
    assert!(d.sketch_hidden_by_group(ids[2]));
    // Auflösen der äußeren Gruppe: Ihr Auge fällt weg.
    d.ungroup_sketch_group(outer).unwrap();
    assert!(!d.sketch_hidden_by_group(ids[1]));
    assert!(!d.sketch_hidden_by_group(ids[2]));
    // Rückgängig bis zum sichtbaren Bild.
    d.restore(visible).unwrap();
    assert!(ids.iter().all(|i| !d.sketch_hidden_by_group(*i)));
    assert!(copy.sketch_hidden_by_group(ids[0]));
}

#[test]
fn ungroup_moves_content_one_level_up() {
    let (mut d, ids) = doc_with(3);
    let inner = d.create_sketch_group(&items(&ids[..2])).unwrap();
    let outer = d
        .create_sketch_group(&[GroupItem::Group(inner), GroupItem::Sketch(ids[2])])
        .unwrap();
    d.ungroup_sketch_group(inner).unwrap();
    let g = d.sketch_groups();
    assert!(g.groups.iter().all(|x| x.id != inner));
    assert_eq!(g.member.get(&ids[0]), Some(&outer));
    assert_eq!(g.member.get(&ids[1]), Some(&outer));
    d.ungroup_sketch_group(outer).unwrap();
    assert!(d.sketch_groups().groups.is_empty());
    assert!(d.sketch_groups().member.is_empty());
    // Untergruppe wandert in den Elternteil der aufgelösten Gruppe.
    let a = d.create_sketch_group(&items(&ids[..1])).unwrap();
    let b = d.create_sketch_group(&[GroupItem::Group(a)]).unwrap();
    let c = d.create_sketch_group(&[GroupItem::Group(b)]).unwrap();
    d.ungroup_sketch_group(b).unwrap();
    let pa = d.sketch_groups().groups.iter().find(|x| x.id == a).unwrap();
    assert_eq!(pa.parent, Some(c));
    d.check_invariants().unwrap();
}

#[test]
fn move_group_into_descendant_refused() {
    let (mut d, ids) = doc_with(2);
    let inner = d.create_sketch_group(&items(&ids[..1])).unwrap();
    let outer = d.create_sketch_group(&[GroupItem::Group(inner)]).unwrap();
    let before = d.sketch_groups().clone();
    let edits = d.edit_count();
    assert!(matches!(
        d.move_to_group(GroupItem::Group(outer), Some(inner)),
        Err(FeatureError::InvalidInput(_))
    ));
    assert!(matches!(
        d.move_to_group(GroupItem::Group(outer), Some(outer)),
        Err(FeatureError::InvalidInput(_))
    ));
    assert_eq!(d.sketch_groups(), &before);
    assert_eq!(d.edit_count(), edits);
    // Erlaubt: Skizze in die innere Gruppe, Gruppe nach oben.
    d.move_to_group(GroupItem::Sketch(ids[1]), Some(inner))
        .unwrap();
    d.move_to_group(GroupItem::Group(inner), None).unwrap();
    assert_eq!(d.sketch_groups().member.get(&ids[1]), Some(&inner));
}

#[test]
fn delete_sketch_in_group_then_undo() {
    let (mut d, ids) = doc_with(2);
    let g = d.create_sketch_group(&items(&ids)).unwrap();
    let before = d.snapshot();
    d.delete(ids[0]).unwrap();
    assert!(!d.sketch_groups().member.contains_key(&ids[0]));
    assert_eq!(d.sketch_groups().member.get(&ids[1]), Some(&g));
    d.restore(before).unwrap();
    assert_eq!(d.sketch_groups().member.get(&ids[0]), Some(&g));
    assert_eq!(d.sketch_groups().member.get(&ids[1]), Some(&g));
}

#[test]
fn empty_group_stays() {
    let (mut d, ids) = doc_with(1);
    let g = d.create_sketch_group(&items(&ids)).unwrap();
    d.delete(ids[0]).unwrap();
    assert!(d.sketch_groups().groups.iter().any(|x| x.id == g));
    assert!(d.sketch_groups().member.is_empty());
}

#[test]
fn groups_do_not_recompute() {
    let (mut d, ids) = doc_with(3);
    let model = d.model_edit_count();
    let versions: Vec<u64> = d.features().iter().map(|f| f.version()).collect();
    let edits = d.edit_count();
    let g = d.create_sketch_group(&items(&ids[..2])).unwrap();
    assert_eq!(d.edit_count(), edits + 1);
    d.rename_sketch_group(g, "Seitenteile").unwrap();
    d.set_sketch_group_hidden(g, true);
    d.move_to_group(GroupItem::Sketch(ids[2]), Some(g)).unwrap();
    d.ungroup_sketch_group(g).unwrap();
    assert_eq!(d.edit_count(), edits + 5);
    assert_eq!(d.model_edit_count(), model);
    let after: Vec<u64> = d.features().iter().map(|f| f.version()).collect();
    assert_eq!(versions, after);
}

#[test]
fn default_names_count_up() {
    let (mut d, ids) = doc_with(3);
    let a = d.create_sketch_group(&items(&ids[..1])).unwrap();
    let b = d.create_sketch_group(&items(&ids[1..2])).unwrap();
    assert_eq!(name_of(&d, a), "Gruppe1");
    assert_eq!(name_of(&d, b), "Gruppe2");
    d.ungroup_sketch_group(a).unwrap();
    let c = d.create_sketch_group(&items(&ids[2..])).unwrap();
    assert_eq!(name_of(&d, c), "Gruppe1");
    // Kennungen werden nie wiederverwendet.
    assert!(c != a && c != b);
    assert_eq!(d.sketch_groups().next, 3);
}

#[test]
fn create_in_common_parent_and_errors() {
    let (mut d, ids) = doc_with(3);
    let g = d.create_sketch_group(&items(&ids[..2])).unwrap();
    // Beide Skizzen liegen in g: die neue Gruppe entsteht in g.
    let n = d.create_sketch_group(&items(&ids[..2])).unwrap();
    let grp = d.sketch_groups().groups.iter().find(|x| x.id == n).unwrap();
    assert_eq!(grp.parent, Some(g));
    // Gemischte Eltern: oben.
    let m = d
        .create_sketch_group(&[GroupItem::Sketch(ids[0]), GroupItem::Sketch(ids[2])])
        .unwrap();
    let grp = d.sketch_groups().groups.iter().find(|x| x.id == m).unwrap();
    assert_eq!(grp.parent, None);
    assert!(d.create_sketch_group(&[]).is_err());
    assert!(d.rename_sketch_group(g, "   ").is_err());
    assert!(d.rename_sketch_group(g, "").is_err());
    assert!(d.rename_sketch_group(SketchGroupId(99), "x").is_err());
    assert!(
        d.create_sketch_group(&[GroupItem::Sketch(FeatureId(999))])
            .is_err()
    );
    d.check_invariants().unwrap();
}

#[test]
fn restore_refuses_broken_groups() {
    let (mut d, ids) = doc_with(2);
    d.create_sketch_group(&items(&ids)).unwrap();
    let good = d.snapshot();
    let mut check = |f: &dyn Fn(&mut Snapshot)| {
        let mut s = good.clone();
        f(&mut s);
        assert!(matches!(
            d.restore(s.clone()),
            Err(FeatureError::InvalidInput(_))
        ));
        assert!(Document::from_snapshot(s, 0.05).is_err());
    };
    // Unbekannte Gruppe, unbekannte Skizze, Kreis, fehlender Elternteil, Mitglied ohne Gruppen,
    // zu kleine nächste Kennung.
    check(&|s| {
        s.sketch_groups.member.insert(ids[0], SketchGroupId(77));
    });
    check(&|s| {
        s.sketch_groups
            .member
            .insert(FeatureId(500), SketchGroupId(0));
    });
    check(&|s| {
        s.sketch_groups.groups[0].parent = Some(SketchGroupId(0));
    });
    check(&|s| {
        s.sketch_groups.groups[0].parent = Some(SketchGroupId(9));
    });
    check(&|s| {
        s.sketch_groups.groups.clear();
    });
    check(&|s| {
        s.sketch_groups.next = 0;
    });
    assert_eq!(d.snapshot().sketch_groups, good.sketch_groups);
}
