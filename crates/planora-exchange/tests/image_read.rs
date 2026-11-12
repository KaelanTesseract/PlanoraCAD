//! Skizzen 9, Task 5: Bild-Leser der Leinwand. Alle Bilder entstehen im Test im Speicher.

use std::io::Cursor;

use image::{ImageFormat as Fmt, Rgb, RgbImage, RgbaImage};
use planora_exchange::{
    ExchangeError, ImageFormat, MAX_IMAGE_BYTES, MAX_IMAGE_EDGE_PX, crc32, decode_rgba, probe_image,
};
use planora_i18n::{english, with_lang};

fn encode(w: u32, h: u32, fmt: Fmt) -> Vec<u8> {
    let img = RgbImage::from_pixel(w, h, Rgb([200, 30, 60]));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, fmt).unwrap();
    out.into_inner()
}

/// PNG, dessen Kopf (IHDR) Maße behauptet, die die Daten nicht halten.
fn png_claiming(w: u32, h: u32) -> Vec<u8> {
    let mut b = encode(1, 1, Fmt::Png);
    // 8 Byte Signatur, 4 Länge, 4 "IHDR", dann Breite und Höhe, danach die CRC über Typ + Daten.
    b[16..20].copy_from_slice(&w.to_be_bytes());
    b[20..24].copy_from_slice(&h.to_be_bytes());
    let crc = crc32(&b[12..29]);
    b[29..33].copy_from_slice(&crc.to_be_bytes());
    b
}

#[test]
fn png_jpg_tiff_sizes() {
    for (ext, fmt, want) in [
        ("png", Fmt::Png, ImageFormat::Png),
        (".JPG", Fmt::Jpeg, ImageFormat::Jpeg),
        ("jpeg", Fmt::Jpeg, ImageFormat::Jpeg),
        ("tif", Fmt::Tiff, ImageFormat::Tiff),
        ("tiff", Fmt::Tiff, ImageFormat::Tiff),
    ] {
        let info = probe_image(ext, &encode(2, 3, fmt)).unwrap();
        assert_eq!(
            (info.format, info.width, info.height),
            (want, 2, 3),
            "{ext}"
        );
    }
}

#[test]
fn extension_must_match_content() {
    let png = encode(2, 3, Fmt::Png);
    let err = probe_image("jpg", &png).unwrap_err();
    assert!(matches!(err, ExchangeError::ImageWrongExtension));
    with_lang(&english(), || {
        assert!(err.to_string().contains("extension"), "{err}");
    });
    // Die Schlüssel-Meldung selbst:
    assert!(matches!(
        probe_image("png", &encode(2, 3, Fmt::Jpeg)),
        Err(ExchangeError::ImageWrongExtension)
    ));
}

#[test]
fn bmp_and_unknown_are_rejected() {
    let bmp = encode(2, 3, Fmt::Bmp);
    assert!(matches!(
        probe_image("bmp", &bmp),
        Err(ExchangeError::ImageUnsupported)
    ));
    assert!(matches!(
        probe_image("png", &bmp),
        Err(ExchangeError::ImageUnsupported)
    ));
    assert!(matches!(
        probe_image("gif", &encode(2, 3, Fmt::Png)),
        Err(ExchangeError::ImageUnsupported)
    ));
}

#[test]
fn too_big_bytes_or_edge_rejected() {
    // Datei über 64 MiB: allein die Länge zählt.
    let huge = vec![0u8; MAX_IMAGE_BYTES as usize + 1];
    assert!(matches!(
        probe_image("png", &huge),
        Err(ExchangeError::ImageTooLarge)
    ));
    assert!(matches!(
        decode_rgba(&huge, 4096),
        Err(ExchangeError::ImageTooLarge)
    ));
    // Kante knapp über der Grenze (gültiger Kopf, kleine Datei).
    assert!(matches!(
        probe_image("png", &png_claiming(MAX_IMAGE_EDGE_PX + 1, 1)),
        Err(ExchangeError::ImageTooBig)
    ));
}

#[test]
fn decompression_bomb_header_is_bounded() {
    // Winziges PNG, das 16384 x 16384 behauptet: darf nicht im Speicher landen.
    let bomb = png_claiming(MAX_IMAGE_EDGE_PX, MAX_IMAGE_EDGE_PX);
    assert!(bomb.len() < 100);
    assert!(decode_rgba(&bomb, 4096).is_err());
    // Und mit riesigen Maßen (u32-Spitze) ebenfalls.
    let bomb = png_claiming(u32::MAX >> 1, u32::MAX >> 1);
    assert!(probe_image("png", &bomb).is_err());
    assert!(decode_rgba(&bomb, 4096).is_err());
}

#[test]
fn decode_downscales_to_4096() {
    let (w, h, px) = decode_rgba(&encode(5000, 100, Fmt::Png), 4096).unwrap();
    assert_eq!((w, h), (4096, 82));
    assert_eq!(px.len(), 4096 * 82 * 4);
    let (w, h, px) = decode_rgba(&encode(100, 5000, Fmt::Png), 4096).unwrap();
    assert_eq!((w, h), (82, 4096));
    assert_eq!(px.len(), 82 * 4096 * 4);
}

#[test]
fn decode_keeps_small_images_and_alpha() {
    let mut img = RgbaImage::new(2, 3);
    img.put_pixel(1, 2, image::Rgba([1, 2, 3, 4]));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, Fmt::Png).unwrap();
    let (w, h, px) = decode_rgba(&out.into_inner(), 4096).unwrap();
    assert_eq!((w, h), (2, 3));
    assert_eq!(&px[(2 * 2 + 1) * 4..][..4], &[1, 2, 3, 4]);
}

#[test]
fn broken_image_is_error_not_panic() {
    let mut png = encode(20, 20, Fmt::Png);
    png.truncate(png.len() / 2);
    assert!(decode_rgba(&png, 4096).is_err());
    assert!(decode_rgba(&[], 4096).is_err());
    assert!(decode_rgba(b"nicht im geringsten ein Bild", 4096).is_err());
    assert!(probe_image("png", &[]).is_err());
    assert!(probe_image("png", &png[..12]).is_err());
    let mut jpg = encode(20, 20, Fmt::Jpeg);
    jpg.truncate(30);
    assert!(decode_rgba(&jpg, 4096).is_err());
    assert!(probe_image("jpg", &jpg).is_err() || decode_rgba(&jpg, 4096).is_err());
}
