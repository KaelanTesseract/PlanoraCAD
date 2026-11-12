// Leinwände (Skizzen 9): texturiertes Rechteck, Alpha des Bildes mal Deckkraft.
// Die Textur ist beim Hochladen vormultipliziert, gemischt wird mit
// BlendState::PREMULTIPLIED_ALPHA_BLENDING; die Deckkraft wirkt darum auf alle vier Kanäle.
// Rgba8Unorm, wie die Flächenfarben unverändert ins (nicht-sRGB-)Ziel geschrieben.

struct Uniforms {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var image: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;

struct CanvasOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) opacity: f32,
};

@vertex
fn vs_canvas(
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) opacity: f32,
) -> CanvasOut {
    var out: CanvasOut;
    out.clip = u.view_proj * vec4<f32>(position, 1.0);
    out.uv = uv;
    out.opacity = opacity;
    return out;
}

@fragment
fn fs_canvas(input: CanvasOut) -> @location(0) vec4<f32> {
    let texel = textureSample(image, image_sampler, input.uv);
    let color = texel * input.opacity;
    // Ganz durchsichtige Stellen auch nicht in die Tiefe oder die Mischung geben.
    if color.a <= 0.0 {
        discard;
    }
    return color;
}
