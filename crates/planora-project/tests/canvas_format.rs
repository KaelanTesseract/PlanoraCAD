//! Dateiformat 19 (Skizzen 9, Task 9, Spec F): Leinwände mit eingebetteten Bildern (Base64 in
//! `bilder`) und der Kontrollpunktrahmen an Skizzenkurven (`rahmen`). Format 19 wird nur
//! geschrieben, wenn eine Leinwand oder ein Rahmen im Dokument ist; alte Dateien bleiben
//! bytegleich.

use planora_exchange::import2d::{Element2d, Import2d, Layer2d};
use planora_exchange::{ImageFormat, MAX_IMAGE_BYTES};
use planora_feature::*;
use planora_i18n::tr;
use planora_math::{DVec2, Frame, Tolerance};
use planora_project::{
    FORMAT_VERSION, ProjectError, base64, decode_version, encode_version, required_format,
};
use planora_sketch::CurveKind;
use planora_sketch::Sketch;
use planora_sketch::import::{InsertOptions, Placement};
use planora_sketch::text_shape::{TextShape, TextShapeError, TextShaper, TextStyle};
use std::io::Cursor;
use std::sync::Arc;

fn format_of(text: &str) -> u64 {
    let v: serde_json::Value = serde_json::from_str(text).unwrap();
    v["format"].as_u64().unwrap()
}

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

/// Bild der Größe `w` × `h` mit einem Farbverlauf (jedes Pixel anders).
fn encoded_image(w: u32, h: u32, fmt: image::ImageFormat) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([x as u8, y as u8, 7]));
    let mut out = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut out, fmt)
        .unwrap();
    out.into_inner()
}

fn png(w: u32, h: u32) -> Vec<u8> {
    encoded_image(w, h, image::ImageFormat::Png)
}

fn canvas_image(bytes: Vec<u8>, format: ImageFormat, width: u32, height: u32) -> CanvasImage {
    CanvasImage {
        hash: image_hash(&bytes),
        bytes: Arc::from(bytes),
        format,
        width,
        height,
    }
}

/// Leinwand, bei der jede Einstellung von der Vorgabe abweicht.
fn odd_canvas(image: ImageId) -> CanvasFeature {
    CanvasFeature {
        offset: v(3.5, -4.25),
        angle: 0.3,
        scale: v(2.0, 0.5),
        flip_h: true,
        flip_v: false,
        opacity: 70,
        display_through: false,
        selectable: false,
        ..CanvasFeature::new(image, CanvasBase::Standard(StandardPlane::XZ))
    }
}

/// Dokument mit einer Leinwand (PNG 3 × 2).
fn canvas_doc() -> (Document, FeatureId) {
    let mut d = Document::new();
    let img = d.add_image(canvas_image(png(3, 2), ImageFormat::Png, 3, 2));
    let id = d
        .add_feature("Leinwand1", FeatureKind::Canvas(odd_canvas(img)))
        .unwrap();
    assert_eq!(d.feature(id).unwrap().status, FeatureStatus::Ok);
    (d, id)
}

fn canvas_of(s: &Snapshot, id: FeatureId) -> &CanvasFeature {
    let f = s.features.iter().find(|f| f.id == id).unwrap();
    let FeatureKind::Canvas(c) = &f.kind else {
        panic!("keine Leinwand");
    };
    c
}

fn sketch_doc(sketch: Sketch) -> Document {
    let mut d = Document::new();
    d.add_feature(
        "Skizze1",
        FeatureKind::Sketch(SketchFeature {
            sketch,
            plane: SketchPlane::Standard(StandardPlane::XY),
            copies: vec![],
            formulas: Default::default(),
        }),
    )
    .unwrap();
    d
}

fn sketch_of(s: &Snapshot) -> &Sketch {
    let FeatureKind::Sketch(sf) = &s.features[0].kind else {
        panic!("keine Skizze");
    };
    &sf.sketch
}

/// Setzer ohne Schrift (die Tests fügen keine Texte ein).
struct NoText;

impl TextShaper for NoText {
    fn shape(&self, _: &str, _: &TextStyle) -> Result<TextShape, TextShapeError> {
        Err(TextShapeError::Empty)
    }
}

