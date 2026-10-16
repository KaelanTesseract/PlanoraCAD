use egui::{Color32, Rect, Shape, pos2, vec2};
use planora_app::icons::{self, ALL, HAND_DRAWN, Icon};
use planora_app::state::BodyKind;
use planora_app::{fonts, theme};
use planora_drawing::dimension::DimKind;

/// Hat die Schrift in ihrer Zeichentabelle einen Eintrag für `c`? (Wie in tests/fonts.rs: die
/// egui-Abfrage `has_glyph` ist hier nicht verlässlich.)
fn font_has_char(bytes: &[u8], c: char) -> bool {
    use skrifa::MetadataProvider;
    skrifa::FontRef::new(bytes)
        .expect("gültige Schriftdatei")
        .charmap()
        .map(c)
        .is_some()
}

fn icon_font_bytes() -> Vec<u8> {
    let defs = fonts::definitions();
    let chain = &defs.families[&fonts::icons()];
    assert_eq!(chain.len(), 1, "Symbolfamilie hat genau eine Schrift");
    defs.font_data[&chain[0]].font.to_vec()
}

#[test]
fn every_icon_has_a_glyph_in_the_icon_font() {
    let bytes = icon_font_bytes();
    assert!(ALL.len() >= 50, "zu wenige Symbole: {}", ALL.len());
    for &icon in ALL {
        let c = icons::glyph(icon);
        assert!(
            ('\u{E000}'..='\u{F8FF}').contains(&c),
            "{icon:?}: {c:?} liegt nicht im privaten Bereich"
        );
        assert!(
            font_has_char(&bytes, c),
            "{icon:?}: {c:?} fehlt in Phosphor-Light.ttf"
        );
    }
}

#[test]
fn glyphs_are_unique() {
    let mut seen = std::collections::BTreeMap::new();
    for &icon in ALL {
        if let Some(other) = seen.insert(icons::glyph(icon), icon) {
            panic!("{icon:?} und {other:?} teilen {:?}", icons::glyph(icon));
        }
    }
}

#[test]
fn every_icon_is_listed_once_in_all() {
    let mut names = std::collections::BTreeSet::new();
    for &icon in ALL {
        assert!(names.insert(format!("{icon:?}")), "{icon:?} doppelt in ALL");
        assert!(!icons::phosphor_name(icon).is_empty());
    }
}

/// Die Codepunkte stammen aus der mitgelieferten CSS-Datei, nicht aus dem Gedächtnis: Zu jedem
/// Symbol steht der Phosphor-Name in der Tabelle, und die CSS-Datei muss dafür denselben
/// Codepunkt liefern.
#[test]
fn glyphs_match_the_phosphor_stylesheet() {
    let css = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets/fonts/Phosphor-Light-style.css"),
    )
    .unwrap();
    let marker = "content: \"\\";
    for &icon in ALL {
        let name = icons::phosphor_name(icon);
        let rule = format!(".ph-light.ph-{name}:before");
        let at = css
            .find(&rule)
            .unwrap_or_else(|| panic!("{icon:?}: {name} nicht in der CSS-Datei"));
        let rest = &css[at..];
        let start = rest.find(marker).expect("content") + marker.len();
        let hex: String = rest[start..]
            .chars()
            .take_while(char::is_ascii_hexdigit)
            .collect();
        let code = u32::from_str_radix(&hex, 16).unwrap();
        assert_eq!(
            icons::glyph(icon) as u32,
            code,
            "{icon:?} ({name}): falscher Codepunkt"
        );
    }
}

fn section_text(job: &egui::text::LayoutJob, i: usize) -> &str {
    let r = &job.sections[i].byte_range;
    &job.text[r.start.0..r.end.0]
}

#[test]
fn icon_label_text_contains_the_label() {
    let w = icons::label(Icon::New, "Neu");
    let egui::WidgetText::LayoutJob(job) = &w else {
        panic!("label liefert einen LayoutJob");
    };
    assert!(job.text.contains("Neu"), "{:?}", job.text);
    assert_eq!(job.sections.len(), 2);
    let glyph = section_text(job, 0);
    assert_eq!(glyph, icons::glyph(Icon::New).to_string());
    let icon_fmt = &job.sections[0].format;
    assert_eq!(icon_fmt.font_id.size, 16.0);
    assert_eq!(icon_fmt.font_id.family, fonts::icons());
    // Abstand 8 px zwischen Symbol und Text
    assert_eq!(job.sections[1].leading_space, 8.0);
    assert_eq!(section_text(job, 1), "Neu");
}

