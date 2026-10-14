//! Stufentest 1 (Teilprojekt 10, Task 6): Quader und Prismen werden geschrieben, mit dem eigenen
//! Leser gelesen und geprüft (Topologie, Euler-Zahl, Geometrie, Vergleich mit dem Kernel,
//! Volumen und Fläche aus den STEP-Flächen); Namen, viele Körper, feindliche Eingaben, Datei.

mod common;

use common::check::{Measure, measure};
use common::model::Model;
use common::{checked, export, options};
use planora_exchange::{Progress, temp_path};
use planora_kernel::{AnalyticKernel, Kernel, Profile, ProfileLoop, Solid};
use planora_math::{DMat4, DVec2, DVec3, Frame};
use planora_step::{StepBody, StepError, export_step_file, write_step};
use std::fs;
use std::path::PathBuf;

/// Sehnentoleranz der Netze (ebene Körper werden exakt vernetzt).
const CHORD: f64 = 0.01;
/// Schranke für Volumen und Fläche ebener Körper (nur Rundung).
const FLAT_REL: f64 = 1e-12;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, m: DMat4) -> Solid {
    k().transform(s, &m).unwrap()
}

fn prism(points: &[DVec2], holes: &[&[DVec2]], h: f64) -> Solid {
    let profile = Profile {
        outer: ProfileLoop::polygon(points).unwrap(),
        holes: holes
            .iter()
            .map(|p| ProfileLoop::polygon(p).unwrap())
            .collect(),
    };
    k().extrude(&profile, &Frame::WORLD, h).unwrap()
}

fn l_shape() -> Solid {
    let p = [
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, 4.0),
        (4.0, 4.0),
        (4.0, 10.0),
        (0.0, 10.0),
    ]
    .map(|(x, y)| DVec2::new(x, y));
    prism(&p, &[], 5.0)
}

fn frame_with_hole() -> Solid {
    let outer = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)].map(|(x, y)| DVec2::new(x, y));
    let hole = [(3.0, 2.0), (7.0, 2.0), (7.0, 6.0), (3.0, 6.0)].map(|(x, y)| DVec2::new(x, y));
    prism(&outer, &[&hole], 2.0)
}

fn rel(a: f64, b: f64) -> f64 {
    ((a - b) / b).abs()
}

fn measured(text: &str, solids: &[&Solid]) -> Measure {
    let m = Model::read(text).unwrap();
    let r = measure(&m, solids, CHORD).unwrap();
    assert_eq!(r.flipped, 0, "kein Dreieck falsch herum");
    r
}

#[test]
fn boxes_and_prisms_pass_all_checks_with_the_right_volume() {
    let cube = k().make_box(DVec3::new(10.0, 20.0, 30.0)).unwrap();
    for (name, s, volume, area, genus) in [
        ("Quader", cube, 6000.0, 2200.0, 0),
        ("L-Form", l_shape(), 320.0, 2.0 * 64.0 + 40.0 * 5.0, 0),
        (
            "Rahmen",
            frame_with_hole(),
            128.0,
            2.0 * 64.0 + (36.0 + 16.0) * 2.0,
            1,
        ),
    ] {
        let (text, report) = export(&[(name, &s)]);
        let (m, stats) = checked(&text, &[&s]);
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].genus, genus, "{name}");
        assert_eq!(report.faces, stats[0].faces);
        assert_eq!(m.products[0].name, name);
        let r = measured(&text, &[&s]);
        assert!(rel(r.volume, volume) < FLAT_REL, "{name}: {}", r.volume);
        assert!(rel(r.area, area) < FLAT_REL, "{name}: {}", r.area);
        assert!(rel(r.kernel_volume, volume) < FLAT_REL);
        assert!(r.max_offset < 1e-9, "{name}: {}", r.max_offset);
    }
}

#[test]
fn moved_rotated_and_mirrored_boxes_keep_their_outward_normals() {
    let base = l_shape();
    for (what, m) in [
        (
            "verschoben",
            DMat4::from_translation(DVec3::new(-50.0, 12.5, 3.0)),
        ),
        (
            "gedreht",
            DMat4::from_rotation_y(0.7) * DMat4::from_rotation_z(-1.1),
        ),
        ("gespiegelt", DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0))),
        (
            "gespiegelt und gedreht",
            DMat4::from_rotation_x(2.0) * DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0)),
        ),
        ("vergrößert", DMat4::from_scale(DVec3::splat(3.5))),
    ] {
        let s = moved(&base, m);
        let (text, _) = export(&[(what, &s)]);
        checked(&text, &[&s]);
        let r = measured(&text, &[&s]);
        let want = 320.0 * m.determinant().abs();
        assert!(
            rel(r.volume, want) < 1e-9,
            "{what}: {} statt {want}",
            r.volume
        );
    }
}

