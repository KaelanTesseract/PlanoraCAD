//! Schreiber für ebene Körper: Kopf, Kontext, Teile, Topologie (Teilprojekt 10, Task 3).

use planora_exchange::Progress;
use planora_kernel::{AnalyticKernel, Kernel, Solid, validate};
use planora_math::DVec3;
use planora_step::*;
use std::time::{Duration, UNIX_EPOCH};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn cube(size: f64) -> Solid {
    k().make_box(DVec3::splat(size)).unwrap()
}

/// Feste Einstellungen (gleiche Datei bei gleichem Inhalt).
fn opts() -> StepOptions {
    StepOptions {
        time: UNIX_EPOCH + Duration::from_secs(1_791_981_296),
        ..StepOptions::new("probe.step")
    }
}

fn export(bodies: &[StepBody]) -> (String, StepReport) {
    let mut out = Vec::new();
    let report = write_step(bodies, &opts(), &mut out, &Progress::new()).unwrap();
    (String::from_utf8(out).unwrap(), report)
}

fn body<'a>(name: &str, solid: &'a Solid) -> StepBody<'a> {
    StepBody {
        name: name.to_string(),
        solid,
    }
}

/// Einträge `#n=…;` (Zeilenumbrüche entfernt) in Reihenfolge.
fn entities(text: &str) -> Vec<(u64, String)> {
    let data = text.split("DATA;").nth(1).unwrap();
    let data = data.split("ENDSEC;").next().unwrap().replace('\n', "");
    data.split(';')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let (id, body) = s.split_once('=').unwrap();
            (
                id.trim_start_matches('#').parse().unwrap(),
                body.to_string(),
            )
        })
        .collect()
}

fn count(text: &str, name: &str) -> usize {
    entities(text)
        .iter()
        .filter(|(_, b)| b.starts_with(&format!("{name}(")))
        .count()
}

#[test]
fn a_cube_becomes_one_part_with_shared_edges_and_vertices() {
    let s = cube(10.0);
    let (text, report) = export(&[body("Quader 1", &s)]);
    assert_eq!(
        (report.bodies, report.faces, report.edges),
        (1, 6, 12),
        "{report:?}"
    );
    assert!(report.rejected.is_empty() && report.shortened_names.is_empty());
    for (name, n) in [
        ("ADVANCED_FACE", 6),
        ("PLANE", 6),
        ("FACE_OUTER_BOUND", 6),
        ("FACE_BOUND", 0),
        ("EDGE_LOOP", 6),
        ("ORIENTED_EDGE", 24),
        ("EDGE_CURVE", 12),
        ("LINE", 12),
        ("VERTEX_POINT", 8),
        ("CLOSED_SHELL", 1),
        ("MANIFOLD_SOLID_BREP", 1),
        ("ADVANCED_BREP_SHAPE_REPRESENTATION", 1),
        ("PRODUCT", 1),
        ("PRODUCT_DEFINITION", 1),
        ("SHAPE_DEFINITION_REPRESENTATION", 1),
        ("APPLICATION_CONTEXT", 1),
    ] {
        assert_eq!(count(&text, name), n, "{name}");
    }
    assert!(text.contains("MANIFOLD_SOLID_BREP('Quader 1',"));
    assert!(text.contains("PRODUCT('Quader 1','Quader 1','',("));
    assert_eq!(report.entities, entities(&text).len() as u64);
}

#[test]
fn the_header_names_schema_time_and_units() {
    let s = cube(1.0);
    let (text, _) = export(&[body("Würfel", &s)]);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "ISO-10303-21;");
    assert_eq!(lines[1], "HEADER;");
    assert_eq!(lines[2], "FILE_DESCRIPTION(('PlanoraCAD Export'),'2;1');");
    assert_eq!(
        lines[3],
        "FILE_NAME('probe.step','2026-10-14T12:34:56',(''),(''),'PlanoraCAD','PlanoraCAD','');"
    );
    assert_eq!(
        lines[4],
        "FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));"
    );
    assert_eq!(lines[5], "ENDSEC;");
    assert_eq!(lines[6], "DATA;");
    assert_eq!(lines[lines.len() - 2], "ENDSEC;");
    assert_eq!(lines[lines.len() - 1], "END-ISO-10303-21;");
    assert!(text.ends_with("END-ISO-10303-21;\n"));
    assert!(text.contains("SI_UNIT(.MILLI.,.METRE.)"));
    assert!(text.contains("SI_UNIT($,.RADIAN.)"));
    assert!(text.contains("SI_UNIT($,.STERADIAN.)"));
    assert!(
        text.contains("LENGTH_MEASURE(1.E-6)"),
        "Unsicherheit = Kernel-Toleranz"
    );
    assert!(text.contains("'W\\X2\\00FC\\X0\\rfel'"));
}

