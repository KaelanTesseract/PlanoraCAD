use crate::scene::{FaceVertex, LineVertex, SceneData};
use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu::util::DeviceExt as _;
use egui_wgpu::{self, wgpu};
use std::sync::Arc;

/// Muss zu `NativeOptions::multisampling` und `NativeOptions::depth_buffer` passen.
pub const MSAA_SAMPLES: u32 = 4;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    light_dir: [f32; 4],
}

/// GPU-Ressourcen des Viewports. Liegen in `egui_wgpu::CallbackResources`.
pub struct GpuScene {
    face_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    grid_pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    face_vertex_buffer: Option<wgpu::Buffer>,
    face_index_buffer: Option<wgpu::Buffer>,
    face_index_count: u32,
    line_vertex_buffer: Option<wgpu::Buffer>,
    line_vertex_count: u32,
    /// Raster und Achsen: ändern sich mit der Kamera, daher je Bild beschrieben.
    grid_vertex_buffer: Option<wgpu::Buffer>,
    grid_capacity: u32,
    grid_vertex_count: u32,
    uploaded_version: Option<u64>,
}

impl GpuScene {
    /// Legt die Pipelines an und hinterlegt sie in den Callback-Ressourcen von egui.
    pub fn install(render_state: &egui_wgpu::RenderState) {
        let gpu = Self::new(&render_state.device, render_state.target_format);
        render_state.renderer.write().callback_resources.insert(gpu);
    }

    fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("planora-viewport"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("planora-viewport"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("planora-viewport"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("planora-uniforms"),
            contents: bytemuck::bytes_of(&Uniforms {
                view_proj: [[0.0; 4]; 4],
                light_dir: [0.0, 0.0, 1.0, 0.0],
            }),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("planora-viewport"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let multisample = wgpu::MultisampleState {
            count: MSAA_SAMPLES,
            ..Default::default()
        };

        let face_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("planora-faces"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_face"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<FaceVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x3,
                        1 => Float32x3,
                        2 => Float32x4
                    ],
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_face"),
                targets: &[Some(target_format.into())],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                // Flächen leicht nach hinten schieben, damit Kanten darüber sauber erscheinen.
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample,
            multiview_mask: None,
            cache: None,
        });

        // Kantenlinien der Körper (deckend) und Raster (mit Alphamischung, blendet aus).
        let make_line_pipeline = |label: &'static str, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_line"),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<LineVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4],
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_line"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::LineList,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample,
                multiview_mask: None,
                cache: None,
            })
        };
        let line_pipeline = make_line_pipeline("planora-lines", None);
        let grid_pipeline =
            make_line_pipeline("planora-grid", Some(wgpu::BlendState::ALPHA_BLENDING));

        Self {
            face_pipeline,
            line_pipeline,
            grid_pipeline,
            uniform_buffer,
            bind_group,
            face_vertex_buffer: None,
            face_index_buffer: None,
            face_index_count: 0,
            line_vertex_buffer: None,
            line_vertex_count: 0,
            grid_vertex_buffer: None,
            grid_capacity: 0,
            grid_vertex_count: 0,
            uploaded_version: None,
        }
    }

    fn upload(&mut self, device: &wgpu::Device, scene: &SceneData) {
        let max_buffer_size = device.limits().max_buffer_size;
        let mut too_large: Vec<&str> = Vec::new();
        let mut buffer = |label: &'static str, bytes: &[u8], usage| {
            if bytes.is_empty() {
                return None;
            }
            if !buffer_fits(bytes.len(), max_buffer_size) {
                too_large.push(label);
                return None;
            }
            Some(
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: bytes,
                    usage,
                }),
            )
        };
        let face_vertex_buffer = buffer(
            "planora-face-vertices",
            bytemuck::cast_slice(&scene.face_vertices),
            wgpu::BufferUsages::VERTEX,
        );
        let face_index_buffer = buffer(
            "planora-face-indices",
            bytemuck::cast_slice(&scene.face_indices),
            wgpu::BufferUsages::INDEX,
        );
        let line_vertex_buffer = buffer(
            "planora-line-vertices",
            bytemuck::cast_slice(&scene.line_vertices),
            wgpu::BufferUsages::VERTEX,
        );

        // Ohne passende Puffer bzw. bei nicht darstellbarer Anzahl wird der Teil nicht gezeichnet.
        match (
            face_vertex_buffer,
            face_index_buffer,
            u32::try_from(scene.face_indices.len()),
        ) {
            (Some(vb), Some(ib), Ok(count)) => {
                self.face_vertex_buffer = Some(vb);
                self.face_index_buffer = Some(ib);
                self.face_index_count = count;
            }
            _ => {
                self.face_vertex_buffer = None;
                self.face_index_buffer = None;
                self.face_index_count = 0;
            }
        }
        match (line_vertex_buffer, u32::try_from(scene.line_vertices.len())) {
            (Some(lvb), Ok(count)) => {
                self.line_vertex_buffer = Some(lvb);
                self.line_vertex_count = count;
            }
            _ => {
                self.line_vertex_buffer = None;
                self.line_vertex_count = 0;
            }
        }
        if !too_large.is_empty() {
            eprintln!(
                "Warning: scene too large for the graphics card (max. {max_buffer_size} bytes per buffer), not drawn: {}",
                too_large.join(", ")
            );
        }
    }

    fn update(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, cb: &ViewportCallback) {
        let uniforms = Uniforms {
            view_proj: cb.view_proj,
            light_dir: [cb.light_dir[0], cb.light_dir[1], cb.light_dir[2], 0.0],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        if self.uploaded_version != Some(cb.scene_version) {
            self.upload(device, &cb.scene);
            self.uploaded_version = Some(cb.scene_version);
        }
        self.upload_grid(device, queue, &cb.grid);
    }

    /// Schreibt Raster und Achsen dieses Bildes; der Puffer wächst bei Bedarf (Zweierpotenz).
    fn upload_grid(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, grid: &[LineVertex]) {
        self.grid_vertex_count = 0;
        let Ok(count) = u32::try_from(grid.len()) else {
            return;
        };
        if count == 0 {
            return;
        }
        if self.grid_vertex_buffer.is_none() || count > self.grid_capacity {
            let capacity = count.next_power_of_two().max(1024);
            let bytes = u64::from(capacity) * std::mem::size_of::<LineVertex>() as u64;
            if bytes > device.limits().max_buffer_size {
                return;
            }
            self.grid_vertex_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("planora-grid-vertices"),
                size: bytes,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.grid_capacity = capacity;
        }
        if let Some(buffer) = &self.grid_vertex_buffer {
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(grid));
            self.grid_vertex_count = count;
        }
    }

    fn paint(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        if let (Some(vb), Some(ib)) = (&self.face_vertex_buffer, &self.face_index_buffer) {
            render_pass.set_pipeline(&self.face_pipeline);
            render_pass.set_vertex_buffer(0, vb.slice(..));
            render_pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..self.face_index_count, 0, 0..1);
        }
        // Raster vor den Kanten, damit es sie nicht überdeckt.
        if let (Some(gvb), true) = (&self.grid_vertex_buffer, self.grid_vertex_count > 0) {
            render_pass.set_pipeline(&self.grid_pipeline);
            render_pass.set_vertex_buffer(0, gvb.slice(..));
            render_pass.draw(0..self.grid_vertex_count, 0..1);
        }
        if let Some(lvb) = &self.line_vertex_buffer {
            render_pass.set_pipeline(&self.line_pipeline);
            render_pass.set_vertex_buffer(0, lvb.slice(..));
            render_pass.draw(0..self.line_vertex_count, 0..1);
        }
    }
}

