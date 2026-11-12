//! Leinwände (Skizzen 9): Bild-Rechtecke in der 3D-Ansicht und ihr Treffertest.

use crate::gpu::{DEPTH_FORMAT, MSAA_SAMPLES};
use crate::pick::Ray;
use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu;
use egui_wgpu::wgpu::util::DeviceExt as _;
use planora_math::DVec3;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

/// Eine Leinwand in diesem Bild.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasDraw {
    /// Bild-Hash; wählt die Textur aus dem Zwischenspeicher der GPU.
    pub key: u64,
    /// Ecken in Weltkoordinaten: oben links, oben rechts, unten rechts, unten links, so wie das
    /// Bild erscheint (Texturkoordinaten (0,0), (1,0), (1,1), (0,1)). Eine Spiegelung steckt
    /// allein in der Lage der Ecken.
    pub corners: [[f32; 3]; 4],
    /// Deckkraft 0..=1; wird mit dem Alpha des Bildes multipliziert.
    pub opacity: f32,
    /// „Durch Objekte anzeigen“: ohne Tiefentest nach allem anderen zeichnen.
    pub display_through: bool,
}

/// Bild einer gezeichneten Leinwand für die GPU (RGBA8, nicht vormultipliziert, vom Aufrufer
/// schon auf höchstens 4096 px Kante verkleinert). Der Aufrufer schickt es in jedem Bild für jede
/// gezeichnete Leinwand mit (billig, `Arc`); die GPU legt es nur an, wenn sie `key` noch nicht hat.
#[derive(Clone, Debug)]
pub struct CanvasTexture {
    pub key: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
}

/// Eckpunkt des Leinwand-Rechtecks auf der GPU.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct CanvasVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub opacity: f32,
}

/// Zwei Dreiecke (0,1,2) und (0,2,3) aus den Ecken von [`CanvasDraw::corners`].
pub fn quad_vertices(corners: [[f32; 3]; 4], opacity: f32) -> [CanvasVertex; 6] {
    const UV: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let v = |i: usize| CanvasVertex {
        position: corners[i],
        uv: UV[i],
        opacity,
    };
    [v(0), v(1), v(2), v(0), v(2), v(3)]
}

/// Trifft der Strahl die Leinwand mit den Ecken `corners` (Reihenfolge wie
/// [`CanvasDraw::corners`])? Liefert die Strahllänge bis zum Treffer. `alpha_at(u, v)` gibt das
/// Alpha des Bildes an der Stelle (u nach rechts, v nach unten). u und v liegen im
/// **geschlossenen** Bereich 0..=1: Am rechten bzw. unteren Rand ist der Wert genau 1, der
/// Aufrufer muss den Pixelindex also begrenzen (`((u * w as f64) as u32).min(w - 1)`). Pixel mit
/// Alpha 0 zählen nicht. Beidseitig; parallel, hinter dem Ursprung oder außerhalb `None`.
pub fn canvas_hit(corners: [DVec3; 4], ray: Ray, alpha_at: impl Fn(f64, f64) -> u8) -> Option<f64> {
    let origin = corners[0];
    // Kanten des Parallelogramms: nach rechts (u) und nach unten (v) im Bild.
    let eu = corners[1] - origin;
    let ev = corners[3] - origin;
    let n = eu.cross(ev);
    let denom = ray.dir.dot(n);
    if !denom.is_finite() || denom.abs() <= 1e-12 * n.length() * ray.dir.length() {
        return None;
    }
    let t = (origin - ray.origin).dot(n) / denom;
    if !(t.is_finite() && t > 1e-9) {
        return None;
    }
    // Lage im Rechteck aus dem 2×2-System der Kanten (auch für schiefe Parallelogramme).
    let w = ray.origin + ray.dir * t - origin;
    let (uu, uv, vv) = (eu.dot(eu), eu.dot(ev), ev.dot(ev));
    let det = uu * vv - uv * uv;
    if !(det.is_finite() && det > 0.0) {
        return None;
    }
    let (wu, wv) = (w.dot(eu), w.dot(ev));
    let u = (wu * vv - wv * uv) / det;
    let v = (wv * uu - wu * uv) / det;
    let inside = (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v);
    (inside && alpha_at(u, v) > 0).then_some(t)
}