#[test]
fn numbers_run_from_one_and_references_point_backwards() {
    let a = cube(2.0);
    let b = k()
        .transform(
            &cube(3.0),
            &planora_math::DMat4::from_translation(DVec3::X * 10.0),
        )
        .unwrap();
    let (text, report) = export(&[body("A", &a), body("B", &b)]);
    assert_eq!(report.bodies, 2);
    let list = entities(&text);
    for (i, (id, body)) in list.iter().enumerate() {
        assert_eq!(*id, i as u64 + 1);
        for part in body.split('#').skip(1) {
            let n: u64 = part
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse()
                .unwrap();
            assert!(n < *id, "#{id} verweist auf #{n}");
        }
    }
    assert_eq!(count(&text, "PRODUCT"), 2);
    assert_eq!(count(&text, "APPLICATION_CONTEXT"), 1, "Kontext gemeinsam");
}

#[test]
fn the_same_bodies_give_the_same_bytes() {
    let s = cube(5.0);
    assert_eq!(export(&[body("Q", &s)]).0, export(&[body("Q", &s)]).0);
}

#[test]
fn invalid_bodies_are_left_out_by_name_and_the_rest_is_written() {
    let good = cube(1.0);
    let mut open = cube(1.0);
    let first = open.faces.iter().next().unwrap().0;
    open.faces[first].outer.half_edges.pop();
    let mut nan = cube(1.0);
    let v = nan.vertices.iter().next().unwrap().0;
    nan.vertices[v].point.x = f64::NAN;
    validate(&nan).unwrap();
    let (text, report) = export(&[body("Offen", &open), body("Gut", &good), body("NaN", &nan)]);
    assert_eq!(report.bodies, 1);
    assert_eq!(report.rejected.len(), 2, "{report:?}");
    match &report.rejected[0] {
        StepError::InvalidBody { name, reason } => {
            assert_eq!(name, "Offen");
            assert!(!reason.is_empty());
        }
        other => panic!("{other:?}"),
    }
    match &report.rejected[1] {
        StepError::InvalidBody { name, reason } => {
            assert_eq!(name, "NaN");
            assert!(reason.contains("keine gültige Zahl"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
    assert!(!text.contains("NaN") && !text.contains("inf"));
    assert_eq!(count(&text, "PRODUCT"), 1);
    let mut out = Vec::new();
    assert_eq!(
        write_step(&[body("Offen", &open)], &opts(), &mut out, &Progress::new()),
        Err(StepError::NothingToExport)
    );
    assert!(out.is_empty(), "nichts geschrieben");
}

#[test]
fn dangling_references_are_refused_without_a_crash() {
    let mut s = cube(1.0);
    let first = s.faces.iter().next().unwrap().0;
    s.faces[first].outer.half_edges[0].edge = planora_kernel::Id::from_index(999);
    let err = check_body(&body("Kaputt", &s), &opts(), &Progress::new()).unwrap_err();
    assert!(
        matches!(&err, StepError::InvalidBody { name, .. } if name == "Kaputt"),
        "{err:?}"
    );
}

#[test]
fn check_body_counts_faces_edges_and_entities() {
    let s = cube(1.0);
    let stats = check_body(&body("Q", &s), &opts(), &Progress::new()).unwrap();
    assert_eq!((stats.faces, stats.edges), (6, 12));
    // 8 Punkte + 8 Eckpunkte, 12 × (Punkt, Richtung, Vektor, Gerade, Kante), 24 orientierte
    // Kanten, 6 × (Schleife, Rand, 4 Einträge Ebene, Fläche), Schale, Körper.
    assert_eq!(stats.entities, 16 + 60 + 24 + 48 + 2);
}

#[test]
fn empty_names_become_numbered_bodies() {
    assert_eq!(display_name("  ", 2), "Körper 3");
    assert_eq!(display_name("Quader 1", 0), "Quader 1");
    let s = cube(1.0);
    let (text, _) = export(&[body("", &s)]);
    assert!(text.contains("PRODUCT('K\\X2\\00F6\\X0\\rper 1'"), "{text}");
}

#[test]
fn timestamps_are_utc() {
    assert_eq!(utc_timestamp(UNIX_EPOCH), "1970-01-01T00:00:00");
    assert_eq!(
        utc_timestamp(UNIX_EPOCH + Duration::from_secs(1_709_251_199)),
        "2024-02-29T23:59:59"
    );
    assert_eq!(
        utc_timestamp(UNIX_EPOCH + Duration::from_secs(1_791_981_296)),
        "2026-10-14T12:34:56"
    );
    assert_eq!(
        utc_timestamp(UNIX_EPOCH - Duration::from_secs(10)),
        "1970-01-01T00:00:00"
    );
}

#[test]
fn bodies_without_faces_are_refused_and_nothing_is_written() {
    let empty = Solid::new();
    let mut hollow = Solid::new();
    hollow.shells = vec![planora_kernel::Shell { faces: vec![] }];
    for (name, solid) in [("Leer", &empty), ("Hülle ohne Flächen", &hollow)] {
        match check_body(&body(name, solid), &opts(), &Progress::new()) {
            Err(StepError::InvalidBody { name: n, reason }) => {
                assert_eq!(n, name);
                assert!(reason.contains("keine Flächen"), "{reason}");
            }
            other => panic!("{name}: {other:?}"),
        }
        let mut out = Vec::new();
        assert_eq!(
            write_step(&[body(name, solid)], &opts(), &mut out, &Progress::new()),
            Err(StepError::NothingToExport)
        );
        assert!(out.is_empty(), "nichts geschrieben");
    }
}

/// Schreibstrom, der den Fortschritt vor jedem Eintrag festhält.
struct Spy<'a> {
    progress: &'a Progress,
    seen: Vec<f32>,
}

impl std::io::Write for Spy<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.first() == Some(&b'#') {
            self.seen.push(self.progress.fraction());
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn progress_reaches_exactly_one_only_at_the_end() {
    let a = cube(1.0);
    let b = cube(2.0);
    let p = Progress::new();
    let mut spy = Spy {
        progress: &p,
        seen: Vec::new(),
    };
    let report = write_step(&[body("A", &a), body("B", &b)], &opts(), &mut spy, &p).unwrap();
    assert_eq!(spy.seen.len() as u64, report.entities);
    assert!(spy.seen.iter().all(|f| *f < 1.0), "vor dem letzten Eintrag");
    assert_eq!(p.fraction(), 1.0);
}

/// Schreibstrom, der nach `limit` Bytes den Abbruch auslöst.
struct CancelAfter<'a> {
    progress: &'a Progress,
    written: usize,
    limit: usize,
}

impl std::io::Write for CancelAfter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.written += buf.len();
        if self.written >= self.limit {
            self.progress.cancel();
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn cancelling_in_the_middle_of_writing_is_a_named_error() {
    let s = cube(1.0);
    // Mehr als zwei Prüfintervalle (4096 Einträge) an Einträgen.
    let bodies: Vec<StepBody> = (0..60).map(|_| body("Q", &s)).collect();
    let p = Progress::new();
    let mut out = CancelAfter {
        progress: &p,
        written: 0,
        limit: 50_000,
    };
    assert_eq!(
        write_step(&bodies, &opts(), &mut out, &p),
        Err(StepError::Cancelled)
    );
    assert!(out.written >= 50_000, "erst nach dem Start abgebrochen");
}

#[test]
fn cancelling_before_writing_is_a_named_error() {
    let s = cube(1.0);
    let bodies: Vec<StepBody> = (0..40).map(|_| body("Q", &s)).collect();
    let p = Progress::new();
    p.cancel();
    let mut out = Vec::new();
    assert_eq!(
        write_step(&bodies, &opts(), &mut out, &p),
        Err(StepError::Cancelled)
    );
}
