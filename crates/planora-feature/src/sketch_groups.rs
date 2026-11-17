//! Skizzengruppen im Browser-Ordner „Skizzen“ (Skizzen 11b, Spec 3): benannte, verschachtelbare
//! Gruppen mit eigenem Auge. Sie gehören zum Dokument ([`crate::Snapshot::sketch_groups`]) und
//! damit zu Datei und Rückgängig; Zeitleiste und Neuberechnung ändern sie nicht.

use crate::error::FeatureError;
use crate::model::{Document, FeatureId, FeatureKind};
use planora_i18n::tr;
use std::collections::BTreeMap;

/// Dauerhafte Kennung einer Skizzengruppe; wird innerhalb eines Dokuments nicht wiederverwendet
/// (siehe [`SketchGroups::next`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SketchGroupId(pub u64);

/// Eine Gruppe: Name, übergeordnete Gruppe (`None` heißt: oben im Ordner „Skizzen“) und Auge
/// (`hidden`: alle Skizzen darin sind ausgeblendet, ihr eigenes Auge bleibt erhalten).
#[derive(Clone, Debug, PartialEq)]
pub struct SketchGroup {
    pub id: SketchGroupId,
    pub name: String,
    pub parent: Option<SketchGroupId>,
    pub hidden: bool,
}

/// Alle Skizzengruppen eines Dokuments: Gruppen in Browser-Reihenfolge, die Gruppe je Skizze
/// (Skizzen ohne Eintrag liegen oben im Ordner) und die nächste zu vergebende Kennung.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SketchGroups {
    pub groups: Vec<SketchGroup>,
    pub member: BTreeMap<FeatureId, SketchGroupId>,
    pub next: u64,
}

impl SketchGroups {
    /// Keine Gruppe und keine Zuordnung (dann braucht die Datei kein Format 22).
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty() && self.member.is_empty()
    }
}

/// Was sich in eine Skizzengruppe legen lässt: eine Skizze oder eine andere Gruppe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupItem {
    Sketch(FeatureId),
    Group(SketchGroupId),
}

impl SketchGroups {
    /// Die Gruppe mit der Kennung `id`.
    pub fn group(&self, id: SketchGroupId) -> Option<&SketchGroup> {
        self.groups.iter().find(|g| g.id == id)
    }

    /// Prüft die Stimmigkeit: Kennungen eindeutig, übergeordnete Gruppen vorhanden und ohne
    /// Kreis, jede Zuordnung zeigt auf eine Skizze (`is_sketch`) und auf eine vorhandene
    /// Gruppe, die nächste Kennung liegt über allen vergebenen. Ein Zustand, der das nicht
    /// erfüllt, darf weder ins Dokument noch in eine Datei (sonst fiele beim Schreiben etwas
    /// still weg oder die Datei ließe sich nicht mehr laden).
    pub fn check(&self, is_sketch: &dyn Fn(FeatureId) -> bool) -> Result<(), FeatureError> {
        let bad = FeatureError::InvalidInput;
        let mut ids = BTreeMap::new();
        for g in &self.groups {
            if ids.insert(g.id, g.parent).is_some() {
                return Err(bad(tr!("feature-sg-twice", id = g.id.0)));
            }
        }
        for g in &self.groups {
            // Nach höchstens so vielen Schritten, wie es Gruppen gibt, muss der Weg oben enden.
            let mut at = g.parent;
            let mut steps = 0;
            while let Some(p) = at {
                match ids.get(&p) {
                    Some(next) if steps < ids.len() && p != g.id => {
                        at = *next;
                        steps += 1;
                    }
                    _ => return Err(bad(tr!("feature-sg-parent", id = g.id.0))),
                }
            }
        }
        for (f, g) in &self.member {
            if !ids.contains_key(g) {
                return Err(bad(tr!("feature-sg-no-group", id = g.0, feature = f.0)));
            }
            if !is_sketch(*f) {
                return Err(bad(tr!("feature-sg-no-sketch", id = g.0, feature = f.0)));
            }
        }
        let min = match ids.keys().next_back() {
            Some(m) => m.0.saturating_add(1),
            None => 0,
        };
        if self.next < min {
            return Err(bad(tr!("feature-sg-next", next = self.next, min = min)));
        }
        Ok(())
    }

