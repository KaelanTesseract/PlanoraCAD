//! STEP-Leser, Stufe 2: Einheiten, Kontext und analytische Geometrie (STEP-Import, Aufgabe 2).
//! Alle Erwartungswerte sind von Hand aus den Formeln von ISO 10303-42 gerechnet.

use planora_exchange::Progress;
use planora_kernel::{Curve, Surface};
use planora_math::{DVec3, Frame};
use planora_step::read::geom::{MAX_RESOLVE_DEPTH, Resolver, Skip, SurfaceGeom};
use planora_step::read::units::{
    COARSE_TOL_MM, DEFAULT_FILE_TOL_MM, LengthUnit, MAX_WELD_TOL_MM, MIN_WELD_TOL_MM, Units,
    context_units,
};
use planora_step::{ReadLimits, StepFile, StepReadError};
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};

/// Kopf (7 Zeilen) + `DATA;` (Zeile 7) + `data` (ab Zeile 8) + Ende.
fn file(data: &str) -> String {
    let head = "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('x'),'2;1');\n\
                FILE_NAME('a.step','2026-10-21T00:00:00',(''),(''),'P','P','');\n\
                FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));\nENDSEC;\nDATA;\n";
    format!("{head}{data}ENDSEC;\nEND-ISO-10303-21;\n")
}

fn parse(data: &str) -> StepFile {
    StepFile::parse(
        file(data).into_bytes(),
        &ReadLimits::DEFAULT,
        &Progress::new(),
    )
    .expect("Datei lädt")
}

/// Zeile von `#id` in `data` (Daten ab Zeile 8, ein Eintrag je Zeile).
fn line_of(data: &str, id: u64) -> usize {
    let prefix = format!("#{id}=");
    8 + data
        .lines()
        .position(|l| l.starts_with(&prefix))
        .expect("Eintrag vorhanden")
}

/// Kontext #909 in Millimeter und Radiant mit der Genauigkeit `uncertainty` (mm).
fn mm_context(uncertainty: &str) -> String {
    format!(
        "#900=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));\n\
         #901=(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.));\n\
         #902=(NAMED_UNIT(*)SI_UNIT($,.STERADIAN.)SOLID_ANGLE_UNIT());\n\
         #903=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE({uncertainty}),#900,'distance_accuracy_value','');\n\
         #909=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#903))\
         GLOBAL_UNIT_ASSIGNED_CONTEXT((#900,#901,#902))REPRESENTATION_CONTEXT('',''));\n"
    )
}

fn close(a: DVec3, b: DVec3, eps: f64) -> bool {
    (a - b).length() <= eps
}

fn assert_close(a: DVec3, b: DVec3, eps: f64) {
    assert!(close(a, b, eps), "{a:?} != {b:?} (±{eps})");
}

fn ready(g: SurfaceGeom) -> Surface {
    match g {
        SurfaceGeom::Ready { surface, flipped } => {
            assert!(!flipped, "analytische Flächen sind nie gekippt");
            surface
        }
        SurfaceGeom::Extrusion { .. } => panic!("Strangfläche statt fertiger Fläche"),
    }
}

fn skip_text<T: std::fmt::Debug>(r: Result<T, Skip>) -> String {
    match r {
        Ok(v) => panic!("sollte ausgelassen werden: {v:?}"),
        Err(Skip(s)) => s,
    }
}

fn orthonormal(f: &Frame) {
    assert!(f.is_orthonormal(1e-12), "{f:?}");
    assert_close(f.y, f.z.cross(f.x), 1e-15);
}