#[test]
fn several_bodies_become_several_named_parts() {
    let a = k().make_box(DVec3::splat(2.0)).unwrap();
    let b = moved(&l_shape(), DMat4::from_translation(DVec3::X * 20.0));
    let c = moved(&frame_with_hole(), DMat4::from_translation(DVec3::Y * 20.0));
    let names = ["Würfel „klein“", "O'Brien \\ L", "Rahmen 😀"];
    let (text, report) = export(&[(names[0], &a), (names[1], &b), (names[2], &c)]);
    assert_eq!(report.bodies, 3);
    let (m, stats) = checked(&text, &[&a, &b, &c]);
    let got: Vec<&str> = m.products.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(got, names);
    assert_eq!(stats.len(), 3);
    assert_eq!(m.file.count("PRODUCT"), 3);
    assert_eq!(m.file.count("APPLICATION_CONTEXT"), 1);
    let r = measured(&text, &[&a, &b, &c]);
    assert!(
        rel(r.volume, 8.0 + 320.0 + 128.0) < FLAT_REL,
        "{}",
        r.volume
    );
}

#[test]
fn the_header_is_read_back_by_the_own_reader() {
    let s = k().make_box(DVec3::splat(1.0)).unwrap();
    let (text, _) = export(&[("Q", &s)]);
    let m = Model::read(&text).unwrap();
    let name = &m.file.header[1];
    assert_eq!(name.args[0].text().unwrap(), "probe.step");
    assert_eq!(name.args[1].text().unwrap(), "2026-10-14T12:34:56");
    assert_eq!(name.args[4].text().unwrap(), "PlanoraCAD");
    assert_eq!(m.uncertainty, 1e-6);
}

#[test]
fn long_names_are_shortened_and_reported() {
    let s = k().make_box(DVec3::splat(1.0)).unwrap();
    let long = "Sehr langer Name ".repeat(20);
    let (text, report) = export(&[(&long, &s)]);
    assert_eq!(report.shortened_names, vec![long.clone()]);
    let m = Model::read(&text).unwrap();
    let got = &m.products[0].name;
    assert_eq!(got.chars().count(), 256);
    assert!(long.starts_with(got.as_str()));
}

#[test]
fn many_bodies_fit_into_one_file() {
    let s = k().make_box(DVec3::splat(1.0)).unwrap();
    let names: Vec<String> = (1..=200).map(|i| format!("Quader {i}")).collect();
    let bodies: Vec<(&str, &Solid)> = names.iter().map(|n| (n.as_str(), &s)).collect();
    let (text, report) = export(&bodies);
    assert_eq!(report.bodies, 200);
    let m = Model::read(&text).unwrap();
    assert_eq!(m.products.len(), 200);
    assert_eq!(m.products[199].name, "Quader 200");
}

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-step-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn files_go_through_a_helper_and_failures_keep_the_old_file() {
    let s = k().make_box(DVec3::splat(3.0)).unwrap();
    let dir = temp_dir("datei");
    let path = dir.join("Quader.step");
    let bodies = [StepBody {
        name: "Quader".into(),
        solid: &s,
    }];
    let (report, bytes) = export_step_file(&path, &bodies, &options(), &Progress::new()).unwrap();
    assert_eq!(report.bodies, 1);
    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(bytes, text.len() as u64);
    checked(&text, &[&s]);
    assert!(!temp_path(&path).exists());
    // Abbrechen und „nichts auszugeben“ lassen die alte Datei stehen.
    let cancelled = Progress::new();
    cancelled.cancel();
    assert_eq!(
        export_step_file(&path, &bodies, &options(), &cancelled).unwrap_err(),
        StepError::Cancelled
    );
    let mut open = s.clone();
    let f = open.faces.iter().next().unwrap().0;
    open.faces[f].outer.half_edges.pop();
    let bad = [StepBody {
        name: "Offen".into(),
        solid: &open,
    }];
    assert_eq!(
        export_step_file(&path, &bad, &options(), &Progress::new()).unwrap_err(),
        StepError::NothingToExport
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), text);
    assert!(!temp_path(&path).exists());
    let missing = dir.join("fehlt").join("x.step");
    assert!(matches!(
        export_step_file(&missing, &bodies, &options(), &Progress::new()),
        Err(StepError::Io { .. })
    ));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn bodies_rejected_before_writing_never_reach_the_file() {
    let good = k().make_box(DVec3::splat(1.0)).unwrap();
    let mut nan = good.clone();
    let v = nan.vertices.iter().next().unwrap().0;
    nan.vertices[v].point.y = f64::INFINITY;
    let mut out = Vec::new();
    let bodies = [
        StepBody {
            name: "Unendlich".into(),
            solid: &nan,
        },
        StepBody {
            name: "Gut".into(),
            solid: &good,
        },
    ];
    let report = write_step(&bodies, &options(), &mut out, &Progress::new()).unwrap();
    assert_eq!(report.bodies, 1);
    assert_eq!(report.rejected.len(), 1);
    let text = String::from_utf8(out).unwrap();
    checked(&text, &[&good]);
}
