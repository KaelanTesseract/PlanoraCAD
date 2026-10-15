//! Stufe 17: Schreiben einer Zeichnung in eine Datei (SVG, DXF) über eine Hilfsdatei im
//! Zielordner. Eine vorhandene Zieldatei bleibt bei jedem Fehler und beim Abbrechen unverändert,
//! die Hilfsdatei verschwindet.

use planora_drawing::file::{DrawingFormat, drawing_to_bytes, export_drawing_file};
use planora_drawing::model::{Paper, Sheet};
use planora_drawing::out2d::Out2d;
use planora_drawing::sheet::{SheetLine, SheetModel, SheetView};
use planora_drawing::{DrawError, ViewId, Visibility, dxf_string, svg_string};
use planora_exchange::Progress;
use planora_math::DVec2;
use std::fs;
use std::path::{Path, PathBuf};

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "planora-drawing-file-{}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// Zahl der Hilfsdateien („….planora-tmp“) im Ordner.
fn temp_files(dir: &Path) -> usize {
    fs::read_dir(dir)
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with("planora-tmp")
        })
        .count()
}

/// Blatt A3 quer mit einer Ansicht „Vorne“: ein Rechteck 100 × 40 (sichtbar) und eine
/// verdeckte Linie.
fn model_with(a: f64) -> SheetModel {
    let l = |x0: f64, y0: f64, x1: f64, y1: f64| Out2d::Line {
        a: DVec2::new(x0, y0),
        b: DVec2::new(x1, y1),
    };
    let lines = vec![
        (Visibility::Visible, l(a, 100.0, 200.0, 100.0)),
        (Visibility::Visible, l(200.0, 100.0, 200.0, 140.0)),
        (Visibility::Visible, l(200.0, 140.0, 100.0, 140.0)),
        (Visibility::Visible, l(100.0, 140.0, 100.0, 100.0)),
        (Visibility::Hidden, l(150.0, 100.0, 150.0, 140.0)),
    ];
    SheetModel {
        sheet: Sheet {
            paper: Paper::A3,
            landscape: true,
        },
        frame: (DVec2::new(10.0, 10.0), DVec2::new(410.0, 287.0)),
        views: vec![SheetView {
            id: ViewId(1),
            name: "Vorne".into(),
            lines: lines
                .into_iter()
                .map(|(vis, curve)| SheetLine { vis, curve })
                .collect(),
            extent_mm: (100.0, 40.0),
        }],
    }
}

fn model() -> SheetModel {
    model_with(100.0)
}

#[test]
fn formats_have_their_extension_and_german_label() {
    assert_eq!(DrawingFormat::Svg.extension(), "svg");
    assert_eq!(DrawingFormat::Dxf.extension(), "dxf");
    assert_eq!(DrawingFormat::Svg.label(), "Zeichnung als SVG");
    assert_eq!(DrawingFormat::Dxf.label(), "Zeichnung als DXF");
}

