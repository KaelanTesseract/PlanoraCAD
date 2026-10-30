//! Task 12: Die Meldungen des Kerns erscheinen unter `with_lang(&english())` auf Englisch, und
//! unter Deutsch bleiben sie wie bisher. Die Art eines Fehlers steht in der Variante, nicht im
//! Text.

use planora_i18n::{english, german, with_lang};
use planora_kernel::blend::build::cavity;
use planora_kernel::blend::march::{folded, inaccurate, oblique_end, section_arc, variable_corner};
use planora_kernel::blend::section::freeform_pole;
use planora_kernel::boolean::{surface_label, too_expensive};
use planora_kernel::*;
use planora_math::{DVec3, Tolerance};

/// Nichts Deutsches und kein Schlüssel, der durchgerutscht ist.
fn assert_english(what: &str, text: &str) {
    assert!(!text.is_empty(), "{what}: leer");
    assert!(
        !text.chars().any(|c| "äöüßÄÖÜ„“".contains(c)),
        "{what}: {text}"
    );
    let looks_like_key = !text.contains(' ')
        && text.contains('-')
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    assert!(!looks_like_key, "{what}: Schlüssel statt Text: {text}");
}

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn kernel_errors() -> Vec<KernelError> {
    vec![
        KernelError::ProfileNotClosed,
        KernelError::DegenerateSegment,
        KernelError::ZeroAreaProfile,
        KernelError::InvalidDimension(0.5),
        KernelError::ProfileOffAxis,
        KernelError::ProfileCrossesAxis,
        KernelError::UnsupportedSegment("x".into()),
        KernelError::NonSimilarityTransform,
        KernelError::InvalidFrame,
        KernelError::HoleOutsideOuter,
        KernelError::ProfileLoopsIntersect,
        KernelError::NestedHoles,
        KernelError::ProfileSelfIntersects,
        KernelError::InvalidSurface("x".into()),
        KernelError::InvalidCurve("x".into()),
    ]
}

fn blend_errors() -> Vec<BlendError> {
    vec![
        BlendError::InvalidParameter("x".into()),
        BlendError::NoEdges,
        BlendError::EdgeNotFound,
        BlendError::InvalidInput("x".into()),
        BlendError::SeamEdge,
        BlendError::NearlyTangent { angle_deg: 1.234 },
        BlendError::TooLarge {
            face: "Deckel".into(),
        },
        BlendError::CurvatureTooSmall {
            face: "Fläche 3".into(),
        },
        BlendError::SpineLost,
        BlendError::Unsupported("x".into()),
        BlendError::Corner("x".into()),
        BlendError::TooExpensive,
        BlendError::ResultInvalid("x".into()),
    ]
}

fn boolean_errors() -> Vec<BooleanError> {
    vec![
        BooleanError::UnsupportedIntersection {
            a: "Zylinder",
            b: "Freiformfläche",
        },
        BooleanError::UnsupportedIntersection {
            a: "Ebene",
            b: "Torus",
        },
        BooleanError::Unsupported("x".into()),
        BooleanError::FreeformLimit("x".into()),
        BooleanError::TooExpensive,
        BooleanError::AmbiguousContact,
        BooleanError::AmbiguousClassification,
        BooleanError::NotClosed,
        BooleanError::ResultInvalid("x".into()),
        BooleanError::EmptyResult,
        BooleanError::InvalidInput("x".into()),
    ]
}

fn validation_errors() -> Vec<ValidationError> {
    vec![
        ValidationError::EdgeUseCount(1, 3),
        ValidationError::EdgeOrientation(2),
        ValidationError::OpenLoop(3),
        ValidationError::EmptyLoop(4),
        ValidationError::MissingPatch(5),
        ValidationError::InvalidEulerCharacteristic(12345),
        ValidationError::ShellMembership(6),
        ValidationError::EdgeOffSurface(7, 8),
        ValidationError::HoleAcrossSeam(9),
    ]
}

#[test]
fn every_error_of_the_kernel_speaks_english() {
    with_lang(&english(), || {
        for e in kernel_errors() {
            assert_english("KernelError", &e.to_string());
        }
        for e in blend_errors() {
            assert_english("BlendError", &e.to_string());
        }
        for e in boolean_errors() {
            assert_english("BooleanError", &e.to_string());
        }
        for e in validation_errors() {
            assert_english("ValidationError", &e.to_string());
        }
        for text in [
            cavity(),
            section_arc(),
            oblique_end(),
            inaccurate(),
            folded(),
            variable_corner(),
            freeform_pole(),
            too_expensive(),
        ] {
            assert_english("Meldung", &text);
        }
        for name in [
            "Ebene",
            "Zylinder",
            "Kegel",
            "Kugel",
            "Torus",
            "Freiformfläche",
        ] {
            assert_english("Flächenart", &surface_label(name));
        }
        for kind in [
            SurfaceKind::Plane,
            SurfaceKind::Cylinder,
            SurfaceKind::Cone,
            SurfaceKind::Sphere,
            SurfaceKind::Torus,
            SurfaceKind::Nurbs,
        ] {
            assert_english("SurfaceKind", &kind.label());
        }
    });
}

