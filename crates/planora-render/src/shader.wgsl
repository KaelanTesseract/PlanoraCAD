struct Uniforms {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    // Slice (Skizzen 12): Ebene n·p + d = 0, Kameraseite positiv; nur bei slice_on > 0,5.
    slice: vec4<f32>,
    slice_on: f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

// Muss zu `SLICE_EPS` in `stroke.rs` passen (Test in `gpu.rs`).
const SLICE_EPS: f32 = 0.001;

// Liegt `p` auf der Kameraseite vor der Schnittebene? Dann fällt der Bildpunkt weg.
fn sliced(p: vec3<f32>) -> bool {
    return u.slice_on > 0.5 && dot(u.slice.xyz, p) + u.slice.w > SLICE_EPS;
}

struct FaceOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world: vec3<f32>,
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
    out.world = position;
    return out;
}

@fragment
fn fs_face(input: FaceOut) -> @location(0) vec4<f32> {
    if sliced(input.world) {
        discard;
    }
    let n = normalize(input.normal);
    let light = normalize(u.light_dir.xyz);
    let diffuse = abs(dot(n, light));
    let intensity = 0.30 + 0.70 * diffuse;
    return vec4<f32>(input.color.rgb * intensity, input.color.a);
}

struct LineOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) world: vec3<f32>,
};

@vertex
fn vs_line(
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
) -> LineOut {
    var out: LineOut;
    out.clip = u.view_proj * vec4<f32>(position, 1.0);
    out.color = color;
    out.world = position;
    return out;
}

// Kanten der Körper: Slice schneidet sie wie die Flächen.
@fragment
fn fs_line(input: LineOut) -> @location(0) vec4<f32> {
    if sliced(input.world) {
        discard;
    }
    return input.color;
}

// Raster und Achsen: gehören nicht zu den Körpern, Slice schneidet sie nicht.
@fragment
fn fs_grid(input: LineOut) -> @location(0) vec4<f32> {
    return input.color;
}
