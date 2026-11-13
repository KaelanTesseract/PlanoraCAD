//! Leinwände in der App (Skizzen 9, Spec C und E): Einfügen und Bearbeiten als ein
//! Rückgängig-Schritt an der Verlaufsmarke, die Sperre während einer offenen Skizze, das Zeichnen
//! in der Ansicht ([`CanvasDraw`] und [`CanvasTexture`] je Bild) mit einem Zwischenspeicher der
//! entschlüsselten Bilder und die Auswahl über [`canvas_hit`].

use crate::jobs::{Job, Poll};
use crate::state::AppState;
use egui::{Pos2, Rect, Shape, Stroke};
use glam::Vec2;
use planora_feature::{CanvasFeature, CanvasImage, FeatureId, FeatureKind};
use planora_i18n::tr;
use planora_math::{DVec3, Frame};
use planora_render::{CanvasDraw, CanvasTexture, Ray, canvas_hit};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Längste Kante eines Bildes auf der GPU (größere werden verkleinert, die Datei bleibt).
pub const GPU_MAX_EDGE: u32 = 4096;
/// Breite des Rahmens einer gewählten Leinwand in der Ansicht.
const OUTLINE_WIDTH: f32 = 2.0;

/// Vorschau eines offenen Leinwand-Fensters: das Bild mit der Lage aus den Feldern auf dem
/// Träger `base`. Beim Bearbeiten (`edit`) ersetzt sie die Leinwand dieses Features.
#[derive(Clone, Debug)]
pub struct CanvasPreview {
    pub image: Arc<CanvasImage>,
    pub feature: CanvasFeature,
    pub base: Frame,
    pub edit: Option<FeatureId>,
}

/// Ergebnis eines Entschlüsselns im Hintergrund: Breite, Höhe, RGBA oder der Fehlertext.
type Decoded = Result<(u32, u32, Vec<u8>), String>;

/// Entschlüsselte Bilder je Prüfsumme (einmal je Bild, nicht in jedem Bild neu). Fehlt ein Bild,
/// wird es im Hintergrund entschlüsselt (ein Auftrag je Prüfsumme, nie im Bildaufbau: große
/// Bilder brauchen sonst Sekunden); bis dahin wird die Leinwand nicht gezeichnet und nicht
/// getroffen. Ein Bild, das sich nicht entschlüsseln lässt, wird gemerkt und nicht erneut
/// versucht.
#[derive(Default)]
pub struct TextureCache {
    map: HashMap<u64, CanvasTexture>,
    failed: HashSet<u64>,
    jobs: HashMap<u64, Job<Decoded>>,
    /// So viele Aufträge wurden gestartet (Tests: einmal je Bild).
    pub decodes: usize,
}

impl TextureCache {
    /// Legt ein schon entschlüsseltes Bild ab (etwa aus dem Lesen im Hintergrund beim
    /// Einfügen); ein laufender Auftrag für dasselbe Bild entfällt.
    pub fn insert(&mut self, t: CanvasTexture) {
        self.failed.remove(&t.key);
        self.jobs.remove(&t.key);
        self.map.insert(t.key, t);
    }

    pub fn get(&self, key: u64) -> Option<&CanvasTexture> {
        self.map.get(&key)
    }

