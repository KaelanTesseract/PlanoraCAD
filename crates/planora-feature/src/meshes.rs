//! Netz-Körper (Teilprojekt 9, Plan P10, P11): Netztabelle des Dokuments, sichtbare
//! Netz-Körper und Anlegen beim Import (ganz oder gar nicht).

use crate::compute::{check_mesh_extent, check_parameters};
use crate::error::FeatureError;
use crate::model::{BodyRef, Document, FeatureId, FeatureKind, FeatureStatus, MeshId};
use planora_exchange::{ExchangeMesh, MeshTransform};
use planora_i18n::tr;
use std::collections::BTreeSet;
use std::sync::Arc;

/// Ein sichtbarer Netz-Körper mit Netz, Lage und Anzeigename.
#[derive(Clone, Debug)]
pub struct VisibleMesh<'a> {
    pub body: BodyRef,
    pub mesh_id: MeshId,
    pub mesh: &'a Arc<ExchangeMesh>,
    pub transform: MeshTransform,
    pub name: String,
}

impl Document {
    /// Legt ein geprüftes Netz in die Tabelle (ohne Feature; zählt nicht als Änderung, nicht
    /// benutzte Netze werden nicht gespeichert).
    pub fn add_mesh(&mut self, mesh: Arc<ExchangeMesh>) -> Result<MeshId, FeatureError> {
        mesh.validate()
            .map_err(|e| FeatureError::InvalidInput(e.to_string()))?;
        let id = MeshId(self.next_mesh);
        self.next_mesh = self
            .next_mesh
            .checked_add(1)
            .ok_or_else(|| FeatureError::InvalidInput(tr!("feature-mesh-ids-used")))?;
        self.meshes.insert(id, mesh);
        Ok(id)
    }

    pub fn mesh(&self, id: MeshId) -> Option<&Arc<ExchangeMesh>> {
        self.meshes.get(&id)
    }

    pub fn mesh_count(&self) -> usize {
        self.meshes.len()
    }

    /// Netz und Lage des Netz-Körpers `id` (auch wenn er unterdrückt oder fehlerhaft ist).
    pub fn mesh_body(&self, id: FeatureId) -> Option<(&Arc<ExchangeMesh>, MeshTransform)> {
        let kind = &self.feature(id)?.kind;
        let FeatureKind::MeshBody { mesh, .. } = kind else {
            return None;
        };
        Some((self.meshes.get(mesh)?, kind.mesh_transform()?))
    }

    /// Legt Netz und Netz-Körper am Marker an, nur wenn beides gelingt (Spec §5: Import ändert
    /// das Dokument erst nach vollständigem Erfolg). Sonst bleibt das Dokument unverändert, auch
    /// Kennungszähler und Änderungszahl.
    pub fn add_mesh_body(
        &mut self,
        name: impl Into<String>,
        mesh: Arc<ExchangeMesh>,
        t: MeshTransform,
    ) -> Result<FeatureId, FeatureError> {
        mesh.validate()
            .map_err(|e| FeatureError::InvalidInput(e.to_string()))?;
        let probe = FeatureKind::MeshBody {
            mesh: MeshId(0),
            offset: t.offset,
            rotation: t.rotation,
            scale: t.scale,
        };
        check_parameters(&probe)?;
        check_mesh_extent(&mesh, &t)?;
        let saved = (self.next_id, self.next_mesh, self.edits, self.marker);
        let mesh_id = self.add_mesh(mesh)?;
        let kind = FeatureKind::MeshBody {
            mesh: mesh_id,
            offset: t.offset,
            rotation: t.rotation,
            scale: t.scale,
        };
        let id = match self.add_feature(name, kind) {
            Ok(id) => id,
            Err(e) => {
                self.meshes.remove(&mesh_id);
                self.next_mesh = saved.1;
                return Err(e);
            }
        };
        match self.feature(id).map(|f| f.status.clone()) {
            Some(FeatureStatus::Ok) => Ok(id),
            status => {
                self.features.retain(|f| f.id != id);
                self.meshes.remove(&mesh_id);
                (self.next_id, self.next_mesh, self.edits, self.marker) = saved;
                self.recompute();
                let message = match status {
                    Some(FeatureStatus::Error(m) | FeatureStatus::Skipped(m)) => m,
                    _ => tr!("feature-mesh-not-created"),
                };
                Err(FeatureError::InvalidInput(message))
            }
        }
    }

    /// Sichtbare Netz-Körper (berechnet, nicht unterdrückt) in der Reihenfolge der Zeitleiste.
    pub fn visible_meshes(&self) -> Vec<VisibleMesh<'_>> {
        self.features
            .iter()
            .filter(|f| f.status == FeatureStatus::Ok && f.result.is_some())
            .filter_map(|f| {
                let FeatureKind::MeshBody { mesh, .. } = &f.kind else {
                    return None;
                };
                Some(VisibleMesh {
                    body: BodyRef {
                        feature: f.id,
                        index: 0,
                    },
                    mesh_id: *mesh,
                    mesh: self.meshes.get(mesh)?,
                    transform: f.kind.mesh_transform()?,
                    name: f.name.clone(),
                })
            })
            .collect()
    }

    /// Netze, auf die ein Feature verweist.
    pub(crate) fn used_meshes(&self) -> BTreeSet<MeshId> {
        self.features
            .iter()
            .filter_map(|f| match &f.kind {
                FeatureKind::MeshBody { mesh, .. } => Some(*mesh),
                _ => None,
            })
            .collect()
    }

    /// Entfernt Netze, auf die kein Feature mehr verweist.
    pub(crate) fn prune_meshes(&mut self) {
        let used = self.used_meshes();
        self.meshes.retain(|id, _| used.contains(id));
    }
}