fn inserted(elements: Vec<Element2d>, show_frame: bool) -> Sketch {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze1", Tolerance::default());
    let imp = Import2d {
        layers: vec![Layer2d {
            name: "0".into(),
            elements,
        }],
        ..Default::default()
    };
    let opt = InsertOptions {
        show_frame,
        ..Default::default()
    };
    sk.insert_geometry(&imp, &Placement::default(), &opt, &NoText)
        .unwrap();
    sk
}

fn cubic() -> Element2d {
    Element2d::Cubic([v(0.0, 0.0), v(1.0, 2.0), v(3.0, 2.0), v(4.0, 0.0)])
}

fn rational_quarter_circle() -> Element2d {
    let w = std::f64::consts::FRAC_1_SQRT_2;
    Element2d::Nurbs {
        degree: 2,
        knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        points: vec![v(1.0, 0.0), v(1.0, 1.0), v(0.0, 1.0)],
        weights: Some(vec![1.0, w, 1.0]),
    }
}

fn edited(text: &str, f: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut v: serde_json::Value = serde_json::from_str(text).unwrap();
    f(&mut v);
    serde_json::to_string_pretty(&v).unwrap()
}

fn err_text(text: &str) -> String {
    decode_version(text, "x.json").unwrap_err().to_string()
}

#[test]
fn format_version_is_19() {
    assert_eq!(FORMAT_VERSION, 21);
}

#[test]
fn old_file_without_canvas_stays_byte_equal() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut seen = 0;
    for dir in ["fixtures_format18", "data", "fixtures", "step_fixtures"] {
        let mut entries: Vec<_> = std::fs::read_dir(root.join(dir))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        entries.sort();
        for p in entries {
            let name = p.display().to_string();
            let raw = std::fs::read_to_string(&p).unwrap().replace("\r\n", "\n");
            let text = raw.trim_end_matches('\n');
            let snap = decode_version(text, &name).unwrap_or_else(|e| panic!("{name}: {e}"));
            let back = encode_version(&snap).unwrap();
            assert_eq!(back, text, "{name}");
            assert_eq!(format_of(&back), format_of(text), "{name}");
            assert!(format_of(&back) <= 18, "{name}");
            // (`rahmen` gibt es dort schon als Lage eines Textes, darum nur `bilder` prüfen.)
            assert!(!back.contains("\"bilder\""), "{name}");
            seen += 1;
        }
    }
    // Die Datei aus Format 18 (mit Text) ist dabei.
    assert!(seen >= 14, "{seen}");
}

#[test]
fn canvas_roundtrip_keeps_bytes_and_settings() {
    let (d, id) = canvas_doc();
    let snap = d.snapshot();
    let before = canvas_of(&snap, id).clone();
    assert!(before.last_base.is_some(), "last_base nach dem Rechnen");
    let text = encode_version(&snap).unwrap();
    assert_eq!(format_of(&text), 19);
    assert!(text.contains("\"bilder\""));
    assert!(text.contains("\"leinwand\""));
    let back = decode_version(&text, "x.json").unwrap();
    assert_eq!(canvas_of(&back, id), &before);
    let (img_id, img) = back.images.iter().next().unwrap();
    assert_eq!(*img_id, before.image);
    let orig = &snap.images[&before.image];
    assert_eq!(img.bytes, orig.bytes);
    assert_eq!(img.format, ImageFormat::Png);
    assert_eq!((img.width, img.height), (3, 2));
    assert_eq!(img.hash, orig.hash);
    // Laden ergibt ein rechenbares Dokument, das sich gleich wieder schreibt.
    let doc = Document::from_snapshot(back, 0.05).unwrap();
    assert_eq!(doc.feature(id).unwrap().status, FeatureStatus::Ok);
    assert_eq!(encode_version(&doc.snapshot()).unwrap(), text);
}