    /// Die Gruppe, in der `item` liegt (`None`: oben im Ordner „Skizzen“).
    fn parent_of(&self, item: GroupItem) -> Option<SketchGroupId> {
        match item {
            GroupItem::Sketch(f) => self.member.get(&f).copied(),
            GroupItem::Group(g) => self.group(g).and_then(|x| x.parent),
        }
    }

    /// Liegt `id` in `anc` oder ist `anc` selbst (Weg nach oben; die Gruppen sind kreisfrei).
    fn is_within(&self, id: SketchGroupId, anc: SketchGroupId) -> bool {
        let mut at = Some(id);
        let mut steps = 0;
        while let Some(g) = at {
            if g == anc {
                return true;
            }
            steps += 1;
            if steps > self.groups.len() {
                return false;
            }
            at = self.group(g).and_then(|x| x.parent);
        }
        false
    }

    /// Nimmt die Zuordnung der Skizzen in `gone` weg (Skizze gelöscht); leere Gruppen bleiben.
    pub(crate) fn forget_sketches(&mut self, gone: &[FeatureId]) {
        self.member.retain(|f, _| !gone.contains(f));
    }
}

impl Document {
    fn is_sketch(&self, f: FeatureId) -> bool {
        self.feature(f)
            .is_some_and(|x| matches!(x.kind, FeatureKind::Sketch(_)))
    }

    fn group_exists(&self, id: SketchGroupId) -> Result<(), FeatureError> {
        self.sketch_groups
            .group(id)
            .map(|_| ())
            .ok_or_else(|| FeatureError::InvalidInput(tr!("tree-group-unknown")))
    }

    fn check_item(&self, item: GroupItem) -> Result<(), FeatureError> {
        match item {
            GroupItem::Group(g) => self.group_exists(g),
            GroupItem::Sketch(f) if self.is_sketch(f) => Ok(()),
            GroupItem::Sketch(_) => Err(FeatureError::InvalidInput(tr!("tree-group-not-sketch"))),
        }
    }

    /// Legt eine neue Gruppe an, die `items` enthält. Sie entsteht im gemeinsamen Elternteil
    /// aller `items`, sonst oben im Ordner. Der Name ist „Gruppe“ plus die kleinste freie Zahl.
    /// Zählt als Änderung der Anzeige; rechnet nicht neu.
    pub fn create_sketch_group(
        &mut self,
        items: &[GroupItem],
    ) -> Result<SketchGroupId, FeatureError> {
        let Some(first) = items.first() else {
            return Err(FeatureError::InvalidInput(tr!("tree-group-no-items")));
        };
        for i in items {
            self.check_item(*i)?;
        }
        let parent = self.sketch_groups.parent_of(*first);
        let parent = items
            .iter()
            .all(|i| self.sketch_groups.parent_of(*i) == parent)
            .then_some(parent)
            .flatten();
        let id = SketchGroupId(self.sketch_groups.next);
        let next =
            id.0.checked_add(1)
                .ok_or_else(|| FeatureError::InvalidInput(tr!("tree-group-unknown")))?;
        let mut k = 1usize;
        let name = loop {
            let n = tr!("tree-group-name", n = k);
            if self.sketch_groups.groups.iter().all(|g| g.name != n) {
                break n;
            }
            k += 1;
        };
        let mut g = self.sketch_groups.clone();
        g.next = next;
        g.groups.push(SketchGroup {
            id,
            name,
            parent,
            hidden: false,
        });
        for i in items {
            match i {
                GroupItem::Sketch(f) => {
                    g.member.insert(*f, id);
                }
                GroupItem::Group(c) => {
                    if let Some(x) = g.groups.iter_mut().find(|x| x.id == *c) {
                        x.parent = Some(id);
                    }
                }
            }
        }
        // Erst prüfen, dann übernehmen: bei Fehler bleibt alles, wie es war.
        g.check(&|f| self.is_sketch(f))?;
        self.sketch_groups = g;
        self.drawing_edits += 1;
        Ok(id)
    }