#[test]
fn placements_are_orthonormal() {
    let data = "#1=CARTESIAN_POINT('',(1.,2.,3.));\n\
                #2=DIRECTION('',(0.,0.,1.));\n\
                #3=DIRECTION('',(1.,0.,0.1));\n\
                #4=AXIS2_PLACEMENT_3D('',#1,#2,#3);\n\
                #5=AXIS2_PLACEMENT_3D('',#1,$,$);\n\
                #6=DIRECTION('',(0.,0.,0.));\n\
                #7=AXIS2_PLACEMENT_3D('',#1,#6,$);\n\
                #8=DIRECTION('',(0.,0.,2.));\n\
                #9=DIRECTION('',(0.,0.,-3.));\n\
                #10=AXIS2_PLACEMENT_3D('',#1,#8,#9);\n\
                #11=DIRECTION('',(0.,3.,4.));\n\
                #12=DIRECTION('',(2.,2.,0.));\n\
                #13=AXIS2_PLACEMENT_3D('',#1,#11,#12);\n\
                #14=DIRECTION('',(0.,1.E-13,0.));\n\
                #15=CARTESIAN_POINT('',(1.,2.));\n";
    let f = parse(data);
    let mut r = Resolver::new(&f, Units::MM);

    // Schiefe Bezugsrichtung (1, 0, 0,1) zu z = (0, 0, 1): x wird genau (1, 0, 0).
    let p = r.placement(4).unwrap();
    assert_close(p.origin, DVec3::new(1.0, 2.0, 3.0), 0.0);
    assert_close(p.z, DVec3::Z, 0.0);
    assert_close(p.x, DVec3::X, 1e-15);
    assert_close(p.y, DVec3::Y, 1e-15);
    orthonormal(&p);

    // `$` → Weltachsen.
    let w = r.placement(5).unwrap();
    assert_eq!((w.x, w.y, w.z), (DVec3::X, DVec3::Y, DVec3::Z));

    // Richtung der Länge null → ausgelassen mit Zeile der Richtung.
    assert_eq!(
        skip_text(r.placement(7)),
        format!(
            "Zeile {}: Eine Richtung hat die Länge null.",
            line_of(data, 6)
        )
    );
    assert!(skip_text(r.direction(14)).contains("Länge null"));
    assert!(skip_text(r.point(15)).starts_with(&format!("Zeile {}:", line_of(data, 15))));

    // Achse nicht normiert, Bezugsrichtung parallel zur Achse → beliebige Senkrechte.
    let q = r.placement(10).unwrap();
    assert_close(q.z, DVec3::Z, 0.0);
    assert!(q.x.dot(q.z).abs() <= 1e-15);
    orthonormal(&q);

    // Schräge Achse, nicht normierte Bezugsrichtung: x ist die Projektion, y = z × x.
    let s = r.placement(13).unwrap();
    assert_close(s.z, DVec3::new(0.0, 0.6, 0.8), 1e-15);
    let x = DVec3::new(1.0, 1.0, 0.0) - s.z * 0.6;
    assert_close(s.x, x.normalize(), 1e-15);
    orthonormal(&s);
    // Rechtshändig.
    assert!((s.x.cross(s.y).dot(s.z) - 1.0).abs() <= 1e-12);

    // Richtungen haben Länge 1.
    assert_close(r.direction(11).unwrap(), DVec3::new(0.0, 0.6, 0.8), 1e-15);
}