#[test]
fn last_base_from_the_file_is_kept() {
    let (d, id) = canvas_doc();
    let text = encode_version(&d.snapshot()).unwrap();
    let text = edited(&text, |v| {
        v["features"][0]["art"]["letzte_lage"]["ursprung"] = serde_json::json!([7.0, 8.0, 9.0]);
    });
    let back = decode_version(&text, "x.json").unwrap();
    let lb = canvas_of(&back, id).last_base.unwrap();
    assert_eq!(lb.origin.to_array(), [7.0, 8.0, 9.0]);
    // Ohne `letzte_lage` lädt die Leinwand auch (noch nie berechnet).
    let text = edited(&text, |v| {
        v["features"][0]["art"]
            .as_object_mut()
            .unwrap()
            .remove("letzte_lage");
    });
    let back = decode_version(&text, "x.json").unwrap();
    assert_eq!(canvas_of(&back, id).last_base, None);
}

#[test]
fn jpeg_and_tiff_roundtrip() {
    for (fmt, ours) in [
        (image::ImageFormat::Jpeg, ImageFormat::Jpeg),
        (image::ImageFormat::Tiff, ImageFormat::Tiff),
    ] {
        let mut d = Document::new();
        let bytes = encoded_image(5, 4, fmt);
        let img = d.add_image(canvas_image(bytes.clone(), ours, 5, 4));
        d.add_feature(
            "Leinwand1",
            FeatureKind::Canvas(CanvasFeature::new(
                img,
                CanvasBase::Standard(StandardPlane::XY),
            )),
        )
        .unwrap();
        let back = decode_version(&encode_version(&d.snapshot()).unwrap(), "x.json").unwrap();
        let got = &back.images[&img];
        assert_eq!(got.format, ours);
        assert_eq!(&*got.bytes, &bytes[..]);
        assert_eq!((got.width, got.height), (5, 4));
    }
}

#[test]
fn image_size_and_hash_come_from_the_bytes() {
    // Falsche Maße und Prüfsumme im Bild des Dokuments: Die Datei hat keine Felder dafür,
    // beim Laden werden sie aus den Bytes gerechnet.
    let mut d = Document::new();
    let bytes = png(3, 2);
    let img = d.add_image(CanvasImage {
        hash: 1,
        ..canvas_image(bytes.clone(), ImageFormat::Png, 99, 77)
    });
    d.add_feature(
        "Leinwand1",
        FeatureKind::Canvas(CanvasFeature::new(
            img,
            CanvasBase::Standard(StandardPlane::XY),
        )),
    )
    .unwrap();
    let text = encode_version(&d.snapshot()).unwrap();
    let back = decode_version(&text, "x.json").unwrap();
    let got = &back.images[&img];
    assert_eq!((got.width, got.height), (3, 2));
    assert_eq!(got.hash, image_hash(&bytes));
}

#[test]
fn format_19_only_with_canvas_or_frame() {
    // Spline mit Kontrollpunkten ohne Rahmen: Format 17, kein `rahmen`.
    let d = sketch_doc(inserted(vec![cubic()], false));
    let s = d.snapshot();
    assert_eq!(required_format(&s), 17);
    let text = encode_version(&s).unwrap();
    assert_eq!(format_of(&text), 17);
    assert!(!text.contains("\"rahmen\"") && !text.contains("\"bilder\""));
    // Mit Rahmen: 19.
    let d = sketch_doc(inserted(vec![cubic()], true));
    assert_eq!(required_format(&d.snapshot()), 19);
    // Mit Leinwand: 19, auch unterdrückt.
    let (mut d, id) = canvas_doc();
    assert_eq!(required_format(&d.snapshot()), 19);
    d.set_suppressed(id, true).unwrap();
    assert_eq!(required_format(&d.snapshot()), 19);
    // Ein leeres Dokument bleibt bei 1.
    assert_eq!(required_format(&Document::new().snapshot()), 1);
}

#[test]
fn show_frame_roundtrip() {
    let d = sketch_doc(inserted(vec![cubic(), rational_quarter_circle()], true));
    let text = encode_version(&d.snapshot()).unwrap();
    assert_eq!(format_of(&text), 19);
    assert_eq!(text.matches("\"rahmen\": true").count(), 2, "{text}");
    let back = decode_version(&text, "x.json").unwrap();
    let flags: Vec<_> = sketch_of(&back)
        .curves()
        .map(|(_, c)| (c.show_frame, c.kind.clone()))
        .collect();
    assert_eq!(flags.len(), 2);
    assert!(flags.iter().all(|(f, _)| *f), "{flags:?}");
    // Die Datei schreibt sich gleich wieder.
    let doc = Document::from_snapshot(back, 0.05).unwrap();
    assert_eq!(encode_version(&doc.snapshot()).unwrap(), text);
}