    pub fn contains(&self, key: u64) -> bool {
        self.map.contains_key(&key)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Läuft noch ein Entschlüsseln (dann soll die Ansicht bald neu zeichnen)?
    pub fn pending(&self) -> bool {
        !self.jobs.is_empty()
    }

    /// Anzahl laufender Aufträge.
    pub fn running(&self) -> usize {
        self.jobs.len()
    }

    /// Übernimmt fertige Aufträge (ohne zu warten).
    pub fn poll(&mut self) {
        self.collect(false);
    }

    /// Wartet auf alle laufenden Aufträge (Tests und Abläufe ohne Fenster).
    pub fn wait_all(&mut self) {
        self.collect(true);
    }

    fn collect(&mut self, block: bool) {
        let keys: Vec<u64> = self.jobs.keys().copied().collect();
        for key in keys {
            let Some(job) = self.jobs.get(&key) else {
                continue;
            };
            let result = match job.fetch(block) {
                Poll::Pending => continue,
                Poll::Ready(r) => r,
                Poll::Lost => Err("decoder thread lost".to_string()),
            };
            self.jobs.remove(&key);
            match result {
                Ok((width, height, rgba)) => {
                    self.map.insert(
                        key,
                        CanvasTexture {
                            key,
                            width,
                            height,
                            rgba: Arc::from(rgba),
                        },
                    );
                }
                Err(e) => {
                    crate::diag::warn(&format!("Canvas: image could not be decoded: {e}"));
                    self.failed.insert(key);
                }
            }
        }
    }

    /// Das Bild für `img`, wenn es schon entschlüsselt ist; sonst startet (einmal) das
    /// Entschlüsseln im Hintergrund und es kommt `None`.
    pub fn get_or_start(&mut self, img: &CanvasImage) -> Option<&CanvasTexture> {
        let key = img.hash;
        if !self.map.contains_key(&key)
            && !self.failed.contains(&key)
            && !self.jobs.contains_key(&key)
        {
            self.decodes += 1;
            let bytes = img.bytes.clone();
            let job = Job::spawn(move |_| {
                planora_exchange::decode_rgba(&bytes, GPU_MAX_EDGE).map_err(|e| e.to_string())
            });
            self.jobs.insert(key, job);
        }
        self.map.get(&key)
    }

    /// Behält nur die Bilder mit diesen Prüfsummen (Aufträge für andere brechen ab).
    pub fn retain(&mut self, keys: &HashSet<u64>) {
        self.map.retain(|k, _| keys.contains(k));
        self.failed.retain(|k| keys.contains(k));
        self.jobs.retain(|k, _| keys.contains(k));
    }
}

/// Alpha des Bildes `t` an der Stelle `(u, v)` (0..=1, von oben links); Rand eingeschlossen.
pub fn alpha_at(t: &CanvasTexture, u: f64, v: f64) -> u8 {
    if t.width == 0 || t.height == 0 {
        return 0;
    }
    let x = ((u.clamp(0.0, 1.0) * f64::from(t.width)) as u32).min(t.width - 1);
    let y = ((v.clamp(0.0, 1.0) * f64::from(t.height)) as u32).min(t.height - 1);
    let i = (y as usize * t.width as usize + x as usize) * 4 + 3;
    t.rgba.get(i).copied().unwrap_or(0)
}

/// Eine gezeichnete Leinwand dieses Bildes: Feature (bei der Vorschau eines neuen Bildes
/// `None`), Ecken, Bild und Einstellungen.
struct Drawn {
    id: Option<FeatureId>,
    corners: [DVec3; 4],
    image: Arc<CanvasImage>,
    opacity: u8,
    display_through: bool,
    selectable: bool,
}

fn to_f32(c: [DVec3; 4]) -> [[f32; 3]; 4] {
    c.map(|p| p.as_vec3().to_array())
}

impl AppState {
    /// Die Leinwand-Features in der Reihenfolge des Verlaufs.
    pub fn canvas_ids(&self) -> Vec<FeatureId> {
        self.document
            .features()
            .iter()
            .filter(|f| matches!(f.kind, FeatureKind::Canvas(_)))
            .map(|f| f.id)
            .collect()
    }

    /// Gibt es eine Leinwand (dann steht der Ordner „Leinwände“ im Baum)?
    pub fn has_canvases(&self) -> bool {
        self.document
            .features()
            .iter()
            .any(|f| matches!(f.kind, FeatureKind::Canvas(_)))
    }

    /// Ist `id` eine Leinwand?
    pub fn is_canvas(&self, id: FeatureId) -> bool {
        self.document
            .feature(id)
            .is_some_and(|f| matches!(f.kind, FeatureKind::Canvas(_)))
    }

    /// Warum „Leinwand …“, „Leinwand bearbeiten“ und „Kalibrieren“ gerade nicht gehen: während
    /// eine Skizze offen ist (Spec, „Nach der Prüfung festgelegt“).
    pub fn canvas_lock(&self) -> Option<String> {
        self.open_sketch_id()
            .map(|_| tr!("insert-canvas-close-sketch"))
    }

    /// Fügt die Leinwand `f` mit dem Bild `img` an der Verlaufsmarke ein (ein Rückgängig-Schritt;
    /// das Bild kommt in die Bildtabelle, gleiche Bytes werden geteilt). Gesperrt, solange eine
    /// Skizze offen ist.
    pub fn insert_canvas(
        &mut self,
        img: CanvasImage,
        mut f: CanvasFeature,
    ) -> Result<FeatureId, String> {
        if let Some(why) = self.canvas_lock() {
            return self.fail(why);
        }
        f.image = self.document.add_image(img);
        f.last_base = None;
        let id = self.add_feature(None, FeatureKind::Canvas(f))?;
        self.select_canvas(id);
        Ok(id)
    }