/// Ein Zeichenaufruf: Textur `key`, Eckpunkte `vertices` im gemeinsamen Puffer.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CanvasBatch {
    pub key: u64,
    pub vertices: Range<u32>,
}

/// Eckpunkte aller zeichenbaren Leinwände dieses Bildes in einem Puffer und die Aufrufe, getrennt
/// nach Tiefentest (`depth`) und „Durch Objekte anzeigen“ (`through`). Leinwände ohne Textur
/// (`has_texture` falsch) und mit Deckkraft 0 fallen weg; die Deckkraft wird auf 0..=1 begrenzt.
pub(crate) fn canvas_batches(
    draws: &[CanvasDraw],
    has_texture: impl Fn(u64) -> bool,
) -> (Vec<CanvasVertex>, Vec<CanvasBatch>, Vec<CanvasBatch>) {
    let mut vertices = Vec::with_capacity(draws.len() * 6);
    let mut push = |d: &CanvasDraw, out: &mut Vec<CanvasBatch>| {
        // Auch NaN fällt hier weg.
        if d.opacity.is_nan() || d.opacity <= 0.0 || !has_texture(d.key) {
            return;
        }
        let (Ok(start), Ok(end)) = (
            u32::try_from(vertices.len()),
            u32::try_from(vertices.len() + 6),
        ) else {
            return;
        };
        vertices.extend(quad_vertices(d.corners, d.opacity.min(1.0)));
        out.push(CanvasBatch {
            key: d.key,
            vertices: start..end,
        });
    };
    let (mut depth, mut through) = (Vec::new(), Vec::new());
    for d in draws {
        if d.display_through {
            push(d, &mut through);
        } else {
            push(d, &mut depth);
        }
    }
    (vertices, depth, through)
}

/// Zwischenspeicher der Texturen: Behalten wird genau, was `draws` in diesem Bild benutzt. Was
/// nicht mehr gezeichnet wird (Leinwand gelöscht, ausgeblendet, anderes Bild), fällt sofort weg.
/// Weil der Aufrufer die Bilder aller gezeichneten Leinwände in jedem Bild mitschickt, kommt eine
/// wieder gezeigte Leinwand von selbst zurück; ein verworfener egui-Durchlauf (ohne `prepare`)
/// schadet nicht.
pub(crate) fn prune_unreferenced<V>(cache: &mut HashMap<u64, V>, draws: &[CanvasDraw]) {
    cache.retain(|key, _| draws.iter().any(|d| d.key == *key));
}

/// Wie [`prune_unreferenced`] für die Menge der abgelehnten Bilder: Verschwindet ein `key`, darf
/// er beim nächsten Auftauchen wieder gemeldet werden.
pub(crate) fn prune_failed(failed: &mut HashSet<u64>, draws: &[CanvasDraw]) {
    failed.retain(|key| draws.iter().any(|d| d.key == *key));
}

/// Was mit einem mitgeschickten Bild geschieht.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UploadStep {
    /// Schon im Zwischenspeicher: nichts neu anlegen.
    Skip,
    /// Neu und passend: Textur anlegen.
    Upload,
    /// Passt nicht zur GPU; `log` nur beim ersten Mal, solange der `key` gezeichnet wird.
    Reject { log: bool },
}

/// Entscheidet für ein mitgeschicktes Bild; merkt sich abgelehnte Schlüssel in `failed`.
pub(crate) fn upload_step(
    tex: &CanvasTexture,
    cached: bool,
    failed: &mut HashSet<u64>,
    max_edge: u32,
) -> UploadStep {
    if cached {
        UploadStep::Skip
    } else if failed.contains(&tex.key) {
        UploadStep::Reject { log: false }
    } else if !texture_fits(tex, max_edge) {
        failed.insert(tex.key);
        UploadStep::Reject { log: true }
    } else {
        UploadStep::Upload
    }
}

