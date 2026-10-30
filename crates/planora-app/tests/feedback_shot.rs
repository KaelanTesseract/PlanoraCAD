//! Bild vom Fenster: PNG-Kodierung, Speichern im Rückmeldungsordner und der Ablauf
//! verbergen → anfordern → wieder zeigen (mit eingespeister Uhr).

mod common;

use egui::{Color32, ColorImage};
use planora_app::feedback_shot::*;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn dummy(w: usize, h: usize) -> ColorImage {
    let pixels = (0..w * h)
        .map(|i| Color32::from_rgb((i * 20) as u8, (i * 7) as u8, 200))
        .collect();
    ColorImage::new([w, h], pixels)
}

#[test]
fn constants() {
    assert_eq!(SHOT_FILE, "bild.png");
    assert_eq!(SHOT_TIMEOUT, Duration::from_secs(3));
    assert_eq!(shot_failed(), "Das Bild hat nicht geklappt.");
}

#[test]
fn encode_roundtrips_a_4x3_dummy() {
    let img = dummy(4, 3);
    let bytes = encode_png(&img).unwrap();
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!((info.width, info.height), (4, 3));
    assert_eq!(info.color_type, png::ColorType::Rgba);
    let back: Vec<Color32> = buf[..info.buffer_size()]
        .chunks(4)
        .map(|p| Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
        .collect();
    assert_eq!(back, img.pixels);
}

#[test]
fn save_png_writes_bild_png_in_the_bundle_dir() {
    let dir = common::temp_root("planora-shot-speichern")
        .join("rueckmeldungen")
        .join("x");
    let path = save_png(&dir, &dummy(2, 2)).unwrap();
    assert_eq!(path, dir.join("bild.png"));
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn save_png_into_unwritable_dir_fails_cleanly() {
    let root = common::temp_root("planora-shot-gesperrt");
    std::fs::create_dir_all(&root).unwrap();
    // Wo der Ordner hin soll, liegt eine Datei.
    let blocked = root.join("kein-ordner");
    std::fs::write(&blocked, "Datei").unwrap();
    assert!(save_png(&blocked, &dummy(2, 2)).is_err());
}

#[test]
fn flow_hides_then_requests_then_shows_again() {
    let t0 = Instant::now();
    let mut f = ShotFlow::default();
    assert!(!f.hides_dialog());
    assert_eq!(f.tick(t0), ShotStep::Nothing);
    f.start();
    assert!(f.hides_dialog(), "erst ein Bild ohne Fenster");
    assert_eq!(f.tick(t0), ShotStep::RequestScreenshot);
    assert!(f.hides_dialog(), "während des Wartens verborgen");
    assert_eq!(f.tick(t0 + Duration::from_millis(100)), ShotStep::Nothing);
    let img = Arc::new(dummy(3, 2));
    assert_eq!(f.received(img.clone()), ShotStep::ShowDialogAgain);
    assert!(!f.hides_dialog());
    assert!(Arc::ptr_eq(f.image().unwrap(), &img));
    assert_eq!(preview_line(&img), "bild.png, 3 × 2 pixels");
}

#[test]
fn timeout_shows_dialog_again_with_failed_hint() {
    let t0 = Instant::now();
    let mut f = ShotFlow::default();
    f.start();
    assert_eq!(f.tick(t0), ShotStep::RequestScreenshot);
    assert_eq!(
        f.tick(t0 + SHOT_TIMEOUT - Duration::from_millis(1)),
        ShotStep::Nothing
    );
    assert_eq!(f.tick(t0 + SHOT_TIMEOUT), ShotStep::ShowDialogAgain);
    assert!(matches!(f.phase, ShotPhase::Failed));
    assert!(!f.hides_dialog());
    assert!(f.image().is_none());
    // Ein zu spätes Bild wird nicht mehr angenommen.
    assert_eq!(f.received(Arc::new(dummy(1, 1))), ShotStep::Nothing);
    assert!(f.image().is_none());
}

#[test]
fn foreign_screenshot_events_are_ignored() {
    assert!(is_our_shot(&shot_user_data()));
    assert!(!is_our_shot(&egui::UserData::default()));
    assert!(!is_our_shot(&egui::UserData::new("anderes_bild")));
    assert!(!is_our_shot(&egui::UserData::new(42_u32)));
    // Ohne Anforderung bleibt ein Bild unbeachtet.
    let mut f = ShotFlow::default();
    assert_eq!(f.received(Arc::new(dummy(1, 1))), ShotStep::Nothing);
    assert!(f.image().is_none());
}
