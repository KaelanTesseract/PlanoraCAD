//! Eingebettete Bilder: Programmsymbol und vier Screenshots (1280 × 720, aus dem Wiki
//! verkleinert). Die Texturen werden beim Start einmal geladen; ein unlesbares Bild fehlt nur.

use std::sync::Arc;

/// Programmsymbol 256 × 256 (Fenstersymbol und Logo neben dem Schriftzug).
pub const ICON_256: &[u8] = include_bytes!("../../../../assets/logo/planoracad-icon-256.png");
/// Startseite der App (Willkommen).
const STARTSEITE: &[u8] = include_bytes!("../../assets/startseite.png");
/// Skizze zeichnen (Karte auf der Installieren-Seite).
const SKIZZE: &[u8] = include_bytes!("../../assets/skizze.png");
/// Zeichnung (Karte auf der Installieren-Seite).
const ZEICHNUNG: &[u8] = include_bytes!("../../assets/zeichnung.png");
/// Schnittansicht (Karte auf der Installieren-Seite).
const SCHNITT: &[u8] = include_bytes!("../../assets/schnitt.png");

/// Liest ein PNG als RGBA-Bild (8 Bit je Kanal, nicht vormultipliziert); `None`, wenn die Daten
/// kein lesbares PNG sind. Wie `logo::decode` der App.
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

/// Fenstersymbol (Taskleiste, Titelleiste); `None` bei unlesbarem Bild.
pub fn window_icon() -> Option<Arc<egui::IconData>> {
    decode(ICON_256).map(Arc::new)
}

/// Eine geladene Textur mit Seitenverhältnis (Breite / Höhe).
pub struct Picture {
    pub texture: egui::TextureHandle,
    pub aspect: f32,
}

/// Alle Bilder der Oberfläche.
pub struct Assets {
    pub logo: Option<Picture>,
    pub welcome: Option<Picture>,
    /// Karten der Installieren-Seite: Skizze, Zeichnung, Schnitt.
    pub teasers: [Option<Picture>; 3],
}

impl Assets {
    /// Lädt alle Bilder einmal als Texturen.
    pub fn load(ctx: &egui::Context) -> Assets {
        let load = |name: &str, bytes: &[u8]| {
            let icon = decode(bytes)?;
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [icon.width as usize, icon.height as usize],
                &icon.rgba,
            );
            Some(Picture {
                texture: ctx.load_texture(name, image, egui::TextureOptions::LINEAR),
                aspect: icon.width as f32 / icon.height as f32,
            })
        };
        Assets {
            logo: load("setup-logo", ICON_256),
            welcome: load("setup-startseite", STARTSEITE),
            teasers: [
                load("setup-skizze", SKIZZE),
                load("setup-zeichnung", ZEICHNUNG),
                load("setup-schnitt", SCHNITT),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_images_decode() {
        let icon = decode(ICON_256).expect("Symbol lesbar");
        assert_eq!((icon.width, icon.height), (256, 256));
        for bytes in [STARTSEITE, SKIZZE, ZEICHNUNG, SCHNITT] {
            let shot = decode(bytes).expect("Screenshot lesbar");
            assert_eq!((shot.width, shot.height), (1280, 720));
        }
        assert!(decode(b"kein png").is_none());
    }
}