    /// Benennt eine Gruppe um. Leer oder nur Leerzeichen ist ein Fehler.
    pub fn rename_sketch_group(
        &mut self,
        id: SketchGroupId,
        name: &str,
    ) -> Result<(), FeatureError> {
        self.group_exists(id)?;
        let name = name.trim();
        if name.is_empty() {
            return Err(FeatureError::InvalidInput(tr!("tree-rename-empty")));
        }
        if let Some(g) = self.sketch_groups.groups.iter_mut().find(|g| g.id == id)
            && g.name != name
        {
            g.name = name.to_string();
            self.drawing_edits += 1;
        }
        Ok(())
    }

    /// Löst eine Gruppe auf: Skizzen und Untergruppen rücken eine Ebene höher, die Gruppe fällt
    /// weg. Ihre Kennung wird nicht wieder vergeben. Dasselbe ist „Gruppe löschen“.
    pub fn ungroup_sketch_group(&mut self, id: SketchGroupId) -> Result<(), FeatureError> {
        self.group_exists(id)?;
        let up = self.sketch_groups.group(id).and_then(|g| g.parent);
        let g = &mut self.sketch_groups;
        for x in g.groups.iter_mut().filter(|x| x.parent == Some(id)) {
            x.parent = up;
        }
        let moved: Vec<FeatureId> = g
            .member
            .iter()
            .filter(|(_, m)| **m == id)
            .map(|(f, _)| *f)
            .collect();
        for f in moved {
            match up {
                Some(u) => g.member.insert(f, u),
                None => g.member.remove(&f),
            };
        }
        g.groups.retain(|x| x.id != id);
        self.drawing_edits += 1;
        Ok(())
    }

    /// Legt eine Skizze oder Gruppe in `target` (`None`: oben im Ordner). Eine Gruppe in sich
    /// selbst oder in einen Nachfahren ist ein Fehler und ändert nichts.
    pub fn move_to_group(
        &mut self,
        item: GroupItem,
        target: Option<SketchGroupId>,
    ) -> Result<(), FeatureError> {
        self.check_item(item)?;
        if let Some(t) = target {
            self.group_exists(t)?;
        }
        if self.sketch_groups.parent_of(item) == target {
            return Ok(());
        }
        match item {
            GroupItem::Sketch(f) => match target {
                Some(t) => {
                    self.sketch_groups.member.insert(f, t);
                }
                None => {
                    self.sketch_groups.member.remove(&f);
                }
            },
            GroupItem::Group(c) => {
                if let Some(t) = target
                    && self.sketch_groups.is_within(t, c)
                {
                    return Err(FeatureError::InvalidInput(tr!("tree-group-cycle")));
                }
                if let Some(x) = self.sketch_groups.groups.iter_mut().find(|x| x.id == c) {
                    x.parent = target;
                }
            }
        }
        self.drawing_edits += 1;
        Ok(())
    }

    /// Schaltet das Auge einer Gruppe. Das Auge der Skizzen darin bleibt, wie es ist. Eine
    /// unbekannte Gruppe oder derselbe Zustand ändert nichts.
    pub fn set_sketch_group_hidden(&mut self, id: SketchGroupId, hidden: bool) {
        if let Some(g) = self.sketch_groups.groups.iter_mut().find(|g| g.id == id)
            && g.hidden != hidden
        {
            g.hidden = hidden;
            self.drawing_edits += 1;
        }
    }

    /// Ist die Skizze durch eine Gruppe auf dem Weg nach oben ausgeblendet? Das eigene Auge der
    /// Skizze zählt hier nicht.
    pub fn sketch_hidden_by_group(&self, id: FeatureId) -> bool {
        let g = &self.sketch_groups;
        let mut at = g.member.get(&id).copied();
        let mut steps = 0;
        while let Some(gid) = at {
            let Some(x) = g.group(gid) else {
                return false;
            };
            if x.hidden {
                return true;
            }
            steps += 1;
            if steps > g.groups.len() {
                return false;
            }
            at = x.parent;
        }
        false
    }

    /// Prüft die Skizzengruppen eines Bildes vor dem Übernehmen (Laden und Rückgängig).
    pub(crate) fn check_snapshot_groups(s: &crate::Snapshot) -> Result<(), FeatureError> {
        s.sketch_groups.check(&|f| {
            s.features
                .iter()
                .any(|x| x.id == f && matches!(x.kind, FeatureKind::Sketch(_)))
        })
    }
}