#[test]
fn frame_on_a_line_is_refused() {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze1", Tolerance::default());
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(1.0, 0.0).unwrap();
    sk.add_line(a, b).unwrap();
    let text = encode_version(&sketch_doc(sk).snapshot()).unwrap();
    let text = edited(&text, |v| {
        v["format"] = 19.into();
        v["features"][0]["art"]["skizze"]["kurven"][0]["rahmen"] = true.into();
    });
    let e = err_text(&text);
    assert!(e.contains(&tr!("sketch-err-data-show-frame")), "{e}");
}

#[test]
fn format_18_file_with_canvas_or_frame_is_refused() {
    let (d, _) = canvas_doc();
    let text = encode_version(&d.snapshot()).unwrap();
    let want = tr!(
        "proj-since-format",
        what = tr!("proj-feat-canvas"),
        format = "19"
    );
    let e = err_text(&text.replacen("\"format\": 19", "\"format\": 18", 1));
    assert!(e.contains(&want), "{e}");
    let d = sketch_doc(inserted(vec![cubic()], true));
    let text = encode_version(&d.snapshot()).unwrap();
    let want = tr!(
        "proj-since-format",
        what = tr!("proj-feat-control-frame"),
        format = "19"
    );
    let e = err_text(&text.replacen("\"format\": 19", "\"format\": 18", 1));
    assert!(e.contains(&want), "{e}");
}

#[test]
fn format_21_is_too_new() {
    let (d, _) = canvas_doc();
    let text =
        encode_version(&d.snapshot())
            .unwrap()
            .replacen("\"format\": 19", "\"format\": 22", 1);
    let e = decode_version(&text, "x.json").unwrap_err();
    assert!(matches!(e, ProjectError::TooNew { found: 22, .. }), "{e:?}");
}

#[test]
fn canvas_folder_hidden_roundtrip() {
    let (mut d, id) = canvas_doc();
    let mut vis = d.visibility().clone();
    vis.folders.canvases = true;
    vis.features.insert(id);
    d.set_visibility(vis.clone());
    assert!(d.visibility().folders.canvases);
    let text = encode_version(&d.snapshot()).unwrap();
    assert!(text.contains("\"leinwaende\""), "{text}");
    let back = decode_version(&text, "x.json").unwrap();
    assert_eq!(back.visibility, vis);
    let doc = Document::from_snapshot(back, 0.05).unwrap();
    assert_eq!(doc.visibility(), &vis);
    // Sichtbarer Ordner: kein Eintrag.
    let (d, _) = canvas_doc();
    assert!(
        !encode_version(&d.snapshot())
            .unwrap()
            .contains("leinwaende")
    );
}

#[test]
fn broken_images_are_load_errors() {
    let (d, _) = canvas_doc();
    let text = encode_version(&d.snapshot()).unwrap();
    let ctx = |msg: String| tr!("proj-ctx-image", id = "1", msg = msg);
    // Kein Bild (Base64 gültig, Inhalt nicht).
    let bad = edited(&text, |v| {
        v["bilder"][0]["daten"] = base64::encode(b"kein Bild hier").into();
    });
    let e = err_text(&bad);
    assert!(e.contains(&ctx(tr!("exchange-image-unsupported"))), "{e}");
    // Ungültiges Base64.
    let bad = edited(&text, |v| v["bilder"][0]["daten"] = "@@@".into());
    assert!(err_text(&bad).contains(&ctx(tr!("proj-base64-invalid"))));
    // Format in der Datei passt nicht zum Inhalt.
    let bad = edited(&text, |v| v["bilder"][0]["format"] = "jpeg".into());
    let e = err_text(&bad);
    assert!(
        e.contains(&ctx(tr!("proj-image-format-mismatch", format = "jpeg"))),
        "{e}"
    );
    // Unbekanntes Format.
    let bad = edited(&text, |v| v["bilder"][0]["format"] = "bmp".into());
    assert!(err_text(&bad).contains(&ctx(tr!("proj-unknown-image-format", value = "bmp"))));
    // Kennung 0 und doppelte Kennung.
    let bad = edited(&text, |v| v["bilder"][0]["id"] = 0.into());
    assert!(err_text(&bad).contains(&tr!("proj-bad-id")));
    let bad = edited(&text, |v| {
        let b = v["bilder"][0].clone();
        v["bilder"].as_array_mut().unwrap().push(b);
    });
    assert!(err_text(&bad).contains(&tr!("proj-duplicate-id")));
    // Leinwand ohne ihr Bild.
    let bad = edited(&text, |v| v["bilder"] = serde_json::json!([]));
    assert!(err_text(&bad).contains(&tr!("proj-image-missing", id = "1")));
}