    /// Ändert die Leinwand `id` auf `f` (Fenster „Leinwand bearbeiten“): ein Schritt. Bild und
    /// letzte Lage bleiben die der Leinwand.
    pub fn update_canvas(&mut self, id: FeatureId, mut f: CanvasFeature) -> Result<(), String> {
        if let Some(why) = self.canvas_lock() {
            return self.fail(why);
        }
        let Some(FeatureKind::Canvas(old)) = self.document.feature(id).map(|x| &x.kind) else {
            return self.fail(tr!("insert-canvas-not-found"));
        };
        f.image = old.image;
        if f.base == old.base {
            f.last_base = old.last_base;
        } else {
            f.last_base = None;
        }
        if *old == f {
            return Ok(());
        }
        let formulas = self
            .document
            .feature(id)
            .map(|x| x.formulas().clone())
            .unwrap_or_default();
        if let Err(e) =
            self.document
                .update_with_formulas(id, |k| *k = FeatureKind::Canvas(f), formulas)
        {
            return self.fail(tr!("edit-rejected", error = e.to_string()));
        }
        self.message = None;
        self.refresh();
        Ok(())
    }

    /// Wählt die Leinwand `id` (wie ein Klick auf ihre Zeile im Baum).
    pub fn select_canvas(&mut self, id: FeatureId) {
        self.set_selection(None);
        self.selected_feature = Some(id);
        self.selected_body = None;
        self.drawing_ui.selected_drawing = None;
    }

    /// Alle Leinwände, die jetzt zu sehen sind: eingeblendet und berechnet, beim Bearbeiten statt
    /// der bearbeiteten die Vorschau, dazu die Vorschau eines neuen Bildes.
    fn drawn_canvases(&self) -> Vec<Drawn> {
        let editing = self.canvas_preview.as_ref().and_then(|p| p.edit);
        let mut out = Vec::new();
        for f in self.document.features() {
            let FeatureKind::Canvas(c) = &f.kind else {
                continue;
            };
            if self.is_feature_hidden(f.id) || editing == Some(f.id) {
                continue;
            }
            let (Some(frame), Some(img)) = (
                self.document.canvas_frame(f.id),
                self.document.image(c.image),
            ) else {
                continue;
            };
            out.push(Drawn {
                id: Some(f.id),
                corners: c.corners(&frame, img),
                image: img.clone(),
                opacity: c.opacity,
                display_through: c.display_through,
                selectable: c.selectable,
            });
        }
        if let Some(p) = &self.canvas_preview {
            let frame = p.feature.placed_frame(&p.base);
            out.push(Drawn {
                id: None,
                corners: p.feature.corners(&frame, &p.image),
                image: p.image.clone(),
                opacity: p.feature.opacity,
                display_through: p.feature.display_through,
                selectable: false,
            });
        }
        out
    }

    /// Die Leinwände dieses Bildes für die GPU und je gezeichneter Leinwand ihr Bild (Vertrag
    /// mit `planora_render`: jedes gezeichnete Bild in jedem Bild mitschicken). Bilder, die
    /// nicht mehr gebraucht werden, fallen aus dem Zwischenspeicher.
    pub fn canvas_frame_data(&mut self) -> (Vec<CanvasDraw>, Vec<CanvasTexture>) {
        let drawn = self.drawn_canvases();
        let mut keep: HashSet<u64> = self
            .document
            .features()
            .iter()
            .filter_map(|f| match &f.kind {
                FeatureKind::Canvas(c) => self.document.image(c.image).map(|i| i.hash),
                _ => None,
            })
            .collect();
        if let Some(p) = &self.canvas_preview {
            keep.insert(p.image.hash);
        }
        self.canvas_textures.retain(&keep);
        self.canvas_textures.poll();
        let mut draws = Vec::new();
        let mut textures: Vec<CanvasTexture> = Vec::new();
        for d in drawn {
            let Some(t) = self.canvas_textures.get_or_start(&d.image) else {
                continue;
            };
            if !textures.iter().any(|x| x.key == t.key) {
                textures.push(t.clone());
            }
            draws.push(CanvasDraw {
                key: d.image.hash,
                corners: to_f32(d.corners),
                opacity: f32::from(d.opacity.min(100)) / 100.0,
                display_through: d.display_through,
            });
        }
        (draws, textures)
    }

    /// Sehstrahl durch den Zeiger `cursor` (Punkte ab der linken oberen Ecke der Ansicht der
    /// Größe `viewport`); bei einer leeren Ansicht `None`.
    pub(crate) fn view_ray(&self, cursor: Vec2, viewport: Vec2) -> Option<Ray> {
        if viewport.x < 1.0 || viewport.y < 1.0 {
            return None;
        }
        let ndc = Vec2::new(
            2.0 * cursor.x / viewport.x - 1.0,
            1.0 - 2.0 * cursor.y / viewport.y,
        );
        let (origin, dir) = self.camera.ray(ndc);
        Some(Ray {
            origin: origin.as_dvec3(),
            dir: dir.as_dvec3(),
        })
    }