/// RGBA mit vormultipliziertem Alpha (gerundet), passend zu `PREMULTIPLIED_ALPHA_BLENDING`;
/// verhindert dunkle Säume beim Filtern an durchsichtigen Rändern.
pub(crate) fn premultiply(rgba: &[u8]) -> Vec<u8> {
    let mut out = rgba.to_vec();
    for px in out.as_chunks_mut::<4>().0 {
        let a = u16::from(px[3]);
        for c in &mut px[..3] {
            // (c·a + 127) / 255 ≤ 255, passt also immer in u8.
            *c = u8::try_from((u16::from(*c) * a + 127) / 255).unwrap_or(u8::MAX);
        }
    }
    out
}

/// Passt die Textur zur GPU? Kanten 1..=`max_edge` und genau `width × height × 4` Bytes.
pub(crate) fn texture_fits(tex: &CanvasTexture, max_edge: u32) -> bool {
    let edge_ok = |e: u32| (1..=max_edge).contains(&e);
    let expected = u64::from(tex.width)
        .checked_mul(u64::from(tex.height))
        .and_then(|n| n.checked_mul(4));
    edge_ok(tex.width)
        && edge_ok(tex.height)
        && expected.is_some_and(|n| u64::try_from(tex.rgba.len()) == Ok(n))
}

/// Eine Textur im Zwischenspeicher; die Bindegruppe hält Textur und Ansicht am Leben.
struct CachedTexture {
    bind_group: wgpu::BindGroup,
}

/// GPU-Teil der Leinwände: zwei Pipelines (mit und ohne Tiefentest), Texturen je `key`.
pub(crate) struct CanvasGpu {
    depth_pipeline: wgpu::RenderPipeline,
    through_pipeline: wgpu::RenderPipeline,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    textures: HashMap<u64, CachedTexture>,
    /// Abgelehnte Bilder (passen nicht zur GPU), damit die Warnung nicht jedes Bild kommt.
    failed: HashSet<u64>,
    vertex_buffer: Option<wgpu::Buffer>,
    vertex_capacity: u32,
    depth: Vec<CanvasBatch>,
    through: Vec<CanvasBatch>,
}

