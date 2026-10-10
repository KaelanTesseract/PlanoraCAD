use crate::sketch_state::SketchSession;
use glam::{Vec2, Vec3};
use planora_feature::{
    BodyRef, Document, FeatureError, FeatureId, FeatureKind, FeatureStatus, SketchFeature,
    SketchPlane,
};
use planora_kernel::boolean::BoolOp;
use planora_kernel::{AnalyticKernel, Face, Id, Profile, Solid};
use planora_math::{DVec2, DVec3, Frame};
use planora_mesh::{MassProperties, Mesh, TessellationOptions, mass_properties, tessellate};
use planora_render::{
    EdgeRef, FaceRef, Highlight, OrbitCamera, Palette, Pick, SceneData, build_scene, pick_edge,
    pick_face,
};
use planora_sketch::model::{CurveId, Sketch};
use planora_sketch::{ExtractedProfiles, plane_of_face};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    Box,
    Cylinder,
    Sphere,
    Cone,
    Extrusion,
    Revolution,
    /// Ergebnis einer Booleschen Operation (oder Art unbekannt).
    Combined,
}

/// Grundebenen für neue Skizzen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandardPlane {
    XY,
    XZ,
    YZ,
}

/// Sichtbarer Körper, abgeleitet aus dem Feature-Dokument (nie direkt verändert).
pub struct Body {
    pub name: String,
    pub kind: BodyKind,
    /// Verweis auf den Körper im Dokument.
    pub source: BodyRef,
    pub solid: Solid,
    pub mesh: Mesh,
    pub mass: MassProperties,
    /// Achsenparallele Hülle `(min, max)` in Millimetern.
    pub bounds: (DVec3, DVec3),
}

/// Wunsch aus dem Browser (Kontextmenü einer Skizze), einen Dialog zu öffnen; die App setzt ihn
/// im selben Bild in den passenden Dialog um.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SketchRequest {
    /// „Skizze extrudieren“ für die Skizze mit diesem Index.
    Extrude(usize),
    /// „Skizze drehen“ für die Skizze mit diesem Index.
    Revolve(usize),
}

/// Alles, was die Oberfläche anzeigt. Enthält keine egui-Typen und ist deshalb testbar.
pub struct AppState {
    /// Das Feature-Dokument; Quelle aller Körper.
    pub document: Document,
    /// Sichtbare Körper des Dokuments, abgeleitet mit [`AppState::refresh`].
    pub bodies: Vec<Body>,
    pub selected_body: Option<usize>,
    pub selection: Option<Pick>,
    pub hover: Option<Pick>,
    pub camera: OrbitCamera,
    pub scene: Arc<SceneData>,
    pub scene_version: u64,
    pub dark: bool,
    pub kernel: AnalyticKernel,
    pub chord_tolerance: f64,
    /// Letzte Fehlermeldung für die Statuszeile.
    pub message: Option<String>,
    /// Laufende Nummer je Bezeichnung für neue Features („Quader 1“, „Quader 2“, …); zählt
    /// nur hoch, damit Namen auch nach dem Löschen eindeutig bleiben.
    name_counters: BTreeMap<String, u64>,
    /// Skizzen des Dokuments.
    pub sketches: Vec<Sketch>,
    /// Im Browser gewählte Skizze.
    pub selected_sketch: Option<usize>,
    /// Geöffneter Skizzen-Modus.
    pub sketch_session: Option<SketchSession>,
    /// Kamera vor dem Öffnen des Skizzen-Modus.
    saved_camera: Option<OrbitCamera>,
    /// Laufende Nummer für den Namen der nächsten Skizze; zählt nur hoch, damit Namen auch nach
    /// dem Löschen eindeutig bleiben.
    pub next_sketch_number: u64,
    /// Offener Dialogwunsch aus dem Browser (siehe [`SketchRequest`]).
    pub sketch_request: Option<SketchRequest>,
    /// Zuletzt berechnete Profile (für die Dialoge, die jedes Bild neu zeichnen).
    pub profile_cache: ProfileCache,
}

/// Merkt sich die Profile der zuletzt gefragten Skizze zu ihrem Stand
/// ([`Sketch::revision`], über alle Skizzen eindeutig). Ändert sich die Skizze, ändert sich der
/// Stand, und die Profile werden neu berechnet.
#[derive(Debug, Default)]
pub struct ProfileCache {
    entry: RefCell<Option<(u64, Arc<ExtractedProfiles>)>>,
    computed: Cell<usize>,
}

impl ProfileCache {
    /// Die Profile von `sk`, aus dem Speicher, wenn sich die Skizze seither nicht geändert hat.
    pub fn get(&self, sk: &Sketch) -> Arc<ExtractedProfiles> {
        let mut entry = self.entry.borrow_mut();
        if let Some((rev, p)) = entry.as_ref()
            && *rev == sk.revision()
        {
            return Arc::clone(p);
        }
        let p = Arc::new(sk.profiles());
        self.computed.set(self.computed.get() + 1);
        *entry = Some((sk.revision(), Arc::clone(&p)));
        p
    }

    /// Wie oft bisher tatsächlich berechnet wurde (für Tests).
    pub fn computations(&self) -> usize {
        self.computed.get()
    }
}