#[test]
fn image_too_big_in_file_is_error() {
    let (d, _) = canvas_doc();
    let text = encode_version(&d.snapshot()).unwrap();
    // Eine Zeichenkette, die mehr als MAX_IMAGE_BYTES ergäbe; wird vor dem Dekodieren abgelehnt.
    let n = (MAX_IMAGE_BYTES as usize / 3 + 1) * 4;
    let bad = edited(&text, |v| v["bilder"][0]["daten"] = "A".repeat(n).into());
    assert!(err_text(&bad).contains(&tr!("proj-image-too-big")));
}

#[test]
fn image_total_limit_is_error() {
    // Gesamtgrenze 1 GiB (Gesamtprüfung I5), nicht mehr 8 GiB wie bei Netzen.
    assert_eq!(planora_exchange::MAX_IMAGES_TOTAL_BYTES, 1024 * 1024 * 1024);
    // Mehr Bilder, als die Gesamtgrenze erlaubt; alle teilen sich dieselben Bytes (je so groß
    // wie ein Bild höchstens sein darf), darum braucht der Test nur einmal den Speicher.
    let (d, _) = canvas_doc();
    let mut s = d.snapshot();
    let big: Arc<[u8]> = vec![0u8; MAX_IMAGE_BYTES as usize].into();
    let n = (planora_exchange::MAX_IMAGES_TOTAL_BYTES / MAX_IMAGE_BYTES) + 1;
    for i in 0..n {
        s.images.insert(
            ImageId(100 + i),
            Arc::new(CanvasImage {
                bytes: big.clone(),
                format: ImageFormat::Png,
                width: 1,
                height: 1,
                hash: 0,
            }),
        );
    }
    let e = encode_version(&s).unwrap_err();
    // Das kleine Bild der Leinwand zählt mit, ändert die ganzen MB aber nicht.
    let mib = 1024 * 1024;
    let want = tr!(
        "proj-images-too-large-to-save",
        size = (n * MAX_IMAGE_BYTES / mib).to_string(),
        max = (planora_exchange::MAX_IMAGES_TOTAL_BYTES / mib).to_string()
    );
    assert!(matches!(e, ProjectError::Invalid(_)), "{e:?}");
    assert_eq!(e.to_string(), want);
}

#[test]
fn fixed_spline_outside_projection_loads() {
    let sk = inserted(vec![rational_quarter_circle()], false);
    let d = sketch_doc(sk);
    let text = encode_version(&d.snapshot()).unwrap();
    // Fester Spline ohne Projektion: Format 17 (seit Skizzen 7), kein Rahmen.
    assert_eq!(format_of(&text), 17);
    assert!(text.contains("fester_spline"), "{text}");
    let back = decode_version(&text, "x.json").unwrap();
    let FeatureKind::Sketch(sf) = &back.features[0].kind else {
        panic!("keine Skizze");
    };
    assert!(sf.copies.is_empty());
    assert!(
        sf.sketch
            .curves()
            .any(|(_, c)| matches!(c.kind, CurveKind::FixedSpline { .. }))
    );
    let doc = Document::from_snapshot(back, 0.05).unwrap();
    assert_eq!(doc.feature(FeatureId(1)).unwrap().status, FeatureStatus::Ok);
    assert_eq!(encode_version(&doc.snapshot()).unwrap(), text);
}