#[test]
fn curves_match_hand_values() {
    let mut data = String::from(
        "#20=CARTESIAN_POINT('',(1.,2.,3.));\n\
         #21=DIRECTION('',(0.,0.,1.));\n\
         #22=DIRECTION('',(1.,0.,0.));\n\
         #23=AXIS2_PLACEMENT_3D('',#20,#21,#22);\n\
         #24=CIRCLE('',#23,5.);\n\
         #25=ELLIPSE('',#23,2.,4.);\n\
         #26=DIRECTION('',(0.,3.,4.));\n\
         #27=VECTOR('',#26,7.);\n\
         #28=LINE('',#20,#27);\n\
         #29=SEAM_CURVE('',#24,(#30,#31),.PCURVE_S1.);\n\
         #30=PCURVE('',$,$);\n\
         #31=PCURVE('',$,$);\n\
         #32=TRIMMED_CURVE('',#24,(PARAMETER_VALUE(0.)),(PARAMETER_VALUE(1.)),.T.,.PARAMETER.);\n\
         #33=TRIMMED_CURVE('',#33,(PARAMETER_VALUE(0.)),(PARAMETER_VALUE(1.)),.T.,.PARAMETER.);\n\
         #34=TRIMMED_CURVE('',#35,(PARAMETER_VALUE(0.)),(PARAMETER_VALUE(1.)),.T.,.PARAMETER.);\n\
         #35=SURFACE_CURVE('',#34,(#30),.CURVE_3D.);\n\
         #36=B_SPLINE_CURVE_WITH_KNOTS('',1,(#20,#20),.UNSPECIFIED.,.F.,.F.,(2,2),(0.,1.),.UNSPECIFIED.);\n\
         #37=(BOUNDED_CURVE()B_SPLINE_CURVE(1,(#20,#20),.UNSPECIFIED.,.F.,.F.)B_SPLINE_CURVE_WITH_KNOTS((2,2),(0.,1.),.UNSPECIFIED.)CURVE()GEOMETRIC_REPRESENTATION_ITEM()RATIONAL_B_SPLINE_CURVE((1.,2.))REPRESENTATION_ITEM(''));\n\
         #38=INTERSECTION_CURVE('',#28,(#30,#31),.CURVE_3D.);\n\
         #39=CIRCLE('',#23,0.);\n\
         #40=ELLIPSE('',#23,2.,-1.);\n\
         #41=POLYLINE('',(#20,#20));\n",
    );
    // Kette aus 61 bzw. 62 Trimmkurven über dem Kreis #24: Kreis, Lage und Punkt kommen dazu.
    for i in 0..62u64 {
        let base = if i == 0 { 24 } else { 99 + i };
        data.push_str(&format!(
            "#{}=TRIMMED_CURVE('',#{base},(PARAMETER_VALUE(0.)),(PARAMETER_VALUE(1.)),.T.,.PARAMETER.);\n",
            100 + i
        ));
    }
    let f = parse(&data);
    let mut r = Resolver::new(&f, Units::MM);

    let c = r.curve(24).unwrap();
    assert_close(c.point(FRAC_PI_2), DVec3::new(1.0, 7.0, 3.0), 1e-12);
    assert_close(c.point(0.0), DVec3::new(6.0, 2.0, 3.0), 1e-12);

    // Ellipse a1 = 2 < a2 = 4: erste Halbachse entlang x, wie geschrieben.
    let e = r.curve(25).unwrap();
    assert!(matches!(e, Curve::Ellipse { major, minor, .. } if major == 2.0 && minor == 4.0));
    assert_close(e.point(0.0), DVec3::new(3.0, 2.0, 3.0), 1e-12);
    assert_close(e.point(FRAC_PI_2), DVec3::new(1.0, 6.0, 3.0), 1e-12);

    // Gerade: Richtung normiert, Betrag des Vektors zählt nicht.
    match r.curve(28).unwrap() {
        Curve::Line { origin, dir } => {
            assert_eq!(origin, DVec3::new(1.0, 2.0, 3.0));
            assert!((dir.length() - 1.0).abs() <= 1e-15);
            assert_close(dir, DVec3::new(0.0, 0.6, 0.8), 1e-15);
        }
        other => panic!("{other:?}"),
    }

    // Naht-, Schnitt- und Trimmkurven liefern ihre 3D-Kurve.
    assert_close(
        r.curve(29).unwrap().point(PI),
        DVec3::new(-4.0, 2.0, 3.0),
        1e-12,
    );
    assert!(matches!(r.curve(32).unwrap(), Curve::Circle { radius, .. } if radius == 5.0));
    assert!(matches!(r.curve(38).unwrap(), Curve::Line { .. }));

    // Verweise im Kreis: auf sich selbst (beim Zerlegen) und über zwei Einträge (Stapel).
    assert!(skip_text(r.curve(33)).contains("verweisen im Kreis"));
    assert_eq!(
        skip_text(r.curve(34)),
        StepReadError::Cycle {
            line: line_of(&data, 34),
            id: 34
        }
        .to_string()
    );

    // Freiformkanten kommen in Aufgabe 5.
    assert_eq!(
        skip_text(r.curve(36)),
        "Kante vom Typ B_SPLINE_CURVE_WITH_KNOTS wird noch nicht unterstützt."
    );
    assert_eq!(
        skip_text(r.curve(37)),
        "Kante vom Typ B_SPLINE_CURVE_WITH_KNOTS wird noch nicht unterstützt."
    );
    assert_eq!(
        skip_text(r.curve(41)),
        "Kante vom Typ POLYLINE wird noch nicht unterstützt."
    );
    assert_eq!(
        skip_text(r.curve(39)),
        format!(
            "Zeile {}: Der Radius eines Kreises ist nicht größer als null.",
            line_of(&data, 39)
        )
    );
    assert!(skip_text(r.curve(40)).contains("Halbachse einer Ellipse"));

    // Tiefe: 61 Trimmkurven + Kreis + Lage + Punkt = 64 Ebenen gehen, eine mehr nicht.
    assert_eq!(MAX_RESOLVE_DEPTH, 64);
    let mut fresh = Resolver::new(&f, Units::MM);
    assert!(matches!(fresh.curve(160).unwrap(), Curve::Circle { .. }));
    let mut fresh = Resolver::new(&f, Units::MM);
    assert_eq!(
        skip_text(fresh.curve(161)),
        StepReadError::TooDeep {
            line: line_of(&data, 20)
        }
        .to_string()
    );
}

