struct Uniforms {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct FaceOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_face(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
) -> FaceOut {
    var out: FaceOut;
    out.clip = u.view_proj * vec4<f32>(position, 1.0);
    out.normal = normal;
    out.color = color;
    return out;
}

@fragment
fn fs_face(input: FaceOut) -> @location(0) vec4<f32> {
    let n = normalize(input.normal);
    let light = normalize(u.light_dir.xyz);
    let diffuse = abs(dot(n, light));
    let intensity = 0.30 + 0.70 * diffuse;
    return vec4<f32>(input.color.rgb * intensity, input.color.a);
}

struct LineOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_line(
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
) -> LineOut {
    var out: LineOut;
    out.clip = u.view_proj * vec4<f32>(position, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs_line(input: LineOut) -> @location(0) vec4<f32> {
    return input.color;
}