/// Passt ein Puffer mit `bytes` Byte unter die Obergrenze des Geräts?
fn buffer_fits(bytes: usize, max_buffer_size: u64) -> bool {
    u64::try_from(bytes).is_ok_and(|b| b <= max_buffer_size)
}

/// Wird pro Frame mit `egui_wgpu::Callback::new_paint_callback(rect, …)` in egui eingehängt.
pub struct ViewportCallback {
    pub view_proj: [[f32; 4]; 4],
    pub light_dir: [f32; 3],
    pub scene: Arc<SceneData>,
    /// Erhöht sich, wenn sich `scene` geändert hat; steuert das Neuladen der Puffer.
    pub scene_version: u64,
    /// Raster und Achsen (Linienliste) für dieses Bild, aus der Kamera berechnet.
    pub grid: Vec<LineVertex>,
}

impl egui_wgpu::CallbackTrait for ViewportCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(gpu) = resources.get_mut::<GpuScene>() {
            gpu.update(device, queue, self);
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(gpu) = resources.get::<GpuScene>() {
            gpu.paint(render_pass);
        }
    }
}

#[cfg(test)]
mod tests {
    use naga::valid::{Capabilities, ValidationFlags, Validator};

    #[test]
    fn buffers_above_the_device_limit_are_refused() {
        let limit = 256 * 1024 * 1024;
        assert!(super::buffer_fits(16, limit));
        assert!(super::buffer_fits(limit as usize, limit));
        assert!(!super::buffer_fits(limit as usize + 1, limit));
        assert!(!super::buffer_fits(usize::MAX, 0));
    }

    /// Prüft ohne GPU, dass der Shader syntaktisch und typseitig gültig ist.
    #[test]
    fn shader_is_valid_wgsl() {
        let module = naga::front::wgsl::parse_str(include_str!("shader.wgsl"))
            .expect("WGSL muss sich parsen lassen");
        Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .expect("WGSL muss die Validierung bestehen");
        let entries: Vec<_> = module
            .entry_points
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        for name in ["vs_face", "fs_face", "vs_line", "fs_line"] {
            assert!(entries.contains(&name), "Einstiegspunkt {name} fehlt");
        }
    }
}