const GRID_HALF_EXTENT: f32 = 500.0;
const PICK_EDGE_PX: f32 = 6.0;
const BODY_GAP_MM: f64 = 10.0;
/// Mindestgröße des Bereichs, auf den die Ansicht beim Öffnen einer Skizze eingepasst wird.
const MIN_SKETCH_VIEW_MM: f64 = 10.0;

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        let chord_tolerance = 0.05;
        let mut state = Self {
            document: Document::with_options(chord_tolerance),
            bodies: Vec::new(),
            selected_body: None,
            selection: None,
            hover: None,
            camera: OrbitCamera::default(),
            scene: Arc::new(SceneData::default()),
            scene_version: 0,
            dark: true,
            kernel: AnalyticKernel::default(),
            chord_tolerance,
            message: None,
            name_counters: BTreeMap::new(),
            sketches: Vec::new(),
            selected_sketch: None,
            sketch_session: None,
            saved_camera: None,
            next_sketch_number: 1,
            sketch_request: None,
            profile_cache: ProfileCache::default(),
        };
        state.rebuild_scene();
        state
    }

    pub fn palette(&self) -> Palette {
        if self.dark {
            Palette::dark()
        } else {
            Palette::light()
        }
    }

    /// Palette der Szene: im Skizzen-Modus gedämpft.
    fn scene_palette(&self) -> Palette {
        if self.sketch_session.is_some() {
            self.palette().dimmed()
        } else {
            self.palette()
        }
    }

    /// Baut Körperliste, Netze, Maße und Szene aus den sichtbaren Körpern des Dokuments neu
    /// auf. Auswahl, Hervorhebung und gewählter Körper bleiben, solange ihr Körper sichtbar ist
    /// (Indizes werden auf die neue Liste abgebildet); sonst werden sie gelöscht. Die Ansicht
    /// wird nur neu eingepasst, wenn sich die Zahl der Körper geändert hat.
    pub fn refresh(&mut self) {
        self.refresh_following(None);
    }

    /// Wie [`AppState::refresh`]; Verweise auf den Körper `follow.0` gehen auf `follow.1` über
    /// (z. B. auf das Ergebnis eines neuen Verschiebens).
    fn refresh_following(&mut self, follow: Option<(BodyRef, BodyRef)>) {
        let old: Vec<BodyRef> = self.bodies.iter().map(|b| b.source).collect();
        let count_before = self.bodies.len();
        let mut bodies = Vec::new();
        let mut failed = Vec::new();
        for v in self.document.visible_bodies() {
            let name = body_name(&self.document, v.body).unwrap_or_else(|| v.name.clone());
            let kind = body_kind(&self.document, v.body);
            match self.make_body(kind, name.clone(), v.body, v.solid.clone()) {
                Ok(b) => bodies.push(b),
                Err(e) => failed.push(format!("„{name}“: {e}")),
            }
        }
        self.bodies = bodies;
        let new: Vec<BodyRef> = self.bodies.iter().map(|b| b.source).collect();
        let map = |i: usize| -> Option<usize> {
            let mut r = *old.get(i)?;
            if let Some((from, to)) = follow
                && r == from
            {
                r = to;
            }
            new.iter().position(|n| *n == r)
        };
        self.selected_body = self.selected_body.and_then(map);
        self.selection = self.selection.and_then(|p| self.remap_pick(p, map));
        self.hover = self.hover.and_then(|p| self.remap_pick(p, map));
        if !failed.is_empty() {
            self.message = Some(failed.join("; "));
        }
        self.rebuild_scene();
        if self.bodies.len() != count_before {
            self.frame_all();
        }
    }

    /// Bildet eine Auswahl auf den neuen Körperindex ab; gibt es die Fläche oder Kante im
    /// neuen Körper nicht mehr, entfällt sie.
    fn remap_pick(&self, pick: Pick, map: impl Fn(usize) -> Option<usize>) -> Option<Pick> {
        match pick {
            Pick::Face(FaceRef { body, face }) => {
                let body = map(body)?;
                self.bodies[body].solid.faces.get(face)?;
                Some(Pick::Face(FaceRef { body, face }))
            }
            Pick::Edge(EdgeRef { body, edge }) => {
                let body = map(body)?;
                self.bodies[body].solid.edges.get(edge)?;
                Some(Pick::Edge(EdgeRef { body, edge }))
            }
        }
    }

    /// Index des Körpers `r` in der Körperliste, falls er sichtbar ist.
    pub fn body_index(&self, r: BodyRef) -> Option<usize> {
        self.bodies.iter().position(|b| b.source == r)
    }

    /// Nächster Name `{Bezeichnung} {Nummer}` für `label`, ohne den Zähler zu erhöhen.
    fn next_name(&self, label: &str) -> String {
        let n = self.name_counters.get(label).copied().unwrap_or(0) + 1;
        format!("{label} {n}")
    }

    /// Zählt den Namenszähler für `label` weiter (nur nach erfolgreichem Anlegen).
    fn count_name(&mut self, label: &str) {
        *self.name_counters.entry(label.to_string()).or_insert(0) += 1;
    }

    /// Legt ein Feature `{Bezeichnung} {Nummer}` am Marker an (Bezeichnung `base_name` oder
    /// die der Feature-Art; der Zähler je Bezeichnung zählt nur hoch) und aktualisiert die
    /// Körper. Ein Feature mit dem Status `Error` wird trotzdem angelegt; seine Meldung steht
    /// dann in `message`.
    pub fn add_feature(
        &mut self,
        base_name: Option<&str>,
        kind: FeatureKind,
    ) -> Result<FeatureId, String> {
        let label = base_name.unwrap_or(kind.label()).to_string();
        let name = self.next_name(&label);
        match self.document.add_feature(name, kind) {
            Ok(id) => {
                self.count_name(&label);
                self.refresh();
                if let Some(FeatureStatus::Error(e)) = self.document.feature(id).map(|f| &f.status)
                {
                    self.message = Some(e.clone());
                }
                Ok(id)
            }
            Err(e) => self.fail(e.to_string()),
        }
    }

    /// Ursprung für einen neuen Grundkörper rechts neben den vorhandenen Körpern (10 mm
    /// Abstand). `extent_x` ist der Abstand vom Ursprung des Grundkörpers zu seiner linken
    /// Kante (Quader 0, sonst der Radius). Ohne Körper liegt er im Nullpunkt.
    pub fn free_origin(&self, extent_x: f64) -> DVec3 {
        match self.bodies.iter().map(|b| b.bounds.1.x).reduce(f64::max) {
            None => DVec3::ZERO,
            Some(max_x) => DVec3::new(max_x + BODY_GAP_MM + extent_x, 0.0, 0.0),
        }
    }

    /// Prüft die Maße eines Grundkörpers (endlich und größer als die Toleranz).
    fn check_sizes(&mut self, values: &[f64]) -> Result<(), String> {
        let min = self.kernel.tolerance.linear;
        if values.iter().all(|v| v.is_finite() && *v > min) {
            Ok(())
        } else {
            self.fail("Erzeugen fehlgeschlagen: Die Maße müssen größer als null sein".into())
        }
    }

    /// Quader neben den vorhandenen Körpern.
    pub fn create_box(&mut self, size: DVec3) -> Result<FeatureId, String> {
        self.check_sizes(&size.to_array())?;
        let origin = self.free_origin(0.0);
        self.add_feature(None, FeatureKind::Box { size, origin })
    }

    /// Zylinder neben den vorhandenen Körpern.
    pub fn create_cylinder(&mut self, radius: f64, height: f64) -> Result<FeatureId, String> {
        self.check_sizes(&[radius, height])?;
        let origin = self.free_origin(radius);
        self.add_feature(
            None,
            FeatureKind::Cylinder {
                radius,
                height,
                origin,
            },
        )
    }

    /// Kugel neben den vorhandenen Körpern (linke Kante am freien Rand).
    pub fn create_sphere(&mut self, radius: f64) -> Result<FeatureId, String> {
        self.check_sizes(&[radius])?;
        let origin = self.free_origin(radius);
        self.add_feature(None, FeatureKind::Sphere { radius, origin })
    }

    /// Kegel neben den vorhandenen Körpern.
    pub fn create_cone(&mut self, radius: f64, height: f64) -> Result<FeatureId, String> {
        self.check_sizes(&[radius, height])?;
        let origin = self.free_origin(radius);
        self.add_feature(
            None,
            FeatureKind::Cone {
                radius,
                height,
                origin,
            },
        )
    }

    /// Legt eine Boolesche Operation `a op b` (Körperindizes) an. Das Ergebnis ersetzt beide
    /// Körper an der Stelle des vorderen und wird gewählt. Schlägt die Operation fehl, wird das
    /// Feature wieder entfernt: Dokument und Körper bleiben unverändert, die Meldung steht in
    /// `message`.
    pub fn add_boolean(&mut self, a: usize, b: usize, op: BoolOp) -> Result<FeatureId, String> {
        if a == b || a >= self.bodies.len() || b >= self.bodies.len() {
            return self.fail("Bitte zwei verschiedene Körper wählen".into());
        }
        let label = match op {
            BoolOp::Union => "Vereinigung",
            BoolOp::Difference => "Abzug",
            BoolOp::Intersection => "Schnitt",
        };
        let kind = FeatureKind::Boolean {
            target: self.bodies[a].source,
            tool: self.bodies[b].source,
            op,
        };
        let id = self.add_checked(label, kind)?;
        self.hover = None;
        self.selection = None;
        self.refresh();
        self.selected_body = self.body_index(BodyRef {
            feature: id,
            index: 0,
        });
        Ok(id)
    }

    /// Legt ein Feature an und nimmt es wieder heraus, wenn es nicht berechnet werden konnte
    /// (die Meldung steht dann in `message`). Aktualisiert die Körper nicht.
    fn add_checked(&mut self, label: &str, kind: FeatureKind) -> Result<FeatureId, String> {
        let name = self.next_name(label);
        let id = match self.document.add_feature(name, kind) {
            Ok(id) => id,
            Err(e) => return self.fail(e.to_string()),
        };
        match self.document.feature(id).map(|f| f.status.clone()) {
            Some(FeatureStatus::Ok) => {
                self.count_name(label);
                Ok(id)
            }
            status => {
                let _ = self.document.delete_with_dependents(id);
                self.fail(status_message(status))
            }
        }
    }

    /// Verschiebt den Körper `index` um `delta` (Millimeter) **zusätzlich** zur derzeitigen
    /// Lage (Entscheidung F9). Ist der Körper das Ergebnis eines Verschiebens und liest ihn
    /// danach kein anderes Feature (siehe [`AppState::read_later`]), wird dessen Verschiebung
    /// erhöht; sonst entsteht ein neues Verschieben am Marker. Auswahl
    /// und gewählter Körper gehen auf das Ergebnis über. Bei einem Fehler bleibt alles
    /// unverändert.
    pub fn move_body(&mut self, index: usize, delta: DVec3) -> Result<(), String> {
        if !delta.is_finite() {
            return self.fail("Ungültige Verschiebung".into());
        }
        let Some(r) = self.bodies.get(index).map(|b| b.source) else {
            return self.fail("Körper nicht gefunden".into());
        };
        if delta == DVec3::ZERO {
            return Ok(());
        }
        let extendable = r.index == 0
            && self
                .document
                .feature(r.feature)
                .is_some_and(|f| matches!(f.kind, FeatureKind::Move { .. }))
            && !self.read_later(index);
        let moved = if extendable {
            self.extend_move(r.feature, delta)?;
            r
        } else {
            let id = self.add_checked("Verschieben", FeatureKind::Move { body: r, delta })?;
            BodyRef {
                feature: id,
                index: 0,
            }
        };
        self.refresh_following(Some((r, moved)));
        Ok(())
    }

    /// Liest ein Feature nach dem erzeugenden Feature des Körpers `index` (und vor dem
    /// Marker) diesen Körper? Gelesen wird über einen Verweis auf das erzeugende Feature selbst
    /// oder über die Flächenmarken des Körpers: Skizzen auf Flächen und Kantenkopien verweisen
    /// auf die Herkunft der Marke (etwa den Quader), nicht auf das Verschieben.
    fn read_later(&self, index: usize) -> bool {
        let Some(body) = self.bodies.get(index) else {
            return false;
        };
        let mut origins: Vec<FeatureId> = body
            .solid
            .faces
            .iter()
            .filter_map(|(_, f)| f.tag.as_ref().map(|t| FeatureId(t.origin)))
            .collect();
        origins.push(body.source.feature);
        let Some(start) = self.document.index_of(body.source.feature) else {
            return false;
        };
        let end = self.document.marker();
        self.document.features()[start + 1..end.max(start + 1)]
            .iter()
            .any(|f| f.kind.dependencies().iter().any(|d| origins.contains(d)))
    }

    /// Erhöht die Verschiebung des Features `id` um `delta`. Ist das Verschieben danach nicht
    /// berechenbar oder schlägt dadurch ein späteres, bisher fehlerfreies Feature fehl, wird
    /// die alte Verschiebung zurückgeschrieben: Das Dokument bleibt dann unverändert.
    fn extend_move(&mut self, id: FeatureId, delta: DVec3) -> Result<(), String> {
        let old = match self.document.feature(id).map(|f| &f.kind) {
            Some(FeatureKind::Move { delta, .. }) => *delta,
            _ => return self.fail("Das Verschieben gibt es nicht mehr".into()),
        };
        let was_ok: Vec<FeatureId> = self
            .document
            .features()
            .iter()
            .filter(|f| f.status == FeatureStatus::Ok)
            .map(|f| f.id)
            .collect();
        let set = |k: &mut FeatureKind, to: DVec3| {
            if let FeatureKind::Move { delta, .. } = k {
                *delta = to;
            }
        };
        if let Err(e) = self.document.update(id, |k| set(k, old + delta)) {
            return self.fail(e.to_string());
        }
        let broken = self
            .document
            .features()
            .iter()
            .find(|f| was_ok.contains(&f.id) && f.status != FeatureStatus::Ok)
            .map(|f| (f.id, f.status.clone()));
        match broken {
            None => Ok(()),
            Some((broken_id, status)) => {
                let _ = self.document.update(id, |k| set(k, old));
                let message = status_message(Some(status));
                if broken_id == id {
                    self.fail(message)
                } else {
                    let name = self
                        .document
                        .feature(broken_id)
                        .map_or_else(String::new, |f| f.name.clone());
                    self.fail(format!(
                        "Verschieben verworfen: Feature „{name}“ ließe sich danach nicht mehr \
                         berechnen ({message})"
                    ))
                }
            }
        }
    }

    /// Das Feature, das den Namen des Körpers `index` trägt: das erzeugende Feature, bei
    /// verschobenen Körpern das Feature, das den Körper ursprünglich erzeugt hat.
    pub fn naming_feature(&self, index: usize) -> Option<FeatureId> {
        let r = self.bodies.get(index)?.source;
        Some(origin_of(&self.document, r).feature)
    }

    /// Benennt ein Feature um (leere Namen sind ungültig) und aktualisiert die Körpernamen.
    pub fn rename_feature(&mut self, id: FeatureId, name: &str) -> Result<(), String> {
        match self.document.rename(id, name) {
            Ok(()) => {
                for i in 0..self.bodies.len() {
                    if let Some(n) = body_name(&self.document, self.bodies[i].source) {
                        self.bodies[i].name = n;
                    }
                }
                Ok(())
            }
            Err(e) => self.fail(e.to_string()),
        }
    }

    /// Löscht ein Feature, von dem nichts abhängt. Hängen andere Features davon ab, bleibt
    /// alles unverändert und der Fehlertext nennt die abhängigen Features.
    pub fn delete_feature(&mut self, id: FeatureId) -> Result<(), String> {
        match self.document.delete(id) {
            Ok(()) => {
                self.refresh();
                Ok(())
            }
            Err(FeatureError::HasDependents(ids)) => {
                let name = self
                    .document
                    .feature(id)
                    .map_or_else(String::new, |f| f.name.clone());
                let names: Vec<String> = ids
                    .iter()
                    .filter_map(|d| self.document.feature(FeatureId(*d)))
                    .map(|f| format!("„{}“", f.name))
                    .collect();
                self.fail(format!(
                    "„{name}“ lässt sich nicht löschen, weil diese Features darauf aufbauen: {}",
                    names.join(", ")
                ))
            }
            Err(e) => self.fail(e.to_string()),
        }
    }

    /// Löscht ein Feature samt allen abhängigen Features; liefert die entfernten Ids.
    pub fn delete_feature_with_dependents(
        &mut self,
        id: FeatureId,
    ) -> Result<Vec<FeatureId>, String> {
        match self.document.delete_with_dependents(id) {
            Ok(removed) => {
                self.refresh();
                Ok(removed)
            }
            Err(e) => self.fail(e.to_string()),
        }
    }

    /// „Körper löschen“ im Eigenschaftenfenster: löscht das Feature, das den Körper erzeugt
    /// hat, samt seiner Verschiebungen. Bauen andere Features darauf auf, bleibt alles
    /// unverändert und die Meldung nennt sie (die Rückfrage folgt mit dem Feature-Baum).
    pub fn delete_body(&mut self, index: usize) -> Result<(), String> {
        let Some(id) = self.naming_feature(index) else {
            return self.fail("Körper nicht gefunden".into());
        };
        let only_moves = self.document.dependents(id).iter().all(|d| {
            self.document
                .feature(*d)
                .is_some_and(|f| matches!(f.kind, FeatureKind::Move { .. }))
        });
        if only_moves {
            self.delete_feature_with_dependents(id).map(|_| ())
        } else {
            self.delete_feature(id)
        }
    }

    /// Netz, Masse und Hülle eines sichtbaren Körpers berechnen.
    fn make_body(
        &self,
        kind: BodyKind,
        name: String,
        source: BodyRef,
        solid: Solid,
    ) -> Result<Body, String> {
        let mesh = self.mesh_of(&solid)?;
        let mass = mass_properties(&mesh);
        let bounds = bounds_of(&mesh);
        Ok(Body {
            name,
            kind,
            source,
            solid,
            mesh,
            mass,
            bounds,
        })
    }

    fn mesh_of(&self, solid: &Solid) -> Result<Mesh, String> {
        tessellate(
            solid,
            &TessellationOptions {
                chord_tolerance: self.chord_tolerance,
            },
        )
        .map_err(|e| format!("Tessellierung fehlgeschlagen: {e}"))
    }

    pub fn rebuild_scene(&mut self) {
        let meshes: Vec<&Mesh> = self.bodies.iter().map(|b| &b.mesh).collect();
        let highlight = Highlight {
            hover: self.hover,
            selected: self.selection,
        };
        self.scene = Arc::new(build_scene(
            &meshes,
            &highlight,
            &self.scene_palette(),
            GRID_HALF_EXTENT,
        ));
        self.scene_version += 1;
    }

    /// Richtet die Kamera auf alle Körper aus (ohne Körper: Standardansicht auf den Ursprung).
    pub fn frame_all(&mut self) {
        let mut iter = self.bodies.iter().map(|b| b.bounds);
        let Some(first) = iter.next() else {
            self.camera
                .frame_bounds(Vec3::splat(-25.0), Vec3::splat(25.0));
            return;
        };
        let (min, max) = iter.fold(first, |(lo, hi), (a, b)| (lo.min(a), hi.max(b)));
        self.camera.frame_bounds(
            Vec3::new(min.x as f32, min.y as f32, min.z as f32),
            Vec3::new(max.x as f32, max.y as f32, max.z as f32),
        );
    }

    pub fn set_hover(&mut self, hover: Option<Pick>) {
        if self.hover != hover {
            self.hover = hover;
            self.rebuild_scene();
        }
    }

    pub fn set_selection(&mut self, selection: Option<Pick>) {
        if self.selection != selection {
            self.selection = selection;
            if selection.is_some() {
                self.selected_sketch = None;
            }
            self.selected_body = match selection {
                Some(Pick::Face(FaceRef { body, .. })) | Some(Pick::Edge(EdgeRef { body, .. })) => {
                    Some(body)
                }
                None => self.selected_body,
            };
            self.rebuild_scene();
        }
    }

    /// Was liegt unter dem Mauszeiger? `cursor` in Pixeln (Ursprung oben links) im Viewport der Größe `viewport`.
    /// Kanten haben Vorrang, wenn sie nahe genug und nicht verdeckt sind.
    pub fn pick(&self, cursor: Vec2, viewport: Vec2) -> Option<Pick> {
        if viewport.x < 1.0 || viewport.y < 1.0 {
            return None;
        }
        let ndc = Vec2::new(
            2.0 * cursor.x / viewport.x - 1.0,
            1.0 - 2.0 * cursor.y / viewport.y,
        );
        let (origin, dir) = self.camera.ray(ndc);

        let mut nearest_face: Option<(usize, planora_render::FaceHit)> = None;
        for (body, b) in self.bodies.iter().enumerate() {
            if let Some(hit) = pick_face(&b.mesh, origin, dir)
                && nearest_face.is_none_or(|(_, h)| hit.distance < h.distance)
            {
                nearest_face = Some((body, hit));
            }
        }
        let occluder = nearest_face.map(|(_, h)| h.distance);

        let mut best_edge: Option<(usize, planora_render::EdgeHit)> = None;
        for (body, b) in self.bodies.iter().enumerate() {
            if let Some(hit) = pick_edge(
                &b.mesh,
                &self.camera,
                viewport,
                cursor,
                PICK_EDGE_PX,
                occluder,
            ) && best_edge.is_none_or(|(_, h)| hit.pixel_distance < h.pixel_distance)
            {
                best_edge = Some((body, hit));
            }
        }
        if let Some((body, hit)) = best_edge {
            return Some(Pick::Edge(EdgeRef {
                body,
                edge: hit.edge,
            }));
        }
        nearest_face.map(|(body, hit)| {
            Pick::Face(FaceRef {
                body,
                face: hit.face,
            })
        })
    }
}