/// Kontext #909 in Zoll (25,4 · mm) und Grad; Genauigkeit 1e-3 Zoll.
const INCH_DEGREE: &str = "#900=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));\n\
     #901=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(25.4),#900);\n\
     #902=DIMENSIONAL_EXPONENTS(1.,0.,0.,0.,0.,0.,0.);\n\
     #903=(CONVERSION_BASED_UNIT('INCH',#901)LENGTH_UNIT()NAMED_UNIT(#902));\n\
     #904=(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.));\n\
     #905=PLANE_ANGLE_MEASURE_WITH_UNIT(PLANE_ANGLE_MEASURE(0.0174532925199433),#904);\n\
     #906=(CONVERSION_BASED_UNIT('DEGREE',#905)NAMED_UNIT(#902)PLANE_ANGLE_UNIT());\n\
     #907=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-3),#903,'distance_accuracy_value','');\n\
     #909=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#907))GLOBAL_UNIT_ASSIGNED_CONTEXT((#903,#906))REPRESENTATION_CONTEXT('',''));\n";

/// Kontext #909 nur mit einer Längeneinheit `unit` (Eintrag #900).
fn length_only(unit: &str) -> String {
    format!(
        "#900={unit};\n\
         #909=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNIT_ASSIGNED_CONTEXT((#900))REPRESENTATION_CONTEXT('',''));\n"
    )
}