#[test]
fn written_files_hold_exactly_the_writer_output() {
    let dir = temp_dir("inhalt");
    let m = model();
    for (format, name, want) in [
        (DrawingFormat::Svg, "z.svg", svg_string(&m).unwrap()),
        (DrawingFormat::Dxf, "z.dxf", dxf_string(&m).unwrap()),
    ] {
        let path = dir.join(name);
        let n = export_drawing_file(&path, &m, format, &Progress::new()).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(n, text.len() as u64);
        assert_eq!(n, fs::metadata(&path).unwrap().len());
        assert_eq!(text, want);
        assert_eq!(drawing_to_bytes(&m, format).unwrap(), want.into_bytes());
    }
    let svg = fs::read_to_string(dir.join("z.svg")).unwrap();
    assert!(svg.starts_with("<?xml") && svg.contains("<svg"));
    let dxf = fs::read_to_string(dir.join("z.dxf")).unwrap();
    assert!(dxf.contains("SECTION") && dxf.trim_end().ends_with("EOF"));
    assert_eq!(temp_files(&dir), 0);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn atomic_write_keeps_old_file_on_cancel() {
    let dir = temp_dir("abbrechen");
    let m = model();
    for (format, name) in [
        (DrawingFormat::Svg, "alt.svg"),
        (DrawingFormat::Dxf, "alt.dxf"),
    ] {
        let path = dir.join(name);
        fs::write(&path, "alt").unwrap();
        let p = Progress::new();
        p.cancel();
        let e = export_drawing_file(&path, &m, format, &p).unwrap_err();
        assert!(matches!(e, DrawError::Cancelled), "{e:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), "alt");
        assert_eq!(temp_files(&dir), 0);
        // Danach geht es: die alte Datei wird ersetzt.
        let n = export_drawing_file(&path, &m, format, &Progress::new()).unwrap();
        assert_eq!(n, fs::metadata(&path).unwrap().len());
        assert_ne!(fs::read_to_string(&path).unwrap(), "alt");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn temp_file_is_removed_on_error() {
    let dir = temp_dir("fehler");
    let m = model();
    // Ziel ist ein Ordner: die Hilfsdatei wird ganz geschrieben, das Umbenennen scheitert.
    let target = dir.join("ordner.svg");
    fs::create_dir_all(&target).unwrap();
    for format in [DrawingFormat::Svg, DrawingFormat::Dxf] {
        let e = export_drawing_file(&target, &m, format, &Progress::new()).unwrap_err();
        assert!(matches!(e, DrawError::Io(_)), "{e:?}");
        assert!(
            e.to_string()
                .starts_with("Die Datei lässt sich nicht schreiben: "),
            "{e}"
        );
        assert!(target.is_dir());
        assert_eq!(temp_files(&dir), 0, "Hilfsdatei muss weg sein");
    }
    // Ungültige Zahl im Modell: Fehler vor dem Schreiben, alte Datei bleibt.
    let old = dir.join("alt.dxf");
    fs::write(&old, "alt").unwrap();
    for format in [DrawingFormat::Svg, DrawingFormat::Dxf] {
        let e =
            export_drawing_file(&old, &model_with(f64::NAN), format, &Progress::new()).unwrap_err();
        assert!(matches!(e, DrawError::NonFinite), "{e:?}");
        assert_eq!(fs::read_to_string(&old).unwrap(), "alt");
        assert_eq!(temp_files(&dir), 0);
    }
    // Ordner fehlt: verständlicher Fehler, nichts angelegt.
    let missing = dir.join("fehlt").join("z.svg");
    let e = export_drawing_file(&missing, &m, DrawingFormat::Svg, &Progress::new()).unwrap_err();
    assert!(matches!(e, DrawError::Io(_)), "{e:?}");
    assert!(!dir.join("fehlt").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_read_only_target_stays_and_leaves_no_helper_file() {
    let dir = temp_dir("schreibgeschuetzt");
    let path = dir.join("fest.svg");
    fs::write(&path, "alt").unwrap();
    let mut perm = fs::metadata(&path).unwrap().permissions();
    perm.set_readonly(true);
    fs::set_permissions(&path, perm.clone()).unwrap();
    let r = export_drawing_file(&path, &model(), DrawingFormat::Svg, &Progress::new());
    let text = fs::read_to_string(&path).unwrap();
    #[allow(clippy::permissions_set_readonly_false)]
    perm.set_readonly(false);
    fs::set_permissions(&path, perm).unwrap();
    if cfg!(windows) {
        // Windows ersetzt keine schreibgeschützte Datei.
        let e = r.unwrap_err();
        assert!(matches!(e, DrawError::Io(_)), "{e:?}");
        assert_eq!(text, "alt");
    } else {
        // Unter Unix entscheidet der Ordner; dann ist die Datei vollständig ersetzt.
        assert!(r.is_err() && text == "alt" || r.is_ok() && text.starts_with("<?xml"));
    }
    assert_eq!(temp_files(&dir), 0);
    let _ = fs::remove_dir_all(&dir);
}