fn to_vec3(v: DVec3) -> Vec3 {
    Vec3::new(v.x as f32, v.y as f32, v.z as f32)
}

// ---- Skizzen ----------------------------------------------------------------------------------

impl AppState {
    /// Lage einer Grundebene (rechtshändig, `z` = Normale).
    pub fn standard_frame(plane: StandardPlane) -> Frame {
        feature_plane_of(plane).frame()
    }

    /// Legt eine leere Skizze „Skizze N“ auf der Ebene `frame` an.
    pub fn new_sketch(&mut self, frame: Frame) -> usize {
        let name = format!("Skizze {}", self.next_sketch_number);
        self.next_sketch_number += 1;
        self.sketches
            .push(Sketch::new(frame, name, self.kernel.tolerance));
        self.sketches.len() - 1
    }

    /// Fehler als Meldung ablegen und zurückgeben.
    fn fail<T>(&mut self, e: String) -> Result<T, String> {
        self.message = Some(e.clone());
        Err(e)
    }

    /// Legt eine Skizze auf der ebenen Fläche `face` des Körpers `body` an.
    pub fn new_sketch_on_face(&mut self, body: usize, face: Id<Face>) -> Result<usize, String> {
        let Some(b) = self.bodies.get(body) else {
            return self.fail("Körper nicht gefunden".into());
        };
        if b.solid.faces.get(face).is_none() {
            return self.fail("Fläche nicht gefunden".into());
        }
        match plane_of_face(&b.solid, face) {
            Some(frame) => Ok(self.new_sketch(frame)),
            None => self.fail("Skizzen lassen sich nur auf ebenen Flächen anlegen".into()),
        }
    }