#[test]
fn units_and_angles() {
    let geo = "#1=CARTESIAN_POINT('',(1.,0.,0.));\n\
               #2=AXIS2_PLACEMENT_3D('',#1,$,$);\n\
               #3=CONICAL_SURFACE('',#2,1.,45.);\n";
    let data = format!("{INCH_DEGREE}{geo}");
    let f = parse(&data);
    let u = context_units(&f, 909).unwrap();
    assert_eq!(u.length, LengthUnit::Zoll);
    assert!(u.length_known);
    assert_eq!(u.length.mm(), 25.4);
    assert_eq!(u.length.label(), "Zoll");
    assert_eq!(u.angle_rad, PI / 180.0);
    assert_eq!(u.uncertainty_mm, Some(0.0254));
    assert_eq!(u.weld_tol_mm(), 1e-3);
    assert!(u.is_coarse());
    let mut r = Resolver::new(&f, u);
    assert_eq!(r.point(1).unwrap(), DVec3::new(25.4, 0.0, 0.0));
    match ready(r.surface(3).unwrap()) {
        Surface::Cone {
            frame,
            radius,
            slope,
        } => {
            assert_eq!(frame.origin, DVec3::new(25.4, 0.0, 0.0));
            assert_eq!(radius, 25.4);
            assert!((slope - 1.0).abs() <= 1e-12, "{slope}");
        }
        other => panic!("{other:?}"),
    }

    // Zoll über Zentimeter (2,54 · 10 mm) und Fuß über Zoll (12 · 25,4 mm): rekursiv.
    let data = "#900=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.CENTI.,.METRE.));\n\
                #901=(LENGTH_MEASURE_WITH_UNIT()MEASURE_WITH_UNIT(LENGTH_MEASURE(2.54),#900));\n\
                #902=(CONVERSION_BASED_UNIT('INCH',#901)LENGTH_UNIT()NAMED_UNIT(*));\n\
                #903=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(12.),#902);\n\
                #904=(CONVERSION_BASED_UNIT('FOOT',#903)LENGTH_UNIT()NAMED_UNIT(*));\n\
                #908=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNIT_ASSIGNED_CONTEXT((#902))REPRESENTATION_CONTEXT('',''));\n\
                #909=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNIT_ASSIGNED_CONTEXT((#904))REPRESENTATION_CONTEXT('',''));\n";
    let f = parse(data);
    assert_eq!(context_units(&f, 908).unwrap().length, LengthUnit::Zoll);
    let foot = context_units(&f, 909).unwrap().length;
    assert!(matches!(foot, LengthUnit::Other { mm } if (mm - 304.8).abs() < 1e-9));
    assert_eq!(foot.label(), "1 Einheit = 304,8 mm");

    // SI-Vorsätze.
    for (unit, want, mm) in [
        (
            "(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT($,.METRE.))",
            LengthUnit::Meter,
            1000.0,
        ),
        (
            "(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.CENTI.,.METRE.))",
            LengthUnit::Zentimeter,
            10.0,
        ),
        (
            "(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.))",
            LengthUnit::Millimeter,
            1.0,
        ),
        ("SI_UNIT(*,.MILLI.,.METRE.)", LengthUnit::Millimeter, 1.0),
        (
            "(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.DECI.,.METRE.))",
            LengthUnit::Other { mm: 100.0 },
            100.0,
        ),
        (
            "(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.KILO.,.METRE.))",
            LengthUnit::Other { mm: 1e6 },
            1e6,
        ),
        (
            "(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MICRO.,.METRE.))",
            LengthUnit::Other { mm: 1e-3 },
            1e-3,
        ),
    ] {
        let data = format!("{}#1=CARTESIAN_POINT('',(1.,0.,0.));\n", length_only(unit));
        let f = parse(&data);
        let u = context_units(&f, 909).unwrap();
        assert_eq!(u.length, want, "{unit}");
        assert_eq!(u.length.mm(), mm, "{unit}");
        assert!(u.length_known);
        assert_eq!(u.angle_rad, 1.0, "fehlender Winkel → Radiant");
        let mut r = Resolver::new(&f, u);
        assert_eq!(r.point(1).unwrap(), DVec3::new(mm, 0.0, 0.0), "{unit}");
    }
    assert_eq!(LengthUnit::Meter.label(), "Meter");
    assert_eq!(LengthUnit::Zentimeter.label(), "Zentimeter");
    assert_eq!(LengthUnit::Millimeter.label(), "Millimeter");

    // Genauigkeit 1e-7 mm → Untergrenze 1e-6, nicht grob.
    let f = parse(&mm_context("1.E-7"));
    let u = context_units(&f, 909).unwrap();
    assert_eq!(u.length, LengthUnit::Millimeter);
    assert_eq!(u.uncertainty_mm, Some(1e-7));
    assert_eq!(u.weld_tol_mm(), MIN_WELD_TOL_MM);
    assert!(!u.is_coarse());
    // 1e-5 mm ist gerade noch nicht grob, 2e-5 mm schon.
    let f = parse(&mm_context("1.E-5"));
    assert!(!context_units(&f, 909).unwrap().is_coarse());
    let f = parse(&mm_context("2.E-5"));
    assert!(context_units(&f, 909).unwrap().is_coarse());
    assert_eq!(COARSE_TOL_MM, 1e-5);

    // Mehrere Genauigkeiten: die kleinste Länge zählt, Winkel-Genauigkeiten nicht.
    let f = parse(
        "#900=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));\n\
         #901=(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.));\n\
         #902=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-4),#900,'a','');\n\
         #903=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(3.E-6),#900,'b','');\n\
         #904=UNCERTAINTY_MEASURE_WITH_UNIT(PLANE_ANGLE_MEASURE(1.E-9),#901,'c','');\n\
         #909=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#902,#903,#904))\
         GLOBAL_UNIT_ASSIGNED_CONTEXT((#900,#901))REPRESENTATION_CONTEXT('',''));\n",
    );
    let u = context_units(&f, 909).unwrap();
    assert_eq!(u.uncertainty_mm, Some(3e-6));
    assert_eq!(u.weld_tol_mm(), 3e-6);

    // Ohne Genauigkeit: Vorgabe 1e-3 mm und grob.
    let f = parse(&length_only(
        "(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.))",
    ));
    let u = context_units(&f, 909).unwrap();
    assert_eq!(u.uncertainty_mm, None);
    assert_eq!(u.weld_tol_mm(), DEFAULT_FILE_TOL_MM);
    assert_eq!(u.weld_tol_mm(), MAX_WELD_TOL_MM);
    assert!(u.is_coarse());

    // Ohne Längeneinheit: Millimeter angenommen, aber nicht bekannt.
    let f = parse(
        "#901=(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.));\n\
         #909=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNIT_ASSIGNED_CONTEXT((#901))REPRESENTATION_CONTEXT('',''));\n\
         #910=GEOMETRIC_REPRESENTATION_CONTEXT(3);\n",
    );
    for ctx in [909, 910] {
        let u = context_units(&f, ctx).unwrap();
        assert_eq!(u.length, LengthUnit::Millimeter);
        assert!(!u.length_known);
        assert_eq!(u.angle_rad, 1.0);
    }

    // Unbekannter Vorsatz und Einheitenkreis → Meldung mit Zeile statt Absturz.
    let data = length_only("(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.FOO.,.METRE.))");
    let f = parse(&data);
    let e = context_units(&f, 909).unwrap_err().to_string();
    assert!(
        e.starts_with(&format!("Zeile {}:", line_of(&data, 900))),
        "{e}"
    );
    let data = "#900=(CONVERSION_BASED_UNIT('X',#901)LENGTH_UNIT()NAMED_UNIT(*));\n\
                #901=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(2.),#900);\n\
                #909=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNIT_ASSIGNED_CONTEXT((#900))REPRESENTATION_CONTEXT('',''));\n";
    let f = parse(data);
    assert!(
        context_units(&f, 909)
            .unwrap_err()
            .to_string()
            .contains("höchstens 8 Stufen")
    );
}