#[test]
fn german_texts_stay_as_they_were() {
    with_lang(&german(), || {
        assert_eq!(
            KernelError::ProfileNotClosed.to_string(),
            "Das Profil ist nicht geschlossen"
        );
        assert_eq!(
            KernelError::InvalidDimension(0.5).to_string(),
            "Das Maß muss eine Zahl größer als die Toleranz sein (war 0.5)"
        );
        assert_eq!(
            BlendError::NearlyTangent { angle_deg: 1.234 }.to_string(),
            "Die Flächen an der Kante gehen fast tangential ineinander über (Kantenwinkel 1.2°); dort ist keine Rundung oder Fase möglich"
        );
        assert_eq!(
            BooleanError::UnsupportedIntersection {
                a: "Zylinder",
                b: "Freiformfläche"
            }
            .to_string(),
            "Der Schnitt von Zylinder mit Freiformfläche wird noch nicht unterstützt"
        );
        assert_eq!(
            BooleanError::TooExpensive.to_string(),
            "Die Berechnung ist zu aufwendig (Arbeitsgrenze für Freiformflächen erreicht); bitte weniger Spline-Punkte oder eine einfachere Form verwenden"
        );
        assert_eq!(SurfaceKind::Plane.label(), "Ebene");
        assert_eq!(SurfaceKind::Nurbs.label(), "Freiformfläche");
    });
}

#[test]
fn the_kind_of_a_boolean_error_does_not_depend_on_the_language() {
    for lang in [german(), english()] {
        with_lang(&lang, || {
            assert!(BooleanError::TooExpensive.has_user_hint());
            assert!(BooleanError::TooExpensive.is_freeform_limit());
            assert!(!BooleanError::FreeformLimit("x".into()).has_user_hint());
            assert!(BooleanError::FreeformLimit("x".into()).is_freeform_limit());
            // Ein Wortlaut, der wie die Arbeitsgrenze klingt, macht aus einer allgemeinen
            // Meldung keine Freiform-Grenze.
            assert!(!BooleanError::Unsupported(too_expensive()).has_user_hint());
            assert!(!BooleanError::Unsupported(too_expensive()).is_freeform_limit());
            assert!(
                BooleanError::UnsupportedIntersection {
                    a: "Ebene",
                    b: "Freiformfläche"
                }
                .is_freeform_limit()
            );
            assert!(
                !BooleanError::UnsupportedIntersection {
                    a: "Ebene",
                    b: "Kegel"
                }
                .is_freeform_limit()
            );
        });
    }
}

#[test]
fn failing_operations_speak_english() {
    with_lang(&english(), || {
        let tol = Tolerance::default();
        // Zu großes Maß, Rundung ohne Kante, ungültiges Maß.
        let body = k().make_box(DVec3::splat(10.0)).unwrap();
        let edges: Vec<Id<Edge>> = body.edges.iter().map(|(id, _)| id).take(1).collect();
        let request = |edges: &[Id<Edge>], radius: f64| BlendRequest {
            edges: edges
                .iter()
                .map(|&edge| BlendEdge {
                    edge,
                    first_face: None,
                })
                .collect(),
            kind: BlendKind::Fillet { radius },
            tangent_chain: false,
        };
        for (what, r) in [
            ("zu groß", request(&edges, 50.0)),
            ("ohne Kante", request(&[], 1.0)),
            ("nicht endlich", request(&edges, f64::NAN)),
            ("unendlich", request(&edges, f64::INFINITY)),
            ("null", request(&edges, 0.0)),
            ("winzig", request(&edges, 1e-300)),
        ] {
            let e = k().blend(&body, &r).unwrap_err();
            assert_english(what, &e.to_string());
        }
        // Zwei Körper, die sich nur tangential berühren, und ein leerer Körper.
        let a = k().make_box(DVec3::splat(10.0)).unwrap();
        let empty = Solid::default();
        let e = planora_kernel::boolean::boolean(&a, &empty, BoolOp::Union, &tol).unwrap_err();
        assert_english("leerer Körper", &e.to_string());
        // Ungültige Ähnlichkeitstransformation.
        let skew = planora_math::DMat4::from_scale(DVec3::new(1.0, 2.0, 3.0));
        let e = k().transform(&a, &skew).unwrap_err();
        assert_english("Matrix", &e.to_string());
    });
}

#[test]
fn face_roles_are_shown_in_the_current_language() {
    use planora_kernel::blend::topo::role_label;
    let roles = [
        ("Deckel", "Top"),
        ("Boden", "Bottom"),
        ("Wand 2", "Wall 2"),
        ("Fläche 3", "Face 3"),
        ("Ecke 1", "Corner 1"),
        ("STEP-Fläche 7", "STEP face 7"),
        ("Rundung 1", "Blend 1"),
        ("Fase 2", "Chamfer 2"),
        ("Loch 1, Wand 3", "Hole 1, Wall 3"),
    ];
    with_lang(&german(), || {
        for (de, _) in roles {
            assert_eq!(role_label(de), de);
        }
        assert_eq!(role_label("Meine Marke"), "Meine Marke");
    });
    with_lang(&english(), || {
        for (de, en) in roles {
            assert_eq!(role_label(de), en);
        }
        // Unbekannte Marken und Nummern ohne Ziffern bleiben unverändert.
        assert_eq!(role_label("Meine Marke"), "Meine Marke");
        assert_eq!(role_label("Wand x"), "Wand x");
        assert_eq!(role_label(""), "");
        let e = BlendError::TooLarge {
            face: "Wand 1".into(),
        };
        assert!(e.to_string().contains("\"Wall 1\""), "{e}");
    });
}
