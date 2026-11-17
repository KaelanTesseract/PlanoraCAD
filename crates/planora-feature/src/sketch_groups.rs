//! Skizzengruppen im Browser-Ordner „Skizzen“ (Skizzen 11b, Spec 3): benannte, verschachtelbare
//! Gruppen mit eigenem Auge. Sie gehören zum Dokument ([`crate::Snapshot::sketch_groups`]) und
//! damit zu Datei und Rückgängig; Zeitleiste und Neuberechnung ändern sie nicht.

use crate::model::FeatureId;
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
