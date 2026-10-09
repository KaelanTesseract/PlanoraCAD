use crate::sketch_state::SketchSession;
use glam::{Vec2, Vec3};
use planora_kernel::boolean::BoolOp;
use planora_kernel::{AnalyticKernel, Face, Id, Kernel, Profile, Solid};
use planora_math::{DMat4, DVec2, DVec3, Frame};
use planora_mesh::{
    MassProperties, Mesh, TessellationOptions, boolean_volume_plausible, mass_properties,
    tessellate,
};
use planora_render::{
    EdgeRef, FaceRef, Highlight, OrbitCamera, Palette, Pick, SceneData, build_scene, pick_edge,
    pick_face,
};
use planora_sketch::model::{CurveId, Sketch};
use planora_sketch::{extrude_profile, plane_of_face, revolve_profile};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    Box,
    Cylinder,
    Sphere,
    Cone,
    Extrusion,
    Revolution,
}

/// Grundebenen für neue Skizzen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandardPlane {
    XY,
    XZ,
    YZ,
}

pub struct Body {
    pub name: String,
    pub kind: BodyKind,
    pub solid: Solid,
    pub mesh: Mesh,
    pub mass: MassProperties,
    /// Achsenparallele Hülle `(min, max)` in Millimetern.
    pub bounds: (DVec3, DVec3),
}