    /// Öffnet den Skizzen-Modus für die Skizze `index`: Kamera senkrecht auf die Skizzenebene,
    /// übrige Szene gedämpft. Beim Wechsel zwischen Skizzen bleibt die gemerkte Kamera erhalten.
    pub fn open_sketch(&mut self, index: usize) -> Result<(), String> {
        let Some(sk) = self.sketches.get(index) else {
            return self.fail("Skizze nicht gefunden".into());
        };
        let frame = *sk.frame();
        let extent = sk.extent();
        let mut session = SketchSession::new(index);
        session.last_report = Some(sk.clone().solve());
        if self.sketch_session.is_none() {
            self.saved_camera = Some(self.camera);
        }
        self.sketch_session = Some(session);
        match extent {
            Some((lo, hi)) => {
                // Sehr kleine Skizzen (ein Punkt, eine winzige Kurve) mit Mindestgröße einpassen.
                let (lo, hi) = if (hi - lo).max_element() < MIN_SKETCH_VIEW_MM {
                    let c = (lo + hi) * 0.5;
                    let half = DVec2::splat(MIN_SKETCH_VIEW_MM * 0.5);
                    (c - half, c + half)
                } else {
                    (lo, hi)
                };
                let corners = [
                    frame.point_2d(lo),
                    frame.point_2d(hi),
                    frame.point_2d(DVec2::new(lo.x, hi.y)),
                    frame.point_2d(DVec2::new(hi.x, lo.y)),
                ];
                let (min, max) = corners
                    .iter()
                    .fold((corners[0], corners[0]), |(a, b), &p| (a.min(p), b.max(p)));
                self.camera.frame_bounds(to_vec3(min), to_vec3(max));
            }
            None => self.camera.target = to_vec3(frame.origin),
        }
        self.camera.look_along(to_vec3(frame.z), to_vec3(frame.y));
        self.rebuild_scene();
        Ok(())
    }

