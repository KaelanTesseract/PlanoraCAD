//! Bild-Leser für die Leinwand (Skizzen 9): erkennt PNG, JPG und TIFF, prüft Endung, Größe und
//! Kantenlänge und entschlüsselt zu RGBA8. Dekompressionsbomben werden über `image::Limits`
//! begrenzt: Ein winziger Kopf, der riesige Maße behauptet, wird abgewiesen, bevor Speicher
//! reserviert wird.
//!
//! Der Modulname deckt sich mit dem Crate `image`; darum steht überall `::image` für das Crate.

use std::io::Cursor;

use ::image::imageops::FilterType;
use ::image::{ImageDecoder, ImageError, ImageFormat as Fmt, ImageReader, Limits};

use crate::error::ExchangeError;
use crate::limits::{MAX_IMAGE_BYTES, MAX_IMAGE_EDGE_PX};

/// Höchstens so viel Speicher darf ein Bild beim Entschlüsseln belegen (1 GiB).
const MAX_DECODE_ALLOC: u64 = 1024 * 1024 * 1024;

/// Bildformate, die eine Leinwand kennt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Tiff,
}

/// Format und Maße eines Bildes, ohne es ganz zu entschlüsseln.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageInfo {
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
}

fn limits() -> Limits {
    let mut l = Limits::default();
    l.max_image_width = Some(MAX_IMAGE_EDGE_PX);
    l.max_image_height = Some(MAX_IMAGE_EDGE_PX);
    l.max_alloc = Some(MAX_DECODE_ALLOC);
    l
}

fn broken(e: ImageError) -> ExchangeError {
    match e {
        ImageError::Limits(_) => ExchangeError::ImageTooBig,
        other => ExchangeError::ImageBroken(other.to_string()),
    }
}

/// Format aus der Dateiendung (ohne oder mit Punkt, Groß-/Kleinschreibung egal).
fn format_of_ext(ext: &str) -> Option<ImageFormat> {
    match ext.trim_start_matches('.').to_ascii_lowercase().as_str() {
        "png" => Some(ImageFormat::Png),
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "tif" | "tiff" => Some(ImageFormat::Tiff),
        _ => None,
    }
}

fn format_of_content(f: Fmt) -> Option<ImageFormat> {
    match f {
        Fmt::Png => Some(ImageFormat::Png),
        Fmt::Jpeg => Some(ImageFormat::Jpeg),
        Fmt::Tiff => Some(ImageFormat::Tiff),
        _ => None,
    }
}

fn check_size(bytes: &[u8]) -> Result<(), ExchangeError> {
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err(ExchangeError::ImageTooLarge);
    }
    Ok(())
}

/// Format und vorbereiteter Leser eines Bildes.
type Prepared<'a> = (ImageFormat, ImageReader<Cursor<&'a [u8]>>);

fn reader(bytes: &[u8]) -> Result<Prepared<'_>, ExchangeError> {
    let guessed = ::image::guess_format(bytes).map_err(|_| ExchangeError::ImageUnsupported)?;
    let format = format_of_content(guessed).ok_or(ExchangeError::ImageUnsupported)?;
    let mut r = ImageReader::with_format(Cursor::new(bytes), guessed);
    r.limits(limits());
    Ok((format, r))
}

/// Prüft ein Bild: Größe der Datei, Endung gegen Inhalt, Kantenlänge. `path_ext` ist die
/// Dateiendung (mit oder ohne Punkt). BMP und alles andere außer PNG, JPG und TIFF wird
/// abgewiesen.
pub fn probe_image(path_ext: &str, bytes: &[u8]) -> Result<ImageInfo, ExchangeError> {
    check_size(bytes)?;
    let by_ext = format_of_ext(path_ext).ok_or(ExchangeError::ImageUnsupported)?;
    let (format, r) = reader(bytes)?;
    if format != by_ext {
        return Err(ExchangeError::ImageWrongExtension);
    }
    let decoder = r.into_decoder().map_err(broken)?;
    let (width, height) = decoder.dimensions();
    if width > MAX_IMAGE_EDGE_PX || height > MAX_IMAGE_EDGE_PX {
        return Err(ExchangeError::ImageTooBig);
    }
    Ok(ImageInfo {
        format,
        width,
        height,
    })
}

/// Entschlüsselt zu RGBA8 (Breite, Höhe, Bytes). Ist eine Kante länger als `max_edge`, wird
/// das Bild verkleinert; das Seitenverhältnis bleibt, keine Kante wird kleiner als 1.
pub fn decode_rgba(bytes: &[u8], max_edge: u32) -> Result<(u32, u32, Vec<u8>), ExchangeError> {
    check_size(bytes)?;
    let (_, r) = reader(bytes)?;
    let img = r.decode().map_err(broken)?;
    let (w, h) = (img.width(), img.height());
    let max_edge = max_edge.max(1);
    let img = if w.max(h) > max_edge {
        let scale = f64::from(max_edge) / f64::from(w.max(h));
        let nw = ((f64::from(w) * scale).round() as u32).clamp(1, max_edge);
        let nh = ((f64::from(h) * scale).round() as u32).clamp(1, max_edge);
        img.resize_exact(nw, nh, FilterType::Triangle)
    } else {
        img
    };
    let rgba = img.into_rgba8();
    Ok((rgba.width(), rgba.height(), rgba.into_raw()))
}