impl CanvasGpu {
    /// `uniform_layout` ist die Gruppe 0 der übrigen Pipelines (Kamera).
    pub(crate) fn new(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
        uniform_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("planora-canvas"),
            source: wgpu::ShaderSource::Wgsl(include_str!("canvas.wgsl").into()),
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("planora-canvas-texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("planora-canvas"),
            bind_group_layouts: &[Some(uniform_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let make = |label: &'static str, depth_compare: wgpu::CompareFunction| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_canvas"),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<CanvasVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![
                            0 => Float32x3,
                            1 => Float32x2,
                            2 => Float32
                        ],
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_canvas"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    // Beidseitig sichtbar.
                    cull_mode: None,
                    ..Default::default()
                },
                // Tiefe wird nie geschrieben: Leinwände verdecken weder Körper noch einander.
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(depth_compare),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: MSAA_SAMPLES,
                    ..Default::default()
                },
                multiview_mask: None,
                cache: None,
            })
        };
        let depth_pipeline = make("planora-canvas-depth", wgpu::CompareFunction::LessEqual);
        let through_pipeline = make("planora-canvas-through", wgpu::CompareFunction::Always);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("planora-canvas"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            depth_pipeline,
            through_pipeline,
            texture_layout,
            sampler,
            textures: HashMap::new(),
            failed: HashSet::new(),
            vertex_buffer: None,
            vertex_capacity: 0,
            depth: Vec::new(),
            through: Vec::new(),
        }
    }

    /// Legt Texturen für noch unbekannte Schlüssel an ([`upload_step`]), wirft nicht mehr benutzte
    /// weg ([`prune_unreferenced`]) und schreibt die Eckpunkte dieses Bildes. `textures` sind die
    /// Bilder aller gezeichneten Leinwände; bekannte werden übersprungen.
    pub(crate) fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        draws: &[CanvasDraw],
        textures: &[CanvasTexture],
    ) {
        let max_edge = device.limits().max_texture_dimension_2d;
        for tex in textures {
            let cached = self.textures.contains_key(&tex.key);
            match upload_step(tex, cached, &mut self.failed, max_edge) {
                UploadStep::Skip | UploadStep::Reject { log: false } => continue,
                UploadStep::Reject { log: true } => {
                    eprintln!(
                        "Warning: canvas image {}x{} does not fit the graphics card (max. {max_edge} px), not drawn",
                        tex.width, tex.height
                    );
                    continue;
                }
                UploadStep::Upload => {}
            }
            let texture = device.create_texture_with_data(
                queue,
                &wgpu::TextureDescriptor {
                    label: Some("planora-canvas-image"),
                    size: wgpu::Extent3d {
                        width: tex.width,
                        height: tex.height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    // Unorm wie das Ziel: Die Bildwerte gehen unverändert durch (keine
                    // sRGB-Umrechnung), genau wie die Flächenfarben.
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                &premultiply(&tex.rgba),
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("planora-canvas-image"),
                layout: &self.texture_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            self.textures.insert(tex.key, CachedTexture { bind_group });
        }
        prune_unreferenced(&mut self.textures, draws);
        prune_failed(&mut self.failed, draws);

        let (vertices, depth, through) = canvas_batches(draws, |k| self.textures.contains_key(&k));
        self.depth.clear();
        self.through.clear();
        let Ok(count) = u32::try_from(vertices.len()) else {
            return;
        };
        if count == 0 {
            return;
        }
        if self.vertex_buffer.is_none() || count > self.vertex_capacity {
            let capacity = count.next_power_of_two().max(64);
            let bytes = u64::from(capacity) * std::mem::size_of::<CanvasVertex>() as u64;
            if bytes > device.limits().max_buffer_size {
                return;
            }
            self.vertex_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("planora-canvas-vertices"),
                size: bytes,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.vertex_capacity = capacity;
        }
        if let Some(buffer) = &self.vertex_buffer {
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(&vertices));
            self.depth = depth;
            self.through = through;
        }
    }

    /// Erst die Leinwände mit Tiefentest, dann die mit „Durch Objekte anzeigen“. Gruppe 0
    /// (Kamera) muss schon gebunden sein.
    pub(crate) fn paint(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        let Some(vb) = &self.vertex_buffer else {
            return;
        };
        for (pipeline, batches) in [
            (&self.depth_pipeline, &self.depth),
            (&self.through_pipeline, &self.through),
        ] {
            if batches.is_empty() {
                continue;
            }
            render_pass.set_pipeline(pipeline);
            render_pass.set_vertex_buffer(0, vb.slice(..));
            for batch in batches {
                if let Some(tex) = self.textures.get(&batch.key) {
                    render_pass.set_bind_group(1, &tex.bind_group, &[]);
                    render_pass.draw(batch.vertices.clone(), 0..1);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// Quadrat 100 × 100 in der XY-Ebene, Mitte im Ursprung, Bild ungespiegelt.
    fn square() -> [DVec3; 4] {
        [
            DVec3::new(-50.0, 50.0, 0.0),
            DVec3::new(50.0, 50.0, 0.0),
            DVec3::new(50.0, -50.0, 0.0),
            DVec3::new(-50.0, -50.0, 0.0),
        ]
    }

    fn down_at(x: f64, y: f64) -> Ray {
        Ray {
            origin: DVec3::new(x, y, 10.0),
            dir: DVec3::new(0.0, 0.0, -1.0),
        }
    }

    #[test]
    fn canvas_hit_center() {
        let seen = Cell::new((f64::NAN, f64::NAN));
        let t = canvas_hit(square(), down_at(0.0, 0.0), |u, v| {
            seen.set((u, v));
            255
        });
        assert_eq!(t, Some(10.0));
        let (u, v) = seen.get();
        assert!(
            (u - 0.5).abs() < 1e-12 && (v - 0.5).abs() < 1e-12,
            "{u} {v}"
        );

        // Oben links im Bild ist u, v klein (v zählt nach unten).
        canvas_hit(square(), down_at(-25.0, 25.0), |u, v| {
            seen.set((u, v));
            255
        })
        .expect("Treffer");
        let (u, v) = seen.get();
        assert!(
            (u - 0.25).abs() < 1e-12 && (v - 0.25).abs() < 1e-12,
            "{u} {v}"
        );

        // Neben dem Rechteck kein Treffer.
        assert_eq!(canvas_hit(square(), down_at(60.0, 0.0), |_, _| 255), None);
    }

    #[test]
    fn canvas_hit_transparent_pixel_misses() {
        // Linke Bildhälfte durchsichtig.
        let alpha = |u: f64, _v: f64| if u < 0.5 { 0 } else { 255 };
        assert_eq!(canvas_hit(square(), down_at(-25.0, 0.0), alpha), None);
        assert_eq!(canvas_hit(square(), down_at(25.0, 0.0), alpha), Some(10.0));
        // Ein kaum sichtbares Pixel zählt.
        assert_eq!(
            canvas_hit(square(), down_at(0.0, 0.0), |_, _| 1),
            Some(10.0)
        );
    }

    #[test]
    fn canvas_hit_behind_camera_none() {
        let away = Ray {
            origin: DVec3::new(0.0, 0.0, 10.0),
            dir: DVec3::new(0.0, 0.0, 1.0),
        };
        assert_eq!(canvas_hit(square(), away, |_, _| 255), None);
        let parallel = Ray {
            origin: DVec3::new(0.0, 0.0, 10.0),
            dir: DVec3::new(1.0, 0.0, 0.0),
        };
        assert_eq!(canvas_hit(square(), parallel, |_, _| 255), None);
        // Von hinten (unter der Ebene, nach oben) trifft es auch.
        let below = Ray {
            origin: DVec3::new(0.0, 0.0, -4.0),
            dir: DVec3::new(0.0, 0.0, 1.0),
        };
        assert_eq!(canvas_hit(square(), below, |_, _| 255), Some(4.0));
    }

    #[test]
    fn quad_uv_respects_flip() {
        let f = |c: [DVec3; 4]| c.map(|p| [p.x as f32, p.y as f32, p.z as f32]);
        let plain = quad_vertices(f(square()), 0.5);
        // Waagrecht gespiegelt: Die Lage liefert links und rechts vertauschte Ecken.
        let s = square();
        let flipped = quad_vertices(f([s[1], s[0], s[3], s[2]]), 0.5);

        let uv_at = |q: &[CanvasVertex; 6], x: f32, y: f32| {
            q.iter()
                .find(|v| v.position[0] == x && v.position[1] == y)
                .map(|v| v.uv)
                .expect("Ecke vorhanden")
        };
        // Ungespiegelt: oben links (0,0), unten rechts (1,1).
        assert_eq!(uv_at(&plain, -50.0, 50.0), [0.0, 0.0]);
        assert_eq!(uv_at(&plain, 50.0, -50.0), [1.0, 1.0]);
        // Gespiegelt: an der Weltstelle oben links liegt der rechte Bildrand.
        assert_eq!(uv_at(&flipped, -50.0, 50.0), [1.0, 0.0]);
        assert_eq!(uv_at(&flipped, 50.0, -50.0), [0.0, 1.0]);
        assert!(plain.iter().all(|v| v.opacity == 0.5));
        // Dreiecke (0,1,2) und (0,2,3).
        assert_eq!(plain[0], plain[3]);
        assert_eq!(plain[2], plain[4]);
    }

    fn draw(key: u64, opacity: f32, display_through: bool) -> CanvasDraw {
        CanvasDraw {
            key,
            corners: [[0.0; 3]; 4],
            opacity,
            display_through,
        }
    }

    #[test]
    fn batches_split_by_display_through_and_skip_missing() {
        let draws = [
            draw(1, 0.5, true),
            draw(2, 0.5, false),
            draw(3, 0.5, false), // ohne Textur
            draw(4, 0.0, false), // unsichtbar
            draw(5, 7.0, false), // wird auf 1 begrenzt
        ];
        let (verts, depth, through) = canvas_batches(&draws, |k| k != 3);
        assert_eq!(verts.len(), 18);
        assert_eq!(
            through,
            vec![CanvasBatch {
                key: 1,
                vertices: 0..6
            }]
        );
        assert_eq!(
            depth,
            vec![
                CanvasBatch {
                    key: 2,
                    vertices: 6..12
                },
                CanvasBatch {
                    key: 5,
                    vertices: 12..18
                }
            ]
        );
        assert!(verts[12..].iter().all(|v| v.opacity == 1.0));
    }

    #[test]
    fn cache_keeps_only_referenced_textures() {
        let mut cache: HashMap<u64, ()> = [(1, ()), (2, ()), (3, ())].into_iter().collect();
        prune_unreferenced(&mut cache, &[draw(2, 0.5, false), draw(9, 0.5, true)]);
        assert_eq!(cache.keys().copied().collect::<Vec<_>>(), vec![2]);
        prune_unreferenced(&mut cache, &[]);
        assert!(cache.is_empty());
    }

    #[test]
    fn texture_size_is_checked() {
        let tex = |w: u32, h: u32, len: usize| CanvasTexture {
            key: 1,
            width: w,
            height: h,
            rgba: vec![0u8; len].into(),
        };
        assert!(texture_fits(&tex(2, 3, 24), 4096));
        assert!(!texture_fits(&tex(2, 3, 23), 4096));
        assert!(!texture_fits(&tex(0, 3, 0), 4096));
        assert!(!texture_fits(&tex(4097, 1, 4097 * 4), 4096));
        assert!(!texture_fits(&tex(u32::MAX, u32::MAX, 0), u32::MAX));
    }

    /// Prüft ohne GPU, dass der Leinwand-Shader gültig ist.
    #[test]
    fn canvas_shader_is_valid_wgsl() {
        use naga::valid::{Capabilities, ValidationFlags, Validator};
        let module = naga::front::wgsl::parse_str(include_str!("canvas.wgsl"))
            .expect("WGSL muss sich parsen lassen");
        Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .expect("WGSL muss die Validierung bestehen");
        let entries: Vec<_> = module
            .entry_points
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert!(entries.contains(&"vs_canvas") && entries.contains(&"fs_canvas"));
        // Vormultipliziert: Die Deckkraft wirkt auf alle vier Kanäle.
        let src = include_str!("canvas.wgsl");
        assert!(
            src.contains("texel * input.opacity"),
            "Deckkraft muss rgba multiplizieren"
        );
    }

    fn tex(key: u64, w: u32, h: u32) -> CanvasTexture {
        CanvasTexture {
            key,
            width: w,
            height: h,
            rgba: vec![0u8; (w * h * 4) as usize].into(),
        }
    }

    #[test]
    fn resend_of_cached_key_does_not_recreate() {
        let mut failed = HashSet::new();
        assert_eq!(
            upload_step(&tex(1, 2, 2), false, &mut failed, 4096),
            UploadStep::Upload
        );
        // Dasselbe Bild im nächsten Bild noch einmal: schon da, nichts neu anlegen.
        assert_eq!(
            upload_step(&tex(1, 2, 2), true, &mut failed, 4096),
            UploadStep::Skip
        );
        assert!(failed.is_empty());
    }

    #[test]
    fn failed_key_is_logged_once() {
        let mut failed = HashSet::new();
        let big = tex(7, 8, 1);
        assert_eq!(
            upload_step(&big, false, &mut failed, 4),
            UploadStep::Reject { log: true }
        );
        for _ in 0..3 {
            assert_eq!(
                upload_step(&big, false, &mut failed, 4),
                UploadStep::Reject { log: false }
            );
        }
        // Solange die Leinwand gezeichnet wird, bleibt es gemerkt ...
        prune_failed(&mut failed, &[draw(7, 0.5, false)]);
        assert!(failed.contains(&7));
        // ... verschwindet sie, darf sie beim Wiederkommen erneut gemeldet werden.
        prune_failed(&mut failed, &[]);
        assert!(failed.is_empty());
        assert_eq!(
            upload_step(&big, false, &mut failed, 4),
            UploadStep::Reject { log: true }
        );
    }

    #[test]
    fn upload_premultiplies_alpha() {
        let out = premultiply(&[255, 128, 0, 128, 10, 20, 30, 0, 200, 100, 50, 255]);
        assert_eq!(out, vec![128, 64, 0, 128, 0, 0, 0, 0, 200, 100, 50, 255]);
    }
}