    /// Die vorderste auswählbare Leinwand unter dem Zeiger und die Strahllänge bis zu ihr.
    /// Nicht auswählbare werden durchklickt, Pixel mit Alpha 0 zählen nicht.
    pub fn canvas_at(&mut self, cursor: Vec2, viewport: Vec2) -> Option<(FeatureId, f64)> {
        let ray = self.view_ray(cursor, viewport)?;
        let mut best: Option<(FeatureId, f64)> = None;
        for d in self.drawn_canvases() {
            let Some(id) = d.id.filter(|_| d.selectable) else {
                continue;
            };
            // Noch nicht entschlüsselt: nicht gezeichnet, also auch nicht getroffen.
            let Some(t) = self.canvas_textures.get(d.image.hash) else {
                continue;
            };
            if let Some(dist) = canvas_hit(d.corners, ray, |u, v| alpha_at(t, u, v))
                && best.is_none_or(|(_, b)| dist < b)
            {
                best = Some((id, dist));
            }
        }
        best
    }

    /// Die Leinwand, die ein Klick in der normalen Auswahl trifft: nur, wenn sie vor allem
    /// anderen unter dem Zeiger liegt (Ecken und Kanten liegen immer vorn).
    pub fn canvas_click_at(&mut self, cursor: Vec2, viewport: Vec2) -> Option<FeatureId> {
        let (id, t) = self.canvas_at(cursor, viewport)?;
        let other = self.view_click_at(cursor, viewport);
        match self.click_distance(cursor, viewport, other) {
            Some(d) if d <= t => None,
            _ => Some(id),
        }
    }

    /// Klick in die Ansicht bei der normalen Auswahl: eine Leinwand vorn wird gewählt (ihre
    /// Zeile im Baum leuchtet), sonst das Getroffene wie bisher. `true`, wenn es eine Leinwand
    /// war.
    pub fn click_canvas(&mut self, cursor: Vec2, viewport: Vec2) -> bool {
        match self.canvas_click_at(cursor, viewport) {
            Some(id) => {
                self.select_canvas(id);
                true
            }
            None => false,
        }
    }

    /// Zeiger über der Ansicht bei der normalen Auswahl: Liegt eine auswählbare Leinwand vorn,
    /// wird sie hervorgehoben (Rahmen) statt des Körpers oder der Ebene dahinter, wie beim
    /// Klick. `true`, wenn es eine Leinwand ist.
    pub fn hover_canvas(&mut self, cursor: Vec2, viewport: Vec2) -> bool {
        self.canvas_hover = self.canvas_click_at(cursor, viewport);
        self.canvas_hover.is_some()
    }
}

/// Rahmen der gewählten Leinwand (die Hervorhebung zur Zeile im Baum) und der Leinwand unter
/// dem Zeiger in der Ansicht.
pub fn canvas_outlines(state: &AppState, rect: Rect) -> Vec<Shape> {
    let selected = state.selected_feature.filter(|id| state.is_canvas(*id));
    let mut ids: Vec<FeatureId> = selected.into_iter().collect();
    if let Some(h) = state.canvas_hover
        && Some(h) != selected
    {
        ids.push(h);
    }
    ids.into_iter()
        .flat_map(|id| outline(state, rect, id))
        .collect()
}

fn outline(state: &AppState, rect: Rect, id: FeatureId) -> Option<Shape> {
    if state.is_feature_hidden(id) || state.canvas_preview.as_ref().and_then(|p| p.edit) == Some(id)
    {
        return None;
    }
    let Some(FeatureKind::Canvas(c)) = state.document.feature(id).map(|f| &f.kind) else {
        return None;
    };
    let (Some(frame), Some(img)) = (
        state.document.canvas_frame(id),
        state.document.image(c.image),
    ) else {
        return None;
    };
    let viewport = Vec2::new(rect.width(), rect.height());
    let points: Option<Vec<Pos2>> = c
        .corners(&frame, img)
        .into_iter()
        .map(|p| {
            state
                .camera
                .project(p.as_vec3(), viewport)
                .filter(|v| v.is_finite())
                .map(|v| egui::pos2(rect.left() + v.x, rect.top() + v.y))
        })
        .collect();
    let color = crate::theme::sketch_colors(state.dark).selected;
    Some(Shape::closed_line(
        points?,
        Stroke::new(OUTLINE_WIDTH, color),
    ))
}
