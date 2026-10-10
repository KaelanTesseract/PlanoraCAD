use planora_kernel::{Profile, ProfileLoop, Segment};
use planora_math::{DVec2, PI};

/// Hinweis in den Beispielprofil-Dialogen auf eigene Profile aus Skizzen.
pub const OWN_PROFILE_HINT: &str = "Eigene Profile: eine Skizze zeichnen und dann „Skizze extrudieren …“ bzw. „Skizze drehen …“ wählen.";

/// Fest eingebaute Beispielprofile für die Extrusion. Eigene Profile entstehen aus Skizzen
/// (Dialoge „Skizze extrudieren“ / „Skizze drehen“).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtrudeSample {
    PlateWithHole,
    LShape,
    NotchedBar,
    TwoHoles,
}

impl ExtrudeSample {
    pub const ALL: [ExtrudeSample; 4] = [
        ExtrudeSample::PlateWithHole,
        ExtrudeSample::LShape,
        ExtrudeSample::NotchedBar,
        ExtrudeSample::TwoHoles,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ExtrudeSample::PlateWithHole => "Platte mit rundem Loch",
            ExtrudeSample::LShape => "L-Profil",
            ExtrudeSample::NotchedBar => "Leiste mit runder Kerbe",
            ExtrudeSample::TwoHoles => "Platte mit zwei Löchern",
        }
    }

    pub fn default_height(self) -> f64 {
        match self {
            ExtrudeSample::PlateWithHole | ExtrudeSample::TwoHoles => 5.0,
            ExtrudeSample::LShape => 20.0,
            ExtrudeSample::NotchedBar => 15.0,
        }
    }

    pub fn profile(self) -> Profile {
        match self {
            ExtrudeSample::PlateWithHole => Profile {
                outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(40.0, 40.0)),
                holes: vec![ProfileLoop::circle(DVec2::new(20.0, 20.0), 8.0)],
            },
            ExtrudeSample::LShape => Profile {
                outer: ProfileLoop::polygon(&[
                    DVec2::new(0.0, 0.0),
                    DVec2::new(40.0, 0.0),
                    DVec2::new(40.0, 10.0),
                    DVec2::new(10.0, 10.0),
                    DVec2::new(10.0, 30.0),
                    DVec2::new(0.0, 30.0),
                ])
                .expect("Beispielprofil"),
                holes: vec![],
            },
            ExtrudeSample::NotchedBar => Profile {
                outer: ProfileLoop {
                    start: DVec2::new(0.0, 0.0),
                    segments: vec![
                        Segment::Line {
                            to: DVec2::new(40.0, 0.0),
                        },
                        Segment::Line {
                            to: DVec2::new(40.0, 20.0),
                        },
                        Segment::Line {
                            to: DVec2::new(30.0, 20.0),
                        },
                        Segment::Arc {
                            center: DVec2::new(20.0, 20.0),
                            sweep: -PI,
                        },
                        Segment::Line {
                            to: DVec2::new(0.0, 20.0),
                        },
                        Segment::Line {
                            to: DVec2::new(0.0, 0.0),
                        },
                    ],
                },
                holes: vec![],
            },
            ExtrudeSample::TwoHoles => Profile {
                outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(70.0, 30.0)),
                holes: vec![
                    ProfileLoop::circle(DVec2::new(20.0, 15.0), 6.0),
                    ProfileLoop::circle(DVec2::new(50.0, 15.0), 6.0),
                ],
            },
        }
    }
}

/// Fest eingebaute Beispielketten für die Rotation (Halbebene: x = Abstand von der Achse, y = Höhe).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevolveSample {
    Vase,
    Capsule,
    Puck,
}

impl RevolveSample {
    pub const ALL: [RevolveSample; 3] = [
        RevolveSample::Vase,
        RevolveSample::Capsule,
        RevolveSample::Puck,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RevolveSample::Vase => "Vase",
            RevolveSample::Capsule => "Kapsel",
            RevolveSample::Puck => "Scheibe mit gerundeter Kante",
        }
    }

    pub fn chain(self) -> ProfileLoop {
        match self {
            RevolveSample::Vase => ProfileLoop {
                start: DVec2::new(0.0, 0.0),
                segments: vec![
                    Segment::Line {
                        to: DVec2::new(15.0, 0.0),
                    },
                    Segment::Line {
                        to: DVec2::new(10.0, 25.0),
                    },
                    Segment::Line {
                        to: DVec2::new(14.0, 45.0),
                    },
                    Segment::Line {
                        to: DVec2::new(0.0, 45.0),
                    },
                ],
            },
            RevolveSample::Capsule => ProfileLoop {
                start: DVec2::new(0.0, 0.0),
                segments: vec![
                    Segment::Arc {
                        center: DVec2::new(0.0, 10.0),
                        sweep: PI / 2.0,
                    },
                    Segment::Line {
                        to: DVec2::new(10.0, 30.0),
                    },
                    Segment::Arc {
                        center: DVec2::new(0.0, 30.0),
                        sweep: PI / 2.0,
                    },
                ],
            },
            RevolveSample::Puck => ProfileLoop {
                start: DVec2::new(0.0, 0.0),
                segments: vec![
                    Segment::Line {
                        to: DVec2::new(30.0, 0.0),
                    },
                    Segment::Arc {
                        center: DVec2::new(30.0, 10.0),
                        sweep: PI / 2.0,
                    },
                    Segment::Line {
                        to: DVec2::new(0.0, 10.0),
                    },
                ],
            },
        }
    }
}