    /// Beendet den Skizzen-Modus und stellt die vorherige Kamera wieder her.
    pub fn close_sketch(&mut self) {
        if self.sketch_session.take().is_none() {
            return;
        }
        if let Some(mut cam) = self.saved_camera.take() {
            // Das Seitenverhältnis gehört zum aktuellen Fenster.
            cam.aspect = self.camera.aspect;
            self.camera = cam;
        }
        self.rebuild_scene();
    }

    /// Löscht die Skizze `index`; eine Sitzung auf dieser Skizze wird geschlossen, die Indizes
    /// einer Sitzung auf einer späteren Skizze werden angepasst.
    pub fn delete_sketch(&mut self, index: usize) {
        if index >= self.sketches.len() {
            return;
        }
        match self.sketch_session.as_ref().map(|s| s.sketch) {
            Some(i) if i == index => self.close_sketch(),
            Some(i) if i > index => {
                if let Some(s) = self.sketch_session.as_mut() {
                    s.sketch = i - 1;
                }
            }
            _ => {}
        }
        self.selected_sketch = match self.selected_sketch {
            Some(i) if i == index => None,
            Some(i) if i > index => Some(i - 1),
            other => other,
        };
        self.sketches.remove(index);
    }

    /// Wunsch aus dem Kontextmenü des Browsers, „Skizze extrudieren/drehen“ zu öffnen: beendet
    /// die offene Skizze, wählt die Skizze im Browser und hebt die Auswahl von Körpern und in
    /// der 3D-Ansicht auf (wie ein einfacher Klick auf die Skizze).
    pub fn request_sketch_dialog(&mut self, request: SketchRequest) {
        let (SketchRequest::Extrude(i) | SketchRequest::Revolve(i)) = request;
        self.close_sketch();
        self.selected_sketch = Some(i);
        self.selected_body = None;
        self.set_selection(None);
        self.sketch_request = Some(request);
    }