#[test]
fn surfaces_match_the_kernel_conventions() {
    // Lage O = (1, 2, 3), z = (0, 0, 1), x = (0, 1, 0) → y = z × x = (−1, 0, 0).
    let data = "#1=CARTESIAN_POINT('',(1.,2.,3.));\n\
                #2=DIRECTION('',(0.,0.,1.));\n\
                #3=DIRECTION('',(0.,1.,0.));\n\
                #4=AXIS2_PLACEMENT_3D('',#1,#2,#3);\n\
                #5=PLANE('',#4);\n\
                #6=CYLINDRICAL_SURFACE('',#4,5.);\n\
                #7=CONICAL_SURFACE('',#4,2.,0.785398163397448);\n\
                #8=SPHERICAL_SURFACE('',#4,3.);\n\
                #9=AXIS2_PLACEMENT_3D('',#10,$,$);\n\
                #10=CARTESIAN_POINT('',(0.,0.,0.));\n\
                #11=TOROIDAL_SURFACE('',#9,20.,5.);\n\
                #12=TOROIDAL_SURFACE('',#9,5.,5.);\n\
                #13=TOROIDAL_SURFACE('',#9,5.,6.);\n\
                #14=B_SPLINE_SURFACE_WITH_KNOTS('',1,1,((#1,#1),(#1,#1)),.UNSPECIFIED.,.F.,.F.,.F.,(2,2),(2,2),(0.,1.),(0.,1.),.UNSPECIFIED.);\n\
                #15=OFFSET_SURFACE('',#5,1.,.F.);\n\
                #16=CYLINDRICAL_SURFACE('',#4,0.);\n\
                #17=CYLINDRICAL_SURFACE('',#4,-1.);\n\
                #18=CONICAL_SURFACE('',#4,2.,0.);\n\
                #19=CONICAL_SURFACE('',#4,2.,1.5707963267948966);\n\
                #20=CONICAL_SURFACE('',#4,0.,0.5);\n\
                #21=(BOUNDED_SURFACE()B_SPLINE_SURFACE(1,1,((#1,#1),(#1,#1)),.UNSPECIFIED.,.F.,.F.,.F.)B_SPLINE_SURFACE_WITH_KNOTS((2,2),(2,2),(0.,1.),(0.,1.),.UNSPECIFIED.)GEOMETRIC_REPRESENTATION_ITEM()RATIONAL_B_SPLINE_SURFACE(((1.,1.),(1.,1.)))REPRESENTATION_ITEM('')SURFACE());\n\
                #22=SURFACE_OF_REVOLUTION('',#23,#24);\n\
                #23=LINE('',#1,#25);\n\
                #24=AXIS1_PLACEMENT('',#1,#2);\n\
                #25=VECTOR('',#2,1.);\n\
                #26=SPHERICAL_SURFACE('',#4,-3.);\n\
                #27=TOROIDAL_SURFACE('',#9,20.,0.);\n";
    let f = parse(data);
    let mut r = Resolver::new(&f, Units::MM);
    let o = DVec3::new(1.0, 2.0, 3.0);
    let (x, y, z) = (DVec3::Y, DVec3::NEG_X, DVec3::Z);

    // Ebene: σ = O + u·x + v·y, Normale z.
    let s = ready(r.surface(5).unwrap());
    assert_close(s.point(2.0, 3.0), DVec3::new(-2.0, 4.0, 3.0), 1e-12);
    assert_close(s.point(2.0, 3.0), o + x * 2.0 + y * 3.0, 1e-12);
    assert_close(s.normal(2.0, 3.0), z, 1e-12);

    // Zylinder r 5: σ = O + 5(cos u·x + sin u·y) + v·z; Normale radial nach außen.
    let s = ready(r.surface(6).unwrap());
    assert_close(s.point(0.0, 4.0), DVec3::new(1.0, 7.0, 7.0), 1e-12);
    assert_close(s.point(FRAC_PI_2, 0.0), DVec3::new(-4.0, 2.0, 3.0), 1e-12);
    for (u, v) in [(0.0, 4.0), (FRAC_PI_2, 0.0), (2.0, -1.0)] {
        let p = s.point(u, v);
        let out = (p - (o + z * v)) / 5.0;
        assert_close(s.normal(u, v), out, 1e-12);
    }

    // Kegel r 2, α = π/4: σ = O + (2 + v·tan α)(cos u·x + sin u·y) + v·z;
    // STEP-Normale σ_u × σ_v bei u = 0: (x − tan α·z)/√2.
    let s = ready(r.surface(7).unwrap());
    assert_close(s.point(0.0, 2.0), DVec3::new(1.0, 6.0, 5.0), 1e-12);
    assert_close(s.point(PI, -1.0), o - x * 1.0 - z, 1e-12);
    assert_close(
        s.normal(0.0, 2.0),
        DVec3::new(0.0, FRAC_PI_4.cos(), -FRAC_PI_4.sin()),
        1e-12,
    );

    // Kugel r 3: σ = O + 3(cos v(cos u·x + sin u·y) + sin v·z); Normale nach außen.
    let s = ready(r.surface(8).unwrap());
    assert_close(s.point(0.0, 0.0), DVec3::new(1.0, 5.0, 3.0), 1e-12);
    assert_close(s.point(0.0, FRAC_PI_2), DVec3::new(1.0, 2.0, 6.0), 1e-12);
    for (u, v) in [(0.0, 0.0), (1.0, 0.5), (-2.0, -1.2)] {
        assert_close(s.normal(u, v), (s.point(u, v) - o) / 3.0, 1e-12);
    }

    // Torus R 20, r 5 um den Ursprung: σ = (20 + 5 cos v)(cos u, sin u, 0) + 5 sin v·z.
    let s = ready(r.surface(11).unwrap());
    assert_close(s.point(0.0, 0.0), DVec3::new(25.0, 0.0, 0.0), 1e-12);
    assert_close(
        s.point(FRAC_PI_2, FRAC_PI_2),
        DVec3::new(0.0, 20.0, 5.0),
        1e-12,
    );
    assert_close(s.point(0.0, PI), DVec3::new(15.0, 0.0, 0.0), 1e-12);
    // Normale vom Rohrmittelkreis weg (innen: zur Achse hin).
    assert_close(s.normal(0.0, PI), DVec3::new(-1.0, 0.0, 0.0), 1e-12);
    assert_close(s.normal(0.0, 0.0), DVec3::new(1.0, 0.0, 0.0), 1e-12);

    let torus = "Eine Torusfläche mit kleinem Radius größer oder gleich dem großen wird noch nicht unterstützt.";
    assert_eq!(skip_text(r.surface(12)), torus);
    assert_eq!(skip_text(r.surface(13)), torus);

    assert_eq!(
        skip_text(r.surface(14)),
        "Fläche vom Typ B_SPLINE_SURFACE_WITH_KNOTS wird noch nicht unterstützt."
    );
    assert_eq!(
        skip_text(r.surface(21)),
        "Fläche vom Typ B_SPLINE_SURFACE_WITH_KNOTS wird noch nicht unterstützt."
    );
    assert_eq!(
        skip_text(r.surface(15)),
        "Fläche vom Typ OFFSET_SURFACE wird noch nicht unterstützt."
    );
    assert_eq!(
        skip_text(r.surface(22)),
        "Fläche vom Typ SURFACE_OF_REVOLUTION wird noch nicht unterstützt."
    );

    // Radien ≤ 0 und Kegelwinkel außerhalb (0, π/2) → mit Zeile ausgelassen.
    for (id, text) in [
        (16, "Der Radius eines Zylinders ist nicht größer als null."),
        (17, "Der Radius eines Zylinders ist nicht größer als null."),
        (26, "Der Radius einer Kugel ist nicht größer als null."),
        (
            27,
            "Der kleine Radius eines Torus ist nicht größer als null.",
        ),
        (
            18,
            "Der halbe Öffnungswinkel eines Kegels liegt nicht zwischen 0 und 90 Grad.",
        ),
        (
            19,
            "Der halbe Öffnungswinkel eines Kegels liegt nicht zwischen 0 und 90 Grad.",
        ),
    ] {
        assert_eq!(
            skip_text(r.surface(id)),
            format!("Zeile {}: {text}", line_of(data, id))
        );
    }

    // Kegel mit Spitze im Ursprung der Lage (Radius 0, ISO 10303-42 erlaubt das).
    match ready(r.surface(20).unwrap()) {
        Surface::Cone { radius, slope, .. } => {
            assert_eq!(radius, 0.0);
            assert!((slope - 0.5f64.tan()).abs() <= 1e-15);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn units_override() {
    let data = format!(
        "{}#1=CARTESIAN_POINT('',(1.,0.,0.));\n",
        mm_context("0.001")
    );
    let f = parse(&data);
    let file_units = context_units(&f, 909).unwrap();
    assert_eq!(file_units.uncertainty_mm, Some(0.001));
    let u = file_units.with_length(LengthUnit::Meter);
    assert_eq!(u.length, LengthUnit::Meter);
    assert!(u.length_known);
    assert_eq!(u.angle_rad, file_units.angle_rad);
    assert_eq!(u.uncertainty_mm, Some(1.0));
    assert_eq!(u.weld_tol_mm(), 1e-3);
    let mut r = Resolver::new(&f, u);
    assert_eq!(r.point(1).unwrap(), DVec3::new(1000.0, 0.0, 0.0));
    assert_eq!(r.units(), &u);

    // Zoll-Datei auf Millimeter umgestellt: 1e-3 Zoll = 0,0254 mm → 1e-3 mm-Zahlen.
    let f = parse(INCH_DEGREE);
    let inch = context_units(&f, 909).unwrap();
    let mm = inch.with_length(LengthUnit::Millimeter);
    assert!((mm.uncertainty_mm.unwrap() - 1e-3).abs() <= 1e-18);
    assert_eq!(
        mm.angle_rad,
        PI / 180.0,
        "der Winkel bleibt wie in der Datei"
    );

    // Einheit unbekannt → nach dem Umstellen bekannt.
    let unknown = Units {
        length_known: false,
        ..Units::MM
    };
    assert!(unknown.with_length(LengthUnit::Zoll).length_known);
    assert_eq!(Units::MM.with_length(LengthUnit::Zoll).uncertainty_mm, None);
}

#[test]
fn skip_carries_the_message() {
    let s: Skip = StepReadError::Cycle { line: 9, id: 4 }.into();
    assert_eq!(
        s,
        Skip("Zeile 9: Die Einträge verweisen im Kreis (#4).".into())
    );
    let f = parse("#1=CARTESIAN_POINT('',(0.,0.,0.));\n");
    let mut r = Resolver::new(&f, Units::MM);
    assert_eq!(skip_text(r.point(77)), "Den Eintrag #77 gibt es nicht.");
    assert!(std::ptr::eq(r.file(), &f));
}
