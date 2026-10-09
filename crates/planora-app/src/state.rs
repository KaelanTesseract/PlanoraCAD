use glam::{Vec2, Vec3};
use planora_kernel::boolean::BoolOp;
use planora_kernel::{AnalyticKernel, Kernel, Solid};
use planora_math::{DMat4, DVec3};
use planora_mesh::{
    MassProperties, Mesh, TessellationOptions, boolean_volume_plausible, mass_properties,
    tessellate,
};
use planora_render::{
    EdgeRef, FaceRef, Highlight, OrbitCamera, Palette, Pick, SceneData, build_scene, pick_edge,
    pick_face,
};
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
}

const GRID_HALF_EXTENT: f32 = 500.0;
const PICK_EDGE_PX: f32 = 6.0;
const BODY_GAP_MM: f64 = 10.0;

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
            &self.palette(),
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

fn bounds_of(mesh: &Mesh) -> (DVec3, DVec3) {
    mesh.positions.iter().fold(
        (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    )
}