    /// Skizze `sketch` und ihr Profil `profile` (Index in `profiles()`).
    fn sketch_profile(&self, sketch: usize, profile: usize) -> Result<(&Sketch, Profile), String> {
        let sk = self
            .sketches
            .get(sketch)
            .ok_or_else(|| "Skizze nicht gefunden".to_string())?;
        let p = self
            .profile_cache
            .get(sk)
            .profiles
            .get(profile)
            .cloned()
            .ok_or_else(|| "Profil nicht gefunden".to_string())?;
        Ok((sk, p))
    }

    /// Ebene für das Skizzen-Feature einer Skizze aus der Liste (Übergang, F13): eine Grundebene,
    /// wenn die Lage der Skizze ihr gleicht, sonst die ebene Fläche eines sichtbaren Körpers,
    /// auf der die Skizze liegt.
    fn feature_plane(&self, frame: &Frame) -> Result<SketchPlane, String> {
        let tol = self.kernel.tolerance;
        for p in [StandardPlane::XY, StandardPlane::XZ, StandardPlane::YZ].map(feature_plane_of) {
            if same_frame(&p.frame(), frame, tol.linear) {
                return Ok(SketchPlane::Standard(p));
            }
        }
        for b in &self.bodies {
            for (face, _) in b.solid.faces.iter() {
                if plane_of_face(&b.solid, face).is_some_and(|f| same_frame(&f, frame, tol.linear))
                    && let Ok(r) = planora_feature::FaceRef::from_face(&b.solid, face, &tol)
                {
                    return Ok(SketchPlane::OnFace(r));
                }
            }
        }
        Err(
            "Die Skizze liegt weder auf einer Grundebene noch auf einer Fläche eines sichtbaren \
             Körpers; bitte die Skizze neu anlegen"
                .into(),
        )
    }

