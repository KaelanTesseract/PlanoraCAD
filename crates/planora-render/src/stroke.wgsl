// Skizzenlinien und -flaechen (Skizzen 12): Eckpunkte kommen schon in NDC an (Breite und
// Tiefenversatz sind auf der CPU berechnet). Der Fragment-Shader glaettet die Kanten quer zur
// Linie ueber `edge` (-1...1 bis zum Rand des Saums, Flaechen 0).

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
    // `edge` laeuft von -1 bis 1 ueber das Viereck, das einen Saum von 0,5 px je Seite ueber die
    // Linienbreite hinaus hat (STROKE_FEATHER_PX). (1 - |edge|) / |Gefaelle| ist der Abstand zum
    // aeusseren Rand in Pixeln; ab 1 px voll deckend. Muss zu `stroke_alpha` in stroke.rs passen.
    // Flaechen (edge = 0, kein Gefaelle) bleiben voll deckend.
    let g = max(length(vec2<f32>(dpdx(input.edge), dpdy(input.edge))), 0.0001);
    let a = clamp((1.0 - abs(input.edge)) / g, 0.0, 1.0);
    return vec4<f32>(input.color.rgb, input.color.a * a);
}