/// Alles, was die Oberfläche anzeigt. Enthält keine egui-Typen und ist deshalb testbar.
pub struct AppState {
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
    /// Laufende Nummer für den nächsten erzeugten Körper; zählt nur hoch, damit Namen auch nach
    /// dem Löschen eindeutig bleiben.
    pub next_body_number: u64,
    /// Skizzen des Dokuments.
    pub sketches: Vec<Sketch>,
    /// Geöffneter Skizzen-Modus.
    pub sketch_session: Option<SketchSession>,
    /// Kamera vor dem Öffnen des Skizzen-Modus.
    saved_camera: Option<OrbitCamera>,
    /// Laufende Nummer für den Namen der nächsten Skizze; zählt nur hoch, damit Namen auch nach
    /// dem Löschen eindeutig bleiben.
    pub next_sketch_number: u64,
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
        let mut state = Self {
            bodies: Vec::new(),
            selected_body: None,
            selection: None,
            hover: None,
            camera: OrbitCamera::default(),
            scene: Arc::new(SceneData::default()),
            scene_version: 0,
            dark: true,
            kernel: AnalyticKernel::default(),
            chord_tolerance: 0.05,
            message: None,
            next_body_number: 1,
            sketches: Vec::new(),
            sketch_session: None,
            saved_camera: None,
            next_sketch_number: 1,
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

    /// Prüft den Körper, stellt ihn neben die vorhandenen und nimmt ihn auf.
    pub fn add_body(
        &mut self,
        kind: BodyKind,
        name: String,
        solid: Solid,
    ) -> Result<usize, String> {
        self.kernel
            .validate(&solid)
            .map_err(|e| format!("Ungültiger Körper: {e}"))?;
        let solid = match self.bodies.iter().map(|b| b.bounds.1.x).reduce(f64::max) {
            None => solid,
            Some(max_x) => {
                let mesh = self.mesh_of(&solid)?;
                let (min, _) = bounds_of(&mesh);
                let shift =
                    DMat4::from_translation(DVec3::new(max_x + BODY_GAP_MM - min.x, 0.0, 0.0));
                self.kernel
                    .transform(&solid, &shift)
                    .map_err(|e| format!("Platzieren fehlgeschlagen: {e}"))?
            }
        };
        let body = self.make_body(kind, name, solid)?;
        self.bodies.push(body);
        self.rebuild_scene();
        self.frame_all();
        Ok(self.bodies.len() - 1)
    }

    /// Wie `add_body`, aber ohne das Verschieben neben die vorhandenen Körper.
    pub fn add_body_in_place(
        &mut self,
        kind: BodyKind,
        name: String,
        solid: Solid,
    ) -> Result<usize, String> {
        self.kernel
            .validate(&solid)
            .map_err(|e| format!("Ungültiger Körper: {e}"))?;
        let body = self.make_body(kind, name, solid)?;
        self.bodies.push(body);
        self.rebuild_scene();
        self.frame_all();
        Ok(self.bodies.len() - 1)
    }

    /// Verschiebt den Körper `index` um `delta` (Millimeter).
    pub fn translate_body(&mut self, index: usize, delta: DVec3) -> Result<(), String> {
        let body = self
            .bodies
            .get(index)
            .ok_or_else(|| "Körper nicht gefunden".to_string())?;
        if !delta.is_finite() {
            return Err("Ungültige Verschiebung".into());
        }
        let moved = self
            .kernel
            .transform(&body.solid, &DMat4::from_translation(delta))
            .map_err(|e| format!("Verschieben fehlgeschlagen: {e}"))?;
        let (kind, name) = (body.kind, body.name.clone());
        let new = self.make_body(kind, name, moved)?;
        self.bodies[index] = new;
        self.rebuild_scene();
        Ok(())
    }

    /// Führt `a op b` aus und ersetzt beide Körper durch das Ergebnis (an der Stelle des
    /// vorderen in der Liste, ohne neu zu platzieren). Liefert den Index des Ergebnisses; bei
    /// einem Fehler bleibt alles unverändert.
    pub fn boolean(&mut self, a: usize, b: usize, op: BoolOp) -> Result<usize, String> {
        if a == b || a >= self.bodies.len() || b >= self.bodies.len() {
            return Err("Bitte zwei verschiedene Körper wählen".into());
        }
        let result = self
            .kernel
            .boolean(&self.bodies[a].solid, &self.bodies[b].solid, op)
            .map_err(|e| e.to_string())?;
        let (name_a, name_b) = (&self.bodies[a].name, &self.bodies[b].name);
        let name = match op {
            BoolOp::Union => format!("Vereinigung: {name_a} + {name_b}"),
            BoolOp::Difference => format!("Abzug: {name_a} − {name_b}"),
            BoolOp::Intersection => format!("Schnitt: {name_a} ∩ {name_b}"),
        };
        let kind = self.bodies[a].kind;
        let body = self.make_body(kind, name, result)?;
        let (va, vb) = (self.bodies[a].mass.volume, self.bodies[b].mass.volume);
        if !boolean_volume_plausible(op, va, vb, body.mass.volume, 0.02) {
            return Err(
                "Das Ergebnis ist unplausibel (Volumenprobe fehlgeschlagen) und wurde verworfen"
                    .into(),
            );
        }
        let (lo, hi) = (a.min(b), a.max(b));
        self.bodies.remove(hi);
        self.bodies[lo] = body;
        self.hover = None;
        self.selection = None;
        self.selected_body = Some(lo);
        self.rebuild_scene();
        Ok(lo)
    }

    /// Netz, Masse und Hülle eines Körpers berechnen (gemeinsam für Platzieren, Verschieben und
    /// Ergebnisse Boolescher Operationen).
    fn make_body(&self, kind: BodyKind, name: String, solid: Solid) -> Result<Body, String> {
        let mesh = self.mesh_of(&solid)?;
        let mass = mass_properties(&mesh);
        let bounds = bounds_of(&mesh);
        Ok(Body {
            name,
            kind,
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

    pub fn remove_body(&mut self, index: usize) {
        if index >= self.bodies.len() {
            return;
        }
        self.bodies.remove(index);
        self.hover = None;
        self.selection = None;
        self.selected_body = match self.selected_body {
            Some(i) if i == index => None,
            Some(i) if i > index => Some(i - 1),
            other => other,
        };
        self.rebuild_scene();
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
        let (x, y, z) = match plane {
            StandardPlane::XY => (DVec3::X, DVec3::Y, DVec3::Z),
            StandardPlane::XZ => (DVec3::X, DVec3::Z, -DVec3::Y),
            StandardPlane::YZ => (DVec3::Y, DVec3::Z, DVec3::X),
        };
        Frame {
            origin: DVec3::ZERO,
            x,
            y,
            z,
        }
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
        self.sketches.remove(index);
    }

    /// Skizze `sketch` und ihr Profil `profile` (Index in `profiles()`).
    fn sketch_profile(&self, sketch: usize, profile: usize) -> Result<(&Sketch, Profile), String> {
        let sk = self
            .sketches
            .get(sketch)
            .ok_or_else(|| "Skizze nicht gefunden".to_string())?;
        let p = sk
            .profiles()
            .profiles
            .into_iter()
            .nth(profile)
            .ok_or_else(|| "Profil nicht gefunden".to_string())?;
        Ok((sk, p))
    }

    /// Nimmt einen Körper aus einer Skizze an Ort und Stelle auf oder legt den Fehler ab.
    fn add_sketch_body(
        &mut self,
        kind: BodyKind,
        made: Result<(Solid, String), String>,
    ) -> Result<usize, String> {
        match made.and_then(|(solid, name)| self.add_body_in_place(kind, name, solid)) {
            Ok(i) => Ok(i),
            Err(e) => self.fail(e),
        }
    }

    /// Extrudiert Profil `profile` der Skizze `sketch` um `height` entlang der Skizzennormalen;
    /// der Körper entsteht an Ort und Stelle. Bei einem Fehler bleibt das Dokument unverändert.
    pub fn extrude_sketch(
        &mut self,
        sketch: usize,
        profile: usize,
        height: f64,
    ) -> Result<usize, String> {
        let made = if !(height.is_finite() && height > self.kernel.tolerance.linear) {
            Err("Die Höhe muss größer als null sein".to_string())
        } else {
            self.sketch_profile(sketch, profile).and_then(|(sk, p)| {
                let solid =
                    extrude_profile(&self.kernel, sk, &p, height).map_err(|e| e.to_string())?;
                Ok((solid, format!("Extrusion: {}", sk.name)))
            })
        };
        self.add_sketch_body(BodyKind::Extrusion, made)
    }

    /// Dreht Profil `profile` der Skizze `sketch` um die Linie `axis` (360°); der Körper entsteht
    /// an Ort und Stelle. Bei einem Fehler bleibt das Dokument unverändert.
    pub fn revolve_sketch(
        &mut self,
        sketch: usize,
        profile: usize,
        axis: CurveId,
    ) -> Result<usize, String> {
        let made = self.sketch_profile(sketch, profile).and_then(|(sk, p)| {
            let solid = revolve_profile(&self.kernel, sk, &p, axis).map_err(|e| e.to_string())?;
            Ok((solid, format!("Rotation: {}", sk.name)))
        });
        self.add_sketch_body(BodyKind::Revolution, made)
    }
}

fn bounds_of(mesh: &Mesh) -> (DVec3, DVec3) {
    mesh.positions.iter().fold(
        (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    )
}