    /// Legt die Skizze `sketch` als Skizzen-Feature (Kopie) und darauf das Feature `make(id)`
    /// an. Schlägt eines davon fehl, werden beide wieder entfernt und die Meldung abgelegt.
    /// Liefert den Index des neuen Körpers.
    fn add_sketch_body(
        &mut self,
        sketch: usize,
        name: String,
        make: impl FnOnce(FeatureId) -> FeatureKind,
    ) -> Result<usize, String> {
        let Some(sk) = self.sketches.get(sketch) else {
            return self.fail("Skizze nicht gefunden".into());
        };
        let (sk, sketch_name) = (sk.clone(), sk.name.clone());
        let plane = match self.feature_plane(sk.frame()) {
            Ok(p) => p,
            Err(e) => return self.fail(e),
        };
        let sketch_feature = FeatureKind::Sketch(SketchFeature {
            sketch: sk,
            plane,
            copies: Vec::new(),
        });
        let s_id = match self.document.add_feature(sketch_name, sketch_feature) {
            Ok(id) => id,
            Err(e) => return self.fail(e.to_string()),
        };
        let result = match self.document.feature(s_id).map(|f| f.status.clone()) {
            Some(FeatureStatus::Ok) => self
                .document
                .add_feature(name, make(s_id))
                .map_err(|e| e.to_string())
                .and_then(
                    |id| match self.document.feature(id).map(|f| f.status.clone()) {
                        Some(FeatureStatus::Ok) => Ok(id),
                        status => Err(status_message(status)),
                    },
                ),
            status => Err(status_message(status)),
        };
        match result {
            Ok(id) => {
                self.refresh();
                self.body_index(BodyRef {
                    feature: id,
                    index: 0,
                })
                .ok_or_else(|| "Der neue Körper ist nicht sichtbar".to_string())
            }
            Err(e) => {
                let _ = self.document.delete_with_dependents(s_id);
                self.fail(e)
            }
        }
    }