#[test]
fn icon_text_uses_the_icon_family() {
    let t = icons::text(Icon::Save, 14.0, Color32::RED);
    assert_eq!(t.text(), icons::glyph(Icon::Save).to_string());
    let w: egui::WidgetText = t.into();
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    let mut o = ctx.run_ui(egui::RawInput::default(), |ui| {
        let galley = w
            .clone()
            .into_galley(ui, None, f32::INFINITY, egui::TextStyle::Button);
        let fmt = &galley.job.sections[0].format;
        assert_eq!(fmt.font_id.family, fonts::icons());
        assert_eq!(fmt.font_id.size, 14.0);
        assert_eq!(fmt.color, Color32::RED);
    });
    o.textures_delta.clear();
}

/// Alle Strichbreiten, die ein Zeichenaufruf erzeugt (Linien, Pfade, Rechtecke, Kreise,
/// Ellipsen; gefüllte Punkte ohne Strich zählen nicht).
fn widths(paint: impl Fn(&egui::Painter, Rect)) -> Vec<f32> {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    let mut out = Vec::new();
    let mut o = ctx.run_ui(egui::RawInput::default(), |ui| {
        let rect = Rect::from_min_size(pos2(10.0, 10.0), vec2(16.0, 16.0));
        paint(ui.painter(), rect);
    });
    o.textures_delta.clear();
    fn walk(s: &Shape, out: &mut Vec<f32>) {
        match s {
            Shape::LineSegment { stroke, .. } => out.push(stroke.width),
            Shape::Path(p) => out.push(p.stroke.width),
            Shape::Rect(r) => out.push(r.stroke.width),
            Shape::Circle(c) => out.push(c.stroke.width),
            Shape::Ellipse(e) => out.push(e.stroke.width),
            Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    for c in &o.shapes {
        walk(&c.shape, &mut out);
    }
    out.retain(|w| *w > 0.0);
    out
}

#[test]
fn hand_drawn_icons_use_theme_stroke() {
    for dark in [true, false] {
        for active in [false, true] {
            let color = icons::stroke_color(dark, active);
            let p = theme::palette(dark);
            assert_eq!(
                color,
                if active {
                    p.text_primary
                } else {
                    p.text_secondary
                }
            );
        }
    }
    let color = icons::stroke_color(true, false);
    for kind in [
        BodyKind::Box,
        BodyKind::Cylinder,
        BodyKind::Sphere,
        BodyKind::Cone,
        BodyKind::Extrusion,
        BodyKind::Revolution,
        BodyKind::Combined,
        BodyKind::Mesh,
    ] {
        let w = widths(|p, r| icons::paint_body_icon(p, r, kind, color));
        assert!(!w.is_empty(), "{kind:?} zeichnet nichts");
        assert!(w.iter().all(|w| *w == 1.5), "{kind:?}: {w:?}");
    }
    assert_eq!(icons::STROKE_WIDTH, 1.5);
}

#[test]
fn every_hand_drawn_icon_draws_with_the_theme_width() {
    let color = icons::stroke_color(false, true);
    let mut all: Vec<(String, Vec<f32>)> = vec![
        (
            "sketch".into(),
            widths(|p, r| icons::paint_sketch_icon(p, r, color)),
        ),
        (
            "drawing".into(),
            widths(|p, r| icons::paint_drawing_icon(p, r, color)),
        ),
        (
            "view".into(),
            widths(|p, r| icons::paint_view_icon(p, r, color)),
        ),
    ];
    for fi in [
        icons::FeatureIcon::Sketch,
        icons::FeatureIcon::Move,
        icons::FeatureIcon::Blend,
    ] {
        all.push((
            format!("{fi:?}"),
            widths(|p, r| icons::paint_feature_icon(p, r, fi, color)),
        ));
    }
    for k in [
        DimKind::Horizontal,
        DimKind::Vertical,
        DimKind::Aligned,
        DimKind::Diameter,
        DimKind::Radius,
        DimKind::Angle,
    ] {
        all.push((
            format!("{k:?}"),
            widths(|p, r| icons::paint_dimension_icon(k)(p, r, color)),
        ));
    }
    assert!(!HAND_DRAWN.is_empty());
    for &h in HAND_DRAWN {
        all.push((
            format!("{h:?}"),
            widths(|p, r| icons::paint_hand_icon(p, r, h, color)),
        ));
    }
    for (name, w) in all {
        assert!(!w.is_empty(), "{name} zeichnet nichts");
        assert!(
            w.contains(&1.5),
            "{name}: keine Linie der Breite 1,5: {w:?}"
        );
        // Feinere Hilfslinien (1,0) sind erlaubt, alles andere nicht.
        assert!(w.iter().all(|w| *w == 1.5 || *w == 1.0), "{name}: {w:?}");
    }
}
