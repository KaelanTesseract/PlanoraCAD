//! Das Programmsymbol: als PNG in das Programm eingebettet, für das Fensterbild (Taskleiste), das
//! kleine Zeichen im oberen Feld und das Logo im Dialog „Über PlanoraCAD“. Lässt sich ein Bild
//! nicht lesen, fehlt nur das Symbol; das Programm läuft weiter.

use std::sync::Arc;

/// Symbol in 256 × 256 Pixeln (Fenstersymbol und Dialog).
pub const ICON_256: &[u8] = include_bytes!("../../../assets/logo/planoracad-icon-256.png");

/// Symbol in 64 × 64 Pixeln (kleines Zeichen im oberen Feld, 24 Punkte groß).
pub const ICON_64: &[u8] = include_bytes!("../../../assets/logo/planoracad-icon-64.png");

/// Kantenlänge des kleinen Zeichens im oberen Feld (Punkte).
pub const SMALL_SIZE: f32 = 24.0;

/// Kantenlänge des Logos im Dialog (Punkte).
pub const DIALOG_SIZE: f32 = 96.0;

/// Liest ein PNG als RGBA-Bild (8 Bit je Kanal, nicht vormultipliziert); `None`, wenn die Daten
/// kein lesbares PNG sind.
pub fn decode(bytes: &[u8]) -> Option<egui::IconData> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    if info.width == 0 || info.height == 0 || rgba.len() != (info.width * info.height * 4) as usize
    {
        return None;
    }
    Some(egui::IconData {
        rgba,
        width: info.width,
        height: info.height,
    })
}

/// Fenstersymbol aus den PNG-Daten `bytes`; `None` (kein Symbol, kein Absturz), wenn sie sich
/// nicht lesen lassen.
pub fn window_icon_from(bytes: &[u8]) -> Option<Arc<egui::IconData>> {
    decode(bytes).map(Arc::new)
}

/// Fenstersymbol des Programms (Taskleiste, Titelleiste).
pub fn window_icon() -> Option<Arc<egui::IconData>> {
    window_icon_from(ICON_256)
}

/// Bild für `bytes` (PNG) in `size` Punkten Kantenlänge, mit Textur im Zwischenspeicher des
/// Kontexts (`key` unterscheidet die Bilder). `None`, wenn die Daten nicht lesbar sind.
pub fn image(
    ctx: &egui::Context,
    key: &'static str,
    bytes: &[u8],
    size: f32,
) -> Option<egui::Image<'static>> {
    let id = egui::Id::new(("planora-logo", key));
    let texture = match ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        Some(t) => t,
        None => {
            let icon = decode(bytes)?;
            let color = egui::ColorImage::from_rgba_unmultiplied(
                [icon.width as usize, icon.height as usize],
                &icon.rgba,
            );
            let t = ctx.load_texture(key, color, egui::TextureOptions::LINEAR);
            ctx.data_mut(|d| d.insert_temp(id, t.clone()));
            t
        }
    };
    Some(
        egui::Image::new(&texture)
            .fit_to_exact_size(egui::vec2(size, size))
            .alt_text("PlanoraCAD-Symbol"),
    )
}