    /// Extrudiert Profil `profile` der Skizze `sketch` um `height` entlang der Skizzennormalen;
    /// der Körper entsteht an Ort und Stelle (als Skizzen- und Extrusions-Feature). Bei einem
    /// Fehler bleibt das Dokument unverändert.
    pub fn extrude_sketch(
        &mut self,
        sketch: usize,
        profile: usize,
        height: f64,
    ) -> Result<usize, String> {
        if !(height.is_finite() && height > self.kernel.tolerance.linear) {
            return self.fail("Die Höhe muss größer als null sein".into());
        }
        let name = match self.sketch_profile(sketch, profile) {
            Ok((sk, _)) => format!("Extrusion: {}", sk.name),
            Err(e) => return self.fail(e),
        };
        self.add_sketch_body(sketch, name, |id| FeatureKind::Extrude {
            sketch: id,
            profile,
            height,
        })
    }

    /// Dreht Profil `profile` der Skizze `sketch` um die Linie `axis` (360°); der Körper entsteht
    /// an Ort und Stelle (als Skizzen- und Rotations-Feature). Bei einem Fehler bleibt das
    /// Dokument unverändert.
    pub fn revolve_sketch(
        &mut self,
        sketch: usize,
        profile: usize,
        axis: CurveId,
    ) -> Result<usize, String> {
        let name = match self.sketch_profile(sketch, profile) {
            Ok((sk, _)) => format!("Rotation: {}", sk.name),
            Err(e) => return self.fail(e),
        };
        self.add_sketch_body(sketch, name, |id| FeatureKind::Revolve {
            sketch: id,
            profile,
            axis,
        })
    }
}

fn bounds_of(mesh: &Mesh) -> (DVec3, DVec3) {
    mesh.positions.iter().fold(
        (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    )
}

/// Meldung zu einem Feature, das nicht berechnet werden konnte.
fn status_message(status: Option<FeatureStatus>) -> String {
    match status {
        Some(FeatureStatus::Error(e) | FeatureStatus::Skipped(e)) => e,
        _ => "Das Feature konnte nicht berechnet werden".into(),
    }
}

/// Folgt einer Kette von Verschiebungen bis zu dem Körper, den ein anderes Feature erzeugt hat.
fn origin_of(doc: &Document, mut r: BodyRef) -> BodyRef {
    // Verweise zeigen nur nach vorn: spätestens nach so vielen Schritten ist die Kette zu Ende.
    for _ in 0..doc.features().len() {
        match doc.feature(r.feature).map(|f| &f.kind) {
            Some(FeatureKind::Move { body, .. }) => r = *body,
            _ => break,
        }
    }
    r
}

/// Anzeigename eines Körpers: der Name des Features, das ihn ursprünglich erzeugt hat (mit
/// laufender Nummer, wenn es mehrere Körper erzeugt).
fn body_name(doc: &Document, r: BodyRef) -> Option<String> {
    let r = origin_of(doc, r);
    let f = doc.feature(r.feature)?;
    let several = r.index > 0
        || doc
            .body(BodyRef {
                feature: r.feature,
                index: 1,
            })
            .is_some();
    Some(if several {
        format!("{} {}", f.name, r.index + 1)
    } else {
        f.name.clone()
    })
}

/// Art eines Körpers aus dem Feature, das ihn erzeugt hat; Verschieben übernimmt die Art des
/// verschobenen Körpers.
fn body_kind(doc: &Document, r: BodyRef) -> BodyKind {
    match doc.feature(origin_of(doc, r).feature).map(|f| &f.kind) {
        Some(FeatureKind::Box { .. }) => BodyKind::Box,
        Some(FeatureKind::Cylinder { .. }) => BodyKind::Cylinder,
        Some(FeatureKind::Sphere { .. }) => BodyKind::Sphere,
        Some(FeatureKind::Cone { .. }) => BodyKind::Cone,
        Some(FeatureKind::Extrude { .. }) => BodyKind::Extrusion,
        Some(FeatureKind::Revolve { .. }) => BodyKind::Revolution,
        _ => BodyKind::Combined,
    }
}

/// Stimmen zwei Koordinatensysteme überein (Ursprung und Achsen innerhalb von `tol`)?
fn same_frame(a: &Frame, b: &Frame, tol: f64) -> bool {
    a.origin.distance(b.origin) <= tol
        && a.x.distance(b.x) <= tol
        && a.y.distance(b.y) <= tol
        && a.z.distance(b.z) <= tol
}

/// Grundebene der App als Grundebene des Feature-Modells (eine gemeinsame Definition der Lage).
fn feature_plane_of(plane: StandardPlane) -> planora_feature::StandardPlane {
    match plane {
        StandardPlane::XY => planora_feature::StandardPlane::XY,
        StandardPlane::XZ => planora_feature::StandardPlane::XZ,
        StandardPlane::YZ => planora_feature::StandardPlane::YZ,
    }
}
