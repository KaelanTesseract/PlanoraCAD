// Skizzenlinien und -flaechen (Skizzen 12): Eckpunkte kommen schon in NDC an (Breite und
// Tiefenversatz sind auf der CPU berechnet). Der Fragment-Shader glaettet die Kanten quer zur
// Linie ueber `edge` (-1...1, Flaechen 0).

struct StrokeIn {
    @location(0) ndc: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) edge: f32,
};

struct StrokeOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) edge: f32,
};

@vertex
fn vs_stroke(input: StrokeIn) -> StrokeOut {
    var out: StrokeOut;
    out.clip = vec4<f32>(input.ndc, 1.0);
    out.color = input.color;
    out.edge = input.edge;
    return out;
}

@fragment
fn fs_stroke(input: StrokeOut) -> @location(0) vec4<f32> {
    // `edge` laeuft ueber die Linienbreite von -1 bis 1; fwidth ist 2 / Breite in Pixeln, ein
    // Pixel Randstreifen also fwidth / 2 = 1 / Breite. Flaechen (edge = 0) bleiben voll deckend.
    let fade = max(fwidth(input.edge) * 0.5, 0.0001);
    let a = 1.0 - smoothstep(1.0 - fade, 1.0, abs(input.edge));
    return vec4<f32>(input.color.rgb, input.color.a * a);
}
